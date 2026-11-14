use agent_memory::clock::{Deadline, clock_identity, monotonic_ns};
use agent_memory::process::{self, FileSize, Group, Input, Refusal, Spec, Status, Stream, Target};
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct PrivateDir(PathBuf);

impl PrivateDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = PathBuf::from(format!(
                "/tmp/am-t0-process-{}-{}-{}",
                std::process::id(),
                monotonic_ns(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("Private test directory: {error}"),
            }
        }
    }

    fn file(&self, name: &str) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.0.join(name))
            .unwrap()
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
        argv: args,
        search_path: false,
        env: &[],
        cwd: Some(cwd),
        stdin: Input::Null,
        stdout: Target::Capture {
            limit: 2 * 1024 * 1024,
        },
        stderr: Target::Capture {
            limit: 2 * 1024 * 1024,
        },
        deadline: Deadline::after(Duration::from_secs(10)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    }
}

fn assert_gone_and_reaped(pid: u32) {
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    let mut status = 0;
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

#[test]
fn captures_exact_stdout_stderr_and_nonzero_exit() {
    let dir = PrivateDir::new();
    let args = argv(&[
        "/bin/sh",
        "-c",
        "printf 'out\\n\\n'; printf 'err\\n' >&2; exit 7",
    ]);
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
    let args = argv(&[
        "/bin/sh",
        "-c",
        "i=0; while [ \"$i\" -lt 40000 ]; do printf 'out!'; printf 'err!' >&2; i=$((i+1)); done",
    ]);
    let output = process::run(&spec(&args, &dir.0)).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, b"out!".repeat(40000));
    assert_eq!(output.stderr, b"err!".repeat(40000));
}

#[test]
fn deadline_kills_and_reaps_only_the_owned_child_with_evidence() {
    let dir = PrivateDir::new();
    let args = argv(&[
        "/bin/sh",
        "-c",
        "printf 'partial-out'; printf 'partial-err' >&2; exec /bin/sleep 30",
    ]);
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
        unsafe {
            libc::kill(self.0, libc::SIGKILL);
        }
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
        assert!(
            !deadline.expired(),
            "Grandchild must respond after the direct child was reaped"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn background_grandchild_is_not_signalled_on_deadline() {
    let dir = PrivateDir::new();
    let args = argv(&[
        "/bin/sh",
        "-c",
        "(while [ ! -e release ]; do /bin/sleep 0.01; done; printf survived > survived; exec /bin/sleep 30) & p=$!; printf '%s\\n' \"$p\" > grandchild.pid; printf 'ready'; wait",
    ]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::after(Duration::from_millis(400)).unwrap();
    request.new_session = true;
    let error = process::run(&request).unwrap_err();
    let grandchild = RecordedGrandchild(
        fs::read_to_string(dir.0.join("grandchild.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    );
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
    let args = argv(&[
        "/bin/sh",
        "-c",
        "(while [ ! -e release ]; do /bin/sleep 0.01; done; printf survived > survived; exec /bin/sleep 30) & p=$!; printf '%s\\n' \"$p\" > grandchild.pid; printf 'done'; exit 0",
    ]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::after(Duration::from_millis(400)).unwrap();
    request.new_session = true;
    let error = process::run(&request).unwrap_err();
    let grandchild = RecordedGrandchild(
        fs::read_to_string(dir.0.join("grandchild.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    );
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
        assert_eq!(
            error.reason,
            Refusal::CaptureLimit {
                stream,
                limit: 1024
            }
        );
        let evidence = error.evidence.unwrap();
        assert!(evidence.killed && evidence.reaped);
        let bytes = if stream == Stream::Stdout {
            &evidence.stdout
        } else {
            &evidence.stderr
        };
        assert_eq!(bytes, &b"0123456789abcdef".repeat(64));
        assert_gone_and_reaped(evidence.pid);
    }
}

#[test]
fn supplied_environment_replaces_instead_of_overlaying_parent() {
    let dir = PrivateDir::new();
    let args = argv(&["/usr/bin/env"]);
    let env = [(
        OsString::from("ONLY_SUPPLIED"),
        OsString::from("exact-value"),
    )];
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
    assert_eq!(
        fs::metadata(dir.0.join("relative")).unwrap().mode() & 0o777,
        0o600
    );
}

#[test]
fn file_size_limit_stops_oversized_redirected_writes() {
    let dir = PrivateDir::new();
    let body = dir.file("body");
    let args = argv(&[
        "/bin/sh",
        "-c",
        "exec /bin/dd if=/dev/zero bs=4096 count=16",
    ]);
    let mut request = spec(&args, &dir.0);
    request.stdout = Target::Descriptor(body.as_fd());
    request.file_size_limit = Some(FileSize::SoftCap(1024));
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
    let duplicated = unsafe { libc::fcntl(fd, libc::F_DUPFD, minimum) };
    assert!(duplicated >= minimum, "{}", std::io::Error::last_os_error());
    unsafe { OwnedFd::from_raw_fd(duplicated) }
}

#[test]
fn pass_fds_retains_exact_numbers_and_every_other_fd_is_closed() {
    let dir = PrivateDir::new();
    let mut passed_file = dir.file("passed");
    let hidden_file = dir.file("hidden");
    let passed = duplicate_at(passed_file.as_raw_fd(), 64);
    let hidden = duplicate_at(hidden_file.as_raw_fd(), 100);
    // dash rejects multi-digit >&N operands; /dev/fd still checks the exact inherited fd.
    let script = format!(
        "echo x > /dev/fd/\"$1\"; if [ -e /dev/fd/{} ]; then exit 91; fi",
        hidden.as_raw_fd()
    );
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
    assert_eq!(
        process::run(&request).unwrap_err().reason,
        Refusal::InvalidInput
    );
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

#[test]
fn invalid_environment_names_are_refused_before_spawn() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "printf executed > executed"]);
    for key in [
        OsString::new(),
        OsString::from("BAD=NAME"),
        OsString::from_vec(b"BAD\0NAME".to_vec()),
    ] {
        let env = [(key, OsString::from("value"))];
        let mut request = spec(&args, &dir.0);
        request.env = &env;
        let error = process::run(&request).unwrap_err();
        assert_eq!(error.reason, Refusal::InvalidInput);
        assert!(error.evidence.is_none());
        assert_eq!(
            error.source.unwrap().to_string(),
            "illegal environment variable name"
        );
        assert!(!dir.0.join("executed").exists());
    }
}

fn inherited_file_size() -> libc::rlimit {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit) },
        0
    );
    limit
}

/// `ulimit -f` units differ by shell (512-byte blocks in dash, 1024 in the
/// bash 3.2 behind macOS /bin/sh). Measure them without the primitive under test.
fn shell_block_bytes() -> u64 {
    const PROBE: u64 = 1 << 20;
    let mut inherited = inherited_file_size();
    if inherited.rlim_max != libc::RLIM_INFINITY && inherited.rlim_max < PROBE {
        panic!("inherited hard file-size limit is too small to calibrate the shell");
    }
    inherited.rlim_cur = PROBE;
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "ulimit -S -f"]);
    unsafe {
        command.pre_exec(move || {
            if libc::setrlimit(libc::RLIMIT_FSIZE, &inherited) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = command.output().unwrap();
    assert!(output.status.success());
    let blocks: u64 = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let unit = PROBE / blocks;
    assert!(
        unit == 512 || unit == 1024,
        "unexpected shell file-size unit {unit}"
    );
    unit
}

fn shell_file_size(value: u64) -> String {
    if value == libc::RLIM_INFINITY {
        "unlimited".to_owned()
    } else {
        (value / shell_block_bytes()).to_string()
    }
}

fn assert_file_size_modes() {
    let dir = PrivateDir::new();
    let inherited = inherited_file_size();
    let bound = 8192;
    let soft = if inherited.rlim_max == libc::RLIM_INFINITY {
        bound
    } else {
        bound.min(inherited.rlim_max)
    };
    let pinned = [inherited.rlim_cur, inherited.rlim_max]
        .into_iter()
        .filter(|&value| value != libc::RLIM_INFINITY)
        .fold(bound, u64::min);
    for (mode, soft, hard) in [
        (FileSize::SoftCap(bound), soft, inherited.rlim_max),
        (FileSize::Pinned(bound), pinned, pinned),
    ] {
        let args = argv(&["/bin/sh", "-c", "ulimit -S -f; ulimit -H -f"]);
        let mut request = spec(&args, &dir.0);
        request.file_size_limit = Some(mode);
        let output = process::run(&request).unwrap();
        assert_eq!(output.status, Status::Code(0));
        assert_eq!(
            output.stdout,
            format!("{}\n{}\n", shell_file_size(soft), shell_file_size(hard)).as_bytes()
        );
        assert_eq!(output.stderr, b"");
    }
    let after = inherited_file_size();
    assert_eq!(
        (after.rlim_cur, after.rlim_max),
        (inherited.rlim_cur, inherited.rlim_max)
    );
}

#[test]
fn file_size_modes_match_both_native_limits() {
    assert_file_size_modes();
}

#[test]
fn pinned_file_size_preserves_an_inherited_finite_soft_limit() {
    const MARKER: &str = "AM_T0_PINNED_FSIZE_WORKER";
    if std::env::var_os(MARKER).is_some() {
        assert_ne!(inherited_file_size().rlim_cur, libc::RLIM_INFINITY);
        assert_file_size_modes();
        return;
    }
    let mut inherited = inherited_file_size();
    inherited.rlim_cur = if inherited.rlim_max == libc::RLIM_INFINITY {
        1024
    } else {
        1024.min(inherited.rlim_max)
    };
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "pinned_file_size_preserves_an_inherited_finite_soft_limit",
            "--nocapture",
        ])
        .env(MARKER, "1");
    unsafe {
        command.pre_exec(move || {
            if libc::setrlimit(libc::RLIMIT_FSIZE, &inherited) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ignored_sigxfsz_is_restored_to_default_in_the_child_only() {
    const MARKER: &str = "AM_T0_SIGXFSZ_WORKER";
    if std::env::var_os(MARKER).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ignored_sigxfsz_is_restored_to_default_in_the_child_only",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_ne!(
        unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) },
        libc::SIG_ERR
    );
    let dir = PrivateDir::new();
    let script = format!("ulimit -c 0; kill -{} $$; printf survived", libc::SIGXFSZ);
    let args = argv(&["/bin/sh", "-c", &script]);
    let output = process::run(&spec(&args, &dir.0)).unwrap();
    assert_eq!(output.status, Status::Signal(libc::SIGXFSZ));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) },
        libc::SIG_IGN
    );
}

#[cfg(target_os = "linux")]
fn deny_syscalls(calls: &[libc::c_long], errno: i32) {
    let mut filter = vec![libc::sock_filter {
        code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
        jt: 0,
        jf: 0,
        k: 0,
    }];
    for &call in calls {
        filter.push(libc::sock_filter {
            code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            jt: 0,
            jf: 1,
            k: call as u32,
        });
        filter.push(libc::sock_filter {
            code: (libc::BPF_RET | libc::BPF_K) as u16,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ERRNO | errno as u32,
        });
    }
    filter.push(libc::sock_filter {
        code: (libc::BPF_RET | libc::BPF_K) as u16,
        jt: 0,
        jf: 0,
        k: libc::SECCOMP_RET_ALLOW,
    });
    install_seccomp_filter(&mut filter);
}

#[cfg(target_os = "linux")]
fn install_seccomp_filter(filter: &mut [libc::sock_filter]) {
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    assert_eq!(
        unsafe {
            libc::prctl(
                libc::PR_SET_NO_NEW_PRIVS,
                1 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        },
        0,
        "{}",
        std::io::Error::last_os_error()
    );
    assert_eq!(
        unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER as libc::c_ulong,
                &program as *const libc::sock_fprog,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        },
        0,
        "{}",
        std::io::Error::last_os_error()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn close_range_falls_back_only_for_compatibility_errnos() {
    const MARKER: &str = "AM_T0_CLOSE_RANGE_ERRNO";
    let Some(errno) = std::env::var_os(MARKER) else {
        for errno in [libc::ENOSYS, libc::EINVAL, libc::EPERM, libc::EIO] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "close_range_falls_back_only_for_compatibility_errnos",
                    "--nocapture",
                ])
                .env(MARKER, errno.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "errno {errno}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    let errno: i32 = errno.to_str().unwrap().parse().unwrap();
    let dir = PrivateDir::new();
    let mut passed_file = dir.file("passed");
    // Force multi-digit coverage without relying on descriptors inherited by the worker.
    let passed = duplicate_at(passed_file.as_raw_fd(), 64);
    // More than one getdents64 stack buffer, with inheritable descriptors.
    let hidden: Vec<OwnedFd> = (0..200)
        .map(|_| duplicate_at(passed_file.as_raw_fd(), 3))
        .collect();
    let hidden_numbers = hidden
        .iter()
        .map(|fd| fd.as_raw_fd().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "echo x > /dev/fd/\"$1\"; for fd in {hidden_numbers}; do if [ -e /proc/self/fd/$fd ]; then exit 91; fi; done"
    );
    let number = passed.as_raw_fd().to_string();
    let args = argv(&["/bin/sh", "-c", &script, "fallback-test", &number]);
    let pass_fds = [passed.as_fd()];
    let mut request = spec(&args, &dir.0);
    request.pass_fds = &pass_fds;
    deny_syscalls(&[libc::SYS_close_range], errno);
    let result = process::run(&request);
    if errno == libc::EIO {
        let error = result.unwrap_err();
        assert_eq!(error.reason, Refusal::Spawn);
        assert_eq!(error.source.unwrap().raw_os_error(), Some(libc::EIO));
        assert!(error.evidence.is_none());
        assert_eq!(passed_file.metadata().unwrap().len(), 0);
    } else {
        let output = result.unwrap();
        assert_eq!(output.status, Status::Code(0));
        drop(request);
        passed_file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        passed_file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"x\n");
    }
}

#[test]
fn group_children_finish_out_of_order_but_are_consumed_in_order() {
    let dir = PrivateDir::new();
    let args = [
        argv(&[
            "/bin/sh",
            "-c",
            "while [ ! -e second.finished ] || [ ! -e third.finished ]; do /bin/sleep 0.01; done; printf first",
        ]),
        argv(&[
            "/bin/sh",
            "-c",
            "while [ ! -e third.finished ]; do /bin/sleep 0.01; done; printf second; printf yes > second.finished",
        ]),
        argv(&["/bin/sh", "-c", "printf third; printf yes > third.finished"]),
    ];
    let requests: Vec<_> = args.iter().map(|args| spec(args, &dir.0)).collect();
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    for (index, expected) in [b"first".as_slice(), b"second", b"third"]
        .into_iter()
        .enumerate()
    {
        let output = group.value(index).unwrap();
        assert_eq!(output.status, Status::Code(0));
        assert_eq!(output.stdout, expected);
        assert_gone_and_reaped(output.pid);
    }
}

#[test]
fn one_group_deadline_does_not_cancel_completed_siblings() {
    let dir = PrivateDir::new();
    let args = [
        argv(&["/bin/sh", "-c", "printf slow; exec /bin/sleep 30"]),
        argv(&["/bin/echo", "second"]),
        argv(&["/bin/echo", "third"]),
    ];
    let requests: Vec<_> = args.iter().map(|args| spec(args, &dir.0)).collect();
    let mut group = Group::launch(&requests, Duration::from_millis(400)).unwrap();
    let error = group.value(0).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert!(evidence.killed && evidence.reaped);
    assert_eq!(evidence.stdout, b"slow");
    assert_gone_and_reaped(evidence.pid);
    assert_eq!(group.value(1).unwrap().stdout, b"second\n");
    assert_eq!(group.value(2).unwrap().stdout, b"third\n");
}

#[test]
fn dropping_a_group_kills_and_reaps_all_outstanding_direct_children() {
    let dir = PrivateDir::new();
    let files: Vec<_> = (0..3)
        .map(|index| dir.file(&format!("pid-{index}")))
        .collect();
    let args = argv(&["/bin/sh", "-c", "printf '%s\\n' $$; exec /bin/sleep 30"]);
    let requests: Vec<_> = files
        .iter()
        .map(|file| {
            let mut request = spec(&args, &dir.0);
            request.stdout = Target::Descriptor(file.as_fd());
            request
        })
        .collect();
    let group = Group::launch(&requests, Duration::from_secs(30)).unwrap();
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    let mut pids = Vec::new();
    for index in 0..3 {
        loop {
            let bytes = fs::read(dir.0.join(format!("pid-{index}"))).unwrap();
            if bytes.ends_with(b"\n") {
                pids.push(
                    std::str::from_utf8(&bytes)
                        .unwrap()
                        .trim()
                        .parse::<u32>()
                        .unwrap(),
                );
                break;
            }
            assert!(!deadline.expired(), "Child did not record its own PID");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    drop(group);
    for pid in pids {
        assert_gone_and_reaped(pid);
    }
}

#[test]
fn group_rejects_out_of_order_and_repeated_consumption_without_panicking() {
    let dir = PrivateDir::new();
    let first = argv(&["/bin/echo", "first"]);
    let second = argv(&["/bin/echo", "second"]);
    let requests = [spec(&first, &dir.0), spec(&second, &dir.0)];
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    assert_eq!(group.value(1).unwrap_err().reason, Refusal::InvalidInput);
    assert_eq!(group.value(0).unwrap().stdout, b"first\n");
    assert_eq!(group.value(0).unwrap_err().reason, Refusal::InvalidInput);
    assert_eq!(group.value(1).unwrap().stdout, b"second\n");
    assert_eq!(group.value(2).unwrap_err().reason, Refusal::InvalidInput);
}

#[test]
fn group_reaps_a_child_that_closes_both_pipes_and_lingers() {
    let dir = PrivateDir::new();
    let args = argv(&[
        "/bin/sh",
        "-c",
        "printf '%s\\n' $$; exec 1>&- 2>&-; exec /bin/sleep 30",
    ]);
    let requests = [spec(&args, &dir.0)];
    let mut group = Group::launch(&requests, Duration::from_millis(400)).unwrap();
    group
        .set_exit_poll_interval(Duration::from_millis(100))
        .unwrap();
    let error = group.value(0).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert!(evidence.killed && evidence.reaped);
    assert!(
        evidence.poll_wakeups <= 12,
        "Closed-pipe child caused {} poll wakeups",
        evidence.poll_wakeups
    );
    let pid: u32 = std::str::from_utf8(&evidence.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(pid, evidence.pid);
    assert_gone_and_reaped(pid);
    group.finish().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn group_poll_fault_invalidates_all_unconsumed_outcomes_and_reaps_children() {
    const MARKER: &str = "AM_T0_GROUP_POLL_WORKER";
    if std::env::var_os(MARKER).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "group_poll_fault_invalidates_all_unconsumed_outcomes_and_reaps_children",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sh", "-c", "exec /bin/sleep 30"]);
    let requests = [spec(&args, &dir.0), spec(&args, &dir.0)];
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    #[cfg(target_arch = "x86_64")]
    let syscalls = [libc::SYS_poll, libc::SYS_ppoll];
    #[cfg(not(target_arch = "x86_64"))]
    let syscalls = [libc::SYS_ppoll];
    deny_syscalls(&syscalls, libc::EIO);
    for index in 0..2 {
        let error = group.value(index).unwrap_err();
        assert_eq!(error.reason, Refusal::Infrastructure);
        assert_eq!(error.source.unwrap().raw_os_error(), Some(libc::EIO));
        let evidence = error.evidence.unwrap();
        assert!(evidence.killed && evidence.reaped);
        assert_gone_and_reaped(evidence.pid);
    }
}

#[test]
fn group_spawn_failure_is_independent_of_other_children() {
    let dir = PrivateDir::new();
    let first = argv(&["/bin/echo", "first"]);
    let missing = vec![dir.0.join("missing-program").into_os_string()];
    let third = argv(&["/bin/echo", "third"]);
    let requests = [
        spec(&first, &dir.0),
        spec(&missing, &dir.0),
        spec(&third, &dir.0),
    ];
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    assert_eq!(group.value(0).unwrap().stdout, b"first\n");
    let error = group.value(1).unwrap_err();
    assert_eq!(error.reason, Refusal::Spawn);
    assert_eq!(error.source.unwrap().raw_os_error(), Some(libc::ENOENT));
    assert!(error.evidence.is_none());
    assert_eq!(group.value(2).unwrap().stdout, b"third\n");
}

#[test]
fn group_uses_launch_relative_timeouts_instead_of_spec_absolute_deadlines() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/echo", "fresh interval"]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::from_absolute_ns(0);
    let requests = [request];
    let mut group = Group::launch(&requests, Duration::from_secs(5)).unwrap();
    assert_eq!(group.value(0).unwrap().stdout, b"fresh interval\n");
}

#[test]
fn supplied_path_search_preserves_bare_argv0_and_stdin() {
    let dir = PrivateDir::new();
    let env = [(OsString::from("PATH"), OsString::from("/bin:/usr/bin"))];
    let args = argv(&["cat"]);
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    request.env = &env;
    request.stdin = Input::Bytes(b"bare cat\n");
    assert_eq!(process::run(&request).unwrap().stdout, b"bare cat\n");
    let args = argv(&["sh", "-c", "printf '%s' \"$0\""]);
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    request.env = &env;
    assert_eq!(process::run(&request).unwrap().stdout, b"sh");
}

#[test]
fn path_search_never_uses_the_parent_path_and_has_a_fixed_default() {
    const MARKER: &str = "AM_T0_PATH_SEARCH_WORKER";
    if std::env::var_os(MARKER).is_none() {
        let dir = PrivateDir::new();
        std::os::unix::fs::symlink("/bin/sh", dir.0.join("parent-only-program")).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "path_search_never_uses_the_parent_path_and_has_a_fixed_default",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .env("PATH", &dir.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = PrivateDir::new();
    let args = argv(&["parent-only-program"]);
    let env = [(OsString::from("PATH"), OsString::from("/bin:/usr/bin"))];
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    request.env = &env;
    let error = process::run(&request).unwrap_err();
    assert_eq!(error.reason, Refusal::Spawn);
    assert_eq!(error.source.unwrap().raw_os_error(), Some(libc::ENOENT));
    let args = argv(&["env"]);
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    let output = process::run(&request).unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, b"");
}

#[test]
fn empty_and_relative_path_entries_use_the_child_cwd() {
    let dir = PrivateDir::new();
    fs::create_dir(dir.0.join("bin")).unwrap();
    std::os::unix::fs::symlink("/bin/sh", dir.0.join("bin").join("local-sh")).unwrap();
    std::os::unix::fs::symlink("/bin/sh", dir.0.join("cwd-sh")).unwrap();
    for (name, path) in [("local-sh", "missing:bin"), ("cwd-sh", "missing:")] {
        let args = argv(&[name, "-c", "printf '%s' \"$0\""]);
        let env = [(OsString::from("PATH"), OsString::from(path))];
        let mut request = spec(&args, &dir.0);
        request.search_path = true;
        request.env = &env;
        assert_eq!(process::run(&request).unwrap().stdout, name.as_bytes());
    }
    let args = argv(&["bin/local-sh", "-c", "exit 0"]);
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    assert_eq!(
        process::run(&request).unwrap_err().reason,
        Refusal::InvalidInput
    );
}

#[test]
fn path_search_retains_first_other_errno_and_last_missing_errno() {
    let dir = PrivateDir::new();
    fs::create_dir(dir.0.join("denied")).unwrap();
    fs::create_dir(dir.0.join("empty")).unwrap();
    fs::write(dir.0.join("denied").join("program"), b"not executable").unwrap();
    fs::write(dir.0.join("not-a-directory"), b"ordinary file").unwrap();
    let args = argv(&["program"]);
    for (path, expected) in [
        ("denied:empty", libc::EACCES),
        ("empty:not-a-directory", libc::ENOTDIR),
        ("not-a-directory:empty", libc::ENOENT),
    ] {
        let env = [(OsString::from("PATH"), OsString::from(path))];
        let mut request = spec(&args, &dir.0);
        request.search_path = true;
        request.env = &env;
        let error = process::run(&request).unwrap_err();
        assert_eq!(error.reason, Refusal::Spawn);
        assert_eq!(error.source.unwrap().raw_os_error(), Some(expected));
    }
    let env = [(OsString::from("PATH"), OsString::from("denied:/bin"))];
    let args = argv(&["sh", "-c", "printf found"]);
    fs::write(dir.0.join("denied").join("sh"), b"not executable").unwrap();
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    request.env = &env;
    assert_eq!(process::run(&request).unwrap().stdout, b"found");
}

#[test]
fn exec_format_errors_never_fall_back_to_a_shell() {
    let dir = PrivateDir::new();
    let executable = dir.0.join("plain-text");
    fs::write(&executable, b"printf executed > shell-fallback-marker\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let args = argv(&["plain-text"]);
    let env = [(OsString::from("PATH"), dir.0.clone().into_os_string())];
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    request.env = &env;
    let error = process::run(&request).unwrap_err();
    assert_eq!(error.reason, Refusal::Spawn);
    assert_eq!(error.source.unwrap().raw_os_error(), Some(libc::ENOEXEC));
    assert!(!dir.0.join("shell-fallback-marker").exists());
}

#[test]
fn run_for_excludes_real_path_search_launch_time_from_its_timeout() {
    let dir = PrivateDir::new();
    let path = format!("{}/bin", ":".repeat(100_000));
    let env = [(OsString::from("PATH"), OsString::from(path))];
    let args = argv(&["sh", "-c", "exec /bin/sleep 30"]);
    let mut request = spec(&args, &dir.0);
    request.search_path = true;
    request.env = &env;
    request.deadline = Deadline::from_absolute_ns(0);
    let timeout = Duration::from_millis(100);
    let error = process::run_for(&request, timeout).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert_eq!(evidence.argv0, OsString::from("sh"));
    assert!(evidence.launch_elapsed_ns > 0);
    assert!(evidence.elapsed_ns >= evidence.launch_elapsed_ns + timeout.as_nanos() as u64);
    assert!(evidence.killed && evidence.reaped);
    assert_gone_and_reaped(evidence.pid);
}

#[test]
fn finish_reports_unconsumed_outcomes_and_still_cleans_up() {
    let dir = PrivateDir::new();
    let file = dir.file("pid");
    let args = argv(&["/bin/sh", "-c", "printf '%s\\n' $$; exec /bin/sleep 30"]);
    let mut request = spec(&args, &dir.0);
    request.stdout = Target::Descriptor(file.as_fd());
    let requests = [request];
    let group = Group::launch(&requests, Duration::from_secs(30)).unwrap();
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    let pid = loop {
        let bytes = fs::read(dir.0.join("pid")).unwrap();
        if bytes.ends_with(b"\n") {
            break std::str::from_utf8(&bytes)
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap();
        }
        assert!(!deadline.expired());
        std::thread::sleep(Duration::from_millis(5));
    };
    let error = group.finish().unwrap_err();
    assert_eq!(error.reason, Refusal::Infrastructure);
    assert!(error.incomplete);
    assert!(error.cleanup_error.is_none());
    assert_gone_and_reaped(pid);
}

#[cfg(target_os = "linux")]
#[test]
fn supervision_fault_preserves_the_already_terminal_prefix() {
    const MARKER: &str = "AM_T0_TERMINAL_PREFIX_WORKER";
    if std::env::var_os(MARKER).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "supervision_fault_preserves_the_already_terminal_prefix",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = PrivateDir::new();
    let args = [
        argv(&[
            "/bin/sh",
            "-c",
            "while [ ! -e sibling.pid ]; do /bin/sleep 0.01; done; read p < sibling.pid; while kill -0 \"$p\" 2>/dev/null; do /bin/sleep 0.01; done; printf zero",
        ]),
        argv(&[
            "/bin/sh",
            "-c",
            "printf '%s\\n' $$ > sibling.pid; printf terminal",
        ]),
        argv(&["/bin/sh", "-c", "exec /bin/sleep 30"]),
    ];
    let requests: Vec<_> = args.iter().map(|args| spec(args, &dir.0)).collect();
    let mut group = Group::launch(&requests, Duration::from_secs(10)).unwrap();
    assert_eq!(group.value(0).unwrap().stdout, b"zero");
    deny_syscalls(&[libc::SYS_read], libc::EIO);
    assert_eq!(group.value(1).unwrap().stdout, b"terminal");
    let error = group.value(2).unwrap_err();
    assert_eq!(error.reason, Refusal::Infrastructure);
    let evidence = error.evidence.unwrap();
    assert!(evidence.killed && evidence.reaped);
    assert_gone_and_reaped(evidence.pid);
    let error = group.finish().unwrap_err();
    assert_eq!(error.reason, Refusal::Infrastructure);
    assert!(!error.incomplete);
}

#[cfg(target_os = "linux")]
#[test]
fn finish_reports_real_close_failures_after_cancelling_and_reaping() {
    const MARKER: &str = "AM_T0_CLOSE_FAILURE_WORKER";
    let Some(root) = std::env::var_os(MARKER) else {
        // The parent owns the directory: once close(2) is denied in the worker,
        // std's own teardown (remove_dir_all, closedir) can no longer run there.
        let dir = PrivateDir::new();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "finish_reports_real_close_failures_after_cancelling_and_reaping",
                "--nocapture",
            ])
            .env(MARKER, &dir.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let dir = std::mem::ManuallyDrop::new(PrivateDir(PathBuf::from(root)));
    let file = dir.file("pid");
    let args = argv(&["/bin/sh", "-c", "printf '%s\\n' $$; exec /bin/sleep 30"]);
    let mut request = spec(&args, &dir.0);
    request.stdout = Target::Descriptor(file.as_fd());
    let requests = [request];
    let group = Group::launch(&requests, Duration::from_secs(30)).unwrap();
    let deadline = Deadline::after(Duration::from_secs(5)).unwrap();
    let pid = loop {
        let bytes = fs::read(dir.0.join("pid")).unwrap();
        if bytes.ends_with(b"\n") {
            break std::str::from_utf8(&bytes)
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap();
        }
        assert!(!deadline.expired());
        std::thread::sleep(Duration::from_millis(5));
    };
    deny_syscalls(&[libc::SYS_close], libc::EIO);
    let error = group.finish().unwrap_err();
    assert_eq!(error.reason, Refusal::Infrastructure);
    assert!(error.incomplete);
    assert_eq!(error.cleanup_error.unwrap().raw_os_error(), Some(libc::EIO));
    assert_gone_and_reaped(pid);
    std::process::exit(0);
}

#[cfg(target_os = "linux")]
fn deny_capture_reads() {
    // Leave the spawn handshake's short reads intact; fail full capture-buffer
    // reads with a real kernel EIO after the child has successfully spawned.
    let mut filter = [
        libc::sock_filter {
            code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
            jt: 0,
            jf: 0,
            k: 0,
        },
        libc::sock_filter {
            code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            jt: 0,
            jf: 3,
            k: libc::SYS_read as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
            jt: 0,
            jf: 0,
            k: (std::mem::offset_of!(libc::seccomp_data, args) + 2 * 8) as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            jt: 0,
            jf: 1,
            k: 16_384,
        },
        libc::sock_filter {
            code: (libc::BPF_RET | libc::BPF_K) as u16,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ERRNO | libc::EIO as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_RET | libc::BPF_K) as u16,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ALLOW,
        },
    ];
    install_seccomp_filter(&mut filter);
}

#[cfg(target_os = "linux")]
fn assert_single_read_fault_retains_evidence(test_name: &str, interval: bool) {
    const MARKER: &str = "AM_T0_SINGLE_READ_FAULT_WORKER";
    if std::env::var_os(MARKER).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
            .env(MARKER, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sleep", "30"]);
    let request = spec(&args, &dir.0);
    deny_capture_reads();
    let error = if interval {
        process::run_for(&request, Duration::from_secs(5))
    } else {
        process::run(&request)
    }
    .unwrap_err();
    assert_eq!(error.reason, Refusal::Infrastructure);
    assert_eq!(error.source.unwrap().raw_os_error(), Some(libc::EIO));
    assert!(error.cleanup_error.is_none());
    let evidence = error.evidence.unwrap();
    assert_eq!(evidence.argv0, OsString::from("/bin/sleep"));
    assert_eq!(evidence.stdout, b"");
    assert_eq!(evidence.stderr, b"");
    assert!(evidence.killed && evidence.reaped);
    assert_eq!(evidence.status, Some(Status::Signal(libc::SIGKILL)));
    assert_gone_and_reaped(evidence.pid);
}

#[cfg(target_os = "linux")]
#[test]
fn run_retains_read_fault_bytes_and_kill_reap_evidence() {
    assert_single_read_fault_retains_evidence(
        "run_retains_read_fault_bytes_and_kill_reap_evidence",
        false,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn run_for_retains_read_fault_bytes_and_kill_reap_evidence() {
    assert_single_read_fault_retains_evidence(
        "run_for_retains_read_fault_bytes_and_kill_reap_evidence",
        true,
    );
}

#[test]
fn group_refuses_an_exit_first_observed_after_its_interval_deadline() {
    let dir = PrivateDir::new();
    let args = [
        argv(&[
            "/bin/sh",
            "-c",
            "while [ ! -e closed.ready ]; do /bin/sleep 0.01; done; printf first",
        ]),
        argv(&[
            "/bin/sh",
            "-c",
            "printf '%s\\n' $$ > late.pid; exec 1>&- 2>&-; printf closed > closed.ready; /bin/sleep 0.6; exit 0",
        ]),
    ];
    let requests: Vec<_> = args.iter().map(|args| spec(args, &dir.0)).collect();
    let mut group = Group::launch(&requests, Duration::from_millis(400)).unwrap();
    // Query zero cannot finish until query one closed both pipes. Its ordered
    // consumption pumps and closes those pipes while query one still lingers.
    assert_eq!(group.value(0).unwrap().stdout, b"first");
    let pid: u32 = fs::read_to_string(dir.0.join("late.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut status: libc::siginfo_t = unsafe { std::mem::zeroed() };
    loop {
        // WNOWAIT proves the direct child exited without stealing its reap.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut status,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == 0 {
            break;
        }
        let error = std::io::Error::last_os_error();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted, "{error}");
    }
    let error = group.value(1).unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert_eq!(evidence.pid, pid);
    assert!(evidence.elapsed_ns >= 400_000_000);
    assert!(!evidence.killed && evidence.reaped);
    assert_eq!(evidence.status, Some(Status::Code(0)));
    assert_gone_and_reaped(pid);
    group.finish().unwrap();
}

#[test]
fn owned_process_retains_only_required_launch_data_and_pending_input() {
    let dir = PrivateDir::new();
    let expected: Vec<u8> = (0..1024 * 1024 + 37).map(|n| (n % 251) as u8).collect();
    let child = {
        let args = argv(&["/bin/cat"]);
        let cwd = dir.0.clone();
        let payload = expected.clone();
        let env = vec![(
            OsString::from("PRIVATE_LAUNCH_VALUE"),
            OsString::from("discarded"),
        )];
        let mut request = spec(&args, &cwd);
        request.env = &env;
        request.stdin = Input::Bytes(&payload);
        process::spawn_owned(&request).unwrap()
    };
    let output = child
        .finish(Deadline::after(Duration::from_secs(10)).unwrap())
        .unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
    assert_gone_and_reaped(output.pid);
}

#[test]
fn owned_process_deadline_cannot_be_widened_and_retains_reaped_evidence() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sleep", "30"]);
    let mut request = spec(&args, &dir.0);
    request.deadline = Deadline::after(Duration::from_millis(100)).unwrap();
    let child = process::spawn_owned(&request).unwrap();
    let error = child
        .finish(Deadline::after(Duration::from_secs(20)).unwrap())
        .unwrap_err();
    assert_eq!(error.reason, Refusal::Deadline);
    let evidence = error.evidence.unwrap();
    assert!(evidence.killed && evidence.reaped);
    assert!(evidence.elapsed_ns < 5_000_000_000);
    assert_gone_and_reaped(evidence.pid);
}

#[test]
fn owned_process_explicit_close_is_idempotent() {
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sleep", "30"]);
    let mut child = process::spawn_owned(&spec(&args, &dir.0)).unwrap();
    child.close().unwrap();
    child.close().unwrap();
    let error = child
        .finish(Deadline::after(Duration::from_secs(1)).unwrap())
        .unwrap_err();
    assert_eq!(error.reason, Refusal::InvalidInput);
}

#[test]
fn owned_process_capacity_worker() {
    if std::env::var_os("AM_OWNED_CAPACITY_WORKER").is_none() {
        return;
    }
    process::install_termination_handler().unwrap();
    let dir = PrivateDir::new();
    let args = argv(&["/bin/sleep", "30"]);
    let mut children = Vec::new();
    for _ in 0..3 {
        children.push(process::spawn_owned(&spec(&args, &dir.0)).unwrap());
    }
    let error = match process::spawn_owned(&spec(&args, &dir.0)) {
        Ok(_) => panic!("fourth registered owned child was launched"),
        Err(error) => error,
    };
    assert_eq!(error.reason, Refusal::OwnedCapacity);
    assert!(error.evidence.is_none());
    children.pop().unwrap().close().unwrap();
    let mut replacement = process::spawn_owned(&spec(&args, &dir.0)).unwrap();
    replacement.close().unwrap();
    for mut child in children {
        child.close().unwrap();
    }
    fs::write(
        std::env::current_dir()
            .unwrap()
            .join("owned-capacity.completed"),
        b"capacity/refusal/reuse verified\n",
    )
    .unwrap();
}

#[test]
fn owned_process_uses_existing_three_slot_termination_registry() {
    let dir = PrivateDir::new();
    let args = vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "owned_process_capacity_worker".into(),
        "--nocapture".into(),
    ];
    let env = [(
        OsString::from("AM_OWNED_CAPACITY_WORKER"),
        OsString::from("1"),
    )];
    let mut request = spec(&args, &dir.0);
    request.env = &env;
    let output = process::run(&request).unwrap();
    assert_eq!(
        output.status,
        Status::Code(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (receipt, _) = agent_memory::fs::read_regular(
        &dir.0.canonicalize().unwrap(),
        Path::new("owned-capacity.completed"),
        1024,
    )
    .unwrap();
    assert_eq!(receipt, b"capacity/refusal/reuse verified\n");
}

// Interrupt cleanup (baseline identity_git `_cleanup` under KeyboardInterrupt):
// one shared 250 ms reap budget, a second signal that ends the wait, and the
// handoff of children still unreaped. Each case runs in a re-exec worker,
// because the handler and its once-per-process cleanup are process-global.
// The dev-tools seam cancels with SIGTERM instead of SIGKILL, so the
// TERM-recording (or stopped) children below outlive their cancellation the
// way a child in uninterruptible sleep outlives SIGKILL.
#[cfg(feature = "dev-tools")]
mod interrupt_reap {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::time::Instant;

    const WORKER_ROOT: &str = "AM_T0_INTERRUPT_REAP_ROOT";
    const WORKER_MODE: &str = "AM_T0_INTERRUPT_REAP_MODE";
    const GRACE: Duration = Duration::from_millis(250);
    /// Above one budget and below three: the budget is shared, not per child.
    const SHARED_BOUND: Duration = Duration::from_millis(700);
    const NOTE_THREE: &str = "Owned child cleanup: 3 killed-but-unreaped or cancellation-unproved children handed to process exit; no further signals";
    /// Appends one line to `$0.term` per SIGTERM (the trap runs between short
    /// sleeps) and exits by itself after about five seconds.
    const RECORDER: &str = "trap 'echo t >> \"$0.term\"' TERM; echo $$ > \"$0.pid\"; i=0; while [ \"$i\" -lt 100 ]; do /bin/sleep 0.05; i=$((i+1)); done";
    /// Ignores SIGTERM and exits by itself after 0.6 s.
    const LINGERER: &str = "trap '' TERM; echo $$ > \"$0.pid\"; /bin/sleep 0.6";

    /// SIGKILLs and reaps the worker's children however the worker ends.
    struct Children(Vec<i32>);

    impl Drop for Children {
        fn drop(&mut self) {
            for &pid in &self.0 {
                let mut status = 0;
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    libc::waitpid(pid, &mut status, 0);
                }
            }
        }
    }

    /// Sends `signal` to the creating thread `after` it is armed.
    struct Signaller {
        trigger: std::sync::mpsc::Sender<()>,
        thread: std::thread::JoinHandle<u64>,
    }

    impl Signaller {
        fn new(after: Duration, signal: libc::c_int) -> Self {
            let target = unsafe { libc::pthread_self() } as usize;
            let (trigger, armed) = std::sync::mpsc::channel::<()>();
            let thread = std::thread::spawn(move || {
                armed.recv().unwrap();
                std::thread::sleep(after);
                let sent_ns = monotonic_ns();
                assert_eq!(
                    unsafe { libc::pthread_kill(target as libc::pthread_t, signal) },
                    0
                );
                sent_ns
            });
            Self { trigger, thread }
        }

        fn arm(&self) {
            self.trigger.send(()).unwrap();
        }

        /// Monotonic time at which the signal was sent.
        fn sent_ns(self) -> u64 {
            self.thread.join().unwrap()
        }
    }

    fn scripts(root: &Path, body: &str) -> Vec<Vec<OsString>> {
        (0..3)
            .map(|index| {
                vec![
                    OsString::from("/bin/sh"),
                    OsString::from("-c"),
                    OsString::from(body),
                    root.join(format!("child-{index}")).into_os_string(),
                ]
            })
            .collect()
    }

    fn child_pid(root: &Path, index: usize) -> i32 {
        let path = root.join(format!("child-{index}.pid"));
        let start = Instant::now();
        loop {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(pid) = raw.trim().parse() {
                    return pid;
                }
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "child {index} never started"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn children(root: &Path) -> Children {
        Children((0..3).map(|index| child_pid(root, index)).collect())
    }

    /// SIGTERMs child `index` has recorded so far.
    fn terms(root: &Path, index: usize) -> usize {
        fs::read_to_string(root.join(format!("child-{index}.term")))
            .map_or(0, |text| text.lines().count())
    }

    /// Long enough for every pending TERM trap to have run.
    fn settle_traps() {
        std::thread::sleep(Duration::from_millis(300));
    }

    /// Still this process's live, unreaped child.
    fn unreaped(pid: i32) -> bool {
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) == 0 }
    }

    fn assert_handed_off_note() {
        assert_eq!(
            process::interrupt_cleanup_note().as_deref(),
            Some(NOTE_THREE)
        );
    }

    fn assert_interrupted_value(group: &mut Group<'_>, index: usize) {
        let error = group.value(index).unwrap_err();
        assert_eq!(error.reason, Refusal::Interrupted);
        assert!(error.cleanup_error.is_none(), "{error}");
        assert!(!error.evidence.unwrap().reaped);
    }

    fn assert_no_launch_after_interrupt(root: &Path) {
        let args = argv(&["/bin/sh", "-c", ": > \"$0\"", "marker"]);
        let refused = process::run(&spec(&args, root)).unwrap_err();
        assert_eq!(refused.reason, Refusal::Interrupted);
        assert!(refused.evidence.is_none());
    }

    fn assert_elapsed_after_signal(sent_ns: u64, returned_ns: u64) {
        let elapsed = Duration::from_nanos(returned_ns - sent_ns);
        assert!(elapsed >= GRACE, "returned {elapsed:?} after SIGINT");
        assert!(elapsed < SHARED_BOUND, "returned {elapsed:?} after SIGINT");
    }

    // A SIGINT raised here runs the handler synchronously on this thread.
    fn shared_budget(root: &Path) {
        let argv = scripts(root, RECORDER);
        let specs: Vec<Spec<'_>> = argv.iter().map(|argv| spec(argv, root)).collect();
        let mut group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
        let pids = children(root);
        // A stopped child acts on no cancellation signal but SIGKILL.
        let mut status = 0;
        assert_eq!(unsafe { libc::kill(pids.0[0], libc::SIGSTOP) }, 0);
        assert_eq!(
            unsafe { libc::waitpid(pids.0[0], &mut status, libc::WUNTRACED) },
            pids.0[0]
        );
        assert!(libc::WIFSTOPPED(status));
        let start = Instant::now();
        assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
        let elapsed = start.elapsed();
        assert!(elapsed >= GRACE, "cleanup took {elapsed:?}");
        assert!(elapsed < SHARED_BOUND, "cleanup took {elapsed:?}");
        assert!(process::interrupted());
        assert_handed_off_note();
        // Handed off: supervision refuses at once, never signalling or waiting.
        let start = Instant::now();
        for index in 0..3 {
            assert_interrupted_value(&mut group, index);
        }
        group.finish().unwrap();
        assert!(start.elapsed() < GRACE);
        settle_traps();
        assert_eq!(terms(root, 0), 0);
        assert_eq!(terms(root, 1), 1);
        assert_eq!(terms(root, 2), 1);
        for &pid in &pids.0 {
            assert!(
                unreaped(pid),
                "handed-off child {pid} was signalled or reaped"
            );
        }
        assert_no_launch_after_interrupt(root);
    }

    fn second_during_reap(root: &Path) {
        let argv = scripts(root, RECORDER);
        let specs: Vec<Spec<'_>> = argv.iter().map(|argv| spec(argv, root)).collect();
        let mut group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
        let pids = children(root);
        let second = Signaller::new(Duration::from_millis(60), libc::SIGINT);
        second.arm();
        let start = Instant::now();
        assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
        let elapsed = start.elapsed();
        let returned_ns = monotonic_ns();
        let sent_ns = second.sent_ns();
        assert!(sent_ns <= returned_ns);
        assert!(
            elapsed < GRACE,
            "a second SIGINT did not end the wait: {elapsed:?}"
        );
        assert!(process::interrupted());
        assert_handed_off_note();
        for index in 0..3 {
            assert_interrupted_value(&mut group, index);
        }
        group.finish().unwrap();
        settle_traps();
        for (index, &pid) in pids.0.iter().enumerate() {
            assert_eq!(terms(root, index), 1);
            assert!(
                unreaped(pid),
                "handed-off child {pid} was signalled or reaped"
            );
        }
        assert_no_launch_after_interrupt(root);
    }

    // Dies by SIGTERM; the parent test checks the timing and the kills.
    fn pending_second(root: &Path) {
        let argv = scripts(root, RECORDER);
        let specs: Vec<Spec<'_>> = argv.iter().map(|argv| spec(argv, root)).collect();
        let _group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
        for index in 0..3 {
            child_pid(root, index);
        }
        // Both pending on this thread before either is delivered: the first
        // invocation's kills run masked, and the bounded wait's unblock
        // delivers the other signal before any reap poll.
        let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGINT);
            libc::sigaddset(&mut set, libc::SIGTERM);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()),
                0
            );
            assert_eq!(libc::pthread_kill(libc::pthread_self(), libc::SIGINT), 0);
            assert_eq!(libc::pthread_kill(libc::pthread_self(), libc::SIGTERM), 0);
        }
        fs::write(root.join("unblock.ns"), monotonic_ns().to_string()).unwrap();
        unsafe {
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
        }
        panic!("the worker survived a SIGTERM");
    }

    fn timeout_cleanup(root: &Path) {
        let argv = scripts(root, RECORDER);
        let specs: Vec<Spec<'_>> = argv.iter().map(|argv| spec(argv, root)).collect();
        let mut group = Group::launch(&specs, Duration::from_millis(200)).unwrap();
        let pids = children(root);
        let interrupt = Signaller::new(Duration::from_millis(600), libc::SIGINT);
        interrupt.arm();
        // Child 0 times out, is cancelled by the supervisor and ignores it:
        // its ordinary reap would wait until SIGINT switches to the budget.
        let error = group.value(0).unwrap_err();
        let returned_ns = monotonic_ns();
        assert_elapsed_after_signal(interrupt.sent_ns(), returned_ns);
        assert_eq!(error.reason, Refusal::Deadline);
        assert!(error.cleanup_error.is_none(), "{error}");
        let evidence = error.evidence.unwrap();
        assert!(evidence.killed && !evidence.reaped);
        assert_handed_off_note();
        for index in 1..3 {
            assert_interrupted_value(&mut group, index);
        }
        group.finish().unwrap();
        // A later SIGINT neither restarts the budget nor signals anything.
        let start = Instant::now();
        assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
        assert!(start.elapsed() < Duration::from_millis(100));
        assert_handed_off_note();
        settle_traps();
        for (index, &pid) in pids.0.iter().enumerate() {
            assert_eq!(terms(root, index), 1, "child {index} signalled again");
            assert!(
                unreaped(pid),
                "handed-off child {pid} was signalled or reaped"
            );
        }
        assert_no_launch_after_interrupt(root);
    }

    fn ordinary_reap(root: &Path) {
        let argv = scripts(root, RECORDER);
        let specs: Vec<Spec<'_>> = argv.iter().map(|argv| spec(argv, root)).collect();
        let group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
        let pids = children(root);
        let interrupt = Signaller::new(Duration::from_millis(300), libc::SIGINT);
        interrupt.arm();
        // Ordinary cleanup cancels all three, then waits without a bound
        // until SIGINT switches it to the one shared budget.
        drop(group);
        let returned_ns = monotonic_ns();
        assert_elapsed_after_signal(interrupt.sent_ns(), returned_ns);
        assert!(process::interrupted());
        assert_handed_off_note();
        settle_traps();
        for (index, &pid) in pids.0.iter().enumerate() {
            assert_eq!(terms(root, index), 1, "child {index} signalled again");
            assert!(
                unreaped(pid),
                "handed-off child {pid} was signalled or reaped"
            );
        }
    }

    fn ordinary_unbounded(root: &Path) {
        let argv = scripts(root, LINGERER);
        let specs: Vec<Spec<'_>> = argv.iter().map(|argv| spec(argv, root)).collect();
        let group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
        let pids = children(root);
        let start = Instant::now();
        drop(group);
        let elapsed = start.elapsed();
        assert!(
            elapsed > GRACE,
            "ordinary cleanup stopped waiting after {elapsed:?}"
        );
        for &pid in &pids.0 {
            let mut status = 0;
            assert_eq!(
                unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
        }
        assert!(!process::interrupted());
        assert_eq!(process::interrupt_cleanup_note(), None);
    }

    /// The parent's SIGINT/SIGTERM block around fork does not leak into the
    /// child, and the parent's own mask is unchanged afterwards.
    fn child_mask(root: &Path) {
        let query = || {
            let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, std::ptr::null(), &mut mask) },
                0
            );
            [libc::SIGINT, libc::SIGTERM].map(|signal| unsafe { libc::sigismember(&mask, signal) })
        };
        let before = query();
        for (signal, name) in [(libc::SIGINT, "INT"), (libc::SIGTERM, "TERM")] {
            let script = format!("kill -{name} $$; echo blocked");
            let args = argv(&["/bin/sh", "-c", &script]);
            let output = process::run(&spec(&args, root)).unwrap();
            assert_eq!(output.status, Status::Signal(signal));
            assert!(output.stdout.is_empty());
        }
        assert_eq!(query(), before);
        assert_eq!(before, [0, 0]);
    }

    #[test]
    fn worker() {
        let Some(root) = std::env::var_os(WORKER_ROOT) else {
            return;
        };
        let root = PathBuf::from(root);
        let mode = std::env::var(WORKER_MODE).unwrap();
        process::set_cancellation_signal(libc::SIGTERM).unwrap();
        process::install_termination_handler().unwrap();
        match mode.as_str() {
            "shared-budget" => shared_budget(&root),
            "second-during-reap" => second_during_reap(&root),
            "pending-second" => pending_second(&root),
            "timeout-cleanup" => timeout_cleanup(&root),
            "ordinary-reap" => ordinary_reap(&root),
            "ordinary-unbounded" => ordinary_unbounded(&root),
            "child-mask" => child_mask(&root),
            other => panic!("unknown worker mode {other}"),
        }
        fs::write(root.join("worker.completed"), mode.as_bytes()).unwrap();
    }

    fn run_worker(mode: &str) -> (PrivateDir, std::process::ExitStatus, String) {
        let dir = PrivateDir::new();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "interrupt_reap::worker",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(WORKER_ROOT, &dir.0)
            .env(WORKER_MODE, mode)
            .output()
            .unwrap();
        let text = format!(
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        (dir, output.status, text)
    }

    fn passes(mode: &str) {
        let (dir, status, text) = run_worker(mode);
        assert!(status.success(), "{text}");
        assert_eq!(
            fs::read(dir.0.join("worker.completed")).unwrap(),
            mode.as_bytes(),
            "{text}"
        );
    }

    // obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count
    #[test]
    fn interrupt_reap_budget_is_shared_bounded_and_reports_unreaped_count() {
        passes("shared-budget");
    }

    // obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_second_interrupt_during_reap_ends_all_remaining_waits
    #[test]
    fn second_interrupt_during_the_bounded_reap_ends_every_remaining_wait() {
        passes("second-during-reap");
    }

    // obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup
    #[test]
    fn signal_pending_at_the_bounded_wait_stops_it_before_any_reap_after_every_kill() {
        let (dir, status, text) = run_worker("pending-second");
        let died_ns = monotonic_ns();
        assert_eq!(status.signal(), Some(libc::SIGTERM), "{text}");
        let unblocked_ns: u64 = fs::read_to_string(dir.0.join("unblock.ns"))
            .unwrap()
            .parse()
            .unwrap();
        let elapsed = Duration::from_nanos(died_ns - unblocked_ns);
        assert!(elapsed < GRACE, "the worker waited {elapsed:?} for reaps");
        // Every kill was attempted although the second signal came first.
        let start = Instant::now();
        while (0..3).any(|index| terms(&dir.0, index) == 0) {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "a child never received its cancellation"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        settle_traps();
        for index in 0..3 {
            assert_eq!(terms(&dir.0, index), 1);
        }
    }

    // obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff
    #[test]
    fn interrupt_in_timeout_cleanup_never_restarts_the_budget_or_signals_a_handoff() {
        passes("timeout-cleanup");
    }

    // obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace
    #[test]
    fn interrupt_during_an_ordinary_reap_switches_to_the_shared_budget() {
        passes("ordinary-reap");
    }

    // obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_ordinary_and_system_exit_keep_unbounded_os_wait_parity
    #[test]
    fn ordinary_cleanup_without_an_interrupt_keeps_waiting_beyond_the_budget() {
        passes("ordinary-unbounded");
    }

    #[test]
    fn owned_children_start_with_the_callers_signal_mask() {
        passes("child-mask");
    }
}
