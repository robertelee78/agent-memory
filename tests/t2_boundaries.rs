//! claude_boundary and codex_boundary: legacy source-bound acknowledgment of
//! retained pending boundaries, observation dispatch, pending inventories and
//! the `am claude-boundary` / `am codex-boundary` routes. Every case runs the
//! real `am` binary or the library inside a private 0700 world (HOME and the
//! XDG/agent-memory roots inside it); no global environment is mutated.
#[allow(dead_code)]
#[path = "support/admission.rs"]
mod admission;

use agent_memory::claude_boundary::{self, PageOffset};
use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::turn_check::CompletionError;
use agent_memory::{bank_writes, codex_boundary, host, process, scope_binding, turn_sources};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

const CLAUDE_REPAIR: &str = "Inspect am claude-boundary --list-pending. Current hook observations require top-level session,prompt,observation_id,source_sha256,outcome,assessment; assessment.source_id is observation_id. Legacy source_uuid applies only to legacy boundaries. Preserve original sources; read turn/GUIDE.md before retrying.";
const CLAUDE_UNAVAILABLE: &str = "Original source or pending record is unavailable or unsafe; preserve the retained observation.";
const CODEX_REPAIR: &str =
    "Preserve boundary records; inspect retained completed and pending evidence before retrying.";
const CODEX_PENDING: &str =
    "Boundary completion remains pending; preserve evidence and inspect the failure.";

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn s(text: &str) -> Value {
    Value::String(text.to_owned())
}
fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.map(|(k, v)| (k.to_owned(), v)).into())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn set(value: &mut Value, key: &str, field: Value) {
    *value.get_mut(key).unwrap() = field;
}
fn decode(text: &[u8]) -> Value {
    json::decode_bytes(text, DecodePolicy::default()).unwrap()
}
fn encode(value: &Value) -> String {
    json::encode(value, json::DEFAULT).unwrap()
}
fn uuid() -> String {
    host::uuid4().unwrap()
}
fn write_private(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}

/// `json.dumps(record)` failure envelope of a route, with its trailing newline.
fn failure(reason: &str, repair: &str) -> String {
    format!(
        "{{\"verified\": false, \"publication_verified\": false, \"canonical_writes_performed\": false, \"reason\": \"{reason}\", \"repair\": \"{repair}\"}}\n"
    )
}

/// Run the real `am` binary with `env`, `cwd` and bounded `stdin`.
fn run_am(
    env: &[(OsString, OsString)],
    cwd: &Path,
    args: &[&str],
    stdin: &[u8],
) -> (i32, String, String) {
    let argv: Vec<OsString> = std::iter::once(OsString::from(env!("CARGO_BIN_EXE_am")))
        .chain(args.iter().map(OsString::from))
        .collect();
    let spec = process::Spec {
        argv: &argv,
        env,
        cwd: Some(cwd),
        search_path: false,
        stdin: process::Input::Bytes(stdin),
        stdout: process::Target::Capture {
            limit: 4 * 1024 * 1024,
        },
        stderr: process::Target::Capture {
            limit: 4 * 1024 * 1024,
        },
        deadline: Deadline::after(Duration::from_secs(120)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    let output = process::run_for(&spec, Duration::from_secs(120)).unwrap();
    let code = match output.status {
        process::Status::Code(code) => code,
        process::Status::Signal(signal) => panic!("am ended by signal {signal}"),
    };
    (
        code,
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

/// A private 0700 world for routes that refuse before any repository access.
struct Sandbox {
    root: PathBuf,
    env: Vec<(OsString, OsString)>,
}
impl Sandbox {
    fn new() -> Self {
        let root = agent_memory::fs::canonical_path(&std::env::temp_dir(), true)
            .unwrap()
            .join(format!("am-boundaries-{}", uuid()));
        for directory in ["", "home", "repo"] {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(directory))
                .unwrap();
        }
        let env = [
            ("PATH", OsString::from("/usr/bin:/bin")),
            ("HOME", root.join("home").into_os_string()),
            (
                "AGENT_MEMORY_CONFIG_HOME",
                root.join("config").into_os_string(),
            ),
            (
                "AGENT_MEMORY_STATE_HOME",
                root.join("state/agent-memory").into_os_string(),
            ),
            ("XDG_CONFIG_HOME", root.join("config").into_os_string()),
            ("XDG_STATE_HOME", root.join("state").into_os_string()),
            ("XDG_DATA_HOME", root.join("data").into_os_string()),
            ("GIT_CONFIG_NOSYSTEM", OsString::from("1")),
            ("LC_ALL", OsString::from("C")),
        ]
        .into_iter()
        .map(|(key, value)| (OsString::from(key), value))
        .collect();
        Self { root, env }
    }
    fn repo(&self) -> String {
        self.root.join("repo").to_str().unwrap().to_owned()
    }
    fn am(&self, args: &[&str], stdin: &[u8]) -> (i32, String, String) {
        run_am(&self.env, &self.root, args, stdin)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// `am` inside an `admission::in_world` body: the process already carries
/// the private world environment.
fn world_am(world: &admission::World, args: &[&str], stdin: &[u8]) -> (i32, String, String) {
    let env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    run_am(&env, &world.root, args, stdin)
}

/// The legacy boundary directory `<state>/writes/<bank>/<name>` of `repo`,
/// created owner-only as the stop hooks create it.
fn boundary_directory(repo: &Path, name: &str) -> PathBuf {
    let (root, bank) = bank_writes::bank_path(repo).unwrap();
    let (operations, _) = bank_writes::state_paths(&root, &bank, &uuid()).unwrap();
    let directory = operations.parent().unwrap().join(name);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}
fn project_id(repo: &Path) -> Value {
    get(
        &scope_binding::require(repo, None, Default::default()).unwrap(),
        "project_id",
    )
    .clone()
}
fn key(session: &str, prompt: &str) -> String {
    sha256(format!("{session}:{prompt}").as_bytes())
}

/// The record `claude_stop.py` retains for one legacy Claude boundary
/// (`json.dumps` default separators, insertion order).
fn claude_pending(
    repo: &Path,
    session: &str,
    prompt: &str,
    message: &str,
    source: &str,
) -> (PathBuf, Vec<u8>) {
    let directory = boundary_directory(repo, "claude-boundaries");
    let record = obj([
        ("schema", s("agent-memory/claude-boundary-pending-v1")),
        ("project_id", project_id(repo)),
        ("source_uuid", s(message)),
        ("source_sha256", s(&sha256(source.as_bytes()))),
        ("assistant_sha256", s(&sha256(b"Synthetic answer"))),
    ]);
    let path = directory.join(format!("{}.pending", key(session, prompt)));
    let bytes = encode(&record).into_bytes();
    write_private(&path, &bytes);
    (path, bytes)
}
fn no_learning(session: &str, prompt: &str, message: &str, source: &str, review: &str) -> Value {
    obj([
        ("session", s(session)),
        ("prompt", s(prompt)),
        ("source_uuid", s(message)),
        ("source_sha256", s(&sha256(source.as_bytes()))),
        ("outcome", s("no-learning")),
        (
            "assessment",
            obj([
                ("source_id", s(message)),
                ("source_review", s(review)),
                ("candidates", Value::Array(vec![])),
            ]),
        ),
    ])
}
fn save(world: &admission::World, value: &Value, name: &str) -> String {
    let path = world.root.join(name);
    write_private(&path, encode(value).as_bytes());
    path.to_str().unwrap().to_owned()
}
fn completed(directory: &Path, session: &str, prompt: &str) -> PathBuf {
    directory.join(format!("{}.done", key(session, prompt)))
}

#[test]
fn page_offsets_follow_python_int() {
    for (text, zero, value) in [
        ("0", true, 0),
        (" 7 ", false, 7),
        ("+3", false, 3),
        ("1_000", false, 1000),
        ("-0", true, 0),
        ("00", true, 0),
        ("-1", false, usize::MAX),
        ("99999999999999999999999999", false, usize::MAX),
    ] {
        let offset = PageOffset::parse(text).unwrap();
        assert_eq!((offset.is_zero(), offset.value()), (zero, value), "{text}");
    }
    for text in ["", "x", "1.5", "1__0", "_1", "1_", "+", "0x10", "١"] {
        assert_eq!(PageOffset::parse(text), None, "{text}");
    }
}

#[test]
fn claude_route_refuses_mode_mixtures_and_unbounded_stdin() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repo();
    let input = sandbox.root.join("input.json");
    write_private(&input, b"{}");
    let input = input.to_str().unwrap();
    let pending = failure("Select pending inspection or acknowledgment", CLAUDE_REPAIR);
    for args in [
        vec!["--list-pending", "--quiet"],
        vec!["--list-pending", "--refresh"],
        vec!["--list-pending", "--input", input],
    ] {
        let mut argv = vec!["claude-boundary", "--repo", &repo];
        argv.extend(args);
        assert_eq!(sandbox.am(&argv, b""), (1, pending.clone(), String::new()));
    }
    let exact = failure(
        "Acknowledgment requires its exact input, not pending-inventory page offsets",
        CLAUDE_REPAIR,
    );
    for args in [
        vec![],
        vec!["--input", ""],
        vec!["--input", input, "--offset", "1"],
        vec!["--input", input, "--legacy-offset", "-2"],
        vec!["--input", input, "--enrollment-offset", "3"],
        vec!["--input", input, "--session", "s"],
        vec!["--input", input, "--prompt", "p"],
    ] {
        let mut argv = vec!["claude-boundary", "--repo", &repo];
        argv.extend(args);
        assert_eq!(sandbox.am(&argv, b""), (1, exact.clone(), String::new()));
    }
    let stdin = ["claude-boundary", "--repo", &repo, "--input", "-"];
    let oversized = vec![b' '; claude_boundary::MAX_STDIN + 1];
    assert_eq!(
        sandbox.am(&stdin, &oversized),
        (
            1,
            failure("Stdin assessment exceeds64KiB", CLAUDE_REPAIR),
            String::new()
        )
    );
    let quiet = [
        "claude-boundary",
        "--repo",
        &repo,
        "--input",
        "-",
        "--refresh",
        "--quiet",
    ];
    assert_eq!(
        sandbox.am(&quiet, &oversized),
        (
            1,
            String::new(),
            format!(
                "am: source assessment pending: Stdin assessment exceeds64KiB {CLAUDE_REPAIR}\n"
            )
        )
    );
    let published = failure(
        "Stdin supports no-learning only; published requests require private file input",
        CLAUDE_REPAIR,
    );
    for body in [
        &b"{\"outcome\": \"published\", \"verified\": true}"[..],
        b"[]",
    ] {
        assert_eq!(
            sandbox.am(&stdin, body),
            (1, published.clone(), String::new())
        );
    }
    // Undecodable or unavailable input never exposes its parser or OS text.
    let unavailable = failure(CLAUDE_UNAVAILABLE, CLAUDE_REPAIR);
    for body in [&b"not json"[..], b"{\"a\": 1, \"a\": 2}", b"\xff"] {
        assert_eq!(
            sandbox.am(&stdin, body),
            (1, unavailable.clone(), String::new())
        );
    }
    let missing = sandbox.root.join("missing.json");
    assert_eq!(
        sandbox.am(
            &[
                "claude-boundary",
                "--repo",
                &repo,
                "--input",
                missing.to_str().unwrap()
            ],
            b""
        ),
        (1, unavailable, String::new())
    );
}

#[test]
fn codex_route_keeps_fixed_pending_reason_and_quiet_channel() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repo();
    let pending = failure(CODEX_PENDING, CODEX_REPAIR);
    let quiet = format!("am: source assessment pending: {CODEX_PENDING} {CODEX_REPAIR}\n");
    let input = sandbox.root.join("input.json");
    write_private(&input, b"{}");
    let input = input.to_str().unwrap();
    for args in [
        vec!["--list-pending", "--stdin"],
        vec!["--list-pending", "--refresh"],
        vec!["--list-pending", "--input", input],
        vec![],
        vec!["--input", ""],
        vec!["--input", input, "--stdin"],
    ] {
        let mut argv = vec!["codex-boundary", "--repo", &repo];
        argv.extend(args);
        assert_eq!(sandbox.am(&argv, b""), (1, pending.clone(), String::new()));
    }
    assert_eq!(
        sandbox.am(
            &[
                "codex-boundary",
                "--repo",
                &repo,
                "--list-pending",
                "--quiet"
            ],
            b""
        ),
        (1, String::new(), quiet.clone())
    );
    let stdin = ["codex-boundary", "--repo", &repo, "--stdin"];
    let oversized = vec![b' '; claude_boundary::MAX_STDIN + 1];
    assert_eq!(
        sandbox.am(&stdin, &oversized),
        (1, pending.clone(), String::new())
    );
    for body in [&b"{\"outcome\": \"published\"}"[..], b"not json"] {
        assert_eq!(
            sandbox.am(&stdin, body),
            (1, pending.clone(), String::new())
        );
        let quiet_args = ["codex-boundary", "--repo", &repo, "--stdin", "--quiet"];
        assert_eq!(
            sandbox.am(&quiet_args, body),
            (1, String::new(), quiet.clone())
        );
    }
}

#[test]
fn boundary_routes_parse_like_their_baseline_helpers() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repo();
    for (route, description) in [
        (
            "claude-boundary",
            "Acknowledge one source-bound Claude boundary without inventing publication.",
        ),
        (
            "codex-boundary",
            "Acknowledge one source-bound Codex boundary without inventing publication.",
        ),
    ] {
        let (code, stdout, stderr) = sandbox.am(&[route, "--help"], b"");
        assert_eq!((code, stderr.as_str()), (0, ""));
        assert!(
            stdout.starts_with(&format!("usage: am {route} [-h] --repo REPO")),
            "{stdout}"
        );
        assert!(stdout.contains(description), "{stdout}");
        assert!(
            stdout.contains("--enrollment-offset ENROLLMENT_OFFSET"),
            "{stdout}"
        );
        for (flag, value) in [
            ("--offset", "x"),
            ("--legacy-offset", "1.5"),
            ("--enrollment-offset", ""),
        ] {
            let (code, stdout, stderr) = sandbox.am(
                &[route, "--repo", &repo, "--list-pending", flag, value],
                b"",
            );
            assert_eq!((code, stdout.as_str()), (2, ""));
            assert!(
                stderr.ends_with(&format!(
                    "am {route}: error: argument {flag}: invalid int value: '{value}'\n"
                )),
                "{stderr}"
            );
        }
        let (code, _, stderr) = sandbox.am(&[route, "--list-pending"], b"");
        assert_eq!(code, 2);
        assert!(
            stderr.ends_with(&format!(
                "am {route}: error: the following arguments are required: --repo\n"
            )),
            "{stderr}"
        );
    }
    let (code, _, stderr) = sandbox.am(&["claude-boundary", "--repo", &repo, "--stdin"], b"");
    assert_eq!(code, 2);
    assert!(
        stderr.ends_with("am claude-boundary: error: unrecognized arguments: --stdin\n"),
        "{stderr}"
    );
}

#[test]
fn claude_route_hides_os_failures_of_callees_behind_the_fixed_reason() {
    admission::in_world(
        "claude_route_hides_os_failures_of_callees_behind_the_fixed_reason",
        |world| {
            let proposal = agent_memory::developer_identity::preview(
                "create",
                None,
                &Value::Array(vec![s("Fixture observer")]),
                &Value::Array(vec![]),
            )
            .unwrap();
            agent_memory::developer_identity::apply(&proposal).unwrap();
            let registry = world.root.join("config/developers.json");
            assert!(registry.is_file());
            let repo = world.root.join("repository");
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&repo)
                .unwrap();
            let request = no_learning(&uuid(), &uuid(), &uuid(), "source", "Reviewed source.");
            let args = [
                "claude-boundary",
                "--repo",
                repo.to_str().unwrap(),
                "--input",
                "-",
            ];
            let body = encode(&request);
            // The readable registry reaches a contract refusal with its own reason.
            let (code, stdout, _) = world_am(world, &args, body.as_bytes());
            assert_eq!(code, 1);
            assert_ne!(
                text(&decode(stdout.as_bytes()), "reason"),
                CLAUDE_UNAVAILABLE
            );
            // An unreadable registry is an OS failure inside scope resolution.
            std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o000)).unwrap();
            let outcome = world_am(world, &args, body.as_bytes());
            std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(
                outcome,
                (1, failure(CLAUDE_UNAVAILABLE, CLAUDE_REPAIR), String::new())
            );
        },
    );
}

#[test]
fn codex_completion_requires_an_object_before_scope() {
    for value in [Value::Array(vec![]), s("text"), Value::Null] {
        match codex_boundary::complete(Path::new("/nonexistent"), &value, None) {
            Err(CompletionError::Contract(error)) => {
                assert_eq!(error.message(), "Explicit source assessment required")
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

// obligation: tests/contracts/test_claude_boundary.py::ClaudeBoundaryContract.test_bound_source_and_duplicate_are_fail_closed
#[test]
#[ignore = "needs recovery_frontier registered"]
fn bound_source_and_duplicate_are_fail_closed() {
    admission::in_world("bound_source_and_duplicate_are_fail_closed", |world| {
        let (repo, bank, _) = world.fixture();
        let before = world.state(&repo, &bank);
        let (session, prompt, message) = (uuid(), uuid(), uuid());
        // The legacy record claude_stop.py retains at its first Stop.
        let (pending, pending_bytes) =
            claude_pending(&repo, &session, &prompt, &message, "Synthetic decision");
        let directory = pending.parent().unwrap().to_path_buf();
        let request = no_learning(
            &session,
            &prompt,
            &message,
            "Synthetic decision",
            "Reviewed source; no reusable lesson.",
        );
        let repo_text = repo.to_str().unwrap();
        let mut bad = request.clone();
        set(&mut bad, "source_sha256", s(&"0".repeat(64)));
        let input = save(world, &bad, "assessment.json");
        let args = ["claude-boundary", "--repo", repo_text, "--input", &input];
        let (code, stdout, _) = world_am(world, &args, b"");
        assert_ne!(code, 0);
        assert_eq!(
            text(&decode(stdout.as_bytes()), "reason"),
            "Pending boundary does not match the assessed source"
        );
        assert_eq!(std::fs::read(&pending).unwrap(), pending_bytes);
        save(world, &request, "assessment.json");
        let (code, stdout, stderr) = world_am(world, &args, b"");
        assert_eq!(code, 0, "{stdout}{stderr}");
        let result = decode(stdout.as_bytes());
        assert_eq!(
            get(&result, "assessment_is_agent_assertion"),
            &Value::Bool(true)
        );
        assert_eq!(get(&result, "publication_verified"), &Value::Bool(false));
        assert_eq!(
            get(&result, "source_publication_link_verified"),
            &Value::Bool(false)
        );
        assert!(!pending.exists());
        let done = completed(&directory, &session, &prompt);
        assert_eq!(mode(&done), 0o600);
        let record = decode(&std::fs::read(&done).unwrap());
        assert_eq!(
            text(&record, "schema"),
            "agent-memory/claude-boundary-completed-v1"
        );
        assert_eq!(text(&record, "pending_sha256"), sha256(&pending_bytes));
        assert_eq!(
            text(&record, "input_sha256"),
            sha256(
                json::encode(&request, json::SORTED_COMPACT_UTF8)
                    .unwrap()
                    .as_bytes()
            )
        );
        // Interrupted after completion but before the pending record left.
        write_private(&pending, &pending_bytes);
        let (code, stdout, stderr) = world_am(world, &args, b"");
        assert_eq!(code, 0, "{stdout}{stderr}");
        assert!(!pending.exists());
        let (code, stdout, stderr) = world_am(world, &args, b"");
        assert_eq!(code, 0, "{stdout}{stderr}");
        assert_eq!(
            get(&decode(stdout.as_bytes()), "already_completed"),
            &Value::Bool(true)
        );
        let mut changed = request.clone();
        set(&mut changed, "assessment", s("A different decision."));
        save(world, &changed, "assessment.json");
        let (code, _, _) = world_am(world, &args, b"");
        assert_ne!(code, 0);
        assert_eq!(world.state(&repo, &bank), before);
    });
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap_or_default()
}

// obligation: tests/contracts/test_claude_source_turns.py::ClaudeSourceTurns.test_legacy_unicode_done_is_bounded_and_corruption_is_not_quiet
#[test]
#[ignore = "needs recovery_frontier registered"]
fn legacy_unicode_done_is_bounded_and_corruption_is_not_quiet() {
    admission::in_world(
        "legacy_unicode_done_is_bounded_and_corruption_is_not_quiet",
        |world| {
            let (repo, _, _) = world.fixture();
            let (session, prompt, message) = (uuid(), uuid(), uuid());
            let text_source = "Fixture arithmetic question";
            let (pending, _) = claude_pending(&repo, &session, &prompt, &message, text_source);
            let review = format!("Reviewed transient fixture question. {}", "🌻".repeat(380));
            let request = no_learning(&session, &prompt, &message, text_source, &review);
            let input = save(world, &request, "legacy-assessment.json");
            let args = [
                "claude-boundary",
                "--repo",
                repo.to_str().unwrap(),
                "--input",
                &input,
            ];
            let (code, stdout, stderr) = world_am(world, &args, b"");
            assert_eq!(code, 0, "{stdout}{stderr}");
            let done = completed(pending.parent().unwrap(), &session, &prompt);
            let raw = std::fs::read(&done).unwrap();
            assert!(raw.len() <= claude_boundary::MAX_RECORD);
            assert!(String::from_utf8(raw.clone()).unwrap().contains("🌻"));
            let mut record = decode(&raw);
            set(&mut record, "outcome", s("invented"));
            write_private(&done, json::encode(&record, json::UTF8).unwrap().as_bytes());
            let (code, stdout, _) = world_am(world, &args, b"");
            assert_eq!(code, 1);
            assert_eq!(
                text(&decode(stdout.as_bytes()), "reason"),
                "Completed boundary differs from requested retry"
            );
        },
    );
}

// obligation: tests/contracts/test_claude_source_turns.py::ClaudeSourceTurns.test_interrupted_old_unicode_completion_retries_without_rewriting_evidence
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interrupted_old_unicode_completion_retries_without_rewriting_evidence() {
    admission::in_world(
        "interrupted_old_unicode_completion_retries_without_rewriting_evidence",
        |world| {
            let (repo, _, _) = world.fixture();
            let (session, prompt, message) = (uuid(), uuid(), uuid());
            let source = "Transient fixture question";
            let (pending, pending_raw) = claude_pending(&repo, &session, &prompt, &message, source);
            let review = format!("Reviewed transient fixture question. {}", "🌻".repeat(50));
            let request = no_learning(&session, &prompt, &message, source, &review);
            let input = save(world, &request, "legacy-retry.json");
            let args = [
                "claude-boundary",
                "--repo",
                repo.to_str().unwrap(),
                "--input",
                &input,
            ];
            let (code, stdout, stderr) = world_am(world, &args, b"");
            assert_eq!(code, 0, "{stdout}{stderr}");
            let done = completed(pending.parent().unwrap(), &session, &prompt);
            // An old v1 writer used ASCII escapes. Simulate interruption after
            // durable completion publication but before pending was removed.
            let old_record = decode(&std::fs::read(&done).unwrap());
            let old_raw = json::encode(&old_record, json::SORTED_ASCII).unwrap();
            assert!(old_raw.len() <= claude_boundary::MAX_RECORD);
            assert!(old_raw.contains("\\ud83c\\udf3b"));
            write_private(&done, old_raw.as_bytes());
            write_private(&pending, &pending_raw);
            let (code, stdout, stderr) = world_am(world, &args, b"");
            assert_eq!(code, 0, "{stdout}{stderr}");
            assert_eq!(
                get(&decode(stdout.as_bytes()), "verified"),
                &Value::Bool(true)
            );
            assert!(!pending.exists());
            assert_eq!(std::fs::read(&done).unwrap(), old_raw.as_bytes());
        },
    );
}

// obligation: tests/contracts/test_codex_stop.py::CodexStopContract.test_legacy_completion_remains_quiet_without_rewriting_source
#[test]
#[ignore = "needs recovery_frontier registered"]
fn legacy_codex_completion_is_exact_and_listed_until_acknowledged() {
    admission::in_world(
        "legacy_codex_completion_is_exact_and_listed_until_acknowledged",
        |world| {
            let (repo, _, _) = world.fixture();
            let (session, turn) = (uuid(), uuid());
            let digest = sha256(b"actual legacy source");
            let directory = boundary_directory(&repo, "codex-boundaries");
            let pending = obj([
                ("schema", s("agent-memory/codex-boundary-pending-v1")),
                ("project_id", project_id(&repo)),
                ("session", s(&session)),
                ("turn", s(&turn)),
                ("source_uuid", s(&turn)),
                ("source_sha256", s(&digest)),
            ]);
            let path = directory.join(format!("{}.pending", key(&session, &turn)));
            write_private(&path, encode(&pending).as_bytes());
            let runtime = Path::new(env!("CARGO_MANIFEST_DIR"));
            let listed = codex_boundary::legacy_list_pending(&repo, runtime).unwrap();
            assert_eq!(
                get(&listed, "pending"),
                &Value::Array(vec![pending.clone()])
            );
            let review = format!(
                "Reviewed actual retained legacy source; no durable lesson. {}",
                "😀".repeat(380)
            );
            let mut request = obj([
                ("session", s(&session)),
                ("prompt", s(&turn)),
                ("source_uuid", s(&turn)),
                ("source_sha256", s(&digest)),
                ("outcome", s("no-learning")),
                (
                    "assessment",
                    obj([
                        ("source_id", s(&turn)),
                        ("source_review", s(&review)),
                        ("candidates", Value::Array(vec![])),
                    ]),
                ),
            ]);
            // The pending record binds the exact native session and turn.
            set(&mut request, "session", s(&uuid()));
            let input = save(world, &request, "legacy-assessment.json");
            let args = [
                "codex-boundary",
                "--repo",
                repo.to_str().unwrap(),
                "--input",
                &input,
            ];
            let (code, _, _) = world_am(world, &args, b"");
            assert_eq!(code, 1);
            assert!(path.exists());
            set(&mut request, "session", s(&session));
            save(world, &request, "legacy-assessment.json");
            let (code, stdout, stderr) = world_am(world, &args, b"");
            assert_eq!(code, 0, "{stdout}{stderr}");
            assert!(!path.exists());
            let record = decode(&std::fs::read(completed(&directory, &session, &turn)).unwrap());
            assert_eq!(
                text(&record, "schema"),
                "agent-memory/codex-boundary-completed-v1"
            );
            let listed = codex_boundary::legacy_list_pending(&repo, runtime).unwrap();
            assert_eq!(get(&listed, "pending"), &Value::Array(vec![]));
            let (code, stdout, stderr) = world_am(world, &args, b"");
            assert_eq!(code, 0, "{stdout}{stderr}");
            assert_eq!(
                get(&decode(stdout.as_bytes()), "already_completed"),
                &Value::Bool(true)
            );
        },
    );
}

// obligation: tests/contracts/test_claude_source_turns.py::ClaudeSourceTurns.test_interrupted_prompt_is_discoverable_and_stdin_cannot_claim_publication
#[test]
#[ignore = "needs security, recovery_frontier, installation and their real dependency providers registered"]
fn interrupted_prompt_is_discoverable_and_stdin_cannot_claim_publication() {
    admission::in_world(
        "interrupted_prompt_is_discoverable_and_stdin_cannot_claim_publication",
        |world| {
            let (repo, _, _) = world.fixture();
            let runtime = Path::new(env!("CARGO_MANIFEST_DIR"));
            let (session, prompt) = (uuid(), uuid());
            // The observation claude_prompt.py registers before any transcript exists.
            turn_sources::register(
                &repo,
                "claude",
                &session,
                &prompt,
                &sha256(b"Interrupted fixture input"),
                Some(&prompt),
                None,
                None,
                runtime,
            )
            .unwrap();
            let later =
                agent_memory::claude_turns::pending(&repo, None, None, (0, 0, 0), runtime).unwrap();
            assert!(
                get(&later, "turns")
                    .as_array()
                    .is_some_and(|turns| !turns.is_empty())
            );
            let args = [
                "claude-boundary",
                "--repo",
                repo.to_str().unwrap(),
                "--input",
                "-",
            ];
            let (code, _, _) = world_am(
                world,
                &args,
                b"{\"outcome\": \"published\", \"verified\": true}",
            );
            assert_ne!(code, 0);
        },
    );
}
