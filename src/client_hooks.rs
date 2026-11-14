//! Bounded, ownership-preserving client hook configuration edits.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, Value};
use crate::error::{ContractError, Result};
use std::path::Path;

pub const EVENT: &str = "UserPromptSubmit";
pub const MAX_CONFIG: usize = 1024 * 1024;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn object() -> Value {
    Value::Object(Vec::new())
}
fn field<'a>(value: &'a mut Value, key: &str, default: Value) -> Result<&'a mut Value> {
    let Value::Object(pairs) = value else {
        return Err(err("unsupported_shape"));
    };
    let index = match pairs.iter().position(|(k, _)| k == key) {
        Some(index) => index,
        None => {
            pairs.push((key.to_owned(), default));
            pairs.len() - 1
        }
    };
    pairs
        .get_mut(index)
        .map(|(_, value)| value)
        .ok_or_else(|| err("unsupported_shape"))
}
fn remove_field(value: &mut Value, key: &str) {
    if let Value::Object(pairs) = value {
        pairs.retain(|(k, _)| k != key);
    }
}
pub fn parse(raw: &[u8]) -> Result<Value> {
    if raw.len() > MAX_CONFIG {
        return Err(err("config_unavailable"));
    }
    let value = if raw.is_empty() {
        object()
    } else {
        json::decode_bytes(
            raw,
            DecodePolicy {
                duplicate_keys: DuplicateKeys::Reject,
                max_bytes: MAX_CONFIG,
                ..DecodePolicy::default()
            },
        )
        .map_err(|e| {
            if e.message().starts_with("Duplicate metadata key:") {
                err("duplicate_json_key")
            } else {
                err("malformed_json")
            }
        })?
    };
    if value.as_object().is_none() {
        return Err(err("unsupported_shape"));
    }
    if let Some(hooks) = value.get("hooks") {
        if hooks.as_object().is_none()
            || [EVENT, "Stop"]
                .iter()
                .any(|event| hooks.get(event).is_some_and(|v| v.as_array().is_none()))
        {
            return Err(err("unsupported_shape"));
        }
    }
    Ok(value)
}
pub fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut raw = json::encode(value, json::INDENT2_UTF8)?.into_bytes();
    raw.push(b'\n');
    if raw.len() > MAX_CONFIG {
        return Err(err("Updated client hook configuration exceeds its bound"));
    }
    Ok(raw)
}
fn blank(command: &str) -> bool {
    command
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
}
pub fn entry(command: &str) -> Result<Value> {
    if blank(command) || command.len() > 4096 {
        return Err(err("Invalid owned hook command"));
    }
    Ok(Value::Object(vec![(
        "hooks".into(),
        Value::Array(vec![Value::Object(vec![
            ("type".into(), Value::String("command".into())),
            ("command".into(), Value::String(command.into())),
        ])]),
    )]))
}
pub fn equal(left: &Value, right: &Value) -> Result<bool> {
    Ok(json::encode(left, json::SORTED_UTF8)? == json::encode(right, json::SORTED_UTF8)?)
}
// Python's list/dict membership uses structural equality, independently of
// the stricter serialized comparison used to locate a receipt-owned group.
pub(crate) fn membership_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.iter()
                        .find(|(other, _)| other == key)
                        .is_some_and(|(_, other)| membership_equal(value, other))
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| membership_equal(a, b))
        }
        (Value::Bool(a), Value::Int(b)) | (Value::Int(b), Value::Bool(a)) => {
            b.as_str() == if *a { "1" } else { "0" }
        }
        (Value::Bool(a), Value::Float(b)) | (Value::Float(b), Value::Bool(a)) => {
            *b == if *a { 1.0 } else { 0.0 }
        }
        (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
            b.is_finite()
                && b.fract() == 0.0
                && (if *b == 0.0 {
                    "0".to_owned()
                } else {
                    format!("{b:.0}")
                }) == a.as_str()
        }
        _ => left == right,
    }
}
pub fn locate(document: &Value, owned: &Value, event: &str) -> Result<usize> {
    let valid = owned
        .as_object()
        .is_some_and(|v| v.len() == 1 && v.first().is_some_and(|(key, _)| key == "hooks"))
        && owned
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                !hooks.is_empty()
                    && hooks.iter().all(|item| {
                        item.as_object().is_some()
                            && item.get("type").and_then(Value::as_str) == Some("command")
                            && item
                                .get("command")
                                .and_then(Value::as_str)
                                .is_some_and(|command| !blank(command) && command.len() <= 4096)
                    })
            });
    if !valid {
        return Err(err("insufficient_receipt"));
    }
    let groups = document.get("hooks").and_then(|v| v.get(event));
    let groups = match groups {
        Some(v) => v.as_array().ok_or_else(|| err("unsupported_shape"))?,
        None => &[],
    };
    let expected = json::encode(owned, json::SORTED_UTF8)?;
    let mut matches = Vec::new();
    for (index, item) in groups.iter().enumerate() {
        if json::encode(item, json::SORTED_UTF8)? == expected {
            matches.push(index);
        }
    }
    match matches.as_slice() {
        [] => Err(err("owned_group_missing_or_changed")),
        [index] => Ok(*index),
        _ => Err(err("owned_group_duplicate")),
    }
}
pub fn merge(raw: &[u8], owned: Option<&Value>, desired: &Value, event: &str) -> Result<Vec<u8>> {
    let mut document = parse(raw)?;
    let position = match owned {
        Some(value) => Some(locate(&document, value, event)?),
        None => None,
    };
    let hooks = field(&mut document, "hooks", object())?;
    let groups = field(hooks, event, Value::Array(Vec::new()))?;
    let Value::Array(groups) = groups else {
        return Err(err(
            "Existing client hook configuration has an unsupported shape",
        ));
    };
    if let Some(position) = position {
        if owned.is_some_and(|owned| !membership_equal(owned, desired))
            && groups.iter().any(|group| membership_equal(group, desired))
        {
            return Err(err("Desired hook collides with an existing client hook"));
        }
        *groups
            .get_mut(position)
            .ok_or_else(|| err("owned_group_missing_or_changed"))? = desired.clone();
    } else {
        if groups.iter().any(|group| membership_equal(group, desired)) {
            return Err(err(
                "Identical unowned hook already exists; preserve client configuration",
            ));
        }
        groups.push(desired.clone());
    }
    encode(&document)
}
pub fn remove(
    raw: &[u8],
    owned: &Value,
    originally_absent: bool,
    event_present: bool,
    hooks_present: bool,
    event: &str,
) -> Result<Option<Vec<u8>>> {
    let mut document = parse(raw)?;
    let position = locate(&document, owned, event)?;
    let hooks = document
        .get_mut("hooks")
        .ok_or_else(|| err("unsupported_shape"))?;
    let groups = hooks
        .get_mut(event)
        .ok_or_else(|| err("unsupported_shape"))?;
    let Value::Array(groups) = groups else {
        return Err(err("unsupported_shape"));
    };
    // locate returned an index in this unchanged groups array.
    groups.remove(position);
    if groups.is_empty() && !event_present {
        remove_field(hooks, event);
    }
    if hooks.as_object().is_some_and(|v| v.is_empty()) && !hooks_present {
        remove_field(&mut document, "hooks");
    }
    if originally_absent && document.as_object().is_some_and(|v| v.is_empty()) {
        return Ok(None);
    }
    Ok(Some(encode(&document)?))
}
pub fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
/// Exact Python-era installer commands, retained for adoption recognition only.
pub fn python_era_entries(client: &str, pack: &Path) -> Result<(Value, Value)> {
    let pack = pack.to_str().ok_or_else(|| err("Non-UTF-8 hook path"))?;
    let launcher = shell_quote(&format!("{pack}/hooks/python3.sh"));
    let prompt = if client == "codex" {
        "codex_prompt.py"
    } else {
        "turn_hook.py"
    };
    let stop = if client == "codex" {
        "codex_stop.py"
    } else {
        "claude_stop.py"
    };
    let prompt = entry(&format!(
        "/bin/sh {launcher} {}",
        shell_quote(&format!("{pack}/hooks/{prompt}"))
    ))?;
    let mut stop = entry(&format!(
        "/bin/sh {launcher} {}",
        shell_quote(&format!("{pack}/hooks/{stop}"))
    ))?;
    if client == "claude" {
        if let Some(Value::Array(hooks)) = stop.get_mut("hooks") {
            if let Some(Value::Object(fields)) = hooks.first_mut() {
                fields.push(("asyncRewake".into(), Value::Bool(true)));
            }
        }
    }
    Ok((prompt, stop))
}
/// Native owned command, separately constructed from legacy recognizers.
pub fn rust_era_command(am: &Path, name: &str) -> Result<String> {
    let path = am.to_str().ok_or_else(|| err("Non-UTF-8 hook path"))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(err("Invalid owned hook command"));
    }
    Ok(format!("{} hook {}", shell_quote(path), name))
}
