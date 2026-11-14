//! Private, bounded hook failure observations; absence never proves completion.
use crate::clock::Deadline;
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::ContractError;
use crate::fs::{self, Descriptor, DirectoryChain, FlockGuard, LockMode, RenameMode};
use crate::host;
use crate::process::{self, Input, Spec, Status, Target};
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SCHEMA: &str = "agent-memory/hook-failure-v1";
pub const STAGES: &[&str] = &["event", "repository", "identity", "boundary", "guidance"];
pub const REASONS: &[&str] = &[
    "event.invalid",
    "repository.unavailable",
    "identity.unresolved",
    "boundary.missing",
    "boundary.unsafe",
    "boundary.source_mismatch",
    "state.unavailable",
    "guidance.unavailable",
    "internal.unexpected",
];
pub const ADAPTERS: &[(&str, &str)] = &[("codex", "prompt"), ("codex", "stop")];
pub const MAX: usize = 4096;
const GIT_OUTPUT_MAX: usize = 65536;

#[derive(Debug)]
pub enum Error {
    NoRepository(ContractError),
    Contract(ContractError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRepository(e) | Self::Contract(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for Error {}
impl From<ContractError> for Error {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
fn failure(message: impl Into<String>) -> Error {
    Error::Contract(ContractError::new(message))
}
fn io_error(e: impl fmt::Display) -> Error {
    failure(e.to_string())
}

/// Resolve the repository identity and its private, per-adapter slot.
pub fn paths(repo: &Path, harness: &str, adapter: &str) -> Result<(PathBuf, String)> {
    if !ADAPTERS.contains(&(harness, adapter)) {
        return Err(failure("Unsupported hook adapter"));
    }
    let repo = fs::canonical_path(repo, true)?;
    let argv = vec![
        OsString::from("git"),
        OsString::from("-C"),
        repo.as_os_str().to_owned(),
        OsString::from("rev-parse"),
        OsString::from("--git-common-dir"),
    ];
    let env: Vec<_> = std::env::vars_os().collect();
    let spec = Spec {
        argv: &argv,
        search_path: true,
        env: &env,
        cwd: None,
        stdin: Input::Inherit,
        stdout: Target::Capture {
            limit: GIT_OUTPUT_MAX,
        },
        stderr: Target::Capture {
            limit: GIT_OUTPUT_MAX,
        },
        deadline: Deadline::after(Duration::from_secs(5))?,
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    let output = process::run(&spec).map_err(|e| {
        if e.source
            .as_ref()
            .is_some_and(|s| s.kind() == std::io::ErrorKind::NotFound)
        {
            Error::NoRepository(ContractError::new("Git unavailable for hook diagnostics"))
        } else {
            io_error(e)
        }
    })?;
    if output.status != Status::Code(0) {
        return Err(Error::NoRepository(ContractError::new(
            "Repository unavailable for hook diagnostics",
        )));
    }
    let common_text = std::str::from_utf8(&output.stdout)
        .map_err(io_error)?
        .trim();
    let common = fs::canonical_path(&repo.join(common_text), true)?;
    let raw = std::env::var_os("AGENT_MEMORY_STATE_HOME").filter(|s| !s.is_empty());
    let root = match raw {
        Some(value) => PathBuf::from(value),
        None => {
            let state = match std::env::var_os("XDG_STATE_HOME") {
                Some(value) => PathBuf::from(value),
                None => host::expanduser(Path::new("~/.local/state"))?,
            };
            state.join("agent-memory")
        }
    };
    let root = host::expanduser(&root)?;
    let resolved = fs::canonical_path(&root, false)?;
    if !root.is_absolute()
        || resolved.starts_with(&repo)
        || resolved.starts_with(&common)
        || (common.file_name() == Some(OsStr::new(".git"))
            && common.parent().is_some_and(|p| resolved.starts_with(p)))
    {
        return Err(failure(
            "Hook diagnostics require private state outside the repository",
        ));
    }
    let directory = root.join("hook-failures");
    fs::trust::inspect(&directory, fs::trust::InspectOptions::default())?;
    let common_text = host::text(common.as_os_str(), "Git common directory")?;
    let repo_text = host::text(repo.as_os_str(), "repository path")?;
    let mut hash = Sha256::new();
    for (index, part) in [common_text, repo_text, harness, adapter]
        .iter()
        .enumerate()
    {
        if index != 0 {
            hash.update(b"\0");
        }
        hash.update(part.as_bytes());
    }
    Ok((
        directory,
        format!("{}.json", crate::validation_memo::hex(&hash.finalize())),
    ))
}

// The sole writer is record(): UTC datetime.isoformat(), with six fractional
// digits exactly when microseconds are nonzero. Other fromisoformat forms are
// a declared compatibility delta, not a second timestamp grammar.
fn iso_timestamp(text: &str) -> bool {
    let Some(clock) = text.strip_suffix("+00:00") else {
        return false;
    };
    let (whole, fraction) = match clock.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (clock, None),
    };
    if whole.len() != 19
        || whole.as_bytes().get(10) != Some(&b'T')
        || fraction.is_some_and(|value| {
            value.len() != 6
                || !value.bytes().all(|byte| byte.is_ascii_digit())
                || value == "000000"
        })
    {
        return false;
    }
    crate::lifecycle::time_value(&Value::String(format!("{whole}Z"))).is_ok()
}

pub fn validate<'a>(value: &'a Value, harness: &str, adapter: &str) -> Result<&'a Value> {
    let invalid = || failure("Invalid hook failure metadata");
    let fields = value.as_object().ok_or_else(invalid)?;
    let names = [
        "schema",
        "harness",
        "adapter",
        "stage",
        "reason",
        "timestamp",
    ];
    if fields.len() != names.len()
        || fields
            .iter()
            .any(|(key, value)| !names.contains(&key.as_str()) || value.as_str().is_none())
        || value.get("schema").and_then(Value::as_str) != Some(SCHEMA)
        || value.get("harness").and_then(Value::as_str) != Some(harness)
        || value.get("adapter").and_then(Value::as_str) != Some(adapter)
        || !STAGES.contains(&value.get("stage").and_then(Value::as_str).unwrap_or(""))
        || !REASONS.contains(&value.get("reason").and_then(Value::as_str).unwrap_or(""))
        || value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .count()
            > 40
    {
        return Err(invalid());
    }
    if !iso_timestamp(
        value
            .get("timestamp")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?,
    ) {
        return Err(failure("Invalid hook failure timestamp"));
    }
    Ok(value)
}

fn safe_file(info: fs::Identity) -> bool {
    info.is_regular()
        && info.uid == host::effective_uid()
        && info.nlink == 1
        && info.mode & 0o077 == 0
}

fn read_opened(
    directory: &Descriptor,
    name: &OsStr,
    content: bool,
) -> Result<Option<(Vec<u8>, fs::Identity)>> {
    let mut file = match directory.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let info = file.stat().map_err(io_error)?;
    if !safe_file(info) || (content && info.size > MAX as i64) {
        return Err(failure("Unsafe hook failure metadata"));
    }
    let bytes = if content {
        file.read_bounded(MAX).map_err(io_error)?
    } else {
        file.read_prefix(MAX + 1).map_err(io_error)?
    };
    let named = directory.stat_at(name).map_err(io_error)?;
    if named != info {
        return Err(failure("Unsafe hook failure metadata"));
    }
    Ok(Some((bytes, info)))
}

fn verify_slot(directory: &Descriptor, name: &OsStr, expected: Option<fs::Identity>) -> Result<()> {
    match (directory.stat_at(name), expected) {
        (Err(e), None) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(actual), Some(expected)) if actual == expected => Ok(()),
        _ => Err(failure("Hook failure metadata changed")),
    }
}
/// Read the bounded raw prefix used when replacing or clearing an existing slot.
pub fn read_raw(directory: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let chain = match DirectoryChain::open(directory) {
        Ok(chain) => chain,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    if chain
        .directory()
        .stat_at(OsStr::new(name))
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(None);
    }
    fs::trust::state(directory)?;
    let bytes = read_opened(chain.directory(), OsStr::new(name), false)?.map(|(bytes, _)| bytes);
    chain.recheck().map_err(io_error)?;
    Ok(bytes)
}

/// Read a single slot, returning its validated JSON or absence.
pub fn read_one(
    directory: &Path,
    name: &str,
    harness: &str,
    adapter: &str,
) -> Result<Option<Value>> {
    let chain = match DirectoryChain::open(directory) {
        Ok(chain) => chain,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    if chain
        .directory()
        .stat_at(OsStr::new(name))
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(None);
    }
    fs::trust::state(directory)?;
    let bytes = read_opened(chain.directory(), OsStr::new(name), true)?;
    chain.recheck().map_err(io_error)?;
    match bytes {
        None => Ok(None),
        Some((bytes, _)) => {
            let value = json::decode_bytes(
                &bytes,
                DecodePolicy {
                    max_bytes: MAX,
                    ..DecodePolicy::default()
                },
            )?;
            validate(&value, harness, adapter)?;
            Ok(Some(value))
        }
    }
}

/// Read both adapters; when supplied, errors collect per-adapter unavailable records.
pub fn read(repo: &Path, mut errors: Option<&mut Vec<Value>>) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    for &(harness, adapter) in ADAPTERS {
        let (directory, name) = match paths(repo, harness, adapter) {
            Ok(paths) => paths,
            Err(Error::NoRepository(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        match read_one(&directory, &name, harness, adapter) {
            Ok(Some(value)) => values.push(value),
            Ok(None) => {}
            Err(e) => match errors.as_deref_mut() {
                Some(errors) => errors.push(crate::lifecycle::object([
                    ("harness", crate::lifecycle::string(harness)),
                    ("adapter", crate::lifecycle::string(adapter)),
                    ("reason", crate::lifecycle::string("state.unavailable")),
                ])),
                None => return Err(e),
            },
        }
    }
    Ok(values)
}

/// Replace or remove one observation under its nonblocking private flock.
pub fn write(repo: &Path, harness: &str, adapter: &str, value: Option<&Value>) -> Result<bool> {
    let (directory, name) = paths(repo, harness, adapter)?;
    if value.is_none() {
        match DirectoryChain::open(&directory) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(io_error(e)),
            Ok(chain) => match chain.directory().stat_at(OsStr::new(&name)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(io_error(e)),
                Ok(_) => {}
            },
        }
    }
    fs::private_mkdir(&directory)?;
    fs::trust::state(&directory)?;
    let chain = DirectoryChain::open(&directory).map_err(io_error)?;
    let dir = chain.directory();
    let lock_name = format!("{name}.lock");
    let (lock, _) = dir
        .open_or_create(
            OsStr::new(&lock_name),
            libc::O_RDWR | libc::O_NONBLOCK,
            0o600,
        )
        .map_err(io_error)?;
    let lock_info = lock.stat().map_err(io_error)?;
    if !safe_file(lock_info) {
        return Err(failure("Unsafe hook failure lock"));
    }
    let guard = FlockGuard::try_acquire(lock, LockMode::Exclusive).map_err(io_error)?;
    if !dir
        .stat_at(OsStr::new(&lock_name))
        .map_err(io_error)?
        .same_binding(lock_info)
    {
        return Err(failure("Unsafe hook failure lock"));
    }
    let old = read_opened(dir, OsStr::new(&name), false)?.map(|(_, identity)| identity);
    chain.recheck().map_err(io_error)?;
    match value {
        None => {
            if old.is_none() {
                return Ok(false);
            }
            verify_slot(dir, OsStr::new(&name), old)?;
            dir.unlink_at(OsStr::new(&name)).map_err(io_error)?;
        }
        Some(value) => {
            let raw = json::encode(validate(value, harness, adapter)?, json::SORTED_ASCII)?;
            if raw.len() > MAX {
                return Err(failure("Hook failure metadata exceeds bound"));
            }
            let temp = format!(".{}.tmp", host::uuid4()?.replace('-', ""));
            let mut out = dir
                .create_exclusive(OsStr::new(&temp), libc::O_WRONLY, 0o600)
                .map_err(io_error)?;
            let temp_info = out.stat().map_err(io_error)?;
            let result = (|| -> Result<()> {
                out.write_all(raw.as_bytes()).map_err(io_error)?;
                out.fsync().map_err(io_error)?;
                chain.recheck().map_err(io_error)?;
                verify_slot(dir, OsStr::new(&name), old)?;
                if !dir
                    .stat_at(OsStr::new(&temp))
                    .map_err(io_error)?
                    .same_inode(temp_info)
                {
                    return Err(failure("Hook failure temporary file changed"));
                }
                fs::rename_at(
                    dir,
                    OsStr::new(&temp),
                    dir,
                    OsStr::new(&name),
                    RenameMode::Replace,
                )
                .map_err(io_error)?;
                Ok(())
            })();
            if result.is_err()
                && dir
                    .stat_at(OsStr::new(&temp))
                    .is_ok_and(|info| info.same_inode(temp_info))
            {
                let _ = dir.unlink_at(OsStr::new(&temp));
            }
            result?;
        }
    }
    dir.fsync_directory().map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    drop(guard);
    Ok(true)
}

pub fn record(
    repo: &Path,
    harness: &str,
    adapter: &str,
    stage: &str,
    reason: &str,
) -> Result<bool> {
    let value = crate::lifecycle::object([
        ("schema", crate::lifecycle::string(SCHEMA)),
        ("harness", crate::lifecycle::string(harness)),
        ("adapter", crate::lifecycle::string(adapter)),
        ("stage", crate::lifecycle::string(stage)),
        ("reason", crate::lifecycle::string(reason)),
        (
            "timestamp",
            crate::lifecycle::string(host::utc_now_isoformat()?),
        ),
    ]);
    write(repo, harness, adapter, Some(&value))
}

pub fn clear(repo: &Path, harness: &str, adapter: &str) -> Result<bool> {
    write(repo, harness, adapter, None)
}
