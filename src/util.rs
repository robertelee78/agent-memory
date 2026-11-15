//! Errors, hashing, time, ids, files and child processes shared by every command.
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A refusal explained in plain words; `am` prints it and exits non-zero.
#[derive(Debug)]
pub struct Error(pub String);
impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Return early with a plain-words error.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::util::Error::new(format!($($arg)*))) };
}

/// Attach the path to an I/O failure.
pub fn io_at<T>(result: std::io::Result<T>, path: &Path) -> Result<T> {
    result.map_err(|e| Error::new(format!("{}: {e}", path.display())))
}

pub fn sha256_hex(raw: &[u8]) -> String {
    hex(&Sha256::digest(raw))
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A random UUIDv4 from the system source.
pub fn uuid4() -> String {
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        // Fall back to time and pid entropy; ids only need to be unique.
        let seed = format!("{:?}{}", std::time::SystemTime::now(), std::process::id());
        bytes.copy_from_slice(&Sha256::digest(seed.as_bytes())[..16]);
    }
    uuid_from(bytes)
}
/// A stable UUIDv4-shaped id derived from text (developer ids from e-mail).
pub fn uuid_for(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid_from(bytes)
}
fn uuid_from(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h = hex(&bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}
pub fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit() && !b.is_ascii_uppercase(),
        })
}

/// UTC now as `YYYY-MM-DDTHH:MM:SS.ffffff+00:00`, the bank's timestamp form.
pub fn now() -> String {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = elapsed.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Howard Hinnant's civil-from-days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:06}+00:00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        elapsed.subsec_micros()
    )
}

pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::new(format!("{}: {e}", path.display()))),
    }
}

/// Largest bank file `am` reads; longer files are cut at this size.
pub const MAX_READ: u64 = 1024 * 1024;

/// A regular file's first `MAX_READ` bytes; `None` when absent, a link or
/// not a regular file.
pub fn read_regular(path: &Path) -> Option<Vec<u8>> {
    if !path.symlink_metadata().ok()?.is_file() {
        return None;
    }
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_READ)
        .read_to_end(&mut raw)
        .ok()?;
    Some(raw)
}

/// The sibling temporary name `write_atomic` uses for `path`.
pub fn temp_for(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .unwrap_or(OsStr::new("file"))
        .to_string_lossy();
    parent.join(format!(".{name}.am-{}", uuid4()))
}

/// Replace `path` atomically: write a sibling temporary file, sync, rename.
pub fn write_atomic(path: &Path, raw: &[u8], mode: u32) -> Result<()> {
    write_atomic_via(path, &temp_for(path), raw, mode)
}

/// `write_atomic` through a temporary path the caller chose (and recorded).
pub fn write_atomic_via(path: &Path, temp: &Path, raw: &[u8], mode: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(temp)?;
        file.write_all(raw)?;
        file.sync_all()?;
        std::fs::rename(temp, path)
    };
    let result = write();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    io_at(result, path)
}

/// An exclusive advisory lock held until dropped.
pub struct Lock(#[allow(dead_code)] std::fs::File);

/// Lock a directory (or a lock file) exclusively, waiting for other `am` processes.
pub fn lock(path: &Path) -> Result<Lock> {
    let file = if path.is_dir() {
        io_at(std::fs::File::open(path), path)?
    } else {
        io_at(
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(path),
            path,
        )?
    };
    io_at(file.lock(), path)?;
    Ok(Lock(file))
}

/// Captured output of a child process.
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
impl Output {
    pub fn ok(&self) -> bool {
        self.code == 0
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_owned()
    }
    pub fn error_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_owned()
    }
}

/// Run a program, feeding `input` and capturing both streams.
pub fn run(
    program: &str,
    args: &[&OsStr],
    cwd: Option<&Path>,
    env: &[(&str, &OsStr)],
    input: Option<&[u8]>,
) -> Result<Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .map_err(|e| Error::new(format!("Cannot run {program}: {e}")))?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take().ok_or_else(|| Error::new("no stdin"))?;
        let data = input.to_vec();
        // Write from a thread so a child that fills its stdout cannot deadlock us.
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
        let output = child.wait_with_output()?;
        let _ = writer.join();
        return Ok(finish(output));
    }
    Ok(finish(child.wait_with_output()?))
}
fn finish(output: std::process::Output) -> Output {
    Output {
        code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

/// `which`: the first executable `name` on PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|path| {
                path.metadata()
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
    })
}

/// `$HOME` or the directory named by an XDG-style variable.
pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).components().collect())
        .ok_or_else(|| Error::new("HOME is not set"))
}
pub fn env_dir(key: &str, home_relative: &str) -> Result<PathBuf> {
    match std::env::var_os(key).filter(|v| !v.is_empty()) {
        Some(value) => Ok(PathBuf::from(value).components().collect()),
        None => Ok(home()?.join(home_relative)),
    }
}

/// Truncate text to at most `limit` bytes on a character boundary.
pub fn clip(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
