//! Project native stored entries without copying display events or provider state.
//!
//! Baseline `omp_projection.py`. The caller must supply a detached
//! SessionManager stored entry, never a native message event. A returned gap
//! is not assessable source coverage.
use crate::codec::json::{self, Value};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::omp_installation::{ManualComparison, OwnedManual};
use crate::turn_sources;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Projection, retained snapshot and screened instruction-argument bound
/// (baseline `16 * 1024 * 1024`).
pub const MAX_SOURCE: usize = 16 * 1024 * 1024;
pub const EXCLUSIONS: [&str; 2] = ["provider-metadata", "reasoning-and-signatures"];
pub const ROLES: [&str; 9] = [
    "user",
    "developer",
    "assistant",
    "toolResult",
    "bashExecution",
    "pythonExecution",
    "fileMention",
    "custom",
    "hookMessage",
];
pub const FORBIDDEN_KEYS: [&str; 12] = [
    "providerPayload",
    "provider_payload",
    "thinking",
    "reasoning",
    "thinkingSignature",
    "textSignature",
    "signature",
    "auth",
    "credentials",
    "credential",
    "config",
    "contextSnapshot",
];
pub const PERSISTENCE_TRUNCATION_NOTICE: &str = "\n\n[Session persistence truncated large content]";
/// Checked native inventories: parents and carriers (baseline `16384`).
pub const MAX_INVENTORY: usize = 16384;
const OPAQUE_TYPES: [&str; 6] = [
    "image",
    "image_url",
    "input_image",
    "reasoning",
    "thinking",
    "unavailable",
];
const CONTEXT_SCHEMA: &str = "agent-memory/owned-instruction-context-v1";
const RENDERER: &str = "omp-18.16-full-skill-v1";

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

/// The fixed exclusion list value.
pub fn exclusions() -> Value {
    Value::Array(EXCLUSIONS.iter().map(|item| string(*item)).collect())
}

/// Python dict/list equality on validated JSON: canonical sorted encodings.
pub(crate) fn same(left: &Value, right: &Value) -> bool {
    match (turn_sources::encoded(left), turn_sources::encoded(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// `set(value) == set(fields)`.
pub(crate) fn has_exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len() && fields.iter().all(|field| value.get(field).is_some())
    })
}

/// Python truthiness of a JSON value.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Int(number) => number.as_str() != "0",
        Value::Float(number) => *number != 0.0,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(pairs) => !pairs.is_empty(),
    }
}

fn hex64(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// Baseline `_instruction_uri`: only the canonical native URI for a complete
/// instruction file.
pub fn instruction_uri(relative: &str) -> Option<String> {
    if relative == "SKILL.md" {
        return Some("skill://agent-memory".to_owned());
    }
    let directories = relative.strip_suffix("/GUIDE.md")?;
    let valid = directories.split('/').all(|part| {
        let mut characters = part.chars();
        characters.next().is_some_and(|c| c.is_ascii_lowercase())
            && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    });
    valid.then(|| format!("skill://agent-memory/{relative}"))
}

/// Baseline `_instruction_relative`: invert canonical URIs without normalizing aliases.
pub fn instruction_relative(uri: &Value) -> Option<String> {
    let uri = uri.as_str()?;
    if uri == "skill://agent-memory" {
        return Some("SKILL.md".to_owned());
    }
    let relative = uri.strip_prefix("skill://agent-memory/")?;
    (instruction_uri(relative).as_deref() == Some(uri)).then(|| relative.to_owned())
}

/// A validated stored timestamp, ordered as the baseline aware `datetime`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StoredTime {
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    micros: u32,
}

fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
fn parse_time(text: &str) -> Option<StoredTime> {
    let body = text.strip_suffix('Z')?;
    let (main, fraction) = match body.split_once('.') {
        Some((main, fraction)) => (main, Some(fraction)),
        None => (body, None),
    };
    let bytes = main.as_bytes();
    if bytes.len() != 19
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let field = |start: usize, end: usize| main.get(start..end).and_then(digits);
    let micros = match fraction {
        None => 0,
        Some(fraction) if (1..=6).contains(&fraction.len()) => {
            digits(fraction)? * 10u32.pow(6 - fraction.len() as u32)
        }
        Some(_) => return None,
    };
    let value = StoredTime {
        year: field(0, 4)?,
        month: field(5, 7)?,
        day: field(8, 10)?,
        hour: field(11, 13)?,
        minute: field(14, 16)?,
        second: field(17, 19)?,
        micros,
    };
    let valid = value.year >= 1
        && (1..=12).contains(&value.month)
        && value.day >= 1
        && value.day <= days_in_month(value.year, value.month)
        && value.hour < 24
        && value.minute < 60
        && value.second < 60;
    valid.then_some(value)
}

/// Baseline `stored_timestamp`: `None` stays absent; anything else must be an
/// exact UTC stored timestamp with ASCII digits.
pub fn stored_timestamp(value: &Value) -> Result<Option<StoredTime>> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => parse_time(text)
            .map(Some)
            .ok_or_else(|| err("Invalid native stored timestamp")),
        _ => Err(err("Invalid native stored timestamp")),
    }
}

fn absolute_text(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text.starts_with('/'))
}

fn git_top(cwd: &str) -> Result<Option<PathBuf>> {
    let arguments: Vec<OsString> = ["rev-parse", "--show-toplevel"]
        .iter()
        .map(OsString::from)
        .collect();
    let top = crate::identity_git::git(Path::new(cwd), &arguments, true, false)
        .map_err(|error| ContractError::new(error.to_string()))?;
    Ok(top.map(PathBuf::from))
}

fn view_header(view: &Value, session: &str) -> Result<()> {
    let header = get(view, "header");
    if !has_exact(
        header,
        &["id", "cwd", "version", "parent_session", "timestamp"],
    ) || get(header, "id").as_str() != Some(session)
        || get(header, "version").as_int().map(|n| n.as_str()) != Some("3")
        || !matches!(
            get(view, "source_kind").as_str(),
            Some("session-manager" | "retired-file")
        )
    {
        return Err(err("Invalid checked native source header"));
    }
    if !matches!(
        get(view, "tail_status").as_str(),
        Some("available" | "active-unavailable")
    ) {
        return Err(err("Invalid checked native tail qualification"));
    }
    for cwd in [get(view, "cwd"), get(header, "cwd")] {
        if !absolute_text(cwd) {
            return Err(err("Invalid checked native working directory"));
        }
    }
    Ok(())
}

/// Parent identities: native ID → parent ID, refusing duplicates and gap prefixes.
fn parent_identities(parents: &Value) -> Result<BTreeMap<String, Option<String>>> {
    let parents = parents
        .as_array()
        .filter(|items| items.len() <= MAX_INVENTORY)
        .ok_or_else(|| err("Checked native source inventory exceeds bound"))?;
    let mut identities = BTreeMap::new();
    for entry in parents {
        if !has_exact(entry, &["id", "parent_id", "timestamp"]) {
            return Err(err("Invalid checked native parent identity"));
        }
        let native = turn_sources::bounded(get(entry, "id"), 512)?;
        if native.starts_with("event:") || native.starts_with("tool:") {
            return Err(err("Native identity uses a reserved gap prefix"));
        }
        let parent = match get(entry, "parent_id") {
            Value::Null => None,
            other => Some(turn_sources::bounded(other, 512)?.to_owned()),
        };
        if identities.insert(native.to_owned(), parent).is_some() {
            return Err(err("Duplicate checked native identity"));
        }
    }
    let mut finished = BTreeSet::new();
    for native in identities.keys() {
        let mut chain = BTreeSet::new();
        let mut current = Some(native.clone());
        while let Some(node) = current.filter(|node| !finished.contains(node)) {
            let next = identities.get(&node);
            if next.is_none() || chain.contains(&node) {
                return Err(err("Incomplete or cyclic checked native source tree"));
            }
            current = next.cloned().flatten();
            chain.insert(node);
        }
        finished.extend(chain);
    }
    Ok(identities)
}

fn check_carriers(payload: &Value, identities: &BTreeMap<String, Option<String>>) -> Result<()> {
    let entries = get(payload, "entries")
        .as_array()
        .filter(|items| items.len() <= MAX_INVENTORY)
        .ok_or_else(|| err("Checked native source carriers exceed bound"))?;
    let mut seen = BTreeSet::new();
    for entry in entries {
        let id = get(entry, "id").as_str();
        let parent = id.and_then(|id| identities.get(id));
        let expected = match parent {
            Some(Some(parent)) => string(parent.clone()),
            _ => Value::Null,
        };
        let Some(id) = id.filter(|id| entry.as_object().is_some() && !seen.contains(*id)) else {
            return Err(err("Checked native carrier identity differs"));
        };
        if parent.is_none() || get(entry, "parentId") != &expected {
            return Err(err("Checked native carrier identity differs"));
        }
        if get(entry, "type").as_str() == Some("message")
            && get(entry, "message").as_object().is_none()
        {
            return Err(err("Checked native message carrier is unavailable"));
        }
        seen.insert(id.to_owned());
    }
    if identities.keys().any(|native| !seen.contains(native)) {
        return Err(err("Checked native inventory omits stored carriers"));
    }
    Ok(())
}

fn check_fork(view: &Value, payload: &Value) -> Result<()> {
    let header = get(view, "header");
    let fork_time = stored_timestamp(get(header, "timestamp"))?;
    let mut times = Vec::new();
    for entry in get(view, "native_parents").as_array().unwrap_or_default() {
        let native = get(entry, "id").as_str().unwrap_or_default().to_owned();
        times.push((native, stored_timestamp(get(entry, "timestamp"))?));
    }
    let expected = if get(header, "parent_session") == &Value::Null {
        "none"
    } else if fork_time.is_some() && times.iter().all(|(_, time)| time.is_some()) {
        "verified"
    } else {
        "unavailable"
    };
    if get(view, "fork_status").as_str() != Some(expected) {
        return Err(err("Native fork evidence qualification differs"));
    }
    let mut inherited: Vec<&str> = times
        .iter()
        .filter(|(_, time)| expected == "verified" && *time <= fork_time)
        .map(|(native, _)| native.as_str())
        .collect();
    inherited.sort_unstable();
    let declared = match get(payload, "inherited_ids") {
        Value::Null => Some(Vec::new()),
        Value::Array(items) => items.iter().map(Value::as_str).collect(),
        _ => None,
    };
    let mut declared =
        declared.ok_or_else(|| err("Inherited native IDs differ from stored fork evidence"))?;
    declared.sort_unstable();
    if declared != inherited {
        return Err(err("Inherited native IDs differ from stored fork evidence"));
    }
    Ok(())
}

/// Baseline `checked_view`: the validated detached stored view and its
/// pre-guard Git root.
pub fn checked_view(repo: &Path, session: &str, payload: &Value) -> Result<(Value, PathBuf)> {
    let view = get(payload, "view");
    if !has_exact(
        view,
        &[
            "session",
            "cwd",
            "file",
            "header",
            "leaf_id",
            "native_parents",
            "source_kind",
            "tail_status",
            "fork_status",
        ],
    ) || get(view, "session").as_str() != Some(session)
    {
        return Err(err("Invalid checked native source view"));
    }
    view_header(view, session)?;
    let expected_root = crate::fs::canonical_path(repo, false)?;
    let cwd = get(view, "cwd").as_str().unwrap_or_default();
    let top = git_top(cwd)?.ok_or_else(|| err("Checked native source Git root is unavailable"))?;
    if crate::fs::canonical_path(&top, false)? != expected_root {
        return Err(err("Checked native source belongs to another bank scope"));
    }
    let file = get(view, "file");
    if file != &Value::Null && !absolute_text(file) {
        return Err(err("Invalid checked native source file"));
    }
    if get(view, "source_kind").as_str() == Some("retired-file")
        && (file == &Value::Null || get(view, "tail_status").as_str() != Some("available"))
    {
        return Err(err(
            "Retired native source requires its exact available file",
        ));
    }
    let parent_session = get(get(view, "header"), "parent_session");
    if parent_session != &Value::Null {
        turn_sources::bounded(parent_session, 8192)?;
    }
    let identities = parent_identities(get(view, "native_parents"))?;
    let leaf = get(view, "leaf_id");
    let mut current = match leaf {
        Value::Null => None,
        Value::String(leaf) if identities.contains_key(leaf) => Some(leaf.clone()),
        _ => return Err(err("Checked native leaf is unavailable")),
    };
    let mut branch = Vec::new();
    while let Some(node) = current {
        current = identities.get(&node).cloned().flatten();
        branch.push(string(node));
    }
    branch.reverse();
    if get(payload, "branch_ids") != &Value::Array(branch) {
        return Err(err(
            "Checked native branch differs from its parent identities",
        ));
    }
    check_carriers(payload, &identities)?;
    check_fork(view, payload)?;
    Ok((view.clone(), expected_root))
}

fn persistence_truncated(value: &Value) -> bool {
    match value {
        Value::String(text) => text.ends_with(PERSISTENCE_TRUNCATION_NOTICE),
        Value::Array(items) => items.iter().any(persistence_truncated),
        Value::Object(pairs) => pairs.iter().any(|(_, child)| persistence_truncated(child)),
        _ => false,
    }
}

/// Baseline exception classes of `project`: `UnsafeSource` versus every other
/// projection refusal.
enum Refusal {
    Unsafe,
    Payload,
}
impl From<ContractError> for Refusal {
    fn from(_: ContractError) -> Self {
        Self::Payload
    }
}
type Projected<T> = std::result::Result<T, Refusal>;

fn unsafe_text(text: &str) -> Projected<bool> {
    Ok(crate::security::matches_any(text)?)
}

/// Baseline `_safe_tree`: reject opaque payloads instead of copying them.
fn safe_tree(value: &Value) -> Projected<()> {
    match value {
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => Ok(()),
        Value::String(text) => {
            if unsafe_text(text)? {
                return Err(Refusal::Unsafe);
            }
            Ok(())
        }
        Value::Array(items) => items.iter().try_for_each(safe_tree),
        Value::Object(pairs) => {
            if get(value, "type")
                .as_str()
                .is_some_and(|kind| OPAQUE_TYPES.contains(&kind))
            {
                return Err(Refusal::Payload);
            }
            for (key, child) in pairs {
                if FORBIDDEN_KEYS.contains(&key.as_str()) {
                    return Err(Refusal::Payload);
                }
                if unsafe_text(key)? {
                    return Err(Refusal::Unsafe);
                }
                safe_tree(child)?;
            }
            Ok(())
        }
    }
}

fn text(value: &Value) -> Projected<Value> {
    match value {
        Value::String(_) => Ok(value.clone()),
        _ => Err(Refusal::Payload),
    }
}

/// Baseline `_blocks`: text and tool calls; unknown blocks are a payload gap.
fn blocks(value: &Value) -> Projected<(Value, bool)> {
    let items = match value {
        Value::String(_) => return Ok((value.clone(), false)),
        Value::Array(items) => items,
        _ => return Err(Refusal::Payload),
    };
    let mut result = Vec::new();
    let mut gap = false;
    for item in items {
        if item.as_object().is_none() {
            return Err(Refusal::Payload);
        }
        match get(item, "type").as_str() {
            Some("text") => result.push(object([
                ("type", string("text")),
                ("text", text(get(item, "text"))?),
            ])),
            Some("toolCall") => {
                let id = text(get(item, "id"))?;
                let name = text(get(item, "name"))?;
                let arguments = get(item, "arguments");
                if arguments.as_object().is_none() {
                    return Err(Refusal::Payload);
                }
                result.push(object([
                    ("type", string("toolCall")),
                    ("id", id),
                    ("name", name),
                    ("arguments", arguments.clone()),
                ]));
            }
            Some("thinking" | "reasoning" | "redactedThinking") => {}
            // Images, references and unknown blocks need independently checked
            // payload evidence. Do not serialize inline bytes or infer coverage.
            _ => gap = true,
        }
    }
    Ok((Value::Array(result), gap))
}

fn push(result: &mut Value, key: &str, value: Value) {
    if let Value::Object(pairs) = result {
        pairs.push((key.to_owned(), value));
    }
}

fn execution(message: &Value, role: &str, result: &mut Value) -> Projected<bool> {
    let command_key = if role == "bashExecution" {
        "command"
    } else {
        "code"
    };
    push(result, command_key, text(get(message, command_key))?);
    push(result, "output", text(get(message, "output"))?);
    let gap = truthy(get(message, "truncated")) || truthy(get(message, "images"));
    for key in ["cancelled", "truncated", "excludeFromContext"] {
        if let Some(flag) = message.get(key) {
            if flag.as_bool().is_none() {
                return Err(Refusal::Payload);
            }
            push(result, key, flag.clone());
        }
    }
    let code = get(message, "exitCode");
    if !matches!(code, Value::Null | Value::Int(_)) {
        return Err(Refusal::Payload);
    }
    push(result, "exitCode", code.clone());
    Ok(gap)
}

fn file_mention(message: &Value, result: &mut Value) -> Projected<bool> {
    let files = get(message, "files")
        .as_array()
        .filter(|files| files.len() <= 1024)
        .ok_or(Refusal::Payload)?;
    let mut retained = Vec::new();
    let mut gap = false;
    for item in files {
        if item.as_object().is_none()
            || truthy(get(item, "skippedReason"))
            || truthy(get(item, "image"))
        {
            gap = true;
            continue;
        }
        retained.push(object([
            ("path", text(get(item, "path"))?),
            ("content", text(get(item, "content"))?),
        ]));
    }
    push(result, "files", Value::Array(retained));
    Ok(gap)
}

/// The `message` carrier; `None` is the content-free missing-stored-source gap.
fn message_carrier(
    entry: &Value,
    native: Value,
    parent: Value,
) -> Projected<Option<(Value, bool)>> {
    let message = get(entry, "message");
    let role = match get(message, "role") {
        // An unhashable role raised TypeError in the baseline set membership.
        Value::Array(_) | Value::Object(_) if message.as_object().is_some() => {
            return Err(Refusal::Payload);
        }
        Value::String(role) if message.as_object().is_some() && ROLES.contains(&role.as_str()) => {
            role.clone()
        }
        _ => return Ok(None),
    };
    let mut result = object([
        ("carrier", string("message")),
        ("native_entry_id", native),
        ("native_parent_id", parent),
        ("role", string(role.clone())),
    ]);
    for key in ["attribution", "toolCallId", "toolName", "isError"] {
        if let Some(value) = message.get(key) {
            if key == "isError" {
                if value.as_bool().is_none() {
                    return Err(Refusal::Payload);
                }
            } else {
                text(value)?;
            }
            push(&mut result, key, value.clone());
        }
    }
    let gap = match role.as_str() {
        "bashExecution" | "pythonExecution" => execution(message, &role, &mut result)?,
        "fileMention" => file_mention(message, &mut result)?,
        _ => {
            let (content, gap) = blocks(get(message, "content"))?;
            push(&mut result, "content", content);
            if role == "toolResult" {
                if let Some(details) = message.get("details") {
                    push(&mut result, "details", details.clone());
                }
            }
            if role == "custom" || role == "hookMessage" {
                push(&mut result, "customType", text(get(message, "customType"))?);
            }
            gap
        }
    };
    Ok(Some((result, gap)))
}

fn custom_carrier(entry: &Value, native: Value, parent: Value) -> Projected<(Value, bool)> {
    let mut result = object([
        ("carrier", string("custom_message")),
        ("native_entry_id", native),
        ("native_parent_id", parent),
        ("customType", text(get(entry, "customType"))?),
    ]);
    if let Some(attribution) = entry.get("attribution") {
        push(&mut result, "attribution", text(attribution)?);
    }
    let (content, gap) = blocks(get(entry, "content"))?;
    push(&mut result, "content", content);
    Ok((result, gap))
}

fn gap(reason: &str) -> Value {
    object([("gap", string(reason))])
}

fn project_checked(entry: &Value) -> Projected<Value> {
    let kind = get(entry, "type").as_str();
    if entry.as_object().is_none() || !matches!(kind, Some("message" | "custom_message")) {
        return Ok(gap("missing-stored-source"));
    }
    let native = text(get(entry, "id"))?;
    let parent = match get(entry, "parentId") {
        Value::Null => Value::Null,
        other => text(other)?,
    };
    let (result, has_gap) = if kind == Some("message") {
        match message_carrier(entry, native, parent)? {
            Some(projected) => projected,
            None => return Ok(gap("missing-stored-source")),
        }
    } else {
        custom_carrier(entry, native, parent)?
    };
    if persistence_truncated(&result) {
        return Ok(gap("oversized-source"));
    }
    safe_tree(&result)?;
    let raw = json::encode(&result, json::SORTED_COMPACT_UTF8_FINITE)?;
    if raw.len() > MAX_SOURCE {
        return Ok(gap("oversized-source"));
    }
    // Apply the shared patterns to the complete projection, including nested
    // tool arguments/details, before retaining it. Never return matched text.
    if unsafe_text(&raw)? {
        return Ok(gap("unsafe-representation"));
    }
    if has_gap {
        return Ok(gap("missing-payload"));
    }
    Ok(object([("snapshot", result), ("exclusions", exclusions())]))
}

/// Baseline `project`: a projected entry or a fixed, content-free coverage gap.
///
/// This performs no enrollment, canonical write or complete assessment.
/// Unknown fields are deliberately excluded rather than copied. (The baseline
/// `persisted` keyword was accepted but unused; it is not carried over.)
pub fn project(entry: &Value) -> Value {
    match project_checked(entry) {
        Ok(value) => value,
        Err(Refusal::Unsafe) => gap("unsafe-representation"),
        Err(Refusal::Payload) => gap("missing-payload"),
    }
}

fn context_shape(snapshot: &Value) -> bool {
    let required = [
        "carrier",
        "native_entry_id",
        "native_parent_id",
        "role",
        "toolCallId",
        "toolName",
        "isError",
        "content",
        "instruction_context",
    ];
    let Some(pairs) = snapshot.as_object() else {
        return false;
    };
    if !required.iter().all(|key| snapshot.get(key).is_some())
        || pairs
            .iter()
            .any(|(key, _)| !required.contains(&key.as_str()) && key != "attribution")
    {
        return false;
    }
    let context = get(snapshot, "instruction_context");
    has_exact(
        context,
        &[
            "schema",
            "kind",
            "installation_id",
            "observed_receipt_sha256",
            "artifact_digest",
            "manual_file_sha256",
            "manual_relative_path",
            "renderer",
            "invocation_native_entry_id",
            "tool_call_id",
        ],
    )
}

fn context_values(snapshot: &Value) -> bool {
    let context = get(snapshot, "instruction_context");
    let relative_known = get(context, "manual_relative_path")
        .as_str()
        .and_then(instruction_uri)
        .is_some();
    if get(snapshot, "carrier").as_str() != Some("message")
        || get(snapshot, "role").as_str() != Some("toolResult")
        || get(snapshot, "toolName").as_str() != Some("read")
        || get(snapshot, "isError") != &Value::Bool(false)
        || get(snapshot, "content") != &Value::Array(Vec::new())
        || get(context, "schema").as_str() != Some(CONTEXT_SCHEMA)
        || get(context, "kind").as_str() != Some("installed-am-manual")
        || !relative_known
        || get(context, "renderer").as_str() != Some(RENDERER)
        || !same(get(context, "tool_call_id"), get(snapshot, "toolCallId"))
    {
        return false;
    }
    let nonempty = [
        get(snapshot, "native_entry_id"),
        get(snapshot, "toolCallId"),
        get(context, "installation_id"),
        get(context, "invocation_native_entry_id"),
    ];
    if !nonempty
        .iter()
        .all(|value| value.as_str().is_some_and(|text| !text.is_empty()))
    {
        return false;
    }
    if !matches!(
        get(snapshot, "native_parent_id"),
        Value::Null | Value::String(_)
    ) {
        return false;
    }
    if snapshot
        .get("attribution")
        .is_some_and(|value| value.as_str().is_none())
    {
        return false;
    }
    [
        "observed_receipt_sha256",
        "artifact_digest",
        "manual_file_sha256",
    ]
    .iter()
    .all(|key| hex64(get(context, key)))
}

fn screened(value: &Value) -> bool {
    if safe_tree(value).is_err() {
        return false;
    }
    match json::encode(value, json::SORTED_COMPACT_UTF8_FINITE) {
        Ok(raw) => {
            raw.len() <= MAX_SOURCE && matches!(crate::security::matches_any(&raw), Ok(false))
        }
        Err(_) => false,
    }
}

/// Baseline `_valid_instruction_context`: a historical metadata-only manual
/// assertion, if present, must have its exact closed shape.
pub fn valid_instruction_context(snapshot: &Value) -> bool {
    if snapshot.as_object().is_none() {
        return false;
    }
    if snapshot.get("instruction_context").is_none() {
        return true;
    }
    context_shape(snapshot) && context_values(snapshot) && screened(snapshot)
}

/// The witness's stored comparison (baseline `witness._comparison`).
pub(crate) fn comparison_of(witness: &OwnedManual) -> ManualComparison {
    ManualComparison {
        text: witness.text().to_owned(),
        header_path: witness.header_path().to_owned(),
        installation_id: witness.installation_id().to_owned(),
        observed_receipt_sha256: witness.observed_receipt_sha256().to_owned(),
        artifact_digest: witness.artifact_digest().to_owned(),
        manual_file_sha256: witness.manual_file_sha256().to_owned(),
    }
}

fn manual_result(message: &Value) -> Option<&str> {
    let required = [
        "role",
        "toolCallId",
        "toolName",
        "isError",
        "content",
        "details",
    ];
    let pairs = message.as_object()?;
    if !required.iter().all(|key| message.get(key).is_some())
        || pairs
            .iter()
            .any(|(key, _)| !required.contains(&key.as_str()) && key != "attribution")
        || get(message, "role").as_str() != Some("toolResult")
        || get(message, "toolName").as_str() != Some("read")
        || get(message, "isError") != &Value::Bool(false)
    {
        return None;
    }
    get(message, "toolCallId").as_str()
}

/// The exact single full-read call of `call_id` with canonical `uri` arguments.
fn manual_invocation(invocation: &Value, call_id: &str, uri: &str) -> Option<()> {
    let message = get(invocation, "message");
    if get(invocation, "type").as_str() != Some("message")
        || get(message, "role").as_str() != Some("assistant")
    {
        return None;
    }
    let content = get(message, "content").as_array()?;
    let calls: Vec<&Value> = content
        .iter()
        .filter(|block| {
            get(block, "type").as_str() == Some("toolCall")
                && get(block, "id").as_str() == Some(call_id)
        })
        .collect();
    let [call] = calls.as_slice() else {
        return None;
    };
    if !has_exact(call, &["type", "id", "name", "arguments"])
        || get(call, "name").as_str() != Some("read")
    {
        return None;
    }
    let arguments = get(call, "arguments");
    if !(has_exact(arguments, &["path"]) || has_exact(arguments, &["i", "path"]))
        || get(arguments, "path").as_str() != Some(uri)
    {
        return None;
    }
    if let Some(intent) = arguments.get("i") {
        intent.as_str()?;
        safe_tree(intent).ok()?;
        let raw = json::encode(intent, json::UTF8_FINITE).ok()?;
        if raw.len() > MAX_SOURCE || !matches!(crate::security::matches_any(&raw), Ok(false)) {
            return None;
        }
    }
    Some(())
}

fn manual_details(path: &str, body: &str, uri: &str) -> (Value, Value) {
    let lines = body.split('\n').count();
    let content = Value::Array(vec![object([
        ("type", string("text")),
        ("text", string(format!("[Skill file: {path}]\n{body}"))),
    ])]);
    let details = object([
        ("contentType", string("text/markdown")),
        ("resolvedPath", string(path)),
        ("rowPrefixes", Value::Bool(false)),
        ("totalLines", Value::Int(lines.into())),
        (
            "displayContent",
            object([
                (
                    "lineNumbers",
                    Value::Array((1..=lines).map(|n| Value::Int(n.into())).collect()),
                ),
                ("startLine", Value::Int(1u8.into())),
                ("text", string(body)),
            ]),
        ),
        (
            "meta",
            object([(
                "source",
                object([("type", string("internal")), ("value", string(uri))]),
            )]),
        ),
    ]);
    (content, details)
}

/// Baseline `_project_owned_manual`: project only an exact native full read
/// backed by an installation witness whose comparison equals `verified`.
pub fn project_owned_manual(
    entry: &Value,
    invocation: &Value,
    witness: &OwnedManual,
    verified: &ManualComparison,
) -> Option<Value> {
    if *verified != comparison_of(witness) {
        return None;
    }
    let relative = witness.relative_path();
    let uri = instruction_uri(relative)?;
    if !has_exact(entry, &["type", "id", "parentId", "message"])
        || get(entry, "type").as_str() != Some("message")
    {
        return None;
    }
    let message = get(entry, "message");
    let call_id = manual_result(message)?;
    manual_invocation(invocation, call_id, &uri)?;
    let body = verified.text.trim_end_matches('\n');
    let (content, details) = manual_details(&verified.header_path, body, &uri);
    // Strict encoded equality also refuses booleans/floats standing in for integers.
    if !same(get(message, "content"), &content) || !same(get(message, "details"), &details) {
        return None;
    }
    let mut snapshot = object([
        ("carrier", string("message")),
        ("native_entry_id", string(get(entry, "id").as_str()?)),
        ("native_parent_id", get(entry, "parentId").clone()),
        ("role", string("toolResult")),
        ("toolCallId", string(call_id)),
        ("toolName", string("read")),
        ("isError", Value::Bool(false)),
        ("content", Value::Array(Vec::new())),
        (
            "instruction_context",
            object([
                ("schema", string(CONTEXT_SCHEMA)),
                ("kind", string("installed-am-manual")),
                ("installation_id", string(&verified.installation_id)),
                (
                    "observed_receipt_sha256",
                    string(&verified.observed_receipt_sha256),
                ),
                ("artifact_digest", string(&verified.artifact_digest)),
                ("manual_file_sha256", string(&verified.manual_file_sha256)),
                ("manual_relative_path", string(relative)),
                ("renderer", string(RENDERER)),
                (
                    "invocation_native_entry_id",
                    string(get(invocation, "id").as_str()?),
                ),
                ("tool_call_id", string(call_id)),
            ]),
        ),
    ]);
    if let Some(attribution) = message.get("attribution") {
        push(&mut snapshot, "attribution", string(attribution.as_str()?));
    }
    if !valid_instruction_context(&snapshot) {
        return None;
    }
    Some(object([
        ("snapshot", snapshot),
        ("exclusions", exclusions()),
    ]))
}
