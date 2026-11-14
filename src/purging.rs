//! Explicit, inventory-bound local bank deletion with resumable quarantine.
//!
//! Baseline `purging.py` (`am purge-data`). A purge deletes exactly one
//! repository's resolved Memory Bank, only after an exact preview digest is
//! confirmed. The bank is detached into a sibling quarantine under its own
//! journal-bound lock, then removed entry by entry with recorded progress, so
//! an interrupted purge resumes only the same confirmed inventory.

use crate::codec::json::{self, DecodePolicy, DuplicateKeys, Value};
use crate::error::ContractError;
use crate::fs::{self, Descriptor, DirectoryChain, FlockGuard, Identity, LockError, LockMode};
use crate::installation::{
    self as install, Command, Environment, Error, InstallRequest, Result, Services, require,
};
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::updating::{merged, printed};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/purge-receipt-v1";
/// Baseline `MAX_RECORD = 32 * 2**20`.
pub const MAX_RECORD: usize = 32 * 1024 * 1024;
/// Ordinary purge inventory file bound (64 MiB).
pub const MAX_FILE: u64 = 64 * 1024 * 1024;
/// Purge inventory bounds: entries and bytes.
pub const MAX_ENTRIES: usize = 40000;
pub const MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Bank lock records are bounded to 65,536 bytes.
pub const MAX_LOCK: usize = 65536;
/// Router bound of the selected bank.
pub const MAX_ROUTER: u64 = 16384;
pub const LOCK: &str = ".agent-memory.lock";
pub const OPERATION: &str = "bank-purge";
pub const SCOPE: &str = "Only the confirmed local bank; Git history, remote copies, runtime and external backups are retained";
pub const REPAIR: &str = "Preserve the bank, quarantine and external purge journal. Inspect the exact reported path; retry the same confirmed operation only after its writer has ended, or obtain a fresh preview for changed data.";
const STAGE_PREFIX: &str = ".agent-memory-purge-";

fn contract(message: impl Into<String>) -> Error {
    Error::Contract(ContractError::new(message.into()))
}

fn io(error: std::io::Error) -> Error {
    contract(error.to_string())
}

fn set(value: &mut Value, key: &str, field: Value) {
    *value = merged(
        std::mem::replace(value, Value::Null),
        object([(key, field)]),
    );
}

fn split(path: &Path) -> Result<(&Path, &OsStr)> {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => Ok((parent, name)),
        _ => Err(contract("Purge path requires a parent and a name")),
    }
}

/// Remove one node through its pinned parent and sync that parent.
fn remove_node(path: &Path, directory: bool) -> Result<()> {
    let (parent, name) = split(path)?;
    let chain = DirectoryChain::open(parent).map_err(io)?;
    if directory {
        chain.directory().remove_directory_at(name).map_err(io)?;
    } else {
        chain.directory().unlink_at(name).map_err(io)?;
    }
    install::sync_dir(parent)
}

/// Baseline `canonical`: explicit absolute paths without traversal or links.
pub fn canonical(env: &Environment, value: &Path) -> Result<PathBuf> {
    let path = env.expanduser(value)?;
    require(
        path.is_absolute()
            && !path
                .components()
                .any(|part| matches!(part, Component::ParentDir)),
        "Purge paths must be explicit absolute paths without traversal",
    )?;
    // The filesystem root itself is never a symlink.
    for node in path.ancestors().filter(|node| node.parent().is_some()) {
        require(
            !install::named(node)?.is_some_and(Identity::is_symlink),
            "Purge paths must not cross symlinks",
        )?;
    }
    Ok(fs::canonical_path(&path, false)?)
}

/// The baseline `purge-data` argparse namespace.
#[derive(Debug, Clone, Default)]
pub struct PurgeRequest {
    pub repo: PathBuf,
    pub path: PathBuf,
    pub check: bool,
    pub confirm: Option<String>,
}

struct Selection {
    repo: PathBuf,
    bank: PathBuf,
    locations: install::Locations,
}

/// Baseline `selection`.
fn selection(env: &Environment, services: &Services, request: &PurgeRequest) -> Result<Selection> {
    let repo = canonical(env, &request.repo)?;
    let supplied = canonical(env, &request.path)?;
    let home = fs::canonical_path(&env.home()?, false)?;
    require(
        repo != Path::new("/") && repo != home && install::is_directory(&repo)?,
        "A bounded project repository root is required",
    )?;
    let bank = crate::repository_config::bank_path(&repo)?;
    require(
        supplied == bank && bank.starts_with(&repo) && bank != repo,
        "Purge path must be exactly this repository's resolved Memory Bank, not a broad directory",
    )?;
    let locations = install::locations(env)?;
    for retained in [&locations.state, &locations.code, &locations.runtime, &home] {
        require(
            &bank != retained && !retained.starts_with(&bank),
            "Purge path would include retained runtime or home data",
        )?;
    }
    let old = install::read_receipt(env, &locations.state)?;
    install::preflight_layout(
        env,
        services,
        Command::Recover,
        &InstallRequest::default(),
        &locations,
        old.as_ref(),
    )?;
    Ok(Selection {
        repo,
        bank,
        locations,
    })
}

/// Baseline `identity`: bounded ordinary files and directories only.
fn identity(path: &Path) -> Result<Value> {
    let info = install::named(path)?
        .ok_or_else(|| contract(format!("[Errno 2] No such file or directory: {path:?}")))?;
    require(
        !info.is_symlink(),
        format!("Purge inventory contains a symlink: {}", path.display()),
    )?;
    let base = [
        ("device", Value::Int(info.dev.into())),
        ("inode", Value::Int(info.ino.into())),
        ("mode", Value::Int((info.mode & 0o7777).into())),
    ];
    if info.is_directory() {
        let mut fields: Vec<(String, Value)> =
            base.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        fields.push(("kind".into(), string("directory")));
        return Ok(Value::Object(fields));
    }
    let size = u64::try_from(info.size).unwrap_or(u64::MAX);
    require(
        info.is_regular() && size <= MAX_FILE,
        "Purge inventory requires bounded ordinary files and directories",
    )?;
    let raw = install::read_optional(path, MAX_FILE as usize)?.unwrap_or_default();
    let mut fields: Vec<(String, Value)> =
        base.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    fields.extend([
        ("kind".to_owned(), string("file")),
        ("bytes".to_owned(), Value::Int(size.into())),
        (
            "change_time_ns".to_owned(),
            Value::Int(info.ctime_ns.into()),
        ),
        (
            "modification_time_ns".to_owned(),
            Value::Int(info.mtime_ns.into()),
        ),
        ("sha256".to_owned(), string(install::sha(&raw))),
    ]);
    Ok(Value::Object(fields))
}

fn read_bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    Ok(install::read_optional(path, limit)?.unwrap_or_default())
}

fn names(directory: &Path, limit: usize) -> Result<Vec<String>> {
    let entries = Descriptor::open_directory(directory)
        .map_err(io)?
        .entries_bounded(limit)
        .map_err(io)?
        .ok_or_else(|| contract("Purge inventory exceeds its explicit bound"))?;
    let mut names: Vec<String> = entries
        .into_iter()
        .map(|name| {
            name.into_string()
                .map_err(|_| contract("Purge inventory names must be UTF-8"))
        })
        .collect::<Result<_>>()?;
    names.sort();
    Ok(names)
}

/// Baseline `inventory`: a top-down walk of the bank, refusing links,
/// nested Git metadata, live locks and pending transactions.
fn inventory(bank: &Path, owner: Option<&[u8]>) -> Result<(Value, Vec<(String, Value)>)> {
    let root = identity(bank)?;
    require(
        text(&root, "kind") == Some("directory"),
        "An existing ordinary Memory Bank directory is required",
    )?;
    let lock = bank.join(LOCK);
    if install::lexists(&lock)? {
        let held = owner.is_some_and(|owner| {
            install::is_regular(&lock).unwrap_or(false)
                && read_bytes(&lock, MAX_LOCK).is_ok_and(|raw| raw == owner)
        });
        require(
            held,
            "Memory Bank is locked; preserve it and finish or recover its writer first",
        )?;
    }
    require(
        !install::lexists(&bank.join(crate::bank_transactions::MARKER))?,
        "Pending bank transaction requires its own recovery before purge",
    )?;
    let mut entries = Vec::new();
    let mut total: u64 = 0;
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let directory = bank.join(&relative);
        let mut subdirectories = Vec::new();
        for name in names(&directory, MAX_ENTRIES + 1)? {
            let relative_name = relative.join(&name);
            let key = install::path_text(&relative_name)?.to_owned();
            if key == LOCK && owner.is_some() {
                continue;
            }
            require(
                !relative_name
                    .components()
                    .any(|part| part.as_os_str() == ".git"),
                "Purge must not delete nested Git metadata",
            )?;
            let item = identity(&directory.join(&name))?;
            total = total.saturating_add(
                get(&item, "bytes")
                    .as_int()
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            );
            if text(&item, "kind") == Some("directory") {
                subdirectories.push(relative_name);
            }
            entries.push((key, item));
            require(
                entries.len() <= MAX_ENTRIES && total <= MAX_BYTES,
                "Purge inventory exceeds its explicit bound",
            )?;
        }
        // Top-down order: visit this directory's children before deeper ones.
        subdirectories.reverse();
        pending.extend(subdirectories);
    }
    Ok((root, entries))
}

fn count(entries: &[(String, Value)], select: impl Fn(&str, &Value) -> bool) -> usize {
    entries
        .iter()
        .filter(|(name, item)| select(name, item))
        .count()
}

/// Baseline `preview`: the exact confirmable inventory of the bank.
fn preview(selection: &Selection, owner: Option<&[u8]>) -> Result<Value> {
    let repo = &selection.repo;
    let bank = &selection.bank;
    let token =
        crate::bank_writes::recovery_frontier_provider()?.observe_owned(repo, owner, None)?;
    let indexed = crate::layout_recovery_state::index(repo, &selection.locations.runtime_pack())?;
    let active = get(&indexed, "operations")
        .as_object()
        .unwrap_or(&[])
        .iter()
        .any(|(_, row)| matches!(text(row, "state"), Some("active" | "interrupted")));
    require(
        get(&token, "coverage_complete") == &Value::Bool(true)
            && get(&token, "hold_sha256") == &Value::Null
            && !active,
        "Recovery evidence/dependencies remain pending; bank purge cannot discard them",
    )?;
    let (root, entries) = inventory(bank, owner)?;
    let router = bank.join("MEMORY.md");
    let bounded = install::named(&router)?.is_some_and(|info| {
        info.is_regular() && u64::try_from(info.size).unwrap_or(u64::MAX) <= MAX_ROUTER
    });
    require(
        bounded,
        "Purge requires the selected bank's bounded ordinary router",
    )?;
    let raw = read_bytes(&router, MAX_ROUTER as usize)?;
    let router_text =
        std::str::from_utf8(&raw).map_err(|_| contract("Purge router is not UTF-8"))?;
    let (metadata, _, _) = crate::bank_contract::parse_router(router_text)?;
    let files = |name: &str, item: &Value| -> bool {
        let _ = name;
        text(item, "kind") == Some("file")
    };
    let bytes: u64 = entries
        .iter()
        .map(|(_, item)| {
            get(item, "bytes")
                .as_int()
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        })
        .sum();
    let result = object([
        ("schema", string(SCHEMA)),
        ("repository", string(install::path_text(repo)?)),
        ("path", string(install::path_text(bank)?)),
        ("project_id", get(&metadata, "project-id").clone()),
        ("root_identity", root),
        ("entries", Value::Object(entries.clone())),
        (
            "counts",
            object([
                ("files", Value::Int(count(&entries, files).into())),
                (
                    "directories",
                    Value::Int(
                        (1 + count(&entries, |_, item| text(item, "kind") == Some("directory")))
                            .into(),
                    ),
                ),
                ("bytes", Value::Int(bytes.into())),
                (
                    "durable_files",
                    Value::Int(
                        count(&entries, |name, item| {
                            files(name, item) && !crate::bank_contract::runtime_path(name)
                        })
                        .into(),
                    ),
                ),
                (
                    "generated_runtime_files",
                    Value::Int(
                        count(&entries, |name, item| {
                            files(name, item) && crate::bank_contract::runtime_path(name)
                        })
                        .into(),
                    ),
                ),
            ]),
        ),
    ]);
    let encoded = install::encoded(&result)?;
    require(
        encoded.len() <= MAX_RECORD / 2,
        "Purge inventory exceeds its journal bound",
    )?;
    Ok(merged(
        result,
        object([("confirmation", string(install::sha(&encoded)))]),
    ))
}

/// Baseline `read_record`.
fn read_record(path: &Path) -> Result<Value> {
    let ordinary = install::named(path)?.is_some_and(|info| {
        info.is_regular() && u64::try_from(info.size).unwrap_or(u64::MAX) <= MAX_RECORD as u64
    });
    require(ordinary, "Purge record is missing, linked or oversized")?;
    let raw = read_bytes(path, MAX_RECORD)?;
    Ok(json::decode_bytes(
        &raw,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            max_bytes: MAX_RECORD,
            ..DecodePolicy::default()
        },
    )?)
}

/// Baseline `save`.
fn save(path: &Path, record: &Value) -> Result<()> {
    require(
        install::encoded(record)?.len() <= MAX_RECORD,
        "Purge journal exceeds its bound",
    )?;
    install::replace_record(path, record)
}

/// A newly published purge lock (baseline `local_registry.exclusive`):
/// released by path only while it still holds the same inode and owner bytes.
struct NewLock {
    _guard: FlockGuard,
    path: PathBuf,
    inode: Identity,
    payload: Vec<u8>,
}

impl NewLock {
    fn acquire(lock: &Path) -> Result<Self> {
        let (parent, name) = split(lock)?;
        let owner = object([
            ("operation_id", string(crate::host::uuid4()?)),
            ("operation", string(OPERATION)),
            ("host", string(crate::host::hostname()?)),
            ("pid", Value::Int(crate::host::pid().into())),
            ("start", string(crate::host::utc_now_isoformat()?)),
        ]);
        let payload = crate::developer_identity::encoded(&owner)?;
        let chain = DirectoryChain::open(parent).map_err(io)?;
        let file = chain
            .directory()
            .create_exclusive(name, libc::O_WRONLY, 0o600)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    contract("Memory metadata is locked; inspect status/recover")
                } else {
                    io(error)
                }
            })?;
        let created = file.stat().map_err(io)?;
        let mut guard = match FlockGuard::try_acquire_owned(file, LockMode::Exclusive) {
            Ok(guard) => guard,
            Err((file, error)) => {
                fs::discard_created_empty_slot(lock, &file, created).map_err(|_| {
                    contract(
                        "Metadata ownership acquisition failed; empty-slot cleanup became unsafe",
                    )
                })?;
                return Err(contract(match error {
                    LockError::Busy => {
                        "Metadata ownership acquisition was contended before publication; retry without adopting the slot"
                    }
                    _ => {
                        "Metadata ownership acquisition failed before publication; retry without adopting the slot"
                    }
                }));
            }
        };
        let written = guard.write_once(&payload).map_err(|_| {
            contract("Metadata owner publication failed; preserve the marker for inspection")
        })?;
        require(
            written == payload.len(),
            "Incomplete metadata owner publication; preserve the marker for inspection",
        )?;
        guard.descriptor().fsync().map_err(io)?;
        Ok(Self {
            _guard: guard,
            path: lock.to_path_buf(),
            inode: created,
            payload,
        })
    }

    /// Unlink only the same published owner, as the baseline context exit.
    fn release(self) -> Result<()> {
        let same = install::named(&self.path)?.is_some_and(|info| {
            info.is_regular()
                && info.same_inode(self.inode)
                && read_bytes(&self.path, MAX_LOCK).is_ok_and(|raw| raw == self.payload)
        });
        if same {
            let (parent, name) = split(&self.path)?;
            DirectoryChain::open(parent)
                .map_err(io)?
                .directory()
                .unlink_at(name)
                .map_err(io)?;
        }
        Ok(())
    }
}

fn existing_owner(lock: &Path, expected_hash: Option<&str>) -> Result<Vec<u8>> {
    let bounded = install::named(lock)?.is_some_and(|info| {
        info.is_regular() && u64::try_from(info.size).unwrap_or(u64::MAX) <= MAX_LOCK as u64
    });
    require(bounded, "Purge lock must be an ordinary bounded file")?;
    let owner = read_bytes(lock, MAX_LOCK)?;
    let metadata = json::decode_bytes(
        &owner,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            max_bytes: MAX_LOCK,
            ..DecodePolicy::default()
        },
    )?;
    let keys: BTreeSet<&str> = metadata
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(key, _)| key.as_str())
        .collect();
    let pid = get(&metadata, "pid")
        .as_int()
        .and_then(|value| value.as_i64())
        .filter(|pid| *pid > 0 && *pid < (1 << 31));
    require(
        metadata.as_object().is_some_and(|fields| fields.len() == 5)
            && keys == BTreeSet::from(["operation_id", "operation", "host", "pid", "start"])
            && crate::developer_identity::uuid_valid(get(&metadata, "operation_id"))
            && pid.is_some(),
        "Existing purge lock has unknown ownership metadata; preserve it",
    )?;
    crate::lifecycle::time_value(get(&metadata, "start"))
        .map_err(|_| contract("Existing purge lock timestamp is invalid; preserve it"))?;
    require(
        text(&metadata, "operation") == Some(OPERATION)
            && text(&metadata, "host") == Some(crate::host::hostname()?.as_str()),
        "Existing lock belongs to another operation or host; preserve it",
    )?;
    if let Some(expected) = expected_hash {
        require(
            install::sha(&owner) == expected,
            "Purge lock differs from its journal",
        )?;
    }
    let pid = i32::try_from(pid.unwrap_or_default())
        .map_err(|_| contract("Existing purge lock has unknown ownership metadata; preserve it"))?;
    match crate::host::pid_state(pid)? {
        crate::host::PidState::Gone => Ok(owner),
        crate::host::PidState::Alive => Err(contract(
            "Recorded purge process is still live; do not steal its lock",
        )),
        crate::host::PidState::Denied => Err(contract(format!(
            "[Errno 1] Operation not permitted: pid {pid}"
        ))),
    }
}

/// Baseline `bank_lock`: a fresh purge lock, or recovery of the recorded one.
fn bank_lock<T>(
    bank: &Path,
    expected_hash: Option<&str>,
    body: impl FnOnce(&[u8]) -> Result<T>,
) -> Result<T> {
    require(
        install::named(bank)?.is_some_and(Identity::is_directory),
        "An existing ordinary purge bank/quarantine is required",
    )?;
    let lock = bank.join(LOCK);
    if !install::lexists(&lock)? {
        require(expected_hash.is_none(), "Purge quarantine lock is missing")?;
        let owned = crate::runtime_faults::mutate("purge-bank-lock", || NewLock::acquire(&lock))?;
        let payload = owned.payload.clone();
        let result = body(&payload);
        let released = if install::lexists(&lock)? {
            crate::runtime_faults::mutate("purge-bank-unlock", || owned.release())
        } else {
            owned.release()
        };
        released?;
        return result;
    }
    let owner = existing_owner(&lock, expected_hash)?;
    let (parent, name) = split(&lock)?;
    let chain = DirectoryChain::open(parent).map_err(io)?;
    let descriptor = chain
        .directory()
        .open_at(name, libc::O_RDWR | libc::O_NOFOLLOW, 0)
        .map_err(io)?;
    let guard = match FlockGuard::try_acquire(descriptor, LockMode::Exclusive) {
        Ok(guard) => guard,
        Err(LockError::Busy) => {
            return Err(contract("Another process still holds the purge lock"));
        }
        Err(error) => return Err(contract(error.to_string())),
    };
    let unchanged = |guard: &FlockGuard| -> Result<bool> {
        let held = guard.descriptor().stat().map_err(io)?;
        Ok(
            install::named(&lock)?.is_some_and(|info| info.is_regular() && info.same_inode(held))
                && held.nlink == 1
                && read_bytes(&lock, MAX_LOCK)? == owner,
        )
    };
    require(unchanged(&guard)?, "Purge lock changed during recovery")?;
    let result = body(&owner);
    // A detached quarantine retains its journal-bound lock on refusal. Only a
    // pre-detachment bank lock can be released after a refused request
    // without consuming pending deletion/recovery evidence.
    let released = if expected_hash.is_none() && install::lexists(&lock)? {
        require(
            unchanged(&guard)?,
            "Purge lock changed before release; preserve it",
        )
        .and_then(|()| {
            crate::runtime_faults::mutate("purge-bank-unlock", || remove_node(&lock, false))
        })
    } else {
        Ok(())
    };
    drop(guard);
    released?;
    result
}

fn inode_key(item: &Value) -> String {
    format!(
        "{}:{}",
        get(item, "device")
            .as_int()
            .map(|v| v.as_str().to_owned())
            .unwrap_or_default(),
        get(item, "inode")
            .as_int()
            .map(|v| v.as_str().to_owned())
            .unwrap_or_default()
    )
}

fn approved_entries(record: &Value) -> &[(String, Value)] {
    get(get(record, "preview"), "entries")
        .as_object()
        .unwrap_or(&[])
}

fn approved(record: &Value, name: &str) -> Option<Value> {
    approved_entries(record)
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, item)| item.clone())
}

/// Baseline `effective_entry`: an approved file with its recorded
/// self-mutation ctime override.
fn effective_entry(record: &Value, name: &str) -> Result<Value> {
    let entry = approved(record, name)
        .ok_or_else(|| contract("Purge quarantine contains new or changed data; preserve it"))?;
    let overrides = get(record, "ctime_overrides");
    match overrides.get(&inode_key(&entry)) {
        Some(value) if text(&entry, "kind") == Some("file") && *value != Value::Null => {
            let mut entry = entry;
            set(&mut entry, "change_time_ns", value.clone());
            Ok(entry)
        }
        _ => Ok(entry),
    }
}

fn without_ctime(value: &Value) -> Value {
    Value::Object(
        value
            .as_object()
            .unwrap_or(&[])
            .iter()
            .filter(|(key, _)| key != "change_time_ns")
            .cloned()
            .collect(),
    )
}

/// Baseline `unchanged_data`.
fn unchanged_data(actual: &Value, expected: &Value) -> bool {
    install::same(&without_ctime(actual), &without_ctime(expected))
}

fn set_override(record: &mut Value, key: &str, value: Value) {
    let mut overrides = get(record, "ctime_overrides").clone();
    if overrides.as_object().is_none() {
        overrides = Value::Object(Vec::new());
    }
    set(&mut overrides, key, value);
    set(record, "ctime_overrides", overrides);
}

fn outcome(record: Value, bank: &Path, changed: bool) -> Result<Value> {
    Ok(object([
        ("operation", string("purge-data")),
        ("verified", Value::Bool(true)),
        ("changed", Value::Bool(changed)),
        ("receipt", record),
        (
            "replacement_bank_present",
            Value::Bool(install::lexists(bank)?),
        ),
        ("scope", string(SCOPE)),
    ]))
}

fn groups_of(entries: &[(String, Value)]) -> BTreeMap<String, BTreeSet<String>> {
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, item) in entries {
        if text(item, "kind") == Some("file") {
            groups
                .entry(inode_key(item))
                .or_default()
                .insert(name.clone());
        }
    }
    groups
}

fn depth(name: &str) -> usize {
    Path::new(name).components().count()
}

/// Baseline `finish`: remove the confirmed quarantine entry by entry.
fn finish(
    mut record: Value,
    record_path: &Path,
    bank: &Path,
    stage: &Path,
    owner: Option<&[u8]>,
) -> Result<Value> {
    let (root, remaining) = inventory(stage, owner)?;
    require(
        install::same(&root, get(get(&record, "preview"), "root_identity")),
        "Purge quarantine root identity changed",
    )?;
    let original_groups = groups_of(approved_entries(&record));
    let mut groups = groups_of(&remaining);
    let overrides_valid = get(&record, "ctime_overrides")
        .as_object()
        .unwrap_or(&[])
        .iter()
        .all(|(key, value)| {
            original_groups
                .get(key)
                .is_some_and(|names| names.len() > 1)
                && value
                    .as_int()
                    .and_then(|v| v.as_i128())
                    .is_some_and(|v| v > 0)
        });
    require(
        get(&record, "ctime_overrides") == &Value::Null || overrides_valid,
        "Purge self-mutation bookkeeping is invalid",
    )?;
    let lookup = |name: &str| {
        remaining
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v)
    };
    if let Some(pending) = text(&record, "pending_unlink").map(str::to_owned) {
        let entry = approved(&record, &pending)
            .filter(|entry| text(entry, "kind") == Some("file"))
            .ok_or_else(|| contract("Purge unlink intent is invalid"))?;
        let key = inode_key(&entry);
        require(
            original_groups
                .get(&key)
                .is_some_and(|names| names.len() > 1),
            "Purge unlink intent has no approved hardlink aliases",
        )?;
        if lookup(&pending).is_none() {
            let aliases = groups.get(&key).cloned().unwrap_or_default();
            for alias in &aliases {
                let actual = lookup(alias).cloned().unwrap_or(Value::Null);
                require(
                    unchanged_data(&actual, &effective_entry(&record, alias)?),
                    "Purge alias data changed after an interrupted unlink",
                )?;
            }
            if let Some(first) = aliases.iter().next() {
                let changed = get(lookup(first).unwrap_or(&Value::Null), "change_time_ns").clone();
                set_override(&mut record, &key, changed);
            }
        }
        set(&mut record, "pending_unlink", Value::Null);
        save(record_path, &record)?;
    }
    for (name, item) in &remaining {
        require(
            approved(&record, name).is_some()
                && install::same(item, &effective_entry(&record, name)?),
            "Purge quarantine contains new or changed data; preserve it",
        )?;
    }
    set(&mut record, "status", string("detached"));
    save(record_path, &record)?;
    let mut ordered: Vec<&(String, Value)> = remaining.iter().collect();
    ordered.sort_by(|(left, left_item), (right, right_item)| {
        let key = |name: &String, item: &Value| {
            (
                text(item, "kind") == Some("directory"),
                std::cmp::Reverse(depth(name)),
                name.clone(),
            )
        };
        key(left, left_item).cmp(&key(right, right_item))
    });
    for (index, (name, item)) in ordered.into_iter().enumerate() {
        let path = stage.join(name);
        install::unchanged_parent(&path)?;
        require(
            install::same(&identity(&path)?, &effective_entry(&record, name)?),
            format!("Purge entry changed immediately before removal: {name}"),
        )?;
        let key = inode_key(item);
        let file = text(item, "kind") == Some("file");
        let account_aliases = file && groups.get(&key).is_some_and(|names| names.len() > 1);
        if account_aliases {
            set(&mut record, "pending_unlink", string(name));
            save(record_path, &record)?;
        }
        crate::runtime_faults::mutate("purge-entry-remove", || remove_node(&path, !file))?;
        if file {
            if let Some(names) = groups.get_mut(&key) {
                names.remove(name);
            }
        }
        if account_aliases {
            crate::bank_writes::fault("purge-after-alias-unlink")?;
            let other = groups
                .get(&key)
                .and_then(|names| names.iter().next().cloned())
                .ok_or_else(|| contract("Purge alias data changed during removal"))?;
            let actual = identity(&stage.join(&other))?;
            require(
                unchanged_data(&actual, &effective_entry(&record, &other)?),
                "Purge alias data changed during removal",
            )?;
            set_override(&mut record, &key, get(&actual, "change_time_ns").clone());
            set(&mut record, "pending_unlink", Value::Null);
            save(record_path, &record)?;
        }
        crate::bank_writes::fault(&format!("purge-after-entry-{index}"))?;
    }
    let lock = stage.join(LOCK);
    if install::lexists(&lock)? {
        let held = owner.is_some_and(|owner| {
            install::is_regular(&lock).unwrap_or(false)
                && read_bytes(&lock, MAX_LOCK).is_ok_and(|raw| raw == owner)
        });
        require(held, "Purge lock changed before final removal")?;
        crate::runtime_faults::mutate("purge-lock-remove", || remove_node(&lock, false))?;
    }
    crate::runtime_faults::mutate("purge-root-remove", || remove_node(stage, true))?;
    crate::bank_writes::fault("purge-after-root-remove")?;
    set(&mut record, "status", string("complete"));
    set(
        &mut record,
        "completed_at",
        string(crate::host::utc_now_isoformat()?),
    );
    set(
        &mut record,
        "deletion_readback_verified",
        Value::Bool(!install::lexists(stage)?),
    );
    save(record_path, &record)?;
    crate::bank_writes::fault("purge-after-receipt")?;
    outcome(record, bank, true)
}

fn is_stage_name(name: &str) -> bool {
    name.strip_prefix(STAGE_PREFIX).is_some_and(|hex| {
        hex.len() == 32
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Resume or settle an existing journal for this exact confirmation.
fn resume(
    record: Value,
    record_path: &Path,
    selection: &Selection,
    confirm: &str,
) -> Result<Option<Value>> {
    let bank = &selection.bank;
    let preview_value = get(&record, "preview");
    require(
        text(&record, "schema") == Some(SCHEMA)
            && text(&record, "confirmation") == Some(confirm)
            && matches!(
                text(&record, "status"),
                Some("staging" | "detached" | "complete")
            )
            && text(preview_value, "repository") == selection.repo.to_str()
            && text(preview_value, "path") == bank.to_str(),
        "Purge journal does not bind this exact confirmed repository/path",
    )?;
    let proof = Value::Object(
        preview_value
            .as_object()
            .unwrap_or(&[])
            .iter()
            .filter(|(key, _)| key != "confirmation")
            .cloned()
            .collect(),
    );
    require(
        text(preview_value, "confirmation") == Some(confirm)
            && install::sha(&install::encoded(&proof)?) == confirm,
        "Purge journal inventory differs from the confirmed preview",
    )?;
    let stage = PathBuf::from(text(&record, "stage").unwrap_or_default());
    require(
        stage.parent() == bank.parent()
            && stage
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(is_stage_name),
        "Purge journal quarantine path is invalid",
    )?;
    let root_identity = get(preview_value, "root_identity").clone();
    let replaced = |path: &Path| -> Result<bool> {
        Ok(!install::lexists(path)? || !install::same(&identity(path)?, &root_identity))
    };
    if text(&record, "status") == Some("complete") {
        require(
            get(&record, "deletion_readback_verified") == &Value::Bool(true)
                && !install::lexists(&stage)?
                && replaced(bank)?,
            "Completed purge readback differs from its receipt; preserve and reconcile the targets",
        )?;
        return outcome(record, bank, false).map(Some);
    }
    if install::lexists(&stage)? {
        require(
            install::same(&identity(&stage)?, &root_identity),
            "Purge quarantine root identity changed",
        )?;
        if !install::lexists(&stage.join(LOCK))? {
            require(
                names(&stage, 1)?.is_empty() && install::same(&identity(&stage)?, &root_identity),
                "Purge quarantine lost its lock while containing data",
            )?;
            return finish(record, record_path, bank, &stage, None).map(Some);
        }
        let lock_hash = text(&record, "lock_hash").map(str::to_owned);
        return bank_lock(
            &stage,
            Some(lock_hash.as_deref().unwrap_or_default()),
            |owner| finish(record, record_path, bank, &stage, Some(owner)),
        )
        .map(Some);
    }
    if text(&record, "status") == Some("detached") {
        require(
            replaced(bank)?,
            "Confirmed bank reappeared at its original path; preserve and reconcile it",
        )?;
        let mut record = record;
        set(&mut record, "status", string("complete"));
        set(&mut record, "deletion_readback_verified", Value::Bool(true));
        set(
            &mut record,
            "completed_at",
            string(crate::host::utc_now_isoformat()?),
        );
        save(record_path, &record)?;
        return outcome(record, bank, false).map(Some);
    }
    Ok(None)
}

/// Baseline `run`.
pub fn run(env: &Environment, services: &Services, request: &PurgeRequest) -> Result<Value> {
    let selection = selection(env, services, request)?;
    let state = selection.locations.state.clone();
    if request.check {
        return Ok(object([
            ("operation", string("purge-data")),
            ("verified", Value::Bool(false)),
            ("check_complete", Value::Bool(true)),
            ("writes_performed", Value::Bool(false)),
            ("preview", preview(&selection, None)?),
            (
                "retained",
                object([
                    ("installation_state", string(install::path_text(&state)?)),
                    (
                        "runtime",
                        string(install::path_text(&selection.locations.runtime)?),
                    ),
                    ("git_history_and_external_backups", Value::Bool(true)),
                ]),
            ),
        ]));
    }
    let confirm = request.confirm.clone().unwrap_or_default();
    require(
        is_digest(&confirm),
        "Explicit confirmation must equal the exact preview digest",
    )?;
    let records = state.join("purges");
    let record_path = records.join(format!("{confirm}.json"));
    install::unchanged_parent(&record_path)?;
    let paths = vec![state.clone(), selection.bank.clone()];
    crate::runtime_faults::operation("purge", &paths, || {
        install::locked(env, &state, || {
            let record = if install::lexists(&record_path)? {
                Some(read_record(&record_path)?)
            } else {
                None
            };
            if let Some(record) = record.clone() {
                if let Some(settled) = resume(record, &record_path, &selection, &confirm)? {
                    return Ok(settled);
                }
            }
            let mut barrier = crate::bank_writes::turn_sources_provider()?
                .canonical_writer_barrier(&selection.repo)?;
            let result = bank_lock(&selection.bank, None, |owner| {
                detach(record, &record_path, &records, &selection, &confirm, owner)
            });
            let finished = barrier.finish();
            let value = result?;
            finished?;
            Ok(value)
        })
    })
}

/// The locked detach: fresh preview, journal, native rename, then finish.
fn detach(
    record: Option<Value>,
    record_path: &Path,
    records: &Path,
    selection: &Selection,
    confirm: &str,
    owner: &[u8],
) -> Result<Value> {
    let bank = &selection.bank;
    let state = &selection.locations.state;
    crate::bank_writes::fault("purge-bank-lock-ready")?;
    let selected = preview(selection, Some(owner))?;
    require(
        text(&selected, "confirmation") == Some(confirm),
        "Purge preview is stale; inspect a fresh exact preview",
    )?;
    require(
        crate::repository_config::bank_path(&selection.repo)? == *bank,
        "Bank selection changed during purge preflight",
    )?;
    let record = match record {
        None => {
            let directory = install::named(records)?;
            require(
                directory.is_none_or(Identity::is_directory),
                "Purge journal directory is linked or unrelated",
            )?;
            if directory.is_none() {
                crate::runtime_faults::mutate("purge-journal-directory", || {
                    fs::private_mkdir(records)?;
                    install::sync_dir(state)
                })?;
            }
            let parent = bank
                .parent()
                .ok_or_else(|| contract("Purge bank has no parent"))?;
            let stage = parent.join(format!(
                "{STAGE_PREFIX}{}",
                crate::host::uuid4()?.replace('-', "")
            ));
            let record = object([
                ("schema", string(SCHEMA)),
                ("status", string("staging")),
                ("confirmation", string(confirm)),
                ("preview", selected),
                ("stage", string(install::path_text(&stage)?)),
                ("lock_hash", string(install::sha(owner))),
            ]);
            save(record_path, &record)?;
            record
        }
        Some(mut record) => {
            require(
                install::same(&selected, get(&record, "preview")),
                "Current bank differs from pending purge consent",
            )?;
            set(&mut record, "lock_hash", string(install::sha(owner)));
            save(record_path, &record)?;
            record
        }
    };
    let stage = PathBuf::from(text(&record, "stage").unwrap_or_default());
    install::native_rename(bank, &stage, false)?;
    crate::bank_writes::fault("purge-after-detach")?;
    finish(record, record_path, bank, &stage, Some(owner))
}

/// `am purge-data`: the typed baseline `execute`.
pub fn purge_command(
    env: &Environment,
    services: &Services,
    request: &PurgeRequest,
) -> CommandResult {
    let outcome = run(env, services, request).and_then(|result| {
        crate::runtime_faults::response("purge", &result)?;
        Ok(result)
    });
    printed(match outcome {
        Ok(value) => value,
        Err(error) => object([
            ("operation", string("purge-data")),
            ("verified", Value::Bool(false)),
            ("error", string(error.to_string())),
            ("repair", string(REPAIR)),
        ]),
    })
}
