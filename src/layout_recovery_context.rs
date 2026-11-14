//! Internal recovery authorization: exact private proofs plus live read-set pins.
use crate::codec::json::Value;
use crate::error::Result;
use crate::layout_resolution::{
    digest, err, field, frontier, has_keys, items, operation_paths, read_optional, read_raw,
    read_value, same, sha, text,
};
use crate::layout_restore::{
    self, LayoutRecovery, RECOVERY_JOURNAL, RecoverySource, layout_recovery_provider,
};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_observation, bank_writes as writes, layout_recovery_state as state, recovery_quiescence,
    sharing_limits,
};
use std::path::{Path, PathBuf};

const READ_SET: &str = "agent-memory/layout-recovery-read-set-v1";
const READ_SET_FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "project_id",
    "proposal_sha256",
    "manifest_sha256",
    "private_sha256",
    "runtime_artifact_digest",
    "frontier",
];

/// Python truthiness of a decoded JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Int(number)) => number.as_str() != "0",
        Some(Value::Float(number)) => *number != 0.0,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(values)) => !values.is_empty(),
        Some(Value::Object(pairs)) => !pairs.is_empty(),
    }
}

/// Measure the executing runtime and require an attestation for inherited work.
pub fn runtime_guard(
    repo: &Path,
    manifest: &Value,
    proposal: &Value,
    runtime_root: &Path,
) -> Result<String> {
    let measured = recovery_quiescence::measured_runtime(runtime_root)?;
    if !recovery_quiescence::frontier_capable(&measured) {
        return Err(err("pending-runtime-compatibility"));
    }
    let measured_digest = text(&measured, "artifact_digest")?;
    if truthy(proposal.get("pending_units")) || truthy(proposal.get("inherited_unit_ids")) {
        let attestation = recovery_quiescence::selected_attestation(
            repo,
            text(proposal, "operation_id")?,
            proposal,
            measured_digest,
            runtime_root,
        )?
        .ok_or_else(|| err("pending-runtime-compatibility"))?;
        recovery_quiescence::attestation_value(&attestation, manifest, &measured)?;
    }
    Ok(measured_digest.to_owned())
}

fn pin_json(
    pins: &mut Vec<Value>,
    operation_id: &str,
    relative: String,
    value: &Value,
) -> Result<()> {
    pins.push(object([
        ("operation_id", string(operation_id)),
        ("path", string(relative)),
        ("sha256", string(sha(value)?)),
    ]));
    Ok(())
}

/// Private proof pins plus the measured runtime digest of one recovery.
pub fn evidence(
    repo: &Path,
    manifest: &Value,
    proposal: &Value,
    owner: Option<&[u8]>,
    transaction: &[u8],
    runtime_root: &Path,
) -> Result<(Vec<Value>, String)> {
    let recovery = layout_recovery_provider()?;
    let frontier = frontier()?;
    let marker = frontier.marker_value(transaction)?;
    let operation_id = text(proposal, "operation_id")?;
    if get(&marker, "schema").as_str() != Some(writes::TRANSACTION)
        || get(&marker, "operation_id").as_str() != Some(operation_id)
        || !same(
            get(manifest, "operation_id"),
            field(proposal, "operation_id")?,
        )
        || !same(
            field(&marker, "project_id")?,
            field(manifest, "project_id")?,
        )
    {
        return Err(err(
            "Recovery observer requires the exact internal typed transaction",
        ));
    }
    let (root, _, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let journal = read_value(&directory.join("journal.json"))?;
    let proposal_id = text(proposal, "proposal_id")?;
    let proposal_file = format!("proposals/{proposal_id}.json");
    let retained = read_value(&directory.join(&proposal_file))?;
    if get(&journal, "schema").as_str() != Some(RECOVERY_JOURNAL)
        || get(&journal, "operation_id").as_str() != Some(operation_id)
        || get(&journal, "proposal_sha256").as_str() != Some(sha(proposal)?.as_str())
        || get(&journal, "manifest_sha256").as_str() != Some(sha(manifest)?.as_str())
        || !same(&retained, proposal)
    {
        return Err(err("Recovery private journal/proposal/manifest changed"));
    }
    let mut pins = Vec::new();
    pin_json(&mut pins, operation_id, "manifest.json".into(), manifest)?;
    pin_json(&mut pins, operation_id, proposal_file, proposal)?;
    pin_json(&mut pins, operation_id, "journal.json".into(), &journal)?;
    let mut originals: Vec<(String, Value)> = vec![(operation_id.to_owned(), manifest.clone())];
    let (guard_manifest, guard_proposal) =
        if get(proposal, "schema").as_str() == Some(layout_restore::SCHEMA) {
            layout_restore::value(&root, proposal, runtime_root)?;
            let RecoverySource {
                operation,
                proposal: parent,
                ..
            } = recovery.recovery_source(
                &root,
                text(proposal, "parent_operation_id")?,
                runtime_root,
            )?;
            let parent_dir = operation.directory;
            let parent_manifest = operation.manifest;
            let receipt = recovery
                .receipt_value(&parent_dir, &parent)?
                .unwrap_or(Value::Null);
            let head = read_value(&parent_dir.join("ledger-head.json"))?;
            if get(proposal, "parent_proposal_sha256").as_str() != Some(sha(&parent)?.as_str())
                || get(proposal, "parent_receipt_sha256").as_str() != Some(sha(&receipt)?.as_str())
                || get(proposal, "parent_ledger_head_sha256").as_str() != Some(sha(&head)?.as_str())
            {
                return Err(err("Child restore parent evidence/dependency changed"));
            }
            let parent_id = text(&parent, "operation_id")?;
            let parent_proposal = format!("proposals/{}.json", text(&parent, "proposal_id")?);
            pin_json(
                &mut pins,
                parent_id,
                "manifest.json".into(),
                &parent_manifest,
            )?;
            pin_json(&mut pins, parent_id, parent_proposal, &parent)?;
            pin_json(&mut pins, parent_id, "receipt.json".into(), &receipt)?;
            pin_json(&mut pins, parent_id, "ledger-head.json".into(), &head)?;
            originals.push((parent_id.to_owned(), parent_manifest.clone()));
            let actual = frontier.observe_owned(&root, owner, Some(transaction))?;
            let pinned = field(proposal, "frontier")?;
            let hidden = [
                "ledger_heads",
                "unit_ids",
                "reference_map_sha256",
                "frontier_digest",
            ]
            .iter()
            .any(|key| !same(get(&actual, key), get(pinned, key)) || pinned.get(key).is_none());
            if hidden {
                return Err(err(
                    "Child restore would hide a later same-bank recovery dependency",
                ));
            }
            (parent_manifest, parent)
        } else {
            recovery.proposal_value(&root, proposal, runtime_root)?;
            let review_file = format!("reviews/{}.json", text(proposal, "review_id")?);
            let review = read_value(&directory.join(&review_file))?;
            let unit_map = read_value(&directory.join("unit-map.json"))?;
            if get(proposal, "review_sha256").as_str() != Some(sha(&review)?.as_str())
                || get(proposal, "unit_map_sha256").as_str() != Some(sha(&unit_map)?.as_str())
            {
                return Err(err("Recovery independent review/unit map changed"));
            }
            pin_json(&mut pins, operation_id, review_file, &review)?;
            pin_json(&mut pins, operation_id, "unit-map.json".into(), &unit_map)?;
            let head = read_value(&directory.join("ledger-head.json"))?;
            if crate::layout_resolution::int(get(&head, "generation")) != Some(0)
                || !same(
                    field(&head, "sha256")?,
                    field(&journal, "initial_ledger_sha256")?,
                )
            {
                return Err(err(
                    "Layout recovery cannot replay over a later ledger generation",
                ));
            }
            (manifest.clone(), proposal.clone())
        };
    for (operation, original) in &originals {
        for source in items(field(original, "sources")?)? {
            pins.push(object([
                ("operation_id", string(operation.as_str())),
                ("path", field(source, "blob")?.clone()),
                ("sha256", field(source, "sha256")?.clone()),
            ]));
        }
    }
    let runtime = runtime_guard(&root, &guard_manifest, &guard_proposal, runtime_root)?;
    Ok((pins, runtime))
}

/// Not serializable and never accepted from a proposal or ordinary writer:
/// only `observe` constructs it, from actual locked observations.
pub struct RecoveryReadSet {
    root: PathBuf,
    manifest: Value,
    proposal: Value,
    owner: Option<Vec<u8>>,
    transaction: Vec<u8>,
    context: Value,
    context_bytes: Vec<u8>,
}

impl RecoveryReadSet {
    pub fn observe(
        repo: &Path,
        manifest: &Value,
        proposal: &Value,
        owner: Option<&[u8]>,
        transaction: &[u8],
        runtime_root: &Path,
    ) -> Result<Self> {
        let recovery: std::sync::Arc<dyn LayoutRecovery> = layout_recovery_provider()?;
        let operation_id = text(proposal, "operation_id")?;
        let (root, bank, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
        bank_observation::markers(&bank, owner, Some(transaction))?;
        let (pins, runtime) =
            evidence(&root, manifest, proposal, owner, transaction, runtime_root)?;
        let context_path = directory.join("source-context.json");
        let context = match read_optional(&context_path)? {
            Some(context) => context,
            None => {
                let current = recovery.view(&bank, &[])?;
                let boundary = if get(proposal, "schema").as_str() == Some(layout_restore::SCHEMA) {
                    layout_restore::boundary(proposal, &current, None)?
                } else {
                    recovery.partial_boundary(proposal, manifest, &current)?
                };
                if boundary != 0 {
                    return Err(err(
                        "Missing original recovery read-set evidence after canonical mutation",
                    ));
                }
                let context = object([
                    ("schema", string(READ_SET)),
                    ("operation_id", string(operation_id)),
                    ("project_id", field(proposal, "project_id")?.clone()),
                    ("proposal_sha256", string(sha(proposal)?)),
                    ("manifest_sha256", string(sha(manifest)?)),
                    ("private_sha256", Value::Array(pins.clone())),
                    ("runtime_artifact_digest", string(runtime.as_str())),
                    (
                        "frontier",
                        frontier()?.observe_owned(&root, owner, Some(transaction))?,
                    ),
                ]);
                state::write_once(&context_path, &context)?;
                context
            }
        };
        if !has_keys(&context, READ_SET_FIELDS)
            || get(&context, "schema").as_str() != Some(READ_SET)
            || get(&context, "operation_id").as_str() != Some(operation_id)
            || !same(get(&context, "project_id"), get(proposal, "project_id"))
            || get(&context, "proposal_sha256").as_str() != Some(sha(proposal)?.as_str())
            || get(&context, "manifest_sha256").as_str() != Some(sha(manifest)?.as_str())
            || !same(get(&context, "private_sha256"), &Value::Array(pins))
            || get(&context, "runtime_artifact_digest").as_str() != Some(runtime.as_str())
        {
            return Err(err(
                "Recovery read-set evidence changed or belongs to another operation",
            ));
        }
        let context_bytes = crate::developer_identity::encoded(&context)?;
        let result = Self {
            root,
            manifest: manifest.clone(),
            proposal: proposal.clone(),
            owner: owner.map(<[u8]>::to_vec),
            transaction: transaction.to_vec(),
            context,
            context_bytes,
        };
        result.check(runtime_root)?;
        Ok(result)
    }

    /// Recheck every pinned proof, the runtime, the frontier and the bank binding.
    pub fn check(&self, runtime_root: &Path) -> Result<bool> {
        let recovery = layout_recovery_provider()?;
        let operation_id = text(&self.proposal, "operation_id")?;
        let (_, bank, _, directory) = operation_paths(&self.root, operation_id, runtime_root)?;
        let owner = self.owner.as_deref();
        let transaction = Some(self.transaction.as_slice());
        bank_observation::markers(&bank, owner, transaction)?;
        if read_raw(
            &directory.join("source-context.json"),
            sharing_limits::MAX_PRIVATE,
        )? != self.context_bytes
        {
            return Err(err("Recovery source-context journal changed"));
        }
        let (pins, runtime) = evidence(
            &self.root,
            &self.manifest,
            &self.proposal,
            owner,
            &self.transaction,
            runtime_root,
        )?;
        if !same(
            &Value::Array(pins.clone()),
            field(&self.context, "private_sha256")?,
        ) || get(&self.context, "runtime_artifact_digest").as_str() != Some(runtime.as_str())
        {
            return Err(err("Recovery proof/runtime pins changed"));
        }
        for pin in &pins {
            let (_, _, _, pinned) =
                operation_paths(&self.root, text(pin, "operation_id")?, runtime_root)?;
            let location = pinned.join(text(pin, "path")?);
            let actual = digest(&read_raw(&location, sharing_limits::MAX_PRIVATE)?);
            if get(pin, "sha256").as_str() != Some(actual.as_str()) {
                return Err(err("Recovery original/proposal/review evidence changed"));
            }
        }
        frontier()?.check(
            field(&self.context, "frontier")?,
            &self.root,
            owner,
            transaction,
        )?;
        recovery.rebind(&self.root, &self.manifest, owner, transaction, runtime_root)?;
        Ok(true)
    }
}
