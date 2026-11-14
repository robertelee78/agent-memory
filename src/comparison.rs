//! Source-bound search/read and explicit agent judgment before durable learning.
use crate::bank_contract as bank;
use crate::bank_writes as writes;
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{encoded, sha256_hex as digest, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::layout_resolution::{field, has_keys, items, same, set, text, with};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_observation, discovery, host, layout_recovery_state, records, retrieval, security,
    sharing_limits,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const LIMIT: usize = 65536;
const MAX_SOURCE: usize = 16384;
const CLASSES: &[&str] = &[
    "equivalent",
    "refinement",
    "scope-correction",
    "distinct-conditions",
    "distinct-knowledge",
    "contradiction",
    "related-context",
    "uncertain",
];
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
fn render(metadata: &Value) -> Result<String> {
    frontmatter::write(
        metadata
            .as_object()
            .ok_or_else(|| err("Invalid record metadata"))?,
    )
}
pub fn safe_text(value: &Value, maximum: usize) -> Result<&str> {
    let value = value
        .as_str()
        .filter(|v| {
            !bank::trim(v).is_empty()
                && v.len() <= maximum
                && !v.chars().any(|c| c < ' ' && c != '\n' && c != '\t')
        })
        .ok_or_else(|| err("Comparison needs bounded explicit search/reason text"))?;
    if !items(get(
        &security::scan(&string(value), security::LIMIT)?,
        "reasons",
    ))?
    .is_empty()
    {
        return Err(err(
            "Unsafe comparison text must be reviewed without creating a usable proof",
        ));
    }
    Ok(value)
}
pub fn request_value(request: &Value) -> Result<&Value> {
    let required = ["project_id", "candidate", "searches", "read_paths"];
    let keys = crate::layout_resolution::keys(request);
    if request.as_object().is_none()
        || required.iter().any(|k| !keys.contains(k))
        || keys
            .iter()
            .any(|k| !required.contains(k) && *k != "discovery")
        || !uuid_valid(get(request, "project_id"))
        || !has_keys(get(request, "candidate"), &["metadata", "body"])
        || !get(request, "searches")
            .as_array()
            .is_some_and(|a| (2..=8).contains(&a.len()))
        || !get(request, "read_paths")
            .as_array()
            .is_some_and(|a| a.len() <= 64)
        || encoded(request)?.len() > LIMIT
    {
        return Err(err(
            "Compare one explicit record proposal with bounded alternative searches and read paths",
        ));
    }
    let candidate = field(request, "candidate")?;
    let metadata = field(candidate, "metadata")?;
    let body = text(candidate, "body")?;
    let (kind, _) = records::validate(metadata, body, &[])?;
    if kind != "standalone-record" || get(metadata, "project-id") != get(request, "project_id") {
        return Err(err(
            "Candidate must be a same-project standalone proposal, never a tombstone or event",
        ));
    }
    security::ensure_safe(&object([(
        "candidate.md",
        string(render(metadata)? + body),
    )]))?;
    let mut searches = BTreeSet::new();
    for query in items(field(request, "searches")?)? {
        let folded = crate::unicode::casefold(safe_text(query, 256)?);
        let normalized = folded
            .split(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !searches.insert(normalized) {
            return Err(err(
                "Search the concept and genuinely different wording, not repeated query strings",
            ));
        }
    }
    let mut paths = BTreeSet::new();
    for path in items(field(request, "read_paths")?)? {
        if !paths.insert(bank::path_value(path)?) {
            return Err(err("Explicit plausible source paths must be unique"));
        }
    }
    Ok(metadata)
}
fn source(bank_path: &Path, path: &str, observed: &Value, message: &str) -> Result<String> {
    let raw = bank_observation::read_source(bank_path, path, MAX_SOURCE)?;
    if get(observed, path).as_str() != Some(digest(&raw).as_str()) {
        return Err(err(message));
    }
    String::from_utf8(raw).map_err(|_| ContractError::non_utf8("comparison source"))
}
pub fn inspect(repo: &Path, request: &Value, owner: Option<&[u8]>) -> Result<Value> {
    let metadata = request_value(request)?;
    let (root, bank_path) = writes::bank_path(repo)?;
    let frontier = writes::recovery_frontier_provider()?;
    let context =
        frontier.require_scope_owned(&root, field(request, "project_id")?, None, owner, None)?;
    let token = frontier.observe_owned(&root, owner, None)?;
    let pending = frontier.pending_rows(&root, &token, owner, None, &crate::cli::pack_root()?)?;
    let (checked, observed) = writes::snapshot(&root, owner)?;
    let selected: BTreeSet<String> = items(field(request, "read_paths")?)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| err("Invalid comparison path"))
        })
        .collect::<Result<_>>()?;
    let mut discovered = BTreeSet::new();
    if let Some(trace) = request.get("discovery") {
        for row in items(field(trace, "sources")?)? {
            discovered.insert(text(row, "path")?.to_owned());
        }
        discovery::check_trace(
            &root,
            trace,
            &checked,
            &observed,
            &context,
            "comparison",
            &selected,
            owner,
            None,
        )?;
    }
    if get(&checked, "project_id") != get(request, "project_id") {
        return Err(err("Bank identity changed during comparison"));
    }
    let entries = items(field(&checked, "entries")?)?;
    let deleted: BTreeSet<&str> = entries
        .iter()
        .filter(|v| get(v, "format").as_str() == Some("tombstone"))
        .map(|v| text(v, "id"))
        .collect::<Result<_>>()?;
    let mut referenced = BTreeSet::from([text(metadata, "id")?]);
    for key in ["derived-from", "corrects", "supersedes"] {
        for reference in items(field(metadata, key)?)? {
            referenced.insert(text(reference, "id")?);
        }
    }
    if !deleted.is_disjoint(&referenced) {
        return Err(err(
            "Terminal deleted identity cannot be reused or restored through a comparison proposal",
        ));
    }
    let by_path: BTreeMap<&str, &Value> = entries
        .iter()
        .map(|v| Ok((text(v, "path")?, v)))
        .collect::<Result<_>>()?;
    if selected.iter().any(|p| {
        !by_path.get(p.as_str()).is_some_and(|e| {
            matches!(
                get(e, "format").as_str(),
                Some("standalone-record" | "context-prose")
            )
        })
    }) {
        return Err(err(
            "Read each plausible match from actual canonical learning, not an absent path or event",
        ));
    }
    let mut capture_owners = BTreeSet::new();
    let mut identity_review = BTreeSet::new();
    for entry in entries {
        let path = text(entry, "path")?;
        if !path.starts_with("history/sources/") {
            continue;
        }
        if text(entry, "format")? != "standalone-record" {
            return Err(err(
                "Original capture provenance is incomplete; inspect before claiming absence",
            ));
        }
        let raw = source(
            &bank_path,
            path,
            &observed,
            "Capture provenance changed or exceeds its reader",
        )?;
        let (_, source, _) = records::parse(&raw, &[])?;
        let captured_id = records::source_capture_id(path, &source);
        let id = text(metadata, "id")?;
        if path.starts_with(&format!("history/sources/{id}/")) && captured_id.as_deref() != Some(id)
        {
            return Err(err(
                "Original capture identity is unresolved; inspect before claiming absence",
            ));
        }
        if captured_id.as_deref() == Some(id) {
            capture_owners.insert(text(&source, "id")?.to_owned());
            if let Some(refs) = source.get("superseded-by") {
                for reference in items(refs)? {
                    capture_owners.insert(text(reference, "id")?.to_owned());
                }
            }
        } else if captured_id.is_none()
            && items(field(&source, "derived-from")?)?
                .iter()
                .any(|v| get(v, "id").as_str() == Some(id))
        {
            identity_review.insert(path.to_owned());
        }
    }
    let router = source(
        &bank_path,
        "MEMORY.md",
        &observed,
        "Router changed during comparison",
    )?;
    security::ensure_safe(&object([("MEMORY.md", string(&router))]))?;
    let mut used = 0usize;
    for row in &pending {
        used += encoded(row)?.len();
    }
    if pending.len() > 64 || used > LIMIT {
        return Err(err(
            "Pending frontier exceeds complete bounded review; recovery remains pending",
        ));
    }
    let mut matches = Vec::new();
    // Preserve inventory traversal for refusal order; sort only the final match rows.
    for entry in entries {
        let format = text(entry, "format")?;
        if !matches!(format, "standalone-record" | "context-prose") {
            continue;
        }
        let path = text(entry, "path")?;
        let content = source(
            &bank_path,
            path,
            &observed,
            "Memory changed during plausible-match search",
        )?;
        let (existing, body) = if format == "standalone-record" {
            let (_, m, b) = records::parse(&content, &[])?;
            (m, b)
        } else {
            (Value::Null, content.clone())
        };
        let mut hits = Vec::new();
        for query in items(field(request, "searches")?)? {
            if retrieval::matching(
                &object([
                    ("ids", Value::Array(vec![])),
                    ("paths", Value::Array(vec![])),
                    ("query", query.clone()),
                ]),
                &object([("path", string(path)), ("body", string(&body))]),
            )?
            .is_some()
            {
                hits.push(query.clone());
            }
        }
        let same_id = existing != Value::Null
            && (get(&existing, "id") == get(metadata, "id")
                || capture_owners.contains(text(&existing, "id")?));
        if hits.is_empty()
            && !selected.contains(path)
            && !discovered.contains(path)
            && !identity_review.contains(path)
            && !same_id
        {
            continue;
        }
        security::ensure_safe(&object([(
            path,
            string(format!(
                "{content}\n{}",
                json::encode(field(entry, "scope_chain")?, json::UTF8)?
            )),
        )]))?;
        if get(&existing, "visibility").as_str() == Some("hidden") {
            return Err(err(
                "A plausible hidden match needs explicit source review; do not create a duplicate around it",
            ));
        }
        let row = object([
            ("path", string(path)),
            ("sha256", field(&observed, path)?.clone()),
            ("format", string(format)),
            ("body", string(body)),
            ("metadata", existing),
            ("scope_chain", field(entry, "scope_chain")?.clone()),
            ("lifecycle", field(entry, "lifecycle")?.clone()),
            ("matched_searches", Value::Array(hits)),
            (
                "selection_class",
                string("comparison-only-not-current-guidance"),
            ),
            ("source_assertions_verified", Value::Bool(false)),
        ]);
        used += encoded(&row)?.len();
        if matches.len() + pending.len() >= 64 || used > LIMIT {
            return Err(err(
                "Plausible matches exceed a complete bounded review; refine and inspect before writing",
            ));
        }
        matches.push(row);
    }
    matches.sort_by(|a, b| get(a, "path").as_str().cmp(&get(b, "path").as_str()));
    frontier.require_scope_owned(
        &root,
        field(&checked, "project_id")?,
        Some(&context),
        owner,
        None,
    )?;
    if !same(&writes::snapshot(&root, owner)?.1, &observed) {
        return Err(err(
            "Canonical memory changed during comparison; repeat searches and read new matches",
        ));
    }
    frontier.check(&token, &root, owner, None)?;
    let observation = object([
        ("schema", string("agent-memory/comparison-observation-v2")),
        ("project_id", field(&checked, "project_id")?.clone()),
        ("request_sha256", string(digest(&encoded(request)?))),
        (
            "bank_observation_sha256",
            string(digest(&encoded(&object([
                ("project_id", field(&checked, "project_id")?.clone()),
                ("sources", observed.clone()),
            ]))?)),
        ),
        ("router_sha256", field(&observed, "MEMORY.md")?.clone()),
        ("recovery_frontier", token.clone()),
        (
            "matches",
            Value::Array(
                matches
                    .iter()
                    .map(|v| {
                        object([
                            ("path", get(v, "path").clone()),
                            ("sha256", get(v, "sha256").clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let ids: BTreeSet<String> = matches
        .iter()
        .filter(|v| get(v, "metadata") != &Value::Null)
        .map(|v| text(get(v, "metadata"), "id").map(str::to_owned))
        .collect::<Result<_>>()?;
    let audit = security::audit(
        &root,
        &host::uuid4()?,
        "retrieval",
        "checked",
        &context,
        &["external"],
        &[],
        &ids.into_iter().collect::<Vec<_>>(),
        "policy-check",
        None,
    )?;
    frontier.require_scope_owned(
        &root,
        field(&checked, "project_id")?,
        Some(&context),
        owner,
        None,
    )?;
    if !same(&writes::snapshot(&root, owner)?.1, &observed) {
        return Err(err(
            "Memory changed before comparison readback; do not use an earlier novelty judgment",
        ));
    }
    frontier.check(&token, &root, owner, None)?;
    Ok(object([
        ("schema", string("agent-memory/comparison-result-v2")),
        ("operation", string("inspect")),
        ("observation", observation),
        ("capture_identity_owners", strings(capture_owners)),
        ("capture_identity_review_paths", strings(identity_review)),
        ("router", string(router)),
        ("matches", Value::Array(matches)),
        ("candidate", field(request, "candidate")?.clone()),
        ("pending_matches", Value::Array(pending)),
        (
            "detecting_developer_id",
            field(&context, "developer_id")?.clone(),
        ),
        ("audit", audit),
        ("comparison_read_complete", Value::Bool(true)),
        ("semantic_completeness_verified", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
        ("main_freshness_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("admission_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("audit_writes_performed", Value::Bool(true)),
        (
            "validation_scope",
            string("local-source-bound-search-and-agent-judgment-only"),
        ),
    ]))
}

pub fn judge(inspection: &Value, request: &Value, judgment: &Value) -> Result<Value> {
    if !has_keys(
        judgment,
        &["observation", "decisions", "pending_decisions", "rationale"],
    ) || get(judgment, "decisions").as_array().is_none()
        || get(judgment, "pending_decisions").as_array().is_none()
    {
        return Err(err(
            "Judgment must review the complete v2 canonical and private pending observation",
        ));
    }
    let mut normalized = field(judgment, "observation")?.clone();
    let token = normalized
        .get_mut("recovery_frontier")
        .ok_or_else(|| err("Invalid judgment observation"))?;
    if token.as_object().is_none() {
        return Err(err("Invalid judgment observation"));
    }
    set(token, "review_digest", Value::Null);
    if !same(&normalized, field(inspection, "observation")?) {
        return Err(err(
            "Judgment must name the actual read observation; changed bank requires a new comparison",
        ));
    }
    safe_text(field(judgment, "rationale")?, 4096)?;
    if encoded(field(judgment, "rationale")?)?.len() > 4096 {
        return Err(err(
            "Top-level comparison rationale exceeds its complete encoded bound",
        ));
    }
    let decisions = items(field(judgment, "decisions")?)?;
    let rows: BTreeMap<&str, &Value> = items(field(inspection, "matches")?)?
        .iter()
        .map(|v| Ok((text(v, "path")?, v)))
        .collect::<Result<_>>()?;
    let mut paths = BTreeSet::new();
    for d in decisions {
        if !has_keys(d, &["path", "classification", "rationale"])
            || !CLASSES.contains(&text(d, "classification")?)
            || !paths.insert(text(d, "path")?)
        {
            return Err(err(
                "Every plausible match needs an explicit classified semantic judgment",
            ));
        }
    }
    if paths != rows.keys().copied().collect() {
        return Err(err(
            "Do not omit, substitute or duplicate a plausible source in the comparison judgment",
        ));
    }
    let candidate = field(field(request, "candidate")?, "metadata")?;
    let review_paths = items(field(inspection, "capture_identity_review_paths")?)?;
    let mut targets = Vec::new();
    let mut conflicts = Vec::new();
    let mut uncertain = Vec::new();
    let mut inactive = Vec::new();
    let mut corrections = Vec::new();
    let mut peers = BTreeSet::new();
    for decision in decisions {
        safe_text(field(decision, "rationale")?, 2048)?;
        let path = text(decision, "path")?;
        let row = rows
            .get(path)
            .ok_or_else(|| err("Unknown comparison source"))?;
        let class = text(decision, "classification")?;
        let existing = field(row, "metadata")?;
        if (review_paths.contains(&string(path)) || class == "contradiction")
            && existing != &Value::Null
            && get(get(existing, "origin"), "developer-id")
                != get(inspection, "detecting_developer_id")
        {
            peers.insert(text(field(existing, "origin")?, "developer-id")?.to_owned());
        }
        if class == "contradiction" {
            conflicts.push(string(path));
        }
        if class == "uncertain" {
            uncertain.push(string(path));
        }
        if existing != &Value::Null {
            let same_conditions = ["scope", "conditions", "applicability"]
                .iter()
                .all(|k| same(get(existing, k), get(candidate, k)));
            if matches!(class, "equivalent" | "refinement")
                && (!same_conditions || get(existing, "kind") != get(candidate, "kind"))
            {
                return Err(err(
                    "Different conditions or types must stay distinct rather than be silently combined",
                ));
            }
            if class == "distinct-conditions" && same_conditions {
                return Err(err(
                    "A condition distinction needs actual different conditions, not a duplicate label",
                ));
            }
        }
        if class == "scope-correction" {
            let old = get(existing, "conditions")
                .as_object()
                .ok_or_else(|| err("Scope correction requires typed metadata"))?;
            let new = get(candidate, "conditions")
                .as_object()
                .ok_or_else(|| err("Scope correction requires typed metadata"))?;
            if existing == &Value::Null
                || ["id", "kind", "scope", "applicability"]
                    .iter()
                    .any(|k| !same(get(existing, k), get(candidate, k)))
                || old.len() >= new.len()
                || old.iter().any(|(k, v)| {
                    get(candidate, "conditions")
                        .get(k)
                        .is_none_or(|n| !same(n, v))
                })
                || new.iter().any(|(_, v)| v == &Value::Null)
                || get(row, "body") != get(get(request, "candidate"), "body")
            {
                return Err(err(
                    "Scope correction retains the exact same learning body and known conditions while adding a narrower constraint",
                ));
            }
            corrections.push(string(path));
        }
        if matches!(class, "equivalent" | "refinement" | "scope-correction") {
            targets.push(object([
                ("path", string(path)),
                ("sha256", field(row, "sha256")?.clone()),
                ("id", get(existing, "id").clone()),
                ("version", get(existing, "version").clone()),
                ("required_preserved_metadata", existing.clone()),
                ("scope_chain", field(row, "scope_chain")?.clone()),
            ]));
            if text(row, "lifecycle")? != "active" {
                inactive.push(string(path));
            }
        }
    }
    let pending: BTreeMap<&str, &Value> = items(field(inspection, "pending_matches")?)?
        .iter()
        .map(|v| Ok((text(v, "unit_id")?, v)))
        .collect::<Result<_>>()?;
    let pending_decisions = items(field(judgment, "pending_decisions")?)?;
    let mut selected = BTreeSet::new();
    for decision in pending_decisions {
        selected.insert(text(decision, "unit_id")?);
    }
    if pending_decisions.len() != pending.len() || selected != pending.keys().copied().collect() {
        return Err(err(
            "Every unresolved same-bank unit requires exactly one pending decision",
        ));
    }
    for decision in pending_decisions {
        if !has_keys(
            decision,
            &["unit_id", "classification", "rationale", "coverage_claim"],
        ) || !CLASSES.contains(&text(decision, "classification")?)
        {
            return Err(err(
                "Pending comparison requires classified source-bound coverage claims",
            ));
        }
        safe_text(field(decision, "rationale")?, 2048)?;
        let id = text(decision, "unit_id")?;
        writes::recovery_frontier_provider()?.claim_value(
            field(decision, "coverage_claim")?,
            pending.get(id).ok_or_else(|| err("Unknown pending unit"))?,
        )?;
        let class = text(decision, "classification")?;
        if matches!(class, "contradiction" | "uncertain" | "scope-correction")
            && get(get(decision, "coverage_claim"), "coverage").as_str() != Some("none")
        {
            return Err(err(
                "Unclarified pending meaning cannot receive terminal coverage credit",
            ));
        }
        if class == "contradiction" {
            conflicts.push(string(format!("unit:{id}")));
        } else if matches!(class, "uncertain" | "scope-correction") {
            uncertain.push(string(format!("unit:{id}")));
        }
    }
    let disposition = if !conflicts.is_empty() {
        "clarification-required"
    } else if !review_paths.is_empty() {
        "provenance-identity-review-required"
    } else if !uncertain.is_empty() {
        "review-required"
    } else if !corrections.is_empty() {
        "governance-required"
    } else if !inactive.is_empty() {
        "lifecycle-review-required"
    } else if targets.len() == 1 {
        "update-existing"
    } else if !targets.is_empty() {
        "consolidate-existing"
    } else {
        "create-distinct"
    };
    if disposition == "create-distinct"
        && (!items(field(inspection, "capture_identity_owners")?)?.is_empty()
            || rows.values().any(|row| {
                get(row, "metadata") != &Value::Null
                    && get(get(row, "metadata"), "id") == get(candidate, "id")
            }))
    {
        return Err(err(
            "Distinct new knowledge must use a separate stable identity, not reuse an existing one",
        ));
    }
    let has_targets = !targets.is_empty();
    let proof = object([
        ("schema", string("agent-memory/comparison-proof-v2")),
        ("judgment", judgment.clone()),
        ("disposition", string(disposition)),
        ("targets", Value::Array(targets)),
    ]);
    if encoded(&proof)?.len() > LIMIT {
        return Err(err(
            "Complete comparison proof exceeds its reader budget; review smaller independent learning units",
        ));
    }
    let recommend = !peers.is_empty();
    Ok(with(
        inspection.clone(),
        [
            ("operation", string("review")),
            ("proof_sha256", string(digest(&encoded(&proof)?))),
            ("proof", proof),
            ("disposition", string(disposition)),
            (
                "same_learning_requires_existing_identity",
                Value::Bool(has_targets),
            ),
            ("disputed_paths", Value::Array(conflicts)),
            ("other_developer_ids", strings(peers)),
            (
                "recommend_other_developer_clarification",
                Value::Bool(recommend),
            ),
            ("messages_sent", Value::Bool(false)),
            ("judgment_is_agent_assertion", Value::Bool(true)),
            ("semantic_completeness_verified", Value::Bool(false)),
            ("admission_verified", Value::Bool(false)),
            ("publication_verified", Value::Bool(false)),
        ],
    ))
}
pub fn review(
    repo: &Path,
    request: &Value,
    judgment: &Value,
    owner: Option<&[u8]>,
    coverage_review: Option<&Value>,
) -> Result<Value> {
    let mut judgment = judgment.clone();
    if let Some(coverage) = coverage_review {
        let operation = field(coverage, "operation_id")?;
        if !uuid_valid(operation) {
            return Err(err(
                "Coverage review must name its actual existing recovery operation",
            ));
        }
        let hash = digest(&encoded(coverage)?);
        let observation = judgment
            .get_mut("observation")
            .ok_or_else(|| err("Invalid judgment observation"))?;
        let token = observation
            .get_mut("recovery_frontier")
            .filter(|v| v.as_object().is_some())
            .ok_or_else(|| err("Invalid judgment frontier"))?;
        set(token, "review_digest", string(&hash));
        let paths =
            layout_recovery_state::paths(repo, operation.as_str(), &crate::cli::pack_root()?)?;
        let directory = paths
            .3
            .ok_or_else(|| err("Coverage review requires an operation directory"))?;
        layout_recovery_state::write_once(
            &directory
                .join("coverage-reviews")
                .join(format!("{hash}.json")),
            coverage,
        )?;
    }
    let result = judge(&inspect(repo, request, owner)?, request, &judgment)?;
    writes::recovery_frontier_provider()?.validate_comparison_review(
        repo,
        field(&result, "proof")?,
        owner,
        None,
        &crate::cli::pack_root()?,
    )?;
    Ok(result)
}
pub fn check(repo: &Path, request: &Value, proof: &Value, owner: Option<&[u8]>) -> Result<Value> {
    if !has_keys(proof, &["schema", "judgment", "disposition", "targets"])
        || get(proof, "schema").as_str() != Some("agent-memory/comparison-proof-v2")
    {
        return Err(err(
            "Revalidation requires the exact original comparison proof",
        ));
    }
    let result = review(repo, request, field(proof, "judgment")?, owner, None)?;
    if encoded(field(&result, "proof")?)? != encoded(proof)? {
        return Err(err(
            "Comparison proof was changed or does not describe the current canonical sources",
        ));
    }
    Ok(with(
        result,
        [
            ("operation", string("check")),
            ("source_observation_current", Value::Bool(true)),
        ],
    ))
}
pub fn read_input(path: &Path) -> Result<Value> {
    crate::developer_identity::load_with_limit(&sharing_limits::read_bytes(path, LIMIT)?, LIMIT)
}
pub fn cli_judgment(value: &Value) -> Value {
    let observed = get(value, "observation");
    if has_keys(
        value,
        &["observation", "decisions", "pending_decisions", "rationale"],
    ) && get(observed, "schema").as_str() == Some("agent-memory/comparison-result-v2")
        && get(observed, "operation").as_str() == Some("inspect")
        && get(observed, "comparison_read_complete") == &Value::Bool(true)
    {
        return with(
            value.clone(),
            [("observation", get(observed, "observation").clone())],
        );
    }
    value.clone()
}
pub struct Comparison;
impl crate::admission::Comparison for Comparison {
    fn request_value(&self, request: &Value) -> Result<()> {
        request_value(request).map(|_| ())
    }
    fn check(
        &self,
        repo: &Path,
        request: &Value,
        proof: &Value,
        owner: Option<&[u8]>,
    ) -> Result<Value> {
        check(repo, request, proof, owner)
    }
}
pub fn command_result(outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    match outcome {
        Ok(value) => crate::lifecycle::CommandResult::json(value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let message = if matches!(error.kind(), Kind::BankCapacity { .. }) {
                error.to_string()
            } else {
                "Incomplete, unsafe, unavailable or changed comparison; preserve sources and do not infer novelty".into()
            };
            let mut value = object([
                ("schema", string("agent-memory/comparison-result-v2")),
                ("verified", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
                ("comparison_read_complete", Value::Bool(false)),
                ("error", string(message)),
            ]);
            match bank::merge(&mut value, &crate::bank_capacity::details(&error)) {
                Ok(()) => crate::lifecycle::CommandResult::json(value, 1, json::DEFAULT),
                Err(error) => crate::lifecycle::CommandResult {
                    value: None,
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("{error}\n"),
                },
            }
        }
    }
}
