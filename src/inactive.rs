//! Minimal inactive representations and canonical invalidation of derived input.
use crate::codec::{frontmatter, json::{self, Value}};
use crate::developer_identity::{digest, uuid_valid};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string, CommandResult};
use crate::{bank_writes as writes, records, scope_binding, tombstones};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const INACTIVE: &[&str] = &["superseded", "revoked", "expired"];
pub const MAX_INPUT: usize = 65536;
pub const INVALID_INPUT: &str = "Invalid or unsafe inactive representation, scope, canonical state or derived input; preserve original state for inspection";
fn err(message: &str) -> ContractError { ContractError::new(message) }
/// Caller input is bounded before parsing; no original body is echoed on refusal.
pub fn bounded_file(path: &Path) -> Result<Vec<u8>> {
    let canonical = crate::fs::canonical_path(path, true)?;
    let parent = canonical.parent().ok_or_else(|| err(INVALID_INPUT))?;
    let name = canonical.file_name().ok_or_else(|| err(INVALID_INPUT))?;
    crate::fs::PinnedRead::open(parent, Path::new(name), MAX_INPUT)
        .and_then(crate::fs::PinnedRead::read)
        .map(|read| read.bytes)
        .map_err(|error| match error {
            crate::fs::ReadError::SizeLimit { .. } => err("Inactive representation input exceeds its bounded reader"),
            _ => err(INVALID_INPUT),
        })
}
pub fn compact(content: &str) -> Result<Value> {
    let (kind, metadata, _) = records::parse(content, &[])?;
    let rendered = if kind == "tombstone" {
        get(&tombstones::render(&metadata)?, "content").clone()
    } else {
        if kind != "standalone-record" || !get(&metadata, "status").as_str().is_some_and(|s| INACTIVE.contains(&s)) {
            return Err(err("Compaction requires an already inactive record or exact terminal tombstone"));
        }
        let required = get(&records::MODEL, "required").as_array().ok_or_else(|| err(INVALID_INPUT))?;
        let kept = Value::Object(metadata.as_object().ok_or_else(|| err(INVALID_INPUT))?.iter().filter(|(key, _)| required.iter().any(|name| name.as_str() == Some(key.as_str())) || matches!(key.as_str(), "state-reason" | "superseded-by")).cloned().collect());
        let status = get(&metadata, "status").as_str().ok_or_else(|| err(INVALID_INPUT))?;
        let rendered = frontmatter::write(kept.as_object().ok_or_else(|| err(INVALID_INPUT))?)? + &format!("# Inactive memory\n\nThis version is {status}; it is excluded from current guidance.\n");
        records::parse(&rendered, &[])?;
        let scan = crate::repository_config::security_scan_provider()?.scan(&rendered)?;
        let reasons = get(&scan, "reasons").as_array().ok_or_else(|| err(INVALID_INPUT))?;
        if !reasons.is_empty() { return Err(err(INVALID_INPUT)); }
        string(rendered)
    };
    Ok(object([("schema", string("agent-memory/inactive-result-v1")), ("operation", string("compact")), ("format", string(kind)), ("content", rendered), ("writes_performed", Value::Bool(false)), ("lifecycle_change_performed", Value::Bool(false)), ("validation_scope", string("safe-representation-only"))]))
}
fn equal(a: &Value, b: &Value) -> Result<bool> { Ok(json::encode(a, json::SORTED_COMPACT_ASCII)? == json::encode(b, json::SORTED_COMPACT_ASCII)?) }
pub fn filter_projection(repo: &Path, request: &Value) -> Result<Value> {
    let fields = ["project_id", "records"];
    if records::fields(request, &fields, &fields).is_err() || !uuid_valid(get(request, "project_id")) || !get(request, "records").as_array().is_some_and(|rows| rows.len() <= 64) {
        return Err(err("Derived input needs an explicit project and at most 64 versioned record references"));
    }
    let rows = get(request, "records").as_array().ok_or_else(|| err(INVALID_INPUT))?;
    let mut seen = BTreeSet::new();
    for item in rows {
        if item.as_object().is_none() || !uuid_valid(get(item, "id")) || !get(item, "version").as_int().is_some_and(|n| !n.as_str().starts_with('-') && n.as_str() != "0") {
            return Err(err("Derived records need unique stable IDs and positive versions"));
        }
        let key = (get(item, "id").as_str().ok_or_else(|| err(INVALID_INPUT))?, json::encode(get(item, "version"), json::DEFAULT)?);
        if !seen.insert(key) { return Err(err("Derived records need unique stable IDs and positive versions")); }
    }
    let (root, bank) = writes::bank_path(repo)?;
    let project_id = get(request, "project_id").as_str().ok_or_else(|| err(INVALID_INPUT))?;
    let context = scope_binding::require(&root, Some(project_id), scope_binding::RequireOptions::default())?;
    let (checked, observed) = writes::snapshot(&root, None)?;
    let (project, deletion_hashes, deleted) = tombstones::history(&root)?;
    if !equal(&deletion_hashes, &observed)? || project != *get(request, "project_id") { return Err(err("Canonical bank changed before derived-input invalidation")); }
    let mut canonical = BTreeMap::new();
    for entry in get(&checked, "entries").as_array().into_iter().flatten() {
        if get(entry, "format").as_str() != Some("standalone-record") { continue; }
        let path = get(entry, "path").as_str().ok_or_else(|| err(INVALID_INPUT))?;
        let raw = crate::fs::PinnedRead::open(&bank, Path::new(path), MAX_INPUT).and_then(crate::fs::PinnedRead::read).map_err(|_| err(INVALID_INPUT))?.bytes;
        if digest(Some(&raw)).as_deref() != get(&observed, path).as_str() { return Err(err("Canonical source changed during derived-input invalidation")); }
        let content = std::str::from_utf8(&raw).map_err(|_| err(INVALID_INPUT))?;
        let (_, metadata, _) = records::parse(content, &[])?;
        let id = get(&metadata, "id").as_str().ok_or_else(|| err(INVALID_INPUT))?.to_owned();
        let version = json::encode(get(&metadata, "version"), json::DEFAULT)?;
        canonical.insert((id, version), (metadata, path.to_owned()));
    }
    let mut remaining = Vec::new();
    let mut record_ids = BTreeSet::new();
    for item in rows {
        let id = get(item, "id").as_str().ok_or_else(|| err(INVALID_INPUT))?;
        record_ids.insert(id.to_owned());
        if deleted.contains_key(id) { continue; }
        if let Some((metadata, path)) = canonical.get(&(id.to_owned(), json::encode(get(item, "version"), json::DEFAULT)?)) {
            if get(metadata, "status").as_str() == Some("active") {
                remaining.push(object([("id", string(id)), ("version", get(item, "version").clone()), ("path", string(path))]));
            }
        }
    }
    scope_binding::require(&root, project.as_str(), scope_binding::RequireOptions { expected: Some(&context), ..scope_binding::RequireOptions::default() })?;
    if !equal(&writes::snapshot(&root, None)?.1, &observed)? { return Err(err("Canonical bank changed; repeat invalidation before rebuilding or consolidation")); }
    let operation_id = crate::host::uuid4()?;
    let audit = writes::security_provider()?.audit_records(&root, &operation_id, "retrieval", "checked", &context, &["external"], &[], &record_ids.into_iter().collect::<Vec<_>>(), "policy-check")?;
    Ok(object([("schema", string("agent-memory/inactive-result-v1")), ("operation", string("filter")), ("project_id", project), ("discarded", Value::Int((rows.len() - remaining.len()).into())), ("remaining_references", Value::Array(remaining)), ("audit", audit), ("cached_payloads_returned", Value::Bool(false)), ("current_guidance_verified", Value::Bool(false)), ("canonical_writes_performed", Value::Bool(false)), ("audit_writes_performed", Value::Bool(true)), ("validation_scope", string("erasure-invalidation-only"))]))
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let details = crate::bank_capacity::details(&error);
            let mut value = object([("schema", string("agent-memory/inactive-result-v1")), ("operation", string(operation)), ("error", string(if details.as_object().is_some_and(|fields| !fields.is_empty()) { error.to_string() } else { INVALID_INPUT.to_owned() })), ("verified", Value::Bool(false))]);
            if let Value::Object(fields) = &mut value {
                if operation == "compact" { fields.push(("writes_performed".to_owned(), Value::Bool(false))); }
                fields.push(("canonical_writes_performed".to_owned(), Value::Bool(false)));
                if let Value::Object(details) = details { fields.extend(details); }
            }
            (value, 1, json::DEFAULT)
        }
    };
    match json::encode(&value, style) { Ok(stdout) => CommandResult { value: Some(value), exit_code, stdout: stdout + "\n", stderr: String::new() }, Err(_) => crate::cli::refusal("inactive-records", INVALID_INPUT) }
}
