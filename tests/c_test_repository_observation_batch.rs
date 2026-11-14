mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use agent_memory::{repository_config as config, scope_binding};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use support::cutover::*;
use support::{RunOptions, World};

struct SourcePack;
impl agent_memory::cli::PackRoot for SourcePack {
    fn pack_root(&self) -> agent_memory::error::Result<PathBuf> {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"))
    }
}
fn fixture() -> Recall {
    let r = Recall::new();
    r.world
        .git_with(
            ["add", "."],
            RunOptions {
                cwd: Some(&r.repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    r.world
        .git_with(
            ["commit", "-m", "Owned repository observation fixture"],
            RunOptions {
                cwd: Some(&r.repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    r
}
fn shim(w: &mut World, body: &str) {
    let directory = w.inside("git-query-faults").unwrap();
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("git");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' $$ >> '{}'\n{body}\nexec /usr/bin/git \"$@\"\n",
        w.root.join("query-pids").display()
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    let old = w.env.get(std::ffi::OsStr::new("PATH")).unwrap().clone();
    let mut search = vec![directory];
    search.extend(std::env::split_paths(&old));
    w.env
        .insert("PATH".into(), std::env::join_paths(search).unwrap());
}
fn library(w: &World, repo: &Path, mode: &str, mutation: &str) -> Value {
    let request = object([
        ("repo", text(repo.to_str().unwrap())),
        ("mode", text(mode)),
        ("mutation", text(mutation)),
    ]);
    let input = save(w, &request, "repository-call.json");
    let env = [(
        "AGENT_MEMORY_FAULT_REPOSITORY_INPUT".into(),
        input.into_os_string(),
    )];
    let output = w
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "repository_worker".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            RunOptions {
                check: false,
                env: &env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let raw = stdout
        .lines()
        .find_map(|line| line.split_once("AM-REPOSITORY:").map(|(_, raw)| raw))
        .unwrap();
    json::decode(raw, Default::default()).unwrap()
}
fn no_live_queries(w: &World) {
    if let Ok(raw) = fs::read_to_string(w.root.join("query-pids")) {
        for line in raw.lines() {
            let pid: i32 = line.parse().unwrap();
            assert!(
                !agent_memory::process::pid_is_alive(pid).unwrap(),
                "query {pid} escaped its observation"
            );
        }
    }
}
#[test]
fn repository_worker() {
    let Some(input) = std::env::var_os("AGENT_MEMORY_FAULT_REPOSITORY_INPUT") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    agent_memory::cli::register_pack_root(Some(std::sync::Arc::new(SourcePack)));
    let request = json::decode_bytes(&fs::read(input).unwrap(), Default::default()).unwrap();
    let repo = Path::new(string(&request, "repo"));
    let mode = string(&request, "mode");
    let mutation = string(&request, "mutation");
    let result: agent_memory::error::Result<Value> = if mode == "settings" || mode == "raw-settings"
    {
        let result = if mode == "settings" {
            config::settings(repo)
        } else {
            config::recovery_settings(repo)
        };
        result.map(|s| {
            object([
                ("bank_root", text(s.bank_root)),
                (
                    "authority_branch",
                    s.authority_branch.map(text).unwrap_or(Value::Null),
                ),
            ])
        })
    } else if mode == "discover" {
        agent_memory::project_identity::discover(repo, None, Default::default())
    } else {
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
        scope_binding::private_enrollment_context(repo, &pack).and_then(|context| {
            if !mutation.is_empty() {
                let other = repo.join("late-locator/MEMORY.md");
                fs::create_dir_all(other.parent().unwrap()).unwrap();
                let bytes = if mutation == "raw-only" {
                    b"# Raw source, not a memory router.\n".to_vec()
                } else {
                    fs::read(repo.join("memory-bank/MEMORY.md")).unwrap()
                };
                fs::write(&other, bytes).unwrap();
                if mutation == "indexed" || mutation == "committed" {
                    let output = std::process::Command::new("/usr/bin/git")
                        .arg("-C")
                        .arg(repo)
                        .args(["add", "late-locator/MEMORY.md"])
                        .output()
                        .unwrap();
                    assert!(output.status.success());
                }
                if mutation == "committed" {
                    for args in [
                        vec!["commit", "-m", "Concurrent committed locator fixture"],
                        vec!["rm", "late-locator/MEMORY.md"],
                    ] {
                        assert!(
                            std::process::Command::new("/usr/bin/git")
                                .arg("-C")
                                .arg(repo)
                                .args(args)
                                .output()
                                .unwrap()
                                .status
                                .success()
                        );
                    }
                }
            }
            context.within(|context| {
                Ok(object([
                    ("project_id", context.project_id().clone()),
                    ("held", Value::Bool(context.held())),
                    ("root", text(context.root().to_str().unwrap())),
                ]))
            })
        })
    };
    let output = match result {
        Ok(value) => object([("ok", Value::Bool(true)), ("value", value)]),
        Err(error) => object([
            ("ok", Value::Bool(false)),
            ("error", text(error.to_string())),
        ]),
    };
    println!(
        "AM-REPOSITORY:{}",
        json::encode(&output, json::COMPACT_ASCII).unwrap()
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_changed_head_with_deleted_competing_router_refuses_enrollment_exit
fn changed_head_with_deleted_competing_router_refuses_enrollment() {
    let r = fixture();
    assert!(yes(&library(&r.world, &r.repo, "enrollment", ""), "ok"));
    assert!(!yes(
        &library(&r.world, &r.repo, "enrollment", "committed"),
        "ok"
    ));
    let saved = physical(&r.repo);
    assert!(!yes(&library(&r.world, &r.repo, "enrollment", ""), "ok"));
    assert_eq!(physical(&r.repo), saved);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_index_untracked_and_raw_locators_added_after_scope_read_refuse_exit
fn late_indexed_untracked_and_raw_locators_refuse_enrollment() {
    for kind in ["indexed", "untracked", "raw-only"] {
        let r = fixture();
        assert!(!yes(&library(&r.world, &r.repo, "enrollment", kind), "ok"));
        let saved = physical(&r.repo);
        assert!(!yes(&library(&r.world, &r.repo, "enrollment", ""), "ok"));
        assert_eq!(physical(&r.repo), saved);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_earlier_real_config_refusal_beats_every_later_child_failure
fn invalid_real_config_precedes_every_later_child_failure() {
    for command_name in ["ls-files", "ls-tree"] {
        for body in [
            "exec /missing-owned-git",
            "exec /bin/sh /missing-owned-input",
            "exit 17",
            "kill -TERM $$",
            "exec /bin/sleep 30",
            "printf 'malformed inventory without a NUL'; exit 0",
        ] {
            let mut r = fixture();
            for root in ["memory-bank", "another-bank"] {
                r.world
                    .git_with(
                        ["config", "--add", "agent-memory.bankRoot", root],
                        RunOptions {
                            cwd: Some(&r.repo),
                            ..RunOptions::default()
                        },
                    )
                    .unwrap();
            }
            let before = physical(&r.repo);
            shim(
                &mut r.world,
                &format!("for a do if [ \"$a\" = '{command_name}' ]; then {body}; fi; done"),
            );
            let value = library(&r.world, &r.repo, "settings", "");
            assert!(!yes(&value, "ok"));
            assert!(string(&value, "error").contains("one explicit value per key"));
            assert_eq!(physical(&r.repo), before);
            no_live_queries(&r.world);
        }
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_config_child_output_never_accepts_speculative_names
fn malformed_config_child_output_never_accepts_speculative_names() {
    for printf in [
        "agent-memory.bankroot\\nmemory-bank",
        "agent-memory.bankroot\\000",
        "agent-memory.unknown\\nmemory-bank\\000",
        "agent-memory.bankroot\\nmemory-bank\\000agent-memory.bankroot\\nmemory-bank\\000",
    ] {
        let mut r = fixture();
        let before = physical(&r.repo);
        shim(
            &mut r.world,
            &format!(
                "for a do if [ \"$a\" = '--get-regexp' ]; then printf '{printf}'; exit 0; fi; done"
            ),
        );
        let value = library(&r.world, &r.repo, "settings", "");
        assert!(!yes(&value, "ok"));
        assert!(string(&value, "error").contains("Repository memory configuration"));
        assert_eq!(physical(&r.repo), before);
        no_live_queries(&r.world);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_name_children_refuse_after_valid_real_config
fn malformed_inventory_children_refuse_after_valid_real_config() {
    for (command_name, printf) in [
        ("ls-files", "H memory-bank/MEMORY.md"),
        ("ls-files", "X memory-bank/MEMORY.md\\000"),
        ("ls-tree", "/outside/MEMORY.md\\000"),
    ] {
        let mut r = fixture();
        let before = physical(&r.repo);
        shim(
            &mut r.world,
            &format!(
                "for a do if [ \"$a\" = '{command_name}' ]; then printf '{printf}'; exit 0; fi; done"
            ),
        );
        assert!(!yes(&library(&r.world, &r.repo, "settings", ""), "ok"));
        assert_eq!(physical(&r.repo), before);
        no_live_queries(&r.world);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_parent_traversal_name_never_adopts_outside_router
fn parent_traversal_inventory_never_adopts_outside_router() {
    let mut r = fixture();
    let outside = r.world.inside("outside/MEMORY.md").unwrap();
    fs::create_dir(outside.parent().unwrap()).unwrap();
    let router = fs::read(r.bank.join("MEMORY.md")).unwrap();
    fs::write(&outside, &router).unwrap();
    let before = physical(&r.repo);
    let normal = library(&r.world, &r.repo, "settings", "");
    let raw = library(&r.world, &r.repo, "raw-settings", "");
    shim(
        &mut r.world,
        "for a do if [ \"$a\" = 'ls-tree' ]; then printf '../outside/MEMORY.md\\000'; exit 0; fi; done",
    );
    assert_eq!(library(&r.world, &r.repo, "settings", ""), normal);
    assert_eq!(library(&r.world, &r.repo, "raw-settings", ""), raw);
    assert!(fs::symlink_metadata(&outside).unwrap().is_file());
    assert_eq!(fs::read(outside).unwrap(), router);
    assert_eq!(physical(&r.repo), before);
    no_live_queries(&r.world);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_config_validation_time_is_charged_to_unresolved_inventory_child
fn config_observation_time_is_charged_to_pending_inventory_deadline() {
    let mut r = fixture();
    let before = physical(&r.repo);
    shim(
        &mut r.world,
        "for a do if [ \"$a\" = 'ls-files' ]; then exec /bin/sleep 30; fi; done\nfor a do if [ \"$a\" = '--get-regexp' ]; then /bin/sleep 1; fi; done",
    );
    let start = Instant::now();
    let value = library(&r.world, &r.repo, "settings", "");
    assert!(!yes(&value, "ok"));
    assert!(string(&value, "error").contains("inventory is unavailable"));
    assert!(start.elapsed() < Duration::from_secs(15));
    assert_eq!(physical(&r.repo), before);
    no_live_queries(&r.world);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_sampled_head_is_immutable_and_next_settings_read_observes_later_commit
fn sampled_head_is_immutable_and_next_settings_read_observes_later_commit() {
    let mut r = fixture();
    let marker = r.world.root.join("committed-once");
    let body = format!(
        "for a do if [ \"$a\" = 'ls-files' ] && [ ! -e '{marker}' ]; then : > '{marker}'; mkdir '{repo}/late-commit'; cp '{repo}/memory-bank/MEMORY.md' '{repo}/late-commit/MEMORY.md'; /usr/bin/git -C '{repo}' add late-commit/MEMORY.md; /usr/bin/git -C '{repo}' commit -m 'Late committed router' >/dev/null; /usr/bin/git -C '{repo}' rm late-commit/MEMORY.md >/dev/null; fi; done",
        marker = marker.display(),
        repo = r.repo.display()
    );
    shim(&mut r.world, &body);
    let first = library(&r.world, &r.repo, "settings", "");
    assert!(yes(&first, "ok"), "{first:?}");
    assert!(marker.exists());
    let before = physical(&r.repo);
    assert!(!yes(&library(&r.world, &r.repo, "settings", ""), "ok"));
    assert_eq!(physical(&r.repo), before);
    no_live_queries(&r.world);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_timeout_and_launch_failure_never_mint_identity_from_partial_output
fn timeout_and_git_launch_failures_never_mint_identity_from_partial_output() {
    for body in [
        "exec /bin/sleep 30",
        "exec /missing-owned-git",
        "exec /bin/sh /missing-owned-input",
    ] {
        let mut r = fixture();
        let before = physical(&r.repo);
        let root = r.repo.to_str().unwrap().to_owned();
        shim(
            &mut r.world,
            &format!(
                "for a do if [ \"$a\" = '--show-toplevel' ]; then printf '{root}\\n{}\\n'; {body}; fi; done",
                "a".repeat(40)
            ),
        );
        assert!(!yes(&library(&r.world, &r.repo, "settings", ""), "ok"));
        assert!(!yes(&library(&r.world, &r.repo, "enrollment", ""), "ok"));
        assert_eq!(physical(&r.repo), before);
        no_live_queries(&r.world);
        let plain = r.world.inside("genuine non-Git launch failure").unwrap();
        fs::create_dir(&plain).unwrap();
        let value = library(&r.world, &plain, "settings", "");
        assert!(yes(&value, "ok"));
        assert_eq!(string(get(&value, "value"), "bank_root"), "memory-bank");
        assert!(!plain.join("memory-bank").exists());
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_tracked_body_and_enrollment_boundaries_have_no_live_query_children
fn tracked_body_and_enrollment_boundaries_leave_no_live_query_children() {
    let mut r = fixture();
    let marker = r.world.root.join("group-query-pids");
    let body = format!(
        "grouped=no\nfor a do case \"$a\" in ls-files|ls-tree|--get-regexp|remote) grouped=yes;; esac; done\nif [ \"$grouped\" = yes ]; then printf '%s\\n' $$ >> '{marker}'; elif [ -f '{marker}' ]; then while read p; do if kill -0 \"$p\" 2>/dev/null; then printf 'query escaped boundary' >&2; exit 99; fi; done < '{marker}'; fi",
        marker = marker.display()
    );
    shim(&mut r.world, &body);
    let before = physical(&r.repo);
    assert!(yes(&library(&r.world, &r.repo, "settings", ""), "ok"));
    assert!(yes(&library(&r.world, &r.repo, "enrollment", ""), "ok"));
    assert!(!fs::read_to_string(marker).unwrap().is_empty());
    assert_eq!(physical(&r.repo), before);
    no_live_queries(&r.world);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_interrupt_in_real_config_validation_preserves_traceback_and_cancels_siblings
fn interrupt_during_real_config_observation_cancels_siblings_and_preserves_bank() {
    for signal in ["INT", "TERM"] {
        let mut r = fixture();
        let before = physical(&r.repo);
        shim(
            &mut r.world,
            &format!(
                "for a do if [ \"$a\" = '--get-regexp' ]; then kill -{signal} \"$PPID\"; exec /bin/sleep 30; fi; done"
            ),
        );
        let output = r
            .world
            .am_with(
                [
                    "--json".into(),
                    "bank".into(),
                    "validate".into(),
                    "--repo".into(),
                    r.repo.as_os_str().to_owned(),
                ],
                RunOptions {
                    check: false,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        assert_eq!(
            output.status,
            Status::Signal(if signal == "INT" {
                libc::SIGINT
            } else {
                libc::SIGTERM
            })
        );
        assert!(output.stdout.is_empty());
        assert_eq!(physical(&r.repo), before);
        no_live_queries(&r.world);
    }
}
