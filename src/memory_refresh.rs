//! Reconcile landed repository memory without importing feature code or changing its index.
use crate::bank_contract as bank;
use crate::bank_transactions as tx;
use crate::bank_writes as writes;
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, load_with_limit, sha256_hex, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, Descriptor, DirectoryChain, Identity};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::{
    host, records, retrieval, scope_binding, sharing_boundary, sharing_holds,
    sharing_limits as limits, sharing_recovery as recovery, trust_paths,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

pub const LIMIT: usize = 4 * 1048576;
pub const MAX_INDEX_BYTES: usize = limits::MAX_PRIVATE;
pub const CURSOR: &str = "agent-memory/refresh-cursor-v1";
pub const CURSOR_FIELDS: &[&str] = &[
    "schema",
    "repo_root",
    "project_id",
    "authority_head",
    "operation_id",
    "receipt_sha256",
    "resulting_hashes",
    "git_context",
    "checksum",
];
pub const CURRENT_REPAIR: &str = "Preserve memory and feature work. Use am refresh inspect --v2 --repo REPO --output /private/path/inspection.json and complete its source review; do not stage, reset or replace the bank merely to pass the quick check.";
pub const ENVELOPE_REPAIR: &str = "Preserve the original preparation and pending evidence. Use the unchanged complete private prepare result or its exact inner proposal. If the retained result was modified, obtain a newly source-reviewed preparation; do not repair report fields by hand.";
const TRANSACTION_REPAIR: &str = "Preserve this state path. Inspect the reported path owner and Unix permissions from a trusted helper. Do not change existing directory modes or discard state merely to pass the check.";
const TRANSACTION_TYPE_REPAIR: &str = "Preserve this state path. Inspect its owner, directory type and Unix permissions from a trusted helper; do not follow redirects or chmod shared parents blindly.";

/// Fixed local observation failure shared with diagnostics; never arbitrary Git text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentFailure {
    pub code: String,
    pub component: String,
    pub message: String,
    pub repair: String,
    pub path: Option<String>,
}
impl CurrentFailure {
    pub fn new(
        code: &str,
        message: &str,
        component: &str,
        repair: Option<&str>,
        path: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            component: component.into(),
            message: message.into(),
            repair: repair.unwrap_or(CURRENT_REPAIR).into(),
            path,
        }
    }
    pub fn from_error(error: &ContractError) -> Option<Self> {
        if let Kind::RefreshCurrent {
            code,
            component,
            repair,
            path,
        } = error.kind()
        {
            Some(Self {
                code: code.clone(),
                component: component.clone(),
                message: error.to_string(),
                repair: repair.clone(),
                path: path.clone(),
            })
        } else {
            None
        }
    }
    pub fn into_error(self) -> ContractError {
        ContractError::refresh_current(
            &self.code,
            &self.component,
            &self.message,
            &self.repair,
            self.path,
        )
    }
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    err(&error.to_string())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid refresh metadata"))
}
fn pairs(value: &Value) -> Result<&[(String, Value)]> {
    value
        .as_object()
        .ok_or_else(|| err("Invalid refresh object"))
}
fn rows(value: &Value) -> Result<&[Value]> {
    value.as_array().ok_or_else(|| err("Invalid refresh list"))
}
fn path_text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "refresh path")
}
fn put(value: &mut Value, key: &str, item: Value) -> Result<()> {
    bank::merge(value, &object([(key, item)]))
}
fn remove(value: &mut Value, key: &str) {
    if let Value::Object(items) = value {
        items.retain(|(name, _)| name != key);
    }
}
fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|items| {
        items.len() == fields.len() && items.iter().all(|(key, _)| fields.contains(&key.as_str()))
    })
}
fn same(a: &Value, b: &Value) -> bool {
    retrieval::same(a, b)
}
fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
fn digest(raw: Option<&[u8]>) -> Value {
    raw.map(|bytes| string(sha256_hex(bytes)))
        .unwrap_or(Value::Null)
}
fn failure(code: &str, message: &str, component: &str) -> ContractError {
    CurrentFailure::new(code, message, component, None, None).into_error()
}
fn named(path: &Path) -> Result<Option<Identity>> {
    let parent = path.parent().ok_or_else(|| err("Invalid refresh path"))?;
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    let name = path
        .file_name()
        .ok_or_else(|| err("Invalid refresh path"))?;
    let info = match chain.directory().stat_at(name) {
        Ok(info) => Some(info),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error(error)),
    };
    chain.recheck().map_err(io_error)?;
    Ok(info)
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Invalid refresh input path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Invalid refresh input path"))?;
    fs::read_source(parent, Path::new(name), maximum)
        .map(|value| value.bytes)
        .map_err(io_error)
}
/// Read max+1 bytes so even an invalid oversized disposable cursor retains its observed hash.
fn prefix(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Invalid refresh cursor path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Invalid refresh cursor path"))?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    let mut file = chain
        .directory()
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    let before = file.stat().map_err(io_error)?;
    let raw = file
        .read_prefix(
            maximum
                .checked_add(1)
                .ok_or_else(|| err("Invalid refresh reader bound"))?,
        )
        .map_err(io_error)?;
    if file.stat().map_err(io_error)? != before
        || chain.directory().stat_at(name).map_err(io_error)? != before
    {
        return Err(err("Refresh cursor changed during observation"));
    }
    chain.recheck().map_err(io_error)?;
    Ok(raw)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    crate::identity_git::git(
        root,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        false,
        false,
    )
    .map_err(io_error)?
    .map(|value| host::text(&value, "refresh Git output").map(str::to_owned))
    .transpose()?
    .ok_or_else(|| err("Refresh Git context is unavailable"))
}

pub type PolicyPin = (u64, u64, u32, u32);
pub type MutablePin = (PolicyPin, i128, i128);
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryStatus {
    Absent(usize),
    Empty,
}
impl InventoryStatus {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Absent(_) => "absent",
            Self::Empty => "empty",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionInventory {
    pub policy_pins: Vec<PolicyPin>,
    pub mutable_pins: Vec<MutablePin>,
    pub status: InventoryStatus,
}
fn policy_pin(value: Identity) -> PolicyPin {
    (value.dev, value.ino, value.mode, value.uid)
}
fn inventory_changed() -> ContractError {
    failure(
        "current-source-changed",
        "Transaction recovery path changed during the observation.",
        "current.transaction-state",
    )
}
fn transaction_untrusted(path: &Path, policy: bool) -> ContractError {
    CurrentFailure::new(
        "transaction-path-untrusted",
        if policy {
            "Transaction recovery path violates the shared Unix owner, permission or link policy."
        } else {
            "Transaction recovery path is unavailable, redirected or not an ordinary directory."
        },
        "current.transaction-state",
        Some(if policy {
            TRANSACTION_REPAIR
        } else {
            TRANSACTION_TYPE_REPAIR
        }),
        Some(path.to_string_lossy().into_owned()),
    )
    .into_error()
}
fn finish_inventory(
    descriptors: &[Descriptor],
    names: &[OsString],
    pins: &[PolicyPin],
    mutable: Vec<MutablePin>,
    status: InventoryStatus,
    missing: Option<&OsStr>,
) -> Result<TransactionInventory> {
    for (fd, pin) in descriptors.iter().zip(pins) {
        if policy_pin(fd.stat().map_err(|_| inventory_changed())?) != *pin {
            return Err(inventory_changed());
        }
    }
    for (index, name) in names.iter().enumerate() {
        let parent = descriptors.get(index).ok_or_else(inventory_changed)?;
        let child = descriptors.get(index + 1).ok_or_else(inventory_changed)?;
        let expected = pins.get(index + 1).ok_or_else(inventory_changed)?;
        if policy_pin(parent.stat_at(name).map_err(|_| inventory_changed())?) != *expected
            || policy_pin(child.stat().map_err(|_| inventory_changed())?) != *expected
        {
            return Err(inventory_changed());
        }
    }
    if let Some(name) = missing {
        let parent = descriptors.last().ok_or_else(inventory_changed)?;
        match parent.stat_at(name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err(inventory_changed()),
        }
    }
    Ok(TransactionInventory {
        policy_pins: pins.to_vec(),
        mutable_pins: mutable,
        status,
    })
}
pub fn transaction_inventory(operations: &Path) -> Result<TransactionInventory> {
    transaction_inventory_observed(operations, &mut |_| Ok(()))
}
/// Synchronous entry observer for real chmod/rebinding race qualification.
pub fn transaction_inventory_observed(
    operations: &Path,
    observed: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<TransactionInventory> {
    let state_root = operations
        .ancestors()
        .nth(3)
        .ok_or_else(|| transaction_untrusted(operations, false))?;
    if !operations.is_absolute()
        || operations
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(transaction_untrusted(operations, false));
    }
    let mut pieces: Vec<OsString> = state_root
        .components()
        .filter_map(|c| {
            if let Component::Normal(p) = c {
                Some(p.to_os_string())
            } else {
                None
            }
        })
        .collect();
    let state_parts = pieces.len();
    pieces.push("writes".into());
    pieces.push(
        operations
            .parent()
            .and_then(Path::file_name)
            .ok_or_else(|| transaction_untrusted(operations, false))?
            .to_owned(),
    );
    pieces.push("transactions".into());
    let mut node = PathBuf::from("/");
    let root =
        Descriptor::open_directory(&node).map_err(|_| transaction_untrusted(&node, false))?;
    let info = root
        .stat()
        .map_err(|_| transaction_untrusted(&node, false))?;
    let mut protected = trust_paths::inspect_entry(&node, &info, false, false, false)
        .map_err(|_| transaction_untrusted(&node, true))?
        .0;
    let mut descriptors = vec![root];
    let mut pins = vec![policy_pin(info)];
    let mut mutable = Vec::new();
    let mut names = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        node.push(piece);
        let parent = descriptors.last().ok_or_else(inventory_changed)?;
        let child = match parent.open_directory_at(piece) {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return finish_inventory(
                    &descriptors,
                    &names,
                    &pins,
                    mutable,
                    InventoryStatus::Absent(index),
                    Some(piece),
                );
            }
            Err(_) => return Err(transaction_untrusted(&node, false)),
        };
        let info = child
            .stat()
            .map_err(|_| transaction_untrusted(&node, false))?;
        protected = trust_paths::inspect_entry(&node, &info, protected, false, false)
            .map_err(|_| transaction_untrusted(&node, true))?
            .0;
        let pin = policy_pin(info);
        pins.push(pin);
        names.push(piece.clone());
        descriptors.push(child);
        if index.saturating_add(1) >= state_parts {
            mutable.push((pin, info.mtime_ns, info.ctime_ns));
        }
        observed(&node)?;
    }
    if descriptors
        .last()
        .ok_or_else(inventory_changed)?
        .entries_bounded(0)
        .map_err(|_| inventory_changed())?
        .is_none()
    {
        return Err(CurrentFailure::new("transaction-recovery-pending","Retained transaction recovery evidence requires review.","current.transaction-state",Some("Preserve the transaction directory and original journal. Inspect the existing operation and use its governed recovery workflow; do not delete or age-steal recovery evidence."),Some(path_text(&node)?.into())).into_error());
    }
    finish_inventory(
        &descriptors,
        &names,
        &pins,
        mutable,
        InventoryStatus::Empty,
        None,
    )
}
pub fn state_operations(root: &Path, bank: &Path) -> Result<PathBuf> {
    writes::state_paths(root,bank,&host::uuid4()?).map(|value| value.0).map_err(|_| CurrentFailure::new("transaction-path-untrusted","The private state locator is unavailable, redirected or not outside the repository.","current.transaction-state",Some("Preserve the state locator and bank. Inspect AGENT_MEMORY_STATE_HOME or XDG_STATE_HOME and their ownership/links; do not reset state or follow a redirected path."),None).into_error())
}
pub fn local_prerequisites(repo: &Path) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    let inventory = transaction_inventory(&state_operations(&root, &bank)?)?;
    Ok(object([
        ("scope", string("local-transaction-path-and-recovery-only")),
        ("transaction_state", string(inventory.status.name())),
        ("remote_check", string("not-exercised")),
        ("remote_freshness_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}
pub fn result(operation: &str, state: &str, values: Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/refresh-result-v1")),
        ("operation", string(operation)),
        ("sharing_state", string(state)),
        ("refresh_applied", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("original_index_writes_performed", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("feature_code_freshness_verified", Value::Bool(false)),
        ("remote_freshness_verified", Value::Bool(false)),
        ("relevant_imports_read", Value::Bool(false)),
        ("local_memory_available", Value::Bool(true)),
    ]);
    bank::merge(&mut value, &values)?;
    Ok(value)
}
pub fn index_hash(root: &Path) -> Result<Value> {
    let path = PathBuf::from(git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?);
    match named(&path)? {
        Some(info) if info.is_symlink() => Err(failure(
            "current-source-changed",
            "Refresh refuses a redirected original index.",
            "current.head-index",
        )),
        Some(_) => Ok(digest(Some(&read(&path, MAX_INDEX_BYTES)?))),
        None => Ok(Value::Null),
    }
}
pub fn cursor_path(root: &Path) -> Result<PathBuf> {
    let (root, bank) = writes::bank_path(root)?;
    let operations = state_operations(&root, &bank)?;
    let target = operations
        .parent()
        .ok_or_else(|| err("Invalid refresh state path"))?
        .join("refresh-cursor.json");
    if named(&target)?.is_some_and(Identity::is_symlink) {
        return Err(err(
            "Disposable refresh cursor is a symlink; preserve it for inspection",
        ));
    }
    Ok(target)
}
pub type CursorObservation = (Option<Value>, Option<String>, String);
pub fn cursor(root: &Path) -> Result<CursorObservation> {
    let target = cursor_path(root)?;
    if named(&target)?.is_none() {
        return Ok((None, None, "absent".into()));
    }
    let raw = prefix(&target, limits::MAX_PRIVATE)?;
    let sha = sha256_hex(&raw);
    let checked = (|| -> Result<(Value, bool)> {
        if raw.len() > limits::MAX_PRIVATE {
            return Err(err("Oversized cursor"));
        }
        let value = load_with_limit(&raw, limits::MAX_PRIVATE)?;
        let mut checksum_value = value.clone();
        remove(&mut checksum_value, "checksum");
        if !exact(&value, CURSOR_FIELDS)
            || get(&value, "schema").as_str() != Some(CURSOR)
            || get(&value, "repo_root").as_str() != Some(path_text(root)?)
            || !uuid_valid(get(&value, "operation_id"))
            || !same(
                get(&value, "project_id"),
                get(
                    &scope_binding::require(root, None, Default::default())?,
                    "project_id",
                ),
            )
            || get(&value, "checksum").as_str()
                != Some(sha256_hex(&encoded(&checksum_value)?).as_str())
        {
            return Err(err("Unverified cursor"));
        }
        recovery::oid(text(&value, "authority_head")?)?;
        let (located, bank) = writes::bank_path(root)?;
        let (_, receipt) = tx::locations(&located, &bank, text(&value, "operation_id")?)?;
        if !named(&receipt)?.is_some_and(Identity::is_regular) {
            return Err(err("Cursor does not bind its actual completed receipt"));
        }
        let receipt_raw = prefix(&receipt, limits::MAX_PRIVATE)?;
        if receipt_raw.len() > limits::MAX_PRIVATE
            || get(&value, "receipt_sha256").as_str() != Some(sha256_hex(&receipt_raw).as_str())
        {
            return Err(err(
                "Cursor receipt is changed or exceeds the bounded reader",
            ));
        }
        let r = load_with_limit(&receipt_raw, limits::MAX_PRIVATE)?;
        if !exact(&r, tx::RECEIPT_FIELDS)
            || get(&r, "schema").as_str() != Some("agent-memory/transaction-receipt-v1")
            || get(&r, "verified") != &Value::Bool(true)
            || get(&r, "outcome").as_str() != Some("finished")
            || !same(get(&r, "operation_id"), get(&value, "operation_id"))
            || !same(get(&r, "project_id"), get(&value, "project_id"))
            || get(&r, "bank_path").as_str() != Some(path_text(&bank)?)
            || !same(get(&r, "resulting_hashes"), get(&value, "resulting_hashes"))
        {
            return Err(err("Cursor receipt does not prove this bank operation"));
        }
        let recorded = get(&value, "git_context");
        if !(exact(recorded, &["revision"]) || exact(recorded, &["branch", "revision"]))
            || pairs(recorded)?
                .iter()
                .any(|(_, v)| !v.as_str().is_some_and(|s| !s.is_empty()))
        {
            return Err(err("Cursor lacks its original branch/revision context"));
        }
        recovery::oid(text(recorded, "revision")?)?;
        let changed = !same(recorded, &retrieval::git_context(root)?);
        Ok((value, changed))
    })();
    match checked {
        Ok((_, true)) => Ok((None, Some(sha), "checkout-context-changed".into())),
        Ok((value, false)) => Ok((Some(value), Some(sha), "verified-local-hint".into())),
        Err(error) if matches!(error.kind(), Kind::MissingProvider) => Err(error),
        Err(_) => Ok((None, Some(sha), "unverified-preserved".into())),
    }
}
pub fn inspect(repo: &Path, base: Option<&str>, version2: bool) -> Result<Value> {
    let (root, _) = writes::bank_path(repo)?;
    let frontier_provider = writes::recovery_frontier_provider()?;
    let frontier = frontier_provider.observe(&root)?;
    if get(&frontier, "coverage_complete") != &Value::Bool(true)
        || get(&frontier, "hold_sha256") != &Value::Null
    {
        frontier_provider.require_scope(&root, get(&frontier, "project_id"), None)?;
        let request = object([
            ("project_id", get(&frontier, "project_id").clone()),
            ("query", string("")),
            ("ids", Value::Array(vec![])),
            ("paths", Value::Array(vec![])),
            ("context", object(Vec::<(&str, Value)>::new())),
            (
                "kinds",
                get(get(get(&records::MODEL, "properties"), "kind"), "enum").clone(),
            ),
            ("minimum_trust", string("reviewed")),
            ("limit", Value::Int(64.into())),
            ("include_context", Value::Bool(true)),
        ]);
        let recalled = retrieval::retrieve(&root, &request, None, &[])?;
        frontier_provider.check(&frontier, &root, None, None)?;
        let mut fields = object([("recall", recalled)]);
        bank::merge(
            &mut fields,
            &bank::recovery_coverage_provider()?.public_coverage(&frontier)?,
        )?;
        bank::merge(
            &mut fields,
            &object([
                ("remote_freshness_verified", Value::Bool(false)),
                ("git_metadata_writes_performed", Value::Bool(false)),
                ("refresh_applied", Value::Bool(false)),
            ]),
        )?;
        return result("inspect", "pending-recovery", fields);
    }
    let observed_context = retrieval::git_context(&root)?;
    let (previous, _, state) = cursor(&root)?;
    let chosen = base.or_else(|| {
        previous
            .as_ref()
            .and_then(|v| get(v, "authority_head").as_str())
    });
    let value = if version2 {
        let mut request = crate::sharing_frontier::request_value(None)?;
        put(
            &mut request,
            "base",
            chosen.map(string).unwrap_or(Value::Null),
        )?;
        crate::sharing_frontier::inspect(&root, Some(&request), None)?
    } else {
        crate::semantic_sharing::inspect(&root, chosen, None)?
    };
    if !same(&retrieval::git_context(&root)?, &observed_context)
        || !same(
            get(get(&value, "binding"), "local_head"),
            get(&observed_context, "revision"),
        )
    {
        return Err(err(
            "Checkout changed during cursor/base inspection; inspect again",
        ));
    }
    result(
        "inspect",
        "review-required",
        object([
            ("inspection", value),
            ("cursor_state", string(state)),
            (
                "baseline_source",
                string(if base.is_some() {
                    "explicit"
                } else if previous.is_some() {
                    "cursor"
                } else {
                    "git-common-base"
                }),
            ),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]),
    )
}

#[derive(PartialEq)]
struct RuntimeState {
    holds: Value,
    hold_sha: Option<String>,
    cursor: CursorObservation,
    inventory: TransactionInventory,
}
fn runtime_state(
    root: &Path,
    bank: &Path,
    context: &Value,
    runtime_root: &Path,
) -> Result<RuntimeState> {
    let (holds, hold_sha) = sharing_holds::read_store(
        root,
        text(context, "project_id")?,
        runtime_root,
        Default::default(),
    )?;
    if limits::truthy(get(&holds, "records")) {
        return Err(failure(
            "current-review-required",
            "Known memory disputes require explicit review before current guidance is established.",
            "current.known-disputes",
        ));
    }
    let operations = state_operations(root, bank)?;
    Ok(RuntimeState {
        holds,
        hold_sha,
        cursor: cursor(root)?,
        inventory: transaction_inventory(&operations)?,
    })
}
#[derive(PartialEq)]
struct ModesAndIndex {
    tree: String,
    index: String,
    modes: Vec<(String, u32)>,
}
fn mode_and_index(
    root: &Path,
    bank_path: &Path,
    values: &Value,
    observation: &Value,
    receipt_covered: bool,
) -> Result<ModesAndIndex> {
    let mut modes = Vec::new();
    let mut local_paths = BTreeSet::new();
    for (path, _) in pairs(get(values, "local"))? {
        let info = named(&bank_path.join(path))?.ok_or_else(|| {
            failure(
                "current-source-changed",
                "A local bank entry is not an ordinary readable Markdown file.",
                "current.local-bank",
            )
        })?;
        let permissions = info.mode & 0o7777;
        if !info.is_regular()
            || permissions & 0o444 == 0
            || permissions & (0o7000 | 0o111 | 0o002) != 0
        {
            return Err(failure(
                "current-source-changed",
                "A local bank entry is not an ordinary readable Markdown file.",
                "current.local-bank",
            ));
        }
        modes.push((path.clone(), permissions));
        local_paths.insert(path.clone());
    }
    let prefix = format!("{}/", text(observation, "bank_root")?);
    let tree_raw = recovery::command(
        root,
        &[
            "ls-tree",
            "-rz",
            "--full-tree",
            text(observation, "local_head")?,
            "--",
            text(observation, "bank_root")?,
        ],
        false,
        recovery::MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    let index_raw = recovery::command(
        root,
        &[
            "ls-files",
            "--stage",
            "-z",
            "--",
            text(observation, "bank_root")?,
        ],
        false,
        recovery::MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    let mut tree_entries = BTreeMap::new();
    let mut index_entries = BTreeMap::new();
    for item in tree_raw.split('\0').filter(|s| !s.is_empty()) {
        let parsed = (|| -> Option<(&str, &str)> {
            let (header, path) = item.split_once('\t')?;
            let mut fields = header.split_whitespace();
            let mode = fields.next()?;
            let kind = fields.next()?;
            let oid = fields.next()?;
            if fields.next().is_some()
                || !path.starts_with(&prefix)
                || mode != "100644"
                || kind != "blob"
                || tree_entries.contains_key(path)
            {
                return None;
            }
            Some((path, oid))
        })();
        let (path, oid) = parsed.ok_or_else(|| {
            failure(
                "current-source-changed",
                "A HEAD bank entry is nonordinary; preserve the index and use full review.",
                "current.head-index",
            )
        })?;
        tree_entries.insert(path.to_owned(), oid.to_owned());
    }
    for item in index_raw.split('\0').filter(|s| !s.is_empty()) {
        let parsed = (|| -> Option<(&str, &str)> {
            let (header, path) = item.split_once('\t')?;
            let mut fields = header.split_whitespace();
            let mode = fields.next()?;
            let oid = fields.next()?;
            let stage = fields.next()?;
            if fields.next().is_some()
                || !path.starts_with(&prefix)
                || mode != "100644"
                || stage != "0"
                || index_entries.contains_key(path)
            {
                return None;
            }
            Some((path, oid))
        })();
        let (path, oid) = parsed.ok_or_else(|| {
            failure(
                "current-source-changed",
                "An index bank entry is nonordinary or unmerged; preserve it and use full review.",
                "current.head-index",
            )
        })?;
        index_entries.insert(path.to_owned(), oid.to_owned());
    }
    if tree_entries != index_entries
        || (!receipt_covered
            && tree_entries.keys().cloned().collect::<BTreeSet<_>>()
                != local_paths
                    .into_iter()
                    .map(|p| format!("{prefix}{p}"))
                    .collect())
    {
        return Err(failure(
            "current-source-changed",
            "HEAD and index bank inventories differ; preserve staged memory and use full review.",
            "current.head-index",
        ));
    }
    Ok(ModesAndIndex {
        tree: tree_raw,
        index: index_raw,
        modes,
    })
}
fn review_required_current() -> Result<Value> {
    result(
        "current",
        "review-required",
        object([
            ("pending_capture", string("unknown")),
            ("pending_publication", string("unknown")),
            ("semantic_truth_verified", Value::Bool(false)),
            ("git_metadata_writes_performed", Value::Bool(true)),
        ]),
    )
}
pub fn current(repo: &Path) -> Result<Value> {
    current_with_runtime(repo, &crate::cli::runtime_root()?)
}

/// Observe current memory against the explicitly located installed runtime.
pub fn current_with_runtime(repo: &Path, runtime_root: &Path) -> Result<Value> {
    writes::recovery_frontier_provider()?.require_complete(repo, None, None)?;
    let (root, bank_path) = writes::bank_path(repo)?;
    let before_index = index_hash(&root)?;
    let before_git = retrieval::git_context(&root)?;
    let (prior, cursor_sha, cursor_state) = cursor(&root)?;
    if !["absent", "verified-local-hint"].contains(&cursor_state.as_str()) {
        return Err(failure(
            "current-review-required",
            "The refresh cursor is unverified or belongs to another checkout context; preserve it and use full review.",
            "current.cursor",
        ));
    }
    let mut receipt_covered = if let Some(prior) = &prior {
        same(
            &writes::snapshot(&root, None)?.1,
            get(prior, "resulting_hashes"),
        )
    } else {
        false
    };
    let boundary = sharing_boundary::inspect(&root, None)?;
    if get(&boundary, "local_boundary_ready") != &Value::Bool(true) {
        let blockers = rows(get(&boundary, "blockers"))?
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        if blockers == BTreeSet::from(["canonical-memory-is-not-tracked"]) {
            if !receipt_covered {
                return Err(ContractError::untracked_canonical_memory());
            }
        } else {
            return Err(failure(
                "current-review-required",
                "The local sharing boundary is not ready; full source review is required.",
                "current.local-boundary",
            ));
        }
    }
    let context = scope_binding::require(&root, None, Default::default())?;
    let before_state = runtime_state(&root, &bank_path, &context, runtime_root)?;
    let (_, observation, binding, values, checked) = crate::semantic_sharing::snapshots(
        &root,
        prior
            .as_ref()
            .and_then(|v| get(v, "authority_head").as_str()),
        None,
        false,
    )?;
    if get(&observation, "shared_bank_present") != &Value::Bool(true)
        || !limits::truthy(get(&values, "base"))
        || !same(get(&values, "base"), get(&values, "local"))
        || !same(get(&values, "local"), get(&values, "landed"))
    {
        return review_required_current();
    }
    receipt_covered = prior.as_ref().is_some_and(|prior| {
        tx::file_hashes(get(&values, "local"))
            .is_ok_and(|hashes| same(&hashes, get(prior, "resulting_hashes")))
    });
    let head = recovery::tree(
        &root,
        text(&observation, "local_head")?,
        text(&observation, "bank_root")?,
        false,
    )?;
    if !same(&head, get(&values, "local")) && !receipt_covered {
        return review_required_current();
    }
    for entry in rows(get(get(&checked, "local"), "entries"))? {
        if get(entry, "format").as_str() == Some("standalone-record") {
            let (_, metadata, _) =
                records::parse(text(get(&values, "local"), text(entry, "path")?)?, &[])?;
            if get(&metadata, "visibility").as_str() != Some("visible")
                || ["candidate", "quarantined"]
                    .contains(&get(&metadata, "status").as_str().unwrap_or(""))
            {
                return Err(failure(
                    "current-review-required",
                    "Hidden or unadmitted memory requires source review.",
                    "current.local-bank",
                ));
            }
        }
    }
    let before_modes = mode_and_index(&root, &bank_path, &values, &observation, receipt_covered)?;
    writes::fault("refresh-current-before-final-recheck")?;
    let final_check = (|| -> Result<()> {
        crate::semantic_sharing::final_sources(&root, &observation, &binding, None)?;
        let scope = scope_binding::require(
            &root,
            Some(text(&context, "project_id")?),
            scope_binding::RequireOptions {
                expected: Some(&context),
                ..Default::default()
            },
        )?;
        let final_cursor = cursor(&root)?;
        if !same(&scope, &context)
            || !same(&retrieval::git_context(&root)?, &before_git)
            || !same(&index_hash(&root)?, &before_index)
            || !same(&sharing_boundary::inspect(&root, None)?, &boundary)
            || runtime_state(&root, &bank_path, &context, runtime_root)? != before_state
            || mode_and_index(&root, &bank_path, &values, &observation, receipt_covered)?
                != before_modes
            || final_cursor.1 != cursor_sha
            || final_cursor.2 != cursor_state
        {
            return Err(failure(
                "current-source-changed",
                "Memory, index, identity or recovery state changed during the observation; obtain fresh source review.",
                "current.final-recheck",
            ));
        }
        Ok(())
    })();
    if let Err(error) = final_check {
        if matches!(
            error.kind(),
            Kind::RefreshCurrent { .. }
                | Kind::SharingAuthority { .. }
                | Kind::HoldFailure { .. }
                | Kind::BankCapacity { .. }
                | Kind::MissingProvider
        ) {
            return Err(error);
        }
        return Err(failure(
            "current-source-changed",
            "Final memory, index, identity or recovery evidence changed or became unavailable during the observation; obtain fresh source review.",
            "current.final-recheck",
        ));
    }
    if recovery::server(
        &root,
        text(&observation, "authority_remote")?,
        text(&observation, "authority_branch")?,
    )? != text(&observation, "authority_head")?
    {
        return Err(failure(
            "current-source-changed",
            "The repository authority advanced during the observation; obtain a fresh observation.",
            "current.authority",
        ));
    }
    result(
        "current",
        "bank-synchronized",
        object([
            ("pending_capture", string("unknown")),
            ("pending_publication", string("unknown")),
            ("semantic_truth_verified", Value::Bool(false)),
            ("source_assertions_verified", Value::Bool(false)),
            ("remote_freshness_verified", Value::Bool(true)),
            (
                "observed_authority_head",
                get(&observation, "authority_head").clone(),
            ),
            ("git_metadata_writes_performed", Value::Bool(true)),
        ]),
    )
}
pub fn current_result(repo: &Path) -> Result<Value> {
    current_result_from(current(repo))
}
pub fn current_result_from(outcome: Result<Value>) -> Result<Value> {
    match outcome {
        Ok(mut value) => {
            if get(&value, "sharing_state").as_str() == Some("review-required") {
                bank::merge(
                    &mut value,
                    &object([
                        ("failure_component", string("current.snapshot")),
                        ("failure_code", string("current-review-required")),
                        (
                            "reason",
                            string(
                                "Local, baseline and authority memory require source review before synchronization is established.",
                            ),
                        ),
                        (
                            "repair",
                            string(
                                "Preserve memory and feature work. Run am refresh inspect --v2 --repo REPO --output /private/path/inspection.json and complete its governed review; do not stage or reset memory merely to satisfy the quick check.",
                            ),
                        ),
                    ]),
                )?;
            }
            Ok(value)
        }
        Err(error) => {
            let (component,code,reason,repair,path) = match error.kind() {
                Kind::RefreshCurrent { code,component,repair,path } => (component.as_str(),code.as_str(),error.to_string(),repair.as_str(),path.clone()),
                Kind::SharingAuthority { code,component,repair } => (component.as_str(),code.as_str(),error.to_string(),repair.as_str(),None),
                Kind::HoldFailure { code,repair } => ("current.known-disputes",code.as_str(),error.to_string(),repair.as_str(),None),
                Kind::BankCapacity {..} => {
                    let details = crate::bank_capacity::details(&error);
                    return result("current","pending-inspection",with_capacity_error(&error,object([("canonical_writes_performed",Value::Bool(false)),("git_metadata_writes_performed",Value::Null),("pending_capture",string("unknown")),("pending_publication",string("unknown")),("semantic_truth_verified",Value::Bool(false)),("failure_component",string("canonical-bank")),("failure_code",get(&details,"failure_code").clone()),("reason",string(error.to_string())),("repair",get(&details,"repair").clone())]))?);
                }
                Kind::UntrackedCanonicalMemory => ("current.local-boundary","canonical-memory-untracked","Locally untracked canonical memory lacks a completed snapshot witness and requires full review.".into(),"Preserve local memory and feature work. Run am refresh inspect --v2 --repo REPO --output /private/path/inspection.json and review original sources; do not stage or reset memory just to pass the quick check.",None),
                Kind::TrustPath {..} => ("current.state-path","transaction-path-untrusted","A private state path violates the shared Unix owner, permission or link policy.".into(),"Preserve state and inspect the local refresh prerequisite in am doctor. Do not follow redirects or chmod existing parent directories blindly.",None),
                _ => ("current.prerequisites","refresh-prerequisite-unverified","A local bank, identity or snapshot prerequisite could not be verified.".into(),"Preserve the bank and pending evidence. Run am doctor, then am refresh inspect --v2 --repo REPO --output /private/path/inspection.json to inspect the failed prerequisite before retrying.",None),
            };
            let mut fields = object([
                ("canonical_writes_performed", Value::Bool(false)),
                (
                    "git_metadata_writes_performed",
                    if matches!(error.kind(), Kind::UntrackedCanonicalMemory) {
                        Value::Bool(false)
                    } else {
                        Value::Null
                    },
                ),
                ("pending_capture", string("unknown")),
                ("pending_publication", string("unknown")),
                ("semantic_truth_verified", Value::Bool(false)),
                ("failure_component", string(component)),
                ("failure_code", string(code)),
                ("reason", string(reason)),
                ("repair", string(repair)),
            ]);
            if let Some(path) = path {
                put(&mut fields, "failure_path", string(path))?;
            }
            result(
                "current",
                "pending-inspection",
                with_capacity_error(&error, fields)?,
            )
        }
    }
}
fn with_capacity_error(error: &ContractError, mut value: Value) -> Result<Value> {
    bank::merge(&mut value, &crate::bank_capacity::details(error))?;
    Ok(value)
}

pub fn validate_transaction(p: &Value, owner: Option<&[u8]>) -> Result<Value> {
    let evidence = get(p, "refresh_evidence");
    if get(p, "operation").as_str() != Some("refresh")
        || !exact(
            evidence,
            &["proof", "recall", "git_context", "recovery_frontier_token"],
        )
    {
        return Err(err(
            "Source-bound refresh requires the exact semantic proof, recall and checkout context",
        ));
    }
    let repo = Path::new(text(p, "repo_root")?);
    let frontier = writes::recovery_frontier_provider()?.require_complete(repo, owner, None)?;
    if !same(get(evidence, "recovery_frontier_token"), &frontier) {
        return Err(err(
            "Refresh outer recovery frontier token is missing, stale or changed",
        ));
    }
    retrieval::request_value(get(evidence, "recall"))?;
    retrieval::observed_context(repo, get(get(evidence, "recall"), "context"))?;
    let binding = get(get(get(evidence, "proof"), "judgment"), "binding");
    if !same(
        get(get(evidence, "recall"), "project_id"),
        get(p, "project_id"),
    ) || !same(get(binding, "project_id"), get(p, "project_id"))
        || !same(get(binding, "repo_root"), get(p, "repo_root"))
        || !same(get(binding, "scope_context"), get(p, "scope_context"))
        || !same(
            get(get(binding, "source_hashes"), "local"),
            get(p, "observed_hashes"),
        )
        || !same(&retrieval::git_context(repo)?, get(evidence, "git_context"))
    {
        return Err(err(
            "Refresh cannot substitute a project, branch, source set or original observed bank",
        ));
    }
    let reviewed = crate::semantic_sharing::check(repo, get(evidence, "proof"), owner)?;
    if !same(get(&reviewed, "refresh_files"), get(p, "files"))
        || !same(
            &tx::file_hashes(get(p, "files"))?,
            get(get(evidence, "proof"), "refresh_hashes"),
        )
    {
        return Err(err(
            "Refresh transaction is not the complete exact reviewed three-way result",
        ));
    }
    Ok(reviewed)
}
pub fn prepare(repo: &Path, request: &Value) -> Result<Value> {
    if !exact(request, &["proof", "recall"]) {
        return Err(err(
            "Prepare refresh with complete proof and an explicit relevant recall request",
        ));
    }
    retrieval::request_value(get(request, "recall"))?;
    let (root, _) = writes::bank_path(repo)?;
    let before = index_hash(&root)?;
    let frontier = writes::recovery_frontier_provider()?.require_complete(&root, None, None)?;
    let reviewed = crate::semantic_sharing::check(&root, get(request, "proof"), None)?;
    let mut p = get(
        &tx::preview(&root, get(&reviewed, "refresh_files"), "refresh")?,
        "proposal",
    )
    .clone();
    let mut evidence = request.clone();
    put(&mut evidence, "git_context", retrieval::git_context(&root)?)?;
    put(&mut evidence, "recovery_frontier_token", frontier)?;
    put(&mut p, "refresh_evidence", evidence)?;
    validate_transaction(&p, None)?;
    if !same(&index_hash(&root)?, &before) {
        return Err(err(
            "Original index changed during preparation; preserve it and inspect",
        ));
    }
    let changed = strings(tx::changed(&p)?);
    result(
        "prepare",
        "prepared",
        object([
            ("proposal", p),
            ("conflicts", get(&reviewed, "conflicts").clone()),
            ("blocked_paths", get(&reviewed, "blocked_paths").clone()),
            ("changed_memory_paths", changed),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]),
    )
}
pub fn save_cursor(
    root: &Path,
    p: &Value,
    observation: &Value,
    receipt: &Value,
    expected: Option<&str>,
) -> Result<Value> {
    let evidence = get(p, "refresh_evidence");
    let proof = get(evidence, "proof");
    let held = limits::truthy(get(proof, "blocked_paths"))
        || limits::truthy(get(proof, "blocked_components"));
    let mut value = object([
        ("schema", string(CURSOR)),
        ("repo_root", string(path_text(root)?)),
        ("project_id", get(p, "project_id").clone()),
        (
            "authority_head",
            if held {
                get(get(get(proof, "judgment"), "binding"), "base").clone()
            } else {
                get(observation, "authority_head").clone()
            },
        ),
        ("operation_id", get(p, "operation_id").clone()),
        (
            "receipt_sha256",
            string(sha256_hex(&read(
                Path::new(text(receipt, "receipt_path")?),
                limits::MAX_PRIVATE,
            )?)),
        ),
        ("resulting_hashes", get(receipt, "resulting_hashes").clone()),
        ("git_context", get(evidence, "git_context").clone()),
    ]);
    let checksum = sha256_hex(&encoded(&value)?);
    put(&mut value, "checksum", string(checksum))?;
    let path = cursor_path(root)?;
    crate::local_registry::private_mkdir(
        path.parent()
            .ok_or_else(|| err("Invalid refresh cursor parent"))?,
    )?;
    crate::local_registry::replace_with_limit(
        &path,
        &limits::private_bytes(&value)?,
        expected,
        limits::MAX_PRIVATE,
    )?;
    Ok(object([
        ("stored", Value::Bool(true)),
        ("authority_head", get(observation, "authority_head").clone()),
        ("content_free", Value::Bool(true)),
    ]))
}
pub fn prepared_proposal(value: &Value) -> Result<&Value> {
    let p = tx::proposal_value(value)?;
    if value.get("proposal").is_none() {
        return Ok(p);
    }
    let proof = get(get(p, "refresh_evidence"), "proof");
    if get(p, "operation").as_str() != Some("refresh")
        || proof.as_object().is_none()
        || get(proof, "blocked_paths").as_array().is_none()
        || get(value, "conflicts").as_array().is_none()
    {
        return Err(ContractError::refresh_prepared_envelope());
    }
    let expected = result(
        "prepare",
        "prepared",
        object([
            ("conflicts", get(value, "conflicts").clone()),
            ("blocked_paths", get(proof, "blocked_paths").clone()),
            ("changed_memory_paths", strings(tx::changed(p)?)),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]),
    )?;
    let mut metadata = value.clone();
    remove(&mut metadata, "proposal");
    let expected_rows = pairs(&expected)?;
    if pairs(&metadata)?.len() != expected_rows.len()
        || expected_rows.iter().any(|(key, item)| {
            metadata.get(key).is_none_or(|observed| {
                std::mem::discriminant(observed) != std::mem::discriminant(item)
                    || !same(observed, item)
            })
        })
    {
        return Err(ContractError::refresh_prepared_envelope());
    }
    Ok(p)
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = prepared_proposal(prepared)?;
    if p.get("refresh_evidence").is_none() {
        return Err(err(
            "The refresh operation requires its original source-bound proposal",
        ));
    }
    let (root, bank_path) = writes::bank_path(Path::new(text(p, "repo_root")?))?;
    let original_index = index_hash(&root)?;
    let (old_cursor, cursor_sha, cursor_state) = cursor(&root)?;
    let had_receipt =
        named(&tx::locations(&root, &bank_path, text(p, "operation_id")?)?.1)?.is_some();
    let receipt = tx::apply(p)?;
    if !same(&index_hash(&root)?, &original_index) {
        return Err(err(
            "Original feature index changed externally; preserve and inspect it",
        ));
    }
    let evidence = get(p, "refresh_evidence");
    let proof = get(evidence, "proof");
    let held = rows(get(proof, "blocked_paths"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| err("Invalid blocked refresh path"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let conflicts_pending = !held.is_empty() || limits::truthy(get(proof, "blocked_components"));
    let mut common = object([
        ("receipt", receipt.clone()),
        (
            "canonical_writes_performed",
            get(&receipt, "writes_performed").clone(),
        ),
        (
            "refresh_applied",
            Value::Bool(get(&receipt, "outcome").as_str() == Some("finished")),
        ),
        ("cursor_state", string(&cursor_state)),
        ("conflicts_pending", Value::Bool(conflicts_pending)),
        ("blocked_paths", strings(held.iter().cloned())),
        (
            "blocked_components",
            get(proof, "blocked_components").clone(),
        ),
        ("git_metadata_writes_performed", Value::Bool(!had_receipt)),
        (
            "authority_snapshot",
            get(get(get(proof, "judgment"), "binding"), "authority_head").clone(),
        ),
    ]);
    if get(&receipt, "still_current") != &Value::Bool(true)
        || get(&receipt, "outcome").as_str() != Some("finished")
        || !same(
            &retrieval::git_context(&root)?,
            get(evidence, "git_context"),
        )
    {
        put(
            &mut common,
            "reason",
            string(
                "Recorded import remains historical; current local bank/branch changed and is preserved",
            ),
        )?;
        return result("apply", "historical-or-changed-local", common);
    }
    let observation = match recovery::observe(
        &root,
        None,
        get(proof, "schema").as_str() == Some("agent-memory/semantic-sharing-proof-v2"),
    ) {
        Ok(value) => value,
        Err(error) => {
            put(&mut common, "git_metadata_writes_performed", Value::Null)?;
            if matches!(error.kind(), Kind::BankCapacity { .. }) {
                put(&mut common, "reason", string(error.to_string()))?;
                common = with_capacity_error(&error, common)?;
            } else if matches!(error.kind(), Kind::MissingProvider) {
                return Err(error);
            } else {
                put(
                    &mut common,
                    "reason",
                    string(
                        "Import is consistent but latest authority could not be verified; inspect again before shared recall",
                    ),
                )?;
            }
            return result("apply", "pending-stale", common);
        }
    };
    put(
        &mut common,
        "git_metadata_writes_performed",
        Value::Bool(true),
    )?;
    if !same(
        get(&observation, "landed_hashes"),
        get(
            get(get(get(proof, "judgment"), "binding"), "source_hashes"),
            "landed",
        ),
    ) {
        put(
            &mut common,
            "reason",
            string(
                "New shared memory landed during import; inspect and review it before current imported recall",
            ),
        )?;
        return result("apply", "pending-new-authority", common);
    }
    let deferred: Vec<String> = held.into_iter().collect();
    let recalled = retrieval::retrieve(&root, get(evidence, "recall"), None, &deferred)?;
    if !same(
        &writes::snapshot(&root, None)?.1,
        get(&receipt, "resulting_hashes"),
    ) || !same(
        &retrieval::git_context(&root)?,
        get(evidence, "git_context"),
    ) {
        return Err(err(
            "Bank/branch changed during imported recall; preserve and inspect it",
        ));
    }
    let mut stored = object([
        ("stored", Value::Bool(false)),
        (
            "reason",
            string(
                "Prior cursor cannot safely advance and is preserved; Git can reconstruct a baseline",
            ),
        ),
    ]);
    if cursor_state != "unverified-preserved"
        && match &old_cursor {
            None => true,
            Some(prior) => recovery::ancestor(
                &root,
                text(prior, "authority_head")?,
                text(&observation, "authority_head")?,
            )?,
        }
    {
        stored = save_cursor(&root, p, &observation, &receipt, cursor_sha.as_deref())?;
    }
    let imported: BTreeSet<String> = tx::changed(p)?.into_iter().collect();
    let recalled_import_paths = rows(get(&recalled, "results"))?
        .iter()
        .filter_map(|row| get(row, "path").as_str())
        .filter(|path| imported.contains(*path))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    bank::merge(
        &mut common,
        &object([
            ("remote_freshness_verified", Value::Bool(true)),
            ("relevant_imports_read", Value::Bool(true)),
            ("recall", recalled),
            ("recalled_import_paths", strings(recalled_import_paths)),
            (
                "observed_authority_head",
                get(&observation, "authority_head").clone(),
            ),
            ("cursor", stored),
            ("source_assertions_verified", Value::Bool(false)),
            ("semantic_truth_verified", Value::Bool(false)),
            ("feature_history_changed", Value::Bool(false)),
            ("unrelated_working_files_changed", Value::Bool(false)),
        ]),
    )?;
    result(
        "apply",
        if conflicts_pending {
            "refreshed-with-conflicts"
        } else {
            "refreshed"
        },
        common,
    )
}
pub fn check_sources(p: &Value, owner: Option<&[u8]>, marker: Option<&[u8]>) -> Result<()> {
    let evidence = get(p, "refresh_evidence");
    let repo = Path::new(text(p, "repo_root")?);
    let token = writes::recovery_frontier_provider()?.require_complete(repo, owner, marker)?;
    if !same(get(evidence, "recovery_frontier_token"), &token) {
        return Err(err(
            "Recovery frontier changed during refresh final readback",
        ));
    }
    if !same(&retrieval::git_context(repo)?, get(evidence, "git_context")) {
        return Err(err("Checkout changed during refresh"));
    }
    let binding = get(get(get(evidence, "proof"), "judgment"), "binding");
    if recovery::server(
        repo,
        text(binding, "authority_remote")?,
        text(binding, "authority_branch")?,
    )? != text(binding, "authority_head")?
    {
        return Err(err(
            "Authority advanced during refresh final source recheck",
        ));
    }
    Ok(())
}
pub fn read_input(path: &Path, version2: bool) -> Result<Value> {
    let maximum = if version2 { limits::MAX_PRIVATE } else { LIMIT };
    let value = load_with_limit(&limits::read_bytes(path, maximum)?, maximum)?;
    let body = value.get("proposal").unwrap_or(&value);
    let proof = body
        .get("proof")
        .filter(|p| limits::truthy(p))
        .unwrap_or_else(|| get(get(body, "refresh_evidence"), "proof"));
    if get(proof, "schema").as_str() == Some("agent-memory/semantic-sharing-proof-v2") || !version2
    {
        Ok(value)
    } else {
        Err(err(
            "Explicit v2 refresh reader requires the actual v2 source-bound proof",
        ))
    }
}

pub struct MemoryRefresh;
impl tx::TransactionValidator for MemoryRefresh {
    fn validate_transaction(&self, p: &Value, owner: Option<&[u8]>) -> Result<()> {
        validate_transaction(p, owner).map(|_| ())
    }
    fn check_sources(&self, p: &Value, owner: &[u8], marker: &[u8]) -> Result<()> {
        check_sources(p, Some(owner), Some(marker))
    }
}
impl crate::turn_check::MemoryRefresh for MemoryRefresh {
    fn current_result(&self, repo: &Path) -> crate::turn_check::CompletionResult<Value> {
        current_result(repo).map_err(Into::into)
    }
    fn local_prerequisites(
        &self,
        repo: &Path,
    ) -> Result<std::result::Result<Value, crate::turn_check::CurrentFailure>> {
        match local_prerequisites(repo) {
            Ok(value) => Ok(Ok(value)),
            Err(error) => match CurrentFailure::from_error(&error) {
                Some(failure) => Ok(Err(crate::turn_check::CurrentFailure {
                    code: failure.code,
                    message: failure.message,
                    repair: failure.repair,
                    path: failure.path,
                })),
                None => Err(error),
            },
        }
    }
}

#[derive(Debug)]
enum CommandFailure {
    Contract(ContractError),
    Output(ContractError),
}
impl From<ContractError> for CommandFailure {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
fn execute(
    operation: &str,
    repo: Option<&Path>,
    input: Option<&Path>,
    base: Option<&str>,
    output: Option<&Path>,
    version2: bool,
    runtime: Result<PathBuf>,
) -> std::result::Result<Value, CommandFailure> {
    let mut input_value = None;
    let mut output_repo = None;
    let mut runtime_root = None;
    if let Some(output) = output {
        let supplied = host::expanduser(output)?;
        if !supplied.is_absolute()
            || supplied
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Ok(result(
                operation,
                "pending-private-output",
                object([
                    ("canonical_writes_performed", Value::Bool(false)),
                    ("git_metadata_writes_performed", Value::Bool(false)),
                    ("failure_component", string("output")),
                    ("failure_code", string("private-output-path-invalid")),
                    (
                        "reason",
                        string(
                            "The private output path must be absolute and must not contain parent-directory components.",
                        ),
                    ),
                    (
                        "repair",
                        string(
                            "Choose a new absolute path in an owner-only directory outside the repository and installed runtime, then retry with --output. Preserve the bank and pending evidence.",
                        ),
                    ),
                ]),
            )?);
        }
        let root = if let Some(repo) = repo {
            repo.to_path_buf()
        } else {
            let value = read_input(input.ok_or_else(|| err("Missing refresh input"))?, version2)?;
            let root = PathBuf::from(text(prepared_proposal(&value)?, "repo_root")?);
            input_value = Some(value);
            root
        };
        let runtime = runtime?;
        limits::preflight_private_output(output, &root, &runtime)
            .map_err(CommandFailure::Output)?;
        output_repo = Some(root);
        runtime_root = Some(runtime);
    }
    let mut value = match operation {
        "current" => current_result(repo.ok_or_else(|| err("Missing refresh repository"))?)?,
        "inspect" => inspect(
            repo.ok_or_else(|| err("Missing refresh repository"))?,
            base,
            version2,
        )?,
        "prepare" => {
            let request = read_input(input.ok_or_else(|| err("Missing refresh input"))?, version2)?;
            if get(get(&request, "proof"), "schema").as_str()
                == Some("agent-memory/semantic-sharing-proof-v2")
                && output.is_none()
            {
                return Ok(result(
                    "prepare",
                    "pending-private-output",
                    object([
                        ("failure_component", string("prepare")),
                        ("failure_code", string("private-output-required")),
                        (
                            "reason",
                            string(
                                "A v2 refresh proposal may retain quarantined local originals and requires private output.",
                            ),
                        ),
                        (
                            "repair",
                            string(
                                "Rerun am refresh prepare with the original --repo and --input plus --v2 --output /private/path/refresh.json; select an owner-only location outside the repository.",
                            ),
                        ),
                    ]),
                )?);
            }
            prepare(
                repo.ok_or_else(|| err("Missing refresh repository"))?,
                &request,
            )?
        }
        "apply" => apply(&match input_value {
            Some(value) => value,
            None => read_input(input.ok_or_else(|| err("Missing refresh input"))?, version2)?,
        })?,
        _ => return Err(err("Unknown refresh operation").into()),
    };
    if let Some(output) = output {
        value = limits::write_private(
            output,
            &value,
            output_repo
                .as_deref()
                .ok_or_else(|| err("Missing private output repository"))?,
            runtime_root
                .as_deref()
                .ok_or_else(|| err("Missing private output runtime"))?,
        )
        .map_err(CommandFailure::Output)?;
    }
    Ok(value)
}
fn command_failure(operation: &str, failure: CommandFailure) -> Result<Value> {
    let (output_failed, error) = match failure {
        CommandFailure::Output(error) => (true, error),
        CommandFailure::Contract(error) => (false, error),
    };
    let untracked = matches!(error.kind(), Kind::UntrackedCanonicalMemory);
    let envelope = matches!(error.kind(), Kind::RefreshPreparedEnvelope);
    let known = matches!(
        error.kind(),
        Kind::SharingAuthority { .. } | Kind::HoldFailure { .. } | Kind::RefreshPreparedEnvelope
    );
    let (component,code,repair)=match error.kind() {
        _ if output_failed=>("output","private-output-invalid",error.to_string()),
        _ if untracked=>("local-boundary","canonical-memory-untracked","Preserve local memory and feature work. Run am refresh inspect --v2 --repo REPO --output /private/path/inspection.json using an owner-only directory outside the repository, then complete the required review. Do not stage or reset memory just to pass this quick check.".into()),
        Kind::SharingAuthority{component,code,repair}=>(component.as_str(),code.as_str(),repair.clone()),
        Kind::HoldFailure{code,repair}=>("known-dispute-state",code.as_str(),repair.clone()),
        Kind::RefreshPreparedEnvelope=>("apply.prepared-envelope","refresh-prepared-envelope-invalid",ENVELOPE_REPAIR.into()),
        _=>(operation,"refresh-unverified","Preserve the proposal, bank and pending evidence. Run am doctor for this repository and inspect the failed component before repeating refresh inspection; do not reset or replace the bank.".into()),
    };
    let reason = if output_failed
        || untracked
        || known
        || matches!(error.kind(), Kind::BankCapacity { .. })
    {
        error.to_string()
    } else {
        "Unresolved, offline, unsafe, interrupted or changed refresh; retain the original proposal and local memory, inspect/recover before retry".into()
    };
    let mut fields = object([
        (
            "canonical_writes_performed",
            if output_failed || envelope || operation == "current" {
                Value::Bool(false)
            } else {
                Value::Null
            },
        ),
        (
            "git_metadata_writes_performed",
            if output_failed || untracked || envelope {
                Value::Bool(false)
            } else {
                Value::Null
            },
        ),
    ]);
    if operation == "current" {
        bank::merge(
            &mut fields,
            &object([
                ("pending_capture", string("unknown")),
                ("pending_publication", string("unknown")),
                ("semantic_truth_verified", Value::Bool(false)),
            ]),
        )?;
    }
    bank::merge(
        &mut fields,
        &object([
            ("failure_component", string(component)),
            ("failure_code", string(code)),
            ("reason", string(reason)),
            ("repair", string(repair)),
        ]),
    )?;
    result(
        operation,
        if output_failed {
            "pending-private-output"
        } else {
            "pending-inspection"
        },
        with_capacity_error(&error, fields)?,
    )
}
pub fn command_result(
    operation: &str,
    repo: Option<&Path>,
    input: Option<&Path>,
    base: Option<&str>,
    output: Option<&Path>,
    version2: bool,
    runtime: Result<PathBuf>,
) -> CommandResult {
    crate::validation_memo::operation(|_| {
        match execute(operation, repo, input, base, output, version2, runtime) {
            Ok(value) => {
                let pending = get(&value, "sharing_state").as_str();
                let exit = if (operation == "current" && pending == Some("pending-inspection"))
                    || pending == Some("pending-private-output")
                {
                    1
                } else {
                    0
                };
                let style = if get(&value, "failure_code").as_str()
                    == Some("private-output-path-invalid")
                {
                    json::DEFAULT
                } else {
                    json::INDENT2_ASCII
                };
                CommandResult::json(value, exit, style)
            }
            Err(failure) => match command_failure(operation, failure) {
                Ok(value) => CommandResult::json(value, 1, json::DEFAULT),
                Err(error) => CommandResult {
                    value: None,
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("{error}\n"),
                },
            },
        }
    })
}
