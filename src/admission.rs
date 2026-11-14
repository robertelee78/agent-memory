//! Small source-bound capture and governed admission, without publication.
use crate::bank_contract as contract;
use crate::bank_transactions as tx;
use crate::bank_writes as writes;
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{encoded, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::lifecycle::{self, get, object, string};
use crate::{
    admission_navigation, fs, host, layout_resolution, records, scope_binding, security,
    trust_paths,
};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const REQUEST_LIMIT: usize = 16384;
pub const INPUT_LIMIT: usize = 65536;
const REQUEST: &[&str] = &[
    "project_id",
    "candidate",
    "source_class",
    "repository_evidence",
    "assessment",
];
const ASSESSMENT: &[&str] = &[
    "category",
    "utility",
    "durable",
    "evidence_confidence",
    "evidence_reason",
    "confirmed",
    "source_redundant",
    "work_state_copy",
    "review",
];
const CAPTURE: &[&str] = &[
    "schema",
    "request",
    "scope_context",
    "source_observation",
    "decision",
    "reasons",
];
const EVIDENCE: &[&str] = &[
    "capture",
    "comparison_request",
    "comparison_proof",
    "recorded_at",
    "unit_resolution_claims",
    "comparison_proof_sha256",
    "reference_map_sha256",
];

/// Source-bound semantic comparison; implemented by comparison.
pub trait Comparison: Send + Sync {
    fn request_value(&self, request: &Value) -> Result<()>;
    fn check(
        &self,
        repo: &Path,
        request: &Value,
        proof: &Value,
        owner: Option<&[u8]>,
    ) -> Result<Value>;
}
static COMPARISON: RwLock<Option<Arc<dyn Comparison>>> = RwLock::new(None);
pub fn register_comparison(value: Option<Arc<dyn Comparison>>) {
    *COMPARISON.write().unwrap_or_else(|e| e.into_inner()) = value;
}
pub(crate) fn comparison_provider() -> Result<Arc<dyn Comparison>> {
    COMPARISON
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| ContractError::missing_provider("Comparison provider is not registered"))
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid admission metadata"))
}
fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|rows| {
        rows.len() == fields.len() && rows.iter().all(|(k, _)| fields.contains(&k.as_str()))
    })
}
fn array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .ok_or_else(|| err("Invalid admission evidence list"))
}
fn put(value: &mut Value, key: &str, item: Value) -> Result<()> {
    contract::merge(value, &object([(key, item)]))
}
fn remove(value: &mut Value, key: &str) {
    if let Value::Object(rows) = value {
        rows.retain(|(name, _)| name != key);
    }
}
fn same(a: &Value, b: &Value) -> Result<bool> {
    Ok(encoded(a)? == encoded(b)?)
}
fn render(metadata: &Value) -> Result<String> {
    frontmatter::write(
        metadata
            .as_object()
            .ok_or_else(|| err("Invalid record metadata"))?,
    )
}
fn git(repo: &Path, args: &[&str]) -> Result<Value> {
    crate::identity_git::git(
        repo,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        true,
        false,
    )
    .map_err(|_| err("Originating Git context is unavailable"))?
    .map(|s| host::text(&s, "admission Git context").map(string))
    .transpose()
    .map(|v| v.unwrap_or(Value::Null))
}
fn source_observation(root: &Path, source: &Value) -> Result<Value> {
    Ok(object([
        ("head", git(root, &["rev-parse", "--verify", "HEAD"])?),
        (
            "branch",
            git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?,
        ),
        ("repository_evidence", source.clone()),
    ]))
}
pub fn result(operation: &str, fields: &Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/admission-result-v1")),
        ("operation", string(operation)),
        ("semantic_truth_verified", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
        ("main_freshness_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("identity_authenticated", Value::Bool(false)),
    ]);
    contract::merge(&mut value, fields)?;
    Ok(value)
}
pub fn request_value(request: &Value) -> Result<Value> {
    let candidate = get(request, "candidate");
    if !exact(request, REQUEST)
        || encoded(request)?.len() > REQUEST_LIMIT
        || !uuid_valid(get(request, "project_id"))
        || !get(request, "source_class")
            .as_str()
            .is_some_and(|s| security::CLASSES.contains(&s))
        || !exact(candidate, &["metadata", "body"])
    {
        return Err(err(
            "Capture requires one small explicit project learning and source classification",
        ));
    }
    let body = text(candidate, "body")?;
    let (kind, metadata) = records::validate(get(candidate, "metadata"), body, &[])?;
    if kind != "standalone-record"
        || get(&metadata, "project-id") != get(request, "project_id")
        || get(&metadata, "status").as_str() != Some("candidate")
        || get(&metadata, "version") != &Value::Int(1.into())
        || !array(get(&metadata, "corrects"))?.is_empty()
        || !array(get(&metadata, "supersedes"))?.is_empty()
        || metadata.get("superseded-by").is_some()
        || get(get(&metadata, "source"), "kind") != get(request, "source_class")
        || body.len() > 4096
    {
        return Err(err(
            "Capture a small first-version candidate, preserving its true source; updates use consolidation",
        ));
    }
    if get(get(&metadata, "origin"), "code-state").as_str() == Some("unknown")
        && get(&metadata, "applicability").as_str() != Some("unknown")
    {
        return Err(err(
            "Unknown code applicability must remain unknown, not become unconditional guidance",
        ));
    }
    let a = get(request, "assessment");
    let useful = |key| {
        get(a, key)
            .as_str()
            .is_some_and(|s| !contract::trim(s).is_empty() && s.len() <= 1024)
    };
    if !exact(a, ASSESSMENT)
        || !get(a, "category").as_str().is_some_and(|s| {
            [
                "durable-discovery",
                "verified-fix",
                "failed-approach",
                "user-correction",
                "confirmed-decision",
            ]
            .contains(&s)
        })
        || !get(a, "evidence_confidence")
            .as_str()
            .is_some_and(|s| ["confirmed", "inferred", "unconfirmed"].contains(&s))
        || [
            "durable",
            "confirmed",
            "source_redundant",
            "work_state_copy",
        ]
        .iter()
        .any(|k| get(a, k).as_bool().is_none())
        || !useful("utility")
        || !useful("evidence_reason")
        || (get(a, "review") != &Value::Null && !useful("review"))
    {
        return Err(err(
            "Assess utility, durability, evidence confidence, confirmation and redundant source/work state explicitly",
        ));
    }
    if get(request, "source_class").as_str() == Some("verified-repository")
        && get(request, "repository_evidence") == &Value::Null
    {
        return Err(err(
            "Repository classification requires actual current local source evidence",
        ));
    }
    Ok(metadata)
}
pub fn classify(repo: &Path, request: &Value, owner: Option<&[u8]>) -> Result<Value> {
    let metadata = request_value(request)?;
    let (root, _) = writes::bank_path(repo)?;
    let frontier = writes::recovery_frontier_provider()?;
    let context =
        frontier.require_scope_owned(&root, get(request, "project_id"), None, owner, None)?;
    if get(get(&metadata, "origin"), "developer-id") != get(&context, "developer_id") {
        return Err(err(
            "A new capture must name the actual detecting developer; existing provenance belongs to consolidation",
        ));
    }
    let (checked, observed) = writes::snapshot(&root, owner)?;
    let source = get(request, "repository_evidence");
    if source != &Value::Null {
        security::repository_source(&root, source)?;
        if !array(get(&metadata, "evidence"))?.contains(get(source, "path")) {
            return Err(err(
                "Retain the observed supporting repository path in record evidence",
            ));
        }
    }
    let observation = source_observation(&root, source)?;
    let branch = get(&observation, "branch").as_str();
    let head = get(&observation, "head").as_str();
    let conditions = get(&metadata, "conditions");
    let code = get(get(&metadata, "origin"), "code-state").as_str();
    if code == Some("unmerged")
        && !(branch.is_some_and(|b| {
            get(conditions, "branch").as_str() == Some(b)
                || get(&metadata, "scope").as_str() == Some(format!("branch:{b}").as_str())
        }) || head.is_some_and(|h| get(conditions, "revision").as_str() == Some(h))
            || get(&metadata, "applicability").as_str() == Some("unknown")
            || conditions
                .as_object()
                .is_some_and(|rows| rows.iter().any(|(_, v)| v == &Value::Null)))
    {
        return Err(err(
            "Retain observed branch/revision or unknown applicability for unfinished code",
        ));
    }
    if code == Some("default-branch") {
        return Err(err(
            "Local capture cannot prove default-branch freshness; retain conditional unmerged or unknown code state",
        ));
    }
    let assessment = get(request, "assessment");
    let contents = [
        render(&metadata)? + text(get(request, "candidate"), "body")?,
        String::from_utf8(encoded(assessment)?).map_err(|_| err("Invalid assessment text"))?,
        text(assessment, "utility")?.to_owned(),
        text(assessment, "evidence_reason")?.to_owned(),
    ];
    let mut reasons = BTreeSet::new();
    for content in contents {
        let scanned = security::scan(&string(content), security::LIMIT)?;
        for reason in array(get(&scanned, "reasons"))? {
            reasons.insert(
                reason
                    .as_str()
                    .ok_or_else(|| err("Invalid scan reason"))?
                    .to_owned(),
            );
        }
    }
    let mut reasons: Vec<String> = reasons.into_iter().collect();
    let deleted: BTreeSet<_> = array(get(&checked, "entries"))?
        .iter()
        .filter(|e| get(e, "format").as_str() == Some("tombstone"))
        .filter_map(|e| get(e, "id").as_str())
        .collect();
    if deleted.contains(text(&metadata, "id")?)
        || array(get(&metadata, "derived-from"))?
            .iter()
            .any(|r| get(r, "id").as_str().is_some_and(|id| deleted.contains(id)))
    {
        reasons.push("terminal-deletion".into());
    }
    let (decision, special) = if !reasons.is_empty() {
        ("quarantine", None)
    } else if get(assessment, "durable") == &Value::Bool(false)
        || get(assessment, "source_redundant") == &Value::Bool(true)
        || get(assessment, "work_state_copy") == &Value::Bool(true)
    {
        ("discard", Some("not-distinct-durable-learning"))
    } else if get(&metadata, "kind").as_str() == Some("procedural-candidate") {
        (
            "review-required",
            Some("explicit-procedure-promotion-required"),
        )
    } else if get(assessment, "confirmed") != &Value::Bool(true)
        || get(assessment, "evidence_confidence").as_str() != Some("confirmed")
    {
        ("review-required", Some("unconfirmed-or-inferred"))
    } else if ["assistant", "tool", "external"].contains(&text(request, "source_class")?)
        && (source == &Value::Null || get(assessment, "review") == &Value::Null)
    {
        ("review-required", Some("stronger-source-review-required"))
    } else {
        ("eligible", None)
    };
    if let Some(reason) = special {
        reasons.push(reason.into());
    }
    frontier.require_scope_owned(
        &root,
        get(&checked, "project_id"),
        Some(&context),
        owner,
        None,
    )?;
    if !same(&writes::snapshot(&root, owner)?.1, &observed)? {
        return Err(err(
            "Memory changed during capture; reread before classification",
        ));
    }
    if source != &Value::Null {
        security::repository_source(&root, source)?;
    }
    Ok(object([
        ("schema", string("agent-memory/capture-v1")),
        ("request", request.clone()),
        ("scope_context", context),
        ("source_observation", observation),
        ("decision", string(decision)),
        (
            "reasons",
            Value::Array(reasons.into_iter().map(string).collect()),
        ),
    ]))
}
pub fn capture(repo: &Path, request: &Value) -> Result<Value> {
    let frontier_provider = writes::recovery_frontier_provider()?;
    let frontier = frontier_provider.observe(repo)?;
    let proof = classify(repo, request, None)?;
    let reasons: Vec<String> = array(get(&proof, "reasons"))?
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| security::REASONS.contains(s))
        .map(str::to_owned)
        .collect();
    let event = security::audit(
        repo,
        &host::uuid4()?,
        "admission",
        "checked",
        get(&proof, "scope_context"),
        &[text(request, "source_class")?],
        &reasons,
        &[text(get(get(request, "candidate"), "metadata"), "id")?.to_owned()],
        "policy-check",
        None,
    )?;
    frontier_provider.check(&frontier, repo, None, None)?;
    let decision = text(&proof, "decision")?;
    let mut fields = object([
        ("decision", string(decision)),
        ("reasons", get(&proof, "reasons").clone()),
        (
            "capture",
            if ["eligible", "review-required"].contains(&decision) {
                proof.clone()
            } else {
                Value::Null
            },
        ),
        (
            "source_bytes_observed",
            Value::Bool(get(request, "repository_evidence") != &Value::Null),
        ),
        (
            "candidate_is_unconfirmed",
            Value::Bool(decision != "eligible"),
        ),
        ("admission_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("audit", event),
    ]);
    contract::merge(
        &mut fields,
        &contract::recovery_coverage_provider()?.public_coverage(&frontier)?,
    )?;
    result("capture", &fields)
}
pub fn draft(repo: &Path, value: &Value) -> Result<Value> {
    draft_with_sources(repo, value, host::Sources::default())
}
pub fn draft_with_sources(repo: &Path, value: &Value, sources: host::Sources) -> Result<Value> {
    if !exact(
        value,
        &[
            "body",
            "kind",
            "scope",
            "applicability",
            "conditions",
            "source",
            "evidence",
            "harness",
            "session",
            "code_state",
            "assessment",
            "repository_evidence",
        ],
    ) || get(value, "source").as_object().is_none()
    {
        return Err(err(
            "Draft requires explicit learning, source, applicability and assessment",
        ));
    }
    let context = scope_binding::require(repo, None, Default::default())?;
    let now = string((sources.utc_now)()?);
    let metadata = object([
        ("schema", string("agent-memory/record-v1")),
        ("id", string((sources.uuid4)()?)),
        ("version", Value::Int(1.into())),
        ("project-id", get(&context, "project_id").clone()),
        ("kind", get(value, "kind").clone()),
        ("status", string("candidate")),
        ("scope", get(value, "scope").clone()),
        ("applicability", get(value, "applicability").clone()),
        ("conditions", get(value, "conditions").clone()),
        ("source", get(value, "source").clone()),
        ("evidence", get(value, "evidence").clone()),
        (
            "origin",
            object([
                ("developer-id", get(&context, "developer_id").clone()),
                ("harness", get(value, "harness").clone()),
                ("session", get(value, "session").clone()),
                ("code-state", get(value, "code_state").clone()),
            ]),
        ),
        ("recorded-at", now.clone()),
        ("updated", now),
        ("valid-from", Value::Null),
        ("valid-until", Value::Null),
        ("derived-from", Value::Array(vec![])),
        ("corrects", Value::Array(vec![])),
        ("supersedes", Value::Array(vec![])),
    ]);
    let request = object([
        ("project_id", get(&context, "project_id").clone()),
        (
            "candidate",
            object([("metadata", metadata), ("body", get(value, "body").clone())]),
        ),
        ("source_class", get(get(value, "source"), "kind").clone()),
        (
            "repository_evidence",
            get(value, "repository_evidence").clone(),
        ),
        ("assessment", get(value, "assessment").clone()),
    ]);
    request_value(&request)?;
    result(
        "draft",
        &object([
            ("request", request),
            ("admission_verified", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
            (
                "note",
                string(
                    "Explicit agent assessment only; capture and semantic comparison are still required.",
                ),
            ),
        ]),
    )
}
pub fn start(repo: &Path, value: &Value) -> Result<Value> {
    if !exact(value, &["candidate", "task", "searches"]) {
        return Err(err(
            "Start requires explicit candidate, task and alternative searches",
        ));
    }
    let drafted = draft(repo, get(value, "candidate"))?;
    let request = get(&drafted, "request");
    let comparison_request = object([
        ("project_id", get(request, "project_id").clone()),
        ("candidate", get(request, "candidate").clone()),
        ("searches", get(value, "searches").clone()),
        ("read_paths", Value::Array(vec![])),
    ]);
    let captured = capture(repo, request)?;
    let eligible = get(&captured, "decision").as_str() == Some("eligible");
    let discovery = if eligible {
        comparison_provider()?.request_value(&comparison_request)?;
        crate::discovery::start(
            repo,
            get(value, "task"),
            "comparison",
            &object(Vec::<(String, Value)>::new()),
        )?
    } else {
        Value::Null
    };
    let mut fields = object([
        ("decision", get(&captured, "decision").clone()),
        ("reasons", get(&captured, "reasons").clone()),
        ("capture", get(&captured, "capture").clone()),
        ("discovery", discovery),
        (
            "comparison_request",
            if eligible {
                comparison_request
            } else {
                Value::Null
            },
        ),
        ("admission_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]);
    if eligible {
        put(
            &mut fields,
            "note",
            string(
                "Discovery is incomplete; select sources and perform semantic review before admission.",
            ),
        )?;
    }
    result("start", &fields)
}
pub fn capture_value(repo: &Path, value: &Value, owner: Option<&[u8]>) -> Result<Value> {
    let value = value.get("capture").unwrap_or(value);
    if !exact(value, CAPTURE) || get(value, "schema").as_str() != Some("agent-memory/capture-v1") {
        return Err(err("Use the exact current capture artifact"));
    }
    let current = classify(repo, get(value, "request"), owner)?;
    if !same(value, &current)? || get(&current, "decision").as_str() != Some("eligible") {
        return Err(err(
            "Capture is changed, unconfirmed or ineligible; inspect its evidence again",
        ));
    }
    Ok(current)
}
pub fn desired(base: &Value, evidence: &Value, operation_id: &str) -> Result<Value> {
    let capture = get(evidence, "capture");
    let request = get(capture, "request");
    let candidate = get(request, "candidate");
    let mut metadata = get(candidate, "metadata").clone();
    let assessment = get(request, "assessment");
    let mut reason = format!(
        "{}: {} Evidence assessment: {}",
        text(assessment, "category")?,
        text(assessment, "utility")?,
        text(assessment, "evidence_reason")?
    );
    if let Some(review) = get(assessment, "review").as_str() {
        reason.push_str(" Source review: ");
        reason.push_str(review);
    }
    contract::merge(
        &mut metadata,
        &object([
            ("status", string("active")),
            ("trust", string("reviewed")),
            ("visibility", string("visible")),
            ("updated", get(evidence, "recorded_at").clone()),
            ("admission-reason", string(reason)),
        ]),
    )?;
    remove(&mut metadata, "state-reason");
    remove(&mut metadata, "redaction");
    let source = get(request, "repository_evidence");
    if source != &Value::Null {
        let reference = string(format!(
            "repo:{}#sha256={}",
            text(source, "path")?,
            text(source, "sha256")?
        ));
        let mut references = array(get(&metadata, "evidence"))?.to_vec();
        if !references.contains(&reference) {
            references.push(reference);
        }
        let mut unique = Vec::new();
        for reference in references {
            if !unique.contains(&reference) {
                unique.push(reference);
            }
        }
        put(&mut metadata, "evidence", Value::Array(unique))?;
    }
    records::validate(&metadata, text(candidate, "body")?, &[])?;
    let event = object([
        ("id", string(operation_id)),
        ("evidence", get(&metadata, "evidence").clone()),
        (
            "authority",
            get(get(capture, "scope_context"), "developer_id").clone(),
        ),
        ("reason", get(assessment, "utility").clone()),
        ("reviewed", Value::Bool(true)),
    ]);
    if get(
        &lifecycle::transition(get(candidate, "metadata"), &string("admit"), &event)?,
        "legal",
    ) != &Value::Bool(true)
    {
        return Err(err(
            "Candidate does not satisfy the shared admission transition",
        ));
    }
    let path = format!("topics/{}.md", text(&metadata, "id")?);
    let history = format!("history/events/{operation_id}.md");
    if base.get(&path).is_some() || base.get(&history).is_some() {
        return Err(err(
            "A new learning/event must not replace an existing source",
        ));
    }
    let history_metadata = object([
        ("schema", string("agent-memory/history-event-v1")),
        ("id", string(operation_id)),
        ("project-id", get(&metadata, "project-id").clone()),
        ("kind", string("memory-admitted")),
        (
            "record",
            object([
                ("id", get(&metadata, "id").clone()),
                ("version", get(&metadata, "version").clone()),
            ]),
        ),
        (
            "actor",
            get(get(capture, "scope_context"), "developer_id").clone(),
        ),
        ("recorded-at", get(evidence, "recorded_at").clone()),
        ("evidence", get(&metadata, "evidence").clone()),
    ]);
    records::event(&history_metadata)?;
    let mut files = if let Some(navigation) = evidence.get("navigation") {
        admission_navigation::apply(
            base,
            navigation,
            &path,
            &history,
            &metadata,
            get(evidence, "recorded_at")
                .as_str()
                .ok_or_else(|| err("Admission evidence recorded_at is malformed"))?,
        )?
    } else {
        let (mut router, mut routes, omitted) = contract::parse_router(text(base, "MEMORY.md")?)?;
        put(
            &mut router,
            "router-updated",
            get(evidence, "recorded_at").clone(),
        )?;
        routes.push(object([
            ("path", string(&path)),
            ("purpose", string("Evidenced durable project learning")),
            ("scope", get(&metadata, "scope").clone()),
            ("lifecycle", string("active")),
            (
                "read-when",
                string("When this learning and its conditions apply"),
            ),
        ]));
        routes.push(object([
            ("path", string(&history)),
            ("purpose", string("Admission provenance event")),
            ("scope", string("repository")),
            ("lifecycle", string("active")),
            ("read-when", string("When inspecting the admission history")),
        ]));
        let mut files = base.clone();
        put(
            &mut files,
            "MEMORY.md",
            string(contract::router_text(&router, &routes, &omitted)?),
        )?;
        files
    };
    put(
        &mut files,
        &path,
        string(render(&metadata)? + text(candidate, "body")?),
    )?;
    put(&mut files, &history, string(render(&history_metadata)?))?;
    Ok(files)
}
pub fn evidence_value(value: &Value) -> Result<&Value> {
    let rows = value
        .as_object()
        .ok_or_else(|| err("Admission needs exact capture, comparison and event evidence"))?;
    if EVIDENCE.iter().any(|k| value.get(k).is_none())
        || rows
            .iter()
            .any(|(k, _)| !EVIDENCE.contains(&k.as_str()) && k != "navigation")
    {
        return Err(err(
            "Admission needs exact capture, comparison and event evidence",
        ));
    }
    if let Some(navigation) = value.get("navigation") {
        admission_navigation::navigation_value(navigation)?;
        if get(value, "comparison_request").get("discovery").is_none() {
            return Err(err(
                "Supported semantic admission requires its completed source-bound discovery trace",
            ));
        }
    }
    if lifecycle::time_value(get(value, "recorded_at"))?
        > lifecycle::time_value(&string(host::utc_now_isoformat()?))?
    {
        return Err(err("Admission event time cannot be in the future"));
    }
    if !same(
        get(get(value, "comparison_request"), "candidate"),
        get(get(get(value, "capture"), "request"), "candidate"),
    )? {
        return Err(err(
            "Compare the exact captured candidate, not a substitute",
        ));
    }
    Ok(value)
}
pub fn preview(
    repo: &Path,
    captured: &Value,
    request: &Value,
    proof: &Value,
    navigation: Option<&Value>,
    operation_id: Option<&str>,
    recorded_at: Option<&str>,
) -> Result<Value> {
    let captured = capture_value(repo, captured, None)?;
    let proof = proof.get("proof").unwrap_or(proof);
    if !same(
        get(request, "candidate"),
        get(get(&captured, "request"), "candidate"),
    )? {
        return Err(err("Compare the exact captured candidate"));
    }
    let compared = comparison_provider()?.check(repo, request, proof, None)?;
    if get(&compared, "disposition").as_str() != Some("create-distinct") {
        let mut fields = object([
            ("disposition", get(&compared, "disposition").clone()),
            ("admission_verified", Value::Bool(false)),
            ("proposal", Value::Null),
            ("canonical_writes_performed", Value::Bool(false)),
            ("messages_sent", Value::Bool(false)),
        ]);
        for key in [
            "detecting_developer_id",
            "other_developer_ids",
            "recommend_other_developer_clarification",
        ] {
            put(&mut fields, key, get(&compared, key).clone())?;
        }
        return result("preview", &fields);
    }
    let (root, bank) = writes::bank_path(repo)?;
    let mut evidence = object([
        ("capture", captured),
        ("comparison_request", request.clone()),
        ("comparison_proof", proof.clone()),
        (
            "recorded_at",
            string(match recorded_at {
                Some(s) => s.to_owned(),
                None => host::utc_now_isoformat()?,
            }),
        ),
    ]);
    contract::merge(
        &mut evidence,
        &layout_resolution::evidence_fields((*proof != Value::Null).then_some(proof))?,
    )?;
    evidence_value(&evidence)?;
    if let Some(navigation) = navigation {
        put(&mut evidence, "navigation", navigation.clone())?;
        evidence_value(&evidence)?;
    }
    let operation_id = match operation_id {
        Some(id) => id.to_owned(),
        None => host::uuid4()?,
    };
    if !uuid_valid(&string(&operation_id)) {
        return Err(err(
            "Admission planning requires a canonical operation UUIDv4",
        ));
    }
    let base = tx::files_at(&bank)?;
    let mut proposal = get(
        &tx::preview(&root, &desired(&base, &evidence, &operation_id)?, "capture")?,
        "proposal",
    )
    .clone();
    put(&mut proposal, "operation_id", string(operation_id))?;
    put(&mut proposal, "admission_evidence", evidence)?;
    validate_transaction(&proposal, None)?;
    result(
        "preview",
        &object([
            ("proposal", proposal),
            ("admission_verified", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
            ("disposition", string("admit-distinct")),
            ("per_record_operator_approval_required", Value::Bool(false)),
        ]),
    )
}
pub fn validate_transaction(p: &Value, owner: Option<&[u8]>) -> Result<()> {
    let evidence = evidence_value(get(p, "admission_evidence"))?;
    if get(p, "operation").as_str() != Some("capture")
        || get(p, "project_id") != get(get(get(evidence, "capture"), "request"), "project_id")
    {
        return Err(err(
            "Admission evidence belongs only to its own project capture operation",
        ));
    }
    let repo = Path::new(text(p, "repo_root")?);
    capture_value(repo, get(evidence, "capture"), owner)?;
    let compared = comparison_provider()?.check(
        repo,
        get(evidence, "comparison_request"),
        get(evidence, "comparison_proof"),
        owner,
    )?;
    if get(&compared, "disposition").as_str() != Some("create-distinct") {
        return Err(err(
            "Existing learning needs consolidation or clarification before a new admission",
        ));
    }
    let (root, bank) = writes::bank_path(repo)?;
    let base = tx::files_at(&bank)?;
    if !same(&tx::file_hashes(&base)?, get(p, "observed_hashes"))?
        || !same(
            &desired(&base, evidence, text(p, "operation_id")?)?,
            get(p, "files"),
        )?
    {
        return Err(err(
            "Admission proposal changed canonical content, provenance or the observed bank",
        ));
    }
    writes::recovery_frontier_provider()?.require_scope_owned(
        &root,
        get(p, "project_id"),
        Some(get(p, "scope_context")),
        owner,
        None,
    )?;
    // Baseline layout_resolution.claims_for(p, owner=owned_lock): it validates
    // the retained unit claims and admission discards the returned values.
    layout_resolution::claims_for(p, owner, None, &crate::cli::runtime_root()?)?;
    Ok(())
}
pub fn check_sources(p: &Value, owner: Option<&[u8]>, marker: Option<&[u8]>) -> Result<()> {
    let evidence = evidence_value(get(p, "admission_evidence"))?;
    let captured = get(evidence, "capture");
    let root = Path::new(text(p, "repo_root")?);
    let frontier = writes::recovery_frontier_provider()?;
    frontier.require_scope_owned(
        root,
        get(p, "project_id"),
        Some(get(p, "scope_context")),
        owner,
        marker,
    )?;
    frontier.check(
        get(
            get(
                get(get(evidence, "comparison_proof"), "judgment"),
                "observation",
            ),
            "recovery_frontier",
        ),
        root,
        owner,
        marker,
    )?;
    let source = get(get(captured, "request"), "repository_evidence");
    if !same(
        &source_observation(root, source)?,
        get(captured, "source_observation"),
    )? {
        return Err(err(
            "Originating source/Git context changed; preserve the interrupted admission",
        ));
    }
    if source != &Value::Null {
        security::repository_source(root, source)?;
    }
    Ok(())
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = tx::proposal_value(prepared)?;
    if p.get("admission_evidence").is_none() {
        return Err(err("Use a complete governed admission proposal"));
    }
    let receipt = tx::apply(p)?;
    let mut fields = receipt.clone();
    remove(&mut fields, "schema");
    remove(&mut fields, "operation");
    put(
        &mut fields,
        "admission_verified",
        Value::Bool(
            get(&receipt, "verified") == &Value::Bool(true)
                && get(&receipt, "outcome").as_str() == Some("finished"),
        ),
    )?;
    put(
        &mut fields,
        "canonical_writes_performed",
        get(&receipt, "writes_performed").clone(),
    )?;
    result("apply", &fields)
}

fn private_error(message: &str) -> ContractError {
    ContractError::private_input(message)
}
fn private_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| private_error("Admission input is not in a trusted private workspace"))?
            .join(path)
    };
    // abspath normalizes parent components without resolving arbitrary links.
    let mut normalized = PathBuf::from("/");
    for part in path.components() {
        match part {
            Component::Normal(p) => normalized.push(p),
            Component::ParentDir => {
                normalized.pop();
            }
            _ => {}
        }
    }
    #[cfg(target_os = "macos")]
    for alias in ["tmp", "var", "etc"] {
        let prefix = PathBuf::from("/").join(alias);
        if normalized.starts_with(&prefix) {
            let chain = fs::DirectoryChain::open(Path::new("/")).map_err(|_| {
                private_error("Admission input is not in a trusted private workspace")
            })?;
            let info = chain
                .directory()
                .stat_at(std::ffi::OsStr::new(alias))
                .map_err(|_| private_error("Unexpected system path alias"))?;
            if info.is_symlink() {
                let expected = PathBuf::from("/private").join(alias);
                if info.uid != 0 || fs::canonical_path(&prefix, true)? != expected {
                    return Err(private_error("Unexpected system path alias"));
                }
                normalized = expected.join(
                    normalized
                        .strip_prefix(&prefix)
                        .map_err(|_| private_error("Unexpected system path alias"))?,
                );
            }
            break;
        }
    }
    Ok(normalized)
}
/// Protected input reader for the composite private_state hooks and CLI.
pub fn read_input(path: &Path, maximum: usize) -> Result<Value> {
    read_input_observed(path, maximum, &mut || Ok(()))
}
/// The callback allows deterministic real-filesystem race qualification without
/// replacing filesystem primitives. Production uses an empty callback.
pub fn read_input_observed(
    path: &Path,
    maximum: usize,
    before_read: &mut dyn FnMut() -> Result<()>,
) -> Result<Value> {
    let mut read = || -> Result<Vec<u8>> {
        let path = private_path(path)?;
        let parent = path
            .parent()
            .ok_or_else(|| private_error("Admission input requires a private parent"))?;
        trust_paths::state(parent)?;
        let chain = fs::DirectoryChain::open(parent)
            .map_err(|_| private_error("Admission input is not in a trusted private workspace"))?;
        let mut policy = Vec::new();
        let mut protected = false;
        let mut node = PathBuf::from("/");
        let mut parts = parent.components().filter_map(|c| {
            if let Component::Normal(p) = c {
                Some(p)
            } else {
                None
            }
        });
        for (index, descriptor) in chain.descriptors().iter().enumerate() {
            if index > 0 {
                node.push(
                    parts
                        .next()
                        .ok_or_else(|| private_error("Admission input directory changed"))?,
                );
            }
            let info = descriptor
                .stat()
                .map_err(|_| private_error("Admission input directory changed"))?;
            protected = trust_paths::inspect_entry(&node, &info, protected, false, false)?.0;
            policy.push(info);
        }
        let info = chain
            .directory()
            .stat()
            .map_err(|_| private_error("Admission input is not in a trusted private workspace"))?;
        if info.uid != host::effective_uid() || info.mode & 0o7777 != 0o700 {
            return Err(private_error("Admission input requires a private parent"));
        }
        let name = path
            .file_name()
            .ok_or_else(|| private_error("Admission input requires a file"))?;
        let pinned = fs::PinnedRead::open_with_options(
            parent,
            Path::new(name),
            maximum,
            fs::ReadOptions {
                require_single_link: true,
                ..Default::default()
            },
        )
        .map_err(|_| private_error("Admission input must be a protected regular file"))?;
        let file = pinned.identity();
        if file.uid != host::effective_uid() || file.mode & 0o022 != 0 {
            return Err(private_error(
                "Admission input must be a protected regular file",
            ));
        }
        before_read()?;
        let raw = pinned
            .read()
            .map_err(|error| match error {
                fs::ReadError::SizeLimit { .. } => {
                    err("Admission input exceeds its bounded reader")
                }
                _ => private_error("Admission input changed during read"),
            })?
            .bytes;
        chain
            .recheck()
            .map_err(|_| private_error("Admission input directory changed"))?;
        for (descriptor, expected) in chain.descriptors().iter().zip(policy) {
            if !descriptor
                .stat()
                .map_err(|_| private_error("Admission input directory changed"))?
                .same_binding(expected)
            {
                return Err(private_error("Admission input directory changed"));
            }
        }
        Ok(raw)
    };
    let raw = read().map_err(|error| {
        if matches!(error.kind(), Kind::TrustPath { .. }) {
            private_error("Admission input is not in a trusted private workspace")
        } else {
            error
        }
    })?;
    crate::developer_identity::load_with_limit(&raw, maximum)
}
pub struct Admission;
impl tx::TransactionValidator for Admission {
    fn validate_transaction(&self, p: &Value, owner: Option<&[u8]>) -> Result<()> {
        validate_transaction(p, owner)
    }
    fn check_sources(&self, p: &Value, owner: &[u8], marker: &[u8]) -> Result<()> {
        check_sources(p, Some(owner), Some(marker))
    }
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> lifecycle::CommandResult {
    match outcome {
        Ok(value) => lifecycle::CommandResult::json(value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let pending =
                operation == "apply" && !matches!(error.kind(), Kind::SourceTransportBusy { .. });
            let message = if matches!(error.kind(), Kind::BankCapacity { .. }) {
                error.to_string()
            } else if matches!(error.kind(), Kind::PrivateInput) {
                "Admission input must be a protected file in an owner-only directory. Use am private create and am private put; preserve the existing input.".into()
            } else {
                "Unsafe, ineligible, unavailable or changed admission; preserve sources and repeat evidence/comparison review".into()
            };
            let mut fields = object([
                ("admission_verified", Value::Bool(false)),
                (
                    "canonical_writes_performed",
                    if pending {
                        Value::Null
                    } else {
                        Value::Bool(false)
                    },
                ),
                ("recovery_may_be_required", Value::Bool(pending)),
                ("error", string(message)),
            ]);
            let value = contract::merge(&mut fields, &crate::bank_capacity::details(&error))
                .and_then(|_| result(operation, &fields));
            match value {
                Ok(value) => lifecycle::CommandResult::json(value, 1, json::DEFAULT),
                Err(error) => lifecycle::CommandResult {
                    value: None,
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("{error}\n"),
                },
            }
        }
    }
}
