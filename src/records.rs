//! Standalone Markdown records and optional history format contracts.
use crate::codec::{
    frontmatter,
    json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value},
};
use crate::developer_identity::uuid_valid;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string, time_value};
use crate::scopes;
use std::sync::{Arc, LazyLock, RwLock};

fn schema(s: &str) -> Value {
    // Invariant: every caller passes a schema compiled in from the pack (decoded by tests).
    json::decode(
        s,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled record schema")
}
pub static MODEL: LazyLock<Value> =
    LazyLock::new(|| schema(include_str!("../skills/agent-memory/schemas/record.json")));
pub static EVENT: LazyLock<Value> = LazyLock::new(|| {
    schema(include_str!(
        "../skills/agent-memory/schemas/history-event.json"
    ))
});
pub static TOMBSTONE: LazyLock<Value> = LazyLock::new(|| {
    schema(include_str!(
        "../skills/agent-memory/schemas/tombstone.json"
    ))
});

/// Later-tier tombstone parser; registration is required before tombstone dispatch.
pub trait TombstoneParser: Send + Sync {
    fn parse(&self, content: &str) -> Result<Value>;
}
static TOMBSTONE_PARSER: RwLock<Option<Arc<dyn TombstoneParser>>> = RwLock::new(None);
pub fn register_tombstone_parser(parser: Option<Arc<dyn TombstoneParser>>) {
    *TOMBSTONE_PARSER.write().unwrap_or_else(|e| e.into_inner()) = parser;
}
fn err(s: &str) -> ContractError {
    ContractError::new(s)
}
fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
pub fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| {
            !trim(s).is_empty() && trim(s) == *s && !s.chars().any(|c| c < ' ' || c == '\u{7f}')
        })
        .ok_or_else(|| err("A nonempty explicit metadata string is required"))
}
pub fn fields(value: &Value, required: &[&str], allowed: &[&str]) -> Result<()> {
    let entries = value
        .as_object()
        .ok_or_else(|| err("Metadata requires the known fields and no extras"))?;
    if required.iter().any(|k| value.get(k).is_none())
        || entries.iter().any(|(k, _)| !allowed.contains(&k.as_str()))
    {
        return Err(err("Metadata requires the known fields and no extras"));
    }
    Ok(())
}
fn schema_fields(value: &Value, spec: &Value) -> Result<()> {
    let required: Vec<&str> = get(spec, "required")
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let allowed: Vec<&str> = get(spec, "properties")
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.as_str())
        .collect();
    fields(value, &required, &allowed)
}
/// Text of a value already accepted by `identity`, `text` or `reference`.
fn accepted_text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| err("A nonempty explicit metadata string is required"))
}
fn member(array: &Value, item: &Value) -> bool {
    array.as_array().is_some_and(|a| a.contains(item))
}
fn enum_member(spec: &Value, field: &str, value: &Value) -> bool {
    member(get(get(get(spec, "properties"), field), "enum"), value)
}
pub fn identity(value: &Value) -> Result<()> {
    if uuid_valid(value) {
        Ok(())
    } else {
        Err(err("Canonical random UUIDv4 identity required"))
    }
}
pub fn reference(value: &Value) -> Result<&Value> {
    fields(value, &["id", "version"], &["id", "version"])?;
    identity(get(value, "id"))?;
    if !get(value, "version")
        .as_int()
        .is_some_and(|v| v.as_str() != "0" && !v.as_str().starts_with('-'))
    {
        return Err(err("An exact positive record version is required"));
    }
    Ok(value)
}
pub fn captured_source_path(capture_id: &Value, source_id: &Value) -> Result<String> {
    identity(capture_id)?;
    identity(source_id)?;
    Ok(format!(
        "history/sources/{}/{}.md",
        accepted_text(capture_id)?,
        accepted_text(source_id)?
    ))
}
pub fn source_capture_id(path: &str, metadata: &Value) -> Option<String> {
    let parts: Vec<_> = path.split('/').collect();
    let ["history", "sources", capture, leaf] = parts.as_slice() else {
        return None;
    };
    if !uuid_valid(&string(*capture)) || *leaf != format!("{}.md", get(metadata, "id").as_str()?) {
        return None;
    }
    let found = get(metadata, "derived-from")
        .as_array()
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.as_object().is_some_and(|o| o.len() == 2)
                    && get(item, "id").as_str() == Some(*capture)
                    && match get(item, "version") {
                        Value::Int(n) => n.as_str() == "1",
                        Value::Float(n) => *n == 1.0,
                        Value::Bool(b) => *b,
                        _ => false,
                    }
            })
        });
    if found { Some((*capture).into()) } else { None }
}
fn version_digits(value: &Value) -> Result<&str> {
    value
        .as_int()
        .map(|v| v.as_str())
        .ok_or_else(|| err("An exact positive record version is required"))
}
/// Compares two positive canonical JSON integers (no sign, no leading zeros).
fn int_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
pub fn references(
    values: &Value,
    record: &Value,
    replacement: bool,
    correction: bool,
) -> Result<()> {
    let items = values
        .as_array()
        .ok_or_else(|| err("Version links must be a list"))?;
    let mut seen = std::collections::HashSet::new();
    for item in items {
        reference(item)?;
        let id = accepted_text(get(item, "id"))?;
        let ver = version_digits(get(item, "version"))?;
        if !seen.insert((id, ver)) {
            return Err(err("Duplicate version link"));
        }
        let same = get(item, "id") == get(record, "id");
        // Python compares versions only for a link to the same record.
        let order = if same {
            Some(int_cmp(ver, version_digits(get(record, "version"))?))
        } else {
            None
        };
        if let Some(cmp) = order {
            if (replacement && cmp != std::cmp::Ordering::Greater)
                || (!replacement && cmp != std::cmp::Ordering::Less)
            {
                return Err(err(
                    "Self-version links must point to the proper earlier or replacement version",
                ));
            }
        }
        if correction && order != Some(std::cmp::Ordering::Less) {
            return Err(err(
                "A correction must retain the record ID and name an earlier version",
            ));
        }
    }
    Ok(())
}
pub fn evidence(values: &Value) -> Result<()> {
    let values = values
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| err("At least one evidence reference is required"))?;
    for value in values {
        text(value)?;
    }
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(accepted_text(value)?) {
            return Err(err("Duplicate evidence reference"));
        }
    }
    Ok(())
}
pub fn record<'a>(metadata: &'a Value, required_metadata: &[&str]) -> Result<&'a Value> {
    schema_fields(metadata, &MODEL)?;
    if get(metadata, "schema") != get(&MODEL, "$id") {
        return Err(err("Unknown record schema"));
    }
    reference(&object([
        ("id", get(metadata, "id").clone()),
        ("version", get(metadata, "version").clone()),
    ]))?;
    identity(get(metadata, "project-id"))?;
    for key in ["kind", "status", "applicability"] {
        if !enum_member(&MODEL, key, get(metadata, key)) {
            return Err(err("Unknown record kind, lifecycle or applicability"));
        }
    }
    let scope = scopes::parse_scope(get(metadata, "scope"))?;
    let conditions = scopes::conditions(get(metadata, "conditions"), true)?;
    let source = get(metadata, "source");
    fields(source, &["actor", "kind"], &["actor", "kind"])?;
    text(get(source, "actor"))?;
    text(get(source, "kind"))?;
    evidence(get(metadata, "evidence"))?;
    let origin = get(metadata, "origin");
    let spec = get(get(&MODEL, "properties"), "origin");
    schema_fields(origin, spec)?;
    identity(get(origin, "developer-id"))?;
    text(get(origin, "harness"))?;
    text(get(origin, "session"))?;
    if !enum_member(spec, "code-state", get(origin, "code-state")) {
        return Err(err("Explicit originating code state required"));
    }
    let conditional = scope.as_ref().is_some_and(|(k, _)| k == "branch")
        || ["branch", "revision"]
            .iter()
            .any(|k| conditions.get(k).is_some_and(|v| *v != Value::Null));
    let unknown = get(metadata, "applicability").as_str() == Some("unknown")
        || conditions
            .as_object()
            .into_iter()
            .flatten()
            .any(|(_, v)| *v == Value::Null);
    if get(origin, "code-state").as_str() == Some("unmerged") && !conditional && !unknown {
        return Err(err(
            "Unmerged observations require explicit branch/revision conditions or unknown applicability",
        ));
    }
    if time_value(get(metadata, "updated"))? < time_value(get(metadata, "recorded-at"))? {
        return Err(err("Updated time cannot precede recording"));
    }
    let start = if *get(metadata, "valid-from") == Value::Null {
        None
    } else {
        Some(time_value(get(metadata, "valid-from"))?)
    };
    let end = if *get(metadata, "valid-until") == Value::Null {
        None
    } else {
        Some(time_value(get(metadata, "valid-until"))?)
    };
    if start.is_some() && end.is_some() && end <= start {
        return Err(err("Validity must have a strictly later exclusive end"));
    }
    for key in ["derived-from", "corrects", "supersedes"] {
        references(get(metadata, key), metadata, false, key == "corrects")?;
    }
    if required_metadata
        .iter()
        .any(|k| !member(get(&MODEL, "x-policy-fields"), &string(*k)))
    {
        return Err(err("Unknown policy-required metadata field"));
    }
    if required_metadata.iter().any(|k| metadata.get(k).is_none())
        || (get(metadata, "status").as_str() == Some("active")
            && ["admission-reason", "trust", "visibility"]
                .iter()
                .any(|k| metadata.get(k).is_none()))
    {
        return Err(err("Policy-required metadata is missing"));
    }
    for key in ["admission-reason", "redaction", "state-reason"] {
        if let Some(v) = metadata.get(key) {
            text(v)?;
        }
    }
    for key in ["trust", "visibility"] {
        if let Some(v) = metadata.get(key) {
            if !enum_member(&MODEL, key, v) {
                return Err(err("Unknown trust or visibility"));
            }
        }
    }
    if let Some(v) = metadata.get("confidence") {
        let valid = match v {
            Value::Null => true,
            Value::Int(i) => matches!(i.as_str(), "0" | "-0" | "1"),
            Value::Float(f) => f.is_finite() && *f >= 0.0 && *f <= 1.0,
            _ => false,
        };
        if !valid {
            return Err(err(
                "Confidence must be null or finite in the range zero to one",
            ));
        }
    }
    let status = get(metadata, "status").as_str().unwrap_or_default();
    if ["quarantined", "superseded", "revoked", "expired"].contains(&status)
        && metadata.get("state-reason").is_none()
    {
        return Err(err("Non-current lifecycle needs a visible reason"));
    }
    let replacement = metadata.get("superseded-by");
    let truthy = crate::sharing_limits::truthy;
    if status == "superseded" && !replacement.is_some_and(truthy) {
        return Err(err("Superseded records must name the replacement"));
    }
    if let Some(v) = replacement {
        if !truthy(v) {
            return Err(err("Replacement links cannot be empty"));
        }
        references(v, metadata, true, false)?;
    }
    Ok(metadata)
}
pub fn event(metadata: &Value) -> Result<&Value> {
    schema_fields(metadata, &EVENT)?;
    if get(metadata, "schema") != get(&EVENT, "$id")
        || !enum_member(&EVENT, "kind", get(metadata, "kind"))
    {
        return Err(err(
            "Unknown historical event; deletion uses only a safe tombstone",
        ));
    }
    identity(get(metadata, "id"))?;
    identity(get(metadata, "project-id"))?;
    reference(get(metadata, "record"))?;
    text(get(metadata, "actor"))?;
    time_value(get(metadata, "recorded-at"))?;
    evidence(get(metadata, "evidence"))?;
    Ok(metadata)
}
pub fn validate_with_parser(
    metadata: &Value,
    body: &str,
    required_metadata: &[&str],
    parser: Option<&dyn TombstoneParser>,
) -> Result<(String, Value)> {
    let entries = metadata
        .as_object()
        .ok_or_else(|| err("Explicit metadata and Markdown body are required"))?;
    let keys: Vec<&str> = get(&TOMBSTONE, "required")
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if entries.len() == keys.len() && keys.iter().all(|k| metadata.get(k).is_some()) {
        if !required_metadata.is_empty() {
            return Err(err(
                "Tombstones must not retain policy or provenance fields",
            ));
        }
        let parser = parser
            .ok_or_else(|| ContractError::missing_provider("Tombstone parser is not registered"))?;
        return Ok((
            "tombstone".into(),
            parser.parse(&(frontmatter::write(entries)? + body))?,
        ));
    }
    if get(metadata, "schema") == get(&EVENT, "$id") {
        if !trim(body).is_empty() || !required_metadata.is_empty() {
            return Err(err(
                "Optional history events have empty bodies and their own format",
            ));
        }
        return Ok(("optional-history-event".into(), event(metadata)?.clone()));
    }
    if trim(body).is_empty() {
        return Err(err(
            "Standalone current memory requires a readable Markdown body",
        ));
    }
    Ok((
        "standalone-record".into(),
        record(metadata, required_metadata)?.clone(),
    ))
}
pub fn validate(
    metadata: &Value,
    body: &str,
    required_metadata: &[&str],
) -> Result<(String, Value)> {
    let parser = TOMBSTONE_PARSER
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    validate_with_parser(metadata, body, required_metadata, parser.as_deref())
}
pub fn parse(content: &str, required_metadata: &[&str]) -> Result<(String, Value, String)> {
    let mut allowed = Vec::new();
    for spec in [&*MODEL, &*EVENT] {
        for (k, _) in get(spec, "properties").as_object().into_iter().flatten() {
            if !allowed.contains(&k.as_str()) {
                allowed.push(k.as_str());
            }
        }
    }
    let tombstone = get(&TOMBSTONE, "required").as_array().into_iter().flatten();
    for k in tombstone.filter_map(Value::as_str) {
        if !allowed.contains(&k) {
            allowed.push(k);
        }
    }
    let (meta, body) = frontmatter::read(content, &allowed)?;
    let (kind, meta) = validate(&Value::Object(meta), body, required_metadata)?;
    Ok((kind, meta, body.into()))
}
pub fn applicability(metadata: &Value, request: &Value) -> Result<Value> {
    fields(
        request,
        &["bank_project_id", "context"],
        &["bank_project_id", "context"],
    )?;
    record(metadata, &[])?;
    let mut result = scopes::decide(&object([
        ("bank_project_id", get(request, "bank_project_id").clone()),
        ("project_id", get(metadata, "project-id").clone()),
        ("scope", get(metadata, "scope").clone()),
        ("conditions", get(metadata, "conditions").clone()),
        ("context", get(request, "context").clone()),
    ]))?;
    if get(metadata, "applicability").as_str() == Some("unknown") {
        set(&mut result, "applicable", Value::Bool(false));
        if let Some(Value::Array(a)) = result.get_mut("reasons") {
            a.push(string("unknown-applicability"));
        }
    }
    set(
        &mut result,
        "validation_scope",
        string("record-applicability-only; not lifecycle-currentness-or-admission"),
    );
    Ok(result)
}
pub fn definition() -> Value {
    object([
        ("record", MODEL.clone()),
        ("optional_history_event", EVENT.clone()),
        ("tombstone", TOMBSTONE.clone()),
        ("writes_performed", Value::Bool(false)),
    ])
}
/// Inputs are already read by the CLI tier; read/UTF-8 failures should be passed as errors.
pub fn command(
    operation: &str,
    input: Result<Option<Value>>,
    content: Result<Option<String>>,
    required: &[&str],
) -> crate::lifecycle::CommandResult {
    let result = (|| -> Result<Value> {
        if operation == "model" {
            return Ok(definition());
        }
        let (kind, metadata, body) = if operation == "render" {
            let input = input.as_ref().map_err(|_| err("Invalid input"))?;
            let req = input.as_ref().ok_or_else(|| err("Missing input"))?;
            fields(req, &["metadata", "body"], &["metadata", "body"])?;
            let body = get(req, "body")
                .as_str()
                .ok_or_else(|| err("Explicit metadata and Markdown body are required"))?;
            let (kind, metadata) = validate(get(req, "metadata"), body, required)?;
            (kind, metadata, body.to_owned())
        } else {
            parse(
                content?.as_deref().ok_or_else(|| err("Missing file"))?,
                required,
            )?
        };
        if operation == "applicability" {
            if kind != "standalone-record" {
                return Err(err(
                    "Only standalone records have conditional applicability",
                ));
            }
            let input = input?;
            return applicability(
                &metadata,
                input.as_ref().ok_or_else(|| err("Missing input"))?,
            );
        }
        if operation != "render" && operation != "validate" {
            return Err(err("Unknown operation"));
        }
        let mut out = vec![
            ("schema".into(), string("agent-memory/record-result-v1")),
            ("format".into(), string(kind)),
            ("metadata".into(), metadata.clone()),
            ("body".into(), string(&body)),
            ("valid".into(), Value::Bool(true)),
            ("writes_performed".into(), Value::Bool(false)),
            (
                "validation_scope".into(),
                string("format-and-local-invariants"),
            ),
            ("identity_authenticated".into(), Value::Bool(false)),
            ("sources_verified".into(), Value::Bool(false)),
            ("admission_verified".into(), Value::Bool(false)),
        ];
        if operation == "render" {
            let entries = metadata
                .as_object()
                .ok_or_else(|| err("Explicit metadata and Markdown body are required"))?;
            out.push((
                "content".into(),
                string(frontmatter::write(entries)? + &body),
            ));
        }
        Ok(Value::Object(out))
    })();
    let failure = || {
        object([
            ("schema", string("agent-memory/record-result-v1")),
            ("valid", Value::Bool(false)),
            ("writes_performed", Value::Bool(false)),
            (
                "error",
                string(
                    "Invalid record, history event, tombstone or applicability request; inspect the original input",
                ),
            ),
        ])
    };
    // Python encodes inside its try block: an unencodable success document
    // (non-finite float, depth) takes the same fixed refusal.
    match result {
        Ok(value) if json::encode(&value, json::INDENT2_UTF8_FINITE).is_ok() => {
            crate::lifecycle::CommandResult::json(value, 0, json::INDENT2_UTF8_FINITE)
        }
        _ => crate::lifecycle::CommandResult::json(failure(), 1, json::DEFAULT),
    }
}
fn set(result: &mut Value, key: &str, value: Value) {
    if let Some(slot) = result.get_mut(key) {
        *slot = value;
    }
}
