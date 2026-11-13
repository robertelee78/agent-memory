use crate::clock::{Deadline, monotonic_ns};
use std::ffi::{CString, OsStr, OsString};
use std::fmt;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd, IntoRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::Duration;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("agent-memory process supervision requires Linux or macOS");

pub enum Input<'a> {
    /// Inherit descriptor zero as-is, including a closed descriptor.
    Inherit,
    Null,
    Bytes(&'a [u8]),
    Descriptor(BorrowedFd<'a>),
}

pub enum Target<'a> {
    Capture { limit: usize },
    Descriptor(BorrowedFd<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSize {
    /// Release downloads may raise the inherited soft limit, never the hard limit.
    SoftCap(u64),
    /// Raw readers preserve every finite inherited bound and pin both limits.
    Pinned(u64),
}

pub struct Spec<'a> {
    pub argv: &'a [OsString],
    pub search_path: bool,
    pub env: &'a [(OsString, OsString)],
    /// None inherits the parent's cwd without a chdir or getcwd call.
    pub cwd: Option<&'a Path>,
    pub stdin: Input<'a>,
    pub stdout: Target<'a>,
    pub stderr: Target<'a>,
    pub deadline: Deadline,
    pub new_session: bool,
    pub file_size_limit: Option<FileSize>,
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
            None => Self::Signal(
                status
                    .signal()
                    .expect("Unix child has neither exit code nor signal"),
            ),
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
    pub launch_elapsed_ns: u64,
    pub poll_wakeups: u64,
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
    Infrastructure,
}

#[derive(Debug)]
pub struct Evidence {
    pub argv0: OsString,
    pub pid: u32,
    pub elapsed_ns: u64,
    pub launch_elapsed_ns: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub killed: bool,
    pub reaped: bool,
    pub status: Option<Status>,
    pub poll_wakeups: u64,
}

#[derive(Debug)]
pub struct ProcessError {
    pub reason: Refusal,
    pub evidence: Option<Evidence>,
    pub source: Option<io::Error>,
    pub cleanup_error: Option<io::Error>,
    pub incomplete: bool,
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Owned child refused: {:?}", self.reason)?;
        if let Some(evidence) = &self.evidence {
            write!(
                f,
                "; argv0={:?}; pid={}; elapsed_ns={}; stdout_bytes={}; stderr_bytes={}; killed={}; reaped={}",
                evidence.argv0,
                evidence.pid,
                evidence.elapsed_ns,
                evidence.stdout.len(),
                evidence.stderr.len(),
                evidence.killed,
                evidence.reaped
            )?;
        }
        if let Some(error) = &self.source {
            write!(f, "; {error}")?;
        }
        if let Some(error) = &self.cleanup_error {
            write!(f, "; cleanup: {error}")?;
        }
        if self.incomplete {
            write!(f, "; Incomplete ordered child query observation")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

fn early_error(reason: Refusal, error: io::Error) -> ProcessError {
    ProcessError {
        reason,
        evidence: None,
        source: Some(error),
        cleanup_error: None,
        incomplete: false,
    }
}

fn invalid(message: &'static str) -> ProcessError {
    early_error(
        Refusal::InvalidInput,
        io::Error::new(io::ErrorKind::InvalidInput, message),
    )
}

struct Owner {
    child: Child,
    status: Option<ExitStatus>,
    // First observed reap time, never an inferred actual child-exit instant.
    observed_ns: Option<u64>,
    killed: bool,
}

impl Owner {
    fn observe(&mut self) -> io::Result<u64> {
        if self.status.is_some() {
            return Ok(self.observed_ns.unwrap_or_else(monotonic_ns));
        }
        loop {
            match self.child.try_wait() {
                Ok(status) => {
                    let observed_ns = monotonic_ns();
                    self.status = status;
                    if status.is_some() {
                        self.observed_ns = Some(observed_ns);
                    }
                    return Ok(observed_ns);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    }

    fn cancel(&mut self) -> io::Result<()> {
        self.observe()?;
        if self.status.is_some() || self.killed {
            return Ok(());
        }
        // Until waitpid has reaped it, this owned PID cannot be reused.
        match self.child.kill() {
            Ok(()) => {
                self.killed = true;
                Ok(())
            }
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn reap(&mut self) -> io::Result<()> {
        if self.status.is_some() {
            return Ok(());
        }
        loop {
            match self.child.wait() {
                Ok(status) => {
                    self.status = Some(status);
                    self.observed_ns = Some(monotonic_ns());
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    }

    fn stop(&mut self) -> io::Result<()> {
        let cancellation = self.cancel();
        self.reap().and(cancellation)
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

/// Spare `proc_fdinfo` slots (8 bytes each) beyond the descriptors seen before fork.
#[cfg(target_os = "macos")]
const DESCRIPTOR_HEADROOM: usize = 4096;

#[cfg(target_os = "macos")]
fn descriptor_buffer() -> io::Result<Vec<libc::proc_fdinfo>> {
    let bytes = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDLISTFDS,
            0,
            std::ptr::null_mut(),
            0,
        )
    };
    if bytes <= 0 {
        return Err(io::Error::last_os_error());
    }
    // Other threads may open descriptors between this sizing and the child's
    // snapshot after fork. The headroom makes that overflow implausible; the
    // child still refuses (EOVERFLOW) rather than apply a partial snapshot.
    let count = bytes as usize / std::mem::size_of::<libc::proc_fdinfo>() + DESCRIPTOR_HEADROOM;
    if count > i32::MAX as usize / std::mem::size_of::<libc::proc_fdinfo>() {
        return Err(io::Error::from_raw_os_error(libc::EOVERFLOW));
    }
    Ok((0..count)
        .map(|_| libc::proc_fdinfo {
            proc_fd: -1,
            proc_fdtype: 0,
        })
        .collect())
}

#[cfg(target_os = "linux")]
unsafe fn cloexec_all(passed: &[i32]) -> io::Result<()> {
    let code = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    if code == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOSYS | libc::EINVAL | libc::EPERM) => unsafe { cloexec_proc_fds(passed) },
        _ => Err(error),
    }
}

#[cfg(target_os = "linux")]
unsafe fn cloexec_proc_fds(passed: &[i32]) -> io::Result<()> {
    let directory = unsafe {
        libc::open(
            c"/proc/self/fd".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if directory < 0 {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut buffer = [0u8; 4096];
        loop {
            let count = unsafe {
                libc::syscall(
                    libc::SYS_getdents64,
                    directory,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                )
            };
            if count < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if count == 0 {
                return Ok(());
            }
            let count = count as usize;
            if count > buffer.len() {
                return Err(io::Error::from_raw_os_error(libc::EIO));
            }
            let mut offset = 0;
            while offset < count {
                // linux_dirent64 has a 19-byte fixed header on both supported
                // architectures; d_reclen at byte 16 includes the trailing NUL.
                if count - offset < 20 {
                    return Err(io::Error::from_raw_os_error(libc::EIO));
                }
                let length =
                    u16::from_ne_bytes([buffer[offset + 16], buffer[offset + 17]]) as usize;
                if length < 20 || length > count - offset {
                    return Err(io::Error::from_raw_os_error(libc::EIO));
                }
                let name = &buffer[offset + 19..offset + length];
                let Some(end) = name.iter().position(|&byte| byte == 0) else {
                    return Err(io::Error::from_raw_os_error(libc::EIO));
                };
                let name = &name[..end];
                offset += length;
                if name.is_empty() || !name.iter().all(u8::is_ascii_digit) {
                    continue;
                }
                let mut fd = 0i32;
                for &digit in name {
                    fd = fd
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(i32::from(digit - b'0')))
                        .ok_or_else(|| io::Error::from_raw_os_error(libc::EOVERFLOW))?;
                }
                if fd >= 3 && fd != directory && !passed.contains(&fd) {
                    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
                    if flags < 0
                        || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                }
            }
        }
    })();
    let closed = unsafe { libc::close(directory) };
    if result.is_ok() && closed != 0 {
        return Err(io::Error::last_os_error());
    }
    result
}

#[cfg(target_os = "macos")]
unsafe fn cloexec_all(buffer: &mut [libc::proc_fdinfo]) -> io::Result<()> {
    // libproc's PIDLISTFDS wrapper is a direct proc_info syscall; storage is
    // allocated before fork. A full snapshot is refused, never partly applied.
    let capacity = std::mem::size_of_val(buffer);
    let bytes = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDLISTFDS,
            0,
            buffer.as_mut_ptr().cast(),
            capacity as i32,
        )
    };
    if bytes <= 0 {
        return Err(io::Error::last_os_error());
    }
    if bytes as usize >= capacity || bytes as usize % std::mem::size_of::<libc::proc_fdinfo>() != 0
    {
        return Err(io::Error::from_raw_os_error(libc::EOVERFLOW));
    }
    for entry in &buffer[..bytes as usize / std::mem::size_of::<libc::proc_fdinfo>()] {
        if entry.proc_fd >= 3
            && unsafe { libc::fcntl(entry.proc_fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn close_pipe<T: IntoRawFd>(pipe: Option<T>) -> io::Result<()> {
    if let Some(pipe) = pipe {
        if unsafe { libc::close(pipe.into_raw_fd()) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn drain<R: Read + IntoRawFd>(
    pipe: &mut Option<R>,
    bytes: &mut Vec<u8>,
    limit: usize,
    stream: Stream,
) -> std::result::Result<(), (Refusal, Option<io::Error>)> {
    let Some(reader) = pipe.as_mut() else {
        return Ok(());
    };
    let mut buffer = [0u8; 16_384];
    match reader.read(&mut buffer) {
        Ok(0) => close_pipe(pipe.take()).map_err(|error| (Refusal::Infrastructure, Some(error))),
        Ok(count) => {
            let keep = count.min(limit.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&buffer[..keep]);
            if keep != count {
                Err((Refusal::CaptureLimit { stream, limit }, None))
            } else {
                Ok(())
            }
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(())
        }
        Err(error) => Err((Refusal::Infrastructure, Some(error))),
    }
}

struct CStringArray {
    strings: Vec<CString>,
    pointers: Vec<*const libc::c_char>,
}

// Every pointer refers to immutable heap storage owned by strings. Neither
// vector is modified after construction; moving the owner does not move bytes.
unsafe impl Send for CStringArray {}
unsafe impl Sync for CStringArray {}

impl CStringArray {
    fn new(strings: Vec<CString>) -> Self {
        let mut pointers = Vec::with_capacity(strings.len() + 1);
        pointers.extend(strings.iter().map(|value| value.as_ptr()));
        pointers.push(std::ptr::null());
        Self { strings, pointers }
    }
}

struct PreparedExec {
    argv: CStringArray,
    env: CStringArray,
    // Empty means the absolute executable is argv[0], without another copy.
    candidates: Vec<CString>,
}

fn nul_error(_: std::ffi::NulError) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "NUL byte in process argv or environment",
    )
}

impl PreparedExec {
    fn new(spec: &Spec<'_>, argv0: &OsStr) -> io::Result<Self> {
        let argv = spec
            .argv
            .iter()
            .map(|arg| CString::new(arg.as_bytes()).map_err(nul_error))
            .collect::<io::Result<Vec<_>>>()?;
        let env = spec
            .env
            .iter()
            .map(|(key, value)| {
                let mut bytes = Vec::with_capacity(key.len() + 1 + value.len());
                bytes.extend_from_slice(key.as_bytes());
                bytes.push(b'=');
                bytes.extend_from_slice(value.as_bytes());
                CString::new(bytes).map_err(nul_error)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let candidates = if Path::new(argv0).is_absolute() {
            Vec::new()
        } else {
            let path = spec
                .env
                .iter()
                .find(|(key, _)| key == "PATH")
                .map_or(b"/bin:/usr/bin".as_slice(), |(_, value)| value.as_bytes());
            path.split(|&byte| byte == b':')
                .map(|directory| {
                    let candidate = Path::new(OsStr::from_bytes(directory)).join(argv0);
                    CString::new(candidate.as_os_str().as_bytes()).map_err(nul_error)
                })
                .collect::<io::Result<Vec<_>>>()?
        };
        Ok(Self {
            argv: CStringArray::new(argv),
            env: CStringArray::new(env),
            candidates,
        })
    }

    unsafe fn execute(&self) -> io::Error {
        if self.candidates.is_empty() {
            unsafe {
                libc::execve(
                    self.argv.strings[0].as_ptr(),
                    self.argv.pointers.as_ptr(),
                    self.env.pointers.as_ptr(),
                );
            }
            return io::Error::last_os_error();
        }
        let mut first_error = None;
        let mut last_error = libc::ENOENT;
        for candidate in &self.candidates {
            unsafe {
                libc::execve(
                    candidate.as_ptr(),
                    self.argv.pointers.as_ptr(),
                    self.env.pointers.as_ptr(),
                );
            }
            let errno = io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO);
            last_error = errno;
            if errno != libc::ENOENT && errno != libc::ENOTDIR && first_error.is_none() {
                first_error = Some(errno);
            }
        }
        io::Error::from_raw_os_error(first_error.unwrap_or(last_error))
    }
}

#[derive(Clone, Copy)]
enum Timing {
    Absolute,
    FromLaunch(Duration),
}

fn spawn<'a>(spec: &Spec<'a>, timing: Timing) -> std::result::Result<Active<'a>, ProcessError> {
    let Some(argv0) = spec.argv.first() else {
        return Err(invalid("Empty argv"));
    };
    if argv0.is_empty()
        || spec.cwd.is_some_and(|cwd| !cwd.is_absolute())
        || (!Path::new(argv0).is_absolute()
            && (!spec.search_path || argv0.as_bytes().contains(&b'/')))
    {
        return Err(invalid(
            "Cwd and slash-containing executables must be absolute; bare names require search_path",
        ));
    }
    if spec.umask.is_some_and(|mask| mask > 0o777) {
        return Err(invalid("Umask exceeds permission bits"));
    }
    for (index, fd) in spec.pass_fds.iter().enumerate() {
        if fd.as_raw_fd() <= 2
            || spec.pass_fds[..index]
                .iter()
                .any(|other| other.as_raw_fd() == fd.as_raw_fd())
        {
            return Err(invalid(
                "Pass-through descriptors must be distinct and greater than 2",
            ));
        }
    }
    for (index, (key, _)) in spec.env.iter().enumerate() {
        if key.is_empty() || key.as_bytes().iter().any(|&byte| byte == b'=' || byte == 0) {
            return Err(invalid("illegal environment variable name"));
        }
        if spec.env[..index].iter().any(|(other, _)| other == key) {
            return Err(invalid("Environment keys must be distinct"));
        }
    }
    if matches!(timing, Timing::Absolute) && spec.deadline.expired() {
        return Err(early_error(
            Refusal::Deadline,
            io::Error::new(
                io::ErrorKind::TimedOut,
                "Deadline elapsed before spawn; no child created",
            ),
        ));
    }
    let start = monotonic_ns();
    let executable =
        PreparedExec::new(spec, argv0).map_err(|error| early_error(Refusal::Spawn, error))?;
    let mut command = Command::new(argv0);
    command.env_clear();
    if let Some(cwd) = spec.cwd {
        command.current_dir(cwd);
    }
    let stdin = match &spec.stdin {
        Input::Inherit => Stdio::inherit(),
        Input::Null => Stdio::null(),
        Input::Bytes(_) => Stdio::piped(),
        Input::Descriptor(fd) => Stdio::from(
            fd.try_clone_to_owned()
                .map_err(|error| early_error(Refusal::Spawn, error))?,
        ),
    };
    command
        .stdin(stdin)
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
            cloexec_all(&passed)?;
            #[cfg(target_os = "macos")]
            cloexec_all(&mut fd_buffer)?;
            for &raw in &passed {
                let flags = libc::fcntl(raw, libc::F_GETFD);
                if flags < 0 || libc::fcntl(raw, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            if libc::signal(libc::SIGXFSZ, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(io::Error::last_os_error());
            }
            if new_session && libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if let Some(bound) = bound {
                let mut limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
                match bound {
                    FileSize::SoftCap(bound) => {
                        limit.rlim_cur = if limit.rlim_max == libc::RLIM_INFINITY {
                            bound
                        } else {
                            bound.min(limit.rlim_max)
                        };
                    }
                    FileSize::Pinned(bound) => {
                        let mut cap = bound;
                        for previous in [limit.rlim_cur, limit.rlim_max] {
                            if previous != libc::RLIM_INFINITY {
                                cap = cap.min(previous);
                            }
                        }
                        limit.rlim_cur = cap;
                        limit.rlim_max = cap;
                    }
                }
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            if let Some(mask) = mask {
                libc::umask(mask as libc::mode_t);
            }
            Err(executable.execute())
        });
    }
    let child = command
        .spawn()
        .map_err(|error| early_error(Refusal::Spawn, error))?;
    let launch_elapsed_ns = monotonic_ns().saturating_sub(start);
    let mut owner = Owner {
        child,
        status: None,
        observed_ns: None,
        killed: false,
    };
    let input = owner.child.stdin.take();
    let stdout = owner.child.stdout.take();
    let stderr = owner.child.stderr.take();
    let mut active = Active {
        owner,
        input,
        stdout,
        stderr,
        start,
        launch_elapsed_ns,
        deadline: spec.deadline,
        interval_deadline: matches!(timing, Timing::FromLaunch(_)),
        payload: match &spec.stdin {
            Input::Bytes(bytes) => *bytes,
            _ => &[],
        },
        sent: 0,
        out: Vec::new(),
        err: Vec::new(),
        poll_wakeups: 0,
        stdout_limit: match spec.stdout {
            Target::Capture { limit } => limit,
            _ => 0,
        },
        stderr_limit: match spec.stderr {
            Target::Capture { limit } => limit,
            _ => 0,
        },
    };
    if let Timing::FromLaunch(timeout) = timing {
        match Deadline::after(timeout) {
            Ok(deadline) => active.deadline = deadline,
            Err(_) => {
                return Err(active.refuse(
                    argv0.clone(),
                    Refusal::InvalidInput,
                    Some(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Child deadline overflows",
                    )),
                    None,
                ));
            }
        }
    }
    for fd in active.fds() {
        if fd.fd >= 0 {
            if let Err(error) = nonblocking(fd.fd) {
                return Err(active.refuse(
                    argv0.clone(),
                    Refusal::Infrastructure,
                    Some(error),
                    None,
                ));
            }
        }
    }
    Ok(active)
}

struct Active<'a> {
    owner: Owner,
    input: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    payload: &'a [u8],
    sent: usize,
    out: Vec<u8>,
    err: Vec<u8>,
    stdout_limit: usize,
    stderr_limit: usize,
    start: u64,
    launch_elapsed_ns: u64,
    deadline: Deadline,
    // Launch-relative queries refuse completion first observed at the deadline.
    interval_deadline: bool,
    poll_wakeups: u64,
}

impl Active<'_> {
    fn complete(&self) -> bool {
        self.owner.status.is_some() && self.stdout.is_none() && self.stderr.is_none()
    }

    fn advance(&mut self) -> std::result::Result<bool, (Refusal, Option<io::Error>)> {
        if self.interval_deadline && self.deadline.expired() {
            return Err((Refusal::Deadline, None));
        }
        let observed_ns = self
            .owner
            .observe()
            .map_err(|error| (Refusal::Infrastructure, Some(error)))?;
        if self.interval_deadline && observed_ns >= self.deadline.as_ns() {
            return Err((Refusal::Deadline, None));
        }
        if self.complete() {
            return Ok(true);
        }
        if self.deadline.expired() {
            return Err((Refusal::Deadline, None));
        }
        drain(
            &mut self.stdout,
            &mut self.out,
            self.stdout_limit,
            Stream::Stdout,
        )?;
        drain(
            &mut self.stderr,
            &mut self.err,
            self.stderr_limit,
            Stream::Stderr,
        )?;
        if self.sent == self.payload.len() || self.owner.status.is_some() {
            close_pipe(self.input.take())
                .map_err(|error| (Refusal::Infrastructure, Some(error)))?;
        } else if let Some(writer) = self.input.as_mut() {
            let end = self.payload.len().min(self.sent.saturating_add(16_384));
            match writer.write(&self.payload[self.sent..end]) {
                Ok(0) => {
                    return Err((
                        Refusal::Infrastructure,
                        Some(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "Child stdin write returned zero",
                        )),
                    ));
                }
                Ok(count) => self.sent += count,
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                    close_pipe(self.input.take())
                        .map_err(|error| (Refusal::Infrastructure, Some(error)))?;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err((Refusal::Infrastructure, Some(error))),
            }
        }
        if self.sent == self.payload.len() {
            close_pipe(self.input.take())
                .map_err(|error| (Refusal::Infrastructure, Some(error)))?;
        }
        if self.interval_deadline && self.deadline.expired() {
            return Err((Refusal::Deadline, None));
        }
        Ok(self.complete())
    }

    fn fds(&self) -> [libc::pollfd; 3] {
        [
            libc::pollfd {
                fd: self.stdout.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.stderr.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.input.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLOUT,
                revents: 0,
            },
        ]
    }

    fn close_pipes(&mut self) -> io::Result<()> {
        let input = close_pipe(self.input.take());
        let stdout = close_pipe(self.stdout.take());
        let stderr = close_pipe(self.stderr.take());
        input.and(stdout).and(stderr)
    }

    fn finish(mut self, argv0: &OsStr) -> std::result::Result<Output, ProcessError> {
        if let Err(error) = self.close_pipes() {
            return Err(self.refuse(
                argv0.to_os_string(),
                Refusal::Infrastructure,
                Some(error),
                None,
            ));
        }
        Ok(Output {
            pid: self.owner.child.id(),
            status: Status::from(
                self.owner
                    .status
                    .expect("Completed supervision reaps the child"),
            ),
            stdout: std::mem::take(&mut self.out),
            stderr: std::mem::take(&mut self.err),
            elapsed_ns: monotonic_ns().saturating_sub(self.start),
            launch_elapsed_ns: self.launch_elapsed_ns,
            poll_wakeups: self.poll_wakeups,
        })
    }

    fn refuse(
        mut self,
        argv0: OsString,
        reason: Refusal,
        source: Option<io::Error>,
        prior_cleanup: Option<io::Error>,
    ) -> ProcessError {
        let input_error = close_pipe(self.input.take()).err();
        let cancellation_error = self.owner.stop().err();
        let close_error = self.close_pipes().err();
        ProcessError {
            reason,
            source,
            cleanup_error: prior_cleanup
                .or(input_error)
                .or(cancellation_error)
                .or(close_error),
            incomplete: false,
            evidence: Some(Evidence {
                argv0,
                pid: self.owner.child.id(),
                elapsed_ns: monotonic_ns().saturating_sub(self.start),
                launch_elapsed_ns: self.launch_elapsed_ns,
                stdout: std::mem::take(&mut self.out),
                stderr: std::mem::take(&mut self.err),
                killed: self.owner.killed,
                reaped: self.owner.status.is_some(),
                status: self.owner.status.map(Status::from),
                poll_wakeups: self.poll_wakeups,
            }),
        }
    }
}

struct Query<'a> {
    argv0: &'a OsStr,
    active: Option<Active<'a>>,
    outcome: Option<std::result::Result<Output, ProcessError>>,
    cleanup_error: Option<io::Error>,
}

const MAX_CHILDREN: usize = 3;

pub const DEFAULT_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// One invoking owner supervises one to three children. `launch` replaces each
/// Spec's absolute deadline with the supplied interval starting at its own
/// successful spawn return; `run` retains the Spec's absolute deadline.
/// No live PID or descriptor accessor is exposed.
pub struct Group<'a> {
    queries: [Query<'a>; MAX_CHILDREN],
    len: usize,
    next: usize,
    failed: bool,
    failure: Option<io::Error>,
    exit_poll_interval: Duration,
    closed: bool,
}

fn copy_io_error(error: &io::Error) -> io::Error {
    match error.raw_os_error() {
        Some(errno) => io::Error::from_raw_os_error(errno),
        None => io::Error::new(error.kind(), error.to_string()),
    }
}

impl<'a> Group<'a> {
    pub fn launch(
        specs: &[Spec<'a>],
        timeout: Duration,
    ) -> std::result::Result<Self, ProcessError> {
        if Deadline::after(timeout).is_err() {
            return Err(invalid("Group timeout overflows monotonic nanoseconds"));
        }
        Self::launch_with(specs, Timing::FromLaunch(timeout))
    }

    fn launch_with(specs: &[Spec<'a>], timing: Timing) -> std::result::Result<Self, ProcessError> {
        if specs.is_empty() || specs.len() > MAX_CHILDREN {
            return Err(invalid("A child group requires one to three commands"));
        }
        let queries = std::array::from_fn(|index| Query {
            argv0: specs
                .get(index)
                .and_then(|spec| spec.argv.first())
                .map_or(OsStr::new(""), |arg| arg.as_os_str()),
            active: None,
            outcome: None,
            cleanup_error: None,
        });
        let mut group = Self {
            queries,
            len: specs.len(),
            next: 0,
            failed: false,
            failure: None,
            exit_poll_interval: DEFAULT_EXIT_POLL_INTERVAL,
            closed: false,
        };
        for (index, spec) in specs.iter().enumerate() {
            group.sweep();
            if group.failed {
                break;
            }
            match spawn(spec, timing) {
                Ok(active) => group.queries[index].active = Some(active),
                Err(error) => {
                    let fault = if error.reason == Refusal::Infrastructure {
                        Some(
                            error
                                .source
                                .as_ref()
                                .map(copy_io_error)
                                .unwrap_or_else(|| io::Error::other("Child pipe setup failed")),
                        )
                    } else {
                        None
                    };
                    group.queries[index].outcome = Some(Err(error));
                    if let Some(fault) = fault {
                        group.fail(fault, Some(index));
                        break;
                    }
                }
            }
            group.sweep();
        }
        Ok(group)
    }

    fn cancel_all(&mut self) {
        // Cancel every child before waiting for any one of them.
        for query in &mut self.queries {
            if let Some(active) = &mut query.active {
                let error = active.owner.cancel().err();
                if query.cleanup_error.is_none() {
                    query.cleanup_error = error;
                }
            }
        }
        for query in &mut self.queries {
            if let Some(active) = &mut query.active {
                let error = active.close_pipes().err();
                if query.cleanup_error.is_none() {
                    query.cleanup_error = error;
                }
            }
        }
    }

    fn fail(&mut self, fault: io::Error, pending: Option<usize>) {
        self.failed = true;
        let barrier = (self.next..self.len)
            .find(|&index| self.queries[index].outcome.is_none() || pending == Some(index))
            .unwrap_or(self.len);
        self.failure = Some(copy_io_error(&fault));
        self.cancel_all();
        for query in &mut self.queries[barrier..self.len] {
            let source = Some(copy_io_error(&fault));
            let error = if let Some(active) = query.active.take() {
                active.refuse(
                    query.argv0.to_os_string(),
                    Refusal::Infrastructure,
                    source,
                    query.cleanup_error.take(),
                )
            } else {
                let evidence = match query.outcome.take() {
                    Some(Ok(output)) => Some(Evidence {
                        argv0: query.argv0.to_os_string(),
                        pid: output.pid,
                        elapsed_ns: output.elapsed_ns,
                        launch_elapsed_ns: output.launch_elapsed_ns,
                        stdout: output.stdout,
                        stderr: output.stderr,
                        killed: false,
                        reaped: true,
                        status: Some(output.status),
                        poll_wakeups: output.poll_wakeups,
                    }),
                    Some(Err(mut error)) => {
                        query.cleanup_error = error.cleanup_error.take();
                        error.evidence
                    }
                    None => None,
                };
                ProcessError {
                    reason: Refusal::Infrastructure,
                    evidence,
                    source,
                    cleanup_error: query.cleanup_error.take(),
                    incomplete: false,
                }
            };
            query.outcome = Some(Err(error));
        }
    }

    fn sweep(&mut self) {
        if self.failed {
            return;
        }
        for index in self.next..self.len {
            let query = &mut self.queries[index];
            let Some(active) = query.active.as_mut() else {
                continue;
            };
            match active.advance() {
                Ok(false) => {}
                Ok(true) => {
                    let active = query.active.take().expect("Active query was just observed");
                    let outcome = active.finish(query.argv0);
                    let fault = outcome
                        .as_ref()
                        .err()
                        .and_then(|error| error.source.as_ref())
                        .map(copy_io_error);
                    query.outcome = Some(outcome);
                    if let Some(fault) = fault {
                        self.fail(fault, Some(index));
                        return;
                    }
                }
                Err((Refusal::Infrastructure, source)) => {
                    self.fail(
                        source.unwrap_or_else(|| io::Error::other("Child supervision failed")),
                        Some(index),
                    );
                    return;
                }
                Err((reason, source)) => {
                    let active = query.active.take().expect("Active query was just observed");
                    let error = active.refuse(
                        query.argv0.to_os_string(),
                        reason,
                        source,
                        query.cleanup_error.take(),
                    );
                    let fault = error.cleanup_error.as_ref().map(copy_io_error);
                    query.outcome = Some(Err(error));
                    if let Some(fault) = fault {
                        self.fail(fault, Some(index));
                        return;
                    }
                }
            }
        }
    }

    fn poll(&mut self) {
        let mut fds: [libc::pollfd; 3 * MAX_CHILDREN] = std::array::from_fn(|_| libc::pollfd {
            fd: -1,
            events: 0,
            revents: 0,
        });
        let mut count = 0;
        let mut remaining = Duration::MAX;
        let mut pending = false;
        for query in &mut self.queries[self.next..self.len] {
            if let Some(active) = &mut query.active {
                pending = true;
                remaining = remaining.min(active.deadline.remaining());
                if active.stdout.is_none()
                    && active.stderr.is_none()
                    && active.owner.status.is_none()
                {
                    remaining = remaining.min(self.exit_poll_interval);
                }
                active.poll_wakeups = active.poll_wakeups.saturating_add(1);
                for fd in active.fds() {
                    if fd.fd >= 0 {
                        fds[count] = fd;
                        count += 1;
                    }
                }
            }
        }
        if !pending {
            return;
        }
        // A closed-pipe child still needs bounded waitpid sweeps; round positive
        // intervals up to a millisecond instead of spinning below 1ms.
        let millis = remaining
            .as_nanos()
            .div_ceil(1_000_000)
            .min(i32::MAX as u128) as i32;
        let code = unsafe { libc::poll(fds.as_mut_ptr(), count as libc::nfds_t, millis) };
        if code < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                self.fail(error, None);
            }
        } else if fds[..count]
            .iter()
            .any(|fd| fd.revents & libc::POLLNVAL != 0)
        {
            self.fail(io::Error::from_raw_os_error(libc::EBADF), None);
        }
    }

    /// Each outcome is consumed exactly once, in launch order. Command errors
    /// do not cancel siblings. Supervision faults preserve the terminal prefix
    /// before the first pending query and invalidate that query and its suffix.
    pub fn value(&mut self, index: usize) -> std::result::Result<Output, ProcessError> {
        if index != self.next || index >= self.len {
            return Err(invalid(
                "Child outcomes must be consumed once in original order",
            ));
        }
        loop {
            self.sweep();
            if let Some(outcome) = self.queries[index].outcome.take() {
                self.next += 1;
                return outcome;
            }
            self.poll();
        }
    }

    /// True once any supervision fault has invalidated a pending query.
    pub fn failed(&self) -> bool {
        self.failed
    }

    pub fn set_exit_poll_interval(
        &mut self,
        interval: Duration,
    ) -> std::result::Result<(), ProcessError> {
        if interval.is_zero() {
            return Err(invalid("Exit poll interval must be positive"));
        }
        self.exit_poll_interval = interval;
        Ok(())
    }

    fn cleanup(&mut self) -> Option<io::Error> {
        self.cancel_all();
        let mut first = None;
        for query in &mut self.queries {
            if let Some(active) = &mut query.active {
                let error = active.owner.reap().err();
                if query.cleanup_error.is_none() {
                    query.cleanup_error = error;
                }
            }
            if let Some(error) = query.cleanup_error.take() {
                if first.is_none() {
                    first = Some(error);
                }
            }
        }
        self.closed = true;
        first
    }

    pub fn finish(mut self) -> std::result::Result<(), ProcessError> {
        let cleanup_error = self.cleanup();
        let incomplete = self.next != self.len;
        if self.failure.is_some() || cleanup_error.is_some() || incomplete {
            let source = self.failure.take().or_else(|| {
                if incomplete {
                    Some(io::Error::other(
                        "Incomplete ordered child query observation",
                    ))
                } else {
                    None
                }
            });
            return Err(ProcessError {
                reason: Refusal::Infrastructure,
                evidence: None,
                source,
                cleanup_error,
                incomplete,
            });
        }
        Ok(())
    }
}

impl Drop for Group<'_> {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.cleanup();
        }
    }
}

/// Runs one direct child with exactly the same supervision machinery as Group,
/// retaining the caller's absolute monotonic deadline.
pub fn run(spec: &Spec<'_>) -> std::result::Result<Output, ProcessError> {
    consume_one(Group::launch_with(
        std::slice::from_ref(spec),
        Timing::Absolute,
    )?)
}

/// Starts the timeout at this child's successful spawn return, as communicate
/// does in the baseline. Launch time is still included in elapsed_ns.
pub fn run_for(spec: &Spec<'_>, timeout: Duration) -> std::result::Result<Output, ProcessError> {
    consume_one(Group::launch(std::slice::from_ref(spec), timeout)?)
}

fn consume_one(mut group: Group<'_>) -> std::result::Result<Output, ProcessError> {
    match (group.value(0), group.finish()) {
        (Err(mut error), Err(finish)) => {
            error.cleanup_error = error.cleanup_error.or(finish.cleanup_error);
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(finish)) => Err(finish),
        (Ok(output), Ok(())) => Ok(output),
    }
}

/// Signal zero observes liveness only; EPERM is not evidence of a dead owner.
pub fn pid_is_alive(pid: i32) -> io::Result<bool> {
    if pid <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PID must be positive",
        ));
    }
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod inherited_git_context_tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    use std::path::PathBuf;

    // Re-exec keeps cwd/fd-zero changes out of the parallel test runner.
    #[test]
    fn git_inherited_context_worker() {
        let Some(repo) = std::env::var_os("AM_TEST_INHERITED_GIT_REPO") else {
            return;
        };
        let repo = PathBuf::from(repo);
        let mode = std::env::var("AM_TEST_INHERITED_GIT_MODE").unwrap();
        if mode != "closed-stdin" {
            let removed = repo.parent().unwrap().join("removed-cwd");
            std::fs::create_dir(&removed).unwrap();
            std::env::set_current_dir(&removed).unwrap();
            std::fs::remove_dir(&removed).unwrap();
            assert!(std::env::current_dir().is_err());
        }
        if mode != "deleted-cwd" {
            // This is an isolated re-exec worker; no other thread uses stdin.
            let closed = unsafe { libc::close(0) };
            assert!(
                closed == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EBADF)
            );
            assert_eq!(unsafe { libc::fcntl(0, libc::F_GETFD) }, -1);
        }
        let observed = crate::identity_git::root_head(&repo).unwrap();
        assert_eq!(observed.0.as_deref(), Some(repo.as_os_str()));
        assert_eq!(observed.1, None);
        let specs = vec![(
            vec![
                OsString::from("rev-parse"),
                OsString::from("--show-toplevel"),
            ],
            false,
        )];
        crate::identity_git::_git_group(&repo, &specs, |group| {
            assert_eq!(group.value(0, false)?.as_deref(), Some(repo.as_os_str()));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn git_inherits_deleted_cwd_and_closed_stdin_without_optional_absence() {
        struct PrivateDir(PathBuf);
        impl Drop for PrivateDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        for mode in ["deleted-cwd", "closed-stdin", "both"] {
            let path = PathBuf::from(format!(
                "/tmp/am-inherited-git-{}-{}-{mode}",
                std::process::id(),
                monotonic_ns()
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            let root = PrivateDir(std::fs::canonicalize(path).unwrap());
            let repo = root.0.join("repo");
            std::fs::create_dir(&repo).unwrap();
            let init = crate::identity_git::_run(
                &repo,
                &[
                    OsString::from("init"),
                    OsString::from("--initial-branch=main"),
                ],
                false,
            )
            .unwrap();
            assert_eq!(init.status, Status::Code(0));
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::inherited_git_context_tests::git_inherited_context_worker",
                    "--nocapture",
                ])
                .env("AM_TEST_INHERITED_GIT_REPO", &repo)
                .env("AM_TEST_INHERITED_GIT_MODE", mode)
                .output()
                .unwrap();
            assert!(
                child.status.success(),
                "{mode}: stdout={} stderr={}",
                String::from_utf8_lossy(&child.stdout),
                String::from_utf8_lossy(&child.stderr)
            );
        }
    }
}
