//! Shared attribution/project resolution and explicit, recoverable scope binding.
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{self as developer, encoded, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, DirectoryChain};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_capacity, bank_contract as bank, bank_transactions as transactions, bank_writes as writes,
    layout_recovery_state as recovery_state, local_registry as storage,
    project_identity as project, repository_config,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

pub const SCHEMA: &str = "agent-memory/scope-binding-v1";
pub const FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "developer_id",
    "developer_sha256",
    "project",
    "base_files",
    "transaction",
];
pub const CONTEXT_FIELDS: &[&str] = &[
    "developer_id",
    "project_id",
    "developer_sha256",
    "project_sha256",
    "checkout_key",
    "repository_settings_sha256",
    "initialization_only",
];
pub const MAX_BINDING_BYTES: usize = crate::sharing_limits::MAX_PRIVATE;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    get(v, key)
        .as_str()
        .ok_or_else(|| err("Invalid scope binding bundle"))
}
fn path_text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "scope path")
}
fn sha(raw: &[u8]) -> String {
    developer::sha256_hex(raw)
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len() && pairs.iter().all(|(key, _)| fields.contains(&key.as_str()))
    })
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Int(v) => v.as_str() != "0",
        Value::Float(v) => *v != 0.0,
    }
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
            let indexed: BTreeMap<_, _> = b.iter().map(|(k, v)| (k.as_str(), v)).collect();
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
fn named(path: &Path) -> Result<Option<fs::Identity>> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Scope path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Scope path has no filename"))?;
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let value = match chain.directory().stat_at(name) {
        Ok(info) => Some(info),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(e)),
    };
    chain.recheck().map_err(io_error)?;
    Ok(value)
}
fn exists(path: &Path) -> Result<bool> {
    Ok(named(path)?.is_some())
}
fn read(path: &Path, bound: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Scope path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Scope path has no filename"))?;
    fs::read_regular(parent, Path::new(name), bound)
        .map(|(raw, _)| raw)
        .map_err(io_error)
}

pub fn result(
    operation: &str,
    fields: impl IntoIterator<Item = (impl Into<String>, Value)>,
) -> Value {
    let mut pairs = vec![
        ("schema".into(), string("agent-memory/scope-result-v1")),
        ("operation".into(), string(operation)),
        ("identity_authenticated".into(), Value::Bool(false)),
        (
            "security_boundary".into(),
            get(&developer::SPEC, "security-boundary").clone(),
        ),
    ];
    pairs.extend(fields.into_iter().map(|(k, v)| (k.into(), v)));
    Value::Object(pairs)
}

pub fn principal(
    repo: &Path,
    observations: &[Value],
    harness: Option<&str>,
    session: Option<&str>,
) -> Result<Value> {
    let value = developer::status(observations, harness, session, repo)?;
    if truthy(get(&value, "setup_required"))
        || truthy(get(&value, "attribution_conflicts"))
        || truthy(get(&value, "lock_present"))
    {
        return Err(ContractError::identity_unresolved(
            "Developer attribution is missing, conflicting or locked; explicitly create/adopt/select or repair it",
        ));
    }
    Ok(value)
}

pub fn status(
    repo: &Path,
    observations: &[Value],
    harness: Option<&str>,
    session: Option<&str>,
) -> Result<Value> {
    let mut issues = Vec::new();
    let dev = match developer::status(observations, harness, session, repo) {
        Ok(dev) => {
            if truthy(get(&dev, "setup_required"))
                || truthy(get(&dev, "attribution_conflicts"))
                || truthy(get(&dev, "lock_present"))
            {
                issues.push(string(
                    "Developer attribution requires explicit setup or clarification",
                ));
            }
            dev
        }
        Err(e) => {
            issues.push(string(e.to_string()));
            object([
                ("developer_id", Value::Null),
                ("error", string(e.to_string())),
            ])
        }
    };
    let proj = match (|| {
        let proj = project::status(repo)?;
        issues.extend(
            get(&proj, "issues")
                .as_array()
                .ok_or_else(|| err("Invalid project identity status"))?
                .iter()
                .cloned(),
        );
        if exists(&storage::paths("projects")?.2)? {
            issues.push(string(
                "Project registry is locked; inspect/recover before a write",
            ));
        }
        Ok::<Value, ContractError>(proj)
    })() {
        Ok(proj) => proj,
        Err(e) if matches!(e.kind(), Kind::BankCapacity { .. }) => return Err(e),
        Err(e) => {
            issues.push(string(e.to_string()));
            object([
                ("project_id", Value::Null),
                ("resolved", Value::Bool(false)),
                ("error", string(e.to_string())),
            ])
        }
    };
    Ok(result(
        "status",
        [
            ("developer_id", get(&dev, "developer_id").clone()),
            ("project_id", get(&proj, "project_id").clone()),
            ("resolved", Value::Bool(issues.is_empty())),
            ("issues", Value::Array(issues)),
            ("developer", dev),
            ("project", proj),
            ("writes_performed", Value::Bool(false)),
        ],
    ))
}

#[derive(Default, Clone, Copy)]
pub struct RequireOptions<'a> {
    pub expected: Option<&'a Value>,
    pub initialize: bool,
    pub owned_lock: Option<&'a [u8]>,
    pub owned_transaction: Option<&'a [u8]>,
    pub migration_reader: Option<project::MigrationReader<'a>>,
}
fn scope_value(dev: &Value, proj: &Value, initialize: bool) -> Result<Value> {
    let proof = get(proj, "evidence");
    Ok(object([
        ("developer_id", get(dev, "developer_id").clone()),
        ("project_id", get(proj, "project_id").clone()),
        ("developer_sha256", get(dev, "observed_sha256").clone()),
        ("project_sha256", get(proj, "observed_sha256").clone()),
        ("checkout_key", get(proof, "checkout_key").clone()),
        (
            "repository_settings_sha256",
            string(sha(&encoded(get(proof, "repository_settings"))?)),
        ),
        ("initialization_only", Value::Bool(initialize)),
    ]))
}
pub fn require(
    repo: &Path,
    project_id: Option<&str>,
    options: RequireOptions<'_>,
) -> Result<Value> {
    let dev = principal(repo, &[], None, None)?;
    if options.initialize && options.migration_reader.is_some() {
        return Err(err(
            "Migration cannot initialize or rebind project identity",
        ));
    }
    let proj = project::status_with_options(
        repo,
        None,
        project::DiscoveryOptions {
            owned_lock: options.owned_lock,
            owned_transaction: options.owned_transaction,
            migration_reader: options.migration_reader,
            raw_identity: None,
        },
    )?;
    if exists(&storage::paths("projects")?.2)? {
        return Err(err(
            "Project registry is locked; inspect/recover before a write",
        ));
    }
    if options.initialize {
        let pending = Value::Array(vec![string(
            "Bank initialization or identity update is pending",
        )]);
        if get(get(&proj, "evidence"), "bank_project_id") != &Value::Null
            || !truthy(get(&proj, "checkout_entry"))
            || !truthy(get(&proj, "project_entry"))
            || !same(get(&proj, "issues"), &pending)
        {
            return Err(err(
                "Initialization requires an explicitly registered empty checkout with unambiguous authority",
            ));
        }
    } else if get(&proj, "resolved").as_bool() != Some(true) {
        let issues = get(&proj, "issues")
            .as_array()
            .ok_or_else(|| err("Invalid project identity status"))?;
        let messages = issues
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| err("Invalid project identity status"))
            })
            .collect::<Result<Vec<_>>>()?;
        return Err(ContractError::identity_unresolved(format!(
            "Project identity is unresolved: {}",
            messages.join("; ")
        )));
    }
    if project_id.is_some_and(|id| get(&proj, "project_id").as_str() != Some(id)) {
        return Err(ContractError::identity_unresolved(
            "Operation project ID differs from the resolved repository",
        ));
    }
    let context = scope_value(&dev, &proj, options.initialize)?;
    if options
        .expected
        .is_some_and(|expected| !same(&context, expected))
    {
        return Err(ContractError::identity_unresolved(
            "Resolved identity changed during the operation; retain original provenance and prepare again",
        ));
    }
    Ok(context)
}

pub fn raw_recovery_context(
    repo: &Path,
    raw_identity: &project::RawIdentityEvidence,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<Value> {
    let root = fs::canonical_path(repo, false)?;
    if raw_identity.repo_root() != path_text(&root)? {
        return Err(err("Raw identity belongs to another checkout"));
    }
    let dev = principal(&root, &[], None, None)?;
    let proj = project::status_with_options(
        &root,
        None,
        project::DiscoveryOptions {
            owned_lock: owner,
            owned_transaction: transaction,
            raw_identity: Some(raw_identity),
            migration_reader: None,
        },
    )?;
    if exists(&storage::paths("projects")?.2)? || get(&proj, "resolved").as_bool() != Some(true) {
        return Err(err(
            "Recovery project identity is unresolved; retain the original evidence",
        ));
    }
    let context = scope_value(&dev, &proj, false)?;
    context_value(&context, false)?;
    Ok(context)
}

/// The recovery_frontier implementation constructs this only after full hold,
/// source, runtime, ledger, attestation and independently resolved identity checks.
/// Not JSON, not a write proof; no deserialization or conversion to Value exists.
#[derive(Clone)]
pub struct VerifiedEnrollmentHold {
    root: PathBuf,
    bank: PathBuf,
    marker: Vec<u8>,
    token: Value,
    scope: Value,
    operation_id: String,
    runtime_artifact_digest: String,
}
impl VerifiedEnrollmentHold {
    pub fn from_verified_observation(
        root: PathBuf,
        bank: PathBuf,
        marker: Vec<u8>,
        token: Value,
        scope: Value,
        operation_id: String,
        runtime_artifact_digest: String,
    ) -> Result<Self> {
        context_value(&scope, false)?;
        if !uuid_valid(&string(&operation_id)) {
            return Err(err("Invalid verified recovery operation"));
        }
        Ok(Self {
            root,
            bank,
            marker,
            token,
            scope,
            operation_id,
            runtime_artifact_digest,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn bank(&self) -> &Path {
        &self.bank
    }
    pub fn marker(&self) -> &[u8] {
        &self.marker
    }
    pub fn token(&self) -> &Value {
        &self.token
    }
    pub fn scope(&self) -> &Value {
        &self.scope
    }
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn runtime_artifact_digest(&self) -> &str {
        &self.runtime_artifact_digest
    }
}
fn same_hold(a: Option<&VerifiedEnrollmentHold>, b: Option<&VerifiedEnrollmentHold>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.root == b.root
                && a.bank == b.bank
                && a.marker == b.marker
                && same(&a.token, &b.token)
                && same(&a.scope, &b.scope)
                && a.operation_id == b.operation_id
                && a.runtime_artifact_digest == b.runtime_artifact_digest
        }
        _ => false,
    }
}
/// Recovery source-marker observation must unwind even on errors/panics.
/// Implemented by recovery_frontier; Drop restores the previous observation.
pub trait SourceMarkerObservation {
    fn finish(&mut self) -> Result<()>;
}

/// Purpose-limited live identity, sealed by private fields and an unforgeable Rc.
/// It is neither Clone nor Send; synchronous operation scopes cannot migrate to
/// another thread. There is deliberately no serialization/Value/write API.
pub struct PrivateEnrollmentContext {
    root: PathBuf,
    bank: PathBuf,
    runtime_root: PathBuf,
    scope: Value,
    hold: Option<VerifiedEnrollmentHold>,
    context_digest: String,
    pins: Value,
    seal: Rc<()>,
}
impl PrivateEnrollmentContext {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn bank(&self) -> &Path {
        &self.bank
    }
    pub fn scope(&self) -> Value {
        self.scope.clone()
    }
    pub fn project_id(&self) -> &Value {
        get(&self.scope, "project_id")
    }
    pub fn developer_id(&self) -> &Value {
        get(&self.scope, "developer_id")
    }
    pub fn held(&self) -> bool {
        self.hold.is_some()
    }
    pub fn hold(&self) -> Option<&VerifiedEnrollmentHold> {
        self.hold.as_ref()
    }
    pub fn context_digest(&self) -> &str {
        &self.context_digest
    }
    /// Equivalent to successful context-manager entry and exit. Failed bodies
    /// preserve their own refusal; successful bodies get an independent recheck.
    pub fn within<T>(&self, body: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        recheck_private_enrollment(self)?;
        let value = body(self)?;
        recheck_private_enrollment(self)?;
        Ok(value)
    }
}
#[derive(Clone)]
struct ReadContext {
    seal: Weak<()>,
    root: PathBuf,
    bank: PathBuf,
    runtime_root: PathBuf,
}
impl ReadContext {
    fn of(context: &PrivateEnrollmentContext) -> Self {
        Self {
            seal: Rc::downgrade(&context.seal),
            root: context.root.clone(),
            bank: context.bank.clone(),
            runtime_root: context.runtime_root.clone(),
        }
    }
    fn matches(&self, context: &PrivateEnrollmentContext) -> bool {
        self.seal
            .upgrade()
            .is_some_and(|seal| Rc::ptr_eq(&seal, &context.seal))
    }
}
thread_local! {
    static ENROLLMENT_READS: RefCell<Option<ReadContext>> = const { RefCell::new(None) };
    static ENROLLMENT_LOCATIONS: RefCell<Option<ReadContext>> = const { RefCell::new(None) };
}
struct RestoreReads(Option<ReadContext>);
impl Drop for RestoreReads {
    fn drop(&mut self) {
        ENROLLMENT_READS.with(|v| *v.borrow_mut() = self.0.take());
    }
}
struct RestoreLocations(Option<ReadContext>);
impl Drop for RestoreLocations {
    fn drop(&mut self) {
        ENROLLMENT_LOCATIONS.with(|v| *v.borrow_mut() = self.0.take());
    }
}

pub fn fresh_enrollment_locations<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    let previous = ENROLLMENT_LOCATIONS.with(|v| v.borrow_mut().take());
    let _restore = RestoreLocations(previous);
    body()
}
pub fn private_enrollment_reads<T>(
    context: &PrivateEnrollmentContext,
    body: impl FnOnce(&PrivateEnrollmentContext) -> Result<T>,
) -> Result<T> {
    let previous = ENROLLMENT_READS.with(|v| v.replace(Some(ReadContext::of(context))));
    let _restore = RestoreReads(previous);
    let value = body(context)?;
    recheck_private_enrollment(context)?;
    Ok(value)
}
pub fn enrollment_read_context<'a>(
    repo: &Path,
    context: &'a PrivateEnrollmentContext,
) -> Result<&'a PrivateEnrollmentContext> {
    let active = ENROLLMENT_READS.with(|v| v.borrow().as_ref().is_some_and(|v| v.matches(context)));
    if !active || fs::canonical_path(repo, false)? != context.root {
        return Err(err(
            "Private enrollment read context is foreign or outside its live operation",
        ));
    }
    Ok(context)
}
pub fn observe_enrollment_locations<T>(
    context: &PrivateEnrollmentContext,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    enrollment_read_context(&context.root, context)?;
    let previous = ENROLLMENT_LOCATIONS.with(|v| v.replace(Some(ReadContext::of(context))));
    let _restore = RestoreLocations(previous);
    observe_enrollment_location(&context.root, Some(context), &context.runtime_root)?;
    body()
}
pub fn observe_enrollment_location(
    repo: &Path,
    context: Option<&PrivateEnrollmentContext>,
    runtime_root: &Path,
) -> Result<Option<recovery_state::RecoveryPaths>> {
    let active = ENROLLMENT_LOCATIONS.with(|v| v.borrow().clone());
    let Some(active) = active else {
        if context.is_some() {
            return Err(err("Observer locator reuse is outside its live operation"));
        }
        return Ok(None);
    };
    if context.is_some_and(|context| !active.matches(context)) {
        return Err(err(
            "Observer locator differs from its exact active context",
        ));
    }
    let root = fs::canonical_path(repo, false)?;
    if root != active.root && context.is_none() {
        return Ok(None);
    }
    let reading = ENROLLMENT_READS.with(|v| {
        v.borrow().as_ref().is_some_and(|v| {
            v.root == root && v.seal.ptr_eq(&active.seal) && v.seal.upgrade().is_some()
        })
    });
    if !reading || root != active.root {
        return Err(err(
            "Private enrollment read context is foreign or outside its live operation",
        ));
    }
    if fs::canonical_path(runtime_root, false)? != active.runtime_root {
        return Err(err(
            "Private enrollment runtime location changed during observation",
        ));
    }
    let location =
        recovery_state::_paths_for_bank(&active.root, &active.bank, None, &active.runtime_root)?;
    let barrier = location
        .2
        .parent()
        .ok_or_else(|| err("Private enrollment source location has no parent"))?;
    if !crate::turn_sources::source_barrier_held(barrier) {
        return Err(err(
            "Observer locator reuse requires its already held source barrier",
        ));
    }
    Ok(Some(location))
}
fn canonical_writer(error: storage::CanonicalWriterError) -> ContractError {
    match error {
        storage::CanonicalWriterError::Busy => ContractError::canonical_writer_busy(),
        storage::CanonicalWriterError::Initializing => {
            ContractError::canonical_writer_initializing()
        }
        storage::CanonicalWriterError::Contract(error) => error,
    }
}
pub fn private_enrollment_context(
    repo: &Path,
    runtime_root: &Path,
) -> Result<PrivateEnrollmentContext> {
    let outcome = fresh_enrollment_locations(|| {
        repository_config::observe_settings(repo, || {
            let mut observation = writes::recovery_frontier_provider()?.observe_source_markers()?;
            let value = private_enrollment_context_at(repo, runtime_root)?;
            observation.finish()?;
            Ok(value)
        })
    });
    outcome.map_err(|e| match e.kind() {
        Kind::CanonicalWriterBusy => ContractError::source_transport_busy(false),
        Kind::CanonicalWriterInitializing => ContractError::source_transport_busy(true),
        _ => e,
    })
}
fn binding_pin(path: &Path) -> Result<Value> {
    let chain = DirectoryChain::open(path).map_err(io_error)?;
    let info = chain.directory().stat().map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    Ok(Value::Array(vec![
        string(path_text(path)?),
        Value::Int(info.dev.into()),
        Value::Int(info.ino.into()),
        Value::Int(info.uid.into()),
        Value::Int(info.gid.into()),
        Value::Int(info.mode.into()),
    ]))
}
fn private_enrollment_context_at(
    repo: &Path,
    runtime_root: &Path,
) -> Result<PrivateEnrollmentContext> {
    let runtime_root = fs::canonical_path(runtime_root, false)?;
    let (root, bank) = writes::bank_path(repo)?;
    let (raw_root, raw_bank, recovery_base, _) = recovery_state::paths(&root, None, &runtime_root)?;
    if raw_root != root || raw_bank != bank {
        return Err(err(
            "Private enrollment raw recovery bank differs from the ordinary bank",
        ));
    }
    storage::check_canonical_writer(&bank.join(".agent-memory.lock"), writes::OPERATIONS)
        .map_err(canonical_writer)?;
    let source_base = recovery_base
        .parent()
        .ok_or_else(|| err("Private enrollment source location has no parent"))?;
    crate::turn_sources::check_source_compatibility(source_base)?;
    let frontier = writes::recovery_frontier_provider()?;
    let hold = frontier.verified_enrollment_hold_at_bank(&raw_root, &raw_bank, &runtime_root)?;
    let scope = match &hold {
        Some(hold) => hold.scope.clone(),
        None => require(&root, None, RequireOptions::default())?,
    };
    #[cfg(feature = "dev-tools")]
    writes::fault("enrollment.scope.after-require")?;
    let token = match &hold {
        Some(hold) => hold.token.clone(),
        None => frontier.observe_enrollment_bank(&raw_root, &raw_bank, &runtime_root)?,
    };
    let parent = bank
        .parent()
        .ok_or_else(|| err("Private enrollment bank has no parent"))?;
    let mut pins = object([
        ("scope", scope.clone()),
        ("frontier", token),
        (
            "binding",
            Value::Array(vec![
                binding_pin(&root)?,
                binding_pin(parent)?,
                binding_pin(&bank)?,
            ]),
        ),
        (
            "marker",
            hold.as_ref()
                .map(|hold| string(sha(&hold.marker)))
                .unwrap_or(Value::Null),
        ),
    ]);
    if let Some(hold) = &hold {
        if hold.root != root || hold.bank != bank {
            return Err(err(
                "Private enrollment recovery location changed during observation",
            ));
        }
        let directory =
            recovery_state::_paths_for_bank(&root, &bank, Some(&hold.operation_id), &runtime_root)?
                .3
                .ok_or_else(|| err("Private enrollment recovery directory is missing"))?;
        let bindings = recovery_state::read_bytes(
            &storage::paths("layout-recovery-bindings")?.1,
            MAX_BINDING_BYTES,
            false,
        )?
        .ok_or_else(|| err("Private enrollment recovery bindings are missing"))?;
        let journal =
            recovery_state::read_bytes(&directory.join("journal.json"), MAX_BINDING_BYTES, false)?
                .ok_or_else(|| err("Private enrollment recovery journal is missing"))?;
        bank::merge(
            &mut pins,
            &object([
                (
                    "runtime_artifact_digest",
                    string(&hold.runtime_artifact_digest),
                ),
                ("private_bindings_sha256", string(sha(&bindings))),
                ("journal_sha256", string(sha(&journal))),
            ]),
        )?;
    }
    if writes::bank_path(&root)? != (root.clone(), bank.clone()) {
        return Err(err(
            "Private enrollment bank location changed during observation",
        ));
    }
    let raw = recovery_state::paths(&root, None, &runtime_root)?;
    if raw.0 != root || raw.1 != bank {
        return Err(err(
            "Private enrollment raw recovery bank location changed during observation",
        ));
    }
    let context_digest = sha(&encoded(&pins)?);
    Ok(PrivateEnrollmentContext {
        root,
        bank,
        runtime_root,
        scope,
        hold,
        context_digest,
        pins,
        seal: Rc::new(()),
    })
}
pub fn recheck_private_enrollment(
    context: &PrivateEnrollmentContext,
) -> Result<&PrivateEnrollmentContext> {
    let current = private_enrollment_context(&context.root, &context.runtime_root)?;
    if current.root != context.root
        || current.bank != context.bank
        || !same(&current.scope, &context.scope)
        || !same_hold(current.hold.as_ref(), context.hold.as_ref())
        || !same(&current.pins, &context.pins)
        || current.context_digest != context.context_digest
    {
        return Err(err(
            "Private enrollment identity or recovery hold changed; retain the original pending source",
        ));
    }
    Ok(context)
}

pub fn capture(repo: &Path) -> Result<Value> {
    match require(repo, None, RequireOptions::default()) {
        Ok(context) => Ok(context),
        Err(e) if matches!(e.kind(), Kind::BankCapacity { .. }) => Err(e),
        Err(_) => Ok(Value::Null),
    }
}
pub fn context_value(context: &Value, initialize: bool) -> Result<()> {
    if context == &Value::Null {
        return Ok(());
    }
    if !exact(context, CONTEXT_FIELDS)
        || !uuid_valid(get(context, "developer_id"))
        || !uuid_valid(get(context, "project_id"))
        || get(context, "initialization_only").as_bool() != Some(initialize)
    {
        return Err(err("Invalid pinned scope context"));
    }
    Ok(())
}
fn replace_metadata(metadata: &mut frontmatter::Metadata, key: &str, value: Value) {
    if let Some((_, old)) = metadata.iter_mut().find(|(name, _)| name == key) {
        *old = value;
    } else {
        metadata.push((key.into(), value));
    }
}
pub fn identity_files(files: &Value, identity: &Value) -> Result<Value> {
    let mut updated = files.clone();
    let original = get(files, "MEMORY.md")
        .as_str()
        .ok_or_else(|| err("Invalid scope binding bank files"))?;
    let (root, _, _) = bank::parse_router(original)?;
    let allowed = root
        .as_object()
        .ok_or_else(|| err("Invalid bank router metadata"))?
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    let (mut metadata, body) = frontmatter::read(original, &allowed)?;
    replace_metadata(
        &mut metadata,
        "project-id",
        get(identity, "project-id").clone(),
    );
    if get(identity, "canonical-remote") == &Value::Null {
        metadata.retain(|(key, _)| key != "canonical-remote");
    } else {
        replace_metadata(
            &mut metadata,
            "canonical-remote",
            get(identity, "canonical-remote").clone(),
        );
    }
    bank::merge(
        &mut updated,
        &object([("MEMORY.md", string(frontmatter::write(&metadata)? + body))]),
    )?;
    let checked = bank::validate_files(files, None)?;
    let entries = get(&checked, "entries")
        .as_array()
        .ok_or_else(|| err("Invalid checked bank inventory"))?;
    for entry in entries {
        if get(entry, "format").as_str() == Some("context-prose") {
            continue;
        }
        let path = text(entry, "path")?;
        let original = get(files, path)
            .as_str()
            .ok_or_else(|| err("Invalid scope binding bank files"))?;
        let header = original
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n").map(|(head, _)| head))
            .ok_or_else(|| err("Invalid scope binding record metadata"))?;
        let keys = header
            .lines()
            .filter(|line| !line.starts_with(' '))
            .map(|line| line.split_once(':').map(|(key, _)| key).unwrap_or(line))
            .collect::<Vec<_>>();
        let (mut metadata, content) = frontmatter::read(original, &keys)?;
        let key = if metadata.iter().any(|(key, _)| key == "project-id") {
            "project-id"
        } else {
            "scope"
        };
        replace_metadata(&mut metadata, key, get(identity, "project-id").clone());
        bank::merge(
            &mut updated,
            &object([(path, string(frontmatter::write(&metadata)? + content))]),
        )?;
    }
    bank::validate_files(&updated, None)?;
    Ok(updated)
}

pub fn preview(
    repo: &Path,
    action: &str,
    target: Option<&str>,
    reason: &str,
    authority: Option<&str>,
) -> Result<Value> {
    let dev = principal(repo, &[], None, None)?;
    let project_result = project::preview(repo, action, target, reason, authority)?;
    let prepared = get(&project_result, "proposal");
    let proof = get(prepared, "evidence");
    let root = Path::new(text(proof, "repo_root")?);
    let mut base = Value::Null;
    let mut transaction = Value::Null;
    if get(proof, "bank_project_id") != &Value::Null {
        let (_, observed) = writes::snapshot(root, None)?;
        base = transactions::files_at(&repository_config::bank_path(root)?)?;
        if !same(&transactions::file_hashes(&base)?, &observed) {
            return Err(err("Bank changed while preparing binding"));
        }
        let desired = identity_files(&base, get(prepared, "bank_identity"))?;
        if !same(&desired, &base) {
            transaction = get(&transactions::preview(root, &desired, "bind")?, "proposal").clone();
        }
    }
    let changed = if transaction == Value::Null {
        Vec::new()
    } else {
        let relative = repository_config::relative(root)?;
        transactions::changed(&transaction)?
            .into_iter()
            .map(|path| string(format!("{relative}/{path}")))
            .collect()
    };
    let pending = base == Value::Null;
    let bundle = object([
        ("schema", string(SCHEMA)),
        ("operation_id", get(prepared, "operation_id").clone()),
        ("developer_id", get(&dev, "developer_id").clone()),
        ("developer_sha256", get(&dev, "observed_sha256").clone()),
        ("project", prepared.clone()),
        ("base_files", base),
        ("transaction", transaction),
    ]);
    Ok(result(
        "preview",
        [
            ("proposal", bundle),
            ("project_id", get(prepared, "project_id").clone()),
            ("developer_id", get(&dev, "developer_id").clone()),
            ("registry_change", prepared.clone()),
            ("changed_memory_paths", Value::Array(changed)),
            ("initialization_pending", Value::Bool(pending)),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ],
    ))
}
pub fn bundle_value(prepared: &Value) -> Result<&Value> {
    let p = prepared.get("proposal").unwrap_or(prepared);
    if !exact(p, FIELDS)
        || get(p, "schema").as_str() != Some(SCHEMA)
        || !uuid_valid(get(p, "operation_id"))
        || !uuid_valid(get(p, "developer_id"))
        || get(p, "project").as_object().is_none()
        || get(get(p, "project"), "operation_id") != get(p, "operation_id")
    {
        return Err(err("Invalid scope binding bundle"));
    }
    let proj = get(p, "project");
    let proof = get(proj, "evidence");
    let base = get(p, "base_files");
    let tx = get(p, "transaction");
    if base != &Value::Null {
        bank::validate_files(base, None)?;
        let router = get(base, "MEMORY.md")
            .as_str()
            .ok_or_else(|| err("Invalid scope binding bank files"))?;
        if get(proof, "router_sha256").as_str() != Some(sha(router.as_bytes()).as_str()) {
            return Err(err("Binding base differs from the project preview"));
        }
        let desired = identity_files(base, get(proj, "bank_identity"))?;
        if same(&desired, base) {
            if tx != &Value::Null {
                return Err(err("Unnecessary or unknown bank transaction in binding"));
            }
        } else if tx == &Value::Null
            || get(transactions::proposal_value(tx)?, "operation").as_str() != Some("bind")
            || get(tx, "repo_root") != get(proof, "repo_root")
            || get(tx, "project_id") != get(proof, "bank_project_id")
            || !same(
                get(tx, "observed_hashes"),
                &transactions::file_hashes(base)?,
            )
            || !same(get(tx, "files"), &desired)
        {
            return Err(err(
                "Binding may change namespace fields only, using the observed bank transaction",
            ));
        }
    } else if tx != &Value::Null
        || get(proof, "bank_project_id") != &Value::Null
        || get(proj, "action").as_str() != Some("create")
    {
        return Err(err("Only explicit create can precede bank initialization"));
    }
    Ok(p)
}
pub fn binding_guard(p: &Value, owner: Option<&[u8]>, marker: Option<&[u8]>) -> Result<()> {
    let proj = get(p, "project");
    let original = get(proj, "evidence");
    let root = Path::new(text(original, "repo_root")?);
    let dev = principal(root, &[], None, None)?;
    if get(&dev, "developer_id") != get(p, "developer_id")
        || get(&dev, "observed_sha256") != get(p, "developer_sha256")
    {
        return Err(err(
            "Developer selection/registry changed after preview; do not reattribute this binding",
        ));
    }
    let (data, _) = project::observe()?;
    if exists(&storage::paths("projects")?.2)? {
        return Err(err("Project registry is locked; retry after inspection"));
    }
    let proof = project::evidence(
        root,
        &data,
        None,
        project::DiscoveryOptions {
            owned_lock: owner,
            owned_transaction: marker,
            ..Default::default()
        },
    )?;
    let operation = get(&data, "operations").get(text(p, "operation_id")?);
    let repaired_alias = get(proj, "action").as_str() == Some("link")
        && get(original, "binding_source").as_str() == Some("historical-device-change")
        && get(&proof, "binding_source").as_str() == Some("exact")
        && get(&proof, "checkout_key") == get(original, "observed_checkout_key")
        && get(&proof, "observed_checkout_key") == get(original, "observed_checkout_key");
    let original_fields = original
        .as_object()
        .ok_or_else(|| err("Invalid scope binding evidence"))?;
    let changed = original_fields.iter().any(|(key, value)| {
        ![
            "bank_project_id",
            "bank_canonical_remote",
            "router_sha256",
            "binding_source",
            "continuity_issues",
        ]
        .contains(&key.as_str())
            && !(repaired_alias && key == "checkout_key")
            && proof.get(key).is_none_or(|current| !same(current, value))
    });
    let checkout = get(&data, "checkouts").get(text(&proof, "checkout_key")?);
    let entry = get(&data, "projects").get(text(proj, "project_id")?);
    let proposal_sha256 = sha(&encoded(proj)?);
    if operation
        .is_none_or(|op| get(op, "proposal_sha256").as_str() != Some(proposal_sha256.as_str()))
        || changed
        || checkout.is_none_or(|c| get(c, "project_id") != get(proj, "project_id"))
        || entry.is_none_or(|e| get(e, "canonical_remote") != get(original, "canonical_remote"))
        || (get(&proof, "bank_project_id") != get(original, "bank_project_id")
            && get(&proof, "bank_project_id") != get(proj, "project_id"))
    {
        return Err(err(
            "Binding decision no longer matches the registry/repository; preserve the pending bank",
        ));
    }
    Ok(())
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = bundle_value(prepared)?;
    let proj = get(p, "project");
    let root = Path::new(text(get(proj, "evidence"), "repo_root")?);
    let dev = principal(root, &[], None, None)?;
    if get(&dev, "developer_id") != get(p, "developer_id")
        || get(&dev, "observed_sha256") != get(p, "developer_sha256")
    {
        return Err(err(
            "Developer selection/registry changed after binding preview",
        ));
    }
    let tx = get(p, "transaction");
    let operation_id = text(p, "operation_id")?;
    let (registered, _) = project::observe()?;
    if get(&registered, "operations").get(operation_id).is_none()
        && get(p, "base_files") != &Value::Null
        && !same(
            &writes::snapshot(root, None)?.1,
            &transactions::file_hashes(get(p, "base_files"))?,
        )
    {
        return Err(err(
            "Bank changed after binding preview; preserve the edit and prepare again",
        ));
    }
    let audit_context = object([
        ("developer_id", get(&dev, "developer_id").clone()),
        ("project_id", get(proj, "project_id").clone()),
    ]);
    crate::security::audit(
        root,
        operation_id,
        "mutation",
        "started",
        &audit_context,
        &["external"],
        &[],
        &[],
        "bind",
        None,
    )?;
    let bundle_path = if tx != &Value::Null {
        let (journal, receipt) = transactions::locations(
            root,
            &repository_config::bank_path(root)?,
            text(tx, "operation_id")?,
        )?;
        let path = journal
            .parent()
            .ok_or_else(|| err("Binding journal has no parent"))?
            .join("binding.json");
        let raw = encoded(p)?;
        if let Some(info) = named(&path)? {
            if info.is_symlink() || read(&path, MAX_BINDING_BYTES)? != raw {
                return Err(err("Unknown binding recovery bundle; preserve it"));
            }
        } else if transactions::receipt_value(&receipt, tx)?.is_none() {
            storage::private_mkdir(
                path.parent()
                    .ok_or_else(|| err("Binding bundle has no parent"))?,
            )?;
            storage::replace_with_limit(&path, &raw, None, MAX_BINDING_BYTES)?;
        }
        Some(path)
    } else {
        None
    };
    let registry = project::apply(proj)?;
    writes::fault("binding-after-registry")?;
    let marker_path = repository_config::bank_path(root)?.join(transactions::MARKER);
    let marker = if tx != &Value::Null && exists(&marker_path)? {
        Some(transactions::marker_bytes(tx)?)
    } else {
        None
    };
    binding_guard(p, None, marker.as_deref())?;
    let bank_result = if tx != &Value::Null {
        if exists(&marker_path)? {
            let expected_marker = transactions::marker_bytes(tx)?;
            if named(&marker_path)?.is_some_and(|info| info.is_symlink())
                || read(&marker_path, transactions::MAX_MARKER_BYTES)? != expected_marker
            {
                return Err(err(
                    "Another interrupted operation is present; inspect/recover it first",
                ));
            }
            let recovery =
                transactions::recover_with_identity(root, Some("finish"), None, None, false)?;
            let guard = |owner: &[u8], marker: &[u8]| binding_guard(p, Some(owner), Some(marker));
            transactions::recover_with_identity(
                root,
                None,
                Some(get(&recovery, "proposal")),
                Some(&guard),
                false,
            )?
        } else {
            transactions::apply_with_identity(tx, |owner| binding_guard(p, Some(owner), None))?
        }
    } else {
        Value::Null
    };
    if let Some(path) = &bundle_path {
        if exists(path)? && get(&bank_result, "verified").as_bool() == Some(true) {
            transactions::cleanup(path, &encoded(p)?)?;
        }
    }
    let current = status(root, &[], None, None)?;
    let pending = get(get(get(&current, "project"), "evidence"), "bank_project_id") == &Value::Null;
    let still_current = get(&current, "project_id") == get(proj, "project_id")
        && (bank_result == Value::Null
            || get(&bank_result, "still_current").as_bool() == Some(true));
    let verified = !pending && get(&current, "resolved").as_bool() == Some(true) && still_current;
    if !pending && !verified {
        return Err(err(
            "Binding readback differs or was superseded; preserve current state and inspect",
        ));
    }
    let audit = if verified {
        crate::security::audit(
            root,
            operation_id,
            "mutation",
            "completed",
            &audit_context,
            &["external"],
            &[],
            &[],
            "bind",
            None,
        )?
    } else {
        Value::Null
    };
    let initialization = if pending {
        require(
            root,
            None,
            RequireOptions {
                initialize: true,
                ..Default::default()
            },
        )?
    } else {
        Value::Null
    };
    let changed = if bank_result == Value::Null {
        Value::Array(Vec::new())
    } else {
        get(&bank_result, "changed_memory_paths").clone()
    };
    let wrote =
        truthy(get(&registry, "writes_performed")) || truthy(get(&bank_result, "writes_performed"));
    let registry_verified = get(&registry, "verified").clone();
    Ok(result(
        "apply",
        [
            ("operation_id", string(operation_id)),
            ("project_id", get(proj, "project_id").clone()),
            ("developer_id", get(p, "developer_id").clone()),
            ("registry", registry),
            ("bank", bank_result),
            ("status", current),
            ("resolved", Value::Bool(verified)),
            ("initialization_pending", Value::Bool(pending)),
            ("initialization_context", initialization),
            (
                "recovery_bundle",
                bundle_path
                    .as_ref()
                    .map(|path| path_text(path).map(string))
                    .transpose()?
                    .unwrap_or(Value::Null),
            ),
            ("writes_performed", Value::Bool(wrote)),
            ("changed_memory_paths", changed),
            ("verified", Value::Bool(verified)),
            ("registry_verified", registry_verified),
            ("still_current", Value::Bool(still_current)),
            ("security_audit", audit),
        ],
    ))
}
/// A proposal's project ID: JSON null for none, otherwise the exact string.
pub(crate) fn optional_project(value: &Value) -> Result<Option<&str>> {
    if value == &Value::Null {
        Ok(None)
    } else {
        value
            .as_str()
            .map(Some)
            .ok_or_else(|| err("Operation project ID differs from the resolved repository"))
    }
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(e) => {
            let mut value = result(
                operation,
                [
                    ("error", string(e.to_string())),
                    ("verified", Value::Bool(false)),
                    ("inspection_required", Value::Bool(true)),
                ],
            );
            if let Err(error) = bank::merge(&mut value, &bank_capacity::details(&e)) {
                return crate::lifecycle::CommandResult::json(
                    result(
                        operation,
                        [
                            ("error", string(error.to_string())),
                            ("verified", Value::Bool(false)),
                        ],
                    ),
                    1,
                    json::DEFAULT,
                );
            }
            (value, 1, json::DEFAULT)
        }
    };
    crate::lifecycle::CommandResult::json(value, code, style)
}
