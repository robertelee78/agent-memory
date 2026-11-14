mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use support::cutover::*;
use support::{RunOptions, World};

#[test]
fn cutover_dispatch_worker() {
    dispatch_worker();
}
fn call(w: &World, args: &[OsString]) -> (i32, Value) {
    let (code, stdout, stderr) = dispatch(w, args, None);
    let value = json::decode(&stdout, Default::default())
        .unwrap_or_else(|e| panic!("{e}: {stdout}\n{stderr}"));
    (code, value)
}
fn ok(w: &World, args: &[OsString]) -> Value {
    let (code, value) = call(w, args);
    assert_eq!(code, 0, "{value:?}");
    value
}
fn git(w: &World, repo: &Path, args: &[&str]) -> Vec<u8> {
    w.git_with(
        args,
        RunOptions {
            cwd: Some(repo),
            ..RunOptions::default()
        },
    )
    .unwrap()
    .stdout
}
fn work(w: &World, repo: &Path) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        fs::read(repo.join(".git/index")).unwrap(),
        git(w, repo, &["rev-parse", "HEAD"]),
        fs::read(repo.join("src/app.rs")).unwrap(),
        fs::read(repo.join("feature.rs")).unwrap(),
    )
}
fn no_federation(w: &World) {
    for name in ["federation", "catalog", "graph", "vector-store"] {
        assert!(!w.root.join(name).exists());
    }
    for name in ["codex", "claude", "opencode", "credentials"] {
        assert!(
            !physical(&w.paths[name])
                .keys()
                .any(|p| !p.starts_with("git-"))
        );
    }
}
fn claim(fact: &str, value: i64, reference: &str, context: &str) -> Value {
    object([
        ("context", text(context)),
        ("fact", text(fact)),
        ("fact_key", text("retry-limit")),
        (
            "summary",
            text(format!("Reviewed {fact} retry limit: {value}.")),
        ),
        ("value", Value::Int(value.into())),
        ("scope", text("repository")),
        (
            "conditions",
            if fact == "implementation" {
                object([("branch", text("main"))])
            } else {
                object(Vec::<(String, Value)>::new())
            },
        ),
        ("evidence", Value::Array(vec![text(reference)])),
        ("current", Value::Bool(true)),
        ("applicable", Value::Bool(true)),
        ("qualified", Value::Bool(true)),
        (
            "authority_kind",
            text(if fact == "intent" {
                "approved-planning"
            } else {
                "code"
            }),
        ),
    ])
}
fn initialized() -> (World, PathBuf, PathBuf, String) {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    fs::create_dir_all(repo.join("docs/adr")).unwrap();
    fs::create_dir(repo.join("src")).unwrap();
    fs::create_dir(repo.join("tests")).unwrap();
    fs::write(
        repo.join("README.md"),
        "# Widget\n\nDOC_FULL_TEXT_CANARY should not be copied into memory.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/adr/one.md"),
        "---\nstatus: accepted\n---\n# Retry intent\n\nThe approved target is five retries.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/adr/two.md"),
        "---\nstatus: accepted\n---\n# Wall color\n\nThe wall is blue.\n",
    )
    .unwrap();
    fs::write(repo.join("src/app.rs"), "const RETRY_LIMIT: u32 = 3;\n").unwrap();
    fs::write(
        repo.join("tests/test_app.rs"),
        "assert_eq!(RETRY_LIMIT, 3);\n",
    )
    .unwrap();
    let p = good(
        &w,
        "am developer",
        &[
            "preview".into(),
            "--create".into(),
            "--label".into(),
            "Detecting developer".into(),
        ],
    );
    good(
        &w,
        "am developer",
        &[
            "apply".into(),
            "--proposal".into(),
            save(&w, &p, "developer.json").into_os_string(),
        ],
    );
    let p = good(
        &w,
        "am identity",
        &[
            "preview".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--action".into(),
            "create".into(),
            "--reason".into(),
            "Explicit new repository identity".into(),
        ],
    );
    let bound = good(
        &w,
        "am identity",
        &[
            "apply".into(),
            "--proposal".into(),
            save(&w, &p, "binding.json").into_os_string(),
        ],
    );
    let project = string(&bound, "project_id").to_owned();
    git(&w, &repo, &["add", "."]);
    git(&w, &repo, &["commit", "-m", "Local source evidence"]);
    fs::write(repo.join("feature.rs"), "const UNFINISHED: bool = true;\n").unwrap();
    git(&w, &repo, &["add", "feature.rs"]);
    let before = work(&w, &repo);
    let observed = ok(
        &w,
        &[
            "--json".into(),
            "init".into(),
            "inspect".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    assert_eq!(string(&observed, "bank_state"), "absent");
    let review = object([
        ("inventory", observed),
        ("existing_memory_reviewed", Value::Bool(false)),
        (
            "claims",
            Value::Array(vec![
                claim("intent", 5, "docs/adr/one.md", "projectbrief.md"),
                claim("implementation", 3, "src/app.rs", "techContext.md"),
            ]),
        ),
        ("questions", Value::Array(vec![])),
    ]);
    let proposal = ok(
        &w,
        &[
            "--json".into(),
            "init".into(),
            "preview".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--review".into(),
            save(&w, &review, "review.json").into_os_string(),
        ],
    );
    let done = ok(
        &w,
        &[
            "--json".into(),
            "init".into(),
            "apply".into(),
            "--proposal".into(),
            save(&w, &proposal, "initialization.json").into_os_string(),
        ],
    );
    assert!(yes(&done, "verified"));
    assert!(!yes(&done, "publication_verified"));
    assert_eq!(work(&w, &repo), before);
    no_federation(&w);
    let bank = repo.join("memory-bank");
    (w, repo, bank, project)
}
fn checkpoint(w: &World, repo: &Path) -> Value {
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
    assert!(!yes(&current, "preparation_required"));
    let fields = object([
        (
            "focus",
            text("Inspect the local retry evidence before broadening applicability."),
        ),
        (
            "recent_changes",
            Value::Array(vec![text(
                "Initialization retained intended five retries and observed three separately.",
            )]),
        ),
        (
            "open_questions",
            Value::Array(vec![text(
                "Behavior outside the current retry component remains unverified.",
            )]),
        ),
        (
            "next_step",
            text("Inspect the recovery result and source conditions."),
        ),
        (
            "evidence",
            Value::Array(vec![text("../src/app.rs"), text("../docs/adr/one.md")]),
        ),
        ("conditions", object([("branch", text("main"))])),
        ("harness", text("custom")),
        ("session", text("local-offline-session")),
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
            save(w, &fields, "checkpoint-fields.json").into_os_string(),
        ],
    )
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_independent_local_bank.py::IndependentLocalBankContract.test_initialize_checkpoint_resume_and_search_without_any_global_service
fn initialize_checkpoint_resume_and_search_without_global_services() {
    let (w, repo, bank, project) = initialized();
    let before = work(&w, &repo);
    let p = checkpoint(&w, &repo);
    let done = ok(
        &w,
        &[
            "--json".into(),
            "checkpoint".into(),
            "apply".into(),
            "--proposal".into(),
            save(&w, &p, "checkpoint.json").into_os_string(),
        ],
    );
    assert!(yes(&done, "verified"));
    assert!(!yes(&done, "publication_verified"));
    assert_eq!(
        get(&done, "changed_memory_paths"),
        &Value::Array(vec![text("memory-bank/activeContext.md")])
    );
    let saved = physical(&bank);
    let resumed = ok(
        &w,
        &[
            "--json".into(),
            "checkpoint".into(),
            "inspect".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    assert!(
        string(&resumed, "active_context")
            .contains("outside the current retry component remains unverified")
    );
    assert!(string(&resumed, "active_context").contains("../src/app.rs"));
    let request = object([
        ("project_id", text(&project)),
        ("query", text("retry")),
        ("ids", Value::Array(vec![])),
        ("paths", Value::Array(vec![])),
        ("context", object([("branch", text("main"))])),
        ("kinds", Value::Array(vec![])),
        ("minimum_trust", text("reviewed")),
        ("limit", Value::Int(64.into())),
        ("include_context", Value::Bool(true)),
    ]);
    let recalled = ok(
        &w,
        &[
            "--json".into(),
            "search".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--request".into(),
            save(&w, &request, "recall.json").into_os_string(),
        ],
    );
    let rows = get(&recalled, "results").as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|row| string(row, "selection_class") == "orientation-only")
    );
    assert!(
        rows.iter()
            .any(|row| string(row, "body").contains("Inspect the local retry evidence"))
    );
    assert!(
        fs::read_to_string(bank.join("projectbrief.md"))
            .unwrap()
            .contains("Reviewed intent retry limit: 5")
    );
    assert!(
        fs::read_to_string(bank.join("techContext.md"))
            .unwrap()
            .contains("Reviewed implementation retry limit: 3")
    );
    let checked = good(
        &w,
        "am bank",
        &[
            "validate".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    assert_eq!(string(&checked, "project_id"), project);
    assert_eq!(physical(&bank), saved);
    assert_eq!(work(&w, &repo), before);
    no_federation(&w);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_independent_local_bank.py::IndependentLocalBankContract.test_version_export_and_plain_file_repair_after_disposable_state_loss
fn version_export_and_plain_file_repair_after_state_loss() {
    let (w, repo, bank, project) = initialized();
    git(&w, &repo, &["add", "memory-bank"]);
    git(
        &w,
        &repo,
        &[
            "commit",
            "--only",
            "memory-bank",
            "-m",
            "Version the local bank",
        ],
    );
    let saved = physical(&bank);
    let before = work(&w, &repo);
    let archive = w.inside("local-bank.tar").unwrap();
    w.git_with(
        [
            OsString::from("archive"),
            "--format=tar".into(),
            "--output".into(),
            archive.as_os_str().to_owned(),
            "HEAD".into(),
            "memory-bank".into(),
        ],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    let exported = w.inside("export").unwrap();
    fs::create_dir(&exported).unwrap();
    w.run([
        OsString::from("/usr/bin/tar"),
        "-xf".into(),
        archive.into_os_string(),
        "-C".into(),
        exported.as_os_str().to_owned(),
    ])
    .unwrap();
    assert_eq!(physical(&exported.join("memory-bank")), saved);
    for name in ["config", "state", "cache"] {
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
    let target = bank.join("activeContext.md");
    let body = fs::read_to_string(&target).unwrap();
    fs::write(
        &target,
        body + "\n## Manual note\n\nRetain the same project boundary.\n",
    )
    .unwrap();
    assert!(fs::read_to_string(&target).unwrap().contains("Manual note"));
    let validate = || {
        command(
            &w,
            "am bank",
            &[
                "validate".into(),
                "--repo".into(),
                repo.as_os_str().to_owned(),
            ],
        )
    };
    let (output, value) = validate();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(string(&value, "project_id"), project);
    fs::write(&target, "   \n").unwrap();
    assert_eq!(validate().0.status, Status::Code(1));
    fs::write(
        &target,
        git(&w, &repo, &["show", "HEAD:memory-bank/activeContext.md"]),
    )
    .unwrap();
    let (output, value) = validate();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(string(&value, "project_id"), project);
    assert_eq!(physical(&bank), saved);
    assert_eq!(work(&w, &repo), before);
    for name in ["config", "state", "cache"] {
        assert!(!w.paths[name].exists());
    }
    no_federation(&w);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_independent_local_bank.py::IndependentLocalBankContract.test_local_interrupted_checkpoint_recovers_with_retained_private_receipt_evidence
fn interrupted_local_checkpoint_recovers_from_retained_receipt_evidence() {
    let (w, repo, bank, project) = initialized();
    let before = work(&w, &repo);
    let proposal = checkpoint(&w, &repo);
    let file = save(&w, &proposal, "interrupted-checkpoint.json");
    let request = object([
        (
            "args",
            Value::Array(vec![
                text("--json"),
                text("checkpoint"),
                text("apply"),
                text("--proposal"),
                text(file.to_str().unwrap()),
            ]),
        ),
        ("event", Value::Null),
    ]);
    let input = save(&w, &request, "fault-dispatch.json");
    let env = [
        (
            "AGENT_MEMORY_FAULT_DISPATCH_INPUT".into(),
            input.into_os_string(),
        ),
        ("AGENT_MEMORY_FAULT_STAGE".into(), "after-replace".into()),
        ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
    ];
    let output = w
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "cutover_dispatch_worker".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                check: false,
                env: &env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(86));
    assert!(bank.join(".agent-memory.lock").exists());
    refused(
        &w,
        "am bank",
        &[
            "validate".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    let unlock = good(
        &w,
        "am writes",
        &[
            "recover".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    good(
        &w,
        "am writes",
        &[
            "recover".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--proposal".into(),
            save(&w, &unlock, "unlock.json").into_os_string(),
        ],
    );
    let done = ok(
        &w,
        &[
            "--json".into(),
            "checkpoint".into(),
            "apply".into(),
            "--proposal".into(),
            file.into_os_string(),
        ],
    );
    assert!(yes(&done, "verified"));
    assert!(!yes(&done, "writes_performed"));
    assert!(!bank.join(".agent-memory.lock").exists());
    let checked = good(
        &w,
        "am bank",
        &[
            "validate".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
        ],
    );
    assert_eq!(string(&checked, "project_id"), project);
    assert_eq!(work(&w, &repo), before);
    no_federation(&w);
}
