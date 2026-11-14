//! Append-only source-bound terminal evidence and atomic recovery hold handoff.
//!
//! The executing runtime root is explicit on every entry point (as in
//! `layout_recovery_state::paths`). Shared value helpers used by the other
//! layout-recovery modules live here once.
use crate::bank_writes::{self as writes, RecoveryFrontier};
use crate::client_hooks::membership_equal;
use crate::codec::json::Value;
use crate::developer_identity::{encoded, sha256_hex, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{Descriptor, Identity, RenameMode};
use crate::host::Sources;
use crate::lifecycle::{get, object, string};
use crate::scope_binding::VerifiedEnrollmentHold;
use crate::{
    bank_observation, bank_transactions as tx, host, layout_history,
    layout_recovery_observation as raw, layout_recovery_state as state, lifecycle,
    local_registry as storage, records, sharing_limits,
};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const OUTPUT_PLAN: &str = "agent-memory/governed-output-plan-v1";
pub const OUTPUT_PLAN_FIELDS: &[&str] = &[
    "schema",
    "operation",
    "repo_root",
    "project_id",
    "operation_id",
    "recorded_at",
    "request_input_sha256",
    "observed_hashes",
    "files",
];
const LEDGER_HEAD: &str = "agent-memory/layout-ledger-head-v1";
const LEDGER_TRANSITION: &str = "agent-memory/layout-ledger-transition-v1";
const RESOLUTION_JOURNAL: &str = "agent-memory/layout-resolution-journal-v1";
const RESOLUTION_PROPOSAL: &str = "agent-memory/layout-resolution-proposal-v1";
const JOURNAL_FIELDS: &[&str] = &["schema", "proposal", "units", "hold", "scope_context"];
/// Baseline 4096-byte ledger-head read bound.
const HEAD_MAX_BYTES: usize = 4096;
/// Recovery sentinel file name (baseline layout_recovery.SENTINEL).
pub const SENTINEL_FILE: &str = ".agent-memory-recovery.md";
/// Declared bound on private recovery directory listings (baseline was unbounded).
pub const MAX_LISTING: usize = raw::MAX_ENTRIES;
/// A missing field of retained private evidence (baseline KeyError/TypeError).
pub const MALFORMED: &str =
    "Recovery evidence is malformed or incomplete; preserve it for inspection";

// ---- shared helpers (one definition for the layout-recovery modules) ----

pub(crate) fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
pub(crate) fn io_error(error: std::io::Error) -> ContractError {
    ContractError::new(error.to_string())
}
/// Python structural `==` between decoded JSON values.
pub(crate) fn same(left: &Value, right: &Value) -> bool {
    membership_equal(left, right)
}
pub(crate) fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| err(MALFORMED))
}
pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?.as_str().ok_or_else(|| err(MALFORMED))
}
pub(crate) fn int(value: &Value) -> Option<i128> {
    value.as_int().and_then(|number| number.as_i128())
}
pub(crate) fn number(value: impl Into<crate::codec::json::Integer>) -> Value {
    Value::Int(value.into())
}
pub(crate) fn items(value: &Value) -> Result<&[Value]> {
    value.as_array().ok_or_else(|| err(MALFORMED))
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}
/// `digest(encoded(value))`.
pub(crate) fn sha(value: &Value) -> Result<String> {
    Ok(digest(&encoded(value)?))
}
pub(crate) fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .map(|pairs| pairs.iter().map(|(key, _)| key.as_str()).collect())
        .unwrap_or_default()
}
/// `isinstance(value, dict) and set(value) == expected`.
pub(crate) fn has_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some() && keys(value) == expected.iter().copied().collect()
}
pub(crate) fn uuid(value: &Value) -> bool {
    uuid_valid(value)
}
/// `{**value, key: field, ...}`: replace in place, otherwise append.
pub(crate) fn with(
    mut value: Value,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> Value {
    if let Value::Object(pairs) = &mut value {
        for (key, field) in fields {
            if let Some(slot) = pairs.iter_mut().find(|(name, _)| name == key) {
                slot.1 = field;
            } else {
                pairs.push((key.to_owned(), field));
            }
        }
    }
    value
}
pub(crate) fn set(value: &mut Value, key: &str, field: Value) {
    if let Value::Object(pairs) = value {
        if let Some(slot) = pairs.iter_mut().find(|(name, _)| name == key) {
            slot.1 = field;
        } else {
            pairs.push((key.to_owned(), field));
        }
    }
}
pub(crate) fn path_text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "recovery path")
}
/// `str(generation).zfill(8) + '.json'`.
pub(crate) fn generation_name(generation: &Value) -> Result<String> {
    let generation = int(generation).ok_or_else(|| err(MALFORMED))?;
    Ok(format!("{generation:08}.json"))
}
pub(crate) fn read_value(path: &Path) -> Result<Value> {
    state::read(path, sharing_limits::MAX_PRIVATE, false)?.ok_or_else(|| err(MALFORMED))
}
pub(crate) fn read_optional(path: &Path) -> Result<Option<Value>> {
    state::read(path, sharing_limits::MAX_PRIVATE, true)
}
pub(crate) fn read_raw(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    state::read_bytes(path, maximum, false)?.ok_or_else(|| err(MALFORMED))
}
/// `digest(state.read_bytes(path))` of a required private record.
pub(crate) fn file_sha(path: &Path) -> Result<String> {
    Ok(digest(&read_raw(path, sharing_limits::MAX_PRIVATE)?))
}
pub(crate) fn frontier() -> Result<Arc<dyn RecoveryFrontier>> {
    writes::recovery_frontier_provider()
}
pub(crate) fn safe_text(value: &Value, maximum: usize) -> Result<()> {
    crate::comparison::safe_text(value, maximum).map(|_| ())
}
/// `state.paths(repo, operation_id)` with its operation directory.
pub(crate) fn operation_paths(
    repo: &Path,
    operation_id: &str,
    runtime_root: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let (root, bank, base, directory) = state::paths(repo, Some(operation_id), runtime_root)?;
    let directory =
        directory.ok_or_else(|| err("Recovery operations require canonical UUIDv4 identifiers"))?;
    Ok((root, bank, base, directory))
}
pub(crate) fn operations(indexed: &Value) -> Result<&Value> {
    field(indexed, "operations")
}
pub(crate) fn operation_row<'a>(indexed: &'a Value, operation_id: &str) -> Result<&'a Value> {
    field(operations(indexed)?, operation_id)
}
pub(crate) fn set_operation_row(indexed: &mut Value, operation_id: &str, row: Value) {
    if let Some(rows) = indexed.get_mut("operations") {
        set(rows, operation_id, row);
    }
}
/// `state.set_index(root, indexed, digest(state.read_bytes(base / 'index.json')))`.
pub(crate) fn publish_index(
    root: &Path,
    base: &Path,
    indexed: &Value,
    runtime_root: &Path,
) -> Result<()> {
    let current = file_sha(&base.join("index.json"))?;
    state::set_index(root, indexed, Some(&current), runtime_root)?;
    Ok(())
}
/// Named no-follow status, `None` when absent.
pub(crate) fn lstat(path: &Path) -> Result<Option<Identity>> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Descriptor::pin_directory(path)
            .and_then(|directory| directory.stat())
            .map(Some)
            .map_err(io_error);
    };
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
/// Sorted names of a private directory, bounded by `MAX_LISTING`.
pub(crate) fn listing(directory: &Path) -> Result<Vec<String>> {
    let names = Descriptor::open_directory(directory)
        .and_then(|held| held.entries_bounded(MAX_LISTING))
        .map_err(io_error)?
        .ok_or_else(|| err("Recovery evidence directory exceeds its listing bound"))?;
    names
        .into_iter()
        .map(|name| {
            name.into_string()
                .map_err(|_| err("Unknown recovery evidence name"))
        })
        .collect()
}
/// Remove one named bank entry relative to the held bank directory.
pub(crate) fn unlink_bank_entry(bank: &Path, name: &str) -> Result<()> {
    let directory = Descriptor::open_directory(bank).map_err(io_error)?;
    directory.unlink_at(OsStr::new(name)).map_err(io_error)?;
    Ok(())
}
/// `with canonical_writer_barrier(root), storage.exclusive(bank lock, id, 'bank-recover')`.
pub(crate) fn locked<T>(
    root: &Path,
    bank: &Path,
    id: &str,
    body: impl FnOnce(&[u8]) -> Result<T>,
) -> Result<T> {
    let mut barrier =
        crate::turn_sources::canonical_writer_barrier(root, None, &crate::cli::pack_root()?)?;
    let outcome = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        id,
        "bank-recover",
        Sources::default(),
        body,
    )?;
    barrier.finish()?;
    Ok(outcome)
}
pub(crate) fn hold_marker(held: &VerifiedEnrollmentHold) -> Result<Value> {
    frontier()?.marker_value(held.marker())
}
/// Require an actual verified hold where the baseline dereferenced it.
pub(crate) fn require_hold(held: Option<VerifiedEnrollmentHold>) -> Result<VerifiedEnrollmentHold> {
    held.ok_or_else(|| err("Recovery requires the exact persistent checked hold"))
}
pub(crate) fn unit_ids(token: &Value) -> Result<Vec<String>> {
    items(field(token, "unit_ids")?)?
        .iter()
        .map(|id| id.as_str().map(str::to_owned).ok_or_else(|| err(MALFORMED)))
        .collect()
}
/// `(source_id, start, end)` ordering key of a source range.
pub(crate) fn range_key(range: &Value) -> Result<(String, i128, i128)> {
    Ok((
        text(range, "source_id")?.to_owned(),
        int(field(range, "start")?).ok_or_else(|| err(MALFORMED))?,
        int(field(range, "end")?).ok_or_else(|| err(MALFORMED))?,
    ))
}
pub(crate) fn sorted_ranges(ranges: &[Value]) -> Result<Vec<Value>> {
    let mut keyed = Vec::new();
    for range in ranges {
        keyed.push((range_key(range)?, range.clone()));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(keyed.into_iter().map(|(_, range)| range).collect())
}

// ---- baseline functions ----

/// Durable atomic replacement of the bank transaction marker.
pub fn atomic_marker(
    bank: &Path,
    expected: Option<&[u8]>,
    replacement: &[u8],
    owner: Option<&[u8]>,
) -> Result<()> {
    bank_observation::markers(bank, owner, expected)?;
    let before = lstat(bank)?
        .ok_or_else(|| err("Bank parent binding changed before atomic recovery marker handoff"))?;
    let name = format!(
        ".agent-memory-marker-{}.tmp",
        host::uuid4()?.replace('-', "")
    );
    let directory = Descriptor::open_directory(bank).map_err(io_error)?;
    let mut file = directory
        .create_exclusive(OsStr::new(&name), libc::O_WRONLY, 0o600)
        .map_err(io_error)?;
    let outcome = (|| -> Result<()> {
        file.write_all(replacement).map_err(io_error)?;
        file.fsync().map_err(io_error)?;
        bank_observation::markers(bank, owner, expected)?;
        let current = lstat(bank)?.ok_or_else(|| {
            err("Bank parent binding changed before atomic recovery marker handoff")
        })?;
        if !before.same_inode(current) {
            return Err(err(
                "Bank parent binding changed before atomic recovery marker handoff",
            ));
        }
        crate::fs::rename_at(
            &directory,
            OsStr::new(&name),
            &directory,
            OsStr::new(tx::MARKER),
            RenameMode::Replace,
        )
        .map_err(io_error)?;
        tx::flush_directory(bank)?;
        bank_observation::markers(bank, owner, Some(replacement))
    })();
    drop(file);
    if directory.stat_at(OsStr::new(&name)).is_ok() {
        directory.unlink_at(OsStr::new(&name)).map_err(io_error)?;
    }
    outcome
}

/// Validate, but never publish, one exact journal-bound head/index gap.
pub fn interrupted_successor(
    repo: &Path,
    operation_id: &str,
    units: &Value,
    expected_head: &Value,
    unit_map_sha256: Option<&str>,
    runtime_root: &Path,
) -> Result<(Value, Value, Value)> {
    let (root, bank, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let indexed = state::index(&root, runtime_root)?;
    let row = operation_row(&indexed, operation_id)?;
    let observed = state::read(&directory.join("ledger-head.json"), HEAD_MAX_BYTES, false)?
        .ok_or_else(|| err(MALFORMED))?;
    let generation = int(field(expected_head, "generation")?).ok_or_else(|| err(MALFORMED))? + 1;
    let previous = read_value(
        &directory
            .join("ledger")
            .join(generation_name(field(expected_head, "generation")?)?),
    )?;
    let lost = || err("Ledger reconciliation lost its exact immutable predecessor");
    let pinned = object([
        ("schema", string(LEDGER_HEAD)),
        ("operation_id", string(operation_id)),
        ("generation", field(&previous, "generation")?.clone()),
        ("sha256", string(sha(&previous)?)),
    ]);
    if !same(expected_head, &pinned)
        || get(&previous, "operation_id").as_str() != Some(operation_id)
        || !same(field(&previous, "project_id")?, field(row, "project_id")?)
    {
        return Err(lost());
    }
    let mut successor = with(
        previous.clone(),
        [
            ("units", units.clone()),
            ("generation", number(generation)),
            (
                "previous_generation_sha256",
                field(expected_head, "sha256")?.clone(),
            ),
        ],
    );
    if let Some(unit_map) = unit_map_sha256 {
        set(&mut successor, "unit_map_sha256", string(unit_map));
    }
    let head = object([
        ("schema", field(expected_head, "schema")?.clone()),
        ("operation_id", string(operation_id)),
        ("generation", number(generation)),
        ("sha256", string(sha(&successor)?)),
    ]);
    let transition = read_value(
        &directory
            .join("ledger-transitions")
            .join(generation_name(&number(generation))?),
    )?;
    let expected_transition = object([
        ("schema", string(LEDGER_TRANSITION)),
        ("operation_id", string(operation_id)),
        ("previous_head", expected_head.clone()),
        ("head", head.clone()),
        ("units_sha256", string(sha(units)?)),
    ]);
    let indexed_head = get(row, "ledger_head_sha256").as_str();
    if !same(&observed, &head)
        || !same(&transition, &expected_transition)
        || (indexed_head != Some(sha(expected_head)?.as_str())
            && indexed_head != Some(sha(&head)?.as_str()))
    {
        return Err(err(
            "Ledger retry does not match its exact journaled successor",
        ));
    }
    let projected = with(row.clone(), [("ledger_head_sha256", string(sha(&head)?))]);
    let frontier = frontier()?;
    let (_, verified_head, current) =
        frontier.ledger(&root, operation_id, &projected, &bank, runtime_root)?;
    if !same(&verified_head, &head) || !same(&current, &successor) {
        return Err(err(
            "Journaled ledger successor differs from retained immutable evidence",
        ));
    }
    frontier.evidence(&directory, &current, &projected, runtime_root)?;
    Ok((projected, head, current))
}

/// Publish one immutable ledger generation, then its head and index row.
#[allow(clippy::too_many_arguments)]
pub fn advance_ledger(
    repo: &Path,
    operation_id: &str,
    units: &Value,
    expected_head: &Value,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
    unit_map_sha256: Option<&str>,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, base, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let frontier = frontier()?;
    let marker = match transaction.filter(|marker| !marker.is_empty()) {
        Some(marker) => Some(marker.to_vec()),
        None => frontier.marker_bytes(&bank, tx::MARKER, None)?,
    };
    bank_observation::markers(&bank, owner, marker.as_deref())?;
    let mut indexed = state::index(&root, runtime_root)?;
    let row = operation_row(&indexed, operation_id)?.clone();
    let observed_head = state::read(&directory.join("ledger-head.json"), HEAD_MAX_BYTES, false)?
        .ok_or_else(|| err(MALFORMED))?;
    let generation = int(field(expected_head, "generation")?).ok_or_else(|| err(MALFORMED))? + 1;
    let generation_file = generation_name(&number(generation))?;
    let transition_path = directory.join("ledger-transitions").join(&generation_file);
    if !same(&observed_head, expected_head) {
        let (projected, _, _) = interrupted_successor(
            &root,
            operation_id,
            units,
            expected_head,
            unit_map_sha256,
            runtime_root,
        )?;
        set_operation_row(&mut indexed, operation_id, projected.clone());
        bank_observation::markers(&bank, owner, marker.as_deref())?;
        publish_index(&root, &base, &indexed, runtime_root)?;
        let (_, head, current) =
            frontier.ledger(&root, operation_id, &projected, &bank, runtime_root)?;
        frontier.invalidate(&root)?;
        return Ok(object([
            ("head", head),
            ("ledger", current),
            ("already_applied", Value::Bool(true)),
        ]));
    }
    let (_, head, current) = frontier.ledger(&root, operation_id, &row, &bank, runtime_root)?;
    let mut successor = with(
        current,
        [
            ("units", units.clone()),
            ("generation", number(generation)),
            (
                "previous_generation_sha256",
                field(&head, "sha256")?.clone(),
            ),
        ],
    );
    if let Some(unit_map) = unit_map_sha256 {
        let retained = state::sha(&string(unit_map))
            && file_sha(&directory.join("unit-maps").join(format!("{unit_map}.json")))
                .is_ok_and(|actual| actual == unit_map);
        if !retained {
            return Err(err(
                "Unit revision must retain its exact immutable reviewed source map",
            ));
        }
        set(&mut successor, "unit_map_sha256", string(unit_map));
    }
    let payload_sha =
        state::write_once(&directory.join("ledger").join(&generation_file), &successor)?;
    let new_head = object([
        ("schema", field(&head, "schema")?.clone()),
        ("operation_id", string(operation_id)),
        ("generation", number(generation)),
        ("sha256", string(payload_sha)),
    ]);
    state::write_once(
        &transition_path,
        &object([
            ("schema", string(LEDGER_TRANSITION)),
            ("operation_id", string(operation_id)),
            ("previous_head", head.clone()),
            ("head", new_head.clone()),
            ("units_sha256", string(sha(units)?)),
        ]),
    )?;
    writes::fault("layout-recovery-ledger-generation-before-head")?;
    bank_observation::markers(&bank, owner, marker.as_deref())?;
    state::replace(
        &directory.join("ledger-head.json"),
        &new_head,
        Some(&sha(&head)?),
    )?;
    writes::fault("layout-recovery-ledger-generation-after-head")?;
    let row = with(row, [("ledger_head_sha256", string(sha(&new_head)?))]);
    set_operation_row(&mut indexed, operation_id, row.clone());
    publish_index(&root, &base, &indexed, runtime_root)?;
    let (_, verified_head, verified) =
        frontier.ledger(&root, operation_id, &row, &bank, runtime_root)?;
    if !same(&verified, &successor) || !same(&verified_head, &new_head) {
        return Err(err(
            "Ledger successor readback differs; preserve the typed marker",
        ));
    }
    frontier.invalidate(&root)?;
    Ok(object([
        ("head", verified_head),
        ("ledger", verified),
        ("already_applied", Value::Bool(false)),
    ]))
}

/// Restore the exact persistent hold of an active or retained rolled-back operation.
pub fn restore_hold(
    repo: &Path,
    operation_id: &str,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Vec<u8>> {
    let (root, bank, _, _) = operation_paths(repo, operation_id, runtime_root)?;
    let frontier = frontier()?;
    let token = frontier.observe_owned(&root, owner, transaction)?;
    let indexed = state::index(&root, runtime_root)?;
    let row = operation_row(&indexed, operation_id)?;
    if !matches!(text(row, "state")?, "active" | "rolled-back") {
        return Err(err(
            "Only an active or retained rolled-back recovery operation can restore its hold",
        ));
    }
    let marker = encoded(&object([
        ("schema", string(writes::HOLD)),
        ("project_id", field(row, "project_id")?.clone()),
        ("operation_id", string(operation_id)),
        ("hold_id", field(row, "hold_id")?.clone()),
    ]))?;
    writes::fault("layout-recovery-before-hold-restore")?;
    atomic_marker(&bank, transaction, &marker, owner)?;
    writes::fault("layout-recovery-after-hold-restore")?;
    frontier.check(&token, &root, owner, None)?;
    Ok(marker)
}

fn pending(units: &Value) -> Result<bool> {
    Ok(items(units)?
        .iter()
        .any(|unit| get(unit, "state").as_str() == Some("pending-admission")))
}

/// Retire a fully resolved recovery safeguard and its exact dependencies.
pub fn retire(
    repo: &Path,
    operation_id: &str,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, base, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let frontier = frontier()?;
    let mut indexed = state::index(&root, runtime_root)?;
    let row = operation_row(&indexed, operation_id)?.clone();
    let (_, head, current) = frontier.ledger(&root, operation_id, &row, &bank, runtime_root)?;
    if pending(field(&current, "units")?)? {
        return Err(err(
            "Unresolved source units cannot retire their recovery safeguard",
        ));
    }
    let retirement = read_optional(&directory.join("retirement.json"))?;
    if text(&row, "state")? == "terminal" {
        if retirement.as_ref().is_none_or(|value| {
            !value
                .get("ledger_head")
                .is_some_and(|pinned| same(pinned, &head))
        }) {
            return Err(err(
                "Only the exact durable retirement may finish a terminal safeguard",
            ));
        }
        for (other_id, other_row) in operations(&indexed)?.as_object().unwrap_or_default() {
            if get(other_row, "state").as_str() == Some("cleaned") {
                continue;
            }
            let (other_directory, _, other) =
                frontier.ledger(&root, other_id, other_row, &bank, runtime_root)?;
            frontier.evidence(&other_directory, &other, other_row, runtime_root)?;
            if pending(field(&other, "units")?)?
                || (other_id != operation_id
                    && matches!(text(other_row, "state")?, "active" | "interrupted"))
            {
                return Err(err(
                    "A retained same-bank dependency still requires its safeguard",
                ));
            }
        }
    } else if !unit_ids(&frontier.observe_owned(&root, owner, transaction)?)?.is_empty() {
        return Err(err(
            "Other retained same-bank source units still require this recovery safeguard",
        ));
    }
    let receipt = read_value(&directory.join("receipt.json"))?;
    if get(&receipt, "verified").as_bool() != Some(true) {
        return Err(err(
            "Recovery retirement needs the immutable verified layout receipt",
        ));
    }
    let nested_base = directory.join("nested");
    if lstat(&nested_base)?.is_some() {
        for name in listing(&nested_base)? {
            let nested = nested_base.join(&name);
            let is_directory = lstat(&nested)?.is_some_and(|info| info.is_directory());
            if !is_directory || !uuid(&string(name.as_str())) {
                return Err(err(
                    "Unknown nested dependency prevents recovery retirement",
                ));
            }
            let completion = read_optional(&nested.join("completion.json"))?;
            if completion
                .as_ref()
                .is_none_or(|value| get(value, "verified").as_bool() != Some(true))
            {
                return Err(err(
                    "Nested canonical/resolution dependency remains interrupted",
                ));
            }
        }
    }
    let marker = match transaction.filter(|marker| !marker.is_empty()) {
        Some(marker) => Some(marker.to_vec()),
        None => frontier.marker_bytes(&bank, tx::MARKER, None)?,
    };
    if let Some(marker) = &marker {
        let value = frontier.marker_value(marker)?;
        if get(&value, "operation_id").as_str() != Some(operation_id)
            || !same(field(&value, "project_id")?, field(&row, "project_id")?)
            || !value
                .get("hold_id")
                .is_some_and(|hold| same(hold, get(&row, "hold_id")))
            || !matches!(
                get(&value, "schema").as_str(),
                Some(writes::HOLD | writes::NESTED)
            )
        {
            return Err(err(
                "Recovery retirement cannot remove another operation marker",
            ));
        }
        bank_observation::markers(&bank, owner, Some(marker))?;
    }
    let sentinel = encoded(&object([
        ("schema", string(writes::SENTINEL)),
        ("project_id", field(&row, "project_id")?.clone()),
        ("operation_id", string(operation_id)),
    ]))?;
    let current_sentinel = frontier.marker_bytes(&bank, SENTINEL_FILE, None)?;
    if current_sentinel
        .as_ref()
        .is_some_and(|value| *value != sentinel)
    {
        return Err(err(
            "Recovery retirement found a changed companion sentinel",
        ));
    }
    let dependencies = match &retirement {
        Some(retirement) => field(retirement, "dependencies")?.clone(),
        None => {
            let mut rows = Vec::new();
            for (key, value) in operations(&indexed)?.as_object().unwrap_or_default() {
                if matches!(get(value, "state").as_str(), Some("active" | "rolled-back")) {
                    rows.push((
                        key.clone(),
                        object([
                            ("operation_id", string(key.as_str())),
                            (
                                "ledger_head_sha256",
                                field(value, "ledger_head_sha256")?.clone(),
                            ),
                        ]),
                    ));
                }
            }
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Array(rows.into_iter().map(|(_, row)| row).collect())
        }
    };
    let dependency_rows = items(&dependencies)?;
    if !dependency_rows
        .iter()
        .any(|item| get(item, "operation_id").as_str() == Some(operation_id))
    {
        return Err(err("Retirement omitted the exact holding operation"));
    }
    for item in dependency_rows {
        let dependent = text(item, "operation_id")
            .ok()
            .and_then(|id| operations(&indexed).ok()?.get(id));
        let changed = match dependent {
            None => true,
            Some(dependent) => {
                !same(field(dependent, "project_id")?, field(&row, "project_id")?)
                    || !same(
                        field(dependent, "ledger_head_sha256")?,
                        field(item, "ledger_head_sha256")?,
                    )
                    || !matches!(
                        text(dependent, "state")?,
                        "active" | "rolled-back" | "terminal"
                    )
            }
        };
        if changed {
            return Err(err(
                "Retirement dependency changed after durable terminal review",
            ));
        }
    }
    state::write_once(
        &directory.join("retirement.json"),
        &object([
            ("schema", string("agent-memory/layout-retirement-v1")),
            ("operation_id", string(operation_id)),
            ("ledger_head", head),
            ("receipt_sha256", string(sha(&receipt)?)),
            ("dependencies", dependencies.clone()),
            ("verified", Value::Bool(true)),
        ]),
    )?;
    for item in dependency_rows {
        let id = text(item, "operation_id")?;
        let updated = with(
            operation_row(&indexed, id)?.clone(),
            [("state", string("terminal"))],
        );
        set_operation_row(&mut indexed, id, updated);
    }
    publish_index(&root, &base, &indexed, runtime_root)?;
    writes::fault("layout-recovery-retirement-after-index")?;
    let project_id = text(&row, "project_id")?;
    for item in dependency_rows {
        let id = text(item, "operation_id")?;
        let (_, _, _, dependent_directory) = operation_paths(&root, id, runtime_root)?;
        let dependent_manifest = read_value(&dependent_directory.join("manifest.json"))?;
        let checkout = text(
            field(field(&dependent_manifest, "identity")?, "scope_context")?,
            "checkout_key",
        )?;
        state::bind(&root, id, project_id, checkout, "terminal", runtime_root)?;
        writes::fault("layout-recovery-retirement-after-binding")?;
    }
    writes::fault("layout-recovery-sentinel-retirement")?;
    if let Some(marker) = &marker {
        bank_observation::markers(&bank, owner, Some(marker))?;
        unlink_bank_entry(&bank, tx::MARKER)?;
        tx::flush_directory(&bank)?;
        writes::fault("layout-recovery-retirement-after-marker-removal")?;
    }
    if current_sentinel.is_some() {
        if frontier
            .marker_bytes(&bank, SENTINEL_FILE, None)?
            .as_deref()
            != Some(sentinel.as_slice())
        {
            return Err(err("Recovery sentinel changed before terminal retirement"));
        }
        unlink_bank_entry(&bank, SENTINEL_FILE)?;
        tx::flush_directory(&bank)?;
        writes::fault("layout-recovery-retirement-after-sentinel-removal")?;
    }
    frontier.invalidate(&root)?;
    Ok(object([
        ("operation_id", string(operation_id)),
        ("semantic_recovery_complete", Value::Bool(true)),
        ("pending_unit_ids", Value::Array(Vec::new())),
        ("safeguard_retired", Value::Bool(true)),
    ]))
}

/// The comparison evidence, proof and request a governed transaction carries.
pub fn transaction_evidence(p: &Value) -> Result<Option<(&Value, &Value, &Value)>> {
    for name in ["admission_evidence", "consolidation_evidence"] {
        if let Some(value) = p.get(name) {
            return Ok(Some((
                value,
                field(value, "comparison_proof")?,
                field(value, "comparison_request")?,
            )));
        }
    }
    if let Some(value) = p.get("governance_evidence") {
        let comparison = field(field(value, "request")?, "comparison")?;
        if !matches!(comparison, Value::Null) {
            return Ok(Some((
                value,
                field(comparison, "proof")?,
                field(comparison, "request")?,
            )));
        }
    }
    Ok(None)
}

fn pending_decisions(proof: &Value) -> Result<&[Value]> {
    items(field(field(proof, "judgment")?, "pending_decisions")?)
}
fn proof_frontier(proof: &Value) -> Result<&Value> {
    field(
        field(field(proof, "judgment")?, "observation")?,
        "recovery_frontier",
    )
}
fn coverage_claims(proof: &Value) -> Result<Vec<Value>> {
    pending_decisions(proof)?
        .iter()
        .map(|decision| field(decision, "coverage_claim").cloned())
        .collect()
}

/// Validate proof-bound unit coverage claims of a governed transaction.
pub fn claims_for(
    p: &Value,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<(Vec<Value>, Option<Value>)> {
    let evidence = transaction_evidence(p)?;
    let repo = PathBuf::from(text(p, "repo_root")?);
    let frontier = frontier()?;
    let token = frontier.observe_owned(&repo, owner, transaction)?;
    let Some((evidence, proof, request)) = evidence else {
        if !unit_ids(&token)?.is_empty() {
            return Err(err(
                "A mutation under recovery hold needs complete comparison-v2 frontier evidence",
            ));
        }
        return Ok((Vec::new(), None));
    };
    if get(proof, "schema").as_str() != Some("agent-memory/comparison-proof-v2") {
        return Err(err(
            "Unfinished comparison-v1 evidence cannot authorize a current governed write",
        ));
    }
    let claims = coverage_claims(proof)?;
    let recovery_frontier = proof_frontier(proof)?;
    if !evidence
        .get("unit_resolution_claims")
        .is_some_and(|value| same(value, &Value::Array(claims.clone())))
        || get(evidence, "comparison_proof_sha256").as_str() != Some(sha(proof)?.as_str())
        || !evidence
            .get("reference_map_sha256")
            .is_some_and(|value| same(value, get(recovery_frontier, "reference_map_sha256")))
        || recovery_frontier.get("reference_map_sha256").is_none()
    {
        return Err(err(
            "Transaction evidence must repeat the exact proof-bound unit claims and private reference map",
        ));
    }
    frontier.check(recovery_frontier, &repo, owner, transaction)?;
    let review =
        frontier.validate_comparison_review(&repo, proof, owner, transaction, runtime_root)?;
    if let Some(review) = &review {
        if get(review, "intended_output_sha256").as_str() != Some(sha(field(p, "files")?)?.as_str())
        {
            return Err(err(
                "Coverage reviewer did not inspect this exact complete intended canonical output",
            ));
        }
        let origin = field(field(field(request, "candidate")?, "metadata")?, "origin")?;
        if same(
            field(review, "reviewer_harness")?,
            field(origin, "harness")?,
        ) && same(
            field(review, "reviewer_session")?,
            field(origin, "session")?,
        ) {
            return Err(err(
                "Coverage review must come from a distinct actual reviewing agent/session",
            ));
        }
    }
    let files = field(p, "files")?;
    for claim in &claims {
        for reference in items(field(claim, "target_record_refs")?)? {
            let resolved;
            let reference = if text(reference, "path")?.starts_with("ref:") {
                resolved = frontier.resolve_reference(&repo, reference, runtime_root)?;
                &resolved
            } else {
                reference
            };
            check_target(
                files,
                reference,
                "Coverage target is absent or differs from the intended complete result",
                "Coverage target identity/version differs from its real resulting record",
            )?;
        }
    }
    Ok((claims, review))
}

fn check_target(files: &Value, reference: &Value, absent: &str, identity: &str) -> Result<()> {
    let body = files.get(text(reference, "path")?).and_then(Value::as_str);
    let Some(body) = body.filter(|body| {
        get(reference, "resulting_sha256").as_str() == Some(digest(body.as_bytes()).as_str())
    }) else {
        return Err(err(absent));
    };
    let (kind, metadata, _) = records::parse(body, &[])?;
    if kind != "standalone-record"
        || !same(field(&metadata, "id")?, field(reference, "id")?)
        || !same(field(&metadata, "version")?, field(reference, "version")?)
    {
        return Err(err(identity));
    }
    Ok(())
}

/// Planning fixes meaning/source; final review supplies prospective coverage.
fn output_proof_basis(proof: &Value) -> Result<Value> {
    let mut value = proof.clone();
    let judgment = value.get_mut("judgment").ok_or_else(|| err(MALFORMED))?;
    let observation = judgment
        .get_mut("observation")
        .ok_or_else(|| err(MALFORMED))?;
    let recovery_frontier = observation
        .get_mut("recovery_frontier")
        .ok_or_else(|| err(MALFORMED))?;
    set(recovery_frontier, "review_digest", Value::Null);
    let decisions = judgment
        .get_mut("pending_decisions")
        .ok_or_else(|| err(MALFORMED))?;
    let Value::Array(decisions) = decisions else {
        return Err(err(MALFORMED));
    };
    for decision in decisions {
        let Value::Object(pairs) = decision else {
            return Err(err(MALFORMED));
        };
        let before = pairs.len();
        pairs.retain(|(key, _)| key != "coverage_claim");
        if pairs.len() == before {
            return Err(err(MALFORMED));
        }
    }
    Ok(value)
}

/// Bind semantic planning input, not the later output-coverage attestation.
fn output_input_digest(value: &Value) -> Result<String> {
    if let Some(proof) = value.get("comparison_proof") {
        return sha(&with(
            value.clone(),
            [("comparison_proof", output_proof_basis(proof)?)],
        ));
    }
    if let Some(comparison) = value
        .get("comparison")
        .filter(|comparison| !matches!(comparison, Value::Null))
    {
        let basis = output_proof_basis(field(comparison, "proof")?)?;
        let comparison = with(comparison.clone(), [("proof", basis)]);
        return sha(&with(value.clone(), [("comparison", comparison)]));
    }
    sha(value)
}

fn output_evidence(p: &Value) -> Result<&Value> {
    p.get("consolidation_evidence")
        .or_else(|| p.get("governance_evidence"))
        .ok_or_else(|| err(MALFORMED))
}

/// Expose reproducible private output, never an apply-capable proposal.
pub fn make_output_plan(p: &Value, value: &Value) -> Result<Value> {
    let evidence = output_evidence(p)?;
    Ok(object([
        ("schema", string(OUTPUT_PLAN)),
        ("operation", field(p, "operation")?.clone()),
        ("repo_root", field(p, "repo_root")?.clone()),
        ("project_id", field(p, "project_id")?.clone()),
        ("operation_id", field(p, "operation_id")?.clone()),
        ("recorded_at", field(evidence, "recorded_at")?.clone()),
        ("request_input_sha256", string(output_input_digest(value)?)),
        ("observed_hashes", field(p, "observed_hashes")?.clone()),
        ("files", field(p, "files")?.clone()),
    ]))
}

/// A detached plan supplies stable IDs/time, not source or write authority.
pub fn output_plan_context(plan: &Value, value: &Value) -> Result<(Value, Value)> {
    let refused =
        || err("Output plan differs from the exact governed request and comparison input");
    if !has_keys(plan, OUTPUT_PLAN_FIELDS)
        || get(plan, "schema").as_str() != Some(OUTPUT_PLAN)
        || !uuid(get(plan, "operation_id"))
        || !uuid(get(plan, "project_id"))
        || get(plan, "operation").as_str().is_none()
        || get(plan, "repo_root").as_str().is_none()
        || get(plan, "observed_hashes").as_object().is_none()
        || get(plan, "files").as_object().is_none()
        || get(plan, "request_input_sha256").as_str() != Some(output_input_digest(value)?.as_str())
    {
        return Err(refused());
    }
    let at = lifecycle::time_value(get(plan, "recorded_at"))?;
    let now = lifecycle::time_value(&string(host::utc_now_isoformat()?))?;
    if at.0 > now.0 {
        return Err(err("Output plan cannot supply a future event time"));
    }
    Ok((
        get(plan, "operation_id").clone(),
        get(plan, "recorded_at").clone(),
    ))
}

/// Rebuilding must reproduce every planned byte against the same actual base.
pub fn check_output_plan(p: &Value, value: &Value, plan: &Value) -> Result<()> {
    output_plan_context(plan, value)?;
    let evidence = output_evidence(p)?;
    let expected = [
        ("operation", field(p, "operation")?),
        ("repo_root", field(p, "repo_root")?),
        ("project_id", field(p, "project_id")?),
        ("operation_id", field(p, "operation_id")?),
        ("recorded_at", field(evidence, "recorded_at")?),
        ("observed_hashes", field(p, "observed_hashes")?),
        ("files", field(p, "files")?),
    ];
    for (key, current) in expected {
        if encoded(field(plan, key)?)? != encoded(current)? {
            return Err(err(
                "Current governed output or source differs from the independently reviewed plan",
            ));
        }
    }
    Ok(())
}

pub fn evidence_fields(proof: Option<&Value>) -> Result<Value> {
    let Some(proof) = proof else {
        return Ok(object([
            ("unit_resolution_claims", Value::Array(Vec::new())),
            ("comparison_proof_sha256", Value::Null),
            (
                "reference_map_sha256",
                string(sha(&Value::Object(Vec::new()))?),
            ),
        ]));
    };
    Ok(object([
        (
            "unit_resolution_claims",
            Value::Array(coverage_claims(proof)?),
        ),
        ("comparison_proof_sha256", string(sha(proof)?)),
        (
            "reference_map_sha256",
            field(proof_frontier(proof)?, "reference_map_sha256")?.clone(),
        ),
    ]))
}

pub fn resolution_receipt(p: &Value) -> Result<Value> {
    Ok(object([
        (
            "schema",
            string("agent-memory/layout-resolution-receipt-v1"),
        ),
        ("resolution_id", field(p, "resolution_id")?.clone()),
        ("operation_id", field(p, "operation_id")?.clone()),
        ("unit_id", field(p, "unit_id")?.clone()),
        (
            "generation",
            field(field(p, "ledger_head")?, "generation")?.clone(),
        ),
        ("request_sha256", string(sha(p)?)),
        ("terminal_state", field(p, "terminal_state")?.clone()),
        (
            "canonical_operation_id",
            field(p, "canonical_operation_id")?.clone(),
        ),
        (
            "canonical_receipt_sha256",
            field(p, "canonical_receipt_sha256")?.clone(),
        ),
        ("claims", field(p, "claims")?.clone()),
        (
            "independent_review_sha256",
            field(p, "independent_review_sha256")?.clone(),
        ),
        ("verified", Value::Bool(true)),
    ]))
}

pub fn resolution_completion(
    resolution_id: &str,
    receipt: &Value,
    journal: &Value,
) -> Result<Value> {
    Ok(object([
        (
            "schema",
            string("agent-memory/layout-resolution-completion-v1"),
        ),
        ("resolution_id", string(resolution_id)),
        ("receipt_sha256", string(sha(receipt)?)),
        ("journal_sha256", string(sha(journal)?)),
        ("verified", Value::Bool(true)),
    ]))
}

/// `Path(name).stem`.
fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(position) if position > 0 => name.get(..position).unwrap_or(name),
        _ => name,
    }
}

/// Under the bank lock, reserve every unfinished journal for its exact retry.
pub fn require_completed_resolutions(
    repo: &Path,
    retry: Option<&Value>,
    runtime_root: &Path,
) -> Result<()> {
    let (root, _, base, _) = state::paths(repo, None, runtime_root)?;
    let root_text = path_text(&root)?;
    let indexed = state::index(&root, runtime_root)?;
    for (operation_id, row) in operations(&indexed)?.as_object().unwrap_or_default() {
        if get(row, "state").as_str() == Some("cleaned") {
            continue;
        }
        let directory = base.join(operation_id);
        let journals = directory.join("resolution-journals");
        if lstat(&journals)?.is_none() {
            continue;
        }
        for name in listing(&journals)? {
            let resolution_id = stem(&name);
            let journal = read_value(&journals.join(&name))?;
            let empty = Value::Object(Vec::new());
            let p = if journal.as_object().is_some() {
                journal.get("proposal").unwrap_or(&empty)
            } else {
                &empty
            };
            if name != format!("{resolution_id}.json")
                || !uuid(&string(resolution_id))
                || !has_keys(&journal, JOURNAL_FIELDS)
                || get(&journal, "schema").as_str() != Some(RESOLUTION_JOURNAL)
                || p.as_object().is_none()
                || get(p, "operation_id").as_str() != Some(operation_id.as_str())
                || get(p, "resolution_id").as_str() != Some(resolution_id)
                || get(p, "repo_root").as_str() != Some(root_text)
            {
                return Err(err(
                    "Unknown resolution journal prevents a new recovery action",
                ));
            }
            let receipt = read_optional(&directory.join("resolutions").join(&name))?;
            let completion = read_optional(&directory.join("resolution-completions").join(&name))?;
            if let Some(receipt) = &receipt {
                if !same(receipt, &resolution_receipt(p)?) {
                    return Err(err(
                        "Resolution journal lost its exact source-bound receipt",
                    ));
                }
            }
            if let Some(completion) = &completion {
                let exact = match &receipt {
                    Some(receipt) => same(
                        completion,
                        &resolution_completion(resolution_id, receipt, &journal)?,
                    ),
                    None => false,
                };
                if !exact {
                    return Err(err(
                        "Resolution completion differs from its exact journal and receipt",
                    ));
                }
            } else if !retry.is_some_and(|retry| same(retry, p)) {
                return Err(err(
                    "An unfinished resolution journal requires its exact proposal retry before new recovery actions",
                ));
            }
        }
    }
    Ok(())
}

pub fn resolution_units(directory: &Path, p: &Value, receipt: &Value) -> Result<Value> {
    let ledger_head = field(p, "ledger_head")?;
    let previous = read_value(
        &directory
            .join("ledger")
            .join(generation_name(field(ledger_head, "generation")?)?),
    )?;
    if get(ledger_head, "sha256").as_str() != Some(sha(&previous)?.as_str()) {
        return Err(err(
            "Resolution lost its exact immutable original ledger generation",
        ));
    }
    let mut units = items(field(&previous, "units")?)?.to_vec();
    let unit_id = field(p, "unit_id")?;
    let record = units
        .iter_mut()
        .find(|unit| unit.get("unit_id").is_some_and(|id| same(id, unit_id)))
        .ok_or_else(|| err("Resolution must name one actual unresolved source unit"))?;
    if get(record, "state").as_str() != Some("pending-admission") {
        return Err(err("Resolution cannot replace an already terminal unit"));
    }
    set(record, "state", field(p, "terminal_state")?.clone());
    let entry = object([
        ("resolution_id", field(p, "resolution_id")?.clone()),
        ("sha256", string(sha(receipt)?)),
    ]);
    match record.get_mut("terminal_receipts") {
        Some(Value::Array(receipts)) => receipts.push(entry),
        _ => return Err(err(MALFORMED)),
    }
    Ok(Value::Array(units))
}

/// A durable receipt resumes linkage/retirement; completion alone is historical.
pub fn resolve(
    repo: &Path,
    operation_id: Option<&str>,
    unit_id: Option<&str>,
    terminal: Option<&Value>,
    proposal: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    let Some(proposal) = proposal else {
        let resolution_id = host::uuid4()?;
        let operation_id = operation_id
            .ok_or_else(|| err("Recovery operations require canonical UUIDv4 identifiers"))?;
        let (root, bank, _, _) = operation_paths(repo, operation_id, runtime_root)?;
        let preview = locked(&root, &bank, &resolution_id, |owner| {
            resolution_preview(
                &root,
                operation_id,
                unit_id,
                terminal,
                &resolution_id,
                Some(owner),
                runtime_root,
            )
        })?;
        return Ok(object([
            ("proposal", preview),
            ("canonical_writes_performed", Value::Bool(false)),
            ("writes_performed", Value::Bool(false)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
        ]));
    };
    let p = proposal.get("proposal").unwrap_or(proposal);
    let operation_id = text(p, "operation_id")?;
    let (root, bank, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    if get(p, "schema").as_str() != Some(RESOLUTION_PROPOSAL)
        || text(p, "repo_root")? != path_text(&root)?
        || !uuid(field(p, "resolution_id")?)
    {
        return Err(err(
            "Use the exact private source-bound resolution proposal",
        ));
    }
    let resolution_id = text(p, "resolution_id")?;
    let file = format!("{resolution_id}.json");
    let receipt = resolution_receipt(p)?;
    let known = read_optional(&directory.join("resolutions").join(&file))?;
    if known.as_ref().is_some_and(|known| !same(known, &receipt)) {
        return Err(err(
            "Resolution ID was reused with different evidence or receipt bytes",
        ));
    }
    let completion_path = directory.join("resolution-completions").join(&file);
    let completed = read_optional(&completion_path)?;
    let journal_path = directory.join("resolution-journals").join(&file);
    let journal = read_optional(&journal_path)?;
    let project_id = text(field(p, "recovery_frontier")?, "project_id")?;
    if let Some(completed) = completed {
        let exact = match (&known, &journal) {
            (Some(_), Some(journal)) => {
                journal.get("proposal").is_some_and(|value| same(value, p))
                    && same(
                        &completed,
                        &resolution_completion(resolution_id, &receipt, journal)?,
                    )
            }
            _ => false,
        };
        let (Some(known), Some(journal), true) = (known, journal, exact) else {
            return Err(err(
                "Historical resolution lost its exact completion/linkage evidence",
            ));
        };
        layout_history::check(
            &root,
            field(&journal, "scope_context")?,
            project_id,
            runtime_root,
        )?;
        return Ok(object([
            ("receipt", known),
            ("already_applied", Value::Bool(true)),
            ("receipt_is_historical", Value::Bool(true)),
            ("writes_performed", Value::Bool(false)),
        ]));
    }
    locked(&root, &bank, resolution_id, |owner| {
        let owner = Some(owner);
        require_completed_resolutions(&root, Some(p), runtime_root)?;
        let units = resolution_units(&directory, p, &receipt)?;
        let frontier = frontier()?;
        let mut journal = journal;
        if known.is_none() {
            let fresh = resolution_preview(
                &root,
                operation_id,
                get(p, "unit_id").as_str(),
                p.get("terminal"),
                resolution_id,
                owner,
                runtime_root,
            )?;
            if !same(&fresh, p) {
                return Err(err("Resolution evidence changed after preview"));
            }
            let held =
                require_hold(frontier.verified_hold_value(&root, owner, None, runtime_root)?)?;
            let expected_journal = object([
                ("schema", string(RESOLUTION_JOURNAL)),
                ("proposal", p.clone()),
                ("units", units.clone()),
                ("hold", hold_marker(&held)?),
                ("scope_context", held.scope().clone()),
            ]);
            state::write_once(&journal_path, &expected_journal)?;
            journal = Some(expected_journal);
            state::write_once(&directory.join("resolutions").join(&file), &receipt)?;
            writes::fault("layout-recovery-resolution-after-receipt")?;
        }
        let journal = journal
            .filter(|journal| {
                has_keys(journal, JOURNAL_FIELDS)
                    && get(journal, "schema").as_str() == Some(RESOLUTION_JOURNAL)
                    && same(get(journal, "proposal"), p)
                    && same(get(journal, "units"), &units)
            })
            .ok_or_else(|| {
                err("Resolution retry requires its exact durable source-bound journal")
            })?;
        layout_history::check(
            &root,
            field(&journal, "scope_context")?,
            project_id,
            runtime_root,
        )?;
        let marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
        let hold = field(&journal, "hold")?;
        let hold_operation = text(hold, "operation_id")?;
        let (_, _, _, hold_directory) = operation_paths(&root, hold_operation, runtime_root)?;
        let retirement = read_optional(&hold_directory.join("retirement.json"))?;
        let hold_bytes = encoded(hold)?;
        if marker.as_deref() != Some(hold_bytes.as_slice())
            && !(marker.is_none() && retirement.is_some())
        {
            return Err(err(
                "Standalone resolution cannot bypass a nested, foreign or missing hold",
            ));
        }
        bank_observation::markers(&bank, owner, marker.as_deref())?;
        let indexed = state::index(&root, runtime_root)?;
        let observed = read_value(&directory.join("ledger-head.json"))?;
        let ledger_head = field(p, "ledger_head")?;
        let base_generation =
            int(field(ledger_head, "generation")?).ok_or_else(|| err(MALFORMED))?;
        let observed_generation =
            int(field(&observed, "generation")?).ok_or_else(|| err(MALFORMED))?;
        let successor_head = if observed_generation <= base_generation + 1 {
            let successor = advance_ledger(
                &root,
                operation_id,
                &units,
                ledger_head,
                owner,
                marker.as_deref(),
                None,
                runtime_root,
            )?;
            field(&successor, "head")?.clone()
        } else {
            let expected = read_value(
                &directory
                    .join("ledger")
                    .join(generation_name(&number(base_generation + 1))?),
            )?;
            if !same(field(&expected, "units")?, &units)
                || !same(
                    field(&expected, "previous_generation_sha256")?,
                    field(ledger_head, "sha256")?,
                )
            {
                return Err(err(
                    "Historical resolution linkage differs from its exact successor",
                ));
            }
            frontier.ledger(
                &root,
                operation_id,
                operation_row(&indexed, operation_id)?,
                &bank,
                runtime_root,
            )?;
            observed
        };
        let hold_state = state::index(&root, runtime_root)?;
        if text(operation_row(&hold_state, hold_operation)?, "state")? == "terminal" {
            retire(&root, hold_operation, owner, None, runtime_root)?;
        }
        let token = frontier.observe_owned(&root, owner, None)?;
        let complete = get(&token, "coverage_complete").as_bool() == Some(true);
        if complete && frontier.marker_bytes(&bank, tx::MARKER, None)?.is_some() {
            let held = frontier.verified_hold_value(&root, owner, None, runtime_root)?;
            if held
                .as_ref()
                .is_none_or(|held| held.operation_id() != hold_operation)
            {
                return Err(err(
                    "Terminal resolution lost its exact same-bank recovery hold",
                ));
            }
            retire(&root, hold_operation, owner, None, runtime_root)?;
        }
        state::write_once(
            &completion_path,
            &resolution_completion(resolution_id, &receipt, &journal)?,
        )?;
        Ok(object([
            ("receipt", receipt.clone()),
            ("ledger_head", successor_head),
            ("writes_performed", Value::Bool(true)),
            ("already_applied", Value::Bool(known.is_some())),
            ("semantic_recovery_complete", Value::Bool(complete)),
        ]))
    })
}

#[allow(clippy::too_many_arguments)]
fn resolution_preview(
    repo: &Path,
    operation_id: &str,
    unit_id: Option<&str>,
    terminal: Option<&Value>,
    resolution_id: &str,
    owner: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let journal = read_optional(
        &directory
            .join("resolution-journals")
            .join(format!("{resolution_id}.json")),
    )?;
    let retry = match &journal {
        Some(journal) => Some(field(journal, "proposal")?),
        None => None,
    };
    require_completed_resolutions(&root, retry, runtime_root)?;
    let frontier = frontier()?;
    let held = frontier
        .verified_hold_value(&root, owner, None, runtime_root)?
        .ok_or_else(|| {
            err("Resolution needs the exact persistent hold, never an interrupted nested marker")
        })?;
    let indexed = state::index(&root, runtime_root)?;
    let row = operations(&indexed)?.get(operation_id);
    let row = row
        .filter(|row| {
            same(get(row, "project_id"), get(held.token(), "project_id"))
                && (operation_id == held.operation_id()
                    || get(row, "state").as_str() == Some("rolled-back"))
        })
        .ok_or_else(|| {
            err("Resolution may name only the active or explicitly retained rolled-back same-bank ledger")
        })?;
    let (_, head, current) = frontier.ledger(&root, operation_id, row, &bank, runtime_root)?;
    let unit = items(field(&current, "units")?)?
        .iter()
        .find(|unit| unit_id.is_some_and(|id| get(unit, "unit_id").as_str() == Some(id)))
        .filter(|unit| get(unit, "state").as_str() == Some("pending-admission"))
        .ok_or_else(|| err("Resolution must name one actual unresolved source unit"))?;
    let unit_id = text(unit, "unit_id")?;
    let Some(terminal) = terminal.filter(|terminal| terminal.as_object().is_some()) else {
        return Err(err(
            "Terminal resolution requires actual committed coverage or independently reviewed non-guidance authority",
        ));
    };
    let files = tx::files_at(&bank)?;
    let terminal_keys = keys(terminal);
    let selected = if terminal_keys == BTreeSet::from(["discard_authority"]) {
        discard_selection(
            field(terminal, "discard_authority")?,
            operation_id,
            unit_id,
            unit,
            &head,
            &current,
            &directory,
        )?
    } else if terminal_keys == BTreeSet::from(["transaction_proposals", "coverage_review"]) {
        accumulated_coverage(
            &root,
            operation_id,
            unit,
            &head,
            &current,
            terminal,
            &files,
            runtime_root,
        )?
    } else if terminal_keys == BTreeSet::from(["transaction_proposal"]) {
        let p = tx::proposal_value(field(terminal, "transaction_proposal")?)?;
        let (claims, review) = claims_for(p, owner, None, runtime_root)?;
        let full: Vec<Value> = claims
            .into_iter()
            .filter(|claim| {
                get(claim, "unit_id").as_str() == Some(unit_id)
                    && get(claim, "coverage").as_str() == Some("full")
            })
            .collect();
        let Some(review) = review.filter(|_| full.len() == 1) else {
            return Err(err(
                "Only full independently reviewed committed coverage resolves the whole original unit",
            ));
        };
        let (_, receipt_path) = tx::locations(&root, &bank, text(p, "operation_id")?)?;
        let receipt = tx::receipt_value(&receipt_path, p)?;
        let receipt = match receipt {
            Some(receipt)
                if get(&receipt, "outcome").as_str() == Some("finished")
                    && same(
                        &writes::snapshot(&root, owner)?.1,
                        get(&receipt, "resulting_hashes"),
                    ) =>
            {
                receipt
            }
            _ => {
                return Err(err(
                    "A proposed, interrupted, changed or partial canonical write cannot resolve pending knowledge",
                ));
            }
        };
        let state = if p.get("admission_evidence").is_some() {
            "admitted"
        } else {
            "governed-terminal"
        };
        vec![
            ("terminal_state", string(state)),
            ("canonical_operation_id", field(p, "operation_id")?.clone()),
            ("canonical_receipt_sha256", string(sha(&receipt)?)),
            ("claims", Value::Array(full)),
            ("independent_review_sha256", string(sha(&review)?)),
        ]
    } else {
        return Err(err("Unknown terminal resolution evidence"));
    };
    let remaining: Vec<Value> = frontier
        .pending_rows(&root, held.token(), owner, None, runtime_root)?
        .into_iter()
        .filter(|row| get(row, "unit_id").as_str() != Some(unit_id))
        .collect();
    let remaining_ids: Vec<String> = remaining
        .iter()
        .map(|row| text(row, "unit_id").map(str::to_owned))
        .collect::<Result<_>>()?;
    let successor = frontier.successor_token(held.token(), &remaining_ids, operation_id, true)?;
    frontier.reservation(&files, &remaining, &successor)?;
    let mut proposal = vec![
        ("schema", string(RESOLUTION_PROPOSAL)),
        ("resolution_id", string(resolution_id)),
        ("operation_id", string(operation_id)),
        ("unit_id", string(unit_id)),
        ("repo_root", string(path_text(&root)?)),
        ("ledger_head", head),
        ("recovery_frontier", held.token().clone()),
        ("terminal", terminal.clone()),
    ];
    proposal.extend(selected);
    Ok(object(proposal))
}

const DISCARD_FIELDS: &[&str] = &[
    "schema",
    "review_id",
    "operation_id",
    "unit_id",
    "manifest_sha256",
    "unit_map_sha256",
    "ledger_head_sha256",
    "source_ranges",
    "author_harness",
    "author_session",
    "reviewer_developer_id",
    "reviewer_harness",
    "reviewer_session",
    "review_time",
    "non_guidance",
    "reason",
    "decision",
];

fn discard_selection(
    authority: &Value,
    operation_id: &str,
    unit_id: &str,
    unit: &Value,
    head: &Value,
    current: &Value,
    directory: &Path,
) -> Result<Vec<(&'static str, Value)>> {
    let pair = |a: &str, b: &str| (get(authority, a).clone(), get(authority, b).clone());
    let author = pair("author_harness", "author_session");
    let reviewer = pair("reviewer_harness", "reviewer_session");
    if !has_keys(authority, DISCARD_FIELDS)
        || get(authority, "schema").as_str() != Some("agent-memory/layout-nonguidance-discard-v1")
        || !uuid(get(authority, "review_id"))
        || !uuid(get(authority, "reviewer_developer_id"))
        || get(authority, "operation_id").as_str() != Some(operation_id)
        || get(authority, "unit_id").as_str() != Some(unit_id)
        || !same(
            get(authority, "manifest_sha256"),
            field(current, "manifest_sha256")?,
        )
        || !same(
            get(authority, "unit_map_sha256"),
            field(current, "unit_map_sha256")?,
        )
        || !same(get(authority, "ledger_head_sha256"), field(head, "sha256")?)
        || !same(
            get(authority, "source_ranges"),
            field(unit, "source_ranges")?,
        )
        || get(authority, "non_guidance").as_bool() != Some(true)
        || get(authority, "decision").as_str() != Some("discard")
        || (same(&author.0, &reviewer.0) && same(&author.1, &reviewer.1))
    {
        return Err(err(
            "Non-guidance discard needs exact full-source independent authority, not a backup or elapsed time",
        ));
    }
    safe_text(get(authority, "reason"), 4096)?;
    lifecycle::time_value(get(authority, "review_time"))?;
    for key in [
        "author_harness",
        "author_session",
        "reviewer_harness",
        "reviewer_session",
    ] {
        safe_text(get(authority, key), 256)?;
    }
    state::write_once(
        &directory
            .join("discard-reviews")
            .join(format!("{}.json", text(authority, "review_id")?)),
        authority,
    )?;
    Ok(vec![
        ("terminal_state", string("reviewed-non-guidance-discard")),
        ("canonical_operation_id", Value::Null),
        ("canonical_receipt_sha256", Value::Null),
        ("claims", Value::Array(Vec::new())),
        ("independent_review_sha256", string(sha(authority)?)),
    ])
}

const COVERAGE_REVIEW_FIELDS: &[&str] = &[
    "schema",
    "review_id",
    "operation_id",
    "reviewer_developer_id",
    "reviewer_harness",
    "reviewer_session",
    "review_time",
    "manifest_sha256",
    "unit_map_sha256",
    "ledger_head_sha256",
    "comparison_proof_input_sha256",
    "intended_output_sha256",
    "claims",
    "rationale",
    "decision",
];

/// A fresh independent review credits the complete union of actual receipts.
#[allow(clippy::too_many_arguments)]
pub fn accumulated_coverage(
    repo: &Path,
    operation_id: &str,
    unit: &Value,
    head: &Value,
    ledger: &Value,
    terminal: &Value,
    files: &Value,
    runtime_root: &Path,
) -> Result<Vec<(&'static str, Value)>> {
    let proposals = field(terminal, "transaction_proposals")?;
    let review = field(terminal, "coverage_review")?;
    let proposals = proposals
        .as_array()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| {
            err("Accumulated coverage must retain every actual committed transaction")
        })?;
    let frontier = frontier()?;
    let resolved_repo = crate::fs::canonical_path(repo, false)?;
    let repo_text = path_text(&resolved_repo)?.to_owned();
    let mut claims = Vec::new();
    let mut receipts = Vec::new();
    let mut origins = Vec::new();
    let mut operation_ids = BTreeSet::new();
    let mut all_admission = true;
    for value in proposals {
        let p = tx::proposal_value(value)?;
        let canonical_operation = text(p, "operation_id")?.to_owned();
        if operation_ids.contains(&canonical_operation)
            || get(p, "repo_root").as_str() != Some(repo_text.as_str())
        {
            return Err(err(
                "Accumulated coverage cannot duplicate or transfer committed evidence",
            ));
        }
        operation_ids.insert(canonical_operation.clone());
        all_admission &= value_proposal_has_admission(value);
        let Some((evidence, proof, request)) = transaction_evidence(p)?.filter(|(_, proof, _)| {
            get(proof, "schema").as_str() == Some("agent-memory/comparison-proof-v2")
        }) else {
            return Err(err(
                "Accumulated resolution requires actual comparison-v2 proof-bound fragments",
            ));
        };
        let expected = evidence_fields(Some(proof))?;
        for (key, item) in expected.as_object().unwrap_or_default() {
            if !evidence.get(key).is_some_and(|value| same(value, item)) {
                return Err(err(
                    "A committed transaction does not bind its claimed source coverage",
                ));
            }
        }
        let (root, bank) = writes::bank_path(repo)?;
        let (_, receipt_path) = tx::locations(&root, &bank, &canonical_operation)?;
        let receipt = tx::receipt_value(&receipt_path, p)?
            .filter(|receipt| get(receipt, "outcome").as_str() == Some("finished"))
            .ok_or_else(|| {
                err("A partial/proposed canonical transaction cannot enter terminal coverage")
            })?;
        receipts.push(object([
            ("operation_id", string(canonical_operation.as_str())),
            ("receipt_sha256", string(sha(&receipt)?)),
            ("proof_sha256", string(sha(proof)?)),
        ]));
        origins.push(field(field(field(request, "candidate")?, "metadata")?, "origin")?.clone());
        for decision in pending_decisions(proof)? {
            let claim = field(decision, "coverage_claim")?;
            if !same(field(decision, "unit_id")?, field(unit, "unit_id")?)
                || get(claim, "coverage").as_str() == Some("none")
            {
                continue;
            }
            frontier.claim_value(claim, unit)?;
            for reference in items(field(claim, "target_record_refs")?)? {
                let resolved;
                let reference = if text(reference, "path")?.starts_with("ref:") {
                    resolved = frontier.resolve_reference(repo, reference, runtime_root)?;
                    &resolved
                } else {
                    reference
                };
                check_target(
                    files,
                    reference,
                    "Accumulated coverage target changed, was replaced or was removed",
                    "Accumulated target reference no longer names its actual record",
                )?;
            }
            claims.push(claim.clone());
        }
    }
    let mut unique: Vec<(Vec<u8>, Value)> = Vec::new();
    for claim in &claims {
        for range in items(field(claim, "covered_source_ranges")?)? {
            let key = encoded(range)?;
            if let Some(slot) = unique.iter_mut().find(|(existing, _)| *existing == key) {
                slot.1 = range.clone();
            } else {
                unique.push((key, range.clone()));
            }
        }
    }
    let ranges: Vec<Value> = unique.into_iter().map(|(_, range)| range).collect();
    let mut merged: Vec<Value> = Vec::new();
    for value in sorted_ranges(&ranges)? {
        let (source, start, end) = range_key(&value)?;
        if let Some(last) = merged.last_mut() {
            let (last_source, _, last_end) = range_key(last)?;
            if last_source == source {
                if last_end > start {
                    return Err(err(
                        "Accumulated coverage cannot count overlapping excerpts twice",
                    ));
                }
                if last_end == start {
                    set(last, "end", number(end));
                    continue;
                }
            }
        }
        merged.push(value);
    }
    let expected = sorted_ranges(items(field(unit, "source_ranges")?)?)?;
    if !same(&Value::Array(merged), &Value::Array(expected)) {
        return Err(err(
            "Accumulated coverage omits original conditions, framing or semantic source ranges",
        ));
    }
    let reviewer = (
        get(review, "reviewer_harness").clone(),
        get(review, "reviewer_session").clone(),
    );
    let same_origin = origins.iter().any(|origin| {
        same(&reviewer.0, get(origin, "harness")) && same(&reviewer.1, get(origin, "session"))
    });
    let receipts = Value::Array(receipts);
    if !has_keys(review, COVERAGE_REVIEW_FIELDS)
        || get(review, "schema").as_str() != Some("agent-memory/layout-coverage-review-v1")
        || !uuid(get(review, "review_id"))
        || !uuid(get(review, "reviewer_developer_id"))
        || get(review, "operation_id").as_str() != Some(operation_id)
        || !same(
            get(review, "manifest_sha256"),
            field(ledger, "manifest_sha256")?,
        )
        || !same(
            get(review, "unit_map_sha256"),
            field(ledger, "unit_map_sha256")?,
        )
        || !same(get(review, "ledger_head_sha256"), field(head, "sha256")?)
        || get(review, "comparison_proof_input_sha256").as_str() != Some(sha(&receipts)?.as_str())
        || get(review, "intended_output_sha256").as_str() != Some(sha(files)?.as_str())
        || !same(get(review, "claims"), &Value::Array(claims.clone()))
        || get(review, "decision").as_str() != Some("approve")
        || same_origin
    {
        return Err(err(
            "Terminal union requires a distinct fresh independent review of every receipt and current target",
        ));
    }
    safe_text(get(review, "rationale"), 4096)?;
    lifecycle::time_value(get(review, "review_time"))?;
    let (_, _, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    state::write_once(
        &directory
            .join("union-reviews")
            .join(format!("{}.json", text(review, "review_id")?)),
        review,
    )?;
    let terminal_state = if all_admission {
        "admitted"
    } else {
        "governed-terminal"
    };
    Ok(vec![
        ("terminal_state", string(terminal_state)),
        (
            "canonical_operation_id",
            Value::Array(operation_ids.into_iter().map(string).collect()),
        ),
        ("canonical_receipt_sha256", string(sha(&receipts)?)),
        ("claims", Value::Array(claims)),
        ("independent_review_sha256", string(sha(review)?)),
    ])
}

/// Baseline `'admission_evidence' in value` over the supplied (possibly wrapped) proposal.
fn value_proposal_has_admission(value: &Value) -> bool {
    value.get("admission_evidence").is_some()
}
