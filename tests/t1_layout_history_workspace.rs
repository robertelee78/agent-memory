use agent_memory::codec::json::{self, Value};
use agent_memory::developer_identity::{self as developer, encoded, load};
use agent_memory::error::Result;
use agent_memory::host::{self, Sources};
use agent_memory::{
    bank_contract, cli, clock::Deadline, layout_history as history, layout_recovery_state as state,
    private_workspace as private, process, project_identity as project, scope_binding as scope,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

const PROJECT: &str = "75b38e17-c36e-4fab-8ba9-5fa51f25de91";
const OTHER: &str = "bdcb692b-2322-4f21-b9dd-a4a4eca21145";
const OPERATION: &str = "b3dcae21-f5f3-40ac-bcfe-e845a3d2868b";
const RECOVERY: &str = "3e7c5e91-bac1-4256-b423-d788b388a701";
const CORRUPT: &str = "Known recovery historical receipt is corrupt or request ID was reused";
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn object(values: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap()
}
fn set(value: &mut Value, key: &str, item: Value) {
    let Value::Object(fields) = value else {
        panic!("fixture object")
    };
    if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
        *old = item;
    } else {
        fields.push((key.into(), item));
    }
}
fn fixed_time() -> Result<String> {
    Ok("2026-11-13T12:34:56.000001+00:00".into())
}
fn sources() -> Sources {
    Sources {
        uuid4: host::uuid4,
        utc_now: fixed_time,
    }
}
struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "am-layout-history-workspace-{}",
            host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn private_file(path: &Path, raw: &[u8]) {
    std::fs::write(path, raw).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn canonical() -> Value {
    object([
        ("operation_id", string(OPERATION)),
        ("outcome", string("finished")),
        ("verified", Value::Bool(true)),
    ])
}
fn preview() -> Value {
    object([
        ("operation_id", string(OPERATION)),
        ("recovery_id", string(RECOVERY)),
        ("mode", string("finish")),
    ])
}

#[test]
fn recovery_receipt_bytes_and_completed_readback_bind_both_exact_records() {
    let temp = Temporary::new();
    let directory = &temp.0;
    let request = preview();
    let canonical = canonical();
    assert_eq!(history::completed(directory, &request).unwrap(), None);
    let receipt =
        history::recovery_receipt_with_sources(directory, &request, &canonical, sources()).unwrap();
    assert_eq!(
        get(&receipt, "schema"),
        &string("agent-memory/layout-known-recovery-receipt-v1")
    );
    assert_eq!(get(&receipt, "operation_id"), &string(OPERATION));
    assert_eq!(get(&receipt, "recovery_id"), &string(RECOVERY));
    assert_eq!(
        get(&receipt, "recovery_sha256"),
        &string("1cd00f84f2bc32e048d2c1a2baf261873f5615953b8da40b7a4b5b25bb76b5df")
    );
    assert_eq!(
        get(&receipt, "canonical_receipt_sha256"),
        &string("bdae865af048828b6557bf5da9ef7d904944df4205b4260e6c015bbd84f96d7f")
    );
    assert_eq!(
        get(&receipt, "readback_at"),
        &string("2026-11-13T12:34:56.000001+00:00")
    );
    // These inputs have independently specified baseline encoded bytes.
    assert_eq!(encoded(&request).unwrap(), format!("{{\n  \"mode\": \"finish\",\n  \"operation_id\": \"{OPERATION}\",\n  \"recovery_id\": \"{RECOVERY}\"\n}}\n").into_bytes());
    let canonical_path = directory.join("receipt.json");
    state::write_once(&canonical_path, &canonical).unwrap();
    let receipt_path = directory
        .join("recovery")
        .join(format!("{RECOVERY}.receipt.json"));
    state::write_once(&receipt_path, &receipt).unwrap();
    assert_eq!(
        encoded(&history::completed(directory, &request).unwrap().unwrap()).unwrap(),
        encoded(&canonical).unwrap()
    );
    let original = std::fs::read(&receipt_path).unwrap();
    for (field, changed) in [
        ("schema", string("unknown")),
        ("operation_id", string(OTHER)),
        ("recovery_id", string(OTHER)),
        ("recovery_sha256", string("0".repeat(64))),
        ("canonical_receipt_sha256", string("0".repeat(64))),
        ("verified", Value::Bool(false)),
        ("verified", Value::Int(1.into())),
    ] {
        let mut bad = receipt.clone();
        set(&mut bad, field, changed);
        private_file(&receipt_path, &encoded(&bad).unwrap());
        assert_eq!(
            history::completed(directory, &request)
                .unwrap_err()
                .to_string(),
            CORRUPT
        );
    }
    private_file(&receipt_path, &original);
    let mut reused = request.clone();
    set(&mut reused, "mode", string("rollback"));
    assert_eq!(
        history::completed(directory, &reused)
            .unwrap_err()
            .to_string(),
        CORRUPT
    );
    let mut changed = canonical.clone();
    set(&mut changed, "outcome", string("rolled-back"));
    private_file(&canonical_path, &encoded(&changed).unwrap());
    assert_eq!(
        history::completed(directory, &request)
            .unwrap_err()
            .to_string(),
        CORRUPT
    );
    set(&mut changed, "verified", Value::Bool(false));
    let rebound =
        history::recovery_receipt_with_sources(directory, &request, &changed, sources()).unwrap();
    private_file(&canonical_path, &encoded(&changed).unwrap());
    private_file(&receipt_path, &encoded(&rebound).unwrap());
    assert_eq!(
        history::completed(directory, &request)
            .unwrap_err()
            .to_string(),
        CORRUPT
    );
}

#[test]
fn historical_readback_rejects_noncanonical_and_unsafe_retained_evidence() {
    let temp = Temporary::new();
    let request = preview();
    let canonical = canonical();
    let receipt =
        history::recovery_receipt_with_sources(&temp.0, &request, &canonical, sources()).unwrap();
    let target = temp
        .0
        .join("recovery")
        .join(format!("{RECOVERY}.receipt.json"));
    state::write_once(&temp.0.join("receipt.json"), &canonical).unwrap();
    state::write_once(&target, &receipt).unwrap();
    private_file(
        &target,
        json::encode(&receipt, json::DEFAULT).unwrap().as_bytes(),
    );
    assert!(
        history::completed(&temp.0, &request)
            .unwrap_err()
            .to_string()
            .contains("exact canonical")
    );
    private_file(&target, &encoded(&receipt).unwrap());
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(history::completed(&temp.0, &request).is_err());
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    let alias = temp.0.join("alias.json");
    std::fs::hard_link(&target, &alias).unwrap();
    assert!(history::completed(&temp.0, &request).is_err());
    std::fs::remove_file(&alias).unwrap();
    std::fs::remove_file(&target).unwrap();
    symlink(temp.0.join("receipt.json"), &target).unwrap();
    assert!(history::completed(&temp.0, &request).is_err());
    let mut bad = request.clone();
    set(&mut bad, "recovery_id", string("../../escape"));
    assert_eq!(
        history::completed(&temp.0, &bad).unwrap_err().to_string(),
        CORRUPT
    );
    assert!(history::recovery_receipt(&temp.0, &Value::Null, &canonical).is_err());
}

#[test]
fn private_inputs_refuse_invalid_names_shapes_duplicates_and_bound_before_scope_lookup() {
    let repo = Path::new("/deliberately-absent-private-workspace-repository");
    let workspace = "a".repeat(32);
    for (id, name) in [
        ("A".repeat(32), "request.json".to_owned()),
        ("a".repeat(31), "request.json".to_owned()),
        (workspace.clone(), "binding.json".to_owned()),
        (workspace.clone(), "../bad.json".to_owned()),
        (workspace.clone(), ".hidden.json".to_owned()),
        (workspace.clone(), format!("{}.json", "a".repeat(129))),
        (workspace.clone(), "name.JSON".to_owned()),
        (workspace.clone(), "a.json\n".to_owned()),
        (workspace.clone(), "é.json".to_owned()),
    ] {
        assert_eq!(
            private::put(repo, &id, &name, b"{}")
                .unwrap_err()
                .to_string(),
            "Use the returned workspace ID and a simple new JSON filename"
        );
    }
    for raw in [b"null".as_slice(), b"true", b"1", b"\"string\""] {
        assert_eq!(
            private::put(repo, &workspace, "input.json", raw)
                .unwrap_err()
                .to_string(),
            "Private input must be a JSON object or array"
        );
    }
    for raw in [
        b"{\"x\":1,\"x\":2}".as_slice(),
        b"{",
        b"\xef\xbb\xbf{}",
        b"{\"x\":\"\xff\"}",
    ] {
        assert!(private::put(repo, &workspace, "input.json", raw).is_err());
    }
    let too_large = vec![b' '; private::MAX_INPUT + 1];
    assert_eq!(
        private::put(repo, &workspace, "input.json", &too_large)
            .unwrap_err()
            .to_string(),
        "Private input exceeds the 16 MiB bound"
    );
    let failure =
        private::command_result("put", private::put(repo, &workspace, "../bad.json", b"{}"));
    assert_eq!(failure.exit_code, 1);
    assert!(failure.stderr.is_empty());
    assert_eq!(
        failure.stdout,
        "{\"operation\": \"private-put\", \"verified\": false, \"canonical_writes_performed\": false, \"error\": \"Use the returned workspace ID and a simple new JSON filename\", \"repair\": \"Preserve existing private artifacts and inspect the reported workspace before retrying.\"}\n"
    );
}

#[test]
fn capacity_refusal_replaces_repair_without_duplicate_keys_or_reordering() {
    use agent_memory::bank_capacity::{BankCapacityError, CapacityCode, Measurement};

    let error = BankCapacityError::new(
        CapacityCode::SourceBytes,
        "activeContext.md",
        16_384,
        16_385,
        Measurement::Exact,
    )
    .unwrap();
    let result = private::command_result("create", Err(error.into()));
    assert_eq!(result.exit_code, 1);
    assert!(result.stderr.is_empty());
    // This decoder rejects duplicate keys, including duplicate repair fields.
    let value = load(result.stdout.as_bytes()).unwrap();
    assert_eq!(get(&value, "operation"), &string("private-create"));
    assert_eq!(get(&value, "verified"), &Value::Bool(false));
    assert_eq!(
        get(&value, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(&value, "repair"),
        &string(
            "Preserve the bank, feature work, original index/HEAD and all recovery evidence. Ordinary recall and new capture remain pending. Use am bank recover-layout inspect with a private output path for explicit source-preserving review. Do not truncate, delete, skip a source, raise normal limits or initialize over existing memory; repeating the same operation does not repair capacity."
        )
    );
    assert_eq!(get(&value, "failure_phase"), &string("bank-eligibility"));
    assert_eq!(get(&value, "failure_component"), &string("canonical-bank"));
    assert_eq!(get(&value, "failure_path"), &string("activeContext.md"));
    assert_eq!(get(&value, "capacity_limit"), &Value::Int(16_384.into()));
    assert_eq!(get(&value, "capacity_observed"), &Value::Int(16_385.into()));
    let keys = value
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "operation",
            "verified",
            "canonical_writes_performed",
            "error",
            "repair",
            "failure_code",
            "failure_phase",
            "failure_component",
            "failure_path",
            "capacity_limit",
            "capacity_observed",
            "capacity_measurement",
            "capacity_unit",
        ]
    );
}

#[test]
fn am_private_put_early_refusals_are_ungated_and_leave_state_untouched() {
    let temp = Temporary::new();
    let world = World {
        root: temp.0.clone(),
    };
    let workspace = "a".repeat(32);
    let overflow = vec![b' '; private::MAX_INPUT + 1];
    for (name, raw, message) in [
        (
            "../bad.json",
            b"{}".as_slice(),
            "Use the returned workspace ID and a simple new JSON filename",
        ),
        (
            "overflow.json",
            overflow.as_slice(),
            "Private input exceeds the 16 MiB bound",
        ),
    ] {
        let argv = [
            env!("CARGO_BIN_EXE_am"),
            "--json",
            "private",
            "put",
            "--repo",
            temp.0.to_str().unwrap(),
            "--workspace",
            &workspace,
            "--name",
            name,
        ]
        .map(OsString::from);
        let output = world.command(&argv, raw, &[]);
        assert_eq!(output.status, process::Status::Code(1));
        assert!(output.stderr.is_empty());
        let expected = format!(
            "{{\"operation\": \"private-put\", \"verified\": false, \"canonical_writes_performed\": false, \"error\": \"{message}\", \"repair\": \"Preserve existing private artifacts and inspect the reported workspace before retrying.\"}}\n"
        );
        assert_eq!(output.stdout, expected.as_bytes());
        let value = load(&output.stdout).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert_eq!(get(&value, "error"), &string(message));
    }
    assert!(std::fs::read_dir(&temp.0).unwrap().next().is_none());
}

#[test]
fn pinned_workspace_writes_refuse_overwrite_redirect_and_directory_replacement() {
    let temp = Temporary::new();
    let path = temp.0.join("workspace");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    private::write_exclusive(&path, "binding.json", b"{}").unwrap();
    let (directory, info) = private::checked_directory(&path).unwrap();
    assert_eq!(private::read_binding(&directory).unwrap(), b"{}");
    private::write_fd(&directory, "request.json", b"{\"explicit\":true}").unwrap();
    assert_eq!(
        std::fs::metadata(path.join("request.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o600
    );
    assert!(private::write_fd(&directory, "request.json", b"{}").is_err());
    let outside = temp.0.join("outside.json");
    private_file(&outside, b"untouched");
    symlink(&outside, path.join("link.json")).unwrap();
    assert!(private::write_fd(&directory, "link.json", b"{}").is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
    // Deterministic equivalent of the baseline's write_fd replacement injection:
    // the real held descriptor writes only the old directory, never its replacement.
    private::write_fd(&directory, "race.json", b"{\"race\":true}").unwrap();
    let moved = temp.0.join("moved");
    std::fs::rename(&path, &moved).unwrap();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    assert_eq!(
        private::check_directory(&path, &info)
            .unwrap_err()
            .to_string(),
        "Private workspace changed; preserve it for inspection"
    );
    assert!(!path.join("race.json").exists());
    assert_eq!(
        std::fs::read(moved.join("race.json")).unwrap(),
        b"{\"race\":true}"
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(private::checked_directory(&path).is_err());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o755
    );
}

#[test]
fn private_binding_reads_require_bounded_single_link_regular_owner_only_files() {
    let temp = Temporary::new();
    let (directory, _) = private::checked_directory(&temp.0).unwrap();
    let binding = temp.0.join("binding.json");
    private_file(&binding, &[b'x'; private::MAX_BINDING]);
    assert_eq!(
        private::read_binding(&directory).unwrap().len(),
        private::MAX_BINDING
    );
    private_file(&binding, &[b'x'; private::MAX_BINDING + 1]);
    assert_eq!(
        private::read_binding(&directory).unwrap_err().to_string(),
        "Private workspace binding is unsafe"
    );
    private_file(&binding, b"{}");
    std::fs::set_permissions(&binding, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        private::read_binding(&directory).unwrap_err().to_string(),
        "Private workspace binding is unsafe"
    );
    std::fs::set_permissions(&binding, std::fs::Permissions::from_mode(0o600)).unwrap();
    let alias = temp.0.join("alias.json");
    std::fs::hard_link(&binding, &alias).unwrap();
    assert_eq!(
        private::read_binding(&directory).unwrap_err().to_string(),
        "Private workspace binding is unsafe"
    );
    std::fs::remove_file(&alias).unwrap();
    std::fs::remove_file(&binding).unwrap();
    symlink("absent", &binding).unwrap();
    assert!(private::read_binding(&directory).is_err());
    std::fs::remove_file(&binding).unwrap();
    std::fs::create_dir(&binding).unwrap();
    assert_eq!(
        private::read_binding(&directory).unwrap_err().to_string(),
        "Private workspace binding is unsafe"
    );
}

struct World {
    root: PathBuf,
}
impl World {
    fn env(&self) -> Vec<(OsString, OsString)> {
        vec![
            ("PATH".into(), "/usr/bin:/bin:/usr/local/bin".into()),
            ("HOME".into(), self.root.join("home").into_os_string()),
            (
                "AGENT_MEMORY_CONFIG_HOME".into(),
                self.root.join("config").into_os_string(),
            ),
            (
                "AGENT_MEMORY_STATE_HOME".into(),
                self.root.join("state/agent-memory").into_os_string(),
            ),
            (
                "XDG_CONFIG_HOME".into(),
                self.root.join("xdg-config").into_os_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                self.root.join("state").into_os_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("LC_ALL".into(), "C".into()),
        ]
    }
    fn command(
        &self,
        argv: &[OsString],
        raw: &[u8],
        extra: &[(&str, OsString)],
    ) -> process::Output {
        let mut env = self.env();
        env.extend(
            extra
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone())),
        );
        let spec = process::Spec {
            argv,
            env: &env,
            cwd: Some(&self.root),
            search_path: true,
            stdin: process::Input::Bytes(raw),
            stdout: process::Target::Capture {
                limit: 4 * 1024 * 1024,
            },
            stderr: process::Target::Capture {
                limit: 4 * 1024 * 1024,
            },
            deadline: Deadline::from_absolute_ns(u64::MAX),
            new_session: false,
            file_size_limit: None,
            umask: None,
            pass_fds: &[],
        };
        process::run_for(&spec, Duration::from_secs(120)).unwrap()
    }
    fn git(&self, repo: &Path, args: &[&str]) {
        let mut argv = vec![
            OsString::from("git"),
            "-C".into(),
            repo.as_os_str().to_owned(),
        ];
        argv.extend(args.iter().map(OsString::from));
        let output = self.command(&argv, b"", &[]);
        assert_eq!(
            output.status,
            process::Status::Code(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        std::fs::create_dir(&repo).unwrap();
        self.git(&repo, &["init", "--initial-branch=main"]);
        self.git(&repo, &["config", "user.name", "Private fixture"]);
        self.git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        let omitted = [
            "projectbrief.md",
            "productContext.md",
            "systemPatterns.md",
            "techContext.md",
            "progress.md",
        ];
        let rendered = bank_contract::render(&object([
            ("project-id", string(PROJECT)),
            ("router-updated", string("2026-11-13T12:00:00Z")),
            (
                "contexts",
                object([(
                    "activeContext.md",
                    object([
                        ("body", string("# Work\n\nExplicit isolated fixture.\n")),
                        ("purpose", string("Current work")),
                        ("scope", string("repository")),
                        ("lifecycle", string("active")),
                        ("read-when", string("When resuming")),
                    ]),
                )]),
            ),
            (
                "omitted",
                object(omitted.map(|key| (key, string("No distinct evidence supplied")))),
            ),
        ]))
        .unwrap();
        let bank = repo.join("memory-bank");
        std::fs::create_dir(&bank).unwrap();
        for (name, body) in get(&rendered, "files").as_object().unwrap() {
            std::fs::write(bank.join(name), body.as_str().unwrap()).unwrap();
        }
        self.git(&repo, &["add", "."]);
        self.git(&repo, &["commit", "-m", "Isolated fixture"]);
        let prepared =
            project::preview(&repo, "bind", None, "Explicit fixture binding", None).unwrap();
        project::apply(&prepared).unwrap();
        repo
    }
    fn developer(&self) {
        let prepared = developer::preview(
            "create",
            None,
            &Value::Array(vec![string("Fixture developer")]),
            &Value::Array(Vec::new()),
        )
        .unwrap();
        developer::apply(&prepared).unwrap();
    }
    fn native(&self, args: &[&str], raw: &[u8], code: i32) -> Value {
        let mut argv = vec![OsString::from(env!("CARGO_BIN_EXE_am")), "--json".into()];
        argv.extend(args.iter().map(OsString::from));
        let output = self.command(&argv, raw, &[]);
        assert_eq!(
            output.status,
            process::Status::Code(code),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        load(&output.stdout).unwrap()
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, u32)> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, (Vec<u8>, u32)>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    (
                        std::fs::read(&path).unwrap(),
                        std::fs::metadata(&path).unwrap().permissions().mode(),
                    ),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn in_world(name: &str, body: impl FnOnce(&World)) {
    if std::env::var("AM_WORKSPACE_CASE").ok().as_deref() == Some(name) {
        cli::register_providers().unwrap();
        body(&World {
            root: PathBuf::from(std::env::var_os("AM_WORKSPACE_ROOT").unwrap()),
        });
        return;
    }
    let temp = Temporary::new();
    std::fs::create_dir(temp.0.join("home")).unwrap();
    let world = World {
        root: temp.0.clone(),
    };
    let result = world.command(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            name.into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ],
        b"",
        &[
            ("AM_WORKSPACE_CASE", name.into()),
            ("AM_WORKSPACE_ROOT", temp.0.as_os_str().to_owned()),
        ],
    );
    assert_eq!(
        result.status,
        process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

// obligation: tests/contracts/test_private_workspace.py::PrivateWorkspaceContract.test_explicit_private_input_and_refusals
#[test]
fn explicit_private_input_and_refusals_through_am_private() {
    in_world(
        "explicit_private_input_and_refusals_through_am_private",
        |w| {
            w.developer();
            let repo = w.repo("repository");
            let repo_text = repo.to_str().unwrap();
            std::fs::write(repo.join("feature.txt"), b"staged feature\n").unwrap();
            w.git(&repo, &["add", "feature.txt"]);
            std::fs::write(repo.join("feature.txt"), b"unstaged feature\n").unwrap();
            let before = snapshot(&repo);
            let registry = snapshot(&w.root.join("config"));
            let created = w.native(&["private", "create", "--repo", repo_text], b"", 0);
            assert_eq!(get(&created, "operation"), &string("private-create"));
            assert_eq!(get(&created, "verified"), &Value::Bool(true));
            assert_eq!(
                get(&created, "canonical_writes_performed"),
                &Value::Bool(false)
            );
            let path = PathBuf::from(text(&created, "path"));
            assert!(!path.starts_with(&repo));
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                0o700
            );
            let workspace = text(&created, "workspace");
            let raw = b"{\"explicit\":\"authored\"}";
            let args = [
                "private",
                "put",
                "--repo",
                repo_text,
                "--workspace",
                workspace,
                "--name",
                "request.json",
            ];
            let written = w.native(&args, raw, 0);
            assert_eq!(get(&written, "operation"), &string("private-put"));
            assert_eq!(get(&written, "bytes"), &Value::Int(raw.len().into()));
            assert_eq!(std::fs::read(path.join("request.json")).unwrap(), raw);
            assert_eq!(
                std::fs::metadata(path.join("request.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777,
                0o600
            );
            w.native(&args, b"{\"changed\":true}", 1);
            assert_eq!(std::fs::read(path.join("request.json")).unwrap(), raw);
            for (name, raw) in [
                ("../bad.json", raw.as_slice()),
                ("dupe.json", b"{\"x\":1,\"x\":2}"),
                ("scalar.json", b"true"),
            ] {
                let refused = w.native(
                    &[
                        "private",
                        "put",
                        "--repo",
                        repo_text,
                        "--workspace",
                        workspace,
                        "--name",
                        name,
                    ],
                    raw,
                    1,
                );
                assert_eq!(get(&refused, "verified"), &Value::Bool(false));
                assert_eq!(
                    get(&refused, "canonical_writes_performed"),
                    &Value::Bool(false)
                );
            }
            let outside = w.root.join("outside.json");
            private_file(&outside, b"untouched");
            symlink(&outside, path.join("link.json")).unwrap();
            w.native(
                &[
                    "private",
                    "put",
                    "--repo",
                    repo_text,
                    "--workspace",
                    workspace,
                    "--name",
                    "link.json",
                ],
                raw,
                1,
            );
            assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
            let binding = path.join("binding.json");
            let original = std::fs::read(&binding).unwrap();
            assert_eq!(
                std::fs::metadata(&binding).unwrap().permissions().mode() & 0o7777,
                0o600
            );
            assert_eq!(original, format!("{{\n  \"project_id\": \"{PROJECT}\",\n  \"repo\": \"{repo_text}\",\n  \"schema\": \"agent-memory/private-workspace-v1\",\n  \"workspace\": \"{workspace}\"\n}}\n").into_bytes());
            std::fs::remove_file(&binding).unwrap();
            symlink(&outside, &binding).unwrap();
            assert!(private::put(&repo, workspace, "badbinding.json", raw).is_err());
            std::fs::remove_file(&binding).unwrap();
            private_file(&binding, &[b'x'; private::MAX_BINDING + 1]);
            assert!(private::put(&repo, workspace, "oversize.json", raw).is_err());
            private_file(&binding, &original);
            // Reordered/non-canonical binding JSON compares as a Python dictionary.
            let reordered = object([
                ("workspace", string(workspace)),
                ("project_id", string(PROJECT)),
                ("repo", string(repo_text)),
                ("schema", string("agent-memory/private-workspace-v1")),
            ]);
            private_file(
                &binding,
                json::encode(&reordered, json::DEFAULT).unwrap().as_bytes(),
            );
            private::put(&repo, workspace, "array.json", b"[1,2,3]").unwrap();
            let nonfinite = b"[NaN, Infinity, -Infinity]";
            private::put(&repo, workspace, "numbers.json", nonfinite).unwrap();
            assert_eq!(std::fs::read(path.join("numbers.json")).unwrap(), nonfinite);
            let longest = format!("{}.json", "a".repeat(128));
            private::put(&repo, workspace, &longest, b"{}").unwrap();
            let mut boundary = Vec::with_capacity(private::MAX_INPUT + 1);
            boundary.extend_from_slice(b"{\"x\":\"");
            boundary.resize(private::MAX_INPUT - 2, b'a');
            boundary.extend_from_slice(b"\"}");
            let result = w.native(
                &[
                    "private",
                    "put",
                    "--repo",
                    repo_text,
                    "--workspace",
                    workspace,
                    "--name",
                    "boundary.json",
                ],
                &boundary,
                0,
            );
            assert_eq!(
                get(&result, "bytes"),
                &Value::Int(private::MAX_INPUT.into())
            );
            assert_eq!(std::fs::read(path.join("boundary.json")).unwrap(), boundary);
            boundary.push(b' ');
            let refused = w.native(
                &[
                    "private",
                    "put",
                    "--repo",
                    repo_text,
                    "--workspace",
                    workspace,
                    "--name",
                    "overflow.json",
                ],
                &boundary,
                1,
            );
            assert_eq!(
                get(&refused, "error"),
                &string("Private input exceeds the 16 MiB bound")
            );
            assert!(!path.join("overflow.json").exists());
            let mut wrong = reordered.clone();
            set(&mut wrong, "project_id", string(OTHER));
            private_file(&binding, &encoded(&wrong).unwrap());
            assert_eq!(
                private::put(&repo, workspace, "wrong.json", raw)
                    .unwrap_err()
                    .to_string(),
                "Private workspace does not match this bound repository"
            );
            private_file(&binding, &original);
            let (fd, info) = private::checked_directory(&path).unwrap();
            private::write_fd(&fd, "race.json", raw).unwrap();
            let moved = path.with_extension("moved");
            std::fs::rename(&path, &moved).unwrap();
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            assert!(private::check_directory(&path, &info).is_err());
            assert!(!path.join("race.json").exists());
            assert_eq!(std::fs::read(moved.join("race.json")).unwrap(), raw);
            assert_eq!(snapshot(&repo), before);
            assert_eq!(snapshot(&w.root.join("config")), registry);
        },
    );
}

#[test]
fn historical_checkout_check_ignores_bank_health_but_requires_exact_registered_identity() {
    in_world(
        "historical_checkout_check_ignores_bank_health_but_requires_exact_registered_identity",
        |w| {
            w.developer();
            let repo = w.repo("repository");
            let runtime = w.root.join("runtime");
            std::fs::create_dir(&runtime).unwrap();
            let context = scope::require(&repo, None, scope::RequireOptions::default()).unwrap();
            let bank = repo.join("memory-bank");
            // Current prose, an interrupted marker, and oversized router are not read.
            std::fs::write(bank.join("MEMORY.md"), vec![b'x'; 20_000]).unwrap();
            std::fs::write(
                bank.join(".agent-memory-transaction.md"),
                b"later interruption",
            )
            .unwrap();
            let before = snapshot(&repo);
            assert_eq!(
                history::check(&repo, &context, PROJECT, &runtime).unwrap(),
                repo
            );
            assert_eq!(snapshot(&repo), before);
            assert_eq!(
                history::check(&repo, &context, OTHER, &runtime)
                    .unwrap_err()
                    .to_string(),
                "Historical receipt belongs to a different or unregistered checkout"
            );
            let mut wrong = context.clone();
            set(&mut wrong, "checkout_key", string("1:2"));
            assert!(history::check(&repo, &wrong, PROJECT, &runtime).is_err());
            let lock = agent_memory::local_registry::paths("projects").unwrap().2;
            private_file(&lock, b"busy");
            assert_eq!(
                history::check(&repo, &context, PROJECT, &runtime)
                    .unwrap_err()
                    .to_string(),
                "Historical project registry is locked"
            );
            std::fs::remove_file(lock).unwrap();
            let (mut registry, observed) = project::observe().unwrap();
            let registry_path = agent_memory::local_registry::paths("projects").unwrap().1;
            let original = encoded(&registry).unwrap();
            let key = text(&context, "checkout_key").to_owned();
            let checkout = registry
                .get_mut("checkouts")
                .unwrap()
                .get_mut(&key)
                .unwrap();
            set(
                checkout,
                "locations",
                Value::Array(vec![string(w.root.to_str().unwrap())]),
            );
            project::validate(&registry).unwrap();
            agent_memory::local_registry::replace(
                &registry_path,
                &encoded(&registry).unwrap(),
                observed.as_deref(),
            )
            .unwrap();
            assert_eq!(
                history::check(&repo, &context, PROJECT, &runtime)
                    .unwrap_err()
                    .to_string(),
                "Historical receipt belongs to a different or unregistered checkout"
            );
            let (_, changed_sha) = project::observe().unwrap();
            agent_memory::local_registry::replace(
                &registry_path,
                &original,
                changed_sha.as_deref(),
            )
            .unwrap();
            let other_repo = w.root.join("unregistered");
            std::fs::create_dir(&other_repo).unwrap();
            std::fs::create_dir(other_repo.join("memory-bank")).unwrap();
            assert!(history::check(&other_repo, &context, PROJECT, &runtime).is_err());
            assert_eq!(snapshot(&repo), before);
        },
    );
}
