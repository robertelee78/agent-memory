//! Separately reviewed, exact-file cleanup of terminal private recovery evidence.
//!
//! No recursive deletion, age-based collection, pending-unit disposal, or implicit
//! cleanup by bank purge/runtime uninstall is provided here. Directories remain.
//! The executing runtime root is explicit on every entry point.
use crate::codec::json::Value;
use crate::developer_identity::{load_with_limit, uuid_valid};
use crate::error::Result;
use crate::fs::{Descriptor, trust};
use crate::layout_recovery::publication;
use crate::layout_resolution::{
    MALFORMED, err, field, file_sha, frontier, has_keys, int, io_error, items, keys, locked, lstat,
    number, operation_paths, operation_row, operations, path_text, publish_index, read_optional,
    read_raw, read_value, same, sha, text, uuid, with,
};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_transactions, bank_writes as writes, host, layout_recovery,
    layout_recovery_observation as observation, layout_recovery_state as state,
    layout_recovery_temps as temps, layout_restore, lifecycle, scope_binding, sharing_limits,
};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const SCHEMA: &str = "agent-memory/layout-retention-proposal-v1";
pub const REVIEW: &str = "agent-memory/layout-retention-review-v1";
pub const MAX_ENTRIES: usize = 262_144;
pub const MAX_TOTAL: usize = 512 * 1024 * 1024;
pub const ROOT_FILES: &[&str] = &[
    "manifest.json",
    "manifest.sha256",
    "unit-map.json",
    "journal.json",
    "receipt.json",
    "ledger-head.json",
    "tombstone.json",
    "retention-requirements.json",
    "rollback-plan.json",
    "child-rollback-plan.json",
    "quiescence-attestations.json",
    "staging.json",
    "retirement.json",
    "reference-map.json",
    "source-context.json",
];
const UUID: &str = "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
/// Baseline `PurePosixPath(key).parts` bound of one private evidence path.
const MAX_PARTS: usize = 64;
const SAFE_DIRECTORY: &str =
    "Recovery retention requires the existing checked owner-only directory";
const UNSAFE_NAME: &str = "Unsafe private artifact name";
const BOUND: &str = "Recovery retention inventory exceeds its explicit complete bound";
const STAGING: &str = "agent-memory/layout-recovery-staging-v1";
const STAGING_FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "repo_root",
    "bank_path",
    "project_id",
    "identity",
    "observation_complete",
];
const PROPOSAL_FIELDS: &[&str] = &[
    "schema",
    "cleanup_id",
    "operation_id",
    "repo_root",
    "project_id",
    "scope_context",
    "terminal",
    "dependencies",
    "delete",
    "retain",
    "directories",
    "rollback_capability_surrendered",
    "review",
];
const REVIEW_FIELDS: &[&str] = &[
    "schema",
    "review_id",
    "cleanup_id",
    "preview_sha256",
    "reviewer",
    "reviewed_at",
    "decision",
    "dependency_review_complete",
    "rollback_loss_acknowledged",
    "operator_authorization",
    "rationale",
];

fn pattern(body: String) -> Regex {
    // Invariant: the expression is a constant of this module and compiles.
    Regex::new(&format!("^(?:{body})$")).expect("constant retention pattern")
}

static OWNED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        [
            format!("(?:originals|intended)/{UUID}\\.bin"),
            format!(
                "(?:proposals|reviews|attestations|resolutions|resolution-journals|resolution-completions|union-reviews|discard-reviews|quiescence-reviews)/{UUID}\\.json"
            ),
            "(?:coverage-reviews|unit-maps)/[0-9a-f]{64}\\.json".to_owned(),
            "(?:ledger|progress|ledger-transitions)/[0-9]{8}\\.json".to_owned(),
            "progress/(?:apply|finish|rollback)-[0-9]{8}\\.json".to_owned(),
            format!("recovery/{UUID}(?:\\.receipt)?\\.json"),
            format!("revisions/{UUID}/(?:proposal|review|receipt|journal|unit-map)\\.json"),
            format!(
                "nested/{UUID}/(?:journal|receipt|proposal|resolution|completion|rollback-plan)\\.json"
            ),
            format!("nested/{UUID}/(?:progress|rollback-progress)/[0-9]{{8}}\\.json"),
            format!("publication/{UUID}/(?:observation|request|receipt)\\.json"),
            format!("publication/{UUID}/originals/{UUID}\\.bin"),
            format!("metadata-probe-attempts/{UUID}/(?:plan|completion)\\.json"),
            format!("metadata-probe-attempts/{UUID}/pins/\\.agent-memory-probe-{UUID}\\.json"),
            format!("retention/{UUID}/(?:intent|proposal|receipt|progress/[0-9]{{8}})\\.json"),
        ]
        .join("|"),
    )
});
static OWNED_DIRECTORY: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        [
            "(?:originals|intended|proposals|reviews|attestations|resolutions|resolution-journals|resolution-completions|ledger|progress|recovery|unit-maps|revisions|nested|publication|retention|coverage-reviews|union-reviews|discard-reviews|ledger-transitions|quiescence-reviews|metadata-probe-attempts)".to_owned(),
            format!("(?:revisions|nested|publication|retention|metadata-probe-attempts)/{UUID}"),
            format!("publication/{UUID}/originals"),
            format!("retention/{UUID}/progress"),
            format!("nested/{UUID}/(?:progress|rollback-progress)"),
            format!("metadata-probe-attempts/{UUID}/pins"),
        ]
        .join("|"),
    )
});

fn require(condition: bool, message: &str) -> Result<()> {
    if condition { Ok(()) } else { Err(err(message)) }
}
fn eq_text(value: &Value, expected: &str) -> bool {
    value.as_str() == Some(expected)
}
fn is_true(value: &Value) -> bool {
    value.as_bool() == Some(true)
}
fn object_pairs(value: &Value) -> Result<&[(String, Value)]> {
    value.as_object().ok_or_else(|| err(MALFORMED))
}
/// `name.json` of an evidence directory; a separator in the name is malformed.
fn named_json(directory: &Path, name: &str) -> Result<PathBuf> {
    require(
        !name.is_empty() && !name.contains('/') && name != "." && name != "..",
        MALFORMED,
    )?;
    Ok(directory.join(format!("{name}.json")))
}

/// Sorted complete listing of one held private directory.
fn names_of(held: &Descriptor) -> Result<Vec<String>> {
    let names = held
        .entries_bounded(MAX_ENTRIES)
        .map_err(io_error)?
        .ok_or_else(|| err(BOUND))?;
    let mut names = names
        .into_iter()
        .map(|name| name.into_string().map_err(|_| err(UNSAFE_NAME)))
        .collect::<Result<Vec<String>>>()?;
    names.sort();
    Ok(names)
}

struct Walk {
    files: Vec<(String, Value)>,
    directories: Vec<(String, Value)>,
    total: usize,
}

fn staging_value(raw: &[u8]) -> Result<()> {
    let staging = load_with_limit(raw, sharing_limits::MAX_PRIVATE)?;
    publication::exact(&staging, STAGING_FIELDS, Some(STAGING))?;
    require(
        get(&staging, "observation_complete") == &Value::Bool(false)
            && uuid(get(&staging, "operation_id"))
            && uuid(get(&staging, "project_id"))
            && get(&staging, "identity").as_object().is_some(),
        "Unknown/inconsistent partial staging evidence must remain pending",
    )
}

fn visit(path: &Path, relative: &str, walk: &mut Walk) -> Result<()> {
    trust::state(path)?;
    let held = Descriptor::open_directory(path).map_err(io_error)?;
    let before = held.stat().map_err(io_error)?;
    require(
        before.uid == host::effective_uid() && before.mode & 0o7777 == 0o700,
        SAFE_DIRECTORY,
    )?;
    walk.directories.push((
        relative.to_owned(),
        object([
            ("dev", number(before.dev)),
            ("ino", number(before.ino)),
            ("mode", number(before.mode)),
            ("uid", number(before.uid)),
            ("gid", number(before.gid)),
        ]),
    ));
    let names = names_of(&held)?;
    for name in &names {
        require(
            name != "."
                && name != ".."
                && !name.contains('/')
                && !name.contains('\\')
                && !name.chars().any(|c| (c as u32) < 32 || c as u32 == 127),
            UNSAFE_NAME,
        )?;
        let key = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        require(
            walk.files.len() + walk.directories.len() < MAX_ENTRIES
                && key.split('/').count() <= MAX_PARTS,
            BOUND,
        )?;
        let info = held.stat_at(OsStr::new(name)).map_err(io_error)?;
        if info.is_directory() {
            require(
                OWNED_DIRECTORY.is_match(&key),
                "Unknown private backup/directory is not owned cleanup evidence",
            )?;
            visit(&path.join(name), &key, walk)?;
            continue;
        }
        require(
            info.is_regular()
                && info.nlink == 1
                && info.uid == host::effective_uid()
                && info.mode & 0o7777 == 0o600,
            "Unknown linked, redirected or unsafe recovery artifact; no deletion is authorized",
        )?;
        require(
            ROOT_FILES.contains(&key.as_str()) || OWNED.is_match(&key),
            "Unknown private recovery artifact; dependency inventory remains pending",
        )?;
        let raw = read_raw(&path.join(name), sharing_limits::MAX_PRIVATE)?;
        if key == "staging.json" {
            staging_value(&raw)?;
        }
        walk.total += raw.len();
        require(
            walk.total <= MAX_TOTAL,
            "Recovery cleanup inventory exceeds the private review allowance",
        )?;
        let after = held.stat_at(OsStr::new(name)).map_err(io_error)?;
        require(
            info == after,
            "Private artifact changed during retention inventory",
        )?;
        walk.files.push((
            key,
            object([
                ("physical", observation::signature(&info)),
                ("sha256", string(layout_recovery::digest(&raw))),
                ("bytes", number(raw.len())),
            ]),
        ));
    }
    require(
        names_of(&held)? == names,
        "Private directory contents changed during retention inventory",
    )?;
    require(
        lstat(path)?.is_some_and(|named| named.same_inode(before)),
        "Private directory binding changed during retention inventory",
    )
}

/// Baseline `_inventory(directory)`: pin every named private entry; never
/// follow redirects or ignore unknowns.
pub fn inventory(directory: &Path) -> Result<Value> {
    let mut walk = Walk {
        files: Vec::new(),
        directories: Vec::new(),
        total: 0,
    };
    visit(directory, "", &mut walk)?;
    Ok(object([
        ("files", Value::Object(walk.files)),
        ("directories", Value::Object(walk.directories)),
        ("bytes", number(walk.total)),
    ]))
}

fn files(inventory: &Value) -> Result<&[(String, Value)]> {
    object_pairs(field(inventory, "files")?)
}

/// Baseline `_references(value, operation_id, directory)`.
pub fn references(value: &Value, operation_id: &str, directory: &str) -> bool {
    const KEYS: &[&str] = &[
        "parent_operation_id",
        "recovery_operation_id",
        "source_operation_id",
        "restore_operation_id",
    ];
    match value {
        Value::Object(pairs) => pairs.iter().any(|(key, item)| {
            (KEYS.contains(&key.as_str()) && eq_text(item, operation_id))
                || references(item, operation_id, directory)
        }),
        Value::Array(values) => values
            .iter()
            .any(|item| references(item, operation_id, directory)),
        Value::String(text) => text == directory || text.starts_with(&format!("{directory}/")),
        _ => false,
    }
}

/// Baseline `_child_info`: a child restore has real parent evidence, never a
/// fabricated ledger. Returns `(info, proposal)`.
fn child_info(
    root: &Path,
    operation_id: &str,
    base: &Path,
    index: &Value,
    inventory: &Value,
    complete: bool,
    runtime_root: &Path,
) -> Result<(Value, Value)> {
    let directory = base.join(operation_id);
    let mut candidates = Vec::new();
    for (name, _) in files(inventory)? {
        if name.starts_with("proposals/") && name.ends_with(".json") {
            let candidate = read_value(&directory.join(name))?;
            if eq_text(get(&candidate, "schema"), layout_restore::SCHEMA) {
                candidates.push(candidate);
            }
        }
    }
    require(
        candidates.len() == 1,
        "Unindexed operation is not one known typed child restore",
    )?;
    let candidate = candidates.pop().ok_or_else(|| err(MALFORMED))?;
    let proposal = layout_restore::value(root, &candidate, runtime_root)?;
    require(
        eq_text(get(&proposal, "operation_id"), operation_id),
        "Child restore identity was substituted",
    )?;
    let parent_id = text(&proposal, "parent_operation_id")?;
    require(
        operations(index)?.get(parent_id).is_some(),
        "Child restore has no real indexed parent",
    )?;
    let parent_row = operation_row(index, parent_id)?;
    let parent_directory = base.join(parent_id);
    let parent_receipt = read_value(&parent_directory.join("receipt.json"))?;
    require(
        eq_text(
            get(&parent_receipt, "schema"),
            layout_restore::RECOVERY_RECEIPT,
        ) && is_true(get(&parent_receipt, "verified"))
            && eq_text(get(&parent_receipt, "operation_id"), parent_id)
            && eq_text(get(&parent_receipt, "outcome"), "layout-restored")
            && eq_text(
                get(&proposal, "parent_receipt_sha256"),
                &sha(&parent_receipt)?,
            ),
        "Child restore parent receipt is missing, changed or not a real completed repair",
    )?;
    let parent_proposal = read_value(&named_json(
        &parent_directory.join("proposals"),
        text(&parent_receipt, "proposal_id")?,
    )?)?;
    let parent_sha = sha(&parent_proposal)?;
    let index_row = get(&proposal, "parent_index_row");
    require(
        eq_text(get(&proposal, "parent_proposal_sha256"), &parent_sha)
            && eq_text(get(&parent_receipt, "request_sha256"), &parent_sha)
            && same(get(parent_row, "project_id"), get(&proposal, "project_id"))
            && same(
                get(&proposal, "project_id"),
                get(&parent_receipt, "project_id"),
            )
            && same(
                get(index_row, "ledger_head_sha256"),
                get(&proposal, "parent_ledger_head_sha256"),
            )
            && same(
                get(index_row, "proposal_id"),
                get(&parent_proposal, "proposal_id"),
            ),
        "Child restore does not bind the original parent proposal and reviewed ledger head",
    )?;
    // The existing child contract only restores generation zero. Its actual
    // completed receipt binds that checked head even after later parent cleanup.
    let original_ledger = layout_recovery::ledger_value(&parent_proposal)?;
    let original_head = object([
        ("schema", string("agent-memory/layout-ledger-head-v1")),
        ("operation_id", string(parent_id)),
        ("generation", number(0)),
        ("sha256", string(sha(&original_ledger)?)),
    ]);
    require(
        eq_text(
            get(&proposal, "parent_ledger_head_sha256"),
            &sha(&original_head)?,
        ),
        "Child restore parent head is not the original actual layout generation",
    )?;
    let registry = state::bindings()?;
    let binding = get(&registry, "bindings").get(operation_id).cloned();
    let receipt = read_optional(&directory.join("receipt.json"))?;
    let project_id = get(&proposal, "project_id");
    if let Some(receipt) = &receipt {
        let mode = (
            get(receipt, "restore_mode").as_str(),
            get(receipt, "outcome").as_str(),
        );
        require(
            eq_text(get(receipt, "schema"), layout_restore::RECOVERY_RECEIPT)
                && eq_text(get(receipt, "operation_id"), operation_id)
                && same(get(receipt, "proposal_id"), get(&proposal, "proposal_id"))
                && same(get(receipt, "project_id"), project_id)
                && is_true(get(receipt, "verified"))
                && eq_text(get(receipt, "request_sha256"), &sha(&proposal)?)
                && eq_text(get(receipt, "parent_operation_id"), parent_id)
                && same(
                    get(receipt, "parent_receipt_sha256"),
                    get(&proposal, "parent_receipt_sha256"),
                )
                && matches!(
                    mode,
                    (Some("finish"), Some("rolled-back"))
                        | (Some("rollback"), Some("layout-restored"))
                ),
            "Unknown or incomplete child restoration receipt",
        )?;
        let bound = binding.as_ref().is_some_and(|row| {
            matches!(
                get(row, "lifecycle_state").as_str(),
                Some("terminal" | "cleaned")
            ) && same(get(row, "project_id"), project_id)
                && path_text(root).is_ok_and(|root| eq_text(get(row, "repo_root"), root))
                && same(get(row, "bank_path"), get(index, "bank_path"))
                && same(
                    get(row, "checkout_key"),
                    get(get(&proposal, "scope_context"), "checkout_key"),
                )
        });
        require(
            bound,
            "Child restore still has an interrupted private binding",
        )?;
        match read_optional(&directory.join("journal.json"))? {
            Some(journal) => require(
                eq_text(get(&journal, "schema"), layout_restore::RECOVERY_JOURNAL)
                    && eq_text(get(&journal, "operation_id"), operation_id)
                    && eq_text(get(&journal, "proposal_sha256"), &sha(&proposal)?)
                    && eq_text(get(&journal, "parent_operation_id"), parent_id)
                    && same(
                        get(&journal, "parent_receipt_sha256"),
                        get(&proposal, "parent_receipt_sha256"),
                    ),
                "Child restore journal is not its actual parent-bound operation",
            )?,
            None => {
                let cleaned = binding
                    .as_ref()
                    .is_some_and(|row| eq_text(get(row, "lifecycle_state"), "cleaned"));
                let tombstone = read_value(&directory.join("tombstone.json"))?;
                require(
                    cleaned && eq_text(get(&tombstone, "operation_id"), operation_id),
                    "Child restore journal disappeared without reviewed cleanup",
                )?;
            }
        }
    }
    let completed = receipt.is_some();
    if complete {
        require(
            completed
                && matches!(
                    get(parent_row, "state").as_str(),
                    Some("terminal" | "rolled-back" | "cleaned")
                ),
            "Unfinished child or active parent still depends on restoration evidence",
        )?;
    }
    let lifecycle_state = binding
        .as_ref()
        .filter(|row| row.as_object().is_some_and(|pairs| !pairs.is_empty()))
        .map_or_else(
            || Ok(string("interrupted")),
            |row| field(row, "lifecycle_state").cloned(),
        )?;
    let receipt_sha256 = match &receipt {
        Some(receipt) => string(sha(receipt)?),
        None => Value::Null,
    };
    let info = object([
        ("kind", string("child-restore")),
        ("state", lifecycle_state),
        ("project_id", project_id.clone()),
        ("parent_operation_id", string(parent_id)),
        ("completed", Value::Bool(completed)),
        ("receipt_sha256", receipt_sha256),
    ]);
    Ok((info, proposal))
}

/// `(root, bank, base, directory, terminal)` of a terminal operation.
type Evidence = publication::OperationEvidence;

/// Baseline `_operation_evidence(repo, operation_id, owner=)`.
fn operation_evidence(
    repo: &Path,
    operation_id: &str,
    owner: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Evidence> {
    let (root, bank, base, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let index = state::index(&root, runtime_root)?;
    if operations(&index)?.get(operation_id).is_some() {
        return publication::layout_publication_provider()?.operation_evidence(
            &root,
            operation_id,
            owner,
            runtime_root,
        );
    }
    let pins = inventory(&directory)?;
    let (child, proposal) = child_info(
        &root,
        operation_id,
        &base,
        &index,
        &pins,
        true,
        runtime_root,
    )?;
    let token = frontier()?.observe_owned(&root, owner, None)?;
    let no_units = match get(&token, "unit_ids") {
        Value::Array(units) => units.is_empty(),
        Value::Null | Value::Bool(false) => true,
        _ => false,
    };
    require(
        is_true(get(&token, "coverage_complete")) && no_units,
        "Pending recovery frontier prevents cleanup",
    )?;
    let manifest = read_value(&directory.join("manifest.json"))?;
    require(
        is_true(get(&manifest, "observation_complete"))
            && eq_text(get(&manifest, "operation_id"), operation_id)
            && eq_text(get(&proposal, "manifest_sha256"), &sha(&manifest)?),
        "Child original observation is incomplete or corrupt",
    )?;
    for source in items(field(&manifest, "sources")?)? {
        let source_id = get(source, "source_id");
        let locator = source_id.as_str().map(|id| format!("originals/{id}.bin"));
        require(
            uuid(source_id)
                && locator
                    .as_deref()
                    .is_some_and(|blob| eq_text(get(source, "blob"), blob)),
            "Child original source locator is not owned by this operation",
        )?;
        let blob = locator.ok_or_else(|| err(MALFORMED))?;
        let body = read_raw(&directory.join(blob), publication::MAX_FILE)?;
        require(
            int(get(source, "byte_count")) == i128::try_from(body.len()).ok()
                && eq_text(get(source, "sha256"), &layout_recovery::digest(&body)),
            "Child restore original backup is missing or corrupt",
        )?;
    }
    let terminal = object([
        ("kind", string("child-restore")),
        ("frontier_token", token),
        ("project_id", field(&proposal, "project_id")?.clone()),
        ("receipt_sha256", field(&child, "receipt_sha256")?.clone()),
        ("index_sha256", string(sha(&index)?)),
        (
            "parent_operation_id",
            field(&proposal, "parent_operation_id")?.clone(),
        ),
        (
            "parent_receipt_sha256",
            field(&proposal, "parent_receipt_sha256")?.clone(),
        ),
        (
            "parent_ledger_head_sha256",
            field(&proposal, "parent_ledger_head_sha256")?.clone(),
        ),
    ]);
    Ok((root, bank, base, directory, terminal))
}

/// Baseline `_probe_dependencies(bank, other, operation_id, inventory)`.
fn probe_dependencies(
    bank: &Path,
    other: &Path,
    operation_id: &str,
    inventory: &Value,
) -> Result<Vec<Value>> {
    const PREFIX: &str = "metadata-probe-attempts/";
    let directories = object_pairs(field(inventory, "directories")?)?;
    let mut attempts: Vec<&str> = directories
        .iter()
        .filter_map(|(key, _)| {
            let parts: Vec<&str> = key.split('/').collect();
            match parts.as_slice() {
                [_, attempt] if key.starts_with(PREFIX) => Some(*attempt),
                _ => None,
            }
        })
        .collect();
    attempts.sort();
    let probe_dirs = directories
        .iter()
        .filter(|(key, _)| key == "metadata-probe-attempts" || key.starts_with(PREFIX))
        .count();
    let mut probe_files = 0usize;
    let mut probe_bytes: i128 = 0;
    for (key, pin) in files(inventory)? {
        if key.starts_with(PREFIX) {
            probe_files += 1;
            probe_bytes += int(field(pin, "bytes")?).ok_or_else(|| err(MALFORMED))?;
        }
    }
    require(
        probe_dirs + probe_files <= observation::MAX_ENTRIES
            && probe_bytes <= sharing_limits::MAX_PRIVATE as i128,
        "Metadata probe evidence exceeds its complete aggregate bound",
    )?;
    if attempts.is_empty() {
        return Ok(Vec::new());
    }
    let journal = read_optional(&other.join("journal.json"))?;
    let probes = journal
        .as_ref()
        .and_then(|journal| journal.get("metadata_probes"))
        .filter(|probes| probes.as_object().is_some());
    let probes = match (journal.as_ref().and_then(Value::as_object), probes) {
        (Some(_), Some(probes)) => probes,
        _ => {
            return Err(err("Missing parent journal for metadata probe evidence"));
        }
    };
    require(
        eq_text(get(probes, "operation_id"), operation_id),
        "Metadata probe plan belongs to a different operation",
    )?;
    let live = temps::probe_entries(bank, probes, other)?;
    let mut blockers = Vec::new();
    if !live.is_empty() {
        blockers.push(object([
            ("kind", string("live-metadata-probe")),
            ("operation_id", string(operation_id)),
        ]));
    }
    let names: BTreeSet<&str> = files(inventory)?
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    for attempt in attempts {
        let prefix = format!("{PREFIX}{attempt}/");
        require(
            names.contains(format!("{prefix}plan.json").as_str()),
            "Metadata probe attempt has no owned plan",
        )?;
        let plan = read_value(&other.join(format!("{prefix}plan.json")))?;
        // probe_entries has already validated every complete attempt and pin
        // against this exact journal, without deleting any occupied path.
        let completion = read_optional(&other.join(format!("{prefix}completion.json")))?;
        let expected = object([
            (
                "schema",
                string("agent-memory/layout-metadata-probe-completion-v1"),
            ),
            ("operation_id", string(operation_id)),
            ("attempt_id", string(attempt)),
            ("plan_sha256", string(sha(&plan)?)),
        ]);
        if !completion.is_some_and(|completion| same(&completion, &expected)) {
            blockers.push(object([
                ("kind", string("unfinished-metadata-probe-attempt")),
                ("operation_id", string(operation_id)),
                ("attempt_id", string(attempt)),
            ]));
        }
    }
    Ok(blockers)
}

/// Sorted complete listing of the recovery workspace.
fn workspace_names(base: &Path) -> Result<Vec<String>> {
    trust::state(base)?;
    let held = Descriptor::open_directory(base).map_err(io_error)?;
    names_of(&held)
}

/// Whether a nested transaction journal has its verified completion.
fn nested_complete(
    root: &Path,
    bank: &Path,
    other: &Path,
    other_id: &str,
    name: &str,
    journal: &Value,
    row: &Value,
) -> Result<bool> {
    let nested_id = name.split('/').nth(1).ok_or_else(|| err(MALFORMED))?;
    let stem = name
        .strip_suffix("journal.json")
        .ok_or_else(|| err(MALFORMED))?;
    let completion = read_optional(&other.join(format!("{stem}completion.json")))?;
    let Some(completion) = completion else {
        return Ok(false);
    };
    if !(eq_text(
        get(&completion, "schema"),
        "agent-memory/layout-nested-completion-v1",
    ) && eq_text(get(&completion, "operation_id"), other_id)
        && eq_text(get(&completion, "nested_id"), nested_id)
        && is_true(get(&completion, "verified")))
    {
        return Ok(false);
    }
    if eq_text(get(&completion, "outcome"), "rolled-back") {
        return Ok(has_keys(
            &completion,
            &["schema", "operation_id", "nested_id", "outcome", "verified"],
        ));
    }
    let (_, canonical_path) = bank_transactions::locations(root, bank, nested_id)?;
    let receipt =
        writes::recovery_transactions_provider()?.nested_receipt(journal, &canonical_path)?;
    let Some(receipt) = receipt else {
        return Ok(false);
    };
    Ok(eq_text(
        get(&completion, "canonical_receipt_sha256"),
        &sha(&receipt)?,
    ) && eq_text(get(&receipt, "operation_id"), nested_id)
        && same(get(&receipt, "project_id"), get(row, "project_id")))
}

/// Whether a resolution journal has its exact verified completion.
fn resolution_complete(
    other: &Path,
    name: &str,
    journal: &Value,
    operation_id: &str,
) -> Result<bool> {
    let file_name = name.rsplit('/').next().unwrap_or(name);
    let resolution_id = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let completion = read_optional(&named_json(
        &other.join("resolution-completions"),
        resolution_id,
    )?)?;
    let receipt = read_optional(&named_json(&other.join("resolutions"), resolution_id)?)?;
    let Some(receipt) = receipt else {
        return Ok(false);
    };
    let expected = object([
        (
            "schema",
            string("agent-memory/layout-resolution-completion-v1"),
        ),
        ("resolution_id", string(resolution_id)),
        ("receipt_sha256", string(sha(&receipt)?)),
        ("journal_sha256", string(sha(journal)?)),
        ("verified", Value::Bool(true)),
    ]);
    if !completion.is_some_and(|completion| same(&completion, &expected)) {
        return Ok(false);
    }
    let Some(proposal) = journal.get("proposal") else {
        return Ok(false);
    };
    if !eq_text(get(proposal, "operation_id"), operation_id) {
        return Ok(false);
    }
    Ok(eq_text(get(&receipt, "request_sha256"), &sha(proposal)?))
}

/// Baseline `_dependencies(root, operation_id, base, directory)`.
fn dependencies(
    root: &Path,
    operation_id: &str,
    base: &Path,
    directory: &Path,
    runtime_root: &Path,
) -> Result<Value> {
    let index = state::index(root, runtime_root)?;
    // Union registry and physical discovery; a typed child need not invent an
    // index row/ledger, but no missing bound operation is considered absent.
    let bindings = state::bindings()?;
    let root_text = path_text(root)?;
    let bank_path = get(&index, "bank_path");
    let bound: Vec<(String, Value)> = object_pairs(get(&bindings, "bindings"))?
        .iter()
        .filter(|(_, row)| {
            eq_text(get(row, "repo_root"), root_text) || same(get(row, "bank_path"), bank_path)
        })
        .cloned()
        .collect();
    let names = workspace_names(base)?;
    require(
        names
            .iter()
            .all(|name| name == "index.json" || uuid_valid(&string(name.as_str()))),
        "Unknown recovery workspace occupant prevents a complete dependency proof",
    )?;
    let physical: BTreeSet<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| *name != "index.json")
        .collect();
    let indexed = object_pairs(operations(&index)?)?;
    require(
        indexed
            .iter()
            .map(|(key, _)| key)
            .chain(bound.iter().map(|(key, _)| key))
            .all(|key| physical.contains(key.as_str())),
        "Missing indexed/bound recovery operation; retain all private evidence",
    )?;
    let mut inventories = BTreeMap::new();
    for key in &physical {
        inventories.insert(*key, inventory(&base.join(key))?);
    }
    let mut rows: Vec<(String, Value)> = indexed.to_vec();
    for key in &physical {
        if rows.iter().any(|(name, _)| name == key) {
            continue;
        }
        let pins = inventories.get(key).ok_or_else(|| err(MALFORMED))?;
        let (info, _) = child_info(root, key, base, &index, pins, false, runtime_root)?;
        rows.push(((*key).to_owned(), info));
    }
    let bank = PathBuf::from(bank_path.as_str().ok_or_else(|| err(MALFORMED))?);
    let directory_text = path_text(directory)?;
    let mut pins = Vec::new();
    let mut blockers = Vec::new();
    for other_id in &physical {
        let other = base.join(other_id);
        let inventory = inventories.get(other_id).ok_or_else(|| err(MALFORMED))?;
        let row = rows
            .iter()
            .find(|(name, _)| name == other_id)
            .map(|(_, row)| row)
            .ok_or_else(|| err(MALFORMED))?;
        let child = eq_text(get(row, "kind"), "child-restore");
        blockers.extend(probe_dependencies(&bank, &other, other_id, inventory)?);
        if child && !is_true(get(row, "completed")) {
            blockers.push(object([
                ("kind", string("unfinished-child-restore")),
                ("operation_id", string(*other_id)),
            ]));
        }
        let own = *other_id == operation_id;
        for (name, pin) in files(inventory)? {
            if own && (name.starts_with("retention/") || name == "tombstone.json") {
                continue;
            }
            if name.ends_with(".json") {
                let value = read_value(&other.join(name))?;
                if name == "staging.json" {
                    require(
                        eq_text(get(&value, "operation_id"), other_id)
                            && same(get(&value, "project_id"), get(row, "project_id"))
                            && eq_text(get(&value, "repo_root"), root_text)
                            && same(get(&value, "bank_path"), bank_path),
                        "Partial staging evidence belongs to another operation or checkout",
                    )?;
                }
                if !own && references(&value, operation_id, directory_text) {
                    // A cleaned child has surrendered rollback and keeps only
                    // historical receipts; a live child still owns a dependency.
                    let released = eq_text(get(row, "state"), "cleaned")
                        || (child && is_true(get(row, "completed")));
                    if !released {
                        blockers.push(object([
                            ("kind", string("child-or-referencing-operation")),
                            ("operation_id", string(*other_id)),
                            ("artifact", string(name.as_str())),
                        ]));
                    }
                }
                if own
                    && name.starts_with("nested/")
                    && name.ends_with("/journal.json")
                    && !nested_complete(root, &bank, &other, other_id, name, &value, row)?
                {
                    blockers.push(object([
                        ("kind", string("unfinished-nested-transaction")),
                        ("artifact", string(name.as_str())),
                    ]));
                }
                if own
                    && name.starts_with("resolution-journals/")
                    && !resolution_complete(&other, name, &value, operation_id)?
                {
                    blockers.push(object([
                        ("kind", string("unfinished-resolution")),
                        ("artifact", string(name.as_str())),
                    ]));
                }
                if own && name.starts_with("publication/") && name.ends_with("/request.json") {
                    let stem = name
                        .strip_suffix("request.json")
                        .ok_or_else(|| err(MALFORMED))?;
                    let receipt = read_optional(&other.join(format!("{stem}receipt.json")))?;
                    let verified = receipt.is_some_and(|receipt| {
                        is_true(get(&receipt, "publication_verified"))
                            && sha(&value).is_ok_and(|request| {
                                eq_text(get(&receipt, "request_sha256"), &request)
                            })
                    });
                    if !verified {
                        blockers.push(object([
                            ("kind", string("unfinished-publication")),
                            ("artifact", string(name.as_str())),
                        ]));
                    }
                }
                if own && name == "retention-requirements.json" {
                    require(
                        has_keys(&value, &["schema", "operation_id", "requirements"])
                            && eq_text(
                                get(&value, "schema"),
                                "agent-memory/layout-retention-requirements-v1",
                            )
                            && eq_text(get(&value, "operation_id"), operation_id)
                            && get(&value, "requirements").as_array().is_some(),
                        "Unknown operator retention requirements are unresolved dependencies",
                    )?;
                    let count = items(get(&value, "requirements"))?.len();
                    if count > 0 {
                        blockers.push(object([
                            ("kind", string("operator-retention-requirement")),
                            ("count", number(count)),
                        ]));
                    }
                }
            }
            if !own {
                pins.push((format!("{other_id}/{name}"), pin.clone()));
            }
        }
    }
    require(
        same(&state::index(root, runtime_root)?, &index)
            && same(&state::bindings()?, &bindings)
            && workspace_names(base)? == names,
        "Recovery dependency discovery changed during its complete inventory",
    )?;
    let others = |pairs: Vec<(String, Value)>| {
        Value::Object(
            pairs
                .into_iter()
                .filter(|(key, _)| key != operation_id)
                .collect(),
        )
    };
    Ok(object([
        (
            "schema",
            string("agent-memory/layout-retention-dependencies-v1"),
        ),
        ("operations", others(rows)),
        ("bindings", others(bound)),
        ("artifact_pins", Value::Object(pins)),
        ("blockers", Value::Array(blockers)),
    ]))
}

/// Baseline `_retained(name)`: evidence every cleanup keeps.
pub fn retained(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "receipt.json",
        "journal.json",
        "retention-requirements.json",
        "quiescence-attestations.json",
        "staging.json",
        "retirement.json",
        "reference-map.json",
    ];
    const PREFIXES: &[&str] = &[
        "retention/",
        "resolutions/",
        "resolution-journals/",
        "resolution-completions/",
        "nested/",
        "proposals/",
        "recovery/",
        "revisions/",
        "coverage-reviews/",
        "union-reviews/",
        "discard-reviews/",
        "ledger-transitions/",
        "metadata-probe-attempts/",
        "quiescence-reviews/",
        "attestations/",
        "reviews/",
    ];
    NAMES.contains(&name)
        || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
        || (name.starts_with("publication/") && !name.contains("/originals/"))
}

/// Baseline `_preview(root, operation_id, cleanup_id, owner=)`.
fn preview(
    repo: &Path,
    operation_id: &str,
    cleanup_id: &str,
    owner: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, _, base, directory, terminal) =
        operation_evidence(repo, operation_id, owner, runtime_root)?;
    if !eq_text(get(&terminal, "kind"), "child-restore") {
        let index = state::index(&root, runtime_root)?;
        require(
            matches!(
                get(operation_row(&index, operation_id)?, "state").as_str(),
                Some("terminal" | "rolled-back")
            ),
            "Cleanup cannot retire an active/interrupted recovery operation",
        )?;
    }
    let project_id = field(&terminal, "project_id")?.clone();
    let context = scope_binding::require(
        &root,
        project_id.as_str(),
        scope_binding::RequireOptions {
            owned_lock: owner,
            ..Default::default()
        },
    )?;
    let pins = inventory(&directory)?;
    let dependencies = dependencies(&root, operation_id, &base, &directory, runtime_root)?;
    require(
        items(field(&dependencies, "blockers")?)?.is_empty(),
        "Recovery rollback, resolution, publication or operator retention dependencies remain",
    )?;
    let mut delete = Vec::new();
    let mut retain = Vec::new();
    for (name, pin) in files(&pins)? {
        if !retained(name) && name != "tombstone.json" {
            delete.push((name.clone(), pin.clone()));
        }
        if retained(name) && !name.starts_with("retention/") {
            retain.push((name.clone(), pin.clone()));
        }
    }
    Ok(object([
        ("schema", string(SCHEMA)),
        ("cleanup_id", string(cleanup_id)),
        ("operation_id", string(operation_id)),
        ("repo_root", string(path_text(&root)?)),
        ("project_id", project_id),
        ("scope_context", context),
        ("terminal", terminal),
        ("dependencies", dependencies),
        ("delete", Value::Object(delete)),
        ("retain", Value::Object(retain)),
        ("directories", field(&pins, "directories")?.clone()),
        ("rollback_capability_surrendered", Value::Bool(true)),
        ("review", Value::Null),
    ]))
}

/// Baseline `inspect(repo, operation_id, input=None)`.
pub fn inspect(
    repo: &Path,
    operation_id: &str,
    input: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    require(
        input.is_none_or(|input| has_keys(input, &["cleanup_id", "review"])),
        "Retention inspection accepts only an exact cleanup ID and separate independent review",
    )?;
    let cleanup_id = match input {
        None => string(host::uuid4()?),
        Some(input) => field(input, "cleanup_id")?.clone(),
    };
    require(
        uuid(&cleanup_id),
        "Cleanup requires a new stable canonical UUID",
    )?;
    let cleanup_text = cleanup_id.as_str().ok_or_else(|| err(MALFORMED))?;
    let mut proposal = preview(repo, operation_id, cleanup_text, None, runtime_root)?;
    if let Some(input) = input {
        proposal = with(proposal, [("review", field(input, "review")?.clone())]);
        review(&proposal)?;
    }
    let mut deletion_bytes: i128 = 0;
    for (_, pin) in object_pairs(field(&proposal, "delete")?)? {
        deletion_bytes += int(field(pin, "bytes")?).ok_or_else(|| err(MALFORMED))?;
    }
    Ok(publication::result(
        "retention-inspect",
        operation_id,
        "retention-preview",
        vec![
            ("proposal", proposal),
            ("cleanup_id", cleanup_id),
            ("deletion_bytes", number(deletion_bytes)),
            ("rollback_capability_surrendered", Value::Bool(true)),
            ("canonical_eligibility", string("unavailable")),
            ("semantic_recovery_complete", Value::Bool(false)),
        ],
    ))
}

/// Baseline `_review(proposal)`: separate independent review of this exact
/// dependency/file preview and rollback loss.
pub fn review(proposal: &Value) -> Result<()> {
    let review = field(proposal, "review")?;
    publication::exact(review, REVIEW_FIELDS, Some(REVIEW))?;
    let preview = with(proposal.clone(), [("review", Value::Null)]);
    require(
        uuid(get(review, "review_id"))
            && same(get(review, "cleanup_id"), get(proposal, "cleanup_id"))
            && eq_text(get(review, "preview_sha256"), &sha(&preview)?)
            && eq_text(get(review, "decision"), "approve")
            && is_true(get(review, "dependency_review_complete"))
            && is_true(get(review, "rollback_loss_acknowledged")),
        "Cleanup needs separate independent review of this exact dependency/file preview and rollback loss",
    )?;
    let reviewer = get(review, "reviewer");
    publication::exact(
        reviewer,
        &["developer_id", "harness", "session", "evidence"],
        None,
    )?;
    require(
        uuid(get(reviewer, "developer_id")),
        "Retention reviewer requires actual developer attribution",
    )?;
    for name in ["harness", "session", "evidence"] {
        publication::text(get(reviewer, name), publication::TEXT_MAXIMUM)?;
    }
    // Developer UUID is attribution, not authentication. Independence is a real
    // separate session, bound explicitly rather than guessed from source prose.
    let authorization = get(review, "operator_authorization");
    publication::exact(
        authorization,
        &[
            "developer_id",
            "author_harness",
            "author_session",
            "evidence",
            "statement",
        ],
        None,
    )?;
    let author_session = (
        get(authorization, "author_harness"),
        get(authorization, "author_session"),
    );
    let reviewer_session = (get(reviewer, "harness"), get(reviewer, "session"));
    require(
        same(
            get(authorization, "developer_id"),
            get(get(proposal, "scope_context"), "developer_id"),
        ) && !(same(reviewer_session.0, author_session.0)
            && same(reviewer_session.1, author_session.1)),
        "Cleanup author cannot self-approve the dependency review",
    )?;
    for name in ["author_harness", "author_session", "evidence", "statement"] {
        publication::text(get(authorization, name), publication::TEXT_MAXIMUM)?;
    }
    publication::text(get(review, "rationale"), publication::TEXT_MAXIMUM)?;
    lifecycle::time_value(get(review, "reviewed_at"))?;
    Ok(())
}

/// Baseline `_safe_tombstone(proposal, status)`.
pub fn safe_tombstone(proposal: &Value, status: &str) -> Value {
    object([
        (
            "schema",
            string("agent-memory/layout-retention-tombstone-v1"),
        ),
        ("operation_id", get(proposal, "operation_id").clone()),
        ("project_id", get(proposal, "project_id").clone()),
        ("cleanup_id", get(proposal, "cleanup_id").clone()),
        ("state", string(status)),
        ("original_bytes_available", Value::Bool(false)),
        ("rollback_available", Value::Bool(false)),
        ("historical_receipt_retained", Value::Bool(true)),
        ("cleanup_complete", Value::Bool(status == "cleaned")),
        (
            "retained_receipts",
            Value::Array(vec![string("receipt.json")]),
        ),
        ("private_content_included", Value::Bool(false)),
    ])
}

/// Baseline `_remaining(directory, proposal, progress)`: the current
/// inventory and the set of already missing delete targets.
pub fn remaining(
    directory: &Path,
    proposal: &Value,
    progress: &[String],
) -> Result<(Value, BTreeSet<String>)> {
    let pins = inventory(directory)?;
    let actual: Vec<&(String, Value)> = files(&pins)?
        .iter()
        .filter(|(name, _)| !name.starts_with("retention/") && name != "tombstone.json")
        .collect();
    let delete = field(proposal, "delete")?;
    let retain = field(proposal, "retain")?;
    let deleted = keys(delete);
    let kept = keys(retain);
    require(
        delete.as_object().is_some() && retain.as_object().is_some(),
        MALFORMED,
    )?;
    let present: BTreeSet<&str> = actual.iter().map(|(name, _)| name.as_str()).collect();
    require(
        present
            .iter()
            .all(|name| deleted.contains(name) || kept.contains(name))
            && kept.iter().all(|name| present.contains(name)),
        "Unknown new or missing retained artifact; interrupted cleanup remains pending",
    )?;
    for (name, pin) in &actual {
        let expected = delete.get(name).or_else(|| retain.get(name));
        require(
            expected.is_some_and(|expected| same(pin, expected)),
            "Changed private artifact is not owned by this exact cleanup preview",
        )?;
    }
    require(
        progress.iter().all(|name| !present.contains(name.as_str())),
        "A deleted artifact reappeared; do not delete its replacement",
    )?;
    let missing: BTreeSet<String> = deleted
        .iter()
        .filter(|name| !present.contains(*name))
        .map(|name| (*name).to_owned())
        .collect();
    // One missing current target is allowed for unlink-before-progress crash;
    // the exact durable intent is already present before any deletion.
    let ordered: Vec<String> = deleted.iter().map(|name| (*name).to_owned()).collect();
    let count = progress.len();
    let prefix = ordered
        .get(..count)
        .is_some_and(|prefix| prefix == progress);
    let done: BTreeSet<String> = progress.iter().cloned().collect();
    let next: BTreeSet<String> = ordered.iter().take(count + 1).cloned().collect();
    require(
        prefix && (missing == done || missing == next),
        "Missing artifacts are not an attainable ordered cleanup boundary",
    )?;
    Ok((pins, missing))
}

/// Baseline `_remove(directory, name, pin, directories)`: unlink one exact
/// pinned private file relative to its checked held parent.
pub fn remove(directory: &Path, name: &str, pin: &Value, directories: &Value) -> Result<()> {
    require(
        !name.is_empty()
            && !name.starts_with('/')
            && !name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".."),
        "Cleanup path escaped its operation",
    )?;
    let (key, file_name) = name.rsplit_once('/').unwrap_or(("", name));
    let parent = if key.is_empty() {
        directory.to_path_buf()
    } else {
        directory.join(key)
    };
    let expected_parent = field(directories, key)?;
    trust::state(&parent)?;
    let held = Descriptor::open_directory(&parent).map_err(io_error)?;
    let opened = held.stat().map_err(io_error)?;
    let signature = observation::signature(&opened);
    require(
        object_pairs(expected_parent)?
            .iter()
            .all(|(key, value)| signature.get(key).is_some_and(|actual| same(actual, value))),
        "Cleanup parent changed; preserve the replacement",
    )?;
    let raw = read_raw(&directory.join(name), sharing_limits::MAX_PRIVATE)?;
    let named = held.stat_at(OsStr::new(file_name)).map_err(io_error)?;
    require(
        same(&observation::signature(&named), field(pin, "physical")?)
            && int(field(pin, "bytes")?) == i128::try_from(raw.len()).ok()
            && eq_text(field(pin, "sha256")?, &layout_recovery::digest(&raw)),
        "Cleanup artifact changed immediately before removal",
    )?;
    require(
        lstat(&parent)?.is_some_and(|current| current.same_inode(opened)),
        "Cleanup parent binding changed",
    )?;
    held.unlink_at(OsStr::new(file_name)).map_err(io_error)?;
    held.fsync_directory().map_err(io_error)
}

fn progress_value(cleanup_id: &str, sequence: usize, path: &str) -> Value {
    object([
        (
            "schema",
            string("agent-memory/layout-retention-progress-v1"),
        ),
        ("cleanup_id", string(cleanup_id)),
        ("sequence", number(sequence)),
        ("path", string(path)),
    ])
}

/// Baseline `apply(repo, proposal)`.
pub fn apply(repo: &Path, supplied: &Value, runtime_root: &Path) -> Result<Value> {
    let proposal = match supplied.as_object() {
        Some(_) => supplied.get("proposal").unwrap_or(supplied),
        None => supplied,
    };
    publication::exact(proposal, PROPOSAL_FIELDS, Some(SCHEMA))?;
    require(
        uuid(get(proposal, "cleanup_id"))
            && uuid(get(proposal, "operation_id"))
            && is_true(get(proposal, "rollback_capability_surrendered")),
        "Use the exact reviewed cleanup proposal",
    )?;
    review(proposal)?;
    let operation_id = text(proposal, "operation_id")?;
    let cleanup_id = text(proposal, "cleanup_id")?;
    let (root, bank, base, directory) = operation_paths(repo, operation_id, runtime_root)?;
    require(
        eq_text(get(proposal, "repo_root"), path_text(&root)?),
        "Cleanup proposal belongs to another checkout",
    )?;
    let cleanup = directory.join("retention").join(cleanup_id);
    let request_sha256 = sha(proposal)?;
    if let Some(known) = read_optional(&cleanup.join("receipt.json"))? {
        require(
            eq_text(get(&known, "request_sha256"), &request_sha256)
                && same(&read_value(&cleanup.join("proposal.json"))?, proposal),
            "Cleanup UUID was reused with another request",
        )?;
        return Ok(publication::result(
            "retention-apply",
            operation_id,
            "retention-cleaned",
            vec![
                ("cleanup_id", string(cleanup_id)),
                ("receipt", known),
                ("already_applied", Value::Bool(true)),
                ("receipt_is_historical", Value::Bool(true)),
                ("original_bytes_available", Value::Bool(false)),
                ("rollback_available", Value::Bool(false)),
            ],
        ));
    }
    let project_id = text(proposal, "project_id")?;
    let scope_context = field(proposal, "scope_context")?;
    let expected_dependencies = field(proposal, "dependencies")?;
    let delete = field(proposal, "delete")?;
    let ordered: Vec<String> = keys(delete).into_iter().map(str::to_owned).collect();
    let (receipt, cache_state) = locked(&root, &bank, cleanup_id, |owner| {
        scope_binding::require(
            &root,
            Some(project_id),
            scope_binding::RequireOptions {
                expected: Some(scope_context),
                owned_lock: Some(owner),
                ..Default::default()
            },
        )?;
        let intent = match read_optional(&cleanup.join("intent.json"))? {
            Some(intent) => intent,
            None => {
                let current = preview(&root, operation_id, cleanup_id, Some(owner), runtime_root)?;
                require(
                    same(&with(proposal.clone(), [("review", Value::Null)]), &current),
                    "Retention files, terminal ledger or dependencies changed since review",
                )?;
                state::write_once(&cleanup.join("proposal.json"), proposal)?;
                let intent = object([
                    ("schema", string("agent-memory/layout-retention-intent-v1")),
                    ("cleanup_id", string(cleanup_id)),
                    ("operation_id", string(operation_id)),
                    ("request_sha256", string(request_sha256.as_str())),
                    (
                        "historical_receipt_sha256",
                        field(field(proposal, "terminal")?, "receipt_sha256")?.clone(),
                    ),
                ]);
                state::write_once(&cleanup.join("intent.json"), &intent)?;
                state::write_once(
                    &directory.join("tombstone.json"),
                    &safe_tombstone(proposal, "cleanup-interrupted"),
                )?;
                writes::fault("layout-recovery-retention-intent")?;
                intent
            }
        };
        require(
            eq_text(get(&intent, "request_sha256"), &request_sha256)
                && same(&read_value(&cleanup.join("proposal.json"))?, proposal),
            "Interrupted cleanup does not match this exact request",
        )?;
        let current = dependencies(&root, operation_id, &base, &directory, runtime_root)?;
        require(
            same(&current, expected_dependencies),
            "Cleanup dependency graph changed; preserve remaining artifacts",
        )?;
        let mut index = state::index(&root, runtime_root)?;
        let row_state = operations(&index)?
            .get(operation_id)
            .map(|row| get(row, "state").as_str().map(str::to_owned));
        require(
            match &row_state {
                None => true,
                Some(state) => matches!(
                    state.as_deref(),
                    Some("terminal" | "rolled-back" | "cleaned")
                ),
            },
            "A pending operation can never be cleaned",
        )?;
        match row_state {
            None => {
                child_info(
                    &root,
                    operation_id,
                    &base,
                    &index,
                    &inventory(&directory)?,
                    true,
                    runtime_root,
                )?;
            }
            Some(Some(state)) if state != "cleaned" => {
                // Durable tombstone and historical receipt precede the state change.
                // Readers must stop expecting deleted raw evidence before unlink.
                if let Some(row) = index
                    .get_mut("operations")
                    .and_then(|rows| rows.get_mut(operation_id))
                {
                    crate::layout_resolution::set(row, "state", string("cleaned"));
                }
                publish_index(&root, &base, &index, runtime_root)?;
            }
            Some(_) => {}
        }
        state::bind(
            &root,
            operation_id,
            project_id,
            text(scope_context, "checkout_key")?,
            "cleaned",
            runtime_root,
        )?;
        let frontier = frontier()?;
        let cache_state = frontier.invalidate(&root)?;
        require(
            frontier
                .marker_bytes(&bank, bank_transactions::MARKER, None)?
                .is_none()
                && frontier
                    .marker_bytes(&bank, crate::layout_resolution::SENTINEL_FILE, None)?
                    .is_none(),
            "A live recovery marker must never be removed by retention cleanup",
        )?;
        let mut progress = Vec::new();
        for (sequence, path) in ordered.iter().enumerate() {
            let event =
                read_optional(&cleanup.join("progress").join(format!("{sequence:08}.json")))?;
            let Some(event) = event else {
                break;
            };
            require(
                same(&event, &progress_value(cleanup_id, sequence, path)),
                "Corrupt cleanup progress",
            )?;
            progress.push(path.clone());
        }
        let (_, missing) = remaining(&directory, proposal, &progress)?;
        let directories = field(proposal, "directories")?;
        for (sequence, name) in ordered.iter().enumerate() {
            if sequence < progress.len() {
                continue;
            }
            require(
                same(
                    &dependencies(&root, operation_id, &base, &directory, runtime_root)?,
                    expected_dependencies,
                ),
                "A new dependency appeared during cleanup; retain all remaining evidence",
            )?;
            if !missing.contains(name) {
                remove(&directory, name, field(delete, name)?, directories)?;
            }
            writes::fault("layout-recovery-retention-after-delete")?;
            writes::fault(&format!(
                "layout-recovery-retention-after-delete-{sequence}"
            ))?;
            state::write_once(
                &cleanup.join("progress").join(format!("{sequence:08}.json")),
                &progress_value(cleanup_id, sequence, name),
            )?;
            writes::fault("layout-recovery-retention-after-progress")?;
        }
        remaining(&directory, proposal, &ordered)?;
        let tombstone = safe_tombstone(proposal, "cleaned");
        let tombstone_path = directory.join("tombstone.json");
        state::replace(
            &tombstone_path,
            &tombstone,
            Some(&file_sha(&tombstone_path)?),
        )?;
        writes::fault("layout-recovery-retention-final-tombstone")?;
        let receipt = object([
            ("schema", string("agent-memory/layout-retention-receipt-v1")),
            ("cleanup_id", string(cleanup_id)),
            ("operation_id", string(operation_id)),
            ("request_sha256", string(request_sha256.as_str())),
            ("verified", Value::Bool(true)),
            ("outcome", string("cleaned")),
            ("deleted_file_count", number(ordered.len())),
            ("historical_receipt_retained", Value::Bool(true)),
            ("original_bytes_available", Value::Bool(false)),
            ("rollback_available", Value::Bool(false)),
            ("readback_at", string(host::utc_now_isoformat()?)),
        ]);
        state::write_once(&cleanup.join("receipt.json"), &receipt)?;
        Ok((receipt, cache_state))
    })?;
    Ok(publication::result(
        "retention-apply",
        operation_id,
        "retention-cleaned",
        vec![
            ("cleanup_id", string(cleanup_id)),
            ("receipt", receipt),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("private_artifact_deletions_performed", Value::Bool(true)),
            ("original_bytes_available", Value::Bool(false)),
            ("rollback_available", Value::Bool(false)),
            ("semantic_recovery_complete", Value::Bool(false)),
            ("derived_cache_state", string(cache_state)),
        ],
    ))
}
