//! Injectable OS identity, randomness and UTC clock primitives.
use crate::error::{ContractError, Result};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
pub struct Sources {
    pub uuid4: fn() -> Result<String>,
    pub utc_now: fn() -> Result<String>,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            uuid4,
            utc_now: utc_now_isoformat,
        }
    }
}
/// Sixteen bytes from the kernel entropy call (no `/dev` path is opened).
#[cfg(target_os = "macos")]
fn entropy(bytes: &mut [u8; 16]) -> std::io::Result<()> {
    if unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
/// Sixteen bytes from the kernel entropy call (no `/dev` path is opened).
/// `getrandom` returns at most 256 bytes uninterrupted once the pool is
/// initialised; a short or interrupted result is retried.
#[cfg(target_os = "linux")]
fn entropy(bytes: &mut [u8; 16]) -> std::io::Result<()> {
    let mut filled = 0;
    while filled < bytes.len() {
        let remaining = &mut bytes[filled..];
        let count = unsafe { libc::getrandom(remaining.as_mut_ptr().cast(), remaining.len(), 0) };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        filled += count as usize;
    }
    Ok(())
}
/// Random UUIDv4 from the kernel entropy call.
pub fn uuid4() -> Result<String> {
    let mut bytes = [0u8; 16];
    if let Err(error) = entropy(&mut bytes) {
        return Err(ContractError::new(format!(
            "OS UUID entropy unavailable: {error}"
        )));
    }
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    // `hex` always has exactly 16 entries, one per entropy byte.
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex[0..4].concat(),
        hex[4..6].concat(),
        hex[6..8].concat(),
        hex[8..10].concat(),
        hex[10..16].concat()
    ))
}
pub fn isoformat_utc(secs: i64, micros: u32) -> Result<String> {
    if micros >= 1_000_000 {
        return Err(ContractError::new("Invalid UTC microseconds"));
    }
    let days = secs.div_euclid(86400);
    let seconds = secs.rem_euclid(86400);
    // Gregorian civil_from_days, with a March-based year and 400-year eras.
    let z = i128::from(days) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i128::from(month <= 2);
    if !(1..=9999).contains(&year) {
        return Err(ContractError::new("UTC date outside datetime year range"));
    }
    let fraction = if micros == 0 {
        String::new()
    } else {
        format!(".{micros:06}")
    };
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{fraction}+00:00",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    ))
}
pub fn utc_now_isoformat() -> Result<String> {
    let (seconds, micros) = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => (
            i64::try_from(d.as_secs()).map_err(|_| ContractError::new("UTC clock overflow"))?,
            d.subsec_micros(),
        ),
        Err(e) => {
            let d = e.duration();
            let seconds =
                i64::try_from(d.as_secs()).map_err(|_| ContractError::new("UTC clock overflow"))?;
            if d.subsec_nanos() == 0 {
                (-seconds, 0)
            } else {
                (-seconds - 1, (1_000_000_000 - d.subsec_nanos()) / 1000)
            }
        }
    };
    isoformat_utc(seconds, micros)
}
pub fn hostname() -> Result<String> {
    let mut bytes = [0u8; 1024];
    if unsafe { libc::gethostname(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return Err(ContractError::new(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let len = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| ContractError::new("Hostname exceeds its bound"))?;
    // `len` is a position inside `bytes`, so the prefix is in range.
    text(OsStr::from_bytes(&bytes[..len]), "hostname").map(str::to_owned)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidState {
    Alive,
    Gone,
    Denied,
}
pub fn pid_state(pid: i32) -> Result<PidState> {
    if pid <= 0 {
        return Err(ContractError::new("PID must be positive"));
    }
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(PidState::Alive);
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(PidState::Gone),
        Some(libc::EPERM) => Ok(PidState::Denied),
        _ => Err(ContractError::new(error.to_string())),
    }
}
/// Current effective UID for ownership checks; kept inside the OS boundary.
pub fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}

pub fn pid() -> u32 {
    std::process::id()
}

/// Home directory from the passwd database. Like CPython's `pwd`, any lookup
/// error is "no entry" (`KeyError`), never a distinct failure.
fn passwd_home(name: Option<&OsStr>) -> Option<PathBuf> {
    let name = match name.map(|name| std::ffi::CString::new(name.as_bytes())) {
        Some(Ok(name)) => Some(name),
        Some(Err(_)) => return None,
        None => None,
    };
    let mut size = 1024usize;
    loop {
        let mut buffer = vec![0u8; size];
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let code = unsafe {
            if let Some(name) = &name {
                libc::getpwnam_r(
                    name.as_ptr(),
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &mut result,
                )
            } else {
                libc::getpwuid_r(
                    libc::getuid(),
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &mut result,
                )
            }
        };
        if code == libc::ERANGE && size < 1024 * 1024 {
            size *= 2;
            continue;
        }
        if code != 0 || result.is_null() {
            return None;
        }
        let entry = unsafe { entry.assume_init() };
        if entry.pw_dir.is_null() {
            return None;
        }
        let home = unsafe { std::ffi::CStr::from_ptr(entry.pw_dir) }
            .to_bytes()
            .to_vec();
        return Some(PathBuf::from(OsString::from_vec(home)));
    }
}
fn no_home() -> ContractError {
    ContractError::new("Could not determine home directory.")
}
/// pathlib parsing: repeated separators, `.` parts and trailing separators vanish.
fn pathlib_parts(path: &Path) -> PathBuf {
    path.components()
        .filter(|part| *part != Component::CurDir)
        .collect()
}
/// `pathlib.Path(path).expanduser()`, the form every baseline caller uses.
pub fn expanduser(path: &Path) -> Result<PathBuf> {
    expanduser_with_home(path, std::env::var_os("HOME").as_deref())
}
/// Explicit HOME injection avoids mutating the process environment in tests.
pub fn expanduser_with_home(path: &Path, home: Option<&OsStr>) -> Result<PathBuf> {
    let parsed = pathlib_parts(path);
    let mut parts = parsed.components();
    let first = match parts.next() {
        Some(Component::Normal(first)) if first.as_bytes().first() == Some(&b'~') => first,
        _ => return Ok(parsed),
    };
    let user = first.as_bytes().get(1..).unwrap_or_default();
    // os.path.expanduser: HOME (even empty) for `~`, else the passwd entry.
    let home = if user.is_empty() {
        match home {
            Some(value) => PathBuf::from(value),
            None => passwd_home(None).ok_or_else(no_home)?,
        }
    } else {
        passwd_home(Some(OsStr::from_bytes(user))).ok_or_else(no_home)?
    };
    let trimmed_len = home
        .as_os_str()
        .as_bytes()
        .iter()
        .rposition(|c| *c != b'/')
        .map_or(0, |i| i + 1);
    let trimmed = home
        .as_os_str()
        .as_bytes()
        .get(..trimmed_len)
        .unwrap_or_default();
    let homedir = if trimmed.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(OsStr::from_bytes(trimmed))
    };
    if homedir.as_os_str().as_bytes().first() == Some(&b'~') {
        return Err(no_home());
    }
    let mut expanded = pathlib_parts(&homedir);
    expanded.extend(parts);
    Ok(expanded)
}

/// Native bytes remain native unless a boundary must record Unicode text.
pub fn text<'a>(value: &'a OsStr, what: &str) -> Result<&'a str> {
    value.to_str().ok_or_else(|| ContractError::non_utf8(what))
}
