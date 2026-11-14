use agent_memory::client_hooks as hooks;
use agent_memory::codec::json::{self, DecodePolicy};
use agent_memory::fs::descriptor::Descriptor;
use std::os::unix::fs::DirBuilderExt;

use agent_memory::clock::Deadline;
use agent_memory::codec::json::Value;
use agent_memory::process::{self, Input, Spec, Status, Target};
use agent_memory::{hook_failure, managed_source, sharing_hold_mapping as mapping, turn_check};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct World(PathBuf);
impl World {
    fn new() -> Self {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "am-leaves-{}",
                agent_memory::host::uuid4().unwrap()
            ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        for name in ["home", "state", "bin"] {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(name))
                .unwrap();
        }
        Self(root)
    }
    fn env(&self) -> Vec<(OsString, OsString)> {
        vec![
            ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
            ("HOME".into(), self.0.join("home").into_os_string()),
            (
                "XDG_STATE_HOME".into(),
                self.0.join("state").into_os_string(),
            ),
            (
                "AGENT_MEMORY_STATE_HOME".into(),
                self.0.join("state/agent-memory").into_os_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("LC_ALL".into(), "C".into()),
        ]
    }
    fn git(&self, repo: &Path, args: &[&str]) -> String {
        let mut argv: Vec<OsString> = [
            "git",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
            "-C",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        argv.push(repo.into());
        argv.extend(args.iter().map(OsString::from));
        let output = run(&argv, &self.env(), 10);
        assert_eq!(output.status, Status::Code(0), "{:?}", output);
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
    fn repository(&self, name: &str) -> PathBuf {
        let repo = self.0.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&repo)
            .unwrap();
        self.git(&repo, &["init", "-b", "main"]);
        repo
    }
    fn commit(&self, repo: &Path) -> String {
        self.git(repo, &["add", "."]);
        self.git(repo, &["commit", "-m", "Source fixture"]);
        self.git(repo, &["rev-parse", "HEAD"])
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn run(argv: &[OsString], env: &[(OsString, OsString)], seconds: u64) -> process::Output {
    process::run(&Spec {
        argv,
        env,
        cwd: None,
        search_path: true,
        stdin: Input::Null,
        stdout: Target::Capture { limit: 1024 * 1024 },
        stderr: Target::Capture { limit: 1024 * 1024 },
        deadline: Deadline::after(Duration::from_secs(seconds)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .unwrap()
}
fn write(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}
fn value(text: &str) -> Value {
    json::decode(text, DecodePolicy::default()).unwrap()
}
fn metadata() -> Value {
    value(
        r#"{"schema":"agent-memory/hook-failure-v1","harness":"codex","adapter":"prompt","stage":"identity","reason":"identity.unresolved","timestamp":"2026-11-13T12:00:00+00:00"}"#,
    )
}
fn case(name: &str) {
    let world = World::new();
    let mut env = world.env();
    if name == "linked-state" {
        let repo = world.repository("repo");
        write(&repo.join("a"), b"a");
        world.commit(&repo);
        let linked = world.0.join("linked");
        world.git(
            &repo,
            &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
        );
        env.retain(|(key, _)| key != "AGENT_MEMORY_STATE_HOME");
        env.push((
            "AGENT_MEMORY_STATE_HOME".into(),
            repo.join("memory-bank/private-hook-state").into_os_string(),
        ));
    }
    env.push(("AM_LEAVES_ROOT".into(), world.0.clone().into_os_string()));
    env.push(("AM_LEAVES_CASE".into(), name.into()));
    if matches!(name, "overflow" | "deadline" | "object-size") {
        let script = match name {
            "overflow" => format!(
                "#!/bin/sh\nprintf '%s' \"$$\" > \"$AM_LEAVES_ROOT/pid\"\nprintf '%s' '{}'\nexec /bin/sleep 60\n",
                "x".repeat(10000)
            ),
            "deadline" => {
                "#!/bin/sh\nprintf '%s' \"$$\" > \"$AM_LEAVES_ROOT/pid\"\nexec /bin/sleep 60\n"
                    .into()
            }
            _ => format!(
                "#!/bin/sh\nprintf '%s' '{}' >&2\nprintf '+1\\n'\n",
                "x".repeat(100000)
            ),
        };
        write(&world.0.join("bin/git"), script.as_bytes());
        chmod(&world.0.join("bin/git"), 0o700);
        env.retain(|(key, _)| key != "PATH");
        env.push(("PATH".into(), world.0.join("bin").into_os_string()));
    }
    if name == "environment" {
        env.push((
            "GIT_DIR".into(),
            world.0.join("wrong/.git").into_os_string(),
        ));
        env.push((
            "GIT_INDEX_FILE".into(),
            world.0.join("wrong-index").into_os_string(),
        ));
    }
    let argv = vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "filesystem_child".into(),
        "--nocapture".into(),
    ];
    let result = run(&argv, &env, 50);
    assert_eq!(
        result.status,
        Status::Code(0),
        "case {name}: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
// obligation: tests/contracts/test_client_hooks.py::ClientHookContract.test_owned_entry_roundtrip_preserves_foreign_edits
#[test]
fn owned_roundtrip_preserves_foreign_edits() {
    let original = br#"{"description":"foreign","hooks":{"Stop":[{"hooks":[{"type":"command","command":"foreign"}]}],"UserPromptSubmit":[{"hooks":[{"type":"command","command":"other"}]}]},"new-user-setting":"preserved"}"#;
    let mine = hooks::entry("python3 '/path with spaces/hook.py'").unwrap();
    let raw = hooks::merge(original, None, &mine, hooks::EVENT).unwrap();
    let updated = hooks::merge(&raw, Some(&mine), &mine, hooks::EVENT).unwrap();
    let removed = hooks::remove(&updated, &mine, false, true, true, hooks::EVENT)
        .unwrap()
        .unwrap();
    assert!(
        hooks::equal(
            &hooks::parse(&removed).unwrap(),
            &hooks::parse(original).unwrap()
        )
        .unwrap()
    );
}

// obligation: tests/contracts/test_client_hooks.py::ClientHookContract.test_unowned_collision_and_mutated_or_duplicate_owned_entries_refuse
#[test]
fn ownership_collision_and_missing_are_refused() {
    let mine = hooks::entry("python3 owned.py").unwrap();
    let initial = hooks::merge(b"", None, &mine, hooks::EVENT).unwrap();
    assert!(hooks::merge(&initial, None, &mine, hooks::EVENT).is_err());
    let changed =
        br#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"changed"}]}]}}"#;
    assert!(hooks::remove(changed, &mine, false, false, false, hooks::EVENT).is_err());
    let duplicate = br#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"python3 owned.py"}]},{"hooks":[{"type":"command","command":"python3 owned.py"}]}]}}"#;
    assert!(hooks::remove(duplicate, &mine, false, false, false, hooks::EVENT).is_err());
    assert_eq!(
        hooks::remove(&initial, &mine, true, false, false, hooks::EVENT).unwrap(),
        None
    );
}

// obligation: tests/contracts/test_client_hooks.py::ClientHookContract.test_ambiguous_json_is_refused
#[test]
fn ambiguous_json_refused() {
    assert!(hooks::parse(br#"{"hooks":{},"hooks":{}}"#).is_err());
    assert!(hooks::parse(br#"{"hooks":[]}"#).is_err());
}

// obligation: tests/contracts/test_client_hooks.py::ClientHookContract.test_stop_shape_and_json_boolean_are_not_coerced
#[test]
fn stop_boolean_is_not_numeric() {
    let owned = json::decode(
        r#"{"hooks":[{"type":"command","command":"stop","asyncRewake":true}]}"#,
        DecodePolicy::default(),
    )
    .unwrap();
    let good = hooks::parse(br#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"stop","asyncRewake":true}]}]}}"#).unwrap();
    let changed = hooks::parse(
        br#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"stop","asyncRewake":1}]}]}}"#,
    )
    .unwrap();
    assert_eq!(hooks::locate(&good, &owned, "Stop").unwrap(), 0);
    assert!(hooks::locate(&changed, &owned, "Stop").is_err());
    assert!(hooks::parse(br#"{"hooks":{"Stop":{}}}"#).is_err());
    assert!(
        hooks::parse(br#"{"hooks":{"Stop":[{"hooks":[{"type":"command","type":"other"}]}]}}"#)
            .is_err()
    );
}

#[test]
fn bounded_directory_entries() {
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "am-leaves-{}",
            agent_memory::host::uuid4().unwrap()
        ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let directory = Descriptor::open_directory(&root).unwrap();
    assert_eq!(directory.entries_bounded(0).unwrap(), Some(vec![]));
    std::fs::write(root.join("b"), b"").unwrap();
    assert_eq!(directory.entries_bounded(0).unwrap(), None);
    assert_eq!(
        directory.entries_bounded(1).unwrap().unwrap(),
        vec![std::ffi::OsString::from("b")]
    );
    std::fs::write(root.join("a"), b"").unwrap();
    assert_eq!(directory.entries_bounded(1).unwrap(), None);
    assert_eq!(
        directory.entries_bounded(2).unwrap().unwrap(),
        vec![std::ffi::OsString::from("a"), std::ffi::OsString::from("b")]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_and_native_commands_are_separate() {
    let (prompt, stop) =
        hooks::python_era_entries("claude", std::path::Path::new("/a b/pack")).unwrap();
    assert_eq!(
        prompt.get("hooks").unwrap().as_array().unwrap()[0]
            .get("command")
            .unwrap()
            .as_str(),
        Some("/bin/sh '/a b/pack/hooks/python3.sh' '/a b/pack/hooks/turn_hook.py'")
    );
    assert_eq!(
        stop.get("hooks").unwrap().as_array().unwrap()[0]
            .get("asyncRewake")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        hooks::rust_era_command(std::path::Path::new("/a b/am"), "claude-stop").unwrap(),
        "'/a b/am' hook claude-stop"
    );
}

#[test]
fn hook_metadata_and_compact_request_validation() {
    let decode = |text: &str| json::decode(text, DecodePolicy::default()).unwrap();
    let value = decode(
        r#"{"schema":"agent-memory/hook-failure-v1","harness":"codex","adapter":"prompt","stage":"identity","reason":"identity.unresolved","timestamp":"2026-11-13T12:00:00+00:00"}"#,
    );
    assert!(agent_memory::hook_failure::validate(&value, "codex", "prompt").is_ok());
    assert!(agent_memory::hook_failure::validate(&value, "codex", "stop").is_err());
    let batch = decode(
        r#"{"schema":"agent-memory/turn-check-request-v1","assessments":[{"observation_id":"one","session":"s","prompt":"p","outcome":"no-learning"},{"observation_id":"two","session":"s","prompt":"p","outcome":"no-learning"}]}"#,
    );
    assert_eq!(agent_memory::turn_check::requests(&batch).unwrap().len(), 2);
    let duplicate = decode(
        r#"{"schema":"agent-memory/turn-check-request-v1","assessments":[{"observation_id":"one","outcome":"no-learning"},{"observation_id":"one","outcome":"no-learning"}]}"#,
    );
    assert!(agent_memory::turn_check::requests(&duplicate).is_err());
}

#[test]
fn collision_uses_python_structural_equality() {
    let raw = br#"{"hooks":{"UserPromptSubmit":[{"a":true,"b":2}]}}"#;
    let desired = json::decode(r#"{"b":2.0,"a":1}"#, DecodePolicy::default()).unwrap();
    assert!(hooks::merge(raw, None, &desired, hooks::EVENT).is_err());
}

#[test]
fn filesystem_child() {
    let Some(name) = std::env::var_os("AM_LEAVES_CASE") else {
        return;
    };
    let world = std::mem::ManuallyDrop::new(World(PathBuf::from(
        std::env::var_os("AM_LEAVES_ROOT").unwrap(),
    )));
    match name.to_str().unwrap() {
        "linked-state" => {
            assert!(
                hook_failure::record(
                    &world.0.join("linked"),
                    "codex",
                    "prompt",
                    "identity",
                    "identity.unresolved"
                )
                .is_err()
            );
            assert!(!world.0.join("repo/memory-bank/private-hook-state").exists());
        }
        "independent" => {
            let repo = world.repository("repo");
            let (directory, slot) = hook_failure::paths(&repo, "codex", "prompt").unwrap();
            assert!(hook_failure::read(&repo, None).unwrap().is_empty());
            assert!(!directory.exists());
            write(&repo.join("a"), b"a");
            world.commit(&repo);
            let linked = world.0.join("linked");
            world.git(
                &repo,
                &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
            );
            assert!(
                hook_failure::record(&repo, "codex", "prompt", "identity", "identity.unresolved")
                    .unwrap()
            );
            hook_failure::record(&repo, "codex", "stop", "boundary", "boundary.missing").unwrap();
            assert!(hook_failure::read(&linked, None).unwrap().is_empty());
            assert_eq!(hook_failure::read(&repo, None).unwrap().len(), 2);
            assert_eq!(
                std::fs::metadata(directory.join(&slot))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert!(std::fs::metadata(directory.join(&slot)).unwrap().len() < 4096);
            assert!(!hook_failure::clear(&linked, "codex", "prompt").unwrap());
            assert_eq!(hook_failure::read(&repo, None).unwrap().len(), 2);
            assert!(hook_failure::clear(&repo, "codex", "prompt").unwrap());
            assert_eq!(
                hook_failure::read(&repo, None).unwrap()[0]
                    .get("adapter")
                    .and_then(Value::as_str),
                Some("stop")
            );
            assert!(
                hook_failure::record(
                    &repo,
                    "codex",
                    "stop",
                    "identity",
                    "sensitive arbitrary exception"
                )
                .is_err()
            );
            symlink(world.0.join("foreign"), directory.join(slot)).unwrap();
            assert!(
                hook_failure::record(&repo, "codex", "prompt", "identity", "identity.unresolved")
                    .is_err()
            );
            let forbidden = repo.join("memory-bank/private-hook-state");
            // paths() follows the process environment; use a separately isolated
            // test process to verify linked-checkout exclusion below.
            assert!(!forbidden.exists());
        }
        "unsafe" => {
            let repo = world.repository("repo");
            hook_failure::record(&repo, "codex", "prompt", "identity", "identity.unresolved")
                .unwrap();
            let (directory, slot) = hook_failure::paths(&repo, "codex", "prompt").unwrap();
            let path = directory.join(&slot);
            write(&path, &vec![b'X'; 4097]);
            assert_eq!(
                hook_failure::read(&repo, None).unwrap_err().to_string(),
                "Unsafe hook failure metadata"
            );
            assert!(hook_failure::clear(&repo, "codex", "prompt").unwrap());
            assert!(!path.exists());
            hook_failure::record(&repo, "codex", "prompt", "identity", "identity.unresolved")
                .unwrap();
            write(&path, b"{}");
            hook_failure::record(&repo, "codex", "prompt", "boundary", "boundary.missing").unwrap();
            assert_eq!(
                hook_failure::read(&repo, None).unwrap()[0]
                    .get("reason")
                    .and_then(Value::as_str),
                Some("boundary.missing")
            );
            chmod(&path, 0o644);
            assert!(hook_failure::read(&repo, None).is_err());
            assert!(hook_failure::clear(&repo, "codex", "prompt").is_err());
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o644
            );
            chmod(&path, 0o600);
            std::fs::hard_link(&path, directory.join("alias")).unwrap();
            assert!(hook_failure::clear(&repo, "codex", "prompt").is_err());
        }
        "malformed" => {
            let repo = world.repository("repo");
            let (directory, _) = hook_failure::paths(&repo, "codex", "prompt").unwrap();
            assert!(!hook_failure::clear(&repo, "codex", "prompt").unwrap());
            assert!(!directory.exists());
            hook_failure::record(&repo, "codex", "stop", "identity", "identity.unresolved")
                .unwrap();
            let before = Descriptor::open_directory(&directory)
                .unwrap()
                .entries()
                .unwrap();
            assert!(!hook_failure::clear(&repo, "codex", "prompt").unwrap());
            assert_eq!(
                Descriptor::open_directory(&directory)
                    .unwrap()
                    .entries()
                    .unwrap(),
                before
            );
            let (_, slot) = hook_failure::paths(&repo, "codex", "stop").unwrap();
            for field in ["harness", "adapter", "stage", "reason", "timestamp"] {
                let mut invalid = metadata();
                *invalid.get_mut("adapter").unwrap() = Value::String("stop".into());
                *invalid.get_mut(field).unwrap() = Value::Array(vec![]);
                write(
                    &directory.join(&slot),
                    json::encode(&invalid, json::DEFAULT).unwrap().as_bytes(),
                );
                assert!(hook_failure::read(&repo, None).is_err(), "{field}");
            }
            hook_failure::record(&repo, "codex", "stop", "boundary", "boundary.missing").unwrap();
            assert_eq!(hook_failure::read(&repo, None).unwrap().len(), 1);
        }
        "non-git" => {
            let repo = world.0.join("file-only");
            std::fs::create_dir(&repo).unwrap();
            assert!(hook_failure::read(&repo, None).unwrap().is_empty());
            assert!(matches!(
                hook_failure::record(&repo, "codex", "prompt", "identity", "identity.unresolved"),
                Err(hook_failure::Error::NoRepository(_))
            ));
        }
        "corrupt" => {
            let repo = world.repository("repo");
            for adapter in ["prompt", "stop"] {
                hook_failure::record(&repo, "codex", adapter, "identity", "identity.unresolved")
                    .unwrap();
            }
            let (directory, slot) = hook_failure::paths(&repo, "codex", "prompt").unwrap();
            write(&directory.join(slot), b"{invalid");
            let mut errors = vec![];
            let records = hook_failure::read(&repo, Some(&mut errors)).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(
                records[0].get("adapter").and_then(Value::as_str),
                Some("stop")
            );
            assert_eq!(
                errors,
                vec![value(
                    r#"{"harness":"codex","adapter":"prompt","reason":"state.unavailable"}"#
                )]
            );
        }
        "bytes" => {
            let repo = world.repository("repo");
            let expected = br#"{"adapter": "prompt", "harness": "codex", "reason": "identity.unresolved", "schema": "agent-memory/hook-failure-v1", "stage": "identity", "timestamp": "2026-11-13T12:00:00+00:00"}"#;
            hook_failure::write(&repo, "codex", "prompt", Some(&metadata())).unwrap();
            let (directory, slot) = hook_failure::paths(&repo, "codex", "prompt").unwrap();
            assert_eq!(std::fs::read(directory.join(slot)).unwrap(), expected);
        }
        "overflow" | "deadline" | "object-size" => {
            let error = managed_source::payload(&world.0, &"0".repeat(40)).unwrap_err();
            let message = match name.to_str().unwrap() {
                "overflow" => "Legacy managed source: Git object exceeded its bound while reading",
                "object-size" => "Legacy managed source: Git object exceeds its bound",
                _ => "Legacy managed source: local Git ownership observation failed",
            };
            assert_eq!(error.to_string(), message);
            if name != "object-size" {
                let pid = std::fs::read_to_string(world.0.join("pid"))
                    .unwrap()
                    .parse::<i32>()
                    .unwrap();
                assert!(!process::pid_is_alive(pid).unwrap());
            }
        }
        "environment" => {
            let repo = world.repository("repo");
            write(&repo.join("source"), b"original\n");
            let revision = world.commit(&repo);
            let trap = world.0.join("trap");
            let marker = world.0.join("unsafe");
            write(
                &trap,
                format!("#!/bin/sh\nprintf unsafe > '{}'\n", marker.display()).as_bytes(),
            );
            chmod(&trap, 0o700);
            let attributes = world.0.join("attributes");
            write(&attributes, b"* filter=unexpected\n");
            world.git(&repo, &["config", "core.fsmonitor", trap.to_str().unwrap()]);
            world.git(
                &repo,
                &[
                    "config",
                    "core.attributesFile",
                    attributes.to_str().unwrap(),
                ],
            );
            world.git(
                &repo,
                &["config", "filter.unexpected.clean", trap.to_str().unwrap()],
            );
            let index = std::fs::read(repo.join(".git/index")).unwrap();
            assert_eq!(
                managed_source::verify_payload(&repo, &revision)
                    .unwrap()
                    .files,
                1
            );
            assert!(!marker.exists());
            assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
        }
        other => panic!("unknown fixture {other}"),
    }
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_independent_adapters_worktrees_private_bounds_and_readonly_absence
#[test]
fn independent_adapters_and_worktrees() {
    case("independent");
}
// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_unsafe_or_oversized_metadata_is_preserved_and_refused
#[test]
fn unsafe_or_oversized_metadata() {
    case("unsafe");
}
// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_malformed_fields_are_refused_and_missing_clear_writes_nothing
#[test]
fn malformed_fields_and_readonly_clear() {
    case("malformed");
}
// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_non_git_file_only_diagnostics_are_not_hook_failures
#[test]
fn file_only_has_no_hook_failures() {
    case("non-git");
}
// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_one_corrupt_adapter_does_not_hide_other_failure
#[test]
fn corrupt_adapter_does_not_hide_other() {
    case("corrupt");
}
#[test]
fn hook_failure_has_exact_sorted_ascii_bytes() {
    case("bytes");
}
// obligation: tests/contracts/test_managed_source_bounds.py::ManagedSourceBoundsContract.test_actual_git_stdout_overflow_is_bounded_and_owned_child_is_collected
#[test]
fn git_overflow_kills_and_reaps_owned_child() {
    case("overflow");
}
#[test]
fn git_deadline_is_thirty_seconds() {
    case("deadline");
}
#[test]
fn git_size_requires_ascii_unsigned_digits() {
    case("object-size");
}
// obligation: tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_local_ownership_does_not_execute_fsmonitor_or_filters_or_follow_ambient_git_directory
#[test]
fn git_environment_filters_and_fsmonitor_are_inert() {
    case("environment");
}

// obligation: tests/contracts/test_hook_failure.py::HookFailureContract.test_linked_worktree_rejects_state_in_main_checkout_memory
#[test]
fn linked_worktree_excludes_main_checkout_state() {
    case("linked-state");
}

// obligation: tests/contracts/test_managed_source_bounds.py::ManagedSourceBoundsContract.test_nonportable_executable_cannot_lose_owner_execution_but_keep_group_execution
#[test]
fn executable_intent_requires_owner_execution() {
    let world = World::new();
    let repo = world.repository("mode");
    write(&repo.join("runner.sh"), b"#!/bin/sh\nexit 0\n");
    chmod(&repo.join("runner.sh"), 0o755);
    let revision = world.commit(&repo);
    assert!(managed_source::verify_payload(&repo, &revision).is_ok());
    chmod(&repo.join("runner.sh"), 0o654);
    assert!(matches!(managed_source::verify_payload(&repo, &revision),
        Err(managed_source::Error::PayloadMismatch(message)) if message == "source bytes or executable intent differ: runner.sh"));
}

// obligation: tests/contracts/test_managed_source_bounds.py::ManagedSourceBoundsContract.test_replacement_refs_cannot_substitute_another_source_tree
#[test]
fn replacement_refs_do_not_change_recorded_payload() {
    let world = World::new();
    let repo = world.repository("replace");
    write(&repo.join("source.txt"), b"Original source.\n");
    let original = world.commit(&repo);
    write(&repo.join("source.txt"), b"Substituted source.\n");
    let substitute = world.commit(&repo);
    world.git(&repo, &["replace", &original, &substitute]);
    assert!(
        managed_source::verify_payload(&repo, &original)
            .unwrap_err()
            .to_string()
            .contains("source bytes or executable intent differ")
    );
}

// obligation: tests/contracts/test_managed_source_bounds.py::ManagedSourceBoundsContract.test_file_replacement_by_link_or_fifo_refuses_without_following_or_blocking
#[test]
fn descriptor_replacements_never_follow_or_block() {
    let world = World::new();
    let directory = Descriptor::open_directory(&world.0).unwrap();
    for kind in ["symlink", "fifo"] {
        let path = world.0.join(kind);
        write(&path, b"original");
        let observed = directory.stat_at(OsStr::new(kind)).unwrap();
        std::fs::remove_file(&path).unwrap();
        if kind == "symlink" {
            symlink(world.0.join("unrelated-missing-path"), &path).unwrap();
        } else {
            let argv = vec![OsString::from("/usr/bin/mkfifo"), path.into_os_string()];
            assert_eq!(run(&argv, &world.env(), 5).status, Status::Code(0));
        }
        assert!(managed_source::blob_identity(20, &directory, OsStr::new(kind), observed).is_err());
    }
}

#[test]
fn blob_hashes_are_in_process_and_file_sizes_are_bounded() {
    let world = World::new();
    let directory = Descriptor::open_directory(&world.0).unwrap();
    write(&world.0.join("hello"), b"hello\n");
    let observed = directory.stat_at(OsStr::new("hello")).unwrap();
    assert_eq!(
        managed_source::blob_identity(20, &directory, OsStr::new("hello"), observed).unwrap(),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
    let large = std::fs::File::create(world.0.join("large")).unwrap();
    large
        .set_len((managed_source::MAX_FILE + 1) as u64)
        .unwrap();
    let observed = directory.stat_at(OsStr::new("large")).unwrap();
    assert_eq!(
        managed_source::blob_identity(20, &directory, OsStr::new("large"), observed)
            .unwrap_err()
            .to_string(),
        "Legacy managed source: source file exceeds its bound"
    );
}

// obligation: tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_hidden_tracked_changes_extra_ignored_files_modes_and_channel_are_preserved_and_refused
#[test]
fn raw_payload_and_index_changes_refuse_without_mutation() {
    for case in [
        "hidden-by-index",
        "skip-worktree",
        "staged-only",
        "staged-deletion",
        "ignored-extra",
        "empty-directory",
        "executable",
        "forged-commit",
        "forged-tree",
        "alternate-objects",
    ] {
        let world = World::new();
        let repo = world.repository("source");
        let node = repo.join("source");
        write(&node, b"Original source.\n");
        let revision = world.commit(&repo);
        assert!(
            managed_source::verify_payload(&repo, &revision).is_ok(),
            "{case}"
        );
        match case {
            "hidden-by-index" | "skip-worktree" => {
                world.git(
                    &repo,
                    &[
                        "update-index",
                        if case == "hidden-by-index" {
                            "--assume-unchanged"
                        } else {
                            "--skip-worktree"
                        },
                        "source",
                    ],
                );
                write(&node, b"Changed source.\n");
            }
            "staged-only" => {
                write(&node, b"Staged source.\n");
                world.git(&repo, &["add", "source"]);
                write(&node, b"Original source.\n");
            }
            "staged-deletion" => {
                world.git(&repo, &["rm", "--cached", "source"]);
            }
            "ignored-extra" => {
                write(&repo.join(".git/info/exclude"), b"kept\n");
                write(&repo.join("kept"), b"User data must survive.\n");
            }
            "empty-directory" => {
                std::fs::create_dir(repo.join("kept")).unwrap();
            }
            "executable" => chmod(&node, 0o755),
            "alternate-objects" => write(
                &repo.join(".git/objects/info/alternates"),
                b"/unavailable/object/store\n",
            ),
            _ => {
                let original_oid = if case == "forged-commit" {
                    revision.clone()
                } else {
                    world.git(&repo, &["rev-parse", "HEAD^{tree}"])
                };
                write(&node, b"Different source.\n");
                let next = world.commit(&repo);
                let next_oid = if case == "forged-commit" {
                    next
                } else {
                    world.git(&repo, &["rev-parse", "HEAD^{tree}"])
                };
                let object_path = |oid: &str| {
                    repo.join(".git/objects")
                        .join(oid.get(..2).unwrap())
                        .join(oid.get(2..).unwrap())
                };
                let destination = object_path(&original_oid);
                chmod(&destination, 0o600);
                std::fs::copy(object_path(&next_oid), destination).unwrap();
                write(&node, b"Original source.\n");
            }
        }
        let before = std::fs::read(&node).unwrap();
        let index = std::fs::read(repo.join(".git/index")).unwrap();
        assert!(
            managed_source::verify_payload(&repo, &revision).is_err(),
            "{case}"
        );
        assert_eq!(std::fs::read(&node).unwrap(), before, "{case}");
        assert_eq!(
            std::fs::read(repo.join(".git/index")).unwrap(),
            index,
            "{case}"
        );
    }
}

#[test]
fn sha256_git_repositories_bind_actual_object_bytes() {
    let world = World::new();
    let repo = world.0.join("sha256");
    std::fs::create_dir(&repo).unwrap();
    world.git(&repo, &["init", "-b", "main", "--object-format=sha256"]);
    std::fs::create_dir(repo.join("nested")).unwrap();
    write(&repo.join("nested/source"), b"Original source.\n");
    let revision = world.commit(&repo);
    assert_eq!(revision.len(), 64);
    let observed = managed_source::verify_payload(&repo, &revision).unwrap();
    assert_eq!(observed.files, 1);
    assert_eq!(observed.bytes, 17);
}

// obligation: tests/contracts/test_managed_source_bounds.py::ManagedSourceBoundsContract.test_growth_during_actual_descriptor_read_is_capped
#[test]
fn growing_real_file_is_never_accepted_as_observed_blob() {
    let world = World::new();
    let path = world.0.join("source");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(managed_source::MAX_FILE as u64).unwrap();
    let directory = Descriptor::open_directory(&world.0).unwrap();
    let observed = directory.stat_at(OsStr::new("source")).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let ready = barrier.clone();
    let reader = std::thread::spawn(move || {
        ready.wait();
        managed_source::blob_identity(20, &directory, OsStr::new("source"), observed)
    });
    barrier.wait();
    // The actual descriptor is concurrently grown, not replaced by fake read
    // data. Either pre-open identity or the running/readback bound must veto it.
    file.set_len((managed_source::MAX_FILE + 1) as u64).unwrap();
    assert!(reader.join().unwrap().is_err());
    assert_eq!(
        std::fs::metadata(path).unwrap().len(),
        (managed_source::MAX_FILE + 1) as u64
    );
}

#[test]
fn hook_timestamps_accept_only_writer_reachable_utc_forms() {
    for timestamp in [
        "0001-01-01T00:00:00+00:00",
        "9999-12-31T23:59:59.999999+00:00",
        "2024-02-29T01:02:03.000001+00:00",
        "2026-11-13T12:00:00+00:00",
    ] {
        let mut record = metadata();
        *record.get_mut("timestamp").unwrap() = Value::String(timestamp.into());
        assert!(
            hook_failure::validate(&record, "codex", "prompt").is_ok(),
            "{timestamp}"
        );
    }
    for timestamp in [
        "0000-01-01T00:00:00+00:00",
        "2025-02-29T00:00:00+00:00",
        "2026-11-13T24:00:00+00:00",
        "2026-11-13T23:59:60+00:00",
        "2026-11-13T12:00:00Z",
        "2026-11-13T12:00:00.000000+00:00",
        "2026-11-13T12:00:00.1+00:00",
        "2026-11-13T12:00:00.1234567+00:00",
        "2026-11-13 12:00:00+00:00",
        "20261113T120000+0000",
        "2026-W46-5T12:00:00+00:00",
        "2026-11-13T12+00:00",
        "2026-11-13T12:00:00-00:00",
        "2026-11-13T12:00:00+01:00",
        "2026-11-13T12:00:00+00:00:00",
        "2026-11-13💡12:00:00+00:00",
        "éééééééééééé",
        "2026-11-13T12:00:00,123456+00:00",
    ] {
        let mut record = metadata();
        *record.get_mut("timestamp").unwrap() = Value::String(timestamp.into());
        assert_eq!(
            hook_failure::validate(&record, "codex", "prompt")
                .unwrap_err()
                .to_string(),
            "Invalid hook failure timestamp",
            "{timestamp}"
        );
    }
}

#[test]
fn contract_and_unexpected_completion_failures_remain_distinct() {
    use agent_memory::error::ContractError;
    let refusal = ContractError::from(turn_check::CompletionError::Contract(ContractError::new(
        "Reviewed refusal",
    )));
    let unexpected = ContractError::from(turn_check::CompletionError::Unexpected);
    assert_eq!(refusal.message(), "Reviewed refusal");
    assert_eq!(
        unexpected.message(),
        "Original source or private assessment state is unavailable."
    );
    let synchronized =
        value(r#"{"sharing_state":"bank-synchronized","remote_freshness_verified":true}"#);
    let pending =
        value(r#"{"verified":false,"reason":"Reviewed refusal","repair":"Preserve evidence."}"#);
    let result = turn_check::combined(synchronized.clone(), pending);
    assert_eq!(turn_check::exit_code(&result), 1);
    assert_eq!(
        turn_check::emit(&result, true).unwrap(),
        (
            1,
            String::new(),
            "am: source assessment pending: Reviewed refusal Preserve evidence.\n".into()
        )
    );
    let complete = turn_check::combined(synchronized, value(r#"{"verified":true}"#));
    assert_eq!(
        turn_check::emit(&complete, true).unwrap(),
        (0, String::new(), String::new())
    );
    assert_eq!(turn_check::exit_code(&value(r#"{"verified":0}"#)), 0);
    assert_eq!(turn_check::exit_code(&value(r#"{"verified":false}"#)), 1);
    assert_eq!(
        turn_check::exit_code(&value(
            r#"{"schema":"agent-memory/turn-check-v1","verified":1}"#
        )),
        1
    );
    let refresh = value(
        r#"{"sharing_state":"pending","failure_component":"sharing","failure_code":"review","reason":"Review\nsource.","repair":"Preserve work."}"#,
    );
    let result = turn_check::combined(refresh, value(r#"{"verified":false}"#));
    assert_eq!(
        turn_check::failure_notice(&result).unwrap(),
        "am: memory refresh pending [sharing/review]: Review source. Preserve work.; source assessment pending: Original evidence is unavailable. Preserve evidence and inspect the matching boundary operation."
    );
}

#[test]
fn compact_request_refusal_order_and_json_numeric_equality_match_baseline() {
    let malformed = value(
        r#"{"schema":"agent-memory/turn-check-request-v1","assessments":[{"observation_id":"one"},{"observation_id":"one"},{}]}"#,
    );
    assert_eq!(
        turn_check::requests(&malformed).unwrap_err().message(),
        "A batch requires distinct local observation IDs"
    );
    let same = value(
        r#"{"schema":"agent-memory/turn-check-request-v1","assessments":[{"observation_id":"one","outcome":"no-learning","session":true,"prompt":1},{"observation_id":"two","outcome":"no-learning","session":1,"prompt":1.0}]}"#,
    );
    assert_eq!(turn_check::requests(&same).unwrap().len(), 2);
    assert!(hooks::entry("\u{1c} \u{1f}").is_err());
}

// obligation: tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_known_dispute_survives_rewording_changed_framing_and_registry_recovery
#[test]
fn known_dispute_maps_rewordings_moves_and_exact_version_corrections() {
    let rows = [
        ("base", r#"{"path":"old.md","component":"record","target_path":null,"sides":["base"],"metadata":{"id":"claim","version":1}}"#),
        ("reworded", r#"{"path":"claim.md","component":"record","target_path":null,"sides":["local"],"metadata":{"id":"claim","version":2}}"#),
        ("moved", r#"{"path":"moved.md","component":"record","target_path":null,"sides":["landed"],"metadata":{"id":"claim","version":3}}"#),
        ("correction", r#"{"path":"correction.md","component":"record","target_path":null,"sides":["local"],"metadata":{"id":"correction","version":1,"corrects":[{"id":"claim","version":1}]}}"#),
        ("wrong-version", r#"{"path":"wrong.md","component":"record","target_path":null,"sides":["landed"],"metadata":{"id":"other","version":1,"supersedes":[{"id":"claim","version":9}]}}"#),
        ("witness", r#"{"path":"deleted.md","component":"record","target_path":null,"sides":["landed"],"metadata":{},"witness_metadata":{"id":"deleted","version":1}}"#),
    ].into_iter().map(|(key, row)| (key.into(), value(row))).collect::<mapping::Rows>();
    assert_eq!(mapping::current_rows(&rows).unwrap().len(), 5);
    let index = mapping::source_index(&rows).unwrap();
    assert_eq!(
        mapping::successors(&value(r#"{"record":{"id":"claim","version":1}}"#), &index).unwrap(),
        vec!["correction", "moved", "reworded"]
    );
    assert_eq!(
        mapping::successors(
            &value(r#"{"path":"claim.md","component":"record","target_path":null}"#),
            &index
        )
        .unwrap(),
        vec!["reworded"]
    );
    assert_eq!(
        mapping::successors(&value(r#"{"record":{"id":"deleted","version":1}}"#), &index).unwrap(),
        vec!["witness"]
    );
    assert!(
        mapping::successors(&value(r#"{"record":{"id":"absent","version":1}}"#), &index)
            .unwrap()
            .is_empty()
    );
    assert!(!mapping::cached(&value(r#"{"resolution":null}"#), &rows, &BTreeSet::new()).unwrap());
    assert!(mapping::current_rows(&[("bad".into(), Value::Null)].into_iter().collect()).is_err());
}

#[test]
fn package_generation_refuses_missing_artifact_instead_of_inventing_identity() {
    let world = World::new();
    assert!(
        agent_memory::package::engine_generation(
            &world.0.join("missing-pack"),
            &world.0.join("missing-am")
        )
        .is_err()
    );
}

#[test]
fn shell_quote_matches_python_era_wrapper_bytes() {
    for (input, expected) in [
        ("", "''"),
        ("AZaz09@%+=:,./_-", "AZaz09@%+=:,./_-"),
        ("a b", "'a b'"),
        ("a'b", "'a'\"'\"'b'"),
        ("'", "''\"'\"''"),
        ("a\"b", "'a\"b'"),
        ("é", "'é'"),
        ("line\nbreak", "'line\nbreak'"),
        ("$HOME;*[]", "'$HOME;*[]'"),
    ] {
        assert_eq!(hooks::shell_quote(input), expected, "{input:?}");
    }
}

// obligation: tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_clean_index_refresh_previews_updates_and_repeats_preserving_old_source_and_bank
#[test]
fn clean_index_refresh_does_not_change_payload_ownership() {
    let world = World::new();
    let repo = world.repository("source");
    write(&repo.join("source"), b"Original source.\n");
    let revision = world.commit(&repo);
    let expected = managed_source::verify_payload(&repo, &revision).unwrap();
    std::fs::File::open(repo.join("source"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let old_index = std::fs::read(repo.join(".git/index")).unwrap();
    assert_eq!(
        world.git(&repo, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert_ne!(std::fs::read(repo.join(".git/index")).unwrap(), old_index);
    assert_eq!(
        managed_source::verify_payload(&repo, &revision).unwrap(),
        expected
    );
}

// obligation: tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_hidden_tracked_changes_extra_ignored_files_modes_and_channel_are_preserved_and_refused
#[test]
fn changed_channel_refuses_before_installation_callback() {
    for change in ["origin", "head"] {
        let world = World::new();
        let repo = world.repository("source");
        write(&repo.join("source"), b"Original source.\n");
        let revision = world.commit(&repo);
        world.git(&repo, &["remote", "add", "origin", "/recorded/channel"]);
        let source = Value::Object(vec![
            ("kind".into(), Value::String("managed".into())),
            ("path".into(), Value::String(repo.to_str().unwrap().into())),
            ("remote".into(), Value::String("/recorded/channel".into())),
            ("ref".into(), Value::String("main".into())),
            ("revision".into(), Value::String(revision)),
        ]);
        if change == "origin" {
            world.git(
                &repo,
                &["remote", "set-url", "origin", "/different/channel"],
            );
        } else {
            world.git(&repo, &["checkout", "--detach"]);
        }
        let index = std::fs::read(repo.join(".git/index")).unwrap();
        let message = managed_source::verify(&repo, &source)
            .unwrap_err()
            .to_string();
        assert_eq!(
            message,
            if change == "origin" {
                "Legacy managed source: recorded channel differs"
            } else {
                "Legacy managed source: local Git ownership observation failed"
            }
        );
        assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read(repo.join("source")).unwrap(),
            b"Original source.\n"
        );
    }
}
