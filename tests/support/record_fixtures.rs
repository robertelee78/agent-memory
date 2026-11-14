//! Structural fixture data, not admission evidence or implementation-generated oracles.
use super::world::{Result, WorldError};
use agent_memory::codec::json::{self, Value};

const TEMPLATE: &str =
    include_str!("../../skills/agent-memory/templates/record-proposal.example.json");

pub struct RecordOptions<'a> {
    pub scope: &'a str,
    pub kind: &'a str,
    pub status: &'a str,
}

impl Default for RecordOptions<'_> {
    fn default() -> Self {
        Self {
            scope: "repository",
            kind: "semantic-fact",
            status: "active",
        }
    }
}

fn set(object: &mut Value, key: &str, value: Value) -> Result<()> {
    let Value::Object(pairs) = object else {
        return Err(WorldError::invalid("Fixture template object is missing"));
    };
    if let Some((_, previous)) = pairs.iter_mut().find(|(name, _)| name == key) {
        *previous = value;
    } else {
        pairs.push((key.to_owned(), value));
    }
    Ok(())
}

pub fn record_text(body: &str, project_id: &str) -> Result<String> {
    record_text_with(body, project_id, RecordOptions::default())
}

pub fn record_text_with(
    body: &str,
    project_id: &str,
    options: RecordOptions<'_>,
) -> Result<String> {
    let id = agent_memory::host::uuid4()?;
    let developer = agent_memory::host::uuid4()?;
    record_text_with_ids(body, project_id, options, &id, &developer)
}

/// Deterministic UUID injection lets tests compare baseline bytes without normalizing JSON.
pub fn record_text_with_ids(
    body: &str,
    project_id: &str,
    options: RecordOptions<'_>,
    id: &str,
    developer: &str,
) -> Result<String> {
    let mut proposal = json::decode(TEMPLATE, json::DecodePolicy::default())?;
    let metadata = proposal
        .get_mut("metadata")
        .ok_or_else(|| WorldError::invalid("Fixture template metadata is missing"))?;
    for (key, value) in [
        ("id", id),
        ("project-id", project_id),
        ("kind", options.kind),
        ("status", options.status),
        ("scope", options.scope),
        ("trust", "reviewed"),
        ("visibility", "visible"),
        (
            "admission-reason",
            "Disposable structural fixture; not real admission evidence",
        ),
    ] {
        set(metadata, key, Value::String(value.to_owned()))?;
    }
    set(metadata, "conditions", Value::Object(Vec::new()))?;
    let origin = metadata
        .get_mut("origin")
        .ok_or_else(|| WorldError::invalid("Fixture template origin is missing"))?;
    set(origin, "developer-id", Value::String(developer.to_owned()))?;
    set(origin, "code-state", Value::String("not-code".to_owned()))?;
    if matches!(
        options.status,
        "quarantined" | "superseded" | "revoked" | "expired"
    ) {
        set(
            metadata,
            "state-reason",
            Value::String("Historical fixture; excluded from current guidance".to_owned()),
        )?;
    }
    if options.status == "superseded" {
        set(
            metadata,
            "superseded-by",
            Value::Array(vec![Value::Object(vec![
                ("id".to_owned(), Value::String(id.to_owned())),
                ("version".to_owned(), Value::Int(2.into())),
            ])]),
        )?;
    }
    let fields = metadata
        .as_object()
        .ok_or_else(|| WorldError::invalid("Fixture template metadata is not an object"))?;
    let mut text = String::from("---\n");
    for (key, value) in fields {
        text.push_str(key);
        text.push_str(": ");
        text.push_str(&json::encode(value, json::DEFAULT)?);
        text.push('\n');
    }
    text.push_str("---\n");
    text.push_str(body);
    Ok(text)
}

pub fn project_id(router: &str) -> Result<Value> {
    let value = router
        .lines()
        .find_map(|line| line.strip_prefix("project-id: "))
        .ok_or_else(|| WorldError::invalid("Router has no project-id field"))?;
    Ok(json::decode(value, json::DecodePolicy::default())?)
}
