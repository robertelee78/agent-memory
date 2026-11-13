use agent_memory::clock::{Deadline, clock_identity, monotonic_ns};
use agent_memory::process::{self, Input, Refusal, Spec, Status, Stream, Target};
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct PrivateDir(PathBuf);

impl PrivateDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = PathBuf::from(format!("/tmp/am-t0-process-{}-{}-{}", std::process::id(),
                monotonic_ns(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("Private test directory: {error}"),
            }
        }
    }

    fn file(&self, name: &str) -> File {
        OpenOptions::new().read(true).write(true).create_new(true).mode(0o600)
            .open(self.0.join(name)).unwrap()
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn argv(parts: &[&str]) -> Vec<OsString> {
    parts.iter().map(OsString::from).collect()
}

fn spec<'a>(args: &'a [OsString], cwd: &'a Path) -> Spec<'a> {
    Spec {
        argv: args, env: &[], cwd, stdin: Input::Null,
        stdout: Target::Capture { limit: 2 * 1024 * 1024 },
        stderr: Target::Capture { limit: 2 * 1024 * 1024 },
        deadline: Deadline::after(Duration::from_secs(10)).unwrap(),
        new_session: false, file_size_limit: None, umask: None, pass_fds: &[],
    }
}

fn assert_gone_and_reaped(pid: u32) {
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
    assert!(!process::pid_is_alive(pid as i32).unwrap());
}

#[test]
fn captures_exact_stdout_stderr_and_nonzero_exit() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "printf 'out\\n\\n'; printf 'err\\n' >&2; exit 7"]);
    let output = process::run(&spec(&args, &dir.0)).unwrap();
    assert_eq!(output.status, Status::Code(7));
    assert_eq!(output.stdout, b"out\n\n");
    assert_eq!(output.stderr, b"err\n");
    assert!(output.elapsed_ns > 0);
    assert_gone_and_reaped(output.pid);
}

#[test]
fn feeds_and_drains_more_than_a_pipe_buffer_concurrently() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/cat"]);
    let payload: Vec<u8> = (0..1024 * 1024 + 37).map(|n| (n % 251) as u8).collect();
    let mut request = spec(&args, &dir.0);
    request.stdin = Input::Bytes(&payload);
    let output = process::run(&request).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, payload);
    assert_eq!(output.stderr, b"");
}

#[test]
fn drains_stdout_and_stderr_under_simultaneous_pressure() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "i=0; while [ \"$i\" -lt 40000 ]; do printf 'out!'; printf 'err!' >&2; i=$((i+1)); done"]);
    let output = process::run(&spec(&args, &dir.0)).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, b"out!".repeat(40000));
    assert_eq!(output.stderr, b"err!".repeat(40000));
}

#[test]
fn deadline_kills_and_reaps_only_the_owned_child_with_evidence() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "printf 'partial-out'; printf 'partial-err' >&2; exec /bin/sleep 30"]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::after(Duration::from_millis(400)).unwrap();
    request.new_session = true;
    let error = process::run(&request).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    assert!(error.cleanup_error.is_none());
    let evidence = error.evidence.unwrap();
    assert_eq!(evidence.argv0, OsString::from("/bin/sh"));
    assert_eq!(evidence.stdout, b"partial-out");
    assert_eq!(evidence.stderr, b"partial-err");
    assert!(evidence.elapsed_ns >= 400_000_000);
    assert!(evidence.killed && evidence.reaped);
    assert_eq!(evidence.status, Some(Status::Signal(libc::SIGKILL)));
    assert_gone_and_reaped(evidence.pid);
}

struct RecordedGrandchild(i32);

impl Drop for RecordedGrandchild {
    fn drop(&mut self) {
        unsafe { libc::kill(self.0, libc::SIGKILL); }
    }
}

fn assert_grandchild_responsive(dir: &PrivateDir, grandchild: &RecordedGrandchild) {
    assert!(process::pid_is_alive(grandchild.0).unwrap());
    fs::write(dir.0.join("release"), b"release").unwrap();
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    loop {
        if fs::read(dir.0.join("survived")).is_ok_and(|bytes| bytes == b"survived") {
            break;
        }
        assert!(!deadline.expired(), "Grandchild must respond after the direct child was reaped");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn background_grandchild_is_not_signalled_on_deadline() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "(while [ ! -e release ]; do /bin/sleep 0.01; done; printf survived > survived; exec /bin/sleep 30) & p=$!; printf '%s\\n' \"$p\" > grandchild.pid; printf 'ready'; wait"]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::after(Duration::from_millis(400)).unwrap();
    request.new_session = true;
    let error = process::run(&request).unwrap_err();
    let grandchild = RecordedGrandchild(fs::read_to_string(dir.0.join("grandchild.pid")).unwrap().trim().parse().unwrap());
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert!(evidence.killed && evidence.reaped);
    assert_eq!(evidence.stdout, b"ready");
    assert_grandchild_responsive(&dir, &grandchild);
    assert_gone_and_reaped(evidence.pid);
}

#[test]
fn reaped_child_is_never_signalled_even_when_descendant_holds_pipes() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "(while [ ! -e release ]; do /bin/sleep 0.01; done; printf survived > survived; exec /bin/sleep 30) & p=$!; printf '%s\\n' \"$p\" > grandchild.pid; printf 'done'; exit 0"]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::after(Duration::from_millis(400)).unwrap();
    request.new_session = true;
    let error = process::run(&request).unwrap_err();
    let grandchild = RecordedGrandchild(fs::read_to_string(dir.0.join("grandchild.pid")).unwrap().trim().parse().unwrap());
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert!(!evidence.killed && evidence.reaped);
    assert_eq!(evidence.status, Some(Status::Code(0)));
    assert_eq!(evidence.stdout, b"done");
    assert_grandchild_responsive(&dir, &grandchild);
    assert_gone_and_reaped(evidence.pid);
}

#[test]
fn capture_caps_are_per_stream_and_distinct_from_deadlines() {
    let dir = PrivateDir::new();
    for stream in [Stream::Stdout, Stream::Stderr] {
        let script = if stream == Stream::Stdout {
            "while :; do printf '0123456789abcdef'; done"
        } else {
            "while :; do printf '0123456789abcdef' >&2; done"
        };
        let args = argv(&["/bin/sh", "-c", script]);
        let mut request = spec(&args, &dir.0);
        request.stdout = Target::Capture { limit: 1024 };
        request.stderr = Target::Capture { limit: 1024 };
        let error = process::run(&request).unwrap_err();
        assert_eq!(error.reason, Refusal::CaptureLimit { stream, limit: 1024 });
        let evidence = error.evidence.unwrap();
        assert!(evidence.killed && evidence.reaped);
        let bytes = if stream == Stream::Stdout { &evidence.stdout } else { &evidence.stderr };
        assert_eq!(bytes, &b"0123456789abcdef".repeat(64));
        assert_gone_and_reaped(evidence.pid);
    }
}

#[test]
fn supplied_environment_replaces_instead_of_overlaying_parent() {
    let dir = PrivateDir::new();
    let args = argv(&["/usr/bin/env"]);
    let env = [(OsString::from("ONLY_SUPPLIED"), OsString::from("exact-value"))];
    let mut request = spec(&args, &dir.0);
    request.env = &env;
    let output = process::run(&request).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, b"ONLY_SUPPLIED=exact-value\n");
    assert_eq!(output.stderr, b"");
}

#[test]
fn cwd_and_umask_apply_only_to_the_child() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "printf 'here' > relative; umask"]);
    let mut request = spec(&args, &dir.0);
    request.umask = Some(0o077);
    let output = process::run(&request).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, b"0077\n");
    assert_eq!(fs::read(dir.0.join("relative")).unwrap(), b"here");
    assert_eq!(fs::metadata(dir.0.join("relative")).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn file_size_limit_stops_oversized_redirected_writes() {
    let dir = PrivateDir::new();
    let body = dir.file("body");
    let args = argv(&["/bin/sh", "-c", "exec /bin/dd if=/dev/zero bs=4096 count=16"]);
    let mut request = spec(&args, &dir.0);
    request.stdout = Target::Descriptor(body.as_fd());
    request.file_size_limit = Some(1024);
    let output = process::run(&request).unwrap();
    assert_ne!(output.status, Status::Code(0));
    assert_eq!(body.metadata().unwrap().len(), 1024);
    assert_eq!(output.stdout, b"");
}

#[test]
fn child_termination_by_signal_is_not_an_exit_code() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "kill -TERM $$"]);
    let output = process::run(&spec(&args, &dir.0)).unwrap();
    assert_eq!(output.status, Status::Signal(libc::SIGTERM));
    assert_gone_and_reaped(output.pid);
}

fn duplicate_at(fd: i32, minimum: i32) -> OwnedFd {
    let duplicated = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, minimum) };
    assert!(duplicated >= minimum, "{}", std::io::Error::last_os_error());
    unsafe { OwnedFd::from_raw_fd(duplicated) }
}

#[test]
fn pass_fds_retains_exact_numbers_and_every_other_fd_is_closed() {
    let dir = PrivateDir::new();
    let mut passed_file = dir.file("passed");
    let hidden_file = dir.file("hidden");
    let passed = duplicate_at(passed_file.as_raw_fd(), 3);
    let hidden = duplicate_at(hidden_file.as_raw_fd(), 100);
    let script = format!("echo x >&\"$1\"; if [ -e /dev/fd/{} ]; then exit 91; fi", hidden.as_raw_fd());
    let number = passed.as_raw_fd().to_string();
    let args = argv(&["/bin/sh", "-c", &script, "pass-fd-test", &number]);
    let pass_fds = [passed.as_fd()];
    let mut request = spec(&args, &dir.0);
    request.pass_fds = &pass_fds;
    let output = process::run(&request).unwrap();
    assert_eq!(output.status, Status::Code(0));
    passed_file.seek(SeekFrom::Start(0)).unwrap();
    let mut raw = Vec::new();
    passed_file.read_to_end(&mut raw).unwrap();
    assert_eq!(raw, b"x\n");
    assert_eq!(hidden_file.metadata().unwrap().len(), 0);
    assert!(unsafe { libc::fcntl(hidden.as_raw_fd(), libc::F_GETFD) } >= 0);
}

#[test]
fn duplicate_and_standard_pass_fds_are_refused_before_spawn() {
    let dir = PrivateDir::new();
    let file = dir.file("passed");
    let args = argv(&["/bin/echo", "must not execute"]);
    let duplicates = [file.as_fd(), file.as_fd()];
    let mut request = spec(&args, &dir.0);
    request.pass_fds = &duplicates;
    let error = process::run(&request).unwrap_err();
    assert_eq!(error.reason, Refusal::InvalidInput);
    assert!(error.evidence.is_none());
    let stdout = std::io::stdout();
    let standard = [stdout.as_fd()];
    request.pass_fds = &standard;
    assert_eq!(process::run(&request).unwrap_err().reason, Refusal::InvalidInput);
}

#[test]
fn descriptor_stdin_and_redirected_stderr_are_explicit() {
    let dir = PrivateDir::new();
    fs::write(dir.0.join("input"), b"descriptor bytes\0\n").unwrap();
    let input = File::open(dir.0.join("input")).unwrap();
    let mut errors = dir.file("errors");
    let args = argv(&["/bin/sh", "-c", "/bin/cat; printf 'redirected-error' >&2"]);
    let mut request = spec(&args, &dir.0);
    request.stdin = Input::Descriptor(input.as_fd());
    request.stderr = Target::Descriptor(errors.as_fd());
    let output = process::run(&request).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, b"descriptor bytes\0\n");
    assert_eq!(output.stderr, b"");
    drop(request);
    errors.seek(SeekFrom::Start(0)).unwrap();
    let mut raw = Vec::new();
    errors.read_to_end(&mut raw).unwrap();
    assert_eq!(raw, b"redirected-error");
}

#[test]
fn fixed_clock_and_checked_deadline_arithmetic() {
    let a = monotonic_ns();
    let b = monotonic_ns();
    assert!(b >= a);
    #[cfg(target_os = "linux")]
    assert_eq!(clock_identity(), "linux:clock_gettime(CLOCK_MONOTONIC)");
    #[cfg(target_os = "macos")]
    assert_eq!(clock_identity(), "macos:mach_absolute_time");
    let past = Deadline::from_absolute_ns(0);
    assert!(past.expired());
    assert_eq!(past.remaining(), Duration::ZERO);
    let future = Deadline::from_seconds(1.0).unwrap();
    assert!(future.as_ns() > b);
    assert!(future.remaining() <= Duration::from_secs(1));
    assert!(!future.expired());
    for seconds in [-1.0, f64::NAN, f64::INFINITY, f64::MAX, u64::MAX as f64] {
        assert!(Deadline::from_seconds(seconds).is_err());
    }
    assert!(Deadline::after(Duration::from_secs(u64::MAX)).is_err());
    assert!(Deadline::after(Duration::from_nanos(u64::MAX)).is_err());
}
