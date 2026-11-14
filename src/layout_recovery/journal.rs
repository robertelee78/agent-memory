use super::*;
use crate::layout_recovery_context::RecoveryReadSet;
use crate::layout_recovery_temps as temps;
use crate::layout_resolution::{lstat, operation_row, read_optional};
use crate::{layout_history, layout_restore};

fn by_path(rows: &[Value]) -> Result<BTreeMap<String, Value>> {
    rows.iter()
        .map(|r| Ok((text(r, "path")?.to_owned(), r.clone())))
        .collect()
}
fn equivalent(observed: Option<&Value>, expected: Option<&Value>, changed: bool) -> Result<bool> {
    let Some(expected) = expected else {
        return Ok(observed.is_none());
    };
    let Some(observed) = observed else {
        return Ok(false);
    };
    if !changed {
        return Ok(same(observed, expected));
    }
    Ok(get(observed, "type").as_str() == Some("file")
        && same(get(observed, "sha256"), get(expected, "sha256"))
        && same(&required_facts(observed)?, &required_facts(expected)?)
        && int(get(observed, "nlink")) == Some(1))
}
pub fn partial_boundary(proposal: &Value, manifest: &Value, current: &[Value]) -> Result<usize> {
    let mut before = by_path(&manifest_view(manifest)?)?;
    let actual = by_path(current)?;
    let directories = field(field(proposal, "metadata_plan")?, "directories")?;
    let intended = field(proposal, "intended_files")?;
    for (path, row) in &actual {
        require(
            before.contains_key(path)
                || intended.get(path).is_some()
                || directories.get(path).is_some(),
            "Interrupted recovery contains an unknown physical occupant",
        )?;
        if let Some(facts) = directories.get(path) {
            require(
                get(row, "type").as_str() == Some("directory")
                    && same(&required_facts(row)?, facts),
                "Interrupted recovery directory metadata changed",
            )?;
        }
    }
    let participants: BTreeSet<String> = before
        .keys()
        .cloned()
        .chain(pairs(intended)?.iter().map(|(p, _)| p.clone()))
        .collect();
    let mut changed = BTreeSet::new();
    let actions = items(field(proposal, "actions")?)?;
    for boundary in 0..=actions.len() {
        let mut valid = true;
        for path in &participants {
            if !equivalent(actual.get(path), before.get(path), changed.contains(path))? {
                valid = false;
                break;
            }
        }
        if valid {
            return Ok(boundary);
        }
        if let Some(action) = actions.get(boundary) {
            let path = text(action, "path")?.to_owned();
            changed.insert(path.clone());
            if get(action, "after") == &Value::Null {
                before.remove(&path);
            } else {
                let facts = field(field(field(proposal, "metadata_plan")?, "files")?, &path)?;
                before.insert(
                    path,
                    with(
                        facts.clone(),
                        [
                            ("type", string("file")),
                            ("sha256", field(action, "after")?.clone()),
                        ],
                    ),
                );
            }
        }
    }
    Err(err(
        "Interrupted recovery is not an attainable ordered action boundary",
    ))
}
pub fn rollback_boundary(plan: &Value, manifest: &Value, current: &[Value]) -> Result<usize> {
    let actual = by_path(current)?;
    let mut expected = by_path(items(field(plan, "initial_view")?)?)?;
    let original = by_path(items(field(manifest, "physical_entries")?)?)?;
    let participants: BTreeSet<String> = actual.keys().chain(expected.keys()).cloned().collect();
    let mut changed = BTreeSet::new();
    let actions = items(field(plan, "actions")?)?;
    for boundary in 0..=actions.len() {
        let mut valid = true;
        for path in &participants {
            let row = actual.get(path);
            let desired = expected.get(path);
            let removed_directory = !changed.contains(path)
                && boundary == actions.len()
                && desired.is_some_and(|r| get(r, "type").as_str() == Some("directory"))
                && !original.contains_key(path)
                && row.is_none();
            if !removed_directory && !equivalent(row, desired, changed.contains(path))? {
                valid = false;
                break;
            }
        }
        if valid {
            return Ok(boundary);
        }
        if let Some(action) = actions.get(boundary) {
            let path = text(action, "path")?.to_owned();
            changed.insert(path.clone());
            if get(action, "after") == &Value::Null {
                expected.remove(&path);
            } else {
                expected.insert(path.clone(),original.get(&path).ok_or_else(|| err("Interrupted rollback contains an impossible ordering or external edit"))?.clone());
            }
        }
    }
    Err(err(
        "Interrupted rollback contains an impossible ordering or external edit",
    ))
}
pub fn probe_requirements(
    proposal: &Value,
    manifest: &Value,
    actions: &[Value],
    mode: &str,
) -> Result<Vec<Value>> {
    let old = items(field(manifest, "physical_entries")?)?;
    let mut result = Vec::new();
    for action in actions {
        let path = text(action, "path")?;
        let needs_old = (mode == "initial" && get(action, "before") != &Value::Null)
            || (mode == "rollback" && get(action, "after") != &Value::Null);
        if needs_old {
            if let Some(row) = old.iter().find(|r| {
                get(r, "type").as_str() == Some("file") && get(r, "path").as_str() == Some(path)
            }) {
                result.push(object([
                    ("path", string(path)),
                    ("facts", required_facts(row)?),
                ]));
            }
        }
        if ["initial", "finish"].contains(&mode) && get(action, "after") != &Value::Null {
            result.push(object([
                ("path", string(path)),
                (
                    "facts",
                    field(field(field(proposal, "metadata_plan")?, "files")?, path)?.clone(),
                ),
            ]));
        }
    }
    Ok(result)
}
pub fn recovery_source(
    repo: &Path,
    operation_id: &str,
    runtime_root: &Path,
) -> Result<RecoverySource> {
    let op = operation(repo, operation_id, runtime_root)?;
    let journal = read_value(&op.directory.join("journal.json"))?;
    require(
        get(&journal, "schema").as_str() == Some(JOURNAL)
            && get(&journal, "operation_id").as_str() == Some(operation_id),
        "Known recovery requires its exact typed journal",
    )?;
    let proposal = read_value(
        &op.directory
            .join("proposals")
            .join(format!("{}.json", text(&journal, "proposal_id")?)),
    )?;
    proposal_value(&op.root, &proposal, runtime_root)?;
    let probes = temps::probe_plan(
        operation_id,
        &probe_requirements(
            &proposal,
            &op.manifest,
            items(field(&proposal, "actions")?)?,
            "initial",
        )?,
        field(field(&proposal, "metadata_plan")?, "directories")?,
    )?;
    require(
        get(&journal, "proposal_sha256").as_str() == Some(sha(&proposal)?.as_str())
            && get(&journal, "manifest_sha256").as_str() == Some(sha(&op.manifest)?.as_str())
            && same(
                get(&journal, "review_sha256"),
                get(&proposal, "review_sha256"),
            )
            && same(get(&journal, "metadata_probes"), &probes),
        "Known recovery journal/proposal/source pins do not agree",
    )?;
    Ok(RecoverySource {
        operation: op,
        proposal,
        journal,
    })
}
pub fn recovery_plan(
    directory: &Path,
    proposal: &Value,
    manifest: &Value,
    current: &[Value],
    mode: &str,
) -> Result<Option<Value>> {
    if mode == "finish" {
        partial_boundary(proposal, manifest, current)?;
        return Ok(None);
    }
    if let Some(active) = read_optional(&directory.join("rollback-plan.json"))? {
        fields(
            &active,
            &[
                "schema",
                "operation_id",
                "proposal_sha256",
                "initial_view",
                "actions",
            ],
            Some("agent-memory/layout-rollback-plan-v1"),
        )?;
        require(
            same(get(&active, "operation_id"), get(proposal, "operation_id"))
                && get(&active, "proposal_sha256").as_str() == Some(sha(proposal)?.as_str()),
            "Rollback plan is foreign or corrupt",
        )?;
        rollback_boundary(&active, manifest, current)?;
        return Ok(Some(active));
    }
    partial_boundary(proposal, manifest, current)?;
    let sources = items(field(manifest, "sources")?)?;
    let now = current
        .iter()
        .filter(|r| {
            get(r, "type").as_str() == Some("file") && get(r, "runtime").as_bool() != Some(true)
        })
        .collect::<Vec<_>>();
    let paths = sources
        .iter()
        .chain(now.iter().copied())
        .map(|r| text(r, "path").map(str::to_owned))
        .collect::<Result<BTreeSet<_>>>()?;
    let mut actions = Vec::new();
    for path in router_last(paths) {
        let original = row_at(sources, &path).map_or(&Value::Null, |r| get(r, "sha256"));
        let current = now
            .iter()
            .find(|r| get(r, "path").as_str() == Some(path.as_str()))
            .map_or(&Value::Null, |r| get(r, "sha256"));
        if !same(original, current) {
            actions.push(object([
                ("path", string(path)),
                ("after", original.clone()),
            ]));
        }
    }
    Ok(Some(object([
        ("schema", string("agent-memory/layout-rollback-plan-v1")),
        ("operation_id", field(proposal, "operation_id")?.clone()),
        ("proposal_sha256", string(sha(proposal)?)),
        ("initial_view", Value::Array(current.to_vec())),
        ("actions", Value::Array(actions)),
    ])))
}
#[allow(clippy::too_many_arguments)]
pub fn run_journal(
    root: &Path,
    target: &Path,
    directory: &Path,
    manifest: &Value,
    proposal: &Value,
    owner: &[u8],
    transaction: &[u8],
    mode: &str,
    runtime_root: &Path,
) -> Result<Value> {
    let context = RecoveryReadSet::observe(
        root,
        manifest,
        proposal,
        Some(owner),
        transaction,
        runtime_root,
    )?;
    let current = view(target, &[])?;
    let (boundary, actions) = if mode == "finish" {
        let boundary = partial_boundary(proposal, manifest, &current)?;
        new_directories(target, proposal)?;
        (boundary, items(field(proposal, "actions")?)?.to_vec())
    } else {
        let plan = read_value(&directory.join("rollback-plan.json"))?;
        (
            rollback_boundary(&plan, manifest, &current)?,
            items(field(&plan, "actions")?)?.to_vec(),
        )
    };
    let mut running = view(target, &[])?;
    for (index, action) in actions.iter().enumerate().skip(boundary) {
        let path = text(action, "path")?;
        if path == "MEMORY.md" {
            writes::fault("layout-recovery-before-router")?;
        }
        let (body, facts) = if mode == "finish" {
            let body = get(proposal, "intended_files")
                .get(path)
                .map(|v| {
                    v.as_str()
                        .map(|s| s.as_bytes().to_vec())
                        .ok_or_else(|| err("Recovery intended files require text"))
                })
                .transpose()?;
            (
                body,
                get(get(proposal, "metadata_plan"), "files")
                    .get(path)
                    .cloned(),
            )
        } else {
            let source = row_at(items(field(manifest, "sources")?)?, path);
            let body = source.map(|s| source_bytes(directory, s)).transpose()?;
            let facts = row_at(items(field(manifest, "physical_entries")?)?, path)
                .map(required_facts)
                .transpose()?;
            (body, facts)
        };
        replace_source(
            root,
            target,
            path,
            body.as_deref(),
            facts.as_ref(),
            &running,
            &context,
            runtime_root,
        )?;
        running = view(target, &[])?;
        let progress = object([
            ("schema", string("agent-memory/layout-recovery-progress-v1")),
            ("operation_id", field(proposal, "operation_id")?.clone()),
            ("mode", string(mode)),
            ("path", string(path)),
            (
                "observed_entry",
                row_at(&running, path).cloned().unwrap_or(Value::Null),
            ),
            (
                "inventory_sha256",
                string(sha(&Value::Array(running.clone()))?),
            ),
            (
                "observed_at",
                string(crate::local_registry::now(host::Sources::default())?),
            ),
        ]);
        state::write_once(
            &directory
                .join("progress")
                .join(format!("{mode}-{index:08}.json")),
            &progress,
        )?;
        writes::fault(&format!("layout-recovery-after-progress:{path}"))?;
        if path == "MEMORY.md" {
            writes::fault("layout-recovery-after-router")?;
        }
    }
    if mode == "rollback" {
        remove_directories(target, proposal)?;
        running = view(target, &[])?;
    }
    writes::fault("layout-recovery-before-final-readback")?;
    expected_result(proposal, manifest, &running, mode == "rollback")?;
    context.check(runtime_root)?;
    let eligible = eligibility(root, Some(owner), Some(transaction));
    require(
        mode != "finish" || eligible == Value::Bool(true),
        "Intended native result failed the common complete bank eligibility check",
    )?;
    writes::fault("layout-recovery-before-receipt")?;
    let mut units = items(field(proposal, "pending_units")?)?
        .iter()
        .map(|u| text(u, "unit_id").map(str::to_owned))
        .collect::<Result<Vec<_>>>()?;
    units.extend(
        items(field(proposal, "inherited_unit_ids")?)?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| err(crate::layout_resolution::MALFORMED))
            })
            .collect::<Result<Vec<_>>>()?,
    );
    units.sort();
    let receipt = object([
        ("schema", string(RECEIPT)),
        ("operation_id", field(proposal, "operation_id")?.clone()),
        ("project_id", field(proposal, "project_id")?.clone()),
        ("proposal_id", field(proposal, "proposal_id")?.clone()),
        ("request_sha256", string(sha(proposal)?)),
        (
            "original_manifest_sha256",
            field(proposal, "manifest_sha256")?.clone(),
        ),
        (
            "intended_manifest_sha256",
            string(sha(field(proposal, "intended_files")?)?),
        ),
        (
            "outcome",
            string(if mode == "finish" {
                "layout-restored"
            } else {
                "rolled-back"
            }),
        ),
        (
            "readback_at",
            string(crate::local_registry::now(host::Sources::default())?),
        ),
        ("verified", Value::Bool(true)),
        ("byte_metadata_readback_verified", Value::Bool(true)),
        ("exact_xattr_readback_verified", Value::Bool(true)),
        ("resulting_view", Value::Array(running)),
        (
            "changed_memory_paths",
            Value::Array(
                items(field(proposal, "actions")?)?
                    .iter()
                    .map(|a| field(a, "path").cloned())
                    .collect::<Result<_>>()?,
            ),
        ),
        ("canonical_eligibility_at_commit", eligible),
        (
            "pending_unit_ids_at_commit",
            Value::Array(units.iter().map(|s| string(s.as_str())).collect()),
        ),
        (
            "semantic_recovery_complete_at_commit",
            Value::Bool(mode == "finish" && units.is_empty()),
        ),
        ("original_index_writes_performed", Value::Bool(false)),
        ("head_writes_performed", Value::Bool(false)),
        ("feature_writes_performed", Value::Bool(false)),
    ]);
    state::write_once(&directory.join("receipt.json"), &receipt)?;
    writes::fault("layout-recovery-after-receipt")?;
    finish_index(
        root,
        proposal,
        text(&receipt, "outcome")?,
        owner,
        transaction,
        runtime_root,
    )?;
    Ok(receipt)
}
pub fn historical(
    repo: &Path,
    proposal: &Value,
    directory: &Path,
    receipt: &Value,
    runtime_root: &Path,
) -> Result<Value> {
    layout_history::check(
        repo,
        field(proposal, "scope_context")?,
        text(proposal, "project_id")?,
        runtime_root,
    )?;
    let mut matched = string("unavailable");
    if lstat(&directory.join("manifest.json"))?.is_some() {
        matched = Value::Bool(
            (|| -> Result<bool> {
                let op = operation(repo, text(proposal, "operation_id")?, runtime_root)?;
                let current = view(&op.bank, &[])?;
                expected_result(
                    proposal,
                    &op.manifest,
                    &current,
                    get(receipt, "outcome").as_str() == Some("rolled-back"),
                )?;
                Ok(same(
                    &Value::Array(view(&op.bank, &[])?),
                    get(receipt, "resulting_view"),
                ))
            })()
            .unwrap_or(false),
        );
    }
    Ok(result(
        "apply",
        Some(text(proposal, "operation_id")?),
        [
            ("state", field(receipt, "outcome")?.clone()),
            ("verified", Value::Bool(true)),
            ("receipt", receipt.clone()),
            ("receipt_is_historical", Value::Bool(true)),
            ("already_applied", Value::Bool(true)),
            ("current_receipt_match", matched),
            ("canonical_eligibility", eligibility(repo, None, None)),
            ("semantic_recovery_complete", Value::Bool(false)),
            (
                "pending_unit_ids",
                field(receipt, "pending_unit_ids_at_commit")?.clone(),
            ),
            (
                "pending_units",
                number(items(field(receipt, "pending_unit_ids_at_commit")?)?.len()),
            ),
            (
                "required_actions",
                Value::Array(vec![string(
                    "Consult current status; historical receipt does not assert current semantic coverage",
                )]),
            ),
        ],
    ))
}
pub fn apply(repo: &Path, supplied: &Value, runtime_root: &Path) -> Result<Value> {
    let proposal = proposal_value(repo, supplied, runtime_root)?;
    let id = text(&proposal, "operation_id")?;
    let (root, target, _, directory) = operation_paths(repo, id, runtime_root)?;
    if let Some(known) = receipt_value(&directory, &proposal)? {
        return historical(&root, &proposal, &directory, &known, runtime_root);
    }
    let op = operation(&root, id, runtime_root)?;
    let manifest = &op.manifest;
    let receipt = locked(&root, &target, id, |owner| {
        require_completed_resolutions(&root, None, runtime_root)?;
        let (predecessor, pinned) =
            predecessor_value(&root, Some(manifest), Some(owner), runtime_root)?;
        require(
            same(get(&proposal, "predecessor"), &pinned),
            "Reviewed recovery predecessor changed",
        )?;
        let before = predecessor.as_ref().map(|p| p.marker());
        raw::recheck(&root, manifest, Some(owner), before)?;
        check_review(&root, &directory, manifest, &proposal, runtime_root)?;
        let capacity = frontier()?.prospective_token(
            &root,
            manifest,
            items(field(&proposal, "pending_units")?)?,
            predecessor.as_ref(),
            Some(owner),
            runtime_root,
        )?;
        require(
            same(&capacity, get(&proposal, "capacity_token")),
            "Actual complete recovery reservation changed before installation",
        )?;
        let transaction = marker(&proposal, "apply")?;
        let probes = temps::probe_plan(
            id,
            &probe_requirements(
                &proposal,
                manifest,
                items(field(&proposal, "actions")?)?,
                "initial",
            )?,
            field(field(&proposal, "metadata_plan")?, "directories")?,
        )?;
        let journal = object([
            ("schema", string(JOURNAL)),
            ("operation_id", string(id)),
            ("proposal_id", field(&proposal, "proposal_id")?.clone()),
            ("proposal_sha256", string(sha(&proposal)?)),
            (
                "manifest_sha256",
                field(&proposal, "manifest_sha256")?.clone(),
            ),
            ("review_sha256", field(&proposal, "review_sha256")?.clone()),
            ("marker", crate::developer_identity::load(&transaction)?),
            ("hold", crate::developer_identity::load(&hold(&proposal)?)?),
            ("actions", field(&proposal, "actions")?.clone()),
            ("original_view", Value::Array(manifest_view(manifest)?)),
            (
                "initial_ledger_sha256",
                string(sha(&ledger_value(&proposal)?)?),
            ),
            ("parent_operation_id", Value::Null),
            ("parent_receipt_sha256", Value::Null),
            ("metadata_probes", probes.clone()),
        ]);
        writes::fault("layout-recovery-before-journal")?;
        state::write_once(&directory.join("journal.json"), &journal)?;
        writes::fault("layout-recovery-after-journal")?;
        writes::fault("layout-recovery-probe-journal")?;
        temps::probe(
            &target,
            &probes,
            &directory,
            None,
            None,
            Some(owner),
            before,
        )?;
        rebind(&root, manifest, Some(owner), before, runtime_root)?;
        require(
            same_rows(&view(&target, &[])?, &manifest_view(manifest)?),
            "Recovery source changed during metadata preflight",
        )?;
        ensure_setup(
            &root,
            &directory,
            &proposal,
            owner,
            predecessor.as_ref(),
            runtime_root,
        )?;
        writes::fault("layout-recovery-before-marker")?;
        let held;
        let before = if predecessor.is_none()
            && (!items(field(&proposal, "pending_units")?)?.is_empty()
                || !items(field(&proposal, "inherited_unit_ids")?)?.is_empty())
        {
            held = hold(&proposal)?;
            atomic_marker(&target, None, &held, Some(owner), MARKER, None)?;
            writes::fault("layout-recovery-after-hold")?;
            Some(held.as_slice())
        } else {
            before
        };
        atomic_marker(&target, before, &transaction, Some(owner), MARKER, None)?;
        install_sentinel(&root, &target, &proposal, owner, &transaction, runtime_root)?;
        writes::fault("layout-recovery-after-marker")?;
        run_journal(
            &root,
            &target,
            &directory,
            manifest,
            &proposal,
            owner,
            &transaction,
            "finish",
            runtime_root,
        )
    })?;
    let actions = items(field(&proposal, "actions")?)?;
    Ok(result(
        "apply",
        Some(id),
        [
            ("state", field(&receipt, "outcome")?.clone()),
            ("verified", Value::Bool(true)),
            (
                "canonical_eligibility",
                field(&receipt, "canonical_eligibility_at_commit")?.clone(),
            ),
            (
                "semantic_recovery_complete",
                field(&receipt, "semantic_recovery_complete_at_commit")?.clone(),
            ),
            (
                "pending_unit_ids",
                field(&receipt, "pending_unit_ids_at_commit")?.clone(),
            ),
            (
                "pending_units",
                number(items(field(&receipt, "pending_unit_ids_at_commit")?)?.len()),
            ),
            ("writes_performed", Value::Bool(!actions.is_empty())),
            (
                "canonical_writes_performed",
                Value::Bool(!actions.is_empty()),
            ),
            (
                "history_writes_performed",
                Value::Bool(actions.iter().any(|a| {
                    get(a, "path")
                        .as_str()
                        .is_some_and(|p| p.starts_with("history/"))
                })),
            ),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
            ("receipt", receipt),
            ("receipt_is_historical", Value::Bool(false)),
            ("current_receipt_match", Value::Bool(true)),
            ("recall", recall_after(&root)),
        ],
    ))
}
pub fn recall_after(repo: &Path) -> Value {
    let value = (|| -> Result<Value> {
        let token = frontier()?.observe(repo)?;
        let request = object([
            ("project_id", field(&token, "project_id")?.clone()),
            ("query", string("")),
            ("ids", Value::Array(vec![])),
            ("paths", Value::Array(vec![])),
            ("context", object([] as [(&str, Value); 0])),
            (
                "kinds",
                Value::Array(
                    [
                        "semantic-fact",
                        "user-feedback",
                        "project-decision",
                        "episodic-attempt",
                        "result",
                        "reference",
                        "procedural-candidate",
                    ]
                    .into_iter()
                    .map(string)
                    .collect(),
                ),
            ),
            ("minimum_trust", string("reviewed")),
            ("limit", number(8)),
            ("include_context", Value::Bool(false)),
        ]);
        Ok(object([
            ("state", string("evaluated")),
            (
                "result",
                crate::retrieval::retrieve(repo, &request, None, &[])?,
            ),
        ]))
    })();
    value.unwrap_or_else(|_| object([("state",string("pending")),("required_actions",Value::Array(vec![string("Run ordinary qualified recall with applicable context; the historical layout receipt remains committed")]))]))
}
fn optional_sha(path: &Path) -> Result<Value> {
    Ok(crate::developer_identity::digest(
        state::read_bytes(path, sharing_limits::MAX_PRIVATE, true)?.as_deref(),
    )
    .map_or(Value::Null, string))
}
fn bytes_sha(bytes: Option<&[u8]>) -> Value {
    crate::developer_identity::digest(bytes).map_or(Value::Null, string)
}
fn ignored(pins: &[Value]) -> Result<Vec<String>> {
    pins.iter()
        .map(|p| text(p, "path").map(str::to_owned))
        .collect()
}
fn temporary_entries(
    target: &Path,
    proposal: &Value,
    manifest: &Value,
    runtime_root: &Path,
    root: &Path,
) -> Result<Vec<Value>> {
    temps::entries(
        target,
        text(proposal, "operation_id")?,
        items(field(proposal, "actions")?)?,
        &manifest_view(manifest)?,
        &files_facts(proposal)?,
        &marker_payloads(root, proposal, runtime_root)?,
    )
}
#[allow(clippy::too_many_arguments)]
fn recovery_checks(
    root: &Path,
    target: &Path,
    base: &Path,
    directory: &Path,
    proposal: &Value,
    journal: &Value,
    current_marker: Option<&[u8]>,
    known: Option<&Value>,
    temporary: &[Value],
    probes: &[Value],
) -> Result<Value> {
    Ok(object([
        ("repo_root", string(path_text(root)?)),
        ("proposal_sha256", string(sha(proposal)?)),
        ("journal_sha256", string(sha(journal)?)),
        ("marker_sha256", bytes_sha(current_marker)),
        (
            "sentinel_sha256",
            bytes_sha(frontier()?.marker_bytes(target, SENTINEL, None)?.as_deref()),
        ),
        ("index_sha256", optional_sha(&base.join("index.json"))?),
        (
            "ledger_head_sha256",
            optional_sha(&directory.join("ledger-head.json"))?,
        ),
        (
            "receipt_sha256",
            known.map(sha).transpose()?.map_or(Value::Null, string),
        ),
        (
            "observed_view",
            Value::Array(view(target, &ignored(probes)?)?),
        ),
        ("temporary_entries", Value::Array(temporary.to_vec())),
        ("probe_entries", Value::Array(probes.to_vec())),
        ("git", raw::git_pins(root)?),
    ]))
}
pub fn recover(
    repo: &Path,
    operation_id: Option<&str>,
    mode: Option<&str>,
    supplied: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    if supplied.is_some_and(|p| {
        get(p.get("proposal").unwrap_or(p), "schema").as_str()
            == Some("agent-memory/layout-child-recovery-v1")
    }) {
        return layout_restore::recover(repo, None, None, supplied, runtime_root);
    }
    if supplied.is_none() {
        let id = operation_id
            .ok_or_else(|| err("Recovery operations require canonical UUIDv4 identifiers"))?;
        let (_, _, _, directory) = operation_paths(repo, id, runtime_root)?;
        let known_journal = read_value(&directory.join("journal.json"))?;
        if get(&known_journal, "parent_operation_id") != &Value::Null {
            return layout_restore::recover(repo, Some(id), mode, None, runtime_root);
        }
        let source = recovery_source(repo, id, runtime_root)?;
        let op = source.operation;
        let original = source.proposal;
        let journal = source.journal;
        let mode = mode
            .filter(|m| ["finish", "rollback"].contains(m))
            .ok_or_else(|| err("Known recovery needs an explicit finish or rollback mode"))?;
        let current_marker = frontier()?.marker_bytes(&op.bank, MARKER, None)?;
        bank_observation::markers(&op.bank, None, current_marker.as_deref())?;
        require(
            current_marker.as_ref().is_none_or(|m| {
                marker_payloads(&op.root, &original, runtime_root)
                    .is_ok_and(|values| values.contains(m))
            }),
            "Unknown operation marker; preserve it",
        )?;
        let temporary =
            temporary_entries(&op.bank, &original, &op.manifest, runtime_root, &op.root)?;
        let probes =
            temps::probe_entries(&op.bank, field(&journal, "metadata_probes")?, &op.directory)?;
        let mut ignore = ignored(&temporary)?;
        ignore.extend(ignored(&probes)?);
        let current = view(&op.bank, &ignore)?;
        let known = receipt_value(&op.directory, &original)?;
        if let Some(known) = &known {
            if mode == "rollback" && get(known, "outcome").as_str() == Some("layout-restored") {
                return rollback(repo, Some(id), None, runtime_root);
            }
            require(
                (mode == "finish") == (get(known, "outcome").as_str() == Some("layout-restored")),
                "A durable historical outcome cannot be reversed by interrupted recovery",
            )?;
            expected_result(
                &original,
                &op.manifest,
                &current,
                get(known, "outcome").as_str() == Some("rolled-back"),
            )?;
            require(
                same_rows(&current, items(field(known, "resulting_view")?)?),
                "Durable receipt result changed; do not replay cleanup over later edits",
            )?;
        } else {
            recovery_plan(&op.directory, &original, &op.manifest, &current, mode)?;
        }
        rebind(
            &op.root,
            &op.manifest,
            None,
            current_marker.as_deref(),
            runtime_root,
        )?;
        if mode == "finish" {
            check_review(
                &op.root,
                &op.directory,
                &op.manifest,
                &original,
                runtime_root,
            )?;
        }
        let checks = recovery_checks(
            &op.root,
            &op.bank,
            &op.base,
            &op.directory,
            &original,
            &journal,
            current_marker.as_deref(),
            known.as_ref(),
            &temporary,
            &probes,
        )?;
        let preview = with(
            checks,
            [
                ("schema", string("agent-memory/layout-known-recovery-v1")),
                ("recovery_id", string(host::uuid4()?)),
                ("operation_id", string(id)),
                ("mode", string(mode)),
                (
                    "rollback_plan",
                    if known.is_none() {
                        recovery_plan(&op.directory, &original, &op.manifest, &current, mode)?
                            .unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    },
                ),
            ],
        );
        state::write_once(
            &op.directory
                .join("recovery")
                .join(format!("{}.json", text(&preview, "recovery_id")?)),
            &preview,
        )?;
        return Ok(result(
            "recover",
            Some(id),
            [
                ("state", string("recovery-preview")),
                ("verified", Value::Bool(true)),
                ("proposal", preview),
                ("private_artifact_writes_performed", Value::Bool(true)),
                ("canonical_eligibility", eligibility(repo, None, None)),
                (
                    "required_actions",
                    Value::Array(vec![string(
                        "Apply this exact known-operation preview under the bank lock",
                    )]),
                ),
            ],
        ));
    }
    let supplied = supplied.ok_or_else(|| err(crate::layout_resolution::MALFORMED))?;
    let preview = supplied.get("proposal").unwrap_or(supplied);
    fields(
        preview,
        &[
            "schema",
            "recovery_id",
            "operation_id",
            "repo_root",
            "mode",
            "proposal_sha256",
            "journal_sha256",
            "marker_sha256",
            "sentinel_sha256",
            "index_sha256",
            "ledger_head_sha256",
            "receipt_sha256",
            "observed_view",
            "temporary_entries",
            "probe_entries",
            "git",
            "rollback_plan",
        ],
        Some("agent-memory/layout-known-recovery-v1"),
    )?;
    let id = text(preview, "operation_id")?;
    let mode = text(preview, "mode")?;
    require(
        ["finish", "rollback"].contains(&mode),
        "Known recovery needs an explicit finish or rollback mode",
    )?;
    let (root, target, base, directory) = operation_paths(repo, id, runtime_root)?;
    require(
        uuid(get(preview, "recovery_id")),
        "Known recovery preview is unknown or changed",
    )?;
    require(
        same(
            &read_value(
                &directory
                    .join("recovery")
                    .join(format!("{}.json", text(preview, "recovery_id")?)),
            )?,
            preview,
        ),
        "Known recovery preview is unknown or changed",
    )?;
    if let Some(completed) = layout_history::completed(&directory, preview)? {
        let original = read_value(
            &directory
                .join("proposals")
                .join(format!("{}.json", text(&completed, "proposal_id")?)),
        )?;
        proposal_value(&root, &original, runtime_root)?;
        layout_history::check(
            &root,
            field(&original, "scope_context")?,
            text(&original, "project_id")?,
            runtime_root,
        )?;
        return Ok(result(
            "recover",
            Some(id),
            [
                ("state", field(&completed, "outcome")?.clone()),
                ("verified", Value::Bool(true)),
                ("receipt", completed),
                ("receipt_is_historical", Value::Bool(true)),
                ("already_applied", Value::Bool(true)),
            ],
        ));
    }
    let source = recovery_source(repo, id, runtime_root)?;
    let manifest = source.operation.manifest;
    let original = source.proposal;
    let journal = source.journal;
    let (receipt, was_known) = locked(&root, &target, id, |owner| {
        require_completed_resolutions(&root, None, runtime_root)?;
        let mut current_marker = frontier()?.marker_bytes(&target, MARKER, None)?;
        let known = receipt_value(&directory, &original)?;
        let temporary = temporary_entries(&target, &original, &manifest, runtime_root, &root)?;
        let probes =
            temps::probe_entries(&target, field(&journal, "metadata_probes")?, &directory)?;
        let checks = recovery_checks(
            &root,
            &target,
            &base,
            &directory,
            &original,
            &journal,
            current_marker.as_deref(),
            known.as_ref(),
            &temporary,
            &probes,
        )?;
        require(
            pairs(&checks)?
                .iter()
                .all(|(k, v)| same(get(preview, k), v)),
            "Known recovery preview became stale; preserve newer state",
        )?;
        rebind(
            &root,
            &manifest,
            Some(owner),
            current_marker.as_deref(),
            runtime_root,
        )?;
        temps::clean(&target, &temporary, Some(owner), current_marker.as_deref())?;
        temps::probe_clean(&target, &probes, Some(owner), current_marker.as_deref())?;
        temps::probe_complete(&target, field(&journal, "metadata_probes")?, &directory)?;
        let cleaned = view(&target, &[])?;
        let was_known = known.is_some();
        let receipt = if let Some(known) = known {
            expected_result(
                &original,
                &manifest,
                &cleaned,
                get(&known, "outcome").as_str() == Some("rolled-back"),
            )?;
            require(
                same(&Value::Array(cleaned), get(&known, "resulting_view")),
                "Durable receipt result changed; do not replay cleanup over later edits",
            )?;
            let index = state::index(&root, runtime_root)?;
            if current_marker.is_none()
                && (frontier()?.marker_bytes(&target, SENTINEL, None)?.is_some()
                    || get(operation_row(&index, id)?, "state").as_str() == Some("active"))
            {
                current_marker = Some(marker(&original, mode)?);
                atomic_marker(
                    &target,
                    None,
                    current_marker
                        .as_deref()
                        .ok_or_else(|| err(crate::layout_resolution::MALFORMED))?,
                    Some(owner),
                    MARKER,
                    None,
                )?;
            }
            if let Some(transaction) = &current_marker {
                finish_index(
                    &root,
                    &original,
                    text(&known, "outcome")?,
                    owner,
                    transaction,
                    runtime_root,
                )?;
            }
            known
        } else {
            let plan = if mode == "finish" {
                check_review(&root, &directory, &manifest, &original, runtime_root)?;
                None
            } else {
                let plan = recovery_plan(&directory, &original, &manifest, &cleaned, "rollback")?
                    .ok_or_else(|| err("Rollback action plan changed"))?;
                require(
                    same(&plan, get(preview, "rollback_plan")),
                    "Rollback action plan changed",
                )?;
                state::write_once(&directory.join("rollback-plan.json"), &plan)?;
                Some(plan)
            };
            let (boundary, actions, directories) = if mode == "finish" {
                let boundary = partial_boundary(&original, &manifest, &cleaned)?;
                let mut directories = Vec::new();
                for (path, _) in pairs(field(field(&original, "metadata_plan")?, "directories")?)? {
                    if lstat(&target.join(path))?.is_none() {
                        directories.push(string(path));
                    }
                }
                (
                    boundary,
                    items(field(&original, "actions")?)?,
                    Value::Array(directories),
                )
            } else {
                let plan = plan
                    .as_ref()
                    .ok_or_else(|| err("Rollback action plan changed"))?;
                (
                    rollback_boundary(plan, &manifest, &cleaned)?,
                    items(field(plan, "actions")?)?,
                    Value::Array(vec![]),
                )
            };
            let remaining = actions.get(boundary..).ok_or_else(|| {
                err("Interrupted recovery is not an attainable ordered action boundary")
            })?;
            let requirements =
                Value::Array(probe_requirements(&original, &manifest, remaining, mode)?);
            temps::probe(
                &target,
                field(&journal, "metadata_probes")?,
                &directory,
                Some(&requirements),
                Some(&directories),
                Some(owner),
                current_marker.as_deref(),
            )?;
            require(
                same_rows(&view(&target, &[])?, &cleaned),
                "Recovery sources changed during metadata preflight",
            )?;
            let before_setup = predecessor_marker(&root, &original, runtime_root)?;
            let predecessor = if get(&state::index(&root, runtime_root)?, "operations")
                .get(id)
                .is_none()
            {
                predecessor_value(&root, Some(&manifest), Some(owner), runtime_root)?.0
            } else {
                None
            };
            ensure_setup(
                &root,
                &directory,
                &original,
                owner,
                predecessor.as_ref(),
                runtime_root,
            )?;
            let transaction = marker(&original, mode)?;
            if current_marker.as_deref() != Some(transaction.as_slice()) {
                if current_marker.is_none()
                    && (!items(field(&original, "pending_units")?)?.is_empty()
                        || !items(field(&original, "inherited_unit_ids")?)?.is_empty())
                {
                    let held = hold(&original)?;
                    atomic_marker(&target, None, &held, Some(owner), MARKER, None)?;
                    current_marker = Some(held);
                }
                if current_marker == before_setup && before_setup.is_some() {
                    predecessor_marker(&root, &original, runtime_root)?;
                }
                atomic_marker(
                    &target,
                    current_marker.as_deref(),
                    &transaction,
                    Some(owner),
                    MARKER,
                    None,
                )?;
            }
            install_sentinel(&root, &target, &original, owner, &transaction, runtime_root)?;
            run_journal(
                &root,
                &target,
                &directory,
                &manifest,
                &original,
                owner,
                &transaction,
                mode,
                runtime_root,
            )?
        };
        state::write_once(
            &directory
                .join("recovery")
                .join(format!("{}.receipt.json", text(preview, "recovery_id")?)),
            &layout_history::recovery_receipt(&directory, preview, &receipt)?,
        )?;
        Ok((receipt, was_known))
    })?;
    Ok(result(
        "recover",
        Some(id),
        [
            ("state", field(&receipt, "outcome")?.clone()),
            ("verified", Value::Bool(true)),
            (
                "canonical_eligibility",
                field(&receipt, "canonical_eligibility_at_commit")?.clone(),
            ),
            (
                "semantic_recovery_complete",
                field(&receipt, "semantic_recovery_complete_at_commit")?.clone(),
            ),
            (
                "pending_unit_ids",
                field(&receipt, "pending_unit_ids_at_commit")?.clone(),
            ),
            (
                "pending_units",
                number(items(field(&receipt, "pending_unit_ids_at_commit")?)?.len()),
            ),
            (
                "canonical_writes_performed",
                Value::Bool(!was_known && !items(field(&original, "actions")?)?.is_empty()),
            ),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
            ("receipt", receipt),
        ],
    ))
}
pub fn rollback(
    repo: &Path,
    operation_id: Option<&str>,
    proposal: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    let Some(proposal) = proposal else {
        return layout_restore::preview(
            repo,
            operation_id
                .ok_or_else(|| err("Recovery operations require canonical UUIDv4 identifiers"))?,
            runtime_root,
        );
    };
    let value = proposal.get("proposal").unwrap_or(proposal);
    if get(value, "schema").as_str() == Some("agent-memory/layout-known-recovery-v1") {
        recover(repo, None, None, Some(value), runtime_root)
    } else {
        layout_restore::apply(repo, value, None, runtime_root)
    }
}
pub fn status(repo: &Path, operation_id: Option<&str>, runtime_root: &Path) -> Result<Value> {
    let (root, target, _, directory) = state::paths(repo, operation_id, runtime_root)?;
    let Some(id) = operation_id else {
        let index = state::index(&root, runtime_root)?;
        let mut rows = pairs(field(&index, "operations")?)?
            .iter()
            .collect::<Vec<_>>();
        rows.sort_by(|(a, _), (b, _)| a.cmp(b));
        let operations = rows
            .into_iter()
            .map(|(id, row)| {
                Ok(object([
                    ("operation_id", string(id)),
                    ("state", field(row, "state")?.clone()),
                ]))
            })
            .collect::<Result<_>>()?;
        return Ok(result(
            "status",
            None,
            [
                ("state", string("observed")),
                ("verified", Value::Bool(true)),
                ("canonical_eligibility", eligibility(&root, None, None)),
                ("operations", Value::Array(operations)),
            ],
        ));
    };
    let directory = directory.ok_or_else(|| err(crate::layout_resolution::MALFORMED))?;
    let tombstone = read_optional(&directory.join("tombstone.json"))?;
    let receipt = read_optional(&directory.join("receipt.json"))?;
    if let Some(tombstone) = tombstone {
        let complete = get(&tombstone, "cleanup_complete").as_bool() == Some(true);
        return Ok(result(
            "status",
            Some(id),
            [
                (
                    "state",
                    string(if complete {
                        "retention-cleaned"
                    } else {
                        "cleanup-interrupted"
                    }),
                ),
                ("verified", Value::Bool(true)),
                ("receipt_is_historical", Value::Bool(true)),
                (
                    "original_evidence_available",
                    if complete {
                        Value::Bool(false)
                    } else {
                        string("unknown")
                    },
                ),
                ("canonical_eligibility", eligibility(&root, None, None)),
                (
                    "historical_outcome",
                    receipt
                        .as_ref()
                        .map_or(Value::Null, |r| get(r, "outcome").clone()),
                ),
            ],
        ));
    }
    if lstat(&directory.join("manifest.json"))?.is_none() {
        let staging = read_value(&directory.join("staging.json"))?;
        require(
            get(&staging, "operation_id").as_str() == Some(id)
                && get(&staging, "observation_complete").as_bool() == Some(false),
            "Unknown incomplete observation evidence",
        )?;
        return Ok(result(
            "status",
            Some(id),
            [
                ("state", string("observation-incomplete")),
                ("verified", Value::Bool(true)),
                ("observation_complete", Value::Bool(false)),
                (
                    "required_actions",
                    Value::Array(vec![string(
                        "Preserve the partial private staging evidence; inspect a new complete operation before prepare",
                    )]),
                ),
            ],
        ));
    }
    let op = operation(&root, id, runtime_root)?;
    let journal = read_optional(&directory.join("journal.json"))?;
    if let Some(probes) = journal.as_ref().and_then(|j| j.get("metadata_probes")) {
        temps::probe_entries(&target, probes, &directory)?;
    }
    if let Some(journal) = &journal {
        if get(journal, "parent_operation_id") != &Value::Null {
            return Ok(result(
                "status",
                Some(id),
                [
                    (
                        "state",
                        receipt
                            .as_ref()
                            .map_or_else(|| string("interrupted"), |r| get(r, "outcome").clone()),
                    ),
                    ("verified", Value::Bool(true)),
                    ("receipt_is_historical", Value::Bool(receipt.is_some())),
                    (
                        "parent_operation_id",
                        field(journal, "parent_operation_id")?.clone(),
                    ),
                    ("canonical_eligibility", eligibility(&root, None, None)),
                ],
            ));
        }
    }
    let index = state::index(&root, runtime_root)?;
    let indexed = get(&index, "operations").get(id);
    let mut units = Vec::new();
    let mut complete = false;
    if let Some(indexed) = indexed {
        let f = frontier()?;
        let (_, _, ledger) = f.ledger(&root, id, indexed, &target, runtime_root)?;
        f.evidence(&directory, &ledger, indexed, runtime_root)?;
        units = items(field(&ledger, "units")?)?
            .iter()
            .filter(|u| get(u, "state").as_str() == Some("pending-admission"))
            .map(|u| field(u, "unit_id").cloned())
            .collect::<Result<_>>()?;
        complete = units.is_empty() && get(indexed, "state").as_str() == Some("terminal");
    }
    let mut matched = string("unavailable");
    if let (Some(receipt), Some(journal)) = (&receipt, &journal) {
        let original = read_value(
            &directory
                .join("proposals")
                .join(format!("{}.json", text(journal, "proposal_id")?)),
        )?;
        receipt_value(&directory, &original)?;
        matched = Value::Bool(
            (|| -> Result<bool> {
                expected_result(
                    &original,
                    &op.manifest,
                    &view(&target, &[])?,
                    get(receipt, "outcome").as_str() == Some("rolled-back"),
                )?;
                Ok(same(
                    &Value::Array(view(&target, &[])?),
                    get(receipt, "resulting_view"),
                ))
            })()
            .unwrap_or(false),
        );
    }
    let marker = frontier()?.marker_bytes(&target, MARKER, None)?;
    let marker_value = marker
        .as_ref()
        .map(|m| crate::developer_identity::load(m))
        .transpose()?;
    let held = marker_value
        .as_ref()
        .is_some_and(|m| get(m, "schema").as_str() == Some(writes::HOLD));
    if indexed.is_some_and(|r| {
        ["active", "rolled-back", "terminal"].contains(&get(r, "state").as_str().unwrap_or(""))
    }) && (marker.is_none() || held)
    {
        let current = frontier()?.observe(&root)?;
        units = items(field(&current, "unit_ids")?)?.to_vec();
        complete &= get(&current, "coverage_complete").as_bool() == Some(true);
    }
    let stage = if marker.is_some() && !held {
        string("interrupted")
    } else if let Some(receipt) = &receipt {
        field(receipt, "outcome")?.clone()
    } else if journal.is_some() {
        string("journaled")
    } else {
        string("observed")
    };
    let eligible = eligibility(&root, None, None);
    complete &= eligible == Value::Bool(true);
    Ok(result(
        "status",
        Some(id),
        [
            ("state", stage),
            ("verified", Value::Bool(true)),
            ("observation_complete", Value::Bool(true)),
            ("canonical_eligibility", eligible),
            ("semantic_recovery_complete", Value::Bool(complete)),
            ("pending_units", number(units.len())),
            ("pending_unit_ids", Value::Array(units)),
            ("current_receipt_match", matched),
            ("receipt_is_historical", Value::Bool(receipt.is_some())),
            (
                "required_actions",
                Value::Array(if complete {
                    vec![]
                } else {
                    vec![string(
                        "Retain originals and complete operation-specific review/recovery or pending-unit actions",
                    )]
                }),
            ),
        ],
    ))
}
