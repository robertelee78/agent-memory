//! Content-free terminal deletion markers and projection anti-resurrection.
use crate::bank_capacity;
use crate::bank_contract as contract;
use crate::bank_writes as writes;
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{digest, uuid_valid};
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object, string, time_value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const PREFIX: &str = "history/tombstones/";
pub const INVALID_INPUT: &str = "Invalid tombstone, source identity, projection or canonical bank; preserve input for explicit inspection";
pub use crate::records::TOMBSTONE as SPEC;
pub type History = (Value, Value, BTreeMap<String, Value>);
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn fields() -> Result<Vec<&'static str>> {
    get(&SPEC, "required")
        .as_array()
        .ok_or_else(|| err("Invalid bundled tombstone schema"))?
        .iter()
        .map(|field| {
            field
                .as_str()
                .ok_or_else(|| err("Invalid bundled tombstone schema"))
        })
        .collect()
}
pub fn value(metadata: &Value) -> Result<Value> {
    let fields = fields()?;
    let entries = metadata
        .as_object()
        .ok_or_else(|| err("Tombstones retain exactly the five safe deletion fields"))?;
    if entries.len() != fields.len()
        || !fields
            .iter()
            .all(|field| entries.iter().filter(|(key, _)| key == field).count() == 1)
    {
        return Err(err(
            "Tombstones retain exactly the five safe deletion fields",
        ));
    }
    if ["id", "scope", "deletion-authority"]
        .iter()
        .any(|key| !uuid_valid(get(metadata, key)))
    {
        return Err(err(
            "Tombstone record ID, project scope and deletion authority must be canonical random UUIDv4 values",
        ));
    }
    time_value(get(metadata, "deleted-at"))?;
    if !get(get(get(&SPEC, "properties"), "reason-class"), "enum")
        .as_array()
        .is_some_and(|reasons| reasons.contains(get(metadata, "reason-class")))
    {
        return Err(err("Tombstone reason must be a known non-sensitive class"));
    }
    Ok(object(
        fields
            .into_iter()
            .map(|field| (field, get(metadata, field).clone())),
    ))
}
pub fn relative_path(identity: &str) -> String {
    format!("{PREFIX}{identity}.md")
}
pub fn parse(text: &str) -> Result<Value> {
    let (metadata, body) = frontmatter::read(text, &fields()?)?;
    if !contract::trim(body).is_empty() {
        return Err(err(
            "Tombstone bodies must be empty; never retain deleted content or evidence",
        ));
    }
    value(&Value::Object(metadata))
}
pub fn render(metadata: &Value) -> Result<Value> {
    let metadata = value(metadata)?;
    let identity = get(&metadata, "id")
        .as_str()
        .ok_or_else(|| err(INVALID_INPUT))?;
    let entries = metadata.as_object().ok_or_else(|| err(INVALID_INPUT))?;
    Ok(object([
        ("schema", string("agent-memory/tombstone-result-v1")),
        ("operation", string("render")),
        ("metadata", metadata.clone()),
        ("path", string(relative_path(identity))),
        ("content", string(frontmatter::write(entries)?)),
        ("writes_performed", Value::Bool(false)),
        ("identity_authenticated", Value::Bool(false)),
    ]))
}
pub fn validate(text: &str) -> Result<Value> {
    let metadata = parse(text)?;
    let identity = get(&metadata, "id")
        .as_str()
        .ok_or_else(|| err(INVALID_INPUT))?;
    Ok(object([
        ("schema", string("agent-memory/tombstone-result-v1")),
        ("operation", string("validate")),
        ("metadata", metadata.clone()),
        ("path", string(relative_path(identity))),
        ("valid", Value::Bool(true)),
        ("writes_performed", Value::Bool(false)),
        ("validation_scope", string("tombstone-format")),
        ("identity_authenticated", Value::Bool(false)),
    ]))
}
pub struct TombstoneParser;
impl crate::records::TombstoneParser for TombstoneParser {
    fn parse(&self, text: &str) -> Result<Value> {
        parse(text)
    }
}

pub fn canonical_paths(bank: &Path) -> Result<BTreeSet<String>> {
    let physical = crate::bank_observation::physical_inventory(bank)?;
    crate::bank_observation::physical_contract(&physical)?;
    Ok(physical
        .into_keys()
        .filter(|path| path.starts_with(PREFIX))
        .collect())
}
pub fn history(repo: &Path) -> Result<History> {
    let (checked, observed) = writes::snapshot(repo, None)?;
    let (_, bank) = writes::bank_path(repo)?;
    history_from_snapshot(&bank, &checked, &observed)
}
/// Consumes a freshly validated canonical snapshot, never a projection's claimed hashes.
pub(crate) fn history_from_snapshot(
    bank: &Path,
    checked: &Value,
    observed: &Value,
) -> Result<History> {
    let hashes = observed
        .as_object()
        .ok_or_else(|| err("Deletion history changed during inventory"))?;
    let expected: BTreeSet<_> = hashes
        .iter()
        .filter(|(path, _)| path.starts_with(PREFIX))
        .map(|(path, _)| path.clone())
        .collect();
    if canonical_paths(bank)? != expected {
        return Err(err("Deletion history changed during inventory"));
    }
    let routes = get(checked, "routes")
        .as_array()
        .ok_or_else(|| err(INVALID_INPUT))?;
    let mut tombstones = BTreeMap::new();
    for (path, expected_hash) in hashes {
        if !path.starts_with(PREFIX) {
            continue;
        }
        let raw =
            crate::bank_observation::read_source(bank, path, crate::sharing_limits::MAX_FILE)?;
        if digest(Some(&raw)).as_deref() != expected_hash.as_str() {
            return Err(err("Deletion history changed during inventory"));
        }
        let text = std::str::from_utf8(&raw).map_err(|_| err("Tombstone source must be UTF-8"))?;
        let metadata = parse(text)?;
        let identity = get(&metadata, "id")
            .as_str()
            .ok_or_else(|| err(INVALID_INPUT))?;
        let route = routes
            .iter()
            .find(|route| get(route, "path").as_str() == Some(path));
        if path != &relative_path(identity)
            || get(&metadata, "scope") != get(checked, "project_id")
            || tombstones.contains_key(identity)
            || route.is_some_and(|route| {
                get(route, "lifecycle").as_str() != Some("deleted")
                    || get(route, "scope").as_str() != Some("repository")
            })
        {
            return Err(err(
                "Canonical tombstone route must match its record ID, deleted state and scope",
            ));
        }
        tombstones.insert(identity.to_owned(), metadata);
    }
    Ok((
        get(checked, "project_id").clone(),
        observed.clone(),
        tombstones,
    ))
}
fn projection_records(
    project_id: &Value,
    deleted: &BTreeMap<String, Value>,
    projection: &Value,
) -> Result<(Vec<Value>, usize)> {
    let entries = projection
        .as_object()
        .ok_or_else(|| err("Projection must name this canonical project and its source records"))?;
    if entries.len() != 2
        || projection.get("project_id") != Some(project_id)
        || !entries
            .iter()
            .all(|(key, _)| key == "project_id" || key == "records")
    {
        return Err(err(
            "Projection must name this canonical project and its source records",
        ));
    }
    let records = get(projection, "records")
        .as_array()
        .ok_or_else(|| err("Projection must name this canonical project and its source records"))?;
    let mut seen = BTreeSet::new();
    let mut kept = Vec::new();
    let mut suppressed = 0;
    for record in records {
        let identity = get(record, "id");
        if record.as_object().is_none()
            || !uuid_valid(identity)
            || !seen.insert(identity.as_str().ok_or_else(|| err(INVALID_INPUT))?)
        {
            return Err(err(
                "Projection records require unique stable random IDs; do not infer identity from content",
            ));
        }
        if deleted.contains_key(identity.as_str().ok_or_else(|| err(INVALID_INPUT))?) {
            suppressed += 1;
        } else {
            kept.push(record.clone());
        }
    }
    Ok((kept, suppressed))
}
pub fn filter_projection(repo: &Path, projection: &Value) -> Result<Value> {
    let (project_id, observed, deleted) = history(repo)?;
    let (kept, suppressed) = projection_records(&project_id, &deleted, projection)?;
    let (latest_project, latest_hashes, latest_deleted) = history(repo)?;
    if latest_project != project_id || latest_hashes != observed || latest_deleted != deleted {
        return Err(err(
            "Canonical history changed during projection filtering; reread before rebuild",
        ));
    }
    Ok(object([
        ("schema", string("agent-memory/projection-guard-result-v1")),
        ("operation", string("filter")),
        ("project_id", project_id),
        ("records", Value::Array(kept)),
        ("suppressed", Value::Int(suppressed.into())),
        ("source_hashes", observed),
        ("writes_performed", Value::Bool(false)),
        ("validation_scope", string("terminal-deletion-guard")),
        ("identity_authenticated", Value::Bool(false)),
    ]))
}
pub fn command_result(operation: &str, result: Result<Value>) -> CommandResult {
    let (value, exit_code, style) = match result {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let mut value = object([
                ("schema", string("agent-memory/tombstone-result-v1")),
                ("operation", string(operation)),
                ("valid", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                (
                    "error",
                    string(
                        if bank_capacity::details(&error)
                            .as_object()
                            .is_some_and(|fields| !fields.is_empty())
                        {
                            error.to_string()
                        } else {
                            INVALID_INPUT.to_owned()
                        },
                    ),
                ),
            ]);
            if let (Value::Object(fields), Value::Object(details)) =
                (&mut value, bank_capacity::details(&error))
            {
                fields.extend(details);
            }
            (value, 1, json::DEFAULT)
        }
    };
    let stdout = match json::encode(&value, style) {
        Ok(text) => text + "\n",
        Err(error) => return command_result(operation, Err(error)),
    };
    CommandResult {
        value: Some(value),
        exit_code,
        stdout,
        stderr: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    use std::path::PathBuf;
    use std::sync::{Arc, Once};

    const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
    const DEAD: &str = "a9abac6f-c33e-44c9-b40f-7b2ebafda860";
    const LIVE: &str = "8232597a-ad5e-4f68-96b4-15c3fd71f335";
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
        static PARSER: Once = Once::new();
        PARSER.call_once(|| {
            crate::records::register_tombstone_parser(Some(Arc::new(TombstoneParser)))
        });
        let root = PathBuf::from("/tmp").join(format!("am-tomb-history-{}", crate::host::uuid4()?));
        std::fs::DirBuilder::new().mode(0o700).create(&root)?;
        let fixture = Fixture(root.canonicalize()?);
        let proposal = object([
            ("project-id", string(PROJECT)),
            ("router-updated", string("2026-10-22T20:00:00Z")),
            (
                "contexts",
                object([(
                    "projectbrief.md",
                    object([
                        ("body", string("# Brief\n\nUseful project context.\n")),
                        ("purpose", string("Orientation")),
                        ("scope", string("repository")),
                        ("lifecycle", string("active")),
                        ("read-when", string("When resuming")),
                    ]),
                )]),
            ),
            (
                "omitted",
                object(
                    crate::bank_contract::CONTEXTS
                        .iter()
                        .filter(|path| **path != "projectbrief.md")
                        .map(|path| (*path, string("No additional verified context"))),
                ),
            ),
        ]);
        let rendered = crate::bank_contract::render(&proposal)?;
        for (path, content) in get(&rendered, "files")
            .as_object()
            .ok_or("missing fixture files")?
        {
            std::fs::write(
                fixture.0.join(path),
                content.as_str().ok_or("missing fixture content")?,
            )?;
        }
        let metadata = object([
            ("id", string(DEAD)),
            ("scope", string(PROJECT)),
            ("deleted-at", string("2026-10-22T18:00:00Z")),
            ("deletion-authority", string(LIVE)),
            ("reason-class", string("user-request")),
        ]);
        let marker = render(&metadata)?;
        std::fs::create_dir_all(fixture.0.join("history/tombstones"))?;
        let path = get(&marker, "path").as_str().ok_or("missing marker path")?;
        std::fs::write(
            fixture.0.join(path),
            get(&marker, "content")
                .as_str()
                .ok_or("missing marker content")?,
        )?;
        let router_path = fixture.0.join("MEMORY.md");
        let router = std::fs::read_to_string(&router_path)?;
        let row = format!(
            "| [{path}](<{path}>) | Deletion guard | repository | deleted | Before rebuilding projections |\n"
        );
        std::fs::write(
            router_path,
            router.replace(
                "\n## Omitted contexts\n",
                &(row + "\n## Omitted contexts\n"),
            ),
        )?;
        Ok(fixture)
    }
    fn snapshot(bank: &Path) -> Result<(Value, Value)> {
        let scanned = crate::bank_observation::scan(bank, true, true)?;
        crate::bank_observation::physical_contract(&scanned.physical)?;
        let files = object(
            scanned
                .sources
                .into_iter()
                .map(|(path, bytes)| {
                    String::from_utf8(bytes)
                        .map(|body| (path, string(body)))
                        .map_err(|_| err(INVALID_INPUT))
                })
                .collect::<Result<Vec<_>>>()?,
        );
        Ok((
            crate::bank_contract::validate_files(&files, None)?,
            scanned.hashes,
        ))
    }
    fn deletion_history(bank: &Path) -> Result<History> {
        let (checked, hashes) = snapshot(bank)?;
        history_from_snapshot(bank, &checked, &hashes)
    }

    // obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_delete_rebuild_and_repeat_cannot_resurrect_id_under_changed_conditions
    #[test]
    fn deleted_id_is_terminal_across_projection_conditions()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let first = deletion_history(&fixture.0)?;
        let survivor = object([
            ("id", string(LIVE)),
            ("body", string("Useful surviving lesson")),
            ("conditions", Value::Array(vec![string("component:api")])),
            ("evidence", Value::Array(vec![string("src/api.rs")])),
        ]);
        for conditions in ["branch:old", "repository", "component:other"] {
            let stale = object([
                ("project_id", string(PROJECT)),
                (
                    "records",
                    Value::Array(vec![
                        object([
                            ("id", string(DEAD)),
                            ("body", string("DELETED_SECRET_CANARY")),
                            ("conditions", Value::Array(vec![string(conditions)])),
                        ]),
                        survivor.clone(),
                    ]),
                ),
            ]);
            let (kept, suppressed) = projection_records(&first.0, &first.2, &stale)?;
            assert_eq!(suppressed, 1);
            assert_eq!(kept, vec![survivor.clone()]);
            assert!(
                !json::encode(&Value::Array(kept), json::DEFAULT)?
                    .contains("DELETED_SECRET_CANARY")
            );
            assert_eq!(deletion_history(&fixture.0)?, first);
        }
        Ok(())
    }

    // obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_unknown_ids_wrong_project_invalid_history_and_interrupted_bank_stop_rebuild
    #[test]
    fn wrong_project_missing_duplicate_ids_and_changed_history_refuse()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let history = deletion_history(&fixture.0)?;
        let record = object([
            ("id", string(DEAD)),
            ("body", string("DELETED_SECRET_CANARY")),
        ]);
        for projection in [
            object([
                ("project_id", string(LIVE)),
                ("records", Value::Array(vec![record.clone()])),
            ]),
            object([
                ("project_id", string(PROJECT)),
                (
                    "records",
                    Value::Array(vec![object([("body", string("DELETED_SECRET_CANARY"))])]),
                ),
            ]),
            object([
                ("project_id", string(PROJECT)),
                ("records", Value::Array(vec![record.clone(), record])),
            ]),
        ] {
            assert!(projection_records(&history.0, &history.2, &projection).is_err());
        }
        let (checked, hashes) = snapshot(&fixture.0)?;
        let marker_path = fixture.0.join(relative_path(DEAD));
        let original = std::fs::read(&marker_path)?;
        std::fs::write(
            &marker_path,
            [original.as_slice(), b"DELETED_SECRET_CANARY\n"].concat(),
        )?;
        assert!(history_from_snapshot(&fixture.0, &checked, &hashes).is_err());
        assert!(deletion_history(&fixture.0).is_err());
        std::fs::write(&marker_path, original)?;
        for marker in [".agent-memory.lock", ".agent-memory-transaction.md"] {
            std::fs::write(
                fixture.0.join(marker),
                "Unknown recovery evidence; preserve",
            )?;
            assert!(crate::bank_observation::markers(&fixture.0, None, None).is_err());
            std::fs::remove_file(fixture.0.join(marker))?;
        }
        Ok(())
    }

    // obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_history_namespace_route_state_and_record_identity_must_match_before_rebuild
    #[test]
    fn history_record_scope_identity_and_route_state_must_match()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture()?;
        let (checked, hashes) = snapshot(&fixture.0)?;
        let marker_path = fixture.0.join(relative_path(DEAD));
        let original = std::fs::read_to_string(&marker_path)?;
        for changed in [
            original.replace(&format!("id: \"{DEAD}\""), &format!("id: \"{LIVE}\"")),
            original.replace(
                &format!("scope: \"{PROJECT}\""),
                &format!("scope: \"{LIVE}\""),
            ),
        ] {
            std::fs::write(&marker_path, changed)?;
            let changed_hashes = crate::bank_observation::scan(&fixture.0, false, true)?.hashes;
            assert!(history_from_snapshot(&fixture.0, &checked, &changed_hashes).is_err());
        }
        std::fs::write(&marker_path, original)?;
        let mut changed = checked.clone();
        let routes = changed.get_mut("routes").ok_or("missing checked routes")?;
        if let Value::Array(routes) = routes {
            let route = routes
                .iter_mut()
                .find(|route| get(route, "path").as_str() == Some(&relative_path(DEAD)))
                .ok_or("missing tombstone route")?;
            *route.get_mut("lifecycle").ok_or("missing lifecycle")? = string("active");
        }
        assert!(history_from_snapshot(&fixture.0, &changed, &hashes).is_err());
        Ok(())
    }

    // obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_copied_portable_guard_uses_the_same_canonical_project_history
    #[test]
    fn copied_canonical_history_preserves_deletion_guard_without_local_state()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let source = fixture()?;
        let copied = fixture()?;
        assert_eq!(deletion_history(&source.0)?, deletion_history(&copied.0)?);
        let history = deletion_history(&copied.0)?;
        let projection = object([
            ("project_id", string(PROJECT)),
            (
                "records",
                Value::Array(vec![object([
                    ("id", string(DEAD)),
                    ("body", string("DELETED_SECRET_CANARY")),
                ])]),
            ),
        ]);
        assert_eq!(
            projection_records(&history.0, &history.2, &projection)?,
            (Vec::new(), 1)
        );
        Ok(())
    }
}
