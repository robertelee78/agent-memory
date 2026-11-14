mod support;
use agent_memory::codec::json::{self, Value};
use std::path::PathBuf;
use support::World;
use support::canaries::Canary;
use support::lifecycle::*;

#[test]
fn hook_driver() {
    let Ok(raw) = std::env::var("AGENT_MEMORY_FAULT_HOOK_REQUEST") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    let request = json::decode(&raw, Default::default()).unwrap();
    let repo = PathBuf::from(text(&request, "repo"));
    let pack = PathBuf::from(text(&request, "pack"));
    if get(&request, "compatibility") == &Value::Bool(true) {
        let paths = agent_memory::layout_recovery_state::paths(&repo, None, &pack).unwrap();
        let base = paths.2.parent().unwrap();
        std::fs::create_dir_all(base).unwrap();
        std::fs::write(base.join("turn-sources-compat-v1.json"), b"{}").unwrap();
    }
    let input = json::encode(get(&request, "event"), json::DEFAULT).unwrap();
    let output = agent_memory::hooks::run(text(&request, "hook"), input.as_bytes(), &|| {
        Ok(pack.clone())
    })
    .unwrap();
    let records = agent_memory::hook_failure::read(&repo, None).unwrap();
    let value = Value::Object(vec![
        ("code".into(), Value::Int(output.exit_code.into())),
        ("stdout".into(), Value::String(output.stdout)),
        ("stderr".into(), Value::String(output.stderr)),
        ("records".into(), Value::Array(records)),
    ]);
    println!(
        "HOOK-RESULT:{}",
        json::encode(&value, json::COMPACT_ASCII).unwrap()
    );
}
fn hook(world: &World, fixture: &Canary, adapter: &str, compatibility: bool) -> Value {
    let event = Value::Object(vec![
        (
            "hook_event_name".into(),
            Value::String(
                if adapter == "prompt" {
                    "UserPromptSubmit"
                } else {
                    "Stop"
                }
                .into(),
            ),
        ),
        (
            "session_id".into(),
            Value::String(agent_memory::host::uuid4().unwrap()),
        ),
        (
            "turn_id".into(),
            Value::String(agent_memory::host::uuid4().unwrap()),
        ),
        (
            "cwd".into(),
            Value::String(fixture.repo.to_string_lossy().into_owned()),
        ),
        ("stop_hook_active".into(), Value::Bool(false)),
        (
            "last_assistant_message".into(),
            Value::String("SECRET FINAL".into()),
        ),
        ("prompt".into(), Value::String("SECRET PROMPT".into())),
    ]);
    let request = Value::Object(vec![
        (
            "repo".into(),
            Value::String(fixture.repo.to_string_lossy().into_owned()),
        ),
        (
            "pack".into(),
            Value::String(fixture.channel.pack.to_string_lossy().into_owned()),
        ),
        ("hook".into(), Value::String(format!("codex-{adapter}"))),
        ("compatibility".into(), Value::Bool(compatibility)),
        ("event".into(), event),
    ]);
    let overrides = [(
        std::ffi::OsString::from("AGENT_MEMORY_FAULT_HOOK_REQUEST"),
        std::ffi::OsString::from(json::encode(&request, json::DEFAULT).unwrap()),
    )];
    let output = world
        .run_with(
            [
                std::env::current_exe().unwrap().as_os_str(),
                std::ffi::OsStr::new("hook_driver"),
                std::ffi::OsStr::new("--exact"),
                std::ffi::OsStr::new("--nocapture"),
            ],
            support::RunOptions {
                env: &overrides,
                ..Default::default()
            },
        )
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let row = stdout
        .lines()
        .find_map(|line| line.strip_prefix("HOOK-RESULT:"))
        .unwrap_or_else(|| panic!("{stdout}"));
    json::decode(row, Default::default()).unwrap()
}
fn protected(
    fixture: &Canary,
) -> (
    std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)>,
    Vec<u8>,
    Vec<u8>,
) {
    (
        snapshot(&fixture.bank),
        std::fs::read(fixture.repo.join(".git/index")).unwrap(),
        std::fs::read(fixture.repo.join("app.rs")).unwrap(),
    )
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_source_compatibility_fault_is_not_misclassified_as_identity
#[test]
#[ignore = "needs recovery_frontier registered"]
fn corrupt_source_compatibility_marker_is_boundary_not_identity() {
    let world = World::new().unwrap();
    let fixture = Canary::new(&world);
    let before = protected(&fixture);
    let result = hook(&world, &fixture, "stop", true);
    assert_eq!(get(&result, "code"), &Value::Int(0.into()));
    let records = get(&result, "records").as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(text(&records[0], "stage"), "boundary");
    assert_eq!(text(&records[0], "reason"), "state.unavailable");
    assert!(text(&result, "stdout").contains("[boundary/state.unavailable]"));
    assert!(
        !json::encode(&result, json::DEFAULT)
            .unwrap()
            .contains("SECRET")
    );
    assert_eq!(protected(&fixture), before);
    let markers = find(&world.paths["state"], "turn-sources-compat-v1.json");
    assert_eq!(markers.len(), 1);
    assert_eq!(std::fs::read(&markers[0]).unwrap(), b"{}");
}
fn find(path: &std::path::Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            found.extend(find(&entry.path(), name));
        } else if entry.file_name() == name {
            found.push(entry.path());
        }
    }
    found
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_stop_scope_loss_reports_identity_code_without_bank_change
#[test]
#[ignore = "needs recovery_frontier registered"]
fn missing_real_scope_binding_reports_identity_without_bank_change() {
    let world = World::new().unwrap();
    let fixture = Canary::new(&world);
    let before = protected(&fixture);
    let registry = world.paths["config"].join("agent-memory/projects.json");
    let mut value = read(&registry);
    if let Value::Object(fields) = &mut value {
        fields
            .iter_mut()
            .find(|(key, _)| key == "checkouts")
            .unwrap()
            .1 = Value::Object(vec![]);
    }
    save(&registry, &value);
    let result = hook(&world, &fixture, "stop", false);
    assert_eq!(get(&result, "code"), &Value::Int(0.into()));
    let records = get(&result, "records").as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(text(&records[0], "stage"), "identity");
    assert_eq!(text(&records[0], "reason"), "identity.unresolved");
    assert!(text(&result, "stdout").contains("[identity/identity.unresolved]"));
    assert!(
        !json::encode(&result, json::DEFAULT)
            .unwrap()
            .contains("SECRET")
    );
    assert_eq!(protected(&fixture), before);
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_unexpected_exception_is_fixed_and_does_not_expose_exception_text
#[test]
#[ignore = "needs recovery_frontier registered"]
fn real_unreadable_git_configuration_fails_closed_without_private_detail() {
    for adapter in ["prompt", "stop"] {
        let world = World::new().unwrap();
        let fixture = Canary::new(&world);
        let before = protected(&fixture);
        let config = fixture.repo.join(".git/config");
        mode(&config, 0o000);
        let result = hook(&world, &fixture, adapter, false);
        assert_eq!(get(&result, "code"), &Value::Int(0.into()));
        let response = json::decode(text(&result, "stdout"), Default::default()).unwrap();
        if adapter == "prompt" {
            assert_eq!(text(&response, "decision"), "block");
            assert!(text(&response, "reason").contains("Preserve work"));
        } else {
            assert!(text(&response, "systemMessage").contains("Preserve work"));
        }
        let raw = json::encode(&result, json::DEFAULT).unwrap();
        assert!(!raw.contains("SECRET"));
        assert!(!text(&result, "stdout").contains("Permission denied"));
        assert!(!text(&result, "stdout").contains(fixture.repo.to_str().unwrap()));
        assert_eq!(protected(&fixture), before);
        mode(&config, 0o600);
    }
}
