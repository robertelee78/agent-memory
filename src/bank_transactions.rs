//! Recoverable per-file bank transactions; mechanical, not admission authority.
use crate::bank_contract as contract;
use crate::bank_writes::{self as writes, StateEnvironment};
use crate::codec::json::Value;
use crate::developer_identity::{digest, encoded, load_with_limit, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::host::{self, Sources};
use crate::lifecycle::{get, object, string};
use crate::{local_registry as storage, repository_config, sharing_limits as limits};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const MARKER: &str = ".agent-memory-transaction.md";
pub const SCHEMA: &str = "agent-memory/transaction-proposal-v1";
pub const RECOVERY_SCHEMA: &str = "agent-memory/transaction-recovery-v1";
pub const FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "operation",
    "repo_root",
    "project_id",
    "observed_hashes",
    "files",
    "scope_context",
];
pub const RECOVERY_FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "repo_root",
    "mode",
    "journal_sha256",
    "marker_sha256",
    "observed_hashes",
];
pub const RECEIPT_FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "request_sha256",
    "bank_path",
    "project_id",
    "observed_hashes",
    "resulting_hashes",
    "outcome",
    "changed_memory_paths",
    "readback_at",
    "verified",
];
const EVIDENCE: &[&str] = &[
    "initialization_evidence",
    "admission_evidence",
    "consolidation_evidence",
    "governance_evidence",
    "refresh_evidence",
    "checkpoint_evidence",
];
pub const MAX_MARKER_BYTES: usize = limits::MAX_PRIVATE;
pub const MAX_JOURNAL_BYTES: usize = limits::MAX_PRIVATE;
pub const MAX_RECEIPT_BYTES: usize = limits::MAX_PRIVATE;

/// Operation-keyed semantic/source gate implemented by initialization, admission,
/// consolidation, governance and memory_refresh. Sources are checked separately
/// from structural/result validation; historical receipts never refresh sources.
pub trait TransactionValidator: Send + Sync {
    fn validate_transaction(&self, proposal: &Value, owned_lock: Option<&[u8]>) -> Result<()>;
    fn check_sources(&self, proposal: &Value, owner: &[u8], marker: &[u8]) -> Result<()>;
}
/// Optional, non-authoritative recall_catalog cache maintenance.
pub trait IndexInvalidation: Send + Sync {
    fn invalidate_after_change(&self, repo: &Path, hashes: &Value) -> Result<Value>;
}
/// project_identity's current canonical-remote evidence, not a second identity resolver.
pub trait ProjectIdentity: Send + Sync {
    fn status(&self, repo: &Path) -> Result<Value>;
}
static VALIDATORS: RwLock<BTreeMap<String, Arc<dyn TransactionValidator>>> =
    RwLock::new(BTreeMap::new());
static INDEX: RwLock<Option<Arc<dyn IndexInvalidation>>> = RwLock::new(None);
static PROJECT: RwLock<Option<Arc<dyn ProjectIdentity>>> = RwLock::new(None);
pub fn register_transaction_validator(
    operation: &str,
    provider: Option<Arc<dyn TransactionValidator>>,
) -> Result<()> {
    if !writes::OPERATIONS.contains(&operation) {
        return Err(err("Invalid bank transaction operation kind"));
    }
    let mut registry = VALIDATORS.write().unwrap_or_else(|e| e.into_inner());
    if let Some(provider) = provider {
        registry.insert(operation.to_owned(), provider);
    } else {
        registry.remove(operation);
    }
    Ok(())
}
pub fn register_index_invalidation(provider: Option<Arc<dyn IndexInvalidation>>) {
    *INDEX.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
/// Shared lookup for recovery_transactions; missing governed kinds fail closed.
pub fn transaction_validator(operation: &str) -> Result<Arc<dyn TransactionValidator>> {
    VALIDATORS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(operation)
        .cloned()
        .ok_or_else(|| {
            err(&format!(
                "Transaction validator is not registered for operation kind: {operation}"
            ))
        })
}
/// The baseline consulted semantic validators only for governed evidence or initialize.
pub fn validate_operation(proposal: &Value, owned_lock: Option<&[u8]>) -> Result<()> {
    if governed(proposal) {
        validator(proposal)?.validate_transaction(proposal, owned_lock)?;
    }
    Ok(())
}
pub fn register_project_identity(provider: Option<Arc<dyn ProjectIdentity>>) {
    *PROJECT.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
fn validator(p: &Value) -> Result<Arc<dyn TransactionValidator>> {
    transaction_validator(text(p, "operation")?)
}
fn governed(p: &Value) -> bool {
    EVIDENCE.iter().take(5).any(|key| p.get(key).is_some())
        || get(p, "operation").as_str() == Some("initialize")
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_error(e: impl std::fmt::Display) -> ContractError {
    err(&e.to_string())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    get(v, key)
        .as_str()
        .ok_or_else(|| err("Invalid bank transaction proposal"))
}
fn entries(v: &Value) -> Result<&[(String, Value)]> {
    v.as_object()
        .ok_or_else(|| err("Transaction files and hashes must be objects"))
}
fn path_text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "transaction path")
}
fn sha(raw: &[u8]) -> String {
    // Invariant: digest(Some(bytes)) returns Some even for an empty byte slice.
    digest(Some(raw)).expect("present digest input")
}
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return false;
            }
            if a.iter().zip(b).all(|((a, _), (b, _))| a == b) {
                return a.iter().zip(b).all(|((_, a), (_, b))| same(a, b));
            }
            let indexed: BTreeMap<_, _> =
                b.iter().map(|(key, value)| (key.as_str(), value)).collect();
            a.iter().all(|(key, value)| {
                indexed
                    .get(key.as_str())
                    .is_some_and(|other| same(value, other))
            })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        _ => a == b,
    }
}
fn put(v: &mut Value, key: &str, value: Value) -> Result<()> {
    contract::merge(v, &object([(key, value)]))
}
fn remove(v: &mut Value, key: &str) {
    if let Value::Object(map) = v {
        map.retain(|(k, _)| k != key);
    }
}
fn reject_symlink(path: &Path, message: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Transaction path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Transaction path has no filename"))?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    if chain
        .directory()
        .stat_at(name)
        .map_err(io_error)?
        .is_symlink()
    {
        return Err(err(message));
    }
    chain.recheck().map_err(io_error)
}
fn keys(v: &Value) -> Result<BTreeSet<String>> {
    Ok(entries(v)?.iter().map(|(k, _)| k.clone()).collect())
}
fn participants(p: &Value) -> Result<BTreeSet<String>> {
    Ok(keys(get(p, "observed_hashes"))?
        .union(&keys(get(p, "files"))?)
        .cloned()
        .collect())
}
fn first_bank(p: &Value) -> bool {
    get(p, "operation").as_str() == Some("initialize")
        && get(p, "observed_hashes")
            .as_object()
            .is_some_and(|map| map.is_empty())
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object().is_some_and(|map| {
        map.len() == fields.len() && map.iter().all(|(k, _)| fields.contains(&k.as_str()))
    })
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Transaction path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Transaction path has no filename"))?;
    fs::read_regular(parent, Path::new(name), limit)
        .map(|(raw, _)| raw)
        .map_err(io_error)
}
fn exists(path: &Path) -> Result<bool> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Transaction path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Transaction path has no filename"))?;
    match DirectoryChain::open(parent) {
        Ok(chain) => {
            let present = match chain.directory().stat_at(name) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(io_error(e)),
            };
            chain.recheck().map_err(io_error)?;
            Ok(present)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_error(e)),
    }
}
pub fn result(operation: &str, fields: &Value) -> Result<Value> {
    let mut result = object([
        ("schema", string("agent-memory/transaction-result-v1")),
        ("operation", string(operation)),
    ]);
    contract::merge(&mut result, fields)?;
    Ok(result)
}
pub fn file_hashes(files: &Value) -> Result<Value> {
    Ok(Value::Object(
        entries(files)?
            .iter()
            .map(|(path, body)| {
                Ok((
                    path.clone(),
                    string(sha(body
                        .as_str()
                        .ok_or_else(|| err("Transaction files must contain UTF-8 Markdown"))?
                        .as_bytes())),
                ))
            })
            .collect::<Result<_>>()?,
    ))
}
pub fn files_at(bank: &Path) -> Result<Value> {
    let (physical, observed, sources) = contract::scan_sources(bank)?;
    let files = Value::Object(
        sources
            .into_iter()
            .map(|(path, raw)| {
                Ok((
                    path,
                    string(
                        String::from_utf8(raw)
                            .map_err(|_| ContractError::non_utf8("canonical source"))?,
                    ),
                ))
            })
            .collect::<Result<_>>()?,
    );
    let (after_physical, after, _) = contract::scan_sources(bank)?;
    if !same(&file_hashes(&files)?, &observed)
        || !same(&after, &observed)
        || !same(&after_physical, &physical)
    {
        return Err(err("Bank changed during transaction inventory"));
    }
    Ok(files)
}
pub fn proposal_value(prepared: &Value) -> Result<&Value> {
    let p = prepared.get("proposal").unwrap_or(prepared);
    limits::private_bytes(p)?;
    let fields = entries(p).map_err(|_| err("Invalid bank transaction proposal"))?;
    if !FIELDS.iter().all(|key| p.get(key).is_some())
        || fields
            .iter()
            .any(|(key, _)| !FIELDS.contains(&key.as_str()) && !EVIDENCE.contains(&key.as_str()))
        || fields.len() > FIELDS.len() + 1
        || get(p, "schema").as_str() != Some(SCHEMA)
        || !uuid_valid(get(p, "operation_id"))
        || !uuid_valid(get(p, "project_id"))
        || !get(p, "operation")
            .as_str()
            .is_some_and(|op| writes::OPERATIONS.contains(&op))
    {
        return Err(err("Invalid bank transaction proposal"));
    }
    let root = Path::new(text(p, "repo_root")?);
    if !root.is_absolute()
        || fs::canonical_path(root, true).map_or(true, |canonical| canonical != root)
    {
        return Err(err("Invalid bank transaction proposal"));
    }
    let op = text(p, "operation")?;
    for (key, allowed, message) in [
        (
            "initialization_evidence",
            &["initialize"] as &[&str],
            "Source initialization evidence belongs only to initialization",
        ),
        (
            "admission_evidence",
            &["capture"] as &[&str],
            "Admission evidence belongs only to its governed capture transaction",
        ),
        (
            "governance_evidence",
            &["correct", "supersede", "revoke", "forget"] as &[&str],
            "Governance evidence belongs only to its explicit lifecycle operation",
        ),
        (
            "consolidation_evidence",
            &["compact"] as &[&str],
            "Consolidation evidence belongs only to governed maintenance",
        ),
        (
            "refresh_evidence",
            &["refresh"] as &[&str],
            "Landed memory evidence belongs only to its source-bound refresh",
        ),
    ] {
        if p.get(key).is_some() && !allowed.contains(&op) {
            return Err(err(message));
        }
    }
    let checked = contract::validate_files(get(p, "files"), None)?;
    writes::scope_binding_provider()?
        .context_value_for_initialization(get(p, "scope_context"), first_bank(p))?;
    if op == "checkpoint" {
        return Err(err(
            "Checkpoint must use the single-file activeContext replacement",
        ));
    }
    let observed = get(p, "observed_hashes");
    if observed.as_object().is_none() || (!first_bank(p) && observed.get("MEMORY.md").is_none()) {
        return Err(err(
            "Transaction requires a complete observed existing bank",
        ));
    }
    if first_bank(p)
        && entries(get(p, "files"))?
            .iter()
            .any(|(path, _)| path != "MEMORY.md" && !contract::CONTEXTS.contains(&path.as_str()))
    {
        return Err(err(
            "First initialization creates context orientation only; optional records require governed admission",
        ));
    }
    for (path, hash) in entries(observed)? {
        if path != "MEMORY.md" {
            contract::path_value(&string(path))?;
        }
        if !hash.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err(err("Observed hashes must be SHA256 values"));
        }
    }
    if get(&checked, "project_id") != get(p, "project_id") && op != "bind" {
        return Err(err(
            "Project identity changes require an explicit binding operation",
        ));
    }
    if p.get("checkpoint_evidence").is_some() {
        writes::checkpoint_provider()?.validate_preparation(p, false)?;
    }
    Ok(p)
}
pub fn preview(repo: &Path, files: &Value, operation: &str) -> Result<Value> {
    preview_with_sources(repo, files, operation, Sources::default())
}
pub fn preview_with_sources(
    repo: &Path,
    files: &Value,
    operation: &str,
    sources: Sources,
) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    let (checked, hashes) = writes::snapshot(&root, None)?;
    let p = object([
        ("schema", string(SCHEMA)),
        ("operation_id", string((sources.uuid4)()?)),
        ("operation", string(operation)),
        ("repo_root", string(path_text(&root)?)),
        ("project_id", get(&checked, "project_id").clone()),
        ("observed_hashes", hashes),
        ("files", files.clone()),
        (
            "scope_context",
            writes::recovery_frontier_provider()?.capture_scope(&root)?,
        ),
    ]);
    proposal_value(&p)?;
    preflight(&bank, &p, None)?;
    result(
        "preview",
        &object([
            ("proposal", p),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ]),
    )
}
fn layout_info(bank: &Path, path: &str) -> Result<Option<fs::Identity>> {
    let target = bank.join(path);
    let parent = target.parent().ok_or_else(|| {
        err("Cannot inspect canonical target layout; preserve the existing layout")
    })?;
    let name = target.file_name().ok_or_else(|| {
        err("Cannot inspect canonical target layout; preserve the existing layout")
    })?;
    match DirectoryChain::open(parent) {
        Ok(chain) => {
            let info = match chain.directory().stat_at(name) {
                Ok(info) => Some(info),
                Err(e) if matches!(e.raw_os_error(), Some(libc::ENOENT | libc::ENOTDIR)) => None,
                Err(_) => {
                    return Err(err(
                        "Cannot inspect canonical target layout; preserve the existing layout",
                    ));
                }
            };
            chain.recheck().map_err(io_error)?;
            Ok(info)
        }
        Err(e)
            if e.raw_os_error() == Some(libc::ENOENT)
                || e.raw_os_error() == Some(libc::ENOTDIR) =>
        {
            Ok(None)
        }
        Err(_) => Err(err(
            "Cannot inspect canonical target layout; preserve the existing layout",
        )),
    }
}
pub fn check_layout(bank: &Path, participants: &BTreeSet<String>, observed: &Value) -> Result<()> {
    contract::bank_root(bank)?;
    let physical = contract::physical_paths(bank)?;
    contract::portable_aliases(physical.iter().chain(participants).map(String::as_str))?;
    for path in participants {
        if let Some(info) = layout_info(bank, path)? {
            if !info.is_regular() || observed.get(path).is_none() {
                return Err(err(
                    "Canonical target has an unobserved physical occupant; preserve it for inspection",
                ));
            }
        }
        let mut parent = Path::new(path).parent();
        while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
            if let Some(info) = layout_info(bank, path_text(path)?)? {
                if !info.is_directory() {
                    return Err(err(
                        "Canonical target has a non-directory parent; preserve the existing layout",
                    ));
                }
            }
            parent = path.parent();
        }
    }
    Ok(())
}
pub fn preflight(bank: &Path, p: &Value, observed: Option<&Value>) -> Result<()> {
    let paths = participants(p)?;
    contract::file_topology(paths.iter().map(String::as_str))?;
    check_layout(bank, &paths, observed.unwrap_or(get(p, "observed_hashes")))
}
pub fn locations(root: &Path, bank: &Path, operation_id: &str) -> Result<(PathBuf, PathBuf)> {
    locations_with_environment(root, bank, operation_id, &StateEnvironment::current())
}
pub fn locations_with_environment(
    root: &Path,
    bank: &Path,
    operation_id: &str,
    environment: &StateEnvironment,
) -> Result<(PathBuf, PathBuf)> {
    let (operations, receipt) = writes::state_paths_with(root, bank, operation_id, environment)?;
    let directory = operations
        .parent()
        .ok_or_else(|| err("Invalid transaction state directory"))?
        .join("transactions")
        .join(operation_id);
    let mut prefix = PathBuf::new();
    for part in directory.components() {
        prefix.push(part.as_os_str());
        match DirectoryChain::open(&prefix) {
            Ok(chain) => chain.recheck().map_err(io_error)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => {
                return Err(err(
                    "Transaction journal directories must be canonical directories",
                ));
            }
        }
    }
    let journal = directory.join("journal.json");
    if exists(&journal)? {
        let chain = DirectoryChain::open(&directory).map_err(io_error)?;
        if chain
            .directory()
            .stat_at(OsStr::new("journal.json"))
            .map_err(io_error)?
            .is_symlink()
        {
            return Err(err("Transaction journal must not be a symlink"));
        }
    }
    Ok((journal, receipt))
}
pub fn changed(p: &Value) -> Result<Vec<String>> {
    let final_hashes = file_hashes(get(p, "files"))?;
    Ok(participants(p)?
        .into_iter()
        .filter(|path| get(p, "observed_hashes").get(path) != final_hashes.get(path))
        .collect())
}
pub fn marker_bytes(p: &Value) -> Result<Vec<u8>> {
    encoded(&object([
        ("schema", string("agent-memory/transaction-marker-v1")),
        ("transaction_id", get(p, "operation_id").clone()),
        ("operation", get(p, "operation").clone()),
        ("phase", string("applying")),
        (
            "paths",
            Value::Array(changed(p)?.into_iter().map(string).collect()),
        ),
        ("base_hashes", get(p, "observed_hashes").clone()),
    ]))
}
pub fn flush_directory(path: &Path) -> Result<()> {
    let chain = DirectoryChain::open(path).map_err(io_error)?;
    chain.directory().fsync_directory().map_err(io_error)?;
    chain.recheck().map_err(io_error)
}
pub fn create_marker(path: &Path, payload: &[u8]) -> Result<()> {
    let chain = DirectoryChain::open(path.parent().ok_or_else(|| err("Marker has no parent"))?)
        .map_err(io_error)?;
    let mut file = chain
        .directory()
        .create_exclusive(
            path.file_name().ok_or_else(|| err("Marker has no name"))?,
            libc::O_WRONLY,
            0o600,
        )
        .map_err(io_error)?;
    file.write_all(payload).map_err(io_error)?;
    file.fsync().map_err(io_error)?;
    chain.directory().fsync_directory().map_err(io_error)?;
    chain.recheck().map_err(io_error)
}
pub fn check_marker(bank: &Path, expected: &[u8]) -> Result<()> {
    if read(&bank.join(MARKER), MAX_MARKER_BYTES).map_or(true, |raw| raw != expected) {
        return Err(err(
            "Transaction marker changed or is unknown; preserve for inspection",
        ));
    }
    Ok(())
}
pub fn check_observed(bank: &Path, expected: &Value) -> Result<()> {
    if !same(&writes::hashes(bank)?, expected) {
        return Err(err(
            "External edit observed; preserve it and inspect the interrupted transaction",
        ));
    }
    Ok(())
}
fn canonical_parents(bank: &Path, relative: &Path) -> Result<()> {
    let mut current = bank.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(err("Invalid canonical parent"));
        };
        let chain = DirectoryChain::open(&current).map_err(io_error)?;
        match chain.directory().stat_at(name) {
            Ok(info) if info.is_directory() => (),
            Ok(_) => {
                return Err(err(
                    "Canonical target has a non-directory parent; preserve the existing layout",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                chain
                    .directory()
                    .create_directory(name, 0o755)
                    .map_err(io_error)?;
            }
            Err(e) => return Err(io_error(e)),
        }
        chain.recheck().map_err(io_error)?;
        current.push(name);
    }
    Ok(())
}
struct Temporary<'a> {
    directory: &'a fs::Descriptor,
    name: OsString,
    identity: fs::Identity,
    present: bool,
}
impl Temporary<'_> {
    fn finish(&mut self) -> Result<()> {
        if self.present {
            if !self
                .directory
                .stat_at(&self.name)
                .map_err(io_error)?
                .same_inode(self.identity)
            {
                self.present = false;
                return Err(err(
                    "Transaction temporary changed; preserve it for inspection",
                ));
            }
            self.directory.unlink_at(&self.name).map_err(io_error)?;
            self.present = false;
        }
        Ok(())
    }
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
/// Full read-set recheck immediately before each rename/unlink. Parents stay pinned.
#[allow(clippy::too_many_arguments)]
pub fn replace_file(
    bank: &Path,
    path: &str,
    body: Option<&str>,
    expected: &Value,
    marker: &[u8],
    participants: &BTreeSet<String>,
    mode_bits: Option<u32>,
    before_replace: Option<&dyn Fn() -> Result<()>>,
) -> Result<()> {
    if path != "MEMORY.md" {
        contract::path_value(&string(path))?;
    }
    if mode_bits.is_some_and(|mode| mode > 0o777) {
        return Err(err(
            "Explicit replacement modes must be ordinary permission bits",
        ));
    }
    check_marker(bank, marker)?;
    check_observed(bank, expected)?;
    check_layout(bank, participants, expected)?;
    let relative = Path::new(path);
    if body.is_some() {
        if let Some(parent) = relative.parent() {
            canonical_parents(bank, parent)?;
        }
    }
    let target = bank.join(relative);
    let chain = DirectoryChain::open(target.parent().ok_or_else(|| err("Target has no parent"))?)
        .map_err(io_error)?;
    let directory = chain.directory();
    let name = target
        .file_name()
        .ok_or_else(|| err("Target has no filename"))?;
    let before = match directory.stat_at(name) {
        Ok(info) => Some(info),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(e)),
    };
    let mut temporary = None;
    if let Some(body) = body {
        let mode = mode_bits.unwrap_or_else(|| before.map_or(0o644, |info| info.mode & 0o777));
        let temp_name = OsString::from(format!(".agent-memory-transaction-{}.tmp", host::uuid4()?));
        let mut file = directory
            .create_exclusive(&temp_name, libc::O_WRONLY, mode)
            .map_err(io_error)?;
        temporary = Some(Temporary {
            directory,
            name: temp_name,
            identity: file.stat().map_err(io_error)?,
            present: true,
        });
        file.write_all(body.as_bytes()).map_err(io_error)?;
        file.fsync().map_err(io_error)?;
    }
    writes::fault(&format!("transaction-before-recheck:{path}"))?;
    if let Some(check) = before_replace {
        check()?;
    }
    check_marker(bank, marker)?;
    check_observed(bank, expected)?;
    check_layout(bank, participants, expected)?;
    chain.recheck().map_err(io_error)?;
    let now = match directory.stat_at(name) {
        Ok(info) => Some(info),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(e)),
    };
    if now != before {
        return Err(err(
            "External edit observed; preserve it and inspect the interrupted transaction",
        ));
    }
    if let Some(temp) = temporary.as_mut() {
        if !directory
            .stat_at(&temp.name)
            .map_err(io_error)?
            .same_inode(temp.identity)
        {
            return Err(err(
                "Transaction temporary changed; preserve it for inspection",
            ));
        }
        fs::rename_at(
            directory,
            &temp.name,
            directory,
            name,
            fs::RenameMode::Replace,
        )
        .map_err(io_error)?;
        temp.present = false;
    } else {
        directory.unlink_at(name).map_err(io_error)?;
    }
    directory.fsync_directory().map_err(io_error)?;
    chain.recheck().map_err(io_error)
}
pub fn cleanup(journal: &Path, raw: &[u8]) -> Result<()> {
    if !exists(journal)? {
        return Ok(());
    }
    let parent = journal
        .parent()
        .ok_or_else(|| err("Journal has no parent"))?;
    let name = journal
        .file_name()
        .ok_or_else(|| err("Journal has no filename"))?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    let before = chain.directory().stat_at(name).map_err(io_error)?;
    if read(journal, MAX_JOURNAL_BYTES).map_or(true, |bytes| bytes != raw)
        || chain.directory().stat_at(name).map_err(io_error)? != before
    {
        return Err(err(
            "Recovery journal changed; do not discard unknown recovery data",
        ));
    }
    chain.recheck().map_err(io_error)?;
    chain.directory().unlink_at(name).map_err(io_error)?;
    chain.directory().fsync_directory().map_err(io_error)?;
    if chain
        .directory()
        .entries_bounded(0)
        .map_err(io_error)?
        .is_some()
    {
        let outer = DirectoryChain::open(
            parent
                .parent()
                .ok_or_else(|| err("Journal directory has no parent"))?,
        )
        .map_err(io_error)?;
        let dirname = parent
            .file_name()
            .ok_or_else(|| err("Journal directory has no name"))?;
        chain.recheck().map_err(io_error)?;
        outer
            .directory()
            .remove_directory_at(dirname)
            .map_err(io_error)?;
    }
    Ok(())
}
pub fn receipt_value(path: &Path, p: &Value) -> Result<Option<Value>> {
    if !exists(path)? {
        return Ok(None);
    }
    reject_symlink(path, "Transaction receipt must not be a symlink")?;
    let r = load_with_limit(&read(path, MAX_RECEIPT_BYTES)?, MAX_RECEIPT_BYTES)?;
    let final_hashes = file_hashes(get(p, "files"))?;
    let root = Path::new(text(p, "repo_root")?);
    let expected_paths = changed_memory_paths(root, p)?;
    let outcome = get(&r, "outcome").as_str();
    if !exact(&r, RECEIPT_FIELDS)
        || get(&r, "schema").as_str() != Some("agent-memory/transaction-receipt-v1")
        || get(&r, "operation_id") != get(p, "operation_id")
        || get(&r, "request_sha256").as_str() != Some(sha(&encoded(p)?).as_str())
        || get(&r, "bank_path").as_str() != Some(path_text(&repository_config::bank_path(root)?)?)
        || get(&r, "project_id") != get(p, "project_id")
        || !matches!(outcome, Some("finished" | "rolled-back"))
        || get(&r, "verified").as_bool() != Some(true)
        || !same(get(&r, "observed_hashes"), get(p, "observed_hashes"))
        || !same(
            get(&r, "resulting_hashes"),
            if outcome == Some("finished") {
                &final_hashes
            } else {
                get(p, "observed_hashes")
            },
        )
        || get(&r, "changed_memory_paths") != &expected_paths
    {
        return Err(err("Unknown receipt or reused operation ID; preserve it"));
    }
    crate::lifecycle::time_value(get(&r, "readback_at"))?;
    Ok(Some(r))
}
fn changed_memory_paths(root: &Path, p: &Value) -> Result<Value> {
    let relative = repository_config::relative(root)?;
    Ok(Value::Array(
        changed(p)?
            .into_iter()
            .map(|path| string(format!("{relative}/{path}")))
            .collect(),
    ))
}
pub fn admission_recovery(p: &Value, mode: &str, known: Option<&Value>) -> Result<()> {
    if [
        "admission_evidence",
        "consolidation_evidence",
        "governance_evidence",
        "refresh_evidence",
    ]
    .iter()
    .any(|key| p.get(key).is_some())
        && mode == "finish"
        && known.is_none_or(|known| get(known, "outcome").as_str() != Some("finished"))
    {
        return Err(err(
            "Unfinished governed learning requires rollback, then fresh evidence and comparison; preserve the journal",
        ));
    }
    Ok(())
}
pub fn journal_value(
    raw: &[u8],
    root: &Path,
    bank: &Path,
    operation_id: &str,
    require_marker: bool,
) -> Result<(Value, Value)> {
    let j = load_with_limit(raw, MAX_JOURNAL_BYTES)?;
    if !exact(&j, &["schema", "proposal", "base_files"])
        || get(&j, "schema").as_str() != Some("agent-memory/transaction-journal-v1")
    {
        return Err(err(
            "Unknown transaction journal; operator inspection required",
        ));
    }
    let p = proposal_value(get(&j, "proposal"))?.clone();
    let pinned = get(&p, "scope_context");
    if pinned != &Value::Null {
        let settings = repository_config::settings(root)?;
        let settings = object([
            ("bank_root", string(settings.bank_root)),
            (
                "authority_branch",
                settings.authority_branch.map_or(Value::Null, string),
            ),
        ]);
        if get(pinned, "repository_settings_sha256").as_str()
            != Some(sha(&encoded(&settings)?).as_str())
        {
            return Err(err(
                "Recovery configuration differs from the original bank operation; restore it before recovery",
            ));
        }
    }
    let base = get(&j, "base_files");
    let empty_base = first_bank(&p) && base.as_object().is_some_and(|map| map.is_empty());
    let checked = if empty_base {
        object([("project_id", get(&p, "project_id").clone())])
    } else {
        contract::validate_files(base, None)?
    };
    if text(&p, "repo_root")? != path_text(root)?
        || text(&p, "operation_id")? != operation_id
        || !same(&file_hashes(base)?, get(&p, "observed_hashes"))
        || get(&checked, "project_id") != get(&p, "project_id")
    {
        return Err(err(
            "Recovery journal does not match the original bank/operation",
        ));
    }
    if require_marker {
        check_marker(bank, &marker_bytes(&p)?)?;
    }
    Ok((j, p))
}
pub fn allowed_partial(p: &Value, observed: &Value) -> Result<()> {
    let base = get(p, "observed_hashes");
    let final_hashes = file_hashes(get(p, "files"))?;
    let mut paths = participants(p)?;
    paths.extend(keys(observed)?);
    if paths.iter().any(|path| {
        observed.get(path) != base.get(path) && observed.get(path) != final_hashes.get(path)
    }) {
        return Err(err(
            "Interrupted bank contains an external edit; automatic recovery would overwrite it",
        ));
    }
    Ok(())
}
pub fn invalidate_optional_indexes(root: &Path, hashes: &Value) -> Value {
    INDEX
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .and_then(|provider| provider.invalidate_after_change(root, hashes).ok())
        .unwrap_or_else(|| string("deferred-to-canonical-fallback"))
}
pub fn validate_outcome(
    root: &Path,
    bank: &Path,
    desired: &Value,
    p: &Value,
    owner: &[u8],
    marker: &[u8],
    admission_sources: bool,
) -> Result<Value> {
    if admission_sources && same(desired, get(p, "files")) {
        if p.get("checkpoint_evidence").is_some() {
            writes::checkpoint_provider()?.validate_preparation(p, true)?;
        }
        if [
            "admission_evidence",
            "governance_evidence",
            "consolidation_evidence",
            "refresh_evidence",
        ]
        .iter()
        .any(|key| p.get(key).is_some())
        {
            validator(p)?.check_sources(p, owner, marker)?;
        }
    }
    if desired.as_object().is_some_and(|map| map.is_empty()) && first_bank(p) {
        if !contract::physical_paths(bank)?.is_empty()
            || !entries(&writes::hashes(bank)?)?.is_empty()
        {
            return Err(err(
                "Uninitialized rollback has an unexpected occupant; preserve it",
            ));
        }
        return Ok(object([("project_id", get(p, "project_id").clone())]));
    }
    contract::validate(root, Some(owner), Some(marker))
}
#[allow(clippy::too_many_arguments)]
pub fn converge(
    root: &Path,
    bank: &Path,
    p: &Value,
    base_files: &Value,
    journal: &Path,
    raw: &[u8],
    receipt_path: &Path,
    mode: &str,
    observed: &Value,
    owner: &[u8],
    recovery: bool,
    sources: Sources,
) -> Result<Value> {
    if !["finish", "rollback"].contains(&mode) {
        return Err(err("Invalid transaction recovery proposal"));
    }
    let marker = marker_bytes(p)?;
    let mut known = receipt_value(receipt_path, p)?;
    if recovery {
        admission_recovery(p, mode, known.as_ref())?;
    }
    let historical = recovery && known.is_some();
    let outcome = if mode == "finish" {
        "finished"
    } else {
        "rolled-back"
    };
    if known
        .as_ref()
        .is_some_and(|r| get(r, "outcome").as_str() != Some(outcome))
    {
        return Err(err(
            "Transaction already has a different verified outcome; use a new reviewed operation",
        ));
    }
    let desired = if mode == "finish" {
        get(p, "files")
    } else {
        base_files
    };
    let expected = file_hashes(desired)?;
    let mut running = observed.clone();
    let participants = participants(p)?;
    let mut paths: Vec<_> = keys(&running)?
        .union(&keys(&expected)?)
        .filter(|path| running.get(path.as_str()) != expected.get(path.as_str()))
        .cloned()
        .collect();
    paths.sort_by(|a, b| (a == "MEMORY.md", a).cmp(&(b == "MEMORY.md", b)));
    if known.is_some() && !paths.is_empty() {
        return Err(err(
            "Bank differs from a recorded outcome; do not replay over newer edits",
        ));
    }
    for path in &paths {
        replace_file(
            bank,
            path,
            desired.get(path).and_then(Value::as_str),
            &running,
            &marker,
            &participants,
            None,
            None,
        )?;
        if let Some(hash) = expected.get(path) {
            put(&mut running, path, hash.clone())?;
        } else {
            remove(&mut running, path);
        }
        writes::fault(&format!("transaction-after-write:{path}"))?;
    }
    let recheck = || -> Result<Value> {
        check_marker(bank, &marker)?;
        check_observed(bank, &expected)?;
        check_layout(bank, &participants, &expected)?;
        validate_outcome(root, bank, desired, p, owner, &marker, !historical)
    };
    let checked = recheck()?;
    writes::fault("transaction-before-receipt")?;
    recheck()?;
    if known.is_none() {
        let receipt = object([
            ("schema", string("agent-memory/transaction-receipt-v1")),
            ("operation_id", get(p, "operation_id").clone()),
            ("request_sha256", string(sha(&encoded(p)?))),
            ("bank_path", string(path_text(bank)?)),
            ("project_id", get(&checked, "project_id").clone()),
            ("observed_hashes", get(p, "observed_hashes").clone()),
            ("resulting_hashes", expected.clone()),
            ("outcome", string(outcome)),
            ("changed_memory_paths", changed_memory_paths(root, p)?),
            ("readback_at", string((sources.utc_now)()?)),
            ("verified", Value::Bool(true)),
        ]);
        writes::record(receipt_path, &limits::private_bytes(&receipt)?)?;
        known = Some(receipt);
    }
    writes::fault("transaction-after-receipt")?;
    recheck()?;
    let chain = DirectoryChain::open(bank).map_err(io_error)?;
    check_marker(bank, &marker)?;
    chain.recheck().map_err(io_error)?;
    chain
        .directory()
        .unlink_at(OsStr::new(MARKER))
        .map_err(io_error)?;
    chain.directory().fsync_directory().map_err(io_error)?;
    writes::fault("transaction-after-marker-removal")?;
    cleanup(journal, raw)?;
    let known = known.ok_or_else(|| err("Transaction has no verified receipt"))?;
    let mut fields = Value::Object(
        entries(&known)?
            .iter()
            .filter(|(k, _)| k != "schema")
            .cloned()
            .collect(),
    );
    contract::merge(
        &mut fields,
        &object([
            ("writes_performed", Value::Bool(!paths.is_empty())),
            ("already_applied", Value::Bool(paths.is_empty())),
            ("still_current", Value::Bool(true)),
            ("receipt_path", string(path_text(receipt_path)?)),
            ("interrupted", Value::Bool(false)),
            (
                "derived_cache_state",
                invalidate_optional_indexes(root, &expected),
            ),
        ]),
    )?;
    result(mode, &fields)
}
/// Trusted callers provide their full scope/security guard; not a public CLI bypass.
pub fn apply_with_identity(
    prepared: &Value,
    check_identity: impl Fn(&[u8]) -> Result<()>,
) -> Result<Value> {
    apply_with_identity_and_environment(
        prepared,
        check_identity,
        &StateEnvironment::current(),
        Sources::default(),
    )
}
pub fn apply_with_identity_and_environment(
    prepared: &Value,
    check_identity: impl Fn(&[u8]) -> Result<()>,
    environment: &StateEnvironment,
    sources: Sources,
) -> Result<Value> {
    let p = proposal_value(prepared)?;
    let (root, bank) = writes::bank_path(Path::new(text(p, "repo_root")?))?;
    let (journal, receipt_path) =
        locations_with_environment(&root, &bank, text(p, "operation_id")?, environment)?;
    let mut barrier = writes::turn_sources_provider()?.canonical_transaction_barrier(
        &root,
        if text(p, "operation")? == "initialize" {
            Some(&bank)
        } else {
            None
        },
    )?;
    let outcome = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        text(p, "operation_id")?,
        &format!("bank-{}", text(p, "operation")?),
        sources,
        |owner| {
            writes::fault("transaction-locked-before-inventory")?;
            check_identity(owner)?;
            let known = receipt_value(&receipt_path, p)?;
            let (checked, observed) = if first_bank(p) && !exists(&bank.join("MEMORY.md"))? {
                if exists(&bank.join(MARKER))? || !contract::physical_paths(&bank)?.is_empty() {
                    return Err(err(
                        "Empty-bank initialization found existing or interrupted content; preserve it",
                    ));
                }
                (
                    object([("project_id", get(p, "project_id").clone())]),
                    writes::hashes(&bank)?,
                )
            } else {
                writes::snapshot(&root, Some(owner))?
            };
            if let Some(known) = known {
                if exists(&journal)? {
                    let raw = read(&journal, MAX_JOURNAL_BYTES)?;
                    let (_, recorded) =
                        journal_value(&raw, &root, &bank, text(p, "operation_id")?, false)?;
                    if !same(&recorded, p) {
                        return Err(err("Unknown leftover journal; preserve it"));
                    }
                    cleanup(&journal, &raw)?;
                }
                let mut fields = Value::Object(
                    entries(&known)?
                        .iter()
                        .filter(|(k, _)| k != "schema")
                        .cloned()
                        .collect(),
                );
                contract::merge(
                    &mut fields,
                    &object([
                        ("writes_performed", Value::Bool(false)),
                        ("already_applied", Value::Bool(true)),
                        (
                            "still_current",
                            Value::Bool(same(&observed, get(&known, "resulting_hashes"))),
                        ),
                        ("receipt_path", string(path_text(&receipt_path)?)),
                        (
                            "derived_cache_state",
                            invalidate_optional_indexes(&root, &observed),
                        ),
                    ]),
                )?;
                return result("apply", &fields);
            }
            if get(&checked, "project_id") != get(p, "project_id")
                || !same(&observed, get(p, "observed_hashes"))
            {
                return Err(err(
                    "Bank changed after preview; prepare a fresh transaction",
                ));
            }
            let base_files = files_at(&bank)?;
            if !first_bank(p) {
                contract::validate_files(&base_files, None)?;
            }
            if !same(&file_hashes(&base_files)?, &observed) {
                return Err(err("Bank changed before journaling"));
            }
            preflight(&bank, p, None)?;
            let raw = limits::private_bytes(&object([
                ("schema", string("agent-memory/transaction-journal-v1")),
                ("proposal", p.clone()),
                ("base_files", base_files.clone()),
            ]))?;
            storage::private_mkdir(
                journal
                    .parent()
                    .ok_or_else(|| err("Journal has no parent"))?,
            )?;
            if exists(&journal)? {
                if read(&journal, MAX_JOURNAL_BYTES)? != raw {
                    return Err(err("Unknown journal or reused transaction ID; preserve it"));
                }
            } else {
                storage::replace(&journal, &raw, None)?;
            }
            writes::fault("transaction-before-marker")?;
            check_observed(&bank, &observed)?;
            create_marker(&bank.join(MARKER), &marker_bytes(p)?)?;
            writes::fault("transaction-after-marker")?;
            converge(
                &root,
                &bank,
                p,
                &base_files,
                &journal,
                &raw,
                &receipt_path,
                "finish",
                &observed,
                owner,
                false,
                sources,
            )
        },
    );
    if outcome.is_ok() {
        barrier.finish()?;
    }
    outcome
}
fn changed_files(p: &Value) -> Result<Value> {
    Ok(Value::Object(
        changed(p)?
            .into_iter()
            .filter_map(|path| get(p, "files").get(&path).map(|body| (path, body.clone())))
            .collect(),
    ))
}
pub fn source_classes(p: &Value) -> Result<Vec<String>> {
    let nested = |value: &Value, path: &[&str]| -> Result<String> {
        let mut v = value;
        for key in path {
            v = v
                .get(key)
                .ok_or_else(|| err("Invalid transaction source attribution"))?;
        }
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| err("Invalid transaction source attribution"))
    };
    if let Some(e) = p.get("admission_evidence") {
        return Ok(vec![nested(e, &["capture", "request", "source_class"])?]);
    }
    if let Some(e) = p.get("governance_evidence") {
        return Ok(vec![nested(e, &["request", "authority", "source_class"])?]);
    }
    if let Some(e) = p
        .get("consolidation_evidence")
        .filter(|e| get(e, "capture") != &Value::Null)
    {
        return Ok(vec![nested(e, &["capture", "request", "source_class"])?]);
    }
    if text(p, "operation")? == "initialize" {
        let inventory = p
            .get("initialization_evidence")
            .and_then(|v| v.get("inventory"))
            .ok_or_else(|| err("Invalid transaction source attribution"))?;
        let sources = get(inventory, "sources")
            .as_array()
            .ok_or_else(|| err("Invalid transaction source attribution"))?;
        let awa = get(get(inventory, "awa"), "sources")
            .as_array()
            .ok_or_else(|| err("Invalid transaction source attribution"))?;
        let mut classes = vec!["assistant".to_owned()];
        if !sources.is_empty() {
            classes.push("verified-repository".to_owned());
        }
        if !awa.is_empty() {
            classes.push("tool".to_owned());
        }
        return Ok(classes);
    }
    Ok(vec![
        if p.get("checkpoint_evidence").is_some() {
            "assistant"
        } else {
            "external"
        }
        .to_owned(),
    ])
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = proposal_value(prepared)?;
    let root = Path::new(text(p, "repo_root")?);
    let operation = text(p, "operation")?;
    if operation != "initialize"
        && get(
            &writes::recovery_frontier_provider()?.observe(root)?,
            "hold_sha256",
        ) != &Value::Null
    {
        return writes::recovery_transactions_provider()?.apply_transaction(p);
    }
    if operation == "bind" {
        return Err(err(
            "Identity changes require scope_binding preview/apply, not an ordinary bank transaction",
        ));
    }
    if get(p, "scope_context") == &Value::Null {
        return Err(err(
            "Proposal has no resolved scope; finish identity setup and preview again",
        ));
    }
    let (_, bank) = writes::bank_path(root)?;
    let (_, receipt_path) = locations(root, &bank, text(p, "operation_id")?)?;
    let validate = |owner: Option<&[u8]>| -> Result<()> {
        if operation == "initialize" || !exists(&receipt_path)? {
            validate_operation(p, owner)?;
        }
        if p.get("checkpoint_evidence").is_some() {
            writes::checkpoint_provider()?.validate_preparation(p, !exists(&receipt_path)?)?;
        }
        Ok(())
    };
    validate(None)?;
    let initializing = first_bank(p) && !exists(&bank.join("MEMORY.md"))?;
    let mut pinned = get(p, "scope_context").clone();
    put(
        &mut pinned,
        "initialization_only",
        Value::Bool(initializing),
    )?;
    let scope = writes::scope_binding_provider()?;
    let context = scope.require_transaction(
        root,
        get(p, "project_id"),
        Some(&pinned),
        None,
        None,
        initializing,
    )?;
    let project = PROJECT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| err("project_identity provider is not registered"))?;
    let router = text(get(p, "files"), "MEMORY.md")?;
    let (metadata, _, _) = contract::parse_router(router)?;
    if get(&metadata, "canonical-remote")
        != get(get(&project.status(root)?, "evidence"), "canonical_remote")
    {
        return Err(err(
            "Authority identity changes require explicit scope binding, including during initialization",
        ));
    }
    let security = writes::security_provider()?;
    let classes = source_classes(p)?;
    let classes: Vec<_> = classes.iter().map(String::as_str).collect();
    let check = |owner: &[u8]| -> Result<()> {
        scope.require_transaction(
            root,
            get(p, "project_id"),
            Some(&context),
            Some(owner),
            None,
            initializing,
        )?;
        validate(Some(owner))?;
        if governed(p) || p.get("checkpoint_evidence").is_some() {
            let complete_refresh = get(get(get(p, "refresh_evidence"), "proof"), "schema").as_str()
                == Some("agent-memory/semantic-sharing-proof-v2");
            security.guarded_start_transaction(
                root,
                text(p, "operation_id")?,
                &context,
                &changed_files(p)?,
                &classes,
                if p.get("checkpoint_evidence").is_some() {
                    "checkpoint"
                } else {
                    operation
                },
                complete_refresh,
            )?;
        } else {
            security.audit(
                root,
                text(p, "operation_id")?,
                "mutation",
                "started",
                &context,
                &classes,
                operation,
            )?;
        }
        Ok(())
    };
    let mut receipt = apply_with_identity(p, check)?;
    if get(&receipt, "outcome").as_str() == Some("finished") {
        put(
            &mut receipt,
            "security_audit",
            security.audit(
                root,
                text(p, "operation_id")?,
                "mutation",
                "completed",
                &context,
                &classes,
                operation,
            )?,
        )?;
    }
    put(&mut receipt, "resolved_scope", context)?;
    Ok(receipt)
}
/// Mechanical recovery with the caller's scope guard, shared with scope_binding.
pub fn recover_with_identity(
    repo: &Path,
    mode: Option<&str>,
    prepared: Option<&Value>,
    check_identity: Option<&dyn Fn(&[u8], &[u8]) -> Result<()>>,
    initializing: bool,
) -> Result<Value> {
    recover_with_identity_and_environment(
        repo,
        mode,
        prepared,
        check_identity,
        initializing,
        &StateEnvironment::current(),
        Sources::default(),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn recover_with_identity_and_environment(
    repo: &Path,
    mode: Option<&str>,
    prepared: Option<&Value>,
    check_identity: Option<&dyn Fn(&[u8], &[u8]) -> Result<()>>,
    initializing: bool,
    environment: &StateEnvironment,
    sources: Sources,
) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    if let Some(prepared) = prepared {
        let r = prepared.get("proposal").unwrap_or(prepared);
        if !exact(r, RECOVERY_FIELDS)
            || get(r, "schema").as_str() != Some(RECOVERY_SCHEMA)
            || !uuid_valid(get(r, "operation_id"))
            || get(r, "repo_root").as_str() != Some(path_text(&root)?)
            || !matches!(get(r, "mode").as_str(), Some("finish" | "rollback"))
        {
            return Err(err("Invalid transaction recovery proposal"));
        }
        let mut barrier = writes::turn_sources_provider()?
            .canonical_transaction_barrier(&root, if initializing { Some(&bank) } else { None })?;
        let outcome = storage::exclusive(
            &bank.join(".agent-memory.lock"),
            text(r, "operation_id")?,
            "bank-recover",
            sources,
            |owner| {
                let (journal, receipt_path) = locations_with_environment(
                    &root,
                    &bank,
                    text(r, "operation_id")?,
                    environment,
                )?;
                let raw = read(&journal, MAX_JOURNAL_BYTES)?;
                if get(r, "journal_sha256").as_str() != Some(sha(&raw).as_str())
                    || get(r, "marker_sha256").as_str()
                        != Some(sha(&read(&bank.join(MARKER), MAX_MARKER_BYTES)?).as_str())
                {
                    return Err(err(
                        "Recovery evidence changed after preview; inspect again",
                    ));
                }
                let (j, p) = journal_value(&raw, &root, &bank, text(r, "operation_id")?, true)?;
                let check =
                    check_identity.ok_or_else(|| err("Recovery writer requires resolved scope"))?;
                writes::fault("recovery-locked-before-evidence")?;
                check(owner, &marker_bytes(&p)?)?;
                let observed = writes::hashes(&bank)?;
                if !same(&observed, get(r, "observed_hashes")) {
                    return Err(err(
                        "Bank changed after recovery preview; preserve the edit",
                    ));
                }
                allowed_partial(&p, &observed)?;
                preflight(&bank, &p, Some(&observed))?;
                converge(
                    &root,
                    &bank,
                    &p,
                    get(&j, "base_files"),
                    &journal,
                    &raw,
                    &receipt_path,
                    text(r, "mode")?,
                    &observed,
                    owner,
                    true,
                    sources,
                )
            },
        );
        if outcome.is_ok() {
            barrier.finish()?;
        }
        return outcome;
    }
    let mode = mode
        .filter(|mode| ["finish", "rollback"].contains(mode))
        .ok_or_else(|| err("Recovery preview requires an explicit finish or rollback choice"))?;
    let lock = bank.join(".agent-memory.lock");
    if exists(&lock)? {
        return Err(err("Bank is locked; inspect/recover the bank lock first"));
    }
    let marker = bank.join(MARKER);
    reject_symlink(&marker, "Unknown transaction marker; preserve it")?;
    let mraw = read(&marker, MAX_MARKER_BYTES)?;
    let m = load_with_limit(&mraw, MAX_MARKER_BYTES)?;
    if m.as_object().is_none() || !uuid_valid(get(&m, "transaction_id")) {
        return Err(err(
            "Unknown transaction marker; operator inspection required",
        ));
    }
    let operation_id = text(&m, "transaction_id")?;
    let (journal, receipt_path) =
        locations_with_environment(&root, &bank, operation_id, environment)?;
    let raw = read(&journal, MAX_JOURNAL_BYTES)?;
    let (_, p) = journal_value(&raw, &root, &bank, operation_id, true)?;
    let known = receipt_value(&receipt_path, &p)?;
    admission_recovery(&p, mode, known.as_ref())?;
    if mode == "finish" && p.get("checkpoint_evidence").is_some() {
        writes::checkpoint_provider()?.validate_preparation(&p, known.is_none())?;
    }
    if mode == "finish" && text(&p, "operation")? == "initialize" {
        validator(&p)?.validate_transaction(&p, None)?;
    }
    let observed = writes::hashes(&bank)?;
    allowed_partial(&p, &observed)?;
    preflight(&bank, &p, Some(&observed))?;
    if known.as_ref().is_some_and(|r| {
        get(r, "outcome").as_str()
            != Some(if mode == "finish" {
                "finished"
            } else {
                "rolled-back"
            })
    }) {
        return Err(err("Transaction already has a different verified outcome"));
    }
    if exists(&lock)?
        || read(&marker, MAX_MARKER_BYTES)? != mraw
        || read(&journal, MAX_JOURNAL_BYTES)? != raw
        || !same(&writes::hashes(&bank)?, &observed)
    {
        return Err(err("Recovery evidence changed during inspection"));
    }
    result(
        "recover-preview",
        &object([
            (
                "proposal",
                object([
                    ("schema", string(RECOVERY_SCHEMA)),
                    ("operation_id", get(&p, "operation_id").clone()),
                    ("repo_root", string(path_text(&root)?)),
                    ("mode", string(mode)),
                    ("journal_sha256", string(sha(&raw))),
                    ("marker_sha256", string(sha(&mraw))),
                    ("observed_hashes", observed),
                ]),
            ),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
            ("interrupted", Value::Bool(true)),
        ]),
    )
}
pub fn recover(repo: &Path, mode: Option<&str>, prepared: Option<&Value>) -> Result<Value> {
    let Some(prepared) = prepared else {
        return recover_with_identity(repo, mode, None, None, false);
    };
    let (root, bank) = writes::bank_path(repo)?;
    let r = prepared.get("proposal").unwrap_or(prepared);
    if !uuid_valid(get(r, "operation_id")) {
        return Err(err("Invalid transaction recovery operation"));
    }
    let (journal, receipt_path) = locations(&root, &bank, text(r, "operation_id")?)?;
    let (_, p) = journal_value(
        &read(&journal, MAX_JOURNAL_BYTES)?,
        &root,
        &bank,
        text(r, "operation_id")?,
        true,
    )?;
    let mode = text(r, "mode")?;
    admission_recovery(&p, mode, receipt_value(&receipt_path, &p)?.as_ref())?;
    if text(&p, "operation")? == "bind" {
        return Err(err(
            "Resume identity binding through its original scope_binding bundle",
        ));
    }
    let marker = marker_bytes(&p)?;
    let initializing = text(&p, "operation")? == "initialize" && !exists(&bank.join("MEMORY.md"))?;
    let scope = writes::scope_binding_provider()?;
    let context = scope.require_transaction(
        &root,
        get(&p, "project_id"),
        None,
        None,
        Some(&marker),
        initializing,
    )?;
    let security = writes::security_provider()?;
    let action = format!("recover-{mode}");
    let check = |owner: &[u8], marker: &[u8]| -> Result<()> {
        scope.require_transaction(
            &root,
            get(&p, "project_id"),
            Some(&context),
            Some(owner),
            Some(marker),
            initializing,
        )?;
        if mode == "finish"
            && (p.get("checkpoint_evidence").is_some() || text(&p, "operation")? == "initialize")
        {
            if p.get("checkpoint_evidence").is_some() {
                writes::checkpoint_provider()?
                    .validate_preparation(&p, receipt_value(&receipt_path, &p)?.is_none())?;
            } else {
                validator(&p)?.validate_transaction(&p, None)?;
            }
            security.guarded_start_transaction(
                &root,
                text(&p, "operation_id")?,
                &context,
                &changed_files(&p)?,
                &["tool"],
                &action,
                false,
            )?;
        } else {
            security.audit(
                &root,
                text(&p, "operation_id")?,
                "mutation",
                "started",
                &context,
                &["tool"],
                &action,
            )?;
        }
        Ok(())
    };
    let mut receipt = recover_with_identity(
        &root,
        None,
        Some(prepared),
        Some(&check),
        text(&p, "operation")? == "initialize",
    )?;
    put(
        &mut receipt,
        "security_audit",
        security.audit(
            &root,
            text(&p, "operation_id")?,
            "mutation",
            "completed",
            &context,
            &["tool"],
            &action,
        )?,
    )?;
    put(&mut receipt, "resolved_scope", context)?;
    Ok(receipt)
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(value) => (value, 0, crate::codec::json::INDENT2_UTF8),
        Err(error) => {
            let mut fields = object([
                ("error", string(error.to_string())),
                ("verified", Value::Bool(false)),
                ("inspection_required", Value::Bool(true)),
            ]);
            let _ = contract::merge(&mut fields, &crate::bank_capacity::details(&error));
            (
                result(operation, &fields).unwrap_or(fields),
                1,
                crate::codec::json::DEFAULT,
            )
        }
    };
    let stdout =
        crate::codec::json::encode(&value, style).unwrap_or_else(|error| error.to_string()) + "\n";
    crate::lifecycle::CommandResult {
        stdout,
        stderr: String::new(),
        exit_code,
        value: Some(value),
    }
}
