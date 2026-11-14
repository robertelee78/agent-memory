mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use support::cutover::*;
use support::{RunOptions, World};

#[test]
fn cutover_dispatch_worker() {
    dispatch_worker();
}
fn cli(w: &World, args: &[OsString]) -> (i32, Value) {
    let (code, stdout, stderr) = dispatch(w, args, None);
    (
        code,
        json::decode(&stdout, Default::default())
            .unwrap_or_else(|e| panic!("{e}: {stdout}\n{stderr}")),
    )
}
fn ok(w: &World, args: &[OsString]) -> Value {
    let (code, value) = cli(w, args);
    assert_eq!(code, 0, "{value:?}");
    value
}
fn developer(w: &World) -> String {
    let p = good(
        w,
        "developer_identity.py",
        &[
            "preview".into(),
            "--create".into(),
            "--label".into(),
            "Disposable canary developer".into(),
        ],
    );
    let receipt = good(
        w,
        "developer_identity.py",
        &[
            "apply".into(),
            "--proposal".into(),
            save(w, &p, "developer.json").into_os_string(),
        ],
    );
    string(&receipt, "developer_id").to_owned()
}
fn checkpoint(w: &World, repo: &Path, focus: &str) -> Value {
    let current = ok(
        w,
        &[
            "--json".into(),
            "checkpoint".into(),
            "inspect".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    let request = object([
        ("focus", text(focus)),
        (
            "recent_changes",
            Value::Array(vec![text(
                "The staged implementation uses four retries; a tool report claims five.",
            )]),
        ),
        (
            "open_questions",
            Value::Array(vec![text(
                "Can the five-retry claim be reproduced in another clone?",
            )]),
        ),
        (
            "next_step",
            text("Run an independent reproduction before drawing conclusions."),
        ),
        (
            "conditions",
            object([("component", text("retry")), ("branch", text("main"))]),
        ),
        ("evidence", Value::Array(vec![text("../app.rs")])),
        ("harness", text("custom")),
        ("session", text("canary-session")),
        (
            "observed_active_sha256",
            get(&current, "observed_active_sha256").clone(),
        ),
    ]);
    ok(
        w,
        &[
            "--json".into(),
            "checkpoint".into(),
            "preview".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--input".into(),
            save(w, &request, "checkpoint-fields.json").into_os_string(),
        ],
    )
}
fn apply(w: &World, p: &Value) -> (i32, Value) {
    cli(
        w,
        &[
            "--json".into(),
            "checkpoint".into(),
            "apply".into(),
            "--proposal".into(),
            save(w, p, "checkpoint.json").into_os_string(),
        ],
    )
}
fn fixture() -> Recall {
    let r = Recall::new();
    fs::write(r.bank.join("activeContext.md"),"# Focus\n\nInvestigate retry behavior.\n\n## Original evidence\n\nSource: [implementation](../app.rs); applicability elsewhere is unknown.\n").unwrap();
    r.world
        .git_with(
            ["add", "memory-bank"],
            RunOptions {
                cwd: Some(&r.repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    r.world
        .git_with(
            ["commit", "-m", "Disposable source and memory bank"],
            RunOptions {
                cwd: Some(&r.repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    r
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_identity_canaries.py::IdentityCanaries.test_changed_developer_selection_invalidates_old_checkpoint_without_reattribution
fn changed_developer_selection_invalidates_old_checkpoint_without_reattribution() {
    let r = fixture();
    let first = good(&r.world, "developer_identity.py", &["status".into()]);
    let proposal = checkpoint(
        &r.world,
        &r.repo,
        "First developer is investigating the retry discrepancy.",
    );
    let second = developer(&r.world);
    assert_ne!(string(&first, "developer_id"), second);
    let before = physical(&r.bank);
    let code = r.code();
    let (exit, _) = apply(&r.world, &proposal);
    assert_eq!(exit, 1);
    assert_eq!(physical(&r.bank), before);
    assert_eq!(r.code(), code);
    let fresh = checkpoint(
        &r.world,
        &r.repo,
        "Investigate retry behavior in the verified component.",
    );
    let (exit, done) = apply(&r.world, &fresh);
    assert_eq!(exit, 0);
    assert!(yes(&done, "verified"));
    assert!(
        fs::read_to_string(r.bank.join("activeContext.md"))
            .unwrap()
            .contains(&second)
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_identity_canaries.py::IdentityCanaries.test_replacing_bank_with_another_project_never_silently_rebinds_or_recalls_it
fn replacing_bank_with_another_project_never_silently_rebinds_or_recalls_it() {
    let r = fixture();
    let code = r.code();
    let foreign = "66666666-6666-4666-8666-666666666666";
    let router = r.bank.join("MEMORY.md");
    fs::write(
        &router,
        fs::read_to_string(&router)
            .unwrap()
            .replace(PROJECT, foreign),
    )
    .unwrap();
    fs::write(
        r.bank.join("activeContext.md"),
        "# Focus\n\nFOREIGN_PROJECT_BODY_MUST_NOT_RETURN\n",
    )
    .unwrap();
    let changed = physical(&r.bank);
    for project in [PROJECT, foreign] {
        let request = object([
            ("project_id", text(project)),
            ("category", text("retrieval")),
            ("source_class", text("assistant")),
            ("content_kind", text("summary")),
            (
                "text",
                text("The requested lesson is in the selected repository."),
            ),
            ("record_ids", Value::Array(vec![])),
            ("automatic", Value::Bool(true)),
            ("repository_evidence", Value::Null),
        ]);
        let (output, value) = command(
            &r.world,
            "security_policy.py",
            &[
                "--repo".into(),
                r.repo.as_os_str().to_owned(),
                "--request".into(),
                save(&r.world, &request, "policy.json").into_os_string(),
            ],
        );
        assert_eq!(output.status, Status::Code(1));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("FOREIGN_PROJECT_BODY"));
        assert!(!yes(&value, "canonical_writes_performed"));
    }
    let status = good(
        &r.world,
        "scope_binding.py",
        &[
            "status".into(),
            "--repo".into(),
            r.repo.as_os_str().to_owned(),
        ],
    );
    assert!(!yes(&status, "resolved"));
    assert_eq!(physical(&r.bank), changed);
    assert_eq!(r.code(), code);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_identity_canaries.py::IdentityCanaries.test_checkpoint_provenance_survives_fresh_machine_and_second_developer_without_personal_bank
fn checkpoint_provenance_survives_fresh_machine_and_second_developer() {
    let mut r = fixture();
    let first = string(
        &good(&r.world, "developer_identity.py", &["status".into()]),
        "developer_id",
    )
    .to_owned();
    let before = physical(&r.bank);
    let code = r.code();
    let proposal = checkpoint(
        &r.world,
        &r.repo,
        "Investigate retry behavior in the verified component.",
    );
    let (exit, _) = apply(&r.world, &proposal);
    assert_eq!(exit, 0);
    assert_eq!(r.code(), code);
    for (path, raw) in before
        .iter()
        .filter(|(path, _)| path.as_str() != "activeContext.md")
    {
        assert_eq!(fs::read(r.bank.join(path)).unwrap(), *raw);
    }
    let active = fs::read_to_string(r.bank.join("activeContext.md")).unwrap();
    for phrase in [
        "Original evidence",
        "applicability elsewhere is unknown",
        "independent reproduction",
    ] {
        assert!(active.contains(phrase));
    }
    r.world
        .git_with(
            ["add", "memory-bank"],
            RunOptions {
                cwd: Some(&r.repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    r.world
        .git_with(
            ["commit", "-m", "Explicit fixture memory transfer"],
            RunOptions {
                cwd: Some(&r.repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    let clone = r.world.clone(&r.repo, "second machine clone").unwrap();
    r.world
        .git_with(
            ["remote", "remove", "origin"],
            RunOptions {
                cwd: Some(&clone),
                ..RunOptions::default()
            },
        )
        .unwrap();
    let machine = r.world.inside("second-machine").unwrap();
    fs::create_dir(&machine).unwrap();
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "AGENT_MEMORY_CONFIG_HOME",
        "AGENT_MEMORY_STATE_HOME",
        "GH_CONFIG_DIR",
    ] {
        let target = machine.join(key.to_lowercase());
        fs::create_dir(&target).unwrap();
        r.world.env.insert(key.into(), target.into_os_string());
    }
    let checked = good(
        &r.world,
        "bank_contract.py",
        &[
            "validate".into(),
            "--repo".into(),
            clone.as_os_str().to_owned(),
        ],
    );
    assert_eq!(string(&checked, "project_id"), PROJECT);
    assert_eq!(
        fs::read_to_string(clone.join("memory-bank/activeContext.md")).unwrap(),
        active
    );
    let second = developer(&r.world);
    assert_ne!(first, second);
    support::identity_fixtures::bind(&r.world, &clone).unwrap();
    let code_before = fs::read(clone.join("app.rs")).unwrap();
    let index = fs::read(clone.join(".git/index")).unwrap();
    let proposal = checkpoint(
        &r.world,
        &clone,
        "Investigate retry behavior in the verified component.",
    );
    assert_eq!(apply(&r.world, &proposal).0, 0);
    assert_eq!(fs::read(clone.join("app.rs")).unwrap(), code_before);
    assert_eq!(fs::read(clone.join(".git/index")).unwrap(), index);
    let status = good(
        &r.world,
        "scope_binding.py",
        &[
            "status".into(),
            "--repo".into(),
            clone.as_os_str().to_owned(),
        ],
    );
    assert_eq!(string(&status, "project_id"), PROJECT);
    assert_eq!(
        fs::read_to_string(r.bank.join("activeContext.md")).unwrap(),
        active
    );
    assert!(!clone.join("memory-bank/personal").exists());
}
