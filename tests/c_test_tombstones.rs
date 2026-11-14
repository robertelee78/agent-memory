mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use std::path::PathBuf;
use support::World;
use support::cutover::*;
const DEAD: &str = "22222222-2222-4222-8222-222222222222";
fn metadata() -> Value {
    object([
        ("id", text(DEAD)),
        ("scope", text(PROJECT)),
        ("deleted-at", text("2026-10-22T18:00:00.123456789012345Z")),
        (
            "deletion-authority",
            text("33333333-3333-4333-8333-333333333333"),
        ),
        ("reason-class", text("user-request")),
    ])
}
fn fixture() -> (World, PathBuf, PathBuf, PathBuf, Value) {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    write_files(
        &bank,
        &render(
            &[(
                "projectbrief.md",
                "# Project brief\n\nGoals and boundaries: [project source](../README.md).\n",
            )],
            "2026-10-22T17:00:00Z",
        ),
    );
    let rendered = agent_memory::tombstones::render(&metadata()).unwrap();
    let path = bank.join(string(&rendered, "path"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, string(&rendered, "content")).unwrap();
    let router = bank.join("MEMORY.md");
    let relative = string(&rendered, "path");
    let row = format!(
        "| [{relative}](<{relative}>) | Deletion guard | repository | deleted | Before rebuilding projections |\n"
    );
    fs::write(
        &router,
        fs::read_to_string(&router).unwrap().replace(
            "\n## Omitted contexts\n",
            &format!("{row}\n## Omitted contexts\n"),
        ),
    )
    .unwrap();
    let stale = object([
        ("project_id", text(PROJECT)),
        (
            "records",
            Value::Array(vec![
                object([
                    ("id", text(DEAD)),
                    (
                        "body",
                        text("# Lesson\n\nDELETED_SECRET_CANARY must never be rebuilt.\n"),
                    ),
                    ("conditions", Value::Array(vec![text("branch:old")])),
                ]),
                object([
                    ("id", text("44444444-4444-4444-8444-444444444444")),
                    ("body", text("Useful surviving lesson")),
                    ("conditions", Value::Array(vec![text("component:api")])),
                    ("evidence", Value::Array(vec![text("src/api.rs")])),
                    ("developer", text("55555555-5555-4555-8555-555555555555")),
                ]),
            ]),
        ),
    ]);
    (w, repo, bank, path, stale)
}
fn filter(
    w: &World,
    repo: &std::path::Path,
    input: &Value,
) -> (agent_memory::process::Output, Value) {
    command(
        w,
        "tombstones.py",
        &[
            "filter".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--input".into(),
            save(w, input, "rebuild-input.json").into_os_string(),
        ],
    )
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_delete_rebuild_and_repeat_cannot_resurrect_id_under_changed_conditions
fn repeated_rebuild_cannot_resurrect_terminal_id_under_changed_conditions() {
    let (w, repo, bank, path, mut stale) = fixture();
    let canonical = physical(&bank);
    let source = save(&w, &stale, "old-projection.json");
    let original = fs::read(&source).unwrap();
    let mut outputs = Vec::new();
    for conditions in ["branch:old", "repository", "component:other"] {
        let Value::Array(records) = stale.get_mut("records").unwrap() else {
            panic!("records")
        };
        put(
            &mut records[0],
            "conditions",
            Value::Array(vec![text(conditions)]),
        );
        let (output, guarded) = filter(&w, &repo, &stale);
        assert_eq!(output.status, Status::Code(0));
        assert_eq!(get(&guarded, "suppressed"), &Value::Int(1.into()));
        assert_eq!(
            get(&guarded, "records"),
            &Value::Array(vec![get(&stale, "records").as_array().unwrap()[1].clone()])
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("DELETED_SECRET_CANARY"));
        assert!(!yes(&guarded, "writes_performed"));
        let rebuilt = save(&w, get(&guarded, "records"), "rebuilt-projection.json");
        assert!(
            !fs::read_to_string(rebuilt)
                .unwrap()
                .contains("DELETED_SECRET_CANARY")
        );
        outputs.push(guarded);
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[1], outputs[2]);
    assert_eq!(fs::read(source).unwrap(), original);
    assert_eq!(physical(&bank), canonical);
    assert!(
        !fs::read_to_string(path)
            .unwrap()
            .contains("DELETED_SECRET_CANARY")
    );
    assert!(!w.paths["config"].join("agent-memory").exists());
    assert!(!w.paths["state"].join("agent-memory").exists());
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_unknown_ids_wrong_project_invalid_history_and_interrupted_bank_stop_rebuild
fn wrong_project_unknown_ids_invalid_history_and_interruption_stop_rebuild() {
    let (w, repo, bank, path, stale) = fixture();
    let mut variants = Vec::new();
    let mut wrong = stale.clone();
    put(
        &mut wrong,
        "project_id",
        text("66666666-6666-4666-8666-666666666666"),
    );
    variants.push(wrong);
    let mut missing = stale.clone();
    put(
        &mut missing,
        "records",
        Value::Array(vec![object([("body", text("DELETED_SECRET_CANARY"))])]),
    );
    variants.push(missing);
    let mut repeated = stale.clone();
    let record = get(&stale, "records").as_array().unwrap()[0].clone();
    put(
        &mut repeated,
        "records",
        Value::Array(vec![record.clone(), record]),
    );
    variants.push(repeated);
    for input in variants {
        let (output, _) = filter(&w, &repo, &input);
        assert_eq!(output.status, Status::Code(1));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("DELETED_SECRET_CANARY"));
    }
    let original = fs::read(&path).unwrap();
    let mut corrupt = original.clone();
    corrupt.extend_from_slice(b"DELETED_SECRET_CANARY\n");
    fs::write(&path, corrupt).unwrap();
    let (output, _) = filter(&w, &repo, &stale);
    assert_eq!(output.status, Status::Code(1));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("DELETED_SECRET_CANARY"));
    fs::write(&path, original).unwrap();
    for name in [MARKER, ".agent-memory.lock"] {
        let marker = bank.join(name);
        fs::write(&marker, "Do not guess or bypass interruption").unwrap();
        assert_eq!(filter(&w, &repo, &stale).0.status, Status::Code(1));
        assert!(marker.exists());
        fs::remove_file(marker).unwrap();
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_copied_portable_guard_uses_the_same_canonical_project_history
fn portable_binary_uses_the_same_canonical_tombstone_history() {
    let (w, repo, _, _, stale) = fixture();
    let pack = w.inside("portable pack with spaces").unwrap();
    fs::create_dir(&pack).unwrap();
    let binary = pack.join("am");
    fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
    let output = w
        .run([
            binary.into_os_string(),
            "--json".into(),
            "tombstones".into(),
            "filter".into(),
            "--repo".into(),
            repo.into_os_string(),
            "--input".into(),
            save(&w, &stale, "portable.json").into_os_string(),
        ])
        .unwrap();
    let guarded = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(get(&guarded, "suppressed"), &Value::Int(1.into()));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("DELETED_SECRET_CANARY"));
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_history_namespace_route_state_and_record_identity_must_match_before_rebuild
fn history_namespace_route_state_and_identity_must_match() {
    let (w, repo, bank, path, stale) = fixture();
    let original = fs::read(&path).unwrap();
    let router = fs::read_to_string(bank.join("MEMORY.md")).unwrap();
    for field in ["id", "scope"] {
        let mut changed = metadata();
        put(
            &mut changed,
            field,
            text("66666666-6666-4666-8666-666666666666"),
        );
        let rendered = agent_memory::tombstones::render(&changed).unwrap();
        fs::write(&path, string(&rendered, "content")).unwrap();
        assert_eq!(filter(&w, &repo, &stale).0.status, Status::Code(1));
        fs::write(&path, &original).unwrap();
    }
    fs::write(
        bank.join("MEMORY.md"),
        router.replace("| repository | deleted |", "| repository | active |"),
    )
    .unwrap();
    assert_eq!(filter(&w, &repo, &stale).0.status, Status::Code(1));
    assert_eq!(fs::read(path).unwrap(), original);
}
