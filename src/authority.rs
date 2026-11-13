//! Read-only triage of reviewed assertions.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::developer_identity::uuid_valid;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::scopes;
use std::sync::LazyLock;
pub static MODEL: LazyLock<Value> = LazyLock::new(|| {
    json::decode(
        include_str!("../skills/agent-memory/schemas/authority-policy.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled authority policy")
});
pub fn text(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| !s.trim().is_empty() && !s.chars().any(|c| c < ' '))
}
pub fn comparable(value: &Value) -> Result<String> {
    json::encode(value, json::SORTED_COMPACT_UTF8_FINITE)
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object().is_some_and(|o| {
        o.len() == fields.len() && o.iter().all(|(k, _)| fields.contains(&k.as_str()))
    })
}
fn has(v: &Value, key: &str) -> bool {
    v.get(key).is_some()
}
pub fn assess(request: &Value) -> Result<Value> {
    let bad = || {
        ContractError::new(
            "Authority triage requires a known fact, explicit project/detector and supported remedy",
        )
    };
    let required = [
        "fact",
        "fact_key",
        "project_id",
        "memory",
        "sources",
        "detecting_developer_id",
    ];
    if !request.as_object().is_some_and(|o| {
        o.iter()
            .all(|(k, _)| required.contains(&k.as_str()) || k == "remedy")
    }) || required.iter().any(|k| !has(request, k))
        || !text(get(request, "fact_key"))
        || !uuid_valid(get(request, "project_id"))
        || !uuid_valid(get(request, "detecting_developer_id"))
        || !["correct", "revoke"].contains(
            &request
                .get("remedy")
                .and_then(Value::as_str)
                .unwrap_or(if has(request, "remedy") {
                    ""
                } else {
                    "correct"
                }),
        )
    {
        return Err(bad());
    }
    let domain = get(&MODEL, "domains")
        .as_array()
        .unwrap()
        .iter()
        .find(|d| get(d, "fact") == get(request, "fact"))
        .ok_or_else(bad)?;
    let memory = get(request, "memory");
    let fields = [
        "id",
        "version",
        "project_id",
        "value",
        "scope",
        "conditions",
        "developer_id",
        "evidence",
    ];
    let valid_version = get(memory, "version")
        .as_int()
        .is_some_and(|n| !n.as_str().starts_with('-') && n.as_str() != "0");
    if !exact(memory, &fields)
        || !uuid_valid(get(memory, "id"))
        || get(memory, "project_id") != get(request, "project_id")
        || !uuid_valid(get(memory, "developer_id"))
        || !valid_version
        || !get(memory, "evidence")
            .as_array()
            .is_some_and(|a| a.iter().all(text))
    {
        return Err(ContractError::new(
            "A correction candidate must name the existing project record, version and provenance",
        ));
    }
    scopes::parse_scope(get(memory, "scope"))?;
    scopes::conditions(get(memory, "conditions"), false)?;
    let remembered = comparable(get(memory, "value"))?;
    let sources = get(request, "sources")
        .as_array()
        .ok_or_else(|| ContractError::new("Reviewed source assertions must be a list"))?;
    let mut selected = Vec::new();
    let mut observed = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for source in sources {
        let kind = get(source, "kind");
        let known = kind.as_str() == Some("memory")
            || get(&MODEL, "domains")
                .as_array()
                .unwrap()
                .iter()
                .any(|d| get(d, "source-kinds").as_array().unwrap().contains(kind));
        let r = get(source, "ref");
        if !exact(
            source,
            &[
                "kind",
                "ref",
                "project_id",
                "fact_key",
                "current",
                "applicable",
                "qualified",
                "value",
            ],
        ) || !known
            || !text(r)
            || !seen.insert(r.as_str().unwrap())
            || get(source, "project_id") != get(request, "project_id")
            || get(source, "fact_key") != get(request, "fact_key")
            || ["current", "applicable", "qualified"]
                .iter()
                .any(|k| get(source, k).as_bool().is_none())
        {
            return Err(ContractError::new(
                "Sources require unique references and explicit same-project, same-fact review assertions",
            ));
        }
        comparable(get(source, "value"))?;
        observed.push(r.clone());
        if get(domain, "source-kinds")
            .as_array()
            .unwrap()
            .contains(kind)
            && ["current", "applicable", "qualified"]
                .iter()
                .all(|k| get(source, k).as_bool() == Some(true))
        {
            selected.push(source);
        }
    }
    let refs = Value::Array(selected.iter().map(|s| get(s, "ref").clone()).collect());
    let mut result = object([
        ("schema", string("agent-memory/authority-assessment-v1")),
        ("fact", get(request, "fact").clone()),
        ("authority", get(domain, "authority").clone()),
        ("source_refs", refs.clone()),
        ("observed_refs", Value::Array(observed)),
        ("memory_is_proof", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
        ("identity_authenticated", Value::Bool(false)),
        ("writes_performed", Value::Bool(false)),
        ("validation_scope", string("reviewed-assertion-triage")),
        ("correction_candidate", Value::Null),
        ("clarification", Value::Null),
    ]);
    let action = if selected.is_empty() {
        "inspect-current-authority".to_owned()
    } else {
        let first = comparable(get(selected[0], "value"))?;
        if selected
            .iter()
            .skip(1)
            .any(|s| comparable(get(s, "value")).map_or(true, |v| v != first))
        {
            *result.get_mut("clarification").unwrap() = object([
                (
                    "detecting_developer_id",
                    get(request, "detecting_developer_id").clone(),
                ),
                (
                    "known_other_developer_ids",
                    Value::Array(
                        if get(memory, "developer_id") == get(request, "detecting_developer_id") {
                            vec![]
                        } else {
                            vec![get(memory, "developer_id").clone()]
                        },
                    ),
                ),
                (
                    "question",
                    string("Which applicable current source resolves this project fact?"),
                ),
            ]);
            "clarify".to_owned()
        } else if remembered == first {
            "use-current-authority".to_owned()
        } else {
            let remedy = request
                .get("remedy")
                .and_then(Value::as_str)
                .unwrap_or("correct");
            let mut candidate = object([
                ("operation", string(remedy)),
                ("target", memory.clone()),
                ("project_id", get(request, "project_id").clone()),
                ("fact_key", get(request, "fact_key").clone()),
                ("evidence", refs),
                (
                    "detected_by",
                    get(request, "detecting_developer_id").clone(),
                ),
                (
                    "reason",
                    string("Remembered claim disagrees with applicable current authority"),
                ),
            ]);
            if remedy == "correct" {
                if let Value::Object(pairs) = &mut candidate {
                    pairs.push(("proposed_value".into(), get(selected[0], "value").clone()));
                }
            }
            *result.get_mut("correction_candidate").unwrap() = candidate;
            format!("propose-{remedy}")
        }
    };
    if let Value::Object(pairs) = &mut result {
        pairs.push(("action".into(), string(action)));
    }
    Ok(result)
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
pub fn command_assess(request: Result<Value>) -> crate::lifecycle::CommandResult {
    command_result(request.and_then(|v| assess(&v)))
}
fn command_result(result: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match result {
        Ok(v) => (v, 0, json::INDENT2_UTF8),
        Err(e) => (
            object([
                ("schema", string("agent-memory/authority-assessment-v1")),
                ("valid", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                ("error", string(e.to_string())),
            ]),
            1,
            json::DEFAULT,
        ),
    };
    let stdout = json::encode(&value, style).expect("authority result encoding") + "\n";
    crate::lifecycle::CommandResult {
        value: Some(value),
        exit_code,
        stdout,
        stderr: String::new(),
    }
}
