//! Thin native Claude mapping onto the shared source-observation protocol.
//!
//! Baseline `claude_turns.py`; every source operation is `turn_sources`.
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, text};
use crate::turn_sources;
use std::path::Path;

const FIELDS: [&str; 6] = [
    "session",
    "prompt",
    "observation_id",
    "source_sha256",
    "outcome",
    "assessment",
];

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn hex(value: u8, upper: bool) -> bool {
    value.is_ascii_digit()
        || (b'a'..=b'f').contains(&value)
        || (upper && (b'A'..=b'F').contains(&value))
}
/// `[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}` (full match).
fn observation_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                hex(b, true)
            }
        })
}
/// `[0-9a-f]{64}` (full match).
fn prompt_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| hex(b, false))
}
/// Exactly the six assessment fields, optionally with `publication_request`.
fn exact_fields(value: &Value) -> bool {
    value.as_object().is_some_and(|pairs| {
        FIELDS.iter().all(|name| value.get(name).is_some())
            && (pairs.len() == FIELDS.len()
                || (pairs.len() == FIELDS.len() + 1 && value.get("publication_request").is_some()))
    })
}
fn identifier<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    text(value, key).ok_or_else(|| err("Invalid bounded source identifier"))
}

/// Baseline `complete(repo, value)`: complete one retained Claude observation
/// with its exact verified assessment.
pub fn complete(repo: &Path, value: &Value, runtime_root: &Path) -> Result<Value> {
    if !exact_fields(value) {
        return Err(err("Use an exact native Claude observation assessment"));
    }
    let observation_id = text(value, "observation_id")
        .filter(|id| observation_uuid(id))
        .ok_or_else(|| {
            err("observation_id must be one local observation UUID, not a shell word list")
        })?;
    let source_sha256 = text(value, "source_sha256")
        .filter(|digest| prompt_digest(digest))
        .ok_or_else(|| {
            err("source_sha256 must be the exact 64-character lowercase prompt digest")
        })?;
    let session = identifier(value, "session")?;
    let prompt = identifier(value, "prompt")?;
    let observed = turn_sources::get_source(
        repo,
        "claude",
        session,
        prompt,
        observation_id,
        runtime_root,
    )?;
    if text(&observed, "source_sha256") != Some(source_sha256)
        || text(&observed, "native_source_id") != Some(prompt)
    {
        return Err(err("Claude source differs from its retained observation"));
    }
    let verified = turn_sources::verify_assessment(repo, value, observation_id, runtime_root)?;
    turn_sources::complete(
        repo,
        "claude",
        session,
        prompt,
        observation_id,
        source_sha256,
        &verified,
        runtime_root,
    )
}

/// Baseline `pending(repo, session, prompt, offset=…, legacy_offset=…,
/// enrollment_offset=…)`: the full backlog, or one native source's pending rows.
pub fn pending(
    repo: &Path,
    session: Option<&str>,
    prompt: Option<&str>,
    offsets: (usize, usize, usize),
    runtime_root: &Path,
) -> Result<Value> {
    let (offset, legacy_offset, enrollment_offset) = offsets;
    let (session, prompt) = match (session, prompt) {
        (None, None) => {
            return turn_sources::backlog(
                repo,
                Some("claude"),
                offset,
                legacy_offset,
                enrollment_offset,
                runtime_root,
            );
        }
        (Some(session), Some(prompt)) => (session, prompt),
        _ => return Err(err("Select both native session and prompt, or neither")),
    };
    if offset != 0 || legacy_offset != 0 || enrollment_offset != 0 {
        return Err(err(
            "Page offsets select the full pending inventory, not a single native source",
        ));
    }
    let state = turn_sources::inspect(repo, "claude", session, prompt, runtime_root)?;
    let mut pending = Vec::new();
    for row in get(&state, "pending").as_array().unwrap_or(&[]) {
        let mut selected = Vec::new();
        for key in ["session", "lifecycle", "observation_id", "source_sha256"] {
            let field = row
                .get(key)
                .ok_or_else(|| err("Claude pending source inventory is malformed"))?;
            selected.push((key, field.clone()));
        }
        pending.push(object(selected));
    }
    Ok(object([
        ("verified", Value::Bool(true)),
        ("pending", Value::Array(pending)),
        ("faults", get(&state, "faults").clone()),
        (
            "recovery_pending",
            state
                .get("recovery_pending")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        (
            "unidentified_input_ids",
            state
                .get("unidentified_input_ids")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        ),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}
