use agent_memory::codec::json::Value;
use agent_memory::developer_identity::{digest, encoded};
use agent_memory::layout_recovery_state as state;
use agent_memory::sharing_limits::MAX_PRIVATE;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("am-lrs-{}", agent_memory::host::uuid4().unwrap()));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn join(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn refusal<T: std::fmt::Debug>(result: agent_memory::error::Result<T>, expected: &str) {
    assert_eq!(result.unwrap_err().to_string(), expected);
}
fn put(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn canonical_records_are_immutable_and_private() {
    let root = Root::new();
    let path = root.join("evidence/manifest.json");
    let value = object([("z", s("é")), ("a", Value::Bool(true))]);
    let expected = "{\n  \"a\": true,\n  \"z\": \"é\"\n}\n".as_bytes();
    let hash = state::write_once(&path, &value).unwrap();
    assert_eq!(fs::read(&path).unwrap(), expected);
    assert_eq!(Some(hash.as_str()), digest(Some(expected)).as_deref());
    assert_eq!(state::write_once(&path, &value).unwrap(), hash);
    assert_eq!(
        encoded(&state::read(&path, MAX_PRIVATE, false).unwrap().unwrap()).unwrap(),
        expected
    );
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o7777,
        0o700
    );
    refusal(
        state::put_bytes(&path, b"foreign"),
        "Immutable recovery evidence already exists with different bytes",
    );
    assert_eq!(fs::read(path).unwrap(), expected);
}

#[test]
fn optional_reads_and_compare_replace_preserve_generations() {
    let root = Root::new();
    let path = root.join("missing/record.json");
    assert_eq!(state::read(&path, MAX_PRIVATE, true).unwrap(), None);
    let value = object([("generation", s("one"))]);
    let first = state::replace(&path, &value, None).unwrap();
    refusal(
        state::replace(&path, &Value::Null, None),
        "Recovery state changed; retain evidence and inspect the current generation",
    );
    let second = state::replace(&path, &Value::Null, Some(&first)).unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read(path).unwrap(), b"null\n");
}

#[test]
fn wrong_modes_links_and_bounds_refuse_without_repair() {
    let root = Root::new();
    let path = root.join("record");
    put(&path, b"abcd");
    for mode in [0o400, 0o640, 0o1600] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        refusal(
            state::read_bytes(&path, 4, false),
            "Recovery evidence must be bounded owner-only regular single-link files",
        );
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, mode);
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        state::read_bytes(&path, 4, false).unwrap(),
        Some(b"abcd".to_vec())
    );
    refusal(
        state::read_bytes(&path, 3, false),
        "Recovery evidence must be bounded owner-only regular single-link files",
    );
    let alias = root.join("alias");
    fs::hard_link(&path, &alias).unwrap();
    let expected = agent_memory::error::ContractError::trust_path(&path, "lifecycle records must be current-user-owned, regular, single-link and not group/other writable").to_string();
    refusal(state::read_bytes(&path, 4, false), &expected);
    assert_eq!(fs::read(alias).unwrap(), b"abcd");
    assert_eq!(fs::metadata(&path).unwrap().nlink(), 2);
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(root.join("alias"), &path).unwrap();
    assert!(
        state::read_bytes(&path, 4, true)
            .unwrap_err()
            .to_string()
            .contains("must not be redirected by a symlink")
    );
}

#[test]
fn noncanonical_or_invalid_json_is_not_adopted() {
    let root = Root::new();
    let path = root.join("record.json");
    for raw in [
        b"{}".as_slice(),
        b"{ }\n",
        b"{\n \"a\": 1\n}\n",
        b"{\"a\":1,\"a\":1}\n",
        b"\xef\xbb\xbfnull\n",
        b"NaN\n",
        b"null\n ",
    ] {
        put(&path, raw);
        assert!(state::read(&path, MAX_PRIVATE, false).is_err());
        assert_eq!(fs::read(&path).unwrap(), raw);
    }
    put(&path, b"{}\n");
    assert_eq!(
        state::read(&path, MAX_PRIVATE, false).unwrap(),
        Some(Value::Object(vec![]))
    );
}

#[test]
fn private_parent_never_chmods_existing_state() {
    let root = Root::new();
    let directory = root.join("evidence");
    state::private_parent(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    refusal(
        state::private_parent(&directory),
        "Recovery evidence directories must be exactly 0700; do not chmod existing state",
    );
    assert_eq!(fs::metadata(&directory).unwrap().mode() & 0o7777, 0o500);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn full_signatures_include_every_baseline_stat_field() {
    let root = Root::new();
    let path = root.join("record");
    put(&path, b"evidence");
    let directory = agent_memory::fs::Descriptor::open_directory(&root.0).unwrap();
    let info = directory.stat_at(std::ffi::OsStr::new("record")).unwrap();
    assert_eq!(state::signature(&info), info);
    for field in 0..9 {
        let mut changed = info;
        match field {
            0 => changed.dev ^= 1,
            1 => changed.ino ^= 1,
            2 => changed.mode ^= 1,
            3 => changed.uid ^= 1,
            4 => changed.gid ^= 1,
            5 => changed.nlink ^= 1,
            6 => changed.size ^= 1,
            7 => changed.mtime_ns ^= 1,
            _ => changed.ctime_ns ^= 1,
        }
        assert_ne!(state::signature(&changed), state::signature(&info));
    }
    for value in [
        Value::Null,
        s("A".repeat(64)),
        s("f".repeat(63)),
        s(format!("{}\n", "f".repeat(64))),
    ] {
        assert!(!state::sha(&value));
    }
    assert!(state::sha(&s("0123456789abcdef".repeat(4))));
}

fn isolated(name: &str) -> bool {
    if std::env::var_os("LRS_TEST_CHILD").is_some() {
        return true;
    }
    use agent_memory::{clock::Deadline, process};
    let root = Root::new();
    let argv = vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        name.into(),
        "--nocapture".into(),
    ];
    let mut env: Vec<_> = std::env::vars_os()
        .filter(|(key, _)| {
            !matches!(
                key.to_str(),
                Some(
                    "HOME"
                        | "AGENT_MEMORY_CONFIG_HOME"
                        | "AGENT_MEMORY_STATE_HOME"
                        | "LRS_TEST_CHILD"
                        | "LRS_TEST_ROOT"
                        | "GIT_CONFIG_GLOBAL"
                        | "GIT_CONFIG_SYSTEM"
                )
            )
        })
        .collect();
    env.extend([
        ("HOME".into(), root.0.clone().into_os_string()),
        (
            "AGENT_MEMORY_CONFIG_HOME".into(),
            root.join("config").into_os_string(),
        ),
        (
            "AGENT_MEMORY_STATE_HOME".into(),
            root.join("state").into_os_string(),
        ),
        ("LRS_TEST_CHILD".into(), "1".into()),
        ("LRS_TEST_ROOT".into(), root.0.clone().into_os_string()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
    ]);
    let output = process::run(&process::Spec {
        argv: &argv,
        search_path: false,
        env: &env,
        cwd: Some(&root.0),
        stdin: process::Input::Null,
        stdout: process::Target::Capture { limit: 65536 },
        stderr: process::Target::Capture { limit: 65536 },
        deadline: Deadline::from_seconds(30.0).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    })
    .unwrap();
    assert_eq!(
        output.status,
        process::Status::Code(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}
const OPERATION: &str = "12345678-1234-4123-8123-123456789abc";
const PROJECT: &str = "87654321-4321-4321-8321-cba987654321";
fn child_root() -> PathBuf {
    PathBuf::from(std::env::var_os("LRS_TEST_ROOT").unwrap())
}
fn fixture() -> (PathBuf, PathBuf, PathBuf) {
    let root = child_root();
    let repo = root.join("repo");
    let bank = repo.join("memory-bank");
    fs::create_dir_all(&bank).unwrap();
    fs::set_permissions(&repo, fs::Permissions::from_mode(0o700)).unwrap();
    let runtime = root.join("runtime");
    fs::create_dir(&runtime).unwrap();
    (repo, bank, runtime)
}
fn registry_path() -> PathBuf {
    child_root().join("config/layout-recovery-bindings.json")
}
fn binding(repo: &Path, bank: &Path, base: &Path, lifecycle: &str) -> Value {
    let state_root = child_root().join("state");
    object([
        ("repo_root", s(repo.to_str().unwrap())),
        ("bank_path", s(bank.to_str().unwrap())),
        ("checkout_key", s("checkout")),
        ("project_id", s(PROJECT)),
        ("state_root", s(state_root.to_str().unwrap())),
        ("index_path", s(base.join("index.json").to_str().unwrap())),
        ("lifecycle_state", s(lifecycle)),
    ])
}
fn registry(row: Value) -> Value {
    object([
        ("schema", s(state::BINDINGS)),
        ("bindings", object([(OPERATION, row)])),
    ])
}
fn set(value: &mut Value, key: &str, item: Value) {
    *value.get_mut(key).unwrap() = item;
}

#[test]
fn paths_and_registry_refuse_locks_moves_and_missing_index() {
    if !isolated("paths_and_registry_refuse_locks_moves_and_missing_index") {
        return;
    }
    let (repo, bank, runtime) = fixture();
    let (_, _, base, operation) =
        state::_paths_for_bank(&repo, &bank, Some(OPERATION), &runtime).unwrap();
    assert_eq!(operation, Some(base.join(OPERATION)));
    assert_eq!(base.parent().unwrap().file_name().unwrap().len(), 64);
    let row = binding(&repo, &bank, &base, "active");
    state::write_once(&registry_path(), &registry(row.clone())).unwrap();
    refusal(
        state::_paths_for_bank(&repo, &bank, None, &runtime),
        "Bound recovery index is missing; coverage remains pending",
    );
    state::write_once(&base.join("index.json"), &Value::Null).unwrap();
    assert!(state::_paths_for_bank(&repo, &bank, None, &runtime).is_ok());
    let mut moved = row.clone();
    set(
        &mut moved,
        "state_root",
        s(child_root().join("moved-state").to_str().unwrap()),
    );
    let raw = encoded(&registry(moved)).unwrap();
    put(&registry_path(), &raw);
    refusal(
        state::_paths_for_bank(&repo, &bank, None, &runtime),
        "Recovery state location changed; restore its original verified binding",
    );
    let mut redirected = row.clone();
    set(
        &mut redirected,
        "repo_root",
        s(format!("{}/../repo", repo.display())),
    );
    put(&registry_path(), &encoded(&registry(redirected)).unwrap());
    refusal(
        state::bindings(),
        "Recovery binding moved or became redirected; restore the original locations",
    );
    put(&registry_path(), &encoded(&registry(row)).unwrap());
    let lock = child_root().join("config/.layout-recovery-bindings.lock");
    put(&lock, b"retained");
    refusal(
        state::bindings(),
        "Recovery bindings are locked; retain private evidence",
    );
    assert_eq!(fs::read(lock).unwrap(), b"retained");
}

#[test]
fn unknown_binding_registry_and_rows_are_refused() {
    if !isolated("unknown_binding_registry_and_rows_are_refused") {
        return;
    }
    let (repo, bank, runtime) = fixture();
    let (_, _, base, _) = state::_paths_for_bank(&repo, &bank, None, &runtime).unwrap();
    state::private_parent(registry_path().parent().unwrap()).unwrap();
    for value in [
        Value::Null,
        object([
            ("schema", s("unknown")),
            ("bindings", Value::Object(vec![])),
        ]),
        object([("schema", s(state::BINDINGS)), ("bindings", Value::Null)]),
    ] {
        put(&registry_path(), &encoded(&value).unwrap());
        refusal(state::bindings(), "Unknown recovery state binding registry");
    }
    for field in [
        "project_id",
        "checkout_key",
        "lifecycle_state",
        "state_root",
    ] {
        let mut row = binding(&repo, &bank, &base, "terminal");
        set(&mut row, field, Value::Null);
        put(&registry_path(), &encoded(&registry(row)).unwrap());
        let error = if field == "state_root" {
            "Recovery binding moved or became redirected; restore the original locations"
        } else {
            "Unknown recovery state binding"
        };
        refusal(state::bindings(), error);
    }
    let value = object([
        ("schema", s(state::BINDINGS)),
        (
            "bindings",
            object([("not-uuid", binding(&repo, &bank, &base, "terminal"))]),
        ),
    ]);
    put(&registry_path(), &encoded(&value).unwrap());
    refusal(state::bindings(), "Unknown recovery state binding");
}

#[test]
fn operation_index_requires_exact_schema_identity_and_generations() {
    if !isolated("operation_index_requires_exact_schema_identity_and_generations") {
        return;
    }
    let (repo, bank, runtime) = fixture();
    let (_, _, base, _) = state::_paths_for_bank(&repo, &bank, None, &runtime).unwrap();
    let empty = state::_index_for_bank(&repo, &bank, &runtime).unwrap();
    assert_eq!(empty.get("schema"), Some(&s(state::INDEX)));
    assert_eq!(empty.get("operations"), Some(&Value::Object(vec![])));
    let row = object([
        ("state", s("active")),
        ("project_id", s(PROJECT)),
        ("ledger_head_sha256", s("f".repeat(64))),
        ("proposal_id", s(OPERATION)),
        ("hold_id", s(OPERATION)),
    ]);
    let mut valid = empty.clone();
    set(&mut valid, "operations", object([(OPERATION, row.clone())]));
    state::write_once(&base.join("index.json"), &valid).unwrap();
    assert!(state::_index_for_bank(&repo, &bank, &runtime).is_ok());
    for field in ["schema", "repo_root", "bank_path", "operations"] {
        let mut invalid = valid.clone();
        set(&mut invalid, field, Value::Null);
        put(&base.join("index.json"), &encoded(&invalid).unwrap());
        refusal(
            state::_index_for_bank(&repo, &bank, &runtime),
            "Unknown, foreign or damaged recovery operation index",
        );
    }
    for field in [
        "state",
        "project_id",
        "ledger_head_sha256",
        "proposal_id",
        "hold_id",
    ] {
        let mut invalid_row = row.clone();
        set(&mut invalid_row, field, Value::Null);
        let mut invalid = valid.clone();
        set(
            &mut invalid,
            "operations",
            object([(OPERATION, invalid_row)]),
        );
        put(&base.join("index.json"), &encoded(&invalid).unwrap());
        refusal(
            state::_index_for_bank(&repo, &bank, &runtime),
            "Recovery operation index has an invalid generation or identity",
        );
    }
    for lifecycle in [
        "active",
        "terminal",
        "interrupted",
        "rolled-back",
        "cleaned",
    ] {
        let mut current = row.clone();
        set(&mut current, "state", s(lifecycle));
        let mut value = valid.clone();
        set(&mut value, "operations", object([(OPERATION, current)]));
        put(&base.join("index.json"), &encoded(&value).unwrap());
        assert!(state::_index_for_bank(&repo, &bank, &runtime).is_ok());
    }
}

#[test]
fn bank_runtime_and_operation_guards_preserve_refusal_strings() {
    if !isolated("bank_runtime_and_operation_guards_preserve_refusal_strings") {
        return;
    }
    let (repo, bank, runtime) = fixture();
    refusal(
        state::_paths_for_bank(&repo, &bank, Some("not-uuid"), &runtime),
        "Recovery operations require canonical UUIDv4 identifiers",
    );
    refusal(
        state::_paths_for_bank(&repo, &bank, Some(""), &runtime),
        "Recovery operations require canonical UUIDv4 identifiers",
    );
    refusal(
        state::_paths_for_bank(&repo, &bank, None, &child_root()),
        "Recovery state must remain outside the repository and executing runtime",
    );
    fs::rename(&bank, repo.join("original-bank")).unwrap();
    refusal(
        state::_paths_for_bank(&repo, &bank, None, &runtime),
        "Recovery requires the existing configured ordinary bank directory",
    );
    std::os::unix::fs::symlink(repo.join("original-bank"), &bank).unwrap();
    refusal(
        state::_paths_for_bank(&repo, &bank, None, &runtime),
        "Recovery requires the existing configured ordinary bank directory",
    );
}

#[test]
fn public_bind_and_index_publish_and_preserve_identity() {
    if !isolated("public_bind_and_index_publish_and_preserve_identity") {
        return;
    }
    agent_memory::repository_config::register_security_scan(Some(std::sync::Arc::new(
        agent_memory::security::Security,
    )))
    .unwrap();
    let (repo, bank, runtime) = fixture();
    let (_, _, base, _) = state::paths(&repo, None, &runtime).unwrap();
    let empty = state::index(&repo, &runtime).unwrap();
    let hash = state::set_index(&repo, &empty, None, &runtime).unwrap();
    let row = state::bind(&repo, OPERATION, PROJECT, "checkout", "active", &runtime).unwrap();
    assert_eq!(
        encoded(&row).unwrap(),
        encoded(&binding(&repo, &bank, &base, "active")).unwrap()
    );
    assert_eq!(
        state::set_index(&repo, &empty, Some(&hash), &runtime).unwrap(),
        hash
    );
    let before = fs::read(registry_path()).unwrap();
    refusal(
        state::bind(
            &repo,
            OPERATION,
            PROJECT,
            "foreign-checkout",
            "terminal",
            &runtime,
        ),
        "Recovery binding cannot move or change identity",
    );
    refusal(
        state::bind(
            &repo, OPERATION, OPERATION, "checkout", "terminal", &runtime,
        ),
        "Recovery binding cannot move or change identity",
    );
    assert_eq!(fs::read(registry_path()).unwrap(), before);
    let row = state::bind(&repo, OPERATION, PROJECT, "checkout", "terminal", &runtime).unwrap();
    assert_eq!(row.get("lifecycle_state"), Some(&s("terminal")));
    fs::remove_file(base.join("index.json")).unwrap();
    assert!(state::paths(&repo, None, &runtime).is_ok());
}

#[test]
fn replaced_parent_during_real_file_observation_is_refused() {
    let root = Root::new();
    let parent = root.join("parent");
    state::private_parent(&parent).unwrap();
    let path = parent.join("blob");
    let raw = vec![0x61; 128 * 1024 * 1024];
    state::put_bytes(&path, &raw).unwrap();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_gate = gate.clone();
    let old = root.join("old-parent");
    let worker_parent = parent.clone();
    let worker = std::thread::spawn(move || {
        worker_gate.wait();
        std::thread::sleep(std::time::Duration::from_millis(2));
        fs::rename(&worker_parent, &old).unwrap();
        fs::create_dir(&worker_parent).unwrap();
        fs::set_permissions(&worker_parent, fs::Permissions::from_mode(0o700)).unwrap();
    });
    gate.wait();
    refusal(
        state::read_bytes(&path, MAX_PRIVATE, false),
        "Recovery evidence or its named parent changed during observation",
    );
    worker.join().unwrap();
    assert_eq!(
        fs::metadata(root.join("old-parent/blob")).unwrap().len(),
        raw.len() as u64
    );
}

#[test]
fn oversized_immutable_bytes_refuse_before_creating_any_parent() {
    let root = Root::new();
    let path = root.join("absent/oversize");
    let raw = vec![0; MAX_PRIVATE + 1];
    refusal(
        state::put_bytes(&path, &raw),
        "Recovery evidence exceeds its private artifact bound",
    );
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn bindings_follow_pathlib_normalization_but_refuse_redirected_paths() {
    if !isolated("bindings_follow_pathlib_normalization_but_refuse_redirected_paths") {
        return;
    }
    let (repo, bank, runtime) = fixture();
    let (_, _, base, _) = state::_paths_for_bank(&repo, &bank, None, &runtime).unwrap();
    let mut row = binding(&repo, &bank, &base, "terminal");
    set(&mut row, "repo_root", s(format!("{}/./", repo.display())));
    state::write_once(&registry_path(), &registry(row.clone())).unwrap();
    assert!(state::bindings().is_ok());
    refusal(
        state::_paths_for_bank(&repo, &bank, None, &runtime),
        "Recovery state location changed; restore its original verified binding",
    );
    let alias = child_root().join("aliased-repo");
    std::os::unix::fs::symlink(&repo, &alias).unwrap();
    set(&mut row, "repo_root", s(alias.to_str().unwrap()));
    put(&registry_path(), &encoded(&registry(row)).unwrap());
    refusal(
        state::bindings(),
        "Recovery binding moved or became redirected; restore the original locations",
    );
}

#[test]
fn recovery_replace_accepts_private_records_above_default_registry_bound() {
    let root = Root::new();
    let path = root.join("large-record");
    let file = fs::File::create(&path).unwrap();
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    let length = agent_memory::codec::json::DEFAULT_MAX_BYTES as u64 + 1;
    assert!(length < MAX_PRIVATE as u64);
    file.set_len(length).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let observed = digest(Some(&fs::read(&path).unwrap())).unwrap();
    assert!(agent_memory::local_registry::replace(&path, b"null\n", Some(&observed)).is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), length);
    let published = state::replace(&path, &Value::Null, Some(&observed)).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"null\n");
    assert_eq!(Some(published), digest(Some(b"null\n")));
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
}

#[test]
fn explicit_registry_limit_changes_only_existing_bytes_bound() {
    let root = Root::new();
    let path = root.join("record");
    put(&path, b"12345");
    let observed = digest(Some(b"12345")).unwrap();
    assert!(
        agent_memory::local_registry::replace_with_limit(&path, b"successor", Some(&observed), 4)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"12345");
    agent_memory::local_registry::replace_with_limit(&path, b"successor", Some(&observed), 5)
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"successor");
}
