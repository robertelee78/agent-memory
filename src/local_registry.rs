//! Local metadata storage using pinned directories and durable replacement.
use crate::developer_identity::digest;
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain, RenameMode};
use crate::host::Sources;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;

/// Approved bounded recovery inventory for a possibly hostile lock marker.
pub const LOCK_MAX_BYTES: usize = 1024 * 1024;

fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
struct Temporary<'a> {
    directory: &'a fs::Descriptor,
    name: OsString,
    present: bool,
}
impl Temporary<'_> {
    fn finish(&mut self) -> Result<()> {
        if self.present {
            match self.directory.unlink_at(&self.name) {
                Ok(()) => self.present = false,
                Err(error) if error.kind() == io::ErrorKind::NotFound => self.present = false,
                Err(error) => {
                    self.present = false;
                    return Err(io_error(error));
                }
            }
        }
        Ok(())
    }
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if self.present {
            let _ = self.directory.unlink_at(&self.name);
        }
    }
}

/// Replace only the bytes observed by the caller; synchronize file and parent.
pub fn replace(registry: &Path, payload: &[u8], observed: Option<&str>) -> Result<()> {
    replace_with_sources(registry, payload, observed, Sources::default())
}
pub fn replace_with_sources(
    registry: &Path,
    payload: &[u8],
    observed: Option<&str>,
    sources: Sources,
) -> Result<()> {
    let parent = registry
        .parent()
        .ok_or_else(|| ContractError::new("Registry has no parent directory"))?;
    let name = registry
        .file_name()
        .ok_or_else(|| ContractError::new("Registry has no filename"))?;
    let parent = fs::canonical_path(parent, true)?;
    let chain = DirectoryChain::open(&parent).map_err(io_error)?;
    let dir = chain.directory();
    let stem = registry
        .file_stem()
        .unwrap_or(OsStr::new("registry"))
        .to_string_lossy();
    let (temp, mut file) = {
        let mut created = None;
        for _ in 0..10_000 {
            let temp = OsString::from(format!(".{stem}-{}", (sources.uuid4)()?));
            match dir.create_exclusive(&temp, libc::O_WRONLY, 0o600) {
                Ok(file) => {
                    created = Some((temp, file));
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error(e)),
            }
        }
        created.ok_or_else(|| ContractError::new("No usable temporary file name found"))?
    };
    let mut cleanup = Temporary {
        directory: dir,
        name: temp,
        present: true,
    };
    let outcome = (|| -> Result<()> {
        file.write_all(payload).map_err(io_error)?;
        file.fsync().map_err(io_error)?;
        chain.recheck().map_err(io_error)?;
        let raw = match dir.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
            Ok(mut file) => {
                let identity = file.stat().map_err(io_error)?;
                if !identity.is_regular() {
                    return Err(ContractError::new(
                        "Registry changed immediately before replacement; preserve the external edit",
                    ));
                }
                let raw = file.read_bounded(json_limit()).map_err(io_error)?;
                if !dir
                    .stat_at(name)
                    .is_ok_and(|current| current.same_inode(identity) && !current.is_symlink())
                {
                    return Err(ContractError::new(
                        "Registry changed immediately before replacement; preserve the external edit",
                    ));
                }
                Some(raw)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(_) => {
                return Err(ContractError::new(
                    "Registry changed immediately before replacement; preserve the external edit",
                ));
            }
        };
        if digest(raw.as_deref()).as_deref() != observed {
            return Err(ContractError::new(
                "Registry changed immediately before replacement; preserve the external edit",
            ));
        }
        match dir.stat_at(name) {
            Ok(info) if info.is_symlink() => {
                return Err(ContractError::new(
                    "Registry changed immediately before replacement; preserve the external edit",
                ));
            }
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(e)),
        }
        chain.recheck().map_err(io_error)?;
        fs::rename_at(dir, &cleanup.name, dir, name, RenameMode::Replace).map_err(io_error)?;
        cleanup.present = false;
        dir.fsync_directory().map_err(io_error)?;
        chain.recheck().map_err(io_error)?;
        let mut actual = dir
            .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            .map_err(io_error)?;
        if actual
            .read_bounded(payload.len())
            .map_or(true, |bytes| bytes != payload)
        {
            return Err(ContractError::new(
                "Registry readback differs; inspect and replay the same operation",
            ));
        }
        Ok(())
    })();
    cleanup.finish()?;
    outcome
}
fn json_limit() -> usize {
    crate::codec::json::DEFAULT_MAX_BYTES
}
pub fn private_mkdir(path: &Path) -> Result<()> {
    fs::private_mkdir(path)
}
pub fn now(sources: Sources) -> Result<String> {
    (sources.utc_now)()
}

/// Resolve the metadata root without adopting symlinked ancestors or entries.
pub fn paths(name: &str) -> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    use std::path::{Component, PathBuf};
    let raw = std::env::var_os("AGENT_MEMORY_CONFIG_HOME").filter(|s| !s.is_empty());
    let base = match raw {
        Some(value) => PathBuf::from(value),
        None => {
            let xdg = match std::env::var_os("XDG_CONFIG_HOME") {
                Some(value) => PathBuf::from(value),
                None => crate::host::expanduser(Path::new("~"))?.join(".config"),
            };
            xdg.join("agent-memory")
        }
    };
    let root = crate::host::expanduser(&base)?;
    if !root.is_absolute() {
        return Err(ContractError::new(
            "Registry config root must be absolute and use canonical directories",
        ));
    }
    let mut current = std::path::PathBuf::from("/");
    let mut missing = false;
    for component in root.components() {
        let Component::Normal(part) = component else {
            if component == Component::ParentDir {
                current.pop();
            }
            continue;
        };
        if !missing {
            let chain = DirectoryChain::open(&current).map_err(io_error)?;
            match chain.directory().stat_at(part) {
                Ok(info) if info.is_symlink() => {
                    return Err(ContractError::new(
                        "Registry config root must be absolute and use canonical directories",
                    ));
                }
                // Python refuses only a non-directory root; a non-directory
                // ancestor leaves the root absent (later creation fails).
                Ok(info) if !info.is_directory() && current.join(part) == root => {
                    return Err(ContractError::new(
                        "Registry config root is not a directory",
                    ));
                }
                Ok(info) if !info.is_directory() => missing = true,
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => missing = true,
                Err(e) => return Err(io_error(e)),
            }
        }
        current.push(part);
    }
    let registry = root.join(format!("{name}.json"));
    let lock = root.join(format!(".{name}.lock"));
    if !missing {
        let chain = DirectoryChain::open(&current).map_err(io_error)?;
        for path in [&registry, &lock] {
            let Some(leaf) = path.file_name() else {
                continue;
            };
            match chain.directory().stat_at(leaf) {
                Ok(info) if info.is_symlink() => {
                    return Err(ContractError::new("Registry and lock must not be symlinks"));
                }
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(io_error(e)),
            }
        }
    }
    Ok((root, registry, lock))
}

/// Distinct refusal classes allow only bounded retry, never source authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalWriterError {
    Busy,
    Initializing,
    Contract(ContractError),
}
impl std::fmt::Display for CanonicalWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str(
                "Canonical writer busy; unwind and rediscover before private source work",
            ),
            Self::Initializing => f.write_str(
                "Canonical writer owner is unpublished; bounded retry without authority",
            ),
            Self::Contract(error) => std::fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for CanonicalWriterError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalWriter {
    Absent,
    Busy,
    Initializing,
}
pub fn check_canonical_writer(
    lock: &Path,
    operations: &[&str],
) -> std::result::Result<(), CanonicalWriterError> {
    match probe_canonical_writer(lock, operations).map_err(CanonicalWriterError::Contract)? {
        CanonicalWriter::Absent => Ok(()),
        CanonicalWriter::Busy => Err(CanonicalWriterError::Busy),
        CanonicalWriter::Initializing => Err(CanonicalWriterError::Initializing),
    }
}
/// Python's outer `except OSError` for every descriptor operation in the probe.
fn unsafe_observation() -> ContractError {
    ContractError::new("Canonical writer observation became unsafe; preserve for inspection")
}
fn probe_canonical_writer(lock: &Path, operations: &[&str]) -> Result<CanonicalWriter> {
    use crate::developer_identity::{encoded, load, uuid_valid};
    use crate::fs::{FlockGuard, LockError, LockMode};
    use crate::lifecycle::get;
    let fail = |s| ContractError::new(s);
    let parent = lock
        .parent()
        .ok_or_else(|| fail("Canonical writer bank cannot be safely pinned"))?;
    let name = lock
        .file_name()
        .ok_or_else(|| fail("Canonical writer marker cannot be safely inspected"))?;
    // Follow ordinary ancestors, but leave the bank's final component for
    // DirectoryChain's no-follow open and subsequent binding rechecks.
    let pinned_parent = if let (Some(ancestor), Some(bank)) = (parent.parent(), parent.file_name())
    {
        let resolved = match fs::canonical_path(ancestor, true) {
            Ok(path) => path,
            Err(_) => fs::canonical_path(ancestor, false)?,
        };
        resolved.join(bank)
    } else {
        fs::canonical_path(parent, true)?
    };
    let chain = match DirectoryChain::open(&pinned_parent) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(CanonicalWriter::Absent),
        Err(_) => return Err(fail("Canonical writer bank cannot be safely pinned")),
    };
    let dir = chain.directory();
    let current_name = || -> Result<Option<fs::Identity>> {
        chain
            .recheck()
            .map_err(|_| fail("Canonical writer bank changed; preserve for inspection"))?;
        match dir.stat_at(name) {
            Ok(info) => Ok(Some(info)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(fail(
                "Canonical writer observation became unsafe; preserve for inspection",
            )),
        }
    };
    let named = match dir.stat_at(name) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(CanonicalWriter::Absent),
        Err(_) => return Err(fail("Canonical writer marker cannot be safely inspected")),
    };
    let safe = |v: fs::Identity| {
        v.is_regular()
            && v.uid == crate::host::effective_uid()
            && v.nlink == 1
            && v.mode & 0o077 == 0
            && v.size >= 0
            && v.size <= 4096
    };
    if !safe(named) {
        return Err(fail(
            "Unknown or unsafe canonical writer marker; preserve for inspection",
        ));
    }
    let mut file = match dir.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return if current_name()?.is_none() {
                Ok(CanonicalWriter::Absent)
            } else {
                Err(fail("Canonical writer marker changed before opening"))
            };
        }
        Err(_) => {
            return Err(fail(
                "Canonical writer observation became unsafe; preserve for inspection",
            ));
        }
    };
    let before = file.stat().map_err(|_| unsafe_observation())?;
    let Some(current) = current_name()? else {
        return Ok(CanonicalWriter::Absent);
    };
    let same_identity = |a: fs::Identity, b: fs::Identity| {
        a.dev == b.dev && a.ino == b.ino && a.mode == b.mode && a.uid == b.uid && a.nlink == b.nlink
    };
    let same = |a: fs::Identity, b: fs::Identity| {
        same_identity(a, b)
            && a.size == b.size
            && a.mtime_ns == b.mtime_ns
            && a.ctime_ns == b.ctime_ns
    };
    let publishing = named.size == 0
        && safe(before)
        && safe(current)
        && same_identity(named, before)
        && same_identity(before, current)
        && (before.size == 0 || same(before, current));
    if !(same(named, before) && same(before, current)) && !publishing {
        if current_name()?.is_none() {
            return Ok(CanonicalWriter::Absent);
        }
        return Err(fail("Canonical writer marker changed before observation"));
    }
    chain
        .recheck()
        .map_err(|_| fail("Canonical writer bank changed; preserve for inspection"))?;
    if before.size == 0 {
        let after = file.stat().map_err(|_| unsafe_observation())?;
        let Some(current) = current_name()? else {
            return Ok(CanonicalWriter::Absent);
        };
        if !safe(after)
            || !safe(current)
            || !same_identity(before, after)
            || !same_identity(after, current)
            || (after.size != 0 && !same(after, current))
        {
            if current_name()?.is_none() {
                return Ok(CanonicalWriter::Absent);
            }
            return Err(fail(
                "Canonical writer initialization slot changed unsafely",
            ));
        }
        return Ok(CanonicalWriter::Initializing);
    }
    let raw = match file.read_bounded(4096) {
        Ok(raw) => raw,
        Err(_) if current_name()?.is_none() => return Ok(CanonicalWriter::Absent),
        Err(_) => {
            return Err(fail(
                "Canonical writer marker changed; preserve for inspection",
            ));
        }
    };
    let after = file.stat().map_err(|_| unsafe_observation())?;
    let Some(current) = current_name()? else {
        return Ok(CanonicalWriter::Absent);
    };
    if !same(before, after) || !same(after, current) {
        if current_name()?.is_none() {
            return Ok(CanonicalWriter::Absent);
        }
        return Err(fail(
            "Canonical writer marker changed; preserve for inspection",
        ));
    }
    let owner = load(&raw)
        .map_err(|_| fail("Malformed canonical writer marker; preserve for inspection"))?;
    let valid_fields = owner.as_object().is_some_and(|f| {
        f.len() == 5
            && f.iter().all(|(k, _)| {
                ["operation_id", "operation", "host", "pid", "start"].contains(&k.as_str())
            })
    });
    let operation = get(&owner, "operation")
        .as_str()
        .and_then(|s| s.strip_prefix("bank-"));
    // Any positive JSON integer is accepted here (Python `type(pid) is int and pid > 0`).
    let positive_pid = get(&owner, "pid").as_int().is_some_and(|value| {
        let digits = value.as_str();
        !digits.starts_with('-') && digits.trim_start_matches('0') != ""
    });
    if !valid_fields
        || !uuid_valid(get(&owner, "operation_id"))
        || !operation.is_some_and(|s| operations.contains(&s))
        || get(&owner, "host").as_str() != Some(crate::host::hostname()?.as_str())
        || !positive_pid
        || encoded(&owner)? != raw
    {
        return Err(fail(
            "Unknown canonical writer owner; preserve for inspection",
        ));
    }
    crate::lifecycle::time_value(get(&owner, "start"))?;
    let (held, unheld) = match FlockGuard::try_acquire_owned(file, LockMode::Shared) {
        Ok(guard) => (Some(guard), None),
        Err((file, LockError::Busy)) => (None, Some(file)),
        Err((_, _)) => return Err(unsafe_observation()),
    };
    let inspected = match (&held, &unheld) {
        (Some(guard), _) => guard.descriptor().stat(),
        (None, Some(file)) => file.stat(),
        (None, None) => return Err(unsafe_observation()),
    }
    .map_err(|_| unsafe_observation())?;
    let Some(current) = current_name()? else {
        return Ok(CanonicalWriter::Absent);
    };
    if !same(after, inspected) || !same(inspected, current) {
        if current_name()?.is_none() {
            return Ok(CanonicalWriter::Absent);
        }
        return Err(fail(
            "Canonical writer marker changed during contention observation",
        ));
    }
    if unheld.is_some() {
        Ok(CanonicalWriter::Busy)
    } else {
        Err(fail(
            "Canonical writer marker is not actively held; inspect/recover before private source work",
        ))
    }
}

/// Inspect or remove a stale local owner while holding its nonblocking EX flock.
pub fn recover(
    registry: &Path,
    lock: &Path,
    operations: &[&str],
    prepared: Option<&crate::codec::json::Value>,
    schema: &str,
    sources: Sources,
) -> Result<crate::codec::json::Value> {
    use crate::codec::json::Value;
    use crate::developer_identity::{load, uuid_valid};
    use crate::fs::{FlockGuard, LockError, LockMode};
    use crate::lifecycle::{get, object, string};
    let parent = lock.parent().ok_or_else(|| ContractError::new("Safe stale-lock recovery requires an existing local lock and supported file locking; operator review otherwise"))?;
    let name = lock.file_name().ok_or_else(|| ContractError::new("Safe stale-lock recovery requires an existing local lock and supported file locking; operator review otherwise"))?;
    let parent = fs::canonical_path(parent, false)?;
    let chain = DirectoryChain::open(&parent).map_err(|e| if e.kind() == io::ErrorKind::NotFound { ContractError::new("Safe stale-lock recovery requires an existing local lock and supported file locking; operator review otherwise") } else { io_error(e) })?;
    let dir = chain.directory();
    let file = dir.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0).map_err(|e| if e.kind() == io::ErrorKind::NotFound { ContractError::new("Safe stale-lock recovery requires an existing local lock and supported file locking; operator review otherwise") } else { io_error(e) })?;
    let mut guard = FlockGuard::try_acquire(file, LockMode::Exclusive).map_err(|e| match e {
        LockError::Busy => ContractError::new("Lock held by another writer/recovery"),
        other => io_error(other),
    })?;
    let inode = guard.descriptor().stat().map_err(io_error)?;
    if !dir.stat_at(name).map_err(io_error)?.same_inode(inode) {
        return Err(ContractError::new("Lock changed before inspection"));
    }
    let raw = guard
        .read_bounded(LOCK_MAX_BYTES)
        .map_err(|_| ContractError::new("Unknown lock; preserve it for operator review"))?;
    let owner = load(&raw)?;
    let valid = owner.as_object().is_some_and(|o| {
        o.len() == 5
            && o.iter().all(|(k, _)| {
                ["operation_id", "operation", "host", "pid", "start"].contains(&k.as_str())
            })
    });
    if !valid
        || !uuid_valid(get(&owner, "operation_id"))
        || !get(&owner, "operation")
            .as_str()
            .is_some_and(|s| operations.contains(&s))
    {
        return Err(ContractError::new(
            "Unknown lock; preserve it for operator review",
        ));
    }
    crate::lifecycle::time_value(get(&owner, "start"))?;
    let pid = get(&owner, "pid")
        .as_int()
        .and_then(|p| p.as_str().parse::<i32>().ok())
        .filter(|p| *p > 0);
    let same_host = get(&owner, "host").as_str() == Some(crate::host::hostname()?.as_str());
    let Some(pid) = pid.filter(|_| same_host) else {
        return Err(ContractError::new(
            "Foreign or malformed lock requires operator review",
        ));
    };
    match crate::host::pid_state(pid)? {
        crate::host::PidState::Gone => (),
        crate::host::PidState::Denied => {
            return Err(ContractError::new("Cannot prove owner is gone"));
        }
        crate::host::PidState::Alive => {
            return Err(ContractError::new("Lock owner is still alive"));
        }
    }
    let hash = crate::developer_identity::sha256_hex(&raw);
    let path = crate::host::text(registry.as_os_str(), "registry path")?;
    let Some(prepared) = prepared else {
        return Ok(object([
            (
                "proposal",
                object([
                    ("schema", string(schema)),
                    ("operation_id", string((sources.uuid4)()?)),
                    ("registry_path", string(path)),
                    ("observed_lock_sha256", string(hash)),
                ]),
            ),
            ("lock_owner", owner),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ]));
    };
    let prepared = prepared.get("proposal").unwrap_or(prepared);
    let valid = prepared.as_object().is_some_and(|o| {
        o.len() == 4
            && o.iter().all(|(k, _)| {
                [
                    "schema",
                    "operation_id",
                    "registry_path",
                    "observed_lock_sha256",
                ]
                .contains(&k.as_str())
            })
    });
    if !valid
        || get(prepared, "schema").as_str() != Some(schema)
        || !uuid_valid(get(prepared, "operation_id"))
        || get(prepared, "registry_path").as_str() != Some(path)
        || get(prepared, "observed_lock_sha256").as_str() != Some(hash.as_str())
    {
        return Err(ContractError::new(
            "Stale or invalid lock-recovery proposal",
        ));
    }
    let current = dir.stat_at(name).map_err(io_error)?;
    let mut check = dir
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    if !current.same_inode(inode)
        || !check.stat().map_err(io_error)?.same_inode(inode)
        || check
            .read_bounded(LOCK_MAX_BYTES)
            .map_err(|_| ContractError::new("Stale or invalid lock-recovery proposal"))?
            != raw
    {
        return Err(ContractError::new(
            "Stale or invalid lock-recovery proposal",
        ));
    }
    chain.recheck().map_err(io_error)?;
    dir.unlink_at(name).map_err(io_error)?;
    let verified = match dir.stat_at(name) {
        Ok(value) => !value.same_inode(inode),
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        Err(e) => return Err(io_error(e)),
    };
    Ok(object([
        ("operation_id", get(prepared, "operation_id").clone()),
        ("writes_performed", Value::Bool(true)),
        ("verified", Value::Bool(verified)),
    ]))
}

struct ExclusiveOwner<'a> {
    guard: crate::fs::FlockGuard,
    directory: &'a fs::Descriptor,
    name: &'a OsStr,
    inode: fs::Identity,
    payload: Vec<u8>,
    finished: bool,
}
impl ExclusiveOwner<'_> {
    fn finish(&mut self) -> Result<()> {
        self.finished = true;
        let named = match self.directory.stat_at(self.name) {
            Ok(named) => named,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error(error)),
        };
        if !named.same_inode(self.inode) || named.is_symlink() {
            return Ok(());
        }
        let mut current = self
            .directory
            .open_at(self.name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            .map_err(io_error)?;
        if !current.stat().map_err(io_error)?.same_inode(self.inode) {
            return Ok(());
        }
        let bytes = match current.read_bounded(self.payload.len()) {
            Ok(bytes) => bytes,
            Err(fs::ReadError::SizeLimit { .. }) => return Ok(()),
            Err(error) => return Err(io_error(error)),
        };
        if bytes == self.payload {
            self.directory.unlink_at(self.name).map_err(io_error)?;
        }
        Ok(())
    }
}
impl Drop for ExclusiveOwner<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.finish();
        }
    }
}

/// Publish ownership only after exclusive kernel lock acquisition; unlink only
/// the same owner bytes and inode, including during unwinding.
pub fn exclusive<T>(
    lock: &Path,
    operation_id: &str,
    operation: &str,
    sources: Sources,
    body: impl FnOnce(&[u8]) -> Result<T>,
) -> Result<T> {
    use crate::codec::json::Value;
    use crate::fs::{FlockGuard, LockError, LockMode};
    use crate::lifecycle::{object, string};
    let parent = lock
        .parent()
        .ok_or_else(|| ContractError::new("Metadata lock has no parent"))?;
    let name = lock
        .file_name()
        .ok_or_else(|| ContractError::new("Metadata lock has no filename"))?;
    let parent = fs::canonical_path(parent, false)?;
    fs::private_mkdir(&parent)?;
    let owner = object([
        ("operation_id", string(operation_id)),
        ("operation", string(operation)),
        ("host", string(crate::host::hostname()?)),
        ("pid", Value::Int(crate::host::pid().into())),
        ("start", string((sources.utc_now)()?)),
    ]);
    let payload = crate::developer_identity::encoded(&owner)?;
    let chain = DirectoryChain::open(&parent).map_err(io_error)?;
    let dir = chain.directory();
    let file = dir
        .create_exclusive(name, libc::O_WRONLY, 0o600)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                ContractError::new("Memory metadata is locked; inspect status/recover")
            } else {
                io_error(e)
            }
        })?;
    let created = file.stat().map_err(io_error)?;
    let guard = match FlockGuard::try_acquire_owned(file, LockMode::Exclusive) {
        Ok(guard) => guard,
        Err((file, error)) => {
            fs::discard_created_empty_slot(&parent.join(name), &file, created).map_err(|error| {
                // Python lets the parent-changed refusal through; OS errors become one refusal.
                if error.message() == "Created metadata slot parent changed; preserve the marker" {
                    error
                } else {
                    ContractError::new(
                        "Metadata ownership acquisition failed; empty-slot cleanup became unsafe",
                    )
                }
            })?;
            return Err(ContractError::new(match error {
                LockError::Busy => {
                    "Metadata ownership acquisition was contended before publication; retry without adopting the slot"
                }
                _ => {
                    "Metadata ownership acquisition failed before publication; retry without adopting the slot"
                }
            }));
        }
    };
    let mut owner = ExclusiveOwner {
        guard,
        directory: dir,
        name,
        inode: created,
        payload,
        finished: false,
    };
    let outcome = (|| {
        let written = owner.guard.write_once(&owner.payload).map_err(|_| {
            ContractError::new(
                "Metadata owner publication failed; preserve the marker for inspection",
            )
        })?;
        if written != owner.payload.len() {
            return Err(ContractError::new(
                "Incomplete metadata owner publication; preserve the marker for inspection",
            ));
        }
        owner.guard.descriptor().fsync().map_err(io_error)?;
        body(&owner.payload)
    })();
    owner.finish()?;
    outcome
}
