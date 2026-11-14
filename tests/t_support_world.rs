mod support;

use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::{self, Input, Refusal, Spec, Status, Target};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::record_fixtures::{self, RecordOptions};
use support::world::{self, DIRECTORIES, ROOT_VARIABLES};
use support::{Environment, RunOptions, World, WorldError, require_world};

fn read(path: &Path) -> Vec<u8> {
    agent_memory::fs::read_regular(
        path.parent().unwrap(),
        Path::new(path.file_name().unwrap()),
        64 * 1024,
    )
    .unwrap()
    .0
}

fn worker_args() -> Vec<OsString> {
    vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "world_worker".into(),
        "--nocapture".into(),
    ]
}

fn worker_options(mode: &str) -> Vec<(OsString, OsString)> {
    vec![("AGENT_MEMORY_FAULT_WORLD_WORKER".into(), mode.into())]
}

fn wait_file(path: &Path) {
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    while !path.is_file() {
        assert!(
            !deadline.expired(),
            "worker did not create {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn world_worker() {
    let Some(mode) = std::env::var_os("AGENT_MEMORY_FAULT_WORLD_WORKER") else {
        return;
    };
    match mode.to_str().unwrap() {
        "host-environment" => {
            assert_eq!(std::env::var("PARENT_SECRET").unwrap(), "must-not-leak");
            let world = World::new().unwrap();
            assert_ne!(
                world.root.parent(),
                Some(Path::new(&std::env::var_os("TMPDIR").unwrap()))
            );
            let overrides = worker_options("dump-environment");
            world
                .run_with(
                    worker_args(),
                    RunOptions {
                        env: &overrides,
                        ..RunOptions::default()
                    },
                )
                .unwrap();
            let observed = json::decode_bytes(
                &read(&world.root.join("child-env.json")),
                json::DecodePolicy::default(),
            )
            .unwrap();
            let fields = observed.as_object().unwrap();
            let mut expected: Environment = world.env.clone();
            expected.extend(overrides);
            assert_eq!(fields.len(), expected.len());
            for (key, value) in &expected {
                assert_eq!(
                    observed.get(key.to_str().unwrap()).and_then(Value::as_str),
                    value.to_str()
                );
            }
            for key in [
                "PARENT_SECRET",
                "AWS_SECRET_ACCESS_KEY",
                "GITHUB_TOKEN",
                "PYTHONPATH",
                "GIT_CONFIG_COUNT",
                "SSH_AUTH_SOCK",
            ] {
                assert!(observed.get(key).is_none(), "leaked {key}");
            }
            assert_eq!(
                require_world(&world.env, &world.root.join("repo")).unwrap(),
                world.root
            );
            let outer = support::require_current_world().unwrap();
            fs::write(
                outer.join("environment-isolation.completed"),
                b"exact isolated child environment verified\n",
            )
            .unwrap();
        }
        "dump-environment" => {
            let values = std::env::vars()
                .map(|(key, value)| (key, Value::String(value)))
                .collect();
            let root = PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap());
            assert_eq!(support::require_current_world().unwrap(), root);
            fs::write(
                root.join("child-env.json"),
                json::encode(&Value::Object(values), json::DEFAULT).unwrap(),
            )
            .unwrap();
        }
        "sleep" => {
            let root = PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap());
            fs::write(
                root.join("worker.pid"),
                agent_memory::host::pid().to_string(),
            )
            .unwrap();
            println!("owned child started");
            std::thread::sleep(Duration::from_secs(30));
        }
        "overflow" => {
            use std::io::Write;
            std::io::stdout().write_all(&[b'x'; 8192]).unwrap();
        }
        "failure" => {
            println!("retained stdout");
            eprintln!("retained stderr");
            std::process::exit(7);
        }
        other => panic!("unknown worker mode {other}"),
    }
}

#[test]
fn exact_child_environment_ignores_injected_host_secrets_and_tmpdir() {
    let outer = World::new().unwrap();
    let mut env: Vec<_> = outer
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    for (key, value) in [
        ("AGENT_MEMORY_FAULT_WORLD_WORKER", "host-environment"),
        ("PARENT_SECRET", "must-not-leak"),
        ("AWS_SECRET_ACCESS_KEY", "private"),
        ("GITHUB_TOKEN", "private"),
        ("PYTHONPATH", "/private/python"),
        ("GIT_CONFIG_COUNT", "99"),
        ("SSH_AUTH_SOCK", "/private/agent"),
    ] {
        env.push((key.into(), value.into()));
    }
    let argv = worker_args();
    let cwd = outer.root.join("repo");
    let spec = Spec {
        argv: &argv,
        search_path: false,
        env: &env,
        cwd: Some(&cwd),
        stdin: Input::Null,
        stdout: Target::Capture {
            limit: world::CAPTURE_LIMIT,
        },
        stderr: Target::Capture {
            limit: world::CAPTURE_LIMIT,
        },
        deadline: Deadline::after(Duration::from_secs(10)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    let output = process::run_for(&spec, Duration::from_secs(10)).unwrap();
    assert_eq!(
        output.status,
        Status::Code(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        read(&outer.root.join("environment-isolation.completed")),
        b"exact isolated child environment verified\n"
    );
}

#[test]
fn private_canonical_roots_directories_tokens_and_path_are_isolated() {
    let world = World::new().unwrap();
    let second = World::new().unwrap();
    assert_ne!(world.root, second.root);
    assert_ne!(world.token, second.token);
    assert_eq!(world.token.len(), 48);
    assert!(world.token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(
        world.root.parent(),
        Some(fs::canonicalize("/tmp").unwrap().as_path())
    );
    assert_eq!(
        fs::metadata(&world.root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for &name in DIRECTORIES {
        let path = world.paths.get(name).unwrap();
        assert_eq!(path, &world.root.join(name));
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    for &key in ROOT_VARIABLES {
        let path = PathBuf::from(world.env.get(OsStr::new(key)).unwrap());
        assert!(path.starts_with(&world.root) && path != world.root, "{key}");
    }
    for key in ["GIT_CONFIG_SYSTEM", "GIT_CONFIG_GLOBAL"] {
        let path = PathBuf::from(world.env.get(OsStr::new(key)).unwrap());
        assert!(path.starts_with(&world.root.join("credentials")));
        assert!(read(&path).is_empty());
    }
    let path: Vec<_> = std::env::split_paths(world.env.get(OsStr::new("PATH")).unwrap()).collect();
    assert_eq!(
        path.first().unwrap(),
        Path::new(env!("CARGO_BIN_EXE_am")).parent().unwrap()
    );
    assert!(path.contains(&PathBuf::from("/usr/bin")) && path.contains(&PathBuf::from("/bin")));
    assert_eq!(
        path.iter().collect::<std::collections::BTreeSet<_>>().len(),
        path.len()
    );
}

#[test]
fn inside_refuses_traversal_absolute_and_intermediate_symlink_escapes() {
    let world = World::new().unwrap();
    assert_eq!(
        world.inside("repo/new/../file").unwrap(),
        world.root.join("repo/file")
    );
    assert!(world.inside("../outside").is_err());
    assert!(world.inside("/etc/passwd").is_err());
    symlink("/", world.root.join("repo/escape")).unwrap();
    assert!(world.inside("repo/escape/tmp/nonexistent/child").is_err());
    symlink(&world.paths["bank"], world.root.join("repo/safe")).unwrap();
    assert_eq!(
        world.inside("repo/safe/new").unwrap(),
        world.root.join("bank/new")
    );
    symlink("loop", world.root.join("repo/loop")).unwrap();
    assert!(world.inside("repo/loop/new").is_err());
}

#[test]
fn require_world_refuses_marker_root_environment_and_cwd_tampering() {
    let world = World::new().unwrap();
    let cwd = world.root.join("repo");
    assert_eq!(require_world(&world.env, &cwd).unwrap(), world.root);
    let mut bad = world.env.clone();
    bad.insert("AGENT_MEMORY_TEST_TOKEN".into(), "wrong".into());
    assert!(require_world(&bad, &cwd).is_err());
    for &key in ROOT_VARIABLES {
        let mut bad = world.env.clone();
        bad.insert(key.into(), "/tmp".into());
        assert!(require_world(&bad, &cwd).is_err(), "{key}");
    }
    assert!(require_world(&world.env, Path::new("/tmp")).is_err());
    let marker = world.root.join(".test-world.json");
    fs::remove_file(&marker).unwrap();
    symlink(world.root.join("missing"), marker).unwrap();
    assert!(require_world(&world.env, &cwd).is_err());
}

#[test]
fn run_applies_cwd_fault_overrides_check_and_retains_failure_evidence() {
    let world = World::new().unwrap();
    let cwd = world.root.join("bank");
    let output = world
        .run_with(
            ["/bin/pwd"],
            RunOptions {
                cwd: Some(&cwd),
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(output.stdout, format!("{}\n", cwd.display()).as_bytes());
    for key in [
        "HOME",
        "PATH",
        "GIT_CONFIG_GLOBAL",
        "AGENT_MEMORY_TEST_ROOT",
    ] {
        let overrides = [(key.into(), "/outside".into())];
        assert!(
            world
                .run_with(
                    ["/bin/true"],
                    RunOptions {
                        env: &overrides,
                        ..RunOptions::default()
                    }
                )
                .is_err()
        );
    }
    let overrides = worker_options("failure");
    let output = world
        .run_with(
            worker_args(),
            RunOptions {
                env: &overrides,
                check: false,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(7));
    assert!(
        output
            .stdout
            .windows(b"retained stdout".len())
            .any(|value| value == b"retained stdout")
    );
    assert!(
        output
            .stderr
            .windows(b"retained stderr".len())
            .any(|value| value == b"retained stderr")
    );
    assert!(
        matches!(world.run_with(worker_args(), RunOptions { env: &overrides, ..RunOptions::default() }), Err(WorldError::Check(output)) if output.status == Status::Code(7))
    );
    assert!(world.run(Vec::<OsString>::new()).is_err());
    assert!(
        world
            .run_with(
                ["/bin/true"],
                RunOptions {
                    timeout: Duration::MAX,
                    ..RunOptions::default()
                }
            )
            .is_err()
    );
}

#[test]
fn finite_deadline_kills_and_reaps_real_owned_child() {
    let world = World::new().unwrap();
    let overrides = worker_options("sleep");
    let error = world
        .run_with(
            worker_args(),
            RunOptions {
                env: &overrides,
                timeout: Duration::from_millis(500),
                ..RunOptions::default()
            },
        )
        .unwrap_err();
    let WorldError::Process(error) = error else {
        panic!("expected process deadline");
    };
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert!(evidence.killed && evidence.reaped);
    assert!(!process::pid_is_alive(evidence.pid as i32).unwrap());
    assert!(evidence.elapsed_ns < 5_000_000_000);
    let worker: u32 = String::from_utf8(read(&world.root.join("worker.pid")))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(worker, evidence.pid);
}

#[test]
fn run_collection_failures_are_not_swallowed_by_check_false_or_group_mode() {
    let world = World::new().unwrap();
    for group in [false, true] {
        let overrides = worker_options("overflow");
        let error = world
            .run_with(
                worker_args(),
                RunOptions {
                    env: &overrides,
                    check: false,
                    group,
                    capture_limit: 1024,
                    ..RunOptions::default()
                },
            )
            .unwrap_err();
        let WorldError::Process(error) = error else {
            panic!("expected bounded collection refusal");
        };
        assert!(matches!(
            error.reason,
            Refusal::CaptureLimit { limit: 1024, .. }
        ));
        let evidence = error.evidence.unwrap();
        assert!(evidence.reaped);
        assert!(evidence.stdout.len() <= 1024 && evidence.stderr.len() <= 1024);
    }
    let error = world
        .run_with(
            [world.root.join("nonexistent")],
            RunOptions {
                check: false,
                group: true,
                ..RunOptions::default()
            },
        )
        .unwrap_err();
    assert!(matches!(error, WorldError::Process(error) if error.reason == Refusal::Spawn));
}

#[test]
fn real_git_repository_bare_clone_and_fixed_author_ignore_host_identity() {
    let world = World::new().unwrap();
    let repo = world.repository("repo", false).unwrap();
    fs::write(repo.join("tracked"), b"preserved repository bytes\n").unwrap();
    world.git(["add", "--", "tracked"]).unwrap();
    world
        .git(["commit", "-m", "Disposable initial commit"])
        .unwrap();
    let log = world
        .git(["log", "-1", "--format=%an <%ae>|%cn <%ce>"])
        .unwrap();
    assert_eq!(
        log.stdout,
        b"Test Developer <test@example.invalid>|Test Developer <test@example.invalid>\n"
    );
    assert_eq!(
        world.git(["branch", "--show-current"]).unwrap().stdout,
        b"main\n"
    );
    let bare = world.repository("origin.git", true).unwrap();
    assert_eq!(
        world
            .git_with(
                ["rev-parse", "--is-bare-repository"],
                RunOptions {
                    cwd: Some(&bare),
                    ..RunOptions::default()
                }
            )
            .unwrap()
            .stdout,
        b"true\n"
    );
    world
        .git([OsStr::new("push"), bare.as_os_str(), OsStr::new("main")])
        .unwrap();
    let cloned = world.clone(&bare, "clone").unwrap();
    assert_eq!(
        read(&cloned.join("tracked")),
        b"preserved repository bytes\n"
    );
    assert_eq!(
        world
            .git_with(
                ["status", "--porcelain"],
                RunOptions {
                    cwd: Some(&cloned),
                    ..RunOptions::default()
                }
            )
            .unwrap()
            .stdout,
        b""
    );
    assert!(world.clone("/outside", "refused").is_err());
}

#[test]
fn fixture_mapping_uses_real_registered_commands_and_refuses_unknown_helpers() {
    let world = World::new().unwrap();
    let record = world.inside("fixture.md").unwrap();
    let body = "# Preserved fixture\n\nPublic CLI validation must not rewrite this.\n";
    let text = record_fixtures::record_text(body, "75b38e17-c36e-4fab-8ba9-5fa51f25de91").unwrap();
    fs::write(&record, text.as_bytes()).unwrap();
    let output = world
        .helper(
            "scripts/record_contract.py",
            [
                OsStr::new("validate"),
                OsStr::new("--file"),
                record.as_os_str(),
            ],
        )
        .unwrap();
    let validation = json::decode_bytes(&output.stdout, json::DecodePolicy::default()).unwrap();
    assert_eq!(validation.get("valid").and_then(Value::as_bool), Some(true));
    assert_eq!(
        validation.get("writes_performed").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(validation.get("body").and_then(Value::as_str), Some(body));
    assert_eq!(read(&record), text.as_bytes());
    let output = world
        .am([
            "developer",
            "status",
            "--repo",
            world.root.join("repo").to_str().unwrap(),
        ])
        .unwrap();
    let status = json::decode_bytes(&output.stdout, json::DecodePolicy::default()).unwrap();
    assert_eq!(
        status.get("setup_required").and_then(Value::as_bool),
        Some(true)
    );
    let error = world.helper("future_fixture.py", ["apply"]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Unknown fixture helper: future_fixture.py")
    );
    for (script, route) in [
        ("bank_state.py", "inspect"),
        ("admit_learning.py", "capture"),
    ] {
        let result = world::helper_arguments(script, [route, "--help"]);
        match agent_memory::cli::table::route_named(route) {
            Some(_) => assert_eq!(result.unwrap(), [OsString::from(route), "--help".into()]),
            None => assert!(result.unwrap_err().to_string().contains("not registered")),
        }
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
fn identity_fixture_creates_developer_and_verifies_or_clearly_refuses_unregistered_binding() {
    let world = World::new().unwrap();
    let repo = world.repository("repo", false).unwrap();
    let result: support::Result<_> = support::identity_fixtures::bind(&world, &repo);
    if agent_memory::cli::table::route_named("identity").is_some() {
        assert_eq!(
            result.unwrap().get("resolved").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            support::identity_fixtures::bind(&world, &repo)
                .unwrap()
                .get("resolved")
                .and_then(Value::as_bool),
            Some(true)
        );
    } else {
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("scope_binding.py requires am identity, which is not registered")
        );
    }
    let output = world
        .helper(
            "developer_identity.py",
            [OsStr::new("status"), OsStr::new("--repo"), repo.as_os_str()],
        )
        .unwrap();
    let developer = json::decode_bytes(&output.stdout, json::DecodePolicy::default()).unwrap();
    assert_eq!(
        developer.get("setup_required").and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn record_fixture_preserves_exact_baseline_markdown_and_json_order() {
    let text = record_fixtures::record_text_with_ids(
        "# Exact body\n\nnon-ASCII café\n",
        "project",
        RecordOptions::default(),
        "record",
        "developer",
    )
    .unwrap();
    let expected = concat!(
        "---\nschema: \"agent-memory/record-v1\"\nid: \"record\"\nversion: 1\nproject-id: \"project\"\n",
        "kind: \"semantic-fact\"\nstatus: \"active\"\nscope: \"repository\"\napplicability: \"known\"\nconditions: {}\n",
        "source: {\"actor\": \"fixture-observer\", \"kind\": \"executable-test\"}\n",
        "evidence: [\"../evidence/retry-limit.txt\"]\n",
        "origin: {\"developer-id\": \"developer\", \"harness\": \"custom-harness\", \"session\": \"example-session\", \"code-state\": \"not-code\"}\n",
        "recorded-at: \"2026-10-22T21:00:00Z\"\nupdated: \"2026-10-22T21:00:00Z\"\nvalid-from: null\nvalid-until: null\n",
        "derived-from: []\ncorrects: []\nsupersedes: []\ntrust: \"reviewed\"\nvisibility: \"visible\"\n",
        "admission-reason: \"Disposable structural fixture; not real admission evidence\"\n---\n# Exact body\n\nnon-ASCII café\n",
    );
    assert_eq!(text.as_bytes(), expected.as_bytes());
    assert_eq!(
        record_fixtures::project_id(&text).unwrap(),
        Value::String("project".into())
    );
    assert!(record_fixtures::project_id("no field").is_err());
    assert!(record_fixtures::project_id("project-id: invalid").is_err());
}

#[test]
fn record_fixture_ports_every_historical_state_branch_and_option() {
    for status in ["active", "quarantined", "superseded", "revoked", "expired"] {
        let text = record_fixtures::record_text_with_ids(
            "body",
            "p",
            RecordOptions {
                scope: "component:src",
                kind: "procedural-rule",
                status,
            },
            "r",
            "d",
        )
        .unwrap();
        assert!(text.contains("kind: \"procedural-rule\"\n"));
        assert!(text.contains("scope: \"component:src\"\n"));
        let suffix = match status {
            "active" => {
                "admission-reason: \"Disposable structural fixture; not real admission evidence\"\n---\nbody"
            }
            "superseded" => {
                "state-reason: \"Historical fixture; excluded from current guidance\"\nsuperseded-by: [{\"id\": \"r\", \"version\": 2}]\n---\nbody"
            }
            _ => "state-reason: \"Historical fixture; excluded from current guidance\"\n---\nbody",
        };
        assert!(text.ends_with(suffix), "{status}");
        assert_eq!(text.contains("superseded-by:"), status == "superseded");
        assert_eq!(text.contains("state-reason:"), status != "active");
    }
    assert_ne!(
        record_fixtures::record_text("body", "p").unwrap(),
        record_fixtures::record_text("body", "p").unwrap()
    );
}

#[test]
fn spawned_child_owns_launch_data_and_can_be_collected_without_borrowing_world() {
    let world = World::new().unwrap();
    let child: support::Child = {
        let argv = vec![OsString::from("/bin/pwd")];
        world.spawn(&argv).unwrap()
    };
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(
        output.stdout,
        format!("{}\n", world.root.join("repo").display()).as_bytes()
    );
    assert!(!process::pid_is_alive(output.pid as i32).unwrap());
    assert!(child.wait().is_err());
    let command: support::Command = world.command(["/bin/pwd"], RunOptions::default()).unwrap();
    assert_eq!(command.run().unwrap().stdout, output.stdout);
}

#[test]
fn world_drop_kills_and_reaps_child_even_when_handle_outlives_world() {
    let (child, root, pid) = {
        let world = World::new().unwrap();
        let overrides = worker_options("sleep");
        let child = world
            .spawn_with(
                worker_args(),
                RunOptions {
                    env: &overrides,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        let marker = world.root.join("worker.pid");
        wait_file(&marker);
        let pid: i32 = String::from_utf8(read(&marker)).unwrap().parse().unwrap();
        assert!(process::pid_is_alive(pid).unwrap());
        (child, world.root.clone(), pid)
    };
    assert!(!root.exists());
    assert!(!process::pid_is_alive(pid).unwrap());
    assert!(child.wait().is_err());
    child.close().unwrap();
}

#[test]
fn child_drop_and_explicit_world_close_collect_active_children() {
    let world = World::new().unwrap();
    let overrides = worker_options("sleep");
    let child = world
        .spawn_with(
            worker_args(),
            RunOptions {
                env: &overrides,
                ..RunOptions::default()
            },
        )
        .unwrap();
    let marker = world.root.join("worker.pid");
    wait_file(&marker);
    let pid: i32 = String::from_utf8(read(&marker)).unwrap().parse().unwrap();
    drop(child);
    assert!(!process::pid_is_alive(pid).unwrap());
    let root = world.root.clone();
    world.close().unwrap();
    world.close().unwrap();
    assert!(!root.exists());
    assert!(world.run(["/bin/true"]).is_err());
}

#[test]
fn unwind_removes_world_and_collects_owned_child() {
    let mut surviving_handle = None;
    let mut root = PathBuf::new();
    let mut pid = 0;
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let world = World::new().unwrap();
        root = world.root.clone();
        let overrides = worker_options("sleep");
        surviving_handle = Some(
            world
                .spawn_with(
                    worker_args(),
                    RunOptions {
                        env: &overrides,
                        ..RunOptions::default()
                    },
                )
                .unwrap(),
        );
        let marker = root.join("worker.pid");
        wait_file(&marker);
        pid = String::from_utf8(read(&marker))
            .unwrap()
            .parse::<i32>()
            .unwrap();
        panic!("exercise World unwind cleanup");
    }));
    assert!(unwind.is_err());
    assert!(!root.exists());
    assert!(!process::pid_is_alive(pid).unwrap());
    assert!(surviving_handle.unwrap().wait().is_err());
}

#[test]
fn concurrent_child_bound_refuses_fourth_and_releases_collected_capacity() {
    let world = World::new().unwrap();
    let mut children = Vec::new();
    for _ in 0..3 {
        children.push(world.spawn(["/bin/sleep", "30"]).unwrap());
    }
    assert!(
        matches!(world.spawn(["/usr/bin/true"]), Err(WorldError::Invalid(message)) if message.contains("three active children"))
    );
    children.pop().unwrap().close().unwrap();
    let replacement = world.spawn(["/usr/bin/true"]).unwrap();
    assert_eq!(replacement.wait().unwrap().status, Status::Code(0));
    world.close().unwrap();
    for child in children {
        assert!(child.wait().is_err());
    }
}

#[test]
fn environment_key_allowlist_is_exactly_baseline_without_python() {
    let world = World::new().unwrap();
    let expected: std::collections::BTreeSet<_> = [
        "PATH",
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "TMPDIR",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
        "OPENCODE_CONFIG_DIR",
        "AGENT_MEMORY_CONFIG_HOME",
        "AGENT_MEMORY_STATE_HOME",
        "AGENT_MEMORY_BACKUP_HOME",
        "GH_CONFIG_DIR",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_TERMINAL_PROMPT",
        "GIT_ASKPASS",
        "SSH_ASKPASS",
        "GIT_ALLOW_PROTOCOL",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        "LC_ALL",
        "TZ",
        "NO_COLOR",
        "AGENT_MEMORY_TEST_ROOT",
        "AGENT_MEMORY_TEST_TOKEN",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        world
            .env
            .keys()
            .map(|key| key.to_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    let output = world.run(["/usr/bin/env"]).unwrap();
    let observed: std::collections::BTreeSet<_> = std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.split_once('=').unwrap().0)
        .collect();
    assert_eq!(observed, expected);
}
