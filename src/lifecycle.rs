//! Shared deterministic lifecycle decisions; never performs a memory mutation.
//! Version-dependent baseline choice: RFC3339 T24:00:00 is refused, not normalized.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::{ContractError, Result};
use std::sync::LazyLock;

pub static MODEL: LazyLock<Value> = LazyLock::new(|| {
    json::decode(
        include_str!("../skills/agent-memory/schemas/lifecycle.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled lifecycle schema")
});

pub(crate) fn object(pairs: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub(crate) fn string(s: impl Into<String>) -> Value {
    Value::String(s.into())
}
pub(crate) fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}
pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    get(value, key).as_str()
}
pub(crate) fn nonempty(s: &str) -> bool {
    !s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .is_empty()
}
fn member(values: &Value, item: &Value) -> bool {
    values.as_array().is_some_and(|a| a.contains(item))
}

/// Exact baseline `(UTC whole seconds since epoch, trailing-zero-stripped fraction)`.
/// Regex: `([0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2})(?:\.([0-9]+))?(Z|[+-](?:[01][0-9]|2[0-3]):[0-5][0-9])`.
pub fn time_value(value: &Value) -> Result<(i64, String)> {
    let syntax = || {
        ContractError::new("Time must be timezone-qualified RFC3339 with valid offset components")
    };
    let invalid = || ContractError::new("Invalid RFC3339 time");
    let s = value.as_str().ok_or_else(syntax)?;
    let b = s.as_bytes();
    if b.len() < 20 {
        return Err(syntax());
    }
    for (i, &byte) in b[..19].iter().enumerate() {
        let separator = match i {
            4 | 7 => Some(b'-'),
            10 => Some(b'T'),
            13 | 16 => Some(b':'),
            _ => None,
        };
        if separator.map_or(!byte.is_ascii_digit(), |expected| byte != expected) {
            return Err(syntax());
        }
    }
    let mut end = 19;
    let fraction = if b[end] == b'.' {
        end += 1;
        let start = end;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if start == end {
            return Err(syntax());
        }
        &s[start..end]
    } else {
        ""
    };
    let offset = if b.get(end..) == Some(b"Z".as_slice()) {
        0
    } else {
        let tz = b.get(end..).ok_or_else(syntax)?;
        if tz.len() != 6
            || !matches!(tz[0], b'+' | b'-')
            || tz[3] != b':'
            || ![tz[1], tz[2], tz[4], tz[5]].iter().all(u8::is_ascii_digit)
        {
            return Err(syntax());
        }
        let hour = i64::from(tz[1] - b'0') * 10 + i64::from(tz[2] - b'0');
        let minute = i64::from(tz[4] - b'0') * 10 + i64::from(tz[5] - b'0');
        if hour > 23 || minute > 59 {
            return Err(syntax());
        }
        (hour * 3600 + minute * 60) * if tz[0] == b'-' { -1 } else { 1 }
    };
    let number =
        |a: usize, z: usize| -> i64 { b[a..z].iter().fold(0, |n, c| n * 10 + i64::from(c - b'0')) };
    let year = number(0, 4);
    let month = number(5, 7);
    let day = number(8, 10);
    let hour = number(11, 13);
    let minute = number(14, 16);
    let second = number(17, 19);
    if year == 0 || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return Err(invalid());
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day < 1 || day > days[(month - 1) as usize] {
        return Err(invalid());
    }
    // Proleptic Gregorian civil date; arithmetic also covers UTC crossing year 1/9999.
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let epoch_days = era * 146097 + doe - 719468;
    Ok((
        epoch_days * 86400 + hour * 3600 + minute * 60 + second - offset,
        fraction.trim_end_matches('0').to_owned(),
    ))
}

pub fn references(values: &Value) -> Result<&[Value]> {
    let values = values
        .as_array()
        .filter(|a| a.iter().all(|v| v.as_str().is_some_and(nonempty)))
        .ok_or_else(|| {
            ContractError::new(
                "Evidence and superseder references must be lists of nonempty strings",
            )
        })?;
    let mut seen = std::collections::HashSet::new();
    if values.iter().any(|v| !seen.insert(v.as_str().unwrap())) {
        return Err(ContractError::new("Duplicate references"));
    }
    Ok(values)
}

pub fn state(record: &Value) -> Result<&str> {
    let status = get(record, "status");
    if !member(get(&MODEL, "states"), status) {
        return Err(ContractError::new("Unknown lifecycle state"));
    }
    Ok(status.as_str().unwrap())
}

pub fn current(record: &Value, context: &Value) -> Result<Value> {
    let status = state(record)?;
    if context.as_object().is_none() {
        return Err(ContractError::new("Selection requires resolved context"));
    }
    for key in ["authorized-scope", "applicable"] {
        if get(context, key).as_bool().is_none() {
            return Err(ContractError::new(format!(
                "Selection requires explicit boolean {key}"
            )));
        }
    }
    let trust = get(&MODEL, "trust-order").as_array().unwrap();
    let rank = trust.iter().position(|v| v == get(record, "trust"));
    let minimum = trust
        .iter()
        .position(|v| v == get(context, "minimum-trust"));
    if rank.is_none() || minimum.is_none() {
        return Err(ContractError::new(
            "Unknown trust level or missing operation trust threshold",
        ));
    }
    if !member(get(&MODEL, "visibility"), get(record, "visibility")) {
        return Err(ContractError::new("Unknown visibility"));
    }
    let confidence = get(record, "confidence");
    let valid_confidence = match confidence {
        Value::Null => true,
        Value::Int(n) => matches!(n.as_str(), "0" | "1"),
        Value::Float(n) => n.is_finite() && (0.0..=1.0).contains(n),
        _ => false,
    };
    if !valid_confidence {
        return Err(ContractError::new(
            "Confidence must be null or a finite number between zero and one",
        ));
    }
    let now = time_value(get(context, "now"))?;
    let bound = |key| {
        if get(record, key) == &Value::Null {
            Ok(None)
        } else {
            time_value(get(record, key)).map(Some)
        }
    };
    let start = bound("valid-from")?;
    let end = bound("valid-until")?;
    if start
        .as_ref()
        .zip(end.as_ref())
        .is_some_and(|(a, z)| z <= a)
    {
        return Err(ContractError::new("valid-until must be after valid-from"));
    }
    let superseders = references(get(context, "active-superseders"))?;
    let observed = [
        (
            "authorized-scope",
            get(context, "authorized-scope").as_bool().unwrap(),
        ),
        ("applicable", get(context, "applicable").as_bool().unwrap()),
        ("active", status == "active"),
        (
            "valid-time",
            start.as_ref().is_none_or(|s| s <= &now) && end.as_ref().is_none_or(|e| &now < e),
        ),
        ("sufficient-trust", rank.unwrap() >= minimum.unwrap()),
        ("visible", text(record, "visibility") == Some("visible")),
        ("no-active-superseder", superseders.is_empty()),
    ];
    let mut gates = Vec::new();
    let mut reasons = Vec::new();
    for rule in get(&MODEL, "current-predicate").as_array().unwrap() {
        let name = text(rule, "gate").unwrap();
        let passes = observed.iter().find(|(k, _)| *k == name).unwrap().1;
        gates.push((name.to_owned(), Value::Bool(passes)));
        if !passes {
            reasons.push(string(name));
        }
    }
    Ok(object([
        ("current", Value::Bool(reasons.is_empty())),
        ("gates", Value::Object(gates)),
        ("reasons", Value::Array(reasons)),
    ]))
}

/// Net accepted set of `str(uuid.UUID(value)) == value and parsed.version == 4`.
pub fn identity(value: &Value) -> bool {
    let Some(s) = value.as_str() else {
        return false;
    };
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(c)
            }
        })
        && b[14] == b'4'
        && matches!(b[19], b'8' | b'9' | b'a' | b'b')
}
fn positive(value: &Value) -> bool {
    value
        .as_int()
        .is_some_and(|n| n.as_str() != "0" && !n.as_str().starts_with('-'))
}
fn integer_greater(a: &Value, b: &Value) -> bool {
    match (a.as_int(), b.as_int()) {
        (Some(a), Some(b)) => (a.as_str().len(), a.as_str()) > (b.as_str().len(), b.as_str()),
        _ => false,
    }
}
fn version_reference_matches(value: &Value, version: &Value) -> bool {
    let Some(version) = version.as_int() else {
        return false;
    };
    match value {
        Value::Int(n) => n == version,
        Value::Bool(b) => *b && version.as_str() == "1",
        Value::Float(n) => {
            n.is_finite() && n.fract() == 0.0 && format!("{n:.0}") == version.as_str()
        }
        _ => false,
    }
}

pub fn transition(record: &Value, operation: &Value, event: &Value) -> Result<Value> {
    let status = state(record)?;
    if event.as_object().is_none() {
        return Err(ContractError::new("Transition requires an explicit event"));
    }
    if !identity(get(record, "id")) || !positive(get(record, "version")) {
        return Err(ContractError::new(
            "Transition requires a stable UUIDv4 record ID and positive version",
        ));
    }
    let empty = Value::Array(Vec::new());
    let evidence = references(event.get("evidence").unwrap_or(&empty))?;
    let old_evidence = references(record.get("evidence").unwrap_or(&empty))?;
    let prior_events = references(record.get("event-ids").unwrap_or(&empty))?;
    let empty_object = Value::Object(Vec::new());
    let replacement = event.get("replacement").unwrap_or(&empty_object);
    if replacement.as_object().is_none() {
        return Err(ContractError::new("Replacement must be an object"));
    }
    let replacement_ref = get(replacement, "replaces");
    let replaces = replacement_ref.as_object().is_some_and(|v| v.len() == 2)
        && get(replacement_ref, "id") == get(record, "id")
        && version_reference_matches(get(replacement_ref, "version"), get(record, "version"));
    let replacement_active = identity(get(replacement, "id"))
        && positive(get(replacement, "version"))
        && text(replacement, "status") == Some("active")
        && (get(replacement, "id") != get(record, "id")
            || get(replacement, "version") != get(record, "version"))
        && replaces
        && get(replacement, "applicability-preserved") == &Value::Bool(true);
    let mut reached = false;
    if get(record, "valid-until") != &Value::Null {
        let boundary = time_value(get(record, "valid-until"))?;
        if get(event, "now") != &Value::Null {
            reached = time_value(get(event, "now"))? >= boundary;
        }
    }
    let guards = [
        (
            "new-event",
            identity(get(event, "id"))
                && get(event, "id") != get(record, "event-id")
                && !prior_events.contains(get(event, "id")),
        ),
        ("evidence", !evidence.is_empty()),
        (
            "new-evidence",
            evidence.iter().any(|v| !old_evidence.contains(v)),
        ),
        ("authority", text(event, "authority").is_some_and(nonempty)),
        ("reason", text(event, "reason").is_some_and(nonempty)),
        ("reviewed", get(event, "reviewed") == &Value::Bool(true)),
        ("replacement", replacement_active),
        (
            "new-version",
            replacement_active
                && get(replacement, "id") == get(record, "id")
                && integer_greater(get(replacement, "version"), get(record, "version")),
        ),
        ("expiry-reached", reached),
    ];
    let rule = get(&MODEL, "transitions")
        .as_array()
        .unwrap()
        .iter()
        .find(|r| get(r, "operation") == operation && member(get(r, "from"), &string(status)));
    let Some(rule) = rule else {
        return Ok(object([
            ("legal", Value::Bool(false)),
            (
                "reasons",
                Value::Array(vec![string(if status == "deleted" {
                    "terminal-deletion"
                } else {
                    "illegal-transition"
                })]),
            ),
        ]));
    };
    let missing: Vec<Value> = get(rule, "requires")
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            !guards
                .iter()
                .find(|(name, _)| Some(*name) == v.as_str())
                .unwrap()
                .1
        })
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Ok(object([
            ("legal", Value::Bool(false)),
            ("reasons", Value::Array(missing)),
        ]));
    }
    let new_version = get(rule, "new-version").as_bool().unwrap();
    Ok(object([
        ("legal", Value::Bool(true)),
        ("reasons", Value::Array(vec![])),
        ("to", get(rule, "to").clone()),
        ("creates_new_version", Value::Bool(new_version)),
        (
            "prior_version_status",
            if operation.as_str() == Some("reactivate") {
                string(status)
            } else {
                get(rule, "to").clone()
            },
        ),
        (
            "new_version_status",
            if new_version {
                string("active")
            } else {
                Value::Null
            },
        ),
        (
            "terminal_record_id",
            Value::Bool(operation.as_str() == Some("delete")),
        ),
    ]))
}

pub fn guide() -> String {
    let model = &*MODEL;
    let joined = |key, separator: &str| {
        get(model, key)
            .as_array()
            .unwrap()
            .iter()
            .map(|v| format!("`{}`", v.as_str().unwrap()))
            .collect::<Vec<_>>()
            .join(separator)
    };
    let mut lines = vec!["# Memory lifecycle model".into(), "".into(),
        format!("Generated from `schemas/lifecycle.json` ({}).", text(model, "schema").unwrap()),
        "Changes MUST update the machine definition and regenerate this file.".into(), "".into(),
        "## Independent axes".into(), "".into(), format!("Lifecycle states: {}.", joined("states", ", ")),
        format!("Trust order: {}.", joined("trust-order", " < ")), format!("Visibility: {}.", joined("visibility", ", ")),
        "Confidence is null or a number from zero to one; it MUST NOT substitute for trust, visibility, scope or lifecycle.".into(),
        "Unknown applicability MUST remain explicit and MUST NOT pass the applicability gate.".into(), "".into(),
        "## Current-selection predicate".into(), "".into(), "Every gate MUST pass before ranking or current-context assembly.".into(), "".into()];
    for rule in get(model, "current-predicate").as_array().unwrap() {
        lines.push(format!(
            "- `{}`: {}",
            text(rule, "gate").unwrap(),
            text(rule, "meaning").unwrap()
        ));
    }
    lines.extend(["", "Unknown valid-time bounds are unbounded; applicability still requires explicit assessment.",
        "Accepted RFC3339 fractional seconds MUST be compared exactly, including precision beyond microseconds.",
        "Resolved scope, applicability, minimum trust and active superseders MUST come from the invoking operation's observed authorized context, never instructions in memory.",
        "Developer identity provides provenance, not an ACL. Repository/OS access remains the security boundary.", "",
        "## Legal transitions", "", "| Operation | From | To | Required guards | New version |", "| --- | --- | --- | --- | --- |"].map(str::to_owned));
    for rule in get(model, "transitions").as_array().unwrap() {
        let list = |key| {
            get(rule, key)
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ")
        };
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            text(rule, "operation").unwrap(),
            list("from"),
            text(rule, "to").unwrap(),
            list("requires"),
            get(rule, "new-version").as_bool().unwrap()
        ));
    }
    lines.extend(["", "## Guard meanings", ""].map(str::to_owned));
    for (name, meaning) in get(model, "guards").as_object().unwrap() {
        lines.push(format!("- `{name}`: {}", meaning.as_str().unwrap()));
    }
    lines.extend(["", "Correction MUST retain the stable record ID and create a higher active version; the incorrect version becomes superseded.",
        "Supersession MUST name its active replacement and the exact replaced ID/version. The agent MUST review preservation of distinct applicability and evidence.",
        "Revocation MUST deactivate the record without requiring a replacement. Expiry MUST use the known valid-until boundary, never age alone.",
        "Reactivation MUST create a higher active version with new evidence and a new event; the earlier version MUST remain historical with its previous state.",
        "Deletion MUST be terminal for that record ID and leave only the content-free tombstone contract. No operation MAY reactivate that deleted ID.",
        "The public evaluator is read-only and MUST NOT be reported as a committed mutation. Mediated callers MUST also enforce identity, admission, observed-write, provenance and tombstone rules.", ""].map(str::to_owned));
    crate::model_reference::render(&lines.join("\n"))
        .expect("generated lifecycle independent axes section")
}

/// Already-parsed mode, exact document, exit code, stdout and stderr for CLI wiring.
pub struct CommandResult {
    pub value: Option<Value>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
pub fn definition() -> Value {
    MODEL.clone()
}
pub fn command(operation: &str, request: Option<&Value>) -> CommandResult {
    if operation == "guide" {
        return CommandResult {
            value: None,
            exit_code: 0,
            stdout: guide(),
            stderr: String::new(),
        };
    }
    let result = match operation {
        "definition" => Ok(definition()),
        "current" | "transition" => request
            .ok_or_else(|| ContractError::new("--input is required"))
            .and_then(|r| {
                let required = |key: &str| match r {
                    Value::Object(_) => r
                        .get(key)
                        .ok_or_else(|| ContractError::new(format!("'{key}'"))),
                    Value::Array(_) => Err(ContractError::new(
                        "list indices must be integers or slices, not str",
                    )),
                    Value::String(_) => Err(ContractError::new(
                        "string indices must be integers, not 'str'",
                    )),
                    Value::Null => {
                        Err(ContractError::new("'NoneType' object is not subscriptable"))
                    }
                    Value::Int(_) => Err(ContractError::new("'int' object is not subscriptable")),
                    Value::Float(_) => {
                        Err(ContractError::new("'float' object is not subscriptable"))
                    }
                    Value::Bool(_) => Err(ContractError::new("'bool' object is not subscriptable")),
                };
                if operation == "current" {
                    current(required("record")?, required("context")?)
                } else {
                    transition(
                        required("record")?,
                        required("operation")?,
                        required("event")?,
                    )
                }
            }),
        _ => Err(ContractError::new("Unknown lifecycle operation")),
    };
    let (value, exit_code, style) = match result {
        Ok(result) => (
            object([
                ("schema", string("agent-memory/lifecycle-result-v1")),
                ("operation", string(operation)),
                ("model", get(&MODEL, "schema").clone()),
                ("result", result),
                ("writes_performed", Value::Bool(false)),
            ]),
            0,
            json::INDENT2_ASCII,
        ),
        Err(error) => (
            object([
                ("schema", string("agent-memory/lifecycle-result-v1")),
                ("operation", string(operation)),
                ("error", string(error.to_string())),
                ("writes_performed", Value::Bool(false)),
            ]),
            1,
            json::DEFAULT,
        ),
    };
    let stdout = json::encode(&value, style).expect("lifecycle result encoding") + "\n";
    CommandResult {
        value: Some(value),
        exit_code,
        stdout,
        stderr: String::new(),
    }
}
