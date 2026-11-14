mod support;
use agent_memory::process::Status;
use std::ffi::OsString;
use std::os::unix::fs::symlink;
use std::time::Duration;
use support::{RunOptions, World, require_world};

// obligation: tests/contracts/test_harness.py::HarnessContract.test_worker_is_isolated_and_credentials_are_absent
#[test]
fn child_environment_has_private_roots_and_no_credentials() {
    let world = World::new().unwrap();
    let root = require_world(&world.env, &world.paths["repo"]).unwrap();
    assert!(world.paths["home"].starts_with(&root));
    assert!(world.paths["repo"].starts_with(&root));
    for key in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "SSH_AUTH_SOCK",
        "AWS_ACCESS_KEY_ID",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_CONFIG_COUNT",
        "PYTHONPATH",
    ] {
        assert!(!world.env.contains_key(std::ffi::OsStr::new(key)), "{key}");
    }
    let output = world.run(["/usr/bin/env"]).unwrap();
    let environment = String::from_utf8(output.stdout).unwrap();
    assert!(
        environment
            .lines()
            .any(|line| line == format!("HOME={}", world.paths["home"].display()))
    );
    assert!(!environment.contains("GH_TOKEN="));
    assert!(!environment.contains("SSH_AUTH_SOCK="));
}

// obligation: tests/contracts/test_harness.py::HarnessContract.test_public_command_runs_with_spaces_and_clean_environment
#[test]
fn public_command_with_spaces_gets_only_world_environment() {
    let world = World::new().unwrap();
    let executable = world.root.join("command with spaces");
    symlink("/usr/bin/env", &executable).unwrap();
    let output = world.run([&executable]).unwrap();
    let env = String::from_utf8(output.stdout).unwrap();
    assert!(
        env.lines()
            .any(|line| line == format!("HOME={}", world.paths["home"].display()))
    );
    assert!(
        world
            .paths
            .values()
            .all(|path| path.starts_with(&world.root))
    );
}

// obligation: tests/contracts/test_harness.py::HarnessContract.test_real_git_bare_remote_clones_and_worktree
#[test]
fn real_git_remote_clones_worktree_and_network_refusal() {
    let world = World::new().unwrap();
    let remote = world.repository("remote.git", true).unwrap();
    let a = world.clone(&remote, "clone A").unwrap();
    std::fs::write(a.join("lesson.md"), b"Durable lesson\n").unwrap();
    let options = || RunOptions {
        cwd: Some(&a),
        ..Default::default()
    };
    world.git_with(["add", "lesson.md"], options()).unwrap();
    world
        .git_with(["commit", "-m", "Learn a lesson"], options())
        .unwrap();
    world
        .git_with(["push", "origin", "main"], options())
        .unwrap();
    let b = world.clone(&remote, "clone B").unwrap();
    assert_eq!(
        std::fs::read(b.join("lesson.md")).unwrap(),
        b"Durable lesson\n"
    );
    let worktree = world.root.join("other session");
    world
        .git_with(
            [
                "worktree",
                "add",
                "-b",
                "session-two",
                worktree.to_str().unwrap(),
            ],
            RunOptions {
                cwd: Some(&b),
                ..Default::default()
            },
        )
        .unwrap();
    let branch = world
        .git_with(
            ["branch", "--show-current"],
            RunOptions {
                cwd: Some(&worktree),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(branch.stdout, b"session-two\n");
    let denied = world
        .git_with(
            ["ls-remote", "https://example.invalid/repo"],
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(denied.status, Status::Code(0));
    assert!(String::from_utf8_lossy(&denied.stderr).contains("transport 'https' not allowed"));
}

// obligation: tests/contracts/test_harness.py::HarnessContract.test_escape_and_live_root_overrides_are_refused
#[test]
fn escape_paths_and_live_environment_overrides_refuse() {
    let world = World::new().unwrap();
    for path in ["../outside", "/", "/Users"] {
        assert!(world.inside(path).is_err());
    }
    symlink("/", world.root.join("escape")).unwrap();
    assert!(
        world
            .run_with(
                ["/usr/bin/true"],
                RunOptions {
                    cwd: Some(std::path::Path::new("escape")),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(world.run(Vec::<String>::new()).is_err());
    let overrides = [(OsString::from("HOME"), OsString::from("/"))];
    assert!(
        world
            .run_with(
                ["/usr/bin/true"],
                RunOptions {
                    env: &overrides,
                    ..Default::default()
                }
            )
            .is_err()
    );
    let mut altered = world.env.clone();
    altered.insert("HOME".into(), "/".into());
    assert!(require_world(&altered, &world.paths["repo"]).is_err());
}

// obligation: tests/contracts/test_harness.py::HarnessContract.test_command_failure_and_timeout_are_not_success
#[test]
fn command_failure_and_timeout_are_never_success() {
    let world = World::new().unwrap();
    let command = world.root.join("exit seven");
    std::fs::write(&command, b"#!/bin/sh\nexit 7\n").unwrap();
    support::lifecycle::mode(&command, 0o700);
    let output = world
        .run_with(
            [&command],
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(7));
    assert!(world.run([&command]).is_err());
    let error = world
        .run_with(
            ["/bin/sleep", "10"],
            RunOptions {
                timeout: Duration::from_millis(50),
                ..Default::default()
            },
        )
        .unwrap_err();
    match error {
        support::WorldError::Process(error) => {
            assert_eq!(error.reason, agent_memory::process::Refusal::Deadline);
            let evidence = error.evidence.as_ref().unwrap();
            assert!(evidence.killed);
            assert!(evidence.reaped);
        }
        other => panic!("expected owned-child deadline, got {other}"),
    }
}

// obligation: tests/contracts/test_harness.py::HarnessContract.test_world_cleanup_does_not_follow_escape_symlink
#[test]
fn cleanup_does_not_follow_escape_symlink() {
    let outer = World::new().unwrap();
    let sentinel = outer.root.join("sentinel");
    std::fs::write(&sentinel, b"keep").unwrap();
    let removed = {
        let inner = World::new().unwrap();
        symlink(&sentinel, inner.root.join("escape")).unwrap();
        inner.root.clone()
    };
    assert!(!removed.exists());
    assert_eq!(std::fs::read(sentinel).unwrap(), b"keep");
}
