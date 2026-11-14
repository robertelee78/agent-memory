//! Real filesystem and credential-free Git contracts for memory refresh.
mod support;
use agent_memory::{
    bank_contract as bank, bank_transactions as tx,
    codec::{
        frontmatter,
        json::{self, Value},
    },
    developer_identity::encoded,
    error::{ContractError, Kind},
    fs as native_fs, host, memory_refresh as refresh, records, trust_paths,
};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use support::{RunOptions, World};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn sha256_hex(raw: &[u8]) -> String {
    agent_memory::developer_identity::digest(Some(raw)).unwrap()
}
const PROJECT: &str = "75b38e17-c36e-4fab-8ba9-5fa51f25de91";
fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}
fn rows(value: &Value) -> &[Value] {
    value.as_array().unwrap()
}
fn set(value: &mut Value, key: &str, item: Value) {
    if let Some(v) = value.get_mut(key) {
        *v = item;
    } else if let Value::Object(fields) = value {
        fields.push((key.into(), item));
    } else {
        panic!("fixture object required");
    }
}
fn mode(path: &Path, bits: u32) -> TestResult {
    fs::set_permissions(path, fs::Permissions::from_mode(bits))?;
    Ok(())
}
fn save(w: &World, value: &Value, name: &str) -> TestResult<PathBuf> {
    let path = w.inside(name)?;
    fs::write(&path, encoded(value)?)?;
    mode(&path, 0o600)?;
    Ok(path)
}
fn git(w: &World, repo: &Path, args: &[&str]) -> TestResult<String> {
    let output = w.git_with(
        args,
        RunOptions {
            cwd: Some(repo),
            ..Default::default()
        },
    )?;
    Ok(String::from_utf8(output.stdout)?)
}
fn install_runtime(world: &mut World) {
    let (runtime, command) = support::installed_runtime::installed_runtime(world);
    world
        .env
        .insert("AM_REFRESH_TEST_RUNTIME".into(), runtime.into_os_string());
    world
        .env
        .insert("AM_REFRESH_TEST_COMMAND".into(), command.into_os_string());
}
fn runtime_command(world: &World) -> &OsStr {
    world
        .env
        .get(OsStr::new("AM_REFRESH_TEST_COMMAND"))
        .map(OsString::as_os_str)
        .unwrap_or(OsStr::new(env!("CARGO_BIN_EXE_am")))
}
fn call(w: &World, args: &[&OsStr]) -> TestResult<(i32, Value)> {
    let out = w.run_with(
        std::iter::once(runtime_command(w))
            .chain(std::iter::once(OsStr::new("--json")))
            .chain(args.iter().copied()),
        RunOptions {
            check: false,
            timeout: Duration::from_secs(120),
            ..Default::default()
        },
    )?;
    let code = match out.status {
        agent_memory::process::Status::Code(c) => c,
        _ => -1,
    };
    let value = json::decode_bytes(&out.stdout, Default::default()).map_err(|error| {
        format!(
            "{error}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })?;
    Ok((code, value))
}
fn good(w: &World, args: &[&OsStr]) -> TestResult<Value> {
    let (code, value) = call(w, args)?;
    assert_eq!(code, 0, "{}", json::encode(&value, json::DEFAULT)?);
    Ok(value)
}
fn os(value: &str) -> &OsStr {
    OsStr::new(value)
}
fn failure_code(error: &ContractError) -> &str {
    match error.kind() {
        Kind::RefreshCurrent { code, .. } => code,
        _ => panic!("expected CurrentFailure: {error}"),
    }
}
fn inventory_layout(w: &World) -> TestResult<(PathBuf, PathBuf, PathBuf)> {
    let state = w.paths["state"].join("agent-memory");
    let operations = state.join("writes/checkout/operations");
    fs::create_dir_all(&operations)?;
    let transactions = operations.parent().unwrap().join("transactions");
    fs::create_dir(&transactions)?;
    for directory in [
        &state,
        &state.join("writes"),
        operations.parent().unwrap(),
        &operations,
        &transactions,
    ] {
        mode(directory, 0o700)?;
    }
    Ok((state, operations, transactions))
}

// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_protected_intermediates_share_policy_without_chmod
#[test]
fn protected_intermediates_share_policy_without_chmod() -> TestResult {
    let w = World::new()?;
    let (state, operations, target) = inventory_layout(&w)?;
    for bits in [0o775, 0o777] {
        let directories = [
            state.clone(),
            state.join("writes"),
            operations.parent().unwrap().to_path_buf(),
        ];
        for path in &directories {
            mode(path, bits)?;
        }
        assert!(trust_paths::inspect(&target, Default::default())?.unix_path_policy_verified);
        assert_eq!(
            refresh::transaction_inventory(&operations)?.status,
            refresh::InventoryStatus::Empty
        );
        for path in &directories {
            assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, bits);
        }
    }
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_unprotected_writable_ancestor_rejected_by_both_walks
#[test]
fn unprotected_writable_ancestor_rejected_by_both_walks() -> TestResult {
    let w = World::new()?;
    let (state, operations, target) = inventory_layout(&w)?;
    mode(&w.root, 0o755)?;
    mode(&w.paths["state"], 0o755)?;
    mode(&state, 0o775)?;
    assert!(matches!(
        trust_paths::inspect(&target, Default::default())
            .unwrap_err()
            .kind(),
        Kind::TrustPath { .. }
    ));
    assert_eq!(
        failure_code(&refresh::transaction_inventory(&operations).unwrap_err()),
        "transaction-path-untrusted"
    );
    mode(&w.root, 0o700)?;
    mode(&w.paths["state"], 0o700)?;
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_foreign_owner_rejected_below_private_barrier
#[test]
fn foreign_owner_rejected_below_private_barrier() {
    let info = native_fs::Identity {
        dev: 1,
        ino: 2,
        mode: libc::S_IFDIR as u32 | 0o700,
        uid: host::effective_uid().saturating_add(10000),
        gid: 0,
        nlink: 2,
        size: 0,
        mtime_ns: 0,
        ctime_ns: 0,
    };
    assert!(matches!(
        trust_paths::inspect_entry(
            Path::new("/owned/private/foreign"),
            &info,
            true,
            false,
            false
        )
        .unwrap_err()
        .kind(),
        Kind::TrustPath { .. }
    ));
}
// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_redirected_intermediate_rejected_without_following
#[test]
fn redirected_intermediate_rejected_without_following() -> TestResult {
    let w = World::new()?;
    let (state, operations, target) = inventory_layout(&w)?;
    let moved = state.join("actual");
    fs::rename(state.join("writes"), &moved)?;
    symlink(&moved, state.join("writes"))?;
    assert!(trust_paths::inspect(&target, Default::default()).is_err());
    assert_eq!(
        failure_code(&refresh::transaction_inventory(&operations).unwrap_err()),
        "transaction-path-untrusted"
    );
    assert!(fs::symlink_metadata(state.join("writes"))?.is_symlink());
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_barrier_chmod_or_rebinding_during_observation_is_refused
#[test]
fn barrier_chmod_or_rebinding_during_observation_is_refused() -> TestResult {
    for change in ["chmod", "rebind"] {
        let w = World::new()?;
        let (state, operations, _) = inventory_layout(&w)?;
        let barrier = w.paths["state"].clone();
        let mut done = false;
        let observed = refresh::transaction_inventory_observed(&operations, &mut |path| {
            if path == state && !done {
                done = true;
                if change == "chmod" {
                    mode(&barrier, 0o755).unwrap();
                } else {
                    fs::rename(&barrier, w.root.join("retained-old-state")).unwrap();
                    fs::create_dir(&barrier).unwrap();
                    mode(&barrier, 0o700).unwrap();
                }
            }
            Ok(())
        });
        assert!(done);
        assert_eq!(
            failure_code(&observed.unwrap_err()),
            "current-source-changed"
        );
    }
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_orphan_recovery_is_specific_and_preserved
#[test]
fn orphan_recovery_is_specific_and_preserved() -> TestResult {
    let w = World::new()?;
    let (_, operations, target) = inventory_layout(&w)?;
    let evidence = target.join("retained.json");
    fs::write(&evidence, b"{}")?;
    let error = refresh::transaction_inventory(&operations).unwrap_err();
    assert_eq!(failure_code(&error), "transaction-recovery-pending");
    assert_eq!(fs::read(&evidence)?, b"{}");
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::TransactionPathPolicy.test_unexpected_error_text_is_not_exposed
#[test]
fn unexpected_error_text_is_not_exposed() -> TestResult {
    let value = refresh::current_result_from(Err(ContractError::new("secret-source-body")))?;
    assert_eq!(
        get(&value, "failure_code"),
        &s("refresh-prerequisite-unverified")
    );
    assert!(!json::encode(&value, json::DEFAULT)?.contains("secret-source-body"));
    assert_eq!(
        get(&value, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(&value, "remote_freshness_verified"),
        &Value::Bool(false)
    );
    Ok(())
}
#[test]
fn missing_transaction_paths_are_pinned_and_reader_refuses_parent_components() -> TestResult {
    let w = World::new()?;
    let (state, operations, target) = inventory_layout(&w)?;
    fs::remove_dir(&target)?;
    assert!(matches!(
        refresh::transaction_inventory(&operations)?.status,
        refresh::InventoryStatus::Absent(_)
    ));
    assert!(refresh::transaction_inventory(&state.join("writes/../operations")).is_err());
    Ok(())
}
#[test]
fn typed_current_failures_and_capacity_preserve_only_fixed_diagnostics() -> TestResult {
    let original = refresh::CurrentFailure::new(
        "transaction-path-untrusted",
        "Fixed path refusal.",
        "current.transaction-state",
        Some("Preserve this state path."),
        Some("/private/state".into()),
    );
    let error = original.clone().into_error();
    assert_eq!(refresh::CurrentFailure::from_error(&error), Some(original));
    let value = refresh::current_result_from(Err(error))?;
    assert_eq!(get(&value, "failure_path"), &s("/private/state"));
    assert_eq!(
        get(&value, "failure_code"),
        &s("transaction-path-untrusted")
    );
    let capacity = agent_memory::bank_capacity::BankCapacityError::new(
        agent_memory::bank_capacity::CapacityCode::SourceBytes,
        "activeContext.md",
        16384,
        16385,
        agent_memory::bank_capacity::Measurement::Exact,
    )?;
    let value = refresh::current_result_from(Err(capacity.into()))?;
    assert_eq!(get(&value, "failure_component"), &s("canonical-bank"));
    assert_eq!(get(&value, "failure_code"), &s("bank-source-byte-capacity"));
    assert_eq!(get(&value, "capacity_observed"), &Value::Int(16385.into()));
    Ok(())
}
fn bank_files() -> TestResult<Value> {
    let mut proposal = json::decode(
        include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
        Default::default(),
    )?;
    set(&mut proposal, "project-id", s(PROJECT));
    Ok(get(&bank::render(&proposal)?, "files").clone())
}
fn scope_context() -> Value {
    obj([
        ("developer_id", s("b3dcae21-f5f3-40ac-bcfe-e845a3d2868b")),
        ("project_id", s(PROJECT)),
        ("developer_sha256", s("a".repeat(64))),
        ("project_sha256", s("b".repeat(64))),
        ("checkout_key", s("1:2")),
        ("repository_settings_sha256", s("c".repeat(64))),
        ("initialization_only", Value::Bool(false)),
    ])
}
fn structural_proposal(root: &Path) -> TestResult<Value> {
    let files = bank_files()?;
    Ok(obj([
        ("schema", s(tx::SCHEMA)),
        ("operation_id", s("00000000-0000-4000-8000-000000000000")),
        ("operation", s("refresh")),
        ("repo_root", s(root.to_str().unwrap())),
        ("project_id", s(PROJECT)),
        ("observed_hashes", tx::file_hashes(&files)?),
        ("files", files),
        ("scope_context", scope_context()),
        (
            "refresh_evidence",
            obj([("proof", obj([("blocked_paths", Value::Array(vec![]))]))]),
        ),
    ]))
}
#[test]
fn complete_preparation_metadata_is_exact_without_live_semantic_or_git_reads() -> TestResult {
    agent_memory::cli::register_providers().unwrap();
    let w = World::new()?;
    let repo = w.repository("envelope-repo", false)?;
    let p = structural_proposal(&repo)?;
    let mut envelope = refresh::result(
        "prepare",
        "prepared",
        obj([
            ("conflicts", Value::Array(vec![])),
            ("blocked_paths", Value::Array(vec![])),
            ("changed_memory_paths", Value::Array(vec![])),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]),
    )?;
    set(&mut envelope, "proposal", p.clone());
    assert_eq!(refresh::prepared_proposal(&p)?, &p);
    assert_eq!(refresh::prepared_proposal(&envelope)?, &p);
    for (field, item) in [
        ("refresh_applied", Value::Bool(true)),
        ("canonical_writes_performed", Value::Int(0.into())),
        ("blocked_paths", Value::Array(vec![s("projectbrief.md")])),
        ("changed_memory_paths", Value::Array(vec![s("code.rs")])),
        ("remote_freshness_verified", Value::Bool(false)),
        ("unexpected", s("UNTRUSTED_REPORT_BODY")),
    ] {
        let mut changed = envelope.clone();
        set(&mut changed, field, item);
        let error = refresh::prepared_proposal(&changed).unwrap_err();
        assert!(matches!(error.kind(), Kind::RefreshPreparedEnvelope));
        let out = refresh::command_result(
            "apply",
            None,
            Some(&save(&w, &changed, "bad-preparation.json")?),
            None,
            None,
            false,
            Err(ContractError::new("unused")),
        );
        assert_eq!(out.exit_code, 1);
        assert!(!out.stdout.contains("UNTRUSTED_REPORT_BODY"));
        assert_eq!(
            get(out.value.as_ref().unwrap(), "failure_code"),
            &s("refresh-prepared-envelope-invalid")
        );
        assert_eq!(
            get(out.value.as_ref().unwrap(), "canonical_writes_performed"),
            &Value::Bool(false)
        );
    }
    Ok(())
}
#[test]
fn v2_reader_requires_actual_source_bound_schema_and_remains_bounded() -> TestResult {
    let w = World::new()?;
    let legacy = obj([(
        "proof",
        obj([("schema", s("agent-memory/semantic-sharing-proof-v1"))]),
    )]);
    let path = save(&w, &legacy, "legacy.json")?;
    assert_eq!(refresh::read_input(&path, false)?, legacy);
    assert!(refresh::read_input(&path, true).is_err());
    for value in [
        obj([(
            "proof",
            obj([("schema", s("agent-memory/semantic-sharing-proof-v2"))]),
        )]),
        obj([(
            "proposal",
            obj([(
                "refresh_evidence",
                obj([(
                    "proof",
                    obj([("schema", s("agent-memory/semantic-sharing-proof-v2"))]),
                )]),
            )]),
        )]),
    ] {
        let path = save(&w, &value, "v2.json")?;
        assert_eq!(refresh::read_input(&path, true)?, value);
    }
    let large = obj([("body", s("x".repeat(refresh::LIMIT)))]);
    let path = save(&w, &large, "large.json")?;
    assert!(refresh::read_input(&path, false).is_err());
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_public_cli_rejects_relative_outputs_before_any_refresh_operation
#[test]
fn public_cli_rejects_relative_outputs_before_any_refresh_operation() -> TestResult {
    let w = World::new()?;
    for route in ["refresh", "boundary"] {
        for operation in ["inspect", "prepare", "apply"] {
            let mut args = vec![os(route), os(operation), os("--v2")];
            if operation != "apply" {
                args.extend([os("--repo"), w.root.as_os_str()]);
            }
            if operation != "inspect" {
                args.extend([os("--input"), os("must-not-be-read.json")]);
            }
            args.extend([os("--output"), os("relative-output.json")]);
            let (code, value) = call(&w, &args)?;
            assert_eq!(code, 1);
            assert_eq!(
                get(&value, "failure_code"),
                &s("private-output-path-invalid")
            );
            assert_eq!(
                get(&value, "git_metadata_writes_performed"),
                &Value::Bool(false)
            );
            assert_eq!(
                get(&value, "canonical_writes_performed"),
                &Value::Bool(false)
            );
            assert!(!w.root.join("relative-output.json").exists());
        }
    }
    Ok(())
}

// Complete real refresh journeys below wait only for the production recovery and
// semantic providers. Their fixture never installs a semantic/security mock.
#[derive(PartialEq, Debug)]
struct State {
    bank: BTreeMap<String, Vec<u8>>,
    code: Vec<u8>,
    index: Vec<u8>,
    head: String,
    branch: String,
    unrelated: Option<Vec<u8>>,
}
fn canonical(root: &Path) -> TestResult<BTreeMap<String, Vec<u8>>> {
    fn visit(root: &Path, node: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> TestResult {
        for entry in fs::read_dir(node)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                visit(root, &path, out)?;
            } else if entry.file_type()?.is_file() {
                out.insert(
                    path.strip_prefix(root)?.to_string_lossy().into_owned(),
                    fs::read(path)?,
                );
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out)?;
    Ok(out)
}
fn state(w: &World, repo: &Path) -> TestResult<State> {
    let index = PathBuf::from(
        git(
            w,
            repo,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        )?
        .trim(),
    );
    Ok(State {
        bank: canonical(&repo.join("memory-bank"))?,
        code: fs::read(repo.join("code.rs"))?,
        index: fs::read(index)?,
        head: git(w, repo, &["rev-parse", "HEAD"])?,
        branch: git(w, repo, &["symbolic-ref", "HEAD"])?,
        unrelated: fs::read(repo.join("unrelated.txt")).ok(),
    })
}
fn feature_unchanged(before: &State, after: &State) {
    assert_eq!(before.code, after.code);
    assert_eq!(before.index, after.index);
    assert_eq!(before.head, after.head);
    assert_eq!(before.branch, after.branch);
    assert_eq!(before.unrelated, after.unrelated);
}
struct Fixture {
    world: World,
    repo: PathBuf,
    peer: PathBuf,
    remote: PathBuf,
    base: String,
}
impl Fixture {
    fn new() -> TestResult<Self> {
        let mut w = World::new()?;
        install_runtime(&mut w);
        let repo = w.repository("repo", false)?;
        let remote = w.repository("remote.git", true)?;
        git(
            &w,
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )?;
        git(
            &w,
            &repo,
            &["config", "--local", "agent-memory.authorityBranch", "main"],
        )?;
        let bank_path = repo.join("memory-bank");
        fs::create_dir(&bank_path)?;
        let mut files = bank_files()?;
        let (mut router, mut routes, omitted) = bank::parse_router(text(get(&files, "MEMORY.md")))?;
        let mut omitted = omitted;
        set(&mut router, "router-updated", s("2026-10-22T18:00:00Z"));
        if let Value::Object(fields) = &mut omitted {
            fields.retain(|(key, _)| key != "activeContext.md");
        }
        if !routes
            .iter()
            .any(|r| get(r, "path") == &s("activeContext.md"))
        {
            routes.push(obj([
                ("path", s("activeContext.md")),
                ("purpose", s("Session focus")),
                ("scope", s("repository")),
                ("lifecycle", s("active")),
                ("read-when", s("At a boundary")),
            ]));
        }
        set(
            &mut files,
            "activeContext.md",
            s("# Current focus\n\nA useful isolated session focus.\n"),
        );
        set(
            &mut files,
            "MEMORY.md",
            s(bank::router_text(&router, &routes, &omitted)?),
        );
        for (path, body) in files.as_object().unwrap() {
            fs::write(bank_path.join(path), text(body))?;
        }
        fs::write(repo.join("code.rs"), b"const VALUE: u32 = 1;\n")?;
        support::identity_fixtures::bind(&w, &repo)?;
        git(&w, &repo, &["add", "."])?;
        git(&w, &repo, &["commit", "-m", "Fixture canonical memory"])?;
        git(&w, &repo, &["push", "origin", "HEAD:main"])?;
        let base = git(&w, &repo, &["rev-parse", "HEAD"])?.trim().to_owned();
        let peer = w.clone(&remote, "peer")?;
        git(&w, &repo, &["checkout", "-b", "feature-session"])?;
        fs::write(
            repo.join("code.rs"),
            b"const VALUE: u32 = 2; // unfinished feature ancestor\n",
        )?;
        git(&w, &repo, &["add", "code.rs"])?;
        git(
            &w,
            &repo,
            &["commit", "-m", "Unfinished local feature ancestor"],
        )?;
        fs::write(
            repo.join("code.rs"),
            b"const VALUE: u32 = 3; // staged work\n",
        )?;
        git(&w, &repo, &["add", "code.rs"])?;
        fs::write(
            repo.join("unrelated.txt"),
            b"Unstaged independent local work\n",
        )?;
        Ok(Self {
            world: w,
            repo,
            peer,
            remote,
            base,
        })
    }
    fn bank(&self) -> PathBuf {
        self.repo.join("memory-bank")
    }
    fn refresh(&self, operation: &str, input: Option<&Value>) -> TestResult<(i32, Value)> {
        let path = input
            .map(|v| save(&self.world, v, "refresh-input.json"))
            .transpose()?;
        let mut args = vec![os("refresh"), os(operation)];
        if operation != "apply" {
            args.extend([os("--repo"), self.repo.as_os_str()]);
        }
        if let Some(path) = &path {
            args.extend([os("--input"), path.as_os_str()]);
        }
        call(&self.world, &args)
    }
    fn inspect(&self, base: Option<&str>, v2: bool) -> TestResult<Value> {
        let mut args = vec![
            os("refresh"),
            os("inspect"),
            os("--repo"),
            self.repo.as_os_str(),
        ];
        if let Some(base) = base {
            args.extend([os("--base"), os(base)]);
        }
        if v2 {
            args.push(os("--v2"));
        }
        good(&self.world, &args)
    }
    fn add(
        &self,
        repo: &Path,
        name: &str,
        body: &str,
        conditions: Value,
    ) -> TestResult<(Value, String)> {
        let bank_path = repo.join("memory-bank");
        let id = host::uuid4()?;
        let content = support::record_fixtures::record_text_with_ids(
            body,
            PROJECT,
            support::record_fixtures::RecordOptions {
                scope: "repository",
                kind: "semantic-fact",
                status: "active",
            },
            &id,
            "b3dcae21-f5f3-40ac-bcfe-e845a3d2868b",
        )?;
        let (_, mut metadata, _) = records::parse(&content, &[])?;
        set(&mut metadata, "conditions", conditions.clone());
        set(
            &mut metadata,
            "applicability",
            s(
                if conditions
                    .as_object()
                    .unwrap()
                    .iter()
                    .any(|(_, v)| v == &Value::Null)
                {
                    "unknown"
                } else {
                    "known"
                },
            ),
        );
        let path = format!("topics/{name}.md");
        fs::create_dir_all(bank_path.join("topics"))?;
        fs::write(
            bank_path.join(&path),
            frontmatter::write(metadata.as_object().unwrap())? + body,
        )?;
        let (mut router, mut routes, omitted) =
            bank::parse_router(&fs::read_to_string(bank_path.join("MEMORY.md"))?)?;
        set(&mut router, "router-updated", s(host::utc_now_isoformat()?));
        routes.push(obj([
            ("path", s(&path)),
            ("purpose", s("Reviewed fixture learning")),
            ("scope", s("repository")),
            ("lifecycle", s("active")),
            ("read-when", s("When comparing related concepts")),
        ]));
        fs::write(
            bank_path.join("MEMORY.md"),
            bank::router_text(&router, &routes, &omitted)?,
        )?;
        Ok((metadata, path))
    }
    fn imported(&self, name: &str, body: &str) -> TestResult<(Value, String)> {
        let record = self.add(&self.peer, name, body, obj([]))?;
        self.push()?;
        Ok(record)
    }
    fn push(&self) -> TestResult {
        git(
            &self.world,
            &self.peer,
            &["add", "--all", "--", "memory-bank"],
        )?;
        git(
            &self.world,
            &self.peer,
            &["commit", "-m", "Reviewed shared fixture learning"],
        )?;
        git(&self.world, &self.peer, &["push", "origin", "HEAD:main"])?;
        Ok(())
    }
    fn recall(&self) -> Value {
        obj([
            ("project_id", s(PROJECT)),
            ("query", s("")),
            ("ids", Value::Array(vec![])),
            ("paths", Value::Array(vec![])),
            ("context", obj([])),
            (
                "kinds",
                get(get(get(&records::MODEL, "properties"), "kind"), "enum").clone(),
            ),
            ("minimum_trust", s("reviewed")),
            ("limit", Value::Int(64.into())),
            ("include_context", Value::Bool(true)),
        ])
    }
    fn prepare_with(
        &self,
        recall: Value,
        base: Option<&str>,
        conflict: Option<(&str, &str)>,
        dependencies: Value,
    ) -> TestResult<Value> {
        let inspection = get(&self.inspect(base, false)?, "inspection").clone();
        let source_paths: BTreeMap<&str, &str> = rows(get(&inspection, "sources"))
            .iter()
            .map(|row| (text(get(row, "key")), text(get(row, "path"))))
            .collect();
        let judgments = rows(get(&inspection, "required_pairs"))
            .iter()
            .map(|pair| {
                let pair = rows(pair);
                let left = pair.first().unwrap();
                let right = pair.get(1).unwrap();
                let contradictory = conflict.is_some_and(|(a, b)| {
                    let l = source_paths[text(left)];
                    let r = source_paths[text(right)];
                    (l == a && r == b) || (l == b && r == a)
                });
                obj([
                    ("left", left.clone()),
                    ("right", right.clone()),
                    (
                        "classification",
                        s(if contradictory {
                            "contradiction"
                        } else {
                            "related"
                        }),
                    ),
                    (
                        "reason",
                        s("Actual complete source meaning reviewed for this isolated case."),
                    ),
                ])
            })
            .collect();
        let judgment = obj([
            ("binding", get(&inspection, "binding").clone()),
            ("pairs", Value::Array(judgments)),
            ("dependencies", dependencies),
            ("consolidations", Value::Array(vec![])),
            (
                "rationale",
                s("Preserve actual source conditions and every origin; no landing-order truth."),
            ),
            ("recorded_at", s(host::utc_now_isoformat()?)),
        ]);
        let input = save(&self.world, &judgment, "judgment.json")?;
        let reviewed = good(
            &self.world,
            &[
                os("semantic"),
                os("review"),
                os("--repo"),
                self.repo.as_os_str(),
                os("--input"),
                input.as_os_str(),
            ],
        )?;
        let (code, preparation) = self.refresh(
            "prepare",
            Some(&obj([
                ("proof", get(&reviewed, "proof").clone()),
                ("recall", recall),
            ])),
        )?;
        assert_eq!(code, 0, "{}", json::encode(&preparation, json::DEFAULT)?);
        Ok(preparation)
    }
    fn prepared(&self) -> TestResult<Value> {
        self.prepare_with(self.recall(), None, None, obj([]))
    }
    fn apply(&self, prepared: &Value) -> TestResult<Value> {
        let (code, done) = self.refresh("apply", Some(prepared))?;
        assert_eq!(code, 0, "{}", json::encode(&done, json::DEFAULT)?);
        Ok(done)
    }
    fn cursor_path(&self) -> TestResult<PathBuf> {
        let key = sha256_hex(self.bank().to_str().unwrap().as_bytes());
        Ok(PathBuf::from(
            self.world
                .env
                .get(OsStr::new("AGENT_MEMORY_STATE_HOME"))
                .unwrap(),
        )
        .join("writes")
        .join(key)
        .join("refresh-cursor.json"))
    }
}

// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_second_clone_import_is_recalled_with_conditions_without_code_index_or_local_learning_loss
#[test]
#[ignore = "needs recovery_frontier registered"]
fn second_clone_import_preserves_feature_state_and_recalls_conditions() -> TestResult {
    let f = Fixture::new()?;
    fs::write(
        f.bank().join("activeContext.md"),
        "# Current focus\n\nUnpublished local learning must survive.\n",
    )?;
    fs::write(
        f.peer.join("code.rs"),
        "const VALUE: u32 = 900; // newer authority code remains outside this branch\n",
    )?;
    git(&f.world, &f.peer, &["add", "code.rs"])?;
    let (record, path) = f.add(
        &f.peer,
        "incoming",
        "# Incoming\n\nA durable same-project observation.\n",
        obj([("component", s("code.rs"))]),
    )?;
    f.push()?;
    let before = state(&f.world, &f.repo)?;
    let mut recall = f.recall();
    set(
        &mut recall,
        "ids",
        Value::Array(vec![get(&record, "id").clone()]),
    );
    set(&mut recall, "context", obj([("component", s("code.rs"))]));
    let preparation = f.prepare_with(recall, None, None, obj([]))?;
    assert_eq!(before, state(&f.world, &f.repo)?);
    let done = f.apply(&preparation)?;
    assert_eq!(get(&done, "sharing_state"), &s("refreshed"));
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert_eq!(
        get(&done, "feature_code_freshness_verified"),
        &Value::Bool(false)
    );
    assert!(f.bank().join(&path).is_file());
    assert!(
        fs::read_to_string(f.bank().join("activeContext.md"))?
            .contains("Unpublished local learning")
    );
    assert_eq!(
        get(&rows(get(get(&done, "recall"), "results"))[0], "path"),
        &s(&path)
    );
    assert_eq!(
        get(&rows(get(get(&done, "recall"), "results"))[0], "record"),
        &record
    );
    assert_eq!(
        get(&done, "recalled_import_paths"),
        &Value::Array(vec![s(&path)])
    );
    for key in ["refresh_applied", "relevant_imports_read"] {
        assert_eq!(get(&done, key), &Value::Bool(true));
    }
    for key in ["stored", "content_free"] {
        assert_eq!(get(get(&done, "cursor"), key), &Value::Bool(true));
    }
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    let replay = f.apply(&preparation)?;
    assert_eq!(
        get(get(&replay, "receipt"), "already_applied"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(&replay, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(get(&done, "recall"), "results"),
        get(get(&replay, "recall"), "results")
    );
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_equal_bank_and_orphan_journal
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_equal_bank_and_orphan_journal() -> TestResult {
    let f = Fixture::new()?;
    let before = state(&f.world, &f.repo)?;
    let (code, current) = f.refresh("current", None)?;
    assert_eq!(code, 0);
    assert_eq!(get(&current, "sharing_state"), &s("bank-synchronized"));
    assert_eq!(get(&current, "pending_capture"), &s("unknown"));
    assert_eq!(get(&current, "pending_publication"), &s("unknown"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    let evidence = f
        .cursor_path()?
        .parent()
        .unwrap()
        .join("transactions/unknown/journal.json");
    fs::create_dir_all(evidence.parent().unwrap())?;
    fs::write(&evidence, b"{}")?;
    let (code, pending) = f.refresh("current", None)?;
    assert_eq!(code, 1);
    assert_eq!(
        get(&pending, "failure_code"),
        &s("transaction-recovery-pending")
    );
    assert_eq!(fs::read(&evidence)?, b"{}");
    assert_eq!(before, state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_preserves_private_and_read_only_bank_permissions
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_preserves_private_and_read_only_bank_permissions() -> TestResult {
    let f = Fixture::new()?;
    let before = state(&f.world, &f.repo)?;
    let paths = before
        .bank
        .keys()
        .filter(|p| p.ends_with(".md"))
        .map(|p| f.bank().join(p))
        .collect::<Vec<_>>();
    for bits in [0o600, 0o400, 0o440] {
        for path in &paths {
            mode(path, bits)?;
        }
        let (code, current) = f.refresh("current", None)?;
        assert_eq!(code, 0, "{}", json::encode(&current, json::DEFAULT)?);
        assert_eq!(get(&current, "sharing_state"), &s("bank-synchronized"));
        for path in &paths {
            assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, bits);
        }
        assert_eq!(before, state(&f.world, &f.repo)?);
    }
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_rejects_executable_and_world_writable_bank_files
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_rejects_executable_and_world_writable_bank_files() -> TestResult {
    for bits in [0o755, 0o666] {
        let f = Fixture::new()?;
        let target = f.bank().join("activeContext.md");
        mode(&target, bits)?;
        let before = state(&f.world, &f.repo)?;
        let (code, value) = f.refresh("current", None)?;
        assert_eq!(code, 1);
        assert_eq!(get(&value, "sharing_state"), &s("pending-inspection"));
        assert_eq!(
            get(&value, "remote_freshness_verified"),
            &Value::Bool(false)
        );
        assert_eq!(before, state(&f.world, &f.repo)?);
        assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o777, bits);
    }
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_untracked_canonical_memory_explains_full_review_without_writes
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_untracked_canonical_memory_explains_full_review_without_writes() -> TestResult {
    let f = Fixture::new()?;
    f.add(
        &f.repo,
        "untracked",
        "# Local\n\nAn untracked local record requires source review.\n",
        obj([]),
    )?;
    let before = state(&f.world, &f.repo)?;
    let (code, value) = f.refresh("current", None)?;
    assert_eq!(code, 1);
    assert_eq!(
        get(&value, "failure_code"),
        &s("canonical-memory-untracked")
    );
    assert!(text(get(&value, "repair")).contains("do not stage or reset"));
    assert_eq!(
        get(&value, "git_metadata_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(before, state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_refresh_pending.py::RefreshPendingContract.test_late_local_edit_and_same_commit_branch_change_refuse_the_original_proposal
#[test]
#[ignore = "needs recovery_frontier registered"]
fn late_local_edit_and_same_commit_branch_change_refuse_original_proposal() -> TestResult {
    let f = Fixture::new()?;
    f.imported(
        "incoming",
        "# Incoming\n\nReview actual incoming learning.\n",
    )?;
    let prepared = f.prepared()?;
    fs::write(
        f.bank().join("activeContext.md"),
        "# Current focus\n\nA late unpublished edit must not be lost.\n",
    )?;
    let before = state(&f.world, &f.repo)?;
    let (code, _) = f.refresh("apply", Some(&prepared))?;
    assert_eq!(code, 1);
    assert_eq!(before, state(&f.world, &f.repo)?);
    let fresh = f.prepared()?;
    git(
        &f.world,
        &f.repo,
        &["checkout", "-b", "different-session-branch"],
    )?;
    let changed = state(&f.world, &f.repo)?;
    let (code, _) = f.refresh("apply", Some(&fresh))?;
    assert_eq!(code, 1);
    assert_eq!(changed, state(&f.world, &f.repo)?);
    assert!(!f.bank().join(tx::MARKER).exists());
    Ok(())
}
// obligation: tests/contracts/test_refresh_pending.py::RefreshPendingContract.test_new_authority_and_offline_observation_require_fresh_review_before_import
#[test]
#[ignore = "needs recovery_frontier registered"]
fn new_authority_and_offline_observation_require_fresh_review_before_import() -> TestResult {
    let f = Fixture::new()?;
    let (_, first) = f.imported("first", "# First\n\nInitial shared evidence.\n")?;
    let prepared = f.prepared()?;
    let before = state(&f.world, &f.repo)?;
    let away = f.remote.with_file_name("offline.git");
    fs::rename(&f.remote, &away)?;
    let (code, value) = f.refresh("apply", Some(&prepared))?;
    assert_eq!(code, 1);
    assert_eq!(get(&value, "local_memory_available"), &Value::Bool(true));
    assert_eq!(before, state(&f.world, &f.repo)?);
    fs::rename(&away, &f.remote)?;
    let (_, second) = f.imported(
        "second",
        "# Second\n\nNewly landed evidence requires review too.\n",
    )?;
    let (code, _) = f.refresh("apply", Some(&prepared))?;
    assert_eq!(code, 1);
    assert_eq!(before, state(&f.world, &f.repo)?);
    let done = f.apply(&f.prepared()?)?;
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert!(f.bank().join(first).exists());
    assert!(f.bank().join(second).exists());
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_refresh_pending.py::RefreshPendingContract.test_changed_candidate_or_foreign_recall_project_cannot_bypass_exact_source_review
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_candidate_or_foreign_recall_project_cannot_bypass_source_review() -> TestResult {
    let f = Fixture::new()?;
    let (_, path) = f.imported("incoming", "# Incoming\n\nPreserve this actual evidence.\n")?;
    let prepared = f.prepared()?;
    let original = get(&prepared, "proposal");
    let before = state(&f.world, &f.repo)?;
    let mut variants = Vec::new();
    let mut changed = original.clone();
    let files = changed.get_mut("files").unwrap();
    let body = format!("{}\nUnreviewed extra claim.\n", text(get(files, &path)));
    set(files, &path, s(body));
    variants.push(changed);
    let mut foreign = original.clone();
    set(
        foreign
            .get_mut("refresh_evidence")
            .unwrap()
            .get_mut("recall")
            .unwrap(),
        "project_id",
        s(host::uuid4()?),
    );
    variants.push(foreign);
    let mut branch = original.clone();
    set(
        branch
            .get_mut("refresh_evidence")
            .unwrap()
            .get_mut("recall")
            .unwrap()
            .get_mut("context")
            .unwrap(),
        "branch",
        s("unobserved-branch"),
    );
    variants.push(branch);
    let mut missing = original.clone();
    if let Value::Object(fields) = missing.get_mut("refresh_evidence").unwrap() {
        fields.retain(|(k, _)| k != "recovery_frontier_token");
    }
    variants.push(missing);
    let mut stale = original.clone();
    set(
        stale
            .get_mut("refresh_evidence")
            .unwrap()
            .get_mut("recovery_frontier_token")
            .unwrap(),
        "index_sha256",
        s("f".repeat(64)),
    );
    variants.push(stale);
    for value in variants {
        let (code, _) = f.refresh("apply", Some(&value))?;
        assert_eq!(code, 1);
        assert_eq!(before, state(&f.world, &f.repo)?);
    }
    assert!(!f.bank().join(tx::MARKER).exists());
    Ok(())
}
// obligation: tests/contracts/test_refresh_v2.py::RefreshVersionTwoContract.test_explicit_v2_inspection_uses_changed_learning_frontier_and_retains_the_actual_base
#[test]
#[ignore = "needs recovery_frontier registered"]
fn explicit_v2_inspection_preserves_actual_base_and_feature_state() -> TestResult {
    let f = Fixture::new()?;
    let before = state(&f.world, &f.repo)?;
    let v2 = f.inspect(Some(&f.base), true)?;
    assert_eq!(
        get(get(&v2, "inspection"), "schema"),
        &s("agent-memory/semantic-sharing-inspection-v2")
    );
    assert_eq!(
        get(get(get(&v2, "inspection"), "request"), "base"),
        &s(&f.base)
    );
    assert_eq!(
        get(get(get(&v2, "inspection"), "binding"), "base"),
        &s(&f.base)
    );
    assert_eq!(get(&v2, "baseline_source"), &s("explicit"));
    assert_eq!(get(&v2, "refresh_applied"), &Value::Bool(false));
    assert_eq!(get(&v2, "canonical_writes_performed"), &Value::Bool(false));
    let legacy = f.inspect(Some(&f.base), false)?;
    assert_eq!(
        get(get(&legacy, "inspection"), "schema"),
        &s("agent-memory/semantic-sharing-inspection-v1")
    );
    assert_eq!(before, state(&f.world, &f.repo)?);
    Ok(())
}
fn cursor_checkout_change(switch_branch: bool) -> TestResult {
    let f = Fixture::new()?;
    let (record, path) = f.imported(
        "shared-rule",
        "# Shared rule\n\nRetain the stable retry identity.\n",
    )?;
    let mut recall = f.recall();
    set(
        &mut recall,
        "ids",
        Value::Array(vec![get(&record, "id").clone()]),
    );
    let first = f.apply(&f.prepare_with(recall.clone(), None, None, obj([]))?)?;
    assert_eq!(
        get(
            get(&rows(get(get(&first, "recall"), "results"))[0], "record"),
            "id"
        ),
        get(&record, "id")
    );
    git(
        &f.world,
        &f.repo,
        &[
            "stash",
            "push",
            "-u",
            "-m",
            "Caller preserves imported memory",
            "--",
            "memory-bank",
        ],
    )?;
    assert!(!f.bank().join(&path).exists());
    let stash = git(&f.world, &f.repo, &["rev-parse", "refs/stash"])?;
    if switch_branch {
        git(
            &f.world,
            &f.repo,
            &["checkout", "-b", "another-session-branch"],
        )?;
    } else {
        git(
            &f.world,
            &f.repo,
            &[
                "commit",
                "-m",
                "Caller commits staged feature code after stashing memory",
            ],
        )?;
    }
    fs::write(
        f.bank().join("activeContext.md"),
        "# Current focus\n\nUnpublished learning in the current checkout must survive.\n",
    )?;
    let before = state(&f.world, &f.repo)?;
    let view = f.inspect(None, false)?;
    assert_eq!(get(&view, "baseline_source"), &s("git-common-base"));
    assert_eq!(get(&view, "cursor_state"), &s("checkout-context-changed"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    let imported = f.apply(&f.prepare_with(recall, None, None, obj([]))?)?;
    assert_eq!(
        get(&imported, "remote_freshness_verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(
            get(&rows(get(get(&imported, "recall"), "results"))[0], "record"),
            "id"
        ),
        get(&record, "id")
    );
    assert!(f.bank().join(&path).exists());
    assert!(
        fs::read_to_string(f.bank().join("activeContext.md"))?.contains("Unpublished learning")
    );
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    assert_eq!(stash, git(&f.world, &f.repo, &["rev-parse", "refs/stash"])?);
    let current = f.inspect(None, false)?;
    assert_eq!(get(&current, "baseline_source"), &s("cursor"));
    assert_eq!(get(&current, "cursor_state"), &s("verified-local-hint"));
    Ok(())
}
// obligation: tests/contracts/test_refresh_cursor.py::RefreshCursorContract.test_stashed_import_and_same_head_branch_switch_reimports_the_shared_lesson
#[test]
#[ignore = "needs recovery_frontier registered"]
fn stashed_import_and_same_head_branch_switch_reimports_shared_lesson() -> TestResult {
    cursor_checkout_change(true)
}
// obligation: tests/contracts/test_refresh_cursor.py::RefreshCursorContract.test_stashed_import_and_new_revision_on_same_branch_reimports_the_shared_lesson
#[test]
#[ignore = "needs recovery_frontier registered"]
fn stashed_import_and_new_revision_reimports_shared_lesson() -> TestResult {
    cursor_checkout_change(false)
}
// obligation: tests/contracts/test_refresh_receipt_snapshot.py::ReceiptSnapshotCurrent.test_import_then_current_then_full_noop_then_current_preserves_feature_state
#[test]
#[ignore = "needs recovery_frontier registered"]
fn import_then_current_and_full_noop_preserve_feature_state() -> TestResult {
    let f = Fixture::new()?;
    fs::write(
        f.peer.join("memory-bank/activeContext.md"),
        "# Active context\n\nReviewed imported context edit.\n",
    )?;
    f.imported(
        "receipt-covered",
        "# Receipt-covered lesson\n\nA durable same-project fact.\n",
    )?;
    let done = f.apply(&f.prepared()?)?;
    assert_eq!(get(&done, "sharing_state"), &s("refreshed"));
    let before = state(&f.world, &f.repo)?;
    for after_noop in [false, true] {
        if after_noop {
            let noop = f.apply(&f.prepared()?)?;
            assert_eq!(get(&noop, "remote_freshness_verified"), &Value::Bool(true));
        }
        let (code, current) = f.refresh("current", None)?;
        assert_eq!(code, 0, "{}", json::encode(&current, json::DEFAULT)?);
        assert_eq!(get(&current, "sharing_state"), &s("bank-synchronized"));
        assert_eq!(get(&current, "pending_capture"), &s("unknown"));
        assert_eq!(get(&current, "pending_publication"), &s("unknown"));
        assert_eq!(before, state(&f.world, &f.repo)?);
    }
    assert!(
        fs::read_to_string(f.bank().join("activeContext.md"))?
            .contains("Reviewed imported context edit")
    );
    Ok(())
}
// obligation: tests/contracts/test_refresh_conflicts.py::RefreshConflictsContract.test_conflict_and_real_dependency_are_preserved_but_not_returned_as_current_guidance
#[test]
#[ignore = "needs recovery_frontier registered"]
fn conflict_and_real_dependency_preserved_but_never_current_guidance() -> TestResult {
    let f = Fixture::new()?;
    let (_, local) = f.add(
        &f.repo,
        "yellow",
        "# Wall\n\nThe room wall should be yellow.\n",
        obj([]),
    )?;
    let (_, child) = f.add(
        &f.repo,
        "paint-order",
        "# Order\n\nThis order depends on the local wall color.\n",
        obj([]),
    )?;
    let (_, landed) = f.add(
        &f.peer,
        "blue",
        "# Wall\n\nThe same room wall should be blue.\n",
        obj([]),
    )?;
    fs::write(
        f.peer.join("memory-bank/projectbrief.md"),
        "# Project\n\nIndependent landed orientation still arrives.\n",
    )?;
    f.push()?;
    let before = state(&f.world, &f.repo)?;
    let dependencies = Value::Object(vec![(child.clone(), Value::Array(vec![s(&local)]))]);
    let preparation = f.prepare_with(f.recall(), None, Some((&local, &landed)), dependencies)?;
    assert_eq!(rows(get(&preparation, "conflicts")).len(), 1);
    let done = f.apply(&preparation)?;
    assert_eq!(get(&done, "sharing_state"), &s("refreshed-with-conflicts"));
    assert_eq!(get(&done, "conflicts_pending"), &Value::Bool(true));
    assert!(f.bank().join(&local).exists());
    assert!(f.bank().join(&child).exists());
    assert!(!f.bank().join(&landed).exists());
    assert!(fs::read_to_string(f.bank().join("projectbrief.md"))?.contains("Independent landed"));
    for row in rows(get(get(&done, "recall"), "results")) {
        assert!(![&local, &child, &landed].contains(&&text(get(row, "path")).to_owned()));
    }
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_refresh_conflicts.py::RefreshConflictsContract.test_held_claims_do_not_consume_a_small_budget_and_starve_an_independent_import
#[test]
#[ignore = "needs recovery_frontier registered"]
fn held_claims_do_not_starve_independent_import() -> TestResult {
    let f = Fixture::new()?;
    let (_, local) = f.add(
        &f.repo,
        "yellow",
        "# Wall\n\nThe wall should be yellow.\n",
        obj([]),
    )?;
    let (_, child) = f.add(
        &f.repo,
        "paint-order",
        "# Order\n\nThis depends on the wall decision.\n",
        obj([]),
    )?;
    let (_, landed) = f.add(
        &f.peer,
        "blue",
        "# Wall\n\nThe same wall should be blue.\n",
        obj([]),
    )?;
    let (_, independent) = f.add(
        &f.peer,
        "z-independent",
        "# Independent\n\nAn independent observation remains usable.\n",
        obj([]),
    )?;
    f.push()?;
    let mut recall = f.recall();
    set(
        &mut recall,
        "paths",
        Value::Array([&local, &child, &independent].into_iter().map(s).collect()),
    );
    set(&mut recall, "limit", Value::Int(1.into()));
    let before = state(&f.world, &f.repo)?;
    let prep = f.prepare_with(
        recall,
        None,
        Some((&local, &landed)),
        Value::Object(vec![(child, Value::Array(vec![s(&local)]))]),
    )?;
    let done = f.apply(&prep)?;
    assert_eq!(get(&done, "conflicts_pending"), &Value::Bool(true));
    let results = rows(get(get(&done, "recall"), "results"));
    assert_eq!(results.len(), 1);
    assert_eq!(get(&results[0], "path"), &s(independent));
    assert_eq!(
        get(get(get(&done, "recall"), "metrics"), "selected"),
        &Value::Int(1.into())
    );
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    Ok(())
}

// obligation: tests/contracts/test_refresh_invalidation.py::RefreshInvalidationContract.test_imported_unknown_or_other_branch_conditions_stay_out_of_current_recall
#[test]
#[ignore = "needs recovery_frontier registered"]
fn imported_unknown_and_other_branch_conditions_stay_out_of_current_recall() -> TestResult {
    let f = Fixture::new()?;
    let (unknown, one) = f.add(
        &f.peer,
        "unknown",
        "# Unknown applicability\n\nIts component conditions still need clarification.\n",
        obj([("component", Value::Null)]),
    )?;
    let (other, two) = f.add(
        &f.peer,
        "other-branch",
        "# Other branch\n\nThis applies only to a different branch.\n",
        obj([("branch", s("another-feature"))]),
    )?;
    f.push()?;
    let before = state(&f.world, &f.repo)?;
    let mut recall = f.recall();
    set(
        &mut recall,
        "ids",
        Value::Array(vec![get(&unknown, "id").clone(), get(&other, "id").clone()]),
    );
    let done = f.apply(&f.prepare_with(recall, None, None, obj([]))?)?;
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert!(rows(get(get(&done, "recall"), "results")).is_empty());
    assert!(rows(get(&done, "recalled_import_paths")).is_empty());
    assert!(f.bank().join(one).exists());
    assert!(f.bank().join(two).exists());
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_refresh_invalidation.py::RefreshInvalidationContract.test_landed_tombstone_removes_a_locally_edited_old_record_without_staging_or_resurrection
#[test]
#[ignore = "needs recovery_frontier registered"]
fn landed_tombstone_defeats_locally_edited_guidance_without_staging() -> TestResult {
    let f = Fixture::new()?;
    let (record, path) = f.imported(
        "old",
        "# Old lesson\n\nThis guidance will be explicitly erased.\n",
    )?;
    let first = f.apply(&f.prepared()?)?;
    assert_eq!(get(&first, "remote_freshness_verified"), &Value::Bool(true));
    git(&f.world, &f.repo, &["add", "memory-bank"])?;
    git(
        &f.world,
        &f.repo,
        &[
            "commit",
            "--only",
            "-m",
            "Ordinary local memory checkpoint",
            "--",
            "memory-bank",
        ],
    )?;
    let old = f.bank().join(&path);
    let changed = fs::read_to_string(&old)?
        + "\nUnpublished old-body edit must not restore erased guidance.\n";
    fs::write(&old, changed)?;
    let metadata = obj([
        ("id", get(&record, "id").clone()),
        ("scope", get(&record, "project-id").clone()),
        ("deleted-at", s(host::utc_now_isoformat()?)),
        ("deletion-authority", s(host::uuid4()?)),
        ("reason-class", s("user-request")),
    ]);
    let tomb_path = format!("history/tombstones/{}.md", text(get(&record, "id")));
    let tomb_content = frontmatter::write(metadata.as_object().unwrap())?;
    let shared = f.peer.join("memory-bank");
    fs::remove_file(shared.join(&path))?;
    fs::create_dir_all(shared.join("history/tombstones"))?;
    fs::write(shared.join(&tomb_path), &tomb_content)?;
    let (router, mut routes, omitted) =
        bank::parse_router(&fs::read_to_string(shared.join("MEMORY.md"))?)?;
    routes.retain(|r| get(r, "path") != &s(&path));
    routes.push(obj([
        ("path", s(&tomb_path)),
        ("purpose", s("Terminal deletion guard")),
        ("scope", s("repository")),
        ("lifecycle", s("deleted")),
        ("read-when", s("Before any reuse")),
    ]));
    fs::write(
        shared.join("MEMORY.md"),
        bank::router_text(&router, &routes, &omitted)?,
    )?;
    f.push()?;
    let before = state(&f.world, &f.repo)?;
    let mut recall = f.recall();
    set(
        &mut recall,
        "ids",
        Value::Array(vec![get(&record, "id").clone()]),
    );
    let prep = f.prepare_with(recall, None, None, obj([]))?;
    let done = f.apply(&prep)?;
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert!(!old.exists());
    assert_eq!(fs::read_to_string(f.bank().join(&tomb_path))?, tomb_content);
    assert!(rows(get(get(&done, "recall"), "results")).is_empty());
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    let repeated = f.apply(&prep)?;
    assert_eq!(
        get(get(&repeated, "receipt"), "already_applied"),
        &Value::Bool(true)
    );
    assert!(!old.exists());
    assert!(rows(get(get(&repeated, "recall"), "results")).is_empty());
    Ok(())
}
// obligation: tests/contracts/test_refresh_integration.py::RefreshIntegrationContract.test_linked_worktree_import_does_not_touch_sibling_bank_index_or_feature_heads
#[test]
#[ignore = "needs recovery_frontier registered"]
fn linked_worktree_import_preserves_original_and_sibling_feature_state() -> TestResult {
    let mut f = Fixture::new()?;
    let sibling = f.world.inside("other session worktree")?;
    git(
        &f.world,
        &f.repo,
        &[
            "worktree",
            "add",
            "-b",
            "second-session",
            sibling.to_str().unwrap(),
            &f.base,
        ],
    )?;
    fs::write(
        sibling.join("code.rs"),
        "const VALUE: u32 = 77; // staged sibling\n",
    )?;
    git(&f.world, &sibling, &["add", "code.rs"])?;
    fs::write(
        sibling.join("memory-bank/activeContext.md"),
        "# Current focus\n\nIndependent unpublished sibling learning.\n",
    )?;
    support::identity_fixtures::bind(&f.world, &sibling)?;
    let (_, path) = f.imported(
        "landed",
        "# Landed\n\nA shared observation for separate sessions.\n",
    )?;
    let before = state(&f.world, &sibling)?;
    let original_repo = f.repo.clone();
    let original = state(&f.world, &original_repo)?;
    f.repo = sibling.clone();
    let done = f.apply(&f.prepared()?)?;
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert!(f.bank().join(path).exists());
    feature_unchanged(&before, &state(&f.world, &sibling)?);
    assert_eq!(original, state(&f.world, &original_repo)?);
    assert!(
        fs::read_to_string(f.bank().join("activeContext.md"))?
            .contains("Independent unpublished sibling")
    );
    Ok(())
}
// obligation: tests/contracts/test_refresh_integration.py::RefreshIntegrationContract.test_cursor_loss_recovers_from_git_and_later_normal_code_merge_does_not_duplicate_memory
#[test]
#[ignore = "needs recovery_frontier registered"]
fn cursor_loss_recovers_from_git_without_duplicate_memory_after_normal_merge() -> TestResult {
    let f = Fixture::new()?;
    fs::write(
        f.peer.join("code.rs"),
        "const VALUE: u32 = 900; // authority code awaits normal integration\n",
    )?;
    git(&f.world, &f.peer, &["add", "code.rs"])?;
    f.imported(
        "landed",
        "# Landed\n\nOne stable observation survives normal merges.\n",
    )?;
    let first = f.apply(&f.prepared()?)?;
    assert_eq!(get(get(&first, "cursor"), "stored"), &Value::Bool(true));
    fs::remove_file(f.cursor_path()?)?;
    assert_eq!(
        get(&f.inspect(None, false)?, "baseline_source"),
        &s("git-common-base")
    );
    let again = f.apply(&f.prepared()?)?;
    assert_eq!(get(&again, "remote_freshness_verified"), &Value::Bool(true));
    assert_eq!(fs::read_dir(f.bank().join("topics"))?.count(), 1);
    git(&f.world, &f.repo, &["add", "code.rs", "memory-bank"])?;
    git(
        &f.world,
        &f.repo,
        &[
            "commit",
            "-m",
            "Normal session checkpoint before code integration",
        ],
    )?;
    git(&f.world, &f.repo, &["fetch", "origin"])?;
    let merged = f.world.git_with(
        [
            "merge",
            "--no-ff",
            "origin/main",
            "-m",
            "Normal code merge after memory import",
        ],
        RunOptions {
            cwd: Some(&f.repo),
            check: false,
            ..Default::default()
        },
    )?;
    if merged.status != agent_memory::process::Status::Code(0) {
        assert_eq!(merged.status, agent_memory::process::Status::Code(1));
        assert_eq!(
            git(
                &f.world,
                &f.repo,
                &["diff", "--name-only", "--diff-filter=U"]
            )?
            .trim(),
            "code.rs"
        );
        fs::write(
            f.repo.join("code.rs"),
            "const VALUE: u32 = 900; // explicit normal code resolution\n",
        )?;
        git(&f.world, &f.repo, &["add", "code.rs"])?;
        git(
            &f.world,
            &f.repo,
            &[
                "commit",
                "-m",
                "Resolve only the normal feature-code conflict",
            ],
        )?;
    }
    let before = state(&f.world, &f.repo)?;
    let done = f.apply(&f.prepared()?)?;
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert_eq!(
        get(&done, "feature_code_freshness_verified"),
        &Value::Bool(false)
    );
    assert_eq!(fs::read_dir(f.bank().join("topics"))?.count(), 1);
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    assert!(fs::read_to_string(f.repo.join("code.rs"))?.contains("900"));
    Ok(())
}
fn crash(f: &Fixture, prepared: &Value, stage: &str) -> TestResult {
    let path = save(&f.world, prepared, "crash-proposal.json")?;
    let extra = [
        (
            OsString::from("AGENT_MEMORY_FAULT_STAGE"),
            OsString::from(stage),
        ),
        (
            OsString::from("AGENT_MEMORY_FAULT_MODE"),
            OsString::from("exit"),
        ),
    ];
    let output = f.world.am_with(
        [
            os("--json"),
            os("refresh"),
            os("apply"),
            os("--input"),
            path.as_os_str(),
        ],
        RunOptions {
            check: false,
            env: &extra,
            timeout: Duration::from_secs(120),
            ..Default::default()
        },
    )?;
    assert_eq!(output.status, agent_memory::process::Status::Code(86));
    Ok(())
}
fn unlock(f: &Fixture) -> TestResult {
    let preview = good(
        &f.world,
        &[
            os("writes"),
            os("recover"),
            os("--repo"),
            f.repo.as_os_str(),
        ],
    )?;
    let input = save(&f.world, &preview, "unlock.json")?;
    good(
        &f.world,
        &[
            os("writes"),
            os("recover"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--proposal"),
            input.as_os_str(),
        ],
    )?;
    Ok(())
}
fn recover(f: &Fixture, mode: &str) -> TestResult<Value> {
    let outcome = f.world.am_json(
        "am transaction",
        [
            os("recover"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--mode"),
            os(mode),
        ],
    )?;
    let preview = json::decode_bytes(&outcome.stdout, Default::default())?;
    let path = save(&f.world, &preview, "recover.json")?;
    let outcome = f.world.am_json(
        "am transaction",
        [
            os("recover"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--proposal"),
            path.as_os_str(),
        ],
    )?;
    Ok(json::decode_bytes(&outcome.stdout, Default::default())?)
}
// obligation: tests/contracts/test_refresh_recovery.py::RefreshRecoveryContract.test_lost_completed_response_recovers_without_duplicates_and_remains_honest_while_offline
#[test]
#[ignore = "needs recovery_frontier registered"]
fn lost_completed_response_recovers_honestly_even_while_offline() -> TestResult {
    let f = Fixture::new()?;
    let (_, path) = f.imported(
        "import",
        "# Imported\n\nPreserve the actual completed observation.\n",
    )?;
    let prepared = f.prepared()?;
    let before = state(&f.world, &f.repo)?;
    crash(&f, &prepared, "transaction-after-receipt")?;
    assert!(f.bank().join(tx::MARKER).exists());
    assert!(f.bank().join(&path).exists());
    unlock(&f)?;
    let finished = recover(&f, "finish")?;
    assert_eq!(get(&finished, "outcome"), &s("finished"));
    assert!(!f.bank().join(tx::MARKER).exists());
    let away = f.remote.with_file_name("offline.git");
    fs::rename(&f.remote, &away)?;
    let pending = f.apply(&prepared)?;
    assert_eq!(get(&pending, "sharing_state"), &s("pending-stale"));
    assert_eq!(get(&pending, "refresh_applied"), &Value::Bool(true));
    for key in [
        "remote_freshness_verified",
        "relevant_imports_read",
        "canonical_writes_performed",
    ] {
        assert_eq!(get(&pending, key), &Value::Bool(false));
    }
    assert_eq!(get(&pending, "local_memory_available"), &Value::Bool(true));
    fs::rename(&away, &f.remote)?;
    let online = f.apply(&prepared)?;
    assert_eq!(
        get(&online, "remote_freshness_verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(get(&online, "receipt"), "already_applied"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(&online, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    assert_eq!(fs::read_dir(f.bank().join("topics"))?.count(), 1);
    Ok(())
}
// obligation: tests/contracts/test_refresh_recovery.py::RefreshRecoveryContract.test_unfinished_source_bound_import_requires_rollback_then_fresh_review_not_old_finish
#[test]
#[ignore = "needs recovery_frontier registered"]
fn unfinished_import_requires_rollback_and_fresh_review_not_old_finish() -> TestResult {
    let f = Fixture::new()?;
    let (_, path) = f.imported(
        "import",
        "# Imported\n\nRevalidate after interrupted writes.\n",
    )?;
    let prepared = f.prepared()?;
    let before = state(&f.world, &f.repo)?;
    crash(&f, &prepared, "transaction-after-marker")?;
    unlock(&f)?;
    let result = f.world.am_with(
        [
            os("--json"),
            os("transaction"),
            os("recover"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--mode"),
            os("finish"),
        ],
        RunOptions {
            check: false,
            ..Default::default()
        },
    )?;
    assert_eq!(result.status, agent_memory::process::Status::Code(1));
    assert!(f.bank().join(tx::MARKER).exists());
    let rolled = recover(&f, "rollback")?;
    assert_eq!(get(&rolled, "outcome"), &s("rolled-back"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    let old = f.apply(&prepared)?;
    assert_eq!(get(&old, "refresh_applied"), &Value::Bool(false));
    assert_eq!(get(&old, "canonical_writes_performed"), &Value::Bool(false));
    let fresh = f.prepared()?;
    assert_ne!(
        get(get(&fresh, "proposal"), "operation_id"),
        get(get(&prepared, "proposal"), "operation_id")
    );
    let done = f.apply(&fresh)?;
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    assert!(f.bank().join(path).exists());
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    Ok(())
}
fn wait_ready(_child: &support::Child, path: &Path) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        path.exists(),
        "quick refresh never reached final source recheck"
    );
    Ok(())
}
#[test]
fn current_final_recheck_api_worker() -> TestResult {
    let Some(repo) = std::env::var_os("AM_REFRESH_CURRENT_API_REPO") else {
        return Ok(());
    };
    agent_memory::cli::register_providers().unwrap();
    let runtime = PathBuf::from(std::env::var_os("AM_REFRESH_TEST_RUNTIME").unwrap());
    let error = refresh::current_with_runtime(&PathBuf::from(repo), &runtime).unwrap_err();
    let failure = refresh::CurrentFailure::from_error(&error).unwrap();
    assert_eq!(failure.code, "current-source-changed");
    assert_eq!(failure.component, "current.final-recheck");
    assert!(failure.message.contains("during the observation"));
    assert_eq!(failure.repair, refresh::CURRENT_REPAIR);
    assert!(!failure.message.contains("Concurrent source change"));
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_refuses_local_and_authority_changes_at_final_recheck
// obligation: tests/contracts/test_refresh_path_policy.py::RefreshPathPolicyIntegration.test_final_source_mutation_has_specific_api_and_cli_diagnostic
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_final_recheck_refuses_local_authority_index_and_mode_races() -> TestResult {
    for (scenario, api) in [
        ("local", false),
        ("authority", false),
        ("index", false),
        ("mode", false),
        ("local", true),
    ] {
        let mut f = Fixture::new()?;
        let before = state(&f.world, &f.repo)?;
        let directory = f.world.inside("source-race")?;
        fs::create_dir(&directory)?;
        let stage = "refresh-current-before-final-recheck";
        f.world
            .env
            .insert("AGENT_MEMORY_FAULT_STAGE".into(), stage.into());
        f.world
            .env
            .insert("AGENT_MEMORY_FAULT_MODE".into(), "pause".into());
        f.world.env.insert(
            "AGENT_MEMORY_FAULT_DIR".into(),
            directory.clone().into_os_string(),
        );
        let worker = std::env::current_exe()?;
        let child = if api {
            f.world.env.insert(
                "AM_REFRESH_CURRENT_API_REPO".into(),
                f.repo.clone().into_os_string(),
            );
            f.world.spawn([
                worker.as_os_str(),
                os("--exact"),
                os("current_final_recheck_api_worker"),
                os("--nocapture"),
            ])?
        } else {
            f.world.spawn([
                runtime_command(&f.world),
                os("--json"),
                os("refresh"),
                os("current"),
                os("--repo"),
                f.repo.as_os_str(),
            ])?
        };
        wait_ready(&child, &directory.join(format!("{stage}.ready")))?;
        match scenario {
            "local" => {
                fs::write(
                    f.bank().join("activeContext.md"),
                    "# Current focus\n\nConcurrent source change must remain intact.\n",
                )?;
            }
            "authority" => {
                fs::write(
                    f.peer.join("memory-bank/projectbrief.md"),
                    "# Project\n\nNew authority during observation.\n",
                )?;
                f.push()?;
            }
            "index" => {
                fs::write(
                    f.repo.join("code.rs"),
                    "const VALUE: u32 = 42; // caller stages during observation\n",
                )?;
                git(&f.world, &f.repo, &["add", "code.rs"])?;
            }
            "mode" => mode(&f.bank().join("activeContext.md"), 0o600)?,
            _ => unreachable!(),
        }
        let changed = state(&f.world, &f.repo)?;
        fs::write(directory.join(format!("{stage}.release")), b"")?;
        let output = child.wait()?;
        if api {
            assert_eq!(output.status, agent_memory::process::Status::Code(0));
        } else {
            assert_eq!(output.status, agent_memory::process::Status::Code(1));
            let value = json::decode_bytes(&output.stdout, Default::default())?;
            assert_eq!(get(&value, "failure_code"), &s("current-source-changed"));
            assert_eq!(
                get(&value, "canonical_writes_performed"),
                &Value::Bool(false)
            );
            if scenario == "local" {
                assert_eq!(
                    get(&value, "failure_component"),
                    &s("current.final-recheck")
                );
            }
        }
        assert_eq!(changed, state(&f.world, &f.repo)?);
        if scenario == "local" {
            feature_unchanged(&before, &changed);
        }
    }
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::RefreshPathPolicyIntegration.test_unprotected_state_failure_remains_specific_without_changing_work
#[test]
#[ignore = "needs recovery_frontier registered"]
fn unprotected_state_failure_keeps_specific_refresh_and_doctor_diagnostics() -> TestResult {
    let f = Fixture::new()?;
    let state_root = PathBuf::from(
        f.world
            .env
            .get(OsStr::new("AGENT_MEMORY_STATE_HOME"))
            .unwrap(),
    );
    let before = state(&f.world, &f.repo)?;
    mode(&f.world.root, 0o755)?;
    mode(&f.world.paths["state"], 0o755)?;
    mode(&state_root, 0o775)?;
    let (code, value) = f.refresh("current", None)?;
    assert_eq!(code, 1);
    assert_eq!(
        get(&value, "failure_code"),
        &s("transaction-path-untrusted")
    );
    assert!(text(get(&value, "failure_component")).contains("state"));
    assert_eq!(
        get(&value, "remote_freshness_verified"),
        &Value::Bool(false)
    );
    assert_eq!(before, state(&f.world, &f.repo)?);
    let (_, doctor) = call(&f.world, &[os("doctor"), os("--repo"), f.repo.as_os_str()])?;
    let check = rows(get(&doctor, "checks"))
        .iter()
        .find(|check| get(check, "component") == &s("bank.refresh_prerequisites"))
        .unwrap();
    assert_eq!(get(check, "state"), &s("failure"));
    assert!(text(get(check, "detail")).contains("transaction-path-untrusted"));
    assert_eq!(
        get(get(&doctor, "refresh"), "remote_check"),
        &s("not-exercised")
    );
    assert_eq!(before, state(&f.world, &f.repo)?);
    mode(&f.world.root, 0o700)?;
    mode(&f.world.paths["state"], 0o700)?;
    Ok(())
}
// obligation: tests/contracts/test_refresh_path_policy.py::RefreshPathPolicyIntegration.test_actual_protected_775_chain_current_and_doctor_preserve_feature_work
#[test]
#[ignore = "needs recovery_frontier registered"]
fn protected_775_chain_current_and_doctor_preserve_feature_work() -> TestResult {
    let f = Fixture::new()?;
    let state_root = PathBuf::from(
        f.world
            .env
            .get(OsStr::new("AGENT_MEMORY_STATE_HOME"))
            .unwrap(),
    );
    let checkout = f.cursor_path()?.parent().unwrap().to_path_buf();
    for path in [&state_root, &checkout] {
        mode(path, 0o775)?;
    }
    let before = state(&f.world, &f.repo)?;
    let (code, value) = f.refresh("current", None)?;
    assert_eq!(code, 0);
    assert_eq!(get(&value, "sharing_state"), &s("bank-synchronized"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    for path in [&state_root, &checkout] {
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o775);
    }
    let (_, doctor) = call(&f.world, &[os("doctor"), os("--repo"), f.repo.as_os_str()])?;
    let check = rows(get(&doctor, "checks"))
        .iter()
        .find(|check| get(check, "component") == &s("bank.refresh_prerequisites"))
        .unwrap();
    assert_eq!(get(check, "state"), &s("pass"));
    assert_eq!(
        get(get(&doctor, "refresh"), "remote_check"),
        &s("not-exercised")
    );
    assert_eq!(
        get(get(&doctor, "refresh"), "remote_freshness_verified"),
        &Value::Bool(false)
    );
    let evidence = checkout.join("transactions/unknown/journal.json");
    fs::create_dir_all(evidence.parent().unwrap())?;
    fs::write(&evidence, b"{}")?;
    let (code, value) = f.refresh("current", None)?;
    assert_eq!(code, 1);
    assert_eq!(
        get(&value, "failure_code"),
        &s("transaction-recovery-pending")
    );
    assert!(text(get(&value, "repair")).contains("journal"));
    let (_, doctor) = call(&f.world, &[os("doctor"), os("--repo"), f.repo.as_os_str()])?;
    let check = rows(get(&doctor, "checks"))
        .iter()
        .find(|check| get(check, "component") == &s("bank.refresh_prerequisites"))
        .unwrap();
    assert_eq!(get(check, "state"), &s("failure"));
    assert!(text(get(check, "detail")).contains("transaction-recovery-pending"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    assert!(evidence.exists());
    Ok(())
}

// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_private_state_directory_creation_preserves_existing_modes_and_links
#[test]
fn private_state_directory_creation_preserves_modes_and_links() -> TestResult {
    let w = World::new()?;
    let existing = w.inside("existing-state")?;
    fs::create_dir(&existing)?;
    mode(&existing, 0o775)?;
    let target = existing.join("one/two");
    agent_memory::local_registry::private_mkdir(&target)?;
    assert_eq!(fs::metadata(&existing)?.permissions().mode() & 0o777, 0o775);
    assert_eq!(
        fs::metadata(existing.join("one"))?.permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o777, 0o700);
    let link = existing.join("alias");
    symlink(&target, &link)?;
    assert!(agent_memory::local_registry::private_mkdir(&link.join("child")).is_err());
    assert!(fs::symlink_metadata(&link)?.is_symlink());
    assert!(!target.join("child").exists());
    fs::create_dir(target.join("nested"))?;
    assert!(agent_memory::local_registry::private_mkdir(&link.join("nested/child")).is_err());
    assert!(!target.join("nested/child").exists());
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_private_state_directory_rejects_ancestor_rebinding_during_create
#[test]
fn private_directory_creation_pins_detect_ancestor_rebinding() -> TestResult {
    let w = World::new()?;
    let parent = w.inside("rebind-parent")?;
    fs::create_dir(&parent)?;
    mode(&parent, 0o700)?;
    let target = parent.join("leaf");
    let held = native_fs::DirectoryChain::create(&target, false)?;
    let moved = w.inside("rebind-moved")?;
    fs::rename(&parent, &moved)?;
    fs::create_dir(&parent)?;
    mode(&parent, 0o700)?;
    assert!(held.recheck().is_err());
    assert!(!target.exists());
    assert!(moved.join("leaf").is_dir());
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_private_output_preflight_preserves_identical_retry
#[test]
fn private_output_preflight_preserves_identical_retry() -> TestResult {
    let w = World::new()?;
    let repo = w.repository("private-output-repo", false)?;
    let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
    let target = w.inside("same-output.json")?;
    let value = obj([("operation", s("fixture")), ("value", s("unchanged"))]);
    agent_memory::sharing_limits::write_private(&target, &value, &repo, &pack)?;
    let before = fs::read(&target)?;
    agent_memory::sharing_limits::preflight_private_output(&target, &repo, &pack)?;
    agent_memory::sharing_limits::write_private(&target, &value, &repo, &pack)?;
    assert_eq!(fs::read(&target)?, before);
    assert_eq!(fs::metadata(target)?.permissions().mode() & 0o777, 0o600);
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_public_sharing_commands_reject_unsafe_output_before_work
#[test]
fn public_sharing_commands_reject_unsafe_outputs_before_work() -> TestResult {
    let mut world = World::new()?;
    install_runtime(&mut world);
    let w = &world;
    let repo = w.repository("unsafe-output-repo", false)?;
    let bank_path = repo.join("memory-bank");
    fs::create_dir(&bank_path)?;
    for (path, body) in bank_files()?.as_object().unwrap() {
        fs::write(bank_path.join(path), text(body))?;
    }
    fs::write(repo.join("code.rs"), "const VALUE: u32 = 1;\n")?;
    git(w, &repo, &["add", "."])?;
    git(
        w,
        &repo,
        &["commit", "-m", "Canonical memory before output preflight"],
    )?;
    fs::write(
        repo.join("code.rs"),
        "const VALUE: u32 = 2; // staged feature\n",
    )?;
    git(w, &repo, &["add", "code.rs"])?;
    fs::write(repo.join("unrelated.txt"), "Unstaged unrelated work\n")?;
    let before = state(w, &repo)?;
    let directory = w.inside("unsafe-private-directory")?;
    fs::create_dir(&directory)?;
    mode(&directory, 0o755)?;
    let occupied = w.inside("existing-private.json")?;
    fs::write(&occupied, b"preserve original")?;
    let redirected = w.inside("redirected-private.json")?;
    symlink(&occupied, &redirected)?;
    for target in [directory.join("secret-path.json"), redirected] {
        for command in [
            vec![
                os("semantic"),
                os("inspect-v2"),
                os("--repo"),
                repo.as_os_str(),
            ],
            vec![
                os("publish"),
                os("prepare"),
                os("--repo"),
                repo.as_os_str(),
                os("--input"),
                os("unused.json"),
            ],
            vec![
                os("refresh"),
                os("inspect"),
                os("--v2"),
                os("--repo"),
                repo.as_os_str(),
            ],
        ] {
            let mut args = command;
            args.extend([os("--output"), target.as_os_str()]);
            let (code, value) = call(&w, &args)?;
            assert_eq!(code, 1);
            assert_eq!(get(&value, "failure_code"), &s("private-output-invalid"));
            assert!(text(get(&value, "repair")).contains("owner-only"));
            assert!(!json::encode(&value, json::DEFAULT)?.contains(target.to_str().unwrap()));
            assert_eq!(
                get(&value, "canonical_writes_performed"),
                &Value::Bool(false)
            );
            assert!(
                !value
                    .get("git_metadata_writes_performed")
                    .map(|field| field.as_bool().unwrap())
                    .unwrap_or(false)
            );
            assert_eq!(before, state(w, &repo)?);
        }
    }
    assert_eq!(fs::read(&occupied)?, b"preserve original");
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_succeeds_with_group_writable_umask_for_new_runtime_state
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_succeeds_with_group_writable_umask_for_new_state() -> TestResult {
    let f = Fixture::new()?;
    let before = state(&f.world, &f.repo)?;
    let argv = [
        runtime_command(&f.world).to_owned(),
        "--json".into(),
        "refresh".into(),
        "current".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
    ];
    let env = f
        .world
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    let spec = agent_memory::process::Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&f.repo),
        search_path: false,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        stderr: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        deadline: agent_memory::clock::Deadline::from_absolute_ns(u64::MAX),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o002),
        pass_fds: &[],
    };
    let output = agent_memory::process::run_for(&spec, Duration::from_secs(120))?;
    assert_eq!(output.status, agent_memory::process::Status::Code(0));
    let value = json::decode_bytes(&output.stdout, Default::default())?;
    assert_eq!(get(&value, "sharing_state"), &s("bank-synchronized"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_cursor_advances_memory_without_feature_ancestry_and_old_retry_cannot_reverse_newer_import
#[test]
#[ignore = "needs recovery_frontier registered"]
fn cursor_advances_without_feature_ancestry_and_old_retry_cannot_reverse_import() -> TestResult {
    let f = Fixture::new()?;
    let (_, path) = f.imported(
        "first",
        "# First shared lesson\n\nPreserve its exact source.\n",
    )?;
    let prepared = f.prepared()?;
    let first = f.apply(&prepared)?;
    let before_code = state(&f.world, &f.repo)?;
    let (_, other) = f.imported(
        "second",
        "# Second shared lesson\n\nA later independent observation.\n",
    )?;
    let view = f.inspect(None, false)?;
    assert_eq!(get(&view, "baseline_source"), &s("cursor"));
    assert_eq!(
        get(get(get(&view, "inspection"), "binding"), "base"),
        get(&first, "observed_authority_head")
    );
    let second = f.apply(&f.prepared()?)?;
    assert_eq!(
        get(&second, "remote_freshness_verified"),
        &Value::Bool(true)
    );
    let before = state(&f.world, &f.repo)?;
    let old = f.apply(&prepared)?;
    assert_eq!(
        get(&old, "sharing_state"),
        &s("historical-or-changed-local")
    );
    assert_eq!(get(&old, "canonical_writes_performed"), &Value::Bool(false));
    assert_eq!(get(&old, "relevant_imports_read"), &Value::Bool(false));
    assert_eq!(before, state(&f.world, &f.repo)?);
    feature_unchanged(&before_code, &before);
    assert!(f.bank().join(path).exists());
    assert!(f.bank().join(other).exists());
    Ok(())
}
// obligation: tests/contracts/test_refresh_receipt_snapshot.py::ReceiptSnapshotCurrent.test_imported_remote_deletion_is_part_of_complete_snapshot
#[test]
#[ignore = "needs recovery_frontier registered"]
fn imported_remote_deletion_is_part_of_complete_snapshot() -> TestResult {
    let f = Fixture::new()?;
    let (_, path) = f.imported(
        "receipt-covered",
        "# Receipt-covered lesson\n\nA durable same-project fact.\n",
    )?;
    let first = f.apply(&f.prepared()?)?;
    git(&f.world, &f.repo, &["add", "memory-bank"])?;
    git(
        &f.world,
        &f.repo,
        &[
            "commit",
            "--only",
            "-m",
            "Fixture tracked imported memory",
            "--",
            "memory-bank",
        ],
    )?;
    fs::remove_file(f.peer.join("memory-bank").join(&path))?;
    let router = f.peer.join("memory-bank/MEMORY.md");
    let (meta, mut routes, omitted) = bank::parse_router(&fs::read_to_string(&router)?)?;
    routes.retain(|r| get(r, "path") != &s(&path));
    fs::write(router, bank::router_text(&meta, &routes, &omitted)?)?;
    f.push()?;
    let prep = f.prepare_with(
        f.recall(),
        Some(text(get(&first, "observed_authority_head"))),
        None,
        obj([]),
    )?;
    f.apply(&prep)?;
    assert!(!f.bank().join(&path).exists());
    let before = state(&f.world, &f.repo)?;
    let (code, current) = f.refresh("current", None)?;
    assert_eq!(code, 0);
    assert_eq!(get(&current, "sharing_state"), &s("bank-synchronized"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_refresh_receipt_snapshot.py::ReceiptSnapshotCurrent.test_local_mutations_staged_memory_and_new_authority_remain_review_required
#[test]
#[ignore = "needs recovery_frontier registered"]
fn local_mutations_staged_memory_and_new_authority_remain_review_required() -> TestResult {
    let f = Fixture::new()?;
    let (_, path) = f.imported(
        "receipt-covered",
        "# Receipt-covered lesson\n\nA durable same-project fact.\n",
    )?;
    f.apply(&f.prepared()?)?;
    let baseline = state(&f.world, &f.repo)?;
    let target = f.bank().join(&path);
    let original = fs::read(&target)?;
    for mutation in ["edit", "delete", "add", "mode", "stage"] {
        match mutation {
            "edit" => {
                let mut raw = original.clone();
                raw.extend_from_slice(b"\nUnreviewed local change.\n");
                fs::write(&target, raw)?;
            }
            "delete" => fs::remove_file(&target)?,
            "add" => {
                f.add(
                    &f.repo,
                    "private-local",
                    "# Unpublished local lesson\n\nPreserve local evidence.\n",
                    obj([]),
                )?;
            }
            "mode" => mode(&target, 0o755)?,
            "stage" => {
                git(&f.world, &f.repo, &["add", "memory-bank"])?;
            }
            _ => unreachable!(),
        }
        let before = state(&f.world, &f.repo)?;
        let (_, value) = f.refresh("current", None)?;
        assert_ne!(
            get(&value, "sharing_state"),
            &s("bank-synchronized"),
            "{mutation}"
        );
        assert_eq!(
            get(&value, "canonical_writes_performed"),
            &Value::Bool(false)
        );
        assert_eq!(before, state(&f.world, &f.repo)?);
        if mutation == "add" {
            for extra in before
                .bank
                .keys()
                .filter(|p| !baseline.bank.contains_key(*p))
            {
                fs::remove_file(f.bank().join(extra))?;
            }
            for (p, raw) in &baseline.bank {
                fs::write(f.bank().join(p), raw)?;
            }
        } else if mutation == "mode" {
            mode(&target, 0o644)?;
        } else if mutation == "stage" {
            let index = PathBuf::from(
                git(
                    &f.world,
                    &f.repo,
                    &["rev-parse", "--path-format=absolute", "--git-path", "index"],
                )?
                .trim(),
            );
            fs::write(index, &baseline.index)?;
        } else {
            fs::write(&target, &original)?;
        }
    }
    f.imported(
        "later-authority",
        "# Later remote lesson\n\nRequires a new semantic review.\n",
    )?;
    let (_, value) = f.refresh("current", None)?;
    assert_eq!(get(&value, "sharing_state"), &s("review-required"));
    assert_eq!(baseline, state(&f.world, &f.repo)?);
    Ok(())
}
// obligation: tests/contracts/test_refresh_receipt_snapshot.py::ReceiptSnapshotCurrent.test_cursor_receipt_and_branch_pins_cannot_be_mixed_or_missing
#[test]
#[ignore = "needs recovery_frontier registered"]
fn cursor_receipt_and_branch_pins_cannot_be_mixed_or_missing() -> TestResult {
    let f = Fixture::new()?;
    f.imported(
        "receipt-covered",
        "# Receipt-covered lesson\n\nA durable same-project fact.\n",
    )?;
    let done = f.apply(&f.prepared()?)?;
    let path = f.cursor_path()?;
    let raw = fs::read(&path)?;
    let value = json::decode_bytes(&raw, Default::default())?;
    let receipt_path = PathBuf::from(text(get(get(&done, "receipt"), "receipt_path")));
    let receipt_raw = fs::read(&receipt_path)?;
    for mutation in ["missing", "corrupt", "receipt", "mixed", "branch"] {
        match mutation {
            "missing" => fs::remove_file(&path)?,
            "corrupt" => fs::write(&path, b"{}")?,
            "receipt" | "mixed" => {
                let mut changed = json::decode_bytes(&receipt_raw, Default::default())?;
                set(
                    &mut changed,
                    "operation_id",
                    s("11111111-1111-4111-8111-111111111111"),
                );
                let bytes = encoded(&changed)?;
                fs::write(&receipt_path, &bytes)?;
                if mutation == "mixed" {
                    let mut cursor = value.clone();
                    set(&mut cursor, "receipt_sha256", s(sha256_hex(&bytes)));
                    if let Value::Object(fields) = &mut cursor {
                        fields.retain(|(k, _)| k != "checksum");
                    }
                    let sha = sha256_hex(&encoded(&cursor)?);
                    set(&mut cursor, "checksum", s(sha));
                    fs::write(&path, encoded(&cursor)?)?;
                }
            }
            "branch" => {
                git(
                    &f.world,
                    &f.repo,
                    &[
                        "symbolic-ref",
                        "HEAD",
                        "refs/heads/same-revision-other-branch",
                    ],
                )?;
                git(
                    &f.world,
                    &f.repo,
                    &[
                        "update-ref",
                        "HEAD",
                        text(get(get(&value, "git_context"), "revision")),
                    ],
                )?;
            }
            _ => unreachable!(),
        }
        let before = state(&f.world, &f.repo)?;
        let (_, observed) = f.refresh("current", None)?;
        assert_ne!(
            get(&observed, "sharing_state"),
            &s("bank-synchronized"),
            "{mutation}"
        );
        assert_eq!(before, state(&f.world, &f.repo)?);
        fs::write(&path, &raw)?;
        fs::write(&receipt_path, &receipt_raw)?;
        if mutation == "branch" {
            git(
                &f.world,
                &f.repo,
                &["symbolic-ref", "HEAD", "refs/heads/feature-session"],
            )?;
        }
    }
    Ok(())
}
// obligation: tests/contracts/test_refresh_v2.py::RefreshVersionTwoContract.test_legacy_semantic_inspect_and_check_keep_their_required_input_contract
#[test]
#[ignore = "needs recovery_frontier registered"]
fn legacy_semantic_inspect_and_check_keep_required_input_contract() -> TestResult {
    let f = Fixture::new()?;
    let view = good(
        &f.world,
        &[
            os("semantic"),
            os("inspect"),
            os("--repo"),
            f.repo.as_os_str(),
        ],
    )?;
    assert_eq!(
        get(&view, "schema"),
        &s("agent-memory/semantic-sharing-inspection-v1")
    );
    let prepared = f.prepared()?;
    let proof = get(get(get(&prepared, "proposal"), "refresh_evidence"), "proof");
    let path = save(&f.world, proof, "legacy-proof.json")?;
    let checked = good(
        &f.world,
        &[
            os("semantic"),
            os("check"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--input"),
            path.as_os_str(),
        ],
    )?;
    assert_eq!(
        get(&checked, "source_observation_current"),
        &Value::Bool(true)
    );
    Ok(())
}

// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_current_refuses_unchanged_hidden_record_and_corrupt_hold_anchor
#[test]
#[ignore = "needs recovery_frontier registered"]
fn current_refuses_unchanged_hidden_record_and_corrupt_hold_anchor() -> TestResult {
    for scenario in ["hidden", "hold-anchor"] {
        let mut f = Fixture::new()?;
        let mut anchor = None;
        if scenario == "hidden" {
            let (mut metadata, path) = f.add(
                &f.peer,
                "hidden",
                "# Hidden fixture\n\nA hidden lesson.\n",
                obj([]),
            )?;
            set(&mut metadata, "visibility", s("hidden"));
            fs::write(
                f.peer.join("memory-bank").join(path),
                frontmatter::write(metadata.as_object().unwrap())?
                    + "# Hidden fixture\n\nA hidden lesson.\n",
            )?;
            f.push()?;
            git(&f.world, &f.repo, &["fetch", "origin", "main"])?;
            let sibling = f.world.inside("unchanged-hidden-worktree")?;
            git(
                &f.world,
                &f.repo,
                &[
                    "worktree",
                    "add",
                    "-b",
                    "hidden-session",
                    sibling.to_str().unwrap(),
                    "origin/main",
                ],
            )?;
            support::identity_fixtures::bind(&f.world, &sibling)?;
            f.repo = sibling;
        } else {
            let path = f
                .cursor_path()?
                .parent()
                .unwrap()
                .join("semantic-holds")
                .join(format!("{PROJECT}.present.json"));
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, b"corrupt")?;
            anchor = Some(path);
        }
        let before = state(&f.world, &f.repo)?;
        let (_, current) = f.refresh("current", None)?;
        assert_ne!(get(&current, "sharing_state"), &s("bank-synchronized"));
        assert_eq!(
            get(&current, "remote_freshness_verified"),
            &Value::Bool(false)
        );
        assert_eq!(before, state(&f.world, &f.repo)?);
        if let Some(anchor) = anchor {
            assert_eq!(fs::read(anchor)?, b"corrupt");
        }
    }
    Ok(())
}
fn find_named(root: &Path, name: &str) -> TestResult<PathBuf> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if let Ok(found) = find_named(&path, name) {
                return Ok(found);
            }
        } else if entry.file_name() == name {
            return Ok(path);
        }
    }
    Err(format!("Fixture could not find {name}").into())
}
// obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_primed_unchanged_refresh_uses_catalog_and_matches_canonical_recall
#[test]
#[ignore = "needs recovery_frontier registered"]
fn primed_unchanged_refresh_uses_catalog_and_matches_canonical_recall() -> TestResult {
    let mut selections = Vec::new();
    for disabled in [false, true] {
        let mut f = Fixture::new()?;
        let started = good(
            &f.world,
            &[
                os("discover"),
                os("start"),
                os("--repo"),
                f.repo.as_os_str(),
                os("--intent"),
                os("recall"),
                os("--task"),
                os("Read project orientation."),
            ],
        )?;
        let mut trace = get(&started, "discovery").clone();
        set(&mut trace, "complete", Value::Bool(true));
        set(
            &mut trace,
            "rationale",
            s("Read the observed project orientation."),
        );
        set(
            &mut trace,
            "sources",
            Value::Array(vec![obj([
                ("path", s("projectbrief.md")),
                (
                    "sha256",
                    s(sha256_hex(&fs::read(f.bank().join("projectbrief.md"))?)),
                ),
            ])]),
        );
        let mut recall = f.recall();
        set(
            &mut recall,
            "paths",
            Value::Array(vec![s("projectbrief.md")]),
        );
        set(&mut recall, "discovery", trace);
        let path = save(&f.world, &recall, "catalog-recall.json")?;
        let primed = good(
            &f.world,
            &[
                os("search"),
                os("--repo"),
                f.repo.as_os_str(),
                os("--request"),
                path.as_os_str(),
            ],
        )?;
        assert_eq!(
            get(get(&primed, "metrics"), "catalog_updated"),
            &Value::Bool(true)
        );
        let pointer = find_named(&f.world.paths["state"], "current.json")?;
        let snapshot = fs::read(&pointer)?;
        let before = state(&f.world, &f.repo)?;
        let preparation = f.prepare_with(recall.clone(), None, None, obj([]))?;
        if disabled {
            f.world
                .env
                .insert("AGENT_MEMORY_NO_RECALL_CATALOG".into(), "1".into());
        }
        let done = f.apply(&preparation)?;
        assert_eq!(get(&done, "refresh_applied"), &Value::Bool(true));
        assert_eq!(get(&done, "relevant_imports_read"), &Value::Bool(true));
        assert_eq!(
            get(get(&done, "recall"), "optional_indexes_used"),
            &Value::Bool(!disabled)
        );
        assert_ne!(
            get(get(&done, "recall"), "derived_cache_writes_performed"),
            &Value::Bool(true)
        );
        selections.push(
            rows(get(get(&done, "recall"), "results"))
                .iter()
                .map(|row| {
                    (
                        get(row, "path").clone(),
                        get(row, "body").clone(),
                        get(row, "source_sha256").clone(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(fs::read(&pointer)?, snapshot);
        assert_eq!(before, state(&f.world, &f.repo)?);
        f.world
            .env
            .remove(OsStr::new("AGENT_MEMORY_NO_RECALL_CATALOG"));
        let later = good(
            &f.world,
            &[
                os("search"),
                os("--repo"),
                f.repo.as_os_str(),
                os("--request"),
                path.as_os_str(),
            ],
        )?;
        assert_eq!(get(&later, "optional_indexes_used"), &Value::Bool(true));
        assert_eq!(
            get(get(&later, "metrics"), "catalog_updated"),
            &Value::Bool(false)
        );
        assert_eq!(fs::read(&pointer)?, snapshot);
    }
    assert_eq!(selections[0], selections[1]);
    Ok(())
}
#[test]
fn local_prerequisite_interface_worker() -> TestResult {
    let Some(repo) = std::env::var_os("AM_REFRESH_PREREQ_REPO") else {
        return Ok(());
    };
    agent_memory::cli::register_providers().unwrap();
    let repo = PathBuf::from(repo);
    let provider = refresh::MemoryRefresh;
    let observed = refresh::local_prerequisites(&repo)?;
    assert_eq!(get(&observed, "remote_check"), &s("not-exercised"));
    assert_eq!(
        get(&observed, "remote_freshness_verified"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(&observed, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(get(&observed, "transaction_state"), &s("absent"));
    let (root, bank_path) = agent_memory::bank_writes::bank_path(&repo)?;
    let operations = refresh::state_operations(&root, &bank_path)?;
    let evidence = operations
        .parent()
        .unwrap()
        .join("transactions/unknown/journal.json");
    fs::create_dir_all(evidence.parent().unwrap())?;
    fs::write(&evidence, b"{}")?;
    let typed = agent_memory::turn_check::MemoryRefresh::local_prerequisites(&provider, &repo)?
        .unwrap_err();
    assert_eq!(typed.code, "transaction-recovery-pending");
    assert_eq!(
        typed.message,
        "Retained transaction recovery evidence requires review."
    );
    assert!(typed.repair.contains("original journal"));
    assert!(typed.path.is_some());
    assert_eq!(fs::read(evidence)?, b"{}");
    Ok(())
}
#[test]
fn diagnostics_provider_receives_local_prerequisites_and_typed_current_failure() -> TestResult {
    let mut w = World::new()?;
    let repo = w.repository("local-prerequisite-repo", false)?;
    let bank_path = repo.join("memory-bank");
    fs::create_dir(&bank_path)?;
    for (path, body) in bank_files()?.as_object().unwrap() {
        fs::write(bank_path.join(path), text(body))?;
    }
    w.env
        .insert("AM_REFRESH_PREREQ_REPO".into(), repo.into_os_string());
    w.run([
        std::env::current_exe()?.as_os_str(),
        os("--exact"),
        os("local_prerequisite_interface_worker"),
        os("--nocapture"),
    ])?;
    Ok(())
}
fn v2_inspect(f: &Fixture, request: Option<&Value>) -> TestResult<(PathBuf, Value)> {
    let output = f
        .world
        .inside(format!("v2-inspection-{}.json", host::uuid4()?))?;
    let input = request
        .map(|v| save(&f.world, v, "v2-request.json"))
        .transpose()?;
    let mut args = vec![
        os("semantic"),
        os("inspect-v2"),
        os("--repo"),
        f.repo.as_os_str(),
        os("--output"),
        output.as_os_str(),
    ];
    if let Some(path) = &input {
        args.extend([os("--input"), path.as_os_str()]);
    }
    good(&f.world, &args)?;
    Ok((
        output.clone(),
        json::decode_bytes(&fs::read(output)?, Default::default())?,
    ))
}
fn v2_prepared(f: &Fixture, recall: Value) -> TestResult<(PathBuf, Value)> {
    let (_, initial) = v2_inspect(f, None)?;
    let sources = rows(get(&initial, "sources"))
        .iter()
        .map(|row| (text(get(row, "key")), row))
        .collect::<BTreeMap<_, _>>();
    let mut request = get(&initial, "request").clone();
    let mut selections = Vec::new();
    for key in rows(get(&initial, "changed")) {
        let row = sources[text(key)];
        let subject = get(row, "target_path")
            .as_str()
            .unwrap_or_else(|| text(get(row, "path")));
        selections.push((
            text(key).into(),
            obj([
                (
                    "searches",
                    Value::Array(vec![
                        s(subject),
                        s(subject.strip_suffix(".md").unwrap_or(subject)),
                    ]),
                ),
                ("paths", Value::Array(vec![s(subject)])),
                ("ids", Value::Array(vec![])),
                ("complete", Value::Bool(true)),
                ("uncertain", Value::Bool(false)),
            ]),
        ));
    }
    set(&mut request, "selections", Value::Object(selections));
    let (manifest, view) = v2_inspect(f, Some(&request))?;
    for page in rows(get(&view, "pages")) {
        let index = json::encode(get(page, "index"), json::DEFAULT)?;
        good(
            &f.world,
            &[
                os("semantic"),
                os("page-v2"),
                os("--repo"),
                f.repo.as_os_str(),
                os("--input"),
                manifest.as_os_str(),
                os("--page"),
                os(&index),
            ],
        )?;
    }
    let pairs=rows(get(&view,"required_pairs")).iter().map(|pair|{let pair=rows(pair);obj([("left",pair.first().unwrap().clone()),("right",pair.get(1).unwrap().clone()),("classification",s("related")),("reason",s("Independent authored partition observations and their navigation preserve distinct sources."))])}).collect();
    let judgment = obj([
        ("schema", s("agent-memory/semantic-sharing-judgment-v2")),
        ("request", request),
        ("binding", get(&view, "binding").clone()),
        ("manifest_sha256", get(&view, "manifest_sha256").clone()),
        (
            "pages",
            Value::Array(
                rows(get(&view, "pages"))
                    .iter()
                    .map(|page| get(page, "sha256").clone())
                    .collect(),
            ),
        ),
        ("pairs", Value::Array(pairs)),
        ("dependencies", obj([])),
        ("consolidations", Value::Array(vec![])),
        (
            "rationale",
            s("Bounded mechanical complete-delta control, not semantic truth verification."),
        ),
        ("recorded_at", s(host::utc_now_isoformat()?)),
    ]);
    let reviewed = f.world.inside("v2-reviewed.json")?;
    let input = save(&f.world, &judgment, "v2-judgment.json")?;
    good(
        &f.world,
        &[
            os("semantic"),
            os("review-v2"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
            os("--output"),
            reviewed.as_os_str(),
        ],
    )?;
    let reviewed = json::decode_bytes(&fs::read(reviewed)?, Default::default())?;
    let input = save(
        &f.world,
        &obj([
            ("proof", get(&reviewed, "proof").clone()),
            ("recall", recall),
        ]),
        "v2-refresh-request.json",
    )?;
    let prepared = f.world.inside("v2-prepared.json")?;
    good(
        &f.world,
        &[
            os("refresh"),
            os("prepare"),
            os("--v2"),
            os("--repo"),
            f.repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
            os("--output"),
            prepared.as_os_str(),
        ],
    )?;
    Ok((
        prepared.clone(),
        json::decode_bytes(&fs::read(prepared)?, Default::default())?,
    ))
}
// obligation: tests/contracts/test_large_refresh_security.py::LargeRefreshSecurityContract.test_complete_v2_delta_above_sixty_four_sources_imports_and_qualifies_peer_learning
#[test]
#[ignore = "needs recovery_frontier registered"]
fn complete_v2_delta_above_sixty_four_sources_is_scanned_imported_and_recalled() -> TestResult {
    let f = Fixture::new()?;
    let shared = f.peer.join("memory-bank");
    let mut paths = Vec::new();
    fs::create_dir_all(shared.join("topics"))?;
    for group in 0..2 {
        let mut routes = Vec::new();
        for n in group * 35..(group + 1) * 35 {
            let path = format!("topics/peer-{n}.md");
            let body = format!(
                "# Peer observation {n}\n\nRetain the independent authored partition {n} observation.\n"
            );
            let content = support::record_fixtures::record_text(&body, PROJECT)?;
            fs::write(shared.join(&path), content)?;
            paths.push(path.clone());
            routes.push(obj([
                ("path", s(path)),
                ("purpose", s(format!("Partition {n} observation"))),
                ("scope", s("repository")),
                ("lifecycle", s("active")),
                ("read-when", s("When this partition applies")),
            ]));
        }
        let index = format!("topics/peer-group-{group}.md");
        let request = obj([
            ("path", s(&index)),
            ("project-id", s(PROJECT)),
            ("updated", s("2026-10-25T00:00:00Z")),
            ("routes", Value::Array(routes)),
        ]);
        let rendered = agent_memory::bank_layout::render_index(&request)?;
        fs::write(shared.join(&index), text(get(&rendered, "content")))?;
        let router_path = shared.join("MEMORY.md");
        let (meta, mut routes, omitted) = bank::parse_router(&fs::read_to_string(&router_path)?)?;
        routes.push(obj([
            ("path", s(&index)),
            ("purpose", s("Independent peer partitions")),
            ("scope", s("repository")),
            ("lifecycle", s("active")),
            ("read-when", s("When inspecting peer observations")),
        ]));
        fs::write(router_path, bank::router_text(&meta, &routes, &omitted)?)?;
    }
    f.push()?;
    let before = state(&f.world, &f.repo)?;
    let last = paths.last().unwrap();
    let mut recall = f.recall();
    set(&mut recall, "paths", Value::Array(vec![s(last)]));
    set(&mut recall, "kinds", Value::Array(vec![s("semantic-fact")]));
    set(&mut recall, "limit", Value::Int(8.into()));
    set(&mut recall, "include_context", Value::Bool(false));
    let (prepared_path, preparation) = v2_prepared(&f, recall)?;
    let p = get(&preparation, "proposal");
    let delta = get(p, "files")
        .as_object()
        .unwrap()
        .iter()
        .filter(|(path, body)| {
            get(get(p, "observed_hashes"), path).as_str()
                != Some(sha256_hex(text(body).as_bytes()).as_str())
        })
        .collect::<Vec<_>>();
    assert!(delta.len() > 64);
    assert!(
        delta
            .iter()
            .map(|(_, body)| text(body).len())
            .sum::<usize>()
            > 65536
    );
    let mut tampered = p.clone();
    let files = tampered.get_mut("files").unwrap();
    let changed = format!(
        "{}\npassword=WITHHELD_LATE_DELTA_FIXTURE\n",
        text(get(files, last))
    );
    set(files, last, s(changed));
    let input = save(&f.world, &tampered, "v2-tampered.json")?;
    let target = f.world.inside("must-not-exist.json")?;
    let (code, value) = call(
        &f.world,
        &[
            os("refresh"),
            os("apply"),
            os("--v2"),
            os("--input"),
            input.as_os_str(),
            os("--output"),
            target.as_os_str(),
        ],
    )?;
    assert_eq!(code, 1);
    assert!(!json::encode(&value, json::DEFAULT)?.contains("WITHHELD_LATE_DELTA_FIXTURE"));
    assert_eq!(before, state(&f.world, &f.repo)?);
    assert!(!target.exists());
    let applied = f.world.inside("v2-applied.json")?;
    good(
        &f.world,
        &[
            os("refresh"),
            os("apply"),
            os("--v2"),
            os("--input"),
            prepared_path.as_os_str(),
            os("--output"),
            applied.as_os_str(),
        ],
    )?;
    let done = json::decode_bytes(&fs::read(applied)?, Default::default())?;
    assert_eq!(get(&done, "refresh_applied"), &Value::Bool(true));
    assert_eq!(get(&done, "remote_freshness_verified"), &Value::Bool(true));
    let results = rows(get(get(&done, "recall"), "results"));
    assert_eq!(results.len(), 1);
    assert_eq!(get(&results[0], "path"), &s(last));
    feature_unchanged(&before, &state(&f.world, &f.repo)?);
    for path in paths {
        assert_eq!(
            fs::read(f.bank().join(&path))?,
            fs::read(shared.join(path))?
        );
    }
    Ok(())
}
// obligation: tests/contracts/test_large_refresh_security.py::LargeRefreshSecurityContract.test_complete_refresh_security_primitive_scans_late_sources_and_keeps_other_action_bounds
#[test]
fn complete_refresh_security_scans_late_sources_and_enforces_bank_capacity() -> TestResult {
    let mut contents = Value::Object(
        (0..70)
            .map(|n| {
                (
                    format!("topics/{n}.md"),
                    s("# Source\n\nA safe authored source.\n"),
                )
            })
            .collect(),
    );
    set(
        &mut contents,
        "topics/69.md",
        s("# Withheld\n\npassword=WITHHELD_LATE_PRIMITIVE_FIXTURE\n"),
    );
    let reasons = agent_memory::security::complete_refresh_reasons(&contents)?;
    assert!(reasons.contains(&"credential".to_owned()));
    assert!(agent_memory::security::content_reasons(&contents).is_err());
    let oversized = Value::Object(
        (0..4097)
            .map(|n| (format!("topics/{n}.md"), s("x".repeat(16384))))
            .collect(),
    );
    assert!(agent_memory::security::complete_refresh_reasons(&oversized).is_err());
    Ok(())
}
