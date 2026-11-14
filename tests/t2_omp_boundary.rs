//! omp_boundary and claude_turns: protected native bindings, markers,
//! registry, lifecycle journals and the `am omp-boundary` route. Every
//! environment-dependent case re-executes this binary with a private 0700
//! world (HOME/XDG roots inside it); no global environment is mutated.
#[allow(unused_imports)]
mod support;

use agent_memory::claude_turns;
use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::error::{ContractError, Result};
use agent_memory::{host, installation, omp_boundary as boundary, omp_installation, process};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::{RunOptions, World};

const FAILURE: &str = "{\"verified\": false, \"status\": \"pending\", \"canonical_writes_performed\": false, \"reason\": \"Native source work remains pending; inspect owned installation, binding and retained sources. No completion is inferred.\"}\n";
const QUIET_FAILURE: &str = "am: Native source work remains pending; inspect owned installation, binding and retained sources. No completion is inferred.\n";
const PROJECT: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const ID: &str = "0123456789abcdef0123456789abcdef";

fn decode(text: &str) -> Value {
    json::decode(text, DecodePolicy::default()).unwrap()
}
fn refusal<T: std::fmt::Debug>(value: Result<T>, expected: &str) {
    assert_eq!(value.unwrap_err().message(), expected);
}
fn install_refusal<T: std::fmt::Debug>(value: installation::Result<T>, expected: &str) {
    match value.unwrap_err() {
        installation::Error::Contract(error) => assert_eq!(error.message(), expected),
        other => panic!("unexpected refusal {other}"),
    }
}
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn mkdir(path: &Path) {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .unwrap();
}
fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("am-omp-boundary-{}", host::uuid4().unwrap()));
        mkdir(&path);
        Self(std::fs::canonicalize(path).unwrap())
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static OUTER_LAUNCH: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Run `body` in a re-executed copy of this test with a private environment.
fn isolated(name: &str, body: impl FnOnce(&Path)) {
    if std::env::var("AM_OMP_BOUNDARY_CASE").ok().as_deref() == Some(name) {
        let root = PathBuf::from(std::env::var_os("AM_OMP_BOUNDARY_ROOT").unwrap());
        agent_memory::cli::register_providers().unwrap();
        body(&root);
        return;
    }
    let _launch = OUTER_LAUNCH.lock().unwrap_or_else(|e| e.into_inner());
    let root = Root::new();
    for directory in ["home", "config", "state", "data", "runtime"] {
        mkdir(&root.0.join(directory));
    }
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        name.into(),
        "--include-ignored".into(),
        "--nocapture".into(),
    ];
    let env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("HOME".into(), root.0.join("home").into_os_string()),
        (
            "XDG_CONFIG_HOME".into(),
            root.0.join("config").into_os_string(),
        ),
        (
            "XDG_STATE_HOME".into(),
            root.0.join("state").into_os_string(),
        ),
        ("XDG_DATA_HOME".into(), root.0.join("data").into_os_string()),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("AM_OMP_BOUNDARY_CASE".into(), name.into()),
        ("AM_OMP_BOUNDARY_ROOT".into(), root.0.as_os_str().to_owned()),
    ];
    let spec = process::Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&root.0),
        search_path: false,
        stdin: process::Input::Null,
        stdout: process::Target::Capture { limit: 1024 * 1024 },
        stderr: process::Target::Capture { limit: 1024 * 1024 },
        deadline: Deadline::from_absolute_ns(u64::MAX),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    let output = process::run_for(&spec, Duration::from_secs(60)).unwrap();
    assert_eq!(
        output.status,
        process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A plain (non-Git) repository with a bank and its expected private base.
struct Bank {
    repo: PathBuf,
    bank: PathBuf,
    base: PathBuf,
}
fn bank(root: &Path) -> Bank {
    let repo = root.join("repo");
    let bank = repo.join("memory-bank");
    mkdir(&bank);
    let base = root
        .join("state/agent-memory/writes")
        .join(sha256(bank.to_str().unwrap().as_bytes()));
    Bank { repo, bank, base }
}
fn binding_bytes(fixture: &Bank) -> String {
    format!(
        "{{\"bank_path\":\"{}\",\"private_base\":\"{}\",\"project_id\":\"{PROJECT}\",\"repository\":\"{}\",\"schema\":\"agent-memory/omp-bank-binding-v1\"}}",
        fixture.bank.display(),
        fixture.base.display(),
        fixture.repo.display()
    )
}
fn marker_bytes(token: &str) -> String {
    format!(
        "{{\"harnesses\":[\"omp\"],\"minimum_reader\":\"{token}\",\"schema\":\"agent-memory/turn-sources-compat-v1\"}}"
    )
}
fn registry_bytes(fixture: &Bank) -> String {
    format!(
        "{{\"private_bases\":{{\"{repo}\":\"{base}\"}},\"repositories\":[\"{repo}\"],\"schema\":\"agent-memory/omp-bank-registry-v1\"}}",
        repo = fixture.repo.display(),
        base = fixture.base.display()
    )
}
fn registry_path(root: &Path) -> PathBuf {
    root.join("config/agent-memory/omp-banks.json")
}
/// The protected state a stored-source activation leaves behind.
fn enrolled(root: &Path) -> Bank {
    let fixture = bank(root);
    mkdir(&fixture.base);
    write(
        &fixture.base.join("omp-bank-binding-v1.json"),
        binding_bytes(&fixture).as_bytes(),
    );
    write(
        &fixture.base.join("turn-sources-compat-v1.json"),
        marker_bytes(boundary::CAPABILITY).as_bytes(),
    );
    mkdir(&root.join("config/agent-memory"));
    write(&registry_path(root), registry_bytes(&fixture).as_bytes());
    fixture
}
fn tokens(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

#[test]
fn markers_are_exactly_one_of_the_two_reader_tokens() {
    let root = Root::new();
    let base = root.0.join("base");
    mkdir(&base);
    assert_eq!(boundary::marker(&base).unwrap(), None);
    let path = base.join("turn-sources-compat-v1.json");
    for token in [boundary::HISTORICAL_CAPABILITY, boundary::CAPABILITY] {
        write(&path, marker_bytes(token).as_bytes());
        assert_eq!(boundary::marker(&base).unwrap(), Some(token));
    }
    // Key order and whitespace are the reader's, not a re-encoding demand.
    write(
        &path,
        b"{\"schema\": \"agent-memory/turn-sources-compat-v1\", \"minimum_reader\": \"omp-stored-source-v2\", \"harnesses\": [\"omp\"]}",
    );
    assert_eq!(boundary::marker(&base).unwrap(), Some(boundary::CAPABILITY));
    for malformed in [
        "{\"harnesses\":[\"omp\"],\"minimum_reader\":\"omp-stored-source-v3\",\"schema\":\"agent-memory/turn-sources-compat-v1\"}",
        "{\"harnesses\":[\"omp\",\"codex\"],\"minimum_reader\":\"omp-stored-source-v2\",\"schema\":\"agent-memory/turn-sources-compat-v1\"}",
        "{\"extra\":1,\"harnesses\":[\"omp\"],\"minimum_reader\":\"omp-stored-source-v2\",\"schema\":\"agent-memory/turn-sources-compat-v1\"}",
        "[]",
    ] {
        write(&path, malformed.as_bytes());
        refusal(
            boundary::marker(&base),
            "Unknown or malformed source compatibility marker",
        );
    }
}

#[test]
fn bank_bindings_must_name_their_exact_protected_namespace() {
    let root = Root::new();
    let fixture = bank(&root.0);
    mkdir(&fixture.base);
    let path = fixture.base.join("omp-bank-binding-v1.json");
    refusal(
        boundary::bank_binding(&fixture.base),
        "Unknown native bank binding",
    );
    write(&path, binding_bytes(&fixture).as_bytes());
    let value = boundary::bank_binding(&fixture.base).unwrap();
    assert_eq!(value, decode(&binding_bytes(&fixture)));
    let original = binding_bytes(&fixture);
    let cases = [
        (
            original.replace("omp-bank-binding-v1", "omp-bank-binding-v2"),
            "Unknown native bank binding",
        ),
        (
            original.replace(&format!("\"{PROJECT}\""), "7"),
            "Unknown native bank binding",
        ),
        (
            original.replace(PROJECT, &PROJECT.to_uppercase()),
            "Native bank binding differs from its protected namespace",
        ),
        (
            original.replace(
                &format!("\"repository\":\"{}\"", fixture.repo.display()),
                &format!("\"repository\":\"{}\"", root.0.join("other").display()),
            ),
            "Native bank binding differs from its protected namespace",
        ),
        (
            original.replace(
                &format!("\"repository\":\"{}\"", fixture.repo.display()),
                "\"repository\":\"relative/repo\"",
            ),
            "Native bank binding has a redirected location",
        ),
        (
            original.replace(
                &format!("\"repository\":\"{}\"", fixture.repo.display()),
                &format!("\"repository\":\"{}/../repo\"", fixture.repo.display()),
            ),
            "Native bank binding has a redirected location",
        ),
    ];
    for (bytes, message) in cases {
        write(&path, bytes.as_bytes());
        refusal(boundary::bank_binding(&fixture.base), message);
    }
    // A symlinked repository location is a redirection.
    std::os::unix::fs::symlink(&fixture.repo, root.0.join("alias")).unwrap();
    write(
        &path,
        original
            .replace(
                &format!("\"repository\":\"{}\"", fixture.repo.display()),
                &format!("\"repository\":\"{}\"", root.0.join("alias").display()),
            )
            .as_bytes(),
    );
    refusal(
        boundary::bank_binding(&fixture.base),
        "Native bank binding has a redirected location",
    );
}

#[test]
fn registry_is_empty_when_absent_and_refuses_unverifiable_shapes() {
    isolated(
        "registry_is_empty_when_absent_and_refuses_unverifiable_shapes",
        |root| {
            assert_eq!(
                boundary::registry().unwrap(),
                decode(
                    "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[],\"private_bases\":{}}"
                )
            );
            mkdir(&root.join("config/agent-memory"));
            let path = registry_path(root);
            for malformed in [
                "{\"schema\":\"agent-memory/omp-bank-registry-v2\",\"repositories\":[],\"private_bases\":{}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/b\",\"/a\"],\"private_bases\":{\"/a\":\"/s/a\",\"/b\":\"/s/b\"}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/a\",\"/a\"],\"private_bases\":{\"/a\":\"/s/a\"}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"a\"],\"private_bases\":{\"a\":\"/s/a\"}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/a/../b\"],\"private_bases\":{\"/a/../b\":\"/s/a\"}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/a\"],\"private_bases\":{}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/a\"],\"private_bases\":{\"/a\":\"s/a\"}}",
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/a\"],\"private_bases\":{\"/a\":\"/s/a\"},\"extra\":true}",
            ] {
                write(&path, malformed.as_bytes());
                refusal(
                    boundary::registry(),
                    "Unverifiable omp bank registry; preserve private state",
                );
            }
            let repositories: Vec<String> = (0..1025).map(|i| format!("\"/r{i:04}\"")).collect();
            let bases: Vec<String> = (0..1025).map(|i| format!("\"/r{i:04}\":\"/s\"")).collect();
            write(
                &path,
                format!(
                    "{{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[{}],\"private_bases\":{{{}}}}}",
                    repositories.join(","),
                    bases.join(",")
                )
                .as_bytes(),
            );
            refusal(
                boundary::registry(),
                "Unverifiable omp bank registry; preserve private state",
            );
        },
    );
}

#[test]
fn record_bank_registers_once_and_refuses_a_moved_private_base() {
    isolated(
        "record_bank_registers_once_and_refuses_a_moved_private_base",
        |root| {
            let fixture = bank(root);
            assert_eq!(boundary::base(&fixture.repo).unwrap(), fixture.base);
            boundary::record_bank(&fixture.repo).unwrap();
            let path = registry_path(root);
            assert_eq!(text(&path), registry_bytes(&fixture));
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            let identity = installation::fingerprint(&path).unwrap();
            boundary::record_bank(&fixture.repo).unwrap();
            assert_eq!(installation::fingerprint(&path).unwrap(), identity);
            write(
                &path,
                registry_bytes(&fixture)
                    .replace(fixture.base.to_str().unwrap(), "/moved/base")
                    .as_bytes(),
            );
            refusal(
                boundary::record_bank(&fixture.repo),
                "Native protected private-state location changed; preserve its original binding",
            );
        },
    );
}

// obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_protected_private_root_cannot_silently_move_or_lose_binding
#[test]
fn protected_private_root_cannot_silently_move_or_lose_binding() {
    isolated(
        "protected_private_root_cannot_silently_move_or_lose_binding",
        |root| {
            let fixture = enrolled(root);
            let runtime = root.join("runtime");
            let repositories = boundary::source_capability_repositories(None).unwrap();
            assert!(repositories.contains(&fixture.repo), "{repositories:?}");
            assert_eq!(
                boundary::required_source_capabilities(&fixture.repo, &runtime).unwrap(),
                tokens(&[boundary::HISTORICAL_CAPABILITY, boundary::CAPABILITY])
            );
            std::fs::remove_file(fixture.base.join("omp-bank-binding-v1.json")).unwrap();
            refusal(
                boundary::required_source_capabilities(&fixture.repo, &runtime),
                "Saved native bank lacks its protected private-state binding",
            );
            refusal(
                boundary::source_capability_repositories(None),
                "Native marker/source inventory lacks its bound repository",
            );
            assert!(fixture.base.join("turn-sources-compat-v1.json").exists());
        },
    );
}

/// The discovery half of the protected-root contract needs no bank path.
#[test]
fn saved_native_binding_discovery_refuses_a_lost_binding() {
    isolated(
        "saved_native_binding_discovery_refuses_a_lost_binding",
        |root| {
            let fixture = enrolled(root);
            let repositories = boundary::source_capability_repositories(None).unwrap();
            assert!(repositories.contains(&fixture.repo), "{repositories:?}");
            let registered = boundary::registry().unwrap();
            assert_eq!(
                boundary::source_state_bindings(None, &registered).unwrap(),
                BTreeSet::from([fixture.repo.clone()])
            );
            std::fs::remove_file(fixture.base.join("omp-bank-binding-v1.json")).unwrap();
            refusal(
                boundary::source_capability_repositories(None),
                "Native marker/source inventory lacks its bound repository",
            );
            std::fs::remove_file(fixture.base.join("turn-sources-compat-v1.json")).unwrap();
            refusal(
                boundary::source_capability_repositories(None),
                "Unknown native bank binding",
            );
            std::fs::remove_dir(&fixture.base).unwrap();
            refusal(
                boundary::source_capability_repositories(None),
                "Registered native source binding is unavailable",
            );
        },
    );
}

#[test]
fn required_capabilities_follow_marker_binding_and_stream_evidence() {
    isolated(
        "required_capabilities_follow_marker_binding_and_stream_evidence",
        |root| {
            let fixture = bank(root);
            let runtime = root.join("runtime");
            assert!(
                boundary::required_source_capabilities(&fixture.repo, &runtime)
                    .unwrap()
                    .is_empty()
            );
            mkdir(&fixture.base);
            let marker = fixture.base.join("turn-sources-compat-v1.json");
            write(
                &marker,
                marker_bytes(boundary::HISTORICAL_CAPABILITY).as_bytes(),
            );
            refusal(
                boundary::required_source_capabilities(&fixture.repo, &runtime),
                "Native compatibility marker lacks its protected bank binding",
            );
            write(
                &fixture.base.join("omp-bank-binding-v1.json"),
                binding_bytes(&fixture).as_bytes(),
            );
            assert_eq!(
                boundary::required_source_capabilities(&fixture.repo, &runtime).unwrap(),
                tokens(&[boundary::HISTORICAL_CAPABILITY])
            );
            mkdir(&fixture.base.join("omp-stream-v1"));
            refusal(
                boundary::required_source_capabilities(&fixture.repo, &runtime),
                "Native stream evidence requires the stored-source reader marker",
            );
            write(&marker, marker_bytes(boundary::CAPABILITY).as_bytes());
            assert_eq!(
                boundary::required_source_capabilities(&fixture.repo, &runtime).unwrap(),
                tokens(&[boundary::HISTORICAL_CAPABILITY, boundary::CAPABILITY])
            );
            // A binding for another bank of the same namespace is foreign.
            mkdir(&root.join("config/agent-memory"));
            write(
                &registry_path(root),
                registry_bytes(&fixture)
                    .replace(fixture.base.to_str().unwrap(), "/other/base")
                    .as_bytes(),
            );
            refusal(
                boundary::required_source_capabilities(&fixture.repo, &runtime),
                "Native protected private-state location differs from its saved binding",
            );
        },
    );
}

#[test]
fn orphan_native_state_without_a_binding_refuses_discovery() {
    isolated(
        "orphan_native_state_without_a_binding_refuses_discovery",
        |root| {
            let empty = decode(
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[],\"private_bases\":{}}",
            );
            assert!(
                boundary::source_state_bindings(None, &empty)
                    .unwrap()
                    .is_empty()
            );
            let writes = root.join("state/agent-memory/writes");
            let orphan = writes.join(sha256(b"orphan"));
            mkdir(&orphan.join("omp-source-v1"));
            refusal(
                boundary::source_state_bindings(None, &empty),
                "Native marker/source inventory lacks its bound repository",
            );
            std::fs::remove_dir(orphan.join("omp-source-v1")).unwrap();
            let pending = orphan.join("turn-sources-v2/pending");
            mkdir(&pending);
            write(
                &pending.join("0.json"),
                b"{\"harness\":\"codex\",\"schema\":\"agent-memory/turn-reference-v2\"}",
            );
            assert!(
                boundary::source_state_bindings(None, &empty)
                    .unwrap()
                    .is_empty()
            );
            write(
                &pending.join("1.json"),
                b"{\"harness\":\"omp\",\"schema\":\"agent-memory/turn-reference-v2\"}",
            );
            refusal(
                boundary::source_state_bindings(None, &empty),
                "Orphan indexed native source lacks its bound repository",
            );
            write(&pending.join("1.json"), b"{\"schema\":\"other\"}");
            refusal(
                boundary::source_state_bindings(None, &empty),
                "Private pending source inventory is unverifiable",
            );
            std::fs::remove_dir_all(&orphan).unwrap();
            mkdir(&writes.join("not-a-namespace"));
            refusal(
                boundary::source_state_bindings(None, &empty),
                "Unknown private bank namespace",
            );
            std::fs::remove_dir(writes.join("not-a-namespace")).unwrap();
            let saved = decode(
                "{\"schema\":\"agent-memory/omp-bank-registry-v1\",\"repositories\":[\"/gone/repo\"],\"private_bases\":{\"/gone/repo\":\"/gone/state/writes/base\"}}",
            );
            refusal(
                boundary::source_state_bindings(None, &saved),
                "A saved native private-state root is unavailable",
            );
        },
    );
}

#[test]
fn stream_startup_outside_a_repository_is_absent_or_pending() {
    isolated(
        "stream_startup_outside_a_repository_is_absent_or_pending",
        |root| {
            let runtime = root.join("runtime");
            let receipt = Value::Object(vec![]);
            let plain = root.join("plain");
            mkdir(&plain);
            assert_eq!(
                boundary::stream_startup(&plain, "s", PROJECT, "l", &receipt, &runtime).unwrap(),
                None
            );
            let broken = root.join("broken");
            mkdir(&broken.join("nested"));
            write(&broken.join(".git"), b"not a gitdir pointer\n");
            refusal(
                boundary::stream_startup(
                    &broken.join("nested"),
                    "s",
                    PROJECT,
                    "l",
                    &receipt,
                    &runtime,
                ),
                "Native repository is unavailable; preserve pending sources",
            );
        },
    );
}

fn state_directory(root: &Path) -> PathBuf {
    let state = root.join("state/agent-memory/install");
    mkdir(&state);
    state
}

#[test]
fn finished_native_transition_journal_becomes_completed_once() {
    isolated(
        "finished_native_transition_journal_becomes_completed_once",
        |root| {
            let state = state_directory(root);
            let journal = state.join("omp-transition.json");
            let plan = |backup: bool| {
                let mut fields = vec![(
                    "state".to_owned(),
                    Value::String(state.to_str().unwrap().to_owned()),
                )];
                if backup {
                    fields.push((
                        "omp_disabled_backup".to_owned(),
                        decode("{\"kind\":\"native-binding-disabled\"}"),
                    ));
                }
                Value::Object(fields)
            };
            boundary::finish_native_transition(&plan(true)).unwrap();
            assert!(!journal.exists());
            write(
                &journal,
                b"{\"schema\": \"agent-memory/omp-native-transition-v1\", \"status\": \"disabled\"}",
            );
            boundary::finish_native_transition(&plan(false)).unwrap();
            assert!(text(&journal).contains("\"disabled\""));
            boundary::finish_native_transition(&plan(true)).unwrap();
            let completed = "{\n  \"schema\": \"agent-memory/omp-native-transition-v1\",\n  \"status\": \"completed\"\n}\n";
            assert_eq!(text(&journal), completed);
            let identity = installation::fingerprint(&journal).unwrap();
            boundary::finish_native_transition(&plan(true)).unwrap();
            assert_eq!(installation::fingerprint(&journal).unwrap(), identity);
            write(
                &journal,
                b"{\"schema\": \"agent-memory/omp-native-transition-v1\", \"status\": \"prepared\"}",
            );
            install_refusal(
                boundary::finish_native_transition(&plan(true)),
                "Native transition state changed before lifecycle completion",
            );
        },
    );
}

fn transition_journal(
    source: &str,
    extension: &Path,
    identity: &Value,
    backup: &Path,
    status: &str,
) -> String {
    format!(
        "{{\"schema\": \"agent-memory/omp-native-transition-v1\", \"installation_id\": \"{ID}\", \"source\": {source}, \"extension\": \"{}\", \"identity\": {}, \"disabled_path\": \"{}\", \"status\": \"{status}\"}}",
        extension.display(),
        json::encode(identity, json::DEFAULT).unwrap(),
        backup.display()
    )
}

#[test]
fn native_recovery_validates_its_journal_and_completes_a_committed_uninstall() {
    isolated(
        "native_recovery_validates_its_journal_and_completes_a_committed_uninstall",
        |root| {
            let state = state_directory(root);
            let journal = state.join("omp-transition.json");
            boundary::recover_native_binding(&state).unwrap();
            assert!(!journal.exists());
            assert_eq!(
                boundary::native_recovery_expected(&state, "/any/extension.ts").unwrap(),
                None
            );
            write(
                &journal,
                b"{\"schema\": \"agent-memory/omp-native-transition-v1\", \"status\": \"disabled\"}",
            );
            install_refusal(
                boundary::recover_native_binding(&state),
                "Unverifiable native lifecycle recovery journal",
            );
            let agent = root.join("agent");
            mkdir(&agent.join("extensions"));
            let binding = omp_installation::binding(
                agent.to_str().unwrap(),
                false,
                omp_installation::READER_CAPABILITY,
            )
            .unwrap();
            let target = agent.join("extensions/agent-memory.ts");
            let backup = agent.join(format!(
                "extensions/.agent-memory-disabled-{ID}-{}.ts.disabled",
                host::uuid4().unwrap()
            ));
            write(&backup, b"export default function () {}\n");
            let identity = installation::fingerprint(&backup).unwrap();
            let source = "{\"kind\": \"release\", \"artifact_digest\": \"aa\"}";
            write(
                &journal,
                transition_journal(source, &target, &identity, &backup, "completed").as_bytes(),
            );
            let completed = text(&journal);
            boundary::recover_native_binding(&state).unwrap();
            assert_eq!(text(&journal), completed);
            write(
                &journal,
                transition_journal(source, &target, &identity, &backup, "disabled").as_bytes(),
            );
            // No committed receipt at all: never guess the executable.
            assert!(boundary::recover_native_binding(&state).is_err());
            let receipt = format!(
                "{{\"schema\": \"agent-memory/install-receipt-v3\", \"installation_id\": \"{ID}\", \"verified\": true, \"uninstalled\": true, \"source\": {source}, \"targets\": [], \"omp_binding\": {}}}",
                json::encode(&binding, json::DEFAULT).unwrap()
            );
            write(&state.join("receipt.json"), receipt.as_bytes());
            write(&target, b"foreign\n");
            install_refusal(
                boundary::recover_native_binding(&state),
                "Committed native uninstall differs from its retained disabled executable",
            );
            std::fs::remove_file(&target).unwrap();
            boundary::recover_native_binding(&state).unwrap();
            let recovered = installation::load(&journal).unwrap();
            assert_eq!(
                recovered.get("status"),
                Some(&Value::String("completed".into()))
            );
            assert!(!target.exists());
            assert!(backup.exists());
            assert_eq!(
                boundary::native_recovery_expected(&state, target.to_str().unwrap()).unwrap(),
                None
            );
            let foreign = format!(
                "{{\"schema\": \"agent-memory/install-receipt-v3\", \"installation_id\": \"{ID}\", \"verified\": true, \"uninstalled\": true, \"source\": {{\"kind\": \"release\"}}, \"targets\": [], \"omp_binding\": {}}}",
                json::encode(&binding, json::DEFAULT).unwrap()
            );
            write(&state.join("receipt.json"), foreign.as_bytes());
            write(
                &journal,
                transition_journal(source, &target, &identity, &backup, "disabled").as_bytes(),
            );
            install_refusal(
                boundary::recover_native_binding(&state),
                "Native uninstall recovery has no exact committed receipt",
            );
        },
    );
}

/// A saved native receipt with no enrolled banks: the transition disables only
/// the owned extension, journals it and rewrites the plan as the baseline.
#[test]
fn native_transition_disables_only_the_owned_extension_and_rewrites_the_plan() {
    isolated(
        "native_transition_disables_only_the_owned_extension_and_rewrites_the_plan",
        |root| {
            let runtime = root.join("runtime");
            let state = state_directory(root);
            let agent = root.join("agent");
            mkdir(&agent.join("extensions"));
            mkdir(&agent.join("skills"));
            let binding = omp_installation::binding(
                agent.to_str().unwrap(),
                true,
                omp_installation::READER_CAPABILITY,
            )
            .unwrap();
            let target = agent.join("extensions/agent-memory.ts");
            let skill = agent.join("skills/agent-memory");
            write(&target, b"export default function () {}\n");
            let identity = installation::fingerprint(&target).unwrap();
            let identity_text = json::encode(&identity, json::DEFAULT).unwrap();
            let old = format!(
                "{{\"schema\": \"agent-memory/install-receipt-v3\", \"installation_id\": \"{ID}\", \"verified\": true, \"presets\": \"omp\", \"enabled_adapters\": [\"omp\"], \"no_guidance\": true, \"source\": {{\"kind\": \"release\", \"target\": \"aarch64-apple-darwin\", \"artifact_digest\": \"d\"}}, \"omp_binding\": {}, \"targets\": [{{\"path\": \"{}\", \"kind\": \"wrapper\", \"identity\": {identity_text}, \"artifact_digest\": \"d\"}}, {{\"path\": \"{}\", \"kind\": \"copy\", \"identity\": null, \"artifact_digest\": \"d\"}}]}}",
                json::encode(&binding, json::DEFAULT).unwrap(),
                target.display(),
                skill.display()
            );
            let plan_text = |actions: &str| {
                format!(
                    "{{\"state\": \"{}\", \"old\": {old}, \"omp_transition\": {{\"writers_quiesced\": true}}, \"actions\": {actions}}}",
                    state.display()
                )
            };
            let actions = format!(
                "[{{\"path\": \"{}\", \"kind\": \"wrapper\", \"before\": {identity_text}, \"after\": {{\"kind\": \"file\"}}, \"changed\": false}}, {{\"path\": \"{}\", \"kind\": \"copy\", \"before\": null, \"after\": null, \"changed\": false}}]",
                target.display(),
                skill.display()
            );
            let mut plan = decode(&plan_text(&actions));
            let planned = boundary::native_transition(&mut plan, true, &runtime).unwrap();
            assert_eq!(
                planned,
                decode(
                    "{\"binding_disable_planned\": true, \"drains\": [], \"installation_writes_performed\": false, \"canonical_writes_performed\": false}"
                )
            );
            assert_eq!(plan, decode(&plan_text(&actions)));
            assert!(!state.join("omp-transition.json").exists());
            let result = boundary::native_transition(&mut plan, false, &runtime).unwrap();
            assert_eq!(
                result,
                decode(
                    "{\"binding_disabled\": true, \"drains\": [], \"canonical_writes_performed\": false}"
                )
            );
            assert!(!target.exists());
            let backup = PathBuf::from(
                plan.get("omp_disabled_backup")
                    .and_then(|backup| backup.get("path"))
                    .and_then(Value::as_str)
                    .unwrap(),
            );
            assert_eq!(installation::fingerprint(&backup).unwrap(), identity);
            assert_eq!(
                plan.get("omp_disabled_backup").unwrap(),
                &decode(&format!(
                    "{{\"path\": \"{}\", \"original_path\": \"{}\", \"identity\": {identity_text}, \"available\": true, \"kind\": \"native-binding-disabled\"}}",
                    backup.display(),
                    target.display()
                ))
            );
            assert_eq!(
                plan.get("actions").unwrap(),
                &decode(&format!(
                    "[{{\"path\": \"{}\", \"kind\": \"wrapper\", \"before\": null, \"after\": {{\"kind\": \"file\"}}, \"changed\": true}}, {{\"path\": \"{}\", \"kind\": \"copy\", \"before\": null, \"after\": null, \"changed\": false}}]",
                    target.display(),
                    skill.display()
                ))
            );
            let journal = installation::load(&state.join("omp-transition.json")).unwrap();
            assert!(installation::same(
                &journal,
                &decode(&transition_journal(
                    "{\"kind\": \"release\", \"target\": \"aarch64-apple-darwin\", \"artifact_digest\": \"d\"}",
                    &target,
                    &identity,
                    &backup,
                    "disabled"
                ))
            ));
            // A retried transition reuses the journaled backup and drops an
            // action without a replacement.
            let removed = format!(
                "[{{\"path\": \"{}\", \"kind\": \"wrapper\", \"before\": {identity_text}, \"after\": null, \"changed\": true}}]",
                target.display()
            );
            let mut retry = decode(&plan_text(&removed));
            boundary::native_transition(&mut retry, false, &runtime).unwrap();
            assert_eq!(retry.get("actions").unwrap(), &decode("[]"));
            assert_eq!(
                retry.get("omp_disabled_backup"),
                plan.get("omp_disabled_backup")
            );
            boundary::finish_native_transition(&retry).unwrap();
            let completed = installation::load(&state.join("omp-transition.json")).unwrap();
            assert_eq!(
                completed.get("status"),
                Some(&Value::String("completed".into()))
            );
            // A modified owned extension is never disabled.
            std::fs::remove_file(state.join("omp-transition.json")).unwrap();
            write(&target, b"modified\n");
            let mut modified = decode(&plan_text(&actions));
            install_refusal(
                boundary::native_transition(&mut modified, false, &runtime),
                "Native binding is modified/unowned; preserve it and refuse compatibility drain",
            );
            assert_eq!(text(&target), "modified\n");
        },
    );
}

#[test]
fn native_lifecycle_entry_refusals_and_absent_work() {
    isolated("native_lifecycle_entry_refusals_and_absent_work", |root| {
        let runtime = root.join("runtime");
        let state = state_directory(root);
        let mut plan = decode("{}");
        install_refusal(
            boundary::native_transition(&mut plan, false, &runtime),
            "Native deactivation requires saved ownership and explicit writer quiescence",
        );
        let mut plan = decode(
            "{\"old\": {\"installation_id\": \"x\"}, \"omp_transition\": {\"writers_quiesced\": false}}",
        );
        install_refusal(
            boundary::native_transition(&mut plan, true, &runtime),
            "Native deactivation requires saved ownership and explicit writer quiescence",
        );
        let mut plan = decode("{\"omp_binding\": null}");
        assert_eq!(
            boundary::native_activation(&mut plan, true, &runtime).unwrap(),
            Value::Array(vec![])
        );
        let mut plan = decode(
            "{\"omp_binding\": {\"peer_reader_capability\": \"omp-harness-v1\"}, \"old\": null}",
        );
        refusal(
            boundary::native_activation(&mut plan, true, &runtime),
            "New native activation requires the stored-source reader capability",
        );
        let mut plan = decode(
            "{\"omp_binding\": {\"peer_reader_capability\": \"omp-stored-source-v2\"}, \"old\": {\"omp_binding\": {\"peer_reader_capability\": \"omp-harness-v1\"}}, \"omp_peers_upgraded\": false}",
        );
        refusal(
            boundary::native_activation(&mut plan, true, &runtime),
            "Native source activation requires renewed --omp-peers-upgraded",
        );
        let mut plan = decode(
            "{\"omp_binding\": {\"peer_reader_capability\": \"omp-stored-source-v2\"}, \"old\": {\"omp_binding\": {\"peer_reader_capability\": \"omp-stored-source-v2\"}, \"source\": {\"artifact_digest\": \"d\"}}, \"source\": {\"artifact_digest\": \"d\"}}",
        );
        let before = plan.clone();
        assert_eq!(
            boundary::native_activation(&mut plan, true, &runtime).unwrap(),
            Value::Array(vec![])
        );
        assert_eq!(plan, before);
        assert_eq!(
            boundary::complete_stored_source_activation(&state).unwrap(),
            decode("{\"status\": \"not-required\", \"verified\": true}")
        );
        refusal(
            boundary::drain_source_capabilities(Path::new("."), false, true, &runtime),
            "Source drain requires disabled bindings and writer quiescence",
        );
    });
}

#[test]
fn claude_assessments_and_selection_refuse_before_any_source_access() {
    let repo = Path::new("/nonexistent/claude/repo");
    let runtime = Path::new("/nonexistent/runtime");
    let valid = "{\"session\": \"s\", \"prompt\": \"p\", \"observation_id\": \"0123abcd-0123-4abc-8def-0123456789ab\", \"source_sha256\": \"0000000000000000000000000000000000000000000000000000000000000000\", \"outcome\": \"no-learning\", \"assessment\": {}}";
    for shape in [
        "[]".to_owned(),
        valid.replace("\"outcome\": \"no-learning\", ", ""),
        valid.replace("{\"session\"", "{\"extra\": 1, \"session\""),
    ] {
        refusal(
            claude_turns::complete(repo, &decode(&shape), runtime),
            "Use an exact native Claude observation assessment",
        );
    }
    for identifier in [
        "\"a b\"",
        "7",
        "\"0123abcd-0123-4abc-8def-0123456789ab extra\"",
    ] {
        refusal(
            claude_turns::complete(
                repo,
                &decode(&valid.replace("\"0123abcd-0123-4abc-8def-0123456789ab\"", identifier)),
                runtime,
            ),
            "observation_id must be one local observation UUID, not a shell word list",
        );
    }
    let zeros = "0".repeat(64);
    for digest in ["A".repeat(64), "0".repeat(63), format!("{zeros} ")] {
        refusal(
            claude_turns::complete(
                repo,
                &decode(&valid.replace(&format!("\"{zeros}\""), &format!("\"{digest}\""))),
                runtime,
            ),
            "source_sha256 must be the exact 64-character lowercase prompt digest",
        );
    }
    refusal(
        claude_turns::pending(repo, Some("s"), None, (0, 0, 0), runtime),
        "Select both native session and prompt, or neither",
    );
    refusal(
        claude_turns::pending(repo, None, Some("p"), (0, 0, 0), runtime),
        "Select both native session and prompt, or neither",
    );
    refusal(
        claude_turns::pending(repo, Some("s"), Some("p"), (0, 1, 0), runtime),
        "Page offsets select the full pending inventory, not a single native source",
    );
}

#[test]
fn boundary_command_refusals_are_the_fixed_pending_failure() {
    let repo = Path::new("/nonexistent/omp/repo");
    let request = |stdin, list_pending, source, quiet, refresh| boundary::Request {
        repo,
        stdin,
        list_pending,
        offset: 0,
        source,
        session: None,
        lifecycle: None,
        quiet,
        refresh,
    };
    let mut input: &[u8] = b"";
    for (stdin, list_pending, source) in [
        (false, false, None),
        (false, false, Some("")),
        (true, true, None),
        (false, true, Some("omp:seal:x")),
    ] {
        let result = boundary::command(
            &request(stdin, list_pending, source, false, false),
            &mut input,
        );
        assert_eq!(
            (
                result.exit_code,
                result.stdout.as_str(),
                result.stderr.as_str()
            ),
            (1, FAILURE, "")
        );
        let result = boundary::command(
            &request(stdin, list_pending, source, true, false),
            &mut input,
        );
        assert_eq!(
            (
                result.exit_code,
                result.stdout.as_str(),
                result.stderr.as_str()
            ),
            (1, "", QUIET_FAILURE)
        );
    }
    let result = boundary::command(&request(false, true, None, false, true), &mut input);
    assert_eq!((result.exit_code, result.stdout.as_str()), (1, FAILURE));
    let mut oversized: &[u8] = &[b' '; boundary::MAX_INPUT + 1];
    let result = boundary::command(&request(true, false, None, false, false), &mut oversized);
    assert_eq!((result.exit_code, result.stdout.as_str()), (1, FAILURE));
}

fn am(world: &World, args: &[&str]) -> process::Output {
    world
        .am_with(
            args,
            RunOptions {
                check: false,
                ..RunOptions::default()
            },
        )
        .unwrap()
}
fn output(result: &process::Output) -> (process::Status, String, String) {
    (
        result.status,
        String::from_utf8(result.stdout.clone()).unwrap(),
        String::from_utf8(result.stderr.clone()).unwrap(),
    )
}

#[test]
fn omp_boundary_route_reports_pending_work_and_argument_errors() {
    let world = World::new().unwrap();
    let repo = world.paths["repo"].to_str().unwrap().to_owned();
    let (status, stdout, stderr) = output(&am(&world, &["omp-boundary", "--repo", &repo]));
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (process::Status::Code(1), FAILURE, "")
    );
    let (status, stdout, stderr) = output(&am(
        &world,
        &["omp-boundary", "--repo", &repo, "--list-pending", "--quiet"],
    ));
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (process::Status::Code(1), "", QUIET_FAILURE)
    );
    let (status, stdout, _) = output(&am(
        &world,
        &[
            "omp-boundary",
            "--repo",
            &repo,
            "--list-pending",
            "--refresh",
        ],
    ));
    assert_eq!(
        (status, stdout.as_str()),
        (process::Status::Code(1), FAILURE)
    );
    let (status, stdout, stderr) = output(&am(
        &world,
        &[
            "omp-boundary",
            "--repo",
            &repo,
            "--list-pending",
            "--offset",
            "x",
        ],
    ));
    assert_eq!((status, stdout.as_str()), (process::Status::Code(2), ""));
    assert!(
        stderr.ends_with("am omp-boundary: error: argument --offset: invalid int value: 'x'\n"),
        "{stderr}"
    );
    let (status, _, stderr) = output(&am(
        &world,
        &[
            "omp-boundary",
            "--repo",
            &repo,
            "--list-pending",
            "--instance",
            "i",
        ],
    ));
    assert_eq!(status, process::Status::Code(2));
    assert!(
        stderr.ends_with("am omp-boundary: error: unrecognized arguments: --instance i\n"),
        "{stderr}"
    );
    let (status, _, stderr) = output(&am(&world, &["omp-boundary", "--list-pending"]));
    assert_eq!(status, process::Status::Code(2));
    assert!(
        stderr.ends_with("am omp-boundary: error: the following arguments are required: --repo\n"),
        "{stderr}"
    );
    let (status, stdout, stderr) = output(&am(
        &world,
        &[
            "omp-boundary",
            "--stream",
            "--repo",
            &repo,
            "--session",
            "s",
            "--instance",
            PROJECT,
            "--lifecycle",
            "l",
            "--artifact",
            "a",
            "--omp-agent-dir",
            "/agent",
        ],
    ));
    // `--stream` reaches the real finite stream: its observer binding uses the
    // registered OMP activation composite, whose installed runtime locations
    // refuse before any transport or source work when no receipt owns `am`.
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (
            process::Status::Code(1),
            "",
            "am omp-boundary: No installed Rust runtime receipt owns this executable; pass --pack PATH\n"
        )
    );
    // The stream parser, not the boundary parser, decides `--stream` argv.
    let (status, stdout, stderr) = output(&am(
        &world,
        &[
            "omp-boundary",
            "--repo",
            &repo,
            "--list-pending",
            "--stream",
        ],
    ));
    assert_eq!((status, stdout.as_str()), (process::Status::Code(2), ""));
    assert!(
        stderr.ends_with(
            "am omp-boundary: error: the following arguments are required: --session, --instance, --lifecycle, --artifact, --omp-agent-dir/--agent-dir\n"
        ),
        "{stderr}"
    );
    world.close().unwrap();
}

#[test]
fn missing_transport_and_stream_registrations_are_typed_refusals() {
    let missing = |error: ContractError| {
        assert_eq!(error.kind(), &agent_memory::error::Kind::MissingProvider);
    };
    missing(
        boundary::drain_inventory(Path::new("/nonexistent"), Path::new("/nonexistent"))
            .unwrap_err(),
    );
    let result = boundary::stream(&[], "am omp-boundary");
    assert_eq!(result.exit_code, 1);
    assert_eq!(
        result.stderr,
        "am omp-boundary: omp_stream provider is not registered; native source work remains pending\n"
    );
}
