//! Source-bound semantic navigation; meanings are interpreted only by the invoking harness.
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{encoded, sha256_hex, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::retrieval::{array, set, text};
use crate::{
    bank_contract as contract, bank_layout, bank_observation as observation, bank_writes as writes,
    retrieval,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/discovery-v1";
pub const MAX_INPUT: usize = 65536;
pub const MAX_FILE: usize = 16384;
const FIELDS: &[&str] = &[
    "schema",
    "task",
    "intent",
    "binding",
    "nodes",
    "sources",
    "complete",
    "rationale",
];
const BINDING: &[&str] = &[
    "repo_root",
    "bank_root",
    "project_id",
    "scope_context",
    "context",
    "git_context",
    "inventory_sha256",
    "recovery_frontier",
];
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn failure(code: &str, reason: &str, repair: &str) -> ContractError {
    ContractError::discovery_failure(code, reason, repair)
}
pub fn changed() -> ContractError {
    failure(
        "source-context-changed",
        "The memory map, selected sources, repository identity or checkout context changed during navigation.",
        "Run am discover start again with the current task/context, follow the current map, then retry the qualified read.",
    )
}
pub fn read_input(path: &Path) -> Result<Value> {
    let path = crate::fs::canonical_path(path, true)?;
    let directory = path
        .parent()
        .ok_or_else(|| err("Discovery input requires a file path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Discovery input requires a file path"))?;
    let raw = crate::fs::read_source(directory, Path::new(name), MAX_INPUT).map_err(|error| {
        match error {
            crate::fs::ReadError::SizeLimit { .. } => failure(
                "input-over-bound", "The navigation/request input exceeds64KiB.",
                "Keep only the current bounded navigation trace and selected-source request; use map nodes rather than embedding source bodies.",
            ),
            other => err(&other.to_string()),
        }
    })?;
    crate::developer_identity::load_with_limit(&raw.bytes, MAX_INPUT)
}
pub fn bounded_text(value: &Value, maximum: usize) -> Result<&str> {
    let text = value
        .as_str()
        .filter(|s| {
            !s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                .is_empty()
                && s.len() <= maximum
        })
        .ok_or_else(|| {
            err("Discovery requires bounded actual task and semantic selection rationale")
        })?;
    if !array(get(
        &crate::security::scan(value, crate::security::LIMIT)?,
        "reasons",
    ))
    .is_empty()
    {
        return Err(err("Unsafe discovery text needs source clarification"));
    }
    Ok(text)
}
pub fn trace_value(trace: &Value, completed: bool) -> Result<&Value> {
    if !crate::projections::fields(trace, FIELDS)
        || get(trace, "schema").as_str() != Some(SCHEMA)
        || !matches!(
            get(trace, "intent").as_str(),
            Some("recall" | "comparison" | "sharing")
        )
        || get(trace, "complete").as_bool().is_none()
        || !crate::projections::fields(get(trace, "binding"), BINDING)
        || !uuid_valid(get(get(trace, "binding"), "project_id"))
        || encoded(trace)?.len() > MAX_INPUT
    {
        return Err(err("Use a complete bounded same-project discovery trace"));
    }
    bounded_text(get(trace, "task"), 1024)?;
    if completed && get(trace, "complete").as_bool() == Some(false) {
        return Err(failure(
            "selection-incomplete",
            "Semantic navigation has not selected and qualified its plausible sources.",
            "Follow the map and use am discover read with the selected paths and your selection rationale before answering or admitting learning.",
        ));
    }
    if get(trace, "complete").as_bool() == Some(true) || get(trace, "rationale") != &Value::Null {
        bounded_text(get(trace, "rationale"), 2048)?;
    }
    for name in ["nodes", "sources"] {
        let rows = get(trace, name)
            .as_array()
            .ok_or_else(|| err("Retain bounded exact visited nodes and selected sources"))?;
        if rows.len() > 64 || (name == "nodes" && rows.is_empty()) {
            return Err(err(
                "Retain bounded exact visited nodes and selected sources",
            ));
        }
        let mut paths = BTreeSet::new();
        for row in rows {
            if !crate::projections::fields(row, &["path", "sha256"]) {
                return Err(err(
                    "Retain bounded exact visited nodes and selected sources",
                ));
            }
            let path = text(row, "path")?;
            if path != "MEMORY.md" {
                contract::path_value(get(row, "path"))?;
            }
            if !crate::projections::sha_valid(get(row, "sha256")) {
                return Err(err(
                    "Discovery source references require actual complete-byte hashes",
                ));
            }
            if !paths.insert(path) {
                return Err(err("Do not duplicate a discovery observation"));
            }
        }
    }
    if !array(get(trace, "nodes"))
        .iter()
        .any(|row| get(row, "path").as_str() == Some("MEMORY.md"))
    {
        return Err(err(
            "Semantic navigation starts at the actual bounded repository router",
        ));
    }
    Ok(trace)
}
pub fn bounded_source(bank: &Path, path: &str) -> Result<Vec<u8>> {
    observation::read_source(bank, path, MAX_FILE)
}
fn content(raw: &[u8]) -> Result<&str> {
    std::str::from_utf8(raw).map_err(|_| ContractError::non_utf8("canonical navigation source"))
}
fn routes(path: &str, body: &str) -> Result<(Value, Vec<Value>)> {
    if path == "MEMORY.md" {
        let (metadata, routes, _) = contract::parse_router(body)?;
        return Ok((metadata, routes));
    }
    let allowed: Vec<_> = get(&bank_layout::SPEC, "properties")
        .as_object()
        .ok_or_else(|| err("Invalid compiled index schema"))?
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    let (metadata, body) = frontmatter::read(body, &allowed)?;
    let metadata = Value::Object(metadata);
    Ok((
        metadata.clone(),
        bank_layout::parse_index(path, &metadata, body)?,
    ))
}
fn bank_relative(root: &Path, bank: &Path) -> Result<Value> {
    Ok(string(crate::host::text(
        bank.strip_prefix(root).map_err(|_| changed())?.as_os_str(),
        "bank root",
    )?))
}
fn children(tables: &BTreeMap<String, Vec<Value>>) -> BTreeSet<String> {
    tables
        .values()
        .flatten()
        .filter_map(|row| get(row, "path").as_str())
        .map(str::to_owned)
        .collect()
}
fn reachable(tables: &BTreeMap<String, Vec<Value>>, enforce_depth: bool) -> Result<()> {
    let mut reached = BTreeMap::from([("MEMORY.md".to_owned(), 0usize)]);
    loop {
        let mut next = reached.clone();
        for (parent, depth) in &reached {
            let rows = tables
                .get(parent)
                .ok_or_else(|| err("Missing canonical root navigation table"))?;
            for row in rows {
                let path = text(row, "path")?;
                if !tables.contains_key(path) || reached.contains_key(path) {
                    continue;
                }
                if enforce_depth && depth + 1 > bank_layout::MAX_NESTED_INDEXES {
                    return Err(failure(
                        "navigation-depth",
                        "The visited map exceeds the native topic-index depth.",
                        "Run am doctor and reorganize the topic map through governed maintenance before retrying.",
                    ));
                }
                next.insert(path.into(), depth + 1);
            }
        }
        if next == reached {
            break;
        }
        reached = next;
    }
    if reached.len() != tables.len() {
        return Err(if enforce_depth {
            failure(
                "missing-parent",
                "A visited topic index is missing its actual parent map.",
                "Restart from MEMORY.md and retain each visited parent when following a topic index.",
            )
        } else {
            err("Keep the actual visited parent route for each discovered index")
        });
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn check_trace(
    repo: &Path,
    trace: &Value,
    checked: &Value,
    observed: &Value,
    scope: &Value,
    intent: &str,
    selected_paths: &BTreeSet<String>,
    owner: Option<&[u8]>,
    context: Option<&Value>,
) -> Result<Value> {
    trace_value(trace, true)?;
    let (root, bank) = writes::bank_path(repo)?;
    let binding = get(trace, "binding");
    let (actual, git) =
        retrieval::observed_context(&root, context.unwrap_or(get(binding, "context")))?;
    let provider = writes::recovery_frontier_provider()?;
    let expected = object([
        (
            "repo_root",
            string(crate::host::text(root.as_os_str(), "repository root")?),
        ),
        ("bank_root", bank_relative(&root, &bank)?),
        ("project_id", get(checked, "project_id").clone()),
        ("scope_context", scope.clone()),
        ("context", actual),
        ("git_context", git),
        ("inventory_sha256", string(sha256_hex(&encoded(observed)?))),
        (
            "recovery_frontier",
            provider.observe_owned(&root, owner, None)?,
        ),
    ]);
    if get(trace, "intent").as_str() != Some(intent) || !retrieval::same(binding, &expected) {
        return Err(changed());
    }
    let entries: BTreeMap<_, _> = array(get(checked, "entries"))
        .iter()
        .map(|entry| Ok((text(entry, "path")?, entry)))
        .collect::<Result<_>>()?;
    let mut tables = BTreeMap::new();
    for row in array(get(trace, "nodes")) {
        let path = text(row, "path")?;
        if observed.get(path) != row.get("sha256")
            || (path != "MEMORY.md"
                && entries
                    .get(path)
                    .and_then(|entry| get(entry, "format").as_str())
                    != Some("topic-index"))
        {
            return Err(err(
                "A visited navigation node changed or is not a canonical index",
            ));
        }
        let raw = bounded_source(&bank, path)?;
        if get(row, "sha256").as_str() != Some(sha256_hex(&raw).as_str()) {
            return Err(err("Navigation changed during its source recheck"));
        }
        let body = content(&raw)?;
        crate::security::ensure_safe(&object([(path, string(body))]))?;
        tables.insert(path.into(), routes(path, body)?.1);
    }
    reachable(&tables, false)?;
    let children = children(&tables);
    let sources: BTreeMap<_, _> = array(get(trace, "sources"))
        .iter()
        .map(|row| Ok((text(row, "path")?, get(row, "sha256"))))
        .collect::<Result<_>>()?;
    if selected_paths
        .iter()
        .any(|path| !sources.contains_key(path.as_str()))
    {
        return Err(err(
            "Semantic selection must retain its actual discovered source versions",
        ));
    }
    for (path, sha) in &sources {
        if !children.contains(*path)
            || observed.get(path) != Some(*sha)
            || !matches!(
                entries
                    .get(path)
                    .and_then(|entry| get(entry, "format").as_str()),
                Some("standalone-record" | "context-prose")
            )
        {
            return Err(err(
                "A selected source changed or lacks its visited canonical parent route",
            ));
        }
    }
    provider.require_scope_owned(&root, get(checked, "project_id"), Some(scope), owner, None)?;
    Ok(object([
        ("trace_sha256", string(sha256_hex(&encoded(trace)?))),
        ("semantic_interpreter", string("invoking-harness")),
        ("semantic_completeness_verified", Value::Bool(false)),
        ("visited_nodes", Value::Int(tables.len().into())),
        ("selected_sources", Value::Int(sources.len().into())),
    ]))
}
pub fn navigation_observation(root: &Path, bank: &Path) -> Result<observation::Observation> {
    let provider = writes::recovery_frontier_provider()?;
    let token = provider.observe(root)?;
    let held = if get(&token, "hold_sha256") != &Value::Null {
        Some(provider.verified_hold(root, None)?)
    } else {
        None
    };
    observation::markers(bank, None, held.as_deref())?;
    let scanned = observation::scan(bank, false, true)?;
    observation::physical_contract(&scanned.physical)?;
    let raw = bounded_source(bank, "MEMORY.md")?;
    if get(&scanned.hashes, "MEMORY.md").as_str() != Some(sha256_hex(&raw).as_str()) {
        return Err(changed());
    }
    let (metadata, _, omitted) = contract::parse_router(content(&raw)?)?;
    for (path, _) in omitted.as_object().unwrap_or_default() {
        if scanned.physical.contains_key(path) {
            return Err(failure(
                "omitted-context-occupied",
                "A context declared absent is physically present in the bank.",
                "Run am doctor, inspect that context and reconcile its route/omission before retrying; preserve the existing file.",
            ));
        }
    }
    Ok(observation::Observation {
        root: root.into(),
        bank: bank.into(),
        owner: None,
        transaction: held,
        physical: scanned.physical,
        hashes: scanned.hashes,
        sources: BTreeMap::from([("MEMORY.md".into(), raw)]),
        checked: object([
            ("project_id", get(&metadata, "project-id").clone()),
            ("omitted", omitted),
            ("entries", Value::Array(Vec::new())),
        ]),
        parsed: BTreeMap::new(),
        payload_bytes_read: scanned.payload_bytes,
        payload_file_reads: scanned.file_reads,
        recovery_frontier: token,
    })
}
pub fn start(repo: &Path, task: &Value, intent: &str, context: &Value) -> Result<Value> {
    bounded_text(task, 1024)?;
    if !["recall", "comparison", "sharing"].contains(&intent) {
        return Err(err("Choose the actual discovery intent"));
    }
    let (root, bank) = writes::bank_path(repo)?;
    let provider = writes::recovery_frontier_provider()?;
    let scope = provider.require_scope(&root, &Value::Null, None)?;
    let frontier = provider.observe(&root)?;
    let (actual, git) = retrieval::observed_context(&root, context)?;
    let cached = (|| {
        let runtime = retrieval::catalog_runtime()?;
        let mut catalog = crate::recall_catalog::load_catalog(
            &root,
            text(&scope, "project_id")?,
            &runtime,
            None,
        )?;
        let raw = catalog.source("MEMORY.md", MAX_FILE)?;
        Ok::<_, ContractError>((catalog, raw))
    })()
    .ok();
    let mut observed = if cached.is_none() {
        Some(navigation_observation(&root, &bank)?)
    } else {
        None
    };
    let (hashes, checked, raw) = if let Some((catalog, raw)) = &cached {
        (&catalog.hashes, &catalog.checked, raw.as_slice())
    } else {
        let observation = observed.as_ref().ok_or_else(changed)?;
        (
            &observation.hashes,
            &observation.checked,
            observation
                .sources
                .get("MEMORY.md")
                .ok_or_else(changed)?
                .as_slice(),
        )
    };
    if get(&scope, "project_id") != get(checked, "project_id") {
        return Err(err("Discovery project changed"));
    }
    if raw.len() > MAX_FILE {
        return Err(err("A bounded root router is required"));
    }
    let body = content(raw)?.to_owned();
    crate::security::ensure_safe(&object([("MEMORY.md", string(&body))]))?;
    let disclosed = raw.len();
    let hashes = hashes.clone();
    let trace = object([
        ("schema", string(SCHEMA)),
        ("task", task.clone()),
        ("intent", string(intent)),
        (
            "binding",
            object([
                (
                    "repo_root",
                    string(crate::host::text(root.as_os_str(), "repository root")?),
                ),
                ("bank_root", bank_relative(&root, &bank)?),
                ("project_id", get(&scope, "project_id").clone()),
                ("scope_context", scope.clone()),
                ("context", actual.clone()),
                ("git_context", git.clone()),
                ("inventory_sha256", string(sha256_hex(&encoded(&hashes)?))),
                ("recovery_frontier", frontier.clone()),
            ]),
        ),
        (
            "nodes",
            Value::Array(vec![object([
                ("path", string("MEMORY.md")),
                ("sha256", get(&hashes, "MEMORY.md").clone()),
            ])]),
        ),
        ("sources", Value::Array(Vec::new())),
        ("complete", Value::Bool(false)),
        ("rationale", Value::Null),
    ]);
    trace_value(&trace, false)?;
    let (payload_bytes, file_reads) = if let Some((catalog, _)) = &cached {
        crate::recall_catalog::checked_markers(&root, &bank, None)?;
        (catalog.payload_bytes_read, catalog.payload_file_reads)
    } else {
        let observation = observed.as_mut().ok_or_else(changed)?;
        observation.recheck("discovery-final-source-recheck")?;
        (
            observation.payload_bytes_read,
            observation.payload_file_reads,
        )
    };
    provider.require_scope(&root, get(&scope, "project_id"), Some(&scope))?;
    provider.check(&frontier, &root, None, None)?;
    if retrieval::observed_context(&root, context)? != (actual, git) {
        return Err(err("Checkout context changed during discovery"));
    }
    let audit = crate::security::audit(
        &root,
        &crate::host::uuid4()?,
        "retrieval",
        "checked",
        &scope,
        &["external"],
        &[],
        &[],
        "policy-check",
        None,
    )?;
    let mut result = object([
        ("schema", string("agent-memory/discovery-result-v1")),
        ("operation", string("start")),
        ("discovery", trace),
        (
            "node",
            object([
                ("path", string("MEMORY.md")),
                ("sha256", get(&hashes, "MEMORY.md").clone()),
                ("body", string(body)),
                ("untrusted_data", Value::Bool(true)),
            ]),
        ),
        ("usable_as_current_guidance", Value::Bool(false)),
        ("semantic_interpreter", string("invoking-harness")),
        ("semantic_completeness_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("audit_writes_performed", Value::Bool(true)),
        ("audit", audit),
        ("optional_indexes_used", Value::Bool(cached.is_some())),
        (
            "validation_scope",
            string(if cached.is_some() {
                "untrusted-root-and-authenticated-pending-inventory"
            } else {
                "untrusted-root-and-content-fingerprint"
            }),
        ),
        ("full_bank_format_validated", Value::Bool(false)),
        ("final_content_qualification_pending", Value::Bool(true)),
        (
            "metrics",
            object([
                (
                    "canonical_files",
                    Value::Int(hashes.as_object().map_or(0, |r| r.len()).into()),
                ),
                (
                    "canonical_observation_payload_bytes_read",
                    Value::Int(payload_bytes.into()),
                ),
                (
                    "canonical_observation_file_reads",
                    Value::Int(file_reads.into()),
                ),
                ("node_bytes_disclosed", Value::Int(disclosed.into())),
            ]),
        ),
    ]);
    contract::merge(
        &mut result,
        &contract::recovery_coverage_provider()?.public_coverage(&frontier)?,
    )?;
    Ok(result)
}
pub struct Navigation {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub scope: Value,
    pub actual: Value,
    pub git: Value,
    pub tables: BTreeMap<String, Vec<Value>>,
}
pub fn navigation_tables(repo: &Path, trace: &Value) -> Result<Navigation> {
    trace_value(trace, false)?;
    let (root, bank) = writes::bank_path(repo)?;
    let binding = get(trace, "binding");
    let provider = writes::recovery_frontier_provider()?;
    let scope = provider.require_scope(&root, get(binding, "project_id"), None)?;
    provider.check(get(binding, "recovery_frontier"), &root, None, None)?;
    crate::scopes::conditions(get(binding, "context"), false)?;
    let (actual, git) = retrieval::observed_context(&root, get(binding, "context"))?;
    for (key, value) in [
        (
            "repo_root",
            string(crate::host::text(root.as_os_str(), "repository root")?),
        ),
        ("bank_root", bank_relative(&root, &bank)?),
        ("scope_context", scope.clone()),
        ("context", actual.clone()),
        ("git_context", git.clone()),
    ] {
        if !retrieval::same(get(binding, key), &value) {
            return Err(changed());
        }
    }
    let mut tables = BTreeMap::new();
    for row in array(get(trace, "nodes")) {
        let path = text(row, "path")?;
        let raw = bounded_source(&bank, path)?;
        if get(row, "sha256").as_str() != Some(sha256_hex(&raw).as_str()) {
            return Err(changed());
        }
        let body = content(&raw)?;
        crate::security::ensure_safe(&object([(path, string(body))]))?;
        let (metadata, rows) = routes(path, body)?;
        if get(&metadata, "project-id") != get(&scope, "project_id") {
            return Err(changed());
        }
        tables.insert(path.into(), rows);
    }
    reachable(&tables, true)?;
    Ok(Navigation {
        root,
        bank,
        scope,
        actual,
        git,
        tables,
    })
}
pub fn node(repo: &Path, trace: &Value, path: &str) -> Result<Value> {
    let nav = navigation_tables(repo, trace)?;
    if !children(&nav.tables).contains(path) {
        return Err(failure(
            "route-not-reached",
            "This path is absent from the visited parent maps.",
            "Follow its parent route from MEMORY.md before requesting the topic index.",
        ));
    }
    let raw = bounded_source(&nav.bank, path)?;
    let body = content(&raw)?;
    let (metadata, _) = routes(path, body)?;
    crate::security::ensure_safe(&object([(path, string(body))]))?;
    if get(&metadata, "project-id") != get(&nav.scope, "project_id") {
        return Err(changed());
    }
    let mut value = trace.clone();
    set(&mut value, "complete", Value::Bool(false));
    set(&mut value, "rationale", Value::Null);
    let mut nodes: Vec<_> = array(get(&value, "nodes"))
        .iter()
        .filter(|row| get(row, "path").as_str() != Some(path))
        .cloned()
        .collect();
    nodes.push(object([
        ("path", string(path)),
        ("sha256", string(sha256_hex(&raw))),
    ]));
    set(&mut value, "nodes", Value::Array(nodes));
    navigation_tables(&nav.root, &value)?;
    Ok(object([
        ("schema", string("agent-memory/discovery-result-v1")),
        ("operation", string("node")),
        ("discovery", value),
        (
            "node",
            object([
                ("path", string(path)),
                ("sha256", string(sha256_hex(&raw))),
                ("body", string(body)),
                ("untrusted_data", Value::Bool(true)),
            ]),
        ),
        ("usable_as_current_guidance", Value::Bool(false)),
        ("full_bank_format_validated", Value::Bool(false)),
        ("final_content_qualification_pending", Value::Bool(true)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("semantic_completeness_verified", Value::Bool(false)),
    ]))
}
pub fn qualified_read(
    repo: &Path,
    trace: &Value,
    request: &Value,
    rationale: &Value,
) -> Result<Value> {
    let nav = navigation_tables(repo, trace)?;
    bounded_text(rationale, 2048)?;
    if get(trace, "intent").as_str() == Some("sharing") {
        return Err(failure(
            "sharing-view-required",
            "Shared-memory qualification requires the actual base/local/landed views.",
            "Use the sharing review workflow; a local discover read cannot qualify landed or competing memory.",
        ));
    }
    let field = if get(trace, "intent").as_str() == Some("recall") {
        "paths"
    } else {
        "read_paths"
    };
    let empty = Value::Array(Vec::new());
    let paths = request
        .get(field)
        .unwrap_or(&empty)
        .as_array()
        .filter(|r| r.len() <= 64)
        .ok_or_else(|| {
            failure(
                "selection-invalid",
                "Choose a bounded list of actual plausible source paths.",
                "Read the visited topic map and pass its selected paths in the request.",
            )
        })?;
    let mut selected: BTreeMap<String, Value> = array(get(trace, "sources"))
        .iter()
        .map(|row| Ok((text(row, "path")?.into(), get(row, "sha256").clone())))
        .collect::<Result<_>>()?;
    let children = children(&nav.tables);
    for path in paths {
        let path = contract::path_value(path)?;
        if !children.contains(path) {
            return Err(failure(
                "route-not-reached",
                "A selected source lacks its visited parent route.",
                "Follow its parent topic map before selecting and qualifying the source.",
            ));
        }
        selected.insert(
            path.into(),
            string(sha256_hex(&bounded_source(&nav.bank, path)?)),
        );
    }
    let mut value = trace.clone();
    set(
        &mut value,
        "sources",
        Value::Array(
            selected
                .into_iter()
                .map(|(path, sha)| object([("path", string(path)), ("sha256", sha)]))
                .collect(),
        ),
    );
    set(&mut value, "complete", Value::Bool(true));
    set(&mut value, "rationale", rationale.clone());
    trace_value(&value, true)?;
    if request
        .get("discovery")
        .is_some_and(|v| !retrieval::same(v, trace))
    {
        return Err(failure(
            "request-trace-conflict",
            "The request and navigation input refer to different observations.",
            "Use the same current navigation trace in both inputs, or omit the request discovery field.",
        ));
    }
    let mut request = request.clone();
    set(&mut request, "discovery", value.clone());
    if get(trace, "intent").as_str() == Some("recall") {
        let result = retrieval::retrieve(&nav.root, &request, None, &[])?;
        let current = array(get(&result, "results"))
            .iter()
            .filter(|row| {
                get(row, "selection_class").as_str() == Some("current-record")
                    && get(get(row, "selection"), "current").as_bool() == Some(true)
            })
            .count();
        let orientation = array(get(&result, "results"))
            .iter()
            .filter(|row| get(row, "selection_class").as_str() == Some("orientation-only"))
            .count();
        let mut navigation = trace.clone();
        set(&mut navigation, "sources", Value::Array(Vec::new()));
        set(&mut navigation, "complete", Value::Bool(false));
        set(&mut navigation, "rationale", Value::Null);
        let digest = sha256_hex(&encoded(&result)?);
        return Ok(object([
            ("schema", string("agent-memory/discovery-result-v1")),
            ("operation", string("read")),
            ("discovery", navigation),
            ("qualification", result),
            ("qualification_sha256", string(digest)),
            ("qualification_completed", Value::Bool(true)),
            ("usable_as_current_guidance", Value::Bool(current > 0)),
            ("qualified_current_records", Value::Int(current.into())),
            ("orientation_only_results", Value::Int(orientation.into())),
            (
                "selection_state",
                string(if current > 0 {
                    "current-records-available"
                } else if orientation > 0 {
                    "orientation-only"
                } else {
                    "empty"
                }),
            ),
            ("semantic_interpreter", string("invoking-harness")),
            ("semantic_completeness_verified", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
        ]));
    }
    let result = crate::comparison::inspect(&nav.root, &request, None)?;
    Ok(object([
        ("schema", string("agent-memory/discovery-result-v1")),
        ("operation", string("read")),
        ("discovery", value),
        ("request", request),
        ("inspection", result),
        ("usable_as_current_guidance", Value::Bool(false)),
        ("semantic_interpreter", string("invoking-harness")),
        ("semantic_completeness_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}
pub fn command_result(operation: &str, phase: &str, result: Result<Value>) -> CommandResult {
    match result {
        Ok(value) => CommandResult::json(value, 0, json::INDENT2_ASCII),
        Err(error) => {
            let (code, repair, message) = match error.kind() {
                Kind::DiscoveryFailure { code, repair } => (code.as_str(), repair.as_str(), error.to_string()),
                Kind::BankCapacity { code, .. } => (code.code(), "Preserve source bytes and resolve the reported bank capacity before retrying.", error.to_string()),
                _ => ("discovery-unverified", "Preserve the bank and input. Run am doctor for the selected repository, inspect its failed component, then restart navigation from MEMORY.md; do not initialize an empty replacement.", "Cannot validate this navigation/input against the current canonical bank; no guidance or novelty is certified.".into()),
            };
            let mut value = object([
                ("schema", string("agent-memory/discovery-result-v1")),
                ("operation", string(operation)),
                ("canonical_writes_performed", Value::Bool(false)),
                ("semantic_completeness_verified", Value::Bool(false)),
                ("error", string(message)),
                ("failure_code", string(code)),
                ("failure_component", string(phase)),
                ("repair", string(repair)),
            ]);
            let _ = contract::merge(&mut value, &crate::bank_capacity::details(&error));
            CommandResult::json(value, 1, json::DEFAULT)
        }
    }
}
