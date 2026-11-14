#![cfg(feature = "dev-tools")]
mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::cutover::*;
use support::{RunOptions, World};
fn request(repo: &Path, runtime: &Path, action: &str) -> Value {
    object([
        ("repo", text(repo.to_str().unwrap())),
        ("runtime", text(runtime.to_str().unwrap())),
        ("action", text(action)),
    ])
}
fn spawn(w: &World, request: &Value, stage: Option<&str>) -> (support::Child, PathBuf) {
    let path = save(w, request, "observation-request.json");
    let directory = w.root.join("observation-pause");
    let mut env = vec![(
        "AGENT_MEMORY_FAULT_GAPS_OBSERVATION".into(),
        path.into_os_string(),
    )];
    if let Some(stage) = stage {
        env.extend([
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                directory.clone().into_os_string(),
            ),
        ]);
    }
    let child = w
        .spawn_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "observation_worker".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                env: &env,
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    if let Some(stage) = stage {
        let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(15)).unwrap();
        while !directory.join(format!("{stage}.ready")).exists() {
            assert!(!deadline.expired(), "missing {stage}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    (child, directory)
}
fn collect(child: support::Child) -> Value {
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(0), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    json::decode(
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("GAP-OBSERVATION:"))
            .unwrap(),
        Default::default(),
    )
    .unwrap()
}
fn run(w: &World, request: &Value) -> Value {
    collect(spawn(w, request, None).0)
}
#[test]
fn observation_worker() {
    let Some(path) = std::env::var_os("AGENT_MEMORY_FAULT_GAPS_OBSERVATION") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    let request = json::decode_bytes(&fs::read(path).unwrap(), Default::default()).unwrap();
    let repo = Path::new(string(&request, "repo"));
    let runtime = Path::new(string(&request, "runtime"));
    let result = match string(&request, "action") {
        "create-barrier" => {
            let base = agent_memory::omp_boundary::base(repo).unwrap();
            let guard = agent_memory::turn_sources::source_barrier_at(&base, true, false).unwrap();
            guard.recheck().unwrap();
            Ok(text(base.to_str().unwrap()))
        }
        "private" => agent_memory::private_state::required_private_state_capabilities(
            None,
            Some(&object([(
                "private_state_capabilities",
                Value::Array(vec![]),
            )])),
            "install",
            repo,
            runtime,
        ),
        "discover" => agent_memory::project_identity::discover(repo, None, Default::default()),
        "banks" => agent_memory::updating::banks(
            &agent_memory::installation::Environment::process(),
            runtime,
            &[repo.to_str().unwrap().to_owned()],
            &object([
                (
                    "readable_bank_schemas",
                    Value::Array(vec![text("agent-memory/v1")]),
                ),
                ("migrator_source_schemas", Value::Array(vec![])),
            ]),
            true,
        )
        .map_err(|error| agent_memory::error::ContractError::new(error.to_string())),
        _ => panic!("unknown observation fixture"),
    };
    let value = match result {
        Ok(value) => object([("ok", Value::Bool(true)), ("value", value)]),
        Err(error) => object([
            ("ok", Value::Bool(false)),
            ("error", text(error.to_string())),
        ]),
    };
    println!(
        "GAP-OBSERVATION:{}",
        json::encode(&value, json::DEFAULT).unwrap()
    );
}
fn routerless() -> (World, PathBuf, PathBuf, PathBuf) {
    let w = World::new().unwrap();
    let (runtime, _) = support::installed_runtime::installed_runtime(&w);
    let repo = w.repository("notes-only", false).unwrap();
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    fs::write(
        bank.join("my-note.md"),
        "Preserve ordinary notes without inventing a router.\n",
    )
    .unwrap();
    (w, runtime, repo, bank)
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_existing_barrier_deleted_before_open_is_not_recreated
fn existing_barrier_deleted_before_open_is_not_recreated() {
    let (w, runtime, repo, bank) = routerless();
    let base = run(&w, &request(&repo, &runtime, "create-barrier"));
    let lock = Path::new(get(&base, "value").as_str().unwrap()).join("source-barrier.lock");
    let before = fs::read(bank.join("my-note.md")).unwrap();
    let stage = "private_state.turn_sources.private_lock";
    let (child, directory) = spawn(&w, &request(&repo, &runtime, "private"), Some(stage));
    fs::remove_file(&lock).unwrap();
    fs::write(directory.join(format!("{stage}.release")), b"release").unwrap();
    let result = collect(child);
    assert!(yes(&result, "ok"));
    assert_eq!(string(get(&result, "value"), "decision"), "pending");
    assert!(!lock.exists());
    assert!(!bank.join("MEMORY.md").exists());
    assert_eq!(fs::read(bank.join("my-note.md")).unwrap(), before);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_native_barrier_appearing_during_layout_observation_refuses_without_erasing_it
fn native_barrier_appearing_during_layout_observation_is_preserved() {
    let (w, runtime, repo, bank) = routerless();
    let before = physical(&bank);
    let stage = "private_state.recovery_frontier._observe_bank";
    let (child, directory) = spawn(&w, &request(&repo, &runtime, "private"), Some(stage));
    let base = run(&w, &request(&repo, &runtime, "create-barrier"));
    let lock = Path::new(get(&base, "value").as_str().unwrap()).join("source-barrier.lock");
    let bytes = fs::read(&lock).unwrap();
    fs::write(directory.join(format!("{stage}.release")), b"release").unwrap();
    let result = collect(child);
    assert_eq!(string(get(&result, "value"), "decision"), "pending");
    assert_eq!(fs::read(lock).unwrap(), bytes);
    assert_eq!(physical(&bank), before);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_update_mutable_observations.py::FreshObservationControls.test_mutations_between_router_observations_refuse
fn mutations_between_router_observations_refuse() {
    for mutation in ["head", "index", "config", "bank", "lock"] {
        let r = Recall::new();
        let (runtime, _) = support::installed_runtime::installed_runtime(&r.world);
        r.world
            .git_with(
                ["add", "."],
                RunOptions {
                    cwd: Some(&r.repo),
                    ..Default::default()
                },
            )
            .unwrap();
        r.world
            .git_with(
                ["commit", "-m", "Observed bank"],
                RunOptions {
                    cwd: Some(&r.repo),
                    ..Default::default()
                },
            )
            .unwrap();
        for n in 0..3 {
            let sibling = r.world.root.join(format!("sibling-{n}"));
            r.world
                .git_with(
                    [
                        OsString::from("worktree"),
                        "add".into(),
                        "--detach".into(),
                        sibling.into_os_string(),
                    ],
                    RunOptions {
                        cwd: Some(&r.repo),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let stage = "updating.migration.read_regular";
        let (child, directory) = spawn(&r.world, &request(&r.repo, &runtime, "banks"), Some(stage));
        match mutation {
            "head" | "index" => {
                fs::create_dir(r.repo.join("other")).unwrap();
                fs::copy(r.bank.join("MEMORY.md"), r.repo.join("other/MEMORY.md")).unwrap();
                r.world
                    .git_with(
                        ["add", "other/MEMORY.md"],
                        RunOptions {
                            cwd: Some(&r.repo),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                if mutation == "head" {
                    r.world
                        .git_with(
                            ["commit", "-m", "Changed observed HEAD"],
                            RunOptions {
                                cwd: Some(&r.repo),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
            "config" => {
                r.world
                    .git_with(
                        ["config", "--local", "agent-memory.bankroot", "other"],
                        RunOptions {
                            cwd: Some(&r.repo),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            "bank" => {
                let path = r.bank.join("MEMORY.md");
                fs::write(&path, format!("{}\n", fs::read_to_string(&path).unwrap())).unwrap();
            }
            "lock" => {
                fs::write(r.bank.join(".agent-memory.lock"), "active operation").unwrap();
            }
            _ => unreachable!(),
        }
        let after = physical(&r.bank);
        fs::write(directory.join(format!("{stage}.release")), b"release").unwrap();
        let result = collect(child);
        assert!(!yes(&result, "ok"), "{mutation}: {result:?}");
        assert_eq!(physical(&r.bank), after);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_remote_query_cleanup_failure_cannot_become_successful_discovery
fn remote_query_cleanup_failure_never_becomes_successful_discovery() {
    for close_fault in [false, true] {
        let mut r = Recall::new();
        let (runtime, _) = support::installed_runtime::installed_runtime(&r.world);
        r.world
            .git_with(
                [
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/example/project.git",
                ],
                RunOptions {
                    cwd: Some(&r.repo),
                    ..Default::default()
                },
            )
            .unwrap();
        let tools = r.world.root.join("remote-query-tools");
        fs::create_dir(&tools).unwrap();
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' $$ >> '{}'\nfor argument do\nif [ \"$argument\" = get-url ]; then exec /bin/sleep 30; fi\ndone\nexec /usr/bin/git \"$@\"\n",
            r.world.root.join("query-pids").display()
        );
        fs::write(tools.join("git"), script).unwrap();
        support::lifecycle::mode(&tools.join("git"), 0o700);
        let old = r.world.env[std::ffi::OsStr::new("PATH")].clone();
        r.world.env.insert(
            "PATH".into(),
            std::env::join_paths(std::iter::once(tools).chain(std::env::split_paths(&old)))
                .unwrap(),
        );
        if close_fault {
            r.world.env.insert(
                "AGENT_MEMORY_FAULT_STAGE".into(),
                "project_identity.remote_get_url.Popen.stdout.close".into(),
            );
            r.world
                .env
                .insert("AGENT_MEMORY_FAULT_MODE".into(), "error".into());
        }
        let bank = physical(&r.bank);
        let code = r.code();
        let result = run(&r.world, &request(&r.repo, &runtime, "discover"));
        if close_fault {
            assert!(!yes(&result, "ok"));
        } else {
            assert!(yes(&result, "ok"));
            assert_eq!(get(get(&result, "value"), "canonical_remote"), &Value::Null);
            assert!(!yes(
                get(get(get(&result, "value"), "remotes"), "origin"),
                "consistent"
            ));
        }
        for pid in fs::read_to_string(r.world.root.join("query-pids"))
            .unwrap()
            .lines()
        {
            assert!(!agent_memory::process::pid_is_alive(pid.parse().unwrap()).unwrap());
        }
        assert_eq!(physical(&r.bank), bank);
        assert_eq!(r.code(), code);
    }
}
