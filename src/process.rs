use crate::clock::{Deadline, monotonic_ns};
use std::ffi::{CString, OsStr, OsString};
use std::fmt;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd, IntoRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("agent-memory process supervision requires Linux or macOS");

// Owned-child termination registry (used only once a binary installs the handler).
//
// Invariants:
// - Only `spawn` registers, and only after `install_termination_handler`.
//   With SIGINT/SIGTERM blocked in the spawning thread it reserves a free slot
//   *before* fork (none free: typed refusal, no child is launched), stores the
//   PID right after fork, and only then unblocks. A termination signal thus
//   always sees each live owned direct child.
// - A slot holds 0 (free), RESERVED (fork pending, never signalled) or a PID.
//   It is cleared once its child's status is collected (by the supervisor or
//   by a handler), once waitpid reports ECHILD for it, or when the reserved
//   spawn fails.
// - The handlers signal a recorded PID only after `waitpid(pid, WNOHANG)`
//   returned 0: the PID then still names this process's unreaped child (an
//   already reaped PID yields ECHILD and is skipped), so a recycled PID is
//   never signalled. This holds because the binary supervises owned children
//   on one thread, which the handler interrupts.
// - The supervisor signals a child only with SIGINT/SIGTERM blocked
//   (`Owner::cancel`), so the returning SIGINT handler can never reap a PID
//   between the supervisor's liveness observation and its kill.
// - Only positive PIDs are signalled; never a process group.
//
// SIGINT returns (baseline `KeyboardInterrupt`): after killing and reaping it
// records the signal in INTERRUPTED; `spawn`, `run`, `run_for` and `Group`
// then refuse with `Refusal::Interrupted`, every guard on the stack drops,
// and the binary dies by SIGINT through `exit_if_interrupted`. SIGTERM (which
// the baseline leaves at its default) kills and reaps, then dies by SIGTERM
// at once: the baseline's death without cleanup, minus orphaned children.
//
// Capacity: the largest concurrent set of owned children any baseline call
// site launches is one `Group` of MAX_CHILDREN = 3 (repository_config's
// config/listed/ls-tree query batch); no call site spawns while a group is live.
const OWNED_SLOTS: usize = MAX_CHILDREN;
const RESERVED: i32 = -1;
static OWNED: [AtomicI32; OWNED_SLOTS] = [const { AtomicI32::new(0) }; OWNED_SLOTS];
static TERMINATION: AtomicBool = AtomicBool::new(false);
/// The SIGINT recorded by the returning handler; 0 while uninterrupted.
static INTERRUPTED: AtomicI32 = AtomicI32::new(0);
/// Self-pipe (both ends non-blocking, close-on-exec): the SIGINT handler
/// writes one byte so a supervision `poll` wakes even when the signal landed
/// just before the call. -1 until the handler is installed.
static WAKE_READ: AtomicI32 = AtomicI32::new(-1);
static WAKE_WRITE: AtomicI32 = AtomicI32::new(-1);

fn reserve_owned() -> Option<&'static AtomicI32> {
    OWNED.iter().find(|slot| {
        slot.compare_exchange(0, RESERVED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    })
}

fn release_owned(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if pid <= 0 || !TERMINATION.load(Ordering::SeqCst) {
        return;
    }
    for slot in &OWNED {
        let _ = slot.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Blocks SIGINT/SIGTERM in this thread until dropped.
struct TerminationBlock(libc::sigset_t);

impl TerminationBlock {
    fn new() -> io::Result<Self> {
        // sigset_t is plain data; sigemptyset initializes it before use.
        let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
        let mut previous: libc::sigset_t = unsafe { std::mem::zeroed() };
        let code = unsafe {
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGINT);
            libc::sigaddset(&mut set, libc::SIGTERM);
            libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut previous)
        };
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code));
        }
        Ok(Self(previous))
    }

    /// Blocks only once a binary installed the handler; otherwise no handler
    /// can interleave with the caller and nothing is blocked.
    fn when_installed() -> io::Result<Option<Self>> {
        if TERMINATION.load(Ordering::SeqCst) {
            Self::new().map(Some)
        } else {
            Ok(None)
        }
    }
}

impl Drop for TerminationBlock {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut());
        }
    }
}

/// The thread-local errno location (a plain pointer from libc).
fn errno_location() -> *mut libc::c_int {
    #[cfg(target_os = "macos")]
    let location = unsafe { libc::__error() };
    #[cfg(target_os = "linux")]
    let location = unsafe { libc::__errno_location() };
    location
}

/// Reads errno through the libc thread-local accessor: a plain load.
fn errno() -> libc::c_int {
    unsafe { *errno_location() }
}

/// waitpid retried on EINTR. Async-signal-safe: one syscall and an errno load
/// per iteration; no allocation, lock or formatting.
fn wait_retrying(pid: i32, options: libc::c_int) -> i32 {
    let mut status = 0;
    loop {
        let result = unsafe { libc::waitpid(pid, &mut status, options) };
        if result == -1 && errno() == libc::EINTR {
            continue;
        }
        return result;
    }
}

/// Shared by both handlers. Every step is async-signal-safe: static array
/// iteration, lock-free SeqCst atomics, waitpid, kill and errno loads; no
/// allocation, lock, formatting or Drop.
fn kill_and_reap_owned() {
    for slot in &OWNED {
        let pid = slot.load(Ordering::SeqCst);
        // Free (0) and RESERVED (-1) slots are never signalled.
        if pid <= 0 {
            continue;
        }
        // 0 proves `pid` is still our unreaped child. A PID return reaped an
        // exited child here; ECHILD means it was already reaped.
        if wait_retrying(pid, libc::WNOHANG) == 0 {
            // kill(2) on that positive PID only: never a process group.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
            // Blocking waitpid reaps it before the handler goes on.
            let _ = wait_retrying(pid, 0);
        }
        // Collected in every branch: the slot no longer names a live child.
        let _ = slot.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Restores the default disposition, unblocks `signal` and raises it, so the
/// process dies by that signal; `_exit(128 + signal)` only if it survives.
/// Async-signal-safe: sigaction, pthread_sigmask, raise and _exit only.
fn die_by_signal(signal: libc::c_int) -> ! {
    // sigaction and sigset_t are plain data; every field is set before use.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = libc::SIG_DFL;
    action.sa_flags = 0;
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(signal, &action, std::ptr::null_mut());
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, signal);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
        libc::raise(signal);
        // _exit(2) skips atexit handlers, stdio flushing and destructors.
        libc::_exit(128 + signal)
    }
}

/// SIGTERM: kill and reap owned children, then die by SIGTERM at once.
extern "C" fn terminate(signal: libc::c_int) {
    kill_and_reap_owned();
    die_by_signal(signal)
}

/// SIGINT: kill and reap owned children, record the signal, wake a pending
/// supervision poll and return, so the interrupted supervision refuses and
/// every guard drops normally. The interrupted code's errno is saved and
/// restored.
extern "C" fn interrupt(signal: libc::c_int) {
    let saved = errno();
    kill_and_reap_owned();
    INTERRUPTED.store(signal, Ordering::SeqCst);
    let wake = WAKE_WRITE.load(Ordering::SeqCst);
    if wake >= 0 {
        // write(2) is async-signal-safe; the end is non-blocking, and a full
        // pipe (EAGAIN) is already readable, so the result is irrelevant.
        unsafe {
            libc::write(wake, b"i".as_ptr().cast(), 1);
        }
    }
    unsafe {
        *errno_location() = saved;
    }
}

/// Creates the wake pipe once. Called before any handler exists, by the
/// binary's single thread, so no descriptor can leak to a concurrent fork.
fn create_wake_pipe() -> io::Result<()> {
    if WAKE_READ.load(Ordering::SeqCst) >= 0 {
        return Ok(());
    }
    let mut ends = [-1; 2];
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    for fd in ends {
        let set = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        if set < 0 || nonblocking(fd).is_err() {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(ends[0]);
                libc::close(ends[1]);
            }
            return Err(error);
        }
    }
    WAKE_READ.store(ends[0], Ordering::SeqCst);
    WAKE_WRITE.store(ends[1], Ordering::SeqCst);
    Ok(())
}

/// For a binary that supervises owned children on a single thread. SIGINT
/// kills and reaps every live owned direct child started through this module
/// and returns (see `interrupted`); SIGTERM does the same, then dies by SIGTERM.
///
/// A signal inherited as ignored stays ignored, as for the baseline
/// interpreter (and its children, which keep `SIG_IGN` across exec).
/// Handlers run without `SA_RESTART`: a blocking call returns EINTR, so the
/// supervision loop observes an interruption promptly.
pub fn install_termination_handler() -> io::Result<()> {
    create_wake_pipe()?;
    TERMINATION.store(true, Ordering::SeqCst);
    let handlers: [(libc::c_int, extern "C" fn(libc::c_int)); 2] =
        [(libc::SIGINT, interrupt), (libc::SIGTERM, terminate)];
    for (signal, handler) in handlers {
        // sigaction is plain data; the kernel fills `current`.
        let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
        if unsafe { libc::sigaction(signal, std::ptr::null(), &mut current) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if current.sa_sigaction == libc::SIG_IGN {
            continue;
        }
        // sigaction is plain data; every field is set before the call.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = handler as libc::sighandler_t;
        action.sa_flags = 0;
        let code = unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaddset(&mut action.sa_mask, libc::SIGINT);
            libc::sigaddset(&mut action.sa_mask, libc::SIGTERM);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if code != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// True once the SIGINT handler has run: every owned child is gone, and every
/// later spawn or supervision step refuses with `Refusal::Interrupted`.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst) != 0
}

/// After the interrupted work has unwound: die by the recorded SIGINT, as the
/// baseline does after an uncaught `KeyboardInterrupt`. Returns when
/// uninterrupted.
pub fn exit_if_interrupted() {
    let signal = INTERRUPTED.load(Ordering::SeqCst);
    if signal != 0 {
        die_by_signal(signal);
    }
}

fn interrupted_error() -> ProcessError {
    early_error(
        Refusal::Interrupted,
        io::Error::from_raw_os_error(libc::EINTR),
    )
}

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
    CaptureLimit {
        stream: Stream,
        limit: usize,
    },
    Infrastructure,
    /// Every termination-registry slot is in use; refused before fork.
    OwnedCapacity,
    /// SIGINT arrived: owned children were killed and reaped by the handler,
    /// and no further child is launched or supervised.
    Interrupted,
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
    // Reaped by the SIGINT handler (waitpid reports ECHILD once interrupted);
    // its status was collected there and is unknown here.
    collected: bool,
}

impl Owner {
    fn done(&self) -> bool {
        self.status.is_some() || self.collected
    }

    /// ECHILD after an interruption means the SIGINT handler reaped the child.
    fn collected_by_handler(&mut self, error: &io::Error) -> bool {
        if error.raw_os_error() != Some(libc::ECHILD) {
            return false;
        }
        release_owned(self.child.id());
        if interrupted() {
            self.collected = true;
        }
        self.collected
    }

    fn observe(&mut self) -> io::Result<u64> {
        if self.done() {
            return Ok(self.observed_ns.unwrap_or_else(monotonic_ns));
        }
        loop {
            match self.child.try_wait() {
                Ok(status) => {
                    let observed_ns = monotonic_ns();
                    self.status = status;
                    if status.is_some() {
                        self.observed_ns = Some(observed_ns);
                        release_owned(self.child.id());
                    }
                    return Ok(observed_ns);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    if self.collected_by_handler(&error) {
                        let observed_ns = monotonic_ns();
                        self.observed_ns = Some(observed_ns);
                        return Ok(observed_ns);
                    }
                    return Err(error);
                }
            }
        }
    }

    fn cancel(&mut self) -> io::Result<()> {
        // With SIGINT/SIGTERM blocked the handler cannot reap this child
        // between the observation below and the kill: an unreaped owned PID
        // cannot be reused, so the kill never reaches another process.
        let _block = TerminationBlock::when_installed()?;
        self.observe()?;
        if self.done() || self.killed {
            return Ok(());
        }
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
        if self.done() {
            return Ok(());
        }
        loop {
            match self.child.wait() {
                Ok(status) => {
                    self.status = Some(status);
                    self.observed_ns = Some(monotonic_ns());
                    release_owned(self.child.id());
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    if self.collected_by_handler(&error) {
                        self.observed_ns = Some(monotonic_ns());
                        return Ok(());
                    }
                    return Err(error);
                }
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
        if !self.done() {
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
    // Termination stays blocked from the slot reservation before fork until
    // the PID is recorded; a full registry refuses before any child exists.
    // A mask failure is an infrastructure fault, never a spawn failure that a
    // caller could read as "Git absent".
    let block = TerminationBlock::when_installed()
        .map_err(|error| early_error(Refusal::Infrastructure, error))?;
    // Checked while blocked: a SIGINT arriving after this check stays pending
    // until the PID below is recorded, so the handler then kills that child.
    if interrupted() {
        return Err(interrupted_error());
    }
    let slot = match block {
        Some(_) => match reserve_owned() {
            Some(slot) => Some(slot),
            None => {
                return Err(early_error(
                    Refusal::OwnedCapacity,
                    io::Error::other("Owned child termination registry is full; no child launched"),
                ));
            }
        },
        None => None,
    };
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(slot) = slot {
                slot.store(0, Ordering::SeqCst);
            }
            return Err(early_error(Refusal::Spawn, error));
        }
    };
    let launch_elapsed_ns = monotonic_ns().saturating_sub(start);
    if let Some(slot) = slot {
        // Unix PIDs are positive i32 values; anything else stays unsignalled (0).
        slot.store(
            i32::try_from(child.id())
                .ok()
                .filter(|pid| *pid > 0)
                .unwrap_or(0),
            Ordering::SeqCst,
        );
    }
    drop(block);
    let mut owner = Owner {
        child,
        status: None,
        observed_ns: None,
        killed: false,
        collected: false,
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

    /// One supervision step; once SIGINT was recorded, before or during the
    /// step (an EINTR, or ECHILD for a child the handler reaped), the step
    /// refuses as interrupted instead of reporting what it observed.
    fn advance(&mut self) -> std::result::Result<bool, (Refusal, Option<io::Error>)> {
        if interrupted() {
            return Err((Refusal::Interrupted, None));
        }
        let step = self.advance_once();
        if interrupted() {
            return Err((Refusal::Interrupted, None));
        }
        step
    }

    fn advance_once(&mut self) -> std::result::Result<bool, (Refusal, Option<io::Error>)> {
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
                reaped: self.owner.done(),
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
        let mut fds: [libc::pollfd; 3 * MAX_CHILDREN + 1] = std::array::from_fn(|_| libc::pollfd {
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
        // The SIGINT handler writes to this pipe: a signal landing between the
        // last `interrupted()` check and this poll still wakes it at once.
        let wake = WAKE_READ.load(Ordering::SeqCst);
        if wake >= 0 {
            fds[count] = libc::pollfd {
                fd: wake,
                events: libc::POLLIN,
                revents: 0,
            };
            count += 1;
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

/// An owned direct child. Launch data and borrowed descriptors may be dropped
/// immediately after `spawn_owned`; only argv0 and pending byte input survive.
/// As with `Group`, termination-registry capacity and signal ownership are
/// enforced by the existing spawn path.
pub struct OwnedProcess {
    active: Option<Active<'static>>,
    argv0: OsString,
    payload: Vec<u8>,
}

impl Active<'_> {
    // Moving fields changes only the input slice's lifetime, never OS ownership.
    fn with_payload<'a>(self, payload: &'a [u8]) -> Active<'a> {
        Active {
            owner: self.owner,
            input: self.input,
            stdout: self.stdout,
            stderr: self.stderr,
            payload,
            sent: self.sent,
            out: self.out,
            err: self.err,
            stdout_limit: self.stdout_limit,
            stderr_limit: self.stderr_limit,
            start: self.start,
            launch_elapsed_ns: self.launch_elapsed_ns,
            deadline: self.deadline,
            interval_deadline: self.interval_deadline,
            poll_wakeups: self.poll_wakeups,
        }
    }
}

/// Launch through the same registry, descriptor, deadline and exec machinery as
/// `Group`. No thread or detached supervisor is created.
pub fn spawn_owned(spec: &Spec<'_>) -> std::result::Result<OwnedProcess, ProcessError> {
    let active = spawn(spec, Timing::Absolute)?;
    let payload = active.payload.to_vec();
    // Successful spawn established a nonempty argv in the existing validator.
    let argv0 = spec.argv.first().expect("Spawn validated argv").clone();
    Ok(OwnedProcess {
        active: Some(active.with_payload(&[])),
        argv0,
        payload,
    })
}

impl OwnedProcess {
    /// Collect once, without widening the original absolute spawn deadline.
    /// All capture, timeout and collection errors retain Group's evidence.
    pub fn finish(mut self, deadline: Deadline) -> std::result::Result<Output, ProcessError> {
        let Some(mut active) = self.active.take() else {
            return Err(invalid("Owned child is already closed"));
        };
        active.deadline = active.deadline.min(deadline);
        let mut group = Group {
            queries: std::array::from_fn(|_| Query {
                argv0: OsStr::new(""),
                active: None,
                outcome: None,
                cleanup_error: None,
            }),
            len: 1,
            next: 0,
            failed: false,
            failure: None,
            exit_poll_interval: DEFAULT_EXIT_POLL_INTERVAL,
            closed: false,
        };
        group.queries[0] = Query {
            argv0: &self.argv0,
            active: Some(active.with_payload(&self.payload)),
            outcome: None,
            cleanup_error: None,
        };
        consume_one(group)
    }

    /// Cancel and reap only this still-owned child. Explicit close reports
    /// collection failures; Drop makes the same best-effort cleanup on unwind.
    pub fn close(&mut self) -> std::result::Result<(), ProcessError> {
        let Some(mut active) = self.active.take() else {
            return Ok(());
        };
        let cancellation = active.owner.stop().err();
        let pipes = active.close_pipes().err();
        if cancellation.is_some() || pipes.is_some() {
            return Err(active.refuse(
                self.argv0.clone(),
                Refusal::Infrastructure,
                cancellation,
                pipes,
            ));
        }
        Ok(())
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Nonblocking pipe output for a framed stream writer: a blocked recipient
/// never pins the writing thread, which rechecks its liveness between waits.
pub fn set_nonblocking(fd: BorrowedFd<'_>) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// One `write(2)`; `WouldBlock` reports a full nonblocking pipe.
pub fn write_some(fd: BorrowedFd<'_>, bytes: &[u8]) -> io::Result<usize> {
    loop {
        let count = unsafe { libc::write(fd.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
        if count >= 0 {
            return Ok(count as usize);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// Wait at most `timeout` for `fd` to accept output (or report an error/hangup).
pub fn wait_writable(fd: BorrowedFd<'_>, timeout: Duration) -> io::Result<()> {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLOUT,
        revents: 0,
    };
    let millis = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
    if unsafe { libc::poll(&mut poll, 1, millis) } == -1 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    Ok(())
}

/// Linux parent-process observation for the finite stream's native clock
/// qualification: a pidfd for exit detection and `/proc` reads, never signals.
#[cfg(target_os = "linux")]
pub mod parent {
    use std::fs::File;
    use std::io::{self, Read};
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    /// `(st_mode, st_size, st_dev, st_ino, st_mtime_ns, st_ctime_ns)`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Stat {
        pub mode: u32,
        pub size: u64,
        pub dev: u64,
        pub ino: u64,
        pub mtime_ns: i128,
        pub ctime_ns: i128,
    }
    fn of(meta: &std::fs::Metadata) -> Stat {
        Stat {
            mode: meta.mode(),
            size: meta.size(),
            dev: meta.dev(),
            ino: meta.ino(),
            mtime_ns: i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec()),
            ctime_ns: i128::from(meta.ctime()) * 1_000_000_000 + i128::from(meta.ctime_nsec()),
        }
    }
    pub fn parent_pid() -> i32 {
        std::os::unix::process::parent_id() as i32
    }
    pub fn own_pid() -> i32 {
        std::process::id() as i32
    }
    pub fn pidfd_open(pid: i32) -> io::Result<OwnedFd> {
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = i32::try_from(fd).map_err(|_| io::Error::other("pidfd out of range"))?;
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
    /// A pidfd becomes readable once its process exited.
    pub fn pidfd_exited(fd: BorrowedFd<'_>) -> io::Result<bool> {
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 0) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(poll.revents != 0)
    }
    /// At most `limit` bytes of a `/proc` pseudo-file (sizes there are 0).
    pub fn read_proc(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let mut raw = Vec::new();
        File::open(path)?.take(limit as u64).read_to_end(&mut raw)?;
        Ok(raw)
    }
    /// `stat(2)` following the final link (`/proc/<pid>/exe`, `/proc/<pid>/ns/*`).
    pub fn stat(path: &Path) -> io::Result<Stat> {
        std::fs::metadata(path).map(|meta| of(&meta))
    }
    /// Open `/proc/<pid>/exe` (the magic link must be followed).
    pub fn open_image(pid: i32) -> io::Result<File> {
        File::open(format!("/proc/{pid}/exe"))
    }
    pub fn fstat(file: &File) -> io::Result<Stat> {
        file.metadata().map(|meta| of(&meta))
    }
    pub fn read_block(file: &mut File, count: usize) -> io::Result<Vec<u8>> {
        let mut raw = vec![0u8; count];
        loop {
            match file.read(&mut raw) {
                Ok(size) => {
                    raw.truncate(size);
                    return Ok(raw);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    }
}
