mod support;
use agent_memory::codec::json::{self, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use support::lifecycle::*;
use support::{RunOptions, World};
fn inspect(world: &World, pack: &Path) -> Value {
    run(
        world,
        &["artifact", "inspect", "--pack", pack.to_str().unwrap()],
        0,
    )
}
fn copied(world: &World) -> PathBuf {
    let root = world.root.join("copied complete artifact");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &root,
    );
    root
}

// obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_complete_copy_outside_git_has_the_same_identity_and_explicit_optional_prerequisites
#[test]

fn complete_copy_keeps_identity_and_explicit_optional_prerequisites() {
    let world = World::new().unwrap();
    let pack = copied(&world);
    let before = snapshot(&world.root);
    let original = inspect(
        &world,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
    );
    let copy = inspect(&world, &pack);
    assert_eq!(
        get(&original, "artifact_digest"),
        get(&copy, "artifact_digest")
    );
    assert_eq!(get(&original, "files"), get(&copy, "files"));
    assert_eq!(
        get(&copy, "file_count"),
        &Value::Int(get(&copy, "files").as_object().unwrap().len().into())
    );
    let compatibility = get(&copy, "compatibility");
    assert_eq!(
        get(compatibility, "readable_bank_schemas"),
        &Value::Array(vec![Value::String("agent-memory/v1".into())])
    );
    for key in [
        "migrator_source_schemas",
        "required_agent_clients",
        "required_python_packages",
    ] {
        assert_eq!(get(compatibility, key), &Value::Array(vec![]));
    }
    let adapter = &get(&copy, "optional_adapters").as_array().unwrap()[0];
    assert_eq!(
        get(adapter, "commands"),
        &Value::Array(vec![Value::String("gh".into())])
    );
    assert_eq!(get(adapter, "requires_authentication"), &Value::Bool(true));
    for key in [
        "writes_performed",
        "candidate_code_executed",
        "source_origin_authenticated",
        "installation_state_checked",
        "runtime_behavior_certified",
    ] {
        assert_eq!(get(&copy, key), &Value::Bool(false));
    }
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_inventory_digest_changes_for_actual_bytes_or_modes_without_claiming_a_signature
#[test]

fn actual_byte_and_mode_changes_change_digest_without_authenticating_source() {
    let world = World::new().unwrap();
    let pack = copied(&world);
    let original = inspect(&world, &pack);
    let path = pack.join("maintain/artifact/GUIDE.md");
    let raw = format!(
        "{}\nAdditional maintainer context.\n",
        std::fs::read_to_string(&path).unwrap()
    );
    std::fs::write(path, raw).unwrap();
    let changed = inspect(&world, &pack);
    assert_ne!(
        get(&changed, "artifact_digest"),
        get(&original, "artifact_digest")
    );
    let path = pack.join("templates/router-proposal.example.json");
    mode(&path, 0o600);
    let changed_mode = inspect(&world, &pack);
    assert_ne!(
        get(&changed_mode, "artifact_digest"),
        get(&changed, "artifact_digest")
    );
    assert_eq!(
        get(
            get(
                get(&changed_mode, "files"),
                "templates/router-proposal.example.json"
            ),
            "sha256"
        ),
        get(
            get(
                get(&changed, "files"),
                "templates/router-proposal.example.json"
            ),
            "sha256"
        )
    );
    assert_eq!(
        get(&changed_mode, "source_origin_authenticated"),
        &Value::Bool(false)
    );
}

// obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_every_implemented_documented_helper_imports_and_dispatches_from_the_copy
#[test]
fn every_documented_native_command_dispatches_from_a_complete_copy() {
    let world = World::new().unwrap();
    let source = source(&world);
    let installed = install(&world, &source, &["--presets", "none", "--mode", "copy"]);
    let cli = command(get(&installed, "receipt"));
    let value = inspect(&world, &source.join("skills/agent-memory"));
    let forms: BTreeSet<_> = get(&value, "documented_commands")
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            get(row, "command")
                .as_str()
                .is_some_and(|name| name.starts_with("am"))
        })
        .map(|row| {
            (
                text(row, "command").to_owned(),
                get(row, "operation").as_str().map(str::to_owned),
            )
        })
        .collect();
    assert!(forms.len() >= 66);
    let before = snapshot(&world.root);
    for (command, operation) in forms {
        let mut args = vec![cli.to_string_lossy().into_owned()];
        args.extend(command.split_whitespace().skip(1).map(str::to_owned));
        if let Some(operation) = operation {
            args.push(operation);
        }
        args.push("--help".into());
        world.run(args).unwrap();
    }
    assert_eq!(snapshot(&world.root), before);
}

fn invoke(world: &World, cli: &Path, args: &[&str]) -> Value {
    let argv = std::iter::once(cli.to_str().unwrap())
        .chain(["--json"])
        .chain(args.iter().copied());
    let output = world.run(argv).unwrap();
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}
fn apply(world: &World, cli: &Path, group: &str, proposal: &Value, name: &str) -> Value {
    let path = world.root.join(name);
    save(&path, proposal);
    invoke(
        world,
        cli,
        &[group, "apply", "--proposal", path.to_str().unwrap()],
    )
}

// obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_copied_helpers_initialize_checkpoint_and_recall_while_preserving_unfinished_code
#[test]
#[ignore = "needs recovery_frontier registered"]
fn copied_native_runtime_initializes_checkpoints_recalls_and_preserves_unfinished_code() {
    let world = World::new().unwrap();
    let source = source(&world);
    let installed = install(&world, &source, &["--presets", "none", "--mode", "copy"]);
    let cli = command(get(&installed, "receipt"));
    let repo = world.repository("portable project", false).unwrap();
    std::fs::write(
        repo.join("README.md"),
        b"# Retry worker\n\nInvestigate retry behavior before generalizing it.\n",
    )
    .unwrap();
    std::fs::write(repo.join("app.rs"), b"const RETRY_LIMIT: usize = 3;\n").unwrap();
    let git = |args: &[&str]| {
        world
            .git_with(
                args,
                RunOptions {
                    cwd: Some(&repo),
                    ..Default::default()
                },
            )
            .unwrap()
    };
    git(&["add", "."]);
    git(&["commit", "-m", "Actual disposable source"]);
    let developer = invoke(
        &world,
        &cli,
        &[
            "developer",
            "preview",
            "--create",
            "--label",
            "Portable-copy developer",
        ],
    );
    assert_eq!(
        get(
            &apply(&world, &cli, "developer", &developer, "developer.json"),
            "verified"
        ),
        &Value::Bool(true)
    );
    let proposal = invoke(
        &world,
        &cli,
        &[
            "identity",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--action",
            "create",
            "--reason",
            "Explicit isolated project setup",
        ],
    );
    let identity = apply(&world, &cli, "identity", &proposal, "project.json");
    let project = text(&identity, "project_id");
    std::fs::write(
        repo.join("app.rs"),
        b"const RETRY_LIMIT: usize = 4; // unfinished local change\n",
    )
    .unwrap();
    git(&["add", "app.rs"]);
    std::fs::write(repo.join("scratch.txt"), b"Preserve this unrelated note.\n").unwrap();
    let before = (
        std::fs::read(repo.join("app.rs")).unwrap(),
        std::fs::read(repo.join("scratch.txt")).unwrap(),
        std::fs::read(repo.join(".git/index")).unwrap(),
        git(&["rev-parse", "HEAD"]).stdout,
    );
    let inventory = invoke(
        &world,
        &cli,
        &["init", "inspect", "--repo", repo.to_str().unwrap()],
    );
    let review = Value::Object(vec![
        ("inventory".into(), inventory),
        ("existing_memory_reviewed".into(), Value::Bool(false)),
        ("claims".into(), Value::Array(vec![])),
        ("questions".into(), Value::Array(vec![])),
    ]);
    let review_path = world.root.join("initialization-review.json");
    save(&review_path, &review);
    let proposal = invoke(
        &world,
        &cli,
        &[
            "init",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--review",
            review_path.to_str().unwrap(),
        ],
    );
    assert_eq!(
        get(
            &apply(&world, &cli, "init", &proposal, "initialize.json"),
            "verified"
        ),
        &Value::Bool(true)
    );
    let bank = repo.join("memory-bank");
    let current = invoke(
        &world,
        &cli,
        &["checkpoint", "inspect", "--repo", repo.to_str().unwrap()],
    );
    assert_eq!(get(&current, "preparation_required"), &Value::Bool(false));
    let mut request = json::decode(r#"{"focus":"Review the retry component from this copied artifact.","recent_changes":["Inspected the actual local source map; feature code is still unfinished."],"open_questions":["Does the retry lesson apply beyond this worker?"],"next_step":"Run an isolated retry probe before admitting a durable lesson.","evidence":["../README.md"],"conditions":{"branch":"main"},"harness":"custom","session":"portable-copy"}"#, Default::default()).unwrap();
    if let Value::Object(fields) = &mut request {
        fields.push((
            "observed_active_sha256".into(),
            get(&current, "observed_active_sha256").clone(),
        ));
    }
    let input = world.root.join("checkpoint-input.json");
    save(&input, &request);
    let prior = snapshot(&bank);
    let proposal = invoke(
        &world,
        &cli,
        &[
            "checkpoint",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ],
    );
    let receipt = apply(&world, &cli, "checkpoint", &proposal, "checkpoint.json");
    assert_eq!(get(&receipt, "verified"), &Value::Bool(true));
    assert_eq!(get(&receipt, "publication_verified"), &Value::Bool(false));
    assert_eq!(
        get(&receipt, "changed_memory_paths"),
        &Value::Array(vec![Value::String("memory-bank/activeContext.md".into())])
    );
    for (path, value) in prior.iter().filter(|(path, _)| {
        path.as_os_str() != "activeContext.md"
            && path.extension().is_some_and(|extension| extension == "md")
    }) {
        assert_eq!(snapshot(&bank).get(path), Some(value));
    }
    let mut query = json::decode(r#"{"query":"","ids":[],"paths":["activeContext.md"],"context":{"branch":"main"},"kinds":[],"minimum_trust":"reviewed","limit":4,"include_context":true}"#, Default::default()).unwrap();
    if let Value::Object(fields) = &mut query {
        fields.push(("project_id".into(), Value::String(project.into())));
    }
    let input = world.root.join("recall.json");
    save(&input, &query);
    let recall = invoke(
        &world,
        &cli,
        &[
            "search",
            "--repo",
            repo.to_str().unwrap(),
            "--request",
            input.to_str().unwrap(),
        ],
    );
    let rows = get(&recall, "results").as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(text(&rows[0], "path"), "activeContext.md");
    assert!(text(&rows[0], "body").contains("Review the retry component"));
    assert_eq!(get(&recall, "publication_verified"), &Value::Bool(false));
    assert_eq!(
        (
            std::fs::read(repo.join("app.rs")).unwrap(),
            std::fs::read(repo.join("scratch.txt")).unwrap(),
            std::fs::read(repo.join(".git/index")).unwrap(),
            git(&["rev-parse", "HEAD"]).stdout
        ),
        before
    );
}
