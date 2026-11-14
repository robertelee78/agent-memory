//! Independently pinned private recovery coverage. Pending text is comparison-only.
use crate::bank_writes as writes;
use crate::codec::json::Value;
use crate::developer_identity::{digest, encoded, load_with_limit, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::DirectoryChain;
use crate::lifecycle::{get, object, string};
use crate::{bank_contract, bank_observation, layout_recovery_state as state, local_registry as storage,
    project_identity, scope_binding, sharing_limits};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub const HOLD: &str = "agent-memory/layout-frontier-hold-v1";
pub const NESTED: &str = "agent-memory/layout-frontier-transaction-v1";
pub const TRANSACTION: &str = "agent-memory/layout-recovery-transaction-v1";
pub const SENTINEL: &str = "agent-memory/layout-recovery-sentinel-v1";
pub const TERMINAL: &[&str] = &["admitted", "governed-terminal", "reviewed-non-guidance-discard"];
pub const UNIT_FIELDS: &[&str] = &["unit_id", "source_ranges", "provenance", "reason", "required_actions",
    "detecting_developer_id", "state", "terminal_receipts"];
const MARKER: &str = ".agent-memory-transaction.md";
const RECOVERY: &str = ".agent-memory-recovery.md";
const SOURCE_BOUND: usize = 1_048_576;
const CHANGED: &str = "Private recovery frontier changed; reread every pending source before proceeding";
pub type VerifiedHold = scope_binding::VerifiedEnrollmentHold;

fn err(message: &str) -> ContractError { ContractError::new(message) }
fn io(error: impl std::fmt::Display) -> ContractError { ContractError::new(error.to_string()) }
fn text(path: &Path) -> Result<&str> { crate::host::text(path.as_os_str(), "recovery frontier path") }
fn fields(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|rows| rows.len() == names.len()
        && rows.iter().all(|(name, _)| names.contains(&name.as_str())))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Int(value) => value.as_str() != "0",
        Value::Float(value) => *value != 0.0,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
fn array(value: &Value) -> Result<&[Value]> {
    value.as_array().ok_or_else(|| err("Recovery evidence requires a bounded array"))
}
fn pairs(value: &Value) -> Result<&[(String, Value)]> {
    value.as_object().ok_or_else(|| err("Recovery evidence requires an object"))
}
fn s(value: &Value) -> Result<&str> { value.as_str().ok_or_else(|| err("Recovery evidence requires text")) }
fn integer(value: &Value) -> Option<i64> { value.as_int().and_then(|value| value.as_i64()) }
fn n(value: &Value) -> Result<i64> { integer(value).ok_or_else(|| err("Recovery evidence requires an integer")) }
fn hash(value: &Value) -> Result<Value> { Ok(nullable(digest(Some(&encoded(value)?)))) }
fn nullable(value: Option<String>) -> Value { value.map(string).unwrap_or(Value::Null) }
fn same(a: &Value, b: &Value) -> Result<bool> { Ok(encoded(a)? == encoded(b)?) }
fn put(value: &mut Value, key: &str, replacement: Value) -> Result<()> {
    let Value::Object(rows) = value else { return Err(err("Recovery evidence requires an object")); };
    if let Some((_, old)) = rows.iter_mut().find(|(name, _)| name == key) { *old = replacement; }
    else { rows.push((key.to_owned(), replacement)); }
    Ok(())
}
fn member(value: &Value, allowed: &[&str]) -> bool { value.as_str().is_some_and(|v| allowed.contains(&v)) }
fn read(path: &Path) -> Result<Value> {
    state::read(path, sharing_limits::MAX_PRIVATE, false)?.ok_or_else(|| err("Recovery evidence is missing"))
}
fn read_optional(path: &Path) -> Result<Option<Value>> { state::read(path, sharing_limits::MAX_PRIVATE, true) }
fn raw(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    state::read_bytes(path, maximum, false)?.ok_or_else(|| err("Recovery evidence is missing"))
}
fn operation_directory(root: &Path, bank: &Path, operation: &str, runtime: &Path) -> Result<PathBuf> {
    state::_paths_for_bank(root, bank, Some(operation), runtime)?.3
        .ok_or_else(|| err("Recovery operation directory is missing"))
}
fn sorted_rows(value: &Value) -> Result<Vec<(&str, &Value)>> {
    let mut rows: Vec<_> = pairs(value)?.iter().map(|(id, row)| (id.as_str(), row)).collect();
    rows.sort_unstable_by_key(|(id, _)| *id);
    Ok(rows)
}

thread_local! { static SOURCE_MARKERS: Cell<bool> = const { Cell::new(false) }; }
pub struct SourceMarkerGuard { previous: Option<bool>, thread: PhantomData<Rc<()>> }
impl SourceMarkerGuard {
    fn restore(&mut self) {
        if let Some(previous) = self.previous.take() { SOURCE_MARKERS.with(|flag| flag.set(previous)); }
    }
}
impl Drop for SourceMarkerGuard { fn drop(&mut self) { self.restore(); } }
impl scope_binding::SourceMarkerObservation for SourceMarkerGuard {
    fn finish(&mut self) -> Result<()> { self.restore(); Ok(()) }
}
pub fn observe_source_markers() -> SourceMarkerGuard {
    SourceMarkerGuard { previous: Some(SOURCE_MARKERS.with(|flag| flag.replace(true))), thread: PhantomData }
}
fn writer(bank: &Path) -> Result<()> {
    storage::check_canonical_writer(&bank.join(".agent-memory.lock"), writes::OPERATIONS)
        .map_err(|error| match error {
            storage::CanonicalWriterError::Busy => ContractError::canonical_writer_busy(),
            storage::CanonicalWriterError::Initializing => ContractError::canonical_writer_initializing(),
            storage::CanonicalWriterError::Contract(error) => error,
        })
}
pub fn marker_bytes(bank: &Path, name: &str, expected: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
    if !SOURCE_MARKERS.with(Cell::get) { return read_marker(bank, name, expected); }
    writer(bank)?;
    match read_marker(bank, name, expected) {
        Ok(value) => { writer(bank)?; Ok(value) }
        Err(original) => {
            match writer(bank) {
                Err(error) if !matches!(error.kind(), crate::error::Kind::CanonicalWriterInitializing) => return Err(error),
                _ => {}
            }
            Err(original)
        }
    }
}
fn read_marker(bank: &Path, name: &str, expected: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
    if expected.is_some_and(|raw| name != MARKER || raw.len() > sharing_limits::MAX_PRIVATE) {
        return Err(err("Invalid exact owned transaction marker witness"));
    }
    if !matches!(name, MARKER | RECOVERY) { return Err(err("Unknown or unsafe recovery marker; retain it for reviewed recovery")); }
    let limit = expected.map_or(4096, <[u8]>::len);
    let chain = DirectoryChain::open(bank).map_err(io)?;
    let directory = chain.directory();
    let before = match directory.stat_at(OsStr::new(name)) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => { chain.recheck().map_err(io)?; return Ok(None); }
        Err(error) => return Err(io(error)),
    };
    if !before.is_regular() || before.nlink != 1 || before.size < 0 || before.size as u128 > limit as u128 {
        return Err(err("Unknown or unsafe recovery marker; retain it for reviewed recovery"));
    }
    let mut file = directory.open_at(OsStr::new(name), libc::O_RDONLY | libc::O_NONBLOCK, 0).map_err(io)?;
    let opened = file.stat().map_err(io)?;
    let bytes = file.read_prefix(limit.checked_add(1).ok_or_else(|| err("Invalid marker bound"))?).map_err(io)?;
    let after = file.stat().map_err(io)?;
    let named = directory.stat_at(OsStr::new(name)).map_err(io)?;
    chain.recheck().map_err(|_| err("Recovery marker or bank binding changed during observation"))?;
    if bytes.len() > limit || expected.is_some_and(|expected| expected != bytes)
        || before != opened || opened != after || after != named {
        return Err(err("Recovery marker or bank binding changed during observation"));
    }
    Ok(Some(bytes))
}
pub fn marker_value(raw: &[u8]) -> Result<Value> {
    let value = load_with_limit(raw, sharing_limits::MAX_PRIVATE)?;
    if value.as_object().is_none() { return Err(err("Unknown recovery marker")); }
    let schema = get(&value, "schema");
    let names: &[&str] = match schema.as_str() {
        Some(HOLD) => &["schema", "project_id", "operation_id", "hold_id"],
        Some(NESTED) => &["schema", "project_id", "operation_id", "hold_id", "nested_id"],
        Some(TRANSACTION) => &["schema", "project_id", "operation_id", "transaction_kind"],
        _ => return Err(err("Bank is busy or interrupted; only a verified persistent recovery hold permits ordinary use")),
    };
    if !fields(&value, names) || names.iter().filter(|name| !matches!(**name, "schema" | "transaction_kind"))
        .any(|name| !uuid_valid(get(&value, name)))
        || (schema.as_str() == Some(TRANSACTION) && !member(get(&value, "transaction_kind"), &["apply", "finish", "rollback"]))
        || encoded(&value)? != raw {
        return Err(err("Unknown or malformed typed recovery marker"));
    }
    Ok(value)
}
pub fn range_value(value: &Value) -> Result<&Value> {
    if !fields(value, &["source_id", "start", "end"]) || !uuid_valid(get(value, "source_id"))
        || !integer(get(value, "start")).zip(integer(get(value, "end")))
            .is_some_and(|(start, end)| 0 <= start && start < end && end <= SOURCE_BOUND as i64) {
        return Err(err("Coverage requires exact bounded original source ranges"));
    }
    Ok(value)
}
pub fn ledger(repo: &Path, operation: &str, row: &Value, runtime: &Path) -> Result<(PathBuf, Value, Value)> {
    let (root, bank, _, _) = state::paths(repo, None, runtime)?;
    ledger_at_bank(&root, operation, row, &bank, runtime)
}
pub fn ledger_at_bank(root: &Path, operation: &str, row: &Value, bank: &Path, runtime: &Path) -> Result<(PathBuf, Value, Value)> {
    let directory = operation_directory(root, bank, operation, runtime)?;
    let head = state::read(&directory.join("ledger-head.json"), 4096, false)?.ok_or_else(|| err("Recovery ledger head is missing, changed or inconsistent with its index"))?;
    if !fields(&head, &["schema", "operation_id", "generation", "sha256"])
        || get(&head, "schema").as_str() != Some("agent-memory/layout-ledger-head-v1")
        || get(&head, "operation_id").as_str() != Some(operation)
        || !integer(get(&head, "generation")).is_some_and(|v| (0..=99_999_999).contains(&v))
        || !state::sha(get(&head, "sha256")) || hash(&head)? != *get(row, "ledger_head_sha256") {
        return Err(err("Recovery ledger head is missing, changed or inconsistent with its index"));
    }
    let generation = n(get(&head, "generation"))?;
    let current = read(&directory.join("ledger").join(format!("{generation:08}.json")))?;
    if !fields(&current, &["schema", "operation_id", "project_id", "manifest_sha256", "unit_map_sha256", "proposal_sha256", "generation", "previous_generation_sha256", "units"])
        || get(&current, "schema").as_str() != Some("agent-memory/layout-pending-ledger-v1")
        || get(&current, "operation_id").as_str() != Some(operation)
        || get(&current, "project_id") != get(row, "project_id")
        || get(&current, "generation") != get(&head, "generation")
        || hash(&current)? != *get(&head, "sha256") || get(&current, "units").as_array().is_none() {
        return Err(err("Recovery ledger generation is incomplete or corrupt"));
    }
    let mut expected_previous = get(&current, "previous_generation_sha256").clone();
    if generation == 0 && expected_previous != Value::Null { return Err(err("Initial recovery ledger cannot replace prior history")); }
    let mut remaining = sharing_limits::MAX_PRIVATE.checked_sub(encoded(&current)?.len())
        .ok_or_else(|| err("Recovery ledger exceeds its complete immutable history bound"))?;
    for generation in (0..generation).rev() {
        let bytes = raw(&directory.join("ledger").join(format!("{generation:08}.json")), remaining)?;
        remaining = remaining.checked_sub(bytes.len()).ok_or_else(|| err("Recovery ledger history exceeds its bound"))?;
        let previous = load_with_limit(&bytes, sharing_limits::MAX_PRIVATE)?;
        if nullable(digest(Some(&bytes))) != expected_previous || encoded(&previous)? != bytes
            || integer(get(&previous, "generation")) != Some(generation)
            || get(&previous, "operation_id").as_str() != Some(operation)
            || get(&previous, "project_id") != get(row, "project_id") {
            return Err(err("Recovery ledger successor lost its complete immutable predecessor chain"));
        }
        expected_previous = get(&previous, "previous_generation_sha256").clone();
    }
    if expected_previous != Value::Null { return Err(err("Recovery ledger history does not terminate at its original generation")); }
    let mut seen = BTreeSet::new();
    for unit in array(get(&current, "units"))? {
        let revision = get(unit, "state").as_str() == Some("superseded-by-revision");
        let mut names = UNIT_FIELDS.to_vec();
        if revision { names.push("child_unit_ids"); }
        if !fields(unit, &names) || !uuid_valid(get(unit, "unit_id")) || !seen.insert(s(get(unit, "unit_id"))?)
            || !uuid_valid(get(unit, "detecting_developer_id"))
            || !(member(get(unit, "state"), TERMINAL) || member(get(unit, "state"), &["pending-admission", "superseded-by-revision"]))
            || get(unit, "terminal_receipts").as_array().is_none() || get(unit, "required_actions").as_array().is_none()
            || !get(unit, "source_ranges").as_array().is_some_and(|ranges| !ranges.is_empty()) {
            return Err(err("Recovery unit is missing, duplicated or has an unreviewed disposition"));
        }
        for range in array(get(unit, "source_ranges"))? { range_value(range)?; }
        if member(get(unit, "state"), TERMINAL) {
            let receipts = array(get(unit, "terminal_receipts"))?;
            if receipts.is_empty() { return Err(err("Terminal recovery unit has no actual durable resolution evidence")); }
            let mut whole = false;
            for receipt in receipts {
                if !fields(receipt, &["resolution_id", "sha256"]) || !uuid_valid(get(receipt, "resolution_id")) || !state::sha(get(receipt, "sha256")) {
                    return Err(err("Recovery terminal receipt linkage is invalid"));
                }
                let terminal = read(&directory.join("resolutions").join(format!("{}.json", s(get(receipt, "resolution_id"))?)))?;
                if hash(&terminal)? != *get(receipt, "sha256") || get(&terminal, "unit_id") != get(unit, "unit_id")
                    || get(&terminal, "operation_id").as_str() != Some(operation)
                    || (get(&terminal, "terminal_state") != get(unit, "state") && get(&terminal, "terminal_state").as_str() != Some("pending-admission")) {
                    return Err(err("Recovery terminal receipt is missing or mismatched"));
                }
                whole |= get(&terminal, "terminal_state") == get(unit, "state");
            }
            if !whole { return Err(err("Partial coverage receipts cannot falsely declare a whole unit terminal")); }
        }
        if revision && !get(unit, "child_unit_ids").as_array().is_some_and(|children| !children.is_empty() && children.iter().all(uuid_valid)) {
            return Err(err("Recovery unit revision is incomplete"));
        }
    }
    for unit in array(get(&current, "units"))? {
        if get(unit, "state").as_str() == Some("superseded-by-revision")
            && array(get(unit, "child_unit_ids"))?.iter().any(|child| child.as_str().is_none_or(|id| !seen.contains(id))) {
            return Err(err("Recovery revision omitted a retained child unit"));
        }
    }
    Ok((directory, head, current))
}
pub fn evidence(directory: &Path, current: &Value, row: &Value) -> Result<(Value, Value)> {
    let manifest = read(&directory.join("manifest.json"))?;
    let mut unit_map = read(&directory.join("unit-map.json"))?;
    if hash(&unit_map)? != *get(current, "unit_map_sha256") {
        if !state::sha(get(current, "unit_map_sha256")) { return Err(err("Recovery source, unit map or reviewed proposal changed")); }
        unit_map = read(&directory.join("unit-maps").join(format!("{}.json", s(get(current, "unit_map_sha256"))?)))?;
    }
    if !uuid_valid(get(row, "proposal_id")) { return Err(err("Recovery source, unit map or reviewed proposal changed")); }
    let proposal = read(&directory.join("proposals").join(format!("{}.json", s(get(row, "proposal_id"))?)))?;
    for (value, key) in [(&manifest, "manifest_sha256"), (&unit_map, "unit_map_sha256"), (&proposal, "proposal_sha256")] {
        if hash(value)? != *get(current, key) { return Err(err("Recovery source, unit map or reviewed proposal changed")); }
    }
    if get(&manifest, "observation_complete").as_bool() != Some(true) || get(&manifest, "project_id") != get(row, "project_id") {
        return Err(err("Recovery source identity or complete observation is unavailable"));
    }
    let review = if get(&proposal, "review") == &Value::Null {
        if !uuid_valid(get(&proposal, "review_id")) { return Err(err("Recovery requires retained independent source-bound review")); }
        read(&directory.join("reviews").join(format!("{}.json", s(get(&proposal, "review_id"))?)))?
    } else { get(&proposal, "review").clone() };
    if hash(&review)? != *get(&proposal, "review_sha256") { return Err(err("Recovery independent review does not match its reviewed proposal")); }
    let mut sources = BTreeMap::new();
    for source in array(get(&manifest, "sources"))? {
        let id = s(get(source, "source_id"))?;
        if !uuid_valid(get(source, "source_id")) || sources.insert(id, source).is_some() {
            return Err(err("Recovery manifest duplicates source identities"));
        }
    }
    for unit in array(get(current, "units"))? {
        if get(unit, "state").as_str() != Some("pending-admission") { continue; }
        for range in array(get(unit, "source_ranges"))? {
            range_value(range)?;
            let source = sources.get(s(get(range, "source_id"))?).ok_or_else(|| err("Pending recovery range is outside its exact original source"))?;
            if n(get(range, "end"))? > n(get(source, "byte_count"))? { return Err(err("Pending recovery range is outside its exact original source")); }
            let bytes = raw(&directory.join("originals").join(format!("{}.bin", s(get(source, "source_id"))?)), SOURCE_BOUND)?;
            if i64::try_from(bytes.len()).ok() != integer(get(source, "byte_count")) || nullable(digest(Some(&bytes))) != *get(source, "sha256") {
                return Err(err("Pending recovery original is missing, damaged or changed"));
            }
        }
    }
    Ok((manifest, proposal))
}

pub fn retained_handoff(root: &Path, operation: &str, row: &Value, manifest: &Value,
    current_operation: Option<&str>, index: &Value, runtime: &Path) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut successor = current_operation.map(str::to_owned);
    while successor.as_deref() != Some(operation) {
        let next = successor.as_deref().ok_or_else(|| err("Retained rolled-back dependency lost its verified successor hold"))?;
        if !seen.insert(next.to_owned()) { return Err(err("Retained rolled-back dependency lost its verified successor hold")); }
        let successor_row = get(index, "operations").get(next).ok_or_else(|| err("Recovery handoff names an absent successor"))?;
        let (directory, _, current) = ledger(root, next, successor_row, runtime)?;
        let (source, proposal) = evidence(&directory, &current, successor_row)?;
        let predecessor = get(&source, "predecessor");
        if !fields(predecessor, &["operation_id", "marker_sha256", "frontier"]) || !same(get(&proposal, "predecessor"), predecessor)? {
            return Err(err("Retained rollback has no exact independently reviewed handoff"));
        }
        let previous = s(get(predecessor, "operation_id"))?;
        let previous_row = get(index, "operations").get(previous).ok_or_else(|| err("Recovery handoff predecessor is absent or no longer retained"))?;
        if get(previous_row, "state").as_str() != Some("rolled-back") { return Err(err("Recovery handoff predecessor is absent or no longer retained")); }
        let marker = hold_marker(previous, previous_row);
        let pinned = get(predecessor, "frontier");
        let heads: Vec<_> = array(get(pinned, "ledger_heads"))?.iter().filter(|head| get(head, "operation_id").as_str() == Some(previous)).collect();
        let (_, bank, _, _) = state::paths(root, None, runtime)?;
        if get(predecessor, "marker_sha256") != &hash(&marker)? || get(pinned, "hold_sha256") != &hash(&marker)?
            || get(pinned, "bank_path").as_str() != Some(text(&bank)?) || get(pinned, "project_id") != get(row, "project_id")
            || heads.len() != 1 || !same(get(get(&source, "identity"), "scope_context"), get(get(manifest, "identity"), "scope_context"))? {
            return Err(err("Recovery handoff changed its predecessor identity or frontier evidence"));
        }
        let head = heads.first().ok_or_else(|| err("Recovery handoff lost its retained head"))?;
        let directory = operation_directory(root, &bank, previous, runtime)?;
        let generation = n(get(head, "generation"))?;
        if !(0..=99_999_999).contains(&generation) { return Err(err("Recovery handoff lost its exact retained predecessor generation")); }
        let old = read(&directory.join("ledger").join(format!("{generation:08}.json")))?;
        if hash(&old)? != *get(head, "sha256") || get(&old, "operation_id").as_str() != Some(previous) {
            return Err(err("Recovery handoff lost its exact retained predecessor generation"));
        }
        successor = Some(previous.to_owned());
    }
    Ok(())
}
fn hold_marker(operation: &str, row: &Value) -> Value {
    object([("schema", string(HOLD)), ("project_id", get(row, "project_id").clone()),
        ("operation_id", string(operation)), ("hold_id", get(row, "hold_id").clone())])
}
pub fn observe(repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    let (root, bank, _, _) = state::paths(repo, None, runtime)?;
    let result = observe_bank(&root, &bank, owner, transaction, true, runtime)?;
    let (after_root, after_bank, _, _) = state::paths(&root, None, runtime)?;
    if root != after_root || bank != after_bank { return Err(err("Recovery bank location changed during observation")); }
    Ok(result)
}
pub fn observe_bank(root: &Path, bank: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>, require_identity: bool, runtime: &Path) -> Result<Value> {
    let index = state::_index_for_bank(root, bank, runtime)?;
    observe_index(root, owner, transaction, &index, bank, require_identity, runtime)
}
pub fn observe_index(repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>, index: &Value,
    bank: &Path, require_identity: bool, runtime: &Path) -> Result<Value> {
    let (root, bank, base, _) = state::_paths_for_bank(repo, bank, None, runtime)?;
    let bytes = marker_bytes(&bank, MARKER, transaction)?;
    let sentinel = marker_bytes(&bank, RECOVERY, None)?;
    let raw_value = match &bytes { Some(raw) => load_with_limit(raw, sharing_limits::MAX_PRIVATE)?, None => Value::Object(Vec::new()) };
    if raw_value.as_object().is_none() { return Err(err("Recovery observation found a malformed canonical transaction marker")); }
    let marker_operation = get(&raw_value, "operation_id");
    let operations = get(index, "operations");
    let active: Vec<_> = pairs(operations)?.iter().filter(|(id, row)| member(get(row, "state"), &["active", "interrupted"])
        || (get(row, "state").as_str() == Some("rolled-back") && marker_operation.as_str() == Some(id.as_str()))).collect();
    if active.len() > 1 { return Err(err("Competing recovery operations require explicit reconciliation")); }
    let child_parent = if owner.is_some() && bytes.as_deref() == transaction && bytes.is_some()
        && get(&raw_value, "schema").as_str() == Some(TRANSACTION)
        && operations.get(s(marker_operation)?).is_none() {
        Some(verified_child_transaction(&root, owner, bytes.as_deref().ok_or_else(|| err("Missing owned child marker"))?, index, runtime)?)
    } else { None };
    let bindings = state::bindings()?;
    for (id, binding) in pairs(get(&bindings, "bindings"))? {
        if get(binding, "bank_path").as_str() != Some(text(&bank)?) || !member(get(binding, "lifecycle_state"), &["active", "interrupted"]) { continue; }
        let known_child = child_parent.is_some() && marker_operation.as_str() == Some(id.as_str());
        if !known_child && operations.get(id).is_none_or(|row| !member(get(row, "state"), &["active", "interrupted", "rolled-back"])) {
            return Err(err("An active bound recovery dependency is missing from the operation index"));
        }
    }
    let mut marker = if !active.is_empty() || sentinel.is_some() || child_parent.is_some() {
        bytes.as_deref().map(marker_value).transpose()?
    } else { None };
    if let Some(parent) = &child_parent {
        let value = marker.as_mut().ok_or_else(|| err("Child recovery marker is missing"))?;
        put(value, "operation_id", string(parent))?;
    }
    if let Some((operation, row)) = active.first() {
        let value = marker.as_ref().ok_or_else(|| err("Recovery hold is absent, foreign or changed; semantic coverage remains pending"))?;
        if get(value, "operation_id").as_str() != Some(operation.as_str()) || get(value, "project_id") != get(row, "project_id")
            || (get(value, "hold_id") != &Value::Null && get(value, "hold_id") != get(row, "hold_id")) {
            return Err(err("Recovery hold is absent, foreign or changed; semantic coverage remains pending"));
        }
        if get(value, "schema").as_str() != Some(HOLD) && (owner.is_none() || transaction != bytes.as_deref()) {
            return Err(err("Recovery is interrupted; use the exact known-operation recovery interface"));
        }
        let expected = object([("schema", string(SENTINEL)), ("operation_id", string(operation)), ("project_id", get(row, "project_id").clone())]);
        if sentinel.as_deref() != Some(encoded(&expected)?.as_slice()) { return Err(err("Recovery sentinel/index pair is missing, orphaned or changed")); }
    } else if child_parent.is_none() && (sentinel.is_some() || member(get(&raw_value, "schema"), &[HOLD, NESTED, TRANSACTION])) {
        return Err(err("Orphan recovery marker has no verified private ledger"));
    }
    let mut ledgers = Vec::new();
    let mut unit_ids = Vec::new();
    let mut source_pins = Vec::new();
    let mut project_ids = BTreeSet::new();
    let mut references = Value::Object(Vec::new());
    for (operation, row) in sorted_rows(operations)? {
        if get(row, "state").as_str() == Some("cleaned") { continue; }
        let (directory, head, current) = ledger_at_bank(&root, operation, row, &bank, runtime)?;
        let (manifest, _) = evidence(&directory, &current, row)?;
        if let Some(mapping) = read_optional(&directory.join("reference-map.json"))?.filter(truthy) {
            if mapping.as_object().is_none() { return Err(err("Private recovery reference map is malformed")); }
            put(&mut references, operation, mapping)?;
        }
        project_ids.insert(s(get(row, "project_id"))?.to_owned());
        let pending: Vec<_> = array(get(&current, "units"))?.iter().filter(|unit| get(unit, "state").as_str() == Some("pending-admission")).cloned().collect();
        if !pending.is_empty() && !member(get(row, "state"), &["active", "rolled-back"]) { return Err(err("Recovery index falsely declares unresolved evidence terminal")); }
        for unit in &pending { unit_ids.push(s(get(unit, "unit_id"))?.to_owned()); }
        let has_pending = !pending.is_empty();
        ledgers.push(object([("operation_id", string(operation)), ("generation", get(&head, "generation").clone()), ("sha256", get(&head, "sha256").clone())]));
        source_pins.push(object([("operation_id", string(operation)), ("manifest_sha256", get(&current, "manifest_sha256").clone()),
            ("unit_map_sha256", get(&current, "unit_map_sha256").clone()), ("units", Value::Array(pending)),
            ("quiescence_heads_sha256", nullable(digest(state::read_bytes(&directory.join("quiescence-attestations.json"), sharing_limits::MAX_PRIVATE, true)?.as_deref())))]));
        let binding = get(&bindings, "bindings").get(operation);
        if member(get(row, "state"), &["active", "interrupted"]) || (get(row, "state").as_str() == Some("rolled-back")
            && (has_pending || marker_operation.as_str() == Some(operation)
                || binding.is_some_and(|binding| member(get(binding, "lifecycle_state"), &["active", "interrupted"])))) {
            let binding = binding.ok_or_else(|| err("Recovery private-state binding is missing or changed"))?;
            if get(binding, "project_id") != get(row, "project_id") || !member(get(binding, "lifecycle_state"), &["active", "interrupted"])
                || get(binding, "checkout_key") != get(get(get(&manifest, "identity"), "scope_context"), "checkout_key") {
                return Err(err("Recovery private-state binding is missing or changed"));
            }
            let journal = read(&directory.join("journal.json"))?;
            if get(&journal, "operation_id").as_str() != Some(operation) || get(&journal, "proposal_sha256") != get(&current, "proposal_sha256")
                || get(&journal, "manifest_sha256") != get(&current, "manifest_sha256") { return Err(err("Recovery journal does not bind the exact reviewed operation")); }
            if get(row, "state").as_str() == Some("rolled-back") {
                retained_handoff(&root, operation, row, &manifest, marker.as_ref().and_then(|v| get(v, "operation_id").as_str()), index, runtime)?;
            }
            if let Some(marker) = marker.as_ref().filter(|v| get(v, "schema").as_str() == Some(NESTED) && get(v, "operation_id").as_str() == Some(operation)) {
                let nested = read(&directory.join("nested").join(s(get(marker, "nested_id"))?).join("journal.json"))?;
                if !same(get(&nested, "marker"), marker)? || get(&nested, "operation_id").as_str() != Some(operation) { return Err(err("Nested recovery marker has no exact typed private journal")); }
            }
        }
    }
    if !unit_ids.is_empty() && active.is_empty() { return Err(err("Retained unresolved source units lost their fail-closed recovery hold")); }
    if project_ids.len() > 1 { return Err(err("Recovery private state conflicts on project identity")); }
    let project_id = if let Some(project) = project_ids.first() { string(project) }
        else if require_identity {
            let router = bank_observation::read_source(&bank, "MEMORY.md", 16384)?;
            let router = std::str::from_utf8(&router).map_err(|_| err("Recovery router must be UTF8"))?;
            get(&bank_contract::parse_router(router)?.0, "project-id").clone()
        } else { Value::Null };
    let index_digest = if state::read_bytes(&base.join("index.json"), sharing_limits::MAX_PRIVATE, true)?.is_some() { hash(index)? } else { Value::Null };
    unit_ids.sort_unstable();
    let hold_hash = if !active.is_empty() {
        let marker = marker.as_ref().ok_or_else(|| err("Recovery hold is missing"))?;
        let operation = s(get(marker, "operation_id"))?;
        let row = operations.get(operation).ok_or_else(|| err("Recovery operation is missing"))?;
        hash(&hold_marker(operation, row))?
    } else { Value::Null };
    Ok(object([("bank_path", string(text(&bank)?)), ("project_id", project_id),
        ("operation_ids", Value::Array(ledgers.iter().map(|head| get(head, "operation_id").clone()).collect())),
        ("ledger_heads", Value::Array(ledgers)), ("frontier_digest", hash(&Value::Array(source_pins))?),
        ("coverage_complete", Value::Bool(unit_ids.is_empty())), ("unit_ids", Value::Array(unit_ids.iter().map(string).collect())),
        ("reference_map_sha256", hash(&references)?), ("review_digest", Value::Null), ("index_sha256", index_digest),
        ("hold_sha256", hold_hash), ("sentinel_sha256", nullable(digest(sentinel.as_deref())))]))
}
pub fn check(expected: &Value, repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    let current = observe(repo, owner, transaction, runtime)?;
    let mut pinned = expected.clone();
    put(&mut pinned, "review_digest", Value::Null)?;
    if !same(&current, &pinned)? { return Err(err(CHANGED)); }
    Ok(current)
}

/// Future layout_resolution implementation; a retained ledger never substitutes
/// for the actual durable mutation and resolution provider.
pub trait LayoutResolution: Send + Sync {
    fn advance_ledger(&self, repo: &Path, operation: &str, units: &Value,
        expected_head: &Value, owner: &[u8], transaction: Option<&[u8]>,
        unit_map_sha256: Option<&str>, runtime: &Path) -> Result<Value>;
    fn restore_hold(&self, repo: &Path, operation: &str, owner: &[u8],
        transaction: &[u8], runtime: &Path) -> Result<Vec<u8>>;
    fn retire(&self, repo: &Path, operation: &str, owner: &[u8],
        transaction: Option<&[u8]>, runtime: &Path) -> Result<Value>;
    fn resolve(&self, repo: &Path, operation: Option<&str>, unit: Option<&str>,
        terminal: Option<&str>, proposal: Option<&Value>, runtime: &Path) -> Result<Value>;
}
static RESOLUTION: RwLock<Option<Arc<dyn LayoutResolution>>> = RwLock::new(None);
pub fn register_layout_resolution(provider: Option<Arc<dyn LayoutResolution>>) {
    *RESOLUTION.write().unwrap_or_else(|error| error.into_inner()) = provider;
}
pub(crate) fn layout_resolution_provider() -> Result<Arc<dyn LayoutResolution>> {
    RESOLUTION.read().unwrap_or_else(|error| error.into_inner()).clone()
        .ok_or_else(|| ContractError::missing_provider("layout_resolution provider is not registered; recovery frontier resolution wiring pending"))
}

pub fn verified_hold(repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>,
    runtime: &Path, binary: &Path) -> Result<Option<VerifiedHold>> {
    let (root, bank, _, _) = state::paths(repo, None, runtime)?;
    let held = verified_hold_at_bank(&root, &bank, owner, transaction, runtime, binary)?;
    let (after_root, after_bank, _, _) = state::paths(&root, None, runtime)?;
    if root != after_root || bank != after_bank { return Err(err("Recovery bank location changed during observation")); }
    Ok(held)
}
pub fn verified_hold_at_bank(root: &Path, bank: &Path, owner: Option<&[u8]>,
    transaction: Option<&[u8]>, runtime: &Path, binary: &Path) -> Result<Option<VerifiedHold>> {
    let token = observe_bank(root, bank, owner, transaction, true, runtime)?;
    let Some(bytes) = marker_bytes(bank, MARKER, None)? else { return Ok(None); };
    let marker = marker_value(&bytes)?;
    if get(&marker, "schema").as_str() != Some(HOLD) && transaction != Some(bytes.as_slice()) {
        return Err(err("Only an internally checked persistent hold permits qualified canonical access"));
    }
    let measured = crate::artifact::inspect_runtime(runtime, binary).map_err(io)?;
    if !get(get(&measured, "compatibility"), "private_state_capabilities").as_array()
        .is_some_and(|caps| caps.iter().any(|cap| cap.as_str() == Some("layout-frontier-v1"))) {
        return Err(err("Active runtime cannot enforce the private recovery frontier"));
    }
    let operation = s(get(&marker, "operation_id"))?;
    let index = state::_index_for_bank(root, bank, runtime)?;
    let row = get(&index, "operations").get(operation).ok_or_else(|| err("Recovery hold has no indexed operation"))?;
    let (directory, _, current) = ledger_at_bank(root, operation, row, bank, runtime)?;
    let (manifest, proposal) = evidence(&directory, &current, row)?;
    let expected = get(get(&manifest, "identity"), "scope_context");
    let context = if get(row, "state").as_str() == Some("rolled-back") {
        let router = project_identity::layout_observation_provider()?.read_source(bank, Path::new("MEMORY.md"), SOURCE_BOUND)?;
        let original = array(get(&manifest, "sources"))?.iter().find(|source| get(source, "path").as_str() == Some("MEMORY.md"))
            .ok_or_else(|| err("Rolled-back identity requires the exact reviewed original raw router"))?;
        if integer(get(original, "byte_count")) != i64::try_from(router.len()).ok()
            || get(original, "sha256") != &nullable(digest(Some(&router))) {
            return Err(err("Rolled-back identity requires the exact reviewed original raw router"));
        }
        let identity = project_identity::RawIdentityEvidence::observe(root, &router, get(&manifest, "identity").get("identity_review"))?;
        let context = scope_binding::raw_recovery_context(root, &identity, owner, Some(&bytes))?;
        if !same(&context, expected)? { return Err(err("Rolled-back raw identity differs from the independently reviewed original scope")); }
        context
    } else {
        scope_binding::require(root, get(&token, "project_id").as_str(), scope_binding::RequireOptions {
            expected: Some(expected), owned_lock: owner, owned_transaction: Some(&bytes), ..Default::default()
        })?
    };
    let bindings = state::bindings()?;
    if get(&context, "checkout_key") != get(get(get(&bindings, "bindings"), operation), "checkout_key") {
        return Err(err("Recovery checkout continuity changed"));
    }
    let runtime_digest = s(get(&measured, "artifact_digest"))?;
    let attestation = crate::private_state::recovery_quiescence_provider()?
        .selected_attestation(&directory, &proposal, runtime_digest, runtime)?
        .ok_or_else(|| err("Recovery hold has no independently reviewed attestation for the measured executing runtime"))?;
    if get(&attestation, "operation_id").as_str() != Some(operation)
        || get(&attestation, "project_id") != get(&token, "project_id")
        || get(&attestation, "repo_root").as_str() != Some(text(root)?)
        || get(&attestation, "bank_path").as_str() != Some(text(bank)?)
        || get(&attestation, "checkout_key") != get(&context, "checkout_key") {
        return Err(err("Recovery quiescence attestation has stale identity or route pins"));
    }
    if !same(&observe_bank(root, bank, owner, transaction, true, runtime)?, &token)? { return Err(err(CHANGED)); }
    Ok(Some(VerifiedHold::from_verified_observation(root.to_path_buf(), bank.to_path_buf(), bytes,
        token, context, operation.to_owned(), runtime_digest.to_owned())?))
}
pub fn rolled_back_predecessor(repo: &Path, owner: Option<&[u8]>, runtime: &Path, binary: &Path) -> Result<Option<VerifiedHold>> {
    let (root, bank, _, _) = state::paths(repo, None, runtime)?;
    if marker_bytes(&bank, MARKER, None)?.is_none() {
        bank_observation::markers(&bank, owner, None)?;
        let index = state::index(&root, runtime)?;
        let bindings = state::bindings()?;
        if marker_bytes(&bank, RECOVERY, None)?.is_some()
            || pairs(get(&bindings, "bindings"))?.iter().any(|(_, binding)| get(binding, "bank_path").as_str() == bank.to_str()
                && member(get(binding, "lifecycle_state"), &["active", "interrupted"])) {
            return Err(err("Recovery inspection found an orphaned private dependency or sentinel"));
        }
        for (operation, row) in pairs(get(&index, "operations"))? {
            if get(row, "state").as_str() == Some("cleaned") { continue; }
            let (directory, _, current) = ledger(&root, operation, row, runtime)?;
            evidence(&directory, &current, row)?;
            if member(get(row, "state"), &["active", "interrupted"])
                || array(get(&current, "units"))?.iter().any(|unit| get(unit, "state").as_str() == Some("pending-admission")) {
                return Err(err("Recovery inspection cannot omit a retained frontier without its hold"));
            }
        }
        return Ok(None);
    }
    let held = verified_hold(repo, owner, None, runtime, binary)?;
    if let Some(held) = &held {
        let indexed = state::index(repo, runtime)?;
        if get(get(get(&indexed, "operations"), held.operation_id()), "state").as_str() != Some("rolled-back") {
            return Err(err("Only a completed exact rollback may hand its hold to a fresh reviewed repair"));
        }
    }
    Ok(held)
}
pub fn same_hold(first: Option<&VerifiedHold>, second: Option<&VerifiedHold>) -> Result<bool> {
    match (first, second) {
        (Some(first), Some(second)) => Ok(first.root() == second.root() && first.bank() == second.bank()
            && first.marker() == second.marker() && same(first.token(), second.token())?
            && same(first.scope(), second.scope())? && first.operation_id() == second.operation_id()
            && first.runtime_artifact_digest() == second.runtime_artifact_digest()),
        _ => Ok(false),
    }
}
pub fn require_scope(repo: &Path, project: Option<&str>, options: scope_binding::RequireOptions<'_>,
    runtime: &Path, binary: &Path) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    let chain = DirectoryChain::open(&bank);
    let router_missing = match chain {
        Ok(chain) => match chain.directory().stat_at(OsStr::new("MEMORY.md")) {
            Ok(_) => { chain.recheck().map_err(io)?; false }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => { chain.recheck().map_err(io)?; true }
            Err(error) => return Err(io(error)),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(io(error)),
    };
    if options.initialize && router_missing { return scope_binding::require(&root, project, options); }
    let bytes = marker_bytes(&bank, MARKER, options.owned_transaction)?;
    let held_schema = bytes.as_ref().map(|bytes| load_with_limit(bytes, sharing_limits::MAX_PRIVATE)).transpose()?
        .is_some_and(|value| member(get(&value, "schema"), &[HOLD, NESTED]));
    if !held_schema {
        observe(&root, options.owned_lock, options.owned_transaction, runtime)?;
        return scope_binding::require(&root, project, options);
    }
    let held = verified_hold(&root, options.owned_lock, options.owned_transaction, runtime, binary)?
        .ok_or_else(|| err("Recovery hold disappeared during scope observation"))?;
    if project.is_some_and(|project| get(held.scope(), "project_id").as_str() != Some(project)) {
        return Err(err("Recovery hold belongs to a different project"));
    }
    if let Some(expected) = options.expected {
        if !same(held.scope(), expected)? { return Err(err("Recovery scope changed during the operation")); }
    }
    Ok(held.scope().clone())
}
fn ensure_safe(body: &str) -> Result<()> {
    let report = crate::repository_config::security_scan_provider()?.scan(body)?;
    let reasons = array(get(&report, "reasons"))?;
    if !reasons.is_empty() {
        let reasons = reasons.iter().map(s).collect::<Result<Vec<_>>>()?;
        return Err(ContractError::new(format!("Security policy quarantined proposed memory; redact or review before writing: {}", reasons.join(", "))));
    }
    Ok(())
}
fn disclose_pending(directory: &Path, manifest: &Value, current: &Value, operation: &str, inherited: bool) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    let sources: BTreeMap<_, _> = array(get(manifest, "sources"))?.iter()
        .map(|source| Ok((s(get(source, "source_id"))?, source))).collect::<Result<_>>()?;
    for unit in array(get(current, "units"))? {
        if get(unit, "state").as_str() != Some("pending-admission") { continue; }
        let mut body = Vec::new();
        for range in array(get(unit, "source_ranges"))? {
            range_value(range)?;
            let id = s(get(range, "source_id"))?;
            let source = sources.get(id).ok_or_else(|| err("Pending recovery range is outside its exact original source"))?;
            let bytes = raw(&directory.join("originals").join(format!("{id}.bin")), SOURCE_BOUND)?;
            if nullable(digest(Some(&bytes))) != *get(source, "sha256")
                || integer(get(source, "byte_count")) != i64::try_from(bytes.len()).ok() {
                return Err(err("Pending recovery original is missing, damaged or changed"));
            }
            let start = usize::try_from(n(get(range, "start"))?).map_err(io)?;
            let end = usize::try_from(n(get(range, "end"))?).map_err(io)?;
            let piece = bytes.get(start..end).ok_or_else(|| err("Pending recovery range is outside its exact original source"))?;
            if body.len().checked_add(piece.len()).is_none_or(|len| len > sharing_limits::MAX_FILE) {
                return Err(err(if inherited { "Retained rolled-back unit exceeds the complete comparison reader" }
                    else { "An indivisible pending source exceeds the ordinary complete comparison reader" }));
            }
            body.extend_from_slice(piece);
        }
        let body = String::from_utf8(body).map_err(|_| err("Pending recovery source must be complete UTF8"))?;
        ensure_safe(&body)?;
        rows.push(object([("unit_id", get(unit, "unit_id").clone()), ("operation_id", string(operation)),
            ("body", string(body)), ("source_ranges", get(unit, "source_ranges").clone()),
            ("provenance", get(unit, "provenance").clone()), ("reason", get(unit, "reason").clone()),
            ("selection_class", string("comparison-only-not-current-guidance")), ("source_assertions_verified", Value::Bool(false))]));
    }
    Ok(rows)
}
pub fn pending_rows(repo: &Path, token: &Value, owner: Option<&[u8]>, transaction: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    check(token, repo, owner, transaction, runtime)?;
    let mut rows = Vec::new();
    let index = state::index(repo, runtime)?;
    for (operation, indexed) in sorted_rows(get(&index, "operations"))? {
        if !member(get(indexed, "state"), &["active", "rolled-back"]) { continue; }
        let (directory, _, current) = ledger(repo, operation, indexed, runtime)?;
        let (manifest, _) = evidence(&directory, &current, indexed)?;
        rows.extend(disclose_pending(&directory, &manifest, &current, operation, false)?);
    }
    let actual = rows.iter().map(|row| s(get(row, "unit_id"))).collect::<Result<BTreeSet<_>>>()?;
    let expected = array(get(token, "unit_ids"))?.iter().map(s).collect::<Result<BTreeSet<_>>>()?;
    if actual != expected { return Err(err("Pending frontier disclosure omitted or substituted a source unit")); }
    check(token, repo, owner, transaction, runtime)?;
    Ok(Value::Array(rows))
}
pub fn inherited_pending_rows(repo: &Path, runtime: &Path) -> Result<Value> {
    let mut rows = Vec::new();
    let index = state::index(repo, runtime)?;
    for (operation, row) in sorted_rows(get(&index, "operations"))? {
        if get(row, "state").as_str() != Some("rolled-back") { continue; }
        let (directory, _, current) = ledger(repo, operation, row, runtime)?;
        let (manifest, _) = evidence(&directory, &current, row)?;
        rows.extend(disclose_pending(&directory, &manifest, &current, operation, true)?);
    }
    let ids = rows.iter().map(|row| s(get(row, "unit_id"))).collect::<Result<BTreeSet<_>>>()?;
    if ids.len() != rows.len() { return Err(err("Retained recovery units have conflicting opaque identities")); }
    Ok(Value::Array(rows))
}
pub fn review_for_comparison(repo: &Path, token: &Value, owner: Option<&[u8]>, transaction: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    pending_rows(repo, token, owner, transaction, runtime)
}
pub fn require_complete(repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    let token = observe(repo, owner, transaction, runtime)?;
    if get(&token, "coverage_complete").as_bool() != Some(true) || get(&token, "hold_sha256") != &Value::Null {
        return Err(err("Private recovery coverage is incomplete; resolve all units before refresh or publication"));
    }
    Ok(token)
}
pub fn public_coverage(token: &Value) -> Result<Value> {
    let complete = get(token, "coverage_complete").as_bool().ok_or_else(|| err("Recovery coverage requires a checked frontier"))?;
    let units = array(get(token, "unit_ids"))?;
    Ok(object([("semantic_coverage", string(if complete { "complete-canonical-observation" } else { "incomplete-recovery" })),
        ("pending_recovery_unit_ids", Value::Array(units.to_vec())), ("pending_recovery_units", Value::Int(units.len().into())),
        ("recovery_required_actions", Value::Array(if complete { vec![] } else { vec![string("Review and resolve the retained private recovery units; do not infer semantic absence")] }))]))
}
pub fn invalidate(repo: &Path) -> Result<String> {
    let provider = crate::bank_state::comparison_view_provider()?;
    match provider.invalidate_frontier_transition(repo) {
        Ok(value) => Ok(value),
        Err(error) if matches!(error.kind(), crate::error::Kind::MissingProvider) => Err(error),
        Err(_) => Ok("deferred-to-live-frontier-check".to_owned()),
    }
}
pub fn install(repo: &Path, operation: &str, proposal: &Value, pending_ledger: &Value, hold_id: &str,
    owner: &[u8], predecessor: Option<&VerifiedHold>, runtime: &Path, binary: &Path) -> Result<Value> {
    let (root, bank, base, _) = state::paths(repo, Some(operation), runtime)?;
    let directory = operation_directory(&root, &bank, operation, runtime)?;
    if let Some(predecessor) = predecessor {
        let current = rolled_back_predecessor(&root, Some(owner), runtime, binary)?;
        if !same_hold(current.as_ref(), Some(predecessor))? { return Err(err("Recovery predecessor must be the exact internally verified rolled-back hold")); }
        bank_observation::markers(&bank, Some(owner), Some(predecessor.marker()))?;
    } else { bank_observation::markers(&bank, Some(owner), None)?; }
    if !uuid_valid(&string(hold_id)) || get(pending_ledger, "operation_id").as_str() != Some(operation) {
        return Err(err("Recovery installation needs exact operation/hold identities"));
    }
    let mut previous = state::index(&root, runtime)?;
    if pairs(get(&previous, "operations"))?.iter().any(|(_, row)| member(get(row, "state"), &["active", "interrupted"])) {
        return Err(err("Another unresolved recovery operation already protects this bank"));
    }
    state::write_once(&directory.join("ledger/00000000.json"), pending_ledger)?;
    let head = object([("schema", string("agent-memory/layout-ledger-head-v1")), ("operation_id", string(operation)),
        ("generation", Value::Int(0.into())), ("sha256", hash(pending_ledger)?)]);
    state::write_once(&directory.join("ledger-head.json"), &head)?;
    writes::fault("layout-recovery-index-intent")?;
    let old = digest(state::read_bytes(&base.join("index.json"), sharing_limits::MAX_PRIVATE, true)?.as_deref());
    let row = object([("state", string("active")), ("project_id", get(pending_ledger, "project_id").clone()),
        ("ledger_head_sha256", hash(&head)?), ("proposal_id", get(proposal, "proposal_id").clone()), ("hold_id", string(hold_id))]);
    let operations = previous.get_mut("operations").ok_or_else(|| err("Recovery operation index is missing"))?;
    put(operations, operation, row)?;
    state::set_index(&root, &previous, old.as_deref(), runtime)?;
    state::bind(&root, operation, s(get(pending_ledger, "project_id"))?, s(get(get(proposal, "scope_context"), "checkout_key"))?, "active", runtime)?;
    invalidate(&root)?;
    Ok(object([("schema", string(SENTINEL)), ("project_id", get(pending_ledger, "project_id").clone()), ("operation_id", string(operation))]))
}

pub fn claim_value<'a>(claim: &'a Value, row: &Value) -> Result<&'a Value> {
    if !fields(claim, &["unit_id", "covered_source_ranges", "target_record_refs", "coverage"])
        || get(claim, "unit_id") != get(row, "unit_id") || !member(get(claim, "coverage"), &["full", "partial", "none"])
        || !get(claim, "covered_source_ranges").as_array().is_some_and(|v| v.len() <= 2)
        || !get(claim, "target_record_refs").as_array().is_some_and(|v| v.len() <= 2) {
        return Err(err("Pending coverage must be one bounded source-bound claim fragment"));
    }
    let ranges = array(get(claim, "covered_source_ranges"))?;
    let parents = array(get(row, "source_ranges"))?;
    for range in ranges {
        range_value(range)?;
        let mut contained = false;
        for parent in parents {
            range_value(parent)?;
            contained |= get(parent, "source_id") == get(range, "source_id")
                && n(get(parent, "start"))? <= n(get(range, "start"))? && n(get(range, "end"))? <= n(get(parent, "end"))?;
        }
        if !contained { return Err(err("Coverage fragment escapes its original reviewed unit")); }
    }
    let mut ordered: Vec<_> = ranges.iter().collect();
    ordered.sort_unstable_by_key(|range| (get(range, "source_id").as_str(), integer(get(range, "start"))));
    for adjacent in ordered.windows(2) {
        // windows(2) establishes the two entries.
        let [a, b] = adjacent else { continue; };
        if get(a, "source_id") == get(b, "source_id") && n(get(a, "end"))? > n(get(b, "start"))? {
            return Err(err("Coverage fragment double-counts overlapping original ranges"));
        }
    }
    let references = array(get(claim, "target_record_refs"))?;
    let mut seen = BTreeSet::new();
    for reference in references {
        if !fields(reference, &["id", "version", "path", "resulting_sha256"])
            || !uuid_valid(get(reference, "id")) || !integer(get(reference, "version")).is_some_and(|v| (1..=2_147_483_647).contains(&v))
            || !get(reference, "path").as_str().is_some_and(|path| path.len() <= 64)
            || !state::sha(get(reference, "resulting_sha256")) {
            return Err(err("Coverage references need exact bounded record identity, version, path and hash"));
        }
        let path = s(get(reference, "path"))?;
        if let Some(id) = path.strip_prefix("ref:") {
            if !uuid_valid(&string(id)) { return Err(err("Private reference locator must be an opaque UUIDv4")); }
        } else { bank_contract::path_value(get(reference, "path"))?; }
        if !seen.insert(encoded(reference)?) { return Err(err("Coverage fragment duplicates a resulting reference")); }
    }
    if get(claim, "coverage").as_str() == Some("none") {
        if !ranges.is_empty() || !references.is_empty() { return Err(err("No-coverage claim cannot assert source ranges or resulting records")); }
    } else if ranges.is_empty() || references.is_empty() {
        return Err(err("Coverage requires actual source ranges and resulting record references"));
    }
    if get(claim, "coverage").as_str() == Some("full") {
        let mut expected = parents.to_vec();
        expected.sort_unstable_by(|a, b| (get(a, "source_id").as_str(), integer(get(a, "start")))
            .cmp(&(get(b, "source_id").as_str(), integer(get(b, "start")))));
        let mut merged: Vec<Value> = Vec::new();
        for range in ordered {
            if let Some(last) = merged.last_mut().filter(|last| get(last, "source_id") == get(range, "source_id") && get(last, "end") == get(range, "start")) {
                put(last, "end", get(range, "end").clone())?;
            } else { merged.push(range.clone()); }
        }
        if !same(&Value::Array(merged), &Value::Array(expected))? {
            return Err(err("A full claim must cover the entire original unit, including framing and conditions"));
        }
    }
    Ok(claim)
}
pub fn proof_input(proof: &Value) -> Result<Value> {
    let mut value = proof.clone();
    let frontier = value.get_mut("judgment").and_then(|v| v.get_mut("observation"))
        .and_then(|v| v.get_mut("recovery_frontier")).ok_or_else(|| err("Comparison proof omits its recovery frontier"))?;
    put(frontier, "review_digest", Value::Null)?;
    hash(&value)
}
pub fn validate_comparison_review(repo: &Path, proof: &Value, owner: Option<&[u8]>, transaction: Option<&[u8]>, runtime: &Path) -> Result<Option<Value>> {
    let judgment = get(proof, "judgment");
    let token = get(get(judgment, "observation"), "recovery_frontier");
    check(token, repo, owner, transaction, runtime)?;
    let claims: Vec<_> = array(get(judgment, "pending_decisions"))?.iter().map(|decision| get(decision, "coverage_claim").clone()).collect();
    if array(get(token, "unit_ids"))?.is_empty() {
        if get(token, "review_digest") != &Value::Null || !claims.is_empty() { return Err(err("An empty checked frontier cannot substitute a private coverage review")); }
        return Ok(None);
    }
    if !state::sha(get(token, "review_digest")) { return Err(err("Pending-source comparison requires an independently retained complete coverage review")); }
    let mut reviews = Vec::new();
    let input_hash = proof_input(proof)?;
    let claims = Value::Array(claims);
    for operation in array(get(token, "operation_ids"))? {
        let operation = s(operation)?;
        let (root, bank, _, _) = state::paths(repo, Some(operation), runtime)?;
        let directory = operation_directory(&root, &bank, operation, runtime)?;
        let path = directory.join("coverage-reviews").join(format!("{}.json", s(get(token, "review_digest"))?));
        let Some(review) = read_optional(&path)? else { continue; };
        let index = state::index(repo, runtime)?;
        let indexed = get(&index, "operations").get(operation).ok_or_else(|| err("Coverage review operation is missing"))?;
        let (_, head, current) = ledger(repo, operation, indexed, runtime)?;
        if !fields(&review, &["schema", "review_id", "operation_id", "reviewer_developer_id", "reviewer_harness",
            "reviewer_session", "review_time", "manifest_sha256", "unit_map_sha256", "ledger_head_sha256",
            "comparison_proof_input_sha256", "intended_output_sha256", "claims", "rationale", "decision"])
            || get(&review, "schema").as_str() != Some("agent-memory/layout-coverage-review-v1")
            || hash(&review)? != *get(token, "review_digest") || !uuid_valid(get(&review, "review_id"))
            || get(&review, "operation_id").as_str() != Some(operation) || !uuid_valid(get(&review, "reviewer_developer_id"))
            || get(&review, "manifest_sha256") != get(&current, "manifest_sha256")
            || get(&review, "unit_map_sha256") != get(&current, "unit_map_sha256")
            || get(&review, "ledger_head_sha256") != get(&head, "sha256") || get(&review, "decision").as_str() != Some("approve")
            || get(&review, "comparison_proof_input_sha256") != &input_hash || !same(get(&review, "claims"), &claims)?
            || !state::sha(get(&review, "intended_output_sha256")) {
            return Err(err("Independent coverage review is stale, incomplete or bound to another proposed output"));
        }
        crate::lifecycle::time_value(get(&review, "review_time"))?;
        crate::bank_state::comparison_view_provider()?.safe_text(get(&review, "rationale"), 4096)?;
        if !get(&review, "reviewer_harness").as_str().is_some_and(|v| !v.is_empty())
            || !get(&review, "reviewer_session").as_str().is_some_and(|v| !v.is_empty()) {
            return Err(err("Coverage review needs the actual independent reviewing harness/session"));
        }
        reviews.push(review);
    }
    if reviews.len() != 1 { return Err(err("Pending comparison needs one exact retained independent review")); }
    Ok(reviews.pop())
}
pub fn reservation(files: &Value, pending: &Value, token: &Value) -> Result<Value> {
    let checked = bank_contract::validate_files(files, None)?;
    let mut rows = Vec::new();
    let mut captures = Vec::new();
    let mut identities = BTreeSet::from([crate::host::uuid4()?]);
    for entry in array(get(&checked, "entries"))? {
        if get(entry, "format").as_str() != Some("standalone-record") { continue; }
        let path = s(get(entry, "path"))?;
        let content = s(get(files, path))?;
        let (_, metadata, body) = crate::records::parse(content, &[])?;
        identities.insert(s(get(&metadata, "id"))?.to_owned());
        let row = object([("path", string(path)), ("sha256", nullable(digest(Some(content.as_bytes())))),
            ("format", get(entry, "format").clone()), ("body", string(body)), ("metadata", metadata),
            ("scope_chain", get(entry, "scope_chain").clone()), ("lifecycle", get(entry, "lifecycle").clone()),
            ("matched_searches", Value::Array(vec![string("x".repeat(256)); 8])),
            ("selection_class", string("comparison-only-not-current-guidance")), ("source_assertions_verified", Value::Bool(false))]);
        if path.starts_with("history/sources/") {
            let captured = crate::records::source_capture_id(path, get(&row, "metadata"));
            if let Some(captured) = &captured { identities.insert(captured.clone()); }
            captures.push((captured, rows.len()));
        }
        rows.push(row);
    }
    let pending = array(pending)?;
    for row in pending {
        let body = s(get(row, "body"))?;
        if body.len() > sharing_limits::MAX_FILE { return Err(err("Pending semantic unit is indivisible within the ordinary source bound")); }
        ensure_safe(body)?;
    }
    let mut maximum = object([("rows", Value::Int(0.into())), ("disclosure_bytes", Value::Int(0.into())),
        ("proof_bytes", Value::Int(0.into())), ("witness_identity", Value::Null)]);
    let pending_disclosure = pending.iter().try_fold(0usize, |size, row| -> Result<usize> {
        size.checked_add(encoded(row)?.len()).ok_or_else(|| err("Mandatory recovery frontier reservation cannot guarantee a complete bounded comparison"))
    })?;
    for identity in identities {
        let mut owners = BTreeSet::new();
        for (captured, index) in &captures {
            if captured.as_deref() != Some(identity.as_str()) { continue; }
            let row = rows.get(*index).ok_or_else(|| err("Recovery reservation capture is missing"))?;
            owners.insert(s(get(get(row, "metadata"), "id"))?);
            if let Some(references) = get(get(row, "metadata"), "superseded-by").as_array() {
                for reference in references { owners.insert(s(get(reference, "id"))?); }
            }
        }
        let mut mandatory = Vec::new();
        for row in &rows {
            let id = s(get(get(row, "metadata"), "id"))?;
            let mut captured_legacy = false;
            for (captured, index) in &captures {
                if captured.is_some() { continue; }
                let source = rows.get(*index).ok_or_else(|| err("Recovery reservation capture is missing"))?;
                if get(source, "path") == get(row, "path")
                    && array(get(get(source, "metadata"), "derived-from"))?.iter().any(|reference| get(reference, "id").as_str() == Some(identity.as_str())) {
                    captured_legacy = true;
                }
            }
            if id == identity || owners.contains(id) || captured_legacy { mandatory.push(row); }
        }
        let mut disclosure = pending_disclosure;
        for row in &mandatory { disclosure = disclosure.checked_add(encoded(row)?.len()).ok_or_else(|| err("Recovery disclosure byte count overflow"))?; }
        let mut frontier = token.clone();
        put(&mut frontier, "review_digest", string("f".repeat(64)))?;
        let observation = object([("schema", string("agent-memory/comparison-observation-v2")),
            ("project_id", get(&checked, "project_id").clone()), ("request_sha256", string("f".repeat(64))),
            ("bank_observation_sha256", string("f".repeat(64))), ("router_sha256", string("f".repeat(64))),
            ("recovery_frontier", frontier), ("matches", Value::Array(mandatory.iter().map(|row| object([
                ("path", get(row, "path").clone()), ("sha256", get(row, "sha256").clone())])).collect()))]);
        let fragments = pending.iter().map(|row| object([("unit_id", get(row, "unit_id").clone()),
            ("classification", string("distinct-conditions")), ("rationale", string("\"".repeat(2048))),
            ("coverage_claim", object([("unit_id", get(row, "unit_id").clone()), ("coverage", string("partial")),
                ("covered_source_ranges", Value::Array(vec![object([("source_id", string("ffffffff-ffff-4fff-bfff-ffffffffffff")),
                    ("start", Value::Int(1_048_574.into())), ("end", Value::Int(1_048_576.into()))]); 2])),
                ("target_record_refs", Value::Array(vec![object([("id", string("ffffffff-ffff-4fff-bfff-ffffffffffff")),
                    ("version", Value::Int(2_147_483_647i64.into())), ("path", string("\"".repeat(64))),
                    ("resulting_sha256", string("f".repeat(64)))]); 2]))]))])).collect();
        let skeleton = object([("schema", string("agent-memory/comparison-proof-v2")),
            ("judgment", object([("observation", observation),
                ("decisions", Value::Array(mandatory.iter().map(|row| object([("path", get(row, "path").clone()),
                    ("classification", string("distinct-conditions")), ("rationale", string("\"".repeat(2048)))])).collect())),
                ("pending_decisions", Value::Array(fragments)), ("rationale", string("\"".repeat(2046)))])),
            ("disposition", string("provenance-identity-review-required")),
            ("targets", Value::Array(mandatory.iter().map(|row| object([("path", get(row, "path").clone()),
                ("sha256", get(row, "sha256").clone()), ("id", get(get(row, "metadata"), "id").clone()),
                ("version", get(get(row, "metadata"), "version").clone()), ("required_preserved_metadata", get(row, "metadata").clone()),
                ("scope_chain", get(row, "scope_chain").clone())])).collect()))]);
        let proof_bytes = encoded(&skeleton)?.len();
        let count = pending.len().checked_add(mandatory.len()).ok_or_else(|| err("Recovery comparison row count overflow"))?;
        if count > 64 || disclosure > 65536 || proof_bytes > 65536 {
            return Err(err("Mandatory recovery frontier reservation cannot guarantee a complete bounded comparison"));
        }
        if n(get(&maximum, "proof_bytes"))? <= i64::try_from(proof_bytes).map_err(io)? {
            maximum = object([("rows", Value::Int(count.into())), ("disclosure_bytes", Value::Int(disclosure.into())),
                ("proof_bytes", Value::Int(proof_bytes.into())), ("witness_identity", string(identity))]);
        }
    }
    Ok(maximum)
}
pub fn advance_ledger(repo: &Path, operation: &str, units: &Value, expected_head: &Value, owner: &[u8],
    transaction: Option<&[u8]>, unit_map_sha256: Option<&str>, runtime: &Path) -> Result<Value> {
    layout_resolution_provider()?.advance_ledger(repo, operation, units, expected_head, owner, transaction, unit_map_sha256, runtime)
}
pub fn restore_hold(repo: &Path, operation: &str, owner: &[u8], transaction: &[u8], runtime: &Path) -> Result<Vec<u8>> {
    layout_resolution_provider()?.restore_hold(repo, operation, owner, transaction, runtime)
}
pub fn retire(repo: &Path, operation: &str, owner: &[u8], transaction: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    layout_resolution_provider()?.retire(repo, operation, owner, transaction, runtime)
}
pub fn resolve(repo: &Path, operation: Option<&str>, unit: Option<&str>, terminal: Option<&str>,
    proposal: Option<&Value>, runtime: &Path) -> Result<Value> {
    layout_resolution_provider()?.resolve(repo, operation, unit, terminal, proposal, runtime)
}
pub fn resolve_reference(repo: &Path, reference: &Value, runtime: &Path) -> Result<Value> {
    let mut found = Vec::new();
    let index = state::index(repo, runtime)?;
    for (operation, row) in pairs(get(&index, "operations"))? {
        if !member(get(row, "state"), &["active", "terminal"]) { continue; }
        let (root, bank, _, _) = state::paths(repo, Some(operation), runtime)?;
        let directory = operation_directory(&root, &bank, operation, runtime)?;
        let mapping = read_optional(&directory.join("reference-map.json"))?.unwrap_or_else(|| Value::Object(Vec::new()));
        if let Some(actual) = mapping.get(s(get(reference, "path"))?) {
            if !fields(actual, &["id", "version", "path", "resulting_sha256"]) || get(actual, "id") != get(reference, "id")
                || get(actual, "resulting_sha256") != get(reference, "resulting_sha256")
                || !integer(get(actual, "version")).is_some_and(|v| v >= 1) || integer(get(reference, "version")) != Some(1) {
                return Err(err("Private reference locator does not bind its real record identity/version/hash"));
            }
            bank_contract::path_value(get(actual, "path"))?;
            found.push(actual.clone());
        }
    }
    if found.len() != 1 { return Err(err("Private record reference is missing, ambiguous or foreign")); }
    found.pop().ok_or_else(|| err("Private record reference is missing, ambiguous or foreign"))
}
pub fn recover_nested(repo: &Path, operation: Option<&str>, nested: Option<&str>, mode: Option<&str>,
    proposal: Option<&Value>, runtime: &Path) -> Result<Value> {
    writes::recovery_transactions_provider()?.recover_nested(repo, operation, nested, mode, proposal, runtime)
}
pub fn capture_scope(repo: &Path, runtime: &Path, binary: &Path) -> Result<Value> {
    match require_scope(repo, None, Default::default(), runtime, binary) {
        Ok(scope) => Ok(scope),
        Err(error) if matches!(error.kind(), crate::error::Kind::BankCapacity { .. } | crate::error::Kind::MissingProvider) => Err(error),
        Err(original) => {
            let (root, bank, _, _) = state::paths(repo, None, runtime)?;
            let index = state::index(&root, runtime)?;
            if marker_bytes(&bank, MARKER, None)?.is_some()
                || pairs(get(&index, "operations"))?.iter().any(|(_, row)| member(get(row, "state"), &["active", "interrupted"])) {
                return Err(original);
            }
            Ok(Value::Null)
        }
    }
}
pub fn private_preview(repo: &Path, value: &Value, output: Option<&Path>, runtime: &Path) -> Result<Value> {
    let token = observe(repo, None, None, runtime)?;
    let units = array(get(&token, "unit_ids"))?;
    if output.is_some() || get(value, "output_plan") != &Value::Null || (get(value, "proposal") != &Value::Null && !units.is_empty()) {
        let output = output.ok_or_else(|| err("Governed output plans and recovery-bound proposals require checked --private-output"))?;
        return Ok(object([("schema", get(value, "schema").clone()), ("operation", get(value, "operation").clone()),
            ("private_artifact", sharing_limits::write_private(output, value, repo, runtime)?),
            ("pending_recovery_units", Value::Int(units.len().into())), ("canonical_writes_performed", Value::Bool(false))]));
    }
    Ok(value.clone())
}
pub fn validate_quiescence(repo: &Path, value: &Value, runtime_digest: Option<&str>, routes: &[String],
    publish: bool, owner: Option<&[u8]>, runtime: &Path) -> Result<Value> {
    crate::private_state::recovery_quiescence_provider()?.validate(repo, value, runtime_digest, routes, publish, owner, runtime)
}
pub fn verified_child_transaction(repo: &Path, owner: Option<&[u8]>, transaction: &[u8], indexed: &Value, runtime: &Path) -> Result<String> {
    let marker = marker_value(transaction)?;
    let operation = s(get(&marker, "operation_id"))?;
    let (root, bank, _, _) = state::paths(repo, Some(operation), runtime)?;
    let directory = operation_directory(&root, &bank, operation, runtime)?;
    bank_observation::markers(&bank, owner, Some(transaction))?;
    let journal = read(&directory.join("journal.json"))?;
    if !uuid_valid(get(&journal, "proposal_id")) { return Err(err("Child recovery marker lacks its exact typed source-bound parent journal")); }
    let proposal = read(&directory.join("proposals").join(format!("{}.json", s(get(&journal, "proposal_id"))?)))?;
    if get(&journal, "schema").as_str() != Some("agent-memory/layout-recovery-journal-v1")
        || get(&journal, "operation_id").as_str() != Some(operation) || !same(get(&journal, "marker"), &marker)?
        || get(&proposal, "schema").as_str() != Some("agent-memory/layout-child-restore-v1")
        || get(&proposal, "operation_id").as_str() != Some(operation) || hash(&proposal)? != *get(&journal, "proposal_sha256")
        || get(&proposal, "manifest_sha256") != get(&journal, "manifest_sha256")
        || get(&proposal, "parent_operation_id") != get(&journal, "parent_operation_id")
        || get(&proposal, "parent_receipt_sha256") != get(&journal, "parent_receipt_sha256") {
        return Err(err("Child recovery marker lacks its exact typed source-bound parent journal"));
    }
    let parent = s(get(&proposal, "parent_operation_id"))?;
    let row = get(indexed, "operations").get(parent).ok_or_else(|| err("Child recovery cannot infer an absent or foreign parent operation"))?;
    if get(row, "project_id") != get(&marker, "project_id") { return Err(err("Child recovery cannot infer an absent or foreign parent operation")); }
    let (parent_directory, head, parent_ledger) = ledger(&root, parent, row, runtime)?;
    let (parent_manifest, _) = evidence(&parent_directory, &parent_ledger, row)?;
    let receipt = read(&parent_directory.join("receipt.json"))?;
    if hash(&receipt)? != *get(&proposal, "parent_receipt_sha256") || get(&receipt, "verified").as_bool() != Some(true)
        || hash(&head)? != *get(&proposal, "parent_ledger_head_sha256") {
        return Err(err("Child recovery parent receipt or ledger dependency changed"));
    }
    let manifest = read(&directory.join("manifest.json"))?;
    if hash(&manifest)? != *get(&proposal, "manifest_sha256") || !same(get(&proposal, "scope_context"), get(get(&manifest, "identity"), "scope_context"))? {
        return Err(err("Child recovery identity evidence is missing or changed"));
    }
    let router = project_identity::layout_observation_provider()?.read_source(&bank, Path::new("MEMORY.md"), SOURCE_BOUND)?;
    let identity = project_identity::RawIdentityEvidence::observe(&root, &router, get(&parent_manifest, "identity").get("identity_review"))?;
    let context = scope_binding::raw_recovery_context(&root, &identity, owner, Some(transaction))?;
    if !same(&context, get(&proposal, "scope_context"))? || get(&context, "project_id") != get(&marker, "project_id") {
        return Err(err("Owned child restore raw identity differs from its exact reviewed parent scope"));
    }
    bank_observation::markers(&bank, owner, Some(transaction))?;
    Ok(parent.to_owned())
}
pub fn prospective_token(repo: &Path, manifest: &Value, pending: &Value, predecessor: Option<&VerifiedHold>,
    owner: Option<&[u8]>, runtime: &Path, binary: &Path) -> Result<Value> {
    let (root, bank, _, _) = state::paths(repo, None, runtime)?;
    if get(manifest, "observation_complete").as_bool() != Some(true) || get(manifest, "repo_root").as_str() != Some(text(&root)?)
        || get(manifest, "bank_path").as_str() != Some(text(&bank)?) || !uuid_valid(get(manifest, "operation_id"))
        || get(get(get(manifest, "identity"), "scope_context"), "project_id") != get(manifest, "project_id") {
        return Err(err("Recovery reservation requires the actual complete independently bound raw manifest"));
    }
    let index = state::index(&root, runtime)?;
    if pairs(get(&index, "operations"))?.iter().any(|(_, row)| member(get(row, "state"), &["active", "interrupted"])) {
        return Err(err("A prospective repair cannot omit a competing private recovery frontier"));
    }
    if let Some(predecessor) = predecessor {
        let actual = rolled_back_predecessor(&root, owner, runtime, binary)?;
        if !same_hold(actual.as_ref(), Some(predecessor))? { return Err(err("A prospective continuation must retain the exact checked rollback frontier")); }
    } else if marker_bytes(&bank, MARKER, None)?.is_some() || marker_bytes(&bank, RECOVERY, None)?.is_some() {
        return Err(err("A prospective repair cannot replace an unknown transaction or sentinel"));
    }
    let mut operations = Vec::new();
    let mut heads = Vec::new();
    let mut references = Value::Object(Vec::new());
    for (operation, row) in sorted_rows(get(&index, "operations"))? {
        if get(row, "state").as_str() == Some("cleaned") { continue; }
        let (directory, head, current) = ledger(&root, operation, row, runtime)?;
        evidence(&directory, &current, row)?;
        if get(row, "project_id") != get(manifest, "project_id") || (get(row, "state").as_str() != Some("rolled-back")
            && array(get(&current, "units"))?.iter().any(|unit| get(unit, "state").as_str() == Some("pending-admission"))) {
            return Err(err("Prior private recovery evidence is foreign, unresolved or falsely terminal"));
        }
        if let Some(mapping) = read_optional(&directory.join("reference-map.json"))?.filter(truthy) {
            put(&mut references, operation, mapping)?;
        }
        operations.push(string(operation));
        heads.push(object([("operation_id", string(operation)), ("generation", get(&head, "generation").clone()), ("sha256", get(&head, "sha256").clone())]));
    }
    let operation = s(get(manifest, "operation_id"))?;
    if get(&index, "operations").get(operation).is_some() { return Err(err("A new raw reservation cannot reuse an existing recovery operation")); }
    operations.push(string(operation));
    operations.sort_unstable_by(|a, b| a.as_str().cmp(&b.as_str()));
    heads.push(object([("operation_id", string(operation)), ("generation", Value::Int(0.into())), ("sha256", string("f".repeat(64)))]));
    heads.sort_unstable_by(|a, b| get(a, "operation_id").as_str().cmp(&get(b, "operation_id").as_str()));
    let mut units = Vec::new();
    for unit in array(pending)? {
        if unit.get("state").is_none() || get(unit, "state").as_str() == Some("pending-admission") { units.push(get(unit, "unit_id").clone()); }
    }
    for row in array(&inherited_pending_rows(&root, runtime)?)? { units.push(get(row, "unit_id").clone()); }
    units.sort_unstable_by(|a, b| a.as_str().cmp(&b.as_str()));
    let complete = units.is_empty();
    Ok(object([("bank_path", string(text(&bank)?)), ("project_id", get(manifest, "project_id").clone()),
        ("operation_ids", Value::Array(operations)), ("ledger_heads", Value::Array(heads)), ("frontier_digest", string("f".repeat(64))),
        ("coverage_complete", Value::Bool(complete)), ("unit_ids", Value::Array(units)), ("reference_map_sha256", hash(&references)?),
        ("review_digest", Value::Null), ("index_sha256", string("f".repeat(64))),
        ("hold_sha256", if complete { Value::Null } else { string("f".repeat(64)) }),
        ("sentinel_sha256", if complete { Value::Null } else { string("f".repeat(64)) })]))
}
pub fn successor_token(token: &Value, pending_ids: &Value, operation: &str, generation_change: bool) -> Result<Value> {
    let mut result = token.clone();
    let mut units = array(pending_ids)?.to_vec();
    units.sort_unstable_by(|a, b| a.as_str().cmp(&b.as_str()));
    let complete = units.is_empty();
    put(&mut result, "unit_ids", Value::Array(units))?;
    put(&mut result, "coverage_complete", Value::Bool(complete))?;
    if generation_change {
        let Some(Value::Array(heads)) = result.get_mut("ledger_heads") else { return Err(err("Recovery token ledger heads are missing")); };
        for head in heads {
            if get(head, "operation_id").as_str() == Some(operation) {
                let generation = n(get(head, "generation"))?.checked_add(1).ok_or_else(|| err("Recovery ledger generation overflow"))?;
                put(head, "generation", Value::Int(generation.into()))?;
                put(head, "sha256", string("f".repeat(64)))?;
            }
        }
    }
    if complete { put(&mut result, "hold_sha256", Value::Null)?; put(&mut result, "sentinel_sha256", Value::Null)?; }
    put(&mut result, "review_digest", Value::Null)?;
    Ok(result)
}

/// Runtime locations are injected by the composition root, never guessed from
/// a source tree or accepted from a caller's JSON proof.
pub struct FrontierProvider {
    runtime_root: fn() -> Result<PathBuf>,
    executing_binary: fn() -> Result<PathBuf>,
}
impl FrontierProvider {
    pub fn new(runtime_root: fn() -> Result<PathBuf>, executing_binary: fn() -> Result<PathBuf>) -> Self {
        Self { runtime_root, executing_binary }
    }
}
impl writes::RecoveryFrontier for FrontierProvider {
    fn capture_scope(&self, repo: &Path) -> Result<Value> {
        capture_scope(repo, &(self.runtime_root)()?, &(self.executing_binary)()?)
    }
    fn observe(&self, repo: &Path) -> Result<Value> { observe(repo, None, None, &(self.runtime_root)()?) }
    fn observe_owned(&self, repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>) -> Result<Value> {
        observe(repo, owner, transaction, &(self.runtime_root)()?)
    }
    fn verified_hold(&self, repo: &Path, owner: Option<&[u8]>) -> Result<Vec<u8>> {
        self.verified_hold_for_runtime(repo, owner, &(self.runtime_root)()?)
    }
    fn check(&self, frontier: &Value, repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>) -> Result<()> {
        check(frontier, repo, owner, transaction, &(self.runtime_root)()?).map(|_| ())
    }
    fn require_scope(&self, repo: &Path, project: &Value, expected: Option<&Value>) -> Result<Value> {
        let project = match project {
            Value::Null => None,
            Value::String(project) => Some(project.as_str()),
            _ => return Err(err("Recovery scope project must be an explicit identity or null")),
        };
        require_scope(repo, project, scope_binding::RequireOptions { expected, ..Default::default() },
            &(self.runtime_root)()?, &(self.executing_binary)()?)
    }
    fn observe_bank(&self, repo: &Path, bank: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>, require_identity: bool, runtime_root: &Path) -> Result<Value> {
        observe_bank(repo, bank, owner, transaction, require_identity, runtime_root)
    }
    fn ledger(&self, repo: &Path, operation: &str, row: &Value, bank: &Path, runtime: &Path) -> Result<(PathBuf, Value, Value)> {
        ledger_at_bank(repo, operation, row, bank, runtime)
    }
    fn evidence(&self, directory: &Path, current: &Value, row: &Value, _runtime: &Path) -> Result<(Value, Value)> {
        evidence(directory, current, row)
    }
    fn validate_quiescence(&self, repo: &Path, value: &Value, runtime_digest: Option<&str>, routes: &[String], publish: bool, runtime: &Path) -> Result<Value> {
        validate_quiescence(repo, value, runtime_digest, routes, publish, None, runtime)
    }
    fn verified_hold_for_runtime(&self, repo: &Path, owner: Option<&[u8]>, runtime: &Path) -> Result<Vec<u8>> {
        verified_hold(repo, owner, None, runtime, &(self.executing_binary)()?)?
            .map(|held| held.marker().to_vec()).ok_or_else(|| err("Recovery hold is absent"))
    }
    fn verified_enrollment_hold_at_bank(&self, root: &Path, bank: &Path, runtime: &Path) -> Result<Option<VerifiedHold>> {
        verified_hold_at_bank(root, bank, None, None, runtime, &(self.executing_binary)()?)
    }
    fn observe_enrollment_bank(&self, root: &Path, bank: &Path, runtime: &Path) -> Result<Value> {
        observe_bank(root, bank, None, None, true, runtime)
    }
    fn observe_source_markers(&self) -> Result<Box<dyn scope_binding::SourceMarkerObservation>> { Ok(Box::new(observe_source_markers())) }
}
impl bank_contract::RecoveryCoverage for FrontierProvider {
    fn public_coverage(&self, frontier: &Value) -> Result<Value> { public_coverage(frontier) }
}
