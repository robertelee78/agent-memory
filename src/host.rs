//! Injectable OS identity, randomness and UTC clock primitives.
use crate::error::{ContractError, Result};
use std::io::Read;
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
pub fn uuid4() -> Result<String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|e| ContractError::new(format!("OS UUID entropy unavailable: {e}")))?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
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
    use std::os::unix::ffi::OsStrExt;
    text(std::ffi::OsStr::from_bytes(&bytes[..len]), "hostname").map(str::to_owned)
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

fn passwd(
    name: Option<&std::ffi::OsStr>,
) -> Result<Option<(std::ffi::OsString, std::path::PathBuf)>> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let name = name
        .map(|name| std::ffi::CString::new(name.as_bytes()))
        .transpose()
        .map_err(|_| ContractError::new("NUL in user name"))?;
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
        if code != 0 {
            return Err(ContractError::new(
                std::io::Error::from_raw_os_error(code).to_string(),
            ));
        }
        if result.is_null() {
            return Ok(None);
        }
        let entry = unsafe { entry.assume_init() };
        if entry.pw_name.is_null() || entry.pw_dir.is_null() {
            return Err(ContractError::new("Malformed passwd entry"));
        }
        let user = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) }
            .to_bytes()
            .to_vec();
        let home = unsafe { std::ffi::CStr::from_ptr(entry.pw_dir) }
            .to_bytes()
            .to_vec();
        return Ok(Some((
            std::ffi::OsString::from_vec(user),
            std::path::PathBuf::from(std::ffi::OsString::from_vec(home)),
        )));
    }
}
pub fn current_username() -> Result<Option<std::ffi::OsString>> {
    Ok(passwd(None)?.map(|(name, _)| name))
}
pub fn expanduser(path: &std::path::Path) -> Result<std::path::PathBuf> {
    expanduser_with_home(path, std::env::var_os("HOME").as_deref())
}
/// Explicit HOME injection avoids mutating the process environment in tests.
pub fn expanduser_with_home(
    path: &std::path::Path,
    home: Option<&std::ffi::OsStr>,
) -> Result<std::path::PathBuf> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    if bytes.first() != Some(&b'~') {
        return Ok(path.to_path_buf());
    }
    let end = bytes.iter().position(|c| *c == b'/').unwrap_or(bytes.len());
    let home = if end == 1 {
        match home {
            Some(value) => value.to_owned(),
            None => match passwd(None)? {
                Some((_, home)) => home.into_os_string(),
                None => return Ok(path.to_path_buf()),
            },
        }
    } else {
        match passwd(Some(std::ffi::OsStr::from_bytes(&bytes[1..end])))? {
            Some((_, home)) => home.into_os_string(),
            None => return Ok(path.to_path_buf()),
        }
    };
    let trimmed = home
        .as_bytes()
        .iter()
        .rposition(|c| *c != b'/')
        .map_or(0, |i| i + 1);
    let mut expanded = home.as_bytes()[..trimmed].to_vec();
    expanded.extend_from_slice(&bytes[end..]);
    if expanded.is_empty() {
        expanded.push(b'/');
    }
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_vec(
        expanded,
    )))
}

/// Native bytes remain native unless a boundary must record Unicode text.
pub fn text<'a>(value: &'a std::ffi::OsStr, what: &str) -> Result<&'a str> {
    value.to_str().ok_or_else(|| ContractError::non_utf8(what))
}
