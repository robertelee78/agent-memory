//! Initialization lane: `am init inspect|read|preview|apply` (baseline
//! `initialization.py`) in isolated Worlds. Commands run through the real
//! CLI dispatch in a child copy of this test binary whose whole environment
//! is the World's, so identity locations never reach the user's HOME.
use agent_memory::codec::json::{self, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

struct World {
    root: PathBuf,
    token: String,
}
impl World {
    fn new() -> Self {
        let tmp = std::fs::canonicalize("/tmp").unwrap();
        let token = agent_memory::host::uuid4().unwrap().replace('-', "");
        let root = tmp.join(format!("agent-memory-test-{}", &token[..16]));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["home", "config", "data", "state", "tmp"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        Self { root, token }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn pairs(&self) -> Vec<(String, String)> {
        let p = |name: &str| self.path(name).to_string_lossy().into_owned();
        vec![
            ("HOME".into(), p("home")),
            ("XDG_CONFIG_HOME".into(), p("config")),
            ("XDG_DATA_HOME".into(), p("data")),
            ("XDG_STATE_HOME".into(), p("state")),
            ("TMPDIR".into(), p("tmp")),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_AUTHOR_NAME".into(), "Test Developer".into()),
            ("GIT_AUTHOR_EMAIL".into(), "test@example.invalid".into()),
            ("GIT_COMMITTER_NAME".into(), "Test Developer".into()),
            ("GIT_COMMITTER_EMAIL".into(), "test@example.invalid".into()),
            (
                "AGENT_MEMORY_TEST_ROOT".into(),
                self.root.to_string_lossy().into_owned(),
            ),
            ("AGENT_MEMORY_TEST_TOKEN".into(), self.token.clone()),
        ]
    }
    fn git(&self, cwd: &Path, args: &[&str]) {
        let output = Command::new("/usr/bin/git")
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(self.pairs())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    /// Baseline `World.repository()`: an initialized `main` checkout.
    fn repository(&self) -> PathBuf {
        let repo = self.path("repo");
        std::fs::create_dir(&repo).unwrap();
        self.git(&repo, &["init", "-q", "-b", "main"]);
        repo
    }
    fn save(&self, value: &Value, name: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, json::encode(value, json::DEFAULT).unwrap()).unwrap();
        path
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Child entry: one `am` command through the real dispatch, in the World
/// named by the environment. Inert in parents.
#[test]
fn child_entry() {
    let Ok(arguments) = std::env::var("AM_INIT_CHILD") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    // dev-tools: the runtime pack of a source-checkout run (baseline
    // `artifact.PACK` from this repository); the test binary is not installed.
    agent_memory::cli::use_development_pack(Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
    ));
    let args: Vec<String> = match json::decode(&arguments, Default::default()).unwrap() {
        Value::Array(items) => items
            .into_iter()
            .map(|item| item.as_str().unwrap().to_owned())
            .collect(),
        _ => panic!("arguments"),
    };
    let dispatched = agent_memory::cli::dispatch(&args, "am");
    println!("AM-CODE:{}", dispatched.result.exit_code);
    let printed = Value::String(dispatched.result.stdout);
    println!(
        "AM-STDOUT:{}",
        json::encode(&printed, json::COMPACT_ASCII).unwrap()
    );
}

fn command(world: &World, args: &[&str]) -> (i32, Value) {
    let arguments = Value::Array(args.iter().map(|arg| text_value(arg)).collect());
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "child_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "-q",
        ])
        .env_clear()
        .envs(world.pairs())
        .env(
            "AM_INIT_CHILD",
            json::encode(&arguments, json::DEFAULT).unwrap(),
        )
        .current_dir(&world.root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code: i32 = stdout
        .lines()
        .find_map(|line| line.strip_prefix("AM-CODE:"))
        .unwrap_or_else(|| panic!("{stdout}\n{}", String::from_utf8_lossy(&output.stderr)))
        .parse()
        .unwrap();
    let printed = stdout
        .lines()
        .find_map(|line| line.strip_prefix("AM-STDOUT:"))
        .unwrap();
    let printed = json::decode(printed, Default::default()).unwrap();
    let value = json::decode(printed.as_str().unwrap(), Default::default()).unwrap();
    (code, value)
}
fn good(world: &World, args: &[&str]) -> Value {
    let (code, value) = command(world, args);
    assert_eq!(
        code,
        0,
        "{}",
        json::encode(&value, json::INDENT2_ASCII).unwrap()
    );
    value
}

fn text_value(value: &str) -> Value {
    Value::String(value.to_owned())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap_or_default()
}
fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn refs(value: &Value) -> BTreeSet<String> {
    get(value, "sources")
        .as_array()
        .unwrap()
        .iter()
        .map(|source| text(source, "ref").to_owned())
        .collect()
}
fn gaps(value: &Value) -> Vec<String> {
    get(value, "evidence_gaps")
        .as_array()
        .unwrap()
        .iter()
        .map(|gap| gap.as_str().unwrap().to_owned())
        .collect()
}
/// Every regular file below `root`, with its bytes and mode.
fn files(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, u32)> {
    let mut result = BTreeMap::new();
    if !root.exists() {
        return result;
    }
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let meta = std::fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), (std::fs::read(&path).unwrap(), meta.mode()));
        }
    }
    result
}

/// Baseline `setup` sources (identity setup needs the security tier).
fn sources(repo: &Path) {
    write(
        &repo.join("README.md"),
        "# Widget\n\nDOC_FULL_TEXT_CANARY should not be copied into memory.\n",
    );
    write(
        &repo.join("docs/adr/one.md"),
        "---\nstatus: accepted\n---\n# Retry intent\n\nThe approved target is five retries.\n",
    );
    write(
        &repo.join("docs/adr/two.md"),
        "---\nstatus: accepted\n---\n# Wall color\n\nThe wall is blue.\n",
    );
    write(&repo.join("src/app.py"), "RETRY_LIMIT = 3\n");
    write(&repo.join("tests/test_app.py"), "assert RETRY_LIMIT == 3\n");
}

/// Baseline `setup`: sources, an explicit developer and a new project identity.
fn setup(world: &World) -> (PathBuf, String, String) {
    let repo = world.repository();
    sources(&repo);
    let proposal = good(
        world,
        &[
            "developer",
            "preview",
            "--create",
            "--label",
            "Detecting developer",
        ],
    );
    let path = world.save(&proposal, "developer.json");
    let developer = good(
        world,
        &["developer", "apply", "--proposal", path.to_str().unwrap()],
    );
    let proposal = good(
        world,
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
    );
    let path = world.save(&proposal, "identity.json");
    let project = good(
        world,
        &["identity", "apply", "--proposal", path.to_str().unwrap()],
    );
    (
        repo,
        text(&project, "project_id").to_owned(),
        text(&developer, "developer_id").to_owned(),
    )
}

fn inventory(world: &World, repo: &Path, awa: Option<&Path>) -> Value {
    let repo = repo.to_str().unwrap();
    match awa {
        Some(awa) => good(
            world,
            &[
                "init",
                "inspect",
                "--repo",
                repo,
                "--awa-evidence",
                awa.to_str().unwrap(),
            ],
        ),
        None => good(world, &["init", "inspect", "--repo", repo]),
    }
}

fn review(inventory: &Value) -> Value {
    reviewed(inventory, Vec::new(), Vec::new())
}

/// Baseline `review(inventory, claims, questions)`.
fn reviewed(inventory: &Value, claims: Vec<Value>, questions: Vec<Value>) -> Value {
    Value::Object(vec![
        ("inventory".into(), inventory.clone()),
        (
            "existing_memory_reviewed".into(),
            Value::Bool(text(inventory, "bank_state") == "usable"),
        ),
        ("claims".into(), Value::Array(claims)),
        ("questions".into(), Value::Array(questions)),
    ])
}

fn set(target: &mut Value, key: &str, value: Value) {
    if let Value::Object(fields) = target {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = value,
            None => fields.push((key.to_owned(), value)),
        }
    }
}

/// Baseline `claim(fact, value, ref, key=, context=, summary=)`.
fn claim(
    fact: &str,
    value: Value,
    reference: &str,
    key: &str,
    context: &str,
    summary: Option<&str>,
) -> Value {
    let shown = json::encode(&value, json::DEFAULT).unwrap();
    let shown = value.as_str().map_or(shown, str::to_owned);
    let summary = summary.map_or_else(
        || format!("Reviewed {fact} retry limit: {shown}."),
        str::to_owned,
    );
    let conditions = if fact == "implementation" {
        Value::Object(vec![("branch".into(), text_value("main"))])
    } else {
        Value::Object(Vec::new())
    };
    let authority = match fact {
        "intent" => "approved-planning",
        "implementation" => "code",
        "delivery" => "issue",
        _ => "approved-decision",
    };
    Value::Object(vec![
        ("context".into(), text_value(context)),
        ("fact".into(), text_value(fact)),
        ("fact_key".into(), text_value(key)),
        ("summary".into(), text_value(&summary)),
        ("value".into(), value),
        ("scope".into(), text_value("repository")),
        ("conditions".into(), conditions),
        ("evidence".into(), Value::Array(vec![text_value(reference)])),
        ("current".into(), Value::Bool(true)),
        ("applicable".into(), Value::Bool(true)),
        ("qualified".into(), Value::Bool(true)),
        ("authority_kind".into(), text_value(authority)),
    ])
}

fn try_preview(world: &World, repo: &Path, review: &Value) -> (i32, Value) {
    let path = world.save(review, "review.json");
    command(
        world,
        &[
            "init",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--review",
            path.to_str().unwrap(),
        ],
    )
}
fn preview(world: &World, repo: &Path, review: &Value) -> Value {
    let (code, value) = try_preview(world, repo, review);
    assert_eq!(
        code,
        0,
        "{}",
        json::encode(&value, json::INDENT2_ASCII).unwrap()
    );
    value
}
/// The proposed transaction files of a preview.
fn proposed(preview: &Value) -> Value {
    get(get(get(preview, "proposal"), "transaction"), "files").clone()
}
fn bodies(preview: &Value) -> String {
    proposed(preview)
        .as_object()
        .unwrap()
        .iter()
        .map(|(_, body)| body.as_str().unwrap().to_owned())
        .collect()
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_actual_peer_intent_conflict_prompts_detecting_developer_and_never_chooses_landing_order
#[test]
fn actual_peer_intent_conflict_prompts_detecting_developer_and_never_chooses_landing_order() {
    let world = World::new();
    let (repo, _, developer) = setup(&world);
    let claims = vec![
        claim(
            "intent",
            text_value("yellow"),
            "docs/adr/one.md",
            "wall-color",
            "projectbrief.md",
            Some("The wall should be yellow."),
        ),
        claim(
            "intent",
            text_value("blue"),
            "docs/adr/two.md",
            "wall-color",
            "projectbrief.md",
            Some("The wall should be blue."),
        ),
    ];
    let observed = inventory(&world, &repo, None);
    let previewed = preview(&world, &repo, &reviewed(&observed, claims, vec![]));
    let questions = get(&previewed, "questions").as_array().unwrap();
    assert_eq!(questions.len(), 1);
    assert_eq!(text(&questions[0], "detecting_developer_id"), developer);
    assert!(text(&questions[0], "question").contains("wall-color"));
    let all = bodies(&previewed);
    assert!(!all.contains("The wall should be yellow."));
    assert!(!all.contains("The wall should be blue."));
    assert!(
        text(&proposed(&previewed), "activeContext.md")
            .contains("Product questions for the detecting developer")
    );
    assert_eq!(
        get(&questions[0], "evidence"),
        &Value::Array(vec![
            text_value("docs/adr/one.md"),
            text_value("docs/adr/two.md")
        ])
    );
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_implementation_conditions_match_known_branch_and_exact_revision
#[test]
fn implementation_conditions_match_known_branch_and_exact_revision() {
    let world = World::new();
    let (repo, _, _) = setup(&world);
    world.git(&repo, &["add", "."]);
    world.git(&repo, &["commit", "-q", "-m", "Inspected source baseline"]);
    let observed = inventory(&world, &repo, None);
    let head = text(get(&observed, "local_git_context"), "head").to_owned();
    let with = |conditions: Vec<(&str, &str)>| {
        let mut item = claim(
            "implementation",
            Value::Int(3.into()),
            "src/app.py",
            "retry-limit",
            "projectbrief.md",
            None,
        );
        set(
            &mut item,
            "conditions",
            Value::Object(
                conditions
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), text_value(value)))
                    .collect(),
            ),
        );
        item
    };
    let other = "b".repeat(40);
    for conditions in [
        vec![("revision", other.as_str())],
        vec![("branch", "another")],
        vec![("branch", "main"), ("revision", other.as_str())],
    ] {
        let (code, _) = try_preview(
            &world,
            &repo,
            &reviewed(&observed, vec![with(conditions)], vec![]),
        );
        assert_eq!(code, 1);
    }
    preview(
        &world,
        &repo,
        &reviewed(&observed, vec![with(vec![("revision", &head)])], vec![]),
    );
    world.git(&repo, &["checkout", "-q", "--detach", &head]);
    let detached = inventory(&world, &repo, None);
    assert_eq!(
        get(get(&detached, "local_git_context"), "branch"),
        &Value::Null
    );
    let (code, _) = try_preview(
        &world,
        &repo,
        &reviewed(&detached, vec![with(vec![("branch", "main")])], vec![]),
    );
    assert_eq!(code, 1);
    preview(
        &world,
        &repo,
        &reviewed(&detached, vec![with(vec![("revision", &head)])], vec![]),
    );
    world.git(&repo, &["checkout", "-q", "--orphan", "unborn"]);
    let unborn = inventory(&world, &repo, None);
    assert_eq!(get(get(&unborn, "local_git_context"), "head"), &Value::Null);
    let (code, _) = try_preview(
        &world,
        &repo,
        &reviewed(&unborn, vec![with(vec![("revision", &head)])], vec![]),
    );
    assert_eq!(code, 1);
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_discovery_and_reads_are_bounded_readonly_and_awa_gap_does_not_hide_local_evidence
#[test]
fn discovery_and_reads_are_bounded_readonly_and_awa_gap_does_not_hide_local_evidence() {
    let world = World::new();
    let (repo, project, _) = setup(&world);
    let before = files(&world.path("config"));
    let observed = inventory(&world, &repo, None);
    assert_eq!(
        text(&observed, "schema"),
        "agent-memory/initialization-result-v1"
    );
    assert_eq!(text(&observed, "bank_state"), "absent");
    assert_eq!(text(&observed, "project_id"), project);
    let kinds: BTreeSet<&str> = get(&observed, "sources")
        .as_array()
        .unwrap()
        .iter()
        .map(|source| text(source, "kind"))
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["documentation", "planning", "code", "test"])
    );
    assert_eq!(text(get(&observed, "awa"), "availability"), "unavailable");
    assert!(gaps(&observed).iter().any(|gap| gap.contains("AWA")));
    assert!(!repo.join("memory-bank").exists());
    assert_eq!(files(&world.path("config")), before);
    let source = good(
        &world,
        &[
            "init",
            "read",
            "--repo",
            repo.to_str().unwrap(),
            "--path",
            "README.md",
        ],
    );
    assert_eq!(
        get(&source, "content_is_untrusted_data"),
        &Value::Bool(true)
    );
    assert!(text(&source, "content").contains("DOC_FULL_TEXT_CANARY"));
    assert_eq!(get(&source, "writes_performed"), &Value::Bool(false));
    assert_eq!(
        text(&source, "sha256"),
        agent_memory::release_distribution::digest(&std::fs::read(repo.join("README.md")).unwrap())
    );
}

/// `read` refuses hidden, traversing, excluded and linked paths with the
/// baseline refusal and a compact envelope.
#[test]
fn reads_refuse_unsafe_paths_with_the_error_envelope() {
    let world = World::new();
    let repo = world.repository();
    sources(&repo);
    std::os::unix::fs::symlink(repo.join("README.md"), repo.join("linked.md")).unwrap();
    let repo_text = repo.to_str().unwrap();
    for (path, message) in [
        (
            "../README.md",
            "Read an explicit repository-relative source or canonical memory file",
        ),
        (
            ".git/config",
            "Read an explicit repository-relative source or canonical memory file",
        ),
        (
            "node_modules/x.md",
            "Read an explicit repository-relative source or canonical memory file",
        ),
        (
            "docs//adr/one.md",
            "Read an explicit repository-relative source or canonical memory file",
        ),
        ("linked.md", "Source links must not escape through symlinks"),
        (
            "missing.md",
            "Source is absent or exceeds the bounded reader",
        ),
    ] {
        let (code, value) = command(
            &world,
            &["init", "read", "--repo", repo_text, "--path", path],
        );
        assert_eq!(code, 1, "{path}");
        assert_eq!(text(&value, "error"), message, "{path}");
        assert_eq!(get(&value, "verified"), &Value::Bool(false));
        assert_eq!(get(&value, "inspection_required"), &Value::Bool(true));
        assert_eq!(text(&value, "operation"), "read");
    }
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_explicit_source_selection_respects_ignore_and_unknown_conditions_do_not_become_guidance
#[test]
fn explicit_source_selection_respects_ignore_and_unknown_conditions_do_not_become_guidance() {
    let world = World::new();
    let (repo, _, _) = setup(&world);
    write(&repo.join(".gitignore"), "private.md\n");
    write(&repo.join("private.md"), "PRIVATE_SOURCE_CANARY\n");
    let observed = inventory(&world, &repo, None);
    assert!(!refs(&observed).contains("private.md"));
    let repo_text = repo.to_str().unwrap();
    let (code, _) = command(
        &world,
        &[
            "init",
            "inspect",
            "--repo",
            repo_text,
            "--source",
            "private.md",
        ],
    );
    assert_eq!(code, 1);
    let selected = good(
        &world,
        &[
            "init",
            "inspect",
            "--repo",
            repo_text,
            "--source",
            "src/app.py",
            "--source",
            "docs/adr/one.md",
        ],
    );
    assert_eq!(
        refs(&selected),
        BTreeSet::from(["src/app.py".to_owned(), "docs/adr/one.md".to_owned()])
    );
    assert_eq!(
        get(&selected, "source_selection"),
        &Value::Array(vec![
            text_value("src/app.py"),
            text_value("docs/adr/one.md")
        ])
    );
    let mut unknown = claim(
        "implementation",
        Value::Int(3.into()),
        "src/app.py",
        "retry-limit",
        "techContext.md",
        Some("UNKNOWN_APPLICABILITY_CANARY"),
    );
    set(
        &mut unknown,
        "conditions",
        Value::Object(vec![("prerequisite".into(), Value::Null)]),
    );
    let previewed = preview(
        &world,
        &repo,
        &reviewed(&selected, vec![unknown.clone()], vec![]),
    );
    assert!(!bodies(&previewed).contains("UNKNOWN_APPLICABILITY_CANARY"));
    assert!(
        gaps(&previewed)
            .iter()
            .any(|gap| gap.contains("Unresolved source interpretation"))
    );
    set(&mut unknown, "conditions", Value::Object(Vec::new()));
    let (code, _) = try_preview(&world, &repo, &reviewed(&selected, vec![unknown], vec![]));
    assert_eq!(code, 1);
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_legacy_invalid_memory_and_visible_markers_are_preserved_while_local_sources_remain_inspectable
#[test]
fn legacy_invalid_memory_and_visible_markers_are_preserved_while_local_sources_remain_inspectable()
{
    let world = World::new();
    let repo = world.repository();
    sources(&repo);
    let bank = repo.join("memory-bank");
    write(
        &bank.join("legacy.md"),
        "Do not destroy this older knowledge.\n",
    );
    let observed = inventory(&world, &repo, None);
    assert_eq!(text(&observed, "bank_state"), "needs-inspection");
    assert!(!refs(&observed).is_empty());
    assert!(gaps(&observed).iter().any(|gap| {
        gap.starts_with("Existing memory needs inspection and will not be overwritten: ")
    }));
    let before = files(&bank);
    let path = world.save(&review(&observed), "review.json");
    let (code, value) = command(
        &world,
        &[
            "init",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--review",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1);
    assert_eq!(
        text(&value, "error"),
        "Existing memory is invalid, busy or interrupted; inspect/recover it without overwriting"
    );
    assert_eq!(files(&bank), before);
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_awa_skill_handoff_keeps_work_planned_and_wrong_project_or_unavailable_evidence_honest
#[test]
fn awa_skill_handoff_keeps_work_planned_and_wrong_project_or_unavailable_evidence_honest() {
    let world = World::new();
    let (repo, project, _) = setup(&world);
    let awa = |availability: &str, reason: &str, project: Value, sources: Vec<Value>| {
        Value::Object(vec![
            (
                "schema".into(),
                text_value("agent-memory/awa-initialization-evidence-v1"),
            ),
            ("project_id".into(), project),
            ("availability".into(), text_value(availability)),
            ("reason".into(), text_value(reason)),
            ("sources".into(), Value::Array(sources)),
        ])
    };
    let source = |kind: &str, reference: &str| {
        Value::Object(vec![
            ("kind".into(), text_value(kind)),
            ("ref".into(), text_value(reference)),
        ])
    };
    let available = awa(
        "available",
        "Read through the AWA skill in this fixture",
        text_value(&project),
        vec![
            source("project", "https://github.com/users/fixture/projects/1"),
            source("task", "https://github.com/fixture/widget/issues/1"),
            source("milestone", "https://github.com/fixture/widget/milestone/1"),
        ],
    );
    let path = world.save(&available, "awa.json");
    let observed = inventory(&world, &repo, Some(&path));
    assert_eq!(
        get(get(&observed, "awa"), "sources")
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(gaps(&observed).is_empty(), "{:?}", gaps(&observed));
    let planned = claim(
        "delivery",
        text_value("planned"),
        "https://github.com/fixture/widget/issues/1",
        "retry-limit",
        "progress.md",
        Some("The task is planned; no integration or release proof was supplied."),
    );
    let previewed = preview(&world, &repo, &reviewed(&observed, vec![planned], vec![]));
    let proposed = proposed(&previewed);
    let body = text(&proposed, "progress.md");
    assert!(body.contains("references do not prove delivery"), "{body}");
    assert!(body.contains("The task is planned"), "{body}");
    assert_eq!(
        text(get(&observed, "awa"), "sha256"),
        agent_memory::release_distribution::digest(&std::fs::read(&path).unwrap())
    );
    world.save(
        &awa(
            "unavailable",
            "Observed credentials unavailable",
            text_value(&project),
            Vec::new(),
        ),
        "awa.json",
    );
    let gap = inventory(&world, &repo, Some(&path));
    assert!(
        gaps(&gap)
            .join(" ")
            .contains("Observed credentials unavailable")
    );
    assert!(!refs(&gap).is_empty());
    world.save(
        &awa(
            "unavailable",
            "Observed credentials unavailable",
            text_value("different"),
            Vec::new(),
        ),
        "awa.json",
    );
    let (code, _) = command(
        &world,
        &[
            "init",
            "inspect",
            "--repo",
            repo.to_str().unwrap(),
            "--awa-evidence",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1);
}

/// Python `urllib.parse.unquote` for ASCII percent escapes of UTF-8 bytes.
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap();
            output.push(u8::from_str_radix(hex, 16).unwrap());
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).unwrap()
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_local_source_hrefs_resolve_exact_punctuation_unicode_and_percent_bytes
#[test]
fn local_source_hrefs_resolve_exact_punctuation_unicode_and_percent_bytes() {
    for name in [
        "docs/hash#query?100%ready.md",
        "docs/space ü [bracket]\\backslash.md",
    ] {
        let link = agent_memory::initialization::link(name, "memory-bank");
        let href = link
            .rsplit_once("](")
            .and_then(|(_, href)| href.strip_suffix(')'))
            .unwrap();
        assert!(
            !href.contains('?') && !href.contains('#') && !href.contains(')'),
            "{href}"
        );
        assert_eq!(unquote(href), format!("../{name}"));
        if name.contains('%') {
            assert!(href.contains("%25"));
        }
    }
    let world = World::new();
    let (repo, _, _) = setup(&world);
    let names = [
        "docs/hash#query?100%ready.md",
        "docs/space ü [bracket]\\backslash.md",
    ];
    for name in names {
        write(&repo.join(name), "# Source evidence\n");
    }
    let observed = inventory(&world, &repo, None);
    let previewed = preview(&world, &repo, &reviewed(&observed, vec![], vec![]));
    let body = get(&proposed(&previewed), "projectbrief.md")
        .as_str()
        .unwrap()
        .to_owned();
    let links: Vec<&str> = body
        .split("](")
        .skip(1)
        .filter_map(|tail| tail.split_once(')').map(|(href, _)| href))
        .collect();
    for name in names {
        let href = links
            .iter()
            .find(|href| unquote(href) == format!("../{name}"))
            .unwrap_or_else(|| panic!("{name}: {links:?}"));
        assert!(!href.contains('?') && !href.contains('#'));
        let resolved = repo.join("memory-bank").join(unquote(href));
        // Python `Path.resolve()` is non-strict: the bank does not exist yet.
        assert_eq!(
            agent_memory::fs::canonical_path(&resolved, false).unwrap(),
            agent_memory::fs::canonical_path(&repo.join(name), true).unwrap()
        );
    }
    assert_eq!(
        agent_memory::initialization::link("docs/a]b.md", "notes/team"),
        "[docs/a\\]b.md](../../docs/a%5Db.md)"
    );
    assert_eq!(
        agent_memory::initialization::link("https://example.invalid/a b?x=1#y", "memory-bank"),
        "[https://example.invalid/a b?x=1#y](https://example.invalid/a%20b?x=1#y)"
    );
}

#[test]
fn source_kinds_follow_the_baseline_classification() {
    use agent_memory::initialization::source_kind;
    assert_eq!(source_kind(&["README"]), Some("documentation"));
    assert_eq!(source_kind(&["docs", "ADR", "one.MD"]), Some("planning"));
    assert_eq!(source_kind(&["src", "app.py"]), Some("code"));
    assert_eq!(source_kind(&["Tests", "app.py"]), Some("test"));
    assert_eq!(source_kind(&["src", "test_app.rs"]), Some("test"));
    assert_eq!(source_kind(&[".md"]), None);
    assert_eq!(source_kind(&["image.png"]), None);
}

// obligation: tests/contracts/test_initialization.py::InitializationContract.test_empty_initialization_builds_useful_source_map_distinguishes_domains_and_preserves_code_index_head
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_first_bank_ignores_nonrouter_memory_file_and_preserves_it
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_first_bank_refuses_foreign_router_or_unknown_source_marker
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_first_bank_interrupted_recovery_with_ignored_nonrouter
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_existing_memory_is_read_first_preserved_byte_for_byte_and_only_missing_context_is_created
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_stale_sources_external_memory_and_substituted_content_never_overwrite
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_empty_bank_transaction_interruptions_finish_or_rollback_without_a_circular_identity_gate
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_copied_artifact_and_hostile_source_text_stay_data
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_initialization_and_ordinary_transactions_cannot_change_authority_without_scope_binding
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_source_change_while_waiting_under_the_bank_lock_stops_before_any_canonical_file
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_recovery_rechecks_original_source_branch_head_and_awa_but_rollback_remains_available
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_legacy_initialization_journal_without_source_policy_can_only_rollback
// obligation: tests/contracts/test_initialization.py::InitializationContract.test_recovery_source_change_after_lock_acquisition_stops_before_canonical_writes
// obligation: tests/contracts/test_initialization_routes.py::InitializationRoutes.test_all_roles_are_selective_and_actual_map_supports_bounded_qualified_choices
// obligation: tests/contracts/test_initialization_routes.py::InitializationRoutes.test_adding_missing_context_preserves_custom_rows_prose_and_existing_files
// obligation: tests/contracts/test_initialization_routes.py::InitializationRoutes.test_checkpoint_preparation_uses_matching_task_profile_and_ordinary_save_keeps_router
#[test]
#[ignore = "needs recovery_frontier registered (bank transaction apply)"]
fn preview_and_apply_create_a_reviewed_source_linked_bank() {
    let world = World::new();
    let (repo, _, _) = setup(&world);
    let observed = inventory(&world, &repo, None);
    let path = world.save(&review(&observed), "review.json");
    let preview = good(
        &world,
        &[
            "init",
            "preview",
            "--repo",
            repo.to_str().unwrap(),
            "--review",
            path.to_str().unwrap(),
        ],
    );
    let proposal = world.save(&preview, "proposal.json");
    let applied = good(
        &world,
        &["init", "apply", "--proposal", proposal.to_str().unwrap()],
    );
    assert_eq!(get(&applied, "verified"), &Value::Bool(true));
}

fn fanout_d_component_claim(value: i64) -> Value {
    let mut value = claim(
        "implementation",
        Value::Int(value.into()),
        "src/app.py",
        "retry-limit",
        "techContext.md",
        None,
    );
    set(
        &mut value,
        "conditions",
        Value::Object(vec![("component".into(), text_value("src"))]),
    );
    set(
        &mut value,
        "applicability_review",
        text_value(
            "The inspected retry constant belongs to this component, independently of the memory-bootstrap checkout; qualify the source bytes before use.",
        ),
    );
    value
}

fn fanout_d_init_apply(world: &World, prepared: &Value) -> (i32, Value) {
    let file = world.save(prepared, "fanout-apply.json");
    command(
        world,
        &["init", "apply", "--proposal", file.to_str().unwrap()],
    )
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_observation_applicability.py::ObservationApplicability.test_unchanged_evidence_survives_memory_only_head_and_capture_branch_changes_without_duplication
fn fanout_d_unchanged_evidence_survives_memory_head_and_branch_changes() {
    let world = World::new();
    let (repo, project, _) = setup(&world);
    world.git(&repo, &["add", "."]);
    world.git(&repo, &["commit", "-m", "Reviewed actual source fixture"]);
    world.git(&repo, &["checkout", "-b", "bootstrap-memory"]);
    let original_head =
        std::fs::read_to_string(repo.join(".git/refs/heads/bootstrap-memory")).unwrap();
    let claim = fanout_d_component_claim(3);
    let p = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![claim.clone()], vec![]),
    );
    assert_eq!(fanout_d_init_apply(&world, &p).0, 0);
    let bank = repo.join("memory-bank");
    let before: BTreeMap<_, _> = std::fs::read_dir(&bank)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_owned(), std::fs::read(p).unwrap())
        })
        .collect();
    let body = std::fs::read_to_string(bank.join("techContext.md")).unwrap();
    let provenance = body
        .lines()
        .find_map(|line| {
            line.strip_prefix("Observation provenance: `")
                .and_then(|s| s.strip_suffix("`."))
        })
        .unwrap();
    let provenance = json::decode(provenance, Default::default()).unwrap();
    let source = std::fs::read(repo.join("src/app.py")).unwrap();
    let observed = get(get(&provenance, "sources"), "src/app.py");
    assert_eq!(
        text(observed, "sha256"),
        agent_memory::developer_identity::digest(Some(&source)).unwrap()
    );
    assert_eq!(get(observed, "bytes"), &Value::Int(source.len().into()));
    assert_eq!(
        text(get(&provenance, "observed_checkout"), "branch"),
        "bootstrap-memory"
    );
    assert_eq!(
        text(get(&provenance, "observed_checkout"), "head"),
        original_head.trim()
    );
    assert_eq!(
        get(&provenance, "checkout_context_only"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(&provenance, "publication_verified"),
        &Value::Bool(false)
    );
    let applicability = body
        .lines()
        .find(|line| line.starts_with("Applicability:"))
        .unwrap();
    assert!(applicability.contains("\"component\": \"src\""));
    assert!(!applicability.contains("branch") && !applicability.contains("revision"));
    assert!(body.contains("Applicability review:"));
    world.git(&repo, &["add", "memory-bank"]);
    world.git(&repo, &["commit", "-m", "Memory-only capture"]);
    world.git(&repo, &["checkout", "-b", "another-session"]);
    let request = Value::Object(vec![
        ("project_id".into(), text_value(&project)),
        ("query".into(), text_value("")),
        ("ids".into(), Value::Array(vec![])),
        (
            "paths".into(),
            Value::Array(vec![text_value("techContext.md")]),
        ),
        (
            "context".into(),
            Value::Object(vec![("component".into(), text_value("src"))]),
        ),
        ("kinds".into(), Value::Array(vec![])),
        ("minimum_trust".into(), text_value("reviewed")),
        ("limit".into(), Value::Int(16.into())),
        ("include_context".into(), Value::Bool(true)),
    ]);
    let input = world.save(&request, "fanout-recall.json");
    let recalled = good(
        &world,
        &[
            "get",
            "--repo",
            repo.to_str().unwrap(),
            "--request",
            input.to_str().unwrap(),
        ],
    );
    let row = &get(&recalled, "results").as_array().unwrap()[0];
    assert_eq!(text(row, "selection_class"), "orientation-only");
    assert_eq!(get(row, "trust"), &Value::Null);
    assert_eq!(get(row, "source_assertions_verified"), &Value::Bool(false));
    assert_eq!(text(row, "body"), body);
    let mut duplicate = claim;
    set(&mut duplicate, "context", text_value("progress.md"));
    let again = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![duplicate], vec![]),
    );
    assert!(
        get(&again, "changed_memory_paths")
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !gaps(&again)
            .iter()
            .any(|g| g.contains("Existing implementation observation"))
    );
    let after: BTreeMap<_, _> = std::fs::read_dir(&bank)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_owned(), std::fs::read(p).unwrap())
        })
        .collect();
    assert_eq!(before, after);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_observation_applicability.py::ObservationApplicability.test_changed_source_refuses_stale_write_and_existing_changed_hash_requires_review_without_duplicate
fn fanout_d_changed_source_refuses_stale_write_without_duplicate_observation() {
    let world = World::new();
    let (repo, _, _) = setup(&world);
    let claim = fanout_d_component_claim(3);
    let p = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![claim], vec![]),
    );
    write(&repo.join("src/app.py"), "RETRY_LIMIT = 4\n");
    assert_ne!(fanout_d_init_apply(&world, &p).0, 0);
    assert!(!repo.join("memory-bank").exists());
    let claim = fanout_d_component_claim(4);
    let p = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![claim.clone()], vec![]),
    );
    assert_eq!(fanout_d_init_apply(&world, &p).0, 0);
    let bank = repo.join("memory-bank");
    let original = std::fs::read(bank.join("techContext.md")).unwrap();
    write(
        &repo.join("src/app.py"),
        "RETRY_LIMIT = 4\n# Additional implementation evidence to inspect.\n",
    );
    let mut duplicate = claim;
    set(&mut duplicate, "context", text_value("progress.md"));
    let p = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![duplicate], vec![]),
    );
    assert!(
        get(&p, "changed_memory_paths")
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        gaps(&p)
            .iter()
            .any(|g| g.contains("inspected source bytes differ"))
    );
    assert_eq!(
        std::fs::read(bank.join("techContext.md")).unwrap(),
        original
    );
    assert!(proposed(&p).get("progress.md").is_none());
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_observation_applicability.py::ObservationApplicability.test_legacy_inputs_and_real_branch_conditions_stay_conservative_and_unknown_old_hashes_remain_qualified
fn fanout_d_legacy_observations_and_explicit_branch_conditions_stay_conservative() {
    let world = World::new();
    let (repo, _, _) = setup(&world);
    let mut claim = claim(
        "implementation",
        Value::Int(3.into()),
        "src/app.py",
        "retry-limit",
        "techContext.md",
        None,
    );
    let p = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![claim.clone()], vec![]),
    );
    assert_eq!(fanout_d_init_apply(&world, &p).0, 0);
    let target = repo.join("memory-bank/techContext.md");
    let text = std::fs::read_to_string(&target).unwrap();
    assert!(text.contains("\"branch\": \"main\""));
    let legacy = text
        .lines()
        .filter(|line| !line.starts_with("Observation provenance:"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write(&target, &legacy);
    let mut duplicate = claim.clone();
    set(&mut duplicate, "context", text_value("progress.md"));
    let p = preview(
        &world,
        &repo,
        &reviewed(&inventory(&world, &repo, None), vec![duplicate], vec![]),
    );
    assert!(
        get(&p, "changed_memory_paths")
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        gaps(&p)
            .iter()
            .any(|g| g.contains("recorded byte binding is unknown"))
    );
    world.git(&repo, &["checkout", "-b", "other-branch"]);
    set(
        &mut claim,
        "applicability_review",
        text_value("This reason does not override an explicit main-branch condition."),
    );
    assert_ne!(
        try_preview(
            &world,
            &repo,
            &reviewed(&inventory(&world, &repo, None), vec![claim.clone()], vec![])
        )
        .0,
        0
    );
    if let Value::Object(rows) = &mut claim {
        rows.retain(|(key, _)| key != "applicability_review");
    }
    set(&mut claim, "conditions", Value::Object(vec![]));
    assert_ne!(
        try_preview(
            &world,
            &repo,
            &reviewed(&inventory(&world, &repo, None), vec![claim], vec![])
        )
        .0,
        0
    );
    assert_eq!(std::fs::read_to_string(target).unwrap(), legacy);
}
