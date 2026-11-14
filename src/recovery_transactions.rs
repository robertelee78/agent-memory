//! Typed held transactions; ordinary legacy recovery never consumes these journals.
use crate::codec::json::Value;
use crate::developer_identity::{encoded, sha256_hex as digest, uuid_valid};
use crate::error::Result;
use crate::host::{self, Sources};
use crate::layout_resolution::{
    self as resolution, err, field, has_keys, items, same, set, sha, text, with,
};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_contract, bank_observation, bank_transactions as tx, bank_writes as writes, checkpoint,
    layout_recovery_state as state, local_registry as storage,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
const MAX_PRIVATE: usize = crate::sharing_limits::MAX_PRIVATE;
fn read(path: &Path) -> Result<Value> {
    state::read(path, MAX_PRIVATE, false)?.ok_or_else(|| err(resolution::MALFORMED))
}
fn optional(path: &Path) -> Result<Option<Value>> {
    state::read(path, MAX_PRIVATE, true)
}
fn directory(root: &Path, operation: &str, runtime: &Path) -> Result<PathBuf> {
    state::paths(root, Some(operation), runtime)?
        .3
        .ok_or_else(|| err(resolution::MALFORMED))
}
fn map(value: &Value) -> Result<&[(String, Value)]> {
    value.as_object().ok_or_else(|| err(resolution::MALFORMED))
}
fn remove(value: &mut Value, key: &str) {
    if let Value::Object(rows) = value {
        rows.retain(|(name, _)| name != key);
    }
}
fn row<'a>(indexed: &'a Value, operation: &str) -> Result<&'a Value> {
    field(field(indexed, "operations")?, operation)
}
fn ordered(mut paths: Vec<String>) -> Vec<String> {
    paths.sort_by(|a, b| (a == "MEMORY.md", a).cmp(&(b == "MEMORY.md", b)));
    paths
}
fn participants(p: &Value) -> Result<BTreeSet<String>> {
    Ok(map(field(p, "observed_hashes")?)?
        .iter()
        .chain(map(field(p, "files")?)?)
        .map(|(k, _)| k.clone())
        .collect())
}
fn receipt_fields(receipt: &Value) -> Value {
    let mut v = receipt.clone();
    remove(&mut v, "schema");
    v
}
fn resolution_id(nested: &str, unit: &str) -> Result<String> {
    let hex = sha(&Value::Array(vec![string(nested), string(unit)]))?;
    let mut bytes = hex.bytes().take(32).collect::<Vec<_>>();
    // The digest is always exactly 64 ASCII hexadecimal bytes.
    if bytes.len() != 32 {
        return Err(err("Invalid resolution digest"));
    }
    if let Some(version) = bytes.get_mut(12) {
        *version = b'4';
    }
    if let Some(variant) = bytes.get_mut(16) {
        *variant = match *variant {
            b'0' | b'4' | b'8' | b'c' => b'8',
            b'1' | b'5' | b'9' | b'd' => b'9',
            b'2' | b'6' | b'a' | b'e' => b'a',
            _ => b'b',
        };
    }
    let mut result = String::with_capacity(36);
    for (index, byte) in bytes.into_iter().enumerate() {
        if matches!(index, 8 | 12 | 16 | 20) {
            result.push('-');
        }
        result.push(char::from(byte));
    }
    Ok(result)
}
pub fn marker_for(
    held: &crate::scope_binding::VerifiedEnrollmentHold,
    nested_id: &str,
) -> Result<Value> {
    let original = writes::recovery_frontier_provider()?.marker_value(held.marker())?;
    Ok(object([
        ("schema", string(writes::NESTED)),
        ("project_id", field(&original, "project_id")?.clone()),
        ("operation_id", field(&original, "operation_id")?.clone()),
        ("hold_id", field(&original, "hold_id")?.clone()),
        ("nested_id", string(nested_id)),
    ]))
}
pub fn source_check(p: &Value, owner: &[u8], marker: &[u8]) -> Result<()> {
    let (root, bank) = writes::bank_path(Path::new(text(p, "repo_root")?))?;
    tx::validate_outcome(&root, &bank, field(p, "files")?, p, owner, marker, true)?;
    if let Some((_, proof, _)) = resolution::transaction_evidence(p)? {
        writes::recovery_frontier_provider()?.check(
            field(
                field(field(proof, "judgment")?, "observation")?,
                "recovery_frontier",
            )?,
            &root,
            Some(owner),
            Some(marker),
        )?;
    }
    Ok(())
}
pub fn validate(p: &Value, owner: &[u8], runtime: &Path) -> Result<(Vec<Value>, Option<Value>)> {
    if p.get("checkpoint_evidence").is_some() {
        checkpoint::validate_preparation(p, true)?;
        return Ok((Vec::new(), None));
    }
    if p.get("admission_evidence").is_some() {
        crate::admission::validate_transaction(p, Some(owner))?;
    } else if p.get("governance_evidence").is_some() {
        crate::governance::validate_transaction(p, Some(owner))?;
    } else if p.get("consolidation_evidence").is_some() {
        crate::consolidation::validate_transaction(p, Some(owner))?;
    } else {
        return Err(err(
            "Recovery hold permits only fully evidenced governed learning, not raw replacement",
        ));
    }
    resolution::claims_for(p, Some(owner), None, runtime)
}
pub fn credited_rows(
    repo: &Path,
    token: &Value,
    claims: &[Value],
    owner: Option<&[u8]>,
    marker: Option<&[u8]>,
    runtime: &Path,
) -> Result<Vec<Value>> {
    let full: BTreeSet<_> = claims
        .iter()
        .filter(|c| get(c, "coverage").as_str() == Some("full"))
        .map(|c| text(c, "unit_id"))
        .collect::<Result<_>>()?;
    Ok(writes::recovery_frontier_provider()?
        .pending_rows(repo, token, owner, marker, runtime)?
        .into_iter()
        .filter(|r| {
            !get(r, "unit_id")
                .as_str()
                .is_some_and(|id| full.contains(id))
        })
        .collect())
}
pub fn resolution_plan(
    p: &Value,
    claims: &[Value],
    review: Option<&Value>,
    runtime: &Path,
) -> Result<Value> {
    let root = Path::new(text(p, "repo_root")?);
    let (_, bank) = writes::bank_path(root)?;
    let indexed = state::index(root, runtime)?;
    let frontier = writes::recovery_frontier_provider()?;
    let mut remaining: BTreeMap<_, _> = claims
        .iter()
        .filter(|c| get(c, "coverage").as_str() != Some("none"))
        .map(|c| Ok((text(c, "unit_id")?.to_owned(), c)))
        .collect::<Result<_>>()?;
    let mut operations: Vec<_> = map(field(&indexed, "operations")?)?.iter().collect();
    operations.sort_by(|a, b| a.0.cmp(&b.0));
    let mut plans = Vec::new();
    for (operation, row) in operations {
        if !matches!(get(row, "state").as_str(), Some("active" | "rolled-back")) {
            continue;
        }
        let (_, head, current) = frontier.ledger(root, operation, row, &bank, runtime)?;
        let units = field(&current, "units")?;
        let mut resolutions = Vec::new();
        for unit in items(units)? {
            let Some(claim) = remaining.remove(text(unit, "unit_id")?) else {
                continue;
            };
            if get(unit, "state").as_str() != Some("pending-admission") {
                return Err(err(
                    "A coverage claim cannot reterminalize a historical unit",
                ));
            }
            let terminal = if get(claim, "coverage").as_str() == Some("full") {
                if p.get("admission_evidence").is_some() {
                    "admitted"
                } else {
                    "governed-terminal"
                }
            } else {
                "pending-admission"
            };
            resolutions.push(object([
                (
                    "resolution_id",
                    string(resolution_id(
                        text(p, "operation_id")?,
                        text(unit, "unit_id")?,
                    )?),
                ),
                ("unit_id", field(unit, "unit_id")?.clone()),
                ("claim", claim.clone()),
                ("terminal_state", string(terminal)),
                (
                    "review_sha256",
                    string(sha(review.unwrap_or(&Value::Null))?),
                ),
            ]));
        }
        if !resolutions.is_empty() {
            plans.push(object([
                ("operation_id", string(operation)),
                ("previous_head", head),
                ("units", units.clone()),
                ("resolutions", Value::Array(resolutions)),
            ]));
        }
    }
    if !remaining.is_empty() {
        return Err(err("Coverage claim omitted its exact retained ledger"));
    }
    Ok(object([("ledgers", Value::Array(plans))]))
}
pub fn planned_resolution(
    plan: &Value,
    journal: &Value,
    receipt: &Value,
    nested_id: &str,
    runtime: &Path,
) -> Result<(Value, Vec<Value>)> {
    let p = field(journal, "proposal")?;
    let operation = text(plan, "operation_id")?;
    let root = Path::new(text(p, "repo_root")?);
    let head = field(plan, "previous_head")?;
    let generation = field(head, "generation")?
        .as_int()
        .and_then(|n| n.as_u64())
        .ok_or_else(|| err(resolution::MALFORMED))?;
    let previous = read(
        &directory(root, operation, runtime)?
            .join("ledger")
            .join(format!("{generation:08}.json")),
    )?;
    let head_ref = object([
        ("operation_id", string(operation)),
        ("generation", field(head, "generation")?.clone()),
        ("sha256", field(head, "sha256")?.clone()),
    ]);
    if get(head, "sha256").as_str() != Some(sha(&previous)?.as_str())
        || !same(field(&previous, "units")?, field(plan, "units")?)
        || !items(field(field(journal, "frontier")?, "ledger_heads")?)?
            .iter()
            .any(|h| same(h, &head_ref))
    {
        return Err(err(
            "Nested resolution plan changed its exact original ledger evidence",
        ));
    }
    let evidence = resolution::transaction_evidence(p)?;
    let claims = evidence
        .map(|(e, _, _)| get(e, "unit_resolution_claims"))
        .unwrap_or(&Value::Null);
    let mut units = field(plan, "units")?.clone();
    let mut records = Vec::new();
    for item in items(field(plan, "resolutions")?)? {
        let claim = field(item, "claim")?;
        let terminal = if get(claim, "coverage").as_str() == Some("full") {
            if p.get("admission_evidence").is_some() {
                "admitted"
            } else {
                "governed-terminal"
            }
        } else {
            "pending-admission"
        };
        let proof = evidence.map(|(_, p, _)| p);
        let review = proof.map(|p| {
            get(
                get(get(get(p, "judgment"), "observation"), "recovery_frontier"),
                "review_digest",
            )
        });
        if text(item, "resolution_id")? != resolution_id(nested_id, text(item, "unit_id")?)?
            || !items(claims)?.iter().any(|c| same(c, claim))
            || get(claim, "unit_id") != get(item, "unit_id")
            || get(claim, "coverage").as_str() == Some("none")
            || get(item, "terminal_state").as_str() != Some(terminal)
            || proof.is_none()
            || review != Some(get(item, "review_sha256"))
        {
            return Err(err(
                "Nested resolution lost its exact proof-bound claim and independent review",
            ));
        }
        let record = object([
            (
                "schema",
                string("agent-memory/layout-resolution-receipt-v1"),
            ),
            ("resolution_id", field(item, "resolution_id")?.clone()),
            ("operation_id", string(operation)),
            ("unit_id", field(item, "unit_id")?.clone()),
            ("generation", field(head, "generation")?.clone()),
            ("nested_id", string(nested_id)),
            ("request_sha256", string(sha(p)?)),
            ("canonical_operation_id", string(nested_id)),
            ("canonical_receipt_sha256", string(sha(receipt)?)),
            ("terminal_state", string(terminal)),
            ("claims", Value::Array(vec![claim.clone()])),
            (
                "independent_review_sha256",
                field(item, "review_sha256")?.clone(),
            ),
            ("verified", Value::Bool(true)),
        ]);
        let Value::Array(all) = &mut units else {
            return Err(err(resolution::MALFORMED));
        };
        let unit = all
            .iter_mut()
            .find(|u| get(u, "unit_id") == get(item, "unit_id"))
            .ok_or_else(|| err(resolution::MALFORMED))?;
        if get(unit, "state").as_str() != Some("pending-admission") {
            return Err(err(
                "Nested resolution cannot replace a historical terminal unit",
            ));
        }
        set(unit, "state", string(terminal));
        let reference = object([
            ("resolution_id", field(item, "resolution_id")?.clone()),
            ("sha256", string(sha(&record)?)),
        ]);
        let Some(Value::Array(receipts)) = unit.get_mut("terminal_receipts") else {
            return Err(err(resolution::MALFORMED));
        };
        receipts.push(reference);
        records.push(record);
    }
    Ok((units, records))
}
pub fn terminalize(
    root: &Path,
    operation: &str,
    nested: &str,
    journal: &Value,
    receipt: &Value,
    owner: &[u8],
    marker: &[u8],
    runtime: &Path,
) -> Result<()> {
    let dir = directory(root, operation, runtime)?;
    for plan in items(field(field(journal, "resolution_plan")?, "ledgers")?)? {
        let resolved = text(plan, "operation_id")?;
        let resolved_dir = directory(root, resolved, runtime)?;
        let (units, records) = planned_resolution(plan, journal, receipt, nested, runtime)?;
        for record in records {
            state::write_once(
                &resolved_dir
                    .join("resolutions")
                    .join(format!("{}.json", text(&record, "resolution_id")?)),
                &record,
            )?;
        }
        writes::fault("layout-recovery-admission-receipt-before-resolution")?;
        resolution::advance_ledger(
            root,
            resolved,
            &units,
            field(plan, "previous_head")?,
            Some(owner),
            Some(marker),
            None,
            runtime,
        )?;
    }
    let frontier = writes::recovery_frontier_provider()?;
    let token = frontier.observe_owned(root, Some(owner), Some(marker))?;
    let pending = frontier.pending_rows(root, &token, Some(owner), Some(marker), runtime)?;
    frontier.reservation(
        field(field(journal, "proposal")?, "files")?,
        &pending,
        &token,
    )?;
    let indexed = state::index(root, runtime)?;
    state::write_once(
        &dir.join("nested").join(nested).join("completion.json"),
        &object([
            ("schema", string("agent-memory/layout-nested-completion-v1")),
            ("operation_id", string(operation)),
            ("nested_id", string(nested)),
            ("canonical_receipt_sha256", string(sha(receipt)?)),
            (
                "ledger_head_sha256",
                field(row(&indexed, operation)?, "ledger_head_sha256")?.clone(),
            ),
            ("verified", Value::Bool(true)),
        ]),
    )?;
    if pending.is_empty() {
        resolution::retire(root, operation, Some(owner), Some(marker), runtime)?;
    } else {
        resolution::restore_hold(root, operation, Some(owner), Some(marker), runtime)?;
    }
    Ok(())
}
pub fn apply_transaction(prepared: &Value, runtime: &Path) -> Result<Value> {
    let p = tx::proposal_value(prepared)?;
    let (root, bank) = writes::bank_path(Path::new(text(p, "repo_root")?))?;
    let nested = text(p, "operation_id")?;
    let (_, receipt_path) = tx::locations(&root, &bank, nested)?;
    if let Some(known) = tx::receipt_value(&receipt_path, p)? {
        return tx::result(
            "apply",
            &with(
                receipt_fields(&known),
                [
                    ("writes_performed", Value::Bool(false)),
                    ("already_applied", Value::Bool(true)),
                    ("receipt_is_historical", Value::Bool(true)),
                    ("still_current", Value::Null),
                    ("receipt_path", string(receipt_path.to_string_lossy())),
                    (
                        "interrupted",
                        Value::Bool(
                            writes::recovery_frontier_provider()?
                                .marker_bytes(&bank, tx::MARKER, None)?
                                .is_some(),
                        ),
                    ),
                ],
            ),
        );
    }
    let mut barrier = crate::turn_sources::canonical_writer_barrier(&root, None, runtime)?;
    let result = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        nested,
        &format!("bank-{}", text(p, "operation")?),
        Sources::default(),
        |owner| {
            resolution::require_completed_resolutions(&root, None, runtime)?;
            let frontier = writes::recovery_frontier_provider()?;
            let held = frontier
                .verified_hold_value(&root, Some(owner), None, runtime)?
                .ok_or_else(|| {
                    err("Held transaction scope or complete canonical base changed after preview")
                })?;
            if !same(held.scope(), field(p, "scope_context")?)
                || !same(
                    &writes::snapshot(&root, Some(owner))?.1,
                    field(p, "observed_hashes")?,
                )
            {
                return Err(err(
                    "Held transaction scope or complete canonical base changed after preview",
                ));
            }
            let (claims, review) = validate(p, owner, runtime)?;
            let token = held.token();
            let remaining = credited_rows(&root, token, &claims, Some(owner), None, runtime)?;
            let plan = resolution_plan(p, &claims, review.as_ref(), runtime)?;
            let mut capacity = token.clone();
            let ids = remaining
                .iter()
                .map(|r| text(r, "unit_id").map(str::to_owned))
                .collect::<Result<Vec<_>>>()?;
            for ledger in items(field(&plan, "ledgers")?)? {
                capacity = frontier.successor_token(
                    &capacity,
                    &ids,
                    text(ledger, "operation_id")?,
                    true,
                )?;
            }
            frontier.reservation(field(p, "files")?, &remaining, &capacity)?;
            let base = tx::files_at(&bank)?;
            tx::preflight(&bank, p, None)?;
            let changes = tx::changed(p)?;
            let changed_files =
                object(changes.iter().filter_map(|path| {
                    get(p, "files").get(path).map(|v| (path.clone(), v.clone()))
                }));
            let classes = tx::source_classes(p)?;
            let class_refs = classes.iter().map(String::as_str).collect::<Vec<_>>();
            crate::security::guarded_start(
                &root,
                nested,
                held.scope(),
                &changed_files,
                &class_refs,
                text(p, "operation")?,
                false,
            )?;
            let marker = marker_for(&held, nested)?;
            let marker_raw = encoded(&marker)?;
            let dir = directory(&root, held.operation_id(), runtime)?
                .join("nested")
                .join(nested);
            let journal = object([
                ("schema", string("agent-memory/layout-nested-journal-v1")),
                ("operation_id", string(held.operation_id())),
                ("nested_id", string(nested)),
                ("marker", marker),
                ("hold", frontier.marker_value(held.marker())?),
                ("proposal", p.clone()),
                ("proposal_sha256", string(sha(p)?)),
                ("base_files", base),
                ("frontier", token.clone()),
                (
                    "resolution_plan",
                    resolution_plan(p, &claims, review.as_ref(), runtime)?,
                ),
            ]);
            state::write_once(&dir.join("journal.json"), &journal)?;
            writes::fault("layout-recovery-before-nested-marker")?;
            resolution::atomic_marker(&bank, Some(held.marker()), &marker_raw, Some(owner))?;
            writes::fault("layout-recovery-after-nested-marker")?;
            let mut running = field(p, "observed_hashes")?.clone();
            let paths = ordered(changes);
            let participants = participants(p)?;
            for (index, path) in paths.iter().enumerate() {
                let before = || frontier.check(token, &root, Some(owner), Some(&marker_raw));
                tx::replace_file(
                    &bank,
                    path,
                    get(p, "files").get(path).and_then(Value::as_str),
                    &running,
                    &marker_raw,
                    &participants,
                    None,
                    Some(&before),
                )?;
                if let Some(body) = get(p, "files").get(path) {
                    set(
                        &mut running,
                        path,
                        string(digest(
                            body.as_str()
                                .ok_or_else(|| err(resolution::MALFORMED))?
                                .as_bytes(),
                        )),
                    );
                } else {
                    remove(&mut running, path);
                }
                writes::fault(&format!("layout-recovery-nested-after-rename:{path}"))?;
                state::write_once(
                    &dir.join("progress").join(format!("{index:08}.json")),
                    &object([("path", string(path)), ("hashes", running.clone())]),
                )?;
            }
            source_check(p, owner, &marker_raw)?;
            writes::fault("layout-recovery-nested-before-receipt")?;
            let relative = bank
                .strip_prefix(&root)
                .map_err(|_| err(resolution::MALFORMED))?
                .to_string_lossy();
            let receipt = object([
                ("schema", string("agent-memory/transaction-receipt-v1")),
                ("operation_id", string(nested)),
                ("request_sha256", string(sha(p)?)),
                ("bank_path", string(bank.to_string_lossy())),
                ("project_id", field(p, "project_id")?.clone()),
                ("observed_hashes", field(p, "observed_hashes")?.clone()),
                ("resulting_hashes", tx::file_hashes(field(p, "files")?)?),
                ("outcome", string("finished")),
                (
                    "changed_memory_paths",
                    Value::Array(
                        tx::changed(p)?
                            .into_iter()
                            .map(|path| string(format!("{relative}/{path}")))
                            .collect(),
                    ),
                ),
                ("readback_at", string(storage::now(Sources::default())?)),
                ("verified", Value::Bool(true)),
            ]);
            writes::record(&receipt_path, &encoded(&receipt)?)?;
            writes::fault("layout-recovery-nested-after-receipt")?;
            terminalize(
                &root,
                held.operation_id(),
                nested,
                &journal,
                &receipt,
                owner,
                &marker_raw,
                runtime,
            )?;
            tx::result(
                "apply",
                &with(
                    receipt_fields(&receipt),
                    [
                        ("writes_performed", Value::Bool(!paths.is_empty())),
                        ("already_applied", Value::Bool(false)),
                        ("still_current", Value::Bool(true)),
                        ("receipt_path", string(receipt_path.to_string_lossy())),
                        ("interrupted", Value::Bool(false)),
                        ("resolved_scope", held.scope().clone()),
                    ],
                ),
            )
        },
    );
    let finished = barrier.finish();
    let value = result?;
    finished?;
    Ok(value)
}
pub fn legal_partial(p: &Value, base: &Value, observed: &Value) -> Result<()> {
    let final_hashes = tx::file_hashes(field(p, "files")?)?;
    let mut running = tx::file_hashes(base)?;
    if same(observed, &running) {
        return Ok(());
    }
    for path in ordered(tx::changed(p)?) {
        if let Some(hash) = final_hashes.get(&path) {
            set(&mut running, &path, hash.clone());
        } else {
            remove(&mut running, &path);
        }
        if same(observed, &running) {
            return Ok(());
        }
    }
    Err(err(
        "Nested recovery found an impossible ordered partial state or an external edit",
    ))
}
pub fn rollback_plan(dir: &Path, journal: &Value, observed: &Value) -> Result<(Value, usize)> {
    let known = optional(&dir.join("rollback-plan.json"))?;
    let initial = known
        .as_ref()
        .map(|p| field(p, "initial_hashes"))
        .transpose()?
        .unwrap_or(observed);
    legal_partial(
        field(journal, "proposal")?,
        field(journal, "base_files")?,
        initial,
    )?;
    let expected = tx::file_hashes(field(journal, "base_files")?)?;
    let names: BTreeSet<_> = map(initial)?
        .iter()
        .chain(map(&expected)?)
        .map(|(k, _)| k.clone())
        .collect();
    let actions = ordered(names.into_iter().collect())
        .into_iter()
        .filter(|name| get(initial, name) != get(&expected, name))
        .map(|name| {
            object([
                ("path", string(&name)),
                ("after", get(&expected, &name).clone()),
            ])
        })
        .collect::<Vec<_>>();
    let plan = object([
        ("schema", string("agent-memory/layout-nested-rollback-v1")),
        ("operation_id", field(journal, "operation_id")?.clone()),
        ("nested_id", field(journal, "nested_id")?.clone()),
        ("journal_sha256", string(sha(journal)?)),
        ("initial_hashes", initial.clone()),
        ("actions", Value::Array(actions.clone())),
    ]);
    if known.as_ref().is_some_and(|v| !same(v, &plan)) {
        return Err(err(
            "Nested rollback lost its exact durable original state/action plan",
        ));
    }
    let mut running = initial.clone();
    for (boundary, action) in actions
        .iter()
        .map(Some)
        .chain(std::iter::once(None))
        .enumerate()
    {
        if same(&running, observed) {
            return Ok((plan, boundary));
        }
        if let Some(action) = action {
            let path = text(action, "path")?;
            if get(action, "after") == &Value::Null {
                remove(&mut running, path);
            } else {
                set(&mut running, path, field(action, "after")?.clone());
            }
        }
    }
    Err(err(
        "Nested rollback found an impossible ordered boundary or external edit",
    ))
}
pub fn retired_nested(
    root: &Path,
    operation: &str,
    nested: &str,
    journal: &Value,
    receipt: Option<&Value>,
    indexed: &Value,
    owner: Option<&[u8]>,
    runtime: &Path,
) -> Result<Value> {
    let (_, bank) = writes::bank_path(root)?;
    let dir = directory(root, operation, runtime)?;
    let current_row = row(indexed, operation)?;
    let frontier = writes::recovery_frontier_provider()?;
    let (_, head, _) = frontier.ledger(root, operation, current_row, &bank, runtime)?;
    let retirement = read(&dir.join("retirement.json"))?;
    let canonical = read(&dir.join("receipt.json"))?;
    let completion = read(&dir.join("nested").join(nested).join("completion.json"))?;
    let expected = object([
        ("schema", string("agent-memory/layout-nested-completion-v1")),
        ("operation_id", string(operation)),
        ("nested_id", string(nested)),
        (
            "canonical_receipt_sha256",
            string(sha(receipt.unwrap_or(&Value::Null))?),
        ),
        ("ledger_head_sha256", string(sha(&head)?)),
        ("verified", Value::Bool(true)),
    ]);
    if receipt.is_none()
        || !same(&completion, &expected)
        || get(&canonical, "verified") != &Value::Bool(true)
        || !has_keys(
            &retirement,
            &[
                "schema",
                "operation_id",
                "ledger_head",
                "receipt_sha256",
                "dependencies",
                "verified",
            ],
        )
        || get(&retirement, "schema").as_str() != Some("agent-memory/layout-retirement-v1")
        || get(&retirement, "operation_id").as_str() != Some(operation)
        || !same(get(&retirement, "ledger_head"), &head)
        || get(&retirement, "receipt_sha256").as_str() != Some(sha(&canonical)?.as_str())
        || get(&retirement, "verified") != &Value::Bool(true)
        || get(&retirement, "dependencies").as_array().is_none()
        || get(get(journal, "hold"), "hold_id") != get(current_row, "hold_id")
    {
        return Err(err(
            "Terminal nested recovery needs its exact completed linkage and durable retirement",
        ));
    }
    let mut dependencies = BTreeSet::new();
    let bindings = state::bindings()?;
    let bindings = field(&bindings, "bindings")?;
    for item in items(field(&retirement, "dependencies")?)? {
        if !has_keys(item, &["operation_id", "ledger_head_sha256"])
            || !dependencies.insert(text(item, "operation_id")?.to_owned())
        {
            return Err(err(
                "Nested retirement has an unknown or duplicated dependency",
            ));
        }
        let id = text(item, "operation_id")?;
        let dependency = get(get(indexed, "operations"), id);
        let binding = get(bindings, id);
        let manifest = read(&directory(root, id, runtime)?.join("manifest.json"))?;
        if dependency == &Value::Null
            || get(dependency, "state").as_str() != Some("terminal")
            || get(dependency, "project_id") != get(current_row, "project_id")
            || get(dependency, "ledger_head_sha256") != get(item, "ledger_head_sha256")
            || binding == &Value::Null
            || !matches!(
                get(binding, "lifecycle_state").as_str(),
                Some("active" | "terminal")
            )
            || get(binding, "project_id") != get(current_row, "project_id")
            || get(binding, "bank_path").as_str() != bank.to_str()
            || get(binding, "checkout_key")
                != get(
                    get(get(&manifest, "identity"), "scope_context"),
                    "checkout_key",
                )
        {
            return Err(err(
                "Nested retirement dependency no longer matches its terminal binding and ledger",
            ));
        }
    }
    if !dependencies.contains(operation) {
        return Err(err("Nested retirement omitted its exact holding operation"));
    }
    for (id, other) in map(field(indexed, "operations")?)? {
        if get(other, "state").as_str() == Some("cleaned") {
            continue;
        }
        let (other_dir, _, current) = frontier.ledger(root, id, other, &bank, runtime)?;
        frontier.evidence(&other_dir, &current, other, runtime)?;
        if get(other, "project_id") != get(current_row, "project_id")
            || matches!(get(other, "state").as_str(), Some("active" | "interrupted"))
            || items(field(&current, "units")?)?
                .iter()
                .any(|u| get(u, "state").as_str() == Some("pending-admission"))
        {
            return Err(err(
                "Retained same-bank recovery evidence still requires its safeguard",
            ));
        }
    }
    if map(bindings)?.iter().any(|(id, b)| {
        !dependencies.contains(id)
            && get(b, "bank_path").as_str() == bank.to_str()
            && matches!(
                get(b, "lifecycle_state").as_str(),
                Some("active" | "interrupted")
            )
    }) {
        return Err(err(
            "An active bound dependency prevents nested retirement recovery",
        ));
    }
    let scoped = journal
        .get("write_proposal")
        .unwrap_or(field(journal, "proposal")?);
    crate::layout_history::check(
        root,
        field(scoped, "scope_context")?,
        text(current_row, "project_id")?,
        runtime,
    )?;
    let marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
    let sentinel = frontier.marker_bytes(&bank, resolution::SENTINEL_FILE, None)?;
    let expected_sentinel = encoded(&object([
        ("schema", string(writes::SENTINEL)),
        ("project_id", field(current_row, "project_id")?.clone()),
        ("operation_id", string(operation)),
    ]))?;
    if marker
        .as_deref()
        .is_some_and(|m| encoded(get(journal, "marker")).map_or(true, |expected| m != expected))
        || sentinel.as_deref().is_some_and(|s| s != expected_sentinel)
        || (marker.is_some() && sentinel.is_none())
    {
        return Err(err(
            "Nested retirement found a foreign safeguard or impossible removal order",
        ));
    }
    bank_observation::markers(&bank, owner, marker.as_deref())?;
    Ok(object([
        ("retirement_sha256", string(sha(&retirement)?)),
        ("completion_sha256", string(sha(&completion)?)),
        ("marker_sha256", optional_digest(marker.as_deref())),
        ("sentinel_sha256", optional_digest(sentinel.as_deref())),
    ]))
}
fn optional_digest(bytes: Option<&[u8]>) -> Value {
    bytes.map(|b| string(digest(b))).unwrap_or(Value::Null)
}
pub fn retained_coverage_review(journal: &Value, runtime: &Path) -> Result<()> {
    let p = field(journal, "proposal")?;
    let Some((_, proof, _)) = resolution::transaction_evidence(p)? else {
        return Ok(());
    };
    let token = field(
        field(field(proof, "judgment")?, "observation")?,
        "recovery_frontier",
    )?;
    if items(field(token, "unit_ids")?)?.is_empty() {
        return Ok(());
    }
    let review_digest = field(token, "review_digest")?;
    if !state::sha(review_digest) {
        return Err(err(
            "Nested finish requires its exact retained independent coverage review",
        ));
    }
    let mut matches = 0;
    for operation in items(field(token, "operation_ids")?)? {
        let operation = operation
            .as_str()
            .ok_or_else(|| err(resolution::MALFORMED))?;
        let dir = directory(Path::new(text(p, "repo_root")?), operation, runtime)?;
        if let Some(review) = optional(&dir.join("coverage-reviews").join(
            format!("{}.json",review_digest.as_str().ok_or_else(||err(resolution::MALFORMED))?),
        ))? {
            if review_digest.as_str() != Some(sha(&review)?.as_str()) {
                return Err(err("Nested finish independent coverage review changed"));
            }
            matches += 1;
        }
    }
    if matches != 1 {
        return Err(err(
            "Nested finish requires one exact retained independent coverage review",
        ));
    }
    Ok(())
}
pub fn nested_preview(
    repo: &Path,
    operation: &str,
    nested: &str,
    mode: &str,
    owner: Option<&[u8]>,
    runtime: &Path,
) -> Result<Value> {
    let (root, bank, _, dir) = state::paths(repo, Some(operation), runtime)?;
    let dir = dir.ok_or_else(|| err(resolution::MALFORMED))?;
    if !matches!(mode, "finish" | "rollback") || !uuid_valid(&string(nested)) {
        return Err(err(
            "Known nested recovery needs the exact operation, UUID and finish/rollback mode",
        ));
    }
    let nested_dir = dir.join("nested").join(nested);
    let journal = read(&nested_dir.join("journal.json"))?;
    let mut fields = vec![
        "schema",
        "operation_id",
        "nested_id",
        "marker",
        "hold",
        "proposal",
        "proposal_sha256",
        "base_files",
        "frontier",
        "resolution_plan",
    ];
    if journal.get("write_proposal").is_some() {
        fields.push("write_proposal");
    }
    if !has_keys(&journal, &fields) {
        return Err(err(
            "Nested recovery requires the exact typed retained journal",
        ));
    }
    if get(&journal, "schema").as_str() != Some("agent-memory/layout-nested-journal-v1")
        || get(&journal, "operation_id").as_str() != Some(operation)
        || get(&journal, "nested_id").as_str() != Some(nested)
        || get(&journal, "proposal_sha256").as_str()
            != Some(sha(field(&journal, "proposal")?)?.as_str())
    {
        return Err(err("Unknown, foreign or damaged nested recovery journal"));
    }
    let mut marker = Some(encoded(field(&journal, "marker")?)?);
    let p = tx::proposal_value(field(&journal, "proposal")?)?;
    let frontier = writes::recovery_frontier_provider()?;
    let original_hold = frontier.marker_value(&encoded(field(&journal, "hold")?)?)?;
    let expected_marker = object([
        ("schema", string(writes::NESTED)),
        ("project_id", field(p, "project_id")?.clone()),
        ("operation_id", string(operation)),
        ("nested_id", string(nested)),
        ("hold_id", get(&original_hold, "hold_id").clone()),
    ]);
    if !same(field(&journal, "marker")?, &expected_marker)
        || get(&original_hold, "schema").as_str() != Some(writes::HOLD)
        || get(&original_hold, "operation_id").as_str() != Some(operation)
        || get(&original_hold, "project_id") != get(p, "project_id")
        || get(p, "operation_id").as_str() != Some(nested)
        || get(p, "repo_root").as_str() != root.to_str()
        || !same(
            &tx::file_hashes(field(&journal, "base_files")?)?,
            field(p, "observed_hashes")?,
        )
        || get(get(&journal, "frontier"), "project_id") != get(p, "project_id")
        || get(get(&journal, "frontier"), "bank_path").as_str() != bank.to_str()
    {
        return Err(err(
            "Nested journal changed its canonical base, marker or exact held scope",
        ));
    }
    let observed = writes::hashes(&bank)?;
    let mut rollback = Value::Null;
    if optional(&nested_dir.join("rollback-plan.json"))?.is_some() {
        if mode != "rollback" {
            return Err(err(
                "A durable pre-receipt nested rollback cannot become a finish",
            ));
        }
        rollback = rollback_plan(&nested_dir, &journal, &observed)?.0;
    } else {
        legal_partial(p, field(&journal, "base_files")?, &observed)?;
    }
    let (_, receipt_path) = tx::locations(&root, &bank, nested)?;
    let receipt = nested_receipt(&journal, &receipt_path)?;
    let mut indexed = state::index(&root, runtime)?;
    let index_path = dir
        .parent()
        .ok_or_else(|| err(resolution::MALFORMED))?
        .join("index.json");
    let index_raw = state::read_bytes(&index_path, MAX_PRIVATE, false)?
        .ok_or_else(|| err(resolution::MALFORMED))?;
    let original_index_sha256 = digest(&index_raw);
    if let Some(receipt) = &receipt {
        for plan in items(field(field(&journal, "resolution_plan")?, "ledgers")?)? {
            let resolved = text(plan, "operation_id")?;
            let head = read(&directory(&root, resolved, runtime)?.join("ledger-head.json"))?;
            if !same(&head, field(plan, "previous_head")?) {
                let (units, _) = planned_resolution(plan, &journal, receipt, nested, runtime)?;
                let (projected, _, _) = resolution::interrupted_successor(
                    &root,
                    resolved,
                    &units,
                    field(plan, "previous_head")?,
                    None,
                    runtime,
                )?;
                let operations = indexed
                    .get_mut("operations")
                    .ok_or_else(|| err(resolution::MALFORMED))?;
                set(operations, resolved, projected);
            }
        }
    }
    let (retirement, frontier_token) =
        if get(row(&indexed, operation)?, "state").as_str() == Some("terminal") {
            let retirement = retired_nested(
                &root,
                operation,
                nested,
                &journal,
                receipt.as_ref(),
                &indexed,
                owner,
                runtime,
            )?;
            marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
            (retirement, Value::Null)
        } else {
            bank_observation::markers(&bank, owner, marker.as_deref())?;
            (
                Value::Null,
                frontier.observe_index(&root, owner, marker.as_deref(), &indexed, runtime)?,
            )
        };
    if mode == "finish" && retirement == Value::Null {
        retained_coverage_review(&journal, runtime)?;
    }
    if receipt.is_none() {
        frontier.check(
            field(&journal, "frontier")?,
            &root,
            owner,
            marker.as_deref(),
        )?;
    }
    if receipt.is_none() && mode != "rollback" {
        return Err(err(
            "Unfinished governed nested learning permits rollback only, then fresh evidence",
        ));
    }
    if let Some(receipt) = &receipt {
        if !matches!(
            get(receipt, "outcome").as_str(),
            Some("finished" | "replaced" | "already-present")
        ) || mode != "finish"
            || !same(&observed, field(receipt, "resulting_hashes")?)
        {
            return Err(err(
                "A durably receipted canonical transaction is finish-only and cannot replay over later edits",
            ));
        }
    } else {
        rollback = rollback_plan(&nested_dir, &journal, &observed)?.0;
    }
    Ok(object([
        ("schema", string("agent-memory/layout-nested-recovery-v1")),
        ("recovery_id", string(host::uuid4()?)),
        ("repo_root", string(root.to_string_lossy())),
        ("operation_id", string(operation)),
        ("nested_id", string(nested)),
        ("mode", string(mode)),
        ("journal_sha256", string(sha(&journal)?)),
        ("marker_sha256", optional_digest(marker.as_deref())),
        ("observed_hashes", observed),
        (
            "canonical_receipt_sha256",
            receipt
                .as_ref()
                .map(sha)
                .transpose()?
                .map(string)
                .unwrap_or(Value::Null),
        ),
        ("ledger_head", read(&dir.join("ledger-head.json"))?),
        ("recovery_frontier", frontier_token),
        ("ledger_index_sha256", string(original_index_sha256)),
        ("rollback_plan", rollback),
        ("retirement", retirement),
    ]))
}
pub fn recover_nested(
    repo: &Path,
    operation: Option<&str>,
    nested: Option<&str>,
    mode: Option<&str>,
    proposal: Option<&Value>,
    runtime: &Path,
) -> Result<Value> {
    let Some(proposal) = proposal else {
        let operation = operation.ok_or_else(|| {
            err("Known nested recovery needs the exact operation, UUID and finish/rollback mode")
        })?;
        let nested = nested.ok_or_else(|| {
            err("Known nested recovery needs the exact operation, UUID and finish/rollback mode")
        })?;
        let mode = mode.ok_or_else(|| {
            err("Known nested recovery needs the exact operation, UUID and finish/rollback mode")
        })?;
        let (root, bank) = writes::bank_path(repo)?;
        let mut barrier = crate::turn_sources::canonical_writer_barrier(&root, None, runtime)?;
        let preview = storage::exclusive(
            &bank.join(".agent-memory.lock"),
            nested,
            "bank-recover",
            Sources::default(),
            |owner| nested_preview(&root, operation, nested, mode, Some(owner), runtime),
        );
        let finished = barrier.finish();
        let preview = preview?;
        finished?;
        return Ok(object([
            ("proposal", preview),
            ("writes_performed", Value::Bool(false)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
        ]));
    };
    let p = proposal.get("proposal").unwrap_or(proposal);
    let operation = text(p, "operation_id")?;
    let nested = text(p, "nested_id")?;
    let (root, bank) = writes::bank_path(repo)?;
    let dir = directory(&root, operation, runtime)?;
    let mut barrier = crate::turn_sources::canonical_writer_barrier(&root, None, runtime)?;
    let result = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        text(p, "recovery_id")?,
        "bank-recover",
        Sources::default(),
        |owner| {
            let mut current = nested_preview(
                &root,
                operation,
                nested,
                text(p, "mode")?,
                Some(owner),
                runtime,
            )?;
            set(
                &mut current,
                "recovery_id",
                field(p, "recovery_id")?.clone(),
            );
            if !same(&current, p) {
                return Err(err(
                    "Nested recovery source, journal, marker, ledger or durable receipt changed",
                ));
            }
            let nested_dir = dir.join("nested").join(nested);
            let journal = read(&nested_dir.join("journal.json"))?;
            let original = field(&journal, "proposal")?;
            let marker = encoded(field(&journal, "marker")?)?;
            let (_, receipt_path) = tx::locations(&root, &bank, nested)?;
            if text(p, "mode")? == "finish" {
                let receipt = nested_receipt(&journal, &receipt_path)?
                    .ok_or_else(|| err(resolution::MALFORMED))?;
                if field(p, "retirement")? != &Value::Null {
                    let marker = writes::recovery_frontier_provider()?.marker_bytes(
                        &bank,
                        tx::MARKER,
                        None,
                    )?;
                    resolution::retire(&root, operation, Some(owner), marker.as_deref(), runtime)?;
                } else {
                    terminalize(
                        &root, operation, nested, &journal, &receipt, owner, &marker, runtime,
                    )?;
                }
            } else {
                let (plan, boundary) =
                    rollback_plan(&nested_dir, &journal, field(p, "observed_hashes")?)?;
                if !same(&plan, field(p, "rollback_plan")?) {
                    return Err(err(
                        "Nested rollback changed its exact reviewed initial/action pins",
                    ));
                }
                state::write_once(&nested_dir.join("rollback-plan.json"), &plan)?;
                writes::fault("layout-recovery-nested-rollback-after-plan")?;
                let mut running = field(p, "observed_hashes")?.clone();
                let expected = tx::file_hashes(field(&journal, "base_files")?)?;
                let participants = participants(original)?;
                let frontier = writes::recovery_frontier_provider()?;
                for (number, action) in items(field(&plan, "actions")?)?
                    .iter()
                    .enumerate()
                    .skip(boundary)
                {
                    let path = text(action, "path")?;
                    let before = || {
                        frontier.check(
                            field(&journal, "frontier")?,
                            &root,
                            Some(owner),
                            Some(&marker),
                        )
                    };
                    tx::replace_file(
                        &bank,
                        path,
                        get(&journal, "base_files")
                            .get(path)
                            .and_then(Value::as_str),
                        &running,
                        &marker,
                        &participants,
                        None,
                        Some(&before),
                    )?;
                    if let Some(hash) = expected.get(path) {
                        set(&mut running, path, hash.clone());
                    } else {
                        remove(&mut running, path);
                    }
                    writes::fault(&format!(
                        "layout-recovery-nested-rollback-after-rename:{path}"
                    ))?;
                    state::write_once(
                        &nested_dir
                            .join("rollback-progress")
                            .join(format!("{number:08}.json")),
                        &object([("path", string(path)), ("hashes", running.clone())]),
                    )?;
                    writes::fault(&format!(
                        "layout-recovery-nested-rollback-after-progress:{path}"
                    ))?;
                }
                bank_contract::validate(&root, Some(owner), Some(&marker))?;
                state::write_once(
                    &nested_dir.join("completion.json"),
                    &object([
                        ("schema", string("agent-memory/layout-nested-completion-v1")),
                        ("operation_id", string(operation)),
                        ("nested_id", string(nested)),
                        ("outcome", string("rolled-back")),
                        ("verified", Value::Bool(true)),
                    ]),
                )?;
                resolution::restore_hold(&root, operation, Some(owner), Some(&marker), runtime)?;
            }
            Ok(object([
                ("verified", Value::Bool(true)),
                ("operation_id", string(operation)),
                ("nested_id", string(nested)),
                ("mode", field(p, "mode")?.clone()),
                ("writes_performed", Value::Bool(true)),
                (
                    "canonical_writes_performed",
                    Value::Bool(get(p, "mode").as_str() == Some("rollback")),
                ),
            ]))
        },
    );
    let finished = barrier.finish();
    let value = result?;
    finished?;
    Ok(value)
}
pub fn nested_receipt(journal: &Value, path: &Path) -> Result<Option<Value>> {
    let Some(write) = journal.get("write_proposal") else {
        return tx::receipt_value(path, field(journal, "proposal")?);
    };
    let Some(receipt) = optional(path)? else {
        return Ok(None);
    };
    let mut expected = field(write, "observed_hashes")?.clone();
    set(
        &mut expected,
        text(write, "path")?,
        string(digest(text(write, "content")?.as_bytes())),
    );
    if get(&receipt, "schema").as_str() != Some("agent-memory/write-receipt-v1")
        || get(&receipt, "operation_id") != get(write, "operation_id")
        || get(&receipt, "request_sha256").as_str() != Some(sha(write)?.as_str())
        || get(&receipt, "project_id") != get(write, "project_id")
        || get(&receipt, "verified") != &Value::Bool(true)
        || !same(get(&receipt, "resulting_hashes"), &expected)
        || !matches!(
            get(&receipt, "outcome").as_str(),
            Some("replaced" | "already-present")
        )
    {
        return Err(err(
            "Held checkpoint receipt is unknown or differs from its actual write proposal",
        ));
    }
    Ok(Some(receipt))
}
pub fn apply_checkpoint(prepared: &Value, runtime: &Path) -> Result<Value> {
    let write = writes::proposal_value(prepared)?;
    if get(write, "operation").as_str() != Some("checkpoint")
        || write.get("checkpoint_evidence").is_none()
    {
        return Err(err(
            "A verified recovery hold does not authorize a raw single-file replacement",
        ));
    }
    let (root, bank) = writes::bank_path(Path::new(text(write, "repo_root")?))?;
    let nested = text(write, "operation_id")?;
    let (_, receipt_path) = writes::state_paths(&root, &bank, nested)?;
    if let Some(known) = optional(&receipt_path)? {
        if get(&known, "request_sha256").as_str() != Some(sha(write)?.as_str())
            || get(&known, "schema").as_str() != Some("agent-memory/write-receipt-v1")
        {
            return Err(err("Checkpoint ID was reused for different evidence"));
        }
        return writes::result(
            "apply",
            &with(
                receipt_fields(&known),
                [
                    ("already_applied", Value::Bool(true)),
                    ("writes_performed", Value::Bool(false)),
                    ("still_current", Value::Null),
                    ("receipt_path", string(receipt_path.to_string_lossy())),
                ],
            ),
        );
    }
    let mut barrier = crate::turn_sources::canonical_writer_barrier(&root, None, runtime)?;
    let result = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        nested,
        "bank-checkpoint",
        Sources::default(),
        |owner| {
            resolution::require_completed_resolutions(&root, None, runtime)?;
            let frontier = writes::recovery_frontier_provider()?;
            let held = frontier
                .verified_hold_value(&root, Some(owner), None, runtime)?
                .ok_or_else(|| err("Held checkpoint scope or source changed after preview"))?;
            if !same(held.scope(), field(write, "scope_context")?)
                || !same(
                    &writes::snapshot(&root, Some(owner))?.1,
                    field(write, "observed_hashes")?,
                )
            {
                return Err(err("Held checkpoint scope or source changed after preview"));
            }
            checkpoint::validate_write(write, true)?;
            let base = tx::files_at(&bank)?;
            let mut files = base.clone();
            set(
                &mut files,
                text(write, "path")?,
                field(write, "content")?.clone(),
            );
            bank_contract::validate_files(&files, None)?;
            let pending = frontier.pending_rows(&root, held.token(), Some(owner), None, runtime)?;
            frontier.reservation(&files, &pending, held.token())?;
            let p = object([
                ("schema", string(tx::SCHEMA)),
                ("operation_id", string(nested)),
                ("operation", string("replace")),
                ("repo_root", string(root.to_string_lossy())),
                ("project_id", field(write, "project_id")?.clone()),
                ("observed_hashes", field(write, "observed_hashes")?.clone()),
                ("files", files.clone()),
                ("scope_context", field(write, "scope_context")?.clone()),
            ]);
            let marker = marker_for(&held, nested)?;
            let marker_raw = encoded(&marker)?;
            let journal = object([
                ("schema", string("agent-memory/layout-nested-journal-v1")),
                ("operation_id", string(held.operation_id())),
                ("nested_id", string(nested)),
                ("marker", marker),
                ("hold", frontier.marker_value(held.marker())?),
                ("proposal", p.clone()),
                ("proposal_sha256", string(sha(&p)?)),
                ("base_files", base),
                ("write_proposal", write.clone()),
                ("frontier", held.token().clone()),
                (
                    "resolution_plan",
                    object([("ledgers", Value::Array(Vec::new()))]),
                ),
            ]);
            let dir = directory(&root, held.operation_id(), runtime)?
                .join("nested")
                .join(nested);
            state::write_once(&dir.join("journal.json"), &journal)?;
            crate::security::guarded_start(
                &root,
                nested,
                held.scope(),
                &object([(text(write, "path")?, field(write, "content")?.clone())]),
                &["assistant"],
                "checkpoint",
                false,
            )?;
            resolution::atomic_marker(&bank, Some(held.marker()), &marker_raw, Some(owner))?;
            let before = || {
                frontier.check(held.token(), &root, Some(owner), Some(&marker_raw))?;
                checkpoint::validate_write(write, true)
            };
            let participants = map(&files)?.iter().map(|(k, _)| k.clone()).collect();
            tx::replace_file(
                &bank,
                text(write, "path")?,
                Some(text(write, "content")?),
                field(write, "observed_hashes")?,
                &marker_raw,
                &participants,
                None,
                Some(&before),
            )?;
            checkpoint::validate_write(write, true)?;
            if get(
                &bank_contract::validate(&root, Some(owner), Some(&marker_raw))?,
                "project_id",
            ) != get(write, "project_id")
            {
                return Err(err("Held checkpoint readback changed project identity"));
            }
            let relative = bank
                .strip_prefix(&root)
                .map_err(|_| err(resolution::MALFORMED))?
                .to_string_lossy();
            let receipt = object([
                ("schema", string("agent-memory/write-receipt-v1")),
                ("operation_id", string(nested)),
                ("project_id", field(write, "project_id")?.clone()),
                ("bank_path", string(bank.to_string_lossy())),
                ("request_sha256", string(sha(write)?)),
                ("observed_hashes", field(write, "observed_hashes")?.clone()),
                ("resulting_hashes", tx::file_hashes(&files)?),
                (
                    "changed_memory_paths",
                    Value::Array(vec![string(format!("{relative}/{}", text(write, "path")?))]),
                ),
                ("outcome", string("replaced")),
                ("readback_at", string(storage::now(Sources::default())?)),
                ("verified", Value::Bool(true)),
            ]);
            writes::fault("layout-recovery-nested-before-receipt")?;
            writes::record(&receipt_path, &encoded(&receipt)?)?;
            writes::fault("layout-recovery-nested-after-receipt")?;
            terminalize(
                &root,
                held.operation_id(),
                nested,
                &journal,
                &receipt,
                owner,
                &marker_raw,
                runtime,
            )?;
            writes::result(
                "apply",
                &with(
                    receipt_fields(&receipt),
                    [
                        ("already_applied", Value::Bool(false)),
                        ("writes_performed", Value::Bool(true)),
                        ("still_current", Value::Bool(true)),
                        ("receipt_path", string(receipt_path.to_string_lossy())),
                    ],
                ),
            )
        },
    );
    let finished = barrier.finish();
    let value = result?;
    finished?;
    Ok(value)
}
