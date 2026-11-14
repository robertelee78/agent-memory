//! Receipt and journal codecs (baseline `encoded`, `load`, `read_receipt`,
//! `read_journal`).
use super::filesystem::{lexists, split};
use super::{
    Environment, JOURNAL_SCHEMA, LEGACY_RECEIPT_SCHEMAS, LEGACY_TARGET_KINDS, MAX_RECORD,
    RECEIPT_SCHEMA, Result, TARGET_KINDS, absolute, require,
};
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::ContractError;
use crate::fs;
use crate::lifecycle::{get, text};
use std::path::{Path, PathBuf};

/// Baseline `encoded`: `json.dumps(value, sort_keys=True, indent=2) + "\n"`.
pub fn encoded(value: &Value) -> Result<Vec<u8>> {
    let mut text = json::encode(value, json::SORTED_INDENT2_ASCII)?;
    text.push('\n');
    Ok(text.into_bytes())
}

/// Baseline `load`: a trusted, unlinked, bounded record decoded with unique keys.
pub fn load(path: &Path) -> Result<Value> {
    fs::trust::record(path)?;
    let refused = || {
        ContractError::new(format!(
            "Missing, linked or oversized installation record: {}",
            path.display()
        ))
    };
    let (parent, name) = split(path)?;
    let observed = fs::read_source(parent, Path::new(name), MAX_RECORD).map_err(|_| refused())?;
    Ok(json::decode_bytes(
        &observed.bytes,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Accept,
            max_bytes: MAX_RECORD,
            ..DecodePolicy::default()
        },
    )?)
}

fn is_absolute_canonical(env: &Environment, value: &Value) -> bool {
    value.as_str().is_some_and(|path| {
        absolute(env, Path::new(path)).is_ok_and(|resolved| resolved == Path::new(path))
    })
}

/// Rust-era receipts carry receipt-v3 and their compiled `source.target`.
pub fn receipt_is_rust_era(receipt: &Value) -> bool {
    text(receipt, "schema") == Some(RECEIPT_SCHEMA)
}

/// Baseline `read_receipt`; v1/v2 are Python-era adoption inputs, v3 is Rust.
pub fn read_receipt(env: &Environment, state: &Path) -> Result<Option<Value>> {
    fs::trust::state(state)?;
    let path = state.join("receipt.json");
    if !lexists(&path)? {
        return Ok(None);
    }
    let value = load(&path)?;
    let schema = text(&value, "schema");
    let known = schema
        .is_some_and(|schema| schema == RECEIPT_SCHEMA || LEGACY_RECEIPT_SCHEMAS.contains(&schema));
    require(
        value.as_object().is_some()
            && known
            && get(&value, "verified") == &Value::Bool(true)
            && text(&value, "installation_id").is_some()
            && get(&value, "targets").as_array().is_some()
            && get(&value, "source").as_object().is_some(),
        format!("Invalid installation ownership receipt: {}", path.display()),
    )?;
    if get(&value, "uninstalled") == &Value::Bool(true) {
        return Ok(None);
    }
    let kinds: &[&str] = if receipt_is_rust_era(&value) {
        &TARGET_KINDS
    } else {
        &LEGACY_TARGET_KINDS
    };
    for item in get(&value, "targets").as_array().unwrap_or(&[]) {
        require(
            item.as_object().is_some()
                && text(item, "kind").is_some_and(|kind| kinds.contains(&kind))
                && is_absolute_canonical(env, get(item, "path"))
                && item.get("identity").is_some(),
            "Malformed target ownership receipt",
        )?;
    }
    crate::omp_installation::validate_receipt(&value)?;
    Ok(Some(value))
}

/// Baseline `read_journal`: validate recovery metadata without writing.
pub fn read_journal(env: &Environment, state: &Path) -> Result<Option<Value>> {
    fs::trust::state(state)?;
    let path = state.join("journal.json");
    if !lexists(&path)? {
        return Ok(None);
    }
    let journal = load(&path)?;
    let transaction = text(&journal, "transaction");
    require(
        journal.as_object().is_some()
            && text(&journal, "schema") == Some(JOURNAL_SCHEMA)
            && matches!(
                text(&journal, "status"),
                Some("staging" | "activating" | "complete" | "rolled-back" | "rollback-conflict")
            )
            && get(&journal, "actions").as_array().is_some()
            && get(&journal, "receipt").as_object().is_some()
            && transaction.is_some(),
        "Unsupported lifecycle journal",
    )?;
    let transaction = transaction.unwrap_or_default();
    for (index, action) in get(&journal, "actions")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        let malformed = || ContractError::new("Malformed lifecycle journal target or staging path");
        let path = text(action, "path").ok_or_else(malformed)?;
        let parent = Path::new(path).parent().ok_or_else(malformed)?;
        let stage = parent.join(format!(".agent-memory-{transaction}-{index}"));
        let backup = parent.join(format!(".agent-memory-backup-{transaction}-{index}"));
        let backup_ok = match get(action, "backup") {
            Value::Null => true,
            value => value.as_str().map(PathBuf::from) == Some(backup),
        };
        require(
            action.as_object().is_some()
                && action.get("before").is_some()
                && action.get("after").is_some()
                && is_absolute_canonical(env, get(action, "path"))
                && text(action, "stage").map(PathBuf::from) == Some(stage)
                && backup_ok,
            "Malformed lifecycle journal target or staging path",
        )?;
        // Finish or roll back only steps whose kind this runtime knows.
        require(
            text(action, "kind").is_some_and(|kind| TARGET_KINDS.contains(&kind)),
            "Lifecycle journal step kind is unknown; preserve the journal and its retained stages for reconciliation",
        )?;
    }
    Ok(Some(journal))
}
