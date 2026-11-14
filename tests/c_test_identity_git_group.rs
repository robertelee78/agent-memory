#![cfg(feature = "dev-tools")]
mod support;

use agent_memory::clock::Deadline;
use agent_memory::identity_git;
use agent_memory::process::{self, Group, Input, Refusal, Spec, Status, Target};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use support::World;

fn args(body: &str) -> Vec<OsString> {
    vec!["/bin/sh".into(), "-c".into(), body.into()]
}

fn spec<'a>(argv: &'a [OsString], root: &'a Path) -> Spec<'a> {
    Spec {
        argv,
        env: &[],
        cwd: Some(root),
        search_path: false,
        stdin: Input::Null,
        stdout: Target::Capture { limit: 1024 * 1024 },
        stderr: Target::Capture { limit: 1024 * 1024 },
        deadline: Deadline::from_absolute_ns(0),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    }
}

fn reaped(pid: u32) {
    let mut status = 0;
    // The PID came from the real owned child; only collect that exact child.
    assert_eq!(
        unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    assert!(!process::pid_is_alive(pid as i32).unwrap());
}

fn wait_pid(path: &Path) -> u32 {
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    loop {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(pid) = raw.trim().parse() {
                return pid;
            }
        }
        assert!(!deadline.expired(), "child failed to announce itself");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn closed_pipe_observation(quiet_sibling: bool) {
    let world = World::new().unwrap();
    let root = world.inside("repo").unwrap();
    let first =
        args("printf '%s\\n' $$ > first.pid; exec 1>&- 2>&-; while [ ! -e release ]; do :; done");
    let second = args("printf '%s\\n' $$ > second.pid; exec /bin/sleep 30");
    let mut requests = vec![spec(&first, &root)];
    if quiet_sibling {
        requests.push(spec(&second, &root));
    }
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    group
        .set_exit_poll_interval(Duration::from_millis(50))
        .unwrap();
    let pid = wait_pid(&root.join("first.pid"));
    let sibling = quiet_sibling.then(|| wait_pid(&root.join("second.pid")));
    let release = root.join("release");
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        fs::write(release, b"release").unwrap();
    });
    let start = Instant::now();
    let output = group.value(0).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(output.poll_wakeups <= 20, "closed pipes must not busy-spin");
    assert!(start.elapsed() < Duration::from_secs(2));
    reaped(pid);
    if quiet_sibling {
        let error = group.finish().unwrap_err();
        assert!(error.incomplete);
        reaped(sibling.unwrap());
    } else {
        group.finish().unwrap();
    }
    releaser.join().unwrap();
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b1_closed_pipe_child_is_observed_before_quiet_sibling_deadline
fn b1_closed_pipe_child_is_observed_before_quiet_sibling_deadline() {
    closed_pipe_observation(true);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b1_no_readable_fds_uses_bounded_owner_wait_then_observes_exit
fn b1_no_readable_fds_uses_bounded_owner_wait_then_observes_exit() {
    closed_pipe_observation(false);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_descendant_held_pipes_timeout_without_waiting_for_descendant_eof
fn descendant_held_pipes_timeout_without_waiting_for_descendant_eof() {
    let world = World::new().unwrap();
    let root = world.inside("repo").unwrap();
    let argv = args(
        "(printf '%s\\n' \"$!\" > unused; while [ ! -e release ]; do :; done; printf released > descendant-released) & printf '%s\\n' $! > descendant.pid; exit 0",
    );
    let requests = [spec(&argv, &root)];
    let mut group = Group::launch(&requests, Duration::from_millis(300)).unwrap();
    let descendant = wait_pid(&root.join("descendant.pid"));
    let start = Instant::now();
    let error = group.value(0).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.as_ref().unwrap();
    assert!(evidence.reaped);
    assert!(
        !evidence.killed,
        "the exited direct child must not be signalled"
    );
    reaped(evidence.pid);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(process::pid_is_alive(descendant as i32).unwrap());
    group.finish().unwrap();
    fs::write(root.join("release"), b"release").unwrap();
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    while !root.join("descendant-released").exists() {
        assert!(!deadline.expired());
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_each_deadline_starts_at_own_popen_return_not_first_launch
fn each_deadline_starts_at_own_launch_return() {
    let world = World::new().unwrap();
    let root = world.inside("repo").unwrap();
    let first = args("printf first");
    let second = args("printf second");
    // Absolute deadlines are already expired: Group must assign a fresh interval
    // separately at each child's launch, not inherit either Spec deadline.
    let requests = [spec(&first, &root), spec(&second, &root)];
    let mut group = Group::launch(&requests, Duration::from_secs(2)).unwrap();
    for (index, expected) in [b"first".as_slice(), b"second"].into_iter().enumerate() {
        let output = group.value(index).unwrap();
        assert_eq!(output.stdout, expected);
        assert_eq!(output.status, Status::Code(0));
        reaped(output.pid);
    }
    group.finish().unwrap();
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_later_timeout_does_not_cancel_earlier_pending_success
fn later_timeout_does_not_cancel_earlier_success() {
    let world = World::new().unwrap();
    let root = world.inside("repo").unwrap();
    let first = args("printf first");
    let second = args("printf partial; exec /bin/sleep 30");
    let requests = [spec(&first, &root), spec(&second, &root)];
    let mut group = Group::launch(&requests, Duration::from_millis(300)).unwrap();
    let output = group.value(0).unwrap();
    assert_eq!(output.stdout, b"first");
    reaped(output.pid);
    let error = group.value(1).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert_eq!(evidence.stdout, b"partial");
    assert!(evidence.killed && evidence.reaped);
    reaped(evidence.pid);
    group.finish().unwrap();
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_earlier_parser_refusal_survives_later_command_and_cleanup_faults
fn earlier_parser_refusal_survives_later_command_failure() {
    infrastructure_case("parser");
}

fn interrupt_case(stage: &str, twice: bool) {
    let world = World::new().unwrap();
    let root = world.inside("repo").unwrap();
    let env = [
        (
            "AGENT_MEMORY_FAULT_INTERRUPT_ROOT".into(),
            root.into_os_string(),
        ),
        ("AGENT_MEMORY_FAULT_INTERRUPT_STAGE".into(), stage.into()),
        (
            "AGENT_MEMORY_FAULT_INTERRUPT_TWICE".into(),
            if twice { "yes" } else { "no" }.into(),
        ),
    ];
    let output = world
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "interrupt_worker".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            support::RunOptions {
                env: &env,
                timeout: Duration::from_secs(10),
                ..support::RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(0), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn interrupt_worker() {
    let Some(root) = std::env::var_os("AGENT_MEMORY_FAULT_INTERRUPT_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    process::install_termination_handler().unwrap();
    process::set_cancellation_signal(libc::SIGTERM).unwrap();
    let before = descriptors();
    let stage = std::env::var("AGENT_MEMORY_FAULT_INTERRUPT_STAGE").unwrap();
    let twice = std::env::var("AGENT_MEMORY_FAULT_INTERRUPT_TWICE").unwrap() == "yes";
    let argv: Vec<_> = (0..3).map(|index| args(&format!(
        "trap 'printf x >> cancel-{index}' TERM; printf '%s\\n' $$ > child-{index}; while [ ! -e rescue ]; do :; done"
    ))).collect();
    // This process is deliberately outside the supervisor's ownership registry.
    let mut foreign = std::process::Command::new("/bin/sleep")
        .arg("5")
        .spawn()
        .unwrap();
    let thread = unsafe { libc::pthread_self() } as usize;
    let signal_root = root.clone();
    let signal_stage = stage.clone();
    let signaller = std::thread::spawn(move || {
        if signal_stage == "launch" {
            wait_file(&signal_root.join("pause/identity_git.selectors.register.ready"));
            wait_pid(&signal_root.join("child-0"));
            wait_pid(&signal_root.join("child-1"));
        } else {
            for index in 0..3 {
                wait_pid(&signal_root.join(format!("child-{index}")));
            }
        }
        if signal_stage == "ordinary" || signal_stage == "timeout" {
            // A child's handler is evidence that cancellation already happened:
            // the owner is now in the ordinary/timeout reap, not in polling.
            wait_file(&signal_root.join("cancel-0"));
        } else if signal_stage == "parse" {
            wait_file(&signal_root.join("parsing"));
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            unsafe { libc::pthread_kill(thread as libc::pthread_t, libc::SIGINT) },
            0
        );
        if twice {
            // TERM handlers keep every child alive throughout the reap window.
            // The second real SIGINT therefore necessarily interrupts that window.
            std::thread::sleep(Duration::from_millis(40));
            assert_eq!(
                unsafe { libc::pthread_kill(thread as libc::pthread_t, libc::SIGINT) },
                0
            );
        }
        if signal_stage == "launch" {
            fs::write(
                signal_root.join("pause/identity_git.selectors.register.release"),
                b"release",
            )
            .unwrap();
        }
    });
    if stage == "launch" {
        // The hook is token-bound to this World and is inert in shipped builds.
        unsafe {
            std::env::set_var(
                "AGENT_MEMORY_FAULT_STAGE",
                "identity_git.selectors.register",
            );
            std::env::set_var("AGENT_MEMORY_FAULT_MODE", "pause");
            std::env::set_var("AGENT_MEMORY_FAULT_DIR", root.join("pause"));
        }
    }
    let requests: Vec<_> = argv.iter().map(|argv| spec(argv, &root)).collect();
    let mut group = Group::launch(
        &requests,
        Duration::from_millis(if stage == "timeout" { 100 } else { 5000 }),
    )
    .unwrap();
    if stage != "launch" {
        for index in 0..3 {
            wait_pid(&root.join(format!("child-{index}")));
        }
    }
    let start = Instant::now();
    if stage == "parse" {
        fs::write(root.join("parsing"), b"parser entered").unwrap();
        while !process::interrupted() {
            std::thread::yield_now();
        }
    }
    if stage == "ordinary" {
        assert!(group.finish().unwrap_err().incomplete);
    } else {
        for index in 0..3 {
            let error = group.value(index).unwrap_err();
            assert_eq!(error.reason, Refusal::Interrupted, "{error}");
        }
        group.finish().unwrap();
    }
    let elapsed = start.elapsed();
    if stage != "launch" {
        assert!(elapsed < Duration::from_millis(700), "{stage}: {elapsed:?}");
        if !twice {
            assert!(elapsed >= Duration::from_millis(240), "{elapsed:?}");
        } else {
            assert!(elapsed < Duration::from_millis(240), "{elapsed:?}");
        }
    }
    signaller.join().unwrap();
    let count = if stage == "launch" { 2 } else { 3 };
    let note = process::interrupt_cleanup_note().unwrap();
    assert!(
        note.contains(&format!("{count} killed-but-unreaped")),
        "{note}"
    );
    assert!(note.contains("no further signals"), "{note}");
    assert_eq!(descriptors(), before, "owned capture descriptors leaked");
    assert!(
        foreign.try_wait().unwrap().is_none(),
        "foreign child was cancelled"
    );
    for index in 0..count {
        let path = root.join(format!("cancel-{index}"));
        wait_file(&path);
        assert_eq!(
            fs::read(path).unwrap(),
            b"x",
            "handed-off child was signalled twice"
        );
        assert!(
            process::pid_is_alive(wait_pid(&root.join(format!("child-{index}"))) as i32).unwrap()
        );
    }
    // Rescue is the fixture owner's action, after all ownership assertions.
    fs::write(root.join("rescue"), b"rescue").unwrap();
    for index in 0..count {
        let pid = wait_pid(&root.join(format!("child-{index}")));
        let mut status = 0;
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, &mut status, 0) },
            pid as i32
        );
        reaped(pid);
    }
    foreign.kill().unwrap();
    foreign.wait().unwrap();
}

fn wait_file(path: &Path) {
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    while !path.exists() {
        assert!(
            !deadline.expired(),
            "missing stage rendezvous {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count
fn b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count() {
    interrupt_case("poll", false);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup
fn b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup() {
    interrupt_case("poll", true);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_second_interrupt_during_reap_ends_all_remaining_waits
fn b2_second_interrupt_during_reap_ends_all_remaining_waits() {
    interrupt_case("poll", true);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace
fn interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace() {
    interrupt_case("ordinary", false);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff
fn b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff() {
    interrupt_case("timeout", false);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_interrupts_during_launch_select_and_parsing_preserve_primary_and_cleanup
fn interrupts_during_launch_select_and_parsing_preserve_primary_and_cleanup() {
    for stage in ["launch", "poll", "parse"] {
        interrupt_case(stage, false);
    }
}

fn infrastructure_case(mode: &str) {
    let world = World::new().unwrap();
    let root = world.inside("repo").unwrap();
    let env = [
        ("AGENT_MEMORY_FAULT_IDENTITY_INFRA".into(), mode.into()),
        (
            "AGENT_MEMORY_FAULT_IDENTITY_ROOT".into(),
            root.into_os_string(),
        ),
    ];
    let output = world
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "infrastructure_worker".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            support::RunOptions {
                env: &env,
                ..support::RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(0));
}

fn descriptors() -> std::collections::BTreeSet<i32> {
    (3..1024)
        .filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
        .collect()
}

#[test]
fn infrastructure_worker() {
    let Some(mode) = std::env::var_os("AGENT_MEMORY_FAULT_IDENTITY_INFRA") else {
        return;
    };
    let root =
        std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_FAULT_IDENTITY_ROOT").unwrap());
    if mode == "ordinary-refusal"
        || mode == "system-exit"
        || mode == "parser"
        || mode == "registration"
        || mode == "memory"
        || mode == "runtime"
    {
        staged_infrastructure(&root, mode.to_str().unwrap());
        return;
    }
    let before = descriptors();
    let missing = vec![root.join("missing-owned-git").into_os_string()];
    let first = args("printf '%s\\n' $$ > first.pid; exec /bin/sleep 30");
    let second = args("printf '%s\\n' $$ > second.pid; exec /bin/sleep 30");
    let precedence = mode == "precedence" || mode == "optional-precedence";
    let requests = [
        spec(if precedence { &missing } else { &first }, &root),
        spec(&second, &root),
    ];
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    let second_pid = wait_pid(&root.join("second.pid"));
    let first_pid = (!precedence).then(|| wait_pid(&root.join("first.pid")));
    let opened = descriptors();
    let fd = *opened.difference(&before).next().unwrap();
    // A real EBADF on an owned capture pipe, not a substituted selector.
    assert_eq!(unsafe { libc::close(fd) }, 0);
    if precedence {
        let output = identity_git::_git_output(group.value(0), mode == "optional-precedence");
        if mode == "optional-precedence" {
            assert!(output.unwrap().is_none());
        } else {
            assert!(matches!(
                output.unwrap_err(),
                identity_git::Error::Contract(_)
            ));
        }
        let error = group.value(1).unwrap_err();
        assert_eq!(error.reason, Refusal::Infrastructure);
        assert!(identity_git::_git_output(Err(error), true).is_err());
    } else {
        let error = group.value(0).unwrap_err();
        assert_eq!(error.reason, Refusal::Infrastructure);
        assert!(identity_git::_git_output(Err(error), true).is_err());
        let error = group.value(1).unwrap_err();
        assert_eq!(error.reason, Refusal::Infrastructure);
    }
    let cleanup = group.finish().unwrap_err();
    assert_eq!(cleanup.reason, Refusal::Infrastructure);
    if let Some(pid) = first_pid {
        reaped(pid);
    }
    reaped(second_pid);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_selector_and_fd_infrastructure_never_become_optional_absence
fn real_capture_fd_failure_never_becomes_optional_absence() {
    infrastructure_case("capture");
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_terminal_earlier_command_precedence_and_optional_progress_to_fatal_barrier
fn terminal_earlier_command_precedes_later_real_infrastructure_fault() {
    infrastructure_case("precedence");
    infrastructure_case("optional-precedence");
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_cleanup_faults_attempt_remaining_real_kills_closes_and_reaps
fn cleanup_real_fd_fault_still_finishes_other_children() {
    infrastructure_case("capture");
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_ordinary_and_system_exit_keep_unbounded_os_wait_parity
fn ordinary_refusal_and_nonzero_exit_finish_all_owned_children() {
    for stage in ["ordinary-refusal", "system-exit"] {
        infrastructure_case(stage);
    }
}

fn staged_infrastructure(root: &Path, mode: &str) {
    if mode == "parser" {
        let repo = root.join("parser-repo");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .arg(&repo)
                .status()
                .unwrap()
                .success()
        );
        let before = descriptors();
        let commands = [
            (vec!["rev-parse".into(), "--show-toplevel".into()], false),
            (
                vec![
                    "-c".into(),
                    "alias.late=!printf '%s\\n' $$ > late.pid; exec /bin/sleep 5".into(),
                    "late".into(),
                ],
                false,
            ),
            (vec!["not-a-real-command".into()], false),
        ];
        let env = std::env::vars_os().collect::<Vec<_>>();
        let error =
            identity_git::git_group_with(&repo, &commands, &env, Duration::from_secs(3), |group| {
                assert!(group.value(0, false)?.is_some());
                wait_pid(&repo.join("late.pid"));
                let fd = descriptors().difference(&before).copied().next().unwrap();
                assert_eq!(unsafe { libc::close(fd) }, 0);
                Err::<(), _>(identity_git::Error::Contract(
                    agent_memory::error::ContractError::new("selected parser refusal"),
                ))
            })
            .unwrap_err();
        assert_eq!(
            error.contract().unwrap().to_string(),
            "selected parser refusal"
        );
        assert!(matches!(error, identity_git::Error::Cleanup { .. }));
        assert_eq!(descriptors(), before);
        return;
    }
    let ordinary = mode == "ordinary-refusal" || mode == "system-exit";
    if ordinary {
        process::set_cancellation_signal(libc::SIGTERM).unwrap();
    }
    let before = descriptors();
    let argv: Vec<_> = (0..3).map(|index| {
        if mode == "system-exit" && index == 0 {
            args("printf '%s\\n' $$ > staged-0; exit 7")
        } else {
            args(&format!("trap 'printf x > cancelled-{index}' TERM; printf '%s\\n' $$ > staged-{index}; while [ ! -e release ]; do :; done"))
        }
    }).collect();
    let stage = match mode {
        "registration" => Some("identity_git.selectors.register"),
        "memory" => Some("identity_git.selectors.select.MemoryError"),
        "runtime" => Some("identity_git.selectors.select.RuntimeError"),
        _ => None,
    };
    if let Some(stage) = stage {
        unsafe {
            std::env::set_var("AGENT_MEMORY_FAULT_STAGE", stage);
            std::env::set_var("AGENT_MEMORY_FAULT_MODE", "error");
        }
    }
    let specs: Vec<_> = argv.iter().map(|argv| spec(argv, root)).collect();
    let mut group = Group::launch(&specs, Duration::from_secs(5)).unwrap();
    if ordinary {
        for index in 0..3 {
            wait_pid(&root.join(format!("staged-{index}")));
        }
        let primary = if mode == "system-exit" {
            let output = group.value(0).unwrap();
            assert_eq!(output.status, Status::Code(7));
            Some(identity_git::_git_output(Ok(output), false).unwrap_err())
        } else {
            None
        };
        let cancelled = if primary.is_some() {
            "cancelled-1"
        } else {
            "cancelled-0"
        };
        let directory = root.to_path_buf();
        let release = std::thread::spawn(move || {
            wait_file(&directory.join(cancelled));
            std::thread::sleep(Duration::from_millis(350));
            fs::write(directory.join("release"), b"release").unwrap();
        });
        let start = Instant::now();
        let cleanup = group.finish().unwrap_err();
        assert!(cleanup.incomplete);
        assert!(start.elapsed() >= Duration::from_millis(340));
        assert!(start.elapsed() < Duration::from_secs(2));
        release.join().unwrap();
        for index in 0..3 {
            reaped(wait_pid(&root.join(format!("staged-{index}"))));
        }
        if let Some(primary) = primary {
            assert!(primary.contract().is_some());
        }
    } else {
        let mut pids = Vec::new();
        for index in 0..3 {
            let error = group.value(index).unwrap_err();
            assert_eq!(error.reason, Refusal::Infrastructure, "{error}");
            if let Some(evidence) = &error.evidence {
                pids.push(evidence.pid);
            }
            assert!(identity_git::_git_output(Err(error), true).is_err());
        }
        assert!(group.finish().is_err());
        assert_eq!(pids.len(), if mode == "registration" { 2 } else { 3 });
        for pid in pids {
            reaped(pid);
        }
    }
    assert_eq!(descriptors(), before);
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_partial_registration_failure_closes_previously_owned_children_and_selector
fn partial_registration_failure_closes_previously_owned_children_and_selector() {
    infrastructure_case("registration");
}

#[test]
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_machinery_memory_and_runtime_errors_are_fatal_not_optional_absence
fn machinery_memory_and_runtime_errors_are_fatal_not_optional_absence() {
    infrastructure_case("memory");
    infrastructure_case("runtime");
}
