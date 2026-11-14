//! Disposable authenticated metadata. Canonical Markdown is always authoritative.
//!
//! Stamps are hints only: a loaded witness must pass `Catalog::recheck` before
//! qualified output. Rust generations bind the runtime artifact, never Python
//! bytecode, interpreter versions, or cache tags. Old witnesses are cache misses.
use crate::bank_contract as contract;
use crate::bank_observation::{self as observation, Observation, Physical};
use crate::bank_writes::{self as writes, StateEnvironment};
use crate::codec::json::{self, Value};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain, FlockGuard, LockMode, trust};
use crate::{host, local_registry as storage, package, records, repository_config};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/recall-catalog-v1";
pub const MAX_CACHE: usize = 256 * 1024 * 1024;
pub const MAX_HEADER: usize = 16384;
pub const MAX_NAMESPACE: usize = 256;
pub const MAX_POINTER: usize = 4096;
pub const MAX_KEY: usize = 4096;
pub const MAX_RECORD: usize = MAX_CACHE + MAX_HEADER + 1;
pub const MAX_SOURCE: usize = 16384;
pub const MAX_COLLECTION: usize = 2 * MAX_CACHE;

fn miss(message: &str) -> ContractError {
    ContractError::recall_catalog_miss(message)
}
fn io_error(_: impl std::fmt::Display) -> ContractError {
    miss("cache filesystem unavailable or changed")
}
fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn object(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| miss("catalog payload shape"))
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)?
        .as_str()
        .ok_or_else(|| miss("catalog payload shape"))
}
fn array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .ok_or_else(|| miss("catalog payload shape"))
}
fn fields(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|entries| {
        entries.len() == expected.len() && expected.iter().all(|key| value.get(key).is_some())
    })
}
pub fn compact(value: &Value) -> Result<Vec<u8>> {
    Ok(json::encode(value, json::SORTED_COMPACT_UTF8_FINITE)?.into_bytes())
}
fn decode(raw: &[u8], maximum: usize) -> Result<Value> {
    json::decode_bytes(
        raw,
        json::DecodePolicy {
            max_bytes: maximum,
            ..Default::default()
        },
    )
}
fn same(left: &Value, right: &Value) -> Result<bool> {
    Ok(compact(left)? == compact(right)?)
}
fn sha(raw: &[u8]) -> String {
    crate::validation_memo::hex(&Sha256::digest(raw))
}
fn disabled() -> bool {
    std::env::var_os("AGENT_MEMORY_NO_RECALL_CATALOG").is_some_and(|v| v == "1")
}

#[derive(Clone, Debug)]
pub struct Runtime {
    pub pack: PathBuf,
    pub binary: PathBuf,
}
#[derive(Clone, Debug)]
pub struct Generation {
    value: Value,
    runtime: Runtime,
}
impl Generation {
    pub fn value(&self) -> &Value {
        &self.value
    }
}
/// The replacement for all Python function/class/code/opaque-token identities.
pub fn generation_details(runtime: &Runtime) -> Result<Value> {
    if crate::validation_memo::current().is_some() {
        return Err(miss(
            "native catalog reuse is separate from active operation validation reuse",
        ));
    }
    let digest = package::engine_generation(&runtime.pack, &runtime.binary)
        .map_err(|_| miss("executing package generation unavailable"))?;
    Ok(object([("artifact_digest", s(digest))]))
}
pub fn generation(runtime: &Runtime) -> Result<Generation> {
    Ok(Generation {
        value: generation_details(runtime)?,
        runtime: runtime.clone(),
    })
}
pub fn coherent(engine: &Generation) -> Result<()> {
    if !same(&generation_details(&engine.runtime)?, &engine.value)? {
        return Err(miss("validator generation or dependency closure changed"));
    }
    Ok(())
}

pub fn normalized_physical(physical: &Physical) -> Physical {
    physical
        .iter()
        .filter(|(path, _)| !contract::runtime_path(path))
        .map(|(path, kind)| (path.clone(), kind.clone()))
        .collect()
}
/// Every physical entry, including excluded runtime descendants, spends budget.
pub fn physical_inventory(bank: &Path) -> Result<(Physical, Value)> {
    physical_inventory_with_limit(bank, observation::MAX_FILES)
}
pub fn physical_inventory_with_limit(
    bank: &Path,
    maximum_files: usize,
) -> Result<(Physical, Value)> {
    if maximum_files > observation::MAX_FILES {
        return Err(miss("bank physical bound"));
    }
    let maximum = maximum_files.saturating_mul(2);
    let mut pending = vec![(PathBuf::new(), 0usize)];
    let mut visited = 0usize;
    let mut physical = Physical::new();
    let mut stamps = BTreeMap::new();
    while let Some((relative, depth)) = pending.pop() {
        if depth > observation::MAX_DEPTH {
            return Err(miss("bank physical bound"));
        }
        let chain = DirectoryChain::open(&bank.join(&relative)).map_err(io_error)?;
        let directory = chain.directory().reopen_directory().map_err(io_error)?;
        let names = directory
            .entries_bounded(maximum.saturating_sub(visited))
            .map_err(io_error)?
            .ok_or_else(|| miss("bank physical bound"))?;
        for name in names {
            visited += 1;
            chain.recheck().map_err(io_error)?;
            let info = directory.stat_at(&name).map_err(io_error)?;
            if info.is_symlink() {
                return Err(miss("bank path alias"));
            }
            let path = relative.join(&name);
            let label = host::text(path.as_os_str(), "catalog source path")?.to_owned();
            let kind = if info.is_directory() {
                "directory"
            } else if info.is_regular() {
                "file"
            } else {
                "other"
            };
            if !contract::runtime_path(&label) {
                physical.insert(label.clone(), kind.into());
                if info.is_regular() && label.ends_with(".md") {
                    stamps.insert(
                        label,
                        Value::Array(vec![
                            Value::Int(info.dev.into()),
                            Value::Int(info.ino.into()),
                            Value::Int(info.size.into()),
                            Value::Int(info.mtime_ns.into()),
                            Value::Int(info.ctime_ns.into()),
                            Value::Int(info.mode.into()),
                            Value::Int(info.nlink.into()),
                        ]),
                    );
                }
            }
            if info.is_directory() {
                pending.push((path, depth + 1));
            }
        }
        chain.recheck().map_err(io_error)?;
    }
    observation::physical_contract(&physical)?;
    Ok((physical, object(stamps)))
}
fn physical_value(physical: &Physical) -> Value {
    object(physical.iter().map(|(path, kind)| (path.clone(), s(kind))))
}

pub fn locations(repo: &Path, runtime_root: &Path) -> Result<(PathBuf, PathBuf, PathBuf)> {
    locations_with_environment(repo, runtime_root, &StateEnvironment::current())
}
pub fn locations_with_environment(
    repo: &Path,
    runtime_root: &Path,
    environment: &StateEnvironment,
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let (root, bank) = writes::bank_path(repo)?;
    let (operations, _) = writes::state_paths_with(&root, &bank, &host::uuid4()?, environment)?;
    let directory = operations
        .parent()
        .ok_or_else(|| miss("cache location unavailable"))?
        .join("recall-catalog-v1");
    let canonical = fs::canonical_path(&directory, false)?;
    let source = fs::canonical_path(runtime_root, true)?;
    if canonical.starts_with(&source) || canonical.starts_with(&root) {
        return Err(miss("cache must stay outside source and checkout"));
    }
    trust::inspect(&directory, trust::InspectOptions::default())?;
    Ok((root, bank, directory))
}

pub fn read_private(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>> {
    let inspection = trust::record(path)?;
    if !inspection.present {
        return Ok(None);
    }
    let parent = path.parent().ok_or_else(|| miss("cache record path"))?;
    let name = path.file_name().ok_or_else(|| miss("cache record path"))?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    let mut file = chain
        .directory()
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    let before = file.stat().map_err(io_error)?;
    private_record(before)?;
    let raw = file
        .read_bounded(maximum)
        .map_err(|_| miss("cache record bound or changed"))?;
    if file.stat().map_err(io_error)? != before
        || chain.directory().stat_at(name).map_err(io_error)? != before
    {
        return Err(miss("cache record changed"));
    }
    chain.recheck().map_err(io_error)?;
    Ok(Some(raw))
}
fn private_record(info: fs::Identity) -> Result<()> {
    if !info.is_regular()
        || info.nlink != 1
        || info.uid != host::effective_uid()
        || info.mode & 0o077 != 0
    {
        return Err(miss("cache record permissions"));
    }
    Ok(())
}
fn recovery_name() -> Result<String> {
    Ok(format!("key-recovery-{}", host::uuid4()?.replace('-', "")))
}
pub fn key(directory: &Path, create: bool) -> Result<Vec<u8>> {
    let path = directory.join("key");
    let raw = match read_private(&path, MAX_KEY) {
        Ok(raw) => raw,
        Err(error) if create => {
            let chain = DirectoryChain::open(directory).map_err(io_error)?;
            let info = chain
                .directory()
                .stat_at(OsStr::new("key"))
                .map_err(io_error)?;
            private_record(info)?;
            // Only a bounded-read failure on an otherwise private record is recoverable.
            // Recheck the inode before preserving an oversized key without reading it.
            if info.size <= MAX_KEY as i64 {
                return Err(error);
            }
            chain.recheck().map_err(io_error)?;
            fs::rename_at(
                chain.directory(),
                OsStr::new("key"),
                chain.directory(),
                OsStr::new(&recovery_name()?),
                fs::RenameMode::NoReplace,
            )
            .map_err(io_error)?;
            chain.directory().fsync_directory().map_err(io_error)?;
            chain.recheck().map_err(io_error)?;
            None
        }
        Err(error) => return Err(error),
    };
    if let Some(raw) = raw.as_ref().filter(|raw| raw.len() == 32) {
        return Ok(raw.clone());
    }
    if !create {
        return Err(miss("cache key unavailable"));
    }
    if let Some(raw) = raw.as_deref() {
        storage::replace_with_limit(&directory.join(recovery_name()?), raw, None, MAX_KEY)?;
    }
    let fresh = host::random_32()?.to_vec();
    let expected = raw.as_deref().map(sha);
    if storage::replace_with_limit(&path, &fresh, expected.as_deref(), MAX_KEY).is_err() {
        return read_private(&path, MAX_KEY)?
            .filter(|raw| raw.len() == 32)
            .ok_or_else(|| miss("cache key publication race"));
    }
    if read_private(&path, MAX_KEY)?.as_deref() != Some(fresh.as_slice()) {
        return Err(miss("cache key readback"));
    }
    Ok(fresh)
}

pub fn binding(root: &Path, bank: &Path, project: &str) -> Result<Value> {
    let settings = repository_config::settings(root)?;
    let settings = object([
        ("bank_root", s(settings.bank_root)),
        (
            "authority_branch",
            settings.authority_branch.map(s).unwrap_or(Value::Null),
        ),
    ]);
    Ok(object([
        (
            "repo_root",
            s(host::text(root.as_os_str(), "catalog repository")?),
        ),
        (
            "bank_root",
            s(host::text(bank.as_os_str(), "catalog bank")?),
        ),
        ("project_id", s(project)),
        ("repository_settings_sha256", s(sha(&compact(&settings)?))),
    ]))
}
fn generation_name(name: &str) -> bool {
    name.strip_prefix("g-")
        .and_then(|v| v.strip_suffix(".catalog"))
        .is_some_and(|id| {
            id.len() == 32
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
pub fn pointer(directory: &Path) -> Result<(Value, Vec<u8>)> {
    let raw = read_private(&directory.join("current.json"), MAX_POINTER)?
        .ok_or_else(|| miss("catalog missing"))?;
    let value = decode(&raw, MAX_POINTER)?;
    if !fields(&value, &["schema", "file"])
        || text(&value, "schema")? != SCHEMA
        || !generation_name(text(&value, "file")?)
    {
        return Err(miss("catalog pointer format"));
    }
    Ok((value, raw))
}
pub fn namespace(directory: &Path) -> Result<Vec<OsString>> {
    let chain = DirectoryChain::open(directory).map_err(io_error)?;
    let names = chain
        .directory()
        .reopen_directory()
        .map_err(io_error)?
        .entries_bounded(MAX_NAMESPACE)
        .map_err(io_error)?
        .ok_or_else(|| miss("cache namespace needs bounded maintenance"))?;
    chain.recheck().map_err(io_error)?;
    Ok(names)
}
/// No PID marker, stale-lock recovery, or blocking optional publisher.
pub fn publication_lock(directory: &Path) -> Result<FlockGuard> {
    let path = directory.join("publication.lock");
    let inspection = trust::record(&path)?;
    if !inspection.present && namespace(directory)?.len() >= MAX_NAMESPACE {
        return Err(miss("cache namespace capacity"));
    }
    let chain = DirectoryChain::open(directory).map_err(io_error)?;
    let (file, _) = chain
        .directory()
        .open_or_create(
            OsStr::new("publication.lock"),
            libc::O_RDWR | libc::O_NONBLOCK,
            0o600,
        )
        .map_err(io_error)?;
    let info = file.stat().map_err(io_error)?;
    private_record(info).map_err(|_| miss("cache publication lock permissions"))?;
    let guard = FlockGuard::try_acquire(file, LockMode::Exclusive)
        .map_err(|_| miss("another optional publisher is active"))?;
    if !chain
        .directory()
        .stat_at(OsStr::new("publication.lock"))
        .map_err(io_error)?
        .same_inode(info)
    {
        return Err(miss("cache publication lock changed"));
    }
    chain.recheck().map_err(io_error)?;
    Ok(guard)
}

/// RFC 2104 SHA-256 with streaming input; no extra copy of a cache payload.
fn mac(secret: &[u8], unsigned: &[u8], payload: &[u8]) -> String {
    let hashed;
    let key = if secret.len() > 64 {
        hashed = Sha256::digest(secret);
        hashed.as_slice()
    } else {
        secret
    };
    let mut inner_pad = [0x36u8; 64];
    let mut outer_pad = [0x5cu8; 64];
    for ((inner, outer), byte) in inner_pad.iter_mut().zip(outer_pad.iter_mut()).zip(key) {
        *inner ^= byte;
        *outer ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(unsigned);
    inner.update(b"\n");
    inner.update(payload);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    crate::validation_memo::hex(&outer.finalize())
}
fn constant_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for (a, b) in left.iter().zip(right) {
        difference |= usize::from(a ^ b);
    }
    difference == 0
}
fn split_record(raw: &[u8]) -> Result<(&[u8], &[u8])> {
    let end = raw
        .iter()
        .take(MAX_HEADER + 1)
        .position(|b| *b == b'\n')
        .ok_or_else(|| miss("catalog header bound"))?;
    let first = raw.get(..end).ok_or_else(|| miss("catalog header bound"))?;
    let payload = raw
        .get(end + 1..)
        .ok_or_else(|| miss("catalog header bound"))?;
    Ok((first, payload))
}
pub fn authenticate<'a>(
    raw: &'a [u8],
    root: &Path,
    bank: &Path,
    project: &str,
    secret: &[u8],
    strict_settings: bool,
) -> Result<(Value, &'a [u8])> {
    let (first, payload) = split_record(raw)?;
    let header = decode(first, MAX_HEADER)?;
    if !fields(
        &header,
        &[
            "schema",
            "binding",
            "engine",
            "payload_bytes",
            "payload_sha256",
            "mac",
        ],
    ) || text(&header, "schema")? != SCHEMA
    {
        return Err(miss("catalog binding/digest"));
    }
    let bound = get(&header, "binding")?;
    if strict_settings {
        if !same(bound, &binding(root, bank, project)?)? {
            return Err(miss("catalog binding/digest"));
        }
    } else if bound.as_object().is_none()
        || text(bound, "repo_root")? != host::text(root.as_os_str(), "catalog repository")?
        || text(bound, "bank_root")? != host::text(bank.as_os_str(), "catalog bank")?
        || text(bound, "project_id")? != project
    {
        return Err(miss("catalog binding/digest"));
    }
    let length = get(&header, "payload_bytes")?
        .as_int()
        .and_then(|v| v.as_u64())
        .ok_or_else(|| miss("catalog binding/digest"))?;
    if length == 0
        || length > MAX_CACHE as u64
        || payload.len() as u64 != length
        || text(&header, "payload_sha256")? != sha(payload)
    {
        return Err(miss("catalog binding/digest"));
    }
    let unsigned = object(
        header
            .as_object()
            .ok_or_else(|| miss("catalog binding/digest"))?
            .iter()
            .filter(|(key, _)| key != "mac")
            .cloned(),
    );
    let expected = mac(secret, &compact(&unsigned)?, payload);
    if !constant_equal(text(&header, "mac")?.as_bytes(), expected.as_bytes()) {
        return Err(miss("catalog authentication"));
    }
    Ok((header, payload))
}
fn remove_private(
    directory: &Path,
    name: &OsStr,
    expected: Option<&[u8]>,
    maximum: usize,
    sync: bool,
) -> Result<()> {
    let chain = DirectoryChain::open(directory).map_err(io_error)?;
    let info = chain.directory().stat_at(name).map_err(io_error)?;
    private_record(info)?;
    if let Some(expected) = expected {
        if read_private(&directory.join(name), maximum)?.as_deref() != Some(expected) {
            return Err(miss("catalog pointer changed during frontier invalidation"));
        }
    }
    chain.recheck().map_err(io_error)?;
    if chain.directory().stat_at(name).map_err(io_error)? != info {
        return Err(miss("cache record changed"));
    }
    chain.directory().unlink_at(name).map_err(io_error)?;
    if sync {
        chain.directory().fsync_directory().map_err(io_error)?;
    }
    chain.recheck().map_err(io_error)
}
pub fn collect_owned(
    directory: &Path,
    root: &Path,
    bank: &Path,
    project: &str,
    secret: &[u8],
    keep: Option<&str>,
) -> Result<()> {
    let names = namespace(directory)?;
    let chain = DirectoryChain::open(directory).map_err(io_error)?;
    let mut budget = 0usize;
    for name in names {
        let Some(label) = name
            .to_str()
            .filter(|name| generation_name(name) && Some(*name) != keep)
        else {
            continue;
        };
        let info = match chain.directory().stat_at(&name) {
            Ok(info) => info,
            Err(_) => continue,
        };
        let remaining = MAX_COLLECTION.saturating_sub(budget);
        if remaining == 0 {
            break;
        }
        let allowance = MAX_RECORD.min(remaining);
        budget = budget.saturating_add(allowance + 1);
        let raw = match read_private(&directory.join(label), allowance) {
            Ok(Some(raw)) => raw,
            _ => continue,
        };
        budget = budget.saturating_sub(allowance + 1 - raw.len());
        if authenticate(&raw, root, bank, project, secret, false).is_err() {
            continue;
        }
        // Authentication already read these bytes within the aggregate budget.
        // Do not read them a second time just to delete the same observed inode.
        chain.recheck().map_err(io_error)?;
        if chain.directory().stat_at(&name).map_err(io_error)? != info {
            return Err(miss("cache record changed"));
        }
        chain.directory().unlink_at(&name).map_err(io_error)?;
        chain.recheck().map_err(io_error)?;
    }
    Ok(())
}
pub fn invalidate(repo: &Path, project: &str, expected_pointer: &[u8], runtime_root: &Path) {
    let _ = (|| -> Result<()> {
        let (root, bank, directory) = locations(repo, runtime_root)?;
        invalidate_at(&root, &bank, &directory, project, expected_pointer)
    })();
}
fn invalidate_at(
    root: &Path,
    bank: &Path,
    directory: &Path,
    project: &str,
    expected_pointer: &[u8],
) -> Result<()> {
    let _lock = publication_lock(directory)?;
    let (value, raw) = pointer(directory)?;
    if raw != expected_pointer {
        return Ok(());
    }
    let secret = key(directory, false)?;
    let Some(record) = read_private(&directory.join(text(&value, "file")?), MAX_RECORD)? else {
        return Ok(());
    };
    authenticate(&record, root, bank, project, &secret, true)?;
    remove_private(
        directory,
        OsStr::new("current.json"),
        Some(expected_pointer),
        MAX_POINTER,
        false,
    )?;
    collect_owned(directory, root, bank, project, &secret, None)
}
fn present(directory: &Path) -> Result<bool> {
    Ok(trust::inspect(directory, trust::InspectOptions::default())?.present)
}
fn record_project(raw: &[u8]) -> Result<String> {
    let (first, _) = split_record(raw)?;
    Ok(text(get(&decode(first, MAX_HEADER)?, "binding")?, "project_id")?.to_owned())
}
pub fn invalidate_after_change(
    repo: &Path,
    current_hashes: &Value,
    runtime_root: &Path,
) -> &'static str {
    let result = (|| -> Result<&'static str> {
        let (root, bank, directory) = locations(repo, runtime_root)?;
        invalidate_after_change_at(&root, &bank, &directory, current_hashes)
    })();
    result.unwrap_or("deferred-to-canonical-fallback")
}
/// Also useful for an independently located, already trusted bank.
pub fn invalidate_after_change_at(
    root: &Path,
    bank: &Path,
    directory: &Path,
    current_hashes: &Value,
) -> Result<&'static str> {
    if !present(directory)? {
        return Ok("not-present");
    }
    let _lock = publication_lock(directory)?;
    let (value, expected) = pointer(directory)?;
    let secret = key(directory, false)?;
    let Some(raw) = read_private(&directory.join(text(&value, "file")?), MAX_RECORD)? else {
        return Ok("not-present");
    };
    let project = record_project(&raw)?;
    let (_, payload) = authenticate(&raw, root, bank, &project, &secret, false)?;
    let data = decode(payload, MAX_CACHE)?;
    if same(get(&data, "hashes")?, current_hashes)? {
        return Ok("current");
    }
    remove_private(
        directory,
        OsStr::new("current.json"),
        Some(&expected),
        MAX_POINTER,
        false,
    )?;
    collect_owned(directory, root, bank, &project, &secret, None)?;
    Ok("invalidated")
}
pub fn invalidate_frontier_transition(repo: &Path, runtime_root: &Path) -> Result<&'static str> {
    let (root, bank, directory) = locations(repo, runtime_root)?;
    if !present(&directory)? {
        return Ok("not-present");
    }
    let _lock = publication_lock(&directory)?;
    let (value, expected) = pointer(&directory)?;
    let secret = key(&directory, false)?;
    let raw = read_private(&directory.join(text(&value, "file")?), MAX_RECORD)?
        .ok_or_else(|| miss("catalog generation missing"))?;
    let project = record_project(&raw)?;
    authenticate(&raw, &root, &bank, &project, &secret, true)?;
    remove_private(
        &directory,
        OsStr::new("current.json"),
        Some(&expected),
        MAX_POINTER,
        true,
    )?;
    collect_owned(&directory, &root, &bank, &project, &secret, None)?;
    Ok("invalidated-frontier-transition")
}
pub fn checked_markers(root: &Path, bank: &Path, owner: Option<&[u8]>) -> Result<()> {
    let frontier = writes::recovery_frontier_provider()?;
    let token = frontier.observe_owned(root, owner, None)?;
    let held = if get(&token, "hold_sha256")? != &Value::Null {
        Some(frontier.verified_hold(root, owner)?)
    } else {
        None
    };
    observation::markers(bank, owner, held.as_deref())
}

pub struct Catalog {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub engine: Generation,
    pub data: Value,
    pub header: Value,
    pub hashes: Value,
    pub checked: Value,
    pub entries: BTreeMap<String, Value>,
    pub payload_bytes_read: usize,
    pub payload_file_reads: usize,
    pub parsed_records: usize,
    pub pointer_raw: Vec<u8>,
    directory: PathBuf,
}
impl Catalog {
    pub fn selected_entries(&self, selected: &[String]) -> Result<Vec<Value>> {
        let mut paths: BTreeSet<String> = selected.iter().cloned().collect();
        for path in array(get(&self.data, "oversized")?)? {
            paths.insert(
                path.as_str()
                    .ok_or_else(|| miss("catalog payload shape"))?
                    .to_owned(),
            );
        }
        for path in selected {
            if let Some(entry) = self.entries.get(path) {
                if let (Some(id), Some(version)) = (entry.get("id"), entry.get("version")) {
                    let id = id.as_str().ok_or_else(|| miss("catalog payload shape"))?;
                    let version = version
                        .as_int()
                        .ok_or_else(|| miss("catalog payload shape"))?;
                    if let Some(references) =
                        get(&self.data, "references")?.get(&format!("{id}/{version}"))
                    {
                        for reference in array(references)? {
                            paths.insert(
                                reference
                                    .as_str()
                                    .ok_or_else(|| miss("catalog payload shape"))?
                                    .to_owned(),
                            );
                        }
                    }
                }
            }
        }
        Ok(paths
            .into_iter()
            .filter_map(|path| self.entries.get(&path).cloned())
            .collect())
    }
    pub fn source(&mut self, path: &str, maximum: usize) -> Result<Vec<u8>> {
        if path != "MEMORY.md" {
            contract::path_value(&s(path))?;
        }
        if maximum > observation::MAX_BYTES {
            return Err(miss("selected source bound"));
        }
        let target = self.bank.join(path);
        let chain = DirectoryChain::open(
            target
                .parent()
                .ok_or_else(|| miss("selected source path"))?,
        )
        .map_err(io_error)?;
        let name = target
            .file_name()
            .ok_or_else(|| miss("selected source path"))?;
        let mut file = chain
            .directory()
            .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            .map_err(io_error)?;
        let before = file.stat().map_err(io_error)?;
        if !before.is_regular() || before.nlink != 1 {
            return Err(miss("source no longer ordinary"));
        }
        let raw = file.read_prefix(maximum + 1).map_err(io_error)?;
        if chain.directory().stat_at(name).map_err(io_error)? != before {
            return Err(miss("selected source changed"));
        }
        chain.recheck().map_err(io_error)?;
        self.payload_bytes_read += raw.len();
        self.payload_file_reads += 1;
        if raw.len() <= maximum && text(&self.hashes, path)? != sha(&raw) {
            return Err(miss("selected source changed"));
        }
        Ok(raw)
    }
    pub fn parsed(&mut self, path: &str, text: &str) -> Result<(String, Value, String)> {
        let allowed: Vec<_> = get(&records::MODEL, "properties")?
            .as_object()
            .ok_or_else(|| miss("record schema unavailable"))?
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        let (metadata, body) = crate::codec::frontmatter::read(text, &allowed)?;
        let (kind, metadata) = records::validate(&Value::Object(metadata), body, &[])?;
        let parsed = (kind, metadata, body.to_owned());
        self.parsed_records += 1;
        let entry = self
            .entries
            .get(path)
            .ok_or_else(|| miss("selected format differs"))?;
        if parsed.0 != crate::recall_catalog::text(entry, "format")? {
            return Err(miss("selected format differs"));
        }
        Ok(parsed)
    }
    pub fn recheck(&self, owner: Option<&[u8]>) -> Result<Value> {
        writes::fault("catalog-final-source-recheck")?;
        let (root, bank) = writes::bank_path(&self.root)?;
        if root != self.root || bank != self.bank {
            return Err(miss("bank location changed"));
        }
        checked_markers(&root, &bank, owner)?;
        let scan = observation::scan(&bank, false, true)?;
        observation::physical_contract(&scan.physical)?;
        if !same(&scan.hashes, &self.hashes)?
            || !same(
                &physical_value(&normalized_physical(&scan.physical)),
                get(&self.data, "physical")?,
            )?
        {
            let _ = invalidate_at(
                &root,
                &bank,
                &self.directory,
                text(&self.checked, "project_id")?,
                &self.pointer_raw,
            );
            return Err(miss("actual canonical bytes changed"));
        }
        for omitted in array(get(&self.checked, "omitted")?)? {
            let path = omitted
                .as_str()
                .ok_or_else(|| miss("catalog payload shape"))?;
            if scan.physical.contains_key(path) {
                return Err(miss("omitted path occupied"));
            }
        }
        let (physical, stamps) = physical_inventory(&bank)?;
        if !same(&physical_value(&physical), get(&self.data, "physical")?)?
            || !same(&stamps, get(&self.data, "stamps")?)?
        {
            return Err(miss("late source change"));
        }
        checked_markers(&root, &bank, owner)?;
        coherent(&self.engine)?;
        Ok(object([
            ("payload_bytes", Value::Int(scan.payload_bytes.into())),
            ("file_reads", Value::Int(scan.file_reads.into())),
        ]))
    }
}
fn payload_shape(data: &Value, project: &str) -> Result<BTreeMap<String, Value>> {
    if !fields(
        data,
        &[
            "schema",
            "hashes",
            "checked",
            "physical",
            "stamps",
            "references",
            "oversized",
        ],
    ) || text(data, "schema")? != SCHEMA
        || text(get(data, "checked")?, "project_id")? != project
    {
        return Err(miss("catalog payload shape"));
    }
    let hashes = get(data, "hashes")?
        .as_object()
        .ok_or_else(|| miss("catalog payload shape"))?;
    if hashes.len() > observation::MAX_FILES {
        return Err(miss("catalog payload shape"));
    }
    let mut entries = BTreeMap::new();
    for entry in array(get(get(data, "checked")?, "entries")?)? {
        let path = text(entry, "path")?;
        if path != "MEMORY.md" {
            contract::path_value(&s(path))?;
        }
        if entries.insert(path.to_owned(), entry.clone()).is_some() {
            return Err(miss("catalog payload shape"));
        }
    }
    array(get(get(data, "checked")?, "omitted")?)?;
    array(get(data, "oversized")?)?;
    get(data, "references")?
        .as_object()
        .ok_or_else(|| miss("catalog payload shape"))?;
    Ok(entries)
}
pub fn load_catalog(
    repo: &Path,
    project: &str,
    runtime: &Runtime,
    owner: Option<&[u8]>,
) -> Result<Catalog> {
    load_catalog_with_environment(repo, project, runtime, owner, &StateEnvironment::current())
}
pub fn load_catalog_with_environment(
    repo: &Path,
    project: &str,
    runtime: &Runtime,
    owner: Option<&[u8]>,
    environment: &StateEnvironment,
) -> Result<Catalog> {
    if disabled() {
        return Err(miss("disabled"));
    }
    let (root, bank, directory) = locations_with_environment(repo, &runtime.pack, environment)?;
    checked_markers(&root, &bank, owner)?;
    let engine = generation(runtime)?;
    let (value, pointer_raw) = pointer(&directory)?;
    let secret = key(&directory, false)?;
    let raw = read_private(&directory.join(text(&value, "file")?), MAX_RECORD)?
        .ok_or_else(|| miss("catalog generation missing"))?;
    let (header, payload) = authenticate(&raw, &root, &bank, project, &secret, true)?;
    if !same(get(&header, "engine")?, engine.value())? {
        let _ = invalidate_at(&root, &bank, &directory, project, &pointer_raw);
        return Err(miss("catalog engine changed"));
    }
    let data = decode(payload, MAX_CACHE)?;
    let entries = payload_shape(&data, project)?;
    let (physical, stamps) = physical_inventory(&bank)?;
    if !same(&physical_value(&physical), get(&data, "physical")?)?
        || !same(&stamps, get(&data, "stamps")?)?
    {
        let _ = invalidate_at(&root, &bank, &directory, project, &pointer_raw);
        return Err(miss("catalog source hint changed"));
    }
    checked_markers(&root, &bank, owner)?;
    Ok(Catalog {
        root,
        bank,
        engine,
        hashes: get(&data, "hashes")?.clone(),
        checked: get(&data, "checked")?.clone(),
        data,
        header,
        entries,
        payload_bytes_read: 0,
        payload_file_reads: 0,
        parsed_records: 0,
        pointer_raw,
        directory,
    })
}
fn payload(observed: &Observation, physical: &Physical, stamps: Value) -> Result<Value> {
    let mut references: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut oversized = Vec::new();
    for (path, (kind, metadata, _)) in &observed.parsed {
        if kind != "standalone-record" {
            continue;
        }
        if observed
            .sources
            .get(path)
            .ok_or_else(|| miss("catalog source missing"))?
            .len()
            > MAX_SOURCE
            && text(metadata, "status")? == "active"
        {
            oversized.push(s(path));
        }
        for reference in array(get(metadata, "corrects")?)?
            .iter()
            .chain(array(get(metadata, "supersedes")?)?)
        {
            let id = text(reference, "id")?;
            let version = get(reference, "version")?
                .as_int()
                .ok_or_else(|| miss("catalog reference version"))?;
            references
                .entry(format!("{id}/{version}"))
                .or_default()
                .insert(path.clone());
        }
    }
    let omitted = get(&observed.checked, "omitted")?
        .as_object()
        .ok_or_else(|| miss("catalog omission inventory"))?;
    let checked = object([
        ("project_id", get(&observed.checked, "project_id")?.clone()),
        ("entries", get(&observed.checked, "entries")?.clone()),
        (
            "omitted",
            Value::Array(omitted.iter().map(|(path, _)| s(path)).collect()),
        ),
    ]);
    Ok(object([
        ("schema", s(SCHEMA)),
        ("hashes", observed.hashes.clone()),
        ("checked", checked),
        ("physical", physical_value(physical)),
        ("stamps", stamps),
        (
            "references",
            object(
                references
                    .into_iter()
                    .map(|(id, paths)| (id, Value::Array(paths.into_iter().map(s).collect()))),
            ),
        ),
        ("oversized", Value::Array(oversized)),
    ]))
}
fn seal(bound: Value, engine: &Value, payload: &[u8], secret: &[u8]) -> Result<Vec<u8>> {
    if payload.is_empty() || payload.len() > MAX_CACHE {
        return Err(miss("catalog encoded bound"));
    }
    let mut header = object([
        ("schema", s(SCHEMA)),
        ("binding", bound),
        ("engine", engine.clone()),
        ("payload_bytes", Value::Int(payload.len().into())),
        ("payload_sha256", s(sha(payload))),
    ]);
    let signature = mac(secret, &compact(&header)?, payload);
    if let Value::Object(entries) = &mut header {
        entries.push(("mac".into(), s(signature)));
    }
    let mut raw = compact(&header)?;
    if raw.len() > MAX_HEADER {
        return Err(miss("catalog header bound"));
    }
    raw.push(b'\n');
    raw.extend_from_slice(payload);
    Ok(raw)
}
pub fn publish(observed: &mut Observation, engine: &Generation) -> Result<bool> {
    publish_with_environment(observed, engine, &StateEnvironment::current())
}
pub fn publish_with_environment(
    observed: &mut Observation,
    engine: &Generation,
    environment: &StateEnvironment,
) -> Result<bool> {
    if disabled() {
        return Ok(false);
    }
    coherent(engine)?;
    let (root, bank, directory) =
        locations_with_environment(&observed.root, &engine.runtime.pack, environment)?;
    for path in [
        directory
            .parent()
            .ok_or_else(|| miss("cache location unavailable"))?,
        directory.as_path(),
    ] {
        trust::inspect(path, trust::InspectOptions::default())?;
        storage::private_mkdir(path)?;
    }
    trust::state(&directory)?;
    let _lock = publication_lock(&directory)?;
    if namespace(&directory)?.len() > MAX_NAMESPACE - 4 {
        return Err(miss("cache namespace capacity"));
    }
    observed.recheck("catalog-seal-source-recheck")?;
    let (physical, stamps) = physical_inventory(&bank)?;
    let stamp_paths: BTreeSet<_> = stamps
        .as_object()
        .ok_or_else(|| miss("catalog stamps"))?
        .iter()
        .map(|(path, _)| path)
        .collect();
    let hash_paths: BTreeSet<_> = observed
        .hashes
        .as_object()
        .ok_or_else(|| miss("catalog hashes"))?
        .iter()
        .map(|(path, _)| path)
        .collect();
    if physical != normalized_physical(&observed.physical) || stamp_paths != hash_paths {
        return Err(miss("source topology changed before cache publication"));
    }
    let payload = compact(&payload(observed, &physical, stamps)?)?;
    if payload.len() > MAX_CACHE {
        return Err(miss("catalog encoded bound"));
    }
    let secret = key(&directory, true)?;
    let old = read_private(&directory.join("current.json"), MAX_POINTER)?;
    let project = text(&observed.checked, "project_id")?;
    let record = seal(
        binding(&root, &bank, project)?,
        engine.value(),
        &payload,
        &secret,
    )?;
    coherent(engine)?;
    observation::markers(
        &bank,
        observed.owner.as_deref(),
        observed.transaction.as_deref(),
    )?;
    let name = format!("g-{}.catalog", host::uuid4()?.replace('-', ""));
    storage::replace_with_limit(&directory.join(&name), &record, None, MAX_RECORD)?;
    storage::replace_with_limit(
        &directory.join("current.json"),
        &compact(&object([("schema", s(SCHEMA)), ("file", s(&name))]))?,
        old.as_deref().map(sha).as_deref(),
        MAX_POINTER,
    )?;
    let _ = collect_owned(&directory, &root, &bank, project, &secret, Some(&name));
    Ok(true)
}
/// All domain refusals are recoverable only by rerunning the full canonical reader.
pub fn optional_failure(_: &ContractError) -> bool {
    true
}
