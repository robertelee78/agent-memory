mod support;
use agent_memory::codec::json::{self, Value};
use std::ffi::OsString;
use std::fs;
use support::World;
use support::cutover::*;

#[test]
fn cutover_dispatch_worker() {
    dispatch_worker();
}
fn hook(w: &World, name: &str, event: &Value) -> Value {
    let (code, stdout, stderr) = dispatch(w, &["hook".into(), name.into()], Some(event));
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stderr.is_empty());
    json::decode(&stdout, Default::default()).unwrap()
}
fn event(repo: &std::path::Path, session: &str, turn: &str) -> Value {
    object([
        ("hook_event_name", text("Stop")),
        ("session_id", text(session)),
        ("turn_id", text(turn)),
        ("stop_hook_active", Value::Bool(false)),
        ("cwd", text(repo.to_str().unwrap())),
        ("last_assistant_message", Value::Null),
    ])
}
fn prompt(w: &World, event: &Value, body: &str) {
    let mut event = event.clone();
    put(&mut event, "hook_event_name", text("UserPromptSubmit"));
    put(&mut event, "prompt", text(body));
    hook(w, "codex-prompt", &event);
}
/// Child of the installed World: baseline `core.inspect(repo, 'codex', s, t)`
/// under the World environment with the installed pack.
#[test]
fn turn_inspect_worker() {
    let Some(request) = std::env::var_os("AGENT_MEMORY_FAULT_CODEX_TURN_INSPECT") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    support::installed_runtime::register_component_runtime();
    let request = json::decode_bytes(&fs::read(request).unwrap(), Default::default()).unwrap();
    let value = agent_memory::turn_sources::inspect(
        std::path::Path::new(string(&request, "repo")),
        "codex",
        string(&request, "session"),
        string(&request, "turn"),
        std::path::Path::new(string(&request, "pack")),
    )
    .unwrap();
    println!(
        "CODEX-TURN-INSPECT:{}",
        json::encode(&value, json::DEFAULT).unwrap()
    );
}
fn inspect(w: &World, repo: &std::path::Path, session: &str, turn: &str) -> Value {
    let request = object([
        ("repo", text(repo.to_str().unwrap())),
        ("session", text(session)),
        ("turn", text(turn)),
        ("pack", text(installed_pair(w).0.to_str().unwrap())),
    ]);
    let path = save(w, &request, "codex-turn-inspect.json");
    let env = [(
        OsString::from("AGENT_MEMORY_FAULT_CODEX_TURN_INSPECT"),
        path.into_os_string(),
    )];
    let output = w
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "turn_inspect_worker".into(),
                "--nocapture".into(),
            ],
            support::RunOptions {
                env: &env,
                ..Default::default()
            },
        )
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let row = stdout
        .lines()
        .find_map(|line| line.strip_prefix("CODEX-TURN-INSPECT:"))
        .unwrap_or_else(|| panic!("{stdout}"));
    json::decode(row, Default::default()).unwrap()
}
fn complete(w: &World, repo: &std::path::Path, session: &str, turn: &str) {
    let value = inspect(w, repo, session, turn);
    let source = get(&value, "pending")
        .as_array()
        .unwrap()
        .first()
        .unwrap_or_else(|| panic!("no pending source for {session}: {value:?}"));
    let id = string(source, "observation_id");
    let request = object([
        ("session", text(session)),
        ("prompt", text(turn)),
        ("observation_id", text(id)),
        ("source_sha256", get(source, "source_sha256").clone()),
        ("outcome", text("no-learning")),
        (
            "assessment",
            object([
                ("source_id", text(id)),
                (
                    "source_review",
                    text("Reviewed actual question; no durable learning."),
                ),
                ("candidates", Value::Array(vec![])),
            ]),
        ),
    ]);
    let input = save(w, &request, "codex-completion.json");
    let (code, stdout, stderr) = dispatch(
        w,
        &[
            OsString::from("--json"),
            "codex-boundary".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--input".into(),
            input.into_os_string(),
        ],
        None,
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
}

#[test]
// obligation: tests/contracts/test_codex_stop.py::CodexStopContract.test_nonrepository_uninitialized_and_missing_source
fn nonrepository_uninitialized_and_missing_source_are_quiet() {
    let mut w = World::new().unwrap();
    let runtime = support::installed_runtime::install(&w.root, |argv| w.run(argv).unwrap());
    w.env.insert(
        "AGENT_MEMORY_TEST_OWNED_COMMAND".into(),
        runtime.command.as_os_str().to_owned(),
    );
    let repo = w.repository("repo", false).unwrap();
    let outside = w.inside("outside").unwrap();
    fs::create_dir(&outside).unwrap();
    for root in [repo, outside] {
        let value = hook(
            &w,
            "codex-stop",
            &event(
                &root,
                "11111111-1111-4111-8111-111111111111",
                "22222222-2222-4222-8222-222222222222",
            ),
        );
        assert_eq!(value, object(Vec::<(String, Value)>::new()));
    }
}

#[test]
// obligation: tests/contracts/test_codex_stop.py::CodexStopContract.test_proactive_complete_nullable_final_late_steering_and_durable_wake_cap
fn proactive_completion_nullable_final_late_steering_and_durable_wake_cap() {
    let r = Recall::new();
    let session = "11111111-1111-4111-8111-111111111111";
    let turn = "22222222-2222-4222-8222-222222222222";
    let event = event(&r.repo, session, turn);
    prompt(&r.world, &event, "ordinary no-learning question");
    complete(&r.world, &r.repo, session, turn);
    assert_eq!(
        hook(&r.world, "codex-stop", &event),
        object(Vec::<(String, Value)>::new())
    );
    prompt(&r.world, &event, "late steering source");
    let first = hook(&r.world, "codex-stop", &event);
    assert_eq!(string(&first, "decision"), "block");
    assert!(string(&first, "reason").len() <= 2048);
    assert!(
        hook(&r.world, "codex-stop", &event)
            .get("decision")
            .is_none()
    );
    prompt(&r.world, &event, string(&first, "reason"));
    let state = inspect(&r.world, &r.repo, session, turn);
    assert!(
        get(&state, "sources")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| yes(row, "control_origin_hint"))
    );
    let second = hook(&r.world, "codex-stop", &event);
    assert_eq!(string(&second, "decision"), "block");
    prompt(&r.world, &event, "another actual steering source");
    let mut active = event;
    put(&mut active, "stop_hook_active", Value::Bool(true));
    assert!(
        hook(&r.world, "codex-stop", &active)
            .get("decision")
            .is_none()
    );
}

#[test]
// obligation: tests/contracts/test_codex_stop.py::CodexStopContract.test_quiet_completion_surfaces_earlier_pending_once
fn quiet_completion_surfaces_earlier_pending_only_once() {
    let r = Recall::new();
    let session = "11111111-1111-4111-8111-111111111111";
    let turn = "22222222-2222-4222-8222-222222222222";
    let event = event(&r.repo, session, turn);
    prompt(&r.world, &event, "current");
    complete(&r.world, &r.repo, session, turn);
    let earlier = self::event(
        &r.repo,
        "33333333-3333-4333-8333-333333333333",
        "44444444-4444-4444-8444-444444444444",
    );
    prompt(&r.world, &earlier, "earlier interrupted source");
    let first = hook(&r.world, "codex-stop", &event);
    assert!(string(&first, "systemMessage").contains("earlier source assessments remain pending"));
    assert!(first.get("decision").is_none());
    assert_eq!(
        hook(&r.world, "codex-stop", &event),
        object(Vec::<(String, Value)>::new())
    );
}
