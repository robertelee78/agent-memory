//! Checked immutable recovery evidence, never a second canonical memory bank.
use crate::codec::json::Value;
use crate::developer_identity::{digest, encoded, load_with_limit, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{self, Descriptor, Identity, ReadError, trust};
use crate::lifecycle::{get, object, string};
use crate::{bank_writes, local_registry, sharing_limits};
use std::path::{Path, PathBuf};

pub const BINDINGS: &str = "agent-memory/layout-recovery-bindings-v1";
pub const INDEX: &str = "agent-memory/layout-recovery-index-v1";
const RECORD: &str = "Recovery evidence must be bounded owner-only regular single-link files";
const CHANGED: &str = "Recovery evidence or its named parent changed during observation";

pub fn sha(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn signature(info: &Identity) -> Identity {
    fs::signature(info)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn parts(path: &Path) -> Result<(&Path, &std::ffi::OsStr)> {
    Ok((
        path.parent()
            .ok_or_else(|| ContractError::new("Recovery evidence has no parent directory"))?,
        path.file_name()
            .ok_or_else(|| ContractError::new("Recovery evidence has no filename"))?,
    ))
}
fn named(path: &Path) -> Result<Option<Identity>> {
    if path == Path::new("/") {
        return Descriptor::pin_directory(path)
            .and_then(|dir| dir.stat())
            .map(Some)
            .map_err(io_error);
    }
    let (parent, name) = parts(path)?;
    let directory = match Descriptor::pin_directory(parent) {
        Ok(directory) => directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    match directory.stat_at(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

/// `maximum` is explicit; ordinary callers use `sharing_limits::MAX_PRIVATE`.
pub fn read_bytes(path: &Path, maximum: usize, missing: bool) -> Result<Option<Vec<u8>>> {
    trust::record(path)?;
    if missing && named(path)?.is_none() {
        return Ok(None);
    }
    let (parent, name) = parts(path)?;
    let directory = Descriptor::open_directory(parent).map_err(io_error)?;
    let before_parent = directory.stat().map_err(io_error)?;
    let mut file = directory
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    let before = file.stat().map_err(io_error)?;
    if !before.is_regular()
        || before.uid != crate::host::effective_uid()
        || before.mode & 0o7777 != 0o600
        || before.nlink != 1
        || before.size < 0
        || before.size as u128 > maximum as u128
    {
        return Err(ContractError::new(RECORD));
    }
    let bound = maximum
        .checked_add(1)
        .ok_or_else(|| ContractError::new(RECORD))?;
    let raw = file.read_prefix(bound).map_err(|error| match error {
        ReadError::SourceChanged | ReadError::ParentChanged => ContractError::new(CHANGED),
        other => io_error(other),
    })?;
    let after = file.stat().map_err(io_error)?;
    let current = directory.stat_at(name).map_err(io_error)?;
    let current_parent = named(parent)?.ok_or_else(|| ContractError::new(CHANGED))?;
    if raw.len() as u128 != before.size as u128
        || signature(&before) != signature(&after)
        || signature(&after) != signature(&current)
        || !before_parent.same_inode(current_parent)
    {
        return Err(ContractError::new(CHANGED));
    }
    Ok(Some(raw))
}
pub fn read(path: &Path, maximum: usize, missing: bool) -> Result<Option<Value>> {
    let Some(raw) = read_bytes(path, maximum, missing)? else {
        return Ok(None);
    };
    let value = load_with_limit(&raw, maximum)?;
    if encoded(&value)? != raw {
        return Err(ContractError::new(
            "Immutable recovery JSON is not its exact canonical encoded record",
        ));
    }
    Ok(Some(value))
}
pub fn private_parent(path: &Path) -> Result<()> {
    trust::inspect(path, trust::InspectOptions::default())?;
    local_registry::private_mkdir(path)?;
    trust::state(path)?;
    if Descriptor::pin_directory(path)
        .map_err(io_error)?
        .stat()
        .map_err(io_error)?
        .mode
        & 0o7777
        != 0o700
    {
        return Err(ContractError::new(
            "Recovery evidence directories must be exactly 0700; do not chmod existing state",
        ));
    }
    Ok(())
}
fn raw_digest(raw: &[u8]) -> String {
    // Passing Some establishes that digest always returns Some here.
    digest(Some(raw)).expect("digest of present bytes is present")
}
pub fn put_bytes(path: &Path, raw: &[u8]) -> Result<String> {
    if raw.len() > sharing_limits::MAX_PRIVATE {
        return Err(ContractError::new(
            "Recovery evidence exceeds its private artifact bound",
        ));
    }
    let (parent, name) = parts(path)?;
    private_parent(parent)?;
    if let Some(existing) = read_bytes(path, sharing_limits::MAX_PRIVATE, true)? {
        if existing != raw {
            return Err(ContractError::new(
                "Immutable recovery evidence already exists with different bytes",
            ));
        }
        return Ok(raw_digest(raw));
    }
    let directory = Descriptor::open_directory(parent).map_err(io_error)?;
    let mut file = directory
        .create_exclusive(name, libc::O_WRONLY, 0o600)
        .map_err(io_error)?;
    file.write_all(raw).map_err(io_error)?;
    file.fsync().map_err(io_error)?;
    directory.fsync_directory().map_err(io_error)?;
    drop(file);
    drop(directory);
    if read_bytes(path, sharing_limits::MAX_PRIVATE, false)?.as_deref() != Some(raw) {
        return Err(ContractError::new(
            "Recovery evidence readback differs; retain the interrupted operation",
        ));
    }
    Ok(raw_digest(raw))
}
pub fn write_once(path: &Path, value: &Value) -> Result<String> {
    put_bytes(path, &sharing_limits::private_bytes(value)?)
}
pub fn replace(path: &Path, value: &Value, expected_sha256: Option<&str>) -> Result<String> {
    private_parent(parts(path)?.0)?;
    let current = read_bytes(path, sharing_limits::MAX_PRIVATE, true)?;
    if digest(current.as_deref()).as_deref() != expected_sha256 {
        return Err(ContractError::new(
            "Recovery state changed; retain evidence and inspect the current generation",
        ));
    }
    let raw = sharing_limits::private_bytes(value)?;
    local_registry::replace_with_limit(path, &raw, expected_sha256, sharing_limits::MAX_PRIVATE)?;
    if read_bytes(path, sharing_limits::MAX_PRIVATE, false)?.as_deref() != Some(raw.as_slice()) {
        return Err(ContractError::new(
            "Recovery state atomic publication was not verified",
        ));
    }
    Ok(raw_digest(&raw))
}
fn fields(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == expected.len() && pairs.iter().all(|(k, _)| expected.contains(&k.as_str()))
    })
}
fn lifecycle(value: &Value, allowed: &[&str]) -> bool {
    value.as_str().is_some_and(|state| allowed.contains(&state))
}
fn path_text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "recovery evidence path")
}
pub fn bindings() -> Result<Value> {
    let (_, path, lock) = local_registry::paths("layout-recovery-bindings")?;
    if named(&lock)?.is_some() {
        return Err(ContractError::new(
            "Recovery bindings are locked; retain private evidence",
        ));
    }
    let Some(value) = read(&path, sharing_limits::MAX_PRIVATE, true)? else {
        return Ok(object([
            ("schema", string(BINDINGS)),
            ("bindings", Value::Object(vec![])),
        ]));
    };
    if !fields(&value, &["schema", "bindings"])
        || get(&value, "schema").as_str() != Some(BINDINGS)
        || get(&value, "bindings").as_object().is_none()
    {
        return Err(ContractError::new(
            "Unknown recovery state binding registry",
        ));
    }
    if let Some(rows) = get(&value, "bindings").as_object() {
        for (operation, row) in rows {
            if !uuid_valid(&string(operation))
                || !fields(
                    row,
                    &[
                        "repo_root",
                        "bank_path",
                        "checkout_key",
                        "project_id",
                        "state_root",
                        "index_path",
                        "lifecycle_state",
                    ],
                )
                || !uuid_valid(get(row, "project_id"))
                || get(row, "checkout_key").as_str().is_none()
                || !lifecycle(
                    get(row, "lifecycle_state"),
                    &["active", "terminal", "interrupted", "cleaned"],
                )
            {
                return Err(ContractError::new("Unknown recovery state binding"));
            }
            for name in ["repo_root", "bank_path", "state_root", "index_path"] {
                let refusal = || {
                    ContractError::new(
                        "Recovery binding moved or became redirected; restore the original locations",
                    )
                };
                let supplied = get(row, name).as_str().ok_or_else(refusal)?;
                let path = Path::new(supplied);
                if !path.is_absolute() || path != fs::canonical_path(path, false)? {
                    return Err(refusal());
                }
            }
        }
    }
    Ok(value)
}

pub type RecoveryPaths = (PathBuf, PathBuf, PathBuf, Option<PathBuf>);
/// The executing runtime root is explicit: Rust has no Python `__file__` root.
pub fn paths(
    repo: &Path,
    operation_id: Option<&str>,
    runtime_root: &Path,
) -> Result<RecoveryPaths> {
    let root = fs::canonical_path(repo, false)?;
    let bank = root.join(crate::repository_config::recovery_settings(&root)?.bank_root);
    _paths_for_bank(&root, &bank, operation_id, runtime_root)
}
pub fn _paths_for_bank(
    root: &Path,
    bank: &Path,
    operation_id: Option<&str>,
    runtime_root: &Path,
) -> Result<RecoveryPaths> {
    crate::bank_contract::bank_root(bank)?;
    if !named(bank)?.is_some_and(|info| info.is_directory() && !info.is_symlink()) {
        return Err(ContractError::new(
            "Recovery requires the existing configured ordinary bank directory",
        ));
    }
    let generated;
    // state_paths is stricter than its Python predecessor; defer this module's
    // UUID refusal until after its binding guards, using a fresh locator ID.
    let operation = match operation_id.filter(|id| uuid_valid(&string(*id))) {
        Some(operation) => operation,
        None => {
            generated = crate::host::uuid4()?;
            &generated
        }
    };
    let (operations, _) = bank_writes::state_paths(root, bank, operation)?;
    let base = operations
        .parent()
        .ok_or_else(|| ContractError::new("Recovery operations have no parent"))?
        .join("layout-recovery");
    let runtime_root = fs::canonical_path(runtime_root, false)?;
    if base.starts_with(root) || base.starts_with(&runtime_root) {
        return Err(ContractError::new(
            "Recovery state must remain outside the repository and executing runtime",
        ));
    }
    trust::inspect(&base, trust::InspectOptions::default())?;
    let registry = bindings()?;
    if let Some(rows) = get(&registry, "bindings").as_object() {
        for (_, row) in rows {
            if get(row, "repo_root").as_str() == Some(path_text(root)?)
                || get(row, "bank_path").as_str() == Some(path_text(bank)?)
            {
                let index_path = base.join("index.json");
                if get(row, "repo_root").as_str() != Some(path_text(root)?)
                    || get(row, "bank_path").as_str() != Some(path_text(bank)?)
                    || get(row, "index_path").as_str() != Some(path_text(&index_path)?)
                    || get(row, "state_root").as_str() != Some(path_text(state_root(&operations)?)?)
                {
                    return Err(ContractError::new(
                        "Recovery state location changed; restore its original verified binding",
                    ));
                }
                if lifecycle(get(row, "lifecycle_state"), &["active", "interrupted"])
                    && named(&index_path)?.is_none()
                {
                    return Err(ContractError::new(
                        "Bound recovery index is missing; coverage remains pending",
                    ));
                }
            }
        }
    }
    if operation_id.is_some_and(|id| !uuid_valid(&string(id))) {
        return Err(ContractError::new(
            "Recovery operations require canonical UUIDv4 identifiers",
        ));
    }
    let directory = operation_id
        .filter(|id| !id.is_empty())
        .map(|id| base.join(id));
    Ok((root.to_path_buf(), bank.to_path_buf(), base, directory))
}
fn state_root(operations: &Path) -> Result<&Path> {
    operations
        .ancestors()
        .nth(3)
        .ok_or_else(|| ContractError::new("Recovery operations have no state root"))
}
pub fn index(repo: &Path, runtime_root: &Path) -> Result<Value> {
    let (root, bank, _, _) = paths(repo, None, runtime_root)?;
    _index_for_bank(&root, &bank, runtime_root)
}
pub fn _index_for_bank(root: &Path, bank: &Path, runtime_root: &Path) -> Result<Value> {
    let (root, bank, base, _) = _paths_for_bank(root, bank, None, runtime_root)?;
    let Some(value) = read(&base.join("index.json"), sharing_limits::MAX_PRIVATE, true)? else {
        return Ok(object([
            ("schema", string(INDEX)),
            ("repo_root", string(path_text(&root)?)),
            ("bank_path", string(path_text(&bank)?)),
            ("operations", Value::Object(vec![])),
        ]));
    };
    if !fields(&value, &["schema", "repo_root", "bank_path", "operations"])
        || get(&value, "schema").as_str() != Some(INDEX)
        || get(&value, "repo_root").as_str() != Some(path_text(&root)?)
        || get(&value, "bank_path").as_str() != Some(path_text(&bank)?)
        || get(&value, "operations").as_object().is_none()
    {
        return Err(ContractError::new(
            "Unknown, foreign or damaged recovery operation index",
        ));
    }
    if let Some(rows) = get(&value, "operations").as_object() {
        for (operation, row) in rows {
            if !uuid_valid(&string(operation))
                || !fields(
                    row,
                    &[
                        "state",
                        "project_id",
                        "ledger_head_sha256",
                        "proposal_id",
                        "hold_id",
                    ],
                )
                || !uuid_valid(get(row, "project_id"))
                || !uuid_valid(get(row, "proposal_id"))
                || !uuid_valid(get(row, "hold_id"))
                || !sha(get(row, "ledger_head_sha256"))
                || !lifecycle(
                    get(row, "state"),
                    &[
                        "active",
                        "terminal",
                        "interrupted",
                        "rolled-back",
                        "cleaned",
                    ],
                )
            {
                return Err(ContractError::new(
                    "Recovery operation index has an invalid generation or identity",
                ));
            }
        }
    }
    Ok(value)
}
pub fn set_index(
    repo: &Path,
    value: &Value,
    expected_sha256: Option<&str>,
    runtime_root: &Path,
) -> Result<String> {
    let (_, _, base, _) = paths(repo, None, runtime_root)?;
    replace(&base.join("index.json"), value, expected_sha256)
}
pub fn bind(
    repo: &Path,
    operation_id: &str,
    project_id: &str,
    checkout_key: &str,
    lifecycle_state: &str,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, base, _) = paths(repo, Some(operation_id), runtime_root)?;
    let (operations, _) = bank_writes::state_paths(&root, &bank, operation_id)?;
    let (config, target, _) = local_registry::paths("layout-recovery-bindings")?;
    private_parent(&config)?;
    let mut current = bindings()?;
    let old = digest(read_bytes(&target, sharing_limits::MAX_PRIVATE, true)?.as_deref());
    let row = object([
        ("repo_root", string(path_text(&root)?)),
        ("bank_path", string(path_text(&bank)?)),
        ("checkout_key", string(checkout_key)),
        ("project_id", string(project_id)),
        ("state_root", string(path_text(state_root(&operations)?)?)),
        ("index_path", string(path_text(&base.join("index.json"))?)),
        ("lifecycle_state", string(lifecycle_state)),
    ]);
    let Some(Value::Object(rows)) = current.get_mut("bindings") else {
        return Err(ContractError::new(
            "Unknown recovery state binding registry",
        ));
    };
    if let Some((_, existing)) = rows.iter_mut().find(|(id, _)| id == operation_id) {
        if let Some(fields) = row.as_object() {
            if fields
                .iter()
                .any(|(name, value)| name != "lifecycle_state" && get(existing, name) != value)
            {
                return Err(ContractError::new(
                    "Recovery binding cannot move or change identity",
                ));
            }
        }
        *existing = row.clone();
    } else {
        rows.push((operation_id.to_owned(), row.clone()));
    }
    replace(&target, &current, old.as_deref())?;
    Ok(row)
}
