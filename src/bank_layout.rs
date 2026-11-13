//! Optional-record inventory and bounded topic-index contract.
use crate::bank_contract::{self as bank, ParsedRecords};
use crate::codec::{
    frontmatter,
    json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value},
};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::{bank_capacity, records, scopes};
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

pub static SPEC: LazyLock<Value> = LazyLock::new(|| {
    // Invariant: include_str! embeds the reviewed, valid topic-index schema.
    json::decode(
        include_str!("../skills/agent-memory/schemas/topic-index.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled topic index schema")
});
pub const MAX_INDEX_BYTES: usize = 16_384;
pub const MAX_NESTED_INDEXES: usize = 2;
const INDEX_FIELDS: &[&str] = &["schema", "project-id", "updated"];
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
pub fn index_header(metadata: &Value, path: &str) -> Result<()> {
    bank::path_value(&string(path))?;
    if !path.starts_with("topics/") {
        return Err(err("Topic indices must remain in topics/"));
    }
    records::fields(metadata, INDEX_FIELDS, INDEX_FIELDS)?;
    if get(metadata, "schema") != get(&SPEC, "$id")
        || !crate::developer_identity::uuid_valid(get(metadata, "project-id"))
    {
        return Err(err("Unknown topic index schema or project identity"));
    }
    crate::lifecycle::time_value(get(metadata, "updated"))?;
    Ok(())
}
pub fn index_text(path: &str, metadata: &Value, routes: &[Value]) -> Result<String> {
    index_header(metadata, path)?;
    if routes.is_empty() || routes.len() > bank::MAX_ROUTES {
        return Err(err("A useful bounded topic index requires explicit routes"));
    }
    if routes.iter().any(|r| {
        get(r, "path")
            .as_str()
            .is_some_and(|p| bank::CONTEXTS.contains(&p))
    }) {
        return Err(err(
            "The six context files must be routed directly from MEMORY.md",
        ));
    }
    let content = frontmatter::write(
        metadata
            .as_object()
            .ok_or_else(|| err("Metadata requires the known fields and no extras"))?,
    )? + "# Topic index\n\nLabels name bank-relative files; links are relative to this index.\n\n## Routes\n\n"
        + &bank::route_table(routes, path)?;
    let (_, body) = content
        .split_once("\n---\n")
        .ok_or_else(|| err("Missing frontmatter terminator"))?;
    parse_index(path, metadata, body)?;
    Ok(content)
}
pub fn parse_index(path: &str, metadata: &Value, body: &str) -> Result<Vec<Value>> {
    index_header(metadata, path)?;
    if frontmatter::write(
        metadata
            .as_object()
            .ok_or_else(|| err("Metadata requires the known fields and no extras"))?,
    )?
    .len()
        + body.len()
        > MAX_INDEX_BYTES
        || body.matches("\n## Routes\n").count() != 1
    {
        return Err(err("Topic index requires one bounded route table"));
    }
    let (_, table) = body
        .split_once("\n## Routes\n")
        .ok_or_else(|| err("Topic index requires one bounded route table"))?;
    let routes = bank::parse_table(table, path)?;
    if routes.is_empty()
        || routes.iter().any(|r| {
            get(r, "path")
                .as_str()
                .is_some_and(|p| bank::CONTEXTS.contains(&p))
        })
    {
        return Err(err("Topic indices require useful optional-record routes"));
    }
    Ok(routes)
}
fn within(child: &str, parent: &str) -> bool {
    child
        .strip_prefix(parent)
        .is_some_and(|s| s.starts_with('/'))
}
pub fn constraints(metadata: &Value) -> Result<Value> {
    let mut values = Value::Object(
        get(metadata, "conditions")
            .as_object()
            .ok_or_else(|| err("Conditions and observed context must be objects"))?
            .iter()
            .filter(|(_, v)| *v != Value::Null)
            .cloned()
            .collect(),
    );
    if let Some((kind, value)) = scopes::parse_scope(get(metadata, "scope"))? {
        if let Some(old) = values
            .get(&kind)
            .filter(|v| **v != Value::Null && **v != value)
        {
            if kind != "component" {
                return Err(err("Record scope contradicts its conditions"));
            }
            let old_text = old
                .as_str()
                .ok_or_else(|| err("Record component conditions have no intersection"))?;
            let new_text = value
                .as_str()
                .ok_or_else(|| err("Record component conditions have no intersection"))?;
            let narrower = if within(new_text, old_text) {
                value.clone()
            } else if within(old_text, new_text) {
                old.clone()
            } else {
                return Err(err("Record component conditions have no intersection"));
            };
            *values
                .get_mut(&kind)
                .ok_or_else(|| err("Record component conditions have no intersection"))? = narrower;
        } else {
            bank::merge(&mut values, &object([(kind, value)]))?;
        }
    }
    Ok(values)
}
pub fn inherited(metadata: &Value, ancestors: &[Value], project_id: &Value) -> Result<()> {
    let context = constraints(metadata)?;
    for scope in ancestors {
        let result = scopes::decide(&object([
            ("bank_project_id", project_id.clone()),
            ("project_id", project_id.clone()),
            ("scope", scope.clone()),
            ("context", context.clone()),
        ]))?;
        if get(&result, "applicable").as_bool() != Some(true) {
            return Err(err(
                "Record must retain inherited index conditions in its own metadata",
            ));
        }
    }
    Ok(())
}
struct Inventory<'a> {
    files: &'a Value,
    project_id: &'a Value,
    entries: Vec<Value>,
    visited: HashSet<String>,
    versions: HashSet<(String, String)>,
    active: HashSet<String>,
    deleted: HashSet<String>,
    events: HashSet<String>,
    parsed: Option<&'a mut ParsedRecords>,
    allowed: Vec<&'static str>,
}
impl Inventory<'_> {
    fn visit(
        &mut self,
        route: &Value,
        ancestors: &[Value],
        index_depth: usize,
        stack: &mut Vec<String>,
    ) -> Result<()> {
        let path = get(route, "path")
            .as_str()
            .ok_or_else(|| err("Routes require path, purpose, scope, lifecycle and read-when"))?;
        if stack.iter().any(|p| p == path) {
            return Err(err(
                "Topic index cycle; preserve the bank for clarification",
            ));
        }
        if self.visited.contains(path) {
            return Err(err("A file must have one unambiguous route"));
        }
        let content = self
            .files
            .get(path)
            .and_then(Value::as_str)
            .ok_or_else(|| err("A routed optional file is missing"))?;
        self.visited.insert(path.to_owned());
        let mut chain = ancestors.to_vec();
        chain.push(get(route, "scope").clone());
        let entry = |format: &str| {
            object([
                ("path", string(path)),
                ("format", string(format)),
                ("scope_chain", Value::Array(chain.clone())),
                ("lifecycle", get(route, "lifecycle").clone()),
            ])
        };
        if bank::CONTEXTS.contains(&path) {
            self.entries.push(entry("context-prose"));
            return Ok(());
        }
        // The baseline masks parser failures to avoid disclosing secret metadata.
        let invalid =
            || err("Invalid optional record, index or history; inspect the original file");
        let (metadata, body) = frontmatter::read(content, &self.allowed).map_err(|_| invalid())?;
        let metadata = Value::Object(metadata);
        if get(&metadata, "schema") == get(&SPEC, "$id") {
            let children = (|| -> Result<Vec<Value>> {
                if content.len() > MAX_INDEX_BYTES {
                    return Err(err("Topic index byte limit exceeded"));
                }
                if index_depth >= MAX_NESTED_INDEXES {
                    return Err(err("Topic index nesting limit exceeded"));
                }
                if get(&metadata, "project-id") != self.project_id
                    || get(route, "lifecycle").as_str() != Some("active")
                {
                    return Err(err("Topic index project/state does not match the bank"));
                }
                parse_index(path, &metadata, body)
            })()
            .map_err(|_| invalid())?;
            self.entries.push(entry("topic-index"));
            stack.push(path.to_owned());
            for child in &children {
                self.visit(child, &chain, index_depth + 1, stack)
                    .map_err(|_| invalid())?;
            }
            stack.pop();
            return Ok(());
        }
        let (kind, metadata) = records::validate(&metadata, body, &[]).map_err(|_| invalid())?;
        let mut entry = entry(&kind);
        let id = get(&metadata, "id")
            .as_str()
            .ok_or_else(invalid)?
            .to_owned();
        match kind.as_str() {
            "tombstone" => {
                if path != format!("history/tombstones/{id}.md")
                    || get(&metadata, "scope") != self.project_id
                    || get(route, "scope").as_str() != Some("repository")
                    || get(route, "lifecycle").as_str() != Some("deleted")
                    || self.deleted.contains(&id)
                {
                    return Err(err("Safe tombstone path, project or route does not match"));
                }
                if ancestors.iter().any(|v| v.as_str() != Some("repository")) {
                    return Err(err("Tombstones must not inherit conditional index scopes"));
                }
                self.deleted.insert(id.clone());
                bank::merge(&mut entry, &object([("id", string(&id))]))?;
            }
            "optional-history-event" => {
                if !path.starts_with("history/")
                    || path.starts_with("history/tombstones/")
                    || get(&metadata, "project-id") != self.project_id
                    || get(route, "scope").as_str() != Some("repository")
                    || get(route, "lifecycle").as_str() != Some("active")
                    || self.events.contains(&id)
                {
                    return Err(err(
                        "Optional historical events belong to this project history",
                    ));
                }
                self.events.insert(id.clone());
                bank::merge(
                    &mut entry,
                    &object([
                        ("id", string(&id)),
                        ("current_guidance", Value::Bool(false)),
                    ]),
                )?;
            }
            _ => {
                if get(&metadata, "project-id") != self.project_id
                    || get(route, "scope") != get(&metadata, "scope")
                    || get(route, "lifecycle") != get(&metadata, "status")
                    || matches!(
                        get(&metadata, "status").as_str(),
                        Some("candidate" | "quarantined")
                    )
                    || path.starts_with("history/tombstones/")
                {
                    return Err(err(
                        "Standalone project record must match its admitted or historical route",
                    ));
                }
                if path.starts_with("procedures/")
                    && get(&metadata, "kind").as_str() != Some("procedural-candidate")
                {
                    return Err(err(
                        "Procedure files must remain non-binding procedural candidates",
                    ));
                }
                if path.starts_with("history/")
                    && get(&metadata, "status").as_str() == Some("active")
                {
                    return Err(err(
                        "Optional history cannot contain the only current record",
                    ));
                }
                inherited(&metadata, ancestors, self.project_id)?;
                let version = get(&metadata, "version")
                    .as_int()
                    .ok_or_else(invalid)?
                    .as_str()
                    .to_owned();
                let active = get(&metadata, "status").as_str() == Some("active");
                if !self.versions.insert((id.clone(), version))
                    || (active && self.active.contains(&id))
                {
                    return Err(err("Duplicate record version or ambiguous active versions"));
                }
                if active {
                    self.active.insert(id.clone());
                }
                bank::merge(
                    &mut entry,
                    &object([
                        ("id", string(&id)),
                        ("version", get(&metadata, "version").clone()),
                        ("kind", get(&metadata, "kind").clone()),
                        ("applicability", get(&metadata, "applicability").clone()),
                        ("conditions", get(&metadata, "conditions").clone()),
                        ("binding_procedure", Value::Bool(false)),
                    ]),
                )?;
            }
        }
        if let Some(parsed) = &mut self.parsed {
            parsed.insert(path.to_owned(), (kind, metadata, body.to_owned()));
        }
        self.entries.push(entry);
        Ok(())
    }
}
pub fn inventory<'a>(
    files: &'a Value,
    root_metadata: &'a Value,
    root_routes: &[Value],
    parsed_records: Option<&'a mut ParsedRecords>,
) -> Result<Vec<Value>> {
    let mut allowed = Vec::new();
    for schema in [&*records::MODEL, &*records::EVENT, &*SPEC] {
        for (k, _) in get(schema, "properties")
            .as_object()
            .ok_or_else(|| err("Invalid bundled layout contract"))?
        {
            if !allowed.contains(&k.as_str()) {
                allowed.push(k.as_str());
            }
        }
    }
    for key in get(&records::TOMBSTONE, "required")
        .as_array()
        .ok_or_else(|| err("Invalid bundled tombstone contract"))?
    {
        let key = key
            .as_str()
            .ok_or_else(|| err("Invalid bundled tombstone contract"))?;
        if !allowed.contains(&key) {
            allowed.push(key);
        }
    }
    let mut state = Inventory {
        files,
        project_id: get(root_metadata, "project-id"),
        entries: Vec::new(),
        visited: HashSet::new(),
        versions: HashSet::new(),
        active: HashSet::new(),
        deleted: HashSet::new(),
        events: HashSet::new(),
        parsed: parsed_records,
        allowed,
    };
    for route in root_routes {
        state.visit(route, &[], 0, &mut Vec::new())?;
    }
    if files.as_object().is_none_or(|pairs| {
        pairs.len() != state.visited.len() + 1
            || pairs
                .iter()
                .any(|(p, _)| p != "MEMORY.md" && !state.visited.contains(p))
    }) {
        return Err(err(
            "Project Markdown must be reachable from the bounded route map",
        ));
    }
    if state
        .versions
        .iter()
        .any(|(id, _)| state.deleted.contains(id))
    {
        return Err(err(
            "Deleted record content remains in the canonical bank; reconcile erasure",
        ));
    }
    Ok(state.entries)
}
pub fn model() -> Value {
    object([
        ("model", SPEC.clone()),
        ("writes_performed", Value::Bool(false)),
    ])
}
pub fn render_index(request: &Value) -> Result<Value> {
    records::fields(
        request,
        &["path", "project-id", "updated", "routes"],
        &["path", "project-id", "updated", "routes"],
    )?;
    let path = get(request, "path")
        .as_str()
        .ok_or_else(|| err("Invalid topic index path"))?;
    let metadata = object([
        ("schema", get(&SPEC, "$id").clone()),
        ("project-id", get(request, "project-id").clone()),
        ("updated", get(request, "updated").clone()),
    ]);
    let routes = get(request, "routes")
        .as_array()
        .ok_or_else(|| err("A useful bounded topic index requires explicit routes"))?;
    Ok(object([
        ("schema", string("agent-memory/layout-result-v1")),
        ("path", string(path)),
        ("content", string(index_text(path, &metadata, routes)?)),
        ("writes_performed", Value::Bool(false)),
    ]))
}
pub fn validate_index(path: &str, raw: &[u8]) -> Result<Value> {
    if raw.len() > MAX_INDEX_BYTES {
        return Err(err("Topic index byte limit exceeded"));
    }
    let text = std::str::from_utf8(raw).map_err(|_| ContractError::non_utf8("topic index"))?;
    let (metadata, body) = frontmatter::read(text, INDEX_FIELDS)?;
    let routes = parse_index(path, &Value::Object(metadata), body)?;
    Ok(object([
        ("schema", string("agent-memory/layout-result-v1")),
        ("path", string(path)),
        ("routes", Value::Array(routes)),
        ("valid", Value::Bool(true)),
        ("writes_performed", Value::Bool(false)),
    ]))
}
pub fn inspect(repo: &Path) -> Result<Value> {
    let (checked, hashes) = crate::bank_writes::snapshot(repo, None)?;
    Ok(object([
        ("schema", string("agent-memory/layout-result-v1")),
        ("project_id", get(&checked, "project_id").clone()),
        ("entries", get(&checked, "entries").clone()),
        ("source_hashes", hashes),
        ("writes_performed", Value::Bool(false)),
        (
            "validation_scope",
            string("routed-layout-and-record-format"),
        ),
        ("admission_verified", Value::Bool(false)),
        ("sources_verified", Value::Bool(false)),
        ("identity_authenticated", Value::Bool(false)),
    ]))
}
pub fn command_result(result: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match result {
        Ok(v) => (v, 0, json::INDENT2_UTF8),
        Err(e) => {
            let mut value = object([
                ("schema", string("agent-memory/layout-result-v1")),
                ("valid", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                (
                    "error",
                    string(
                        if matches!(e.kind(), crate::error::Kind::BankCapacity { .. }) {
                            e.to_string()
                        } else {
                            "Invalid optional record layout, index or canonical bank; inspect original files".into()
                        },
                    ),
                ),
            ]);
            // Invariant: both operands are objects constructed in this error branch.
            bank::merge(&mut value, &bank_capacity::details(&e)).expect("capacity result fields");
            (value, 1, json::DEFAULT)
        }
    };
    let stdout = match json::encode(&value, style) {
        Ok(encoded) => encoded + "\n",
        Err(error) => return command_result(Err(error)),
    };
    crate::lifecycle::CommandResult {
        stdout,
        value: Some(value),
        exit_code,
        stderr: String::new(),
    }
}
