//! Proposals, staged recovery evidence and the convergent bank replacement.
use super::{
    FIELDS, LOCK, MAX_FILE, MAX_FILES, MAX_PLAN, Runtime, SCHEMA, audit, bounded_files, catalog,
    context, entries, err, hashes, hex64, keep_record, keys, lexists, locate, locations, path_text,
    read_record, read_regular, relative_text, require, result, retain_runtime, retained_helper,
    router_metadata, same, sha, snapshot, transform, unlink, validate_codec,
};
use crate::codec::json::Value;
use crate::developer_identity::{encoded, uuid_valid};
use crate::error::Result;
use crate::host::Sources;
use crate::lifecycle::{get, object, string, text};
use crate::migration_contract::route;
use crate::{bank_contract as bank, bank_transactions as tx, bank_writes as writes};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MIGRATION_ACTIONS: [&str; 2] = ["migration-finish", "migration-rollback"];

/// `[a-z0-9][a-z0-9_.-]{0,127}`
fn step_id(value: &Value) -> bool {
    let Some(name) = value.as_str() else {
        return false;
    };
    let plain = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let mut bytes = name.bytes();
    bytes.next().is_some_and(plain)
        && name.len() <= 128
        && bytes.all(|b| plain(b) || matches!(b, b'_' | b'.' | b'-'))
}

/// `agent-memory/[A-Za-z0-9_.-]{1,128}`
fn schema_name(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|name| name.strip_prefix("agent-memory/"))
        .is_some_and(|rest| {
            (1..=128).contains(&rest.len())
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        })
}

fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len() && fields.iter().all(|field| value.get(field).is_some())
    })
}

/// `str(Path(value).resolve()) == value` for an absolute string.
fn canonical_root(value: &Value) -> bool {
    value.as_str().is_some_and(|root| {
        Path::new(root).is_absolute()
            && crate::fs::canonical_path(Path::new(root), false)
                .is_ok_and(|resolved| resolved.to_str() == Some(root))
    })
}

fn chain_ids(chain: &Value) -> Option<Vec<&str>> {
    chain.as_array()?.iter().map(Value::as_str).collect()
}

fn base_shape(p: &Value) -> bool {
    let chain = get(p, "chain");
    let unique = chain_ids(chain).is_none_or(|ids| {
        let distinct: BTreeSet<&str> = ids.iter().copied().collect();
        distinct.len() == ids.len()
    });
    exact(p, FIELDS)
        && text(p, "schema") == Some(SCHEMA)
        && uuid_valid(get(p, "operation_id"))
        && uuid_valid(get(p, "project_id"))
        && canonical_root(get(p, "repo_root"))
        && matches!(text(p, "kind"), Some("migration" | "restore"))
        && get(p, "scope_context") != &Value::Null
        && chain.as_array().is_some_and(|ids| ids.len() <= 64)
        && unique
        && chain.as_array().is_some_and(|ids| ids.iter().all(step_id))
}

fn parent_pins(p: &Value) -> bool {
    let parent = get(p, "parent");
    let fields = [
        "operation_id",
        "proposal_sha256",
        "journal_sha256",
        "backup_sha256",
        "receipt_sha256",
    ];
    get(p, "chain").as_array().is_some_and(<[Value]>::is_empty)
        && exact(parent, &fields)
        && uuid_valid(get(parent, "operation_id"))
        && get(parent, "operation_id") != get(p, "operation_id")
        && fields.iter().skip(1).all(|field| hex64(get(parent, field)))
}

fn mode_value(value: &Value) -> bool {
    matches!(value, Value::Int(_))
        && value
            .as_int()
            .and_then(|mode| mode.as_i64())
            .is_some_and(|mode| (0..=0o777).contains(&mode))
}

fn inventories(p: &Value) -> Result<()> {
    for key in ["source_hashes", "resulting_hashes"] {
        let values = get(p, key);
        let complete = values
            .as_object()
            .is_some_and(|pairs| (1..=MAX_FILES).contains(&pairs.len()))
            && values.get("MEMORY.md").is_some();
        require(
            complete,
            "Migration requires complete source/result inventories",
        )?;
        for (path, digest) in entries(values) {
            if path != "MEMORY.md" {
                bank::path_value(&string(path.as_str()))?;
            }
            require(
                hex64(digest),
                "Migration inventory requires exact SHA256 values",
            )?;
        }
    }
    for (key, inventory) in [
        ("source_modes", "source_hashes"),
        ("resulting_modes", "resulting_hashes"),
    ] {
        let modes = get(p, key);
        let matching = modes.as_object().is_some()
            && keys(modes) == keys(get(p, inventory))
            && entries(modes).iter().all(|(_, mode)| mode_value(mode));
        require(
            matching,
            "Migration modes differ from the complete source/result inventory",
        )?;
    }
    let mut paths = keys(get(p, "source_hashes"));
    paths.extend(keys(get(p, "resulting_hashes")));
    bank::file_topology(paths)?;
    Ok(())
}

/// The validated proposal of a preview output or a bare proposal.
pub fn proposal_value(value: &Value) -> Result<Value> {
    let p = if value.as_object().is_some() {
        value.get("proposal").unwrap_or(value)
    } else {
        value
    };
    require(
        base_shape(p),
        "Invalid explicit migration proposal; use the original preview output",
    )?;
    if text(p, "kind") == Some("migration") {
        let complete = get(p, "chain")
            .as_array()
            .is_some_and(|ids| !ids.is_empty())
            && get(p, "parent") == &Value::Null;
        require(
            complete,
            "Format conversion needs its complete chain and no restoration parent",
        )?;
    } else {
        require(
            parent_pins(p),
            "Restoration needs a distinct verified original operation and exact source-record pins",
        )?;
    }
    let bank_root = text(p, "bank_root")
        .ok_or_else(|| err("Memory root requires an explicit repository-relative directory"))?;
    crate::repository_config::validate_root(bank_root)?;
    let scope = get(p, "scope_context");
    crate::scope_binding::context_value(scope, false)?;
    require(
        get(scope, "project_id") == get(p, "project_id"),
        "Migration project differs from the pinned scope",
    )?;
    for key in ["source_schema", "target_schema"] {
        require(
            schema_name(get(p, key)),
            "Invalid migration schema selector",
        )?;
    }
    for key in ["artifact_digest", "catalog_sha256", "preservation_sha256"] {
        require(
            hex64(get(p, key)),
            "Migration needs exact code/catalog/preservation pins",
        )?;
    }
    inventories(p)?;
    require(
        encoded(p)?.len() <= MAX_PLAN,
        "Complete migration proposal exceeds its reader bound",
    )?;
    Ok(p.clone())
}

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    text(value, key).unwrap_or_default()
}

fn readable(measured: &Value, schema: &str) -> bool {
    get(get(measured, "compatibility"), "readable_bank_schemas")
        .as_array()
        .is_some_and(|schemas| schemas.iter().any(|known| known.as_str() == Some(schema)))
}

fn step_names(chain: &[Value]) -> Value {
    Value::Array(chain.iter().map(|step| get(step, "id").clone()).collect())
}

/// The runtime still has the proposal's exact code, catalog and chain.
pub fn check_code(runtime: &Runtime, p: &Value) -> Result<Value> {
    let measured = catalog(runtime)?;
    let unchanged = get(&measured, "artifact_digest") == get(p, "artifact_digest")
        && get(get(&measured, "migration_catalog"), "sha256") == get(p, "catalog_sha256");
    require(
        unchanged,
        "Migration runtime/catalog changed; retain the original plan and trusted recovery package",
    )?;
    if text(p, "kind") == Some("restore") {
        return Ok(measured);
    }
    require(
        readable(&measured, text_of(p, "target_schema")),
        "Migration target is no longer directly readable",
    )?;
    let chain = route(
        get(&measured, "migration_catalog"),
        text_of(p, "source_schema"),
        text_of(p, "target_schema"),
    )?;
    let exact_chain = chain
        .as_ref()
        .is_some_and(|chain| &step_names(chain) == get(p, "chain"));
    require(
        exact_chain,
        "Migration no longer has its exact original complete chain",
    )?;
    Ok(measured)
}

fn bank_location(root: &Path, p: &Value) -> PathBuf {
    root.join(text_of(p, "bank_root"))
}

/// The bank still holds exactly the proposal's source bytes and modes.
pub fn check_source(
    p: &Value,
    root: &Path,
    target: &Path,
    owner: Option<&[u8]>,
    marker: Option<&[u8]>,
    expected_scope: Option<&Value>,
) -> Result<(Value, Value)> {
    require(
        root == Path::new(text_of(p, "repo_root")) && target == bank_location(root, p),
        "Migration repository or bank location changed",
    )?;
    let (files, modes) = snapshot(target, owner, marker)?;
    require(
        same(&hashes(&files), get(p, "source_hashes")) && same(&modes, get(p, "source_modes")),
        "Migration source bytes/modes changed; preserve edits and preview again",
    )?;
    let expected = expected_scope
        .filter(|scope| truthy(scope))
        .unwrap_or_else(|| get(p, "scope_context"));
    context(
        root,
        text_of(p, "project_id"),
        Some(expected),
        owner,
        marker,
    )?;
    Ok((files, modes))
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Object(pairs) => !pairs.is_empty(),
        _ => true,
    }
}

fn changed_paths(p: &Value) -> Value {
    let source = get(p, "source_hashes");
    let resulting = get(p, "resulting_hashes");
    let mut paths = keys(source);
    paths.extend(keys(resulting));
    Value::Array(
        paths
            .into_iter()
            .filter(|path| source.get(path) != resulting.get(path))
            .map(string)
            .collect(),
    )
}

fn record_path(directory: &Path, name: &str) -> Result<Value> {
    Ok(string(path_text(&directory.join(name))?))
}

/// Read-only preview of an explicit conversion to `target_schema`.
pub fn preview(runtime: &Runtime, repo: &Path, target_schema: &str) -> Result<Value> {
    let (root, target) = locate(repo)?;
    let measured = catalog(runtime)?;
    require(
        readable(&measured, target_schema),
        "Migration target is not directly readable by this runtime",
    )?;
    let (files, modes) = snapshot(&target, None, None)?;
    let router = text_of(&files, "MEMORY.md");
    let header = router_metadata(router.as_bytes())?
        .ok_or_else(|| err("Existing bank has no recognized format-family router"))?;
    let source = text_of(&header, "schema").to_owned();
    let (desired, chain, projection) =
        transform(runtime, &measured, &source, target_schema, &files)?;
    let project_id = text_of(&projection, "project_id").to_owned();
    let scope = context(&root, &project_id, None, None, None)?;
    let (current, current_modes) = snapshot(&target, None, None)?;
    let unchanged = same(&current, &files)
        && same(&current_modes, &modes)
        && get(&catalog(runtime)?, "artifact_digest") == get(&measured, "artifact_digest");
    require(
        unchanged,
        "Migration sources/runtime changed during preview; nothing was applied",
    )?;
    let operation_id = crate::host::uuid4()?;
    let directory = locations(runtime, &root, &target, &string(operation_id.as_str()))?;
    let audit = audit(
        &root,
        &operation_id,
        "mutation",
        "checked",
        &scope,
        "migrate",
        &target,
    )?;
    if chain.is_empty() {
        return Ok(result(
            "preview",
            vec![
                ("state", string("already-current")),
                ("source_schema", string(source)),
                ("target_schema", string(target_schema)),
                ("project_id", get(&projection, "project_id").clone()),
                ("migration_needed", Value::Bool(false)),
                ("proposal", Value::Null),
                ("verified", Value::Bool(true)),
                ("canonical_writes_performed", Value::Bool(false)),
                ("backup_created", Value::Bool(false)),
                ("audit", audit),
                ("private_audit_written", Value::Bool(true)),
            ],
        ));
    }
    let resulting_modes = Value::Object(
        entries(&desired)
            .iter()
            .map(|(path, _)| {
                let mode = modes.get(path).cloned().unwrap_or(Value::Int(0o644.into()));
                (path.clone(), mode)
            })
            .collect(),
    );
    let p = proposal_value(&object([
        ("schema", string(SCHEMA)),
        ("operation_id", string(operation_id.as_str())),
        ("repo_root", string(path_text(&root)?)),
        ("kind", string("migration")),
        ("parent", Value::Null),
        ("bank_root", string(relative_text(&root, &target)?)),
        ("project_id", get(&projection, "project_id").clone()),
        ("scope_context", scope),
        ("source_schema", string(source.as_str())),
        ("target_schema", string(target_schema)),
        ("artifact_digest", get(&measured, "artifact_digest").clone()),
        (
            "catalog_sha256",
            get(get(&measured, "migration_catalog"), "sha256").clone(),
        ),
        ("chain", step_names(&chain)),
        ("source_hashes", hashes(&files)),
        ("source_modes", modes.clone()),
        ("resulting_hashes", hashes(&desired)),
        ("resulting_modes", resulting_modes),
        (
            "preservation_sha256",
            get(&projection, "preservation_sha256").clone(),
        ),
    ]))?;
    Ok(result(
        "preview",
        vec![
            ("state", string("prepared")),
            ("proposal", p.clone()),
            ("migration_needed", Value::Bool(true)),
            ("verified", Value::Bool(false)),
            ("source_schema", string(source)),
            ("target_schema", string(target_schema)),
            ("project_id", get(&p, "project_id").clone()),
            ("changed_paths", changed_paths(&p)),
            ("backup_path", record_path(&directory, "backup.json")?),
            ("retained_helper", retained_helper(&directory)?),
            ("canonical_writes_performed", Value::Bool(false)),
            ("backup_created", Value::Bool(false)),
            ("private_audit_written", Value::Bool(true)),
            ("audit", audit),
        ],
    ))
}

fn request_sha(p: &Value) -> Result<String> {
    Ok(sha(&encoded(p)?))
}

/// The exact visible marker bytes that own a bank for this proposal.
pub fn marker_value(p: &Value) -> Result<Vec<u8>> {
    encoded(&object([
        ("schema", string("agent-memory/migration-marker-v1")),
        ("operation_id", get(p, "operation_id").clone()),
        ("request_sha256", string(request_sha(p)?)),
        ("project_id", get(p, "project_id").clone()),
    ]))
}

/// The visible marker when it is this proposal's own; `None` when absent.
pub fn owned_marker(target: &Path, p: &Value) -> Result<Option<Vec<u8>>> {
    if !lexists(&target.join(tx::MARKER))? {
        return Ok(None);
    }
    let (raw, _) = read_regular(target, tx::MARKER, MAX_FILE)?;
    require(
        raw == marker_value(p)?,
        "A different/unknown bank transaction owns the visible marker",
    )?;
    Ok(Some(raw))
}

/// Intent, retained runtime, verified backup and journal, before any bank edit.
#[allow(clippy::too_many_arguments)]
pub fn stage(
    runtime: &Runtime,
    p: &Value,
    root: &Path,
    target: &Path,
    files: &Value,
    modes: &Value,
    desired: &Value,
    measured: &Value,
) -> Result<(PathBuf, Value, Value, PathBuf)> {
    let directory = locations(runtime, root, target, get(p, "operation_id"))?;
    keep_record(
        &directory.join("intent.json"),
        &object([
            ("schema", string("agent-memory/migration-intent-v1")),
            ("proposal", p.clone()),
        ]),
    )?;
    writes::fault("migration-before-backup")?;
    let retained = retain_runtime(runtime, measured, &directory)?;
    let backup = object([
        ("schema", string("agent-memory/migration-backup-v1")),
        ("operation_id", get(p, "operation_id").clone()),
        ("request_sha256", string(request_sha(p)?)),
        ("project_id", get(p, "project_id").clone()),
        ("files", files.clone()),
        ("modes", modes.clone()),
    ]);
    let backup_sha = keep_record(&directory.join("backup.json"), &backup)?;
    let (readback, raw) = read_record(&directory.join("backup.json"), super::MAX_RECORD)?;
    require(
        same(&readback, &backup) && sha(&raw) == backup_sha,
        "Complete migration backup readback failed before canonical mutation",
    )?;
    writes::fault("migration-after-backup")?;
    let journal = object([
        ("schema", string("agent-memory/migration-journal-v1")),
        ("proposal", p.clone()),
        ("backup_sha256", string(backup_sha)),
        ("target_files", desired.clone()),
        ("target_modes", get(p, "resulting_modes").clone()),
        (
            "retained_artifact_digest",
            get(measured, "artifact_digest").clone(),
        ),
    ]);
    keep_record(&directory.join("journal.json"), &journal)?;
    Ok((directory, journal, backup, retained))
}

/// The original intent of an operation in this repository and bank.
pub fn read_intent(
    runtime: &Runtime,
    root: &Path,
    target: &Path,
    operation_id: &Value,
) -> Result<(PathBuf, Value)> {
    let directory = locations(runtime, root, target, operation_id)?;
    let (intent, _) = read_record(&directory.join("intent.json"), MAX_PLAN + 1024)?;
    require(
        exact(&intent, &["schema", "proposal"])
            && text(&intent, "schema") == Some("agent-memory/migration-intent-v1"),
        "Unknown migration intent; preserve all state",
    )?;
    let p = proposal_value(get(&intent, "proposal"))?;
    require(
        text(&p, "repo_root") == Some(path_text(root)?)
            && target == bank_location(root, &p)
            && get(&p, "operation_id") == operation_id,
        "Migration intent belongs to another repository, bank or operation",
    )?;
    Ok((directory, p))
}

const JOURNAL_FIELDS: [&str; 6] = [
    "schema",
    "proposal",
    "backup_sha256",
    "target_files",
    "target_modes",
    "retained_artifact_digest",
];
const BACKUP_FIELDS: [&str; 6] = [
    "schema",
    "operation_id",
    "request_sha256",
    "project_id",
    "files",
    "modes",
];

/// The journal and backup, each still exactly the original plan's.
pub fn read_journal(directory: &Path, p: &Value) -> Result<(Value, Value)> {
    let (journal, _) = read_record(&directory.join("journal.json"), super::MAX_RECORD)?;
    require(
        exact(&journal, &JOURNAL_FIELDS)
            && text(&journal, "schema") == Some("agent-memory/migration-journal-v1")
            && same(get(&journal, "proposal"), p)
            && get(&journal, "retained_artifact_digest") == get(p, "artifact_digest"),
        "Migration journal differs from the original reviewed proposal",
    )?;
    let (backup, raw) = read_record(&directory.join("backup.json"), super::MAX_RECORD)?;
    let differs =
        || err("Migration backup/target bytes or modes no longer match the original plan");
    let header = Some(sha(&raw).as_str()) == text(&journal, "backup_sha256")
        && exact(&backup, &BACKUP_FIELDS)
        && text(&backup, "schema") == Some("agent-memory/migration-backup-v1")
        && get(&backup, "operation_id") == get(p, "operation_id")
        && get(&backup, "project_id") == get(p, "project_id")
        && text(&backup, "request_sha256") == Some(request_sha(p)?.as_str());
    if !header {
        return Err(differs());
    }
    let source = hashes(bounded_files(get(&backup, "files"))?);
    if !same(&source, get(p, "source_hashes"))
        || !same(get(&backup, "modes"), get(p, "source_modes"))
    {
        return Err(differs());
    }
    let resulting = hashes(bounded_files(get(&journal, "target_files"))?);
    if !same(&resulting, get(p, "resulting_hashes"))
        || !same(get(&journal, "target_modes"), get(p, "resulting_modes"))
    {
        return Err(differs());
    }
    Ok((journal, backup))
}

const RECEIPT_FIELDS: [&str; 13] = [
    "schema",
    "operation_id",
    "project_id",
    "request_sha256",
    "outcome",
    "resulting_hashes",
    "resulting_modes",
    "backup_sha256",
    "preparation_cancelled",
    "outcome_format_revalidated",
    "outcome_directly_readable",
    "readback_at",
    "verified",
];

fn boolean(value: &Value, key: &str) -> Option<bool> {
    get(value, key).as_bool()
}

/// The verified receipt of this operation, if one exists.
pub fn receipt_value(directory: &Path, p: &Value) -> Result<Option<Value>> {
    let path = directory.join("receipt.json");
    if !lexists(&path)? {
        return Ok(None);
    }
    let (value, _) = read_record(&path, MAX_PLAN + 4096)?;
    let cancelled = boolean(&value, "preparation_cancelled");
    let known = exact(&value, &RECEIPT_FIELDS)
        && text(&value, "schema") == Some("agent-memory/migration-receipt-v1")
        && get(&value, "operation_id") == get(p, "operation_id")
        && get(&value, "project_id") == get(p, "project_id")
        && text(&value, "request_sha256") == Some(request_sha(p)?.as_str())
        && boolean(&value, "verified") == Some(true)
        && matches!(text(&value, "outcome"), Some("finished" | "rolled-back"))
        && cancelled.is_some()
        && !(cancelled == Some(true) && text(&value, "outcome") != Some("rolled-back"))
        && boolean(&value, "outcome_format_revalidated").is_some()
        && boolean(&value, "outcome_directly_readable").is_some();
    require(
        known,
        "Unknown migration receipt; do not replay over later changes",
    )?;
    let pin = get(&value, "backup_sha256");
    if !hex64(pin) && (pin != &Value::Null || cancelled != Some(true)) {
        return Err(err(
            "Migration receipt requires its exact canonical backup identity",
        ));
    }
    let journal_path = directory.join("journal.json");
    if lexists(&journal_path)? {
        let (journal, _) = read_record(&journal_path, super::MAX_RECORD)?;
        require(
            text(&journal, "schema") == Some("agent-memory/migration-journal-v1")
                && journal.get("proposal").is_some_and(|known| same(known, p))
                && journal.get("backup_sha256") == Some(pin)
                && journal.get("retained_artifact_digest") == Some(get(p, "artifact_digest")),
            "Migration receipt backup identity differs from its original journal",
        )?;
    } else {
        require(
            cancelled == Some(true),
            "Completed migration receipt requires its original journal identity",
        )?;
    }
    let finished = text(&value, "outcome") == Some("finished");
    let (expected, modes) = if finished {
        ("resulting_hashes", "resulting_modes")
    } else {
        ("source_hashes", "source_modes")
    };
    require(
        same(get(&value, "resulting_hashes"), get(p, expected))
            && same(get(&value, "resulting_modes"), get(p, modes)),
        "Migration receipt outcome differs from its exact original inventories",
    )?;
    crate::lifecycle::time_value(get(&value, "readback_at"))?;
    Ok(Some(value))
}

/// Every visible path is either its original or its planned state.
pub fn partial(files: &Value, modes: &Value, p: &Value, journal: &Value) -> Result<()> {
    let actual = hashes(files);
    let mut paths = keys(&actual);
    paths.extend(keys(get(p, "source_hashes")));
    paths.extend(keys(get(p, "resulting_hashes")));
    for path in paths {
        let observed = (actual.get(path), modes.get(path));
        let before = (
            get(p, "source_hashes").get(path),
            get(p, "source_modes").get(path),
        );
        let after = (
            get(p, "resulting_hashes").get(path),
            get(journal, "target_modes").get(path),
        );
        require(
            observed == before || observed == after,
            "Interrupted migration contains a newer external edit; automatic recovery preserves it",
        )?;
    }
    Ok(())
}

fn current_result(
    target: &Path,
    value: &Value,
    owner: Option<&[u8]>,
    marker: Option<&[u8]>,
) -> Result<bool> {
    let (files, modes) = snapshot(target, owner, marker)?;
    Ok(same(&hashes(&files), get(value, "resulting_hashes"))
        && same(&modes, get(value, "resulting_modes")))
}

fn action(outcome: &str) -> &'static str {
    if outcome == "finished" {
        MIGRATION_ACTIONS[0]
    } else {
        MIGRATION_ACTIONS[1]
    }
}

fn remove_marker(target: &Path) -> Result<()> {
    unlink(&target.join(tx::MARKER))
}

/// Read back an already recorded outcome without replaying it.
#[allow(clippy::too_many_arguments)]
pub fn historical(
    directory: &Path,
    root: &Path,
    target: &Path,
    p: &Value,
    value: &Value,
    owner: Option<&[u8]>,
    marker: Option<&[u8]>,
) -> Result<Value> {
    let operation_id = text_of(p, "operation_id");
    let scope = context(root, text_of(p, "project_id"), None, owner, marker)?;
    audit(
        root,
        operation_id,
        "retrieval",
        "checked",
        &scope,
        action(text_of(value, "outcome")),
        target,
    )?;
    let current = current_result(target, value, owner, marker)?;
    if let (Some(marker), true) = (marker, current) {
        tx::check_marker(target, marker)?;
        remove_marker(target)?;
    }
    Ok(result(
        "readback",
        vec![
            ("state", get(value, "outcome").clone()),
            ("receipt", value.clone()),
            ("verified", Value::Bool(true)),
            ("still_current", Value::Bool(current)),
            ("receipt_is_historical", Value::Bool(true)),
            ("current_runtime_readability", Value::Null),
            ("already_applied", Value::Bool(true)),
            ("canonical_writes_performed", Value::Bool(false)),
            ("cleanup_pending", Value::Bool(marker.is_some() && !current)),
            ("receipt_path", record_path(directory, "receipt.json")?),
            ("backup_path", record_path(directory, "backup.json")?),
            ("backup_availability", string("not-rechecked-historical")),
        ],
    ))
}

fn mode_bits(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_int)
        .and_then(|mode| mode.as_u64())
        .and_then(|mode| u32::try_from(mode).ok())
}

/// Drive the bank to the journal's target (`finish`) or the backup (`rollback`)
/// one recorded path at a time, then record the verified receipt.
#[allow(clippy::too_many_arguments)]
pub fn converge(
    runtime: &Runtime,
    directory: &Path,
    root: &Path,
    target: &Path,
    p: &Value,
    journal: &Value,
    backup: &Value,
    mode: &str,
    owner: &[u8],
    scope: &Value,
) -> Result<Value> {
    let finish = mode == "finish";
    let marker = marker_value(p)?;
    tx::check_marker(target, &marker)?;
    if let Some(known) = receipt_value(directory, p)? {
        let expected = if finish { "finished" } else { "rolled-back" };
        require(
            text(&known, "outcome") == Some(expected),
            "Migration already has a different verified outcome; use a new preview",
        )?;
        return historical(
            directory,
            root,
            target,
            p,
            &known,
            Some(owner),
            Some(&marker),
        );
    }
    let (desired, desired_modes) = if finish {
        (get(journal, "target_files"), get(journal, "target_modes"))
    } else {
        (get(backup, "files"), get(backup, "modes"))
    };
    let measured = if finish {
        check_code(runtime, p)?
    } else {
        catalog(runtime)?
    };
    if finish && text(p, "kind") == Some("migration") {
        let projection = object([
            ("project_id", get(p, "project_id").clone()),
            ("preservation_sha256", get(p, "preservation_sha256").clone()),
        ]);
        let source = validate_codec(
            runtime,
            &measured,
            text_of(p, "source_schema"),
            get(backup, "files"),
        )?;
        require(
            same(&source, &projection),
            "Recorded migration representations differ from the reviewed preservation proof",
        )?;
        let converted = validate_codec(runtime, &measured, text_of(p, "target_schema"), desired)?;
        require(
            same(&converted, &projection),
            "Recorded migration representations differ from the reviewed preservation proof",
        )?;
    }
    let (files, modes) = snapshot(target, Some(owner), Some(&marker))?;
    partial(&files, &modes, p, journal)?;
    let mut running: Vec<(String, Value)> = entries(&hashes(&files)).to_vec();
    let mut running_modes: Vec<(String, Value)> = entries(&modes).to_vec();
    let mut participants: BTreeSet<String> = keys(get(p, "source_hashes"))
        .into_iter()
        .map(str::to_owned)
        .collect();
    participants.extend(
        keys(get(p, "resulting_hashes"))
            .into_iter()
            .map(str::to_owned),
    );
    let desired_hashes = hashes(desired);
    let mut changed: Vec<String> = {
        let running = Value::Object(running.clone());
        let running_modes = Value::Object(running_modes.clone());
        let mut paths = keys(&running);
        paths.extend(keys(desired));
        paths
            .into_iter()
            .filter(|path| {
                (running.get(path), running_modes.get(path))
                    != (desired_hashes.get(path), desired_modes.get(path))
            })
            .map(str::to_owned)
            .collect()
    };
    changed.sort_by(|a, b| (a == "MEMORY.md", a).cmp(&(b == "MEMORY.md", b)));
    let operation_id = text_of(p, "operation_id");
    let project_id = text_of(p, "project_id");
    let action = if finish {
        MIGRATION_ACTIONS[0]
    } else {
        MIGRATION_ACTIONS[1]
    };
    audit(
        root,
        operation_id,
        "mutation",
        "started",
        scope,
        action,
        target,
    )?;
    for path in &changed {
        let expected = Value::Object(running.clone());
        let expected_modes = Value::Object(running_modes.clone());
        let check_current = || -> Result<()> {
            context(root, project_id, Some(scope), Some(owner), Some(&marker))?;
            let (actual, actual_modes) = snapshot(target, Some(owner), Some(&marker))?;
            require(
                same(&hashes(&actual), &expected) && same(&actual_modes, &expected_modes),
                "Bank bytes/modes changed immediately before migration replacement",
            )?;
            if finish {
                check_code(runtime, p)?;
            }
            Ok(())
        };
        check_current()?;
        let body = text(desired, path);
        tx::replace_file(
            target,
            path,
            body,
            &expected,
            &marker,
            &participants,
            mode_bits(desired_modes.get(path)),
            Some(&check_current),
        )?;
        running.retain(|(name, _)| name != path);
        running_modes.retain(|(name, _)| name != path);
        if let Some(body) = body {
            running.push((path.clone(), string(sha(body.as_bytes()))));
            running_modes.push((path.clone(), get(desired_modes, path).clone()));
        }
        writes::fault(&format!("migration-after-write:{path}"))?;
    }
    running.sort_by(|a, b| a.0.cmp(&b.0));
    running_modes.sort_by(|a, b| a.0.cmp(&b.0));
    let running = Value::Object(running);
    let running_modes = Value::Object(running_modes);
    let check_current = || -> Result<()> {
        context(root, project_id, Some(scope), Some(owner), Some(&marker))?;
        let (actual, actual_modes) = snapshot(target, Some(owner), Some(&marker))?;
        require(
            same(&hashes(&actual), &running) && same(&actual_modes, &running_modes),
            "Bank bytes/modes changed immediately before migration replacement",
        )?;
        if finish {
            check_code(runtime, p)?;
        }
        Ok(())
    };
    check_current()?;
    let (final_files, final_modes) = snapshot(target, Some(owner), Some(&marker))?;
    require(
        same(&final_files, desired) && same(&final_modes, desired_modes),
        "Complete migration outcome readback differs; preserve recovery evidence",
    )?;
    let outcome_schema = if finish {
        text_of(p, "target_schema")
    } else {
        text_of(p, "source_schema")
    };
    let revalidated = get(get(&measured, "migration_catalog"), "codecs")
        .get(outcome_schema)
        .is_some();
    if revalidated {
        validate_codec(runtime, &measured, outcome_schema, &final_files)?;
    }
    writes::fault("migration-before-receipt")?;
    check_current()?;
    let (actual_journal, actual_backup) = read_journal(directory, p)?;
    require(
        same(&actual_journal, journal) && same(&actual_backup, backup),
        "Migration recovery evidence changed before receipt",
    )?;
    let retained = runtime
        .retained(&directory.join(super::RETAINED))
        .inspect()?;
    require(
        get(&retained, "artifact_digest") == get(p, "artifact_digest"),
        "Retained trusted recovery package changed before receipt",
    )?;
    let outcome = if finish { "finished" } else { "rolled-back" };
    let value = object([
        ("schema", string("agent-memory/migration-receipt-v1")),
        ("operation_id", get(p, "operation_id").clone()),
        ("project_id", get(p, "project_id").clone()),
        ("request_sha256", string(request_sha(p)?)),
        ("outcome", string(outcome)),
        ("resulting_hashes", running.clone()),
        ("resulting_modes", running_modes.clone()),
        ("backup_sha256", get(journal, "backup_sha256").clone()),
        ("preparation_cancelled", Value::Bool(false)),
        ("outcome_format_revalidated", Value::Bool(revalidated)),
        (
            "outcome_directly_readable",
            Value::Bool(readable(&measured, outcome_schema)),
        ),
        (
            "readback_at",
            string(crate::local_registry::now(Sources::default())?),
        ),
        ("verified", Value::Bool(true)),
    ]);
    keep_record(&directory.join("receipt.json"), &value)?;
    writes::fault("migration-after-receipt")?;
    require(
        current_result(target, &value, Some(owner), Some(&marker))?,
        "Bank changed after migration receipt; preserve the marker and newer edit",
    )?;
    context(root, project_id, Some(scope), Some(owner), Some(&marker))?;
    remove_marker(target)?;
    writes::fault("migration-after-marker-removal")?;
    audit(
        root,
        operation_id,
        "mutation",
        "completed",
        scope,
        action,
        target,
    )?;
    Ok(result(
        mode,
        vec![
            ("state", string(outcome)),
            ("migration_kind", get(p, "kind").clone()),
            ("verified", Value::Bool(true)),
            ("still_current", Value::Bool(true)),
            ("receipt", value),
            (
                "canonical_writes_performed",
                Value::Bool(!changed.is_empty()),
            ),
            ("already_applied", Value::Bool(false)),
            ("receipt_path", record_path(directory, "receipt.json")?),
            ("backup_path", record_path(directory, "backup.json")?),
            ("retained_helper", retained_helper(directory)?),
        ],
    ))
}

/// Apply a reviewed conversion or restoration proposal.
pub fn apply(runtime: &Runtime, value: &Value, recovery_scope: Option<&Value>) -> Result<Value> {
    let p = proposal_value(value)?;
    let (root, target) = locate(Path::new(text_of(&p, "repo_root")))?;
    require(
        target == bank_location(&root, &p),
        "Migration bank configuration changed",
    )?;
    let directory = locations(runtime, &root, &target, get(&p, "operation_id"))?;
    let marker = owned_marker(&target, &p)?;
    let project_id = text_of(&p, "project_id");
    let operation_id = text_of(&p, "operation_id");
    context(&root, project_id, None, None, marker.as_deref())?;
    let _barrier = crate::turn_sources::canonical_writer_barrier(&root, None, &runtime.root)?;
    crate::local_registry::exclusive(
        &target.join(LOCK),
        operation_id,
        "bank-migrate",
        Sources::default(),
        |owner| {
            let marker = marker.as_deref();
            let scope = context(&root, project_id, None, Some(owner), marker)?;
            if let Some(known) = receipt_value(&directory, &p)? {
                return historical(&directory, &root, &target, &p, &known, Some(owner), marker);
            }
            require(
                marker.is_none(),
                "Migration is interrupted; inspect explicit recover finish/rollback before retrying",
            )?;
            let expected = recovery_scope.unwrap_or_else(|| get(&p, "scope_context"));
            context(&root, project_id, Some(expected), Some(owner), None)?;
            let measured = check_code(runtime, &p)?;
            let (files, modes) = check_source(&p, &root, &target, Some(owner), None, Some(&scope))?;
            let desired = if text(&p, "kind") == Some("migration") {
                let (desired, chain, projection) = transform(
                    runtime,
                    &measured,
                    text_of(&p, "source_schema"),
                    text_of(&p, "target_schema"),
                    &files,
                )?;
                require(
                    same(&hashes(&desired), get(&p, "resulting_hashes"))
                        && &step_names(&chain) == get(&p, "chain")
                        && get(&projection, "preservation_sha256")
                            == get(&p, "preservation_sha256"),
                    "Migration converter output differs from the original reviewed plan",
                )?;
                desired
            } else {
                restoration_payload(runtime, &root, &target, &p)?
            };
            let started = if text(&p, "kind") == Some("migration") {
                "migrate"
            } else {
                "migration-restore"
            };
            audit(
                &root,
                operation_id,
                "mutation",
                "started",
                &scope,
                started,
                &target,
            )?;
            let (directory, journal, backup, _) = stage(
                runtime, &p, &root, &target, &files, &modes, &desired, &measured,
            )?;
            let (actual_journal, actual_backup) = read_journal(&directory, &p)?;
            require(
                same(&actual_journal, &journal) && same(&actual_backup, &backup),
                "Migration recovery evidence changed before its first canonical edit",
            )?;
            check_source(&p, &root, &target, Some(owner), None, Some(&scope))?;
            check_code(runtime, &p)?;
            writes::fault("migration-before-marker")?;
            check_source(&p, &root, &target, Some(owner), None, Some(&scope))?;
            tx::create_marker(&target.join(tx::MARKER), &marker_value(&p)?)?;
            writes::fault("migration-after-marker")?;
            converge(
                runtime, &directory, &root, &target, &p, &journal, &backup, "finish", owner, &scope,
            )
        },
    )
}

/// `(parent, journal, backup, receipt, pins)` of a finished operation.
pub(crate) fn parent_records(
    runtime: &Runtime,
    root: &Path,
    target: &Path,
    operation_id: &Value,
) -> Result<(Value, Value, Value, Value, Value)> {
    let (directory, parent) = read_intent(runtime, root, target, operation_id)?;
    let (journal, backup) = read_journal(&directory, &parent)?;
    let receipt = receipt_value(&directory, &parent)?
        .filter(|receipt| text(receipt, "outcome") == Some("finished"))
        .ok_or_else(|| {
            err("Restoration requires an actually completed original migration or restoration")
        })?;
    let (_, journal_raw) = read_record(&directory.join("journal.json"), super::MAX_RECORD)?;
    let (_, receipt_raw) = read_record(&directory.join("receipt.json"), MAX_PLAN + 4096)?;
    let pins = object([
        ("operation_id", operation_id.clone()),
        ("proposal_sha256", string(request_sha(&parent)?)),
        ("journal_sha256", string(sha(&journal_raw))),
        ("backup_sha256", get(&journal, "backup_sha256").clone()),
        ("receipt_sha256", string(sha(&receipt_raw))),
    ]);
    Ok((parent, journal, backup, receipt, pins))
}

/// The original backup files a restoration proposal restores.
pub fn restoration_payload(
    runtime: &Runtime,
    root: &Path,
    target: &Path,
    p: &Value,
) -> Result<Value> {
    let (parent, _, backup, receipt, pins) =
        parent_records(runtime, root, target, get(get(p, "parent"), "operation_id"))?;
    let matching = same(&pins, get(p, "parent"))
        && get(&parent, "project_id") == get(p, "project_id")
        && get(p, "source_schema") == get(&parent, "target_schema")
        && get(p, "target_schema") == get(&parent, "source_schema")
        && same(get(p, "source_hashes"), get(&receipt, "resulting_hashes"))
        && same(get(p, "source_modes"), get(&receipt, "resulting_modes"))
        && same(get(p, "resulting_hashes"), get(&parent, "source_hashes"))
        && same(get(p, "resulting_modes"), get(&parent, "source_modes"))
        && get(p, "preservation_sha256") == get(&parent, "preservation_sha256");
    require(
        matching,
        "Restoration no longer matches the exact original finished operation and backup",
    )?;
    Ok(get(&backup, "files").clone())
}

/// Preview a NEW transaction restoring a finished operation's backup.
pub fn restore_preview(runtime: &Runtime, repo: &Path, operation_id: &str) -> Result<Value> {
    let (root, target) = locate(repo)?;
    let measured = catalog(runtime)?;
    let (parent, _, backup, receipt, pins) =
        parent_records(runtime, &root, &target, &string(operation_id))?;
    let (files, modes) = snapshot(&target, None, None)?;
    let scope = context(&root, text_of(&parent, "project_id"), None, None, None)?;
    require(
        same(&hashes(&files), get(&receipt, "resulting_hashes"))
            && same(&modes, get(&receipt, "resulting_modes")),
        "Newer memory differs from the original migrated result; backup restoration must not erase it",
    )?;
    let fresh_id = crate::host::uuid4()?;
    let directory = locations(runtime, &root, &target, &string(fresh_id.as_str()))?;
    let p = proposal_value(&object([
        ("schema", string(SCHEMA)),
        ("operation_id", string(fresh_id.as_str())),
        ("kind", string("restore")),
        ("parent", pins),
        ("repo_root", string(path_text(&root)?)),
        ("bank_root", string(relative_text(&root, &target)?)),
        ("project_id", get(&parent, "project_id").clone()),
        ("scope_context", scope.clone()),
        ("source_schema", get(&parent, "target_schema").clone()),
        ("target_schema", get(&parent, "source_schema").clone()),
        ("artifact_digest", get(&measured, "artifact_digest").clone()),
        (
            "catalog_sha256",
            get(get(&measured, "migration_catalog"), "sha256").clone(),
        ),
        ("chain", Value::Array(Vec::new())),
        ("source_hashes", get(&receipt, "resulting_hashes").clone()),
        ("source_modes", modes.clone()),
        ("resulting_hashes", get(&parent, "source_hashes").clone()),
        ("resulting_modes", get(&backup, "modes").clone()),
        (
            "preservation_sha256",
            get(&parent, "preservation_sha256").clone(),
        ),
    ]))?;
    restoration_payload(runtime, &root, &target, &p)?;
    let (current, current_modes) = snapshot(&target, None, None)?;
    context(&root, text_of(&p, "project_id"), Some(&scope), None, None)?;
    let unchanged = same(&current, &files)
        && same(&current_modes, &modes)
        && get(&catalog(runtime)?, "artifact_digest") == get(&p, "artifact_digest");
    require(
        unchanged,
        "Restoration sources/runtime changed during preview",
    )?;
    let audit = audit(
        &root,
        &fresh_id,
        "mutation",
        "checked",
        &scope,
        "migration-restore",
        &target,
    )?;
    let target_schema = text_of(&p, "target_schema").to_owned();
    Ok(result(
        "restore-preview",
        vec![
            ("state", string("prepared")),
            ("proposal", p.clone()),
            ("verified", Value::Bool(false)),
            ("original_operation_id", string(operation_id)),
            ("source_schema", get(&p, "source_schema").clone()),
            ("target_schema", get(&p, "target_schema").clone()),
            (
                "target_directly_readable",
                Value::Bool(readable(&measured, &target_schema)),
            ),
            ("canonical_writes_performed", Value::Bool(false)),
            ("backup_created", Value::Bool(false)),
            ("private_audit_written", Value::Bool(true)),
            ("audit", audit),
            ("backup_path", record_path(&directory, "backup.json")?),
            ("retained_helper", retained_helper(&directory)?),
        ],
    ))
}
