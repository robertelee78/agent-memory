//! hooks: the native `am hook <name>` commands (baseline pack `hooks/*.py`),
//! the retargeted OpenCode/OMP adapters and the client hook command rewrite.
//! Real-binary cases run `am` with a private 0700 world environment; library
//! cases needing that environment re-execute this test binary inside it.
#[allow(dead_code)]
#[path = "support/admission.rs"]
mod bank_fixture;
#[allow(unused_imports)]
mod support;

use agent_memory::client_hooks;
use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::hooks::{self, boundary_ledger as ledger};
use agent_memory::process::{self, Input, Spec, Status, Target};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::os::fd::AsFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::World;

const AM: &str = env!("CARGO_BIN_EXE_am");
const CASE: &str = "AM_HOOKS_CASE";
const TURN_FAILED: &str = "{\"systemMessage\": \"Agent Memory turn hook failed; automatic memory refresh and capture are pending.\"}\n";
const STOP_FAILED: &str = "{\"systemMessage\": \"Agent Memory boundary could not verify its source or private state; capture and publication are pending. Preserve the bank and pending evidence, run am doctor in this repository, and inspect before retrying.\"}\n";
const PROMPT_BLOCKED: &str = "{\"decision\": \"block\", \"reason\": \"Agent Memory could not retain this input safely. Preserve work; run am doctor and inspect pending source capacity/state before retrying.\"}\n";
const LEDGER_PENDING: &str = "{\"status\": \"pending\", \"reason\": \"ledger-unavailable\"}\n";

// ----- shared helpers -----

fn decode(text: &str) -> Value {
    json::decode(text, DecodePolicy::default()).unwrap_or_else(|e| panic!("{e}: {text}"))
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap_or_default()
}
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
/// `sha256(json.dumps([session, source], separators=(',', ':')))`.
fn turn_key(session: &str, source: &str) -> String {
    let encoded = json::encode(
        &Value::Array(vec![
            Value::String(session.into()),
            Value::String(source.into()),
        ]),
        json::COMPACT_ASCII,
    )
    .unwrap();
    sha256(encoded.as_bytes())
}
fn session_directory(root: &Path, session: &str) -> PathBuf {
    root.join("experimental-boundary")
        .join(sha256(session.as_bytes()))
}
/// Every regular file under `root` with its bytes, in path order.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let info = std::fs::symlink_metadata(&path).unwrap();
            if info.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    files.sort();
    files
}
/// `git -C repo <args>` in the case's world environment.
fn git(case: &Case, repo: &Path, args: &[&str]) {
    let mut all = vec!["-C", repo.to_str().unwrap()];
    all.extend_from_slice(args);
    let run = finished(
        process::run_for(
            &spec(
                &argv("/usr/bin/git", &all),
                &case.env(),
                &case.root,
                Input::Null,
                Duration::from_secs(60),
            ),
            Duration::from_secs(60),
        )
        .unwrap(),
    );
    assert_eq!(run.code, 0, "{run:?}");
}
fn names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}
fn with_suffix(directory: &Path, suffix: &str) -> Vec<String> {
    names(directory)
        .into_iter()
        .filter(|name| name.ends_with(suffix))
        .collect()
}
fn mkdir(path: &Path) {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .unwrap();
}
fn private_write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn no_learning(source: &str) -> Value {
    decode(&format!(
        "{{\"source_id\": {}, \"source_review\": \"Reviewed source.\", \"candidates\": []}}",
        quoted(source)
    ))
}
fn quoted(value: &str) -> String {
    json::encode(&Value::String(value.into()), json::DEFAULT).unwrap()
}

/// A private canonical temporary state root, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let base = agent_memory::fs::canonical_path(Path::new("/tmp"), true).unwrap();
        let path = base.join(format!("am-hooks-{}", agent_memory::host::uuid4().unwrap()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn reserve(root: &Path, session: &str, source: &str, action: &str) -> ledger::Result<Value> {
    ledger::reserve(
        root.to_str().unwrap(),
        session,
        source,
        action,
        ledger::Options::default(),
    )
}
fn reserve_with(
    root: &Path,
    session: &str,
    source: &str,
    action: &str,
    options: ledger::Options<'_>,
) -> ledger::Result<Value> {
    ledger::reserve(root.to_str().unwrap(), session, source, action, options)
}
fn status(value: &ledger::Result<Value>) -> String {
    match value {
        Ok(value) => text(value, "status").to_owned(),
        Err(error) => format!("error: {error}"),
    }
}

/// One finished `am` invocation.
#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}
impl Run {
    fn value(&self) -> Value {
        decode(&self.stdout)
    }
}
fn finished(output: process::Output) -> Run {
    Run {
        code: match output.status {
            Status::Code(code) => code,
            Status::Signal(signal) => -signal,
        },
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}
fn spec<'a>(
    argv: &'a [OsString],
    env: &'a [(OsString, OsString)],
    cwd: &'a Path,
    stdin: Input<'a>,
    timeout: Duration,
) -> Spec<'a> {
    Spec {
        argv,
        search_path: false,
        env,
        cwd: Some(cwd),
        stdin,
        stdout: Target::Capture { limit: 1 << 20 },
        stderr: Target::Capture { limit: 1 << 20 },
        deadline: Deadline::after(timeout).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    }
}
fn argv(program: &str, args: &[&str]) -> Vec<OsString> {
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect()
}
fn world_env(world: &World, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = world
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (key, value) in extra {
        env.retain(|(existing, _)| existing.as_os_str() != std::ffi::OsStr::new(key));
        env.push((OsString::from(key), OsString::from(value)));
    }
    env
}
/// `am <args>` with `input` on stdin in `env`.
fn am(args: &[&str], input: &[u8], env: &[(OsString, OsString)], cwd: &Path) -> Run {
    let argv = argv(AM, args);
    finished(
        process::run_for(
            &spec(
                &argv,
                env,
                cwd,
                Input::Bytes(input),
                Duration::from_secs(120),
            ),
            Duration::from_secs(120),
        )
        .unwrap(),
    )
}
fn hook(world: &World, name: &str, input: &str) -> Run {
    am(
        &["hook", name],
        input.as_bytes(),
        &world_env(world, &[]),
        &world.root,
    )
}
fn ledger_run(world: &World, request: &str, extra: &[(&str, &str)]) -> Run {
    am(
        &["hook", "boundary-ledger"],
        request.as_bytes(),
        &world_env(world, extra),
        &world.root,
    )
}
fn ledger_request(root: &Path, session: &str, source: &str, action: &str, rest: &str) -> String {
    format!(
        "{{\"root\": {}, \"session\": {}, \"source\": {}, \"action\": {}{rest}}}",
        quoted(root.to_str().unwrap()),
        quoted(session),
        quoted(source),
        quoted(action)
    )
}

/// Run `am <args>` paused at fault `stage`; `during` acts while it waits.
fn paused(world: &World, args: &[&str], input: &str, stage: &str, during: impl FnOnce()) -> Run {
    let faults = world.root.join("faults");
    let request = world.root.join("tmp").join(format!("{stage}.json"));
    private_write(&request, input.as_bytes());
    let stdin = std::fs::File::open(&request).unwrap();
    let env = world_env(
        world,
        &[
            ("AGENT_MEMORY_FAULT_STAGE", stage),
            ("AGENT_MEMORY_FAULT_MODE", "pause"),
            ("AGENT_MEMORY_FAULT_DIR", faults.to_str().unwrap()),
        ],
    );
    let argv = argv(AM, args);
    let child = process::spawn_owned(&spec(
        &argv,
        &env,
        &world.root,
        Input::Descriptor(stdin.as_fd()),
        Duration::from_secs(60),
    ))
    .unwrap();
    let ready = faults.join(format!("{stage}.ready"));
    let deadline = Deadline::after(Duration::from_secs(30)).unwrap();
    while !ready.exists() {
        assert!(!deadline.expired(), "fault {stage} was never reached");
        std::thread::sleep(Duration::from_millis(10));
    }
    during();
    private_write(&faults.join(format!("{stage}.release")), b"");
    finished(
        child
            .finish(Deadline::after(Duration::from_secs(60)).unwrap())
            .unwrap(),
    )
}

/// The context of a case re-executed inside its world's environment.
struct Case {
    root: PathBuf,
}
impl Case {
    fn env(&self) -> Vec<(OsString, OsString)> {
        std::env::vars_os().collect()
    }
    fn hook(&self, name: &str, input: &str) -> Run {
        am(&["hook", name], input.as_bytes(), &self.env(), &self.root)
    }
    /// A bound repository with an initialized bank (baseline admission fixture).
    fn bank(&self) -> (PathBuf, PathBuf) {
        let fixture = bank_fixture::World {
            root: self.root.clone(),
        };
        let (repo, bank, _) = fixture.fixture();
        (repo, bank)
    }
    fn snapshot(&self, repo: &Path, bank: &Path) -> (Value, String, String) {
        bank_fixture::World {
            root: self.root.clone(),
        }
        .state(repo, bank)
    }
    /// The executing runtime pack for in-process hooks: this source tree's.
    fn runtime(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory")
    }
    fn library(&self, name: &str, input: &str) -> hooks::Output {
        let pack = self.runtime();
        hooks::run(name, input.as_bytes(), &move || Ok(pack.clone())).unwrap()
    }
}

/// Run `body` in a child of this test binary whose whole environment is a
/// fresh private world (HOME/XDG/state roots inside it).
fn in_world(name: &str, body: impl FnOnce(&Case)) {
    if std::env::var(CASE).ok().as_deref() == Some(name) {
        agent_memory::cli::register_providers().unwrap();
        let root = PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap());
        body(&Case { root });
        return;
    }
    let world = World::new().unwrap();
    let env = world_env(&world, &[(CASE, name)]);
    let exe = std::env::current_exe().unwrap();
    let argv = argv(
        exe.to_str().unwrap(),
        &[
            name,
            "--exact",
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ],
    );
    let run = finished(
        process::run_for(
            &spec(
                &argv,
                &env,
                &world.root,
                Input::Null,
                Duration::from_secs(600),
            ),
            Duration::from_secs(600),
        )
        .unwrap(),
    );
    assert_eq!(run.code, 0, "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("1 passed"), "{}", run.stdout);
}

fn node(world: &World) -> Option<PathBuf> {
    let node = world.root.join("node-tools/node");
    node.exists().then_some(node)
}
fn run_node(world: &World, source: &str, args: &[&Path], extra: &[(&str, &str)]) -> Run {
    let node = node(world).unwrap();
    let mut argv = argv(
        node.to_str().unwrap(),
        &["--input-type=module", "-e", source],
    );
    argv.extend(args.iter().map(|path| path.as_os_str().to_owned()));
    finished(
        process::run_for(
            &spec(
                &argv,
                &world_env(world, extra),
                &world.root,
                Input::Null,
                Duration::from_secs(60),
            ),
            Duration::from_secs(60),
        )
        .unwrap(),
    )
}
fn pack_hooks() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory/hooks")
}
fn stripped_guidance() -> String {
    std::fs::read_to_string(pack_hooks().join("boundary-prompt.txt"))
        .unwrap()
        .trim()
        .to_owned()
}

// ----- turn hook, Claude Stop guard (test_turn_hook.py) -----

// obligation: tests/contracts/test_turn_hook.py::TurnHookContract.test_prompt_keeps_harness_turn_unblocked
#[test]
fn prompt_keeps_harness_turn_unblocked() {
    let world = World::new().unwrap();
    let run = hook(
        &world,
        "turn-hook",
        r#"{"hook_event_name": "UserPromptSubmit", "session_id": "fixture-session", "cwd": "/tmp"}"#,
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    let value = run.value();
    assert!(value.get("decision").is_none());
    let output = get(&value, "hookSpecificOutput");
    assert_eq!(text(output, "hookEventName"), "UserPromptSubmit");
    assert_eq!(text(output, "additionalContext"), stripped_guidance());
    assert!(run.stdout.starts_with("{\"hookSpecificOutput\": {\"hookEventName\": \"UserPromptSubmit\", \"additionalContext\": \"Agent Memory turn boundary:"));
    assert!(run.stdout.ends_with("\"}}\n"));
    assert_eq!(run.stderr, "");
}

// obligation: tests/contracts/test_turn_hook.py::TurnHookContract.test_opt_in_claude_stop_wakes_once_and_preserves_permissions
#[test]
fn opt_in_claude_stop_wakes_once_and_preserves_permissions() {
    let world = World::new().unwrap();
    let guarded = hook(
        &world,
        "claude-stop",
        r#"{"hook_event_name": "Stop", "stop_hook_active": true}"#,
    );
    assert_eq!(guarded.code, 0);
    assert_eq!(guarded.stderr, "");
    let malformed = hook(&world, "claude-stop", r#"{"hook_event_name": "Stop"}"#);
    assert_eq!(malformed.code, 0);
    assert!(text(&malformed.value(), "systemMessage").contains("pending"));
    assert_eq!(malformed.stdout, STOP_FAILED);
}

// obligation: tests/contracts/test_turn_hook.py::TurnHookContract.test_invalid_and_oversized_events_surface_pending_without_blocking
#[test]
fn invalid_and_oversized_events_surface_pending_without_blocking() {
    let world = World::new().unwrap();
    let oversized = "X".repeat(64 * 1024 + 1);
    for raw in ["bad json", "{}", oversized.as_str()] {
        let run = hook(&world, "turn-hook", raw);
        assert_eq!(run.code, 0, "{}", raw.len());
        let value = run.value();
        assert!(value.get("decision").is_none());
        assert!(text(&value, "systemMessage").contains("pending"));
        assert_eq!(run.stdout, TURN_FAILED);
    }
}

// obligation: tests/contracts/test_turn_hook.py::TurnHookContract.test_opencode_v2_exposes_only_observed_session_identity_during_active_queue
#[test]
fn opencode_v2_exposes_only_observed_session_identity_during_active_queue() {
    let world = World::new().unwrap();
    if node(&world).is_none() {
        eprintln!("JavaScript runtime unavailable; skipped as in the baseline");
        return;
    }
    let source = r#"
import {pathToFileURL} from 'node:url';
const module = await import(pathToFileURL(process.argv[1]));
const callbacks = [];
const ctx = {
  tool: {transform: async callback => {
    callback({namespace() {}, add() {}}); return {dispose() {}};
  }},
  session: {hook: async (event, callback) => {
    callbacks.push({event, callback}); return {dispose() {}};
  }},
};
const dispose = await module.default.setup(ctx);
const callback = callbacks.filter(item => item.event === 'context').at(-1).callback;
const good = {sessionID: 'ses_123abc_DEF', system: []};
callback(good); callback(good);
const invalid = {sessionID: 'ses_bad\nforged prompt', system: []};
callback(invalid);
const stale = {sessionID: 'ses_current', system: [{type:'text',text:'Agent Memory observed OpenCode session: ses_stale. foreign context'}]};
callback(stale); callback(stale); dispose();
process.stdout.write(JSON.stringify({good: good.system, invalid: invalid.system, stale: stale.system}));
"#;
    let run = run_node(&world, source, &[&pack_hooks().join("opencode-v2.js")], &[]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let value = run.value();
    let good = get(&value, "good").as_array().unwrap();
    assert_eq!(good.len(), 2);
    let foreground = text(&good[1], "text");
    assert!(foreground.contains("Defer automatic capture"));
    assert!(foreground.contains("explicitly asks for a memory operation"));
    assert!(foreground.contains("queued continuation"));
    assert!(text(&good[0], "text").contains("ses_123abc_DEF"));
    assert!(text(&good[0], "text").contains("does not request foreground memory work"));
    let invalid = get(&value, "invalid").as_array().unwrap();
    assert_eq!(invalid.len(), 1);
    assert!(text(&invalid[0], "text").contains("unavailable or invalid"));
    let stale = get(&value, "stale").as_array().unwrap();
    assert_eq!(stale.len(), 2);
    assert!(text(&stale[0], "text").contains("ses_stale"));
    assert!(text(&stale[1], "text").contains("ses_current"));
    assert!(text(&stale[1], "text").contains("do not record or acknowledge"));
}

// obligation: tests/contracts/test_turn_hook.py::TurnHookContract.test_opencode_adapters_inject_identical_context_once
#[test]
fn opencode_adapters_inject_identical_context_once() {
    let world = World::new().unwrap();
    if node(&world).is_none() {
        eprintln!("JavaScript runtime unavailable; skipped as in the baseline");
        return;
    }
    let source = r#"
import {pathToFileURL} from 'node:url';
const v1 = await import(pathToFileURL(process.argv[1]));
const v2 = await import(pathToFileURL(process.argv[2]));
const a = await v1.AgentMemoryBoundary();
const one = {system: []};
await a['experimental.chat.system.transform']({}, one);
await a['experimental.chat.system.transform']({}, one);
let callback, disposed = false;
const dispose = await v2.default.setup({session: {hook: async (_event, cb) => {
  callback = cb; return {dispose: () => { disposed = true }};
}}});
const two = {system: []}; callback(two); callback(two); dispose();
const failureDispose = await v2.default.setup({session: {hook: async () => { throw new Error('fixture'); }}});
failureDispose();
process.stdout.write(JSON.stringify({one: one.system, two: two.system, disposed}));
"#;
    let target = world.root.join("folder space $& apostrophe's/hooks");
    mkdir(&target);
    for name in ["opencode-v1", "opencode-v2"] {
        std::fs::copy(
            pack_hooks().join(format!("{name}.js")),
            target.join(format!("{name}.mjs")),
        )
        .unwrap();
    }
    for name in ["boundary-prompt.txt", "opencode-v2-queue.js"] {
        std::fs::copy(pack_hooks().join(name), target.join(name)).unwrap();
    }
    std::fs::write(target.join("package.json"), "{\"type\":\"module\"}").unwrap();
    let run = run_node(
        &world,
        source,
        &[
            &target.join("opencode-v1.mjs"),
            &target.join("opencode-v2.mjs"),
        ],
        &[],
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    let value = run.value();
    let one = get(&value, "one").as_array().unwrap();
    let two = get(&value, "two").as_array().unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(two.len(), 1);
    assert_eq!(one[0].as_str().unwrap(), text(&two[0], "text"));
    assert_eq!(get(&value, "disposed"), &Value::Bool(true));
    assert!(run.stderr.contains("pending"));
}

// ----- boundary ledger (test_opencode_boundary_ledger.py) -----

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_durable_reservation_and_restart_refusal
#[test]
fn durable_reservation_and_restart_refusal() {
    let root = Scratch::new();
    let first = reserve(root.path(), "session", "turn", "reserve").unwrap();
    assert_eq!(text(&first, "status"), "reserved");
    assert_eq!(names(&session_directory(root.path(), "session")).len(), 2);
    let again = reserve(root.path(), "session", "turn", "reserve").unwrap();
    assert_eq!(text(&again, "reason"), "existing-intent");
    assert_eq!(
        status(&reserve(root.path(), "session", "other", "reserve")),
        "reserved"
    );
    // The turn key is sha256 of the compact JSON pair ["s","turn"].
    let known = reserve(root.path(), "s", "turn", "reserve").unwrap();
    assert_eq!(
        text(&known, "turn_key"),
        "f6a180926ea524d0eef334bb84d93438eb7e19be12bb5b5ece0d732693f590b0"
    );
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_intent_without_attempt_is_uncertain
#[test]
fn intent_without_attempt_is_uncertain() {
    let root = Scratch::new();
    let directory = session_directory(root.path(), "session");
    mkdir(&directory);
    let key = turn_key("session", "turn");
    std::fs::write(directory.join(format!("{key}.intent")), "interrupted").unwrap();
    assert_eq!(
        status(&reserve(root.path(), "session", "turn", "reserve")),
        "pending"
    );
    assert!(!directory.join(format!("{key}.attempt")).exists());
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_concurrent_single_reservation
#[test]
fn concurrent_single_reservation() {
    let root = Scratch::new();
    let results: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| status(&reserve(root.path(), "session", "turn", "reserve"))))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(
        results.iter().filter(|s| *s == "reserved").count(),
        1,
        "{results:?}"
    );
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_ids_cannot_collide_on_separator
#[test]
fn ids_cannot_collide_on_separator() {
    let root = Scratch::new();
    assert_eq!(
        status(&reserve(root.path(), "a\0b", "c", "reserve")),
        "reserved"
    );
    assert_eq!(
        status(&reserve(root.path(), "a", "b\0c", "reserve")),
        "reserved"
    );
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_supported_no_learning_assessment_compacts_and_deduplicates
#[test]
fn supported_no_learning_assessment_compacts_and_deduplicates() {
    let root = Scratch::new();
    let path = root.path();
    assert_eq!(status(&reserve(path, "s", "turn", "reserve")), "reserved");
    assert_eq!(
        status(&reserve(path, "s", "turn", "inspect")),
        "pending-turn"
    );
    assert!(reserve(path, "s", "turn", "assessed-no-learning").is_err());
    let eligible = decode(
        r#"{"source_id": "turn", "source_review": "Reviewed", "candidates": [{"claim": "Confirmed decision", "disposition": "eligible", "reason": "Durable"}]}"#,
    );
    let options = ledger::Options {
        assessment: Some(&eligible),
        ..ledger::Options::default()
    };
    assert!(reserve_with(path, "s", "turn", "assessed-no-learning", options).is_err());
    assert_eq!(
        status(&reserve(path, "s", "turn", "inspect")),
        "pending-turn"
    );
    let reviewed = no_learning("turn");
    let options = ledger::Options {
        assessment: Some(&reviewed),
        ..ledger::Options::default()
    };
    assert_eq!(
        status(&reserve_with(
            path,
            "s",
            "turn",
            "assessed-no-learning",
            options
        )),
        "assessed-no-learning"
    );
    assert_eq!(
        status(&reserve(path, "s", "turn", "reserve")),
        "assessed-no-learning"
    );
    assert_eq!(
        status(&reserve(path, "s", "turn", "inspect")),
        "completed-source"
    );
    let directory = session_directory(path, "s");
    assert_eq!(names(&directory).len(), 1);
    // The completion marker is the sorted baseline record.
    let key = turn_key("s", "turn");
    let marker = std::fs::read_to_string(directory.join(format!("{key}.assessed"))).unwrap();
    let request_hash =
        sha256(br#"{"candidates":[],"source_id":"turn","source_review":"Reviewed source."}"#);
    assert_eq!(
        marker,
        format!(
            "{{\"repository\": null, \"request_hash\": \"{request_hash}\", \"status\": \"assessed-no-learning\", \"witnessed_commit\": null}}"
        )
    );
    let missing = no_learning("missing");
    let options = ledger::Options {
        assessment: Some(&missing),
        ..ledger::Options::default()
    };
    assert!(matches!(
        reserve_with(path, "s", "missing", "assessed-no-learning", options),
        Err(ledger::Error::Missing(_))
    ));
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_supported_intent_only_is_explicitly_recoverable
#[test]
fn supported_intent_only_is_explicitly_recoverable() {
    let root = Scratch::new();
    let path = root.path();
    reserve(path, "s", "turn", "reserve").unwrap();
    let directory = session_directory(path, "s");
    let attempt = with_suffix(&directory, ".attempt");
    std::fs::remove_file(directory.join(&attempt[0])).unwrap();
    assert_eq!(
        status(&reserve(path, "s", "turn", "inspect")),
        "pending-turn"
    );
    let reviewed = no_learning("turn");
    let options = ledger::Options {
        assessment: Some(&reviewed),
        ..ledger::Options::default()
    };
    assert_eq!(
        status(&reserve_with(
            path,
            "s",
            "turn",
            "assessed-no-learning",
            options
        )),
        "assessed-no-learning"
    );
    assert_eq!(
        status(&reserve(path, "s", "turn", "inspect")),
        "completed-source"
    );
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_supported_tampered_intent_stays_pending
#[test]
fn supported_tampered_intent_stays_pending() {
    for field in ["schema", "turn_key"] {
        let root = Scratch::new();
        let path = root.path();
        reserve(path, "s", "turn", "reserve").unwrap();
        let directory = session_directory(path, "s");
        let intent = directory.join(&with_suffix(&directory, ".intent")[0]);
        let mut value = decode(&std::fs::read_to_string(&intent).unwrap());
        *value.get_mut(field).unwrap() = Value::String("tampered".into());
        std::fs::write(&intent, json::encode(&value, json::DEFAULT).unwrap()).unwrap();
        assert!(matches!(
            reserve(path, "s", "turn", "inspect"),
            Err(ledger::Error::Value(_))
        ));
        let reviewed = no_learning("turn");
        let options = ledger::Options {
            assessment: Some(&reviewed),
            ..ledger::Options::default()
        };
        assert!(matches!(
            reserve_with(path, "s", "turn", "assessed-no-learning", options),
            Err(ledger::Error::Value(_))
        ));
        assert!(intent.exists());
        assert!(with_suffix(&directory, ".assessed").is_empty());
    }
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_supported_capacity_has_non_destructive_recovery
#[test]
fn supported_capacity_has_non_destructive_recovery() {
    let world = World::new().unwrap();
    let root = world.root.join("ledger");
    mkdir(&root);
    let request = |source: &str| ledger_request(&root, "s", source, "reserve", "");
    let one = ledger_run(
        &world,
        &request("one"),
        &[("AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT", "2")],
    );
    assert_eq!(text(&one.value(), "status"), "reserved");
    let two = ledger_run(
        &world,
        &request("two"),
        &[("AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT", "2")],
    );
    assert_eq!(text(&two.value(), "reason"), "ledger-capacity");
    let raised = ledger_run(
        &world,
        &request("two"),
        &[("AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT", "4")],
    );
    assert_eq!(text(&raised.value(), "status"), "reserved");
    assert_eq!(names(&session_directory(&root, "s")).len(), 4);
    for invalid in ["1", "1000001", "many"] {
        let refused = ledger_run(
            &world,
            &request("three"),
            &[("AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT", invalid)],
        );
        assert_eq!(text(&refused.value(), "reason"), "ledger-limit-invalid");
    }
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_rebinding_before_return_fails_pending
#[test]
fn rebinding_before_return_fails_pending() {
    let world = World::new().unwrap();
    let root = world.root.join("ledger");
    mkdir(&root);
    let request = ledger_request(&root, "session", "turn", "reserve", "");
    let run = paused(
        &world,
        &["hook", "boundary-ledger"],
        &request,
        "boundary-ledger-reserve-recheck",
        || {
            std::fs::rename(
                root.join("experimental-boundary"),
                root.join("old-boundary"),
            )
            .unwrap();
            mkdir(&root.join("experimental-boundary"));
        },
    );
    assert_eq!(run.code, 1, "{run:?}");
    assert_eq!(run.stdout, LEDGER_PENDING);
    let moved = root.join("old-boundary").join(sha256(b"session"));
    assert_eq!(names(&moved).len(), 2);
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::BoundaryLedgerTest.test_rejects_symlink_and_untrusted_root
#[test]
fn rejects_symlink_and_untrusted_root() {
    let root = Scratch::new();
    let other = Scratch::new();
    std::os::unix::fs::symlink(other.path(), root.path().join("experimental-boundary")).unwrap();
    assert!(matches!(
        reserve(root.path(), "session", "turn", "reserve"),
        Err(ledger::Error::Os(_))
    ));
    assert!(names(other.path()).is_empty());
    let open = Scratch::new();
    std::fs::set_permissions(open.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(reserve(open.path(), "session", "turn", "reserve").is_err());
    assert!(!open.path().join("experimental-boundary").exists());
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::OpenCodeLauncherContract.test_real_ledger_with_older_python_first
#[test]
fn real_ledger_runs_through_the_am_binary() {
    let world = World::new().unwrap();
    let repo = world.repository("repository", false).unwrap();
    let request = format!(
        "{{\"root\": {}, \"session\": \"s\", \"source\": \"p\", \"repo\": {}}}",
        quoted(world.root.to_str().unwrap()),
        quoted(repo.to_str().unwrap())
    );
    let run = ledger_run(&world, &request, &[]);
    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(run.stdout, "{\"status\": \"not-applicable\"}\n");
}

// obligation: tests/contracts/test_opencode_boundary_ledger.py::UninitializedBoundaryContract.test_absent_context_skips_and_malformed_bank_remains_pending
#[test]
fn absent_context_skips_and_malformed_bank_remains_pending() {
    let world = World::new().unwrap();
    let repo = world.repository("repository", false).unwrap();
    let outside = world.root.join("outside");
    mkdir(&outside);
    let invoke = |location: &Path| {
        let request = format!(
            "{{\"root\": {}, \"session\": \"s\", \"source\": \"p\", \"repo\": {}}}",
            quoted(world.root.to_str().unwrap()),
            quoted(location.to_str().unwrap())
        );
        ledger_run(&world, &request, &[])
    };
    for location in [&repo, &outside] {
        let run = invoke(location);
        assert_eq!(run.code, 0, "{run:?}");
        assert_eq!(text(&run.value(), "status"), "not-applicable");
    }
    assert!(!world.root.join("experimental-boundary").exists());
    mkdir(&repo.join("memory-bank"));
    std::fs::write(repo.join("memory-bank/MEMORY.md"), "malformed bank\n").unwrap();
    let run = invoke(&repo);
    assert_ne!(run.code, 0);
    assert_eq!(text(&run.value(), "status"), "pending");
}

// ----- native OpenCode ledger (test_opencode_native_boundary.py) -----

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_enrollment_backlog_interruption_and_truthful_completion
#[test]
fn enrollment_backlog_interruption_and_truthful_completion() {
    let root = Scratch::new();
    let path = root.path();
    assert_eq!(
        status(&reserve(path, "native-session", "native-source", "enroll")),
        "enrolled"
    );
    let backlog = reserve(path, "next-session", "inventory", "backlog").unwrap();
    assert_eq!(
        get(&backlog, "sources"),
        &decode(r#"[{"session_id": "native-session", "source_id": "native-source"}]"#)
    );
    assert_eq!(
        status(&reserve(path, "native-session", "native-source", "inspect")),
        "pending-turn"
    );
    let again = reserve(path, "native-session", "native-source", "enroll").unwrap();
    assert_eq!(text(&again, "status"), "enrolled");
    assert_eq!(get(&again, "already_enrolled"), &Value::Bool(true));
    let reviewed = decode(
        r#"{"source_id": "native-source", "source_review": "Reviewed actual native source and found no durable project claim.", "candidates": []}"#,
    );
    let done = reserve_with(
        path,
        "native-session",
        "native-source",
        "assessed-no-learning",
        ledger::Options {
            assessment: Some(&reviewed),
            ..ledger::Options::default()
        },
    )
    .unwrap();
    assert_eq!(get(&done, "publication_verified"), &Value::Bool(false));
    let empty = reserve(path, "next-session", "inventory", "backlog").unwrap();
    assert_eq!(get(&empty, "pending_count"), &decode("0"));
    let fingerprint = Value::String("a".repeat(64));
    let woken = reserve_with(
        path,
        "native-session",
        "native-source",
        "wake",
        ledger::Options {
            fingerprint: Some(&fingerprint),
            ..ledger::Options::default()
        },
    );
    assert_eq!(status(&woken), "assessed-no-learning");
}

fn wake(root: &Path, session: &str, source: &str, fingerprint: &str) -> ledger::Result<Value> {
    let fingerprint = Value::String(fingerprint.into());
    reserve_with(
        root,
        session,
        source,
        "wake",
        ledger::Options {
            fingerprint: Some(&fingerprint),
            ..ledger::Options::default()
        },
    )
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_fingerprint_once_and_hard_two_attempt_cap_survive_restart_and_contention
#[test]
fn fingerprint_once_and_hard_two_attempt_cap_survive_restart_and_contention() {
    let root = Scratch::new();
    let path = root.path();
    reserve(path, "session", "source", "enroll").unwrap();
    let a = "a".repeat(64);
    let results: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| status(&wake(path, "session", "source", &a))))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(
        results.iter().filter(|s| *s == "reserved").count(),
        1,
        "{results:?}"
    );
    let repeated = wake(path, "session", "source", &a).unwrap();
    assert_eq!(text(&repeated, "reason"), "wake-already-attempted");
    let second = wake(path, "session", "source", &"b".repeat(64)).unwrap();
    assert_eq!(get(&second, "attempt"), &decode("2"));
    let capped = wake(path, "session", "source", &"c".repeat(64)).unwrap();
    assert_eq!(text(&capped, "reason"), "wake-cap");
    reserve(path, "session", "next-native-source", "enroll").unwrap();
    let next = wake(path, "session", "next-native-source", &"c".repeat(64)).unwrap();
    assert_eq!(get(&next, "attempt"), &decode("1"));
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_racing_lock_creator_preserves_wake_budget_and_completion_across_restart
#[test]
fn racing_lock_creator_preserves_wake_budget_and_completion_across_restart() {
    for first_action in ["wake", "inspect"] {
        let world = World::new().unwrap();
        let root = world.root.join("ledger");
        mkdir(&root);
        reserve(&root, "session", "source", "enroll").unwrap();
        let directory = session_directory(&root, "session");
        let key = turn_key("session", "source");
        // A competing creator won the exclusive first create of the lock.
        private_write(&directory.join(format!("{key}.wake-lock")), b"");
        if first_action == "inspect" {
            assert_eq!(
                status(&reserve(&root, "session", "source", "inspect")),
                "pending-turn"
            );
        }
        assert_eq!(
            get(
                &wake(&root, "session", "source", &"a".repeat(64)).unwrap(),
                "attempt"
            ),
            &decode("1")
        );
        assert_eq!(
            text(
                &wake(&root, "session", "source", &"a".repeat(64)).unwrap(),
                "reason"
            ),
            "wake-already-attempted"
        );
        assert_eq!(
            get(
                &wake(&root, "session", "source", &"b".repeat(64)).unwrap(),
                "attempt"
            ),
            &decode("2")
        );
        assert!(directory.join(format!("{key}.intent")).exists());
        let restarted = |fingerprint: &str| {
            let request = ledger_request(
                &root,
                "session",
                "source",
                "wake",
                &format!(", \"fingerprint\": {}", quoted(fingerprint)),
            );
            let run = ledger_run(&world, &request, &[]);
            assert_eq!(run.code, 0, "{run:?}");
            run.value()
        };
        assert_eq!(text(&restarted(&"c".repeat(64)), "reason"), "wake-cap");
        let reviewed = decode(
            r#"{"source_id": "source", "source_review": "Reviewed retained source; no durable project learning.", "candidates": []}"#,
        );
        let complete = reserve_with(
            &root,
            "session",
            "source",
            "assessed-no-learning",
            ledger::Options {
                assessment: Some(&reviewed),
                ..ledger::Options::default()
            },
        )
        .unwrap();
        assert_eq!(get(&complete, "publication_verified"), &Value::Bool(false));
        assert_eq!(
            text(&restarted(&"c".repeat(64)), "status"),
            "assessed-no-learning"
        );
        assert!(!directory.join(format!("{key}.intent")).exists());
        assert!(directory.join(format!("{key}.assessed")).exists());
        let wakes: Vec<String> = names(&directory)
            .into_iter()
            .filter(|n| *n == format!("{key}.wake-0") || *n == format!("{key}.wake-1"))
            .collect();
        assert_eq!(wakes.len(), 2);
    }
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_lock_removed_after_exclusive_create_conflict_is_not_recreated_or_hidden
#[test]
fn lock_removed_after_exclusive_create_conflict_is_not_recreated_or_hidden() {
    for action in ["wake", "inspect"] {
        let world = World::new().unwrap();
        let root = world.root.join("ledger");
        mkdir(&root);
        reserve(&root, "session", "source", "enroll").unwrap();
        let directory = session_directory(&root, "session");
        let key = turn_key("session", "source");
        let lock = directory.join(format!("{key}.wake-lock"));
        private_write(&lock, b"");
        let request = ledger_request(
            &root,
            "session",
            "source",
            action,
            &format!(", \"fingerprint\": {}", quoted(&"a".repeat(64))),
        );
        let run = paused(
            &world,
            &["hook", "boundary-ledger"],
            &request,
            "boundary-ledger-wake-lock-reopen",
            || std::fs::remove_file(&lock).unwrap(),
        );
        assert_eq!(run.code, 1, "{action}: {run:?}");
        assert_eq!(run.stdout, LEDGER_PENDING);
        assert!(!lock.exists());
        assert!(directory.join(format!("{key}.intent")).exists());
        assert!(!directory.join(format!("{key}.wake-0")).exists());
    }
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_native_backlog_exposes_legacy_uncertainty_without_inventing_ids
#[test]
fn native_backlog_exposes_legacy_uncertainty_without_inventing_ids() {
    let root = Scratch::new();
    let path = root.path();
    reserve(path, "old", "legacy-source", "reserve").unwrap();
    let value = reserve(path, "new", "inventory", "backlog").unwrap();
    assert_eq!(get(&value, "legacy_pending_count"), &decode("1"));
    assert_eq!(get(&value, "sources"), &decode("[]"));
    let directory = session_directory(path, "old");
    let intent = directory.join(&with_suffix(&directory, ".intent")[0]);
    std::fs::write(&intent, "partial").unwrap();
    assert!(matches!(
        reserve(path, "new", "inventory", "backlog"),
        Err(ledger::Error::Value(_))
    ));
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_backlog_pagination_recovers_sources_beyond_first_sixteen
#[test]
fn backlog_pagination_recovers_sources_beyond_first_sixteen() {
    let root = Scratch::new();
    let path = root.path();
    for number in 0..21 {
        reserve(
            path,
            "original-session",
            &format!("source-{number:02}"),
            "enroll",
        )
        .unwrap();
    }
    let first = reserve(path, "current-session", "inventory", "backlog").unwrap();
    let offset = get(&first, "next_offset").clone();
    let second = reserve_with(
        path,
        "current-session",
        "inventory",
        "backlog",
        ledger::Options {
            offset: Some(&offset),
            ..ledger::Options::default()
        },
    )
    .unwrap();
    assert_eq!(get(&first, "pending_count"), &decode("21"));
    assert_eq!(get(&first, "next_offset"), &decode("16"));
    assert_eq!(get(&second, "next_offset"), &Value::Null);
    assert_eq!(
        get(&first, "inventory_sha256"),
        get(&second, "inventory_sha256")
    );
    let mut sources: Vec<String> = Vec::new();
    for page in [&first, &second] {
        for row in get(page, "sources").as_array().unwrap() {
            sources.push(text(row, "source_id").to_owned());
        }
    }
    let expected: Vec<String> = (0..21).map(|n| format!("source-{n:02}")).collect();
    assert_eq!(sources, expected);
    let negative = decode("-1");
    assert!(matches!(
        reserve_with(
            path,
            "current-session",
            "inventory",
            "backlog",
            ledger::Options {
                offset: Some(&negative),
                ..ledger::Options::default()
            },
        ),
        Err(ledger::Error::Value(_))
    ));
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_state_inside_checkout_or_common_git_is_rejected_before_writes
#[test]
fn state_inside_checkout_or_common_git_is_rejected_before_writes() {
    in_world(
        "state_inside_checkout_or_common_git_is_rejected_before_writes",
        |case| {
            let world_git = |repo: &Path, args: &[&str]| {
                let mut all = vec!["-C", repo.to_str().unwrap()];
                all.extend_from_slice(args);
                let run = finished(
                    process::run_for(
                        &spec(
                            &argv("/usr/bin/git", &all),
                            &case.env(),
                            &case.root,
                            Input::Null,
                            Duration::from_secs(60),
                        ),
                        Duration::from_secs(60),
                    )
                    .unwrap(),
                );
                assert_eq!(run.code, 0, "{run:?}");
            };
            let repo = case.root.join("repository");
            mkdir(&repo);
            world_git(&repo, &["init", "-q", "-b", "main"]);
            let bank = repo.join("memory-bank");
            mkdir(&bank);
            std::fs::write(bank.join("README.md"), "bank\n").unwrap();
            world_git(&repo, &["add", "."]);
            world_git(&repo, &["commit", "-q", "-m", "bank"]);
            let linked = case.root.join("linked-native-session");
            world_git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    "linked-native-session",
                    linked.to_str().unwrap(),
                ],
            );
            for (private, session_repo) in [
                (repo.clone(), repo.clone()),
                (repo.join(".git"), repo.clone()),
                (bank.clone(), linked.clone()),
            ] {
                let location = Value::String(session_repo.to_str().unwrap().into());
                let refused = reserve_with(
                    &private,
                    "native-session",
                    "native-source",
                    "enroll",
                    ledger::Options {
                        repo: Some(&location),
                        ..ledger::Options::default()
                    },
                );
                match refused {
                    Err(error) => {
                        assert!(error.to_string().contains("outside repository"), "{error}")
                    }
                    Ok(value) => panic!("accepted {value:?}"),
                }
                assert!(!private.join("experimental-boundary").exists());
            }
        },
    );
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_fifo_markers_and_locks_refuse_without_blocking
#[test]
fn fifo_markers_and_locks_refuse_without_blocking() {
    let world = World::new().unwrap();
    let root = world.root.join("ledger");
    mkdir(&root);
    reserve(&root, "session", "source", "enroll").unwrap();
    let directory = session_directory(&root, "session");
    let key = turn_key("session", "source");
    let intent = directory.join(format!("{key}.intent"));
    let original = std::fs::read(&intent).unwrap();
    for (suffix, action) in [
        (".intent", "inspect"),
        (".wake-lock", "wake"),
        (".assessed", "inspect"),
    ] {
        let path = directory.join(format!("{key}{suffix}"));
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        let made = finished(
            process::run_for(
                &spec(
                    &argv("/usr/bin/mkfifo", &["-m", "600", path.to_str().unwrap()]),
                    &world_env(&world, &[]),
                    &world.root,
                    Input::Null,
                    Duration::from_secs(10),
                ),
                Duration::from_secs(10),
            )
            .unwrap(),
        );
        assert_eq!(made.code, 0, "{made:?}");
        let request = ledger_request(
            &root,
            "session",
            "source",
            action,
            &format!(", \"fingerprint\": {}", quoted(&"a".repeat(64))),
        );
        let run = ledger_run(&world, &request, &[]);
        assert_ne!(run.code, 0, "{suffix}: {run:?}");
        std::fs::remove_file(&path).unwrap();
        if suffix == ".intent" {
            private_write(&intent, &original);
        }
    }
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_new_and_existing_enrollment_verify_namespace_before_acceptance
#[test]
fn new_and_existing_enrollment_verify_namespace_before_acceptance() {
    for existing in [false, true] {
        let world = World::new().unwrap();
        let root = world.root.join("ledger");
        mkdir(&root);
        if existing {
            reserve(&root, "session", "source", "enroll").unwrap();
        }
        let request = ledger_request(&root, "session", "source", "enroll", "");
        let run = paused(
            &world,
            &["hook", "boundary-ledger"],
            &request,
            "boundary-ledger-enroll-verify",
            || {
                std::fs::rename(
                    root.join("experimental-boundary"),
                    root.join("original-state"),
                )
                .unwrap();
                mkdir(&root.join("experimental-boundary"));
            },
        );
        assert_eq!(run.code, 1, "{run:?}");
        assert_eq!(run.stdout, LEDGER_PENDING);
        let moved = session_directory(&root, "session")
            .strip_prefix(root.join("experimental-boundary"))
            .map(|rest| root.join("original-state").join(rest))
            .unwrap();
        assert_eq!(with_suffix(&moved, ".intent").len(), 1);
    }
}

// obligation: tests/contracts/test_opencode_native_boundary.py::NativeLedger.test_actual_plugin_enrolls_before_context_queues_once_and_recovers_native_session
#[test]
#[ignore = "needs recovery_frontier, memory_refresh registered"]
fn actual_plugin_enrolls_before_context_queues_once_and_recovers_native_session() {
    in_world(
        "actual_plugin_enrolls_before_context_queues_once_and_recovers_native_session",
        |case| {
            let (repo, _) = case.bank();
            // The installed layout: <runtime>/bin/am beside <runtime>/skills/agent-memory.
            let runtime = case.root.join("data/agent-memory/runtime");
            let hooks_dir = runtime.join("skills/agent-memory/hooks");
            mkdir(&hooks_dir);
            mkdir(&runtime.join("bin"));
            std::os::unix::fs::symlink(AM, runtime.join("bin/am")).unwrap();
            for name in [
                "opencode-v2.js",
                "opencode-v2-queue.js",
                "boundary-prompt.txt",
            ] {
                std::fs::copy(pack_hooks().join(name), hooks_dir.join(name)).unwrap();
            }
            let code = r#"
import {pathToFileURL} from 'node:url';
const plugin=(await import(pathToFileURL(process.argv[1]))).default;
const repo=process.argv[2],hooks={},tools={},queues=[],sessions=new Map();
const ctx={tool:{transform:async fn=>{fn({namespace(){},add(x){tools[x.name]=x.execute}});return{dispose(){}}}},session:{
 hook:async(name,fn)=>{const prior=hooks[name];hooks[name]=async event=>{if(prior)await prior(event);await fn(event)};return{dispose(){}}},get:async()=>({location:{directory:repo}}),
 context:async({sessionID})=>sessions.get(sessionID),synthetic:async x=>{queues.push(x);return{id:x.id,payload:{text:x.text}}}
}};
const dispose=await plugin.setup(ctx);
const source={type:'user',id:'msg_original',text:'Explain the fixture value without creating memory.'};sessions.set('ses_original',[source]);
const prompt={messageID:source.id,sessionID:'ses_original'};await hooks.prompt(prompt);source.metadata=prompt.metadata;
const firstBacklog=JSON.parse((await tools.pending_sources({}, {sessionID:'ses_original'})).content);
await hooks.context({sessionID:'ses_original',messages:[{id:source.id}]});await hooks.context({sessionID:'ses_original',messages:[{id:source.id}]});
if(queues.length!==1||Buffer.byteLength(queues[0].text)>2048||!queues[0].text.includes('Preserve the substantive task answer'))throw Error('unbounded or duplicate queue');
const rows=sessions.get('ses_original');rows.push({type:'assistant',id:'answer',text:'The fixture value is seven.',finish:'stop',time:{completed:1}},
 {type:'synthetic',id:queues[0].id,text:queues[0].text,metadata:queues[0].metadata},{type:'assistant',id:'continuation'});
const quiet=await tools.source_ack({source_id:source.id,outcome:'no-learning',assessment:{source_id:source.id,source_review:'Reviewed the fixture explanation; no durable claim.',candidates:[]}}, {sessionID:'ses_original',messageID:'continuation'});
if(quiet.content!=='')throw Error('routine receipt body');
const ack=JSON.parse((await tools.source_ack({source_id:source.id,diagnostic:true,outcome:'no-learning',assessment:{source_id:source.id,source_review:'Reviewed the fixture explanation; no durable claim.',candidates:[]}}, {sessionID:'ses_original',messageID:'continuation'})).content);
if(ack.schema!=='agent-memory/turn-check-v1'||!ack.assessment_verified||!ack.refresh_verified||!ack.verified||ack.publication_verified)throw Error('false completion: '+JSON.stringify(ack));
const interrupted={type:'user',id:'msg_interrupted',text:'Inspect the temporary fixture.'};rows.push(interrupted);
const next={messageID:interrupted.id,sessionID:'ses_original'};await hooks.prompt(next);interrupted.metadata=next.metadata;
sessions.set('ses_later',[{type:'assistant',id:'later-assistant'}]);
const backlog=JSON.parse((await tools.pending_sources({}, {sessionID:'ses_later'})).content);
if(backlog.pending_count!==1||backlog.sources[0].session_id!=='ses_original'||backlog.sources[0].source_id!=='msg_interrupted')throw Error('native source lost');
const missing=JSON.parse((await tools.source_window({source_id:interrupted.id,recover:true,source_session_id:'ses_original'}, {sessionID:'ses_later',messageID:'later-assistant'})).content);
if(missing.status!=='pending')throw Error('interrupted source falsely complete');
rows.push({type:'assistant',id:'resumed-answer',text:'The fixture is temporary.',finish:'stop',time:{completed:2}});
const recovered=JSON.parse((await tools.source_ack({source_id:interrupted.id,recover:true,source_session_id:'ses_original',diagnostic:true,outcome:'no-learning',assessment:{source_id:interrupted.id,source_review:'Recovered original native source and resumed answer; no durable claim.',candidates:[]}}, {sessionID:'ses_later',messageID:'later-assistant'})).content);
if(!recovered.verified||!recovered.assessment_verified||recovered.session_id!=='ses_original'||recovered.source_id!==interrupted.id)throw Error('source reassigned');
await hooks.context({sessionID:'ses_original',messages:[{id:interrupted.id}]});
if(queues.length!==1)throw Error('completed source requeued');
dispose();process.stdout.write(JSON.stringify({firstBacklog,ack,backlog,missing,recovered,queues:queues.length,bytes:Buffer.byteLength(queues[0].text)}));
"#;
            let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|dir| dir.join("node"))
                .find(|path| path.exists())
                .expect("JavaScript runtime");
            let mut argv = argv(node.to_str().unwrap(), &["--input-type=module", "-e", code]);
            argv.push(hooks_dir.join("opencode-v2.js").into_os_string());
            argv.push(repo.clone().into_os_string());
            let run = finished(
                process::run_for(
                    &spec(
                        &argv,
                        &case.env(),
                        &case.root,
                        Input::Null,
                        Duration::from_secs(60),
                    ),
                    Duration::from_secs(60),
                )
                .unwrap(),
            );
            assert_eq!(run.code, 0, "{run:?}");
            let value = run.value();
            assert_eq!(get(&value, "queues"), &decode("1"));
            assert!(get(&value, "bytes").as_int().unwrap().as_u64().unwrap() <= 2048);
        },
    );
}

// ----- OpenCode queue (test_opencode_queue.py) -----

// obligation: tests/contracts/test_opencode_queue.py::OpenCodeQueueContract.test_queue_binds_distinct_turns_and_fails_closed_on_missing_sources
#[test]
fn queue_binds_distinct_turns_and_fails_closed_on_missing_sources() {
    let world = World::new().unwrap();
    let Some(node) = node(&world) else {
        eprintln!("JavaScript runtime unavailable; skipped as in the baseline");
        return;
    };
    let source = r#"
import {pathToFileURL} from 'node:url';
const mod=await import(pathToFileURL(process.argv[1]));
const user=(id,text)=>({id,type:'user',text});
const final=(id,text)=>({id,type:'assistant',finish:'stop',time:{completed:1},content:[{type:'text',text}]});
const rows=[user('u1','yellow'),final('a1','done'),user('u2','blue'),final('a2','done')];
const checks={first:JSON.parse(mod.sourceWindow(rows,'u1','s').content),
 second:JSON.parse(mod.sourceWindow(rows,'u2','s').content),
 missing:JSON.parse(mod.sourceWindow(rows,'missing','s').content),
 incomplete:JSON.parse(mod.sourceWindow([user('u1','yellow')],'u1','s').content),
 compacted:JSON.parse(mod.sourceWindow([user('u1','yellow'),{type:'compaction'}],'u1','s').content),
 oversized:JSON.parse(mod.sourceWindow([user('u1','x'.repeat(20000)),final('a1','done')],'u1','s').content)};
const truncatedMiddle={id:'mid',type:'assistant',content:[{type:'tool',state:{metadata:{truncated:true},output:'partial'}}]};
checks.truncatedMiddle=JSON.parse(mod.sourceWindow([user('u','decision'),truncatedMiddle,final('a','done')],'u','s').content);
checks.truncatedUser=JSON.parse(mod.sourceWindow([{...user('u','decision'),truncated:true},final('a','done')],'u','s').content);
checks.truncatedFinal=JSON.parse(mod.sourceWindow([user('u','decision'),{...final('a','done'),truncated:true}],'u','s').content);
const large=[user('u1','yellow'),{id:'i1',type:'assistant',content:'a'.repeat(9000)},{id:'i2',type:'assistant',content:'b'.repeat(9000)},final('a1','done')];
checks.manifest=JSON.parse(mod.sourceWindow(large,'u1','s').content);
checks.retrieved=JSON.parse(mod.sourceWindow(large,'u1','s','i1',checks.manifest.omitted_records[0].sha256).content);
checks.wrongHash=JSON.parse(mod.sourceWindow(large,'u1','s','i1','bad').content);
const huge=[user('u1','yellow'),{id:'i1',type:'assistant',content:'a'.repeat(20000)},final('a1','done')];
const hugeManifest=JSON.parse(mod.sourceWindow(huge,'u1','s').content);
checks.largeRecord=JSON.parse(mod.sourceWindow(huge,'u1','s','i1',hugeManifest.omitted_records[0].sha256).content);
let offset=0, chunks=[];do {const c=JSON.parse(mod.sourceWindow(huge,'u1','s','i1',hugeManifest.omitted_records[0].sha256,offset).content);chunks.push(Buffer.from(c.chunk,'base64'));offset=c.next_offset} while(offset!==null);
checks.chunked=JSON.parse(Buffer.concat(chunks).toString('utf8'));
checks.badOffset=JSON.parse(mod.sourceWindow(huge,'u1','s','i1',hugeManifest.omitted_records[0].sha256,999999).content);
let promptHook,contextHook,tool;const queued=new Map(), registered={};
const ctx={tool:{transform:async cb=>{cb({namespace(){},add(t){registered[t.name]=t;if(t.name==='source_window')tool=t}});return {dispose(){}}}},
 session:{hook:async(name,cb)=>{if(name==='prompt')promptHook=cb;else contextHook=cb;return {dispose(){}}},
 get:async()=>({location:{directory:process.env.TEST_REPO}}), synthetic:async(v)=>{if(!queued.has(v.id))queued.set(v.id,{id:v.id,payload:{text:v.text},metadata:v.metadata});return queued.get(v.id)},context:async()=>rows}};
await mod.default.setup(ctx);
const p1={sessionID:'s',messageID:'u1'};await promptHook(p1);rows[0].metadata=p1.metadata;
await contextHook({sessionID:'s',messages:[{id:'u1'}]});await contextHook({sessionID:'s',messages:[{id:'u1'}]});
const p2={sessionID:'s',messageID:'u2'};await promptHook(p2);rows[2].metadata=p2.metadata;
await contextHook({sessionID:'s',messages:[{id:'u2'}]});
const q1=queued.get(mod.syntheticId('s','u1')),q2=queued.get(mod.syntheticId('s','u2'));
rows.push({id:q1.id,type:'synthetic',metadata:q1.metadata},{id:'t1',type:'assistant'});
checks.mismatch=JSON.parse((await tool.execute({source_id:'u2'},{sessionID:'s',messageID:'t1'})).content);
checks.firstTool=JSON.parse((await tool.execute({source_id:'u1'},{sessionID:'s',messageID:'t1'})).content);
rows.push({id:q2.id,type:'synthetic',metadata:q2.metadata},{id:'t2',type:'assistant'});
checks.secondTool=JSON.parse((await tool.execute({source_id:'u2'},{sessionID:'s',messageID:'t2'})).content);
const q1pos=rows.findIndex(row=>row.id===q1.id);const removed=rows.splice(q1pos,1);
checks.recovered=JSON.parse((await tool.execute({source_id:'u1',recover:true},{sessionID:'s',messageID:'t2'})).content);rows.splice(q1pos,0,...removed);
if(registered.source_ack){checks.eligibleAck=JSON.parse((await registered.source_ack.execute({source_id:'u2',outcome:'no-learning',assessment:{source_id:'u2',source_review:'Reviewed source',candidates:[{claim:'Decision',disposition:'eligible',reason:'Durable'}]}},{sessionID:'s',messageID:'t2'})).content);checks.recoveryAck=JSON.parse((await registered.source_ack.execute({source_id:'u1',outcome:'no-learning',assessment:{source_id:'u1',source_review:'Reviewed source',candidates:[]},recover:true},{sessionID:'s',messageID:'t2'})).content);checks.ack=JSON.parse((await registered.source_ack.execute({source_id:'u2',outcome:'no-learning',assessment:{source_id:'u2',source_review:'Reviewed source',candidates:[]}},{sessionID:'s',messageID:'t2'})).content);checks.badAck=JSON.parse((await registered.source_ack.execute({source_id:'u1',outcome:'no-learning',assessment:{source_id:'u1',source_review:'Reviewed source',candidates:[]}},{sessionID:'s',messageID:'t2'})).content);checks.pubAck=JSON.parse((await registered.source_ack.execute({source_id:'u2',outcome:'published'},{sessionID:'s',messageID:'t2'})).content);}
checks.queued=queued.size;
process.stdout.write(JSON.stringify(checks));
"#;
    // The installed layout: the adapter spawns <runtime>/bin/am. A stand-in
    // binary answers like the baseline's stub ledger and checks its argv.
    let runtime = world.root.join("runtime");
    let hooks_dir = runtime.join("skills/agent-memory/hooks");
    mkdir(&hooks_dir);
    mkdir(&runtime.join("bin"));
    let target = hooks_dir.join("queue.mjs");
    std::fs::copy(pack_hooks().join("opencode-v2-queue.js"), &target).unwrap();
    let stub = format!(
        "#!{}\nimport('node:fs').then(fs=>{{const r=JSON.parse(fs.readFileSync(0,'utf8'));\nif(process.argv.slice(2).join(' ')!=='hook boundary-ledger'||r.repo!==process.env.TEST_REPO)process.exit(3);\nprocess.stdout.write(JSON.stringify({{status:r.action==='enroll'?'enrolled':r.action==='assessed-no-learning'?'assessed-no-learning':r.action==='inspect'?'pending-turn':'reserved'}})+'\\n')}})\n",
        node.display()
    );
    std::fs::write(runtime.join("bin/am"), stub).unwrap();
    std::fs::set_permissions(
        runtime.join("bin/am"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let state = world.root.join("second");
    mkdir(&state);
    let test_repo = world.root.to_str().unwrap().to_owned();
    let run = run_node(
        &world,
        source,
        &[&target],
        &[
            ("AGENT_MEMORY_STATE_HOME", state.to_str().unwrap()),
            ("TEST_REPO", &test_repo),
        ],
    );
    assert_eq!(run.code, 0, "{run:?}");
    let result = run.value();
    let check = |name: &str| get(&result, name).clone();
    assert_eq!(check("queued"), decode("2"));
    assert_eq!(text(&check("ack"), "status"), "assessed-no-learning");
    assert_eq!(text(&check("eligibleAck"), "status"), "pending");
    assert_eq!(text(&check("recovered"), "source_id"), "u1");
    assert_eq!(text(&check("recoveryAck"), "source_id"), "u1");
    assert_eq!(text(&check("badAck"), "status"), "pending");
    assert_eq!(
        text(&check("pubAck"), "reason"),
        "session-repository-unavailable"
    );
    let middle = check("truncatedMiddle");
    assert_eq!(text(&middle, "status"), "observed");
    assert_eq!(
        text(
            &get(&middle, "truncated_records").as_array().unwrap()[0],
            "id"
        ),
        "mid"
    );
    assert_eq!(
        get(&middle, "source_completeness_verified"),
        &Value::Bool(false)
    );
    assert!(text(&middle, "evidence_instruction").contains("if unavailable retain pending"));
    assert_eq!(text(&check("truncatedUser"), "reason"), "source-incomplete");
    assert_eq!(
        text(&check("truncatedFinal"), "reason"),
        "source-incomplete"
    );
    assert_eq!(text(&check("first"), "final_id"), "a1");
    assert_eq!(text(&check("second"), "final_id"), "a2");
    assert_eq!(text(&check("first"), "status"), "observed");
    assert_eq!(
        get(&check("first"), "source_completeness_verified"),
        &Value::Bool(false)
    );
    assert_eq!(text(&check("firstTool"), "session_id"), "s");
    assert_eq!(text(&check("secondTool"), "source_id"), "u2");
    assert_eq!(
        get(&check("manifest"), "omitted_records")
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        get(&check("manifest"), "source_completeness_verified"),
        &Value::Bool(false)
    );
    assert_eq!(text(get(&check("retrieved"), "record"), "id"), "i1");
    assert_eq!(text(&check("wrongHash"), "status"), "pending");
    assert_eq!(text(&check("largeRecord"), "reason"), "record-too-large");
    assert_eq!(text(&check("chunked"), "content").len(), 20000);
    assert_eq!(text(&check("badOffset"), "reason"), "record-offset-invalid");
    for name in [
        "missing",
        "incomplete",
        "compacted",
        "oversized",
        "mismatch",
    ] {
        assert_eq!(text(&check(name), "status"), "pending", "{name}");
        assert_eq!(
            get(&check(name), "publication_verified"),
            &Value::Bool(false)
        );
    }
    let probe = "import {pathToFileURL} from 'node:url'; const m=await import(pathToFileURL(process.argv[1])); const r=[{id:'u',type:'user',text:'x'},{id:'a',type:'assistant',finish:'stop',time:{completed:1},content:[{type:'tool',output:{truncated:true}}]}]; process.stdout.write(m.sourceWindow(r,'u','s').content)";
    let truncated = run_node(&world, probe, &[&target], &[]);
    assert_eq!(text(&truncated.value(), "reason"), "source-incomplete");
}

// ----- Codex/Claude hooks over a bank (test_hook_failure.py and others) -----

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_bank_absence_clears_only_its_own_adapter
#[test]
fn bank_absence_clears_only_its_own_adapter() {
    in_world("bank_absence_clears_only_its_own_adapter", |case| {
        let repo = case.root.join("repository");
        mkdir(&repo);
        let init = finished(
            process::run_for(
                &spec(
                    &argv(
                        "/usr/bin/git",
                        &["-C", repo.to_str().unwrap(), "init", "-q"],
                    ),
                    &case.env(),
                    &case.root,
                    Input::Null,
                    Duration::from_secs(60),
                ),
                Duration::from_secs(60),
            )
            .unwrap(),
        );
        assert_eq!(init.code, 0, "{init:?}");
        for adapter in ["prompt", "stop"] {
            agent_memory::hook_failure::record(
                &repo,
                "codex",
                adapter,
                "identity",
                "identity.unresolved",
            )
            .unwrap();
        }
        let event = format!(
            "{{\"hook_event_name\": \"UserPromptSubmit\", \"session_id\": \"{}\", \"turn_id\": \"{}\", \"cwd\": {}, \"prompt\": \"no bank\"}}",
            agent_memory::host::uuid4().unwrap(),
            agent_memory::host::uuid4().unwrap(),
            quoted(repo.to_str().unwrap())
        );
        let run = case.hook("codex-prompt", &event);
        assert_eq!(run.code, 0, "{run:?}");
        assert_eq!(run.stdout, "{}\n");
        let adapters: Vec<String> = agent_memory::hook_failure::read(&repo, None)
            .unwrap()
            .iter()
            .map(|row| text(row, "adapter").to_owned())
            .collect();
        assert_eq!(adapters, ["stop"]);
        let stop = format!(
            "{{\"hook_event_name\": \"Stop\", \"session_id\": \"{}\", \"turn_id\": \"{}\", \"cwd\": {}, \"stop_hook_active\": false}}",
            agent_memory::host::uuid4().unwrap(),
            agent_memory::host::uuid4().unwrap(),
            quoted(repo.to_str().unwrap())
        );
        let run = case.hook("codex-stop", &stop);
        assert_eq!(run.stdout, "{}\n");
        assert!(
            agent_memory::hook_failure::read(&repo, None)
                .unwrap()
                .is_empty()
        );
    });
}

/// Every hook outside a repository or bank is quiet; malformed events take
/// each hook's fixed failure form; no hook writes outside its private state.
#[test]
fn every_hook_command_runs_through_the_real_binary() {
    let world = World::new().unwrap();
    let outside = world.root.join("outside");
    mkdir(&outside);
    let cwd = quoted(outside.to_str().unwrap());
    let session = agent_memory::host::uuid4().unwrap();
    let turn = agent_memory::host::uuid4().unwrap();
    let cases = [
        (
            "claude-prompt",
            format!(
                "{{\"hook_event_name\": \"UserPromptSubmit\", \"cwd\": {cwd}, \"session_id\": \"{session}\", \"prompt_id\": \"{turn}\", \"prompt\": \"x\", \"transcript_path\": \"/t.jsonl\"}}"
            ),
            "{}\n",
        ),
        (
            "turn-hook",
            format!(
                "{{\"hook_event_name\": \"UserPromptSubmit\", \"cwd\": {cwd}, \"session_id\": \"{session}\", \"prompt_id\": \"{turn}\", \"prompt\": \"x\", \"transcript_path\": \"/t.jsonl\"}}"
            ),
            "{}\n",
        ),
        (
            "claude-stop",
            format!(
                "{{\"hook_event_name\": \"Stop\", \"stop_hook_active\": false, \"cwd\": {cwd}}}"
            ),
            "",
        ),
        (
            "codex-prompt",
            format!(
                "{{\"hook_event_name\": \"UserPromptSubmit\", \"cwd\": {cwd}, \"session_id\": \"{session}\", \"turn_id\": \"{turn}\", \"prompt\": \"x\"}}"
            ),
            "{}\n",
        ),
        (
            "codex-stop",
            format!(
                "{{\"hook_event_name\": \"Stop\", \"stop_hook_active\": false, \"cwd\": {cwd}, \"session_id\": \"{session}\", \"turn_id\": \"{turn}\"}}"
            ),
            "{}\n",
        ),
    ];
    for (name, event, expected) in cases {
        let run = hook(&world, name, &event);
        assert_eq!(
            (run.code, run.stdout.as_str(), run.stderr.as_str()),
            (0, expected, ""),
            "{name}"
        );
    }
    let invalid = [
        ("claude-prompt", PROMPT_BLOCKED.to_owned()),
        ("claude-stop", STOP_FAILED.to_owned()),
        (
            "codex-prompt",
            "{\"decision\": \"block\", \"reason\": \"Agent Memory input not accepted [event/event.invalid]. Preserve work; run am doctor and am codex-boundary --list-pending for this repository, then retry this input after resolving the fault.\"}\n".to_owned(),
        ),
        (
            "codex-stop",
            "{\"systemMessage\": \"Agent Memory pending [event/event.invalid]. Preserve work; inspect am doctor and the retained original sources.\"}\n".to_owned(),
        ),
        ("turn-hook", TURN_FAILED.to_owned()),
    ];
    for (name, expected) in invalid {
        let run = hook(&world, name, "{\"hook_event_name\": \"Other\"}");
        assert_eq!(
            (run.code, run.stdout.as_str()),
            (0, expected.as_str()),
            "{name}"
        );
    }
    let ledger = hook(&world, "boundary-ledger", "[]");
    assert_eq!((ledger.code, ledger.stdout.as_str()), (1, LEDGER_PENDING));
    let unknown = hook(&world, "not-a-hook", "{}");
    assert_eq!(unknown.code, 2);
    assert!(unknown.stderr.contains("invalid choice"), "{unknown:?}");
}

/// Hooks that reach private source state need the installed runtime's pack;
/// an executable that is not the installed runtime fails closed in each
/// hook's fixed form without touching the bank.
#[test]
fn uninstalled_executable_fails_closed_over_a_bank() {
    in_world("uninstalled_executable_fails_closed_over_a_bank", |case| {
        // Only the bank's presence matters: the runtime pack is required first.
        let repo = case.root.join("repository");
        let bank = repo.join("memory-bank");
        mkdir(&bank);
        git(case, &repo, &["init", "-q"]);
        std::fs::write(bank.join("projectbrief.md"), "# Project\n").unwrap();
        let before = tree(&repo);
        let cwd = quoted(repo.to_str().unwrap());
        let session = agent_memory::host::uuid4().unwrap();
        let turn = agent_memory::host::uuid4().unwrap();
        let prompt = case.hook(
            "codex-prompt",
            &format!("{{\"hook_event_name\": \"UserPromptSubmit\", \"cwd\": {cwd}, \"session_id\": \"{session}\", \"turn_id\": \"{turn}\", \"prompt\": \"SECRET PROMPT\"}}"),
        );
        assert_eq!(text(&prompt.value(), "decision"), "block");
        assert!(text(&prompt.value(), "reason").contains("[boundary/state.unavailable]"));
        let stop = case.hook(
            "codex-stop",
            &format!("{{\"hook_event_name\": \"Stop\", \"stop_hook_active\": false, \"cwd\": {cwd}, \"session_id\": \"{session}\", \"turn_id\": \"{turn}\"}}"),
        );
        assert!(text(&stop.value(), "systemMessage").contains("[boundary/state.unavailable]"));
        let records = agent_memory::hook_failure::read(&repo, None).unwrap();
        assert_eq!(records.len(), 2);
        assert!(
            !json::encode(&Value::Array(records), json::DEFAULT)
                .unwrap()
                .contains("SECRET")
        );
        let claude = case.hook(
            "claude-prompt",
            &format!("{{\"hook_event_name\": \"UserPromptSubmit\", \"cwd\": {cwd}, \"session_id\": \"{session}\", \"prompt_id\": \"{turn}\", \"prompt\": \"x\", \"transcript_path\": \"/t.jsonl\"}}"),
        );
        assert_eq!(claude.stdout, PROMPT_BLOCKED);
        let claude_stop = case.hook(
            "claude-stop",
            &format!("{{\"hook_event_name\": \"Stop\", \"stop_hook_active\": false, \"cwd\": {cwd}, \"session_id\": \"{session}\", \"prompt_id\": \"{turn}\"}}"),
        );
        assert_eq!(claude_stop.stdout, STOP_FAILED);
        assert_eq!(tree(&repo), before);
    });
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_hook_failure_codes_survive_without_source_content
#[test]
#[ignore = "needs recovery_frontier registered"]
fn hook_failure_codes_survive_without_source_content() {
    in_world(
        "hook_failure_codes_survive_without_source_content",
        |case| {
            let (repo, _) = case.bank();
            let session = agent_memory::host::uuid4().unwrap();
            let turn = agent_memory::host::uuid4().unwrap();
            let run = |name: &str, event: &str| {
                let output = case.library(name, event);
                assert_eq!(output.exit_code, 0);
                decode(&output.stdout)
            };
            let stop = format!(
                "{{\"hook_event_name\": \"Stop\", \"session_id\": \"{session}\", \"turn_id\": \"{turn}\", \"stop_hook_active\": false, \"cwd\": {}, \"last_assistant_message\": \"SECRET FINAL MUST NOT BE RETAINED\"}}",
                quoted(repo.to_str().unwrap())
            );
            assert!(
                text(&run("codex-stop", "{}"), "systemMessage").contains("event/event.invalid")
            );
            assert!(
                text(&run("codex-stop", &stop), "systemMessage")
                    .contains("boundary/boundary.missing")
            );
            let records = agent_memory::hook_failure::read(&repo, None).unwrap();
            assert_eq!(text(&records[0], "reason"), "boundary.missing");
            let prompt = stop.replace("\"Stop\"", "\"UserPromptSubmit\"").replace(
                "\"stop_hook_active\": false",
                "\"prompt\": \"SECRET PROMPT MUST NOT BE RETAINED\"",
            );
            assert!(
                run("codex-prompt", &prompt)
                    .get("hookSpecificOutput")
                    .is_some()
            );
            assert_eq!(
                agent_memory::hook_failure::read(&repo, None).unwrap().len(),
                1
            );
            assert_eq!(text(&run("codex-stop", &stop), "decision"), "block");
            assert!(
                agent_memory::hook_failure::read(&repo, None)
                    .unwrap()
                    .is_empty()
            );
            let changed = prompt.replace("SECRET PROMPT MUST NOT BE RETAINED", "DIFFERENT SECRET");
            assert!(
                run("codex-prompt", &changed)
                    .get("hookSpecificOutput")
                    .is_some()
            );
            let rendered = json::encode(
                &Value::Array(agent_memory::hook_failure::read(&repo, None).unwrap()),
                json::DEFAULT,
            )
            .unwrap();
            assert!(!rendered.contains("SECRET"));
        },
    );
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_unresolved_identity_retains_fixed_metadata_without_bank_change
#[test]
#[ignore = "needs recovery_frontier registered"]
fn unresolved_identity_retains_fixed_metadata_without_bank_change() {
    in_world(
        "unresolved_identity_retains_fixed_metadata_without_bank_change",
        |case| {
            let (repo, bank) = case.bank();
            let before = case.snapshot(&repo, &bank);
            let registry = case.root.join("config/agent-memory/projects.json");
            let mut value = decode(&std::fs::read_to_string(&registry).unwrap());
            *value.get_mut("checkouts").unwrap() = Value::Object(Vec::new());
            std::fs::write(&registry, json::encode(&value, json::DEFAULT).unwrap()).unwrap();
            let event = format!(
                "{{\"hook_event_name\": \"UserPromptSubmit\", \"session_id\": \"{}\", \"turn_id\": \"{}\", \"cwd\": {}, \"prompt\": \"SECRET PROMPT\"}}",
                agent_memory::host::uuid4().unwrap(),
                agent_memory::host::uuid4().unwrap(),
                quoted(repo.to_str().unwrap())
            );
            let output = case.library("codex-prompt", &event);
            let response = decode(&output.stdout);
            assert_eq!(text(&response, "decision"), "block");
            assert!(!output.stdout.contains("SECRET"));
            let records = agent_memory::hook_failure::read(&repo, None).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(
                (text(&records[0], "stage"), text(&records[0], "reason")),
                ("identity", "identity.unresolved")
            );
            assert!(text(&response, "reason").contains("[identity/identity.unresolved]"));
            let stop = event.replace("\"UserPromptSubmit\"", "\"Stop\"").replace(
                "\"prompt\": \"SECRET PROMPT\"",
                "\"stop_hook_active\": false, \"last_assistant_message\": \"SECRET FINAL\"",
            );
            let output = case.library("codex-stop", &stop);
            assert!(!output.stdout.contains("SECRET"));
            let records = agent_memory::hook_failure::read(&repo, None).unwrap();
            let record = records
                .iter()
                .find(|row| text(row, "adapter") == "stop")
                .unwrap();
            assert_eq!(
                (text(record, "stage"), text(record, "reason")),
                ("identity", "identity.unresolved")
            );
            assert!(
                text(&decode(&output.stdout), "systemMessage")
                    .contains("[identity/identity.unresolved]")
            );
            assert_eq!(case.snapshot(&repo, &bank), before);
        },
    );
}

// ----- native Claude sources (test_claude_source_turns.py) -----

fn claude_event(repo: &Path, session: &str, prompt: &str, transcript: &Path, text: &str) -> String {
    format!(
        "{{\"hook_event_name\": \"UserPromptSubmit\", \"session_id\": \"{session}\", \"prompt_id\": \"{prompt}\", \"cwd\": {}, \"transcript_path\": {}, \"prompt\": {}}}",
        quoted(repo.to_str().unwrap()),
        quoted(transcript.to_str().unwrap()),
        quoted(text)
    )
}

// obligation: tests/contracts/test_claude_source_turns.py::ClaudeSourceTurns.test_enroll_before_transcript_source_assertion_then_quiet_nullable_stop
#[test]
#[ignore = "needs recovery_frontier registered"]
fn enroll_before_transcript_source_assertion_then_quiet_nullable_stop() {
    in_world(
        "enroll_before_transcript_source_assertion_then_quiet_nullable_stop",
        |case| {
            let (repo, _) = case.bank();
            let runtime = case.runtime();
            let (session, prompt) = (
                agent_memory::host::uuid4().unwrap(),
                agent_memory::host::uuid4().unwrap(),
            );
            let transcript = case.root.join("not-yet-written.jsonl");
            let question = "Fixture status question";
            let enrolled = case.library(
                "turn-hook",
                &claude_event(&repo, &session, &prompt, &transcript, question),
            );
            assert_eq!(enrolled.exit_code, 0);
            let context = text(
                get(&decode(&enrolled.stdout), "hookSpecificOutput"),
                "additionalContext",
            )
            .to_owned();
            assert!(context.contains("observation_id="));
            assert!(!transcript.exists());
            let listed = agent_memory::claude_turns::pending(
                &repo,
                Some(&session),
                Some(&prompt),
                (0, 0, 0),
                &runtime,
            )
            .unwrap();
            let pending = get(&listed, "pending").as_array().unwrap();
            assert_eq!(pending.len(), 1);
            let observation = text(&pending[0], "observation_id").to_owned();
            let request = decode(&format!(
                "{{\"session\": \"{session}\", \"prompt\": \"{prompt}\", \"observation_id\": \"{observation}\", \"source_sha256\": \"{}\", \"outcome\": \"no-learning\", \"assessment\": {{\"source_id\": \"{observation}\", \"source_review\": \"Reviewed the fixture status question; no durable learning.\", \"candidates\": []}}}}",
                sha256(question.as_bytes())
            ));
            agent_memory::claude_turns::complete(&repo, &request, &runtime).unwrap();
            let stop = claude_event(&repo, &session, &prompt, &transcript, question)
            .replace("\"UserPromptSubmit\"", "\"Stop\"")
            .replace("\"prompt\": \"Fixture", "\"stop_hook_active\": false, \"last_assistant_message\": null, \"prompt\": \"Fixture");
            let stopped = case.library("claude-stop", &stop);
            assert_eq!(
                (
                    stopped.exit_code,
                    stopped.stdout.as_str(),
                    stopped.stderr.as_str()
                ),
                (0, "", "")
            );
        },
    );
}

// obligation: tests/contracts/test_claude_source_turns.py::ClaudeSourceTurns.test_shared_native_prompt_tracks_three_distinct_observations_without_transcript_projection
#[test]
#[ignore = "needs recovery_frontier registered"]
fn shared_native_prompt_tracks_three_distinct_observations_without_transcript_projection() {
    in_world(
        "shared_native_prompt_tracks_three_distinct_observations_without_transcript_projection",
        |case| {
            let (repo, _) = case.bank();
            let runtime = case.runtime();
            let (session, prompt) = (
                agent_memory::host::uuid4().unwrap(),
                agent_memory::host::uuid4().unwrap(),
            );
            let transcript = case.root.join("native-queued.jsonl");
            let mut observed: Vec<(String, String)> = Vec::new();
            for question in [
                "Fixture question one",
                "Fixture question two",
                "Fixture question two",
            ] {
                let output = case.library(
                    "claude-prompt",
                    &claude_event(&repo, &session, &prompt, &transcript, question),
                );
                assert!(decode(&output.stdout).get("hookSpecificOutput").is_some());
                let listed = agent_memory::claude_turns::pending(
                    &repo,
                    Some(&session),
                    Some(&prompt),
                    (0, 0, 0),
                    &runtime,
                )
                .unwrap();
                let new = get(&listed, "pending")
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| text(row, "observation_id").to_owned())
                    .find(|id| !observed.iter().any(|(known, _)| known == id))
                    .unwrap();
                observed.push((new, question.to_owned()));
            }
            assert_eq!(observed.len(), 3);
            for (index, (observation, question)) in observed.iter().enumerate() {
                let request = |digest: &str| {
                    decode(&format!(
                        "{{\"session\": \"{session}\", \"prompt\": \"{prompt}\", \"observation_id\": \"{observation}\", \"source_sha256\": \"{digest}\", \"outcome\": \"no-learning\", \"assessment\": {{\"source_id\": \"{observation}\", \"source_review\": \"Reviewed this exact disposable fixture question; no durable project learning.\", \"candidates\": []}}}}"
                    ))
                };
                assert!(
                    agent_memory::claude_turns::complete(
                        &repo,
                        &request(&"0".repeat(64)),
                        &runtime
                    )
                    .is_err()
                );
                agent_memory::claude_turns::complete(
                    &repo,
                    &request(&sha256(question.as_bytes())),
                    &runtime,
                )
                .unwrap();
                let listed = agent_memory::claude_turns::pending(
                    &repo,
                    Some(&session),
                    Some(&prompt),
                    (0, 0, 0),
                    &runtime,
                )
                .unwrap();
                assert_eq!(get(&listed, "pending").as_array().unwrap().len(), 2 - index);
            }
            let stop = claude_event(&repo, &session, &prompt, &transcript, "")
                .replace("\"UserPromptSubmit\"", "\"Stop\"")
                .replace(
                    "\"prompt\": \"\"",
                    "\"stop_hook_active\": false, \"last_assistant_message\": null",
                );
            let stopped = case.library("claude-stop", &stop);
            assert_eq!(
                (
                    stopped.exit_code,
                    stopped.stdout.as_str(),
                    stopped.stderr.as_str()
                ),
                (0, "", "")
            );
        },
    );
}

// ----- client hook configuration (design row 'Client hook commands') -----

fn fixture_config(client: &str, file: &str, pack: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v014-state/all-copy")
        .join(client)
        .join(file);
    std::fs::read_to_string(path)
        .unwrap()
        .replace("@WORLD@/data/agent-memory/source/skills/agent-memory", pack)
}

#[test]
fn captured_python_era_hook_commands_are_recognized_exactly() {
    for (client, file) in [("claude", "settings.json"), ("codex", "hooks.json")] {
        let raw = fixture_config(client, file, "/owned/pack");
        let document = client_hooks::parse(raw.as_bytes()).unwrap();
        let (prompt, stop) =
            client_hooks::python_era_entries(client, Path::new("/owned/pack")).unwrap();
        assert_eq!(
            client_hooks::locate(&document, &prompt, client_hooks::EVENT).unwrap(),
            0
        );
        assert_eq!(client_hooks::locate(&document, &stop, "Stop").unwrap(), 0);
        // A different pack or launcher is a foreign entry, never rewritten.
        let (other, _) =
            client_hooks::python_era_entries(client, Path::new("/other/pack")).unwrap();
        assert_eq!(
            client_hooks::locate(&document, &other, client_hooks::EVENT)
                .unwrap_err()
                .message(),
            "owned_group_missing_or_changed"
        );
    }
}

#[test]
fn python_era_entries_rewrite_to_rust_commands_preserving_unrelated_configuration() {
    let pack = Path::new("/owned/pack");
    let binary = Path::new("/owned runtime/bin/am");
    let (old_prompt, old_stop) = client_hooks::python_era_entries("claude", pack).unwrap();
    let python_prompt = "/bin/sh /owned/pack/hooks/python3.sh /owned/pack/hooks/turn_hook.py";
    let python_stop = "/bin/sh /owned/pack/hooks/python3.sh /owned/pack/hooks/claude_stop.py";
    let raw = format!(
        "{{\"model\": \"opus\", \"hooks\": {{\"UserPromptSubmit\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"printf unrelated\"}}]}}, {{\"hooks\": [{{\"type\": \"command\", \"command\": \"{python_prompt}\"}}]}}], \"Stop\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{python_stop}\", \"asyncRewake\": true}}]}}], \"PreToolUse\": [{{\"matcher\": \"Bash\"}}]}}, \"n\u{e4}me\": 1e-05, \"big\": 12345678901234567890}}"
    );
    let (new_prompt, new_stop) =
        agent_memory::installation::client_hook_entries("claude", binary).unwrap();
    let merged = client_hooks::merge(
        raw.as_bytes(),
        Some(&old_prompt),
        &new_prompt,
        client_hooks::EVENT,
    )
    .unwrap();
    let merged = client_hooks::merge(&merged, Some(&old_stop), &new_stop, "Stop").unwrap();
    let expected = "{\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"UserPromptSubmit\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"printf unrelated\"\n          }\n        ]\n      },\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"'/owned runtime/bin/am' hook turn-hook\"\n          }\n        ]\n      }\n    ],\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"'/owned runtime/bin/am' hook claude-stop\",\n            \"asyncRewake\": true\n          }\n        ]\n      }\n    ],\n    \"PreToolUse\": [\n      {\n        \"matcher\": \"Bash\"\n      }\n    ]\n  },\n  \"n\u{e4}me\": 1e-05,\n  \"big\": 12345678901234567890\n}\n";
    assert_eq!(String::from_utf8(merged).unwrap(), expected);
    assert!(!expected.contains("python3.sh") && !expected.contains(".py"));
    // Codex: both owned groups become `am hook codex-*`; nothing else changes.
    let raw = fixture_config("codex", "hooks.json", "/owned/pack");
    let (old_prompt, old_stop) = client_hooks::python_era_entries("codex", pack).unwrap();
    let (new_prompt, new_stop) =
        agent_memory::installation::client_hook_entries("codex", binary).unwrap();
    let merged = client_hooks::merge(
        raw.as_bytes(),
        Some(&old_prompt),
        &new_prompt,
        client_hooks::EVENT,
    )
    .unwrap();
    let merged = String::from_utf8(
        client_hooks::merge(&merged, Some(&old_stop), &new_stop, "Stop").unwrap(),
    )
    .unwrap();
    assert!(merged.contains("\"command\": \"'/owned runtime/bin/am' hook codex-prompt\""));
    assert!(merged.contains("\"command\": \"'/owned runtime/bin/am' hook codex-stop\""));
    assert!(!merged.contains("python3.sh"));
}

/// The generated owned commands run the real binary through the client's
/// shell: each names a declared hook and answers its harness event.
#[test]
fn generated_hook_commands_execute_the_native_hooks() {
    let world = World::new().unwrap();
    let outside = world.root.join("outside");
    mkdir(&outside);
    let cwd = quoted(outside.to_str().unwrap());
    let session = agent_memory::host::uuid4().unwrap();
    let turn = agent_memory::host::uuid4().unwrap();
    for client in ["claude", "codex"] {
        let (prompt, stop) =
            agent_memory::installation::client_hook_entries(client, Path::new(AM)).unwrap();
        for (entry, event) in [
            (
                prompt,
                format!(
                    "{{\"hook_event_name\": \"UserPromptSubmit\", \"cwd\": {cwd}, \"session_id\": \"{session}\", \"turn_id\": \"{turn}\", \"prompt_id\": \"{turn}\", \"prompt\": \"x\", \"transcript_path\": \"/t.jsonl\"}}"
                ),
            ),
            (
                stop,
                format!(
                    "{{\"hook_event_name\": \"Stop\", \"stop_hook_active\": false, \"cwd\": {cwd}, \"session_id\": \"{session}\", \"turn_id\": \"{turn}\"}}"
                ),
            ),
        ] {
            let command = text(&get(&entry, "hooks").as_array().unwrap()[0], "command").to_owned();
            let name = command.rsplit(' ').next().unwrap();
            assert!(hooks::NAMES.contains(&name), "{command}");
            let shell = argv("/bin/sh", &["-c", &command]);
            let env = world_env(&world, &[]);
            let run = finished(
                process::run_for(
                    &spec(
                        &shell,
                        &env,
                        &world.root,
                        Input::Bytes(event.as_bytes()),
                        Duration::from_secs(60),
                    ),
                    Duration::from_secs(60),
                )
                .unwrap(),
            );
            assert_eq!(run.code, 0, "{command}: {run:?}");
            assert!(
                run.stdout == "{}\n" || run.stdout.is_empty(),
                "{command}: {run:?}"
            );
        }
    }
}

/// No adapter in the pack starts Python; pack validation refuses one that does.
#[test]
fn adapters_spawn_only_the_am_binary() {
    for name in [
        "omp.ts",
        "opencode.js",
        "opencode-v1.js",
        "opencode-v2.js",
        "opencode-v2-queue.js",
    ] {
        let source = std::fs::read_to_string(pack_hooks().join(name)).unwrap();
        assert!(
            !source.contains("python") && !source.contains(".py'"),
            "{name}"
        );
    }
    let omp = std::fs::read_to_string(pack_hooks().join("omp.ts")).unwrap();
    assert!(omp.contains("spawn(binding!.executable, ['omp-boundary', '--stream'"));
    let queue = std::fs::read_to_string(pack_hooks().join("opencode-v2-queue.js")).unwrap();
    assert!(queue.contains("execFile(executable, ['hook', 'boundary-ledger']"));
}

#[test]
fn pack_validation_refuses_python_hook_adapters() {
    let surface = agent_memory::cli::surface::Declared::current().unwrap();
    let validate = |adapter: &str| {
        let dir = Scratch::new();
        let pack = dir.path().join("pack");
        mkdir(&pack.join("hooks"));
        std::fs::write(
            pack.join("SKILL.md"),
            "---\nname: agent-memory\ndescription: Native hook adapter contract\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n\nNo command.\n",
        )
        .unwrap();
        std::fs::write(pack.join("hooks/adapter.js"), adapter).unwrap();
        agent_memory::pack_validation::validate_with_surface(&pack, Some(&surface))
    };
    validate("execFile(executable, ['hook', 'boundary-ledger'])\n").unwrap();
    for adapter in [
        "execFile('/bin/sh', [join(hooks, 'python3.sh'), helper])\n",
        "const helper = join(pack, 'scripts', 'omp_boundary.py')\n",
    ] {
        let issue = validate(adapter).unwrap_err();
        assert_eq!(issue.code, "dispatch");
        assert_eq!(issue.path, "hooks/adapter.js");
        assert_eq!(
            issue.detail,
            "Client hook adapters must invoke the am binary, never Python"
        );
    }
}
