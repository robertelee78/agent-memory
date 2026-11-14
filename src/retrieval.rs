//! Project-resolved recall: qualification and invalidation precede ranking.
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, sha256_hex, uuid_valid};
use crate::error::{ContractError, Result};
use crate::lifecycle::{self, CommandResult, get, object, string};
use crate::{
    bank_contract, bank_observation, bank_writes as writes, discovery, handoff, records, scopes,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

pub const MAX_FILE: usize = 16384;
pub const MAX_RESULT: usize = 65536;
pub const MAX_REQUEST: usize = 65536;
const FIELDS: &[&str] = &[
    "project_id",
    "query",
    "ids",
    "paths",
    "context",
    "kinds",
    "minimum_trust",
    "limit",
    "include_context",
];

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
/// Python dictionary equality ignores key order; JSON arrays still retain order.
pub(crate) fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Bool(boolean), Value::Int(integer))
        | (Value::Int(integer), Value::Bool(boolean)) => {
            integer.as_str() == if *boolean { "1" } else { "0" }
        }
        (Value::Bool(boolean), Value::Float(float))
        | (Value::Float(float), Value::Bool(boolean)) => *float == if *boolean { 1.0 } else { 0.0 },
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| right.get(key).is_some_and(|other| same(value, other)))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Int(integer), Value::Float(float)) | (Value::Float(float), Value::Int(integer)) => {
            float.is_finite()
                && float.fract() == 0.0
                && if *float == 0.0 {
                    integer.as_str() == "0"
                } else {
                    integer.as_str() == format!("{float:.0}")
                }
        }
        _ => left == right,
    }
}
pub(crate) fn set(value: &mut Value, key: &str, field: Value) {
    if let Value::Object(rows) = value {
        if let Some((_, old)) = rows.iter_mut().find(|(name, _)| name == key) {
            *old = field;
        } else {
            rows.push((key.into(), field));
        }
    }
}
pub(crate) fn array(value: &Value) -> &[Value] {
    value.as_array().unwrap_or_default()
}
pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid canonical recall metadata"))
}
pub fn request_value(request: &Value) -> Result<&Value> {
    let invalid = || {
        err(
            "Recall needs explicit project, bounded selectors, conditions and safe operation policy",
        )
    };
    let rows = request.as_object().ok_or_else(invalid)?;
    let query = get(request, "query").as_str().ok_or_else(invalid)?;
    let limit = get(request, "limit").as_int().and_then(|n| n.as_u64());
    if FIELDS.iter().any(|key| request.get(key).is_none())
        || rows
            .iter()
            .any(|(key, _)| !FIELDS.contains(&key.as_str()) && key != "discovery")
        || !uuid_valid(get(request, "project_id"))
        || query.len() > 1024
        || query.chars().any(|c| c < ' ' || c == '\u{7f}')
        || !limit.is_some_and(|n| (1..=64).contains(&n))
        || get(request, "include_context").as_bool().is_none()
        || !matches!(
            get(request, "minimum_trust").as_str(),
            Some("reviewed" | "verified")
        )
    {
        return Err(invalid());
    }
    for key in ["ids", "paths", "kinds"] {
        let values = get(request, key)
            .as_array()
            .ok_or_else(|| err("Recall selectors require bounded unique strings"))?;
        let unique: BTreeSet<_> = values.iter().filter_map(Value::as_str).collect();
        if values.len() > 64 || unique.len() != values.len() {
            return Err(err("Recall selectors require bounded unique strings"));
        }
    }
    if array(get(request, "ids")).iter().any(|v| !uuid_valid(v)) {
        return Err(err("Exact record lookup requires stable random IDs"));
    }
    for path in array(get(request, "paths")) {
        bank_contract::path_value(path)?;
    }
    let kinds = array(get(get(get(&records::MODEL, "properties"), "kind"), "enum"));
    if array(get(request, "kinds"))
        .iter()
        .any(|v| !kinds.contains(v))
    {
        return Err(err("Recall requires known standalone memory kinds"));
    }
    scopes::conditions(get(request, "context"), false)?;
    if !query.trim().is_empty()
        && !array(get(
            &crate::security::scan(&string(query), crate::security::LIMIT)?,
            "reasons",
        ))
        .is_empty()
    {
        return Err(err(
            "Unsafe query must not become returned context or audit content",
        ));
    }
    Ok(request)
}
pub fn git_context(root: &Path) -> Result<Value> {
    let mut rows = Vec::new();
    for (key, args) in [
        ("branch", vec!["symbolic-ref", "--quiet", "--short", "HEAD"]),
        ("revision", vec!["rev-parse", "--verify", "HEAD"]),
    ] {
        let args: Vec<_> = args.into_iter().map(OsString::from).collect();
        if let Some(value) =
            crate::identity_git::git(root, &args, true, false).map_err(|e| err(&e.to_string()))?
        {
            rows.push((
                key,
                string(crate::host::text(&value, "Git checkout context")?),
            ));
        }
    }
    Ok(object(rows))
}
pub fn observed_context(root: &Path, supplied: &Value) -> Result<(Value, Value)> {
    let known = git_context(root)?;
    for key in ["branch", "revision"] {
        if supplied.get(key).is_some_and(|v| Some(v) != known.get(key)) {
            return Err(err(
                "Current recall cannot substitute another branch/revision for the observed checkout",
            ));
        }
    }
    let mut actual = supplied.clone();
    bank_contract::merge(&mut actual, &known)?;
    Ok((actual, known))
}
pub struct ComparisonView;
impl crate::bank_state::ComparisonView for ComparisonView {
    fn observed_context(&self, root: &Path, request: &Value) -> Result<(Value, Value)> {
        observed_context(root, request)
    }
    fn invalidate_frontier_transition(&self, repo: &Path) -> Result<String> {
        crate::recall_catalog::invalidate_frontier_transition(repo, &crate::cli::pack_root()?)
            .map(str::to_owned)
    }
    fn safe_text(&self, value: &Value, maximum: usize) -> Result<()> {
        discovery::comparison_provider()?.safe_text(value, maximum)
    }
}
pub fn scope_reasons(project: &Value, entry: &Value, context: &Value) -> Result<Vec<String>> {
    let mut reasons = Vec::new();
    for scope in array(get(entry, "scope_chain")) {
        let decision = scopes::decide(&object([
            ("bank_project_id", project.clone()),
            ("project_id", project.clone()),
            ("scope", scope.clone()),
            ("context", context.clone()),
        ]))?;
        for reason in array(get(&decision, "reasons")) {
            if let Some(reason) = reason.as_str() {
                if !reasons.iter().any(|v| v == reason) {
                    reasons.push(reason.into());
                }
            }
        }
    }
    Ok(reasons)
}
pub fn matching(request: &Value, candidate: &Value) -> Result<Option<Value>> {
    let ids = array(get(request, "ids"));
    let paths = array(get(request, "paths"));
    let id = ids.contains(get(get(candidate, "record"), "id"));
    let path = paths.contains(get(candidate, "path"));
    if !ids.is_empty() || !paths.is_empty() {
        return Ok((id || path).then(|| {
            object([
                (
                    "strategy",
                    string(if id { "exact-id" } else { "exact-path" }),
                ),
                ("score", Value::Int((if id { 10000 } else { 9000 }).into())),
                ("matching_terms", Value::Array(Vec::new())),
            ])
        }));
    }
    let folded = crate::unicode::casefold(text(request, "query")?);
    let mut terms = Vec::new();
    for term in folded
        .split(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .filter(|s| !s.is_empty())
    {
        if !terms.contains(&term) {
            terms.push(term);
        }
    }
    let path = crate::unicode::casefold(text(candidate, "path")?);
    let content = crate::unicode::casefold(&format!(
        "{}\n{}",
        text(candidate, "path")?,
        text(candidate, "body")?
    ));
    if !terms.iter().all(|term| content.contains(term)) {
        return Ok(None);
    }
    let score: usize = terms
        .iter()
        .map(|term| {
            content.matches(term).count().min(16) + if path.contains(term) { 10 } else { 0 }
        })
        .sum();
    Ok(Some(object([
        (
            "strategy",
            string(if terms.is_empty() {
                "current-inventory"
            } else {
                "ordinary-text"
            }),
        ),
        ("score", Value::Int(score.into())),
        (
            "matching_terms",
            Value::Array(terms.into_iter().map(string).collect()),
        ),
    ])))
}
#[derive(Default)]
pub struct Evaluation {
    pub results: Vec<Value>,
    pub excluded: BTreeMap<String, usize>,
    pub used: usize,
    pub pool_count: usize,
    pub ranked_count: usize,
    pub canonical_bytes: usize,
}
fn exclude(counts: &mut BTreeMap<String, usize>, reason: &str) {
    *counts.entry(reason.into()).or_default() += 1;
}
fn exclude_reasons(counts: &mut BTreeMap<String, usize>, reasons: &Value, prefix: &str) {
    for reason in array(reasons).iter().filter_map(Value::as_str) {
        exclude(counts, &format!("{prefix}{reason}"));
    }
}
/// Invocation-local source access, implemented by canonical observations and the optional catalog.
pub trait Sources {
    fn source(&mut self, path: &str) -> Result<Vec<u8>>;
    fn parsed(&mut self, path: &str, text: &str) -> Result<(String, Value, String)>;
}
impl Sources for bank_observation::Observation {
    fn source(&mut self, path: &str) -> Result<Vec<u8>> {
        self.sources
            .get(path)
            .cloned()
            .ok_or_else(|| err("Canonical memory source is missing"))
    }
    fn parsed(&mut self, path: &str, _: &str) -> Result<(String, Value, String)> {
        self.parsed
            .get(path)
            .cloned()
            .ok_or_else(|| err("Canonical record metadata is missing"))
    }
}
impl Sources for crate::recall_catalog::Catalog {
    fn source(&mut self, path: &str) -> Result<Vec<u8>> {
        self.source(path, MAX_FILE)
    }
    fn parsed(&mut self, path: &str, text: &str) -> Result<(String, Value, String)> {
        self.parsed(path, text)
    }
}
pub(crate) fn catalog_runtime() -> Result<crate::recall_catalog::Runtime> {
    Ok(crate::recall_catalog::Runtime {
        pack: crate::cli::pack_root()?,
        binary: crate::host::current_executable()?,
    })
}
#[allow(clippy::too_many_arguments)]
pub fn evaluate_sources(
    root: &Path,
    bank: &Path,
    project: &Value,
    entries: &[Value],
    source: &mut dyn Sources,
    hashes: &Value,
    request: &Value,
    selection_request: &Value,
    context: &Value,
    now: &str,
    deferred: &BTreeSet<String>,
    discovered: Option<&BTreeSet<String>>,
) -> Result<Evaluation> {
    let mut output = Evaluation::default();
    let mut pool = Vec::new();
    let mut superseders: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        let path = text(entry, "path")?;
        let kind = text(entry, "format")?;
        if !matches!(kind, "standalone-record" | "context-prose") {
            exclude(&mut output.excluded, "non-guidance-format");
            continue;
        }
        if get(entry, "lifecycle").as_str() != Some("active") {
            exclude(&mut output.excluded, "active");
            continue;
        }
        if kind == "context-prose" && get(request, "include_context").as_bool() != Some(true) {
            exclude(&mut output.excluded, "orientation-not-requested");
            continue;
        }
        let reasons = scope_reasons(project, entry, context)?;
        if !reasons.is_empty() {
            for reason in reasons {
                exclude(&mut output.excluded, &reason);
            }
            continue;
        }
        let raw = source.source(path)?;
        output.canonical_bytes += raw.len().min(MAX_FILE + 1);
        if raw.len() > MAX_FILE {
            if kind == "standalone-record" {
                return Err(err(
                    "Active record exceeds safe inspection bounds; resolve its metadata before recall",
                ));
            }
            exclude(&mut output.excluded, "content-limit");
            continue;
        }
        if get(hashes, path).as_str() != Some(sha256_hex(&raw).as_str()) {
            return Err(err("Canonical memory changed while reading candidates"));
        }
        let content =
            std::str::from_utf8(&raw).map_err(|_| ContractError::non_utf8("canonical memory"))?;
        let mut metadata = None;
        let mut selection = Value::Null;
        let body;
        if kind == "standalone-record" {
            let (_, record, parsed_body) = source.parsed(path, content)?;
            let applicable = records::applicability(
                &record,
                &object([
                    ("bank_project_id", project.clone()),
                    ("context", context.clone()),
                ]),
            )?;
            selection = lifecycle::current(
                &record,
                &object([
                    ("authorized-scope", Value::Bool(true)),
                    ("applicable", get(&applicable, "applicable").clone()),
                    ("now", string(now)),
                    ("minimum-trust", get(request, "minimum_trust").clone()),
                    ("active-superseders", Value::Array(Vec::new())),
                ]),
            )?;
            if ![
                "authorized-scope",
                "applicable",
                "active",
                "valid-time",
                "sufficient-trust",
            ]
            .iter()
            .all(|key| get(get(&selection, "gates"), key).as_bool() == Some(true))
            {
                exclude_reasons(&mut output.excluded, get(&selection, "reasons"), "");
                exclude_reasons(&mut output.excluded, get(&applicable, "reasons"), "");
                continue;
            }
            for reference in array(get(&record, "supersedes"))
                .iter()
                .chain(array(get(&record, "corrects")))
            {
                let key = (
                    text(reference, "id")?.into(),
                    json::encode(get(reference, "version"), json::COMPACT_UTF8)?,
                );
                superseders.entry(key).or_default().insert(format!(
                    "{}/{}",
                    text(&record, "id")?,
                    json::encode(get(&record, "version"), json::COMPACT_UTF8)?
                ));
            }
            metadata = Some(record);
            body = parsed_body;
        } else {
            body = content.to_owned();
        }
        let safety = crate::security::scan(
            &string(format!(
                "{content}\n{}",
                json::encode(
                    &object([
                        ("path", string(path)),
                        ("scope_chain", get(entry, "scope_chain").clone())
                    ]),
                    json::UTF8
                )?
            )),
            crate::security::LIMIT,
        );
        let safety = match safety {
            Ok(value) => value,
            Err(_) => {
                exclude(&mut output.excluded, "security-content-limit");
                continue;
            }
        };
        if !array(get(&safety, "reasons")).is_empty() {
            exclude_reasons(&mut output.excluded, get(&safety, "reasons"), "security:");
            continue;
        }
        let assessment = if kind == "context-prose" && path == "activeContext.md" {
            handoff::assess(&body, project, context, root, bank, false)?
        } else {
            Value::Null
        };
        if get(&assessment, "managed_checkpoint").as_bool() == Some(true)
            && get(&assessment, "applicable").as_bool() != Some(true)
        {
            exclude_reasons(
                &mut output.excluded,
                get(&assessment, "reasons"),
                "handoff:",
            );
            continue;
        }
        let mut candidate = object([
            ("path", string(path)),
            ("format", string(kind)),
            ("body", string(body)),
            ("source_sha256", get(hashes, path).clone()),
            ("scope_chain", get(entry, "scope_chain").clone()),
            ("source_class", string("external")),
            ("untrusted_data", Value::Bool(true)),
            ("source_assertions_verified", Value::Bool(false)),
            ("admission_verified", Value::Bool(false)),
            ("procedural_guidance", Value::Bool(false)),
        ]);
        if let Some(metadata) = metadata {
            set(&mut candidate, "record", metadata);
            set(&mut candidate, "selection_class", string("current-record"));
            set(&mut candidate, "selection", selection);
        } else {
            set(
                &mut candidate,
                "selection_class",
                string("orientation-only"),
            );
            set(&mut candidate, "trust", Value::Null);
            set(&mut candidate, "assignment", Value::Bool(false));
            set(&mut candidate, "context_assessment", assessment);
            set(
                &mut candidate,
                "explanation",
                string(
                    "Active route with qualified checkpoint conditions when present; independently qualify prose statements before use",
                ),
            );
        }
        pool.push(candidate);
    }
    output.pool_count = pool.len();
    let mut ranked = Vec::new();
    for mut candidate in pool {
        if let Some(record) = candidate.get("record") {
            let key = (
                text(record, "id")?.into(),
                json::encode(get(record, "version"), json::COMPACT_UTF8)?,
            );
            let replacing = superseders
                .get(&key)
                .into_iter()
                .flatten()
                .cloned()
                .map(string)
                .collect();
            let selection = lifecycle::current(
                record,
                &object([
                    ("authorized-scope", Value::Bool(true)),
                    ("applicable", Value::Bool(true)),
                    ("now", string(now)),
                    ("minimum-trust", get(request, "minimum_trust").clone()),
                    ("active-superseders", Value::Array(replacing)),
                ]),
            )?;
            if get(&selection, "current").as_bool() != Some(true) {
                exclude_reasons(&mut output.excluded, get(&selection, "reasons"), "");
                continue;
            }
            if !array(get(request, "kinds")).contains(get(record, "kind")) {
                exclude(&mut output.excluded, "memory-type");
                continue;
            }
            set(&mut candidate, "selection", selection);
        }
        let path = text(&candidate, "path")?;
        if deferred.contains(path) {
            exclude(&mut output.excluded, "pending-semantic-conflict");
            continue;
        }
        if discovered.is_some_and(|selected| !selected.contains(path)) {
            exclude(&mut output.excluded, "outside-semantic-selection");
            continue;
        }
        let Some(matching) = matching(selection_request, &candidate)? else {
            exclude(&mut output.excluded, "selector-or-text-mismatch");
            continue;
        };
        set(&mut candidate, "match", matching);
        ranked.push(candidate);
    }
    ranked.sort_by(|a, b| {
        let score = |v: &Value| {
            get(get(v, "match"), "score")
                .as_int()
                .and_then(|n| n.as_u64())
                .unwrap_or(0)
        };
        score(b)
            .cmp(&score(a))
            .then_with(|| get(a, "path").as_str().cmp(&get(b, "path").as_str()))
    });
    output.ranked_count = ranked.len();
    let limit = get(request, "limit")
        .as_int()
        .and_then(|n| n.as_u64())
        .ok_or_else(|| err("Invalid result limit"))?;
    for candidate in ranked {
        let size = encoded(&candidate)?.len();
        if output.results.len() as u64 >= limit || output.used + size > MAX_RESULT {
            exclude(&mut output.excluded, "result-budget");
            continue;
        }
        output.used += size;
        output.results.push(candidate);
    }
    Ok(output)
}

pub fn retrieve(
    repo: &Path,
    request: &Value,
    owned_lock: Option<&[u8]>,
    deferred_paths: &[String],
) -> Result<Value> {
    let started = crate::clock::monotonic_ns();
    request_value(request)?;
    if deferred_paths.len() > crate::sharing_limits::MAX_FILES {
        return Err(err("Deferred recall paths require a complete bounded set"));
    }
    for path in deferred_paths {
        if path != "MEMORY.md" {
            bank_contract::path_value(&string(path))?;
        }
    }
    let deferred = deferred_paths.iter().cloned().collect();
    let runtime = if request.get("discovery").is_some() {
        catalog_runtime().ok()
    } else {
        None
    };
    if let Some(runtime) = &runtime {
        let cached = crate::validation_memo::suspended(|| {
            let catalog = crate::recall_catalog::load_catalog(
                repo,
                text(request, "project_id")?,
                runtime,
                owned_lock,
            )?;
            retrieve_canonical(
                repo,
                request,
                owned_lock,
                &deferred,
                started,
                Some(catalog),
                None,
            )
        });
        if let Ok(result) = cached {
            return Ok(result);
        }
    }
    let engine = runtime
        .as_ref()
        .and_then(|runtime| crate::recall_catalog::generation(runtime).ok());
    retrieve_canonical(
        repo,
        request,
        owned_lock,
        &deferred,
        started,
        None,
        engine.as_ref(),
    )
}
fn retrieve_canonical(
    repo: &Path,
    request: &Value,
    owner: Option<&[u8]>,
    deferred: &BTreeSet<String>,
    started: u64,
    mut cached: Option<crate::recall_catalog::Catalog>,
    engine: Option<&crate::recall_catalog::Generation>,
) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    let frontier_provider = writes::recovery_frontier_provider()?;
    let frontier = frontier_provider.observe_owned(&root, owner, None)?;
    let scope = frontier_provider.require_scope_owned(
        &root,
        get(request, "project_id"),
        None,
        owner,
        None,
    )?;
    let (context, observed_git) = observed_context(&root, get(request, "context"))?;
    let mut observation = if cached.is_none() {
        Some(bank_observation::observe(&root, owner, None)?)
    } else {
        None
    };
    let (checked, hashes) = if let Some(catalog) = &cached {
        (catalog.checked.clone(), catalog.hashes.clone())
    } else {
        let observed = observation
            .as_ref()
            .ok_or_else(|| err("Missing canonical observation"))?;
        (observed.checked.clone(), observed.hashes.clone())
    };
    let project = get(&checked, "project_id");
    if project != get(request, "project_id") {
        return Err(err("Canonical project changed after scope resolution"));
    }
    let mut selection = request.clone();
    let mut discovered = None;
    let mut discovery_result = None;
    if let Some(trace) = request.get("discovery") {
        discovery::trace_value(trace, true)?;
        let paths: BTreeSet<String> = array(get(trace, "sources"))
            .iter()
            .map(|row| text(row, "path").map(str::to_owned))
            .collect::<Result<_>>()?;
        let entries: Vec<_> = array(get(&checked, "entries"))
            .iter()
            .filter(|entry| {
                get(entry, "path")
                    .as_str()
                    .is_some_and(|path| paths.contains(path))
            })
            .collect();
        if array(get(request, "ids"))
            .iter()
            .any(|id| !entries.iter().any(|entry| get(entry, "id") == id))
        {
            return Err(err(
                "Traced stable-ID lookup must name an actual discovered source version",
            ));
        }
        let mut selected: BTreeSet<String> = array(get(request, "paths"))
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        for entry in entries {
            if array(get(request, "ids")).contains(get(entry, "id")) {
                selected.insert(text(entry, "path")?.into());
            }
        }
        if array(get(request, "paths")).is_empty() && array(get(request, "ids")).is_empty() {
            selected = paths;
        }
        discovery_result = Some(discovery::check_trace(
            &root,
            trace,
            &checked,
            &hashes,
            &scope,
            "recall",
            &selected,
            owner,
            Some(get(request, "context")),
        )?);
        set(&mut selection, "ids", Value::Array(Vec::new()));
        set(
            &mut selection,
            "paths",
            Value::Array(selected.iter().cloned().map(string).collect()),
        );
        discovered = Some(selected);
    }
    let now = crate::host::utc_now_isoformat()?;
    let entries = if let Some(catalog) = &cached {
        catalog.selected_entries(
            &discovered
                .as_ref()
                .ok_or_else(|| err("Catalog recall requires semantic selection"))?
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        )?
    } else {
        array(get(&checked, "entries")).to_vec()
    };
    let source: &mut dyn Sources = if let Some(catalog) = &mut cached {
        catalog
    } else {
        observation
            .as_mut()
            .ok_or_else(|| err("Missing canonical observation"))?
    };
    let evaluation = evaluate_sources(
        &root,
        &bank,
        project,
        &entries,
        source,
        &hashes,
        request,
        &selection,
        &context,
        &now,
        deferred,
        discovered.as_ref(),
    )?;
    frontier_provider.require_scope_owned(&root, project, Some(&scope), owner, None)?;
    let ids: BTreeSet<String> = evaluation
        .results
        .iter()
        .filter_map(|row| row.get("record"))
        .filter_map(|record| get(record, "id").as_str())
        .map(str::to_owned)
        .collect();
    let audit = crate::security::audit(
        &root,
        &crate::host::uuid4()?,
        "retrieval",
        "checked",
        &scope,
        &["external"],
        &[],
        &ids.into_iter().collect::<Vec<_>>(),
        "policy-check",
        None,
    )?;
    let mut catalog_updated = false;
    let (payload_bytes, file_reads, parsed_records) = if let Some(catalog) = &cached {
        let metrics = catalog.recheck(owner)?;
        (
            get(&metrics, "payload_bytes")
                .as_int()
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as usize
                + catalog.payload_bytes_read,
            get(&metrics, "file_reads")
                .as_int()
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as usize
                + catalog.payload_file_reads,
            catalog.parsed_records,
        )
    } else {
        let observed = observation
            .as_mut()
            .ok_or_else(|| err("Missing canonical observation"))?;
        if let Some(engine) = engine {
            catalog_updated = crate::recall_catalog::publish(observed, engine).unwrap_or(false);
        }
        observed.recheck("recall-final-source-recheck")?;
        (
            observed.payload_bytes_read,
            observed.payload_file_reads,
            observed.parsed.len(),
        )
    };
    if git_context(&root)? != observed_git {
        return Err(err(
            "Bank or checkout context changed during recall; reread before use",
        ));
    }
    frontier_provider.require_scope_owned(&root, project, Some(&scope), owner, None)?;
    frontier_provider.check(&frontier, &root, owner, None)?;
    let selected_count = evaluation.results.len();
    let metrics = object([
        (
            "inventory_files",
            Value::Int(hashes.as_object().map_or(0, |rows| rows.len()).into()),
        ),
        (
            "entries",
            Value::Int(array(get(&checked, "entries")).len().into()),
        ),
        (
            "qualified_candidates",
            Value::Int(evaluation.pool_count.into()),
        ),
        ("ranked", Value::Int(evaluation.ranked_count.into())),
        ("selected", Value::Int(selected_count.into())),
        (
            "bounded_candidate_bytes_read",
            Value::Int(evaluation.canonical_bytes.into()),
        ),
        (
            "operation_local_observation_used",
            Value::Bool(cached.is_none()),
        ),
        (
            "canonical_observation_payload_bytes_read",
            Value::Int(payload_bytes.into()),
        ),
        (
            "canonical_observation_file_reads",
            Value::Int(file_reads.into()),
        ),
        ("parsed_records", Value::Int(parsed_records.into())),
        ("catalog_updated", Value::Bool(catalog_updated)),
        ("inspected_entries", Value::Int(entries.len().into())),
        (
            "candidate_count_scope",
            string(if cached.is_some() {
                "semantic-selection-invalidation-and-oversize-guards"
            } else {
                "complete-bank"
            }),
        ),
        ("result_bytes", Value::Int(evaluation.used.into())),
        (
            "elapsed_seconds",
            Value::Float(
                crate::clock::monotonic_ns().saturating_sub(started) as f64 / 1_000_000_000.0,
            ),
        ),
    ]);
    let mut result = object([
        ("schema", string("agent-memory/retrieval-result-v1")),
        ("project_id", project.clone()),
        ("results", Value::Array(evaluation.results)),
        ("evaluated_at", string(now)),
        ("context", context),
        ("scope_context", scope),
        (
            "bank_observation_sha256",
            string(sha256_hex(&encoded(&object([
                ("project_id", project.clone()),
                ("sources", hashes.clone()),
            ]))?)),
        ),
        (
            "excluded_counts",
            object(
                evaluation
                    .excluded
                    .into_iter()
                    .map(|(reason, count)| (reason, Value::Int(count.into()))),
            ),
        ),
        ("metrics", metrics),
        ("audit", audit),
        ("canonical_writes_performed", Value::Bool(false)),
        ("audit_writes_performed", Value::Bool(true)),
        (
            "derived_cache_writes_performed",
            if catalog_updated {
                Value::Bool(true)
            } else if cached.is_some() || request.get("discovery").is_none() {
                Value::Bool(false)
            } else {
                Value::Null
            },
        ),
        ("optional_indexes_used", Value::Bool(cached.is_some())),
        ("source_assertions_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("code_working_bytes_verified", Value::Bool(false)),
        (
            "validation_scope",
            string("metadata-currentness-and-bounded-file-recall"),
        ),
    ]);
    bank_contract::merge(
        &mut result,
        &bank_contract::recovery_coverage_provider()?.public_coverage(&frontier)?,
    )?;
    if let Some(discovery) = discovery_result {
        set(&mut result, "discovery", discovery);
    }
    Ok(result)
}
pub fn command_result(result: Result<Value>) -> CommandResult {
    match result {
        Ok(value) => CommandResult::json(value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let capacity = crate::bank_capacity::details(&error);
            let mut value = object([
                ("schema", string("agent-memory/retrieval-result-v1")),
                ("verified", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
                (
                    "error",
                    string(
                        if matches!(error.kind(), crate::error::Kind::BankCapacity { .. }) {
                            error.to_string()
                        } else {
                            "Invalid, unsafe or changed recall input, scope or canonical state; preserve memory and reread before use".into()
                        },
                    ),
                ),
            ]);
            let _ = bank_contract::merge(&mut value, &capacity);
            CommandResult::json(value, 1, json::DEFAULT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionaries_ignore_order_without_losing_numeric_precision_or_array_order() {
        assert!(same(
            &object([("a", Value::Int(1.into())), ("b", Value::Bool(false))]),
            &object([("b", Value::Bool(false)), ("a", Value::Float(1.0))]),
        ));
        assert!(!same(
            &Value::Int(9_007_199_254_740_993u64.into()),
            &Value::Float(9_007_199_254_740_992.0)
        ));
        assert!(same(&Value::Int(0.into()), &Value::Float(-0.0)));
        assert!(!same(
            &Value::Array(vec![string("a"), string("b")]),
            &Value::Array(vec![string("b"), string("a")])
        ));
    }
}
