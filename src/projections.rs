//! Disposable canonical-reference projections; never independent memory bodies.
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, uuid_valid};
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::{bank_writes as writes, scope_binding};
use std::collections::BTreeSet;
use std::path::Path;

pub const SCHEMA: &str = "agent-memory/reference-projection-v1";
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_BUILD_INPUT: usize = 65536;

/// Native reference-only validation consumed by diagnostics.
pub struct Projections;
impl crate::diagnostics::Projections for Projections {
    fn max_bytes(&self) -> usize {
        MAX_BYTES
    }
    fn value(&self, data: &Value) -> Result<Value> {
        value(data).cloned()
    }
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
pub(crate) fn fields(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|rows| {
        rows.len() == expected.len() && rows.iter().all(|(key, _)| expected.contains(&key.as_str()))
    })
}
pub(crate) fn sha_valid(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
pub fn value(projection: &Value) -> Result<&Value> {
    let hashes = get(projection, "source_hashes").as_object();
    let records = get(projection, "records").as_array();
    if !fields(
        projection,
        &["schema", "project_id", "source_hashes", "records"],
    ) || get(projection, "schema").as_str() != Some(SCHEMA)
        || !uuid_valid(get(projection, "project_id"))
        || !hashes.is_some_and(|rows| {
            !rows.is_empty() && rows.len() <= 8192 && rows.iter().all(|(_, sha)| sha_valid(sha))
        })
        || !records.is_some_and(|rows| rows.len() <= 64)
    {
        return Err(err(
            "Projection requires explicit project, complete source hashes and bounded references only",
        ));
    }
    let mut seen = BTreeSet::new();
    for record in records.into_iter().flatten() {
        let version = match get(record, "version") {
            Value::Int(n) => n.as_str(),
            _ => "",
        };
        let identity = get(record, "id").as_str().unwrap_or_default();
        if !fields(record, &["id", "version", "path"])
            || !uuid_valid(get(record, "id"))
            || version.is_empty()
            || version.starts_with('-')
            || version == "0"
            || !get(record, "path")
                .as_str()
                .is_some_and(|path| get(projection, "source_hashes").get(path).is_some())
            || !seen.insert((identity, version))
        {
            return Err(err(
                "Projection records must be exact canonical ID/version/path references",
            ));
        }
    }
    if encoded(projection)?.len() > MAX_BYTES {
        return Err(err("Projection exceeds its bounded representation"));
    }
    Ok(projection)
}
pub fn read_input(path: &Path, maximum: usize) -> Result<Value> {
    let path = crate::fs::canonical_path(path, true)?;
    let directory = path
        .parent()
        .ok_or_else(|| err("Projection input requires a file path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Projection input requires a file path"))?;
    let raw = crate::fs::read_source(directory, Path::new(name), maximum)
        .map_err(|_| err("Projection input exceeds its bounded reader"))?
        .bytes;
    crate::developer_identity::load_with_limit(&raw, maximum)
}
pub fn build(repo: &Path, request: &Value) -> Result<Value> {
    if request.as_object().is_none() || !uuid_valid(get(request, "project_id")) {
        return Err(err("Projection build requires explicit project selection"));
    }
    let (root, _) = writes::bank_path(repo)?;
    let project = get(request, "project_id");
    let context = scope_binding::require(
        &root,
        project.as_str(),
        scope_binding::RequireOptions::default(),
    )?;
    let (_, before) = writes::snapshot(&root, None)?;
    let guarded = crate::inactive::filter_projection(&root, request)?;
    scope_binding::require(
        &root,
        project.as_str(),
        scope_binding::RequireOptions {
            expected: Some(&context),
            ..Default::default()
        },
    )?;
    if writes::snapshot(&root, None)?.1 != before {
        return Err(err(
            "Bank changed during projection build; rebuild from canonical input",
        ));
    }
    let projection = object([
        ("schema", string(SCHEMA)),
        ("project_id", project.clone()),
        ("source_hashes", before),
        ("records", get(&guarded, "remaining_references").clone()),
    ]);
    value(&projection)?;
    Ok(object([
        ("schema", string("agent-memory/projection-result-v1")),
        ("operation", string("build")),
        ("projection", projection),
        ("discarded", get(&guarded, "discarded").clone()),
        ("audit", get(&guarded, "audit").clone()),
        ("canonical_writes_performed", Value::Bool(false)),
        ("audit_writes_performed", Value::Bool(true)),
        ("admission_verified", Value::Bool(false)),
        ("current_guidance_verified", Value::Bool(false)),
        (
            "validation_scope",
            string("canonical-reference-projection-only"),
        ),
    ]))
}
pub fn check(repo: &Path, projection: &Value) -> Result<Value> {
    value(projection)?;
    let (root, _) = writes::bank_path(repo)?;
    let project = get(projection, "project_id");
    let context = scope_binding::require(
        &root,
        project.as_str(),
        scope_binding::RequireOptions::default(),
    )?;
    let (_, observed) = writes::snapshot(&root, None)?;
    if !crate::retrieval::same(get(projection, "source_hashes"), &observed) {
        return Err(err(
            "Projection sources changed; discard and rebuild before use",
        ));
    }
    let guarded = crate::inactive::filter_projection(
        &root,
        &object([
            ("project_id", project.clone()),
            ("records", get(projection, "records").clone()),
        ]),
    )?;
    if !crate::retrieval::same(
        get(&guarded, "remaining_references"),
        get(projection, "records"),
    ) {
        return Err(err(
            "Projection contains noncanonical, inactive or deleted references",
        ));
    }
    scope_binding::require(
        &root,
        project.as_str(),
        scope_binding::RequireOptions {
            expected: Some(&context),
            ..Default::default()
        },
    )?;
    if writes::snapshot(&root, None)?.1 != observed {
        return Err(err(
            "Bank changed during projection check; discard and rebuild before use",
        ));
    }
    Ok(object([
        ("schema", string("agent-memory/projection-result-v1")),
        ("operation", string("check")),
        ("valid", Value::Bool(true)),
        ("project_id", project.clone()),
        (
            "references",
            Value::Int(
                get(projection, "records")
                    .as_array()
                    .map_or(0, |r| r.len())
                    .into(),
            ),
        ),
        ("audit", get(&guarded, "audit").clone()),
        ("canonical_writes_performed", Value::Bool(false)),
        ("audit_writes_performed", Value::Bool(true)),
        ("admission_verified", Value::Bool(false)),
        ("current_guidance_verified", Value::Bool(false)),
        (
            "validation_scope",
            string("canonical-reference-projection-only"),
        ),
    ]))
}
pub fn command_result(operation: &str, result: Result<Value>) -> CommandResult {
    match result {
        Ok(value) => CommandResult::json(value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let mut value = object([
                ("schema", string("agent-memory/projection-result-v1")),
                ("operation", string(operation)),
                ("valid", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
                (
                    "error",
                    string(
                        if crate::bank_capacity::details(&error)
                            != object(Vec::<(String, Value)>::new())
                        {
                            error.to_string()
                        } else {
                            "Invalid, stale or unauthorized projection/input; preserve canonical memory and rebuild before use".into()
                        },
                    ),
                ),
            ]);
            let _ = crate::bank_contract::merge(&mut value, &crate::bank_capacity::details(&error));
            CommandResult::json(value, 1, json::DEFAULT)
        }
    }
}
