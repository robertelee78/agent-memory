#![cfg(feature = "dev-tools")]
mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::{Input, Output, Spec, Status, Target};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::cutover::*;
use support::{RunOptions, World};
struct Installed {
    r: Recall,
    runtime: PathBuf,
    cli: PathBuf,
}
impl Installed {
    fn new() -> Self {
        let r = Recall::new();
        let (runtime, cli) = support::installed_runtime::installed_runtime(&r.world);
        Self { r, runtime, cli }
    }
    fn call(&self, args: &[&str], expected: i32) -> Value {
        call(&self.r.world, &self.cli, args, expected)
    }
    fn repo(&self) -> &str {
        self.r.repo.to_str().unwrap()
    }
    fn checkpoint(&self, conditions: Value, evidence: Vec<Value>, prepare: bool) -> Value {
        let current = self.call(&["checkpoint", "inspect", "--repo", self.repo()], 0);
        let payload = object([
            (
                "focus",
                text("Verify the task-specific observed retry result."),
            ),
            (
                "recent_changes",
                Value::Array(vec![text("Reviewed the actual repository result.")]),
            ),
            (
                "open_questions",
                Value::Array(vec![text("Broader applicability remains unknown.")]),
            ),
            ("next_step", text("Inspect another independent clone.")),
            ("evidence", Value::Array(evidence)),
            ("conditions", conditions),
            ("harness", text("custom")),
            ("session", text("gap-session")),
            (
                "observed_active_sha256",
                get(&current, "observed_active_sha256").clone(),
            ),
        ]);
        let path = save(&self.r.world, &payload, "checkpoint-input.json");
        self.call(
            &[
                "checkpoint",
                if prepare { "prepare" } else { "preview" },
                "--repo",
                self.repo(),
                "--input",
                path.to_str().unwrap(),
            ],
            0,
        )
    }
    fn apply_checkpoint(&self, p: &Value) -> Value {
        let path = save(&self.r.world, p, "checkpoint-apply.json");
        self.call(
            &["checkpoint", "apply", "--proposal", path.to_str().unwrap()],
            0,
        )
    }
    fn recall(&self, request: &Value, alias: &str) -> Value {
        let path = save(&self.r.world, request, "recall.json");
        self.call(
            &[
                alias,
                "--repo",
                self.repo(),
                "--request",
                path.to_str().unwrap(),
            ],
            0,
        )
    }
}
fn call(w: &World, cli: &Path, args: &[&str], expected: i32) -> Value {
    let output = w
        .run_with(
            std::iter::once(cli.as_os_str())
                .chain(std::iter::once(std::ffi::OsStr::new("--json")))
                .chain(args.iter().map(std::ffi::OsStr::new)),
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(expected), "{output:?}");
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}
fn stdin(w: &World, cli: &Path, args: &[&str], value: &Value) -> Output {
    let argv = std::iter::once(cli.as_os_str().to_owned())
        .chain(args.iter().map(OsString::from))
        .collect::<Vec<_>>();
    let env = w
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    let raw = json::encode(value, json::DEFAULT).unwrap();
    agent_memory::process::run(&Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&w.paths["repo"]),
        search_path: false,
        stdin: Input::Bytes(raw.as_bytes()),
        stdout: Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        stderr: Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(20)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .unwrap()
}
fn decode(output: &Output) -> Value {
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}
fn pause(w: &World, cli: &Path, args: &[&str], stage: &str) -> (support::Child, PathBuf) {
    let directory = w.root.join("gap-rendezvous");
    let env = [
        ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
        ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
        (
            "AGENT_MEMORY_FAULT_DIR".into(),
            directory.clone().into_os_string(),
        ),
    ];
    let child = w
        .spawn_with(
            std::iter::once(cli.as_os_str())
                .chain(std::iter::once(std::ffi::OsStr::new("--json")))
                .chain(args.iter().map(std::ffi::OsStr::new)),
            RunOptions {
                env: &env,
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(10)).unwrap();
    while !directory.join(format!("{stage}.ready")).exists() {
        assert!(!deadline.expired(), "missing {stage}");
        std::thread::sleep(Duration::from_millis(5));
    }
    (child, directory.join(format!("{stage}.release")))
}
fn metadata(bank: &Path, path: &str) -> Value {
    let raw = fs::read_to_string(bank.join(path)).unwrap();
    let (_, metadata, _) = agent_memory::records::parse(&raw, &[]).unwrap();
    metadata
}
fn compact(f: &Installed) -> (Value, Value, Vec<String>) {
    let body = "# Retry policy\n\nUse three attempts for the authored case.\n";
    let p1 = f.r.add(
        "first",
        body,
        &object([("evidence", Value::Array(vec![text("user:first-origin")]))]),
    );
    let p2 = f.r.add(
        "second",
        body,
        &object([("evidence", Value::Array(vec![text("user:second-origin")]))]),
    );
    let first = metadata(&f.r.bank, &p1);
    let request = object([
        ("project_id", text(PROJECT)),
        (
            "candidate",
            object([("metadata", first.clone()), ("body", text(body))]),
        ),
        (
            "searches",
            Value::Array(vec![text("retry policy"), text("attempt cap")]),
        ),
        ("read_paths", Value::Array(vec![text(&p1), text(&p2)])),
    ]);
    let req = save(&f.r.world, &request, "compact-request.json");
    let view = f.call(
        &[
            "compare",
            "inspect",
            "--repo",
            f.repo(),
            "--request",
            req.to_str().unwrap(),
        ],
        0,
    );
    let judgment=object([("observation",get(&view,"observation").clone()),("decisions",Value::Array(get(&view,"matches").as_array().unwrap().iter().map(|r|object([
        ("path",get(r,"path").clone()),("classification",text("equivalent")),("rationale",text("These independently attributed authored caps have the same exact conditions."))])).collect())),
        ("pending_decisions",Value::Array(vec![])),("rationale",text("Retain both actual sources and all correction provenance."))]);
    let j = save(&f.r.world, &judgment, "compact-judgment.json");
    let proof = f.call(
        &[
            "compare",
            "review",
            "--repo",
            f.repo(),
            "--request",
            req.to_str().unwrap(),
            "--judgment",
            j.to_str().unwrap(),
        ],
        0,
    );
    let input = object([
        ("comparison_request", request),
        ("comparison_proof", proof),
        ("capture", Value::Null),
        ("context", object([] as [(&str, Value); 0])),
    ]);
    let path = save(&f.r.world, &input, "compact-input.json");
    let plan = f.call(
        &[
            "compact",
            "plan",
            "--repo",
            f.repo(),
            "--input",
            path.to_str().unwrap(),
        ],
        0,
    );
    let p = save(&f.r.world, &plan, "compact-plan.json");
    let prepared = f.call(
        &[
            "compact",
            "preview",
            "--repo",
            f.repo(),
            "--input",
            path.to_str().unwrap(),
            "--plan",
            p.to_str().unwrap(),
        ],
        0,
    );
    (get(&prepared, "proposal").clone(), first, vec![p1, p2])
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_installed_bank_initialization.py::InstalledBankRoundtrip.test_cold_identity_initialization_and_checkpoint_have_no_bank_dependency
fn cold_identity_initialization_and_checkpoint_have_no_bank_dependency() {
    let w = World::new().unwrap();
    let (_, cli) = support::installed_runtime::installed_runtime(&w);
    let repo = w.repository("cold-repo", false).unwrap();
    fs::create_dir(repo.join("src")).unwrap();
    fs::write(repo.join("src/app.rs"), "const RETRY_LIMIT: u32 = 3;\n").unwrap();
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..Default::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Implementation evidence"],
        RunOptions {
            cwd: Some(&repo),
            ..Default::default()
        },
    )
    .unwrap();
    let developer = call(
        &w,
        &cli,
        &[
            "developer",
            "preview",
            "--create",
            "--label",
            "Explicit detecting developer",
        ],
        0,
    );
    let path = save(&w, &developer, "developer.json");
    let developer = call(
        &w,
        &cli,
        &["developer", "apply", "--proposal", path.to_str().unwrap()],
        0,
    );
    let proposal = call(
        &w,
        &cli,
        &[
            "identity",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--action",
            "create",
            "--reason",
            "Explicit new repository identity",
        ],
        0,
    );
    let path = save(&w, &proposal, "identity.json");
    let identity = call(
        &w,
        &cli,
        &["identity", "apply", "--proposal", path.to_str().unwrap()],
        0,
    );
    assert!(!repo.join("memory-bank").exists());
    let inventory = call(
        &w,
        &cli,
        &["init", "inspect", "--repo", repo.to_str().unwrap()],
        0,
    );
    let claim = object([
        ("context", text("techContext.md")),
        ("fact", text("implementation")),
        ("fact_key", text("retry-limit")),
        ("summary", text("Reviewed implementation retry limit: 3.")),
        ("value", Value::Int(3.into())),
        ("scope", text("repository")),
        ("conditions", object([("branch", text("main"))])),
        ("evidence", Value::Array(vec![text("src/app.rs")])),
        ("current", Value::Bool(true)),
        ("applicable", Value::Bool(true)),
        ("qualified", Value::Bool(true)),
        ("authority_kind", text("code")),
    ]);
    let reviewed = object([
        ("inventory", inventory),
        ("existing_memory_reviewed", Value::Bool(false)),
        ("claims", Value::Array(vec![claim])),
        ("questions", Value::Array(vec![])),
    ]);
    let path = save(&w, &reviewed, "initialization.json");
    let p = call(
        &w,
        &cli,
        &[
            "init",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--input",
            path.to_str().unwrap(),
        ],
        0,
    );
    let path = save(&w, &p, "initialize-proposal.json");
    let done = call(
        &w,
        &cli,
        &["init", "apply", "--proposal", path.to_str().unwrap()],
        0,
    );
    assert!(yes(&done, "verified"));
    let status = call(
        &w,
        &cli,
        &["identity", "status", "--repo", repo.to_str().unwrap()],
        0,
    );
    assert_eq!(get(&status, "project_id"), get(&identity, "project_id"));
    assert_eq!(
        get(&status, "developer_id"),
        get(&developer, "developer_id")
    );
    let bank = repo.join("memory-bank");
    let before = physical(&bank);
    let inspect = call(
        &w,
        &cli,
        &["checkpoint", "inspect", "--repo", repo.to_str().unwrap()],
        0,
    );
    let payload = object([
        ("focus", text("Inspect current implementation conditions.")),
        (
            "recent_changes",
            Value::Array(vec![text("Initialized from reviewed source.")]),
        ),
        ("open_questions", Value::Array(vec![])),
        ("next_step", text("Review another actor's observations.")),
        ("evidence", Value::Array(vec![])),
        ("conditions", object([] as [(&str, Value); 0])),
        ("harness", text("custom")),
        ("session", text("cold-session")),
        (
            "observed_active_sha256",
            get(&inspect, "observed_active_sha256").clone(),
        ),
    ]);
    let path = save(&w, &payload, "checkpoint-input.json");
    let p = call(
        &w,
        &cli,
        &[
            "checkpoint",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--input",
            path.to_str().unwrap(),
        ],
        0,
    );
    let path = save(&w, &p, "checkpoint-proposal.json");
    let done = call(
        &w,
        &cli,
        &["checkpoint", "apply", "--proposal", path.to_str().unwrap()],
        0,
    );
    assert!(yes(&done, "verified"));
    assert!(!yes(&done, "publication_verified"));
    let after = physical(&bank);
    for (path, bytes) in before {
        if path != "activeContext.md" {
            assert_eq!(after.get(&path), Some(&bytes));
        }
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_installed_bank_qualified_recall.py::InstalledBankRoundtrip.test_qualified_recall_aliases_preserve_ids_conditions_validity_and_provenance
fn qualified_recall_aliases_preserve_ids_conditions_validity_and_provenance() {
    let f = Installed::new();
    let path = f.r.add(
        "qualified",
        "# Retry probe\n\nThe verified retry probe remains conditional.\n",
        &object([("conditions", object([("component", text("app.rs"))]))]),
    );
    let expected = metadata(&f.r.bank, &path);
    let mut request =
        f.r.request(&[path.clone()], object([("component", text("app.rs"))]));
    put(
        &mut request,
        "ids",
        Value::Array(vec![get(&expected, "id").clone()]),
    );
    put(&mut request, "paths", Value::Array(vec![]));
    let status = f.call(&["identity", "status", "--repo", f.repo()], 0);
    for alias in ["search", "get", "explain"] {
        let result = f.recall(&request, alias);
        let record = get(&result, "results")
            .as_array()
            .unwrap()
            .first()
            .unwrap()
            .get("record")
            .unwrap();
        for key in [
            "id",
            "version",
            "conditions",
            "origin",
            "valid-from",
            "valid-until",
            "status",
        ] {
            assert_eq!(get(record, key), get(&expected, key));
        }
        assert_eq!(
            get(get(&result, "scope_context"), "developer_id"),
            get(&status, "developer_id")
        );
    }
    put(
        &mut request,
        "context",
        object([("component", text("other.rs"))]),
    );
    assert_eq!(
        get(&f.recall(&request, "get"), "results"),
        &Value::Array(vec![])
    );
    let alias = f.r.world.root.join("bin/am");
    fs::remove_file(&alias).unwrap();
    assert!(!alias.exists());
    assert!(
        fs::read_to_string(f.r.bank.join("MEMORY.md"))
            .unwrap()
            .contains(&path)
    );
    assert!(
        fs::read_to_string(f.r.bank.join(path))
            .unwrap()
            .contains("verified retry probe")
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_portable_handoff.py::PortableHandoff.test_unknown_conditions_malformed_framing_and_stale_evidence_stay_qualified
fn unknown_conditions_malformed_framing_and_stale_evidence_stay_qualified() {
    let f = Installed::new();
    fs::write(f.r.repo.join("result.txt"), "Actual source\n").unwrap();
    let p = f.checkpoint(
        object([("task", Value::Null)]),
        vec![text("../result.txt")],
        false,
    );
    assert!(yes(&f.apply_checkpoint(&p), "verified"));
    let request = f.r.request(
        &["activeContext.md".into()],
        object([("task", text("task-A"))]),
    );
    assert_eq!(
        get(&f.recall(&request, "get"), "results"),
        &Value::Array(vec![])
    );
    let history = f.call(
        &["inspect", "--repo", f.repo(), "--path", "activeContext.md"],
        0,
    );
    assert!(!yes(&history, "usable_as_current_guidance"));
    assert!(format!("{history:?}").contains("unknown-condition:task"));
    let p = f.checkpoint(
        object([("task", text("task-A"))]),
        vec![text("../result.txt")],
        false,
    );
    f.apply_checkpoint(&p);
    let target = f.r.bank.join("activeContext.md");
    let original = fs::read_to_string(&target).unwrap();
    fs::write(
        &target,
        original.replacen("- ../result.txt\n", "- ../different-result.txt\n", 1),
    )
    .unwrap();
    let result = f.recall(&request, "get");
    let row = get(&result, "results").as_array().unwrap().first().unwrap();
    assert_eq!(
        string(get(row, "context_assessment"), "evidence_status"),
        "historical-or-unqualified-evidence"
    );
    fs::write(
        &target,
        format!("{original}\n## Checkpoint conditions\n\n```json\n{{}}\n```\n"),
    )
    .unwrap();
    assert_eq!(
        get(&f.recall(&request, "get"), "results"),
        &Value::Array(vec![])
    );
    assert_eq!(
        string(
            &f.call(
                &["inspect", "--repo", f.repo(), "--path", "activeContext.md"],
                0
            ),
            "body"
        ),
        fs::read_to_string(&target).unwrap()
    );
    fs::write(
        &target,
        "# Manual project orientation\n\nAn unresolved question; no task is assigned.\n",
    )
    .unwrap();
    let result = f.recall(&request, "get");
    let row = get(&result, "results").as_array().unwrap().first().unwrap();
    assert!(!yes(get(row, "context_assessment"), "managed_checkpoint"));
    assert!(!yes(row, "assignment"));
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_portable_handoff.py::PortableHandoff.test_preparation_recovery_retains_original_source_pins_and_can_rollback
fn preparation_recovery_retains_original_source_pins_and_can_rollback() {
    let f = Installed::new();
    fs::remove_file(f.r.bank.join("activeContext.md")).unwrap();
    // Re-render the existing bank with that context explicitly omitted.
    write_files(
        &f.r.bank,
        &render(
            &[(
                "projectbrief.md",
                "# Project\n\nKeep source-bound preparation.\n",
            )],
            "2026-10-22T17:00:00Z",
        ),
    );
    fs::write(f.r.repo.join("result.txt"), "Original source\n").unwrap();
    let before = canonical(&f.r.bank);
    let p = f.checkpoint(
        object([] as [(&str, Value); 0]),
        vec![text("../result.txt")],
        true,
    );
    let path = save(
        &f.r.world,
        get(&p, "transaction"),
        "prepare-transaction.json",
    );
    let env = [
        (
            "AGENT_MEMORY_FAULT_STAGE".into(),
            "transaction-after-write:activeContext.md".into(),
        ),
        ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
    ];
    let output =
        f.r.world
            .run_with(
                [
                    f.cli.as_os_str(),
                    std::ffi::OsStr::new("--json"),
                    std::ffi::OsStr::new("transaction"),
                    std::ffi::OsStr::new("apply"),
                    std::ffi::OsStr::new("--proposal"),
                    path.as_os_str(),
                ],
                RunOptions {
                    env: &env,
                    check: false,
                    ..Default::default()
                },
            )
            .unwrap();
    assert_eq!(output.status, Status::Code(86));
    let unlock = f.call(&["writes", "recover", "--repo", f.repo()], 0);
    let path = save(&f.r.world, &unlock, "unlock.json");
    f.call(
        &[
            "writes",
            "recover",
            "--repo",
            f.repo(),
            "--proposal",
            path.to_str().unwrap(),
        ],
        0,
    );
    fs::write(
        f.r.repo.join("result.txt"),
        "Changed source after partial preparation\n",
    )
    .unwrap();
    f.call(
        &[
            "transaction",
            "recover",
            "--repo",
            f.repo(),
            "--mode",
            "finish",
        ],
        1,
    );
    let p = f.call(
        &[
            "transaction",
            "recover",
            "--repo",
            f.repo(),
            "--mode",
            "rollback",
        ],
        0,
    );
    let path = save(&f.r.world, &p, "rollback.json");
    assert!(yes(
        &f.call(
            &[
                "transaction",
                "recover",
                "--repo",
                f.repo(),
                "--proposal",
                path.to_str().unwrap()
            ],
            0
        ),
        "verified"
    ));
    same_map(&canonical(&f.r.bank), &before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_unified_cli_operations.py::UnifiedCLIOperations.test_installed_context_requires_an_actual_current_text_bound_token_count
fn installed_context_requires_current_text_bound_token_count() {
    let f = Installed::new();
    f.r.add(
        "unicode",
        "# Café\n\nA scoped café observation with 🧠 as inert data.\n",
        &object([] as [(&str, Value); 0]),
    );
    let mut request = object([
        ("recall", f.r.request(&[], object([] as [(&str, Value); 0]))),
        ("max_bytes", Value::Int(65536.into())),
        ("token_budget", Value::Int(65536.into())),
        ("conflicts", Value::Array(vec![])),
    ]);
    let path = save(&f.r.world, &request, "assembly.json");
    let preview = f.call(
        &[
            "context",
            "preview",
            "--repo",
            f.repo(),
            "--request",
            path.to_str().unwrap(),
        ],
        0,
    );
    assert!(!yes(&preview, "usable_context"));
    let counter = object([
        ("schema", text("agent-memory/token-count-v1")),
        ("source_kind", text("harness-tokenizer")),
        ("tokenizer", text("disposable-codepoint-tokenizer-v1")),
        ("context_sha256", get(&preview, "context_sha256").clone()),
        (
            "tokens",
            Value::Int(string(&preview, "context_text").chars().count().into()),
        ),
    ]);
    let count = save(&f.r.world, &counter, "counter.json");
    let checked = f.call(
        &[
            "context",
            "check",
            "--repo",
            f.repo(),
            "--request",
            path.to_str().unwrap(),
            "--counter",
            count.to_str().unwrap(),
        ],
        0,
    );
    assert!(yes(&checked, "usable_context"));
    assert_eq!(get(&checked, "context_text"), get(&preview, "context_text"));
    assert!(!yes(&checked, "token_counter_assertions_verified"));
    put(&mut request, "token_budget", Value::Int(1.into()));
    let path = save(&f.r.world, &request, "tiny-assembly.json");
    assert_eq!(
        get(
            &f.call(
                &[
                    "context",
                    "check",
                    "--repo",
                    f.repo(),
                    "--request",
                    path.to_str().unwrap(),
                    "--counter",
                    count.to_str().unwrap()
                ],
                0
            ),
            "context_text"
        ),
        &Value::Null
    );
    put(&mut request, "token_budget", Value::Int(65536.into()));
    let path = save(&f.r.world, &request, "assembly.json");
    for (key, value) in [
        ("tokens", Value::Bool(true)),
        ("tokens", Value::Int(0.into())),
        ("context_sha256", text("0".repeat(64))),
    ] {
        let mut bad = counter.clone();
        put(&mut bad, key, value);
        let bad = save(&f.r.world, &bad, "bad-counter.json");
        f.call(
            &[
                "context",
                "check",
                "--repo",
                f.repo(),
                "--request",
                path.to_str().unwrap(),
                "--counter",
                bad.to_str().unwrap(),
            ],
            1,
        );
    }
    f.r.add(
        "new",
        "# New current knowledge\n\nContext has changed.\n",
        &object([] as [(&str, Value); 0]),
    );
    let before = physical(&f.r.bank);
    f.call(
        &[
            "context",
            "check",
            "--repo",
            f.repo(),
            "--request",
            path.to_str().unwrap(),
            "--counter",
            count.to_str().unwrap(),
        ],
        1,
    );
    assert_eq!(physical(&f.r.bank), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_unified_cli_operations.py::UnifiedCLIOperations.test_installed_compaction_preserves_distinct_provenance_history_and_converges
fn installed_compaction_preserves_provenance_history_and_converges() {
    let f = Installed::new();
    let (proposal, first, paths) = compact(&f);
    let before_code = f.r.code();
    let p = save(&f.r.world, &proposal, "compact.json");
    let done = f.call(&["compact", "apply", "--proposal", p.to_str().unwrap()], 0);
    assert!(yes(&done, "consolidation_verified"));
    let files = canonical(&f.r.bank);
    let body = format!("{files:?}");
    assert!(body.contains("user:first-origin"));
    assert!(body.contains("user:second-origin"));
    assert!(body.contains(string(&first, "id")));
    assert!(body.contains("consolidat"));
    let request = f.r.request(&paths, object([] as [(&str, Value); 0]));
    let recalled = f.recall(&request, "get");
    let active = get(&recalled, "results").as_array().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(get(get(&active[0], "record"), "id"), get(&first, "id"));
    assert_eq!(f.r.code(), before_code);
    let before = physical(&f.r.bank);
    let repeated = f.call(&["compact", "apply", "--proposal", p.to_str().unwrap()], 0);
    assert!(yes(&repeated, "already_applied"));
    assert_eq!(physical(&f.r.bank), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_recall_observation.py::RecallObservationContract.test_owned_lock_disappearance_or_replacement_refuses_public_maintenance_before_write
fn owned_lock_disappearance_or_replacement_refuses_before_maintenance_write() {
    for changed in [false, true] {
        let f = Installed::new();
        let (proposal, _, _) = compact(&f);
        let before = canonical(&f.r.bank);
        let code = f.r.code();
        let p = save(&f.r.world, &proposal, "maintenance.json");
        let (child, release) = pause(
            &f.r.world,
            &f.cli,
            &["compact", "apply", "--proposal", p.to_str().unwrap()],
            "transaction-locked-before-inventory",
        );
        let lock = f.r.bank.join(".agent-memory.lock");
        assert!(lock.is_file());
        if changed {
            fs::write(
                &lock,
                "Foreign replacement injected by the fixture owner.\n",
            )
            .unwrap();
        } else {
            fs::remove_file(&lock).unwrap();
        }
        fs::write(release, b"release").unwrap();
        let output = child.wait().unwrap();
        assert_eq!(output.status, Status::Code(1));
        assert!(!yes(&decode(&output), "verified"));
        if changed {
            assert_eq!(
                fs::read_to_string(&lock).unwrap(),
                "Foreign replacement injected by the fixture owner.\n"
            );
            fs::remove_file(lock).unwrap();
        }
        same_map(&canonical(&f.r.bank), &before);
        assert_eq!(f.r.code(), code);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_turn_sources.py::Sources.test_real_prompt_registration_steering_stdin_ack_and_cap_blocks_input
fn real_prompt_registration_steering_stdin_ack_and_cap_blocks_input() {
    let f = Installed::new();
    let session = agent_memory::host::uuid4().unwrap();
    let turn = agent_memory::host::uuid4().unwrap();
    let event = |prompt: &str| {
        object([
            ("hook_event_name", text("UserPromptSubmit")),
            ("session_id", text(&session)),
            ("turn_id", text(&turn)),
            ("cwd", text(f.repo())),
            ("prompt", text(prompt)),
        ])
    };
    for prompt in [
        "first actual input",
        "same-turn steering",
        "first actual input",
    ] {
        let result = stdin(
            &f.r.world,
            &f.cli,
            &["hook", "codex-prompt"],
            &event(prompt),
        );
        assert_eq!(result.status, Status::Code(0));
        assert!(decode(&result).get("hookSpecificOutput").is_some());
    }
    let request = object([
        ("action", text("prompt-inventory")),
        ("repo", text(f.repo())),
        ("runtime", text(f.runtime.to_str().unwrap())),
        ("session", text(&session)),
        ("turn", text(&turn)),
    ]);
    let inventory = library(&f.r.world, &request);
    assert_eq!(get(&inventory, "sources").as_array().unwrap().len(), 3);
    let first = get(&inventory, "sources")
        .as_array()
        .unwrap()
        .first()
        .unwrap();
    let ack = object([
        ("session", text(&session)),
        ("prompt", text(&turn)),
        ("observation_id", get(first, "observation_id").clone()),
        ("source_sha256", get(first, "source_sha256").clone()),
        ("outcome", text("no-learning")),
        (
            "assessment",
            object([
                ("source_id", get(first, "observation_id").clone()),
                (
                    "source_review",
                    text("Reviewed the exact short fixture input; it has no durable learning."),
                ),
                ("candidates", Value::Array(vec![])),
            ]),
        ),
    ]);
    let done = stdin(
        &f.r.world,
        &f.cli,
        &["codex-boundary", "--repo", f.repo(), "--stdin"],
        &ack,
    );
    assert_eq!(done.status, Status::Code(0));
    assert!(yes(&decode(&done), "verified"));
    let mut cap = request.clone();
    put(&mut cap, "action", text("prompt-cap"));
    library(&f.r.world, &cap);
    let blocked = stdin(
        &f.r.world,
        &f.cli,
        &["hook", "codex-prompt"],
        &event("must not pass source capacity"),
    );
    assert_eq!(string(&decode(&blocked), "decision"), "block");
    assert!(format!("{:?}", library(&f.r.world, &request)).contains("registration-fault-retained"));
}

fn library(w: &World, request: &Value) -> Value {
    let path = save(w, request, "library-request.json");
    let env = [(
        "AGENT_MEMORY_FAULT_GAPS_LIBRARY".into(),
        path.into_os_string(),
    )];
    let output = w
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "gap_library_worker".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                env: &env,
                ..Default::default()
            },
        )
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let raw = stdout
        .lines()
        .find_map(|line| line.strip_prefix("GAPS-LIBRARY:"))
        .unwrap();
    json::decode(raw, Default::default()).unwrap()
}
#[test]
fn gap_library_worker() {
    let Some(path) = std::env::var_os("AGENT_MEMORY_FAULT_GAPS_LIBRARY") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    let request = json::decode_bytes(&fs::read(path).unwrap(), Default::default()).unwrap();
    let repo = Path::new(string(&request, "repo"));
    let runtime = Path::new(string(&request, "runtime"));
    let value = match string(&request, "action") {
        "hook-failures" => {
            agent_memory::hook_failure::record(
                repo,
                "codex",
                "prompt",
                "identity",
                "identity.unresolved",
            )
            .unwrap();
            agent_memory::hook_failure::record(
                repo,
                "codex",
                "stop",
                "boundary",
                "boundary.missing",
            )
            .unwrap();
            Value::Bool(true)
        }
        "prompt-inventory" | "prompt-cap" => {
            let session = string(&request, "session");
            let turn = string(&request, "turn");
            if string(&request, "action") == "prompt-cap" {
                let (namespace, _) =
                    agent_memory::turn_sources::namespace(repo, "codex", session, turn, false)
                        .unwrap();
                for index in 3..128 {
                    let path = namespace.directory.join(format!("source-{index}.json"));
                    fs::write(&path, "{reserved").unwrap();
                    support::lifecycle::mode(&path, 0o600);
                }
            }
            agent_memory::turn_sources::inspect(repo, "codex", session, turn, runtime).unwrap()
        }
        _ => panic!("unknown fixture operation"),
    };
    println!(
        "GAPS-LIBRARY:{}",
        json::encode(&value, json::DEFAULT).unwrap()
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_checkout_repair.py::CheckoutRepairContract.test_doctor_recognizes_and_persists_old_device_binding
fn doctor_recognizes_and_persists_old_device_binding() {
    let f = Installed::new();
    f.r.world
        .git_with(
            [
                "remote",
                "add",
                "origin",
                "https://github.com/example/memory-fixture.git",
            ],
            RunOptions {
                cwd: Some(&f.r.repo),
                ..Default::default()
            },
        )
        .unwrap();
    let preview =
        f.r.world
            .am_json(
                "am identity",
                &[
                    "preview".into(),
                    "--repo".into(),
                    f.r.repo.as_os_str().to_owned(),
                    "--action".into(),
                    "rebind".into(),
                    "--reason".into(),
                    "Reviewed added authoritative fixture remote".into(),
                ],
            )
            .unwrap();
    let proposal: Value = json::decode_bytes(&preview.stdout, Default::default()).unwrap();
    let path = save(&f.r.world, &proposal, "historical-rebind.json");
    f.r.world
        .am_json(
            "am identity",
            &["apply".into(), "--proposal".into(), path.into_os_string()],
        )
        .unwrap();
    let registry = f.r.world.paths["config"].join("agent-memory/projects.json");
    let mut value = support::lifecycle::read(&registry);
    let Value::Object(checkouts) = value.get_mut("checkouts").unwrap() else {
        panic!("checkouts")
    };
    let key = checkouts.first().unwrap().0.clone();
    let (device, inode) = key.split_once(':').unwrap();
    let old = format!("{}:{inode}", device.parse::<u64>().unwrap() + 100);
    let (_, binding) = checkouts.remove(0);
    checkouts.push((old.clone(), binding));
    support::lifecycle::save(&registry, &value);
    let before = physical(&f.r.bank);
    let observed = f.call(&["doctor", "--repo", f.repo()], 0);
    assert_eq!(
        string(get(&observed, "project"), "binding_source"),
        "historical-device-change"
    );
    assert!(
        get(&support::lifecycle::read(&registry), "checkouts")
            .get(&key)
            .is_none()
    );
    let fixed = f.call(&["doctor", "--fix", "--repo", f.repo()], 0);
    assert!(yes(&fixed, "repairs_performed"));
    let registry = support::lifecycle::read(&registry);
    assert!(get(&registry, "checkouts").get(&key).is_some());
    assert!(get(&registry, "checkouts").get(&old).is_some());
    assert!(!yes(
        &f.call(&["doctor", "--fix", "--repo", f.repo()], 0),
        "repairs_performed"
    ));
    assert_eq!(physical(&f.r.bank), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_checkout_repair.py::CheckoutRepairContract.test_doctor_reports_prompt_and_stop_failures_independently
fn doctor_reports_prompt_and_stop_failures_independently() {
    let f = Installed::new();
    library(
        &f.r.world,
        &object([
            ("action", text("hook-failures")),
            ("repo", text(f.repo())),
            ("runtime", text(f.runtime.to_str().unwrap())),
        ]),
    );
    let result = f.call(&["doctor", "--repo", f.repo()], 1);
    assert_eq!(get(&result, "hook_failures").as_array().unwrap().len(), 2);
    let warnings = get(&result, "warnings").as_array().unwrap();
    for component in ["hook.codex.prompt", "hook.codex.stop"] {
        assert!(
            warnings
                .iter()
                .any(|row| get(row, "component").as_str() == Some(component))
        );
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_discovery.py::DiscoveryContract.test_discovered_equivalent_cannot_be_omitted_from_comparison_to_admit_duplicate
fn discovered_equivalent_cannot_be_omitted_to_admit_duplicate() {
    let f = Installed::new();
    let retained = f.r.add(
        "retained-lesson",
        "# Retry policy\n\nRetry transient requests no more than three times.\n",
        &object([] as [(&str, Value); 0]),
    );
    let status = f.call(&["identity", "status", "--repo", f.repo()], 0);
    let mut candidate = json::decode(
        include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
        Default::default(),
    )
    .unwrap();
    let m = candidate.get_mut("metadata").unwrap();
    put(m, "project-id", text(PROJECT));
    put(m, "scope", text("repository"));
    put(m, "conditions", object([] as [(&str, Value); 0]));
    put(
        m,
        "source",
        object([
            ("kind", text("user-feedback")),
            ("actor", text("Explicit fixture correction")),
        ]),
    );
    put(
        m.get_mut("origin").unwrap(),
        "developer-id",
        get(&status, "developer_id").clone(),
    );
    put(m.get_mut("origin").unwrap(), "code-state", text("not-code"));
    let capture_request = object([
        ("project_id", text(PROJECT)),
        ("candidate", candidate.clone()),
        ("source_class", text("user-feedback")),
        ("repository_evidence", Value::Null),
        (
            "assessment",
            object([
                ("category", text("user-correction")),
                (
                    "utility",
                    text("Retain the bounded retry cap under the named conditions."),
                ),
                ("durable", Value::Bool(true)),
                ("evidence_confidence", text("confirmed")),
                (
                    "evidence_reason",
                    text("The operator explicitly confirmed the retry correction."),
                ),
                ("confirmed", Value::Bool(true)),
                ("source_redundant", Value::Bool(false)),
                ("work_state_copy", Value::Bool(false)),
                ("review", Value::Null),
            ]),
        ),
    ]);
    let path = save(&f.r.world, &capture_request, "capture-request.json");
    let captured = f.call(
        &[
            "capture",
            "--repo",
            f.repo(),
            "--request",
            path.to_str().unwrap(),
        ],
        0,
    );
    let trace = discovery_trace(&f.r, &[retained.clone()], "comparison");
    let request = object([
        ("project_id", text(PROJECT)),
        (
            "candidate",
            get(get(&captured, "capture"), "request")
                .get("candidate")
                .unwrap()
                .clone(),
        ),
        (
            "searches",
            Value::Array(vec![text("unrelatedquasar"), text("unrelatednebula")]),
        ),
        ("read_paths", Value::Array(vec![])),
        ("discovery", trace),
    ]);
    let before = canonical(&f.r.bank);
    let request_path = save(&f.r.world, &request, "comparison.json");
    let inspected = f.call(
        &[
            "compare",
            "inspect",
            "--repo",
            f.repo(),
            "--request",
            request_path.to_str().unwrap(),
        ],
        0,
    );
    assert_eq!(
        get(&inspected, "matches")
            .as_array()
            .unwrap()
            .iter()
            .map(|row| string(row, "path"))
            .collect::<Vec<_>>(),
        vec![retained.as_str()]
    );
    let judgment = object([
        ("observation", get(&inspected, "observation").clone()),
        (
            "decisions",
            Value::Array(vec![object([
                ("path", text(&retained)),
                ("classification", text("equivalent")),
                (
                    "rationale",
                    text(
                        "Both actual reviewed sources retain the same bounded retry cap and conditions.",
                    ),
                ),
            ])]),
        ),
        ("pending_decisions", Value::Array(vec![])),
        (
            "rationale",
            text("Discovery sources cannot disappear through unrelated lexical queries."),
        ),
    ]);
    let j = save(&f.r.world, &judgment, "discovery-judgment.json");
    let reviewed = f.call(
        &[
            "compare",
            "review",
            "--repo",
            f.repo(),
            "--request",
            request_path.to_str().unwrap(),
            "--judgment",
            j.to_str().unwrap(),
        ],
        0,
    );
    let proof = save(&f.r.world, get(&reviewed, "proof"), "discovery-proof.json");
    let capture = save(&f.r.world, get(&captured, "capture"), "capture.json");
    let result = f.call(
        &[
            "admission",
            "preview",
            "--repo",
            f.repo(),
            "--capture",
            capture.to_str().unwrap(),
            "--request",
            request_path.to_str().unwrap(),
            "--proof",
            proof.to_str().unwrap(),
        ],
        0,
    );
    assert_eq!(string(&result, "disposition"), "update-existing");
    assert_eq!(get(&result, "proposal"), &Value::Null);
    same_map(&canonical(&f.r.bank), &before);
}
