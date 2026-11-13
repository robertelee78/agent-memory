use crate::clock::{Deadline, monotonic_ns};
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("agent-memory process supervision requires Linux or macOS");

pub enum Input<'a> {
    Null,
    Bytes(&'a [u8]),
    Descriptor(BorrowedFd<'a>),
}

pub enum Target<'a> {
    Capture { limit: usize },
    Descriptor(BorrowedFd<'a>),
}

pub struct Spec<'a> {
    pub argv: &'a [OsString],
    pub env: &'a [(OsString, OsString)],
    pub cwd: &'a Path,
    pub stdin: Input<'a>,
    pub stdout: Target<'a>,
    pub stderr: Target<'a>,
    pub deadline: Deadline,
    pub new_session: bool,
    /// Soft limit is min(bound, inherited hard); the hard limit is unchanged.
    pub file_size_limit: Option<u64>,
    pub umask: Option<u32>,
    /// These descriptors retain their numbers, as with Python's pass_fds.
    pub pass_fds: &'a [BorrowedFd<'a>],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Code(i32),
    Signal(i32),
}

impl From<ExitStatus> for Status {
    fn from(status: ExitStatus) -> Self {
        match status.code() {
            Some(code) => Self::Code(code),
            None => Self::Signal(status.signal().expect("Unix child has neither exit code nor signal")),
        }
    }
}

#[derive(Debug)]
pub struct Output {
    pub pid: u32,
    pub status: Status,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    InvalidInput,
    Spawn,
    Deadline,
    CaptureLimit { stream: Stream, limit: usize },
    Io,
}

#[derive(Debug)]
pub struct Evidence {
    pub argv0: OsString,
    pub pid: u32,
    pub elapsed_ns: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub killed: bool,
    pub reaped: bool,
    pub status: Option<Status>,
}

#[derive(Debug)]
pub struct ProcessError {
    pub reason: Refusal,
    pub evidence: Option<Evidence>,
    pub source: Option<io::Error>,
    pub cleanup_error: Option<io::Error>,
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Owned child refused: {:?}", self.reason)?;
        if let Some(evidence) = &self.evidence {
            write!(f, "; argv0={:?}; pid={}; elapsed_ns={}; stdout_bytes={}; stderr_bytes={}; killed={}; reaped={}",
                evidence.argv0, evidence.pid, evidence.elapsed_ns, evidence.stdout.len(),
                evidence.stderr.len(), evidence.killed, evidence.reaped)?;
        }
        if let Some(error) = &self.source {
            write!(f, "; {error}")?;
        }
        if let Some(error) = &self.cleanup_error {
            write!(f, "; cleanup: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|error| error as &(dyn std::error::Error + 'static))
    }
}

fn early_error(reason: Refusal, error: io::Error) -> ProcessError {
    ProcessError { reason, evidence: None, source: Some(error), cleanup_error: None }
}

fn invalid(message: &'static str) -> ProcessError {
    early_error(Refusal::InvalidInput, io::Error::new(io::ErrorKind::InvalidInput, message))
}

struct Owner {
    child: Child,
    status: Option<ExitStatus>,
    killed: bool,
}

impl Owner {
    fn observe(&mut self) -> io::Result<()> {
        if self.status.is_none() {
            self.status = self.child.try_wait()?;
        }
        Ok(())
    }

    fn stop(&mut self) -> io::Result<()> {
        self.observe()?;
        if self.status.is_some() {
            return Ok(());
        }
        // Until waitpid has reaped it, this owned PID cannot be reused.
        let kill_error = match self.child.kill() {
            Ok(()) => { self.killed = true; None }
            Err(error) => Some(error),
        };
        loop {
            match self.child.wait() {
                Ok(status) => { self.status = Some(status); break; }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        match kill_error {
            Some(error) if error.raw_os_error() != Some(libc::ESRCH) => Err(error),
            _ => Ok(()),
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = self.stop();
        }
    }
}

fn target_stdio(target: &Target<'_>) -> io::Result<Stdio> {
    match target {
        Target::Capture { .. } => Ok(Stdio::piped()),
        Target::Descriptor(fd) => Ok(Stdio::from(fd.try_clone_to_owned()?)),
    }
}

fn nonblocking(fd: i32) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn descriptor_buffer() -> io::Result<Vec<libc::proc_fdinfo>> {
    let bytes = unsafe {
        libc::proc_pidinfo(libc::getpid(), libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0)
    };
    if bytes <= 0 {
        return Err(io::Error::last_os_error());
    }
    let count = bytes as usize / std::mem::size_of::<libc::proc_fdinfo>() + 64;
    if count > i32::MAX as usize / std::mem::size_of::<libc::proc_fdinfo>() {
        return Err(io::Error::from_raw_os_error(libc::EOVERFLOW));
    }
    Ok((0..count).map(|_| libc::proc_fdinfo { proc_fd: -1, proc_fdtype: 0 }).collect())
}

#[cfg(target_os = "linux")]
unsafe fn cloexec_all() -> io::Result<()> {
    let code = unsafe {
        libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, libc::CLOSE_RANGE_CLOEXEC)
    };
    if code != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn cloexec_all(buffer: &mut [libc::proc_fdinfo]) -> io::Result<()> {
    // libproc's PIDLISTFDS wrapper is a direct proc_info syscall; storage is
    // allocated before fork. A full snapshot is refused, never partly applied.
    let capacity = std::mem::size_of_val(buffer);
    let bytes = unsafe {
        libc::proc_pidinfo(libc::getpid(), libc::PROC_PIDLISTFDS, 0,
            buffer.as_mut_ptr().cast(), capacity as i32)
    };
    if bytes <= 0 {
        return Err(io::Error::last_os_error());
    }
    if bytes as usize >= capacity || bytes as usize % std::mem::size_of::<libc::proc_fdinfo>() != 0 {
        return Err(io::Error::from_raw_os_error(libc::EOVERFLOW));
    }
    for entry in &buffer[..bytes as usize / std::mem::size_of::<libc::proc_fdinfo>()] {
        if entry.proc_fd >= 3 && unsafe { libc::fcntl(entry.proc_fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn drain<R: Read>(pipe: &mut Option<R>, bytes: &mut Vec<u8>, limit: usize, stream: Stream)
    -> std::result::Result<(), (Refusal, Option<io::Error>)>
{
    let Some(reader) = pipe.as_mut() else { return Ok(()); };
    let mut buffer = [0u8; 16_384];
    match reader.read(&mut buffer) {
        Ok(0) => { *pipe = None; Ok(()) }
        Ok(count) => {
            let keep = count.min(limit.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&buffer[..keep]);
            if keep != count {
                Err((Refusal::CaptureLimit { stream, limit }, None))
            } else {
                Ok(())
            }
        }
        Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => Ok(()),
        Err(error) => Err((Refusal::Io, Some(error))),
    }
}

/// Runs exactly one direct child. No process group is ever signalled and no
/// worker threads are created. All three pipe directions share one poll loop.
pub fn run(spec: &Spec<'_>) -> std::result::Result<Output, ProcessError> {
    let Some(argv0) = spec.argv.first() else { return Err(invalid("Empty argv")); };
    if argv0.is_empty() || !Path::new(argv0).is_absolute() || !spec.cwd.is_absolute() {
        return Err(invalid("Executable and cwd must be explicit absolute paths"));
    }
    if spec.umask.is_some_and(|mask| mask > 0o777) {
        return Err(invalid("Umask exceeds permission bits"));
    }
    for (index, fd) in spec.pass_fds.iter().enumerate() {
        if fd.as_raw_fd() <= 2 || spec.pass_fds[..index].iter().any(|other| other.as_raw_fd() == fd.as_raw_fd()) {
            return Err(invalid("Pass-through descriptors must be distinct and greater than 2"));
        }
    }
    for (index, (key, _)) in spec.env.iter().enumerate() {
        if key.is_empty() || spec.env[..index].iter().any(|(other, _)| other == key) {
            return Err(invalid("Environment keys must be nonempty and distinct"));
        }
    }
    if spec.deadline.expired() {
        return Err(early_error(Refusal::Deadline, io::Error::new(io::ErrorKind::TimedOut, "Deadline elapsed before spawn; no child created")));
    }
    let start = monotonic_ns();
    let mut command = Command::new(argv0);
    command.args(&spec.argv[1..]).env_clear().envs(spec.env.iter().map(|(key, value)| (key, value)))
        .current_dir(spec.cwd);
    let stdin = match &spec.stdin {
        Input::Null => Stdio::null(),
        Input::Bytes(_) => Stdio::piped(),
        Input::Descriptor(fd) => Stdio::from(fd.try_clone_to_owned().map_err(|error| early_error(Refusal::Spawn, error))?),
    };
    command.stdin(stdin)
        .stdout(target_stdio(&spec.stdout).map_err(|error| early_error(Refusal::Spawn, error))?)
        .stderr(target_stdio(&spec.stderr).map_err(|error| early_error(Refusal::Spawn, error))?);
    let new_session = spec.new_session;
    let mask = spec.umask;
    let bound = spec.file_size_limit;
    let passed: Vec<i32> = spec.pass_fds.iter().map(AsRawFd::as_raw_fd).collect();
    #[cfg(target_os = "macos")]
    let mut fd_buffer = descriptor_buffer().map_err(|error| early_error(Refusal::Spawn, error))?;
    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            cloexec_all()?;
            #[cfg(target_os = "macos")]
            cloexec_all(&mut fd_buffer)?;
            for &raw in &passed {
                let flags = libc::fcntl(raw, libc::F_GETFD);
                if flags < 0 || libc::fcntl(raw, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            if new_session && libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if let Some(bound) = bound {
                let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
                if libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
                limit.rlim_cur = bound.min(limit.rlim_max);
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            if let Some(mask) = mask {
                libc::umask(mask as libc::mode_t);
            }
            Ok(())
        });
    }
    let child = command.spawn().map_err(|error| early_error(Refusal::Spawn, error))?;
    let mut owner = Owner { child, status: None, killed: false };
    let pid = owner.child.id();
    let mut input = owner.child.stdin.take();
    let mut stdout = owner.child.stdout.take();
    let mut stderr = owner.child.stderr.take();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let payload = match &spec.stdin { Input::Bytes(bytes) => *bytes, _ => &[] };
    let mut sent = 0;
    let stdout_limit = match spec.stdout { Target::Capture { limit } => limit, _ => 0 };
    let stderr_limit = match spec.stderr { Target::Capture { limit } => limit, _ => 0 };
    let operation = (|| -> std::result::Result<(), (Refusal, Option<io::Error>)> {
        for fd in [input.as_ref().map(AsRawFd::as_raw_fd), stdout.as_ref().map(AsRawFd::as_raw_fd), stderr.as_ref().map(AsRawFd::as_raw_fd)].into_iter().flatten() {
            nonblocking(fd).map_err(|error| (Refusal::Io, Some(error)))?;
        }
        loop {
            owner.observe().map_err(|error| (Refusal::Io, Some(error)))?;
            if owner.status.is_some() && stdout.is_none() && stderr.is_none() {
                return Ok(());
            }
            if spec.deadline.expired() {
                return Err((Refusal::Deadline, None));
            }
            drain(&mut stdout, &mut out, stdout_limit, Stream::Stdout)?;
            drain(&mut stderr, &mut err, stderr_limit, Stream::Stderr)?;
            if sent == payload.len() || owner.status.is_some() {
                input = None;
            } else if let Some(writer) = input.as_mut() {
                let end = payload.len().min(sent.saturating_add(16_384));
                match writer.write(&payload[sent..end]) {
                    Ok(0) => return Err((Refusal::Io, Some(io::Error::new(io::ErrorKind::WriteZero, "Child stdin write returned zero")))),
                    Ok(count) => { sent += count; }
                    Err(error) if error.kind() == io::ErrorKind::BrokenPipe => { input = None; }
                    Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {}
                    Err(error) => return Err((Refusal::Io, Some(error))),
                }
            }
            if sent == payload.len() {
                input = None;
            }
            let mut fds = [
                libc::pollfd { fd: stdout.as_ref().map_or(-1, AsRawFd::as_raw_fd), events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: stderr.as_ref().map_or(-1, AsRawFd::as_raw_fd), events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: input.as_ref().map_or(-1, AsRawFd::as_raw_fd), events: libc::POLLOUT, revents: 0 },
            ];
            // Periodic waitpid polling also observes children that close both
            // output pipes before exit. Rounding up never busy-spins below 1ms.
            let ns = spec.deadline.remaining().as_nanos();
            let millis = ns.div_ceil(1_000_000).min(10) as i32;
            let code = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, millis) };
            if code < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err((Refusal::Io, Some(error)));
                }
            }
        }
    })();
    if let Err((reason, source)) = operation {
        drop(input.take());
        let cleanup_error = owner.stop().err();
        return Err(ProcessError {
            reason, source, cleanup_error,
            evidence: Some(Evidence {
                argv0: argv0.clone(), pid, elapsed_ns: monotonic_ns().saturating_sub(start),
                stdout: out, stderr: err, killed: owner.killed, reaped: owner.status.is_some(),
                status: owner.status.map(Status::from),
            }),
        });
    }
    Ok(Output {
        pid, status: Status::from(owner.status.expect("Successful supervision reaps the child")),
        stdout: out, stderr: err, elapsed_ns: monotonic_ns().saturating_sub(start),
    })
}

/// Signal zero observes liveness only; EPERM is not evidence of a dead owner.
pub fn pid_is_alive(pid: i32) -> io::Result<bool> {
    if pid <= 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "PID must be positive"));
    }
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) { Ok(false) } else { Err(error) }
}

