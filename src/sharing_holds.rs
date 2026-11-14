//! Repository/worktree-bound unresolved review evidence with atomic recovery.
//! Private evidence constrains review; it never admits canonical learning.
use crate::bank_writes::{self as writes, StateEnvironment};
use crate::codec::json::Value;
use crate::developer_identity::{encoded, load_with_limit, sha256_hex, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::host::{self, Sources};
use crate::lifecycle::{get, object, string};
use crate::local_registry as storage;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/known-semantic-holds-v2";
pub const MAX_RECORDS: usize = 4096;
pub const MAX_STORE: usize = 16 * 1024 * 1024;
pub const MAX_JOURNAL: usize = 2 * MAX_STORE + 65536;
pub const MAX_MARKER: usize = 4096;
pub const MAX_LEGACY: usize = 65536;
const RECOVERY_SCHEMA: &str = "agent-memory/project-lock-recovery-v1";
const RECOVERY_REPAIR: &str = "Preserve the bank, registry, recovery copy and journal. Run am semantic recover-holds with the same --repo and verified --project-id, then repeat inspect-v2 and the source review. Recovery verifies ownership and will not overwrite unknown later evidence.";
const CAPACITY_REPAIR: &str = "Preserve local learning, pending review, the registry, recovery copy, journal and all predecessor evidence. This runtime cannot retain the proposed additional metadata. A supported, independently reviewed capacity/retention upgrade is required; recovery and identical retries do not increase capacity. Do not prune disputes or history to force this write.";

fn err(reason: &str) -> ContractError {
    ContractError::new(reason)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "known hold path")
}
fn fields(value: &Value, required: &[&str], optional: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        required.iter().all(|key| value.get(key).is_some())
            && pairs.iter().all(|(key, _)| {
                required.contains(&key.as_str()) || optional.contains(&key.as_str())
            })
    })
}
fn pairs(value: &Value) -> Result<&[(String, Value)]> {
    value
        .as_object()
        .ok_or_else(|| err("Known review evidence has an unknown format"))
}
fn rows(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .ok_or_else(|| err("Known hold references are incomplete; no resolution is inferred"))
}
fn set(value: &mut Value, key: &str, new: Value) -> Result<()> {
    let Value::Object(pairs) = value else {
        return Err(err("Known review evidence has an unknown format"));
    };
    if let Some((_, current)) = pairs.iter_mut().find(|(name, _)| name == key) {
        *current = new;
    } else {
        pairs.push((key.into(), new));
    }
    Ok(())
}
fn remove(value: &mut Value, key: &str) {
    if let Value::Object(pairs) = value {
        pairs.retain(|(name, _)| name != key);
    }
}
fn sha(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn without_checksum(value: &Value) -> Result<Value> {
    Ok(Value::Object(
        pairs(value)?
            .iter()
            .filter(|(k, _)| k != "checksum")
            .cloned()
            .collect(),
    ))
}
fn checksum_valid(value: &Value) -> Result<bool> {
    Ok(get(value, "checksum").as_str()
        == Some(sha256_hex(&encoded(&without_checksum(value)?)?).as_str()))
}
fn digest(raw: Option<&[u8]>) -> Value {
    raw.map(|bytes| string(sha256_hex(bytes)))
        .unwrap_or(Value::Null)
}
fn equivalent(left: &Value, right: &Value) -> Result<bool> {
    // JSON object insertion order is not part of Python dictionary equality.
    Ok(encoded(left)? == encoded(right)?)
}
pub fn recovery_needed(code: &str, reason: &str) -> ContractError {
    ContractError::hold_failure(code, reason, RECOVERY_REPAIR)
}
pub fn cause_id(cause: &Value) -> Result<String> {
    let mut sources = rows(get(cause, "sources"))?
        .iter()
        .map(|v| {
            v.as_str().map(str::to_owned).ok_or_else(|| {
                err("Known hold references are incomplete; no resolution is inferred")
            })
        })
        .collect::<Result<Vec<_>>>()?;
    sources.sort();
    if cause.get("classification").is_none() {
        return Err(err(
            "Known hold references are incomplete; no resolution is inferred",
        ));
    }
    Ok(sha256_hex(&encoded(&object([
        (
            "sources",
            Value::Array(sources.into_iter().map(string).collect()),
        ),
        ("classification", get(cause, "classification").clone()),
    ]))?))
}

#[derive(Clone, Debug)]
pub struct Locations {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub directory: PathBuf,
    pub anchor: PathBuf,
}
pub fn locations(root: &Path, project_id: &str, runtime_root: &Path) -> Result<Locations> {
    locations_with_environment(root, project_id, runtime_root, &StateEnvironment::current())
}
pub fn locations_with_environment(
    root: &Path,
    project_id: &str,
    runtime_root: &Path,
    environment: &StateEnvironment,
) -> Result<Locations> {
    if !uuid_valid(&string(project_id)) {
        return Err(err("Known holds require the actual canonical project UUID"));
    }
    let (root, bank) = writes::bank_path(root)?;
    let (operations, _) = writes::state_paths_with(&root, &bank, &host::uuid4()?, environment)?;
    let base = operations
        .parent()
        .ok_or_else(|| err("Invalid known hold state path"))?
        .join("semantic-holds");
    let directory = base.join(project_id);
    let source = fs::canonical_path(runtime_root, false)?;
    if fs::canonical_path(&directory, false)?.starts_with(source) {
        return Err(err(
            "Known hold state must stay outside the executing runtime/source",
        ));
    }
    Ok(Locations {
        root,
        bank,
        directory,
        anchor: base.join(format!("{project_id}.present.json")),
    })
}
pub fn seal(value: &Value) -> Result<Value> {
    let checksum = sha256_hex(&encoded(value)?);
    let mut result = value.clone();
    set(&mut result, "checksum", string(checksum))?;
    Ok(result)
}
fn exhausted(code: &str, amount: usize, limit: usize, dimension: &str) -> ContractError {
    ContractError::hold_failure(
        code,
        format!(
            "Known-dispute {dimension} capacity exhausted: proposed {amount}; supported limit {limit}."
        ),
        CAPACITY_REPAIR,
    )
}
pub fn registry_value<'a>(
    value: &'a Value,
    root: &Path,
    project_id: &str,
    successor: bool,
) -> Result<&'a Value> {
    if !fields(
        value,
        &["schema", "repo_root", "project_id", "records", "checksum"],
        &[],
    ) || get(value, "schema").as_str() != Some(SCHEMA)
        || get(value, "repo_root").as_str() != Some(text(root)?)
        || get(value, "project_id").as_str() != Some(project_id)
        || !checksum_valid(value)?
        || get(value, "records").as_object().is_none()
        || (!successor
            && (pairs(get(value, "records"))?.len() > MAX_RECORDS
                || encoded(value)?.len() > MAX_STORE))
    {
        return Err(err(
            "Known review inventory is changed/oversized; preserve it for recovery",
        ));
    }
    let records = pairs(get(value, "records"))?;
    for (key, record) in records {
        if !fields(
            record,
            &[
                "binding",
                "cause",
                "references",
                "dependencies",
                "judgment_sha256",
                "proof_sha256",
                "resolution",
            ],
            &["resolution_history"],
        ) {
            return Err(err("Known review evidence has an unknown format"));
        }
        if let Some(history) = record.get("resolution_history") {
            let valid = history.as_array().is_some_and(|history| {
                !history.is_empty()
                    && (successor || history.len() <= MAX_RECORDS)
                    && get(record, "resolution").as_object().is_some()
                    && history.iter().all(|previous| {
                        fields(
                            previous,
                            &[
                                "basis",
                                "evidence",
                                "reason",
                                "endpoint_sources",
                                "judgment_sha256",
                                "proof_sha256",
                            ],
                            &[],
                        )
                    })
            });
            if !valid {
                return Err(err(
                    "Known resolution history is incomplete; preserve it for recovery",
                ));
            }
        }
        let cause = get(record, "cause");
        let sources = get(cause, "sources").as_array();
        let references = get(record, "references").as_object();
        if !fields(
            cause,
            &["sources", "classification", "reason", "origin"],
            &[],
        ) || !get(cause, "classification").as_str().is_some_and(|s| {
            [
                "contradiction",
                "uncertain",
                "withheld",
                "uncertain-selection",
            ]
            .contains(&s)
        }) || !sources.is_some_and(|s| {
            (1..=2).contains(&s.len())
                && s.iter().all(sha)
                && s.iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == s.len()
        }) || key != &cause_id(cause)?
            || !references.is_some_and(|r| {
                let names: BTreeSet<_> = r.iter().map(|(k, _)| k.as_str()).collect();
                sources.is_some_and(|s| names == s.iter().filter_map(Value::as_str).collect())
            })
            || get(record, "dependencies").as_object().is_none()
            || !get(cause, "reason")
                .as_str()
                .is_some_and(|s| !crate::bank_contract::trim(s).is_empty() && s.len() <= 4096)
            || !sha(get(record, "judgment_sha256"))
            || !sha(get(record, "proof_sha256"))
            || !sha(get(cause, "origin"))
        {
            return Err(err(
                "Known hold references are incomplete; no resolution is inferred",
            ));
        }
        for (_, reference) in pairs(get(record, "references"))? {
            if !fields(
                reference,
                &["path", "component", "target_path", "source_sha256_by_side"],
                &["record"],
            ) {
                return Err(err("Known logical/source reference is incomplete"));
            }
            if get(reference, "path").as_str() != Some("MEMORY.md") {
                crate::bank_contract::path_value(get(reference, "path"))?;
            }
            if let Some(record) = reference
                .get("record")
                .filter(|v| !matches!(v, Value::Null))
            {
                crate::records::reference(record)?;
            }
        }
    }
    if successor {
        if records.len() > MAX_RECORDS {
            return Err(exhausted(
                "known-holds-record-capacity",
                records.len(),
                MAX_RECORDS,
                "record-count",
            ));
        }
        for (_, record) in records {
            let count = record
                .get("resolution_history")
                .and_then(Value::as_array)
                .map_or(0, <[Value]>::len);
            if count > MAX_RECORDS {
                return Err(exhausted(
                    "known-holds-history-capacity",
                    count,
                    MAX_RECORDS,
                    "per-record predecessor-history",
                ));
            }
        }
        let amount = encoded(value)?.len();
        if amount > MAX_STORE {
            return Err(exhausted(
                "known-holds-byte-capacity",
                amount,
                MAX_STORE,
                "sealed-registry-byte",
            ));
        }
    }
    Ok(value)
}

pub fn trusted_read(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>> {
    if !fs::trust::record(path)?.present {
        return Ok(None);
    }
    // Unlike canonicalizing an input path, this read never follows a swapped link.
    let parent = path
        .parent()
        .ok_or_else(|| err("Invalid known hold record path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Invalid known hold record path"))?;
    let observation =
        fs::read_source(parent, Path::new(name), maximum).map_err(|error| match error {
            fs::ReadError::SizeLimit { .. } => {
                err("Complete private input exceeds its explicit encoded budget; no truncation")
            }
            other => io_error(other),
        })?;
    if observation.identity.uid != host::effective_uid()
        || observation.identity.mode & 0o022 != 0
        || observation.identity.nlink != 1
    {
        return fs::trust::refuse(
            path,
            "lifecycle records must be current-user-owned, regular, single-link and not group/other writable",
        );
    }
    Ok(Some(observation.bytes))
}
pub fn anchor_value(raw: &[u8], root: &Path, project_id: &str) -> Result<Value> {
    let value = load_with_limit(raw, MAX_MARKER)?;
    if !fields(
        &value,
        &["schema", "repo_root", "project_id", "checksum"],
        &[],
    ) || get(&value, "schema").as_str() != Some("agent-memory/known-holds-anchor-v1")
        || get(&value, "repo_root").as_str() != Some(text(root)?)
        || get(&value, "project_id").as_str() != Some(project_id)
        || !checksum_valid(&value)?
    {
        return Err(err(
            "Known hold history anchor changed; preserve it for explicit recovery",
        ));
    }
    Ok(value)
}
pub fn legacy_paths(directory: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let base = directory
        .parent()
        .ok_or_else(|| err("Invalid known hold state path"))?;
    if !fs::trust::state(base)?.present {
        return Ok(BTreeMap::new());
    }
    let chain = DirectoryChain::open(base).map_err(io_error)?;
    let entries = chain
        .directory()
        .entries_bounded(MAX_RECORDS + 64)
        .map_err(io_error)?
        .ok_or_else(|| {
            recovery_needed(
                "known-holds-legacy-capacity",
                "The retained dispute-state namespace exceeds its bounded compatibility inventory.",
            )
        })?;
    let mut paths = BTreeMap::new();
    for name in entries {
        let name = host::text(&name, "known hold legacy filename")?;
        if let Some(key) = name.strip_suffix(".json").filter(|key| sha(&string(*key))) {
            paths.insert(key.to_owned(), base.join(name));
        }
    }
    chain.recheck().map_err(io_error)?;
    Ok(paths)
}
fn empty_store(root: &Path, project_id: &str, records: Value) -> Result<Value> {
    seal(&object([
        ("schema", string(SCHEMA)),
        ("repo_root", string(text(root)?)),
        ("project_id", string(project_id)),
        ("records", records),
    ]))
}
pub fn import_legacy(
    root: &Path,
    project_id: &str,
    directory: &Path,
    existing: &Value,
) -> Result<Value> {
    let mut records = existing.clone();
    pairs(&records)?;
    let mut total = 0;
    for (key, path) in legacy_paths(directory)? {
        if records.get(&key).is_some() {
            continue;
        }
        let raw = trusted_read(&path, MAX_LEGACY)?.ok_or_else(|| err("Legacy dispute evidence is changed or belongs to another project; preserve it for inspection"))?;
        total += raw.len();
        if total > MAX_STORE {
            return Err(err(
                "Legacy dispute evidence exceeds its bounded migration budget; preserve it",
            ));
        }
        let mut value = load_with_limit(&raw, MAX_LEGACY)?;
        if !fields(
            &value,
            &[
                "schema",
                "repo_root",
                "project_id",
                "binding",
                "cause",
                "references",
                "dependencies",
                "judgment_sha256",
                "proof_sha256",
                "checksum",
            ],
            &[],
        ) || get(&value, "schema").as_str() != Some("agent-memory/known-semantic-hold-v1")
            || get(&value, "repo_root").as_str() != Some(text(root)?)
            || get(&value, "project_id").as_str() != Some(project_id)
            || !checksum_valid(&value)?
        {
            return Err(err(
                "Legacy dispute evidence is changed or belongs to another project; preserve it for inspection",
            ));
        }
        for name in ["schema", "repo_root", "project_id", "checksum"] {
            remove(&mut value, name);
        }
        set(&mut value, "resolution", Value::Null)?;
        set(&mut records, &key, value)?;
    }
    let desired = empty_store(root, project_id, records)?;
    registry_value(&desired, root, project_id, false)?;
    Ok(desired)
}

#[derive(Default, Clone, Copy)]
pub struct ReadOptions<'a> {
    pub owner: Option<&'a [u8]>,
    pub allow_journal: bool,
    pub allow_legacy: bool,
}
pub fn read_store(
    root: &Path,
    project_id: &str,
    runtime_root: &Path,
    options: ReadOptions<'_>,
) -> Result<(Value, Option<String>)> {
    read_store_at(
        &locations(root, project_id, runtime_root)?,
        project_id,
        options,
    )
}
pub fn read_store_at(
    paths: &Locations,
    project_id: &str,
    options: ReadOptions<'_>,
) -> Result<(Value, Option<String>)> {
    fs::trust::state(&paths.directory)?;
    let lock = trusted_read(&paths.directory.join("lock"), MAX_MARKER)?;
    if lock
        .as_deref()
        .is_some_and(|raw| Some(raw) != options.owner)
    {
        return Err(ContractError::hold_failure(
            "known-holds-busy",
            "A known-dispute writer is active or its ownership has not been cleared after interruption.",
            "Allow an active review to finish. If interrupted, use am semantic recover-holds with the same --repo and verified --project-id; preserve unknown lock evidence and do not remove it manually.",
        ));
    }
    let journal = present(&paths.directory.join("journal.json"))?;
    if journal && !options.allow_journal {
        return Err(recovery_needed(
            "known-holds-interrupted",
            "A known-dispute transaction has an unfinished recovery journal; its claims cannot be treated as resolved.",
        ));
    }
    let raw = trusted_read(&paths.directory.join("registry.json"), MAX_STORE)?;
    let backup = trusted_read(&paths.directory.join("recovery.json"), MAX_STORE)?;
    let anchored = trusted_read(&paths.anchor, MAX_MARKER)?;
    if let Some(raw) = &anchored {
        anchor_value(raw, &paths.root, project_id)?;
    }
    let Some(raw) = raw else {
        if anchored.is_some() || backup.is_some() || journal {
            return Err(recovery_needed(
                "known-holds-registry-missing",
                "The known-dispute registry is missing, but its anchor or retained recovery evidence proves prior review state existed.",
            ));
        }
        let value = empty_store(&paths.root, project_id, Value::Object(Vec::new()))?;
        if !options.allow_legacy && !legacy_paths(&paths.directory)?.is_empty() {
            return Err(recovery_needed(
                "known-holds-legacy-state",
                "Earlier per-cause dispute evidence exists; absence of the new registry does not establish an empty history.",
            ));
        }
        return Ok((value, None));
    };
    let value = load_with_limit(&raw, MAX_STORE)?;
    registry_value(&value, &paths.root, project_id, false)?;
    if anchored.is_none() || backup.as_deref() != Some(raw.as_slice()) {
        return Err(recovery_needed(
            "known-holds-recovery-incomplete",
            "The known-dispute registry and its recovery anchor/copy do not form one complete verified state.",
        ));
    }
    if !options.allow_legacy
        && legacy_paths(&paths.directory)?
            .keys()
            .any(|key| get(&value, "records").get(key).is_none())
    {
        return Err(recovery_needed(
            "known-holds-legacy-state",
            "Additional earlier per-cause dispute evidence has not yet been imported into the anchored registry.",
        ));
    }
    Ok((value, Some(sha256_hex(&raw))))
}
fn present(path: &Path) -> Result<bool> {
    Ok(fs::trust::inspect(path, fs::trust::InspectOptions::default())?.present)
}
pub fn filtered_records(value: &Value, skip_judgment: Option<&str>) -> Result<Vec<Value>> {
    let mut entries: Vec<_> = pairs(get(value, "records"))?.iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut records = Vec::new();
    for (_, record) in entries {
        if get(record, "judgment_sha256")
            .as_str()
            .is_some_and(|s| Some(s) == skip_judgment)
        {
            continue;
        }
        let mut current = record.clone();
        let resolution = get(&current, "resolution");
        if crate::sharing_limits::truthy(resolution)
            && get(resolution, "judgment_sha256").as_str() == skip_judgment
        {
            let mut history = current
                .get("resolution_history")
                .and_then(Value::as_array)
                .unwrap_or(&[])
                .to_vec();
            let previous = history.pop().unwrap_or(Value::Null);
            set(&mut current, "resolution", previous)?;
            if history.is_empty() {
                remove(&mut current, "resolution_history");
            } else {
                set(&mut current, "resolution_history", Value::Array(history))?;
            }
        }
        records.push(current);
    }
    Ok(records)
}
pub fn load(
    root: &Path,
    project_id: &str,
    runtime_root: &Path,
    skip_judgment: Option<&str>,
) -> Result<Vec<Value>> {
    filtered_records(
        &read_store(root, project_id, runtime_root, ReadOptions::default())?.0,
        skip_judgment,
    )
}
pub fn prepare_directory(directory: &Path) -> Result<()> {
    let parent = directory
        .parent()
        .ok_or_else(|| err("Invalid known hold state path"))?;
    for path in [parent, directory] {
        fs::trust::inspect(path, fs::trust::InspectOptions::default())?;
        storage::private_mkdir(path)?;
        fs::trust::state(path)?;
    }
    Ok(())
}
fn anchor(root: &Path, project_id: &str) -> Result<Vec<u8>> {
    encoded(&seal(&object([
        ("schema", string("agent-memory/known-holds-anchor-v1")),
        ("repo_root", string(text(root)?)),
        ("project_id", string(project_id)),
    ]))?)
}
fn ensure_anchor(paths: &Locations, project_id: &str) -> Result<()> {
    if let Some(previous) = trusted_read(&paths.anchor, MAX_MARKER)? {
        anchor_value(&previous, &paths.root, project_id)?;
    } else {
        storage::replace_with_limit(
            &paths.anchor,
            &anchor(&paths.root, project_id)?,
            None,
            MAX_MARKER,
        )?;
    }
    Ok(())
}
fn unlink_journal(directory: &Path) -> Result<()> {
    let chain = DirectoryChain::open(directory).map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    chain
        .directory()
        .unlink_at(std::ffi::OsStr::new("journal.json"))
        .map_err(io_error)?;
    chain.directory().fsync_directory().map_err(io_error)?;
    chain.recheck().map_err(io_error)
}
pub fn commit_store(
    paths: &Locations,
    project_id: &str,
    desired: &Value,
    observed_sha: Option<&str>,
) -> Result<()> {
    registry_value(desired, &paths.root, project_id, false)?;
    let raw = encoded(desired)?;
    let journal = seal(&object([
        ("schema", string("agent-memory/known-holds-journal-v1")),
        ("repo_root", string(text(&paths.root)?)),
        ("project_id", string(project_id)),
        (
            "observed_sha256",
            observed_sha.map(string).unwrap_or(Value::Null),
        ),
        ("desired", desired.clone()),
        ("desired_sha256", string(sha256_hex(&raw))),
    ]))?;
    let jraw = encoded(&journal)?;
    if jraw.len() > MAX_JOURNAL {
        return Err(err(
            "Complete known-hold recovery journal exceeds its finite budget",
        ));
    }
    let target = paths.directory.join("journal.json");
    if present(&target)? {
        return Err(err(
            "Known-hold recovery is pending; preserve the original journal",
        ));
    }
    storage::replace_with_limit(&target, &jraw, None, MAX_JOURNAL)?;
    writes::fault("holds-after-journal")?;
    storage::replace_with_limit(
        &paths.directory.join("registry.json"),
        &raw,
        observed_sha,
        MAX_STORE,
    )?;
    writes::fault("holds-after-registry")?;
    let backup = trusted_read(&paths.directory.join("recovery.json"), MAX_STORE)?;
    let backup_sha = backup.as_deref().map(sha256_hex);
    storage::replace_with_limit(
        &paths.directory.join("recovery.json"),
        &raw,
        backup_sha.as_deref(),
        MAX_STORE,
    )?;
    ensure_anchor(paths, project_id)?;
    writes::fault("holds-before-journal-cleanup")?;
    if trusted_read(&target, MAX_JOURNAL)?.as_deref() != Some(jraw.as_slice())
        || trusted_read(&paths.directory.join("registry.json"), MAX_STORE)?.as_deref()
            != Some(raw.as_slice())
        || trusted_read(&paths.directory.join("recovery.json"), MAX_STORE)?.as_deref()
            != Some(raw.as_slice())
    {
        return Err(err(
            "Known-hold transaction changed; preserve its recovery journal",
        ));
    }
    unlink_journal(&paths.directory)
}

pub fn bank_owner<T>(
    root: &Path,
    operation_id: &str,
    owned_lock: Option<&[u8]>,
    body: impl FnOnce(&[u8]) -> Result<T>,
) -> Result<T> {
    let (_, bank) = writes::bank_path(root)?;
    if let Some(owner) = owned_lock {
        let chain = DirectoryChain::open(&bank).map_err(io_error)?;
        let linked = chain
            .directory()
            .stat_at(std::ffi::OsStr::new(".agent-memory.lock"))
            .map_err(io_error)?
            .is_symlink();
        chain.recheck().map_err(io_error)?;
        if linked {
            return Err(err(
                "Actual bank ownership is required for known-hold state changes",
            ));
        }
        if trusted_read(&bank.join(".agent-memory.lock"), MAX_MARKER)?.as_deref() != Some(owner) {
            return Err(err(
                "Actual bank ownership is required for known-hold state changes",
            ));
        }
        return body(owner);
    }
    let mut barrier =
        crate::turn_sources::canonical_writer_barrier(root, None, &crate::cli::pack_root()?)?;
    let outcome = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        operation_id,
        "semantic-review-holds",
        Sources::default(),
        body,
    );
    match outcome {
        Ok(value) => {
            barrier.finish()?;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}
pub type Verify<'a> = dyn Fn(&[u8]) -> Result<()> + 'a;
#[derive(Default)]
pub struct RecordOptions<'a> {
    pub resolutions: Option<&'a Value>,
    pub owned_lock: Option<&'a [u8]>,
    pub verify: Option<&'a Verify<'a>>,
    pub expected_inventory: Option<&'a str>,
}
fn operation_id(judgment_sha: &str) -> Result<String> {
    let mut chars: Vec<_> = judgment_sha.chars().take(32).collect();
    if chars.len() != 32 || !chars.iter().all(|c| c.is_ascii_hexdigit()) {
        return Err(err("Invalid known hold judgment digest"));
    }
    // Positions are present because length and ASCII hexadecimal were checked above.
    chars[12] = '4';
    let variant = chars
        .get(16)
        .and_then(|c| c.to_digit(16))
        .ok_or_else(|| err("Invalid known hold judgment digest"))?;
    // Position is present because length was checked above.
    chars[16] = char::from_digit((variant & 3) | 8, 16)
        .ok_or_else(|| err("Invalid known hold judgment digest"))?;
    let mut result = String::with_capacity(36);
    for (i, c) in chars.into_iter().enumerate() {
        if [8, 12, 16, 20].contains(&i) {
            result.push('-');
        }
        result.push(c);
    }
    Ok(result)
}
pub fn record(
    root: &Path,
    project_id: &str,
    runtime_root: &Path,
    proof: &Value,
    references: &Value,
    options: RecordOptions<'_>,
) -> Result<()> {
    record_with_environment(
        root,
        project_id,
        runtime_root,
        proof,
        references,
        options,
        &StateEnvironment::current(),
    )
}
pub fn record_with_environment(
    root: &Path,
    project_id: &str,
    runtime_root: &Path,
    proof: &Value,
    references: &Value,
    options: RecordOptions<'_>,
    environment: &StateEnvironment,
) -> Result<()> {
    let proof_sha = sha256_hex(&crate::sharing_limits::private_bytes(proof)?);
    let causes = proof
        .get("held_causes")
        .map(rows)
        .transpose()?
        .unwrap_or(&[]);
    let empty = Value::Object(Vec::new());
    let resolutions = options
        .resolutions
        .filter(|v| crate::sharing_limits::truthy(v))
        .unwrap_or(&empty);
    let paths = locations_with_environment(root, project_id, runtime_root, environment)?;
    prepare_directory(&paths.directory)?;
    let judgment = proof
        .get("judgment")
        .ok_or_else(|| err("Known hold judgment is missing"))?;
    let judgment_sha = sha256_hex(&encoded(judgment)?);
    let operation = operation_id(&judgment_sha)?;
    bank_owner(&paths.root, &operation, options.owned_lock, |bank_token| {
        if let Some(verify) = options.verify {
            verify(bank_token)?;
        }
        storage::exclusive(
            &paths.directory.join("lock"),
            &operation,
            "semantic-holds",
            Sources::default(),
            |owner| {
                let (current, observed) = read_store_at(
                    &paths,
                    project_id,
                    ReadOptions {
                        owner: Some(owner),
                        ..ReadOptions::default()
                    },
                )?;
                if let Some(expected) = options.expected_inventory {
                    if sha256_hex(&encoded(&Value::Array(filtered_records(
                        &current,
                        Some(&judgment_sha),
                    )?))?)
                        != expected
                    {
                        return Err(err(
                            "Known dispute inventory changed; repeat the actual current review",
                        ));
                    }
                }
                let mut records = get(&current, "records").clone();
                let binding = get(judgment, "binding");
                for cause in causes {
                    let key = cause_id(cause)?;
                    if records.get(&key).is_some() {
                        continue;
                    }
                    let mut selected = Vec::new();
                    for source in rows(get(cause, "sources"))? {
                        let name = source.as_str().ok_or_else(|| {
                            err("Known hold references are incomplete; no resolution is inferred")
                        })?;
                        selected.push((name.to_owned(), references.get(name).ok_or_else(|| err("Known hold references are incomplete; no resolution is inferred"))?.clone()));
                    }
                    let mut bound = Vec::new();
                    for name in ["bank_root", "base", "authority_head", "scope_context"] {
                        bound.push((
                            name.into(),
                            binding
                                .get(name)
                                .ok_or_else(|| err("Known hold binding is incomplete"))?
                                .clone(),
                        ));
                    }
                    let dependencies = judgment
                        .get("dependencies")
                        .ok_or_else(|| err("Known hold dependencies are missing"))?;
                    set(
                        &mut records,
                        &key,
                        object([
                            ("binding", Value::Object(bound)),
                            ("cause", cause.clone()),
                            ("references", Value::Object(selected)),
                            ("dependencies", dependencies.clone()),
                            ("judgment_sha256", string(&judgment_sha)),
                            ("proof_sha256", string(&proof_sha)),
                            ("resolution", Value::Null),
                        ]),
                    )?;
                }
                for (key, resolution) in pairs(resolutions)? {
                    let record = records.get_mut(key).ok_or_else(|| {
                        err("Resolution must retain its actual first-detection evidence")
                    })?;
                    let mut renewed = resolution.clone();
                    set(&mut renewed, "judgment_sha256", string(&judgment_sha))?;
                    set(&mut renewed, "proof_sha256", string(&proof_sha))?;
                    let previous = get(record, "resolution");
                    if crate::sharing_limits::truthy(previous)
                        && get(previous, "judgment_sha256").as_str() == Some(judgment_sha.as_str())
                    {
                        if !equivalent(previous, &renewed)? {
                            return Err(err(
                                "The same resolution judgment cannot replace different proof evidence",
                            ));
                        }
                    } else if crate::sharing_limits::truthy(previous) {
                        let mut history = record
                            .get("resolution_history")
                            .map(rows)
                            .transpose()?
                            .unwrap_or(&[])
                            .to_vec();
                        history.push(previous.clone());
                        set(record, "resolution_history", Value::Array(history))?;
                    }
                    set(record, "resolution", renewed)?;
                }
                let desired = empty_store(&paths.root, project_id, records)?;
                registry_value(&desired, &paths.root, project_id, true)?;
                if !equivalent(&desired, &current)? {
                    commit_store(&paths, project_id, &desired, observed.as_deref())?;
                }
                Ok(())
            },
        )?;
        if let Some(verify) = options.verify {
            verify(bank_token)?;
        }
        Ok(())
    })
}

fn lock_recovery(
    registry: &Path,
    lock: &Path,
    operations: &[&str],
    prepared: Option<&Value>,
) -> Result<Value> {
    storage::recover(
        registry,
        lock,
        operations,
        prepared,
        RECOVERY_SCHEMA,
        Sources::default(),
    )
}
/// Replay a verified journal or restore the exact retained backup. Called under
/// both canonical bank ownership and semantic-holds ownership.
pub fn recover_store(paths: &Locations, project_id: &str, owner: &[u8]) -> Result<()> {
    let target = paths.directory.join("journal.json");
    let jraw = trusted_read(&target, MAX_JOURNAL)?;
    let registry = paths.directory.join("registry.json");
    if let Some(jraw) = jraw {
        let journal = load_with_limit(&jraw, MAX_JOURNAL)?;
        if !fields(
            &journal,
            &[
                "schema",
                "repo_root",
                "project_id",
                "observed_sha256",
                "desired",
                "desired_sha256",
                "checksum",
            ],
            &[],
        ) || get(&journal, "schema").as_str() != Some("agent-memory/known-holds-journal-v1")
            || get(&journal, "repo_root").as_str() != Some(text(&paths.root)?)
            || get(&journal, "project_id").as_str() != Some(project_id)
            || !checksum_valid(&journal)?
        {
            return Err(err(
                "Unknown hold journal; preserve it for explicit inspection",
            ));
        }
        let desired = registry_value(get(&journal, "desired"), &paths.root, project_id, false)?;
        let raw = encoded(desired)?;
        if digest(Some(&raw)) != *get(&journal, "desired_sha256") {
            return Err(err("Changed hold recovery content"));
        }
        let current = trusted_read(&registry, MAX_STORE)?;
        let current_sha = digest(current.as_deref());
        if current_sha != *get(&journal, "observed_sha256")
            && current_sha != *get(&journal, "desired_sha256")
        {
            return Err(err(
                "Later hold evidence is preserved; do not replay over it",
            ));
        }
        if current.as_deref() != Some(raw.as_slice()) {
            storage::replace_with_limit(&registry, &raw, current_sha.as_str(), MAX_STORE)?;
        }
        let backup = trusted_read(&paths.directory.join("recovery.json"), MAX_STORE)?;
        let backup_sha = digest(backup.as_deref());
        storage::replace_with_limit(
            &paths.directory.join("recovery.json"),
            &raw,
            backup_sha.as_str(),
            MAX_STORE,
        )?;
        ensure_anchor(paths, project_id)?;
        if trusted_read(&target, MAX_JOURNAL)?.as_deref() != Some(jraw.as_slice()) {
            return Err(err("Hold journal changed during recovery"));
        }
        unlink_journal(&paths.directory)?;
    } else if !present(&registry)? {
        let backup = trusted_read(&paths.directory.join("recovery.json"), MAX_STORE)?;
        let anchored = trusted_read(&paths.anchor, MAX_MARKER)?;
        match (backup, anchored) {
            (Some(backup), Some(anchored)) => {
                anchor_value(&anchored, &paths.root, project_id)?;
                registry_value(
                    &load_with_limit(&backup, MAX_STORE)?,
                    &paths.root,
                    project_id,
                    false,
                )?;
                storage::replace_with_limit(&registry, &backup, None, MAX_STORE)?;
            }
            (None, None) => (),
            _ => {
                return Err(err(
                    "Missing hold evidence cannot be treated as resolution; inspect canonical/source evidence",
                ));
            }
        }
    }
    let options = ReadOptions {
        owner: Some(owner),
        allow_legacy: true,
        ..ReadOptions::default()
    };
    let (current, observed) = read_store_at(paths, project_id, options)?;
    let desired = import_legacy(
        &paths.root,
        project_id,
        &paths.directory,
        get(&current, "records"),
    )?;
    if !equivalent(&desired, &current)? {
        commit_store(paths, project_id, &desired, observed.as_deref())?;
    }
    read_store_at(
        paths,
        project_id,
        ReadOptions {
            owner: Some(owner),
            ..ReadOptions::default()
        },
    )?;
    Ok(())
}
pub fn recover(root: &Path, project_id: &str, runtime_root: &Path) -> Result<Value> {
    recover_with_environment(root, project_id, runtime_root, &StateEnvironment::current())
}
pub fn recover_with_environment(
    root: &Path,
    project_id: &str,
    runtime_root: &Path,
    environment: &StateEnvironment,
) -> Result<Value> {
    let paths = locations_with_environment(root, project_id, runtime_root, environment)?;
    let bank_lock = paths.bank.join(".agent-memory.lock");
    let stale = if present(&bank_lock)? {
        lock_recovery(
            &paths.bank.join("MEMORY.md"),
            &bank_lock,
            &["semantic-review-holds"],
            None,
        )?;
        trusted_read(&bank_lock, MAX_MARKER)?
    } else {
        None
    };
    // Scope is established before mkdir or clearing either ownership marker.
    let expected = crate::scope_binding::require(
        &paths.root,
        Some(project_id),
        crate::scope_binding::RequireOptions {
            owned_lock: stale.as_deref(),
            ..Default::default()
        },
    )?;
    prepare_directory(&paths.directory)?;
    for (registry, lock, operations) in [
        (
            paths.bank.join("MEMORY.md"),
            bank_lock,
            "semantic-review-holds",
        ),
        (
            paths.directory.join("registry.json"),
            paths.directory.join("lock"),
            "semantic-holds",
        ),
    ] {
        if present(&lock)? {
            trusted_read(&lock, MAX_MARKER)?;
            let preview = lock_recovery(&registry, &lock, &[operations], None)?;
            lock_recovery(&registry, &lock, &[operations], Some(&preview))?;
        }
    }
    let operation = host::uuid4()?;
    bank_owner(&paths.root, &operation, None, |bank_token| {
        crate::scope_binding::require(
            &paths.root,
            Some(project_id),
            crate::scope_binding::RequireOptions {
                expected: Some(&expected),
                owned_lock: Some(bank_token),
                ..Default::default()
            },
        )?;
        storage::exclusive(
            &paths.directory.join("lock"),
            &operation,
            "semantic-holds",
            Sources::default(),
            |owner| recover_store(&paths, project_id, owner),
        )
    })?;
    Ok(object([
        ("schema", string("agent-memory/known-holds-recovery-v1")),
        ("verified", Value::Bool(true)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("private_state_writes_performed", Value::Bool(true)),
        ("source_observation_current", Value::Bool(false)),
        ("requires_fresh_review", Value::Bool(true)),
    ]))
}
