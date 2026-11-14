//! Reviewed semantic refinement of pending units, preserving every predecessor.
use crate::codec::json::Value;
use crate::error::Result;
use crate::host::Sources;
use crate::layout_resolution::{
    MALFORMED, advance_ledger, digest, err, field, frontier, int, items, locked, number,
    operation_row, publish_index, range_key, read_optional, read_value,
    require_completed_resolutions, require_hold, same, set, sha, text, unit_ids, uuid, with,
};
use crate::layout_restore::Operation;
use crate::lifecycle::{get, object, string};
use crate::{
    bank_transactions as tx, bank_writes as writes, host, layout_recovery_state as state,
    lifecycle, local_registry as storage,
};
use std::collections::BTreeSet;
use std::path::Path;

pub const SCHEMA: &str = "agent-memory/layout-unit-revision-v1";
pub const FIELDS: &[&str] = &[
    "schema",
    "revision_id",
    "operation_id",
    "parent_unit_map_sha256",
    "parent_ledger_head_sha256",
    "manifest_sha256",
    "replacements",
    "independent_review_sha256",
    "intended_ledger_sha256",
];
const REQUEST_SCHEMA: &str = "agent-memory/layout-unit-revision-request-v1";
const REVIEW_SCHEMA: &str = "agent-memory/layout-unit-revision-review-v1";
const JOURNAL_SCHEMA: &str = "agent-memory/layout-unit-revision-journal-v1";
const RECEIPT_SCHEMA: &str = "agent-memory/layout-unit-revision-receipt-v1";
const REVIEW_FIELDS: &[&str] = &[
    "schema",
    "review_id",
    "operation_id",
    "author",
    "reviewer",
    "parent_unit_map_sha256",
    "parent_ledger_head_sha256",
    "manifest_sha256",
    "replacements_sha256",
    "intended_unit_map_sha256",
    "reviewed_at",
    "decision",
    "rationale",
    "evidence",
];

/// Merge adjacent original ranges; overlapping bytes are refused.
pub fn merged(ranges: &Value) -> Result<Vec<Value>> {
    let frontier = frontier()?;
    let values = items(ranges)?;
    for span in values {
        frontier.range_value(span)?;
    }
    let mut ordered: Vec<(String, i128, Value)> = Vec::new();
    for span in values {
        let (source, start, _) = range_key(span)?;
        ordered.push((source, start, span.clone()));
    }
    ordered.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut result: Vec<Value> = Vec::new();
    for (source, start, span) in ordered {
        if let Some(last) = result.last_mut() {
            let (last_source, _, last_end) = range_key(last)?;
            if last_source == source && start < last_end {
                return Err(err(
                    "Unit revision double-counts overlapping original bytes",
                ));
            }
            if last_source == source && start == last_end {
                set(last, "end", field(&span, "end")?.clone());
                continue;
            }
        }
        result.push(span);
    }
    Ok(result)
}

fn nonblank(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.trim().is_empty())
}
fn same_ranges(a: &Value, b: &Value) -> Result<bool> {
    Ok(same(&Value::Array(merged(a)?), &Value::Array(merged(b)?)))
}

/// Apply reviewed replacements: each parent becomes superseded by ≥2 children.
pub fn revised(current: &Value, unit_map: &Value, replacements: &Value) -> Result<(Value, Value)> {
    let replacements = replacements
        .as_array()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| err("Semantic revision requires explicit meaningful replacements"))?;
    let mut units = items(field(current, "units")?)?.to_vec();
    let mut mapped = items(field(unit_map, "units")?)?.to_vec();
    let mut all_ids: BTreeSet<String> = BTreeSet::new();
    for unit in &units {
        all_ids.insert(text(unit, "unit_id")?.to_owned());
    }
    let mut parents = BTreeSet::new();
    for replacement in replacements {
        crate::layout_recovery::fields(
            replacement,
            &[
                "parent_unit_id",
                "source_ranges",
                "new_units",
                "semantic_rationale",
            ],
            None,
        )?;
        let parent_id = field(replacement, "parent_unit_id")?;
        let refused = || err("Revision must meaningfully refine one whole still-pending parent");
        let parent_index = units
            .iter()
            .position(|unit| get(unit, "unit_id") == parent_id);
        let primary_index = mapped
            .iter()
            .position(|unit| get(unit, "unit_id") == parent_id);
        let parent_key = parent_id.as_str().map(str::to_owned);
        let (Some(parent_index), Some(primary_index), Some(parent_key)) =
            (parent_index, primary_index, parent_key)
        else {
            return Err(refused());
        };
        let parent = units.get(parent_index).cloned().ok_or_else(refused)?;
        let primary = mapped.get(primary_index).cloned().ok_or_else(refused)?;
        if parents.contains(&parent_key)
            || get(&parent, "state").as_str() != Some("pending-admission")
            || !nonblank(get(replacement, "semantic_rationale"))
            || !same_ranges(
                field(replacement, "source_ranges")?,
                field(&parent, "source_ranges")?,
            )?
            || get(replacement, "new_units")
                .as_array()
                .is_none_or(|children| children.len() < 2)
        {
            return Err(refused());
        }
        parents.insert(parent_key);
        let mut children = Vec::new();
        let mut ranges = Vec::new();
        for child in items(field(replacement, "new_units")?)? {
            crate::layout_recovery::fields(
                child,
                &[
                    "unit_id",
                    "source_ranges",
                    "semantic_rationale",
                    "provenance",
                    "reason",
                    "required_actions",
                ],
                None,
            )?;
            let child_id = get(child, "unit_id");
            let fresh = child_id
                .as_str()
                .is_some_and(|id| uuid(child_id) && !all_ids.contains(id));
            if !fresh
                || !nonblank(get(child, "semantic_rationale"))
                || get(child, "provenance").as_object().is_none()
                || !nonblank(get(child, "reason"))
                || get(child, "required_actions")
                    .as_array()
                    .is_none_or(|actions| actions.is_empty())
            {
                return Err(err(
                    "Revised children need new stable IDs and reviewed meaningful provenance/actions",
                ));
            }
            all_ids.insert(text(child, "unit_id")?.to_owned());
            children.push(child_id.clone());
            ranges.extend(items(field(child, "source_ranges")?)?.iter().cloned());
            units.push(object([
                ("unit_id", child_id.clone()),
                ("source_ranges", field(child, "source_ranges")?.clone()),
                ("provenance", field(child, "provenance")?.clone()),
                ("reason", field(child, "reason")?.clone()),
                (
                    "required_actions",
                    field(child, "required_actions")?.clone(),
                ),
                (
                    "detecting_developer_id",
                    field(&parent, "detecting_developer_id")?.clone(),
                ),
                ("state", string("pending-admission")),
                (
                    "terminal_receipts",
                    field(&parent, "terminal_receipts")?.clone(),
                ),
            ]));
            mapped.push(with(
                primary.clone(),
                [
                    ("unit_id", child_id.clone()),
                    ("source_ranges", field(child, "source_ranges")?.clone()),
                    ("provenance", field(child, "provenance")?.clone()),
                    ("reason", field(child, "reason")?.clone()),
                    (
                        "required_actions",
                        field(child, "required_actions")?.clone(),
                    ),
                ],
            ));
        }
        if !same_ranges(&Value::Array(ranges), field(&parent, "source_ranges")?)? {
            return Err(err(
                "Revision children must partition all original parent bytes without loss",
            ));
        }
        let updated = with(
            parent,
            [
                ("state", string("superseded-by-revision")),
                ("child_unit_ids", Value::Array(children)),
            ],
        );
        if let Some(slot) = units.get_mut(parent_index) {
            *slot = updated;
        }
        if let Some(position) = mapped.iter().position(|unit| same(unit, &primary)) {
            mapped.remove(position);
        }
    }
    let new_map = with(unit_map.clone(), [("units", Value::Array(mapped))]);
    Ok((Value::Array(units), new_map))
}

/// The exact immutable unit map the current ledger generation names.
pub fn map_at(directory: &Path, current: &Value) -> Result<Value> {
    let original = read_value(&directory.join("unit-map.json"))?;
    let expected = text(current, "unit_map_sha256")?;
    if sha(&original)? == expected {
        return Ok(original);
    }
    let value = read_value(&directory.join("unit-maps").join(format!("{expected}.json")))?;
    if sha(&value)? != expected {
        return Err(err("Revised immutable unit map is missing or changed"));
    }
    Ok(value)
}

/// Preview one independently reviewed unit revision under the bank lock.
pub fn preview(
    repo: &Path,
    operation_id: &str,
    request: &Value,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, _, _) =
        crate::layout_resolution::operation_paths(repo, operation_id, runtime_root)?;
    locked(&root, &bank, operation_id, |owner| {
        preview_locked(&root, operation_id, request, owner, runtime_root)
    })
}

fn pending_units(units: &Value) -> Result<Vec<Value>> {
    Ok(items(units)?
        .iter()
        .filter(|unit| get(unit, "state").as_str() == Some("pending-admission"))
        .cloned()
        .collect())
}

fn row_ids(rows: &[Value]) -> Result<Vec<String>> {
    rows.iter()
        .map(|row| text(row, "unit_id").map(str::to_owned))
        .collect()
}

fn preview_locked(
    repo: &Path,
    operation_id: &str,
    request: &Value,
    owner: &[u8],
    runtime_root: &Path,
) -> Result<Value> {
    let owner = Some(owner);
    require_completed_resolutions(repo, None, runtime_root)?;
    crate::layout_recovery::fields(
        request,
        &["schema", "operation_id", "replacements", "review"],
        Some(REQUEST_SCHEMA),
    )?;
    if get(request, "operation_id").as_str() != Some(operation_id) {
        return Err(err("Unit revision belongs to another operation"));
    }
    let frontier = frontier()?;
    let held = frontier
        .verified_hold_value(repo, owner, None, runtime_root)?
        .filter(|held| held.operation_id() == operation_id)
        .ok_or_else(|| err("Unit revision requires the exact persistent checked hold"))?;
    let Operation {
        root,
        bank,
        directory,
        manifest,
        ..
    } = crate::layout_recovery::operation(repo, operation_id, runtime_root)?;
    let index = state::index(&root, runtime_root)?;
    let indexed = operation_row(&index, operation_id)?;
    let (_, head, current) = frontier.ledger(&root, operation_id, indexed, &bank, runtime_root)?;
    let old_map = map_at(&directory, &current)?;
    let replacements = field(request, "replacements")?;
    let (units, new_map) = revised(&current, &old_map, replacements)?;
    let original = read_value(
        &directory
            .join("proposals")
            .join(format!("{}.json", text(indexed, "proposal_id")?)),
    )?;
    crate::layout_recovery::unit_map_value(
        &new_map,
        &manifest,
        field(&original, "intended_files")?,
        None,
    )?;
    let review = field(request, "review")?;
    crate::layout_recovery::fields(review, REVIEW_FIELDS, Some(REVIEW_SCHEMA))?;
    crate::layout_recovery::independent(field(review, "author")?, field(review, "reviewer")?)?;
    lifecycle::time_value(get(review, "reviewed_at"))?;
    if !uuid(get(review, "review_id"))
        || get(review, "operation_id").as_str() != Some(operation_id)
        || get(review, "decision").as_str() != Some("approve")
        || !same(
            get(review, "parent_unit_map_sha256"),
            field(&current, "unit_map_sha256")?,
        )
        || get(review, "parent_ledger_head_sha256").as_str() != Some(sha(&head)?.as_str())
        || get(review, "manifest_sha256").as_str() != Some(sha(&manifest)?.as_str())
        || get(review, "replacements_sha256").as_str() != Some(sha(replacements)?.as_str())
        || get(review, "intended_unit_map_sha256").as_str() != Some(sha(&new_map)?.as_str())
        || !truthy(get(review, "rationale"))
        || !truthy(get(review, "evidence"))
    {
        return Err(err(
            "Unit refinement needs independent full-source/disposition review",
        ));
    }
    let files = tx::files_at(held.bank())?;
    let pending = pending_units(&units)?;
    let other_rows: Vec<Value> = frontier
        .pending_rows(&root, held.token(), owner, None, runtime_root)?
        .into_iter()
        .filter(|row| get(row, "operation_id").as_str() != Some(operation_id))
        .collect();
    let mut rows = crate::layout_recovery::pending_rows(&directory, &manifest, &pending)?;
    rows.extend(other_rows);
    let token = frontier.successor_token(held.token(), &row_ids(&rows)?, operation_id, true)?;
    frontier.reservation(&files, &rows, &token)?;
    let generation = int(field(&current, "generation")?).ok_or_else(|| err(MALFORMED))? + 1;
    let successor = with(
        current.clone(),
        [
            ("generation", number(generation)),
            (
                "previous_generation_sha256",
                field(&head, "sha256")?.clone(),
            ),
            ("unit_map_sha256", string(sha(&new_map)?)),
            ("units", units),
        ],
    );
    let proposal = object([
        ("schema", string(SCHEMA)),
        ("revision_id", string(host::uuid4()?)),
        ("operation_id", string(operation_id)),
        (
            "parent_unit_map_sha256",
            field(&current, "unit_map_sha256")?.clone(),
        ),
        ("parent_ledger_head_sha256", string(sha(&head)?)),
        ("manifest_sha256", string(sha(&manifest)?)),
        ("replacements", replacements.clone()),
        ("independent_review_sha256", string(sha(review)?)),
        ("intended_ledger_sha256", string(sha(&successor)?)),
    ]);
    let location = directory
        .join("revisions")
        .join(text(&proposal, "revision_id")?);
    state::write_once(&location.join("proposal.json"), &proposal)?;
    state::write_once(&location.join("review.json"), review)?;
    state::write_once(&location.join("unit-map.json"), &new_map)?;
    state::write_once(
        &directory
            .join("unit-maps")
            .join(format!("{}.json", sha(&new_map)?)),
        &new_map,
    )?;
    state::write_once(
        &location.join("journal.json"),
        &object([
            ("schema", string(JOURNAL_SCHEMA)),
            ("operation_id", string(operation_id)),
            ("proposal_sha256", string(sha(&proposal)?)),
            ("parent_head", head.clone()),
            ("successor", successor),
            ("hold_sha256", string(digest(held.marker()))),
            ("frontier", held.token().clone()),
            ("bank_hashes", tx::file_hashes(&files)?),
        ]),
    )?;
    frontier.check(held.token(), &root, owner, None)?;
    let pending_ids = field(&token, "unit_ids")?.clone();
    let count = items(&pending_ids)?.len();
    Ok(crate::layout_recovery::result(
        "revise-units",
        Some(operation_id),
        vec![
            ("state", string("unit-revision-preview")),
            ("verified", Value::Bool(true)),
            ("proposal", proposal),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
            ("pending_unit_ids", pending_ids),
            ("pending_units", number(count)),
        ],
    ))
}

/// Python truthiness of a decoded JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Int(number) => number.as_str() != "0",
        Value::Float(number) => *number != 0.0,
        Value::String(text) => !text.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(pairs) => !pairs.is_empty(),
    }
}

/// Apply one exact retained unit revision preview.
pub fn apply(repo: &Path, supplied: &Value, runtime_root: &Path) -> Result<Value> {
    let proposal = supplied.get("proposal").unwrap_or(supplied);
    crate::layout_recovery::fields(proposal, FIELDS, Some(SCHEMA))?;
    if !uuid(get(proposal, "revision_id")) || !uuid(get(proposal, "operation_id")) {
        return Err(err("Unit revision requires stable UUIDv4 identities"));
    }
    let operation_id = text(proposal, "operation_id")?;
    let Operation {
        root,
        bank,
        base,
        directory,
        manifest,
    } = crate::layout_recovery::operation(repo, operation_id, runtime_root)?;
    let revision_id = text(proposal, "revision_id")?;
    let location = directory.join("revisions").join(revision_id);
    if !same(&read_value(&location.join("proposal.json"))?, proposal) {
        return Err(err("Unit revision must use its exact immutable preview"));
    }
    if let Some(known) = read_optional(&location.join("receipt.json"))? {
        if get(&known, "proposal_sha256").as_str() != Some(sha(proposal)?.as_str())
            || get(&known, "verified").as_bool() != Some(true)
        {
            return Err(err("Unit revision historical receipt is corrupt"));
        }
        return Ok(crate::layout_recovery::result(
            "revise-units",
            Some(operation_id),
            vec![
                ("state", string("units-revised")),
                ("verified", Value::Bool(true)),
                ("receipt", known),
                ("receipt_is_historical", Value::Bool(true)),
                ("already_applied", Value::Bool(true)),
            ],
        ));
    }
    let journal = read_value(&location.join("journal.json"))?;
    let review = read_value(&location.join("review.json"))?;
    let new_map = read_value(&location.join("unit-map.json"))?;
    let successor = field(&journal, "successor")?.clone();
    if get(&journal, "proposal_sha256").as_str() != Some(sha(proposal)?.as_str())
        || get(proposal, "independent_review_sha256").as_str() != Some(sha(&review)?.as_str())
        || get(proposal, "intended_ledger_sha256").as_str() != Some(sha(&successor)?.as_str())
        || get(&successor, "unit_map_sha256").as_str() != Some(sha(&new_map)?.as_str())
        || get(proposal, "manifest_sha256").as_str() != Some(sha(&manifest)?.as_str())
    {
        return Err(err("Revision source/review/journal evidence changed"));
    }
    let frontier = frontier()?;
    let (receipt, held) = locked(&root, &bank, revision_id, |owner| {
        let owner = Some(owner);
        require_completed_resolutions(&root, None, runtime_root)?;
        let marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
        if crate::developer_identity::digest(marker.as_deref()).as_deref()
            != get(&journal, "hold_sha256").as_str()
        {
            return Err(err(
                "Revision hold changed or a nested transaction is interrupted",
            ));
        }
        if !same(
            &tx::file_hashes(&tx::files_at(&bank)?)?,
            field(&journal, "bank_hashes")?,
        ) {
            return Err(err(
                "Canonical sources changed after semantic revision review",
            ));
        }
        let head = read_value(&directory.join("ledger-head.json"))?;
        let expected_head = object([
            ("schema", string("agent-memory/layout-ledger-head-v1")),
            ("operation_id", string(operation_id)),
            ("generation", field(&successor, "generation")?.clone()),
            ("sha256", string(sha(&successor)?)),
        ]);
        if same(&head, field(&journal, "parent_head")?) {
            let held =
                require_hold(frontier.verified_hold_value(&root, owner, None, runtime_root)?)?;
            let pending = pending_units(field(&successor, "units")?)?;
            frontier.check(field(&journal, "frontier")?, &root, owner, None)?;
            let others: Vec<Value> = frontier
                .pending_rows(&root, held.token(), owner, None, runtime_root)?
                .into_iter()
                .filter(|row| get(row, "operation_id").as_str() != Some(operation_id))
                .collect();
            let mut rows = crate::layout_recovery::pending_rows(&directory, &manifest, &pending)?;
            rows.extend(others);
            let token =
                frontier.successor_token(held.token(), &row_ids(&rows)?, operation_id, true)?;
            frontier.reservation(&tx::files_at(&bank)?, &rows, &token)?;
            writes::fault("layout-recovery-ledger-generation-before-head")?;
            advance_ledger(
                &root,
                operation_id,
                field(&successor, "units")?,
                &head,
                owner,
                None,
                Some(text(&successor, "unit_map_sha256")?),
                runtime_root,
            )?;
        } else if same(&head, &expected_head) {
            let retained = read_value(&directory.join("ledger").join(
                crate::layout_resolution::generation_name(field(&head, "generation")?)?,
            ))?;
            if !same(&retained, &successor) {
                return Err(err("Revision head has missing successor evidence"));
            }
            // Head is the authority after its durable swap. Repair only this
            // exact journal-pinned index lag, never select an arbitrary ledger.
            let mut indexed = state::index(&root, runtime_root)?;
            let row = with(
                operation_row(&indexed, operation_id)?.clone(),
                [("ledger_head_sha256", string(sha(&head)?))],
            );
            crate::layout_resolution::set_operation_row(&mut indexed, operation_id, row);
            publish_index(&root, &base, &indexed, runtime_root)?;
        } else {
            return Err(err("Revision ledger generation changed"));
        }
        writes::fault("layout-recovery-ledger-generation-after-head")?;
        let held = require_hold(frontier.verified_hold_value(&root, owner, None, runtime_root)?)?;
        let rows = frontier.pending_rows(&root, held.token(), owner, None, runtime_root)?;
        frontier.reservation(&tx::files_at(&bank)?, &rows, held.token())?;
        let receipt = object([
            ("schema", string(RECEIPT_SCHEMA)),
            ("revision_id", string(revision_id)),
            ("operation_id", string(operation_id)),
            ("proposal_sha256", string(sha(proposal)?)),
            ("ledger_head_sha256", string(sha(&expected_head)?)),
            ("verified", Value::Bool(true)),
            ("readback_at", string(storage::now(Sources::default())?)),
        ]);
        state::write_once(&location.join("receipt.json"), &receipt)?;
        frontier.invalidate(&root)?;
        Ok((receipt, held))
    })?;
    let ids = unit_ids(held.token())?;
    Ok(crate::layout_recovery::result(
        "revise-units",
        Some(operation_id),
        vec![
            ("state", string("units-revised")),
            ("verified", Value::Bool(true)),
            ("receipt", receipt),
            ("pending_unit_ids", field(held.token(), "unit_ids")?.clone()),
            ("pending_units", number(ids.len())),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
        ],
    ))
}
