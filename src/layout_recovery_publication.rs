//! Private, source-preserving publication proofs for invalid authority layouts.
//!
//! This deliberately does not use semantic-sharing-v2 as a proof of an invalid tree.
//! The existing publication policy/PR/check machinery remains the landing authority.
use crate::bank_contract as bank;
use crate::bank_transactions as tx;
use crate::bank_writes as writes;
use crate::codec::json::Value;
use crate::developer_identity::{encoded, uuid_valid};
use crate::error::{ContractError, Result};
use crate::layout_recovery_observation::{self as raw_sources, InventoryOptions};
use crate::layout_recovery_state as state;
use crate::lifecycle::{get, object, string};
use crate::memory_publication::{self as publication, GitWrite, TemporaryDirectory, same};
use crate::process::{self, FileSize, Input, Spec, Status, Target};
use crate::scope_binding::{self, RequireOptions};
use crate::sharing_limits::{self as limits, truthy};
use crate::sharing_recovery as git;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

pub const MAX_FILE: usize = raw_sources::MAX_FILE;
pub const MAX_BYTES: usize = raw_sources::MAX_BYTES;
pub const MAX_FILES: usize = raw_sources::MAX_FILES;
pub const MAX_ENTRIES: usize = raw_sources::MAX_ENTRIES;
pub const MAX_DEPTH: usize = raw_sources::MAX_DEPTH;
pub const OBSERVATION: &str = "agent-memory/layout-publication-observation-v1";
pub const REVIEW: &str = "agent-memory/layout-publication-review-v1";
pub const REQUEST: &str = "agent-memory/layout-publication-request-v1";
pub const TERMINAL: &[&str] = &[
    "admitted",
    "governed-terminal",
    "reviewed-non-guidance-discard",
];
const TYPED: &[&str] = &["standalone-record", "tombstone", "optional-history-event"];
/// Release binary inventory bound (the runtime artifact includes `bin/am`).
const MAX_RUNTIME_BINARY: usize = crate::release_distribution::MAX_BINARY;

fn err(message: impl Into<String>) -> ContractError {
    ContractError::new(message)
}
pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition { Ok(()) } else { Err(err(message)) }
}
fn fields(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    object(pairs)
}
fn as_text<'a>(value: &'a Value, message: &str) -> Result<&'a str> {
    value.as_str().ok_or_else(|| err(message))
}
fn rows<'a>(value: &'a Value, message: &str) -> Result<&'a [Value]> {
    value.as_array().ok_or_else(|| err(message))
}
fn path_text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "recovery publication path")
}
fn sha(raw: &[u8]) -> String {
    crate::validation_memo::hex(&Sha256::digest(raw))
}
fn value_digest(value: &Value) -> Result<String> {
    Ok(sha(&encoded(value)?))
}
fn read(path: &Path) -> Result<Value> {
    state::read(path, limits::MAX_PRIVATE, false)?
        .ok_or_else(|| err("Recovery evidence is missing; retain the operation"))
}
fn read_bytes(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    state::read_bytes(path, maximum, false)?
        .ok_or_else(|| err("Recovery evidence is missing; retain the operation"))
}
fn int_is(value: &Value, expected: usize) -> bool {
    value
        .as_int()
        .and_then(|n| n.as_u64())
        .is_some_and(|n| n as u128 == expected as u128)
}
fn int(value: &Value) -> Option<i128> {
    match value {
        Value::Int(n) => n.as_i128(),
        _ => None,
    }
}
fn number(value: usize) -> Value {
    Value::Int((value as u64).into())
}
fn capacity(dimension: &str, limit: usize, measured: usize, exact: bool) -> ContractError {
    ContractError::raw_recovery_capacity(dimension, limit, measured as u64, exact)
}
/// The exact typed private recovery publication artifact shape.
pub fn exact<'a>(value: &'a Value, keys: &[&str], schema: Option<&str>) -> Result<&'a Value> {
    let shaped = value.as_object().is_some_and(|pairs| {
        let names: BTreeSet<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        let wanted: BTreeSet<&str> = keys.iter().copied().collect();
        names == wanted
    }) && schema.is_none_or(|schema| get(value, "schema").as_str() == Some(schema));
    require(
        shaped,
        "Use the exact typed private recovery publication artifact",
    )?;
    Ok(value)
}

/// A bounded explicit source-preservation judgment (4096 encoded bytes by default).
pub fn text(value: &Value, maximum: usize) -> Result<&str> {
    let accepted = value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= maximum);
    accepted.ok_or_else(|| err("A bounded explicit source-preservation judgment is required"))
}
fn judgment(value: &Value) -> Result<&str> {
    text(value, 4096)
}

/// `agent-memory/layout-recovery-result-v1`; `values` override base fields in place.
pub fn result(operation: &str, operation_id: &str, status: &str, values: Value) -> Result<Value> {
    let mut base = fields([
        ("schema", string("agent-memory/layout-recovery-result-v1")),
        ("operation", string(operation)),
        ("operation_id", string(operation_id)),
        ("state", string(status)),
        ("publication_verified", Value::Bool(false)),
        ("verified", Value::Bool(!status.starts_with("pending"))),
        ("canonical_eligibility", string("unavailable")),
        ("semantic_recovery_complete", Value::Bool(false)),
        ("pending_unit_ids", Value::Array(vec![])),
        ("pending_units", number(0)),
        ("required_actions", Value::Array(vec![])),
        ("head_writes_performed", Value::Bool(false)),
        ("feature_writes_performed", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("original_index_writes_performed", Value::Bool(false)),
        ("history_writes_performed", Value::Bool(false)),
        ("private_artifact_writes_performed", Value::Bool(false)),
        ("feature_code_freshness_verified", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
        ("other_machine_import_verified", Value::Bool(false)),
    ]);
    bank::merge(&mut base, &values)?;
    Ok(base)
}

/// Checked terminal recovery evidence (`root`, `bank`, `base`, operation directory, terminal).
pub struct Evidence {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub base: PathBuf,
    pub directory: PathBuf,
    pub terminal: Value,
}

/// Check the real terminal ledger, not a historical layout-completion boolean.
pub fn operation_evidence(
    repo: &Path,
    operation_id: &str,
    owner: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<Evidence> {
    require(
        uuid_valid(&string(operation_id)),
        "Use the original recovery operation UUID",
    )?;
    let (root, bank_path, base, directory) = state::paths(repo, Some(operation_id), runtime_root)?;
    let directory = directory.ok_or_else(|| err("Use the original recovery operation UUID"))?;
    let frontier = writes::recovery_frontier_provider()?;
    let token = frontier.observe_owned(&root, owner, None)?;
    require(
        get(&token, "coverage_complete") == &Value::Bool(true) && !truthy(get(&token, "unit_ids")),
        "Publication/retention is pending while any recovery source remains unresolved",
    )?;
    let index = state::index(&root, runtime_root)?;
    let row = get(get(&index, "operations"), operation_id);
    require(
        get(&index, "operations").get(operation_id).is_some(),
        "Unknown recovery operation; retain all evidence",
    )?;
    let receipt = read(&directory.join("receipt.json"))?;
    require(
        get(&receipt, "schema").as_str() == Some("agent-memory/layout-recovery-receipt-v1")
            && get(&receipt, "operation_id").as_str() == Some(operation_id)
            && get(&receipt, "verified") == &Value::Bool(true)
            && matches!(
                get(&receipt, "outcome").as_str(),
                Some("layout-restored" | "rolled-back")
            ),
        "Recovery needs its immutable, completed layout receipt",
    )?;
    let (directory, head, ledger) =
        frontier.ledger(&root, operation_id, row, &bank_path, runtime_root)?;
    let (manifest, proposal) = frontier.evidence(&directory, &ledger, row, runtime_root)?;
    require(
        get(&receipt, "request_sha256").as_str() == Some(value_digest(&proposal)?.as_str())
            && get(&receipt, "proposal_id") == get(&proposal, "proposal_id")
            && proposal.get("proposal_id").is_some(),
        "Layout receipt is not the original exact reviewed operation",
    )?;
    for source in rows(
        get(&manifest, "sources"),
        "Invalid retained original source identity",
    )? {
        let source_id = get(source, "source_id");
        require(
            uuid_valid(source_id),
            "Invalid retained original source identity",
        )?;
        let name = format!(
            "{}.bin",
            as_text(source_id, "Invalid retained original source identity")?
        );
        let raw = read_bytes(&directory.join("originals").join(name), MAX_FILE)?;
        require(
            int_is(get(source, "byte_count"), raw.len())
                && get(source, "sha256").as_str() == Some(sha(&raw).as_str()),
            "Missing or damaged original backup; publication/retention remains pending",
        )?;
    }
    let units: BTreeMap<String, &Value> = rows(
        get(&ledger, "units"),
        "A recovery unit lacks its actual reviewed terminal disposition",
    )?
    .iter()
    .map(|unit| (get(unit, "unit_id").as_str().unwrap_or("").to_owned(), unit))
    .collect();
    let terminal_state = |unit: &Value| {
        get(unit, "state")
            .as_str()
            .is_some_and(|s| TERMINAL.contains(&s))
    };
    for unit in units.values() {
        if get(unit, "state").as_str() == Some("superseded-by-revision") {
            let children = get(unit, "child_unit_ids").as_array();
            let complete = children.is_some_and(|children| {
                !children.is_empty()
                    && children.iter().all(|child| {
                        child
                            .as_str()
                            .and_then(|id| units.get(id))
                            .is_some_and(|child| terminal_state(child))
                    })
            });
            require(complete, "A revised recovery unit is not terminal")?;
        } else {
            require(
                terminal_state(unit) && truthy(get(unit, "terminal_receipts")),
                "A recovery unit lacks its actual reviewed terminal disposition",
            )?;
        }
    }
    require(
        get(&ledger, "project_id") == get(&token, "project_id")
            && get(&token, "project_id") == get(&receipt, "project_id"),
        "Recovery evidence belongs to another project",
    )?;
    let terminal = fields([
        ("frontier_token", token),
        ("ledger_head", head),
        ("receipt_sha256", string(value_digest(&receipt)?)),
        ("index_sha256", string(value_digest(&index)?)),
        ("project_id", get(&ledger, "project_id").clone()),
    ]);
    Ok(Evidence {
        root,
        bank: bank_path,
        base,
        directory,
        terminal,
    })
}

fn checked_path(path: &str) -> Result<&str> {
    raw_sources::path_value(path)?;
    let depth = path.split('/').count();
    if depth > MAX_DEPTH {
        return Err(capacity("depth", MAX_DEPTH, depth, true));
    }
    Ok(path)
}

/// One verified raw blob: path, object ID and exact bytes.
type RawObject = (String, String, Vec<u8>);

/// Two bounded Git children; verify every source's framing and object identity.
fn objects(
    repo: &Path,
    wanted: &[(String, String)],
    runtime_root: &Path,
) -> Result<Vec<RawObject>> {
    if wanted.is_empty() {
        return Ok(vec![]);
    }
    let mut payload = wanted
        .iter()
        .map(|(_, oid)| oid.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    payload.push(b'\n');
    let metadata = git::raw_command(
        repo,
        &["cat-file", "--batch-check"],
        false,
        wanted.len().saturating_mul(128),
        false,
        Some(&payload),
    )?;
    let metadata = std::str::from_utf8(&metadata)
        .ok()
        .filter(|m| m.is_ascii())
        .ok_or_else(|| err("Incomplete raw object size inventory"))?;
    let lines: Vec<&str> = metadata.lines().collect();
    require(
        lines.len() == wanted.len(),
        "Incomplete raw object size inventory",
    )?;
    let mut sizes = Vec::with_capacity(wanted.len());
    let mut total = 0usize;
    for ((_, oid), row) in wanted.iter().zip(&lines) {
        let parts: Vec<&str> = row.split_whitespace().collect();
        let [listed, kind, size] = parts.as_slice() else {
            return Err(err("Raw object size inventory substituted a source"));
        };
        require(
            listed == oid && *kind == "blob" && size.bytes().all(|b| b.is_ascii_digit()),
            "Raw object size inventory substituted a source",
        )?;
        let size: usize = size
            .parse()
            .map_err(|_| capacity("source-bytes", MAX_FILE, usize::MAX, true))?;
        total = total.saturating_add(size);
        if size > MAX_FILE {
            return Err(capacity("source-bytes", MAX_FILE, size, true));
        }
        if total > MAX_BYTES {
            return Err(capacity("aggregate-bytes", MAX_BYTES, total, false));
        }
        sizes.push(size);
    }
    let (_, _, base, _) = state::paths(repo, None, runtime_root)?;
    state::private_parent(&base)?;
    let limit = total + wanted.len() * 128;
    let mut env = publication::environment();
    env.insert("GIT_ALLOW_PROTOCOL".into(), "".into());
    let env: Vec<(OsString, OsString)> = env.into_iter().collect();
    let mut argv: Vec<OsString> = [
        "git",
        "--no-pager",
        "--no-replace-objects",
        "--literal-pathspecs",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-C",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    argv.push(repo.as_os_str().to_owned());
    argv.extend(["cat-file", "--batch"].map(OsString::from));
    let unavailable = || err("Raw Git object observation unavailable; retain private evidence");
    let null = publication::null_sink().map_err(|_| unavailable())?;
    let deadline =
        crate::clock::Deadline::after(publication::TIMEOUT).map_err(|_| unavailable())?;
    let output = process::run(&Spec {
        argv: &argv,
        search_path: true,
        env: &env,
        cwd: None,
        stdin: Input::Bytes(&payload),
        stdout: Target::Capture { limit },
        stderr: Target::Descriptor(null.as_fd()),
        deadline,
        new_session: false,
        file_size_limit: Some(FileSize::Pinned(limit as u64 + 1)),
        umask: None,
        pass_fds: &[],
    })
    .map_err(|failure| {
        if matches!(failure.reason, process::Refusal::CaptureLimit { .. }) {
            err("Raw object output exceeded its complete bound")
        } else {
            unavailable()
        }
    })?;
    require(
        output.status == Status::Code(0) && output.stdout.len() <= limit,
        "Raw object output exceeded its complete bound",
    )?;
    let spool = output.stdout;
    let mut cursor = 0usize;
    let mut found = Vec::with_capacity(wanted.len());
    for ((path, oid), size) in wanted.iter().zip(sizes) {
        let header = format!("{oid} blob {size}\n");
        let window = spool.get(cursor..).unwrap_or(&[]);
        let line_end = window
            .iter()
            .take(128)
            .position(|b| *b == b'\n')
            .map_or(window.len().min(128), |p| p + 1);
        require(
            window.get(..line_end) == Some(header.as_bytes()),
            "Raw object framing/identity changed",
        )?;
        cursor += line_end;
        let body = spool
            .get(cursor..cursor + size)
            .ok_or_else(|| err("Incomplete raw object source bytes"))?;
        cursor += size;
        let prefix = format!("blob {size}\0");
        let hashed = if oid.len() == 40 {
            let mut hasher = sha1::Sha1::new();
            hasher.update(prefix.as_bytes());
            hasher.update(body);
            crate::validation_memo::hex(&hasher.finalize())
        } else {
            let mut hasher = Sha256::new();
            hasher.update(prefix.as_bytes());
            hasher.update(body);
            crate::validation_memo::hex(&hasher.finalize())
        };
        require(
            spool.get(cursor) == Some(&b'\n') && hashed == *oid,
            "Raw Git source bytes do not match their exact immutable object",
        )?;
        cursor += 1;
        found.push((path.clone(), oid.clone(), body.to_vec()));
    }
    require(
        cursor == spool.len(),
        "Unexpected trailing raw object bytes",
    )?;
    Ok(found)
}

/// Inventory all raw bank objects, including unsupported ordinary filenames.
///
/// Git tree objects are immutable; unlike a working tree they have no Unix
/// ownership metadata. Modes and object identities are the physical facts here.
/// Raw bytes never enter a normal snapshot, stdout or shared proof trailer.
pub fn observe_tree(
    repo: &Path,
    revision: &str,
    prefix: &str,
    directory: Option<&Path>,
    expected: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    git::commit(repo, revision)?;
    let raw = git::raw_command(
        repo,
        &["ls-tree", "-rtz", "--full-tree", revision, "--", prefix],
        false,
        git::MAX_LOCATOR_TREE_BYTES,
        false,
        None,
    )?;
    let mut physical = Vec::new();
    let mut names = BTreeSet::new();
    let mut wanted = Vec::new();
    let prior: BTreeMap<String, &Value> = match expected {
        Some(expected) => rows(
            get(expected, "sources"),
            "Invalid prior raw source inventory",
        )?
        .iter()
        .map(|s| (get(s, "path").as_str().unwrap_or("").to_owned(), s))
        .collect(),
        None => BTreeMap::new(),
    };
    let owned = format!("{prefix}/");
    for item in raw.split(|b| *b == 0) {
        if item.is_empty() {
            continue;
        }
        let tab = item
            .iter()
            .position(|b| *b == b'\t')
            .ok_or_else(|| err("Raw inventory escaped the selected bank"))?;
        let header = std::str::from_utf8(item.get(..tab).unwrap_or(&[]))
            .ok()
            .filter(|h| h.is_ascii())
            .ok_or_else(|| err("Raw inventory escaped the selected bank"))?;
        let full = std::str::from_utf8(item.get(tab + 1..).unwrap_or(&[]))
            .map_err(|_| err("Unsafe raw-source path; preserve the physical bank"))?;
        let parts: Vec<&str> = header.split_whitespace().collect();
        let [mode, kind, oid] = parts.as_slice() else {
            return Err(err("Raw inventory escaped the selected bank"));
        };
        if full == prefix && *kind == "tree" {
            continue;
        }
        let relative = full
            .strip_prefix(&owned)
            .ok_or_else(|| err("Raw inventory escaped the selected bank"))?;
        let path = checked_path(relative)?;
        let alias = bank::portable_key(path);
        require(
            !names.contains(&alias),
            "Raw source aliases require explicit review",
        )?;
        if physical.len() >= MAX_ENTRIES {
            return Err(capacity(
                "physical-entries",
                MAX_ENTRIES,
                physical.len() + 1,
                false,
            ));
        }
        names.insert(alias);
        require(
            matches!((*kind, *mode), ("tree", "040000") | ("blob", "100644")),
            "Redirect, executable, submodule or nonordinary raw bank object requires review",
        )?;
        require(
            !bank::runtime_path(path),
            "A committed recovery/runtime marker is not portable publication evidence",
        )?;
        physical.push(fields([
            ("path", string(path)),
            ("type", string(*kind)),
            ("mode", string(*mode)),
            ("git_oid", string(git::oid(oid)?)),
        ]));
        if *kind == "tree" {
            continue;
        }
        if wanted.len() >= MAX_FILES {
            return Err(capacity("source-files", MAX_FILES, wanted.len() + 1, false));
        }
        wanted.push((path.to_owned(), (*oid).to_owned()));
    }
    let mut sources = Vec::new();
    let mut total = 0usize;
    for (path, oid, body) in objects(repo, &wanted, runtime_root)? {
        total += body.len();
        let source_id = match prior.get(&path) {
            Some(row) => {
                as_text(get(row, "source_id"), "Invalid prior raw source inventory")?.to_owned()
            }
            None => crate::host::uuid4()?,
        };
        let blob = format!("originals/{source_id}.bin");
        if let Some(directory) = directory {
            state::put_bytes(&directory.join(&blob), &body)?;
        }
        sources.push(fields([
            ("source_id", string(source_id)),
            ("path", string(path)),
            ("byte_count", number(body.len())),
            ("sha256", string(sha(&body))),
            ("git_blob", string(oid)),
            ("blob", string(blob)),
        ]));
    }
    let manifest = fields([
        ("schema", string("agent-memory/layout-publication-tree-v1")),
        ("revision", string(revision)),
        ("bank_root", string(prefix)),
        ("observation_complete", Value::Bool(true)),
        ("physical_entries", Value::Array(physical)),
        ("sources", Value::Array(sources)),
        ("source_bytes", number(total)),
    ]);
    if let Some(expected) = expected {
        require(
            same(&manifest, expected)?,
            "Raw authority source inventory changed; obtain a new independent review",
        )?;
    }
    Ok(manifest)
}

fn files_of(observation: &crate::bank_observation::Observation) -> Result<Value> {
    let mut files = Vec::with_capacity(observation.sources.len());
    for (path, raw) in &observation.sources {
        let body =
            std::str::from_utf8(raw).map_err(|_| err("Canonical Markdown source must be UTF-8"))?;
        files.push((path.clone(), string(body)));
    }
    Ok(Value::Object(files))
}

fn raw_body(directory: &Path, source: &Value) -> Result<Vec<u8>> {
    let blob = as_text(
        get(source, "blob"),
        "Missing or damaged raw publication backup; retain the operation",
    )?;
    let raw = read_bytes(&directory.join(blob), MAX_FILE)?;
    require(
        int_is(get(source, "byte_count"), raw.len())
            && get(source, "sha256").as_str() == Some(sha(&raw).as_str()),
        "Missing or damaged raw publication backup; retain the operation",
    )?;
    Ok(raw)
}

fn command(root: &Path, args: &[&str]) -> Result<String> {
    git::command(root, args, false, git::MAX_BYTES, false)
}

fn fetch(root: &Path, remote: &str, branch: &str) -> Result<String> {
    let advertised = git::server(root, remote, branch)?;
    let reference = format!("refs/agent-memory/observations/{}", crate::host::uuid4()?);
    let outcome = (|| {
        let refspec = format!("refs/heads/{branch}:{reference}");
        git::command(
            root,
            &[
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "--refmap=",
                "--no-recurse-submodules",
                "--",
                remote,
                &refspec,
            ],
            true,
            65536,
            false,
        )?;
        require(
            command(root, &["rev-parse", "--verify", &reference])?.trim() == advertised
                && git::server(root, remote, branch)? == advertised,
            "Authority advanced during recovery publication observation",
        )?;
        Ok(advertised.clone())
    })();
    command(root, &["update-ref", "-d", &reference])?;
    outcome
}

/// Keep normal index/ignore rules, proving invalid-base deletions from raw pins.
fn local_boundary(root: &Path, observed: &Value, manifest: &Value) -> Result<String> {
    let prefix = crate::repository_config::relative(root)?;
    let owned = format!("{prefix}/");
    let original: BTreeMap<&str, &Value> =
        rows(get(manifest, "sources"), "Invalid original manifest")?
            .iter()
            .map(|s| (get(s, "path").as_str().unwrap_or(""), s))
            .collect();
    let indexed = git::command(
        root,
        &["ls-files", "--stage", "-z", "--", &prefix],
        false,
        git::MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    for item in indexed.split('\0').filter(|item| !item.is_empty()) {
        let (header, full) = item
            .split_once('\t')
            .ok_or_else(|| err("Unmerged/nonordinary original memory index requires inspection"))?;
        let parts: Vec<&str> = header.split_whitespace().collect();
        let [mode, blob, stage] = parts.as_slice() else {
            return Err(err(
                "Unmerged/nonordinary original memory index requires inspection",
            ));
        };
        require(
            full.starts_with(&owned) && *stage == "0" && *mode == "100644",
            "Unmerged/nonordinary original memory index requires inspection",
        )?;
        let path = full.get(owned.len()..).unwrap_or("");
        if observed.get(path).is_none() {
            let source = original.get(path).filter(|_| !bank::runtime_path(path));
            let source = source.ok_or_else(|| {
                err("Unknown indexed artifact cannot be omitted by recovery publication")
            })?;
            let body = git::raw_command(
                root,
                &["cat-file", "blob", git::oid(blob)?],
                false,
                MAX_FILE,
                false,
                None,
            )?;
            require(
                get(source, "sha256").as_str() == Some(sha(&body).as_str()),
                "Indexed deletion is not the inspected recovery source",
            )?;
        }
    }
    let ignored = git::command(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
            "--",
            &prefix,
        ],
        false,
        git::MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    let leaked = ignored
        .split('\0')
        .filter(|p| !p.is_empty())
        .any(|p| observed.get(p.get(owned.len()..).unwrap_or("")).is_some());
    require(
        !leaked,
        "Ignored canonical memory requires explicit boundary repair",
    )?;
    Ok(sha(indexed.as_bytes()))
}

/// The runtime artifact digest: pack inventory plus the executing `bin/am`.
fn runtime_digest(runtime_root: &Path) -> Result<String> {
    let inventory = crate::pack_validation::inventory(runtime_root)?;
    let files = crate::release_distribution::inventory_value(&inventory);
    let executable = std::env::current_exe()
        .map_err(|_| err("The executing runtime binary cannot be observed"))?;
    let (binary, mode) =
        crate::pack_validation::binary_asset(&executable, "bin/am", MAX_RUNTIME_BINARY)?;
    let (_, digest) = crate::artifact::runtime_digest(&files, &sha(&binary), mode)?;
    Ok(digest)
}

/// One complete private publication observation.
pub struct Observed {
    pub root: PathBuf,
    pub directory: PathBuf,
    pub value: Value,
    pub files: Value,
}

fn observe(
    repo: &Path,
    operation_id: &str,
    observation_id: Option<&str>,
    runtime_root: &Path,
) -> Result<Observed> {
    let evidence = operation_evidence(repo, operation_id, None, runtime_root)?;
    let root = evidence.root.clone();
    let operation = evidence.directory.clone();
    let terminal = evidence.terminal;
    require(
        get(&read(&operation.join("receipt.json"))?, "outcome").as_str() != Some("rolled-back"),
        "A rolled-back invalid layout is not a publication result",
    )?;
    let manifest = read(&operation.join("manifest.json"))?;
    require(
        get(&manifest, "observation_complete") == &Value::Bool(true),
        "Original source observation is incomplete",
    )?;
    let mut local = crate::bank_observation::observe(&root, None, None)?;
    let files = files_of(&local)?;
    require(
        crate::security::complete_refresh_reasons(&files)?.is_empty(),
        "Unsafe/unadmitted publication result requires governed review",
    )?;
    let checked = bank::validate_files(&files, None)?;
    let admitted = rows(get(&checked, "entries"), "Invalid validated entries")?
        .iter()
        .all(|e| {
            !matches!(
                get(e, "lifecycle").as_str(),
                Some("candidate" | "quarantined")
            )
        });
    require(
        get(&checked, "project_id") == get(&terminal, "project_id") && admitted,
        "The complete local result must be admitted, same-project native memory",
    )?;
    let project_id = as_text(
        get(&terminal, "project_id"),
        "Recovery evidence belongs to another project",
    )?;
    let context = scope_binding::require(
        &root,
        Some(project_id),
        RequireOptions {
            expected: Some(get(get(&manifest, "identity"), "scope_context")),
            ..RequireOptions::default()
        },
    )?;
    let project = crate::project_identity::status(&root)?;
    let project = get(&project, "evidence");
    let settings = crate::repository_config::settings(&root)?;
    let remote = get(project, "authority_name")
        .as_str()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| err("Recovery publication requires the existing verified authority"))?
        .to_owned();
    crate::repository_config::safe_label(&remote)?;
    let branch = match settings.authority_branch.clone().filter(|b| !b.is_empty()) {
        Some(branch) => branch,
        None => {
            let reference = format!("refs/remotes/{remote}/HEAD");
            let remote_head = command(&root, &["symbolic-ref", "--quiet", &reference])?;
            let remote_head = remote_head.trim();
            let prefix = format!("refs/remotes/{remote}/");
            remote_head
                .strip_prefix(&prefix)
                .ok_or_else(|| err("Explicit/default authority branch observation is required"))?
                .to_owned()
        }
    };
    crate::repository_config::validate_branch(&branch)?;
    let index = publication::index_bytes(&root)?;
    let head = command(&root, &["rev-parse", "--verify", "HEAD"])?
        .trim()
        .to_owned();
    let authority = fetch(&root, &remote, &branch)?;
    let base = command(&root, &["merge-base", &head, &authority])?
        .trim()
        .to_owned();
    git::commit(&root, &base)?;
    git::authority_bank_location(&root, &authority, Some(&settings.bank_root))?;
    let observation_id = match observation_id {
        Some(id) => id.to_owned(),
        None => crate::host::uuid4()?,
    };
    let directory = operation.join("publication").join(&observation_id);
    let existing = state::read(
        &directory.join("observation.json"),
        limits::MAX_PRIVATE,
        true,
    )?;
    let mut variants = Vec::new();
    for (side, revision) in [("base", &base), ("landed", &authority)] {
        let expected = existing
            .as_ref()
            .map(|existing| get(get(existing, "variants"), side));
        let tree = observe_tree(
            &root,
            revision,
            &settings.bank_root,
            Some(&directory),
            expected,
            runtime_root,
        )?;
        variants.push((side.to_owned(), tree));
    }
    let landed_empty = variants
        .iter()
        .find(|(side, _)| side == "landed")
        .is_some_and(|(_, tree)| !truthy(get(tree, "sources")));
    if landed_empty {
        git::first_bank_history(&root, &authority)?;
    }
    let prior = existing
        .as_ref()
        .map(|existing| get(get(get(existing, "variants"), "local"), "sources"))
        .and_then(Value::as_array);
    let inventory = raw_sources::inventory_with_options(
        &evidence.bank,
        InventoryOptions {
            retain_directory: Some(&directory),
            prior_sources: prior,
            ..InventoryOptions::default()
        },
    )?;
    let raw_hashes: BTreeMap<&str, &Value> = inventory
        .sources
        .iter()
        .map(|s| (get(s, "path").as_str().unwrap_or(""), get(s, "sha256")))
        .collect();
    let agree = local
        .hashes
        .as_object()
        .unwrap_or(&[])
        .iter()
        .all(|(path, sha)| raw_hashes.get(path.as_str()) == Some(&sha));
    require(
        agree,
        "Canonical and complete raw local observations disagree",
    )?;
    variants.push((
        "local".to_owned(),
        fields([
            ("schema", string("agent-memory/layout-publication-local-v1")),
            ("revision", string(head.clone())),
            ("observation_complete", Value::Bool(true)),
            ("sources", Value::Array(inventory.sources.clone())),
            (
                "physical_entries",
                Value::Array(inventory.physical_entries.clone()),
            ),
            ("totals", inventory.totals.clone()),
        ]),
    ));
    let indexed = local_boundary(&root, &local.hashes, &manifest)?;
    let binding = fields([
        ("repo_root", string(path_text(&root)?)),
        ("project_id", get(&checked, "project_id").clone()),
        ("bank_root", string(settings.bank_root.clone())),
        ("authority_remote", string(remote.clone())),
        ("authority_branch", string(branch.clone())),
        ("authority_head", string(authority.clone())),
        ("local_head", string(head)),
        ("base", string(base)),
        ("scope_context", context.clone()),
    ]);
    let observed_policy = publication::policy(&root, &binding)?;
    let value = fields([
        ("schema", string(OBSERVATION)),
        ("observation_id", string(observation_id)),
        ("operation_id", string(operation_id)),
        ("binding", binding),
        ("terminal", terminal.clone()),
        ("original_manifest_sha256", string(value_digest(&manifest)?)),
        ("variants", Value::Object(variants)),
        ("local_hashes", local.hashes.clone()),
        ("original_index_sha256", index.clone()),
        ("index_entries_sha256", string(indexed)),
        ("repository_policy", observed_policy),
        (
            "runtime_artifact_digest",
            string(runtime_digest(runtime_root)?),
        ),
    ]);
    local.recheck("recall-final-source-recheck")?;
    let readback = raw_sources::inventory_with_options(
        &evidence.bank,
        InventoryOptions {
            prior_sources: Some(&inventory.sources),
            ..InventoryOptions::default()
        },
    )?;
    let unchanged = same(
        &Value::Array(readback.physical_entries),
        &Value::Array(inventory.physical_entries),
    )? && same(
        &Value::Array(readback.sources),
        &Value::Array(inventory.sources),
    )? && same(&readback.totals, &inventory.totals)?;
    require(
        unchanged,
        "Complete local raw bytes, occupants or physical metadata changed during publication observation",
    )?;
    writes::recovery_frontier_provider()?.check(
        get(&terminal, "frontier_token"),
        &root,
        None,
        None,
    )?;
    let checked_project = as_text(
        get(&checked, "project_id"),
        "Recovery evidence belongs to another project",
    )?;
    scope_binding::require(
        &root,
        Some(checked_project),
        RequireOptions {
            expected: Some(&context),
            ..RequireOptions::default()
        },
    )?;
    require(
        publication::index_bytes(&root)? == index
            && git::server(&root, &remote, &branch)? == authority,
        "Local index or advertised authority changed during publication observation",
    )?;
    state::write_once(&directory.join("observation.json"), &value)?;
    Ok(Observed {
        root,
        directory,
        value,
        files,
    })
}

fn binding(observation: &Value) -> &Value {
    get(observation, "binding")
}
fn bank_of(root: &Path, runtime_root: &Path) -> Result<PathBuf> {
    Ok(state::paths(root, None, runtime_root)?.1)
}

/// Unsupported source names do not acquire admission merely by relocation.
fn preservable_source(
    observation: &Value,
    source: &Value,
    unit: &Value,
    runtime_root: &Path,
) -> Result<()> {
    let path = get(source, "path");
    if path.as_str() == Some("MEMORY.md") || bank::path_value(path).is_ok() {
        return Ok(());
    }
    let root = PathBuf::from(as_text(
        get(binding(observation), "repo_root"),
        "Publication request binding changed",
    )?);
    let operation_id = as_text(
        get(observation, "operation_id"),
        "Publication request binding changed",
    )?;
    let index = state::index(&root, runtime_root)?;
    let row = get(get(&index, "operations"), operation_id);
    let frontier = writes::recovery_frontier_provider()?;
    let bank_path = bank_of(&root, runtime_root)?;
    let (directory, _, ledger) =
        frontier.ledger(&root, operation_id, row, &bank_path, runtime_root)?;
    let (manifest, _) = frontier.evidence(&directory, &ledger, row, runtime_root)?;
    let original = rows(get(&manifest, "sources"), "Invalid original manifest")?
        .iter()
        .find(|s| {
            get(s, "path") == path
                && get(s, "sha256") == get(source, "sha256")
                && get(s, "byte_count") == get(source, "byte_count")
        })
        .ok_or_else(|| {
            err("New unsupported authority/local source needs explicit recovery/admission review before publication")
        })?;
    let unit_map = read(&directory.join("unit-map.json"))?;
    let mut entitled: Vec<&Value> = rows(get(&unit_map, "units"), "Invalid unit map")?
        .iter()
        .filter(|u| {
            get(u, "disposition").as_str() == Some("preserve")
                && get(u, "semantic_kind").as_str() != Some("unadmitted-source")
        })
        .collect();
    // Revised pending parents remain historical. Actual admitted terminal child
    // ranges, not a superseded parent assertion, establish their entitlement.
    entitled.extend(
        rows(get(&ledger, "units"), "Invalid recovery ledger")?
            .iter()
            .filter(|u| {
                matches!(
                    get(u, "state").as_str(),
                    Some("admitted" | "governed-terminal")
                )
            }),
    );
    let invalid = || err("Invalid complete source byte range");
    let start = int(get(unit, "start")).ok_or_else(invalid)?;
    let end = int(get(unit, "end")).ok_or_else(invalid)?;
    let mut spans = Vec::new();
    for entry in entitled {
        for range in rows(get(entry, "source_ranges"), "Invalid unit source ranges")? {
            if get(range, "source_id") != get(original, "source_id") {
                continue;
            }
            let r_start = int(get(range, "start")).ok_or_else(invalid)?;
            let r_end = int(get(range, "end")).ok_or_else(invalid)?;
            if r_end > start && r_start < end {
                spans.push((r_start.max(start), r_end.min(end)));
            }
        }
    }
    spans.sort();
    let mut cursor = start;
    for (span_start, span_end) in &spans {
        require(
            *span_start <= cursor,
            "A publication preservation range contains unadmitted source material",
        )?;
        cursor = cursor.max(*span_end);
    }
    require(
        !spans.is_empty() && cursor == end,
        "Retained opaque, unknown or unconfirmed source is not admitted guidance by a publication rename",
    )
}

fn typed_record(body: &str) -> Option<(String, Value, String)> {
    let (kind, metadata, content) = crate::records::parse(body, &[]).ok()?;
    TYPED
        .contains(&kind.as_str())
        .then_some((kind, metadata, content))
}
fn record_digest(record: &(String, Value, String)) -> Result<String> {
    value_digest(&Value::Array(vec![
        string(record.0.clone()),
        record.1.clone(),
        string(record.2.clone()),
    ]))
}
fn utf8(raw: Vec<u8>) -> Option<String> {
    String::from_utf8(raw).ok()
}

fn review_sources(observation: &Value) -> Vec<(String, &Value)> {
    let mut found = Vec::new();
    for (side, variant) in get(observation, "variants").as_object().unwrap_or(&[]) {
        for source in get(variant, "sources").as_array().unwrap_or(&[]) {
            found.push((side.clone(), source));
        }
    }
    found
}

fn check_review(
    observation: &Value,
    review: &Value,
    files: &Value,
    directory: &Path,
    runtime_root: &Path,
) -> Result<()> {
    exact(
        review,
        &[
            "schema",
            "review_id",
            "observation_sha256",
            "intended_files_sha256",
            "author",
            "reviewer",
            "reviewed_at",
            "source_units",
            "added_paths",
            "record_evidence",
            "identity_decisions",
            "semantic_preservation",
            "security",
            "decision",
        ],
        Some(REVIEW),
    )?;
    require(
        uuid_valid(get(review, "review_id"))
            && get(review, "observation_sha256").as_str()
                == Some(value_digest(observation)?.as_str())
            && get(review, "intended_files_sha256").as_str() == Some(value_digest(files)?.as_str())
            && get(review, "decision").as_str() == Some("approve")
            && get(review, "semantic_preservation") == &Value::Bool(true)
            && get(review, "security") == &Value::Bool(true),
        "Independent publication review must bind all exact sources and the complete intended result",
    )?;
    let author = get(review, "author");
    let reviewer = get(review, "reviewer");
    for principal in [author, reviewer] {
        exact(
            principal,
            &["developer_id", "harness", "session", "evidence"],
            None,
        )?;
        require(
            uuid_valid(get(principal, "developer_id")),
            "Review requires real developer attribution",
        )?;
        for field in ["harness", "session", "evidence"] {
            judgment(get(principal, field))?;
        }
    }
    require(
        (get(author, "harness"), get(author, "session"))
            != (get(reviewer, "harness"), get(reviewer, "session")),
        "Proposal author cannot independently approve their own publication",
    )?;
    crate::lifecycle::time_value(get(review, "reviewed_at"))?;
    let scope = get(binding(observation), "scope_context");
    require(
        get(author, "developer_id") == get(scope, "developer_id"),
        "Publication author differs from the actual detecting developer",
    )?;
    let listed = review_sources(observation);
    let sources: BTreeMap<(String, String), &Value> = listed
        .iter()
        .map(|(side, s)| {
            (
                (
                    side.clone(),
                    get(s, "source_id").as_str().unwrap_or("").to_owned(),
                ),
                *s,
            )
        })
        .collect();
    let order: Vec<(String, String)> = listed
        .iter()
        .map(|(side, s)| {
            (
                side.clone(),
                get(s, "source_id").as_str().unwrap_or("").to_owned(),
            )
        })
        .collect();
    let mut coverage: BTreeMap<(String, String), Vec<(i128, i128)>> =
        order.iter().map(|key| (key.clone(), Vec::new())).collect();
    let units = rows(
        get(review, "source_units"),
        "Every all-side raw source needs a reviewed primary mapping",
    )?;
    let mut targets: BTreeSet<String> = BTreeSet::new();
    for unit in units {
        exact(
            unit,
            &[
                "unit_id",
                "side",
                "source_id",
                "start",
                "end",
                "disposition",
                "targets",
                "rationale",
                "terminal_evidence",
            ],
            None,
        )?;
        let key = match (get(unit, "side").as_str(), get(unit, "source_id").as_str()) {
            (Some(side), Some(id)) => Some((side.to_owned(), id.to_owned())),
            _ => None,
        };
        let source = key.as_ref().and_then(|key| sources.get(key));
        let (Some(key), Some(source), true) =
            (key.clone(), source, uuid_valid(get(unit, "unit_id")))
        else {
            return Err(err("Publication review substituted an unknown raw source"));
        };
        let byte_count = int(get(source, "byte_count")).unwrap_or(-1);
        let (start, end) = (int(get(unit, "start")), int(get(unit, "end")));
        let (Some(start), Some(end)) = (start, end) else {
            return Err(err("Invalid complete source byte range"));
        };
        require(
            0 <= start && start <= end && end <= byte_count && (start < end || byte_count == 0),
            "Invalid complete source byte range",
        )?;
        coverage.entry(key).or_default().push((start, end));
        judgment(get(unit, "rationale"))?;
        let unit_targets = rows(
            get(unit, "targets"),
            "Source preservation needs exact destination references",
        )?;
        for target in unit_targets {
            exact(target, &["path", "sha256"], None)?;
            let path = get(target, "path").as_str();
            let body = path.and_then(|p| files.get(p)).and_then(Value::as_str);
            require(
                body.is_some_and(|body| {
                    get(target, "sha256").as_str() == Some(sha(body.as_bytes()).as_str())
                }),
                "Source mapping target differs from the intended complete result",
            )?;
            targets.insert(path.unwrap_or("").to_owned());
        }
        let disposition = get(unit, "disposition").as_str();
        require(
            matches!(
                disposition,
                Some("preserve" | "governed-terminal" | "reviewed-non-guidance-discard")
            ),
            "New authority sources needing admission/clarification must remain pending, not publish",
        )?;
        match disposition {
            Some("preserve") => {
                require(
                    !unit_targets.is_empty() && get(unit, "terminal_evidence") == &Value::Null,
                    "Preserved knowledge must remain available, not merely backed up",
                )?;
                preservable_source(observation, source, unit, runtime_root)?;
            }
            Some("governed-terminal") => {
                governed_evidence(observation, unit, source, files)?;
            }
            _ => {
                let evidence = get(unit, "terminal_evidence");
                exact(evidence, &["authority", "reason", "non_guidance"], None)?;
                require(
                    get(evidence, "non_guidance") == &Value::Bool(true) && unit_targets.is_empty(),
                    "Only explicitly reviewed non-guidance may leave current knowledge",
                )?;
                judgment(get(evidence, "authority"))?;
                judgment(get(evidence, "reason"))?;
                // A typed record/history can never be relabelled disposable framing.
                let kind = raw_body(directory, source)
                    .ok()
                    .and_then(utf8)
                    .and_then(|body| crate::records::parse(&body, &[]).ok())
                    .map(|(kind, _, _)| kind);
                require(
                    !kind.is_some_and(|kind| TYPED.contains(&kind.as_str())),
                    "Existing admitted records/history require actual governed terminal evidence",
                )?;
            }
        }
    }
    for key in &order {
        let mut spans = coverage.get(key).cloned().unwrap_or_default();
        spans.sort();
        let mut cursor = 0i128;
        for (start, end) in &spans {
            require(
                *start == cursor,
                "All raw source bytes must be mapped exactly once without gaps or overlap",
            )?;
            cursor = *end;
        }
        let byte_count = sources
            .get(key)
            .map_or(-1, |s| int(get(s, "byte_count")).unwrap_or(-1));
        require(
            !spans.is_empty() && cursor == byte_count,
            "Unread raw source remainder cannot be omitted",
        )?;
    }
    let ids: BTreeSet<Vec<u8>> = units
        .iter()
        .map(|u| encoded(get(u, "unit_id")))
        .collect::<Result<_>>()?;
    require(ids.len() == units.len(), "Duplicate publication unit ID")?;
    let added_names: Option<Vec<&str>> = get(review, "added_paths")
        .as_array()
        .and_then(|rows| rows.iter().map(Value::as_str).collect::<Option<Vec<_>>>());
    let expected_added: BTreeSet<&str> = files
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(p, _)| p.as_str())
        .filter(|p| !targets.contains(*p))
        .collect();
    let accounted = added_names.is_some_and(|names| {
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        unique.len() == names.len() && unique == expected_added
    });
    require(
        accounted,
        "Independent review must account for every intended output; path accounting does not admit records",
    )?;
    let decisions = get(review, "identity_decisions");
    let decided = decisions.as_object().is_some_and(|pairs| {
        let names: BTreeSet<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        names == BTreeSet::from(["base", "local", "landed"])
    });
    require(decided, "Review all three project/authority identities")?;
    let router_text = files
        .get("MEMORY.md")
        .and_then(Value::as_str)
        .ok_or_else(|| err("Complete publication result must pass normal eligibility/security"))?;
    let intended = bank::parse_router(router_text)?.0;
    let observed_binding = binding(observation);
    for (side, variant) in get(observation, "variants").as_object().unwrap_or(&[]) {
        let variant_sources = get(variant, "sources").as_array().unwrap_or(&[]);
        let router = variant_sources
            .iter()
            .find(|s| get(s, "path").as_str() == Some("MEMORY.md"));
        if variant_sources.is_empty() && side != "local" {
            let root = Path::new(as_text(
                get(observed_binding, "repo_root"),
                "Publication request binding changed",
            )?);
            let revision = as_text(
                get(variant, "revision"),
                "Publication request binding changed",
            )?;
            let prefix = as_text(
                get(observed_binding, "bank_root"),
                "Publication request binding changed",
            )?;
            require(
                command(root, &["rev-list", "--max-count=1", revision, "--", prefix])?
                    .trim()
                    .is_empty(),
                "A previously occupied authority bank cannot be treated as empty",
            )?;
            judgment(get(decisions, side))?;
            continue;
        }
        let router = router.ok_or_else(|| {
            err("Absent/ambiguous authority router requires explicit identity repair")
        })?;
        let metadata = raw_body(directory, router)
            .and_then(|raw| String::from_utf8(raw).map_err(|_| err("Malformed raw router")))
            .and_then(|body| bank::parse_router(&body).map(|parsed| parsed.0));
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(_) => {
                identity_review(
                    observation,
                    variant,
                    router,
                    get(decisions, side),
                    &intended,
                )?;
                continue;
            }
        };
        require(
            get(&metadata, "project-id") == get(&intended, "project-id")
                && get(&intended, "project-id") == get(observed_binding, "project_id")
                && metadata.get("canonical-remote") == intended.get("canonical-remote"),
            "Recovery publication cannot choose between conflicting project/authority identities",
        )?;
        judgment(get(decisions, side))?;
    }
    preserve_records(observation, files, review, directory)?;
    record_authority(observation, files, review, directory, runtime_root)
}

fn identity_review(
    observation: &Value,
    variant: &Value,
    router: &Value,
    decision: &Value,
    intended: &Value,
) -> Result<()> {
    exact(
        decision,
        &[
            "schema",
            "project_id",
            "raw_router_sha256",
            "project_registry_sha256",
            "checkout_key",
            "authority_revision",
            "authority_router_path",
            "authority_router_sha256",
            "rationale",
        ],
        Some("agent-memory/layout-publication-identity-review-v1"),
    )?;
    let observed = binding(observation);
    let scope = get(observed, "scope_context");
    require(
        get(decision, "project_id") == get(observed, "project_id")
            && get(decision, "raw_router_sha256") == get(router, "sha256")
            && get(decision, "project_registry_sha256") == get(scope, "project_sha256")
            && get(decision, "checkout_key") == get(scope, "checkout_key"),
        "Malformed raw router needs exact independent existing-project/checkout review",
    )?;
    let root = Path::new(as_text(
        get(observed, "repo_root"),
        "Publication request binding changed",
    )?);
    let revision = git::commit(
        root,
        as_text(
            get(decision, "authority_revision"),
            "An immutable full Git object ID is required",
        )?,
    )?;
    let variant_revision = as_text(
        get(variant, "revision"),
        "Publication request binding changed",
    )?;
    require(
        git::ancestor(root, revision, variant_revision)?,
        "Stronger identity evidence must be checked authority ancestry, not a path assertion",
    )?;
    let prefix = as_text(
        get(observed, "bank_root"),
        "Publication request binding changed",
    )?;
    let path = get(decision, "authority_router_path").as_str();
    let expected_path = format!("{prefix}/MEMORY.md");
    require(
        path == Some(expected_path.as_str()),
        "Identity evidence must name the independently discovered bank",
    )?;
    let expression = format!("{revision}:{expected_path}");
    let raw = crate::repository_config::object_router_bytes(
        root,
        OsStr::new(&expression),
        &mut BTreeMap::new(),
    )?;
    let body = std::str::from_utf8(&raw).map_err(|_| {
        err("Stronger existing authority evidence does not uniquely establish this project")
    })?;
    let metadata = bank::parse_router(body)?.0;
    require(
        get(decision, "authority_router_sha256").as_str() == Some(sha(&raw).as_str())
            && get(&metadata, "project-id") == get(intended, "project-id")
            && get(intended, "project-id") == get(observed, "project_id")
            && metadata.get("canonical-remote") == intended.get("canonical-remote"),
        "Stronger existing authority evidence does not uniquely establish this project",
    )?;
    judgment(get(decision, "rationale"))?;
    Ok(())
}

fn receipt_path(observation: &Value, operation_id: &str) -> Result<(PathBuf, PathBuf)> {
    let observed = binding(observation);
    let root = PathBuf::from(as_text(
        get(observed, "repo_root"),
        "Publication request binding changed",
    )?);
    let prefix = as_text(
        get(observed, "bank_root"),
        "Publication request binding changed",
    )?;
    let (_, receipt) = writes::state_paths(&root, &root.join(prefix), operation_id)?;
    Ok((root, receipt))
}

fn governed_evidence(
    observation: &Value,
    unit: &Value,
    source: &Value,
    files: &Value,
) -> Result<()> {
    let evidence = get(unit, "terminal_evidence");
    exact(
        evidence,
        &["operation_id", "receipt_sha256", "proposal"],
        None,
    )?;
    let proposal = tx::proposal_value(get(evidence, "proposal"))?;
    let observed = binding(observation);
    let source_path = get(source, "path").as_str().unwrap_or("");
    let invalid = || {
        err("Governed source removal requires the actual same-source normal governance proposal")
    };
    require(
        get(proposal, "operation_id") == get(evidence, "operation_id")
            && get(proposal, "repo_root") == get(observed, "repo_root")
            && get(proposal, "project_id") == get(observed, "project_id")
            && proposal.get("governance_evidence").is_some()
            && get(proposal, "observed_hashes").get(source_path) == Some(get(source, "sha256")),
        "Governed source removal requires the actual same-source normal governance proposal",
    )?;
    let operation_id = get(evidence, "operation_id").as_str().ok_or_else(invalid)?;
    let (_, path) = receipt_path(observation, operation_id)?;
    let receipt = tx::receipt_value(&path, proposal)?;
    let changed = tx::changed(proposal)?;
    let matches = changed
        .iter()
        .all(|p| files.get(p) == get(proposal, "files").get(p));
    let finished = match &receipt {
        Some(receipt) => {
            get(receipt, "outcome").as_str() == Some("finished")
                && get(evidence, "receipt_sha256").as_str() == Some(value_digest(receipt)?.as_str())
        }
        None => false,
    };
    require(
        finished && matches,
        "Proposed, partial or mismatched governance is not a terminal publication disposition",
    )
}

fn record_key(metadata: &Value) -> Result<Vec<u8>> {
    encoded(&Value::Array(vec![
        metadata.get("id").cloned().unwrap_or(Value::Null),
        metadata.get("version").cloned().unwrap_or(Value::Null),
    ]))
}

fn preserve_records(
    observation: &Value,
    files: &Value,
    review: &Value,
    directory: &Path,
) -> Result<()> {
    let mut output: BTreeMap<Vec<u8>, (Value, String)> = BTreeMap::new();
    for (_, body) in files.as_object().unwrap_or(&[]) {
        let Some(body) = body.as_str() else {
            continue;
        };
        if let Some((_, metadata, _)) = typed_record(body) {
            output.insert(record_key(&metadata)?, (metadata, body.to_owned()));
        }
    }
    let units = get(review, "source_units").as_array().unwrap_or(&[]);
    for (side, source) in review_sources(observation) {
        let parsed = raw_body(directory, source)
            .ok()
            .and_then(utf8)
            .and_then(|body| typed_record(&body));
        let Some((_, metadata, body)) = parsed else {
            continue;
        };
        let governed = units.iter().any(|u| {
            get(u, "side").as_str() == Some(side.as_str())
                && get(u, "source_id") == get(source, "source_id")
                && get(u, "disposition").as_str() == Some("governed-terminal")
        });
        if governed {
            continue;
        }
        let preserved = match output.get(&record_key(&metadata)?) {
            Some((target_metadata, target_body)) => {
                same(target_metadata, &metadata)?
                    && crate::records::parse(target_body, &[])?.2 == body
            }
            None => false,
        };
        require(
            preserved,
            "Existing typed identities, provenance, conditions and history cannot be rewritten by layout publication",
        )?;
    }
    Ok(())
}

/// Publication preserves admitted identities; it cannot create admission.
///
/// A new local active file is not evidence of its own admission. Its authority
/// must precede this repair (originals or observed Git sides), or come from an
/// exact finished ordinary transaction whose changed record is still current.
/// The same check runs through the final landing source guard.
fn record_authority(
    observation: &Value,
    files: &Value,
    review: &Value,
    directory: &Path,
    runtime_root: &Path,
) -> Result<()> {
    let observed = binding(observation);
    let root = PathBuf::from(as_text(
        get(observed, "repo_root"),
        "Publication request binding changed",
    )?);
    let project_id = get(observed, "project_id");
    let operation_id = as_text(
        get(observation, "operation_id"),
        "Publication request binding changed",
    )?;
    let (_, _, _, operation) = state::paths(&root, Some(operation_id), runtime_root)?;
    let operation = operation.ok_or_else(|| err("Publication request binding changed"))?;
    let manifest = read(&operation.join("manifest.json"))?;
    require(
        get(observation, "original_manifest_sha256").as_str()
            == Some(value_digest(&manifest)?.as_str()),
        "Original admitted-source evidence changed during publication review",
    )?;
    let mut admitted = BTreeSet::new();
    let variants = get(observation, "variants");
    let groups: [(&Path, &[Value], bool); 3] = [
        (
            &operation,
            get(&manifest, "sources").as_array().unwrap_or(&[]),
            false,
        ),
        (
            directory,
            get(get(variants, "base"), "sources")
                .as_array()
                .unwrap_or(&[]),
            true,
        ),
        (
            directory,
            get(get(variants, "landed"), "sources")
                .as_array()
                .unwrap_or(&[]),
            true,
        ),
    ];
    for (source_directory, sources, publication_side) in groups {
        for source in sources {
            if publication_side && bank::path_value(get(source, "path")).is_err() {
                continue;
            }
            let Some(body) = utf8(raw_body(source_directory, source)?) else {
                continue;
            };
            if let Some(record) = typed_record(&body) {
                let (kind, metadata, _) = &record;
                let owner = if kind == "tombstone" {
                    get(metadata, "scope")
                } else {
                    get(metadata, "project-id")
                };
                require(
                    owner == project_id
                        && !matches!(
                            get(metadata, "status").as_str(),
                            Some("candidate" | "quarantined")
                        ),
                    "Unadmitted or foreign source records cannot confer publication authority",
                )?;
                admitted.insert(record_digest(&record)?);
            }
        }
    }
    let evidence_rows = rows(
        get(review, "record_evidence"),
        "Publication needs explicit committed evidence for new local records",
    )?;
    let mut seen = BTreeSet::new();
    let local_hashes = get(observation, "local_hashes");
    let root_text = path_text(&root)?;
    for evidence in evidence_rows {
        exact(
            evidence,
            &["operation_id", "receipt_sha256", "proposal"],
            None,
        )?;
        let proposal = tx::proposal_value(get(evidence, "proposal"))?;
        let evidence_id = get(evidence, "operation_id").as_str();
        let ordinary = [
            "admission_evidence",
            "governance_evidence",
            "consolidation_evidence",
        ]
        .iter()
        .any(|key| proposal.get(key).is_some());
        let valid = evidence_id.is_some_and(|id| {
            get(proposal, "operation_id").as_str() == Some(id) && !seen.contains(id)
        }) && get(proposal, "repo_root").as_str() == Some(root_text)
            && get(proposal, "project_id") == project_id
            && ordinary;
        require(
            valid,
            "New record authority needs its actual same-project ordinary admission/governance operation",
        )?;
        let evidence_id = evidence_id.unwrap_or("");
        seen.insert(evidence_id.to_owned());
        let (_, path) = receipt_path(observation, evidence_id)?;
        let stored = read(&path)?;
        let receipt = tx::receipt_value(&path, proposal)?;
        let committed = match &receipt {
            Some(receipt) => {
                same(receipt, &stored)?
                    && get(receipt, "outcome").as_str() == Some("finished")
                    && get(evidence, "receipt_sha256").as_str()
                        == Some(value_digest(receipt)?.as_str())
            }
            None => false,
        };
        require(
            committed,
            "Caller assertions, proposed or rolled-back transactions do not admit publication records",
        )?;
        let receipt = receipt.unwrap_or(Value::Null);
        let mut current = BTreeSet::new();
        for path in tx::changed(proposal)? {
            let record = get(proposal, "files")
                .get(&path)
                .and_then(Value::as_str)
                .and_then(typed_record);
            if let Some(record) = record {
                if local_hashes.get(&path) == get(&receipt, "resulting_hashes").get(&path) {
                    current.insert(record_digest(&record)?);
                }
            }
        }
        require(
            !current.is_empty(),
            "Committed evidence must identify exact changed records still present in the observed bank",
        )?;
        admitted.extend(current);
    }
    for (_, body) in files.as_object().unwrap_or(&[]) {
        let record = body.as_str().and_then(typed_record);
        let allowed = match &record {
            None => true,
            Some(record) => admitted.contains(&record_digest(record)?),
        };
        require(
            allowed,
            "Every publication record identity/version and body needs observed admitted source or exact current committed admission/governance evidence",
        )?;
    }
    Ok(())
}

fn candidate(root: &Path, observed: &Value, before: &Value, files: &Value) -> Result<String> {
    let prefix = as_text(
        get(observed, "bank_root"),
        "Publication request binding changed",
    )?;
    let head = as_text(
        get(observed, "authority_head"),
        "Publication request binding changed",
    )?;
    let mut paths: BTreeSet<&str> = get(before, "sources")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|s| get(s, "path").as_str())
        .collect();
    paths.extend(
        files
            .as_object()
            .unwrap_or(&[])
            .iter()
            .map(|(p, _)| p.as_str()),
    );
    let (operations, _) = writes::state_paths(root, &root.join(prefix), &crate::host::uuid4()?)?;
    crate::local_registry::private_mkdir(&operations)?;
    let workspace = TemporaryDirectory::new(&operations, "layout-publication-")?;
    let index = workspace.path().join("index");
    let outcome = (|| {
        let local = GitWrite {
            index: Some(&index),
            ..GitWrite::default()
        };
        publication::git_write(root, &["read-tree", head], local)?;
        for path in &paths {
            let full = format!("{prefix}/{path}");
            match files.get(path) {
                None => {
                    publication::git_write(
                        root,
                        &["update-index", "--force-remove", "--", &full],
                        local,
                    )?;
                }
                Some(body) => {
                    let body = as_text(
                        body,
                        "Complete publication result must pass normal eligibility/security",
                    )?;
                    let (_, oid) = publication::git_write(
                        root,
                        &["hash-object", "-w", "--stdin"],
                        GitWrite {
                            payload: Some(body.as_bytes()),
                            ..GitWrite::default()
                        },
                    )?;
                    let info = format!("100644,{},{full}", oid.trim());
                    publication::git_write(
                        root,
                        &["update-index", "--add", "--cacheinfo", &info],
                        local,
                    )?;
                }
            }
        }
        let (_, tree) = publication::git_write(root, &["write-tree"], local)?;
        let signing = publication::git(root, &["config", "--bool", "commit.gpgsign"], true)?;
        let mut args = vec!["commit-tree", tree.trim(), "-p", head];
        if signing.as_deref() == Some("true") {
            args.push("-S");
        }
        args.extend(["-m", "Publish source-preserving project memory recovery"]);
        let (_, commit) = publication::git_write(root, &args, GitWrite::default())?;
        Ok(commit)
    })();
    let commit = workspace.finish(outcome)?;
    Ok(git::oid(commit.trim())?.to_owned())
}

fn delta(
    root: &Path,
    base: &str,
    candidate: &str,
    prefix: &str,
    before: &BTreeMap<String, String>,
    files: &Value,
) -> Result<Value> {
    let parents = command(root, &["rev-list", "--parents", "-n", "1", candidate])?;
    require(
        parents.split_whitespace().skip(1).collect::<Vec<_>>() == [base],
        "Recovery candidate must have exactly the reviewed authority parent",
    )?;
    let raw = git::command(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--no-renames",
            "--raw",
            "-r",
            "-z",
            base,
            candidate,
        ],
        false,
        git::MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    let parts: Vec<&str> = raw.split('\0').collect();
    let owned = format!("{prefix}/");
    let mut changed: Vec<(String, Value)> = Vec::new();
    for pair in parts
        .get(..parts.len().saturating_sub(1))
        .unwrap_or(&[])
        .chunks(2)
    {
        let [header, path] = pair else {
            return Err(err(
                "Entire authority delta contains an unreviewed code, mode, runtime or topology change",
            ));
        };
        let columns: Vec<&str> = header.split_whitespace().collect();
        let reviewed = match columns.as_slice() {
            [old_mode, new_mode, _, _, status] => {
                path.starts_with(&owned)
                    && matches!(*status, "A" | "M" | "D")
                    && matches!(*old_mode, ":000000" | ":100644")
                    && matches!(*new_mode, "000000" | "100644")
            }
            _ => false,
        };
        require(
            reviewed,
            "Entire authority delta contains an unreviewed code, mode, runtime or topology change",
        )?;
        let relative = path.get(owned.len()..).unwrap_or("");
        require(
            !bank::runtime_path(relative),
            "Runtime recovery artifacts must never be published",
        )?;
        let column = |i: usize| string(columns.get(i).copied().unwrap_or(""));
        changed.retain(|(p, _)| p != relative);
        changed.push((
            relative.to_owned(),
            fields([
                ("before", column(2)),
                ("after", column(3)),
                ("status", column(4)),
            ]),
        ));
    }
    let mut expected = BTreeSet::new();
    let file_map = files.as_object().unwrap_or(&[]);
    let all: BTreeSet<&str> = before
        .keys()
        .map(String::as_str)
        .chain(file_map.iter().map(|(p, _)| p.as_str()))
        .collect();
    for path in all {
        let intended = files
            .get(path)
            .and_then(Value::as_str)
            .map(|body| sha(body.as_bytes()));
        if before.get(path) != intended.as_ref() {
            expected.insert(path.to_owned());
        }
    }
    let actual: BTreeSet<String> = changed.iter().map(|(p, _)| p.clone()).collect();
    require(
        actual == expected && !changed.is_empty(),
        "Full candidate delta differs from the complete intended authority transformation",
    )?;
    let candidate_files = git::tree(root, candidate, prefix, false)?;
    require(
        same(&candidate_files, files)?,
        "Candidate normal readback differs from exact intended native files",
    )?;
    Ok(Value::Object(changed))
}

fn landed_hashes(observation: &Value) -> BTreeMap<String, String> {
    get(get(get(observation, "variants"), "landed"), "sources")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .map(|s| {
            (
                get(s, "path").as_str().unwrap_or("").to_owned(),
                get(s, "sha256").as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

/// `runtime_root` is the executing pack (baseline `__file__` parents), outside the repository.
pub fn inspect(
    repo: &Path,
    operation_id: &str,
    input: Option<&Value>,
    runtime_root: &Path,
) -> Result<Value> {
    require(
        input.is_none(),
        "First publication inspection takes no caller-supplied authority pins",
    )?;
    let observed = observe(repo, operation_id, None, runtime_root)?;
    let path = observed.directory.join("observation.json");
    result(
        "publish-inspect",
        operation_id,
        "publication-observed",
        fields([
            ("observation", observed.value),
            ("observation_path", string(path_text(&path)?)),
            ("canonical_eligibility", Value::Bool(true)),
            ("semantic_recovery_complete", Value::Bool(true)),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]),
    )
}

fn eligible(files: &Value, project_id: &Value) -> Result<bool> {
    if files.as_object().is_none() {
        return Ok(false);
    }
    Ok(
        get(&bank::validate_files(files, None)?, "project_id") == project_id
            && crate::security::complete_refresh_reasons(files)?.is_empty(),
    )
}

pub fn prepare(
    repo: &Path,
    operation_id: &str,
    input: &Value,
    runtime_root: &Path,
) -> Result<Value> {
    exact(
        input,
        &["schema", "observation_id", "intended_files", "review"],
        Some("agent-memory/layout-publication-prepare-v1"),
    )?;
    require(
        uuid_valid(get(input, "observation_id")),
        "Use the inspected publication observation UUID",
    )?;
    let observation_id = get(input, "observation_id").as_str();
    let observed = observe(repo, operation_id, observation_id, runtime_root)?;
    let observation = &observed.value;
    let observed_binding = binding(observation);
    let files = get(input, "intended_files");
    require(
        eligible(files, get(observed_binding, "project_id"))?,
        "Complete publication result must pass normal eligibility/security",
    )?;
    let checked = bank::validate_files(files, None)?;
    let admitted = get(&checked, "entries")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .all(|e| {
            !matches!(
                get(e, "lifecycle").as_str(),
                Some("candidate" | "quarantined")
            )
        });
    require(
        admitted,
        "Publication cannot automatically admit newly proposed records",
    )?;
    check_review(
        observation,
        get(input, "review"),
        files,
        &observed.directory,
        runtime_root,
    )?;
    let observed_policy = get(observation, "repository_policy");
    if truthy(get(observed_policy, "unsupported_rules")) {
        return result(
            "publish-prepare",
            operation_id,
            "pending-policy",
            fields([
                ("repository_policy", observed_policy.clone()),
                ("git_metadata_writes_performed", Value::Bool(true)),
                ("private_artifact_writes_performed", Value::Bool(true)),
            ]),
        );
    }
    let before = landed_hashes(observation);
    let file_map = files.as_object().unwrap_or(&[]);
    bank::file_topology(
        before
            .keys()
            .map(String::as_str)
            .chain(file_map.iter().map(|(p, _)| p.as_str())),
    )?;
    let before_value = Value::Object(
        before
            .iter()
            .map(|(p, h)| (p.clone(), string(h.clone())))
            .collect(),
    );
    if same(&before_value, &tx::file_hashes(files)?)? {
        return result(
            "publish-prepare",
            operation_id,
            "no-publishable-delta",
            fields([("git_metadata_writes_performed", Value::Bool(true))]),
        );
    }
    let root = &observed.root;
    let landed = get(get(observation, "variants"), "landed");
    let proposed = candidate(root, observed_binding, landed, files)?;
    let head = as_text(
        get(observed_binding, "authority_head"),
        "Publication request binding changed",
    )?;
    let prefix = as_text(
        get(observed_binding, "bank_root"),
        "Publication request binding changed",
    )?;
    let changed = delta(root, head, &proposed, prefix, &before, files)?;
    let request = fields([
        ("schema", string(REQUEST)),
        ("operation_id", string(operation_id)),
        ("repo_root", string(path_text(root)?)),
        ("project_id", get(observed_binding, "project_id").clone()),
        ("base", string(head)),
        ("candidate", string(proposed)),
        ("observation", observation.clone()),
        ("intended_files", files.clone()),
        ("review", get(input, "review").clone()),
        ("delta", changed),
    ]);
    check_current(root, &request, runtime_root)?;
    state::write_once(&observed.directory.join("request.json"), &request)?;
    result(
        "publish-prepare",
        operation_id,
        "prepared",
        fields([
            ("request", request),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("canonical_eligibility", Value::Bool(true)),
            ("semantic_recovery_complete", Value::Bool(true)),
        ]),
    )
}

fn checked_request(
    repo: &Path,
    operation_id: &str,
    request: &Value,
    runtime_root: &Path,
) -> Result<(PathBuf, PathBuf)> {
    exact(
        request,
        &[
            "schema",
            "operation_id",
            "repo_root",
            "project_id",
            "base",
            "candidate",
            "observation",
            "intended_files",
            "review",
            "delta",
        ],
        Some(REQUEST),
    )?;
    let (root, _, _, operation) = state::paths(repo, Some(operation_id), runtime_root)?;
    let operation = operation.ok_or_else(|| err("Publication request binding changed"))?;
    let observation = get(request, "observation");
    let observed_binding = binding(observation);
    require(
        get(request, "operation_id").as_str() == Some(operation_id)
            && get(request, "repo_root").as_str() == Some(path_text(&root)?)
            && get(observation, "operation_id").as_str() == Some(operation_id)
            && uuid_valid(get(observation, "observation_id"))
            && get(request, "project_id") == get(observed_binding, "project_id")
            && observed_binding.get("project_id").is_some()
            && get(request, "base") == get(observed_binding, "authority_head")
            && observed_binding.get("authority_head").is_some(),
        "Publication request binding changed",
    )?;
    let observation_id = as_text(
        get(observation, "observation_id"),
        "Publication request binding changed",
    )?;
    let directory = operation.join("publication").join(observation_id);
    require(
        same(&read(&directory.join("request.json"))?, request)?,
        "Use the exact immutable prepared publication request",
    )?;
    Ok((root, directory))
}

fn check_current(root: &Path, request: &Value, runtime_root: &Path) -> Result<()> {
    let operation_id = as_text(
        get(request, "operation_id"),
        "Publication request binding changed",
    )?;
    let observation_id = get(get(request, "observation"), "observation_id").as_str();
    let current = observe(root, operation_id, observation_id, runtime_root)?;
    require(
        same(&current.value, get(request, "observation"))?,
        "Authority, policy, runtime, source or terminal frontier changed",
    )?;
    let intended = get(request, "intended_files");
    check_review(
        &current.value,
        get(request, "review"),
        intended,
        &current.directory,
        runtime_root,
    )?;
    let before = landed_hashes(&current.value);
    let observed_binding = binding(&current.value);
    let base = as_text(get(request, "base"), "Publication request binding changed")?;
    let candidate = as_text(
        get(request, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let prefix = as_text(
        get(observed_binding, "bank_root"),
        "Publication request binding changed",
    )?;
    let actual = delta(root, base, candidate, prefix, &before, intended)?;
    require(
        same(&actual, get(request, "delta"))?,
        "Actual prepared authority delta changed",
    )
}

/// An internal checked source guard; never deserialized from a request.
pub struct RecoveryPublicationContext {
    root: PathBuf,
    request: Value,
    runtime_root: PathBuf,
}
impl RecoveryPublicationContext {
    fn new(root: &Path, request: &Value, runtime_root: &Path) -> Result<Self> {
        check_current(root, request, runtime_root)?;
        Ok(Self {
            root: root.to_path_buf(),
            request: request.clone(),
            runtime_root: runtime_root.to_path_buf(),
        })
    }
    pub fn check(&self, root: &Path, observed_binding: &Value) -> Result<()> {
        require(
            root == self.root
                && same(observed_binding, binding(get(&self.request, "observation")))?,
            "Recovery publication context does not match the existing landing operation",
        )?;
        check_current(root, &self.request, &self.runtime_root)
    }
}

fn replay(root: &Path, request: &Value, runtime_root: &Path) -> Result<Value> {
    let observed_binding = binding(get(request, "observation"));
    let remote = as_text(
        get(observed_binding, "authority_remote"),
        "Publication request binding changed",
    )?;
    let branch = as_text(
        get(observed_binding, "authority_branch"),
        "Publication request binding changed",
    )?;
    let prefix = as_text(
        get(observed_binding, "bank_root"),
        "Publication request binding changed",
    )?;
    let base = as_text(get(request, "base"), "Publication request binding changed")?;
    let candidate = as_text(
        get(request, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let intended = get(request, "intended_files");
    let head = fetch(root, remote, branch)?;
    let lineage_text = command(
        root,
        &["rev-list", "--first-parent", "--max-count=130", &head],
    )?;
    let lineage: Vec<&str> = lineage_text.lines().collect();
    let position = lineage.iter().position(|revision| *revision == base);
    let position = position.filter(|p| *p <= 128).ok_or_else(|| {
        err("Recovery publication mainline witness is unavailable; do not blindly retry a push")
    })?;
    let delta_paths: Vec<(String, Option<String>)> = get(request, "delta")
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(path, _)| {
            (
                path.clone(),
                intended
                    .get(path)
                    .and_then(Value::as_str)
                    .map(|b| sha(b.as_bytes())),
            )
        })
        .collect();
    let tree_of = |revision: &str| {
        command(root, &["rev-parse", &format!("{revision}^{{tree}}")]).map(|t| t.trim().to_owned())
    };
    let candidate_tree = tree_of(candidate)?;
    let mut witness = None;
    for revision in lineage.get(..position).unwrap_or(&[]) {
        if tree_of(revision)? == candidate_tree {
            // An exact whole authority tree, not reachability or a subset of
            // memory paths, witnesses this independently reviewed candidate.
            require(
                same(&git::tree(root, revision, prefix, false)?, intended)?,
                "Historical repaired authority tree differs from the complete intended result",
            )?;
            witness = Some((*revision).to_owned());
            break;
        }
    }
    let still_current = match observe_tree(root, &head, prefix, None, None, runtime_root) {
        Ok(current) => {
            let hashes: BTreeMap<&str, &str> = get(&current, "sources")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(|s| {
                    (
                        get(s, "path").as_str().unwrap_or(""),
                        get(s, "sha256").as_str().unwrap_or(""),
                    )
                })
                .collect();
            Value::Bool(
                delta_paths
                    .iter()
                    .all(|(path, sha)| hashes.get(path.as_str()).copied() == sha.as_deref()),
            )
        }
        Err(error) => {
            if witness.is_none() {
                return Err(error);
            }
            string("unavailable")
        }
    };
    require(
        git::server(root, remote, branch)? == head,
        "Authority advanced during historical publication readback",
    )?;
    Ok(fields([
        ("witnessed_commit", witness.map_or(Value::Null, string)),
        ("intended_delta_current", still_current),
    ]))
}

fn receipt_value(operation_id: &str, request: &Value, readback: &Value) -> Result<Value> {
    let mut receipt = fields([
        (
            "schema",
            string("agent-memory/layout-publication-receipt-v1"),
        ),
        ("operation_id", string(operation_id)),
        ("request_sha256", string(value_digest(request)?)),
        ("publication_verified", Value::Bool(true)),
        ("readback_at", string(crate::host::utc_now_isoformat()?)),
    ]);
    bank::merge(&mut receipt, readback)?;
    Ok(receipt)
}

fn with_readback(values: Value, readback: &Value) -> Result<Value> {
    let mut values = values;
    bank::merge(&mut values, readback)?;
    Ok(values)
}

pub fn land(repo: &Path, operation_id: &str, input: &Value, runtime_root: &Path) -> Result<Value> {
    let (root, directory) = checked_request(repo, operation_id, input, runtime_root)?;
    let request = input;
    // Historical success is checked before demanding old local sources reappear.
    // It never replays bytes over later canonical/authority corrections.
    let project = as_text(
        get(request, "project_id"),
        "Publication request binding changed",
    )?;
    scope_binding::require(&root, Some(project), RequireOptions::default())?;
    let original_index = publication::index_bytes(&root)?;
    let readback = replay(&root, request, runtime_root)?;
    if get(&readback, "witnessed_commit") != &Value::Null {
        require(
            publication::index_bytes(&root)? == original_index,
            "Original feature index changed during historical readback",
        )?;
        let path = directory.join("receipt.json");
        let existing = state::read(&path, limits::MAX_PRIVATE, true)?;
        let created = existing.is_none();
        let receipt = match existing {
            Some(receipt) => receipt,
            None => {
                let receipt = receipt_value(operation_id, request, &readback)?;
                state::write_once(&path, &receipt)?;
                receipt
            }
        };
        require(
            get(&receipt, "request_sha256").as_str() == Some(value_digest(request)?.as_str())
                && get(&receipt, "publication_verified") == &Value::Bool(true),
            "Historical publication receipt does not match this exact request",
        )?;
        return result(
            "publish-land",
            operation_id,
            "already-landed",
            with_readback(
                fields([
                    ("publication_verified", Value::Bool(true)),
                    ("receipt_is_historical", Value::Bool(true)),
                    ("already_applied", Value::Bool(true)),
                    ("git_metadata_writes_performed", Value::Bool(true)),
                    ("receipt", receipt),
                    ("private_artifact_writes_performed", Value::Bool(created)),
                ]),
                &readback,
            )?,
        );
    }
    let context = RecoveryPublicationContext::new(&root, request, runtime_root)?;
    let observed_binding = binding(get(request, "observation")).clone();
    let observed_policy = publication::policy(&root, &observed_binding)?;
    if truthy(get(&observed_policy, "unsupported_rules")) {
        return result(
            "publish-land",
            operation_id,
            "pending-policy",
            fields([
                ("repository_policy", observed_policy),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        );
    }
    // Adapter supplies only the binding expected by the existing policy engine;
    // it is never passed to semantic.check or claimed as a sharing-v2 proof.
    let landing = fields([
        ("base", get(request, "base").clone()),
        ("candidate", get(request, "candidate").clone()),
        ("project_id", get(request, "project_id").clone()),
        (
            "proof",
            fields([("judgment", fields([("binding", observed_binding.clone())]))]),
        ),
    ]);
    let candidate = as_text(
        get(request, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let protected = truthy(get(&observed_policy, "protected"));
    let mut proposed = candidate.to_owned();
    let mut url = Value::Null;
    let rules_pending = |reason: Option<String>, url: String| {
        result(
            "publish-land",
            operation_id,
            "pending-repository-rules",
            fields([
                ("reason", reason.map_or(Value::Null, string)),
                ("pull_request", string(url)),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        )
    };
    if protected {
        let (merged, pr, reason) =
            publication::protected_candidate(&root, &landing, &observed_policy, Some(&context))?;
        let Some(merged) = merged else {
            return rules_pending(reason, pr);
        };
        proposed = merged;
        url = string(pr);
    }
    context.check(&root, &observed_binding)?;
    require(
        same(
            &publication::policy(&root, &observed_binding)?,
            &observed_policy,
        )? && publication::index_bytes(&root)? == original_index,
        "Policy/original index changed before recovery landing",
    )?;
    if protected {
        let (merged, pr, reason) =
            publication::protected_candidate(&root, &landing, &observed_policy, Some(&context))?;
        let Some(merged) = merged else {
            return rules_pending(reason, pr);
        };
        proposed = merged;
        url = string(pr);
    }
    context.check(&root, &observed_binding)?;
    writes::fault("layout-recovery-publication-before-push")?;
    let remote = as_text(
        get(&observed_binding, "authority_remote"),
        "Publication request binding changed",
    )?;
    let branch = as_text(
        get(&observed_binding, "authority_branch"),
        "Publication request binding changed",
    )?;
    let refspec = format!("{proposed}:refs/heads/{branch}");
    let (code, _) = publication::git_write(
        &root,
        &[
            "push",
            "--porcelain",
            "--no-follow-tags",
            "--recurse-submodules=no",
            "--",
            remote,
            &refspec,
        ],
        GitWrite {
            network: true,
            optional: true,
            ..GitWrite::default()
        },
    )?;
    writes::fault("layout-recovery-publication-after-push")?;
    let readback = replay(&root, request, runtime_root)?;
    require(
        publication::index_bytes(&root)? == original_index,
        "Original feature index changed externally; preserve it",
    )?;
    if get(&readback, "witnessed_commit") == &Value::Null {
        return result(
            "publish-land",
            operation_id,
            "pending-reconciliation",
            fields([
                ("transport_exit", Value::Int(i64::from(code).into())),
                ("pull_request", url),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        );
    }
    let receipt = receipt_value(operation_id, request, &readback)?;
    state::write_once(&directory.join("receipt.json"), &receipt)?;
    result(
        "publish-land",
        operation_id,
        "landed",
        with_readback(
            fields([
                ("publication_verified", Value::Bool(true)),
                ("receipt", receipt),
                ("pull_request", url),
                ("private_artifact_writes_performed", Value::Bool(true)),
                ("git_metadata_writes_performed", Value::Bool(true)),
                ("history_writes_performed", Value::Bool(true)),
            ]),
            &readback,
        )?,
    )
}
