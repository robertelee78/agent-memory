//! Distinct child restoration of an exact completed layout outcome.
use crate::codec::json::Value;
use crate::developer_identity::{encoded, load};
use crate::error::Result;
use crate::fs::Descriptor;
use crate::host::Sources;
use crate::layout_recovery_context::RecoveryReadSet;
use crate::layout_resolution::{
    MALFORMED, err, field, frontier, int, io_error, items, listing, locked, lstat, number,
    operation_paths, operation_row, path_text, publish_index, read_optional, read_value,
    require_completed_resolutions, same, sha, text, unlink_bank_entry, uuid,
};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_observation, bank_transactions as tx, bank_writes as writes, host, layout_history,
    layout_recovery_observation as raw, layout_recovery_state as state,
    layout_recovery_temps as temps, local_registry as storage, recovery_quiescence,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/layout-child-restore-v1";
pub const FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "proposal_id",
    "parent_operation_id",
    "parent_receipt_sha256",
    "parent_proposal_sha256",
    "parent_ledger_head_sha256",
    "manifest_sha256",
    "repo_root",
    "bank_path",
    "project_id",
    "scope_context",
    "base_view",
    "desired_view",
    "actions",
    "directories_to_remove",
    "parent_marker",
    "parent_index_row",
    "frontier",
    "git",
    "prepared_at",
];
/// Baseline layout_recovery.JOURNAL / RECEIPT schemas; layout_recovery reuses these.
pub const RECOVERY_JOURNAL: &str = "agent-memory/layout-recovery-journal-v1";
pub const RECOVERY_RECEIPT: &str = "agent-memory/layout-recovery-receipt-v1";
const CHILD_RECOVERY: &str = "agent-memory/layout-child-recovery-v1";
const CHILD_RECOVERY_FIELDS: &[&str] = &[
    "schema",
    "recovery_id",
    "operation_id",
    "mode",
    "proposal_sha256",
    "observed_view",
    "marker_sha256",
    "journal_sha256",
    "receipt_sha256",
    "git",
    "temporary_entries",
    "probe_entries",
];

/// `layout_recovery.operation(repo, operation_id)`.
pub struct Operation {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub base: PathBuf,
    pub directory: PathBuf,
    pub manifest: Value,
}
/// `layout_recovery.recovery_source(repo, operation_id)`.
pub struct RecoverySource {
    pub operation: Operation,
    pub proposal: Value,
    pub journal: Value,
}

fn by_path(rows: &[Value]) -> BTreeMap<String, &Value> {
    rows.iter()
        .filter_map(|row| Some((get(row, "path").as_str()?.to_owned(), row)))
        .collect()
}
fn rows(value: &Value) -> Result<&[Value]> {
    items(value)
}
/// `path in directories_to_remove` (a reviewed directory plan, or a path list).
fn listed(directories: &Value, path: &str) -> bool {
    match directories {
        Value::Object(pairs) => pairs.iter().any(|(key, _)| key == path),
        Value::Array(values) => values.iter().any(|value| value.as_str() == Some(path)),
        _ => false,
    }
}
fn directory_names(directories: &Value) -> Vec<String> {
    match directories {
        Value::Object(pairs) => pairs.iter().map(|(key, _)| key.clone()).collect(),
        Value::Array(values) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}
fn key_differs(found: &Value, row: &Value, names: &[&str]) -> Result<bool> {
    for name in names {
        if !same(field(found, name)?, field(row, name)?) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn s_imode(value: &Value) -> Result<Value> {
    Ok(number(int(value).ok_or_else(|| err(MALFORMED))? & 0o7777))
}
fn required_facts(row: &Value) -> Result<Value> {
    Ok(object([
        ("uid", field(row, "uid")?.clone()),
        ("gid", field(row, "gid")?.clone()),
        ("mode", s_imode(field(row, "mode")?)?),
        ("xattrs", field(row, "xattrs")?.clone()),
    ]))
}
/// `sorted(paths, key=lambda path: (path == 'MEMORY.md', path))`.
fn router_last(paths: BTreeSet<String>) -> Vec<String> {
    let mut ordered: Vec<String> = paths.into_iter().collect();
    ordered.sort_by(|a, b| (a == "MEMORY.md", a).cmp(&(b == "MEMORY.md", b)));
    ordered
}
fn source_files(rows: &[Value]) -> BTreeMap<String, &Value> {
    rows.iter()
        .filter(|row| {
            get(row, "type").as_str() == Some("file") && get(row, "runtime").as_bool() != Some(true)
        })
        .filter_map(|row| Some((get(row, "path").as_str()?.to_owned(), row)))
        .collect()
}
fn transaction_marker(p: &Value) -> Result<Vec<u8>> {
    encoded(&object([
        ("schema", string(writes::TRANSACTION)),
        ("project_id", field(p, "project_id")?.clone()),
        ("operation_id", field(p, "operation_id")?.clone()),
        ("transaction_kind", string("rollback")),
    ]))
}
fn parent_marker(p: &Value) -> Result<Option<Vec<u8>>> {
    match field(p, "parent_marker")? {
        Value::Null => Ok(None),
        value if value.as_object().is_some_and(|pairs| pairs.is_empty()) => Ok(None),
        value => Ok(Some(encoded(value)?)),
    }
}
fn option_sha(value: Option<&[u8]>) -> Value {
    crate::developer_identity::digest(value).map_or(Value::Null, string)
}

/// Check an exact retained child-restore proposal.
pub fn value(repo: &Path, supplied: &Value, runtime_root: &Path) -> Result<Value> {
    let p = supplied.get("proposal").unwrap_or(supplied);
    crate::layout_recovery::fields(p, FIELDS, Some(SCHEMA))?;
    if ![
        "operation_id",
        "proposal_id",
        "parent_operation_id",
        "project_id",
    ]
    .iter()
    .all(|key| uuid(get(p, key)))
    {
        return Err(err("Child restore requires canonical stable identities"));
    }
    let operation_id = text(p, "operation_id")?;
    let (root, bank, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let changed = || err("Child restore proposal is unknown or changed");
    if get(p, "operation_id") == get(p, "parent_operation_id")
        || get(p, "repo_root").as_str() != Some(path_text(&root)?)
        || get(p, "bank_path").as_str() != Some(path_text(&bank)?)
    {
        return Err(changed());
    }
    let retained = read_value(
        &directory
            .join("proposals")
            .join(format!("{}.json", text(p, "proposal_id")?)),
    )?;
    if !same(&retained, p) {
        return Err(changed());
    }
    Ok(p.clone())
}

/// The exact ordered action prefix an interrupted child restore reached.
pub fn boundary(p: &Value, current: &[Value], plan: Option<&Value>) -> Result<usize> {
    let plan = plan.filter(|plan| plan.as_object().is_some_and(|pairs| !pairs.is_empty()));
    let (initial, last, actions) = match plan {
        Some(plan) => (
            rows(field(plan, "initial_view")?)?,
            rows(field(plan, "desired_view")?)?,
            rows(field(plan, "actions")?)?,
        ),
        None => (
            rows(field(p, "base_view")?)?,
            rows(field(p, "desired_view")?)?,
            rows(field(p, "actions")?)?,
        ),
    };
    let directories = field(p, "directories_to_remove")?;
    let mut expected: BTreeMap<String, Value> = by_path(initial)
        .into_iter()
        .map(|(path, row)| (path, row.clone()))
        .collect();
    let desired = by_path(last);
    let actual = by_path(current);
    let mut changed = BTreeSet::new();
    for position in 0..=actions.len() {
        let mut valid = true;
        let paths: BTreeSet<&String> = actual.keys().chain(expected.keys()).collect();
        for path in paths {
            let found = actual.get(path).copied();
            let row = expected.get(path);
            if listed(directories, path) {
                if let Some(found) = found {
                    let reference = match row {
                        Some(row) => row,
                        None => *desired.get(path).ok_or_else(|| err(MALFORMED))?,
                    };
                    if get(found, "type").as_str() != Some("directory")
                        || key_differs(found, reference, &["uid", "gid", "mode", "xattrs"])?
                    {
                        valid = false;
                        break;
                    }
                }
                continue;
            }
            if !changed.contains(path) {
                let equal = match (found, row) {
                    (Some(found), Some(row)) => same(found, row),
                    (None, None) => true,
                    _ => false,
                };
                if !equal {
                    valid = false;
                    break;
                }
            } else if let Some(row) = row {
                let Some(found) = found else {
                    valid = false;
                    break;
                };
                if key_differs(
                    found,
                    row,
                    &["type", "uid", "gid", "mode", "sha256", "xattrs"],
                )? {
                    valid = false;
                    break;
                }
            } else if found.is_some() {
                valid = false;
                break;
            }
        }
        if valid {
            return Ok(position);
        }
        if let Some(action) = actions.get(position) {
            let path = text(action, "path")?.to_owned();
            changed.insert(path.clone());
            match desired.get(&path) {
                Some(row) => {
                    expected.insert(path, (*row).clone());
                }
                None => {
                    expected.remove(&path);
                }
            }
        }
    }
    Err(err(
        "Child restore contains an impossible partial ordering or external edit",
    ))
}

/// Prepare a child restore of an exact completed repair.
pub fn preview(repo: &Path, operation_id: &str, runtime_root: &Path) -> Result<Value> {
    let source = crate::layout_recovery::recovery_source(repo, operation_id, runtime_root)?;
    let RecoverySource {
        operation:
            Operation {
                root,
                bank,
                directory: parent_dir,
                manifest,
                ..
            },
        proposal: original,
        ..
    } = source;
    let Some(receipt) = crate::layout_recovery::receipt_value(&parent_dir, &original)? else {
        return crate::layout_recovery::recover(
            repo,
            Some(operation_id),
            Some("rollback"),
            None,
            runtime_root,
        );
    };
    if get(&receipt, "outcome").as_str() != Some("layout-restored") {
        return Err(err("Only a completed repair has a child restore"));
    }
    let frontier = frontier()?;
    let existing_marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
    bank_observation::markers(&bank, None, existing_marker.as_deref())?;
    let hold = crate::layout_recovery::hold(&original)?;
    if existing_marker
        .as_ref()
        .is_some_and(|marker| *marker != hold)
    {
        return Err(err("Finish known marker cleanup before child restore"));
    }
    let current = crate::layout_recovery::view(&bank, &[])?;
    crate::layout_recovery::expected_result(&original, &manifest, &current, false)?;
    let indexed_state = state::index(&root, runtime_root)?;
    let indexed = get(&indexed_state, "operations")
        .get(operation_id)
        .filter(|row| matches!(get(row, "state").as_str(), Some("active" | "terminal")))
        .ok_or_else(|| err("Child restore needs the checked current parent operation"))?
        .clone();
    let (_, head, ledger) = frontier.ledger(&root, operation_id, &indexed, &bank, runtime_root)?;
    if int(get(&head, "generation")) != Some(0)
        || !same(&ledger, &crate::layout_recovery::ledger_value(&original)?)
    {
        return Err(err(
            "Later admission/governance/revision depends on the repaired bank",
        ));
    }
    let child = host::uuid4()?;
    let p = locked(&root, &bank, &child, |owner| {
        let owner = Some(owner);
        require_completed_resolutions(&root, None, runtime_root)?;
        frontier.require_scope_owned(
            &root,
            field(&original, "project_id")?,
            Some(field(&original, "scope_context")?),
            owner,
            existing_marker.as_deref(),
        )?;
        let source_frontier = frontier.observe_owned(&root, owner, existing_marker.as_deref())?;
        let observed = raw::observe(
            &root,
            &child,
            runtime_root,
            &recovery_quiescence::runtime_binary(runtime_root),
            raw::ObserveOptions {
                owner,
                transaction: existing_marker.as_deref(),
                ..raw::ObserveOptions::default()
            },
        )?;
        let (_, _, _, directory) = operation_paths(&root, &child, runtime_root)?;
        state::write_once(&directory.join("manifest.json"), &observed)?;
        state::put_bytes(
            &directory.join("manifest.sha256"),
            format!("{}\n", sha(&observed)?).as_bytes(),
        )?;
        let old = by_path(rows(field(&manifest, "sources")?)?);
        let now = by_path(rows(field(&observed, "sources")?)?);
        let before = source_files(&current);
        let desired_view = crate::layout_recovery::manifest_view(&manifest)?;
        let after = source_files(&desired_view);
        let keys = ["sha256", "uid", "gid", "mode", "xattrs"];
        let mut actions = Vec::new();
        let paths: BTreeSet<String> = old.keys().chain(now.keys()).cloned().collect();
        for path in router_last(paths) {
            let differs = match (before.get(&path), after.get(&path)) {
                (Some(before), Some(after)) => key_differs(before, after, &keys)?,
                _ => true,
            };
            if differs {
                let sha_of = |rows: &BTreeMap<String, &Value>| {
                    rows.get(&path)
                        .map_or(Value::Null, |row| get(row, "sha256").clone())
                };
                actions.push(object([
                    ("path", string(path.as_str())),
                    ("before", sha_of(&now)),
                    ("after", sha_of(&old)),
                ]));
            }
        }
        let parent_marker = match &existing_marker {
            Some(raw) if !raw.is_empty() => load(raw)?,
            _ => Value::Null,
        };
        let p = object([
            ("schema", string(SCHEMA)),
            ("operation_id", string(child.as_str())),
            ("proposal_id", string(host::uuid4()?)),
            ("parent_operation_id", string(operation_id)),
            ("parent_receipt_sha256", string(sha(&receipt)?)),
            ("parent_proposal_sha256", string(sha(&original)?)),
            ("parent_ledger_head_sha256", string(sha(&head)?)),
            ("manifest_sha256", string(sha(&observed)?)),
            ("repo_root", string(path_text(&root)?)),
            ("bank_path", string(path_text(&bank)?)),
            ("project_id", field(&manifest, "project_id")?.clone()),
            ("scope_context", field(&original, "scope_context")?.clone()),
            ("base_view", Value::Array(current.clone())),
            ("desired_view", Value::Array(desired_view)),
            ("actions", Value::Array(actions)),
            (
                "directories_to_remove",
                field(field(&original, "metadata_plan")?, "directories")?.clone(),
            ),
            ("parent_marker", parent_marker),
            ("parent_index_row", indexed.clone()),
            ("frontier", source_frontier),
            ("git", raw::git_pins(&root)?),
            ("prepared_at", string(storage::now(Sources::default())?)),
        ]);
        state::write_once(
            &directory
                .join("proposals")
                .join(format!("{}.json", text(&p, "proposal_id")?)),
            &p,
        )?;
        Ok(p)
    })?;
    Ok(crate::layout_recovery::result(
        "rollback",
        Some(&child),
        vec![
            ("state", string("child-restore-preview")),
            ("verified", Value::Bool(true)),
            ("proposal", p),
            ("parent_operation_id", string(operation_id)),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
        ],
    ))
}

/// Journal-derived temporaries this child restore may have left in the bank.
pub fn temporaries(bank: &Path, p: &Value) -> Result<Vec<Value>> {
    let mut markers = vec![transaction_marker(p)?];
    if let Some(parent) = parent_marker(p)? {
        markers.push(parent);
    }
    let desired: BTreeMap<String, Value> = rows(field(p, "desired_view")?)?
        .iter()
        .filter(|row| get(row, "type").as_str() == Some("file"))
        .filter_map(|row| Some((get(row, "path").as_str()?.to_owned(), row.clone())))
        .collect();
    temps::entries(
        bank,
        text(p, "operation_id")?,
        rows(field(p, "actions")?)?,
        rows(field(p, "base_view")?)?,
        &desired,
        &markers,
    )
}

/// The deterministic metadata probe plan of a child restore.
pub fn metadata_probes(p: &Value) -> Result<Value> {
    let mut paths = BTreeSet::new();
    for action in rows(field(p, "actions")?)? {
        paths.insert(text(action, "path")?.to_owned());
    }
    let mut requirements = Vec::new();
    for view in ["base_view", "desired_view"] {
        for row in rows(field(p, view)?)? {
            let path = get(row, "path").as_str();
            if get(row, "type").as_str() == Some("file")
                && path.is_some_and(|path| paths.contains(path))
            {
                requirements.push(object([
                    ("path", field(row, "path")?.clone()),
                    ("facts", required_facts(row)?),
                ]));
            }
        }
    }
    temps::probe_plan(
        text(p, "operation_id")?,
        &requirements,
        field(p, "directories_to_remove")?,
    )
}

fn pin_paths(plan: &Value) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for key in ["temporary_entries", "probe_entries"] {
        for pin in rows(field(plan, key)?)? {
            paths.push(text(pin, "path")?.to_owned());
        }
    }
    Ok(paths)
}

/// Preview or apply checked recovery of an interrupted child restore.
pub fn recover(
    repo: &Path,
    operation_id: Option<&str>,
    mode: Option<&str>,
    proposal: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    let Some(proposal) = proposal else {
        let operation_id = operation_id
            .ok_or_else(|| err("Recovery operations require canonical UUIDv4 identifiers"))?;
        let Operation {
            root,
            bank,
            directory,
            ..
        } = crate::layout_recovery::operation(repo, operation_id, runtime_root)?;
        let journal = read_value(&directory.join("journal.json"))?;
        let p = value(
            &root,
            &read_value(
                &directory
                    .join("proposals")
                    .join(format!("{}.json", text(&journal, "proposal_id")?)),
            )?,
            runtime_root,
        )?;
        let probes = metadata_probes(&p)?;
        if !journal
            .get("metadata_probes")
            .is_some_and(|value| same(value, &probes))
        {
            return Err(err("Child recovery metadata probe journal changed"));
        }
        let frontier = frontier()?;
        let marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
        bank_observation::markers(&bank, None, marker.as_deref())?;
        let known = read_optional(&directory.join("receipt.json"))?;
        let mode = mode.filter(|mode| matches!(*mode, "finish" | "rollback"));
        let reversed = known
            .as_ref()
            .is_some_and(|known| get(known, "restore_mode").as_str() != mode);
        let Some(mode) = mode.filter(|_| !reversed) else {
            return Err(err(
                "Durable child restore outcome cannot be reversed by interrupted recovery",
            ));
        };
        let mut allowed: Vec<Option<Vec<u8>>> = vec![Some(encoded(field(&journal, "marker")?)?)];
        allowed.push(parent_marker(&p)?);
        if known.is_some() {
            allowed.push(None);
        }
        if !allowed.contains(&marker) {
            return Err(err("Child recovery refuses an unknown operation marker"));
        }
        let temporary_entries = temporaries(&bank, &p)?;
        let probe_entries = temps::probe_entries(&bank, &probes, &directory)?;
        let ignored: Vec<String> = temporary_entries
            .iter()
            .chain(&probe_entries)
            .map(|pin| text(pin, "path").map(str::to_owned))
            .collect::<Result<_>>()?;
        let current = crate::layout_recovery::view(&bank, &ignored)?;
        match &known {
            None => {
                let active = read_optional(&directory.join("child-rollback-plan.json"))?;
                let plan = if mode == "rollback" {
                    active.as_ref()
                } else {
                    None
                };
                boundary(&p, &current, plan)?;
            }
            Some(known) => {
                if !same(
                    &Value::Array(current.clone()),
                    field(known, "resulting_view")?,
                ) {
                    return Err(err("Later external edits prevent child marker cleanup"));
                }
            }
        }
        let plan = object([
            ("schema", string(CHILD_RECOVERY)),
            ("recovery_id", string(host::uuid4()?)),
            ("operation_id", string(operation_id)),
            ("mode", string(mode)),
            ("proposal_sha256", string(sha(&p)?)),
            ("observed_view", Value::Array(current)),
            ("temporary_entries", Value::Array(temporary_entries)),
            ("probe_entries", Value::Array(probe_entries)),
            ("marker_sha256", option_sha(marker.as_deref())),
            ("journal_sha256", string(sha(&journal)?)),
            (
                "receipt_sha256",
                match &known {
                    Some(known) => string(sha(known)?),
                    None => Value::Null,
                },
            ),
            ("git", raw::git_pins(&root)?),
        ]);
        state::write_once(
            &directory
                .join("recovery")
                .join(format!("{}.json", text(&plan, "recovery_id")?)),
            &plan,
        )?;
        return Ok(crate::layout_recovery::result(
            "recover",
            Some(operation_id),
            vec![
                ("state", string("child-recovery-preview")),
                ("verified", Value::Bool(true)),
                ("proposal", plan),
                ("private_artifact_writes_performed", Value::Bool(true)),
            ],
        ));
    };
    let plan = proposal.get("proposal").unwrap_or(proposal);
    crate::layout_recovery::fields(plan, CHILD_RECOVERY_FIELDS, Some(CHILD_RECOVERY))?;
    let operation_id = text(plan, "operation_id")?;
    let (root, _, _, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let plan_path = directory
        .join("recovery")
        .join(format!("{}.json", text(plan, "recovery_id")?));
    if !same(&read_value(&plan_path)?, plan) {
        return Err(err("Child recovery preview is unknown or changed"));
    }
    if let Some(completed) = layout_history::completed(&directory, plan)? {
        let p = value(
            &root,
            &read_value(
                &directory
                    .join("proposals")
                    .join(format!("{}.json", text(&completed, "proposal_id")?)),
            )?,
            runtime_root,
        )?;
        layout_history::check(
            &root,
            field(&p, "scope_context")?,
            text(&p, "project_id")?,
            runtime_root,
        )?;
        return Ok(crate::layout_recovery::result(
            "recover",
            Some(text(&p, "operation_id")?),
            vec![
                ("state", field(&completed, "outcome")?.clone()),
                ("verified", Value::Bool(true)),
                ("receipt", completed.clone()),
                ("receipt_is_historical", Value::Bool(true)),
                ("already_applied", Value::Bool(true)),
            ],
        ));
    }
    crate::layout_recovery::operation(&root, operation_id, runtime_root)?;
    let journal = read_value(&directory.join("journal.json"))?;
    let p = value(
        &root,
        &read_value(
            &directory
                .join("proposals")
                .join(format!("{}.json", text(&journal, "proposal_id")?)),
        )?,
        runtime_root,
    )?;
    if !same(&read_value(&plan_path)?, plan)
        || get(plan, "proposal_sha256").as_str() != Some(sha(&p)?.as_str())
    {
        return Err(err("Child recovery preview is not exact"));
    }
    apply(&root, &p, Some(plan), runtime_root)
}

/// Remove an emptied reviewed new directory relative to its held parent.
fn remove_directory(bank: &Path, path: &str, base_view: &[Value]) -> Result<()> {
    let location = bank.join(path);
    let Some(info) = lstat(&location)? else {
        return Ok(());
    };
    let occupied =
        || err("Child restore directory gained an unrelated occupant or metadata change");
    if info.is_symlink() || !info.is_directory() || !listing(&location)?.is_empty() {
        return Err(occupied());
    }
    let expected = base_view
        .iter()
        .find(|row| get(row, "path").as_str() == Some(path))
        .ok_or_else(|| err(MALFORMED))?;
    let current = crate::layout_recovery::view(bank, &[])?;
    let actual = current
        .iter()
        .find(|row| get(row, "path").as_str() == Some(path));
    let Some(actual) = actual else {
        return Err(occupied());
    };
    if key_differs(actual, expected, &["type", "uid", "gid", "mode", "xattrs"])? {
        return Err(occupied());
    }
    let parent = location.parent().ok_or_else(|| err(MALFORMED))?;
    let name = location.file_name().ok_or_else(|| err(MALFORMED))?;
    Descriptor::open_directory(parent)
        .and_then(|directory| directory.remove_directory_at(name))
        .map_err(io_error)?;
    tx::flush_directory(parent)
}

fn readback(final_view: &[Value], readback: &[Value]) -> Result<()> {
    let actual = by_path(readback);
    let expected: BTreeSet<String> = by_path(final_view).into_keys().collect();
    if actual.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(err("Child restore physical readback failed"));
    }
    for row in final_view {
        let found = actual
            .get(text(row, "path")?)
            .ok_or_else(|| err("Child restore physical readback failed"))?;
        let mut names = vec!["type", "runtime", "uid", "gid", "mode", "xattrs"];
        if get(row, "type").as_str() == Some("file") {
            names.push("sha256");
        }
        if key_differs(found, row, &names)? {
            return Err(err("Child restore bytes/metadata readback failed"));
        }
    }
    Ok(())
}

/// Apply (or finish/roll back) one exact child restore.
pub fn apply(
    repo: &Path,
    supplied: &Value,
    plan: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    let p = value(repo, supplied, runtime_root)?;
    let operation_id = text(&p, "operation_id")?;
    let (root, bank, base, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let known = read_optional(&directory.join("receipt.json"))?;
    let project_id = text(&p, "project_id")?;
    if let (Some(known), None) = (&known, plan) {
        if get(known, "request_sha256").as_str() != Some(sha(&p)?.as_str())
            || get(known, "verified").as_bool() != Some(true)
        {
            return Err(err("Child historical receipt is corrupt"));
        }
        layout_history::check(&root, field(&p, "scope_context")?, project_id, runtime_root)?;
        return Ok(crate::layout_recovery::result(
            "rollback",
            Some(operation_id),
            vec![
                ("state", field(known, "outcome")?.clone()),
                ("verified", Value::Bool(true)),
                ("receipt", known.clone()),
                ("receipt_is_historical", Value::Bool(true)),
                ("already_applied", Value::Bool(true)),
            ],
        ));
    }
    let observed = crate::layout_recovery::operation(&root, operation_id, runtime_root)?.manifest;
    let parent_operation = text(&p, "parent_operation_id")?;
    let parent = crate::layout_recovery::recovery_source(&root, parent_operation, runtime_root)?;
    let parent_dir = parent.operation.directory.clone();
    let manifest = parent.operation.manifest.clone();
    let original = parent.proposal.clone();
    let parent_receipt = crate::layout_recovery::receipt_value(&parent_dir, &original)?;
    let parent_receipt_sha = match &parent_receipt {
        Some(receipt) => sha(receipt)?,
        None => sha(&Value::Null)?,
    };
    if get(&p, "parent_receipt_sha256").as_str() != Some(parent_receipt_sha.as_str())
        || get(&p, "parent_proposal_sha256").as_str() != Some(sha(&original)?.as_str())
        || get(&p, "manifest_sha256").as_str() != Some(sha(&observed)?.as_str())
    {
        return Err(err("Child restore parent/source evidence is damaged"));
    }
    let mode = match plan {
        Some(plan) => text(plan, "mode")?.to_owned(),
        None => "finish".to_owned(),
    };
    let frontier = frontier()?;
    let (known, mutated) = locked(&root, &bank, operation_id, |owner| {
        let owner = Some(owner);
        let mut known = known;
        let mut mutated = false;
        require_completed_resolutions(&root, None, runtime_root)?;
        let current_marker = frontier.marker_bytes(&bank, tx::MARKER, None)?;
        let starting_marker = parent_marker(&p)?;
        let transaction = transaction_marker(&p)?;
        let probes = metadata_probes(&p)?;
        let ignored = match plan {
            Some(plan) => pin_paths(plan)?,
            None => Vec::new(),
        };
        let mut current = crate::layout_recovery::view(&bank, &ignored)?;
        if let Some(plan) = plan {
            let journal = read_value(&directory.join("journal.json"))?;
            let receipt_sha = match &known {
                Some(known) => string(sha(known)?),
                None => Value::Null,
            };
            let changed = !same(
                &Value::Array(current.clone()),
                field(plan, "observed_view")?,
            ) || !same(
                &option_sha(current_marker.as_deref()),
                field(plan, "marker_sha256")?,
            ) || get(plan, "journal_sha256").as_str()
                != Some(sha(&journal)?.as_str())
                || !same(&raw::git_pins(&root)?, field(plan, "git")?)
                || !same(&receipt_sha, field(plan, "receipt_sha256")?)
                || !journal
                    .get("metadata_probes")
                    .is_some_and(|value| same(value, &probes))
                || !same(
                    &Value::Array(temporaries(&bank, &p)?),
                    field(plan, "temporary_entries")?,
                )
                || !same(
                    &Value::Array(temps::probe_entries(&bank, &probes, &directory)?),
                    field(plan, "probe_entries")?,
                );
            if changed {
                return Err(err("Child recovery pins changed"));
            }
        } else if !same(&Value::Array(current.clone()), field(&p, "base_view")?)
            || current_marker != starting_marker
        {
            return Err(err("Child restore would overwrite a later edit"));
        }
        if !same(&raw::git_pins(&root)?, field(&p, "git")?) {
            return Err(err("Child restore HEAD/index/configuration changed"));
        }
        let index = state::index(&root, runtime_root)?;
        let parent_row = operation_row(&index, parent_operation)?;
        let (_, head, _) =
            frontier.ledger(&root, parent_operation, parent_row, &bank, runtime_root)?;
        if get(&p, "parent_ledger_head_sha256").as_str() != Some(sha(&head)?.as_str()) {
            return Err(err("Child restore has a later pending-ledger dependency"));
        }
        crate::layout_recovery::rebind(
            &root,
            &observed,
            owner,
            current_marker.as_deref(),
            runtime_root,
        )?;
        if let Some(plan) = plan {
            temps::probe_clean(
                &bank,
                rows(field(plan, "probe_entries")?)?,
                owner,
                current_marker.as_deref(),
            )?;
            temps::probe_complete(&bank, &probes, &directory)?;
            temps::clean(
                &bank,
                rows(field(plan, "temporary_entries")?)?,
                owner,
                current_marker.as_deref(),
            )?;
            current = crate::layout_recovery::view(&bank, &[])?;
        }
        if let Some(known) = &known {
            if !same(
                &Value::Array(current.clone()),
                field(known, "resulting_view")?,
            ) {
                return Err(err(
                    "External changes after durable child receipt remain pending",
                ));
            }
        }
        let journal = object([
            ("schema", string(RECOVERY_JOURNAL)),
            ("operation_id", string(operation_id)),
            ("proposal_id", field(&p, "proposal_id")?.clone()),
            ("proposal_sha256", string(sha(&p)?)),
            ("manifest_sha256", field(&p, "manifest_sha256")?.clone()),
            ("review_sha256", field(&original, "review_sha256")?.clone()),
            ("marker", load(&transaction)?),
            ("parent_operation_id", string(parent_operation)),
            (
                "parent_receipt_sha256",
                field(&p, "parent_receipt_sha256")?.clone(),
            ),
            ("metadata_probes", probes.clone()),
        ]);
        state::write_once(&directory.join("journal.json"), &journal)?;
        if known.is_none() {
            writes::fault("layout-recovery-probe-journal")?;
            if plan.is_none() {
                temps::probe(
                    &bank,
                    &probes,
                    &directory,
                    None,
                    None,
                    owner,
                    current_marker.as_deref(),
                )?;
            } else {
                let target_view = if mode == "finish" {
                    rows(field(&p, "desired_view")?)?
                } else {
                    rows(field(&p, "base_view")?)?
                };
                let actual = by_path(&current);
                let mut paths = BTreeSet::new();
                for action in rows(field(&p, "actions")?)? {
                    paths.insert(text(action, "path")?.to_owned());
                }
                let keys = ["type", "sha256", "uid", "gid", "mode", "xattrs"];
                let mut remaining = Vec::new();
                for row in target_view {
                    let path = text(row, "path")?;
                    if get(row, "type").as_str() != Some("file") || !paths.contains(path) {
                        continue;
                    }
                    let differs = match actual.get(path) {
                        None => true,
                        Some(found) => keys.iter().any(|key| !same(get(found, key), get(row, key))),
                    };
                    if differs {
                        remaining.push(object([
                            ("path", string(path)),
                            ("facts", required_facts(row)?),
                        ]));
                    }
                }
                let directories: Vec<Value> = if mode == "rollback" {
                    directory_names(field(&p, "directories_to_remove")?)
                        .into_iter()
                        .filter(|path| !actual.contains_key(path))
                        .map(string)
                        .collect()
                } else {
                    Vec::new()
                };
                temps::probe(
                    &bank,
                    &probes,
                    &directory,
                    Some(&Value::Array(remaining)),
                    Some(&Value::Array(directories)),
                    owner,
                    current_marker.as_deref(),
                )?;
            }
            if !same(
                &Value::Array(crate::layout_recovery::view(&bank, &[])?),
                &Value::Array(current.clone()),
            ) || frontier.marker_bytes(&bank, tx::MARKER, None)? != current_marker
            {
                return Err(err("Child restore changed during metadata preflight"));
            }
        }
        let scope = field(&p, "scope_context")?;
        let checkout = text(scope, "checkout_key")?;
        state::bind(
            &root,
            operation_id,
            project_id,
            checkout,
            "interrupted",
            runtime_root,
        )?;
        if current_marker.as_deref() != Some(transaction.as_slice()) {
            crate::layout_recovery::atomic_marker(
                &bank,
                current_marker.as_deref(),
                &transaction,
                owner,
                crate::layout_recovery::MARKER,
                None,
            )?;
        }
        if known.is_none() {
            let context =
                RecoveryReadSet::observe(&root, &observed, &p, owner, &transaction, runtime_root)?;
            let (actions, desired_manifest, desired_dir) = if mode == "finish" {
                let position = boundary(&p, &current, None)?;
                let actions = rows(field(&p, "actions")?)?;
                (
                    actions.get(position..).unwrap_or_default().to_vec(),
                    manifest.clone(),
                    parent_dir.clone(),
                )
            } else {
                let plan_path = directory.join("child-rollback-plan.json");
                let reverse = match read_optional(&plan_path)? {
                    Some(reverse) => reverse,
                    None => {
                        boundary(&p, &current, None)?;
                        let expected = by_path(rows(field(&observed, "sources")?)?);
                        let current_sources = source_files(&current);
                        let base_sources = source_files(rows(field(&p, "base_view")?)?);
                        let keys = ["sha256", "uid", "gid", "mode", "xattrs"];
                        let paths: BTreeSet<String> = expected
                            .keys()
                            .chain(current_sources.keys())
                            .cloned()
                            .collect();
                        let mut actions = Vec::new();
                        for path in router_last(paths) {
                            let differs =
                                match (base_sources.get(&path), current_sources.get(&path)) {
                                    (Some(base), Some(now)) => key_differs(base, now, &keys)?,
                                    _ => true,
                                };
                            if differs {
                                actions.push(object([("path", string(path))]));
                            }
                        }
                        let reverse = object([
                            ("initial_view", Value::Array(current.clone())),
                            ("desired_view", field(&p, "base_view")?.clone()),
                            ("actions", Value::Array(actions)),
                        ]);
                        state::write_once(&plan_path, &reverse)?;
                        reverse
                    }
                };
                let position = boundary(&p, &current, Some(&reverse))?;
                let actions = rows(field(&reverse, "actions")?)?;
                crate::layout_recovery::new_directories(
                    &bank,
                    &object([(
                        "metadata_plan",
                        object([("directories", field(&p, "directories_to_remove")?.clone())]),
                    )]),
                )?;
                (
                    actions.get(position..).unwrap_or_default().to_vec(),
                    observed.clone(),
                    directory.clone(),
                )
            };
            let sources = by_path(rows(field(&desired_manifest, "sources")?)?);
            let facts = by_path(rows(field(&desired_manifest, "physical_entries")?)?);
            let mut running = crate::layout_recovery::view(&bank, &[])?;
            for action in &actions {
                let path = text(action, "path")?;
                let body = match sources.get(path) {
                    Some(source) => Some(
                        state::read_bytes(
                            &desired_dir.join(text(source, "blob")?),
                            raw::MAX_FILE,
                            false,
                        )?
                        .ok_or_else(|| err(MALFORMED))?,
                    ),
                    None => None,
                };
                let required = match facts.get(path) {
                    Some(row) => Some(required_facts(row)?),
                    None => None,
                };
                crate::layout_recovery::replace_source(
                    &root,
                    &bank,
                    path,
                    body.as_deref(),
                    required.as_ref(),
                    &running,
                    &context,
                    runtime_root,
                )?;
                mutated = true;
                running = crate::layout_recovery::view(&bank, &[])?;
            }
            if mode == "finish" {
                let mut ordered = directory_names(field(&p, "directories_to_remove")?);
                ordered.sort_by_key(|path| std::cmp::Reverse(Path::new(path).components().count()));
                for path in ordered {
                    remove_directory(&bank, &path, rows(field(&p, "base_view")?)?)?;
                }
            }
            let final_view = if mode == "finish" {
                rows(field(&p, "desired_view")?)?
            } else {
                rows(field(&p, "base_view")?)?
            };
            let resulting = crate::layout_recovery::view(&bank, &[])?;
            readback(final_view, &resulting)?;
            context.check(runtime_root)?;
            let receipt = object([
                ("schema", string(RECOVERY_RECEIPT)),
                ("operation_id", string(operation_id)),
                ("proposal_id", field(&p, "proposal_id")?.clone()),
                ("project_id", string(project_id)),
                ("request_sha256", string(sha(&p)?)),
                ("parent_operation_id", string(parent_operation)),
                (
                    "parent_receipt_sha256",
                    field(&p, "parent_receipt_sha256")?.clone(),
                ),
                (
                    "outcome",
                    string(if mode == "finish" {
                        "rolled-back"
                    } else {
                        "layout-restored"
                    }),
                ),
                ("restore_mode", string(mode.as_str())),
                ("verified", Value::Bool(true)),
                ("readback_at", string(storage::now(Sources::default())?)),
                ("resulting_view", Value::Array(resulting)),
                ("byte_metadata_readback_verified", Value::Bool(true)),
                ("exact_xattr_readback_verified", Value::Bool(true)),
                (
                    "canonical_eligibility_at_commit",
                    crate::layout_recovery::eligibility(&root, owner, Some(&transaction)),
                ),
                ("semantic_recovery_complete_at_commit", Value::Bool(false)),
                (
                    "pending_unit_ids_at_commit",
                    field(field(&p, "frontier")?, "unit_ids")?.clone(),
                ),
                ("original_index_writes_performed", Value::Bool(false)),
                ("head_writes_performed", Value::Bool(false)),
                ("feature_writes_performed", Value::Bool(false)),
            ]);
            state::write_once(&directory.join("receipt.json"), &receipt)?;
            known = Some(receipt);
            writes::fault("layout-recovery-after-receipt")?;
        }
        let mut index = state::index(&root, runtime_root)?;
        let parent_index_row = field(&p, "parent_index_row")?.clone();
        let restored = if mode == "finish" {
            crate::layout_resolution::with(parent_index_row, [("state", string("rolled-back"))])
        } else {
            parent_index_row
        };
        crate::layout_resolution::set_operation_row(&mut index, parent_operation, restored);
        publish_index(&root, &base, &index, runtime_root)?;
        let pending = !items(field(field(&p, "frontier")?, "unit_ids")?)?.is_empty();
        state::bind(
            &root,
            parent_operation,
            project_id,
            checkout,
            if pending { "active" } else { "terminal" },
            runtime_root,
        )?;
        state::bind(
            &root,
            operation_id,
            project_id,
            checkout,
            "terminal",
            runtime_root,
        )?;
        match &starting_marker {
            Some(starting) if mode == "rollback" || pending => {
                crate::layout_recovery::atomic_marker(
                    &bank,
                    Some(&transaction),
                    starting,
                    owner,
                    crate::layout_recovery::MARKER,
                    None,
                )?;
            }
            _ => {
                tx::check_marker(&bank, &transaction)?;
                unlink_bank_entry(&bank, tx::MARKER)?;
                tx::flush_directory(&bank)?;
                let sentinel =
                    frontier.marker_bytes(&bank, crate::layout_resolution::SENTINEL_FILE, None)?;
                if let Some(sentinel) = sentinel {
                    let expected = encoded(&object([
                        ("schema", string(writes::SENTINEL)),
                        ("operation_id", string(parent_operation)),
                        ("project_id", string(project_id)),
                    ]))?;
                    if sentinel != expected {
                        return Err(err("Unknown sentinel during child restore"));
                    }
                    unlink_bank_entry(&bank, crate::layout_resolution::SENTINEL_FILE)?;
                    tx::flush_directory(&bank)?;
                }
            }
        }
        frontier.invalidate(&root)?;
        let known = known.ok_or_else(|| err(MALFORMED))?;
        if let Some(plan) = plan {
            state::write_once(
                &directory
                    .join("recovery")
                    .join(format!("{}.receipt.json", text(plan, "recovery_id")?)),
                &layout_history::recovery_receipt(&directory, plan, &known)?,
            )?;
        }
        Ok((known, mutated))
    })?;
    Ok(crate::layout_recovery::result(
        "rollback",
        Some(operation_id),
        vec![
            ("state", field(&known, "outcome")?.clone()),
            ("verified", Value::Bool(true)),
            ("receipt", known.clone()),
            (
                "canonical_eligibility",
                field(&known, "canonical_eligibility_at_commit")?.clone(),
            ),
            ("parent_operation_id", string(parent_operation)),
            ("canonical_writes_performed", Value::Bool(mutated)),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
        ],
    ))
}
