//! Optional wake-decision evidence. Written only after the OBSERVED response
//! and only for a started, verified session-manager seal; never assessment
//! authority, never completion, never retried.
use super::coverage::{Coverage, StreamAuthority, source_refs};
use super::executor::SourceCompletion;
use super::inventory::open_value;
use super::{
    MAX_ENTRIES, MAX_METADATA, Pattern, digest, directory, entries, err, exact, int, integer,
    lexists, name, object, opt_string, read_required, same, sequence_of, sha, string, uuid,
};
use crate::codec::json::Value;
use crate::error::Result;
use crate::fs;
use crate::lifecycle::get;
use crate::turn_sources;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "agent-memory/omp-wake-decision-v1";
const KEYS: [&str; 9] = [
    "schema",
    "open_sha256",
    "attempt_sha256",
    "outcome_sha256",
    "sequence",
    "status",
    "continuation_offered",
    "human_root",
    "root_wake_sha256",
];
const PENDING_CODES: [&str; 9] = [
    "pending",
    "pending-missing-human-source",
    "pending-wake-state-unverifiable",
    "pending-already-requested-native-user",
    "pending-wake-guidance-bound",
    "pending-undeliverable-window",
    "pending-wake-unknown-after-reservation",
    "pending-wake-busy",
    "pending-wake-unavailable",
];
const DECISION_EXTRA: [&str; 4] = ["offered", "wake-not-evaluated", "not-armed", "unknown"];
const READBACK_EXTRA: [&str; 2] = ["unrecorded", "unbound"];
const MAX_ROWS: usize = 64;

fn decision_code(status: &str) -> bool {
    PENDING_CODES.contains(&status) || DECISION_EXTRA.contains(&status)
}

/// Derived only from the already checked stream coverage namespace.
pub fn wake_decision_directory(directory: &Path) -> Result<PathBuf> {
    let session = directory
        .parent()
        .ok_or_else(|| err("Wake decision namespace lacks its session"))?;
    let base = session
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| err("Wake decision namespace lacks its base"))?;
    Ok(base
        .join("omp-wake-decisions-v1")
        .join(name(session)?)
        .join(name(directory)?))
}
fn base_of(directory: &Path) -> Result<&Path> {
    directory
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| err("Wake decision namespace lacks its base"))
}
fn session_name(directory: &Path) -> Result<&str> {
    name(
        directory
            .parent()
            .ok_or_else(|| err("Wake decision namespace lacks its session"))?,
    )
}

/// `(open, attempt digest, outcome digest)` of a real verified session-manager seal.
fn seal_evidence(
    directory: &Path,
    sequence: u64,
    opened: Option<&Value>,
    attempt_digest: Option<&str>,
) -> Result<(Value, String, String)> {
    let actual_open = read_required(&directory.join("open.json"), MAX_METADATA)?;
    open_value(&actual_open, directory)?;
    if opened.is_some_and(|opened| !same(&actual_open, opened)) {
        return Err(err("Wake decision OPEN changed"));
    }
    let open_digest = digest(&actual_open)?;
    let file = format!("{sequence}.json");
    let attempt = read_required(&directory.join("attempts").join(&file), MAX_METADATA)?;
    exact(
        &attempt,
        &[
            "schema",
            "open_sha256",
            "instance_id",
            "sequence",
            "action",
            "event_ordinal",
            "disposition",
            "reason",
            "identity",
            "source_kind",
            "frame_kind",
        ],
    )?;
    integer(get(&attempt, "sequence"))?;
    integer(get(&attempt, "event_ordinal"))?;
    let text = |key| get(&attempt, key).as_str();
    if text("schema") != Some("agent-memory/omp-stream-attempt-v1")
        || text("open_sha256") != Some(open_digest.as_str())
        || text("instance_id") != Some(name(directory)?)
        || get(&attempt, "sequence").as_int().and_then(|v| v.as_u64()) != Some(sequence)
        || text("action") != Some("seal")
        || text("disposition") != Some("accepted")
        || !matches!(get(&attempt, "reason"), Value::Null)
        || text("source_kind") != Some("session-manager")
        || text("frame_kind") != Some("REQUEST")
    {
        return Err(err("Wake decision lacks a real session-manager seal"));
    }
    let actual_digest = digest(&attempt)?;
    if attempt_digest.is_some_and(|expected| expected != actual_digest) {
        return Err(err("Wake decision attempt changed"));
    }
    let start = read_required(&directory.join("started").join(&file), 4096)?;
    exact(
        &start,
        &["schema", "open_sha256", "attempt_sha256", "sequence"],
    )?;
    integer(get(&start, "sequence"))?;
    let expected = object([
        ("schema", string("agent-memory/omp-stream-started-v1")),
        ("open_sha256", string(&open_digest)),
        ("attempt_sha256", string(&actual_digest)),
        ("sequence", int(sequence)),
    ]);
    if !same(&start, &expected) {
        return Err(err("Wake decision lacks a bound source start"));
    }
    let outcome = read_required(&directory.join("outcomes").join(&file), MAX_METADATA)?;
    exact(
        &outcome,
        &[
            "schema",
            "open_sha256",
            "attempt_sha256",
            "sequence",
            "outcome",
            "code",
            "source_references",
        ],
    )?;
    integer(get(&outcome, "sequence"))?;
    source_refs(get(&outcome, "source_references"))?;
    let text = |key| get(&outcome, key).as_str();
    if text("schema") != Some("agent-memory/omp-stream-outcome-v1")
        || text("open_sha256") != Some(open_digest.as_str())
        || text("attempt_sha256") != Some(actual_digest.as_str())
        || get(&outcome, "sequence").as_int().and_then(|v| v.as_u64()) != Some(sequence)
        || text("outcome") != Some("verified")
        || text("code") != Some("verified")
    {
        return Err(err("Wake decision lacks a verified source outcome"));
    }
    let outcome_digest = digest(&outcome)?;
    Ok((actual_open, actual_digest, outcome_digest))
}

fn root_path(directory: &Path, root: &str) -> Result<PathBuf> {
    Ok(base_of(directory)?
        .join("omp-source-v1")
        .join(session_name(directory)?)
        .join("root-wakes")
        .join(format!("{root}.json")))
}

fn wake_root(directory: &Path, opened: &Value, path: &Path) -> Result<Value> {
    let root = uuid(&string(super::stem(path)?))?.to_owned();
    if path != root_path(directory, &root)? {
        return Err(err("Wake decision root namespace differs"));
    }
    let value = read_required(path, 4096)?;
    exact(
        &value,
        &[
            "schema",
            "session",
            "human_root",
            "native_user_entry",
            "request_id",
            "status",
        ],
    )?;
    if get(&value, "schema").as_str() != Some("agent-memory/omp-root-wake-v1")
        || get(&value, "session") != get(opened, "session")
        || get(&value, "human_root").as_str() != Some(root.as_str())
        || get(&value, "status").as_str() != Some("requested")
    {
        return Err(err("Wake decision root binding differs"));
    }
    turn_sources::bounded(get(&value, "native_user_entry"), 512)?;
    uuid(get(&value, "request_id"))?;
    Ok(value)
}

/// `retained` or `unavailable` (an existing different record is never rewritten).
pub(crate) fn retain_wake_decision(
    coverage: &Coverage,
    sequence: u64,
    attempt_digest: &str,
    request: &Value,
    result: &Value,
    completion: Option<&SourceCompletion<'_>>,
) -> Result<&'static str> {
    let (opened, attempt_digest, outcome_digest) = seal_evidence(
        &coverage.directory,
        sequence,
        Some(&coverage.open),
        Some(attempt_digest),
    )?;
    let mut status = "unknown".to_owned();
    let mut root = None;
    let mut root_digest = None;
    let returned_status = get(result, "status").as_str();
    let cont = result.get("continue");
    if get(get(request, "payload"), "request_wake") != &Value::Bool(true) {
        status = "not-armed".into();
    } else if cont == Some(&Value::Bool(true)) {
        let path = completion.and_then(SourceCompletion::offered_root_wake_path);
        let request_id = get(result, "request_id")
            .as_str()
            .filter(|id| super::canonical_uuid(id));
        if let (Some(path), Some(request_id)) = (path, request_id) {
            let actual = wake_root(&coverage.directory, &opened, &path)?;
            if get(&actual, "request_id").as_str() != Some(request_id) {
                return Err(err("Wake decision request differs from retained root"));
            }
            status = "offered".into();
            root = get(&actual, "human_root").as_str().map(str::to_owned);
            root_digest = Some(digest(&actual)?);
        }
    } else if let Some(returned) = returned_status {
        if cont == Some(&Value::Bool(false)) && PENDING_CODES.contains(&returned) {
            status = returned.to_owned();
        } else if cont.is_none() && matches!(returned, "observed" | "pending") {
            status = "wake-not-evaluated".into();
        }
    }
    let value = object([
        ("schema", string(SCHEMA)),
        ("open_sha256", string(digest(&opened)?)),
        ("attempt_sha256", string(attempt_digest)),
        ("outcome_sha256", string(outcome_digest)),
        ("sequence", int(sequence)),
        ("continuation_offered", Value::Bool(status == "offered")),
        ("status", string(&status)),
        ("human_root", opt_string(root.as_deref())),
        ("root_wake_sha256", opt_string(root_digest.as_deref())),
    ]);
    let path = wake_decision_directory(&coverage.directory)?.join(format!("{sequence}.json"));
    turn_sources::private_mkdir(super::parent(&path)?)?;
    if !turn_sources::exclusive(&path, &value, 4096)? {
        let existing = turn_sources::read_bytes(&path, 4096)?;
        if existing != Some(turn_sources::encoded(&value)?) {
            return Ok("unavailable");
        }
    }
    Ok("retained")
}

fn checked_wake_decision(
    path: &Path,
    directory: &Path,
    sequence: u64,
    evidence: &(Value, String, String),
) -> Result<String> {
    let value = read_required(path, 4096)?;
    exact(&value, &KEYS)?;
    let (opened, attempt_digest, outcome_digest) = evidence;
    integer(get(&value, "sequence"))?;
    let status = get(&value, "status").as_str();
    let offered = get(&value, "continuation_offered").as_bool();
    let text = |key| get(&value, key).as_str();
    if text("schema") != Some(SCHEMA)
        || get(&value, "sequence").as_int().and_then(|v| v.as_u64()) != Some(sequence)
        || text("open_sha256") != Some(digest(opened)?.as_str())
        || text("attempt_sha256") != Some(attempt_digest.as_str())
        || text("outcome_sha256") != Some(outcome_digest.as_str())
        || !status.is_some_and(decision_code)
        || offered.is_none()
        || offered != Some(status == Some("offered"))
    {
        return Err(err("Unbound optional wake decision"));
    }
    if status == Some("offered") {
        let root = uuid(get(&value, "human_root"))?;
        let actual = wake_root(directory, opened, &root_path(directory, root)?)?;
        if digest(&actual)? != sha(get(&value, "root_wake_sha256"))? {
            return Err(err("Wake decision root digest differs"));
        }
    } else if !matches!(get(&value, "human_root"), Value::Null)
        || !matches!(get(&value, "root_wake_sha256"), Value::Null)
    {
        return Err(err("Non-offered wake decision claims a root"));
    }
    Ok(status.unwrap_or_default().to_owned())
}

/// The fixed diagnostic summary shape.
pub fn wake_decision_summary(available: bool) -> Value {
    let mut codes: Vec<&str> = PENDING_CODES
        .iter()
        .chain(DECISION_EXTRA.iter())
        .chain(READBACK_EXTRA.iter())
        .copied()
        .collect();
    codes.sort_unstable();
    object([
        (
            "availability",
            string(if available {
                "available"
            } else {
                "unavailable-or-unverifiable"
            }),
        ),
        (
            "counts",
            Value::Object(codes.into_iter().map(|c| (c.to_owned(), int(0))).collect()),
        ),
        ("rows", Value::Array(Vec::new())),
        ("truncated", Value::Bool(false)),
        ("delivery", string("not-observed")),
        ("assessment_authority", Value::Bool(false)),
        ("completion_inferred", Value::Bool(false)),
    ])
}

/// Read optional evidence only for an already successful ordinary inventory.
pub fn wake_decisions(
    authority: &dyn StreamAuthority,
    repo: &Path,
    streams: &Value,
    base: Option<&Path>,
) -> Result<Value> {
    let base = match base {
        Some(base) => base.to_path_buf(),
        None => authority.base(repo)?,
    };
    let root = fs::canonical_path(repo, false)?;
    let root = super::path_text(&root)?.to_owned();
    let mut counts: Vec<(String, u64)> = Vec::new();
    let mut rows = Vec::new();
    let mut truncated = false;
    for row in get(streams, "streams").as_array().unwrap_or_default() {
        let (Some(instance), Some(open_sha)) = (
            get(row, "instance_id").as_str(),
            get(row, "open_sha256").as_str(),
        ) else {
            continue;
        };
        let session = get(row, "session").as_str().unwrap_or_default();
        let directory = directory(&base, session, instance)?;
        let opened = read_required(&directory.join("open.json"), MAX_METADATA)?;
        open_value(&opened, &directory)?;
        if digest(&opened)? != open_sha || get(&opened, "repo_root").as_str() != Some(root.as_str())
        {
            return Err(err("Wake diagnostic inventory binding changed"));
        }
        let optional = wake_decision_directory(&directory)?;
        let mut paths = Vec::new();
        if lexists(&optional)? {
            for path in entries(&optional, Pattern::Sequence, false, MAX_ENTRIES)? {
                paths.push((sequence_of(&path)?, path));
            }
        }
        let attempts_dir = directory.join("attempts");
        if !lexists(&attempts_dir)? {
            continue;
        }
        for path in entries(&attempts_dir, Pattern::Sequence, false, MAX_ENTRIES)? {
            let sequence = sequence_of(&path)?;
            let file = name(&path)?;
            let attempt = read_required(&path, MAX_METADATA)?;
            if get(&attempt, "action").as_str() != Some("seal")
                || get(&attempt, "source_kind").as_str() != Some("session-manager")
                || get(&attempt, "disposition").as_str() != Some("accepted")
                || !lexists(&directory.join("started").join(file))?
                || !lexists(&directory.join("outcomes").join(file))?
            {
                continue;
            }
            let outcome = read_required(&directory.join("outcomes").join(file), MAX_METADATA)?;
            if get(&outcome, "outcome").as_str() != Some("verified") {
                continue;
            }
            let evidence = seal_evidence(&directory, sequence, Some(&opened), None)?;
            let mut status = "unrecorded".to_owned();
            if let Some((_, decision)) = paths.iter().find(|(s, _)| *s == sequence) {
                status = checked_wake_decision(decision, &directory, sequence, &evidence)
                    .unwrap_or_else(|_| "unbound".into());
            }
            match counts.iter_mut().find(|(code, _)| *code == status) {
                Some((_, count)) => *count += 1,
                None => counts.push((status.clone(), 1)),
            }
            if rows.len() < MAX_ROWS {
                rows.push(object([
                    ("instance_id", string(instance)),
                    ("sequence", int(sequence)),
                    ("status", string(&status)),
                    ("continuation_offered", Value::Bool(status == "offered")),
                ]));
            } else {
                truncated = true;
            }
        }
    }
    let mut summary = wake_decision_summary(true);
    if let Some(Value::Object(pairs)) = summary.get_mut("counts") {
        for (code, count) in counts {
            if let Some((_, value)) = pairs.iter_mut().find(|(key, _)| *key == code) {
                *value = int(count);
            }
        }
    }
    if let Some(value) = summary.get_mut("rows") {
        *value = Value::Array(rows);
    }
    if let Some(value) = summary.get_mut("truncated") {
        *value = Value::Bool(truncated);
    }
    Ok(summary)
}
