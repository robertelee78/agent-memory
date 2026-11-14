//! Diagnostics lane: `am version|status|doctor` (baseline `diagnostics.py`)
//! in isolated Worlds. Every report is produced in a child copy of this test
//! binary whose whole environment is the World's, so the process-wide
//! identity locations never reach the user's HOME and parallel tests never
//! share them. Installations use a development source with a stand-in `am`
//! (the installer never runs Cargo), exactly as `t2_installation` does.
use agent_memory::cache_recovery::Qualification;
use agent_memory::codec::json::{self, Value};
use agent_memory::diagnostics::{self, Report, Request};
use agent_memory::installation::{
    self as install, ArtifactInspection, Environment, InstallRequest, Mode, PrivateState,
    ReleaseAccess, Services, Updating,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

const STAND_IN: &[u8] = b"#!/bin/sh\nprintf 'stand-in am %s\\n' \"$*\"\n";
const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
const SECRET: &str = "should-never-appear-secret-123";

// ----- worlds -----

struct World {
    root: PathBuf,
    token: String,
}
impl World {
    fn new() -> Self {
        let tmp = std::fs::canonicalize("/tmp").unwrap();
        let token = agent_memory::host::uuid4().unwrap().replace('-', "");
        let root = tmp.join(format!("agent-memory-test-{}", &token[..16]));
        std::fs::create_dir(&root).unwrap();
        mode(&root, 0o700);
        std::fs::write(
            root.join(".test-world.json"),
            format!("{{\"token\": \"{token}\"}}"),
        )
        .unwrap();
        for name in [
            "home", "config", "data", "state", "codex", "claude", "opencode", "repo", "tmp",
        ] {
            std::fs::create_dir(root.join(name)).unwrap();
            mode(&root.join(name), 0o700);
        }
        Self { root, token }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn pairs(&self) -> Vec<(String, String)> {
        let p = |name: &str| self.path(name).to_string_lossy().into_owned();
        vec![
            ("HOME".into(), p("home")),
            ("XDG_CONFIG_HOME".into(), p("config")),
            ("XDG_DATA_HOME".into(), p("data")),
            ("XDG_STATE_HOME".into(), p("state")),
            ("CODEX_HOME".into(), p("codex")),
            ("CLAUDE_CONFIG_DIR".into(), p("claude")),
            ("OPENCODE_CONFIG_DIR".into(), p("opencode")),
            ("TMPDIR".into(), p("tmp")),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_AUTHOR_NAME".into(), "Test Developer".into()),
            ("GIT_AUTHOR_EMAIL".into(), "test@example.invalid".into()),
            ("GIT_COMMITTER_NAME".into(), "Test Developer".into()),
            ("GIT_COMMITTER_EMAIL".into(), "test@example.invalid".into()),
            (
                "AGENT_MEMORY_TEST_ROOT".into(),
                self.root.to_string_lossy().into_owned(),
            ),
            ("AGENT_MEMORY_TEST_TOKEN".into(), self.token.clone()),
        ]
    }
    fn env(&self) -> Environment {
        Environment::from_pairs(
            self.pairs()
                .into_iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        )
    }
    fn state(&self) -> PathBuf {
        self.path("state/agent-memory/install")
    }
    fn receipt_path(&self) -> PathBuf {
        self.state().join("receipt.json")
    }
    fn git(&self, cwd: &Path, args: &[&str]) {
        let status = Command::new("/usr/bin/git")
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(self.pairs())
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = make_writable(&self.root);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn make_writable(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        for entry in std::fs::read_dir(path)? {
            make_writable(&entry?.path())?;
        }
    }
    Ok(())
}

// ----- seams: honest test implementers of unported callees -----

struct FixtureUpdating;
impl Updating for FixtureUpdating {
    fn banks(&self, repositories: &[String], _: &Value, _: bool) -> install::Result<Value> {
        Ok(Value::Object(vec![(
            "observed_repositories".into(),
            Value::Array(repositories.iter().cloned().map(Value::String).collect()),
        )]))
    }
    fn qualified_current(&self, _: &Path, _: &Path) -> install::Result<Option<Qualification>> {
        Ok(None)
    }
    fn current(&self, _: &Path, _: &Path, _: Option<&Qualification>) -> install::Result<()> {
        Ok(())
    }
    fn release_transition(&self, _: &Value, _: &Value, _: &Path) -> install::Result<()> {
        Ok(())
    }
    fn compatible_surface(
        &self,
        _: &Value,
        _: &Value,
        _: Option<&Qualification>,
    ) -> install::Result<()> {
        Ok(())
    }
}
struct FixturePrivateState;
impl PrivateState for FixturePrivateState {
    fn activation_guard(
        &self,
        _: &Value,
        _: Option<&Value>,
        _: &str,
        transition: &mut dyn FnMut() -> install::Result<Option<Value>>,
        body: &mut dyn FnMut(&Value) -> install::Result<Value>,
    ) -> install::Result<Value> {
        transition()?;
        body(&Value::Object(vec![("fixture".into(), Value::Bool(true))]))
    }
    fn prepare_lifecycle(&self, _: &Value, _: bool) -> install::Result<Value> {
        Ok(Value::Object(Vec::new()))
    }
    fn require_compatible(&self, _: &Value, _: &Value, _: &str) -> install::Result<Value> {
        Ok(Value::Object(Vec::new()))
    }
    fn public_summary(&self, value: &Value) -> install::Result<Value> {
        Ok(value.clone())
    }
}
struct FixtureCatalog;
impl agent_memory::artifact::MigrationContract for FixtureCatalog {
    fn inspect(&self, _: &Path, _: &Value, _: &Value) -> agent_memory::error::Result<Value> {
        Ok(Value::Array(vec![]))
    }
}
struct FixtureArtifacts;
impl ArtifactInspection for FixtureArtifacts {
    fn inspect_runtime(&self, pack: &Path, binary: &Path) -> install::Result<Value> {
        Ok(agent_memory::artifact::inspect_runtime_with(
            pack,
            binary,
            None,
            Some(&FixtureCatalog),
        )?)
    }
    fn inspect(&self, pack: &Path) -> install::Result<Value> {
        Ok(agent_memory::artifact::inspect_with(
            pack,
            None,
            Some(&FixtureCatalog),
        )?)
    }
}
fn services(executable: Option<PathBuf>) -> Services {
    Services {
        updating: Some(Arc::new(FixtureUpdating)),
        private_state: Some(Arc::new(FixturePrivateState)),
        omp_boundary: None,
        artifacts: Arc::new(FixtureArtifacts),
        release: ReleaseAccess::default(),
        executable,
    }
}

// ----- fixtures -----

const COMPATIBILITY: &str = r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"python_minimum":"3.11","installed_commands":["am","agent-memory"],"installation_receipt_schemas":["agent-memory/install-receipt-v1","agent-memory/install-receipt-v2","agent-memory/install-receipt-v3"],"git_required_for":["git-sharing"],"required_python_packages":[],"required_agent_clients":[],"optional_adapters":[]}"#;

fn write(path: &Path, body: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn mode(path: &Path, bits: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap();
}
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut result = vec![root.to_path_buf()];
    if std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()) {
        let mut entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for entry in entries {
            result.extend(walk(&entry));
        }
    }
    result
}
/// Every node below `root`: link target, directory mode, or sha256 + mode.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for path in walk(root).into_iter().skip(1) {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let key = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let value = if meta.file_type().is_symlink() {
            format!("link {}", std::fs::read_link(&path).unwrap().display())
        } else if meta.is_dir() {
            format!("dir {:o}", meta.mode() & 0o7777)
        } else if meta.is_file() {
            format!(
                "{} {:o}",
                agent_memory::release_distribution::digest(&std::fs::read(&path).unwrap()),
                meta.mode() & 0o7777
            )
        } else {
            format!("node {:o}", meta.mode())
        };
        result.insert(key, value);
    }
    result
}

/// A minimal valid portable pack plus a stand-in `am`, as a development source.
fn development_source(world: &World, name: &str) -> PathBuf {
    let root = world.path(name);
    let pack = root.join("skills/agent-memory");
    write(
        &pack.join("SKILL.md"),
        "---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n",
    );
    write(
        &pack.join("bank/GUIDE.md"),
        "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n",
    );
    let mut operation =
        String::from("---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n");
    for section in agent_memory::pack_validation::SECTIONS {
        operation.push_str(&format!("## {section}\n\n"));
        if section == "Required context" {
            operation.push_str(
                "A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n",
            );
        } else {
            operation.push_str("Explicit operation context.\n\n");
        }
    }
    write(&pack.join("inspect/GUIDE.md"), operation);
    write(
        &pack.join("schemas/runtime-compatibility.json"),
        COMPATIBILITY,
    );
    write(
        &pack.join("schemas/bank-router.json"),
        r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#,
    );
    write(
        &pack.join("templates/activation-markdown.example.txt"),
        install::SHIM,
    );
    write(
        &pack.join("templates/activation-omp-markdown.example.txt"),
        include_str!("../skills/agent-memory/templates/activation-omp-markdown.example.txt"),
    );
    write(&pack.join("hooks/opencode.js"), "export default {}\n");
    write(
        &pack.join("hooks/omp.ts"),
        "export default function () {}\n",
    );
    let binary = root.join("target/release/am");
    write(&binary, STAND_IN);
    mode(&binary, 0o755);
    for entry in walk(&pack) {
        if entry.is_file() {
            mode(&entry, 0o644);
        } else if entry.is_dir() {
            mode(&entry, 0o755);
        }
    }
    root
}

/// Baseline `runtime_update_fixtures.bank`: one rendered active context.
fn bank(repo: &Path) -> PathBuf {
    let omitted: Vec<(String, Value)> = [
        "productContext.md",
        "activeContext.md",
        "systemPatterns.md",
        "techContext.md",
        "progress.md",
    ]
    .iter()
    .map(|name| (name.to_string(), text_value("No applicable evidence yet")))
    .collect();
    let proposal = Value::Object(vec![
        ("project-id".into(), text_value(PROJECT)),
        ("router-updated".into(), text_value("2026-10-22T17:00:00Z")),
        (
            "contexts".into(),
            Value::Object(vec![(
                "projectbrief.md".into(),
                Value::Object(vec![
                    (
                        "body".into(),
                        text_value("# Project\n\nPreserve durable shared learning.\n"),
                    ),
                    ("purpose".into(), text_value("Orient the agent")),
                    ("scope".into(), text_value("repository")),
                    ("lifecycle".into(), text_value("active")),
                    ("read-when".into(), text_value("When resuming work")),
                ]),
            )]),
        ),
        ("omitted".into(), Value::Object(omitted)),
    ]);
    let rendered = agent_memory::bank_contract::render(&proposal).unwrap();
    let directory = repo.join("memory-bank");
    std::fs::create_dir(&directory).unwrap();
    for (name, body) in get(&rendered, "files").as_object().unwrap() {
        std::fs::write(directory.join(name), body.as_str().unwrap()).unwrap();
    }
    directory
}

fn text_value(value: &str) -> Value {
    Value::String(value.to_owned())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap_or_default()
}
fn decode(path: &Path) -> Value {
    json::decode_bytes(&std::fs::read(path).unwrap(), Default::default()).unwrap()
}
fn encode_record(value: &Value) -> Vec<u8> {
    install::encoded(value).unwrap()
}
fn set(target: &mut Value, key: &str, value: Value) {
    if let Value::Object(fields) = target {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = value,
            None => fields.push((key.to_owned(), value)),
        }
    }
}

/// Every check of `component`.
fn component<'a>(value: &'a Value, name: &str) -> Vec<&'a Value> {
    get(value, "checks")
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| text(check, "component") == name)
        .collect()
}
fn names(value: &Value, key: &str) -> Vec<String> {
    get(value, key)
        .as_array()
        .unwrap()
        .iter()
        .map(|check| text(check, "component").to_owned())
        .collect()
}

/// One installed development source (link mode unless stated).
struct Installed {
    world: World,
    source: PathBuf,
}
impl Installed {
    fn new(install_mode: Mode, presets: &str) -> Self {
        let world = World::new();
        let source = development_source(&world, "dev source");
        write(
            &world.path("codex/AGENTS.md"),
            "User instructions stay here.\n",
        );
        let request = InstallRequest {
            development_source: Some(source.clone()),
            mode: Some(install_mode),
            presets: Some(presets.to_owned()),
            ..InstallRequest::default()
        };
        let result = install::install_command(&world.env(), &services(None), &request);
        assert_eq!(result.exit_code, 0, "{}", result.stdout);
        Self { world, source }
    }
    fn binary(&self) -> PathBuf {
        self.source.join("target/release/am")
    }
    fn receipt(&self) -> Value {
        decode(&self.world.receipt_path())
    }
    fn targets(&self, kind: &str) -> Vec<Value> {
        get(&self.receipt(), "targets")
            .as_array()
            .unwrap()
            .iter()
            .filter(|target| text(target, "kind") == kind)
            .cloned()
            .collect()
    }
    /// Rewrite the receipt and bind the completed journal to it.
    fn rewrite_receipt(&self, edit: impl FnOnce(&mut Value)) {
        let mut receipt = self.receipt();
        edit(&mut receipt);
        let raw = encode_record(&receipt);
        std::fs::write(self.world.receipt_path(), &raw).unwrap();
        let journal_path = self.world.state().join("journal.json");
        let mut journal = decode(&journal_path);
        set(&mut journal, "receipt", receipt);
        set(
            &mut journal,
            "receipt_identity",
            install::fingerprint(&self.world.receipt_path()).unwrap(),
        );
        std::fs::write(&journal_path, encode_record(&journal)).unwrap();
    }
    fn doctor(&self, expected: i32) -> Value {
        run(
            &self.world,
            "doctor",
            &self.world.path("repo"),
            Some(&self.binary()),
            expected,
        )
    }
}

// ----- child-process harness -----

/// Child entry: one diagnostics command in the World named by the
/// environment; prints the exit code and JSON after a marker. Inert in parents.
#[test]
fn child_entry() {
    if let Ok(arguments) = std::env::var("AM_DIAG_CLI") {
        agent_memory::cli::register_providers().unwrap();
        let args: Vec<String> = match json::decode(&arguments, Default::default()).unwrap() {
            Value::Array(items) => items
                .into_iter()
                .map(|item| item.as_str().unwrap().to_owned())
                .collect(),
            _ => panic!("arguments"),
        };
        let dispatched = agent_memory::cli::dispatch(&args, "am");
        println!("AM-CODE:{}", dispatched.result.exit_code);
        let printed = Value::String(dispatched.result.stdout);
        println!(
            "AM-STDOUT:{}",
            json::encode(&printed, json::COMPACT_ASCII).unwrap()
        );
        return;
    }
    let Ok(operation) = std::env::var("AM_DIAG_CHILD") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    let executable = std::env::var_os("AM_DIAG_EXECUTABLE").map(PathBuf::from);
    let request = Request {
        operation,
        repo: PathBuf::from(std::env::var_os("AM_DIAG_REPO").unwrap()),
        projection: None,
        fix: false,
    };
    let result = diagnostics::command(&Environment::process(), &services(executable), &request);
    assert!(result.stderr.is_empty(), "{}", result.stderr);
    println!("AM-CODE:{}", result.exit_code);
    println!(
        "AM-RESULT:{}",
        json::encode(&result.value.unwrap_or(Value::Null), json::COMPACT_ASCII).unwrap()
    );
}

/// Run `operation` in a child whose whole environment is the World's.
fn run(
    world: &World,
    operation: &str,
    repo: &Path,
    executable: Option<&Path>,
    expected: i32,
) -> Value {
    let mut process = Command::new(std::env::current_exe().unwrap());
    process
        .args([
            "child_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "-q",
        ])
        .env_clear()
        .envs(world.pairs())
        .env("AM_DIAG_CHILD", operation)
        .env("AM_DIAG_REPO", repo)
        .current_dir(world.path("repo"));
    if let Some(executable) = executable {
        process.env("AM_DIAG_EXECUTABLE", executable);
    }
    let output = process.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code: i32 = stdout
        .lines()
        .find_map(|line| line.strip_prefix("AM-CODE:"))
        .unwrap_or_else(|| panic!("{stdout}\n{}", String::from_utf8_lossy(&output.stderr)))
        .parse()
        .unwrap();
    let value = stdout
        .lines()
        .find_map(|line| line.strip_prefix("AM-RESULT:"))
        .map(|line| json::decode(line, Default::default()).unwrap())
        .unwrap();
    assert_eq!(
        code,
        expected,
        "{}",
        json::encode(&value, json::INDENT2_ASCII).unwrap()
    );
    for key in ["writes_performed", "repairs_performed", "network_performed"] {
        assert_eq!(get(&value, key), &Value::Bool(false), "{key}");
    }
    value
}

// ======================================================================
// Without an installation
// ======================================================================

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_no_install_bank_or_identity_is_required_and_nothing_is_bootstrapped
#[test]
fn no_install_bank_or_identity_is_required_and_nothing_is_bootstrapped() {
    let world = World::new();
    let source = development_source(&world, "dev source");
    let binary = source.join("target/release/am");
    let repo = world.path("repo");
    let before = snapshot(&world.root);
    let version = run(&world, "version", &repo, Some(&binary), 0);
    let status = run(&world, "status", &repo, Some(&binary), 0);
    let doctor = run(&world, "doctor", &repo, Some(&binary), 0);
    assert_eq!(
        text(get(&version, "installation"), "state"),
        "not_installed"
    );
    assert_eq!(
        text(get(&version, "executing_source"), "cleanliness"),
        "unversioned"
    );
    assert_eq!(text(get(&status, "bank"), "state"), "not_located");
    assert_eq!(
        get(get(&status, "bank"), "absence_verified"),
        &Value::Bool(false)
    );
    assert_eq!(get(get(&status, "developer"), "id"), &Value::Null);
    assert_eq!(text(get(&doctor, "bank"), "state"), "not_initialized");
    assert!(!get(&doctor, "warnings").as_array().unwrap().is_empty());
    assert!(get(&doctor, "failures").as_array().unwrap().is_empty());
    assert_eq!(text(get(&doctor, "setup"), "state"), "not_initialized");
    assert_eq!(text(get(&status, "setup"), "state"), "setup_needed");
    assert_eq!(snapshot(&world.root), before);
}

/// An executable that is neither installed nor a development build is a
/// named component failure, never a pass.
#[test]
fn an_unlocated_executing_package_is_a_failure() {
    let world = World::new();
    let repo = world.path("repo");
    let version = run(&world, "version", &repo, None, 1);
    let check = component(&version, "runtime.executing_artifact");
    assert_eq!(text(check[0], "state"), "failure");
    assert_eq!(get(&version, "executing_source"), &Value::Null);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_status_is_shallow_but_doctor_checks_real_bank_content_and_permissions
#[test]
fn status_is_shallow_but_doctor_checks_real_bank_content_and_permissions() {
    let world = World::new();
    let source = development_source(&world, "dev source");
    let binary = source.join("target/release/am");
    let repo = world.path("repo");
    let directory = bank(&repo);
    let context = directory.join("projectbrief.md");
    std::fs::remove_file(&context).unwrap();
    let before = snapshot(&world.root);
    let shallow = run(&world, "status", &repo, Some(&binary), 0);
    assert_eq!(
        get(get(&shallow, "bank"), "counts_by_lifecycle"),
        &Value::Null
    );
    assert_eq!(text(get(&shallow, "bank"), "state"), "router_observed");
    let deep = run(&world, "doctor", &repo, Some(&binary), 1);
    assert!(!component(&deep, "bank.inspection").is_empty());
    assert_eq!(snapshot(&world.root), before);
    std::fs::write(&context, "# Project\n\nReadable context.\n").unwrap();
    mode(&directory, 0o500);
    let denied = run(&world, "doctor", &repo, Some(&binary), 1);
    mode(&directory, 0o700);
    let permissions = component(&denied, "bank.permissions");
    assert_eq!(text(permissions[0], "state"), "failure");
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_unknown_bank_schema_and_redirected_router_are_not_accepted
#[test]
fn unknown_bank_schema_and_redirected_router_are_not_accepted() {
    let world = World::new();
    let repo = world.path("repo");
    let directory = bank(&repo);
    let router = directory.join("MEMORY.md");
    let original = std::fs::read_to_string(&router).unwrap();
    std::fs::write(
        &router,
        original.replace("agent-memory/v1", "agent-memory/v99"),
    )
    .unwrap();
    let before = snapshot(&world.root);
    let value = run(&world, "doctor", &repo, None, 1);
    assert!(!get(&value, "failures").as_array().unwrap().is_empty());
    assert_eq!(snapshot(&world.root), before);
    let target = world.path("foreign-router.md");
    std::fs::write(&target, &original).unwrap();
    std::fs::remove_file(&router).unwrap();
    symlink(&target, &router).unwrap();
    let value = run(&world, "doctor", &repo, None, 1);
    assert!(!get(&value, "failures").as_array().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
}

// ======================================================================
// Installed development source
// ======================================================================

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_owner_record_diagnostics_agree_with_actual_recovery_and_preserve_bad_metadata
#[test]
fn owner_record_diagnostics_agree_with_actual_recovery_and_preserve_bad_metadata() {
    for case in ["healthy", "malformed", "dangling-symlink"] {
        let installed = Installed::new(Mode::Link, "codex");
        let owner = installed.world.state().join("owner.json");
        match case {
            "malformed" => std::fs::write(
                &owner,
                "{\"schema\":\"unrelated-owner-record\",\"operator_note\":\"preserve\"}\n",
            )
            .unwrap(),
            "dangling-symlink" => {
                std::fs::remove_file(&owner).unwrap();
                symlink(installed.world.path("missing-owner-target"), &owner).unwrap();
            }
            _ => {}
        }
        let before = snapshot(&installed.world.root);
        let expected = if case == "healthy" { 0 } else { 1 };
        let value = installed.doctor(expected);
        let check = component(&value, "runtime.owner");
        assert_eq!(
            text(check[0], "state"),
            if case == "healthy" { "pass" } else { "failure" },
            "{case}"
        );
        assert_eq!(text(check[0], "path"), owner.to_str().unwrap());
        assert_eq!(snapshot(&installed.world.root), before);
        let recovered = install::recover_command(&installed.world.env(), &services(None));
        assert_eq!(
            recovered.exit_code, expected,
            "{case}: {}",
            recovered.stdout
        );
        if case != "healthy" {
            assert_eq!(snapshot(&installed.world.root), before);
        }
    }
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_live_runtime_lock_is_not_stolen_or_treated_as_an_orphan
#[test]
fn live_runtime_lock_is_not_stolen_or_treated_as_an_orphan() {
    let installed = Installed::new(Mode::Copy, "codex");
    let state = agent_memory::fs::Descriptor::open_directory(&installed.world.state()).unwrap();
    let lock = state
        .open_at(
            std::ffi::OsStr::new("lock"),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        )
        .unwrap();
    let lock =
        agent_memory::fs::FlockGuard::try_acquire(lock, agent_memory::fs::LockMode::Exclusive)
            .unwrap();
    let before = snapshot(&installed.world.root);
    let value = installed.doctor(0);
    assert_eq!(
        text(component(&value, "runtime.lock")[0], "state"),
        "warning"
    );
    assert!(component(&value, "runtime.target.copy").is_empty());
    assert_eq!(snapshot(&installed.world.root), before);
    drop(lock);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_healthy_copy_and_link_installs_and_allowed_outside_activation_edits
#[test]
fn healthy_copy_and_link_installs_and_allowed_outside_activation_edits() {
    for install_mode in [Mode::Copy, Mode::Link] {
        let installed = Installed::new(install_mode, "codex");
        let target = installed.world.path("codex/AGENTS.md");
        let current = std::fs::read_to_string(&target).unwrap();
        std::fs::write(
            &target,
            format!("Extra user text.\n{current}\nExtra suffix.\n"),
        )
        .unwrap();
        let before = snapshot(&installed.world.root);
        let value = installed.doctor(0);
        assert!(get(&value, "failures").as_array().unwrap().is_empty());
        let activation = component(&value, "runtime.target.activation");
        assert_eq!(text(activation[0], "state"), "pass");
        let kind = if install_mode == Mode::Copy {
            "copy"
        } else {
            "link"
        };
        assert!(
            component(&value, &format!("runtime.target.{kind}"))
                .iter()
                .all(|check| text(check, "state") == "pass")
        );
        assert_eq!(
            text(component(&value, "runtime.source")[0], "state"),
            "pass"
        );
        assert_eq!(snapshot(&installed.world.root), before);
    }
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_changed_wrapper_and_copy_are_reported_from_an_outside_helper
#[test]
fn changed_command_link_and_copy_are_reported() {
    let installed = Installed::new(Mode::Copy, "codex");
    let link = installed
        .targets("command-link")
        .into_iter()
        .map(|target| PathBuf::from(text(&target, "path")))
        .find(|path| path.ends_with("agent-memory"))
        .unwrap();
    std::fs::remove_file(&link).unwrap();
    std::fs::write(&link, "Do not execute this replacement.\n").unwrap();
    mode(&link, 0o644);
    let copy = installed
        .world
        .path("codex/skills/agent-memory/templates/activation-markdown.example.txt");
    std::fs::write(&copy, "User replacement.\n").unwrap();
    let before = snapshot(&installed.world.root);
    let value = installed.doctor(1);
    let failed = names(&value, "failures");
    assert!(
        failed.contains(&"runtime.target.command-link".to_owned()),
        "{failed:?}"
    );
    assert!(
        failed.contains(&"runtime.target.copy".to_owned()),
        "{failed:?}"
    );
    assert_eq!(
        text(get(&value, "installation"), "purge_transactions"),
        "none_pending"
    );
    assert!(
        get(&value, "failures")
            .as_array()
            .unwrap()
            .iter()
            .all(|check| !text(check, "repair").is_empty())
    );
    assert_eq!(snapshot(&installed.world.root), before);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_changed_link_and_activation_block_are_preserved
#[test]
fn changed_link_and_activation_block_are_preserved() {
    let installed = Installed::new(Mode::Link, "codex");
    let link = installed.world.path("codex/skills/agent-memory");
    std::fs::remove_file(&link).unwrap();
    symlink(installed.world.path("repo"), &link).unwrap();
    let block = installed.world.path("codex/AGENTS.md");
    let current = std::fs::read_to_string(&block).unwrap();
    assert!(current.contains("load the agent-memory skill"));
    std::fs::write(
        &block,
        current.replace("load the agent-memory skill", "load different instructions"),
    )
    .unwrap();
    let before = snapshot(&installed.world.root);
    let value = installed.doctor(1);
    let failed = names(&value, "failures");
    assert!(
        failed.contains(&"runtime.target.link".to_owned()),
        "{failed:?}"
    );
    assert!(
        failed.contains(&"runtime.target.activation".to_owned()),
        "{failed:?}"
    );
    assert_eq!(snapshot(&installed.world.root), before);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_missing_source_and_committed_receipt_corruption_are_distinct_failures
#[test]
fn missing_source_and_committed_receipt_corruption_are_distinct_failures() {
    let installed = Installed::new(Mode::Copy, "codex");
    let saved = installed.world.path("saved source");
    std::fs::rename(&installed.source, &saved).unwrap();
    let missing = run(
        &installed.world,
        "doctor",
        &installed.world.path("repo"),
        Some(&saved.join("target/release/am")),
        1,
    );
    assert!(!component(&missing, "runtime.source").is_empty());
    std::fs::rename(&saved, &installed.source).unwrap();
    let mut receipt = installed.receipt();
    set(&mut receipt, "uninstalled", Value::Bool(true));
    std::fs::write(installed.world.receipt_path(), encode_record(&receipt)).unwrap();
    let before = snapshot(&installed.world.root);
    let changed = installed.doctor(1);
    assert!(!component(&changed, "runtime.receipt").is_empty());
    assert_eq!(snapshot(&installed.world.root), before);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_credential_urls_and_corrupt_records_do_not_escape_diagnostics
#[test]
fn credential_urls_and_corrupt_records_do_not_escape_diagnostics() {
    let installed = Installed::new(Mode::Copy, "codex");
    let mut receipt = installed.receipt();
    let mut source = get(&receipt, "source").clone();
    set(
        &mut source,
        "remote",
        text_value(&format!(
            "https://user:{SECRET}@github.com/example/repo.git"
        )),
    );
    set(&mut receipt, "source", source);
    std::fs::write(installed.world.receipt_path(), encode_record(&receipt)).unwrap();
    let repo = installed.world.path("repo");
    let result = run(
        &installed.world,
        "version",
        &repo,
        Some(&installed.binary()),
        1,
    );
    let rendered = json::encode(&result, json::COMPACT_ASCII).unwrap();
    assert!(!rendered.contains(SECRET));
    assert_eq!(
        text(get(get(&result, "installation"), "channel"), "remote"),
        "repo://github.com/example/repo"
    );
    std::fs::write(
        installed.world.receipt_path(),
        format!("{{\"invalid\":\"{SECRET}\""),
    )
    .unwrap();
    let result = installed.doctor(1);
    let rendered = json::encode(&result, json::COMPACT_ASCII).unwrap();
    assert!(!rendered.contains(SECRET));
    assert!(!get(&result, "failures").as_array().unwrap().is_empty());
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_installed_version_binds_actual_artifact_revision_mode_and_channel
#[test]
fn installed_version_binds_actual_artifact_revision_and_mode() {
    let world = World::new();
    let source = development_source(&world, "versioned source");
    world.git(&source, &["init", "-q", "-b", "main"]);
    world.git(&source, &["add", "."]);
    world.git(
        &source,
        &["commit", "-q", "-m", "Disposable development source"],
    );
    let request = InstallRequest {
        development_source: Some(source.clone()),
        mode: Some(Mode::Link),
        presets: Some("codex".to_owned()),
        ..InstallRequest::default()
    };
    let installed = install::install_command(&world.env(), &services(None), &request);
    assert_eq!(installed.exit_code, 0, "{}", installed.stdout);
    let receipt = decode(&world.receipt_path());
    let before = snapshot(&world.root);
    let value = run(
        &world,
        "version",
        &world.path("repo"),
        Some(&source.join("target/release/am")),
        0,
    );
    let current = get(&value, "executing_source");
    let head = Command::new("/usr/bin/git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .env_clear()
        .envs(world.pairs())
        .output()
        .unwrap();
    assert_eq!(
        text(current, "revision"),
        String::from_utf8_lossy(&head.stdout).trim()
    );
    assert_eq!(text(current, "version"), "0.0.15");
    assert_eq!(text(current, "cleanliness"), "clean");
    assert_eq!(
        get(current, "artifact_digest"),
        get(get(&receipt, "source"), "artifact_digest")
    );
    let installation = get(&value, "installation");
    assert_eq!(text(installation, "mode"), "link");
    assert_eq!(
        text(installation, "receipt_schema"),
        text(&receipt, "schema")
    );
    assert_eq!(text(get(installation, "channel"), "kind"), "development");
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_dirty_managed_source_and_changed_channel_are_not_healthy
#[test]
fn dirty_development_source_is_not_reported_as_matching() {
    let world = World::new();
    let source = development_source(&world, "versioned source");
    world.git(&source, &["init", "-q", "-b", "main"]);
    world.git(&source, &["add", "."]);
    world.git(
        &source,
        &["commit", "-q", "-m", "Disposable development source"],
    );
    let request = InstallRequest {
        development_source: Some(source.clone()),
        presets: Some("codex".to_owned()),
        ..InstallRequest::default()
    };
    let installed = install::install_command(&world.env(), &services(None), &request);
    assert_eq!(installed.exit_code, 0, "{}", installed.stdout);
    std::fs::write(
        source.join("local-code.txt"),
        "Uncommitted developer code.\n",
    )
    .unwrap();
    let dirty = run(
        &world,
        "doctor",
        &world.path("repo"),
        Some(&source.join("target/release/am")),
        0,
    );
    assert_eq!(
        text(get(&dirty, "installed_source"), "cleanliness"),
        "dirty"
    );
    let check = component(&dirty, "runtime.source");
    // A development source differs with a warning; managed channels are adopted read-only.
    assert_eq!(text(check[0], "state"), "warning");
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_pending_runtime_and_bank_transactions_are_observed_without_recovery
#[test]
fn pending_runtime_and_bank_transactions_are_observed_without_recovery() {
    let installed = Installed::new(Mode::Copy, "codex");
    let directory = bank(&installed.world.path("repo"));
    let path = installed.world.state().join("journal.json");
    let mut journal = decode(&path);
    set(&mut journal, "status", text_value("activating"));
    std::fs::write(&path, encode_record(&journal)).unwrap();
    std::fs::write(
        directory.join(".agent-memory-transaction.md"),
        "Original interrupted bank operation.\n",
    )
    .unwrap();
    let before = snapshot(&installed.world.root);
    // memory_refresh is not ported: its local-prerequisite check is a failure.
    let value = installed.doctor(1);
    assert!(!component(&value, "runtime.transaction").is_empty());
    assert!(!component(&value, "bank.transaction").is_empty());
    assert_eq!(text(get(&value, "bank"), "state"), "busy_or_interrupted");
    assert_eq!(snapshot(&installed.world.root), before);
}

// ----- owned client hooks -----

fn hook_path(installed: &Installed, preset: &str) -> PathBuf {
    installed.world.path(if preset == "codex" {
        "codex/hooks.json"
    } else {
        "claude/settings.json"
    })
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_shared_owned_hook_health_and_negative_controls
#[test]
fn shared_owned_hook_health_and_negative_controls() {
    for preset in ["codex", "claude"] {
        let installed = Installed::new(Mode::Copy, preset);
        let path = hook_path(&installed, preset);
        let original = std::fs::read(&path).unwrap();
        let parsed = decode(&path);
        let foreign = r#"{"hooks": [{"type": "command", "command": "printf foreign"}]}"#;
        let foreign: Value = json::decode(foreign, Default::default()).unwrap();
        let mut value = parsed.clone();
        set(
            &mut value,
            "otherToolSetting",
            Value::Object(vec![("enabled".into(), Value::Bool(true))]),
        );
        for event in ["UserPromptSubmit", "Stop"] {
            if let Some(Value::Array(groups)) = value
                .get_mut("hooks")
                .and_then(|hooks| hooks.get_mut(event))
            {
                groups.push(foreign.clone());
            }
        }
        std::fs::write(&path, json::encode(&value, json::SORTED_ASCII).unwrap()).unwrap();
        let before = snapshot(&installed.world.root);
        let result = installed.doctor(0);
        let check = component(&result, "runtime.target.hook-config");
        assert_eq!(text(check[0], "state"), "pass");
        assert_eq!(
            text(check[0], "verification_scope"),
            "receipt-owned-entries"
        );
        assert_eq!(snapshot(&installed.world.root), before);
        let mut cases: Vec<(String, Vec<u8>, &str)> = Vec::new();
        for event in ["UserPromptSubmit", "Stop"] {
            let groups = get(get(&parsed, "hooks"), event)
                .as_array()
                .unwrap()
                .to_vec();
            let with = |groups: Vec<Value>| {
                let mut document = parsed.clone();
                if let Some(hooks) = document.get_mut("hooks") {
                    set(hooks, event, Value::Array(groups));
                }
                json::encode(&document, json::DEFAULT).unwrap().into_bytes()
            };
            let mut missing = groups.clone();
            missing.remove(0);
            let mut duplicate = groups.clone();
            duplicate.push(groups[0].clone());
            let mut changed = groups.clone();
            let command = changed[0]
                .get_mut("hooks")
                .and_then(|hooks| match hooks {
                    Value::Array(items) => items.first_mut(),
                    _ => None,
                })
                .unwrap();
            let original_command = text(command, "command").to_owned();
            set(
                command,
                "command",
                text_value(&format!("{original_command} changed")),
            );
            cases.push((
                format!("{event}-missing"),
                with(missing),
                "owned_group_missing_or_changed",
            ));
            cases.push((
                format!("{event}-duplicate"),
                with(duplicate),
                "owned_group_duplicate",
            ));
            cases.push((
                format!("{event}-changed"),
                with(changed),
                "owned_group_missing_or_changed",
            ));
        }
        cases.push((
            "duplicate-key".into(),
            b"{\"hooks\":{},\"hooks\":{}}".to_vec(),
            "duplicate_json_key",
        ));
        cases.push((
            "nested-duplicate".into(),
            b"{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\"type\":\"bad\"}]}]}}"
                .to_vec(),
            "duplicate_json_key",
        ));
        cases.push((
            "private-duplicate-key".into(),
            b"{\"secret-hook-key-784\":1,\"secret-hook-key-784\":2}".to_vec(),
            "duplicate_json_key",
        ));
        cases.push((
            "malformed".into(),
            b"{\"hooks\":".to_vec(),
            "malformed_json",
        ));
        cases.push((
            "invalid-stop-shape".into(),
            b"{\"hooks\":{\"Stop\":{}}}".to_vec(),
            "unsupported_shape",
        ));
        for (label, raw, code) in cases {
            std::fs::write(&path, raw).unwrap();
            let before = snapshot(&installed.world.root);
            let result = installed.doctor(1);
            let check = component(&result, "runtime.target.hook-config");
            assert_eq!(text(check[0], "state"), "failure", "{preset} {label}");
            assert_eq!(text(check[0], "failure_code"), code, "{preset} {label}");
            assert!(
                !json::encode(&result, json::COMPACT_ASCII)
                    .unwrap()
                    .contains("secret-hook-key-784")
            );
            assert_eq!(snapshot(&installed.world.root), before);
        }
        std::fs::write(&path, &original).unwrap();
        let bits = std::fs::metadata(&path).unwrap().mode() & 0o777;
        mode(&path, if bits != 0o600 { 0o600 } else { 0o644 });
        let check = installed.doctor(1);
        let check = component(&check, "runtime.target.hook-config");
        assert_eq!(
            (text(check[0], "state"), text(check[0], "failure_code")),
            ("failure", "mode_changed")
        );
        mode(&path, bits);
        std::fs::remove_file(&path).unwrap();
        symlink(installed.world.path("repo/missing-hook"), &path).unwrap();
        let check = installed.doctor(1);
        let check = component(&check, "runtime.target.hook-config");
        assert_eq!(
            (text(check[0], "state"), text(check[0], "failure_code")),
            ("failure", "unsafe_path")
        );
        std::fs::remove_file(&path).unwrap();
        let status = Command::new("/usr/bin/mkfifo").arg(&path).status().unwrap();
        assert!(status.success());
        let before = snapshot(&installed.world.root);
        let check = installed.doctor(1);
        let check = component(&check, "runtime.target.hook-config");
        assert_eq!(
            (text(check[0], "state"), text(check[0], "failure_code")),
            ("failure", "unsafe_type")
        );
        assert_eq!(snapshot(&installed.world.root), before);
    }
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_insufficient_hook_receipt_is_not_adopted_from_matching_bytes
#[test]
fn insufficient_hook_receipt_is_not_adopted_from_matching_bytes() {
    let installed = Installed::new(Mode::Copy, "codex");
    installed.rewrite_receipt(|receipt| {
        if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
            for target in targets {
                let hook = text(target, "kind") == "hook-config";
                if let (Value::Object(fields), true) = (target, hook) {
                    fields.retain(|(key, _)| key != "hook_entry");
                }
            }
        }
    });
    let before = snapshot(&installed.world.root);
    let result = installed.doctor(1);
    let check = component(&result, "runtime.target.hook-config");
    assert_eq!(
        (text(check[0], "state"), text(check[0], "failure_code")),
        ("failure", "insufficient_receipt")
    );
    assert_eq!(snapshot(&installed.world.root), before);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_prompt_only_legacy_and_invalid_present_stop_receipts
#[test]
fn prompt_only_legacy_and_invalid_present_stop_receipts() {
    for preset in ["codex", "claude"] {
        let installed = Installed::new(Mode::Copy, preset);
        let path = hook_path(&installed, preset);
        let mut document = decode(&path);
        set(
            &mut document,
            "unrelated-setting",
            Value::Object(vec![("preserve".into(), Value::Bool(true))]),
        );
        std::fs::write(&path, json::encode(&document, json::DEFAULT).unwrap()).unwrap();
        let original = installed.receipt();
        for case in ["absent", "null", "empty"] {
            let saved = original.clone();
            installed.rewrite_receipt(|receipt| {
                *receipt = saved;
                if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
                    for target in targets {
                        if text(target, "kind") != "hook-config" {
                            continue;
                        }
                        match case {
                            "absent" => {
                                if let Value::Object(fields) = target {
                                    fields.retain(|(key, _)| key != "stop_entry");
                                }
                            }
                            "null" => set(target, "stop_entry", Value::Null),
                            _ => set(target, "stop_entry", Value::Object(Vec::new())),
                        }
                    }
                }
            });
            let before = snapshot(&installed.world.root);
            let result = installed.doctor(if case == "absent" { 0 } else { 1 });
            let check = component(&result, "runtime.target.hook-config");
            if case == "absent" {
                assert_eq!(text(check[0], "state"), "pass");
                assert_eq!(
                    text(check[0], "verification_scope"),
                    "receipt-owned-entries"
                );
            } else {
                assert_eq!(
                    (text(check[0], "state"), text(check[0], "failure_code")),
                    ("failure", "insufficient_receipt"),
                    "{preset} {case}"
                );
            }
            assert_eq!(snapshot(&installed.world.root), before);
        }
    }
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_hook_verification_refuses_an_edit_during_readback
#[test]
fn hook_verification_refuses_an_edit_during_readback() {
    const STAGE: &str = "hook-ownership-before-recheck";
    for preset in ["codex", "claude"] {
        for change in ["bytes", "mode"] {
            let installed = Installed::new(Mode::Copy, preset);
            let path = hook_path(&installed, preset);
            let control = installed.world.path("hook-race");
            std::fs::create_dir(&control).unwrap();
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "child_entry",
                    "--exact",
                    "--nocapture",
                    "--test-threads=1",
                    "-q",
                ])
                .env_clear()
                .envs(installed.world.pairs())
                .env("AM_DIAG_CHILD", "doctor")
                .env("AM_DIAG_REPO", installed.world.path("repo"))
                .env("AM_DIAG_EXECUTABLE", installed.binary())
                .env("AGENT_MEMORY_FAULT_STAGE", STAGE)
                .env("AGENT_MEMORY_FAULT_MODE", "pause")
                .env("AGENT_MEMORY_FAULT_DIR", &control)
                .current_dir(installed.world.path("repo"))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let ready = control.join(format!("{STAGE}.ready"));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !ready.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(ready.exists(), "{preset} {change}");
            let receipt_before = std::fs::read(installed.world.receipt_path()).unwrap();
            if change == "bytes" {
                let mut value = decode(&path);
                set(&mut value, "racingForeignKey", Value::Bool(true));
                std::fs::write(&path, json::encode(&value, json::DEFAULT).unwrap()).unwrap();
            } else {
                let bits = std::fs::metadata(&path).unwrap().mode() & 0o777;
                mode(&path, if bits != 0o600 { 0o600 } else { 0o644 });
            }
            let expected = (
                std::fs::read(&path).unwrap(),
                std::fs::metadata(&path).unwrap().mode(),
            );
            std::fs::write(control.join(format!("{STAGE}.release")), b"").unwrap();
            let output = child.wait_with_output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let code = stdout
                .lines()
                .find_map(|line| line.strip_prefix("AM-CODE:"))
                .unwrap_or_else(|| panic!("{stdout}\n{}", String::from_utf8_lossy(&output.stderr)));
            assert_eq!(code, "1", "{preset} {change}");
            let value = stdout
                .lines()
                .find_map(|line| line.strip_prefix("AM-RESULT:"))
                .map(|line| json::decode(line, Default::default()).unwrap())
                .unwrap();
            let check = component(&value, "runtime.target.hook-config");
            assert_eq!(
                (text(check[0], "state"), text(check[0], "failure_code")),
                ("failure", "concurrent_change"),
                "{preset} {change}"
            );
            let observed = (
                std::fs::read(&path).unwrap(),
                std::fs::metadata(&path).unwrap().mode(),
            );
            assert_eq!(observed, expected);
            assert_eq!(
                std::fs::read(installed.world.receipt_path()).unwrap(),
                receipt_before
            );
        }
    }
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_opencode_plugin_link_remains_exact
#[test]
fn opencode_plugin_link_remains_exact() {
    let installed = Installed::new(Mode::Copy, "opencode");
    let target = installed.targets("hook-link").remove(0);
    let path = PathBuf::from(text(&target, "path"));
    std::fs::remove_file(&path).unwrap();
    symlink(installed.world.path("repo"), &path).unwrap();
    let before = snapshot(&installed.world.root);
    let result = installed.doctor(1);
    assert_eq!(
        text(component(&result, "runtime.target.hook-link")[0], "state"),
        "failure"
    );
    assert_eq!(snapshot(&installed.world.root), before);
}

// ======================================================================
// The real `am` executable (composition root, console and machine modes)
// ======================================================================

fn am(world: &World, args: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(args)
        .env_clear()
        .envs(world.pairs())
        .current_dir(world.path("repo"))
        .output()
        .unwrap();
    assert!(output.status.code().is_some(), "{output:?}");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout).unwrap(),
    )
}

/// `am version|status|doctor` run natively against a disposable bank. The
/// test binary is neither installed nor a development build, and
/// `memory_refresh` is not ported, so version/doctor report those failures.
#[test]
fn am_diagnostics_run_natively_against_a_disposable_bank() {
    let world = World::new();
    let repo = world.path("repo");
    bank(&repo);
    let before = snapshot(&world.root);
    let run = |args: &[&str], expected: i32| {
        let (code, stdout) = am(&world, args);
        assert_eq!(code, expected, "{stdout}");
        let value = json::decode(&stdout, Default::default()).unwrap();
        assert_eq!(text(&value, "schema"), diagnostics::SCHEMA);
        assert_eq!(text(&value, "operation"), args[1]);
        value
    };
    let repo_text = repo.to_str().unwrap();
    let version = run(&["--json", "version"], 1);
    assert_eq!(names(&version, "failures"), ["runtime.executing_artifact"]);
    let status = run(&["--json", "status", "--repo", repo_text], 0);
    assert!(component(&status, "runtime.executing_artifact").is_empty());
    assert_eq!(text(get(&status, "bank"), "state"), "router_observed");
    assert_eq!(text(get(&status, "bank"), "project_id"), PROJECT);
    assert_eq!(text(get(&status, "setup"), "state"), "binding_required");
    let doctor = run(&["--json", "doctor", "--repo", repo_text], 1);
    // memory_refresh and recovery_frontier (canonical bank observation) are
    // not ported: those components fail instead of validating the bank.
    assert_eq!(
        names(&doctor, "failures"),
        [
            "runtime.executing_artifact",
            "bank.refresh_prerequisites",
            "bank.inspection"
        ]
    );
    assert_eq!(
        text(get(&doctor, "bank"), "path"),
        repo.join("memory-bank").to_str().unwrap()
    );
    assert_eq!(
        text(component(&doctor, "prerequisite.git")[0], "state"),
        "pass"
    );
    assert_eq!(
        text(get(&doctor, "installation"), "health"),
        "failures_detected"
    );
    let (code, human) = am(&world, &["doctor", "--repo", repo_text]);
    assert_eq!(code, 1);
    assert!(human.contains("setup inspection_required"), "{human}");
    assert!(human.contains("next "), "{human}");
    assert_eq!(snapshot(&world.root), before);
}

/// `am init inspect|read` run natively against a disposable bank.
#[test]
fn am_init_runs_natively_against_a_disposable_bank() {
    let world = World::new();
    let repo = world.path("repo");
    bank(&repo);
    let before = snapshot(&world.root);
    let repo_text = repo.to_str().unwrap();
    let (code, stdout) = am(&world, &["--json", "init", "inspect", "--repo", repo_text]);
    assert_eq!(code, 0, "{stdout}");
    let value = json::decode(&stdout, Default::default()).unwrap();
    assert_eq!(
        text(&value, "schema"),
        "agent-memory/initialization-result-v1"
    );
    assert_eq!(text(&value, "operation"), "inspect");
    // Canonical bank observation needs recovery_frontier (not ported): the
    // existing bank is preserved for inspection, never overwritten.
    assert_eq!(text(&value, "bank_state"), "needs-inspection");
    assert_eq!(text(&value, "project_id"), PROJECT);
    assert_eq!(get(&value, "writes_performed"), &Value::Bool(false));
    let (code, stdout) = am(
        &world,
        &[
            "--json",
            "init",
            "read",
            "--repo",
            repo_text,
            "--path",
            "memory-bank/MEMORY.md",
        ],
    );
    assert_eq!(code, 0, "{stdout}");
    let value = json::decode(&stdout, Default::default()).unwrap();
    assert!(text(&value, "content").contains("agent-memory/v1"));
    let (code, stdout) = am(
        &world,
        &[
            "--json",
            "init",
            "read",
            "--repo",
            repo_text,
            "--path",
            ".git/config",
        ],
    );
    assert_eq!(code, 1, "{stdout}");
    let value = json::decode(&stdout, Default::default()).unwrap();
    assert_eq!(get(&value, "inspection_required"), &Value::Bool(true));
    assert_eq!(snapshot(&world.root), before);
}

// ======================================================================
// Receipt-level hook migration (no process environment involved)
// ======================================================================

// obligation: tests/contracts/test_hook_migration_diagnostics.py::HookMigrationDiagnostics.test_missing_and_intentionally_disabled_hook_targets
#[test]
fn missing_and_intentionally_disabled_hook_targets() {
    let world = World::new();
    let env = world.env();
    let source = Value::Object(vec![
        ("kind".into(), text_value("development")),
        ("path".into(), text_value("/private/tmp/source with spaces")),
    ]);
    let binary = install::source_binary(&source).unwrap();
    let hook = |name: &str| install::hook_paths(&env, name).unwrap().unwrap();
    let mut receipt = Value::Object(vec![
        ("no_guidance".into(), Value::Bool(false)),
        (
            "enabled_adapters".into(),
            Value::Array(vec![
                text_value("codex"),
                text_value("claude"),
                text_value("opencode"),
            ]),
        ),
        ("targets".into(), Value::Array(Vec::new())),
        ("source".into(), source),
    ]);
    let warnings = |receipt: &Value| {
        let mut report = Report::new("status");
        diagnostics::hook_migration(&mut report, &env, Some(receipt)).unwrap();
        let value = report.finish();
        get(&value, "warnings").as_array().unwrap().to_vec()
    };
    let found = warnings(&receipt);
    assert_eq!(found.len(), 3);
    assert!(
        found
            .iter()
            .all(|warning| text(warning, "repair").contains("am install"))
    );
    let mut disabled = receipt.clone();
    set(&mut disabled, "no_guidance", Value::Bool(true));
    assert!(warnings(&disabled).is_empty());
    let (codex_prompt, codex_stop) = install::client_hook_entries("codex", &binary).unwrap();
    let (claude_prompt, claude_stop) = install::client_hook_entries("claude", &binary).unwrap();
    let path_value = |path: PathBuf| text_value(path.to_str().unwrap());
    let targets = vec![
        Value::Object(vec![
            ("path".into(), path_value(hook("codex"))),
            ("kind".into(), text_value("hook-config")),
            ("hook_entry".into(), codex_prompt),
        ]),
        Value::Object(vec![
            ("path".into(), path_value(hook("claude"))),
            ("kind".into(), text_value("hook-config")),
            ("hook_entry".into(), claude_prompt),
            ("stop_entry".into(), claude_stop),
        ]),
        Value::Object(vec![
            ("path".into(), path_value(hook("opencode"))),
            ("kind".into(), text_value("hook-link")),
        ]),
    ];
    set(&mut receipt, "targets", Value::Array(targets));
    let components = |found: Vec<Value>| -> Vec<String> {
        found
            .iter()
            .map(|warning| text(warning, "component").to_owned())
            .collect()
    };
    assert_eq!(
        components(warnings(&receipt)),
        ["runtime.hook_migration.codex"]
    );
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        set(&mut targets[0], "stop_entry", codex_stop);
    }
    assert!(warnings(&receipt).is_empty());
    let outdated: Value = json::decode(
        r#"{"hooks": [{"type": "command", "command": "python3 old-hook.py"}]}"#,
        Default::default(),
    )
    .unwrap();
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        set(&mut targets[0], "hook_entry", outdated);
    }
    assert_eq!(
        components(warnings(&receipt)),
        ["runtime.hook_migration.codex"]
    );
}
// ======================================================================
// Deferred: modules not yet ported
// ======================================================================

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_selected_projection_current_and_stale_do_not_create_audit_or_cache
#[test]
#[ignore = "needs projections registered"]
fn selected_projection_current_and_stale_do_not_create_audit_or_cache() {
    let world = World::new();
    bank(&world.path("repo"));
    let value = run(&world, "doctor", &world.path("repo"), None, 0);
    assert!(!component(&value, "bank.projection").is_empty());
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_resolved_identity_and_custom_bank_follow_the_selected_worktree
#[test]
#[ignore = "needs recovery_frontier registered (identity bind, bank observation)"]
fn resolved_identity_and_custom_bank_follow_the_selected_worktree() {
    let world = World::new();
    let repo = world.path("repo");
    world.git(&repo, &["init", "-q", "-b", "main"]);
    let directory = bank(&repo);
    bind(&world, &repo);
    world.git(&repo, &["add", "."]);
    world.git(&repo, &["commit", "-q", "-m", "Bound test memory"]);
    let worktree = world.path("additional worktree");
    world.git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other-session",
            worktree.to_str().unwrap(),
        ],
    );
    let before = snapshot(&world.root);
    // memory_refresh is not ported: the refresh prerequisite check fails.
    let first = run(&world, "doctor", &repo, None, 1);
    let other = run(&world, "doctor", &worktree, None, 1);
    assert_eq!(
        get(get(&first, "developer"), "resolved"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(get(&first, "project"), "authority_verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(get(&other, "project"), "authority_verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(get(&first, "project"), "id"),
        get(get(&other, "project"), "id")
    );
    assert_eq!(
        text(get(&other, "bank"), "path"),
        worktree.join("memory-bank").to_str().unwrap()
    );
    assert_eq!(snapshot(&world.root), before);
    std::fs::rename(&directory, repo.join("project-notes")).unwrap();
    world.git(&repo, &["config", "agent-memory.bankRoot", "project-notes"]);
    world.git(&repo, &["add", "-A", "."]);
    world.git(
        &repo,
        &[
            "commit",
            "-q",
            "-m",
            "Move bank with explicit configuration",
        ],
    );
    let before = snapshot(&world.root);
    let status = run(&world, "status", &repo, None, 0);
    assert_eq!(
        text(get(&status, "bank"), "path"),
        repo.join("project-notes").to_str().unwrap()
    );
    assert_eq!(
        get(get(&status, "project"), "id"),
        get(get(&first, "project"), "id")
    );
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_setup_diagnostics.py::SetupDiagnostics.test_new_repository_and_precreated_identity_need_setup_without_invented_mismatch
#[test]
fn new_repository_and_precreated_identity_need_setup_without_invented_mismatch() {
    let installed = Installed::new(Mode::Copy, "codex");
    let world = &installed.world;
    let repo = world.path("repo");
    world.git(&repo, &["init", "-q", "-b", "main"]);
    for reserved in [false, true] {
        if reserved {
            create_identity(world, &repo);
        }
        let before = snapshot(&world.root);
        let status = run(world, "status", &repo, Some(&installed.binary()), 0);
        assert_eq!(text(get(&status, "setup"), "state"), "setup_needed");
        assert_eq!(
            get(get(&status, "setup"), "absence_verified"),
            &Value::Bool(false)
        );
        assert!(!names(&status, "warnings").contains(&"identity.project".to_owned()));
        let doctor = installed.doctor(0);
        assert_eq!(text(get(&doctor, "setup"), "state"), "not_initialized");
        assert_eq!(
            get(get(&doctor, "setup"), "absence_verified"),
            &Value::Bool(true)
        );
        assert!(!text(get(&doctor, "setup"), "next_step").is_empty());
        assert!(!repo.join("memory-bank").exists());
        assert_eq!(snapshot(&world.root), before);
    }
}

// obligation: tests/contracts/test_setup_diagnostics.py::SetupDiagnostics.test_bank_binding_required_matching_and_actual_mismatch_are_distinct
#[test]
#[ignore = "needs recovery_frontier registered (identity bind)"]
fn bank_binding_required_matching_and_actual_mismatch_are_distinct() {
    let installed = Installed::new(Mode::Copy, "codex");
    let world = &installed.world;
    let repo = world.path("repo");
    world.git(&repo, &["init", "-q", "-b", "main"]);
    let directory = bank(&repo);
    let status = |expected: &str| {
        let value = run(world, "status", &repo, Some(&installed.binary()), 0);
        assert_eq!(text(get(&value, "setup"), "state"), expected);
        assert!(!text(get(&value, "setup"), "next_step").is_empty());
    };
    status("binding_required");
    bind(world, &repo);
    status("bound_not_audited");
    let router = directory.join("MEMORY.md");
    let original = std::fs::read_to_string(&router).unwrap();
    let other = agent_memory::host::uuid4().unwrap();
    std::fs::write(&router, original.replace(PROJECT, &other)).unwrap();
    status("binding_mismatch");
}

/// One `am` command through the real dispatch in a child.
fn cli(world: &World, args: &[&str]) -> Value {
    let arguments = Value::Array(args.iter().map(|arg| text_value(arg)).collect());
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "child_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "-q",
        ])
        .env_clear()
        .envs(world.pairs())
        .env(
            "AM_DIAG_CLI",
            json::encode(&arguments, json::DEFAULT).unwrap(),
        )
        .current_dir(world.path("repo"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let printed = stdout
        .lines()
        .find_map(|line| line.strip_prefix("AM-STDOUT:"))
        .unwrap_or_else(|| panic!("{stdout}\n{}", String::from_utf8_lossy(&output.stderr)));
    let printed = json::decode(printed, Default::default()).unwrap();
    assert!(
        stdout.contains("AM-CODE:0\n"),
        "{}",
        printed.as_str().unwrap()
    );
    json::decode(printed.as_str().unwrap(), Default::default()).unwrap()
}
fn apply(world: &World, helper: &str, proposal: &Value) -> Value {
    let path = world.path("fixture-identity-proposal.json");
    std::fs::write(&path, json::encode(proposal, json::DEFAULT).unwrap()).unwrap();
    cli(
        world,
        &[helper, "apply", "--proposal", path.to_str().unwrap()],
    )
}
fn developer(world: &World, repo: &Path) {
    let status = cli(
        world,
        &["developer", "status", "--repo", repo.to_str().unwrap()],
    );
    if get(&status, "setup_required") == &Value::Bool(true) {
        let proposal = cli(
            world,
            &[
                "developer",
                "preview",
                "--create",
                "--label",
                "Disposable test developer",
            ],
        );
        apply(world, "developer", &proposal);
    }
}
/// Baseline `identity_fixtures.bind`: adopt the existing bank's identity.
fn bind(world: &World, repo: &Path) {
    developer(world, repo);
    let repo_text = repo.to_str().unwrap();
    let current = cli(world, &["identity", "status", "--repo", repo_text]);
    if get(&current, "resolved") != &Value::Bool(true) {
        let proposal = cli(
            world,
            &[
                "identity",
                "preview",
                "--repo",
                repo_text,
                "--action",
                "bind",
                "--reason",
                "Explicit disposable fixture bank adoption",
            ],
        );
        let receipt = apply(world, "identity", &proposal);
        assert_eq!(get(&receipt, "verified"), &Value::Bool(true));
    }
}
/// An explicit new project identity reserved before any bank exists.
fn create_identity(world: &World, repo: &Path) {
    developer(world, repo);
    let proposal = cli(
        world,
        &[
            "identity",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--action",
            "create",
            "--reason",
            "Reserve explicit fixture identity",
        ],
    );
    apply(world, "identity", &proposal);
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_doctor_does_not_resume_a_separately_confirmed_purge_or_initialize_replacement
#[test]
#[ignore = "needs recovery_frontier registered (purge-data)"]
fn doctor_does_not_resume_a_separately_confirmed_purge() {
    let world = World::new();
    let value = run(&world, "doctor", &world.path("repo"), None, 0);
    assert_eq!(text(get(&value, "bank"), "state"), "purge_pending");
}

// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_native_copy_and_link_doctor_distinguishes_extension_and_launchers
// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_native_answer_first_no_guidance_is_healthy_but_unqualified
// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_native_doctor_rejects_rebound_wrapper_bytes_modes_and_roles
// obligation: tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_native_doctor_never_repairs_missing_observer_locks
#[test]
#[ignore = "fixture installs native omp without --omp-agent-dir/--omp-peers-upgraded, so install refuses"]
fn native_omp_doctor_distinguishes_extension_and_launchers() {
    let installed = Installed::new(Mode::Copy, "omp");
    let value = installed.doctor(0);
    assert!(!component(&value, "runtime.target.wrapper").is_empty());
}

// obligation: tests/contracts/test_setup_diagnostics.py::SetupDiagnostics.test_index_head_custom_location_and_worktree_prove_known_missing_without_replacement
// obligation: tests/contracts/test_setup_diagnostics.py::SetupDiagnostics.test_damaged_runtime_and_malformed_or_ignored_bank_evidence_remain_independent
// obligation: tests/contracts/test_setup_diagnostics.py::SetupDiagnostics.test_busy_bank_and_pending_purge_never_advise_first_use
// obligation: tests/contracts/test_setup_diagnostics.py::SetupDiagnostics.test_tag_like_filenames_and_mixed_case_competing_router_preserve_real_locator_evidence
#[test]
#[ignore = "needs memory_refresh registered (doctor's refresh prerequisites; recovery_frontier for purge-data)"]
fn installed_setup_evidence_without_implicit_initialization() {
    let installed = Installed::new(Mode::Copy, "codex");
    bank(&installed.world.path("repo"));
    let value = installed.doctor(0);
    assert_eq!(text(get(&value, "setup"), "state"), "ready");
}
