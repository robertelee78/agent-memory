//! Explicit finish/rollback of an interrupted operation and dead-lock recovery.
use super::plan::{
    apply, check_code, converge, historical, owned_marker, partial, read_intent, read_journal,
    receipt_value,
};
use super::{
    LOCK, MAX_FILE, Runtime, catalog, context, err, hashes, keep_record, lexists, locate,
    locations, lstat, path_text, read_record, read_regular, relative_text, require, result, same,
    sha, snapshot, validate_codec,
};
use crate::codec::json::Value;
use crate::developer_identity::{digest, encoded, load_with_limit};
use crate::error::Result;
use crate::host::Sources;
use crate::lifecycle::{get, object, string, text};
use crate::{bank_transactions as tx, scope_binding};
use std::path::Path;

pub const RECOVERY_FIELDS: &[&str] = &[
    "schema",
    "repo_root",
    "bank_root",
    "operation_id",
    "mode",
    "scope_context",
    "observed_hashes",
    "observed_modes",
    "marker_sha256",
    "journal_sha256",
    "receipt_sha256",
];
const RECOVERY_SCHEMA: &str = "agent-memory/migration-recovery-v1";

fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len() && fields.iter().all(|field| value.get(field).is_some())
    })
}

fn optional_sha(raw: Option<&[u8]>) -> Value {
    digest(raw).map_or(Value::Null, string)
}

fn optional_record_sha(path: &Path) -> Result<Value> {
    if !lexists(path)? {
        return Ok(Value::Null);
    }
    let (_, raw) = read_record(path, super::MAX_RECORD)?;
    Ok(string(sha(&raw)))
}

fn backup_path(directory: &Path) -> Result<Value> {
    Ok(string(path_text(&directory.join("backup.json"))?))
}

/// The bounded recovery observation a caller reviews before finish/rollback.
pub fn recovery_view(
    runtime: &Runtime,
    root: &Path,
    target: &Path,
    p: &Value,
    directory: &Path,
    mode: &Value,
) -> Result<Value> {
    let mode = mode
        .as_str()
        .filter(|mode| matches!(*mode, "finish" | "rollback"))
        .ok_or_else(|| err("Choose explicit migration recovery finish or rollback"))?;
    let finish = mode == "finish";
    let marker = owned_marker(target, p)?;
    let project_id = text(p, "project_id").unwrap_or_default();
    let scope = context(root, project_id, None, None, marker.as_deref())?;
    let (files, modes) = snapshot(target, None, marker.as_deref())?;
    if let Some(known) = receipt_value(directory, p)? {
        let expected = if finish { "finished" } else { "rolled-back" };
        require(
            text(&known, "outcome") == Some(expected),
            "A finished operation is immutable; use restore to preview a NEW backup-restoration transaction",
        )?;
    } else if marker.is_some() {
        let (journal, _) = read_journal(directory, p)?;
        partial(&files, &modes, p, &journal)?;
        if finish {
            check_code(runtime, p)?;
        }
    } else if !same(&hashes(&files), get(p, "source_hashes"))
        || !same(&modes, get(p, "source_modes"))
    {
        return Err(err(
            "Missing migration marker with changed bank; preserve all state for explicit inspection",
        ));
    } else if finish {
        check_code(runtime, p)?;
    }
    Ok(object([
        ("schema", string(RECOVERY_SCHEMA)),
        ("repo_root", string(path_text(root)?)),
        ("bank_root", string(relative_text(root, target)?)),
        ("operation_id", get(p, "operation_id").clone()),
        ("mode", string(mode)),
        ("scope_context", scope),
        ("observed_hashes", hashes(&files)),
        ("observed_modes", modes),
        ("marker_sha256", optional_sha(marker.as_deref())),
        (
            "journal_sha256",
            optional_record_sha(&directory.join("journal.json"))?,
        ),
        (
            "receipt_sha256",
            optional_record_sha(&directory.join("receipt.json"))?,
        ),
    ]))
}

/// Inspect, preview or execute explicit recovery of an interrupted operation.
pub fn recover(
    runtime: &Runtime,
    repo: &Path,
    mode: Option<&str>,
    operation_id: Option<&str>,
    prepared: Option<&Value>,
) -> Result<Value> {
    let (root, target) = locate(repo)?;
    let mut mode = mode.map(string);
    let mut operation = operation_id.map(string);
    let reviewed = match prepared {
        Some(prepared) => {
            let r = if prepared.as_object().is_some() {
                prepared.get("proposal").unwrap_or(prepared)
            } else {
                prepared
            };
            let matching = exact(r, RECOVERY_FIELDS)
                && text(r, "schema") == Some(RECOVERY_SCHEMA)
                && text(r, "repo_root") == Some(path_text(&root)?)
                && text(r, "bank_root") == Some(relative_text(&root, &target)?.as_str());
            require(
                matching,
                "Use the original bounded migration recovery preview for this repository",
            )?;
            operation = Some(get(r, "operation_id").clone()).filter(|v| v != &Value::Null);
            mode = Some(get(r, "mode").clone()).filter(|v| v != &Value::Null);
            Some(r)
        }
        None => None,
    };
    let operation = match operation {
        Some(operation) => operation,
        None => {
            if !lexists(&target.join(tx::MARKER))? {
                return Ok(result(
                    "recover",
                    vec![
                        ("state", string("no-visible-migration")),
                        ("verified", Value::Bool(true)),
                        ("canonical_writes_performed", Value::Bool(false)),
                        ("private_preparations_not_scanned", Value::Bool(true)),
                    ],
                ));
            }
            let (raw, _) = read_regular(&target, tx::MARKER, MAX_FILE)?;
            let marker = load_with_limit(&raw, MAX_FILE)?;
            require(
                marker.as_object().is_some()
                    && text(&marker, "schema") == Some("agent-memory/migration-marker-v1"),
                "Visible marker belongs to a different transaction; use its own recovery helper",
            )?;
            get(&marker, "operation_id").clone()
        }
    };
    let (directory, p) = read_intent(runtime, &root, &target, &operation)?;
    let Some(mode) = mode else {
        return Ok(result(
            "recover",
            vec![
                ("state", string("recovery-required")),
                ("operation_id", operation),
                (
                    "available_modes",
                    Value::Array(vec![string("finish"), string("rollback")]),
                ),
                ("verified", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
                ("backup_path", backup_path(&directory)?),
            ],
        ));
    };
    let current = recovery_view(runtime, &root, &target, &p, &directory, &mode)?;
    let Some(r) = reviewed else {
        return Ok(result(
            "recover-preview",
            vec![
                ("state", string("prepared")),
                ("proposal", current),
                ("verified", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
                ("backup_path", backup_path(&directory)?),
            ],
        ));
    };
    let reviewed_scope = get(r, "scope_context");
    scope_binding::context_value(reviewed_scope, false)?;
    require(
        same(&current, r),
        "Migration recovery observation changed; preserve new work and preview again",
    )?;
    let finish = mode.as_str() == Some("finish");
    // An interrupted private preparation has not created its canonical marker.
    // Resume the original pinned conversion under apply's own bank lock.
    if get(&current, "marker_sha256") == &Value::Null
        && get(&current, "receipt_sha256") == &Value::Null
        && finish
    {
        return apply(runtime, &p, Some(reviewed_scope));
    }
    let project_id = text(&p, "project_id").unwrap_or_default();
    let operation_id = text(&p, "operation_id").unwrap_or_default();
    let _barrier = crate::turn_sources::canonical_writer_barrier(&root, None, &runtime.root)?;
    crate::local_registry::exclusive(
        &target.join(LOCK),
        operation_id,
        "bank-migrate",
        Sources::default(),
        |owner| {
            let marker = owned_marker(&target, &p)?;
            let marker = marker.as_deref();
            let scope = context(&root, project_id, Some(reviewed_scope), Some(owner), marker)?;
            let (files, modes) = snapshot(&target, Some(owner), marker)?;
            require(
                same(&hashes(&files), get(r, "observed_hashes"))
                    && same(&modes, get(r, "observed_modes")),
                "Bank bytes/modes changed after migration recovery preview",
            )?;
            let pinned = &optional_sha(marker) == get(r, "marker_sha256")
                && &optional_record_sha(&directory.join("journal.json"))?
                    == get(r, "journal_sha256")
                && &optional_record_sha(&directory.join("receipt.json"))?
                    == get(r, "receipt_sha256");
            require(
                pinned,
                "Migration recovery marker/journal/receipt changed under lock",
            )?;
            if let Some(known) = receipt_value(&directory, &p)? {
                return historical(&directory, &root, &target, &p, &known, Some(owner), marker);
            }
            if marker.is_none() {
                let unchanged = !finish
                    && same(&hashes(&files), get(&p, "source_hashes"))
                    && same(&modes, get(&p, "source_modes"));
                require(
                    unchanged,
                    "Unfinished preparation cannot overwrite a changed source bank",
                )?;
                return cancel(runtime, &directory, &p, &files);
            }
            let (journal, backup) = read_journal(&directory, &p)?;
            let mode = if finish { "finish" } else { "rollback" };
            converge(
                runtime, &directory, &root, &target, &p, &journal, &backup, mode, owner, &scope,
            )
        },
    )
}

/// Record the rollback of a preparation that never marked the bank.
fn cancel(runtime: &Runtime, directory: &Path, p: &Value, files: &Value) -> Result<Value> {
    let measured = catalog(runtime)?;
    let source_schema = text(p, "source_schema").unwrap_or_default();
    let revalidated = get(get(&measured, "migration_catalog"), "codecs")
        .get(source_schema)
        .is_some();
    if revalidated {
        validate_codec(runtime, &measured, source_schema, files)?;
    }
    let readable = get(get(&measured, "compatibility"), "readable_bank_schemas")
        .as_array()
        .is_some_and(|schemas| {
            schemas
                .iter()
                .any(|schema| schema.as_str() == Some(source_schema))
        });
    let receipt = object([
        ("schema", string("agent-memory/migration-receipt-v1")),
        ("operation_id", get(p, "operation_id").clone()),
        ("project_id", get(p, "project_id").clone()),
        ("request_sha256", string(sha(&encoded(p)?))),
        ("outcome", string("rolled-back")),
        ("resulting_hashes", get(p, "source_hashes").clone()),
        ("resulting_modes", get(p, "source_modes").clone()),
        (
            "backup_sha256",
            optional_record_sha(&directory.join("backup.json"))?,
        ),
        ("preparation_cancelled", Value::Bool(true)),
        ("outcome_format_revalidated", Value::Bool(revalidated)),
        ("outcome_directly_readable", Value::Bool(readable)),
        (
            "readback_at",
            string(crate::local_registry::now(Sources::default())?),
        ),
        ("verified", Value::Bool(true)),
    ]);
    keep_record(&directory.join("receipt.json"), &receipt)?;
    Ok(result(
        "recover",
        vec![
            ("state", string("rolled-back")),
            ("verified", Value::Bool(true)),
            ("still_current", Value::Bool(true)),
            ("receipt", receipt),
            ("canonical_writes_performed", Value::Bool(false)),
            ("private_preparation_preserved", Value::Bool(true)),
        ],
    ))
}

/// Inspect or remove the dead owner of a migration bank lock.
pub fn recover_lock(runtime: &Runtime, repo: &Path, prepared: Option<&Value>) -> Result<Value> {
    let (root, target) = locate(repo)?;
    let lock = target.join(LOCK);
    let Some(info) = lstat(&lock)? else {
        return Ok(result(
            "recover-lock",
            vec![
                ("state", string("no-lock")),
                ("verified", Value::Bool(true)),
                ("canonical_writes_performed", Value::Bool(false)),
            ],
        ));
    };
    let owned = info.is_regular()
        && info.nlink == 1
        && info.uid == crate::host::effective_uid()
        && info.mode & 0o022 == 0;
    require(
        owned,
        "Migration lock is foreign, linked or writable by another principal; preserve it",
    )?;
    let (raw, _) = read_regular(&target, LOCK, MAX_FILE)?;
    let metadata = load_with_limit(&raw, MAX_FILE)?;
    locations(runtime, &root, &target, get(&metadata, "operation_id"))?;
    let value = crate::local_registry::recover(
        &target.join("MEMORY.md"),
        &lock,
        &["bank-migrate"],
        prepared,
        "agent-memory/migration-lock-recovery-v1",
        Sources::default(),
    )?;
    let mut fields: Vec<(&str, Value)> = super::entries(&value)
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    fields.push(("canonical_writes_performed", Value::Bool(false)));
    Ok(result("recover-lock", fields))
}
