//! Repository namespace and applicability decisions; not an access ACL.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::developer_identity::uuid_valid;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use std::sync::LazyLock;

pub static MODEL: LazyLock<Value> = LazyLock::new(|| {
    // Invariant: the policy is compiled in from the pack and decoded by a test.
    json::decode(
        include_str!("../skills/agent-memory/schemas/scope-policy.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled scope policy")
});

fn member(key: &str, value: &str) -> bool {
    get(&MODEL, key)
        .as_array()
        .into_iter()
        .flatten()
        .any(|v| v.as_str() == Some(value))
}
fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

pub fn condition_value(kind: &str, value: &Value, allow_unknown: bool) -> Result<Value> {
    if allow_unknown && member("condition-kinds", kind) && *value == Value::Null {
        return Ok(Value::Null);
    }
    let invalid = || {
        ContractError::new("Applicability requires an explicit known condition and nonempty value")
    };
    let s = value.as_str().ok_or_else(invalid)?;
    if !member("condition-kinds", kind)
        || s.is_empty()
        || trim(s) != s
        || s.chars()
            .any(|c| c < ' ' || c == '\u{7f}' || c == '|' || c == '\\')
    {
        return Err(invalid());
    }
    if kind == "component"
        && (s.starts_with('/')
            || s == "."
            || s.contains(':')
            || s.split('/').any(|p| p.is_empty() || p == "." || p == ".."))
    {
        return Err(ContractError::new(
            "Component conditions require canonical repository-relative paths",
        ));
    }
    Ok(value.clone())
}

pub fn parse_scope(scope: &Value) -> Result<Option<(String, Value)>> {
    if scope == get(&MODEL, "default-scope") {
        return Ok(None);
    }
    let (kind, value) = scope
        .as_str()
        .and_then(|s| s.split_once(':'))
        .ok_or_else(|| {
            ContractError::new("Scope must be repository or an explicit applicability condition")
        })?;
    if !member("route-kinds", kind) {
        return Err(ContractError::new("Scope requires a known route kind"));
    }
    Ok(Some((
        kind.to_owned(),
        condition_value(kind, &string(value), false)?,
    )))
}

pub fn conditions(value: &Value, allow_unknown: bool) -> Result<Value> {
    let entries = value
        .as_object()
        .ok_or_else(|| ContractError::new("Conditions and observed context must be objects"))?;
    Ok(Value::Object(
        entries
            .iter()
            .map(|(k, v)| Ok((k.clone(), condition_value(k, v, allow_unknown)?)))
            .collect::<Result<Vec<_>>>()?,
    ))
}

pub fn decide(request: &Value) -> Result<Value> {
    let invalid = || {
        ContractError::new(
            "Scope decisions require explicit canonical project IDs and known fields",
        )
    };
    let entries = request.as_object().ok_or_else(invalid)?;
    if entries.iter().any(|(k, _)| {
        ![
            "bank_project_id",
            "project_id",
            "scope",
            "conditions",
            "context",
            "developer_id",
        ]
        .contains(&k.as_str())
    }) || !uuid_valid(get(request, "bank_project_id"))
        || !uuid_valid(get(request, "project_id"))
        || (request.get("developer_id").is_some() && !uuid_valid(get(request, "developer_id")))
    {
        return Err(invalid());
    }
    let scope = request.get("scope").unwrap_or(get(&MODEL, "default-scope"));
    let parsed = parse_scope(scope)?;
    let empty = object(Vec::<(String, Value)>::new());
    let requested = conditions(request.get("conditions").unwrap_or(&empty), true)?;
    let mut requirements = requested.as_object().map(<[_]>::to_vec).unwrap_or_default();
    if let Some(pair) = parsed {
        requirements.insert(0, pair);
    }
    let context = conditions(request.get("context").unwrap_or(&empty), false)?;
    let same = get(request, "bank_project_id") == get(request, "project_id");
    let mut reasons = Vec::<String>::new();
    if !same {
        reasons.push("different-project".into());
    }
    for (kind, expected) in requirements {
        let actual = context.get(&kind);
        let reason = match (expected.as_str(), actual.and_then(Value::as_str)) {
            _ if expected == Value::Null => Some("unknown-condition"),
            (_, None) => Some("missing-context"),
            (Some(wanted), Some(observed)) => {
                let inside = observed
                    .strip_prefix(wanted)
                    .is_some_and(|s| s.starts_with('/'));
                let differs = if kind == "component" {
                    observed != wanted && !inside
                } else {
                    observed != wanted
                };
                differs.then_some("different-context")
            }
            // condition_value admits only strings or (unknown) null.
            (None, Some(_)) => Some("different-context"),
        };
        if let Some(reason) = reason {
            let item = format!("{reason}:{kind}");
            if !reasons.contains(&item) {
                reasons.push(item);
            }
        }
    }
    Ok(object([
        ("schema", string("agent-memory/scope-decision-v1")),
        ("same_project", Value::Bool(same)),
        ("scope", scope.clone()),
        ("applicable", Value::Bool(reasons.is_empty())),
        (
            "reasons",
            Value::Array(reasons.into_iter().map(string).collect()),
        ),
        ("validation_scope", string("repository-applicability-only")),
        ("identity_authenticated", Value::Bool(false)),
        ("writes_performed", Value::Bool(false)),
    ]))
}

pub fn model() -> Value {
    object([
        ("model", MODEL.clone()),
        ("writes_performed", Value::Bool(false)),
    ])
}
pub fn command_model() -> crate::lifecycle::CommandResult {
    command_result(Ok(model()))
}
pub fn command_decide(request: Result<Value>) -> crate::lifecycle::CommandResult {
    command_result(request.and_then(|v| decide(&v)))
}
fn command_result(result: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match result {
        Ok(v) => (v, 0, json::INDENT2_ASCII),
        Err(e) => (
            object([
                ("schema", string("agent-memory/scope-decision-v1")),
                ("valid", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                ("error", string(e.to_string())),
            ]),
            1,
            json::DEFAULT,
        ),
    };
    crate::lifecycle::CommandResult::json(value, exit_code, style)
}
