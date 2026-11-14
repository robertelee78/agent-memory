//! Journaled metadata-probe interruptions at the layout_recovery_temps
//! boundary, with real fault injection in an owned child process.
//!
//! The baseline contracts drive these interruptions through `am bank
//! recover-layout apply/recover`, which needs layout_recovery and
//! recovery_frontier. Each test here ports the probe-ownership half of its
//! contract (journal, pins, cleanup, ambiguity and completion) on the
//! reviewed bank fixture shape; the public CLI half remains pending, so the
//! obligations are recorded as partial.
use agent_memory::codec::json::Value;
use agent_memory::developer_identity::{encoded, load};
use agent_memory::error::Kind;
use agent_memory::layout_recovery_temps as temps;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const OPERATION: &str = "33333333-3333-4333-8333-333333333333";
const OLD: &str = "topics/old.md";
const NEW: &str = "topics/probe-nested/old.md";
const PREFIX: &str = ".agent-memory-probe-";
const CASE: &str = "AM_LAYOUTCORE_PROBE_CASE";
const ROOT: &str = "AM_LAYOUTCORE_PROBE_ROOT";
static CHILD: Mutex<()> = Mutex::new(());

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn n(value: i64) -> Value {
    Value::Int(value.into())
}
fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap()
}

/// A disposable fault world: canonical `/tmp/agent-memory-test-*` with its token.
struct World {
    root: PathBuf,
    token: String,
}
impl World {
    fn new() -> Self {
        let tmp = fs::canonicalize("/tmp").unwrap();
        let token = agent_memory::host::uuid4().unwrap().replace('-', "");
        let root = tmp.join(format!("agent-memory-test-layoutcore-{token}"));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.join(".test-world.json"),
            encoded(&object([("token", s(token.as_str()))])).unwrap(),
        )
        .unwrap();
        let world = Self { root, token };
        for name in ["bank", "operation"] {
            fs::create_dir(world.root.join(name)).unwrap();
            fs::set_permissions(world.root.join(name), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let bank = world.bank();
        fs::create_dir(bank.join("topics")).unwrap();
        fs::set_permissions(bank.join("topics"), fs::Permissions::from_mode(0o755)).unwrap();
        write(&bank.join("MEMORY.md"), b"# Synthetic router\n", 0o644);
        write(&bank.join(OLD), b"# Synthetic record\n", 0o640);
        world
    }
    fn bank(&self) -> PathBuf {
        self.root.join("bank")
    }
    fn directory(&self) -> PathBuf {
        self.root.join("operation")
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn write(path: &Path, body: &[u8], mode: u32) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn facts(path: &Path) -> Value {
    let info = fs::symlink_metadata(path).unwrap();
    object([
        ("uid", n(i64::from(info.uid()))),
        ("gid", n(i64::from(info.gid()))),
        ("mode", n(i64::from(info.mode() & 0o7777))),
        (
            "xattrs",
            agent_memory::layout_recovery_observation::metadata_supported(path).unwrap(),
        ),
    ])
}

/// The reviewed probe plan of the baseline fixture: the router, the original
/// record, the record moved into a new nested directory, and that directory.
fn plan(bank: &Path) -> Value {
    let requirements = [
        object([
            ("path", s("MEMORY.md")),
            ("facts", facts(&bank.join("MEMORY.md"))),
        ]),
        object([("path", s(OLD)), ("facts", facts(&bank.join(OLD)))]),
        object([("path", s(NEW)), ("facts", facts(&bank.join(OLD)))]),
    ];
    let directories = object([("topics/probe-nested", facts(&bank.join("topics")))]);
    temps::probe_plan(OPERATION, &requirements, &directories).unwrap()
}

fn depth(path: &str) -> usize {
    Path::new(path).components().count()
}

/// Every plan node, reachable flag, in the baseline template order.
fn templates(bank: &Path, plan: &Value) -> Vec<(Value, bool)> {
    let mut nodes: BTreeMap<String, Value> = BTreeMap::new();
    let mut reachable = BTreeSet::new();
    for task in get(plan, "tasks").as_array().unwrap() {
        for route in get(task, "routes").as_array().unwrap() {
            let route = route.as_array().unwrap();
            assert!(!route.is_empty());
            let first = text(&route[0], "path");
            let route_reachable = bank.join(first).parent().unwrap().is_dir();
            for node in route {
                let path = text(node, "path").to_owned();
                assert!(!path.starts_with('/') && !path.split('/').any(|part| part == ".."));
                if let Some(previous) = nodes.get(&path) {
                    assert_eq!(previous, node);
                }
                nodes.insert(path.clone(), node.clone());
                if route_reachable {
                    reachable.insert(path);
                }
            }
        }
    }
    let mut ordered: Vec<(String, Value)> = nodes.into_iter().collect();
    ordered.sort_by(|a, b| (depth(&a.0), &a.0).cmp(&(depth(&b.0), &b.0)));
    ordered
        .into_iter()
        .map(|(path, node)| (node, reachable.contains(&path)))
        .collect()
}

fn label(node: &Value) -> (String, String, bool) {
    let target = text(node, "target");
    let role = match target {
        OLD => "original-record",
        NEW => "moved-record",
        other => other,
    };
    let path = text(node, "path");
    let parents: Vec<&str> = path.split('/').collect();
    let shadow = parents
        .iter()
        .take(parents.len() - 1)
        .any(|part| part.starts_with(PREFIX));
    (text(node, "type").to_owned(), role.to_owned(), shadow)
}

/// Reachable candidates sorted by label, each with its template index.
fn candidates(bank: &Path, plan: &Value) -> Vec<((String, String, bool), usize)> {
    let mut rows: Vec<_> = templates(bank, plan)
        .into_iter()
        .enumerate()
        .filter(|(_, (_, reachable))| *reachable)
        .map(|(index, (node, _))| (label(&node), index))
        .collect();
    let labels: BTreeSet<_> = rows.iter().map(|(label, _)| label.clone()).collect();
    assert_eq!(labels.len(), rows.len());
    rows.sort();
    rows
}

fn names(directory: &Path) -> BTreeSet<String> {
    match fs::read_dir(directory) {
        Ok(entries) => entries
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect(),
        Err(_) => BTreeSet::new(),
    }
}
fn snapshot(bank: &Path) -> BTreeMap<String, (Vec<u8>, u32)> {
    fn visit(root: &Path, path: &Path, found: &mut BTreeMap<String, (Vec<u8>, u32)>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let info = fs::symlink_metadata(&path).unwrap();
            let key = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if info.is_dir() {
                found.insert(key, (Vec::new(), info.mode()));
                visit(root, &path, found);
            } else {
                found.insert(key, (fs::read(&path).unwrap(), info.mode()));
            }
        }
    }
    let mut found = BTreeMap::new();
    visit(bank, bank, &mut found);
    found
}

/// Run the probe in an owned child with an injected fault; returns its exit code.
fn faulted(world: &World, case: &str, stage: &str, mode: &str) -> Option<i32> {
    let argv: Vec<OsString> = vec![
        std::env::current_exe().unwrap().into(),
        "--exact".into(),
        case.into(),
        "--include-ignored".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    let env: Vec<(OsString, OsString)> = [
        ("PATH", OsString::from("/usr/bin:/bin")),
        ("LC_ALL", "C".into()),
        (CASE, case.into()),
        (ROOT, world.root.clone().into()),
        ("AGENT_MEMORY_TEST_ROOT", world.root.clone().into()),
        ("AGENT_MEMORY_TEST_TOKEN", world.token.clone().into()),
        ("AGENT_MEMORY_FAULT_STAGE", stage.into()),
        ("AGENT_MEMORY_FAULT_MODE", mode.into()),
    ]
    .into_iter()
    .map(|(key, value)| (OsString::from(key), value))
    .collect();
    let spec = agent_memory::process::Spec {
        argv: &argv,
        env: &env,
        cwd: None,
        search_path: false,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        stderr: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        deadline: agent_memory::clock::Deadline::from_absolute_ns(u64::MAX),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    let output = agent_memory::process::run_for(&spec, Duration::from_secs(120)).unwrap();
    match output.status {
        agent_memory::process::Status::Code(code) => Some(code),
        _ => None,
    }
}

/// Child side: probe once; an injected error is recorded for the parent.
fn child(case: &str) -> bool {
    if std::env::var(CASE).ok().as_deref() != Some(case) {
        return false;
    }
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let bank = root.join("bank");
    let directory = root.join("operation");
    let plan = plan(&bank);
    let outcome = temps::probe(&bank, &plan, &directory, None, None, None, None);
    let recorded = match outcome {
        Ok(()) => "ok".to_owned(),
        Err(error) => format!("error:{error}"),
    };
    fs::write(root.join("child-outcome"), recorded).unwrap();
    true
}

fn outcome(world: &World) -> String {
    fs::read_to_string(world.root.join("child-outcome")).unwrap_or_default()
}

fn attempts(world: &World) -> Vec<PathBuf> {
    names(&world.directory().join("metadata-probe-attempts"))
        .into_iter()
        .map(|name| world.directory().join("metadata-probe-attempts").join(name))
        .collect()
}

fn fresh_attempt(world: &World) -> (PathBuf, Value) {
    let found = attempts(world);
    assert_eq!(found.len(), 1);
    let attempt = found[0].clone();
    let value = load(&fs::read(attempt.join("plan.json")).unwrap()).unwrap();
    (attempt, value)
}

fn node_at(attempt: &Value, index: usize) -> Value {
    get(attempt, "nodes")
        .as_array()
        .unwrap()
        .iter()
        .find(|node| get(node, "index") == &n(index as i64))
        .unwrap()
        .clone()
}

/// Checked continuation: clean exactly-pinned probes, complete, probe again.
fn finish(world: &World, original: &BTreeMap<String, (Vec<u8>, u32)>) {
    let bank = world.bank();
    let directory = world.directory();
    let plan = plan(&bank);
    let pins = temps::probe_entries(&bank, &plan, &directory).unwrap();
    if !pins.is_empty() {
        assert_eq!(
            temps::probe(&bank, &plan, &directory, None, None, None, None)
                .unwrap_err()
                .to_string(),
            "Interrupted metadata probes require checked cleanup"
        );
        temps::probe_clean(&bank, &pins, None, None).unwrap();
    }
    temps::probe_complete(&bank, &plan, &directory).unwrap();
    temps::probe(&bank, &plan, &directory, None, None, None, None).unwrap();
    assert!(
        temps::probe_entries(&bank, &plan, &directory)
            .unwrap()
            .is_empty()
    );
    assert_eq!(&snapshot(&bank), original);
    for attempt in attempts(world) {
        assert!(attempt.join("completion.json").is_file(), "{attempt:?}");
    }
}

fn expect_code(code: Option<i32>, mode: &str) {
    // An injected exit ends the child with 86; an injected error returns.
    assert_eq!(code, Some(if mode == "exit" { 86 } else { 0 }));
}

fn journey(case: &str, stage: &str, mode: &str) {
    if child(case) {
        return;
    }
    let _guard = CHILD.lock().unwrap_or_else(|error| error.into_inner());
    let world = World::new();
    let original = snapshot(&world.bank());
    expect_code(faulted(&world, case, stage, mode), mode);
    if mode == "error" {
        assert!(outcome(&world).starts_with("error:"), "{}", outcome(&world));
    }
    let (attempt, _) = fresh_attempt(&world);
    assert!(attempt.join("plan.json").is_file());
    if stage.ends_with("directories-removed") {
        assert!(attempt.join("completion.json").is_file());
        assert!(!world.bank().join("topics/probe-nested").exists());
        assert_eq!(snapshot(&world.bank()), original);
    }
    if stage.ends_with("after-attempt-journal") {
        assert_eq!(snapshot(&world.bank()), original);
        assert!(!attempt.join("pins").exists());
    }
    finish(&world, &original);
}

fn probe_case(case: &str, kind: &str, index: usize, mode: &str) -> Vec<(String, String, bool)> {
    let world = World::new();
    let original = snapshot(&world.bank());
    let plan = plan(&world.bank());
    let rows = candidates(&world.bank(), &plan);
    assert!(index < rows.len());
    let node_index = rows[index].1;
    let point = if kind == "created" {
        "after-pin"
    } else {
        "removed"
    };
    let stage = format!("layout-recovery-probe-{point}:{node_index}");
    expect_code(faulted(&world, case, &stage, mode), mode);
    if mode == "error" {
        assert!(outcome(&world).starts_with("error:"), "{}", outcome(&world));
    }
    let (_, attempt) = fresh_attempt(&world);
    let node = node_at(&attempt, node_index);
    let templates = templates(&world.bank(), &plan);
    assert_eq!(
        get(&node, "template"),
        get(&templates[node_index].0, "path")
    );
    assert!(!world.bank().join(".agent-memory-transaction.md").exists());
    let path = world.bank().join(text(&node, "path"));
    if kind == "created" && mode == "exit" {
        assert!(path.exists());
    }
    if kind == "removed" {
        assert!(!path.exists());
    }
    finish(&world, &original);
    rows.into_iter().map(|(label, _)| label).collect()
}

fn partition(case: &str, kind: &str, start: usize, mode: &str) {
    if child(case) {
        return;
    }
    let _guard = CHILD.lock().unwrap_or_else(|error| error.into_inner());
    let labels = probe_case(case, kind, start, mode);
    // The baseline fixture has exactly these reachable probe roles.
    assert_eq!(
        labels,
        [
            ("directory".into(), "topics/probe-nested".into(), false),
            ("file".into(), "MEMORY.md".into(), false),
            ("file".into(), "moved-record".into(), true),
            ("file".into(), "original-record".into(), false),
        ]
    );
    for index in (start + 4..labels.len()).step_by(4) {
        probe_case(case, kind, index, mode);
    }
}

fn ambiguity_case(case: &str, node_type: &str, foreign: Option<&str>, mode: &str) {
    if child(case) {
        return;
    }
    let _guard = CHILD.lock().unwrap_or_else(|error| error.into_inner());
    let world = World::new();
    let bank = world.bank();
    let original = snapshot(&bank);
    let plan = plan(&bank);
    let node_index = templates(&bank, &plan)
        .iter()
        .position(|(node, reachable)| *reachable && text(node, "type") == node_type)
        .unwrap();
    let point = if foreign.is_some() {
        "after-pin"
    } else {
        "before-pin"
    };
    let stage = format!("layout-recovery-probe-{point}:{node_index}");
    expect_code(faulted(&world, case, &stage, mode), mode);
    let (attempt_dir, attempt) = fresh_attempt(&world);
    let node = node_at(&attempt, node_index);
    let relative = text(&node, "path").to_owned();
    let target = bank.join(&relative);
    let name = Path::new(&relative)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let pin = attempt_dir.join("pins").join(format!("{name}.json"));
    assert_eq!(pin.exists(), foreign.is_some());
    if mode == "error" {
        assert!(outcome(&world).starts_with("error:"), "{}", outcome(&world));
        assert!(!target.exists());
        finish(&world, &original);
        return;
    }
    assert!(target.exists());
    match foreign {
        Some("inode") => {
            let before = fs::metadata(&target).unwrap().ino();
            fs::rename(
                &target,
                world.root.join("independently-inspected-original-probe"),
            )
            .unwrap();
            fs::write(
                &target,
                b"Foreign fixture occupant; preserve this exact file.\n",
            )
            .unwrap();
            assert_ne!(fs::metadata(&target).unwrap().ino(), before);
        }
        Some(_) => {
            let before = fs::metadata(&target).unwrap().ino();
            fs::write(
                &target,
                b"Foreign fixture occupant; preserve this exact file.\n",
            )
            .unwrap();
            assert_eq!(fs::metadata(&target).unwrap().ino(), before);
        }
        None => {}
    }
    let held = snapshot(&bank);
    let attempt_before = snapshot(&attempt_dir);
    let error = temps::probe_entries(&bank, &plan, &world.directory()).unwrap_err();
    assert_eq!(error.to_string(), "pending-ambiguous-probe-ownership");
    assert_eq!(
        error.kind(),
        &Kind::ProbeOwnership {
            path: relative.clone()
        }
    );
    let refused = temps::probe(&bank, &plan, &world.directory(), None, None, None, None);
    assert!(matches!(
        refused.unwrap_err().kind(),
        Kind::ProbeOwnership { .. }
    ));
    for row in get(&attempt, "nodes").as_array().unwrap() {
        if let Some(nonce) = row.get("nonce").and_then(Value::as_str) {
            assert!(!error.to_string().contains(nonce));
        }
    }
    assert_eq!(snapshot(&bank), held);
    assert_eq!(snapshot(&attempt_dir), attempt_before);
    assert!(!bank.join(".agent-memory-transaction.md").exists());
    // Simulated operator inspection/removal in this disposable fixture only.
    if target.is_dir() {
        fs::remove_dir(&target).unwrap();
    } else {
        fs::remove_file(&target).unwrap();
    }
    finish(&world, &original);
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_attempt_journal_error
#[test]
fn attempt_journal_error() {
    journey(
        "attempt_journal_error",
        "layout-recovery-probe-after-attempt-journal",
        "error",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_attempt_journal_exit
#[test]
fn attempt_journal_exit() {
    journey(
        "attempt_journal_exit",
        "layout-recovery-probe-after-attempt-journal",
        "exit",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_directories_removed_error
#[test]
fn directories_removed_error() {
    journey(
        "directories_removed_error",
        "layout-recovery-probe-directories-removed",
        "error",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_directories_removed_exit
#[test]
fn directories_removed_exit() {
    journey(
        "directories_removed_exit",
        "layout-recovery-probe-directories-removed",
        "exit",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_0_error
#[test]
fn created_0_error() {
    partition("created_0_error", "created", 0, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_0_exit
#[test]
fn created_0_exit() {
    partition("created_0_exit", "created", 0, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_1_error
#[test]
fn created_1_error() {
    partition("created_1_error", "created", 1, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_1_exit
#[test]
fn created_1_exit() {
    partition("created_1_exit", "created", 1, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_2_error
#[test]
fn created_2_error() {
    partition("created_2_error", "created", 2, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_2_exit
#[test]
fn created_2_exit() {
    partition("created_2_exit", "created", 2, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_3_error
#[test]
fn created_3_error() {
    partition("created_3_error", "created", 3, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_3_exit
#[test]
fn created_3_exit() {
    partition("created_3_exit", "created", 3, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_0_error
#[test]
fn removed_0_error() {
    partition("removed_0_error", "removed", 0, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_0_exit
#[test]
fn removed_0_exit() {
    partition("removed_0_exit", "removed", 0, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_1_error
#[test]
fn removed_1_error() {
    partition("removed_1_error", "removed", 1, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_1_exit
#[test]
fn removed_1_exit() {
    partition("removed_1_exit", "removed", 1, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_2_error
#[test]
fn removed_2_error() {
    partition("removed_2_error", "removed", 2, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_2_exit
#[test]
fn removed_2_exit() {
    partition("removed_2_exit", "removed", 2, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_3_error
#[test]
fn removed_3_error() {
    partition("removed_3_error", "removed", 3, "error");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_3_exit
#[test]
fn removed_3_exit() {
    partition("removed_3_exit", "removed", 3, "exit");
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_unpinned_file_remains_pending_until_operator_removes_it
#[test]
fn unpinned_file_remains_pending_until_operator_removes_it() {
    ambiguity_case(
        "unpinned_file_remains_pending_until_operator_removes_it",
        "file",
        None,
        "exit",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_unpinned_directory_remains_pending_until_operator_removes_it
#[test]
fn unpinned_directory_remains_pending_until_operator_removes_it() {
    ambiguity_case(
        "unpinned_directory_remains_pending_until_operator_removes_it",
        "directory",
        None,
        "exit",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_before_pin_file_error_cleans_owned_probe
#[test]
fn before_pin_file_error_cleans_owned_probe() {
    ambiguity_case(
        "before_pin_file_error_cleans_owned_probe",
        "file",
        None,
        "error",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_before_pin_directory_error_cleans_owned_probe
#[test]
fn before_pin_directory_error_cleans_owned_probe() {
    ambiguity_case(
        "before_pin_directory_error_cleans_owned_probe",
        "directory",
        None,
        "error",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_foreign_replacement_after_pin_remains_pending
#[test]
fn foreign_replacement_after_pin_remains_pending() {
    ambiguity_case(
        "foreign_replacement_after_pin_remains_pending",
        "file",
        Some("inode"),
        "exit",
    );
}

// obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_nonce_after_pin_remains_pending
#[test]
fn changed_nonce_after_pin_remains_pending() {
    ambiguity_case(
        "changed_nonce_after_pin_remains_pending",
        "file",
        Some("nonce"),
        "exit",
    );
}

/// Completed-attempt evidence written by a real successful probe.
fn completed(world: &World) -> (PathBuf, Value) {
    let bank = world.bank();
    let plan = plan(&bank);
    temps::probe(&bank, &plan, &world.directory(), None, None, None, None).unwrap();
    let (attempt, value) = fresh_attempt(world);
    assert!(attempt.join("completion.json").is_file());
    (attempt, value)
}

fn refuses_without_deletion(world: &World, expected: &str) {
    let bank = world.bank();
    let before = (snapshot(&bank), snapshot(&world.directory()));
    let plan = plan(&bank);
    let error = temps::probe_entries(&bank, &plan, &world.directory()).unwrap_err();
    assert_eq!(error.to_string(), expected);
    let refused = temps::probe_complete(&bank, &plan, &world.directory()).unwrap_err();
    assert_eq!(refused.to_string(), expected);
    assert_eq!((snapshot(&bank), snapshot(&world.directory())), before);
}

// obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_unknown_pin_refuses_without_deletion
#[test]
fn unknown_pin_refuses_without_deletion() {
    let world = World::new();
    let (attempt, _) = completed(&world);
    let foreign = attempt.join("pins").join(format!(
        "{PREFIX}{}.json",
        agent_memory::host::uuid4().unwrap()
    ));
    write(
        &foreign,
        &encoded(&object([("unowned", Value::Bool(true))])).unwrap(),
        0o600,
    );
    refuses_without_deletion(&world, "Unknown metadata probe pin evidence");
    assert!(foreign.is_file());
}

// obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_changed_attempt_plan_refuses_without_deletion
#[test]
fn changed_attempt_plan_refuses_without_deletion() {
    let world = World::new();
    let (attempt, value) = completed(&world);
    let mut changed = value.clone();
    if let Value::Object(pairs) = &mut changed {
        pairs
            .iter_mut()
            .find(|(key, _)| key == "plan_sha256")
            .unwrap()
            .1 = s("0".repeat(64));
    }
    write(
        &attempt.join("plan.json"),
        &encoded(&changed).unwrap(),
        0o600,
    );
    refuses_without_deletion(&world, "Invalid metadata probe attempt");
}

// obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_foreign_probe_at_completed_path_refuses_without_deletion
#[test]
fn foreign_probe_at_completed_path_refuses_without_deletion() {
    let world = World::new();
    let bank = world.bank();
    let (_, value) = completed(&world);
    let node = get(&value, "nodes")
        .as_array()
        .unwrap()
        .iter()
        .find(|node| {
            text(node, "type") == "file" && bank.join(text(node, "path")).parent().unwrap().is_dir()
        })
        .unwrap();
    let foreign = bank.join(text(node, "path"));
    assert!(!foreign.exists());
    fs::write(&foreign, b"External fixture occupant must remain.\n").unwrap();
    refuses_without_deletion(&world, "pending-ambiguous-probe-ownership");
    assert_eq!(
        fs::read(&foreign).unwrap(),
        b"External fixture occupant must remain.\n"
    );
}
