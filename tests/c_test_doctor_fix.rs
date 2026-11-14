mod support;
use agent_memory::codec::json::{self, Value};
use std::os::unix::fs::symlink;
use support::World;
use support::distribution::Channel;
use support::lifecycle::*;
fn setup(world: &World) -> Channel {
    let channel = Channel::new(world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    channel.install(world, "codex", "copy");
    channel
}
fn doctor(world: &World, repo: &std::path::Path) -> Value {
    let output = world
        .am_with(
            [
                "--json",
                "doctor",
                "--fix",
                "--repo",
                repo.to_str().unwrap(),
            ],
            support::RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_interrupted_bank_is_reported_and_preserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interrupted_bank_is_reported_and_preserved() {
    let world = World::new().unwrap();
    let channel = setup(&world);
    let repo = world.repository("consumer", false).unwrap();
    let memory = bank(&world, &repo);
    std::fs::write(
        memory.join(".agent-memory-transaction.md"),
        b"preserve interrupted transaction\n",
    )
    .unwrap();
    let before = snapshot(&memory);
    let value = channel.run(&world, "doctor", &[repo.to_str().unwrap()], 1);
    assert_eq!(snapshot(&memory), before);
    assert!(
        get(&value, "repairs")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| text(row, "component") == "bank.transaction"
                && text(row, "state") == "skipped")
    );
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_unsafe_bank_markers_are_preserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn linked_and_oversized_markers_are_never_repaired() {
    let world = World::new().unwrap();
    setup(&world);
    let repo = world.repository("consumer", false).unwrap();
    let memory = bank(&world, &repo);
    let marker = memory.join(".agent-memory-transaction.md");
    let external = world.root.join("foreign-marker");
    std::fs::write(&external, b"foreign data").unwrap();
    symlink(&external, &marker).unwrap();
    let value = doctor(&world, &repo);
    assert!(marker.is_symlink());
    assert_eq!(std::fs::read(&external).unwrap(), b"foreign data");
    assert_eq!(get(&value, "repairs_performed"), &Value::Bool(false));
    std::fs::remove_file(&marker).unwrap();
    std::fs::write(&marker, vec![b'x'; 2 * 1024 * 1024]).unwrap();
    let value = doctor(&world, &repo);
    assert_eq!(std::fs::metadata(marker).unwrap().len(), 2 * 1024 * 1024);
    assert_eq!(get(&value, "repairs_performed"), &Value::Bool(false));
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_recover_update_interrupted_before_receipt
#[test]
fn doctor_rolls_back_update_interrupted_before_receipt() {
    let mut world = World::new().unwrap();
    let channel = setup(&world);
    channel.publish("0.0.16", &"b".repeat(40));
    channel.fault(&mut world, "update", "update-before-receipt");
    let result = channel.run(
        &world,
        "doctor",
        &[world.paths["repo"].to_str().unwrap()],
        0,
    );
    assert_eq!(get(&result, "repairs_performed"), &Value::Bool(true));
    assert_eq!(
        text(&read(&state(&world).join("journal.json")), "status"),
        "rolled-back"
    );
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_recover_committed_interrupted_update_from_recorded_journal
#[test]
fn doctor_finishes_committed_interrupted_update_from_recorded_journal() {
    let mut world = World::new().unwrap();
    let channel = setup(&world);
    channel.publish("0.0.16", &"b".repeat(40));
    channel.fault(&mut world, "update", "update-after-receipt");
    let journal = state(&world).join("journal.json");
    assert_ne!(text(&read(&journal), "status"), "complete");
    let result = channel.run(
        &world,
        "doctor",
        &[world.paths["repo"].to_str().unwrap()],
        0,
    );
    assert_eq!(get(&result, "repairs_performed"), &Value::Bool(true));
    assert_eq!(text(&read(&journal), "status"), "complete");
}

fn render(world: &World, repo: &std::path::Path, contexts: &[(&str, &str)], time: &str) -> Value {
    let contexts_value = Value::Object(
        contexts
            .iter()
            .map(|(name, body)| {
                (
                    (*name).into(),
                    Value::Object(vec![
                        ("body".into(), Value::String((*body).into())),
                        ("purpose".into(), Value::String("Fixture context".into())),
                        ("scope".into(), Value::String("repository".into())),
                        ("lifecycle".into(), Value::String("active".into())),
                        ("read-when".into(), Value::String("When resuming".into())),
                    ]),
                )
            })
            .collect(),
    );
    let omitted = Value::Object(
        agent_memory::bank_contract::CONTEXTS
            .iter()
            .filter(|name| !contexts.iter().any(|(path, _)| path == *name))
            .map(|name| {
                (
                    (*name).to_string(),
                    Value::String("No distinct evidence".into()),
                )
            })
            .collect(),
    );
    let proposal = Value::Object(vec![
        (
            "project-id".into(),
            Value::String("8e0b66aa-2be5-4fa3-b47f-1245474aa7dd".into()),
        ),
        ("router-updated".into(), Value::String(time.into())),
        ("contexts".into(), contexts_value),
        ("omitted".into(), omitted),
    ]);
    let input = world.root.join("render.json");
    save(&input, &proposal);
    let _ = repo;
    get(
        &run(
            world,
            &["bank", "render", "--input", input.to_str().unwrap()],
            0,
        ),
        "files",
    )
    .clone()
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_receipted_bank_transaction_cleanup
#[test]
#[ignore = "needs recovery_frontier registered"]
fn doctor_cleans_only_receipted_completed_bank_transaction() {
    let world = World::new().unwrap();
    setup(&world);
    let repo = world.repository("transaction consumer", false).unwrap();
    let before = render(
        &world,
        &repo,
        &[
            ("activeContext.md", "# Focus\n\nOriginal task.\n"),
            (
                "projectbrief.md",
                "# Brief\n\nOLD_CONTENT_CANARY to prune.\n",
            ),
            (
                "techContext.md",
                "# Technical\n\nUnchanged applicability.\n",
            ),
        ],
        "2026-10-22T18:00:00Z",
    );
    let tech = text(&before, "techContext.md");
    let after = render(
        &world,
        &repo,
        &[
            ("activeContext.md", "# Focus\n\nNew task and conditions.\n"),
            (
                "progress.md",
                "# Progress\n\nVerified result; evidence remains bounded.\n",
            ),
            ("techContext.md", tech),
        ],
        "2026-10-22T19:00:00Z",
    );
    let memory = repo.join("memory-bank");
    std::fs::create_dir(&memory).unwrap();
    for (name, body) in before.as_object().unwrap() {
        std::fs::write(memory.join(name), body.as_str().unwrap()).unwrap();
    }
    support::identity_fixtures::bind(&world, &repo).unwrap();
    let input = world.root.join("files.json");
    save(&input, &after);
    let proposal = run(
        &world,
        &[
            "transaction",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--files",
            input.to_str().unwrap(),
            "--operation",
            "compact",
        ],
        0,
    );
    let path = world.root.join("proposal.json");
    save(&path, &proposal);
    assert_eq!(
        fault(
            &world,
            &["transaction", "apply", "--proposal", path.to_str().unwrap()],
            "transaction-after-receipt",
            "exit"
        )
        .status,
        agent_memory::process::Status::Code(86)
    );
    let unlock = run(
        &world,
        &["writes", "recover", "--repo", repo.to_str().unwrap()],
        0,
    );
    save(&input, &unlock);
    run(
        &world,
        &[
            "writes",
            "recover",
            "--repo",
            repo.to_str().unwrap(),
            "--proposal",
            input.to_str().unwrap(),
        ],
        0,
    );
    let value = doctor(&world, &repo);
    assert!(!memory.join(".agent-memory-transaction.md").exists());
    for (path, body) in after.as_object().unwrap() {
        assert_eq!(
            std::fs::read_to_string(memory.join(path)).unwrap(),
            body.as_str().unwrap()
        );
    }
    assert!(!memory.join("projectbrief.md").exists());
    assert!(
        get(&value, "repairs")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| text(row, "component") == "bank.transaction"
                && text(row, "state") == "repaired")
    );
}
