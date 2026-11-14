//! Private append-only source observations; coverage never establishes truth.
use crate::bank_writes::{self as writes, CanonicalWriterBarrier};
use crate::codec::json::Value;
use crate::developer_identity::{load_with_limit, sha256_hex, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, Descriptor, DirectoryChain, Identity, PrivateLock};
use crate::lifecycle::{get, object, string};
use crate::scope_binding::{self as scope, PrivateEnrollmentContext, RequireOptions};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, Weak};
use std::thread::ThreadId;
use std::time::Duration;

pub const MAX_SOURCES: usize = 128;
pub const MAX_TURNS: usize = 4096;
pub const MAX_SOURCE: usize = 4096;
pub const MAX_COMPLETION: usize = 65536;
pub const MAX_INTENT: usize = 8192;
pub const MAX_RETAINED_INPUT: usize = 131072;
pub const HARNESSES: &[&str] = &["codex", "claude", "opencode", "omp"];
pub const INTENT_SCHEMA: &str = "agent-memory/hold-source-enrollment-v1";
pub const UNIDENTIFIED_SCHEMA: &str = "agent-memory/unidentified-private-input-v1";
const MAX_NAMESPACES: usize = 65536;
const MAX_BACKLOG: usize = 60000;

/// T2c must validate the real publication request and replay authority, not flags.
pub trait PublicationGate: Send + Sync {
    fn request_value(&self, repo: &Path, request: &Value) -> Result<()>;
    fn replay(&self, repo: &Path, request: &Value, quarantine_records: bool) -> Result<Value>;
}
static PUBLICATION: RwLock<Option<Arc<dyn PublicationGate>>> = RwLock::new(None);
pub fn register_publication_gate(provider: Option<Arc<dyn PublicationGate>>) {
    *PUBLICATION.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub(crate) fn publication_gate_provider() -> Result<Arc<dyn PublicationGate>> {
    PUBLICATION
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| ContractError::missing_source_publication())
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "source state path")
}
fn leaf(path: &Path) -> Result<&OsStr> {
    path.file_name()
        .ok_or_else(|| err("Source state requires a filename"))
}
fn parent(path: &Path) -> Result<&Path> {
    path.parent()
        .ok_or_else(|| err("Source state requires a parent"))
}
fn fields(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|v| {
        v.len() == expected.len() && expected.iter().all(|k| value.get(k).is_some())
    })
}
fn same(a: &Value, b: &Value) -> Result<bool> {
    Ok(encoded(a)? == encoded(b)?)
}
fn extend(value: &Value, extra: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    let mut pairs = value.as_object().map(<[_]>::to_vec).unwrap_or_default();
    for (key, item) in extra {
        let key = key.into();
        if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == &key) {
            *old = item;
        } else {
            pairs.push((key, item));
        }
    }
    Value::Object(pairs)
}
fn without(value: &Value, keys: &[&str]) -> Value {
    object(
        value
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| !keys.contains(&k.as_str()))
            .cloned(),
    )
}
fn rows(value: &Value, key: &str) -> Vec<Value> {
    get(value, key)
        .as_array()
        .map(<[_]>::to_vec)
        .unwrap_or_default()
}
fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn uuid(value: &Value, v4: bool) -> bool {
    if v4 {
        return uuid_valid(value);
    }
    value.as_str().is_some_and(|s| {
        s.len() == 36
            && s.bytes().enumerate().all(|(i, b)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                }
            })
    })
}
pub fn encoded(value: &Value) -> Result<Vec<u8>> {
    Ok(crate::codec::json::encode(value, crate::codec::json::SORTED_COMPACT_UTF8)?.into_bytes())
}
pub fn bounded(value: &Value, maximum: usize) -> Result<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= maximum && !s.chars().any(|c| (c as u32) < 32))
        .ok_or_else(|| err("Invalid bounded source identifier"))
}
fn named(path: &Path) -> Result<Option<Identity>> {
    let directory = match Descriptor::pin_directory(parent(path)?) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    match directory.stat_at(leaf(path)?) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(e)),
    }
}
pub fn private_mkdir(path: &Path) -> Result<()> {
    fs::private_mkdir_with_sync(path, true)
}
pub fn read_bytes(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>> {
    fs::trust::inspect(path, fs::trust::InspectOptions::default())?;
    if named(path)?.is_none() {
        return Ok(None);
    }
    let chain = match DirectoryChain::open(parent(path)?) {
        Ok(chain) => chain,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let directory = chain.directory();
    let mut file = match directory.open_at(leaf(path)?, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let before = file.stat().map_err(io)?;
    if !before.is_regular()
        || before.uid != crate::host::effective_uid()
        || before.nlink != 1
        || before.mode & 0o077 != 0
        || before.size < 0
        || before.size as u128 > maximum as u128
    {
        return Err(err("Unsafe source state; preserve it for inspection"));
    }
    let raw = file.read_prefix(
        maximum
            .checked_add(1)
            .ok_or_else(|| err("Source read bound overflow"))?,
    );
    let after = file.stat().map_err(io)?;
    let current = match directory.stat_at(leaf(path)?) {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let raw = raw.map_err(io)?;
    if raw.len() as u128 != before.size as u128 || before != after || before != current {
        return Err(err(
            "Source state or its parent changed; retain pending evidence",
        ));
    }
    chain
        .recheck()
        .map_err(|_| err("Source state or its parent changed; retain pending evidence"))?;
    Ok(Some(raw))
}
pub fn read(path: &Path, maximum: usize) -> Result<Option<Value>> {
    read_bytes(path, maximum)?
        .map(|raw| load_with_limit(&raw, maximum))
        .transpose()
}
pub fn sync(directory: &Path) -> Result<()> {
    Descriptor::open_directory(directory)
        .and_then(|d| d.fsync_directory())
        .map_err(io)
}
/// False means an occupied name, never permission to overwrite that reservation.
pub fn exclusive(path: &Path, value: &Value, maximum: usize) -> Result<bool> {
    let raw = encoded(value)?;
    if raw.len() > maximum {
        return Err(err("Encoded source state exceeds bound"));
    }
    let chain = DirectoryChain::open(parent(path)?).map_err(io)?;
    let directory = chain.directory();
    let mut file = match directory.create_exclusive(leaf(path)?, libc::O_WRONLY, 0o600) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(io(e)),
    };
    file.write_all(&raw).map_err(io)?;
    file.fsync().map_err(io)?;
    directory.fsync_directory().map_err(io)?;
    chain.recheck().map_err(io)?;
    if !read(path, maximum)?
        .as_ref()
        .map(|v| same(v, value))
        .transpose()?
        .unwrap_or(false)
    {
        return Err(err("Source state durable readback failed"));
    }
    Ok(true)
}
fn remove_observed(path: &Path, raw: &[u8], maximum: usize) -> Result<()> {
    let chain = DirectoryChain::open(parent(path)?).map_err(io)?;
    let before = chain.directory().stat_at(leaf(path)?).map_err(io)?;
    if read_bytes(path, maximum)?.as_deref() != Some(raw)
        || chain.directory().stat_at(leaf(path)?).map_err(io)? != before
    {
        return Err(err(
            "Source state changed before cleanup; retain pending evidence",
        ));
    }
    chain.recheck().map_err(io)?;
    chain.directory().unlink_at(leaf(path)?).map_err(io)?;
    chain.directory().fsync_directory().map_err(io)?;
    chain.recheck().map_err(io)
}
fn entries(path: &Path, maximum: usize, message: &str) -> Result<Vec<OsString>> {
    entries_snapshot(path, maximum, message)?
        .ok_or_else(|| err("Source directory changed; preserve pending evidence"))
}
fn entries_snapshot(path: &Path, maximum: usize, message: &str) -> Result<Option<Vec<OsString>>> {
    fs::trust::state(path)?;
    let chain = DirectoryChain::open(path).map_err(io)?;
    let before = chain.directory().stat().map_err(io)?;
    let names = chain
        .directory()
        .entries_bounded(maximum)
        .map_err(io)?
        .ok_or_else(|| err(message))?;
    let changed = before != chain.directory().stat().map_err(io)?;
    chain.recheck().map_err(io)?;
    Ok((!changed).then_some(names))
}

thread_local! {
    static BARRIERS: RefCell<BTreeMap<PathBuf, (Weak<PrivateLock>, bool)>> = const { RefCell::new(BTreeMap::new()) };
    static READ_ONLY: Cell<bool> = const { Cell::new(false) };
}
struct RestoreReadOnly(bool);
impl Drop for RestoreReadOnly {
    fn drop(&mut self) {
        READ_ONLY.with(|v| v.set(self.0));
    }
}
pub fn read_only_locks<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    let _restore = RestoreReadOnly(READ_ONLY.with(|v| v.replace(true)));
    body()
}
pub fn private_lock(path: &Path, shared: bool, existing_only: bool) -> Result<PrivateLock> {
    PrivateLock::acquire(path, shared, existing_only || READ_ONLY.with(Cell::get)).map_err(|e| {
        match e {
            fs::LockError::Busy => ContractError::source_lock_busy(
                "Source transport busy; preserve pending work and retry",
            ),
            fs::LockError::Unsafe(e) => e,
            fs::LockError::Io(e) => io(e),
        }
    })
}
pub fn check_source_compatibility(base: &Path) -> Result<()> {
    if let Some(value) = read(&base.join("turn-sources-compat-v1.json"), MAX_SOURCE)? {
        if !fields(&value, &["schema", "minimum_reader", "harnesses"])
            || get(&value, "schema").as_str() != Some("agent-memory/turn-sources-compat-v1")
            || !matches!(
                get(&value, "minimum_reader").as_str(),
                Some("omp-harness-v1" | "omp-stored-source-v2")
            )
            || get(&value, "harnesses") != &Value::Array(vec![string("omp")])
        {
            return Err(err(
                "Unknown or malformed shared source compatibility marker; preserve pending evidence",
            ));
        }
    }
    Ok(())
}
pub fn source_barrier_held(base: &Path) -> bool {
    BARRIERS.with(|v| {
        v.borrow()
            .get(base)
            .is_some_and(|(weak, _)| weak.upgrade().is_some())
    })
}
pub struct SourceBarrier {
    base: PathBuf,
    held: Arc<PrivateLock>,
    owner: ThreadId,
}
impl SourceBarrier {
    pub fn recheck(&self) -> Result<()> {
        if self.owner != std::thread::current().id() {
            return Err(err("Borrowed source barrier is not owned by this context"));
        }
        self.held.recheck()?;
        check_source_compatibility(&self.base)
    }
    pub fn borrowed<T>(&self, body: impl FnOnce() -> Result<T>) -> Result<T> {
        let owned = BARRIERS.with(|v| {
            v.borrow()
                .get(&self.base)
                .and_then(|(v, _)| v.upgrade())
                .is_some_and(|v| Arc::ptr_eq(&v, &self.held))
        });
        if !owned {
            return Err(err("Borrowed source barrier is not owned by this context"));
        }
        self.recheck()?;
        let result = body()?;
        self.recheck()?;
        Ok(result)
    }
}
impl Drop for SourceBarrier {
    fn drop(&mut self) {
        if self.owner == std::thread::current().id() && Arc::strong_count(&self.held) == 1 {
            let _ = BARRIERS.try_with(|v| {
                v.borrow_mut().remove(&self.base);
            });
        }
    }
}
impl CanonicalWriterBarrier for SourceBarrier {
    fn finish(&mut self) -> Result<()> {
        self.recheck()
    }
}
pub fn source_barrier(
    repo: &Path,
    exclusive: bool,
    existing_only: bool,
    ordinary_bank: Option<&Path>,
    runtime_root: &Path,
) -> Result<SourceBarrier> {
    let location = scope::observe_enrollment_location(repo, None, runtime_root)?;
    let (_, bank, recovery, _) = match location {
        Some(v) => v,
        None if ordinary_bank.is_some() => {
            let (root, bank) = writes::bank_path(repo)?;
            crate::layout_recovery_state::_paths_for_bank(&root, &bank, None, runtime_root)?
        }
        None => crate::layout_recovery_state::paths(repo, None, runtime_root)?,
    };
    if ordinary_bank.is_some_and(|ordinary| ordinary != bank) {
        return Err(err(
            "Bank location changed before acquiring the source barrier",
        ));
    }
    let base = parent(&recovery)?.to_path_buf();
    source_barrier_at(&base, exclusive, existing_only)
}
/// Descriptor-level entry shared with isolated contracts; no scope authority is minted.
pub fn source_barrier_at(
    base: &Path,
    exclusive: bool,
    existing_only: bool,
) -> Result<SourceBarrier> {
    check_source_compatibility(base)?;
    let known = BARRIERS.with(|v| {
        v.borrow()
            .get(base)
            .and_then(|(weak, exclusive)| weak.upgrade().map(|v| (v, *exclusive)))
    });
    let held = if let Some((held, was_exclusive)) = known {
        if exclusive && !was_exclusive {
            return Err(err("Cannot upgrade an active shared source barrier"));
        }
        held
    } else {
        let held = Arc::new(private_lock(
            &base.join("source-barrier.lock"),
            !exclusive,
            existing_only,
        )?);
        check_source_compatibility(base)?;
        BARRIERS.with(|v| {
            v.borrow_mut()
                .insert(base.to_path_buf(), (Arc::downgrade(&held), exclusive));
        });
        held
    };
    Ok(SourceBarrier {
        base: base.to_path_buf(),
        held,
        owner: std::thread::current().id(),
    })
}
pub fn canonical_writer_barrier(
    repo: &Path,
    ordinary_bank: Option<&Path>,
    runtime_root: &Path,
) -> Result<SourceBarrier> {
    let deadline = crate::clock::Deadline::from_seconds(30.0)?;
    loop {
        match source_barrier(repo, true, false, ordinary_bank, runtime_root) {
            Err(e)
                if matches!(e.kind(), Kind::SourceTransportBusy { .. }) && !deadline.expired() =>
            {
                std::thread::sleep(Duration::from_millis(25).min(deadline.remaining()));
            }
            result => return result,
        }
    }
}
fn operation<T>(repo: &Path, runtime_root: &Path, body: impl FnOnce() -> Result<T>) -> Result<T> {
    let barrier = source_barrier(repo, false, false, None, runtime_root)?;
    let result = body()?;
    barrier.recheck()?;
    Ok(result)
}
pub fn locked<T>(directory: &Path, body: impl FnOnce() -> Result<T>) -> Result<T> {
    let path = directory.join("completion.lock");
    if READ_ONLY.with(Cell::get) && named(&path)?.is_none() {
        let base = directory
            .ancestors()
            .nth(3)
            .ok_or_else(|| err("Invalid completion namespace"))?;
        let exclusive = BARRIERS.with(|v| {
            v.borrow()
                .get(base)
                .is_some_and(|(w, x)| *x && w.upgrade().is_some())
        });
        if !exclusive {
            return Err(err(
                "Source completion lock is absent without a held source barrier",
            ));
        }
        fs::trust::state(directory)?;
        let chain = DirectoryChain::open(directory).map_err(io)?;
        let before = chain.directory().stat().map_err(io)?;
        let result = body();
        if named(&path)?.is_some() || !chain.directory().stat().map_err(io)?.same_inode(before) {
            return Err(err(
                "Source completion lock appeared during read-only observation",
            ));
        }
        chain.recheck().map_err(io)?;
        return result;
    }
    fs::trust::state(directory)?;
    let chain = DirectoryChain::open(directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            err("Source namespace is unavailable; preserve pending evidence")
        } else {
            io(error)
        }
    })?;
    let namespace = chain.directory().stat().map_err(io)?;
    if !namespace.is_directory()
        || namespace.uid != crate::host::effective_uid()
        || namespace.mode & 0o077 != 0
    {
        return Err(err("Unsafe source completion namespace"));
    }
    chain.recheck().map_err(io)?;
    let file = if READ_ONLY.with(Cell::get) {
        chain
            .directory()
            .open_at(
                OsStr::new("completion.lock"),
                libc::O_RDWR | libc::O_NONBLOCK,
                0,
            )
            .map_err(io)?
    } else {
        chain
            .directory()
            .open_or_create(
                OsStr::new("completion.lock"),
                libc::O_RDWR | libc::O_NONBLOCK,
                0o600,
            )
            .map_err(io)?
            .0
    };
    let identity = file.stat().map_err(io)?;
    if !identity.is_regular()
        || identity.uid != crate::host::effective_uid()
        || identity.nlink != 1
        || identity.mode & 0o077 != 0
    {
        return Err(err("Unsafe completion lock"));
    }
    let held =
        fs::FlockGuard::try_acquire(file, fs::LockMode::Exclusive).map_err(
            |error| match error {
                fs::LockError::Busy => ContractError::source_lock_busy(
                    "Source completion is busy; preserve pending work",
                ),
                fs::LockError::Unsafe(error) => error,
                fs::LockError::Io(error) => io(error),
            },
        )?;
    let recheck = || {
        chain.recheck().map_err(io)?;
        let named = chain
            .directory()
            .stat_at(OsStr::new("completion.lock"))
            .map_err(io)?;
        let opened = held.descriptor().stat().map_err(io)?;
        if !named.same_inode(identity) || !opened.same_inode(identity) {
            return Err(err("Private source lock changed; retain pending evidence"));
        }
        Ok(())
    };
    recheck()?;
    let result = body()?;
    recheck()?;
    Ok(result)
}

pub struct Namespace {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub base: PathBuf,
    pub directory: PathBuf,
    pub identity: Value,
}
fn namespace_at(
    root: &Path,
    bank: &Path,
    project_id: &Value,
    harness: &str,
    session: &str,
    lifecycle: &str,
) -> Result<Namespace> {
    if !HARNESSES.contains(&harness) {
        return Err(err("Unsupported source harness"));
    }
    bounded(&string(session), 512)?;
    bounded(&string(lifecycle), 512)?;
    let (operations, _) = writes::state_paths(root, bank, &crate::host::uuid4()?)?;
    let base = parent(&operations)?.join("turn-sources-v2");
    let key = sha256_hex(&encoded(&Value::Array(vec![
        string(text(root)?),
        string(harness),
        string(session),
        string(lifecycle),
    ]))?);
    let directory = base.join("turns").join(key);
    fs::trust::inspect(&directory, fs::trust::InspectOptions::default())?;
    Ok(Namespace {
        root: root.to_path_buf(),
        bank: bank.to_path_buf(),
        base,
        directory,
        identity: object([
            ("harness", string(harness)),
            ("session", string(session)),
            ("lifecycle", string(lifecycle)),
            ("project_id", project_id.clone()),
        ]),
    })
}
pub fn namespace(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    create: bool,
) -> Result<(Namespace, Value)> {
    let context = scope::require(repo, None, RequireOptions::default())?;
    let (root, bank) = writes::bank_path(repo)?;
    let ns = namespace_at(
        &root,
        &bank,
        get(&context, "project_id"),
        harness,
        session,
        lifecycle,
    )?;
    if create {
        private_mkdir(&ns.directory)?;
    }
    Ok((ns, context))
}
pub fn private_namespace(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    create: bool,
    runtime_root: &Path,
) -> Result<(Namespace, PrivateEnrollmentContext)> {
    let context = scope::private_enrollment_context(repo, runtime_root)?;
    let ns = namespace_at(
        context.root(),
        context.bank(),
        context.project_id(),
        harness,
        session,
        lifecycle,
    )?;
    if create {
        scope::recheck_private_enrollment(&context)?;
        private_mkdir(&ns.directory)?;
    }
    Ok((ns, context))
}
fn enrollment_namespace(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    context: &PrivateEnrollmentContext,
) -> Result<Namespace> {
    scope::enrollment_read_context(repo, context)?;
    let ns = namespace_at(
        context.root(),
        context.bank(),
        context.project_id(),
        harness,
        session,
        lifecycle,
    )?;
    if !source_barrier_held(parent(&ns.base)?) {
        return Err(err(
            "Private enrollment locator reuse requires the active source barrier",
        ));
    }
    Ok(ns)
}
pub fn reference(directory: &Path, identity: &Value) -> Result<Value> {
    Ok(extend(
        identity,
        [
            ("schema", string("agent-memory/turn-reference-v2")),
            (
                "turn",
                string(crate::host::text(leaf(directory)?, "source namespace")?),
            ),
        ],
    ))
}
pub fn validate_reference<'a>(
    value: &'a Value,
    root: &Path,
    project_id: &Value,
) -> Result<&'a Value> {
    if !fields(
        value,
        &[
            "schema",
            "harness",
            "session",
            "lifecycle",
            "project_id",
            "turn",
        ],
    ) || value
        .as_object()
        .is_none_or(|v| v.iter().any(|(_, v)| v.as_str().is_none()))
        || get(value, "schema").as_str() != Some("agent-memory/turn-reference-v2")
        || get(value, "project_id") != project_id
        || !HARNESSES.contains(&get(value, "harness").as_str().unwrap_or(""))
    {
        return Err(err("Pending index reference is malformed or foreign"));
    }
    bounded(get(value, "session"), 512)?;
    bounded(get(value, "lifecycle"), 512)?;
    let key = sha256_hex(&encoded(&Value::Array(vec![
        string(text(root)?),
        get(value, "harness").clone(),
        get(value, "session").clone(),
        get(value, "lifecycle").clone(),
    ]))?);
    if get(value, "turn").as_str() != Some(&key) {
        return Err(err("Pending index reference names a different namespace"));
    }
    Ok(value)
}

pub fn index(ns: &Namespace, context: Option<&PrivateEnrollmentContext>) -> Result<PathBuf> {
    let target = ns.base.join("pending");
    fs::trust::inspect(&target, fs::trust::InspectOptions::default())?;
    if let Some(context) = context {
        scope::recheck_private_enrollment(context)?;
    }
    private_mkdir(&target)?;
    let value = reference(&ns.directory, &ns.identity)?;
    for _ in 0..2 {
        let Some(names) = entries_snapshot(
            &target,
            2 * MAX_TURNS,
            "Pending turn directory exceeds its inventory bound; preserve sources and inspect backlog",
        )?
        else {
            continue;
        };
        let occupied: BTreeSet<usize> = names
            .iter()
            .filter_map(|name| {
                let number = name.to_str()?.strip_suffix(".json")?;
                let parsed = number.parse::<usize>().ok()?;
                (parsed < MAX_TURNS && number == parsed.to_string()).then_some(parsed)
            })
            .collect();
        let mut free = (0..MAX_TURNS).find(|n| !occupied.contains(n));
        let mut matching = None;
        for i in occupied {
            let path = target.join(format!("{i}.json"));
            let old = read(&path, MAX_SOURCE).map_err(|_| err("Pending turn index has an unverifiable reservation; preserve it and inspect backlog"))?;
            let Some(old) = old else {
                if free.is_none_or(|n| i < n) {
                    free = Some(i);
                }
                continue;
            };
            validate_reference(&old, &ns.root, get(&ns.identity, "project_id"))?;
            if same(&old, &value)? {
                matching = Some(path);
            }
        }
        if let Some(path) = matching {
            match read(&path, MAX_SOURCE)? {
                Some(old) if same(&old, &value)? => return Ok(path),
                None => continue,
                _ => {
                    return Err(err(
                        "Pending turn index changed; preserve sources and retry input",
                    ));
                }
            }
        }
        let free = free.ok_or_else(|| {
            err("Pending turn index full; inspect retained backlog before retrying input")
        })?;
        let path = target.join(format!("{free}.json"));
        if let Some(context) = context {
            scope::recheck_private_enrollment(context)?;
        }
        if !exclusive(&path, &value, MAX_SOURCE)? {
            continue;
        }
        if let Some(context) = context {
            scope::recheck_private_enrollment(context)?;
        }
        return Ok(path);
    }
    Err(err(
        "Pending turn index changed concurrently; preserve sources and retry input",
    ))
}
pub fn fault(directory: &Path, code: &str) {
    let _ = exclusive(
        &directory.join("fault.json"),
        &object([
            ("schema", string("agent-memory/turn-fault-v2")),
            ("reason", string(code)),
        ]),
        MAX_SOURCE,
    );
}
pub fn validate_source<'a>(value: &'a Value, identity: &Value) -> Result<&'a Value> {
    let mut keys = vec![
        "schema",
        "observation_id",
        "source_sha256",
        "native_source_id",
        "source_reference",
        "control_origin_hint",
    ];
    keys.extend(
        identity
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k.as_str()),
    );
    if !fields(value, &keys)
        || get(value, "schema").as_str() != Some("agent-memory/turn-source-v2")
        || identity
            .as_object()
            .into_iter()
            .flatten()
            .any(|(k, v)| value.get(k) != Some(v))
    {
        return Err(err("Source observation differs from its namespace"));
    }
    if get(value, "control_origin_hint").as_bool().is_none() {
        return Err(err("Invalid control-origin qualification"));
    }
    if !uuid(get(value, "observation_id"), false) {
        return Err(err("Invalid observation UUID"));
    }
    if !digest(get(value, "source_sha256")) {
        return Err(err("Invalid source digest"));
    }
    if get(value, "native_source_id") != &Value::Null {
        bounded(get(value, "native_source_id"), 512)?;
    }
    if get(value, "source_reference") != &Value::Null {
        bounded(get(value, "source_reference"), 2048)?;
    }
    Ok(value)
}
pub fn enrollment_value<'a>(
    value: &'a Value,
    identity: &Value,
    event: &Value,
) -> Result<&'a Value> {
    if !fields(
        value,
        &[
            "schema",
            "enrollment_id",
            "project_id",
            "harness",
            "session",
            "lifecycle",
            "native_event_id",
            "source_sha256",
            "source_reference",
            "observation_id",
            "context_digest",
            "state",
        ],
    ) || get(value, "schema").as_str() != Some(INTENT_SCHEMA)
        || identity
            .as_object()
            .into_iter()
            .flatten()
            .any(|(k, v)| value.get(k) != Some(v))
        || get(value, "native_event_id") != event
        || !matches!(
            get(value, "state").as_str(),
            Some("intent" | "source-written" | "registered" | "unverified")
        )
    {
        return Err(err(
            "Held enrollment intent is malformed, foreign or conflicting; preserve it",
        ));
    }
    if !uuid(get(value, "enrollment_id"), true) || !uuid(get(value, "observation_id"), true) {
        return Err(err(
            "Held enrollment intent has an invalid observation identity",
        ));
    }
    if !digest(get(value, "source_sha256")) || !digest(get(value, "context_digest")) {
        return Err(err("Held enrollment intent has invalid private pins"));
    }
    bounded(event, 512)?;
    if get(value, "source_reference") != &Value::Null {
        bounded(get(value, "source_reference"), 2048)?;
    }
    Ok(value)
}
pub fn enrollment_state(
    path: &Path,
    intent: &Value,
    state: &str,
    context: Option<&PrivateEnrollmentContext>,
) -> Result<Value> {
    if let Some(context) = context {
        scope::recheck_private_enrollment(context)?;
    }
    let raw = read_bytes(path, MAX_INTENT)?
        .ok_or_else(|| err("Enrollment intent changed; preserve original observation"))?;
    if !same(&load_with_limit(&raw, MAX_INTENT)?, intent)? {
        return Err(err(
            "Enrollment intent changed; preserve original observation",
        ));
    }
    let updated = extend(intent, [("state", string(state))]);
    crate::local_registry::replace_with_limit(
        path,
        &encoded(&updated)?,
        Some(&sha256_hex(&raw)),
        MAX_INTENT,
    )?;
    if !read(path, MAX_INTENT)?
        .as_ref()
        .map(|v| same(v, &updated))
        .transpose()?
        .unwrap_or(false)
    {
        return Err(err("Enrollment intent readback failed"));
    }
    Ok(updated)
}
fn event_key(identity: &Value, event: &Value) -> Result<String> {
    Ok(sha256_hex(&encoded(&Value::Array(vec![
        identity.clone(),
        event.clone(),
    ]))?))
}
pub fn held_register(
    ns: &Namespace,
    context: &PrivateEnrollmentContext,
    value: &Value,
) -> Result<Value> {
    let event = get(value, "native_source_id");
    if event == &Value::Null {
        return Err(err(
            "Recovery-pending enrollment requires an unambiguous actual native event identity",
        ));
    }
    bounded(event, 512)?;
    let intent_path = ns
        .directory
        .join("enrollments")
        .join(format!("{}.json", event_key(&ns.identity, event)?));
    scope::recheck_private_enrollment(context)?;
    let held = private_lock(&ns.directory.join("enrollment.lock"), false, false)?;
    scope::recheck_private_enrollment(context)?;
    private_mkdir(parent(&intent_path)?)?;
    let mut value = value.clone();
    let mut intent = if let Some(intent) = read(&intent_path, MAX_INTENT)? {
        enrollment_value(&intent, &ns.identity, event)?;
        if get(&intent, "source_sha256") != get(&value, "source_sha256")
            || get(&intent, "source_reference") != get(&value, "source_reference")
            || get(&intent, "context_digest").as_str() != Some(context.context_digest())
        {
            return Err(err(
                "Original native event, source or hold pins differ; preserve the original pending enrollment",
            ));
        }
        value = extend(
            &value,
            [("observation_id", get(&intent, "observation_id").clone())],
        );
        intent
    } else {
        let intent = extend(
            &ns.identity,
            [
                ("schema", string(INTENT_SCHEMA)),
                ("enrollment_id", string(crate::host::uuid4()?)),
                ("native_event_id", event.clone()),
                ("source_sha256", get(&value, "source_sha256").clone()),
                ("source_reference", get(&value, "source_reference").clone()),
                ("observation_id", get(&value, "observation_id").clone()),
                ("context_digest", string(context.context_digest())),
                ("state", string("intent")),
            ],
        );
        enrollment_value(&intent, &ns.identity, event)?;
        scope::recheck_private_enrollment(context)?;
        writes::fault("hold-source-before-intent")?;
        if !exclusive(&intent_path, &intent, MAX_INTENT)? {
            return Err(err(
                "Enrollment intent changed; preserve original observation",
            ));
        }
        intent
    };
    let result = (|| {
        writes::fault("hold-source-after-intent")?;
        scope::recheck_private_enrollment(context)?;
        writes::fault("hold-source-before-index")?;
        index(ns, Some(context))?;
        let mut matching = 0;
        let mut free = None;
        for i in 0..MAX_SOURCES {
            let path = ns.directory.join(format!("source-{i}.json"));
            let Some(old) = read(&path, MAX_SOURCE)? else {
                if free.is_none() {
                    free = Some(path);
                }
                continue;
            };
            validate_source(&old, &ns.identity)?;
            if get(&old, "observation_id") == get(&value, "observation_id")
                || get(&old, "native_source_id") == event
            {
                let expected = extend(
                    &value,
                    [(
                        "control_origin_hint",
                        get(&old, "control_origin_hint").clone(),
                    )],
                );
                if !same(&old, &expected)? {
                    return Err(err("Native event has conflicting retained source evidence"));
                }
                value = expected;
                matching += 1;
            }
        }
        if matching > 1 {
            return Err(err(
                "Native event has duplicate source slots; retain all evidence",
            ));
        }
        if matching == 0 {
            let free = free
                .ok_or_else(|| err("Source capacity reached; retain original enrollment intent"))?;
            scope::recheck_private_enrollment(context)?;
            writes::fault("hold-source-before-source")?;
            if !exclusive(&free, &value, MAX_SOURCE)? {
                return Err(err(
                    "Native event source reservation changed; retain original enrollment intent",
                ));
            }
        }
        writes::fault("hold-source-after-source")?;
        scope::recheck_private_enrollment(context)?;
        intent = enrollment_state(&intent_path, &intent, "source-written", Some(context))?;
        index(ns, Some(context))?;
        writes::fault("hold-source-after-index")?;
        scope::recheck_private_enrollment(context)?;
        if rows(&inventory(&ns.directory, &ns.identity)?, "sources")
            .iter()
            .filter(|v| get(v, "observation_id") == get(&value, "observation_id"))
            .count()
            != 1
        {
            return Err(err(
                "Held source durable readback failed; preserve its intent",
            ));
        }
        intent = enrollment_state(&intent_path, &intent, "registered", Some(context))?;
        writes::fault("hold-source-before-final-recheck")?;
        scope::recheck_private_enrollment(context)?;
        held.recheck()?;
        Ok(value)
    })();
    if result.is_err() {
        let _ = enrollment_state(&intent_path, &intent, "unverified", None);
    }
    result
}
fn wake_value(value: &Value) -> Result<()> {
    if !fields(value, &["schema", "fingerprint", "generated_sha256"])
        || get(value, "schema").as_str() != Some("agent-memory/turn-wake-v2")
    {
        return Err(err("Unsafe retained wake evidence"));
    }
    Ok(())
}
fn append_source_at(
    ns: &Namespace,
    context: &PrivateEnrollmentContext,
    source_sha256: &str,
    native_source_id: Option<&str>,
    source_reference: Option<&str>,
) -> Result<Value> {
    let mut value = extend(
        &ns.identity,
        [
            ("schema", string("agent-memory/turn-source-v2")),
            ("observation_id", string(crate::host::uuid4()?)),
            ("source_sha256", string(source_sha256)),
            (
                "native_source_id",
                native_source_id.map(string).unwrap_or(Value::Null),
            ),
            (
                "source_reference",
                source_reference.map(string).unwrap_or(Value::Null),
            ),
            ("control_origin_hint", Value::Bool(false)),
        ],
    );
    validate_source(&value, &ns.identity)?;
    if encoded(&value)?.len() > MAX_SOURCE {
        return Err(err("Source observation exceeds encoded bound"));
    }
    if context.held() && native_source_id.is_none() {
        return Err(err(
            "Recovery-pending input needs its actual native event identity; retain and retry the same event",
        ));
    }
    scope::recheck_private_enrollment(context)?;
    private_mkdir(&ns.directory)?;
    for i in 0..2 {
        if let Some(wake) = read(&ns.directory.join(format!("wake-{i}.json")), MAX_SOURCE)? {
            wake_value(&wake)?;
            if get(&wake, "generated_sha256").as_str() == Some(source_sha256) {
                value = extend(&value, [("control_origin_hint", Value::Bool(true))]);
            }
        }
    }
    if context.held() {
        return held_register(ns, context, &value);
    }
    if let Some(event) = native_source_id {
        let path = ns
            .directory
            .join("enrollments")
            .join(format!("{}.json", event_key(&ns.identity, &string(event))?));
        if named(&path)?.is_some() {
            return Err(err(
                "Original held enrollment requires its exact verified hold pins; preserve the pending observation",
            ));
        }
    }
    let result = (|| {
        index(ns, Some(context))?;
        for i in 0..MAX_SOURCES {
            let path = ns.directory.join(format!("source-{i}.json"));
            if named(&path)?.is_some() {
                continue;
            }
            scope::recheck_private_enrollment(context)?;
            if !exclusive(&path, &value, MAX_SOURCE)? {
                continue;
            }
            scope::recheck_private_enrollment(context)?;
            index(ns, Some(context))?;
            scope::recheck_private_enrollment(context)?;
            return Ok(value);
        }
        scope::recheck_private_enrollment(context)?;
        fault(&ns.directory, "source-capacity");
        Err(err(
            "Source capacity reached; preserve this turn and retry in a new turn after inspecting backlog",
        ))
    })();
    if result.is_err() {
        fault(&ns.directory, "registration-unverified");
    }
    result
}
pub fn register(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    source_sha256: &str,
    native_source_id: Option<&str>,
    source_reference: Option<&str>,
    enrollment_context: Option<&PrivateEnrollmentContext>,
    runtime_root: &Path,
) -> Result<Value> {
    operation(repo, runtime_root, || {
        if let Some(context) = enrollment_context {
            let ns = enrollment_namespace(repo, harness, session, lifecycle, context)?;
            append_source_at(
                &ns,
                context,
                source_sha256,
                native_source_id,
                source_reference,
            )
        } else {
            let (ns, context) =
                private_namespace(repo, harness, session, lifecycle, false, runtime_root)?;
            append_source_at(
                &ns,
                &context,
                source_sha256,
                native_source_id,
                source_reference,
            )
        }
    })
}

pub fn validate_receipt<'a>(
    receipt: &'a Value,
    observation_id: &str,
    source_digest: &str,
) -> Result<&'a Value> {
    if !digest(&string(source_digest)) {
        return Err(err("Invalid completed source digest"));
    }
    if !fields(
        receipt,
        &[
            "outcome",
            "assessment",
            "assessment_is_agent_assertion",
            "source_publication_link_verified",
            "publication_verified",
            "witnessed_commit",
            "input_sha256",
            "canonical_writes_performed",
            "observation_id",
            "source_sha256",
        ],
    ) || get(receipt, "observation_id").as_str() != Some(observation_id)
        || get(receipt, "source_sha256").as_str() != Some(source_digest)
        || get(receipt, "assessment_is_agent_assertion") != &Value::Bool(true)
        || get(receipt, "source_publication_link_verified") != &Value::Bool(false)
        || get(receipt, "canonical_writes_performed") != &Value::Bool(false)
        || !digest(get(receipt, "input_sha256"))
    {
        return Err(err("Completed assessment evidence is malformed"));
    }
    match get(receipt, "outcome").as_str() {
        Some("no-learning") => {
            if get(receipt, "publication_verified") != &Value::Bool(false)
                || get(receipt, "witnessed_commit") != &Value::Null
            {
                return Err(err("No-learning receipt cannot claim landing"));
            }
            crate::boundary_assessment::validate(get(receipt, "assessment"), observation_id)?;
        }
        Some("published") => {
            let commit = get(receipt, "witnessed_commit").as_str().unwrap_or("");
            if get(receipt, "publication_verified") != &Value::Bool(true)
                || !matches!(commit.len(), 40 | 64)
                || !commit
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !assessment_text(get(receipt, "assessment"))
            {
                return Err(err("Published receipt evidence is malformed"));
            }
        }
        _ => return Err(err("Unknown completed assessment outcome")),
    }
    Ok(receipt)
}
fn assessment_text(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() <= 2048
            && !s
                .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                .is_empty()
    })
}
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn base64_encode(raw: &[u8]) -> String {
    let mut result = String::with_capacity(raw.len().div_ceil(3) * 4);
    for block in raw.chunks(3) {
        let a = block.first().copied().unwrap_or(0);
        let b = block.get(1).copied().unwrap_or(0);
        let c = block.get(2).copied().unwrap_or(0);
        // Each index is masked to six bits and BASE64 has exactly 64 entries.
        result.push(BASE64[usize::from(a >> 2)] as char);
        result.push(BASE64[usize::from(((a & 3) << 4) | (b >> 4))] as char);
        result.push(if block.len() > 1 {
            BASE64[usize::from(((b & 15) << 2) | (c >> 6))] as char
        } else {
            '='
        });
        result.push(if block.len() > 2 {
            BASE64[usize::from(c & 63)] as char
        } else {
            '='
        });
    }
    result
}
fn base64_decode(value: &Value) -> Result<Vec<u8>> {
    let value = value
        .as_str()
        .ok_or_else(|| err("Unidentified private input bytes are missing or changed"))?;
    let data = value.trim_end_matches('=');
    let padding = value.len() - data.len();
    if data.len() % 4 == 1
        || (data.len() % 4 == 2 && padding != 2)
        || (data.len() % 4 == 3 && padding != 1)
        || (data.len() % 4 != 0 && value.len() % 4 != 0)
        || (data.len() % 4 == 0 && padding != 0)
    {
        return Err(err(
            "Unidentified private input bytes are missing or changed",
        ));
    }
    let mut raw = Vec::with_capacity(data.len() * 3 / 4);
    let mut accumulator = 0u32;
    let mut bits = 0;
    for b in data.bytes() {
        let n = BASE64
            .iter()
            .position(|v| *v == b)
            .ok_or_else(|| err("Unidentified private input bytes are missing or changed"))?;
        accumulator = (accumulator << 6) | n as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            raw.push((accumulator >> bits) as u8);
        }
    }
    Ok(raw)
}
pub fn unidentified_inventory(directory: &Path, identity: &Value) -> Result<Vec<Value>> {
    let target = directory.join("unidentified-inputs");
    if named(&target)?.is_none() {
        return Ok(vec![]);
    }
    let names = entries(
        &target,
        MAX_SOURCES,
        "Unidentified input retention capacity reached; preserve native originals",
    )?;
    let mut values = Vec::new();
    for name in names {
        let value = read(&target.join(&name), MAX_RETAINED_INPUT)?
            .ok_or_else(|| err("Unidentified private input evidence is malformed or foreign"))?;
        if !fields(
            &value,
            &[
                "schema",
                "retention_id",
                "harness",
                "session",
                "lifecycle",
                "project_id",
                "source_reference",
                "source_base64",
                "source_sha256",
                "context_digest",
                "uncertainty",
            ],
        ) || get(&value, "schema").as_str() != Some(UNIDENTIFIED_SCHEMA)
            || identity
                .as_object()
                .into_iter()
                .flatten()
                .any(|(k, v)| value.get(k) != Some(v))
            || get(&value, "uncertainty").as_str() != Some("native-event-identity-unavailable")
            || !uuid(get(&value, "retention_id"), true)
            || name.to_str()
                != Some(&format!(
                    "{}.json",
                    get(&value, "retention_id").as_str().unwrap_or("")
                ))
        {
            return Err(err(
                "Unidentified private input evidence is malformed or foreign",
            ));
        }
        let raw = base64_decode(get(&value, "source_base64"))?;
        if raw.is_empty()
            || raw.len() > 65536
            || get(&value, "source_sha256").as_str() != Some(&sha256_hex(&raw))
        {
            return Err(err(
                "Unidentified private input bytes are missing or changed",
            ));
        }
        if get(&value, "source_reference") != &Value::Null {
            bounded(get(&value, "source_reference"), 2048)?;
        }
        if !digest(get(&value, "context_digest")) {
            return Err(err(
                "Unidentified private input has invalid original context pins",
            ));
        }
        values.push(value);
    }
    values.sort_by(|a, b| {
        get(a, "retention_id")
            .as_str()
            .cmp(&get(b, "retention_id").as_str())
    });
    Ok(values)
}
pub fn retain_unidentified_input(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    raw_source: &[u8],
    source_reference: Option<&str>,
    runtime_root: &Path,
) -> Result<Value> {
    operation(repo, runtime_root, || {
        let (ns, context) =
            private_namespace(repo, harness, session, lifecycle, true, runtime_root)?;
        if !context.held() || raw_source.is_empty() || raw_source.len() > 65536 {
            return Err(err(
                "Unidentified input retention requires a verified hold and complete bounded source bytes",
            ));
        }
        if let Some(reference) = source_reference {
            bounded(&string(reference), 2048)?;
        }
        let held = private_lock(&ns.directory.join("enrollment.lock"), false, false)?;
        scope::recheck_private_enrollment(&context)?;
        if unidentified_inventory(&ns.directory, &ns.identity)?.len() >= MAX_SOURCES {
            return Err(err(
                "Unidentified input capacity reached; preserve native originals for inspection",
            ));
        }
        let retention_id = crate::host::uuid4()?;
        let value = extend(
            &ns.identity,
            [
                ("schema", string(UNIDENTIFIED_SCHEMA)),
                ("retention_id", string(&retention_id)),
                (
                    "source_reference",
                    source_reference.map(string).unwrap_or(Value::Null),
                ),
                ("source_base64", string(base64_encode(raw_source))),
                ("source_sha256", string(sha256_hex(raw_source))),
                ("context_digest", string(context.context_digest())),
                ("uncertainty", string("native-event-identity-unavailable")),
            ],
        );
        let target = ns.directory.join("unidentified-inputs");
        scope::recheck_private_enrollment(&context)?;
        private_mkdir(&target)?;
        scope::recheck_private_enrollment(&context)?;
        if !exclusive(
            &target.join(format!("{retention_id}.json")),
            &value,
            MAX_RETAINED_INPUT,
        )? {
            return Err(err(
                "Unidentified input retention identity is already occupied",
            ));
        }
        scope::recheck_private_enrollment(&context)?;
        held.recheck()?;
        Ok(object([
            ("retention_id", string(retention_id)),
            ("verified_observation", Value::Bool(false)),
            ("outcome", string("pending-native-event-identity")),
        ]))
    })
}
pub fn enrollment_inventory(directory: &Path, identity: &Value) -> Result<Vec<Value>> {
    let target = directory.join("enrollments");
    if named(&target)?.is_none() {
        return Ok(vec![]);
    }
    let names = entries(
        &target,
        MAX_SOURCES,
        "Held enrollment inventory exceeds source capacity",
    )?;
    let mut values = Vec::new();
    for name in names {
        let value = read(&target.join(&name), MAX_INTENT)?
            .ok_or_else(|| err("Held enrollment intent is unavailable"))?;
        let event = get(&value, "native_event_id");
        enrollment_value(&value, identity, event)?;
        if name.to_str() != Some(&format!("{}.json", event_key(identity, event)?)) {
            return Err(err(
                "Held enrollment intent is outside its original event namespace",
            ));
        }
        values.push(value);
    }
    values.sort_by(|a, b| {
        get(a, "enrollment_id")
            .as_str()
            .cmp(&get(b, "enrollment_id").as_str())
    });
    Ok(values)
}
fn completed_record(value: &Value, source: &Value) -> Result<Value> {
    if !fields(
        value,
        &[
            "schema",
            "observation_id",
            "source_sha256",
            "receipt",
            "record_sha256",
        ],
    ) || get(value, "schema").as_str() != Some("agent-memory/source-completed-v2")
        || get(value, "observation_id") != get(source, "observation_id")
        || get(value, "source_sha256") != get(source, "source_sha256")
        || get(value, "receipt").as_object().is_none()
    {
        return Err(err("Completion differs from observation"));
    }
    if get(value, "record_sha256").as_str()
        != Some(&sha256_hex(&encoded(&without(value, &["record_sha256"]))?))
    {
        return Err(err("Completion checksum differs"));
    }
    validate_receipt(
        get(value, "receipt"),
        get(source, "observation_id").as_str().unwrap_or(""),
        get(source, "source_sha256").as_str().unwrap_or(""),
    )?;
    Ok(get(value, "receipt").clone())
}
pub fn inventory(directory: &Path, identity: &Value) -> Result<Value> {
    if named(directory)?.is_none() {
        return Ok(object([
            ("sources", Value::Array(vec![])),
            ("pending", Value::Array(vec![])),
            ("completed", Value::Array(vec![])),
            ("faults", Value::Array(vec![string("source-missing")])),
            ("fingerprint", string(sha256_hex(b"missing"))),
            ("complete", Value::Bool(false)),
        ]));
    }
    fs::trust::state(directory)?;
    let mut sources = Vec::new();
    let mut faults = Vec::new();
    let mut pins = Vec::new();
    for i in 0..MAX_SOURCES {
        let source = read(&directory.join(format!("source-{i}.json")), MAX_SOURCE).and_then(|v| {
            if let Some(v) = &v {
                validate_source(v, identity)?;
            }
            Ok(v)
        });
        let source = match source {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(_) => {
                faults.push(string("source-reservation-unverifiable"));
                pins.push(Value::Array(vec![
                    Value::Int(i.into()),
                    string("unverifiable"),
                ]));
                continue;
            }
        };
        pins.push(Value::Array(vec![Value::Int(i.into()), source.clone()]));
        let path = directory.join(format!(
            "{}.done",
            get(&source, "observation_id").as_str().unwrap_or("")
        ));
        let completion = match read(&path, MAX_COMPLETION)
            .and_then(|v| v.map(|v| completed_record(&v, &source)).transpose())
        {
            Ok(v) => v,
            Err(_) => {
                faults.push(string("completion-unverifiable"));
                None
            }
        };
        sources.push(extend(
            &source,
            [
                ("completed", Value::Bool(completion.is_some())),
                ("completion", completion.unwrap_or(Value::Null)),
            ],
        ));
    }
    if named(&directory.join("fault.json"))?.is_some() {
        faults.push(string("registration-fault-retained"));
    }
    match enrollment_inventory(directory, identity) {
        Ok(enrollments) => {
            if !enrollments.is_empty() {
                pins.push(Value::Array(vec![
                    string("enrollments"),
                    Value::Array(enrollments.clone()),
                ]));
            }
            if enrollments
                .iter()
                .any(|v| get(v, "state").as_str() != Some("registered"))
            {
                faults.push(string("enrollment-unverified"));
            }
            if enrollments.iter().any(|v| {
                !sources
                    .iter()
                    .any(|s| get(s, "observation_id") == get(v, "observation_id"))
            }) {
                faults.push(string("enrollment-source-missing"));
            }
        }
        Err(_) => {
            faults.push(string("enrollment-unverifiable"));
            pins.push(Value::Array(vec![
                string("enrollments"),
                string("unverifiable"),
            ]));
        }
    }
    let mut unidentified = Vec::new();
    match unidentified_inventory(directory, identity) {
        Ok(retained) if !retained.is_empty() => {
            unidentified = retained
                .iter()
                .map(|v| get(v, "retention_id").clone())
                .collect();
            faults.push(string("unidentified-input-retained"));
            pins.push(Value::Array(vec![
                string("unidentified-inputs"),
                Value::Array(unidentified.clone()),
            ]));
        }
        Ok(_) => (),
        Err(_) => {
            faults.push(string("unidentified-input-unverifiable"));
            pins.push(Value::Array(vec![
                string("unidentified-inputs"),
                string("unverifiable"),
            ]));
        }
    }
    let complete = !sources.is_empty()
        && faults.is_empty()
        && sources
            .iter()
            .all(|v| get(v, "completed") == &Value::Bool(true));
    Ok(object([
        (
            "pending",
            Value::Array(
                sources
                    .iter()
                    .filter(|v| get(v, "completed") != &Value::Bool(true))
                    .cloned()
                    .collect(),
            ),
        ),
        (
            "completed",
            Value::Array(
                sources
                    .iter()
                    .filter(|v| get(v, "completed") == &Value::Bool(true))
                    .cloned()
                    .collect(),
            ),
        ),
        ("sources", Value::Array(sources)),
        ("faults", Value::Array(faults)),
        (
            "fingerprint",
            string(sha256_hex(&encoded(&Value::Array(pins))?)),
        ),
        ("complete", Value::Bool(complete)),
        ("unidentified_input_ids", Value::Array(unidentified)),
    ]))
}
pub fn private_inventory(
    directory: &Path,
    identity: &Value,
    context: &PrivateEnrollmentContext,
) -> Result<Value> {
    let value = inventory(directory, identity)?;
    if !context.held() {
        return Ok(value);
    }
    let fingerprint = sha256_hex(&encoded(&Value::Array(vec![
        get(&value, "fingerprint").clone(),
        string(context.context_digest()),
    ]))?);
    Ok(extend(
        &value,
        [
            ("complete", Value::Bool(false)),
            ("recovery_pending", Value::Bool(true)),
            ("semantic_coverage", string("incomplete-recovery")),
            ("fingerprint", string(fingerprint)),
        ],
    ))
}
pub fn pending_rows(state: &Value, directory: &Path, identity: &Value) -> Result<Vec<Value>> {
    let mut result: Vec<Value> = rows(state, "pending")
        .iter()
        .map(|v| {
            object([
                ("observation_id", get(v, "observation_id").clone()),
                ("source_sha256", get(v, "source_sha256").clone()),
            ])
        })
        .collect();
    let observed = rows(state, "sources");
    for intent in enrollment_inventory(directory, identity)? {
        if get(&intent, "state").as_str() != Some("registered")
            || !observed
                .iter()
                .any(|v| get(v, "observation_id") == get(&intent, "observation_id"))
        {
            let row = object([
                ("observation_id", get(&intent, "observation_id").clone()),
                ("source_sha256", get(&intent, "source_sha256").clone()),
            ]);
            if let Some(existing) = result
                .iter_mut()
                .find(|v| get(v, "observation_id") == get(&intent, "observation_id"))
            {
                *existing = row;
            } else {
                result.push(row);
            }
        }
    }
    Ok(result)
}
pub fn inspect_enrollment(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    context: &PrivateEnrollmentContext,
) -> Result<Value> {
    let ns = enrollment_namespace(repo, harness, session, lifecycle, context)?;
    if named(&ns.directory)?.is_none() {
        return private_inventory(&ns.directory, &ns.identity, context);
    }
    locked(&ns.directory, || {
        let value = private_inventory(&ns.directory, &ns.identity, context)?;
        if !same(
            &value,
            &private_inventory(&ns.directory, &ns.identity, context)?,
        )? {
            return Err(err("Source inventory changed; inspect again"));
        }
        scope::enrollment_read_context(repo, context)?;
        Ok(value)
    })
}
pub fn inspect(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    runtime_root: &Path,
) -> Result<Value> {
    operation(repo, runtime_root, || {
        let (ns, context) =
            private_namespace(repo, harness, session, lifecycle, false, runtime_root)?;
        if named(&ns.directory)?.is_none() {
            let value = private_inventory(&ns.directory, &ns.identity, &context)?;
            scope::recheck_private_enrollment(&context)?;
            return Ok(value);
        }
        locked(&ns.directory, || {
            let value = private_inventory(&ns.directory, &ns.identity, &context)?;
            scope::recheck_private_enrollment(&context)?;
            if !same(
                &value,
                &private_inventory(&ns.directory, &ns.identity, &context)?,
            )? {
                return Err(err("Source inventory changed; inspect again"));
            }
            scope::recheck_private_enrollment(&context)?;
            Ok(value)
        })
    })
}
pub fn get_source(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    observation_id: &str,
    runtime_root: &Path,
) -> Result<Value> {
    if !uuid(&string(observation_id), false) {
        return Err(err("Invalid observation UUID"));
    }
    let state = inspect(repo, harness, session, lifecycle, runtime_root)?;
    let sources = rows(&state, "sources");
    let mut matches = sources
        .iter()
        .filter(|v| get(v, "observation_id").as_str() == Some(observation_id));
    let source = matches
        .next()
        .ok_or_else(|| err("Original observation unavailable; preserve source state"))?;
    if matches.next().is_some() {
        return Err(err(
            "Original observation unavailable; preserve source state",
        ));
    }
    Ok(without(source, &["completed", "completion"]))
}

/// Only verify_assessment can mint this capability; request JSON cannot decode it.
pub struct VerifiedReceipt {
    value: Value,
    verification_value: Value,
    verification_source_id: String,
}
impl VerifiedReceipt {
    pub fn value(&self) -> &Value {
        &self.value
    }
}
pub fn verify_assessment(
    repo: &Path,
    value: &Value,
    source_id: &str,
    runtime_root: &Path,
) -> Result<VerifiedReceipt> {
    operation(repo, runtime_root, || {
        let context = scope::require(repo, None, RequireOptions::default())?;
        if get(value, "observation_id").as_str() != Some(source_id) {
            return Err(err("Assessment must identify its exact observation"));
        }
        if !digest(get(value, "source_sha256")) {
            return Err(err("Assessment must bind exact source digest"));
        }
        let witnessed = match get(value, "outcome").as_str() {
            Some("no-learning") => {
                if value.get("publication_request").is_some() {
                    return Err(err("No-learning cannot claim publication"));
                }
                crate::boundary_assessment::validate(get(value, "assessment"), source_id)?;
                Value::Null
            }
            Some("published") => {
                if !assessment_text(get(value, "assessment")) {
                    return Err(err("Published completion requires a bounded assessment"));
                }
                let gate = publication_gate_provider()?;
                let request = get(value, "publication_request");
                gate.request_value(repo, request)?;
                let proof = get(request, "proof");
                if truthy(get(proof, "blocked_paths")) || truthy(get(proof, "blocked_components")) {
                    return Err(err("Conflicted publication remains pending"));
                }
                let replay = gate.replay(
                    repo,
                    &object([
                        ("base", get(request, "base").clone()),
                        ("candidate", get(request, "candidate").clone()),
                    ]),
                    get(proof, "schema").as_str() == Some("agent-memory/semantic-sharing-proof-v2"),
                )?;
                if !truthy(get(&replay, "intended_delta_landed")) {
                    return Err(err("Publication is not witnessed on authority"));
                }
                replay
                    .get("witnessed_commit")
                    .cloned()
                    .ok_or_else(|| err("Publication is not witnessed on authority"))?
            }
            _ => return Err(err("Explicit source assessment required")),
        };
        let receipt = object([
            ("observation_id", string(source_id)),
            ("source_sha256", get(value, "source_sha256").clone()),
            ("outcome", get(value, "outcome").clone()),
            ("assessment", get(value, "assessment").clone()),
            ("assessment_is_agent_assertion", Value::Bool(true)),
            ("source_publication_link_verified", Value::Bool(false)),
            (
                "publication_verified",
                Value::Bool(witnessed != Value::Null),
            ),
            ("witnessed_commit", witnessed),
            ("input_sha256", string(sha256_hex(&encoded(value)?))),
            ("canonical_writes_performed", Value::Bool(false)),
        ]);
        scope::require(
            repo,
            get(&context, "project_id").as_str(),
            RequireOptions {
                expected: Some(&context),
                ..RequireOptions::default()
            },
        )?;
        Ok(VerifiedReceipt {
            value: receipt,
            verification_value: value.clone(),
            verification_source_id: source_id.into(),
        })
    })
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Int(v) => v.as_str() != "0",
        Value::Float(v) => *v != 0.0,
    }
}
pub fn cleanup(ns: &Namespace) -> Result<()> {
    let pending = ns.base.join("pending");
    if named(&pending)?.is_none() {
        return Ok(());
    }
    let expected = reference(&ns.directory, &ns.identity)?;
    for i in 0..MAX_TURNS {
        let path = pending.join(format!("{i}.json"));
        let Ok(Some(raw)) = read_bytes(&path, MAX_SOURCE) else {
            continue;
        };
        let Ok(value) = load_with_limit(&raw, MAX_SOURCE) else {
            continue;
        };
        if same(&value, &expected)? {
            if get(&inventory(&ns.directory, &ns.identity)?, "complete") != &Value::Bool(true) {
                return Ok(());
            }
            remove_observed(&path, &raw, MAX_SOURCE)?;
            if get(&inventory(&ns.directory, &ns.identity)?, "complete") != &Value::Bool(true) {
                index(ns, None)?;
            }
        }
    }
    Ok(())
}
pub fn complete(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    observation_id: &str,
    source_digest: &str,
    receipt: &VerifiedReceipt,
    runtime_root: &Path,
) -> Result<Value> {
    operation(repo, runtime_root, || {
        let (ns, context) = namespace(repo, harness, session, lifecycle, false)?;
        if receipt.verification_source_id != observation_id
            || get(&receipt.verification_value, "observation_id").as_str() != Some(observation_id)
            || get(&receipt.verification_value, "source_sha256").as_str() != Some(source_digest)
        {
            return Err(err("Verified assessment belongs to another observation"));
        }
        validate_receipt(&receipt.value, observation_id, source_digest)?;
        let value = object([
            ("schema", string("agent-memory/source-completed-v2")),
            ("observation_id", string(observation_id)),
            ("source_sha256", string(source_digest)),
            ("receipt", receipt.value.clone()),
        ]);
        let value = extend(
            &value,
            [("record_sha256", string(sha256_hex(&encoded(&value)?)))],
        );
        if encoded(&value)?.len() > MAX_COMPLETION {
            return Err(err("Encoded completion exceeds bound"));
        }
        locked(&ns.directory, || {
            let checked = verify_assessment(
                &ns.root,
                &receipt.verification_value,
                &receipt.verification_source_id,
                runtime_root,
            )?;
            if !same(&checked.value, &receipt.value)? {
                return Err(err("Assessment evidence changed before completion"));
            }
            let state = inventory(&ns.directory, &ns.identity)?;
            if rows(&state, "sources")
                .iter()
                .filter(|v| {
                    get(v, "observation_id").as_str() == Some(observation_id)
                        && get(v, "source_sha256").as_str() == Some(source_digest)
                })
                .count()
                != 1
            {
                return Err(err("Completion must match an original observation"));
            }
            let path = ns.directory.join(format!("{observation_id}.done"));
            let already = if named(&path)?.is_some() {
                if !read(&path, MAX_COMPLETION)?
                    .as_ref()
                    .map(|v| same(v, &value))
                    .transpose()?
                    .unwrap_or(false)
                {
                    return Err(err("Acknowledgment differs from retained completion"));
                }
                true
            } else {
                if !exclusive(&path, &value, MAX_COMPLETION)? {
                    return Err(err("Acknowledgment differs from retained completion"));
                }
                false
            };
            scope::require(
                &ns.root,
                get(&context, "project_id").as_str(),
                RequireOptions {
                    expected: Some(&context),
                    ..RequireOptions::default()
                },
            )?;
            cleanup(&ns)?;
            Ok(extend(
                &receipt.value,
                [
                    ("verified", Value::Bool(true)),
                    ("already_completed", Value::Bool(already)),
                ],
            ))
        })
    })
}
pub fn reserve_wake(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    fingerprint: &str,
    generated_sha256: &str,
    runtime_root: &Path,
) -> Result<bool> {
    operation(repo, runtime_root, || {
        let (ns, context) =
            private_namespace(repo, harness, session, lifecycle, false, runtime_root)?;
        if !digest(&string(fingerprint)) || !digest(&string(generated_sha256)) {
            return Err(err("Invalid wake digest"));
        }
        locked(&ns.directory, || {
            let state = private_inventory(&ns.directory, &ns.identity, &context)?;
            scope::recheck_private_enrollment(&context)?;
            if get(&state, "complete") == &Value::Bool(true)
                || get(&state, "fingerprint").as_str() != Some(fingerprint)
            {
                return Ok(false);
            }
            for i in 0..2 {
                let path = ns.directory.join(format!("wake-{i}.json"));
                if let Some(old) = read(&path, MAX_SOURCE)? {
                    wake_value(&old)?;
                    if get(&old, "fingerprint").as_str() == Some(fingerprint) {
                        scope::recheck_private_enrollment(&context)?;
                        return Ok(false);
                    }
                } else {
                    scope::recheck_private_enrollment(&context)?;
                    if !exclusive(
                        &path,
                        &object([
                            ("schema", string("agent-memory/turn-wake-v2")),
                            ("fingerprint", string(fingerprint)),
                            ("generated_sha256", string(generated_sha256)),
                        ]),
                        MAX_SOURCE,
                    )? {
                        return Err(err("Wake reservation changed; preserve pending work"));
                    }
                    scope::recheck_private_enrollment(&context)?;
                    return Ok(true);
                }
            }
            scope::recheck_private_enrollment(&context)?;
            Ok(false)
        })
    })
}
fn backlog_error(result: &mut Value, code: &str) {
    if let Some(Value::Array(faults)) = result.get_mut("faults") {
        let code = string(code);
        if !faults.contains(&code) {
            faults.push(code);
        }
    }
}
fn backlog_append(result: &mut Value, key: &str, value: Value) -> Result<bool> {
    let mut values = rows(result, key);
    values.push(value);
    let trial = extend(result, [(key, Value::Array(values))]);
    if encoded(&trial)?.len() > MAX_BACKLOG {
        return Ok(false);
    }
    *result = trial;
    Ok(true)
}
fn backlog_row(ns: &Namespace, state: &Value) -> Result<Value> {
    Ok(extend(
        &ns.identity,
        [
            (
                "pending",
                Value::Array(pending_rows(state, &ns.directory, &ns.identity)?),
            ),
            ("faults", get(state, "faults").clone()),
            ("fingerprint", get(state, "fingerprint").clone()),
            ("complete", Value::Bool(false)),
            (
                "unidentified_input_ids",
                Value::Array(rows(state, "unidentified_input_ids")),
            ),
        ],
    ))
}
pub fn backlog(
    repo: &Path,
    harness: Option<&str>,
    offset: usize,
    legacy_offset: usize,
    enrollment_offset: usize,
    runtime_root: &Path,
) -> Result<Value> {
    operation(repo, runtime_root, || {
        if offset > MAX_TURNS || legacy_offset > 4096 || enrollment_offset > MAX_NAMESPACES {
            return Err(err("Invalid backlog page offset"));
        }
        let (ns, context) = private_namespace(
            repo,
            harness.unwrap_or("codex"),
            "backlog",
            "backlog",
            false,
            runtime_root,
        )?;
        let mut result = object([
            ("turns", Value::Array(vec![])),
            ("legacy", Value::Array(vec![])),
            ("faults", Value::Array(vec![])),
            ("next_offset", Value::Null),
            ("next_legacy_offset", Value::Null),
            ("next_enrollment_offset", Value::Null),
            ("source_completeness_verified", Value::Bool(false)),
            ("recovery_pending", Value::Bool(context.held())),
        ]);
        let mut indexed = BTreeSet::new();
        let pending = ns.base.join("pending");
        if named(&pending)?.is_some() {
            fs::trust::state(&pending)?;
            for i in 0..MAX_TURNS {
                let value = match read(&pending.join(format!("{i}.json")), MAX_SOURCE) {
                    Ok(Some(v)) => v,
                    Ok(None) => continue,
                    Err(_) => {
                        backlog_error(&mut result, "index-reservation-unverifiable");
                        continue;
                    }
                };
                if !fields(
                    &value,
                    &[
                        "schema",
                        "harness",
                        "session",
                        "lifecycle",
                        "project_id",
                        "turn",
                    ],
                ) || get(&value, "schema").as_str() != Some("agent-memory/turn-reference-v2")
                    || get(&value, "project_id") != context.project_id()
                {
                    backlog_error(&mut result, "index-reference-unverifiable");
                    continue;
                }
                validate_reference(&value, &ns.root, context.project_id())?;
                indexed.insert(get(&value, "turn").as_str().unwrap_or("").to_owned());
                if i < offset || harness.is_some_and(|h| get(&value, "harness").as_str() != Some(h))
                {
                    continue;
                }
                let row = (|| {
                    let h = bounded(get(&value, "harness"), 512)?;
                    let s = bounded(get(&value, "session"), 512)?;
                    let l = bounded(get(&value, "lifecycle"), 512)?;
                    let (turn, _) = private_namespace(repo, h, s, l, false, runtime_root)?;
                    if leaf(&turn.directory)?.to_str() != get(&value, "turn").as_str() {
                        return Err(err("Index namespace changed"));
                    }
                    let state = inspect(repo, h, s, l, runtime_root)?;
                    if get(&state, "complete") == &Value::Bool(true) {
                        return Ok(None);
                    }
                    Ok(Some(backlog_row(&turn, &state)?))
                })();
                match row {
                    Ok(Some(row)) => {
                        if !backlog_append(&mut result, "turns", row)? {
                            result = extend(&result, [("next_offset", Value::Int(i.into()))]);
                            break;
                        }
                    }
                    Ok(None) => (),
                    Err(_) => backlog_error(&mut result, "pending-turn-unavailable"),
                }
            }
        }
        let turns = ns.base.join("turns");
        if named(&turns)?.is_some() {
            let names = entries(
                &turns,
                MAX_NAMESPACES,
                "Source namespace inventory exceeds bound",
            )?;
            for (position, name) in names.into_iter().enumerate() {
                if position < enrollment_offset
                    || name.to_str().is_some_and(|n| indexed.contains(n))
                {
                    continue;
                }
                let directory = turns.join(name);
                let row: Result<Option<Value>> = (|| {
                    if named(&directory.join("enrollments"))?.is_none()
                        && named(&directory.join("unidentified-inputs"))?.is_none()
                    {
                        return Ok(None);
                    }
                    fs::trust::state(&directory)?;
                    let target = if named(&directory.join("enrollments"))?.is_some() {
                        directory.join("enrollments")
                    } else {
                        directory.join("unidentified-inputs")
                    };
                    let names = entries(
                        &target,
                        MAX_SOURCES,
                        "Held enrollment inventory exceeds source capacity",
                    )?;
                    let Some(first_name) = names.first() else {
                        return Ok(None);
                    };
                    let first = read(&target.join(first_name), MAX_RETAINED_INPUT)?
                        .ok_or_else(|| err("Held enrollment intent is unavailable"))?;
                    let identity = object(
                        ["harness", "session", "lifecycle", "project_id"]
                            .into_iter()
                            .map(|key| (key, get(&first, key).clone())),
                    );
                    validate_reference(
                        &reference(&directory, &identity)?,
                        &ns.root,
                        context.project_id(),
                    )?;
                    let h = bounded(get(&identity, "harness"), 512)?;
                    if harness.is_some_and(|selected| selected != h) {
                        return Ok(None);
                    }
                    let s = bounded(get(&identity, "session"), 512)?;
                    let l = bounded(get(&identity, "lifecycle"), 512)?;
                    let state = inspect(&ns.root, h, s, l, runtime_root)?;
                    if get(&state, "complete") == &Value::Bool(true) {
                        return Ok(None);
                    }
                    let (turn, _) = private_namespace(repo, h, s, l, false, runtime_root)?;
                    Ok(Some(backlog_row(&turn, &state)?))
                })();
                match row {
                    Ok(Some(row)) => {
                        if !backlog_append(&mut result, "turns", row)? {
                            result = extend(
                                &result,
                                [("next_enrollment_offset", Value::Int(position.into()))],
                            );
                            break;
                        }
                    }
                    Ok(None) => (),
                    Err(_) => backlog_error(&mut result, "enrollment-orphan-unverifiable"),
                }
            }
        }
        let (operations, _) = writes::state_paths(
            &ns.root,
            &writes::bank_path(&ns.root)?.1,
            &crate::host::uuid4()?,
        )?;
        let mut seen = 0;
        for name in ["codex", "claude"] {
            if harness.is_some_and(|h| h != name) {
                continue;
            }
            let directory = parent(&operations)?.join(format!("{name}-boundaries"));
            if named(&directory)?.is_none() {
                continue;
            }
            for file in entries(&directory, MAX_NAMESPACES, "Legacy inventory exceeds bound")? {
                if !file.to_str().is_some_and(|s| s.ends_with(".pending")) {
                    continue;
                }
                if seen >= 4096 {
                    return Err(err("Legacy pending exceeds bound"));
                }
                let position = seen;
                seen += 1;
                if position < legacy_offset {
                    continue;
                }
                let row = (|| {
                    let source = read(&directory.join(&file), MAX_SOURCE)?
                        .ok_or_else(|| err("Legacy project differs"))?;
                    if source.as_object().is_none()
                        || get(&source, "project_id") != context.project_id()
                    {
                        return Err(err("Legacy project differs"));
                    }
                    let selected = object(
                        [
                            "schema",
                            "project_id",
                            "session",
                            "turn",
                            "source_uuid",
                            "source_sha256",
                        ]
                        .into_iter()
                        .filter_map(|k| source.get(k).map(|v| (k, v.clone()))),
                    );
                    Ok(object([
                        ("harness", string(name)),
                        (
                            "legacy_key",
                            string(crate::host::text(&file, "legacy source key")?),
                        ),
                        ("source", selected),
                    ]))
                })();
                match row {
                    Ok(row) => {
                        if !backlog_append(&mut result, "legacy", row)? {
                            result = extend(
                                &result,
                                [("next_legacy_offset", Value::Int(position.into()))],
                            );
                            scope::recheck_private_enrollment(&context)?;
                            return Ok(result);
                        }
                    }
                    Err(_) => backlog_error(&mut result, "legacy-pending-unverifiable"),
                }
            }
        }
        scope::recheck_private_enrollment(&context)?;
        Ok(result)
    })
}
pub fn pending_notice(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    runtime_root: &Path,
) -> Result<Option<Value>> {
    operation(repo, runtime_root, || {
        let context = scope::private_enrollment_context(repo, runtime_root)?;
        let pending = backlog(repo, Some(harness), 0, 0, 0, runtime_root)?;
        scope::recheck_private_enrollment(&context)?;
        let earlier: Vec<Value> = rows(&pending, "turns")
            .into_iter()
            .filter(|v| {
                get(v, "session").as_str() != Some(session)
                    || get(v, "lifecycle").as_str() != Some(lifecycle)
            })
            .collect();
        let summary = object([
            ("turns", Value::Int(earlier.len().into())),
            ("legacy", Value::Int(rows(&pending, "legacy").len().into())),
            ("faults", get(&pending, "faults").clone()),
            (
                "paged",
                Value::Bool(
                    [
                        "next_offset",
                        "next_legacy_offset",
                        "next_enrollment_offset",
                    ]
                    .iter()
                    .any(|k| get(&pending, k) != &Value::Null),
                ),
            ),
            (
                "recovery_pending",
                get(&pending, "recovery_pending").clone(),
            ),
        ]);
        if !summary
            .as_object()
            .into_iter()
            .flatten()
            .any(|(_, v)| truthy(v))
        {
            return Ok(None);
        }
        let fingerprint = sha256_hex(&encoded(&object([
            ("turns", Value::Array(earlier)),
            ("legacy", get(&pending, "legacy").clone()),
            ("faults", get(&pending, "faults").clone()),
            (
                "recovery_pending",
                get(&pending, "recovery_pending").clone(),
            ),
        ]))?);
        let ns = namespace_at(
            context.root(),
            context.bank(),
            context.project_id(),
            harness,
            session,
            "pending-notice",
        )?;
        scope::recheck_private_enrollment(&context)?;
        private_mkdir(&ns.directory)?;
        locked(&ns.directory, || {
            let path = ns.directory.join("notice.json");
            let value = object([("fingerprint", string(fingerprint))]);
            if read(&path, MAX_SOURCE)?
                .as_ref()
                .map(|v| same(v, &value))
                .transpose()?
                .unwrap_or(false)
            {
                scope::recheck_private_enrollment(&context)?;
                return Ok(None);
            }
            let mut store = NoticeStore {
                context: &context,
                path,
            };
            crate::turn_check::NoticeStore::replace(&mut store, &value, MAX_SOURCE)
                .map_err(ContractError::from)?;
            scope::recheck_private_enrollment(&context)?;
            Ok(Some(summary))
        })
    })
}

pub struct NoticeStore<'a> {
    context: &'a PrivateEnrollmentContext,
    path: PathBuf,
}
impl crate::turn_check::NoticeStore for NoticeStore<'_> {
    fn read(&mut self, maximum: usize) -> crate::turn_check::CompletionResult<Option<Value>> {
        Ok(read(&self.path, maximum)?)
    }
    fn replace(
        &mut self,
        value: &Value,
        maximum: usize,
    ) -> crate::turn_check::CompletionResult<()> {
        notice_replace(self.context, &self.path, value, maximum)?;
        Ok(())
    }
    fn remove(&mut self) -> crate::turn_check::CompletionResult<()> {
        scope::recheck_private_enrollment(self.context)?;
        if let Some(raw) = read_bytes(&self.path, MAX_SOURCE)? {
            remove_observed(&self.path, &raw, MAX_SOURCE)?;
        }
        scope::recheck_private_enrollment(self.context)?;
        Ok(())
    }
}
/// Both callee traits are implemented by one real source provider.
pub struct TurnSources;
impl writes::TurnSources for TurnSources {
    fn canonical_writer_barrier(&self, repo: &Path) -> Result<Box<dyn CanonicalWriterBarrier>> {
        Ok(Box::new(canonical_writer_barrier(
            repo,
            None,
            &crate::cli::pack_root()?,
        )?))
    }
    fn canonical_transaction_barrier(
        &self,
        repo: &Path,
        ordinary_bank: Option<&Path>,
    ) -> Result<Box<dyn CanonicalWriterBarrier>> {
        Ok(Box::new(canonical_writer_barrier(
            repo,
            ordinary_bank,
            &crate::cli::pack_root()?,
        )?))
    }
    fn source_barrier(
        &self,
        repo: &Path,
        exclusive: bool,
        existing_only: bool,
        runtime_root: &Path,
    ) -> Result<Box<dyn CanonicalWriterBarrier>> {
        Ok(Box::new(source_barrier(
            repo,
            exclusive,
            existing_only,
            None,
            runtime_root,
        )?))
    }
    fn check_source_compatibility(&self, base: &Path) -> Result<()> {
        check_source_compatibility(base)
    }
    fn source_barrier_held(&self, base: &Path) -> Result<bool> {
        Ok(source_barrier_held(base))
    }
}
impl crate::turn_check::TurnSources for TurnSources {
    fn with_notice_store(
        &self,
        repo: &Path,
        harness: &str,
        session: &str,
        action: &mut dyn FnMut(&mut dyn crate::turn_check::NoticeStore) -> Result<bool>,
    ) -> crate::turn_check::CompletionResult<bool> {
        Ok(operation(repo, &crate::cli::pack_root()?, || {
            let (ns, context) = private_namespace(
                repo,
                harness,
                session,
                "refresh-notice",
                true,
                &crate::cli::pack_root()?,
            )?;
            locked(&ns.directory, || {
                let mut store = NoticeStore {
                    context: &context,
                    path: ns.directory.join("notice.json"),
                };
                action(&mut store)
            })
        })?)
    }
}

fn notice_replace(
    context: &PrivateEnrollmentContext,
    path: &Path,
    value: &Value,
    maximum: usize,
) -> Result<()> {
    let directory = parent(path)?;
    let chain = DirectoryChain::open(directory).map_err(io)?;
    let old = read_bytes(path, maximum)?;
    let old_identity = named(path)?;
    let temporary = directory.join(format!("{}.tmp", crate::host::uuid4()?));
    let mut created = false;
    let outcome = (|| {
        scope::recheck_private_enrollment(context)?;
        if !exclusive(&temporary, value, maximum)? {
            return Err(err("Private notice temporary is already occupied"));
        }
        created = true;
        scope::recheck_private_enrollment(context)?;
        chain.recheck().map_err(io)?;
        if read_bytes(path, maximum)? != old || named(path)? != old_identity {
            return Err(err(
                "Private notice changed before replacement; preserve pending evidence",
            ));
        }
        fs::rename_at(
            chain.directory(),
            leaf(&temporary)?,
            chain.directory(),
            leaf(path)?,
            fs::RenameMode::Replace,
        )
        .map_err(io)?;
        chain.directory().fsync_directory().map_err(io)?;
        chain.recheck().map_err(io)?;
        scope::recheck_private_enrollment(context)?;
        Ok(())
    })();
    if created {
        if let Some(raw) = read_bytes(&temporary, maximum)? {
            remove_observed(&temporary, &raw, maximum)?;
        }
    }
    outcome
}
