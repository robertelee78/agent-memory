//! Evidence-bound lifecycle governance, with no age-based or implicit deletion.
use crate::bank_contract as contract;
use crate::bank_transactions as tx;
use crate::bank_writes as writes;
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{encoded, sha256_hex, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::lifecycle::{self, get, object, string};
use crate::{
    admission, bank_layout, comparison, host, inactive, layout_resolution, records, retrieval,
    security, tombstones,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Baseline `admission.read_input(args.request, 262144)` for plan/preview.
pub const REQUEST_INPUT_LIMIT: usize = 262_144;
/// Baseline `admission.read_input(..., 512*1024*1024)` for proposals and plans.
pub const PROPOSAL_INPUT_LIMIT: usize = 512 * 1024 * 1024;
/// Bounded canonical file and reviewed context edit size.
pub const FILE_LIMIT: usize = 16_384;
const AUTHORITY_LIMIT: usize = 16_384;
const TEXT_LIMIT: usize = 1024;
const EVIDENCE_LIMIT: usize = 64;
const DELETE_LIMIT: usize = 64;
const CLOSURE_LIMIT: usize = 64;

const REQUEST: &[&str] = &[
    "project_id",
    "operation",
    "target",
    "authority",
    "comparison",
    "capture",
    "replacement",
    "delete_ids",
    "context_review",
    "context_edits",
    "context",
];
const AUTHORITY: &[&str] = &[
    "source_class",
    "repository_evidence",
    "evidence",
    "reason",
    "confirmed",
    "strength",
    "basis",
    "impact",
    "review",
    "reason_class",
];
const TARGET: &[&str] = &["path", "id", "version", "sha256"];
const EVIDENCE: &[&str] = &[
    "request",
    "recorded_at",
    "git_context",
    "unit_resolution_claims",
    "comparison_proof_sha256",
    "reference_map_sha256",
];
pub const OPERATIONS: &[&str] = &[
    "correct",
    "supersede",
    "revoke",
    "expire",
    "retention",
    "delete",
    "reactivate",
];
const DEACTIVATION: &[&str] = &["revoke", "expire", "retention", "delete"];
const STRENGTHS: &[&str] = &["established", "ambiguous"];
const BASES: &[&str] = &[
    "stronger-evidence",
    "loss-of-applicability",
    "time-boundary",
    "policy",
    "explicit-deletion",
    "ambiguous",
];
const IMPACTS: &[&str] = &[
    "all-target-conditions",
    "narrower-contexts-remain",
    "unknown",
];
const CURRENT_GATES: &[&str] = &[
    "authorized-scope",
    "applicable",
    "active",
    "valid-time",
    "sufficient-trust",
];

/// Baseline `WRITE_ACTION`: the bank transaction operation of a governance operation.
pub fn write_action(operation: &str) -> Option<&'static str> {
    match operation {
        "correct" | "reactivate" => Some("correct"),
        "supersede" => Some("supersede"),
        "revoke" => Some("revoke"),
        "expire" | "retention" | "delete" => Some("forget"),
        _ => None,
    }
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn invalid() -> ContractError {
    err("Invalid governance evidence or canonical record")
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key).as_str().ok_or_else(invalid)
}
/// A Python subscript: a missing member is a refusal, never a silent null.
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(invalid)
}
fn array(value: &Value) -> Result<&[Value]> {
    value.as_array().ok_or_else(invalid)
}
fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|rows| {
        rows.len() == fields.len() && rows.iter().all(|(k, _)| fields.contains(&k.as_str()))
    })
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
    frontmatter::write(metadata.as_object().ok_or_else(invalid)?)
}
fn member(value: &Value, allowed: &[&str]) -> bool {
    value.as_str().is_some_and(|s| allowed.contains(&s))
}
/// Python truthiness of a decoded JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => n.as_str().trim_start_matches('-') != "0",
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
fn legal(transition: &Value) -> bool {
    get(transition, "legal") == &Value::Bool(true)
}
fn schema_enum<'a>(schema: &'a Value, property: &str) -> &'a [Value] {
    get(get(get(schema, "properties"), property), "enum")
        .as_array()
        .unwrap_or_default()
}
fn property_names(schema: &Value) -> Vec<&str> {
    get(schema, "properties")
        .as_object()
        .unwrap_or_default()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect()
}
/// Python `' '.join(text.split())` compared as its whitespace-separated words.
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .filter(|word| !word.is_empty())
        .collect()
}
fn before_routes(body: &str) -> &str {
    body.split_once("\n## Routes\n")
        .map_or(body, |(prefix, _)| prefix)
}

pub fn result(operation: &str, fields: &Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/governance-result-v1")),
        ("operation", string(operation)),
        ("semantic_truth_verified", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
        ("main_freshness_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("messages_sent", Value::Bool(false)),
    ]);
    contract::merge(&mut value, fields)?;
    Ok(value)
}

pub fn reference(metadata: &Value) -> Result<Value> {
    Ok(object([
        ("id", required(metadata, "id")?.clone()),
        ("version", required(metadata, "version")?.clone()),
    ]))
}

/// Order-preserving de-duplication by canonical encoding; a later equal value
/// replaces the earlier one at its first position.
pub fn union(values: Vec<Value>) -> Result<Value> {
    let mut keys: Vec<Vec<u8>> = Vec::new();
    let mut unique: Vec<Value> = Vec::new();
    for value in values {
        let key = encoded(&value)?;
        if let Some(index) = keys.iter().position(|k| k == &key) {
            if let Some(slot) = unique.get_mut(index) {
                *slot = value;
            }
        } else {
            keys.push(key);
            unique.push(value);
        }
    }
    Ok(Value::Array(unique))
}

pub fn target_value(value: &Value) -> Result<()> {
    let positive = get(value, "version").as_int().is_some_and(|n| {
        let digits = n.as_str();
        !digits.starts_with('-') && digits != "0"
    });
    let sha = get(value, "sha256").as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    });
    if !exact(value, TARGET) || !uuid_valid(get(value, "id")) || !positive || !sha {
        return Err(err(
            "Governance requires an exact observed record path, identity, version and hash",
        ));
    }
    contract::path_value(get(value, "path"))?;
    Ok(())
}

fn request_shape(value: &Value, comparison_member: &str) -> Result<()> {
    let Some(operation) = get(value, "operation")
        .as_str()
        .filter(|o| OPERATIONS.contains(o))
    else {
        return Err(err(
            "Governance needs one explicit project operation and complete evidence",
        ));
    };
    if !exact(value, REQUEST) || !uuid_valid(get(value, "project_id")) {
        return Err(err(
            "Governance needs one explicit project operation and complete evidence",
        ));
    }
    target_value(get(value, "target"))?;
    let a = get(value, "authority");
    if !exact(a, AUTHORITY)
        || !member(get(a, "source_class"), security::CLASSES)
        || get(a, "confirmed").as_bool().is_none()
        || !member(get(a, "strength"), STRENGTHS)
        || !member(get(a, "basis"), BASES)
        || !member(get(a, "impact"), IMPACTS)
        || !matches!(get(a, "review"), Value::Null | Value::String(_))
    {
        return Err(err(
            "Assess actual source, strength, applicable impact and confirmation explicitly",
        ));
    }
    comparison::safe_text(get(a, "reason"), TEXT_LIMIT)?;
    if get(a, "review") != &Value::Null {
        comparison::safe_text(get(a, "review"), TEXT_LIMIT)?;
    }
    records::evidence(get(a, "evidence"))?;
    if array(get(a, "evidence"))?.len() > EVIDENCE_LIMIT || encoded(a)?.len() > AUTHORITY_LIMIT {
        return Err(err(
            "Governance authority is bounded evidence, not a transcript",
        ));
    }
    let reason_class = get(a, "reason_class");
    if (operation == "delete"
        && !schema_enum(&records::TOMBSTONE, "reason-class").contains(reason_class))
        || (operation != "delete" && reason_class != &Value::Null)
    {
        return Err(err("Deletion reason class must remain content free"));
    }
    let delete_ids = get(value, "delete_ids").as_array();
    let unique = delete_ids.is_some_and(|ids| {
        let set: BTreeSet<_> = ids.iter().filter_map(Value::as_str).collect();
        set.len() == ids.len()
    });
    if !delete_ids.is_some_and(|ids| ids.len() <= DELETE_LIMIT && ids.iter().all(uuid_valid))
        || !unique
    {
        return Err(err(
            "Deletion names an explicit bounded set of authorized stable identities",
        ));
    }
    if operation != "delete" && delete_ids.is_some_and(|ids| !ids.is_empty()) {
        return Err(err(
            "Only explicit deletion can authorize terminal identities",
        ));
    }
    if DEACTIVATION.contains(&operation)
        && ["capture", "replacement"]
            .iter()
            .any(|key| get(value, key) != &Value::Null)
    {
        return Err(err(
            "Deactivation and deletion cannot smuggle replacement or admission evidence",
        ));
    }
    if ["correct", "reactivate"].contains(&operation)
        && (get(value, "capture") == &Value::Null
            || get(value, "comparison") == &Value::Null
            || get(value, "replacement") != &Value::Null)
    {
        return Err(err(
            "A new corrected version requires qualified capture and exact comparison",
        ));
    }
    if operation == "supersede" {
        if get(value, "capture") != &Value::Null || get(value, "comparison") == &Value::Null {
            return Err(err("Supersession reviews an existing replacement"));
        }
        target_value(get(value, "replacement"))?;
    }
    let c = get(value, "comparison");
    if c != &Value::Null {
        if !exact(c, &["request", comparison_member]) {
            return Err(ContractError::new(format!(
                "Supply the exact comparison request and {comparison_member}"
            )));
        }
        comparison::request_value(get(c, "request"))?;
    }
    let (Some(review), Some(edits)) = (
        get(value, "context_review").as_object(),
        get(value, "context_edits").as_object(),
    ) else {
        return Err(err(
            "Review ordinary context for copied guidance before invalidating its source",
        ));
    };
    if edits
        .iter()
        .any(|(path, _)| !review.iter().any(|(reviewed, _)| reviewed == path))
    {
        return Err(err(
            "Context changes must be among the actual reviewed canonical files",
        ));
    }
    for (path, text) in edits {
        if !contract::CONTEXTS.contains(&path.as_str())
            || !text.as_str().is_some_and(|t| t.len() <= FILE_LIMIT)
        {
            return Err(err(
                "Only bounded reviewed ordinary context may accompany governance",
            ));
        }
    }
    let recall = object([
        ("project_id", get(value, "project_id").clone()),
        ("query", string("")),
        ("ids", Value::Array(vec![])),
        ("paths", Value::Array(vec![])),
        ("context", get(value, "context").clone()),
        ("kinds", Value::Array(vec![])),
        ("minimum_trust", string("reviewed")),
        ("limit", Value::Int(1.into())),
        ("include_context", Value::Bool(false)),
    ]);
    retrieval::request_value(&recall)?;
    Ok(())
}

/// Baseline `request_value`: an exact request whose comparison carries a proof.
pub fn request_value(value: &Value) -> Result<()> {
    request_shape(value, "proof")
}

/// The current governed observation (baseline `observe` dictionary).
#[derive(Clone, Debug)]
pub struct Observation {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub base: Value,
    pub checked: Value,
    pub observed: Value,
    pub scope: Value,
    pub metadata: Value,
    pub body: String,
    pub context: Value,
    pub git_context: Value,
    /// Qualified current capture, or null.
    pub capture: Value,
    /// Revalidated comparison result, or null.
    pub comparison: Value,
    /// `{metadata, body}` of the existing replacement, or null.
    pub replacement: Value,
}

fn observe_current(repo: &Path, request: &Value, owner: Option<&[u8]>) -> Result<Observation> {
    let authority = get(request, "authority");
    let (root, bank) = writes::bank_path(repo)?;
    let scope = writes::recovery_frontier_provider()?.require_scope_owned(
        &root,
        get(request, "project_id"),
        None,
        owner,
        None,
    )?;
    let (checked, observed) = writes::snapshot(&root, owner)?;
    let base = tx::files_at(&bank)?;
    if !same(&tx::file_hashes(&base)?, &observed)? {
        return Err(err("Bank changed during governance review"));
    }
    let mut expected = Vec::new();
    for path in std::iter::once("MEMORY.md").chain(contract::CONTEXTS.iter().copied()) {
        if let Some(hash) = observed.get(path) {
            expected.push((path, hash.clone()));
        }
    }
    if !same(get(request, "context_review"), &object(expected))? {
        return Err(err(
            "Review the router and every actual ordinary context against its current hash before governance",
        ));
    }
    let target = get(request, "target");
    let path = text(target, "path")?;
    if observed.get(path) != Some(get(target, "sha256")) {
        return Err(err(
            "Governance target changed; read its current meaning and evidence again",
        ));
    }
    let (kind, metadata, body) = records::parse(text(&base, path)?, &[])?;
    if kind != "standalone-record"
        || get(&metadata, "id") != get(target, "id")
        || get(&metadata, "version") != get(target, "version")
        || get(&metadata, "project-id") != get(request, "project_id")
    {
        return Err(err(
            "Governance targets the exact same-project standalone version",
        ));
    }
    let source = get(authority, "repository_evidence");
    if source != &Value::Null {
        security::repository_source(&root, source)?;
        if !array(get(authority, "evidence"))?.contains(get(source, "path")) {
            return Err(err(
                "Retain the observed repository source in governance evidence",
            ));
        }
    }
    let (context, git_context) = retrieval::observed_context(&root, get(request, "context"))?;
    let capture = match get(request, "capture") {
        Value::Null => Value::Null,
        captured => admission::capture_value(&root, captured, owner)?,
    };
    Ok(Observation {
        root,
        bank,
        base,
        checked,
        observed,
        scope,
        metadata,
        body,
        context,
        git_context,
        capture,
        comparison: Value::Null,
        replacement: Value::Null,
    })
}

fn qualify_observation(
    mut observation: Observation,
    request: &Value,
    compared: Value,
    owner: Option<&[u8]>,
) -> Result<Observation> {
    let operation = text(request, "operation")?;
    let authority = get(request, "authority");
    let c = get(request, "comparison");
    let unchanged = object([
        ("metadata", observation.metadata.clone()),
        ("body", string(&observation.body)),
    ]);
    if c != &Value::Null {
        if get(get(c, "request"), "project_id") != get(request, "project_id") {
            return Err(err("Review sources from this project only"));
        }
        if DEACTIVATION.contains(&operation) {
            let frontier = get(
                get(get(get(&compared, "proof"), "judgment"), "observation"),
                "recovery_frontier",
            );
            if !truthy(get(frontier, "unit_ids"))
                || !same(get(get(c, "request"), "candidate"), &unchanged)?
            {
                return Err(err(
                    "Deactivation comparison only reviews pending sources against the exact unchanged target",
                ));
            }
        }
    }
    let captured = &observation.capture;
    if captured != &Value::Null {
        let captured_request = get(captured, "request");
        let candidate = get(captured_request, "candidate");
        if get(get(candidate, "metadata"), "id") != get(&observation.metadata, "id")
            || get(captured_request, "project_id") != get(request, "project_id")
            || !same(candidate, get(get(c, "request"), "candidate"))?
            || get(authority, "source_class") != get(captured_request, "source_class")
            || !same(
                get(authority, "repository_evidence"),
                get(captured_request, "repository_evidence"),
            )?
        {
            return Err(err(
                "Correct the stable identity using exactly its qualified actual source capture",
            ));
        }
    }
    if operation == "supersede" {
        let replacement_ref = get(request, "replacement");
        let path = text(replacement_ref, "path")?;
        if observation.observed.get(path) != Some(get(replacement_ref, "sha256")) {
            return Err(err("The existing replacement changed"));
        }
        let (kind, metadata, body) = records::parse(text(&observation.base, path)?, &[])?;
        if kind != "standalone-record"
            || get(&metadata, "id") != get(replacement_ref, "id")
            || get(&metadata, "version") != get(replacement_ref, "version")
            || get(&metadata, "id") == get(&observation.metadata, "id")
        {
            return Err(err(
                "Use a distinct exact existing replacement; same-ID changes use correction",
            ));
        }
        let replacement = object([("metadata", metadata), ("body", string(body))]);
        if !same(get(get(c, "request"), "candidate"), &replacement)? {
            return Err(err(
                "Review the complete unchanged existing replacement before linking it",
            ));
        }
        if !array(get(get(&compared, "proof"), "targets"))?
            .iter()
            .any(|row| get(row, "path").as_str() == Some(path))
        {
            return Err(err(
                "The exact existing replacement must be reviewed as equivalent to itself",
            ));
        }
        let recall = object([
            ("project_id", get(request, "project_id").clone()),
            ("query", string("")),
            ("ids", Value::Array(vec![])),
            ("paths", Value::Array(vec![string(path)])),
            ("context", get(request, "context").clone()),
            (
                "kinds",
                Value::Array(schema_enum(&records::MODEL, "kind").to_vec()),
            ),
            ("minimum_trust", string("reviewed")),
            ("limit", Value::Int(1.into())),
            ("include_context", Value::Bool(false)),
        ]);
        let recalled = retrieval::retrieve(&observation.root, &recall, owner, &[])?;
        let results = array(get(&recalled, "results"))?;
        let observation_sha = sha256_hex(&encoded(&object([
            ("project_id", get(request, "project_id").clone()),
            ("sources", observation.observed.clone()),
        ]))?);
        if results.len() != 1
            || results
                .iter()
                .any(|row| get(row, "path").as_str() != Some(path))
            || results
                .first()
                .is_none_or(|row| get(row, "source_sha256") != get(replacement_ref, "sha256"))
            || get(&recalled, "bank_observation_sha256").as_str() != Some(observation_sha.as_str())
        {
            return Err(err(
                "Supersession needs a currently qualified canonical replacement",
            ));
        }
        observation.replacement = replacement;
    }
    observation.comparison = compared;
    Ok(observation)
}

pub fn observe(repo: &Path, request: &Value, owner: Option<&[u8]>) -> Result<Observation> {
    request_value(request)?;
    let observation = observe_current(repo, request, owner)?;
    let c = get(request, "comparison");
    let compared = if c == &Value::Null {
        Value::Null
    } else {
        comparison::check(&observation.root, get(c, "request"), get(c, "proof"), owner)?
    };
    qualify_observation(observation, request, compared, owner)
}

/// A pending preview; `deferred` names its disposition first, then any overrides.
pub fn pending(observation: &Observation, deferred: &Value) -> Result<Value> {
    let peer = get(get(&observation.metadata, "origin"), "developer-id");
    let developer = get(&observation.scope, "developer_id");
    let mut defaults = object([
        ("disposition", get(deferred, "disposition").clone()),
        ("proposal", Value::Null),
        ("canonical_writes_performed", Value::Bool(false)),
        ("detecting_developer_id", developer.clone()),
        (
            "other_developer_ids",
            Value::Array(if peer != developer {
                vec![peer.clone()]
            } else {
                vec![]
            }),
        ),
        (
            "recommend_other_developer_clarification",
            Value::Bool(peer != developer),
        ),
    ]);
    contract::merge(&mut defaults, deferred)?;
    result("preview", &defaults)
}
fn deferral(disposition: &str) -> Value {
    object([("disposition", string(disposition))])
}

pub fn narrower_conditions(old: &Value, new: &Value) -> bool {
    let (Some(previous), Some(current)) = (
        get(old, "conditions").as_object(),
        get(new, "conditions").as_object(),
    ) else {
        return false;
    };
    let preserved = previous.iter().all(|(key, value)| {
        get(get(new, "conditions"), key) == value && current.iter().any(|(k, _)| k == key)
    });
    ["kind", "scope", "applicability"]
        .iter()
        .all(|key| old.get(key) == new.get(key))
        && preserved
        && current.len() > previous.len()
        && current.iter().all(|(_, value)| value != &Value::Null)
}

pub fn permitted_impact(observation: &Observation, request: &Value) -> bool {
    let impact = get(get(request, "authority"), "impact").as_str();
    if impact == Some("all-target-conditions") {
        return true;
    }
    let candidate = get(get(&observation.capture, "request"), "candidate");
    get(request, "operation").as_str() == Some("correct")
        && impact == Some("narrower-contexts-remain")
        && narrower_conditions(&observation.metadata, get(candidate, "metadata"))
        && get(candidate, "body").as_str() == Some(observation.body.as_str())
}

/// Whether a retained file still carries the (stripped) deleted body.
pub fn copied_body(body: &str, text: &str) -> bool {
    let target = contract::trim(body);
    !target.is_empty()
        && if target.contains('\n') {
            text.contains(target)
        } else {
            crate::pack_validation::splitlines(text)
                .iter()
                .any(|line| contract::trim(line) == target)
        }
}

fn review_deferral(observation: &Observation, request: &Value) -> Result<Option<Value>> {
    let compared = &observation.comparison;
    if compared == &Value::Null {
        return Ok(None);
    }
    let matches = array(get(compared, "matches"))?;
    let row = |path: &str| {
        matches
            .iter()
            .rev()
            .find(|row| get(row, "path").as_str() == Some(path))
    };
    let reviewed = row(text(get(request, "target"), "path")?);
    if !reviewed
        .is_some_and(|row| same(get(row, "metadata"), &observation.metadata).unwrap_or(false))
    {
        return Err(err("Read the exact existing target during semantic review"));
    }
    let proof = get(compared, "proof");
    if array(get(get(proof, "judgment"), "decisions"))?
        .iter()
        .any(|d| get(d, "classification").as_str() == Some("uncertain"))
    {
        return Ok(Some(deferral("clarification-required")));
    }
    let identity = get(&observation.metadata, "id");
    for path in array(get(compared, "disputed_paths"))? {
        let disputed = row(path.as_str().ok_or_else(invalid)?).ok_or_else(invalid)?;
        let metadata = get(disputed, "metadata");
        if metadata == &Value::Null || get(metadata, "id") != identity {
            return Ok(Some(object([
                ("disposition", string("clarification-required")),
                (
                    "other_developer_ids",
                    required(compared, "other_developer_ids")?.clone(),
                ),
                (
                    "recommend_other_developer_clarification",
                    required(compared, "recommend_other_developer_clarification")?.clone(),
                ),
            ])));
        }
    }
    if ["correct", "reactivate"].contains(&text(request, "operation")?)
        && array(get(proof, "targets"))?
            .iter()
            .any(|target| get(target, "id") != identity)
    {
        return Ok(Some(deferral("consolidation-required")));
    }
    Ok(None)
}

fn next_version(observation: &Observation, identity: &Value) -> Result<Value> {
    let mut maximum: Option<i128> = None;
    for entry in array(get(&observation.checked, "entries"))? {
        if get(entry, "format").as_str() == Some("standalone-record")
            && get(entry, "id") == identity
        {
            let version = get(entry, "version")
                .as_int()
                .and_then(|n| n.as_i128())
                .ok_or_else(invalid)?;
            maximum = Some(maximum.map_or(version, |m| m.max(version)));
        }
    }
    let next = maximum.and_then(|m| m.checked_add(1)).ok_or_else(invalid)?;
    Ok(Value::Int(next.into()))
}

pub fn route(path: &str, purpose: &str, state: &str) -> Value {
    object([
        ("path", string(path)),
        ("purpose", string(purpose)),
        ("scope", string("repository")),
        ("lifecycle", string(state)),
        (
            "read-when",
            string("Before using or rebuilding affected learning"),
        ),
    ])
}

/// The canonical delta being built by `desired`.
struct Delta {
    files: Value,
    removed: BTreeSet<String>,
    changed: BTreeMap<String, String>,
    additions: Vec<Value>,
    /// Bodies of removed standalone records (deletion only).
    deleted_bodies: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn archive(
    delta: &mut Delta,
    metadata: &Value,
    body: &str,
    replacement: &Value,
    at: &str,
    original_path: &str,
    reason: &str,
    reactivating: bool,
) -> Result<()> {
    let path = format!(
        "history/versions/{}/v{}.md",
        text(metadata, "id")?,
        required(metadata, "version")?
            .as_int()
            .ok_or_else(invalid)?
            .as_str()
    );
    if delta.files.get(&path).is_some() {
        return Err(err("Do not overwrite an earlier exact source version"));
    }
    let mut old = metadata.clone();
    let base_note = format!(". Original bank-relative link base: {original_path}");
    if !reactivating {
        contract::merge(
            &mut old,
            &object([
                ("status", string("superseded")),
                ("updated", string(at)),
                ("superseded-by", Value::Array(vec![reference(replacement)?])),
                ("state-reason", string(format!("{reason}{base_note}"))),
            ]),
        )?;
    } else {
        let previous = text(&old, "state-reason")?.to_owned();
        put(&mut old, "state-reason", string(previous + &base_note))?;
    }
    put(&mut delta.files, &path, string(render(&old)? + body))?;
    delta.additions.push(route(
        &path,
        "Historical version and original source",
        text(&old, "status")?,
    ));
    Ok(())
}

pub fn qualified_authority(request: &Value) -> bool {
    let a = get(request, "authority");
    if get(a, "confirmed") != &Value::Bool(true)
        || get(a, "strength").as_str() != Some("established")
        || get(a, "basis").as_str() == Some("ambiguous")
    {
        return false;
    }
    if get(request, "operation").as_str() == Some("expire") {
        return get(a, "basis").as_str() == Some("time-boundary");
    }
    if get(a, "source_class").as_str() == Some("user") {
        return true;
    }
    get(a, "repository_evidence") != &Value::Null
        && (get(a, "source_class").as_str() == Some("verified-repository")
            || get(a, "review") != &Value::Null)
}

fn rows_after(
    rows: &[Value],
    changed: &BTreeMap<String, String>,
    removed: &BTreeSet<String>,
) -> Result<Vec<Value>> {
    let mut kept = Vec::new();
    for row in rows {
        let path = text(row, "path")?;
        if removed.contains(path) {
            continue;
        }
        let mut row = row.clone();
        if let Some(state) = changed.get(path) {
            put(&mut row, "lifecycle", string(state))?;
        }
        kept.push(row);
    }
    Ok(kept)
}

/// Keep introductions, remove empty index chains, and never orphan a child.
fn reroute(
    base: &Value,
    checked: &Value,
    changed: &BTreeMap<String, String>,
    removed: &mut BTreeSet<String>,
    additions: &[Value],
    at: &str,
) -> Result<Value> {
    let mut files = Value::Object(
        base.as_object()
            .ok_or_else(invalid)?
            .iter()
            .filter(|(path, _)| !removed.contains(path))
            .cloned()
            .collect(),
    );
    let mut indices: Vec<&str> = array(get(checked, "entries"))?
        .iter()
        .filter(|e| get(e, "format").as_str() == Some("topic-index"))
        .map(|e| text(e, "path"))
        .collect::<Result<_>>()?;
    indices.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
    let index_fields = property_names(&bank_layout::SPEC);
    for path in indices {
        let (header, body) = frontmatter::read(text(base, path)?, &index_fields)?;
        let mut header = Value::Object(header);
        let rows = bank_layout::parse_index(path, &header, body)?;
        let updated = rows_after(&rows, changed, removed)?;
        if updated.is_empty() {
            let generated = bank_layout::index_text(path, &header, &rows)?;
            let generated = generated.split_once("\n---\n").ok_or_else(invalid)?.1;
            if words(before_routes(body)) != words(before_routes(generated)) {
                return Err(err(
                    "The empty index contains custom context; preserve it before pruning its last route",
                ));
            }
            removed.insert(path.to_owned());
            remove(&mut files, path);
        } else if rows != updated {
            put(&mut header, "updated", string(at))?;
            let content = render(&header)?
                + before_routes(body)
                + "\n## Routes\n\n"
                + &contract::route_table(&updated, path)?;
            put(&mut files, path, string(content))?;
        }
    }
    let router = text(base, "MEMORY.md")?;
    let (mut metadata, rows, _) = contract::parse_router(router)?;
    let (_, body) = frontmatter::read(router, &property_names(&contract::SPEC))?;
    let (prefix, rest) = body.split_once("\n## Routes\n").ok_or_else(invalid)?;
    let (_, omissions) = rest
        .split_once("\n## Omitted contexts\n")
        .ok_or_else(invalid)?;
    put(&mut metadata, "router-updated", string(at))?;
    let mut routes = rows_after(&rows, changed, removed)?;
    routes.extend(additions.iter().cloned());
    let content = render(&metadata)?
        + prefix
        + "\n## Routes\n\n"
        + &contract::route_table(&routes, "MEMORY.md")?
        + "\n## Omitted contexts\n"
        + omissions;
    put(&mut files, "MEMORY.md", string(content))?;
    Ok(files)
}

/// Standalone records of the observed bank: `(path, metadata, body)`.
type Record = (String, Value, String);

struct Closure {
    identities: BTreeSet<String>,
    items: Vec<Record>,
    unresolved: Vec<String>,
}

fn reference_ids<'a>(metadata: &'a Value, key: &str) -> Result<Vec<&'a str>> {
    let Some(references) = metadata.get(key) else {
        return Ok(Vec::new());
    };
    array(references)?.iter().map(|r| text(r, "id")).collect()
}

fn delete_closure(observation: &Observation, request: &Value) -> Result<Closure> {
    let mut identities: BTreeSet<String> = BTreeSet::new();
    identities.insert(text(&observation.metadata, "id")?.to_owned());
    for identity in array(get(request, "delete_ids"))? {
        identities.insert(identity.as_str().ok_or_else(invalid)?.to_owned());
    }
    let entries = array(get(&observation.checked, "entries"))?;
    let mut items: Vec<Record> = Vec::new();
    for entry in entries {
        if get(entry, "format").as_str() == Some("standalone-record") {
            let path = text(entry, "path")?;
            let (_, metadata, body) = records::parse(text(&observation.base, path)?, &[])?;
            items.push((path.to_owned(), metadata, body));
        }
    }
    let mut sources: Vec<(String, String, Option<String>)> = Vec::new();
    for (path, metadata, _) in &items {
        if path.starts_with("history/sources/") {
            sources.push((
                path.clone(),
                text(metadata, "id")?.to_owned(),
                records::source_capture_id(path, metadata),
            ));
        }
    }
    let source_units: BTreeSet<&str> = sources.iter().map(|(_, id, _)| id.as_str()).collect();
    let mut known: BTreeSet<String> = BTreeSet::new();
    for (_, metadata, _) in &items {
        known.insert(text(metadata, "id")?.to_owned());
    }
    for (_, _, alias) in &sources {
        known.extend(alias.clone());
    }
    for entry in entries {
        if get(entry, "format").as_str() == Some("tombstone") {
            known.insert(text(entry, "id")?.to_owned());
        }
    }
    if !identities.is_subset(&known) {
        return Err(err(
            "Deletion scope must name actual same-project canonical identities",
        ));
    }
    loop {
        let mut expanded = identities.clone();
        let mut references: BTreeSet<&str> = BTreeSet::new();
        for (_, metadata, _) in &items {
            let id = text(metadata, "id")?;
            let derived = reference_ids(metadata, "derived-from")?;
            if derived.iter().any(|r| identities.contains(*r)) {
                expanded.insert(id.to_owned());
            }
            if identities.contains(id) {
                references.extend(derived);
            }
        }
        for (_, metadata, _) in &items {
            let id = text(metadata, "id")?;
            if source_units.contains(id)
                && references.contains(id)
                && reference_ids(metadata, "superseded-by")?
                    .iter()
                    .any(|r| identities.contains(*r))
            {
                expanded.insert(id.to_owned());
            }
        }
        for (_, id, alias) in &sources {
            if let Some(alias) = alias {
                if expanded.contains(alias) {
                    expanded.insert(id.clone());
                }
                if expanded.contains(id) {
                    expanded.insert(alias.clone());
                }
            }
        }
        if expanded.len() > CLOSURE_LIMIT {
            return Err(err(
                "Deletion derivation closure requires an explicitly scoped bounded review",
            ));
        }
        if expanded == identities {
            break;
        }
        identities = expanded;
    }
    let unresolved = sources
        .iter()
        .filter(|(_, id, alias)| identities.contains(id) && alias.is_none())
        .map(|(path, _, _)| path.clone())
        .collect();
    Ok(Closure {
        identities,
        items,
        unresolved,
    })
}

fn developer(metadata: &Value) -> Result<&str> {
    text(get(metadata, "origin"), "developer-id")
}

fn peers<'a>(items: impl Iterator<Item = &'a Record>, actor: &str) -> Result<Vec<Value>> {
    let mut found = BTreeSet::new();
    for (_, metadata, _) in items {
        let id = developer(metadata)?;
        if id != actor {
            found.insert(id.to_owned());
        }
    }
    Ok(found.into_iter().map(string).collect())
}

fn history_event(
    delta: &mut Delta,
    observation: &Observation,
    request: &Value,
    identity: &str,
    kind: &str,
    record: &Value,
    evidence: &Value,
    at: &str,
    purpose: &str,
) -> Result<()> {
    let path = format!("history/events/{identity}.md");
    if observation.base.get(&path).is_some() {
        return Err(err("Do not overwrite another lifecycle event"));
    }
    let metadata = object([
        ("schema", string("agent-memory/history-event-v1")),
        ("id", string(identity)),
        ("project-id", get(request, "project_id").clone()),
        ("kind", string(kind)),
        ("record", reference(record)?),
        ("actor", get(&observation.scope, "developer_id").clone()),
        ("recorded-at", string(at)),
        ("evidence", evidence.clone()),
    ]);
    records::event(&metadata)?;
    put(&mut delta.files, &path, string(render(&metadata)?))?;
    delta.additions.push(route(&path, purpose, "active"));
    Ok(())
}

fn delete_delta(
    delta: &mut Delta,
    observation: &Observation,
    request: &Value,
    event: &Value,
    at: &str,
) -> Result<Option<Value>> {
    let a = get(request, "authority");
    let actor = text(&observation.scope, "developer_id")?;
    if get(a, "source_class").as_str() != Some("user")
        || get(a, "basis").as_str() != Some("explicit-deletion")
    {
        return Err(err(
            "Terminal deletion requires explicit operator authority, not automatic pruning",
        ));
    }
    let closure = delete_closure(observation, request)?;
    let identities = &closure.identities;
    if !closure.unresolved.is_empty() {
        let peers = peers(
            closure
                .items
                .iter()
                .filter(|(path, _, _)| closure.unresolved.contains(path)),
            actor,
        )?;
        let mut paths = closure.unresolved.clone();
        paths.sort();
        let recommend = Value::Bool(!peers.is_empty());
        return Ok(Some(object([
            ("disposition", string("provenance-identity-review-required")),
            (
                "source_paths",
                Value::Array(paths.into_iter().map(string).collect()),
            ),
            ("other_developer_ids", Value::Array(peers)),
            ("recommend_other_developer_clarification", recommend),
        ])));
    }
    let requested: BTreeSet<String> = array(get(request, "delete_ids"))?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if &requested != identities {
        let mut extra = Vec::new();
        for item in &closure.items {
            let id = text(&item.1, "id")?;
            if identities.contains(id) && !requested.contains(id) {
                extra.push(item);
            }
        }
        let peers = peers(extra.into_iter(), actor)?;
        let recommend = Value::Bool(!peers.is_empty());
        return Ok(Some(object([
            ("disposition", string("deletion-scope-review-required")),
            (
                "required_delete_ids",
                Value::Array(identities.iter().map(string).collect()),
            ),
            ("other_developer_ids", Value::Array(peers)),
            ("recommend_other_developer_clarification", recommend),
        ])));
    }
    for (path, metadata, body) in &closure.items {
        if !identities.contains(text(metadata, "id")?) {
            continue;
        }
        if !legal(&lifecycle::transition(metadata, &string("delete"), event)?) {
            return Err(err(
                "Terminal deletion must use the common lifecycle guards",
            ));
        }
        delta.removed.insert(path.clone());
        delta.deleted_bodies.push(body.clone());
    }
    for (path, metadata, body) in &closure.items {
        if identities.contains(text(metadata, "id")?) {
            continue;
        }
        // The tombstone now owns terminal invalidation. Remove redundant
        // pointers to erased guidance without deleting an independently
        // evidenced correction/replacement or copying the erased content.
        let previous = array(required(metadata, "supersedes")?)?;
        let mut kept = Vec::new();
        for r in previous {
            if !identities.contains(text(r, "id")?) {
                kept.push(r.clone());
            }
        }
        if kept.len() != previous.len() {
            let mut updated = metadata.clone();
            put(&mut updated, "supersedes", Value::Array(kept))?;
            put(&mut delta.files, path, string(render(&updated)? + body))?;
        }
    }
    for entry in array(get(&observation.checked, "entries"))? {
        if get(entry, "format").as_str() != Some("optional-history-event") {
            continue;
        }
        let path = text(entry, "path")?;
        let (_, metadata, _) = records::parse(text(&observation.base, path)?, &[])?;
        if identities.contains(text(get(&metadata, "record"), "id")?) {
            delta.removed.insert(path.to_owned());
        }
    }
    for identity in identities {
        let path = tombstones::relative_path(identity);
        if let Some(existing) = observation.base.get(&path) {
            let marker = tombstones::parse(existing.as_str().ok_or_else(invalid)?)?;
            if get(&marker, "id").as_str() != Some(identity.as_str())
                || get(&marker, "scope") != get(request, "project_id")
            {
                return Err(err("Never overwrite an unknown terminal marker"));
            }
            continue;
        }
        let metadata = object([
            ("id", string(identity)),
            ("scope", get(request, "project_id").clone()),
            ("deleted-at", string(at)),
            ("deletion-authority", string(actor)),
            ("reason-class", get(a, "reason_class").clone()),
        ]);
        let rendered = tombstones::render(&metadata)?;
        put(
            &mut delta.files,
            &path,
            required(&rendered, "content")?.clone(),
        )?;
        delta
            .additions
            .push(route(&path, "Terminal deletion marker", "deleted"));
    }
    let edits = get(request, "context_edits")
        .as_object()
        .unwrap_or_default();
    for body in &delta.deleted_bodies {
        if edits
            .iter()
            .any(|(_, text)| text.as_str().is_some_and(|t| copied_body(body, t)))
        {
            return Err(err(
                "Deleted learning cannot be copied into retained context",
            ));
        }
    }
    Ok(None)
}

fn deactivate_delta(
    delta: &mut Delta,
    observation: &Observation,
    request: &Value,
    operation_id: &str,
    event: &Value,
    at: &str,
) -> Result<()> {
    let a = get(request, "authority");
    let operation = text(request, "operation")?;
    let basis = get(a, "basis").as_str();
    if operation == "expire" && basis != Some("time-boundary") {
        return Err(err("Expiry uses an observed valid-time boundary"));
    }
    if operation == "retention" && basis != Some("policy") {
        return Err(err("Retention requires its actual policy, never age alone"));
    }
    if operation == "revoke"
        && !basis.is_some_and(|b| ["stronger-evidence", "loss-of-applicability"].contains(&b))
    {
        return Err(err(
            "Automatic pruning requires stronger correction or actual loss of applicability",
        ));
    }
    let old = &observation.metadata;
    let action = if operation == "expire" {
        "expire"
    } else {
        "revoke"
    };
    let transition = lifecycle::transition(old, &string(action), event)?;
    if !legal(&transition) {
        return Err(err(
            "The explicit forgetting operation does not satisfy the common lifecycle guards",
        ));
    }
    let state = text(&transition, "to")?;
    let mut evidence = array(required(old, "evidence")?)?.to_vec();
    evidence.extend(array(get(a, "evidence"))?.iter().cloned());
    let mut metadata = old.clone();
    contract::merge(
        &mut metadata,
        &object([
            ("status", string(state)),
            ("updated", string(at)),
            ("state-reason", get(a, "reason").clone()),
            ("evidence", union(evidence)?),
        ]),
    )?;
    let target = text(get(request, "target"), "path")?;
    let compacted = inactive::compact(&(render(&metadata)? + &observation.body))?;
    put(
        &mut delta.files,
        target,
        required(&compacted, "content")?.clone(),
    )?;
    delta.changed.insert(target.to_owned(), state.to_owned());
    let kind = if operation == "expire" {
        "memory-expired"
    } else {
        "memory-revoked"
    };
    history_event(
        delta,
        observation,
        request,
        operation_id,
        kind,
        old,
        get(a, "evidence"),
        at,
        "Lifecycle decision evidence",
    )
}

fn requalify_delta(
    delta: &mut Delta,
    observation: &Observation,
    request: &Value,
    operation_id: &str,
    event: &mut Value,
    at: &str,
) -> Result<()> {
    let a = get(request, "authority");
    let operation = text(request, "operation")?;
    let correcting = operation == "correct";
    let basis = get(a, "basis").as_str();
    if basis != Some("stronger-evidence") && !(correcting && basis == Some("loss-of-applicability"))
    {
        return Err(err(
            "A new qualified version needs actual stronger evidence or a narrower applicability correction",
        ));
    }
    let old = &observation.metadata;
    let candidate = get(get(&observation.capture, "request"), "candidate");
    let candidate_body = text(candidate, "body")?;
    let mut new = required(candidate, "metadata")?.clone();
    for key in ["kind", "scope", "applicability"] {
        if required(&new, key)? != required(old, key)? {
            return Err(err(
                "Corrected versions preserve their type, scope and assessed applicability",
            ));
        }
    }
    let new_conditions = required(&new, "conditions")?;
    for (key, value) in required(old, "conditions")?
        .as_object()
        .ok_or_else(invalid)?
    {
        if new_conditions.get(key) != Some(value) {
            return Err(err(
                "Do not widen or replace retained conditions while correcting guidance",
            ));
        }
    }
    if correcting {
        for key in ["valid-from", "valid-until"] {
            if required(&new, key)? != required(old, key)? {
                return Err(err(
                    "A correction cannot silently widen its validity period",
                ));
            }
        }
    } else {
        let now = lifecycle::time_value(&string(at))?;
        let from = required(&new, "valid-from")?;
        let until = required(&new, "valid-until")?;
        if from == &Value::Null
            || lifecycle::time_value(from)? > now
            || (until != &Value::Null && lifecycle::time_value(until)? <= now)
        {
            return Err(err(
                "Reactivation requires an explicit newly evidenced currently valid period",
            ));
        }
    }
    let hidden = [old.get("visibility"), new.get("visibility")]
        .iter()
        .any(|v| v.and_then(Value::as_str) == Some("hidden"));
    contract::merge(
        &mut new,
        &object([
            ("version", next_version(observation, required(old, "id")?)?),
            ("status", string("active")),
            ("trust", string("reviewed")),
            ("updated", string(at)),
            (
                "visibility",
                string(if hidden { "hidden" } else { "visible" }),
            ),
        ]),
    )?;
    let prefix = if correcting {
        "Stronger-evidence correction: "
    } else {
        "Explicit requalification: "
    };
    put(
        &mut new,
        "admission-reason",
        string(format!("{prefix}{}", text(a, "reason")?)),
    )?;
    remove(&mut new, "state-reason");
    remove(&mut new, "superseded-by");
    let mut evidence = array(required(&new, "evidence")?)?.to_vec();
    evidence.extend(array(get(a, "evidence"))?.iter().cloned());
    let source = get(a, "repository_evidence");
    if source != &Value::Null {
        evidence.push(string(format!(
            "repo:{}#sha256={}",
            text(source, "path")?,
            text(source, "sha256")?
        )));
    }
    put(event, "evidence", union(evidence.clone())?)?;
    let previous = reference(old)?;
    let mut all_evidence = array(required(old, "evidence")?)?.to_vec();
    all_evidence.extend(evidence);
    put(&mut new, "evidence", union(all_evidence)?)?;
    let mut derived = array(required(old, "derived-from")?)?.to_vec();
    derived.extend(array(required(&new, "derived-from")?)?.iter().cloned());
    derived.push(previous.clone());
    put(&mut new, "derived-from", union(derived)?)?;
    let mut corrects = array(required(old, "corrects")?)?.to_vec();
    if correcting {
        corrects.push(previous.clone());
    }
    put(&mut new, "corrects", union(corrects)?)?;
    let mut supersedes = array(required(old, "supersedes")?)?.to_vec();
    if !correcting {
        supersedes.push(previous.clone());
    }
    put(&mut new, "supersedes", union(supersedes)?)?;
    let mut replacement = reference(&new)?;
    contract::merge(
        &mut replacement,
        &object([
            ("status", string("active")),
            ("replaces", previous),
            ("applicability-preserved", Value::Bool(true)),
        ]),
    )?;
    put(event, "replacement", replacement)?;
    if !legal(&lifecycle::transition(old, &string(operation), event)?) {
        return Err(err(
            "The new version lacks explicit new evidence or a legal common lifecycle transition",
        ));
    }
    records::validate(&new, candidate_body, &[])?;
    let target = text(get(request, "target"), "path")?;
    archive(
        delta,
        old,
        &observation.body,
        &new,
        at,
        target,
        text(a, "reason")?,
        !correcting,
    )?;
    put(
        &mut delta.files,
        target,
        string(render(&new)? + candidate_body),
    )?;
    delta.changed.insert(target.to_owned(), "active".into());
    let kind = if correcting {
        "memory-corrected"
    } else {
        "memory-admitted"
    };
    let evidence = get(event, "evidence").clone();
    history_event(
        delta,
        observation,
        request,
        operation_id,
        kind,
        &new,
        &evidence,
        at,
        "New qualified version event",
    )
}

/// Baseline `str(uuid.UUID(hex=digest(...)[:32], version=4))`.
fn replacement_event_id(operation_id: &str) -> String {
    let hex = sha256_hex(format!("{operation_id}:replacement-version").as_bytes());
    let mut digits: Vec<char> = hex.chars().take(32).collect();
    if let Some(version) = digits.get_mut(12) {
        *version = '4';
    }
    if let Some(variant) = digits.get_mut(16) {
        let nibble = variant.to_digit(16).unwrap_or(0);
        *variant = char::from_digit((nibble & 0x3) | 0x8, 16).unwrap_or('8');
    }
    let mut id = String::with_capacity(36);
    for (index, c) in digits.into_iter().enumerate() {
        if matches!(index, 8 | 12 | 16 | 20) {
            id.push('-');
        }
        id.push(c);
    }
    id
}

fn supersede_delta(
    delta: &mut Delta,
    observation: &Observation,
    request: &Value,
    operation_id: &str,
    event: &mut Value,
    at: &str,
) -> Result<Option<Value>> {
    let a = get(request, "authority");
    if get(a, "basis").as_str() != Some("stronger-evidence") {
        return Err(err("Supersession needs stronger replacement evidence"));
    }
    let old = &observation.metadata;
    let previous = get(&observation.replacement, "metadata");
    let replacement_body = text(&observation.replacement, "body")?;
    for key in [
        "kind",
        "scope",
        "conditions",
        "applicability",
        "valid-from",
        "valid-until",
        "trust",
        "visibility",
    ] {
        if required(previous, key)? != required(old, key)? {
            return Ok(Some(deferral("narrowing-required")));
        }
    }
    let mut new = previous.clone();
    contract::merge(
        &mut new,
        &object([
            (
                "version",
                next_version(observation, required(previous, "id")?)?,
            ),
            ("updated", string(at)),
        ]),
    )?;
    put(
        &mut new,
        "admission-reason",
        string(format!(
            "Existing evidenced replacement: {}",
            text(a, "reason")?
        )),
    )?;
    let mut evidence = array(required(&new, "evidence")?)?.to_vec();
    evidence.extend(array(get(a, "evidence"))?.iter().cloned());
    put(&mut new, "evidence", union(evidence)?)?;
    let mut derived = array(required(&new, "derived-from")?)?.to_vec();
    derived.push(reference(previous)?);
    put(&mut new, "derived-from", union(derived)?)?;
    let mut supersedes = array(required(&new, "supersedes")?)?.to_vec();
    supersedes.push(reference(old)?);
    supersedes.extend(array(required(old, "supersedes")?)?.iter().cloned());
    supersedes.extend(array(required(old, "corrects")?)?.iter().cloned());
    put(&mut new, "supersedes", union(supersedes)?)?;
    let mut replacement = reference(&new)?;
    contract::merge(
        &mut replacement,
        &object([
            ("status", string("active")),
            ("replaces", reference(old)?),
            ("applicability-preserved", Value::Bool(true)),
        ]),
    )?;
    put(event, "replacement", replacement)?;
    if !legal(&lifecycle::transition(old, &string("supersede"), event)?) {
        return Err(err(
            "Supersession must name its qualified replacement and preserve applicability",
        ));
    }
    records::validate(&new, replacement_body, &[])?;
    let replacement_path = text(get(request, "replacement"), "path")?;
    archive(
        delta,
        previous,
        replacement_body,
        &new,
        at,
        replacement_path,
        text(a, "reason")?,
        false,
    )?;
    put(
        &mut delta.files,
        replacement_path,
        string(render(&new)? + replacement_body),
    )?;
    let mut prior = old.clone();
    contract::merge(
        &mut prior,
        &object([
            ("status", string("superseded")),
            ("updated", string(at)),
            ("state-reason", get(a, "reason").clone()),
            ("superseded-by", Value::Array(vec![reference(&new)?])),
        ]),
    )?;
    let target = text(get(request, "target"), "path")?;
    put(
        &mut delta.files,
        target,
        string(render(&prior)? + &observation.body),
    )?;
    delta.changed.insert(target.to_owned(), "superseded".into());
    for (identity, kind, record) in [
        (operation_id.to_owned(), "memory-superseded", old),
        (replacement_event_id(operation_id), "memory-admitted", &new),
    ] {
        history_event(
            delta,
            observation,
            request,
            &identity,
            kind,
            record,
            get(a, "evidence"),
            at,
            "Supersession and replacement version event",
        )?;
    }
    Ok(None)
}

/// The exact desired canonical files, or the deferral that keeps it pending.
#[derive(Clone, Debug, PartialEq)]
pub enum Desired {
    Files(Value),
    Deferred(Value),
}

pub fn desired(
    observation: &Observation,
    request: &Value,
    operation_id: &str,
    at: &str,
) -> Result<Desired> {
    let base = &observation.base;
    let operation = text(request, "operation")?;
    let a = get(request, "authority");
    let mut delta = Delta {
        files: base.clone(),
        removed: BTreeSet::new(),
        changed: BTreeMap::new(),
        additions: Vec::new(),
        deleted_bodies: Vec::new(),
    };
    let mut event = object([
        ("id", string(operation_id)),
        ("authority", get(&observation.scope, "developer_id").clone()),
        ("reason", get(a, "reason").clone()),
        ("evidence", get(a, "evidence").clone()),
        ("now", string(at)),
    ]);
    if let Some(deferred) = review_deferral(observation, request)? {
        return Ok(Desired::Deferred(deferred));
    }
    let deferred = match operation {
        "delete" => delete_delta(&mut delta, observation, request, &event, at)?,
        "revoke" | "expire" | "retention" => {
            deactivate_delta(&mut delta, observation, request, operation_id, &event, at)?;
            None
        }
        "correct" | "reactivate" => {
            requalify_delta(
                &mut delta,
                observation,
                request,
                operation_id,
                &mut event,
                at,
            )?;
            None
        }
        "supersede" => supersede_delta(
            &mut delta,
            observation,
            request,
            operation_id,
            &mut event,
            at,
        )?,
        _ => return Err(invalid()),
    };
    if let Some(deferred) = deferred {
        return Ok(Desired::Deferred(deferred));
    }
    let mut routed = reroute(
        base,
        &observation.checked,
        &delta.changed,
        &mut delta.removed,
        &delta.additions,
        at,
    )?;
    for (path, body) in delta.files.as_object().ok_or_else(invalid)? {
        if !delta.removed.contains(path) && base.get(path) != Some(body) {
            put(&mut routed, path, body.clone())?;
        }
    }
    contract::merge(&mut routed, get(request, "context_edits"))?;
    if operation == "delete" {
        let reviewed = get(request, "context_review")
            .as_object()
            .ok_or_else(invalid)?;
        for body in &delta.deleted_bodies {
            for (path, _) in reviewed {
                if copied_body(body, text(&routed, path)?) {
                    return Err(err(
                        "A retained ordinary context still contains the deleted assertion",
                    ));
                }
            }
        }
    }
    contract::validate_files(&routed, None)?;
    let changed_files = Value::Object(
        routed
            .as_object()
            .ok_or_else(invalid)?
            .iter()
            .filter(|(path, body)| base.get(path) != Some(body))
            .cloned()
            .collect(),
    );
    if changed_files
        .as_object()
        .unwrap_or_default()
        .iter()
        .any(|(_, body)| body.as_str().is_none_or(|b| b.len() > FILE_LIMIT))
    {
        return Err(err(
            "Governance must preserve complete bounded canonical files",
        ));
    }
    security::ensure_safe(&changed_files)?;
    Ok(Desired::Files(routed))
}

fn normalized_request(request: &Value, comparison_member: &str) -> Result<Value> {
    let mut request = request.clone();
    if let Some(capture) = request.get_mut("capture") {
        if let Some(inner) = capture
            .get("capture")
            .filter(|_| capture.as_object().is_some())
        {
            *capture = inner.clone();
        }
    }
    if comparison_member == "proof" {
        if let Some(proof) = request
            .get_mut("comparison")
            .and_then(|c| c.get_mut("proof"))
        {
            if let Some(inner) = proof.get("proof").filter(|_| proof.as_object().is_some()) {
                *proof = inner.clone();
            }
        }
    }
    request_shape(&request, comparison_member)?;
    Ok(request)
}

/// Baseline `normalized_request`: unwrap saved capture/proof envelopes and validate.
pub fn normalize(request: &Value) -> Result<Value> {
    normalized_request(request, "proof")
}

enum Built {
    Proposal(Value),
    Pending(Value),
}

fn build_proposal(
    observation: &Observation,
    request: &Value,
    output_context: Option<(String, String)>,
) -> Result<Built> {
    if !qualified_authority(request) {
        return Ok(Built::Pending(pending(
            observation,
            &deferral("clarification-required"),
        )?));
    }
    if !permitted_impact(observation, request) {
        return Ok(Built::Pending(pending(
            observation,
            &deferral("narrowing-required"),
        )?));
    }
    let (operation_id, at) = match output_context {
        Some(context) => context,
        None => (host::uuid4()?, host::utc_now_isoformat()?),
    };
    let files = match desired(observation, request, &operation_id, &at)? {
        Desired::Files(files) => files,
        Desired::Deferred(deferred) => return Ok(Built::Pending(pending(observation, &deferred)?)),
    };
    let action = write_action(text(request, "operation")?).ok_or_else(invalid)?;
    let mut p = required(&tx::preview(&observation.root, &files, action)?, "proposal")?.clone();
    if !same(get(&p, "observed_hashes"), &observation.observed)? {
        return Err(err("Bank changed while building the exact governed output"));
    }
    let mut evidence = object([
        ("request", request.clone()),
        ("recorded_at", string(&at)),
        ("git_context", observation.git_context.clone()),
    ]);
    let proof = get(get(request, "comparison"), "proof");
    contract::merge(
        &mut evidence,
        &layout_resolution::evidence_fields((proof != &Value::Null).then_some(proof))?,
    )?;
    put(&mut p, "operation_id", string(operation_id))?;
    put(&mut p, "governance_evidence", evidence)?;
    Ok(Built::Proposal(p))
}

/// Reviewed, private, non-applicable output planning from a pure judgment.
pub fn plan(repo: &Path, request: &Value) -> Result<Value> {
    let mut request = normalized_request(request, "judgment")?;
    let observation = observe_current(repo, &request, None)?;
    let frontier_provider = writes::recovery_frontier_provider()?;
    let c = get(&request, "comparison").clone();
    let (frontier, compared) = if c == Value::Null {
        let frontier = frontier_provider.observe(&observation.root)?;
        if get(&frontier, "hold_sha256") != &Value::Null || truthy(get(&frontier, "unit_ids")) {
            return Err(err(
                "Held governance planning requires complete current semantic judgments",
            ));
        }
        (frontier, Value::Null)
    } else {
        let inspection = comparison::inspect(&observation.root, get(&c, "request"), None)?;
        let compared = comparison::judge(&inspection, get(&c, "request"), get(&c, "judgment"))?;
        let judgment = get(get(&compared, "proof"), "judgment");
        let frontier = get(get(judgment, "observation"), "recovery_frontier").clone();
        if get(&frontier, "review_digest") != &Value::Null
            || array(get(judgment, "pending_decisions"))?
                .iter()
                .any(|row| get(get(row, "coverage_claim"), "coverage").as_str() != Some("none"))
        {
            return Err(err(
                "Planning requires coverage:none for every pending source and no preliminary coverage review",
            ));
        }
        // The pure judgment's provisional proof stays private to construction.
        // Normal observe/preview revalidate an independently reviewed proof.
        put(
            &mut request,
            "comparison",
            object([
                ("request", get(&c, "request").clone()),
                ("proof", get(&compared, "proof").clone()),
            ]),
        )?;
        (frontier, compared)
    };
    let observation = qualify_observation(observation, &request, compared, None)?;
    let built = build_proposal(&observation, &request, None)?;
    frontier_provider.check(&frontier, &observation.root, None, None)?;
    let p = match built {
        Built::Pending(mut deferred) => {
            contract::merge(
                &mut deferred,
                &object([
                    ("operation", string("plan")),
                    ("output_plan", Value::Null),
                    ("coverage_review_required", Value::Bool(true)),
                    ("verified", Value::Bool(false)),
                ]),
            )?;
            return Ok(deferred);
        }
        Built::Proposal(p) => p,
    };
    check_sources(&p, None, None)?;
    result(
        "plan",
        &object([
            ("disposition", get(&request, "operation").clone()),
            (
                "output_plan",
                layout_resolution::make_output_plan(&p, &request)?,
            ),
            ("proposal", Value::Null),
            ("canonical_writes_performed", Value::Bool(false)),
            ("coverage_review_required", Value::Bool(true)),
            ("verified", Value::Bool(false)),
        ]),
    )
}

pub fn preview(
    repo: &Path,
    request: &Value,
    output_plan: Option<&Value>,
    operation_id: Option<&str>,
    recorded_at: Option<&str>,
) -> Result<Value> {
    let request = normalize(request)?;
    let mut plan = None;
    let output_context = if let Some(supplied) = output_plan {
        if operation_id.is_some() || recorded_at.is_some() {
            return Err(err(
                "Output plan cannot be combined with explicit operation context",
            ));
        }
        let supplied = if supplied.as_object().is_some() {
            supplied.get("output_plan").unwrap_or(supplied)
        } else {
            supplied
        };
        plan = Some(supplied);
        let (id, at) = layout_resolution::output_plan_context(supplied, &request)?;
        Some((
            id.as_str().ok_or_else(invalid)?.to_owned(),
            at.as_str().ok_or_else(invalid)?.to_owned(),
        ))
    } else if operation_id.is_some() || recorded_at.is_some() {
        let id = match operation_id.filter(|id| !id.is_empty()) {
            Some(id) => id.to_owned(),
            None => host::uuid4()?,
        };
        if !uuid_valid(&string(&id)) {
            return Err(err(
                "Governance planning requires a canonical operation UUIDv4",
            ));
        }
        let at = match recorded_at.filter(|at| !at.is_empty()) {
            Some(at) => at.to_owned(),
            None => host::utc_now_isoformat()?,
        };
        Some((id, at))
    } else {
        None
    };
    let observation = observe(repo, &request, None)?;
    let p = match build_proposal(&observation, &request, output_context)? {
        Built::Pending(deferred) => {
            if plan.is_some() {
                return Err(err("Deferred governance cannot consume an output plan"));
            }
            return Ok(deferred);
        }
        Built::Proposal(p) => p,
    };
    if let Some(plan) = plan {
        layout_resolution::check_output_plan(&p, &request, plan)?;
    }
    validate_transaction(&p, None)?;
    result(
        "preview",
        &object([
            ("disposition", get(&request, "operation").clone()),
            ("proposal", p),
            ("canonical_writes_performed", Value::Bool(false)),
        ]),
    )
}

pub fn evidence_value(p: &Value) -> Result<&Value> {
    let evidence = get(p, "governance_evidence");
    if !exact(evidence, EVIDENCE) {
        return Err(err(
            "Governance needs its exact request and observed event context",
        ));
    }
    Ok(evidence)
}

pub fn validate_transaction(p: &Value, owner: Option<&[u8]>) -> Result<()> {
    let evidence = evidence_value(p)?;
    let at = get(evidence, "recorded_at");
    if lifecycle::time_value(at)? > lifecycle::time_value(&string(host::utc_now_isoformat()?))? {
        return Err(err("Governance event cannot be future dated"));
    }
    let at = at.as_str().ok_or_else(invalid)?;
    let request = get(evidence, "request");
    let repo = Path::new(text(p, "repo_root")?);
    let observation = observe(repo, request, owner)?;
    if get(p, "operation").as_str() != write_action(text(request, "operation")?)
        || get(p, "project_id") != get(request, "project_id")
        || !qualified_authority(request)
        || !same(&observation.git_context, get(evidence, "git_context"))?
    {
        return Err(err("Governance evidence or actual Git context changed"));
    }
    if !permitted_impact(&observation, request) {
        return Err(err("Preserve narrower applicability before pruning"));
    }
    let files = desired(&observation, request, text(p, "operation_id")?, at)?;
    let exact_delta = match &files {
        Desired::Files(files) => same(files, get(p, "files"))?,
        Desired::Deferred(_) => false,
    };
    if !exact_delta || !same(&observation.observed, get(p, "observed_hashes"))? {
        return Err(err(
            "Governance must apply its exact observed canonical delta",
        ));
    }
    writes::recovery_frontier_provider()?.require_scope_owned(
        repo,
        get(request, "project_id"),
        Some(get(p, "scope_context")),
        owner,
        None,
    )?;
    // Baseline layout_resolution.claims_for(p, owner=owned_lock); the claims are
    // validated there and their values are not used here.
    layout_resolution::claims_for(p, owner, None, &crate::cli::runtime_root()?)?;
    check_sources(p, owner, None)
}

pub fn check_sources(p: &Value, owner: Option<&[u8]>, marker: Option<&[u8]>) -> Result<()> {
    let evidence = get(p, "governance_evidence");
    let request = get(evidence, "request");
    let root = Path::new(text(p, "repo_root")?);
    let frontier = writes::recovery_frontier_provider()?;
    frontier.require_scope_owned(
        root,
        get(request, "project_id"),
        Some(get(p, "scope_context")),
        owner,
        marker,
    )?;
    let c = get(request, "comparison");
    if c != &Value::Null {
        frontier.check(
            get(
                get(get(get(c, "proof"), "judgment"), "observation"),
                "recovery_frontier",
            ),
            root,
            owner,
            marker,
        )?;
    }
    if !same(&retrieval::git_context(root)?, get(evidence, "git_context"))? {
        return Err(err("Checkout changed during governance"));
    }
    let source = get(get(request, "authority"), "repository_evidence");
    if source != &Value::Null {
        security::repository_source(root, source)?;
    }
    if get(request, "capture") != &Value::Null {
        let mut admission_evidence = object([
            ("capture", get(request, "capture").clone()),
            ("comparison_request", get(c, "request").clone()),
            ("comparison_proof", get(c, "proof").clone()),
            ("recorded_at", get(evidence, "recorded_at").clone()),
        ]);
        for key in [
            "unit_resolution_claims",
            "comparison_proof_sha256",
            "reference_map_sha256",
        ] {
            put(
                &mut admission_evidence,
                key,
                required(evidence, key)?.clone(),
            )?;
        }
        let mut admitted = p.clone();
        put(&mut admitted, "admission_evidence", admission_evidence)?;
        admission::check_sources(&admitted, owner, marker)?;
    }
    let operation = text(request, "operation")?;
    let target = text(get(request, "target"), "path")?;
    if operation == "reactivate" {
        let (_, metadata, _) = records::parse(text(get(p, "files"), target)?, &[])?;
        let now = lifecycle::time_value(&string(host::utc_now_isoformat()?))?;
        let until = required(&metadata, "valid-until")?;
        if lifecycle::time_value(required(&metadata, "valid-from")?)? > now
            || (until != &Value::Null && lifecycle::time_value(until)? <= now)
        {
            return Err(err("The newly requalified period ended during governance"));
        }
    }
    if operation == "supersede" {
        replacement_current(p, request, root)?;
    }
    Ok(())
}

/// The linked replacement must still be the current qualified guidance.
fn replacement_current(p: &Value, request: &Value, root: &Path) -> Result<()> {
    let files = get(p, "files");
    let checked = contract::validate_files(files, None)?;
    let path = text(get(request, "replacement"), "path")?;
    let (_, replacement, _) = records::parse(text(files, path)?, &[])?;
    let (context, _) = retrieval::observed_context(root, get(request, "context"))?;
    let now = string(host::utc_now_isoformat()?);
    let project = get(request, "project_id");
    let applicability_request = object([
        ("bank_project_id", project.clone()),
        ("context", context.clone()),
    ]);
    let linked = reference(&replacement)?;
    let mut superseders = Vec::new();
    for entry in array(get(&checked, "entries"))? {
        if get(entry, "format").as_str() != Some("standalone-record")
            || !retrieval::scope_reasons(project, entry, &context)?.is_empty()
        {
            continue;
        }
        let (_, metadata, _) = records::parse(text(files, text(entry, "path")?)?, &[])?;
        let applicable = records::applicability(&metadata, &applicability_request)?;
        let selection = lifecycle::current(
            &metadata,
            &object([
                ("authorized-scope", Value::Bool(true)),
                ("applicable", get(&applicable, "applicable").clone()),
                ("now", now.clone()),
                ("minimum-trust", string("reviewed")),
                ("active-superseders", Value::Array(vec![])),
            ]),
        )?;
        let gates = get(&selection, "gates");
        let links = array(required(&metadata, "supersedes")?)?
            .iter()
            .chain(array(required(&metadata, "corrects")?)?)
            .any(|r| r == &linked);
        if CURRENT_GATES
            .iter()
            .all(|key| get(gates, key) == &Value::Bool(true))
            && links
        {
            superseders.push(string(format!(
                "{}/{}",
                text(&metadata, "id")?,
                required(&metadata, "version")?
                    .as_int()
                    .ok_or_else(invalid)?
                    .as_str()
            )));
        }
    }
    let applicable = records::applicability(&replacement, &applicability_request)?;
    let selection = lifecycle::current(
        &replacement,
        &object([
            ("authorized-scope", Value::Bool(true)),
            ("applicable", get(&applicable, "applicable").clone()),
            ("now", now),
            ("minimum-trust", string("reviewed")),
            ("active-superseders", Value::Array(superseders)),
        ]),
    )?;
    if get(&selection, "current") != &Value::Bool(true) {
        return Err(err(
            "The replacement lost its current qualification during governance",
        ));
    }
    Ok(())
}

pub fn apply(prepared: &Value) -> Result<Value> {
    let p = tx::proposal_value(prepared)?;
    if p.get("governance_evidence").is_none() {
        return Err(err("Use a complete governed lifecycle proposal"));
    }
    let receipt = tx::apply(p)?;
    let mut fields = receipt.clone();
    remove(&mut fields, "schema");
    remove(&mut fields, "operation");
    put(
        &mut fields,
        "governance_verified",
        Value::Bool(
            get(&receipt, "verified") == &Value::Bool(true)
                && get(&receipt, "outcome").as_str() == Some("finished"),
        ),
    )?;
    result("apply", &fields)
}

/// Baseline `main(required_operations=...)` intent check for the alias routes.
pub fn require_intent(
    prepared: &Value,
    preparing: bool,
    required_operations: Option<&[&str]>,
) -> Result<()> {
    let Some(allowed) = required_operations else {
        return Ok(());
    };
    let request = if preparing {
        prepared
    } else {
        required(
            required(tx::proposal_value(prepared)?, "governance_evidence")?,
            "request",
        )?
    };
    if !member(required(request, "operation")?, allowed) {
        return Err(err("The requested operation differs from the CLI intent"));
    }
    Ok(())
}

/// Operation-keyed transaction gate for correct, supersede, revoke and forget.
pub struct Governance;
impl tx::TransactionValidator for Governance {
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
            let preparing = matches!(operation, "plan" | "preview");
            let message = if matches!(error.kind(), Kind::BankCapacity { .. }) {
                error.to_string()
            } else {
                "Unqualified, ambiguous, changed or incomplete governance; preserve evidence and inspect".into()
            };
            let mut fields = object([
                ("verified", Value::Bool(false)),
                (
                    "canonical_writes_performed",
                    if preparing {
                        Value::Bool(false)
                    } else {
                        Value::Null
                    },
                ),
                (
                    "recovery_may_be_required",
                    Value::Bool(operation == "apply"),
                ),
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
