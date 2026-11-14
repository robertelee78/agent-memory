//! Retained independently reviewed same-bank invocation/runtime renewals.
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::layout_resolution::{
    err, field, frontier, has_keys, locked, read_optional, read_value, safe_text, same, set, sha,
    text, uuid,
};
use crate::lifecycle::{get, object, string};
use crate::scope_binding::{self, RequireOptions};
use crate::{
    artifact, bank_observation, bank_writes as writes, host, installation,
    layout_recovery_state as state, lifecycle, sharing_limits,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const ATTESTATION_STATEMENT: &str = "All authorized legacy invocation routes and in-flight sessions are disabled or quiesced until terminal recovery retirement, or use the independently verified capable runtime and bound private state.";
const ATTESTATION_SCHEMA: &str = "agent-memory/layout-quiescence-attestation-v1";
const REVIEW_SCHEMA: &str = "agent-memory/layout-quiescence-review-v1";
const PENDING: &str = "pending-runtime-compatibility";
const ATTESTATION_FIELDS: &[&str] = &[
    "schema",
    "attestation_id",
    "operation_id",
    "project_id",
    "repo_root",
    "bank_path",
    "checkout_key",
    "state_root",
    "manifest_sha256",
    "runtime_artifact_digest",
    "operator_developer_id",
    "issued_at",
    "declared_principals",
    "declared_invocation_routes",
    "stopped_sessions",
    "statement",
    "review_id",
];
const REVIEW_FIELDS: &[&str] = &[
    "schema",
    "review_id",
    "operation_id",
    "attestation_sha256",
    "reviewer_developer_id",
    "reviewer_harness",
    "reviewer_session",
    "review_time",
    "decision",
    "rationale",
];
/// Baseline bound on each declared list and on each listed item.
const MAX_DECLARED: usize = 1024;
const MAX_DECLARED_ITEM_BYTES: usize = 4096;

/// The executing runtime binary inside an explicit runtime root.
pub fn runtime_binary(runtime_root: &Path) -> PathBuf {
    runtime_root.join(installation::RELEASE_BINARY)
}
/// Baseline `artifact.inspect(artifact.PACK)`: the executing runtime measured
/// from its explicit root (pack plus `bin/am`), never from ambient state.
pub fn measured_runtime(runtime_root: &Path) -> Result<Value> {
    Ok(artifact::inspect_runtime(
        &runtime_root.join(installation::RELEASE_PACK),
        &runtime_binary(runtime_root),
    )?)
}
/// `'layout-frontier-v1' in runtime['compatibility'].get('private_state_capabilities', [])`.
pub fn frontier_capable(runtime: &Value) -> bool {
    get(get(runtime, "compatibility"), "private_state_capabilities")
        .as_array()
        .is_some_and(|tokens| {
            tokens
                .iter()
                .any(|token| token.as_str() == Some("layout-frontier-v1"))
        })
}

fn pending() -> ContractError {
    err(PENDING)
}

fn declared(value: &Value, name: &str) -> bool {
    let Some(values) = value.get(name).and_then(Value::as_array) else {
        return false;
    };
    let mut seen = BTreeSet::new();
    (name == "stopped_sessions" || !values.is_empty())
        && values.len() <= MAX_DECLARED
        && values.iter().all(|item| {
            item.as_str().is_some_and(|text| {
                !text.trim().is_empty()
                    && text.len() <= MAX_DECLARED_ITEM_BYTES
                    && seen.insert(text.to_owned())
            })
        })
}

/// Validate one attestation against its manifest and measured runtime.
pub fn attestation_value(value: &Value, manifest: &Value, runtime: &Value) -> Result<Value> {
    let scope = field(field(manifest, "identity")?, "scope_context")?;
    let matches = |key: &str, expected: &Value| value.get(key).is_some_and(|v| same(v, expected));
    if !has_keys(value, ATTESTATION_FIELDS)
        || get(value, "schema").as_str() != Some(ATTESTATION_SCHEMA)
        || !uuid(get(value, "attestation_id"))
        || !uuid(get(value, "review_id"))
        || !matches("operation_id", field(manifest, "operation_id")?)
        || !matches("project_id", field(manifest, "project_id")?)
        || !matches("repo_root", field(manifest, "repo_root")?)
        || !matches("bank_path", field(manifest, "bank_path")?)
        || !matches("checkout_key", field(scope, "checkout_key")?)
        || get(value, "manifest_sha256").as_str() != Some(sha(manifest)?.as_str())
        || !matches("runtime_artifact_digest", get(runtime, "artifact_digest"))
        || !matches("operator_developer_id", field(scope, "developer_id")?)
        || get(value, "statement").as_str() != Some(ATTESTATION_STATEMENT)
        || !frontier_capable(runtime)
    {
        return Err(pending());
    }
    let (operations, _) = writes::state_paths(
        Path::new(text(manifest, "repo_root")?),
        Path::new(text(manifest, "bank_path")?),
        text(manifest, "operation_id")?,
    )?;
    let state_root = operations.ancestors().nth(3).ok_or_else(pending)?;
    if get(value, "state_root").as_str() != Some(host::text(state_root.as_os_str(), "state root")?)
    {
        return Err(pending());
    }
    for name in [
        "declared_principals",
        "declared_invocation_routes",
        "stopped_sessions",
    ] {
        if !declared(value, name) {
            return Err(pending());
        }
    }
    lifecycle::time_value(get(value, "issued_at"))?;
    Ok(value.clone())
}

/// Baseline `selected_attestation(repo, operation_id, proposal, runtime_digest)`.
pub fn selected_attestation(
    repo: &Path,
    operation_id: &str,
    proposal: &Value,
    runtime_digest: &str,
    runtime_root: &Path,
) -> Result<Option<Value>> {
    let (_, _, _, directory) = state::paths(repo, Some(operation_id), runtime_root)?;
    let directory = directory.ok_or_else(|| err(crate::layout_resolution::MALFORMED))?;
    selected_at(&directory, proposal, runtime_digest)
}

/// Baseline `_selected_attestation(directory, proposal, runtime_digest)`.
pub fn selected_at(
    directory: &Path,
    proposal: &Value,
    runtime_digest: &str,
) -> Result<Option<Value>> {
    let heads = read_optional(&directory.join("quiescence-attestations.json"))?
        .unwrap_or(Value::Object(Vec::new()));
    let Some(row) = heads.get(runtime_digest) else {
        let attestation = get(proposal, "quiescence_attestation");
        let review = get(proposal, "review");
        if attestation.as_object().is_none()
            || get(attestation, "runtime_artifact_digest").as_str() != Some(runtime_digest)
            || get(proposal, "quiescence_attestation_sha256").as_str()
                != Some(sha(attestation)?.as_str())
            || review.as_object().is_none()
            || get(proposal, "review_sha256").as_str() != Some(sha(review)?.as_str())
            || !same(get(review, "review_id"), get(attestation, "review_id"))
            || get(review, "quiescence_attestation_sha256").as_str()
                != Some(sha(attestation)?.as_str())
        {
            return Ok(None);
        }
        return Ok(Some(attestation.clone()));
    };
    let attestation = read_value(
        &directory
            .join("attestations")
            .join(format!("{}.json", text(row, "attestation_id")?)),
    )?;
    let review = read_value(
        &directory
            .join("quiescence-reviews")
            .join(format!("{}.json", text(row, "review_id")?)),
    )?;
    if get(row, "sha256").as_str() != Some(sha(&attestation)?.as_str())
        || get(row, "review_sha256").as_str() != Some(sha(&review)?.as_str())
        || !same(field(&review, "attestation_sha256")?, field(row, "sha256")?)
        || field(&review, "decision")?.as_str() != Some("approve")
        || field(&attestation, "runtime_artifact_digest")?.as_str() != Some(runtime_digest)
    {
        return Err(err(
            "Renewed runtime quiescence attestation is missing, corrupt or mismatched",
        ));
    }
    Ok(Some(attestation))
}

/// A caller supplies a measured target digest, never a boolean bypass.
#[allow(clippy::too_many_arguments)]
pub fn validate(
    repo: &Path,
    value: &Value,
    runtime_digest: Option<&str>,
    required_routes: &[String],
    publish: bool,
    owner: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, _, _) = state::paths(repo, None, runtime_root)?;
    if !has_keys(value, &["attestation", "review"])
        || get(value, "attestation").as_object().is_none()
    {
        return Err(err(
            "New invocation routes require the exact private attestation and independent review envelope",
        ));
    }
    let attestation = get(value, "attestation");
    let review = get(value, "review");
    let operation_id = get(attestation, "operation_id").as_str();
    let indexed = state::_index_for_bank(&root, &bank, runtime_root)?;
    let row = operation_id.and_then(|id| get(&indexed, "operations").get(id));
    let (Some(operation_id), Some(row)) = (
        operation_id,
        row.filter(|row| get(row, "state").as_str() == Some("active")),
    ) else {
        return Err(err(
            "Quiescence renewal must bind the actual active recovery operation",
        ));
    };
    let frontier = frontier()?;
    let (directory, _, current) = frontier.ledger(&root, operation_id, row, &bank, runtime_root)?;
    let (manifest, proposal) = frontier.evidence(&directory, &current, row, runtime_root)?;
    let mut runtime = measured_runtime(runtime_root)?;
    set(
        &mut runtime,
        "artifact_digest",
        runtime_digest.map_or(Value::Null, string),
    );
    attestation_value(attestation, &manifest, &runtime)?;
    let declared: BTreeSet<&str> = get(attestation, "declared_invocation_routes")
        .as_array()
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if required_routes
        .iter()
        .any(|route| !declared.contains(route.as_str()))
    {
        return Err(err(
            "Renewed quiescence omits a newly enabled same-bank invocation route",
        ));
    }
    if !has_keys(review, REVIEW_FIELDS)
        || get(review, "schema").as_str() != Some(REVIEW_SCHEMA)
        || !same(get(review, "review_id"), get(attestation, "review_id"))
        || !uuid(get(review, "review_id"))
        || !uuid(get(review, "reviewer_developer_id"))
        || get(review, "operation_id").as_str() != Some(operation_id)
        || get(review, "attestation_sha256").as_str() != Some(sha(attestation)?.as_str())
        || get(review, "decision").as_str() != Some("approve")
    {
        return Err(err(
            "Quiescence renewal needs retained independent review of the exact complete declaration",
        ));
    }
    let author = get(&proposal, "author");
    if same(get(review, "reviewer_harness"), get(author, "harness"))
        && same(get(review, "reviewer_session"), get(author, "session"))
    {
        return Err(err(
            "Quiescence review must come from a different actual agent/session",
        ));
    }
    lifecycle::time_value(get(review, "review_time"))?;
    safe_text(get(review, "rationale"), 4096)?;
    safe_text(get(review, "reviewer_harness"), 256)?;
    safe_text(get(review, "reviewer_session"), 256)?;
    // Bank location is fresh at entry and frontier.check below freshly resolves
    // it again. Do not carry this read-only path observation across publication.
    let live = frontier.observe_bank(&root, &bank, owner, None, true, runtime_root)?;
    let marker = frontier.marker_bytes(&bank, crate::bank_transactions::MARKER, None)?;
    let marker_value = match &marker {
        Some(raw) => Some(frontier.marker_value(raw)?),
        None => None,
    };
    if !marker_value.as_ref().is_some_and(|value| {
        get(value, "schema").as_str() == Some(writes::HOLD)
            && get(value, "operation_id").as_str() == Some(operation_id)
    }) {
        return Err(err(
            "Quiescence renewal requires the exact persistent hold, not an interrupted transaction",
        ));
    }
    bank_observation::markers(&bank, owner, marker.as_deref())?;
    let scope = field(field(&manifest, "identity")?, "scope_context")?;
    scope_binding::require(
        &root,
        Some(text(&manifest, "project_id")?),
        RequireOptions {
            expected: Some(scope),
            initialize: false,
            owned_lock: owner,
            owned_transaction: marker.as_deref(),
            migration_reader: None,
        },
    )?;
    let executing = measured_runtime(runtime_root)?;
    let executing_digest = text(&executing, "artifact_digest")?;
    if selected_at(&directory, &proposal, executing_digest)?.is_none()
        && Some(executing_digest) != runtime_digest
    {
        return Err(err(
            "The executing verifier is neither the reviewed existing runtime nor the reviewed replacement",
        ));
    }
    frontier.check(&live, &root, owner, None)?;
    if publish {
        if owner.is_none() {
            let id = host::uuid4()?;
            return locked(&root, &bank, &id, |owned| {
                validate(
                    &root,
                    value,
                    runtime_digest,
                    required_routes,
                    true,
                    Some(owned),
                    runtime_root,
                )
            });
        }
        state::write_once(
            &directory
                .join("attestations")
                .join(format!("{}.json", text(attestation, "attestation_id")?)),
            attestation,
        )?;
        state::write_once(
            &directory
                .join("quiescence-reviews")
                .join(format!("{}.json", text(review, "review_id")?)),
            review,
        )?;
        let heads_path = directory.join("quiescence-attestations.json");
        let mut heads = read_optional(&heads_path)?.unwrap_or(Value::Object(Vec::new()));
        let old = crate::developer_identity::digest(
            state::read_bytes(&heads_path, sharing_limits::MAX_PRIVATE, true)?.as_deref(),
        );
        let next_head = object([
            ("attestation_id", get(attestation, "attestation_id").clone()),
            ("sha256", string(sha(attestation)?)),
            ("review_id", get(review, "review_id").clone()),
            ("review_sha256", string(sha(review)?)),
        ]);
        // json.dumps renders a None key as "null".
        let key = runtime_digest.unwrap_or("null");
        if !heads.get(key).is_some_and(|head| same(head, &next_head)) {
            if heads.as_object().is_none() {
                return Err(err(crate::layout_resolution::MALFORMED));
            }
            set(&mut heads, key, next_head);
            state::replace(&heads_path, &heads, old.as_deref())?;
            frontier.invalidate(&root)?;
        }
    }
    Ok(object([
        ("operation_id", string(operation_id)),
        (
            "runtime_artifact_digest",
            runtime_digest.map_or(Value::Null, string),
        ),
        ("attestation_sha256", string(sha(attestation)?)),
        (
            "invocation_routes",
            get(attestation, "declared_invocation_routes").clone(),
        ),
    ]))
}

/// Production `private_state::RecoveryQuiescence` provider.
pub struct RecoveryQuiescence;
impl crate::private_state::RecoveryQuiescence for RecoveryQuiescence {
    fn selected_attestation(
        &self,
        directory: &Path,
        proposal: &Value,
        runtime_digest: &str,
        _runtime_root: &Path,
    ) -> Result<Option<Value>> {
        selected_at(directory, proposal, runtime_digest)
    }
    fn validate(
        &self,
        repo: &Path,
        value: &Value,
        runtime_digest: Option<&str>,
        required_routes: &[String],
        publish: bool,
        owner: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Value> {
        validate(
            repo,
            value,
            runtime_digest,
            required_routes,
            publish,
            owner,
            runtime_root,
        )
    }
}
