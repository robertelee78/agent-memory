//! Native omp boundary CLI and conservative bank-wide lifecycle coordination.
//!
//! Baseline `omp_boundary.py`. Directly called ported callees: `bank_writes`,
//! `identity_git`, `installation`, `layout_recovery_state`, `local_registry`,
//! `omp_installation`, `private_state`, `project_identity`,
//! `repository_config`, `scope_binding`, `turn_check`, `turn_sources`.
//! `omp_transport` and `omp_stream` are reached through the registration
//! traits below; `recovery_frontier` through the existing
//! `bank_writes::RecoveryFrontier` registration. A missing registration is a
//! typed refusal, never an empty inventory.
use crate::bank_writes;
use crate::codec::json::{self, Value};
use crate::developer_identity::sha256_hex;
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain, Identity};
use crate::installation::{self, Environment, Services};
use crate::layout_recovery_state;
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::local_registry;
use crate::omp_installation::{self, HeldSource};
use crate::private_state;
use crate::project_identity;
use crate::repository_config;
use crate::scope_binding::{self as scope, PrivateEnrollmentContext};
use crate::turn_sources::{self, Namespace};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const HISTORICAL_CAPABILITY: &str = "omp-harness-v1";
pub const CAPABILITY: &str = "omp-stored-source-v2";
pub const MARKER_SCHEMA: &str = "agent-memory/turn-sources-compat-v1";
pub const BINDING_SCHEMA: &str = "agent-memory/omp-bank-binding-v1";
pub const REGISTRY_SCHEMA: &str = "agent-memory/omp-bank-registry-v1";
pub const DRAIN_SCHEMA: &str = "agent-memory/omp-drain-v1";
pub const STREAM_DRAIN_SCHEMA: &str = "agent-memory/omp-stream-drain-v1";
pub const TRANSITION_SCHEMA: &str = "agent-memory/omp-native-transition-v1";
/// Marker, binding, index references and drain proofs (baseline `4096`).
pub const MAX_RECORD: usize = 4096;
/// The omp bank registry read bound (baseline `1024 * 1024`).
pub const MAX_REGISTRY: usize = 1024 * 1024;
/// Registered native repositories (baseline `1024`).
pub const MAX_REPOSITORIES: usize = 1024;
/// Private bank namespaces and shared source namespaces (baseline `65536`).
pub const MAX_NAMESPACES: usize = 65536;
/// Files in one drain inventory (baseline `262144`).
pub const MAX_DRAIN_FILES: usize = 262144;
/// Reviewed layout quiescence attestations (baseline `65536`).
pub const MAX_QUIESCENCE: usize = 65536;
/// `--stdin` assessment input (baseline `65536`).
pub const MAX_INPUT: usize = 65536;
const MARKER_FILE: &str = "turn-sources-compat-v1.json";
const BINDING_FILE: &str = "omp-bank-binding-v1.json";
const TRANSPORT_LOCK: &str = "omp-transport.lock";
const TRANSITION_FILE: &str = "omp-transition.json";
const NATIVE_STATE: [&str; 5] = [
    BINDING_FILE,
    MARKER_FILE,
    "omp-source-v1",
    "omp-stream-v1",
    "omp-stream-session-locks",
];
const STREAM_STATE: [&str; 2] = ["omp-stream-v1", "omp-stream-session-locks"];
const FAILURE_REASON: &str = "Native source work remains pending; inspect owned installation, binding and retained sources. No completion is inferred.";

/// Baseline `omp_transport` callees. Implemented by `omp_transport`.
pub trait OmpTransport: Send + Sync {
    /// `pending_inventory(repo)`: `(rows, faults)`.
    fn pending_inventory(
        &self,
        repo: &Path,
        runtime_root: &Path,
    ) -> Result<(Vec<Value>, Vec<Value>)>;
    /// `all_segments(repo)`: `(segment directory, cursor)` pairs.
    fn all_segments(&self, repo: &Path, runtime_root: &Path) -> Result<Vec<(PathBuf, Value)>>;
    /// `MAX_PAYLOAD`: the per-file read bound of retained transport evidence.
    fn max_payload(&self) -> usize;
    fn list_pending(&self, repo: &Path, offset: i64, runtime_root: &Path) -> Result<Value>;
    fn source(
        &self,
        repo: &Path,
        session: Option<&str>,
        lifecycle: Option<&str>,
        identifier: &str,
        runtime_root: &Path,
    ) -> Result<Value>;
    fn complete(&self, repo: &Path, value: &Value, runtime_root: &Path) -> Result<Value>;
}
static TRANSPORT: RwLock<Option<Arc<dyn OmpTransport>>> = RwLock::new(None);
pub fn register_omp_transport(provider: Option<Arc<dyn OmpTransport>>) {
    *TRANSPORT.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub(crate) fn omp_transport_provider() -> Result<Arc<dyn OmpTransport>> {
    TRANSPORT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| {
            ContractError::missing_provider(
                "omp_transport provider is not registered; native source work remains pending",
            )
        })
}

/// Baseline `omp_stream` callees. Implemented by `omp_stream`.
pub trait OmpStream: Send + Sync {
    /// `drain_state(repo, base=base, read_only=…)`: the finite stream inventory
    /// with `pending` and `inventory_sha256`.
    fn drain_state(
        &self,
        repo: &Path,
        base: &Path,
        read_only: bool,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// `main(argv)` for `omp-boundary --stream` (the flag already removed);
    /// frames go to the process stdout directly.
    fn main(&self, argv: &[String], prog: &str) -> CommandResult;
}
static STREAM: RwLock<Option<Arc<dyn OmpStream>>> = RwLock::new(None);
pub fn register_omp_stream(provider: Option<Arc<dyn OmpStream>>) {
    *STREAM.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub(crate) fn omp_stream_provider() -> Result<Arc<dyn OmpStream>> {
    STREAM
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| {
            ContractError::missing_provider(
                "omp_stream provider is not registered; native source work remains pending",
            )
        })
}

fn err(message: impl Into<String>) -> ContractError {
    ContractError::new(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn contract(error: installation::Error) -> ContractError {
    match error {
        installation::Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    }
}
fn omp_contract(error: omp_installation::Error) -> ContractError {
    match error {
        omp_installation::Error::Contract(error) => error,
        missing @ omp_installation::Error::MissingActivation => {
            ContractError::missing_provider(missing.to_string())
        }
    }
}
fn path_text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "native source path")
}
fn path_value(path: &Path) -> Result<Value> {
    Ok(string(path_text(path)?))
}
fn leaf(path: &Path) -> Result<&OsStr> {
    path.file_name()
        .ok_or_else(|| err("Native source path requires a filename"))
}
fn parent(path: &Path) -> Result<&Path> {
    path.parent()
        .ok_or_else(|| err("Native source path requires a parent"))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Int(v) => v.as_str() != "0",
        Value::Float(v) => *v != 0.0,
    }
}
/// Python dict/list equality: independent of object key order.
fn same(left: &Value, right: &Value) -> bool {
    installation::same(left, right)
}
fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// `str(uuid.UUID(value)) == value`: the canonical lowercase hyphenated form.
fn canonical_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
/// Absolute with no `..` component (baseline `is_absolute()` and parts check).
fn plain_absolute(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute() && !path.components().any(|c| c == Component::ParentDir)
}
fn absent(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}
/// `os.lstat`: ancestors resolve like the baseline path API; the leaf never follows.
fn named(path: &Path) -> Result<Option<Identity>> {
    let directory = fs::canonical_path(parent(path)?, false)?;
    let chain = match DirectoryChain::open(&directory) {
        Ok(chain) => chain,
        Err(error) if absent(&error) => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    match chain.directory().stat_at(leaf(path)?) {
        Ok(info) => Ok(Some(info)),
        Err(error) if absent(&error) => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}
/// `os.path.lexists`.
fn lexists(path: &Path) -> Result<bool> {
    Ok(named(path)?.is_some())
}
/// `Path.is_dir()`: follows a final symlink.
fn is_dir(path: &Path) -> Result<bool> {
    let resolved = fs::canonical_path(path, false)?;
    if resolved == Path::new("/") {
        return Ok(true);
    }
    Ok(named(&resolved)?.is_some_and(Identity::is_directory))
}
/// A bounded sorted directory listing; `None` when the bound is exceeded.
fn listing(path: &Path, maximum: usize) -> Result<Option<Vec<OsString>>> {
    let chain = DirectoryChain::open(path).map_err(io_error)?;
    let names = chain
        .directory()
        .entries_bounded(maximum)
        .map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    Ok(names.map(|mut names| {
        names.sort();
        names
    }))
}
fn sha_of(path: &Path, maximum: usize, missing: &str) -> Result<String> {
    let raw = turn_sources::read_bytes(path, maximum)?.ok_or_else(|| err(missing))?;
    Ok(sha256_hex(&raw))
}
fn relative(path: &Path, base: &Path) -> Result<Value> {
    let rest = path
        .strip_prefix(base)
        .map_err(|_| err("Native evidence lies outside its private base"))?;
    path_value(rest)
}
fn strings(value: &Value, refusal: &str) -> Result<Vec<String>> {
    match value {
        Value::Null => Ok(vec![]),
        Value::Array(items) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(|| err(refusal)))
            .collect(),
        _ => Err(err(refusal)),
    }
}
fn set_field(value: &mut Value, key: &str, replacement: Value) {
    if let Value::Object(fields) = value {
        if let Some((_, current)) = fields.iter_mut().find(|(name, _)| name == key) {
            *current = replacement;
        } else {
            fields.push((key.to_owned(), replacement));
        }
    }
}
fn runtime_root() -> Result<PathBuf> {
    crate::cli::pack_root()
}

fn marker_value(token: &str) -> Value {
    object([
        ("schema", string(MARKER_SCHEMA)),
        ("minimum_reader", string(token)),
        ("harnesses", Value::Array(vec![string("omp")])),
    ])
}
fn binding_value(root: &Path, bank: &Path, base: &Path, project_id: &Value) -> Result<Value> {
    Ok(object([
        ("schema", string(BINDING_SCHEMA)),
        ("repository", path_value(root)?),
        ("bank_path", path_value(bank)?),
        ("private_base", path_value(base)?),
        ("project_id", project_id.clone()),
    ]))
}

/// Baseline `_base(repo)`: the bank's private-state base.
pub fn base(repo: &Path) -> Result<PathBuf> {
    let (root, bank) = bank_writes::bank_path(repo)?;
    let (operations, _) = bank_writes::state_paths(&root, &bank, &crate::host::uuid4()?)?;
    Ok(parent(&operations)?.to_path_buf())
}
/// Baseline `_base(repo, enrollment_context=…)`: the live locator of an
/// already-held private enrollment context.
pub fn enrollment_base(
    repo: &Path,
    context: &PrivateEnrollmentContext,
    runtime_root: &Path,
) -> Result<PathBuf> {
    let location = scope::observe_enrollment_location(repo, Some(context), runtime_root)?
        .ok_or_else(|| err("Observer locator reuse is outside its live operation"))?;
    Ok(parent(&location.2)?.to_path_buf())
}

/// Baseline `_marker(base)`: the minimum reader token, or `None` when absent.
pub fn marker(base: &Path) -> Result<Option<&'static str>> {
    let Some(value) = turn_sources::read(&base.join(MARKER_FILE), MAX_RECORD)? else {
        return Ok(None);
    };
    for token in [HISTORICAL_CAPABILITY, CAPABILITY] {
        if same(&value, &marker_value(token)) {
            return Ok(Some(token));
        }
    }
    Err(err("Unknown or malformed source compatibility marker"))
}

/// The observer context shared with omp_installation/omp_stream callbacks.
fn context_value(context: &PrivateEnrollmentContext) -> Result<Value> {
    Ok(object([
        ("root", path_value(context.root())?),
        ("bank", path_value(context.bank())?),
        ("project_id", context.project_id().clone()),
        ("scope", context.scope()),
        ("held", Value::Bool(context.held())),
    ]))
}

/// Entry checks of baseline `_observe_marker`, minted inside this boundary.
fn checked_marker(
    repo: &Path,
    held: Option<&dyn HeldSource>,
    runtime_root: &Path,
) -> Result<(PrivateEnrollmentContext, PathBuf, Value)> {
    let context = scope::private_enrollment_context(repo, runtime_root)?;
    let paths =
        layout_recovery_state::_paths_for_bank(context.root(), context.bank(), None, runtime_root)?;
    let base = parent(&paths.2)?.to_path_buf();
    if let Some(held) = held {
        if !turn_sources::source_barrier_held(&base) {
            return Err(err(
                "Native marker source barrier is not owned by this context",
            ));
        }
        held.recheck().map_err(omp_contract)?;
    }
    let expected = binding_value(context.root(), context.bank(), &base, context.project_id())?;
    verify_marker_binding(&base, &expected, context.root())?;
    Ok((context, base, expected))
}
fn verify_marker_binding(base: &Path, expected: &Value, root: &Path) -> Result<()> {
    if marker(base)? != Some(CAPABILITY) || !same(&bank_binding(base)?, expected) {
        return Err(err(
            "Native stored-source activation is required; preserve existing private state",
        ));
    }
    let registered = registry()?;
    let saved = get(get(&registered, "private_bases"), path_text(root)?);
    if saved.as_str() != Some(path_text(base)?) {
        return Err(err(
            "Native source bank registration differs from its protected binding",
        ));
    }
    Ok(())
}

/// Baseline `_observe_marker`: verify the marker/binding/registration at entry,
/// run `body` inside the live private enrollment read scope, and freshly
/// recheck the marker binding at exit, also after a refusal. No writes.
pub fn observe_marker<T>(
    repo: &Path,
    held: Option<&dyn HeldSource>,
    runtime_root: &Path,
    body: impl FnOnce(&PrivateEnrollmentContext) -> Result<T>,
) -> Result<T> {
    let (context, base, expected) = checked_marker(repo, held, runtime_root)?;
    let outcome = scope::private_enrollment_reads(&context, body);
    verify_marker_binding(&base, &expected, context.root())?;
    outcome
}

/// Baseline `verify_marker(repo)`: observer verification never enrolls or upgrades.
pub fn verify_marker(repo: &Path, runtime_root: &Path) -> Result<PrivateEnrollmentContext> {
    let (context, _, _) = checked_marker(repo, None, runtime_root)?;
    scope::private_enrollment_reads(&context, |_| Ok(()))?;
    Ok(context)
}

/// The `OmpActivation::observe_marker` half owned by this module: the callback
/// receives `{root, bank, project_id, scope, held}`.
pub fn observe_marker_operation(
    repo: &Path,
    held: Option<&dyn HeldSource>,
    runtime_root: &Path,
    operation: &mut dyn FnMut(&Value) -> omp_installation::Result<()>,
) -> omp_installation::Result<()> {
    let mut failure = None;
    observe_marker(repo, held, runtime_root, |context| {
        let value = context_value(context)?;
        // A refused operation still receives the enrollment and marker rechecks.
        if let Err(error) = operation(&value) {
            failure = Some(error);
        }
        Ok(())
    })
    .map_err(omp_installation::Error::Contract)?;
    failure.map_or(Ok(()), Err)
}

/// Baseline `_activate_marker`: lifecycle-only mutation under the exclusive barrier.
pub fn activate_marker(
    repo: &Path,
    fresh_only: bool,
    runtime_root: &Path,
) -> Result<PrivateEnrollmentContext> {
    let barrier = turn_sources::source_barrier(repo, true, false, None, runtime_root)?;
    let context = scope::private_enrollment_context(repo, runtime_root)?;
    let root = context.root().to_path_buf();
    let base = base(&root)?;
    let current = marker(&base)?;
    if fresh_only {
        let mut existing = current.is_some();
        for name in ["omp-source-v1", "omp-stream-v1", "omp-stream-session-locks"] {
            existing |= lexists(&base.join(name))?;
        }
        if existing {
            return Err(err(
                "Existing native state requires explicit stored-source lifecycle upgrade",
            ));
        }
    }
    let binding = binding_value(&root, context.bank(), &base, context.project_id())?;
    record_bank(&root)?;
    if !turn_sources::exclusive(&base.join(BINDING_FILE), &binding, MAX_RECORD)?
        && !same(&bank_binding(&base)?, &binding)
    {
        return Err(err("Native source bank/private-state binding changed"));
    }
    let path = base.join(MARKER_FILE);
    match current {
        None => {
            if !turn_sources::exclusive(&path, &marker_value(CAPABILITY), MAX_RECORD)? {
                return Err(err(
                    "Source compatibility marker appeared during activation; preserve it",
                ));
            }
        }
        Some(token) if token != CAPABILITY => {
            let observed = sha_of(
                &path,
                MAX_RECORD,
                "Source compatibility marker changed during activation",
            )?;
            layout_recovery_state::replace(&path, &marker_value(CAPABILITY), Some(&observed))?;
        }
        Some(_) => {}
    }
    scope::recheck_private_enrollment(&context)?;
    let verified = verify_marker(&root, runtime_root)?;
    barrier.recheck()?;
    Ok(verified)
}

/// Baseline `stream_startup`: fresh enrollment is a lifecycle transaction,
/// never an observer write. `None` outside a Git repository with a bank.
pub fn stream_startup(
    repo: &Path,
    session: &str,
    instance_id: &str,
    lifecycle: &str,
    receipt: &Value,
    runtime_root: &Path,
) -> Result<Option<Value>> {
    let cwd = fs::canonical_path(repo, true)?;
    let arguments: Vec<OsString> = ["rev-parse", "--show-toplevel"]
        .iter()
        .map(OsString::from)
        .collect();
    let top = crate::identity_git::git(&cwd, &arguments, true, false).map_err(io_error)?;
    let Some(top) = top else {
        for ancestor in cwd.ancestors() {
            if lexists(&ancestor.join(".git"))? {
                return Err(err(
                    "Native repository is unavailable; preserve pending sources",
                ));
            }
        }
        return Ok(None);
    };
    let root = fs::canonical_path(Path::new(&top), true)?;
    if !is_dir(&repository_config::bank_path(&root)?)? {
        return Ok(None);
    }
    let source = get(receipt, "source");
    let binding = get(receipt, "omp_binding");
    let digest = text(source, "artifact_digest")
        .ok_or_else(|| err("Native stream requires the renewed stored-source installation"))?;
    let agent_dir = text(binding, "agent_dir")
        .ok_or_else(|| err("Native stream requires the renewed stored-source installation"))?;
    let current = omp_installation::observer_binding(digest, agent_dir).map_err(omp_contract)?;
    if !same(&current, receipt) || text(binding, "peer_reader_capability") != Some(CAPABILITY) {
        return Err(err(
            "Native stream requires the renewed stored-source installation",
        ));
    }
    turn_sources::bounded(&string(session), 512)?;
    turn_sources::bounded(&string(lifecycle), 512)?;
    if !canonical_uuid(instance_id) {
        return Err(err("Invalid native stream instance"));
    }
    let present = {
        let shared = turn_sources::source_barrier(&root, false, false, None, runtime_root)?;
        let present = marker(&base(&root)?)?.is_some();
        let context = if present {
            Some(verify_marker(&root, runtime_root)?)
        } else {
            None
        };
        shared.recheck()?;
        context
    };
    let context = match present {
        Some(context) => context,
        None => {
            let barrier = turn_sources::source_barrier(&root, true, false, None, runtime_root)?;
            let context = scope::private_enrollment_context(&root, runtime_root)?;
            let mut required: BTreeSet<String> = [HISTORICAL_CAPABILITY, CAPABILITY]
                .iter()
                .map(|token| (*token).to_owned())
                .collect();
            if context.held() {
                required.insert("layout-frontier-v1".to_owned());
            }
            let supported = private_state::compatibility(Some(source))?;
            if !required.is_subset(&supported) {
                return Err(err(
                    "Native fresh-bank enrollment lacks the bank-local private-state readers",
                ));
            }
            drop(context);
            if marker(&base(&root)?)?.is_none() {
                activate_marker(&root, true, runtime_root)?;
            }
            let verified = verify_marker(&root, runtime_root)?;
            barrier.recheck()?;
            verified
        }
    };
    let scope_value = context.scope();
    Ok(Some(object([
        ("project_id", context.project_id().clone()),
        ("repo_root", path_value(context.root())?),
        ("bank_path", path_value(context.bank())?),
        ("checkout_key", get(&scope_value, "checkout_key").clone()),
        ("artifact_digest", string(digest)),
        ("installation_id", get(receipt, "installation_id").clone()),
        ("initial_lifecycle", string(lifecycle)),
    ])))
}

fn empty_registry() -> Value {
    object([
        ("schema", string(REGISTRY_SCHEMA)),
        ("repositories", Value::Array(vec![])),
        ("private_bases", Value::Object(vec![])),
    ])
}
fn validate_registry(value: &Value) -> Result<()> {
    let refusal = || err("Unverifiable omp bank registry; preserve private state");
    let fields = value.as_object().ok_or_else(refusal)?;
    if fields.len() != 3
        || ["schema", "repositories", "private_bases"]
            .iter()
            .any(|key| value.get(key).is_none())
        || text(value, "schema") != Some(REGISTRY_SCHEMA)
    {
        return Err(refusal());
    }
    let repositories = get(value, "repositories").as_array().ok_or_else(refusal)?;
    if repositories.len() > MAX_REPOSITORIES {
        return Err(refusal());
    }
    let mut names = Vec::with_capacity(repositories.len());
    for repository in repositories {
        let name = repository.as_str().ok_or_else(refusal)?;
        if !plain_absolute(name) {
            return Err(refusal());
        }
        names.push(name);
    }
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    if !unique.iter().copied().eq(names.iter().copied()) {
        return Err(refusal());
    }
    let bases = get(value, "private_bases")
        .as_object()
        .ok_or_else(refusal)?;
    let keys: BTreeSet<&str> = bases.iter().map(|(key, _)| key.as_str()).collect();
    if keys.len() != bases.len() || keys != unique {
        return Err(refusal());
    }
    for (_, base) in bases {
        if !base.as_str().is_some_and(plain_absolute) {
            return Err(refusal());
        }
    }
    Ok(())
}

/// Baseline `_registry()`: the protected omp bank registry.
pub fn registry() -> Result<Value> {
    let (_, path, _) = local_registry::paths("omp-banks")?;
    let Some(value) = turn_sources::read(&path, MAX_REGISTRY)? else {
        return Ok(empty_registry());
    };
    validate_registry(&value)?;
    Ok(value)
}

/// Baseline `record_bank(repo)`: register the bank's protected private base once.
pub fn record_bank(repo: &Path) -> Result<()> {
    let root = fs::canonical_path(repo, false)?;
    let root_text = path_text(&root)?.to_owned();
    let private_base = path_text(&base(repo)?)?.to_owned();
    let value = registry()?;
    let saved = |value: &Value| {
        get(get(value, "private_bases"), &root_text)
            .as_str()
            .map(str::to_owned)
    };
    let listed = |value: &Value| {
        get(value, "repositories")
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(&root_text)))
    };
    if listed(&value) {
        if saved(&value).as_deref() != Some(private_base.as_str()) {
            return Err(err(
                "Native protected private-state location changed; preserve its original binding",
            ));
        }
        return Ok(());
    }
    let (directory, registry_path, lock) = local_registry::paths("omp-banks")?;
    local_registry::private_mkdir(&directory)?;
    local_registry::exclusive(
        &lock,
        &crate::host::uuid4()?,
        "omp-enrollment",
        crate::host::Sources::default(),
        |_| {
            let mut value = registry()?;
            let observed =
                turn_sources::read_bytes(&registry_path, MAX_REGISTRY)?.map(|raw| sha256_hex(&raw));
            let mut repositories: BTreeSet<String> = strings(
                get(&value, "repositories"),
                "Unverifiable omp bank registry; preserve private state",
            )?
            .into_iter()
            .collect();
            repositories.insert(root_text.clone());
            if repositories.len() > MAX_REPOSITORIES {
                return Err(err("omp bank registry capacity reached"));
            }
            set_field(
                &mut value,
                "repositories",
                Value::Array(repositories.into_iter().map(string).collect()),
            );
            let mut bases = get(&value, "private_bases").clone();
            set_field(&mut bases, &root_text, string(&private_base));
            set_field(&mut value, "private_bases", bases);
            local_registry::replace(
                &registry_path,
                &turn_sources::encoded(&value)?,
                observed.as_deref(),
            )
        },
    )
}

/// Baseline `_bank_binding(base)`: the exact protected bank binding.
pub fn bank_binding(base: &Path) -> Result<Value> {
    let value = turn_sources::read(&base.join(BINDING_FILE), MAX_RECORD)?
        .ok_or_else(|| err("Unknown native bank binding"))?;
    let fields = [
        "schema",
        "repository",
        "bank_path",
        "private_base",
        "project_id",
    ];
    if value
        .as_object()
        .is_none_or(|pairs| pairs.len() != fields.len())
        || fields
            .iter()
            .any(|name| value.get(name).and_then(Value::as_str).is_none())
        || text(&value, "schema") != Some(BINDING_SCHEMA)
    {
        return Err(err("Unknown native bank binding"));
    }
    for name in ["repository", "bank_path", "private_base"] {
        let supplied = text(&value, name).unwrap_or_default();
        let path = Path::new(supplied);
        if !plain_absolute(supplied) || fs::canonical_path(path, false)? != path {
            return Err(err("Native bank binding has a redirected location"));
        }
    }
    let bank_path = text(&value, "bank_path").unwrap_or_default();
    let namespace = base.file_name().and_then(OsStr::to_str);
    if text(&value, "private_base") != Some(path_text(base)?)
        || namespace != Some(sha256_hex(bank_path.as_bytes()).as_str())
        || !Path::new(bank_path).starts_with(text(&value, "repository").unwrap_or_default())
        || !text(&value, "project_id").is_some_and(canonical_uuid)
    {
        return Err(err(
            "Native bank binding differs from its protected namespace",
        ));
    }
    Ok(value)
}

/// Baseline `_has_native_source_state(base)`: cheap native evidence names.
pub fn has_native_source_state(base: &Path) -> Result<bool> {
    for name in NATIVE_STATE {
        if lexists(&base.join(name))? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn registered_bases(registered: &Value) -> Result<Vec<(String, PathBuf)>> {
    let bases = get(registered, "private_bases")
        .as_object()
        .ok_or_else(|| err("Unverifiable omp bank registry; preserve private state"))?;
    bases
        .iter()
        .map(|(repo, base)| {
            base.as_str()
                .map(|base| (repo.clone(), PathBuf::from(base)))
                .ok_or_else(|| err("Unverifiable omp bank registry; preserve private state"))
        })
        .collect()
}

fn orphan_pending(base: &Path) -> Result<()> {
    let pending = base.join("turn-sources-v2").join("pending");
    if !lexists(&pending)? {
        return Ok(());
    }
    fs::trust::state(&pending)?;
    let names = listing(&pending, turn_sources::MAX_TURNS)?
        .ok_or_else(|| err("Private pending source inventory exceeds bound"))?;
    for name in names {
        let value = turn_sources::read(&pending.join(&name), MAX_RECORD)?
            .ok_or_else(|| err("Private pending source inventory is unverifiable"))?;
        if value.as_object().is_none()
            || text(&value, "schema") != Some("agent-memory/turn-reference-v2")
        {
            return Err(err("Private pending source inventory is unverifiable"));
        }
        if text(&value, "harness") == Some("omp") {
            return Err(err(
                "Orphan indexed native source lacks its bound repository",
            ));
        }
    }
    Ok(())
}

/// Baseline `_source_state_bindings(installation, registered)`: every protected
/// native binding under every known private-state root.
pub fn source_state_bindings(
    installation: Option<&Value>,
    registered: &Value,
) -> Result<BTreeSet<PathBuf>> {
    let saved = registered_bases(registered)?;
    let current_root = bank_writes::state_root(None)?;
    let mut state_roots = BTreeSet::from([current_root.clone()]);
    for (_, private_base) in &saved {
        let state_root = private_base
            .ancestors()
            .nth(2)
            .ok_or_else(|| err("Unverifiable omp bank registry; preserve private state"))?;
        state_roots.insert(state_root.to_path_buf());
    }
    if let Some(runtime_state) = installation
        .map(|value| get(value, "runtime_state"))
        .filter(|value| truthy(value))
    {
        let runtime_state = runtime_state
            .as_str()
            .ok_or_else(|| err("Unverifiable saved runtime state"))?;
        state_roots.insert(PathBuf::from(runtime_state));
    }
    let mut roots = BTreeSet::new();
    for state_root in &state_roots {
        fs::trust::inspect(state_root, fs::trust::InspectOptions::default())?;
        if !lexists(state_root)? {
            if saved.iter().any(|(_, base)| base.starts_with(state_root)) {
                return Err(err("A saved native private-state root is unavailable"));
            }
            continue;
        }
        let writes = state_root.join("writes");
        if !lexists(&writes)? {
            continue;
        }
        fs::trust::state(&writes)?;
        let names = listing(&writes, MAX_NAMESPACES)?
            .ok_or_else(|| err("Private bank inventory exceeds its complete bound"))?;
        for name in names {
            let name = name
                .to_str()
                .filter(|name| hex64(name))
                .ok_or_else(|| err("Unknown private bank namespace"))?;
            let base = writes.join(name);
            fs::trust::state(&base)?;
            if lexists(&base.join(BINDING_FILE))? {
                let binding = bank_binding(&base)?;
                roots.insert(PathBuf::from(
                    text(&binding, "repository").unwrap_or_default(),
                ));
                if base != current_root.join("writes").join(name) {
                    return Err(err(
                        "Native bank remains bound to another protected private-state root",
                    ));
                }
                continue;
            }
            if has_native_source_state(&base)? {
                return Err(err(
                    "Native marker/source inventory lacks its bound repository",
                ));
            }
            orphan_pending(&base)?;
        }
    }
    for (repo, private_base) in &saved {
        if !is_dir(private_base)?
            || text(&bank_binding(private_base)?, "repository") != Some(repo.as_str())
        {
            return Err(err("Registered native source binding is unavailable"));
        }
    }
    Ok(roots)
}

fn installation_repositories(installation: &Value, keys: &[&str]) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for key in keys {
        for value in strings(get(installation, key), "Unverifiable saved bank binding")? {
            result.push(PathBuf::from(value));
        }
    }
    Ok(result)
}
fn binding_rows(installation: &Value, key: &str) -> Result<Vec<PathBuf>> {
    let rows = match get(installation, key) {
        Value::Null => return Ok(vec![]),
        Value::Array(rows) => rows,
        _ => return Err(err("Unverifiable saved bank binding")),
    };
    rows.iter()
        .map(|row| {
            row.as_object()
                .and(text(row, "repository"))
                .map(PathBuf::from)
                .ok_or_else(|| err("Unverifiable saved bank binding"))
        })
        .collect()
}

/// Baseline `source_capability_repositories(installation)`: accessible ordinary
/// locations and every protected native binding.
pub fn source_capability_repositories(installation: Option<&Value>) -> Result<BTreeSet<PathBuf>> {
    let registered = registry()?;
    let mut roots: BTreeSet<PathBuf> = strings(
        get(&registered, "repositories"),
        "Unverifiable omp bank registry; preserve private state",
    )?
    .into_iter()
    .map(PathBuf::from)
    .collect();
    roots.extend(source_state_bindings(installation, &registered)?);
    let (projects, _) = project_identity::observe()?;
    let checkouts = get(&projects, "checkouts")
        .as_object()
        .ok_or_else(|| err("Invalid project registry"))?;
    let arguments: Vec<OsString> = ["worktree", "list", "--porcelain"]
        .iter()
        .map(OsString::from)
        .collect();
    for (_, checkout) in checkouts {
        for location in strings(get(checkout, "locations"), "Invalid project registry")? {
            let path = PathBuf::from(location);
            // Missing ordinary history is not a protected binding.
            if !lexists(&path)? {
                continue;
            }
            if !is_dir(&path)? {
                return Err(err("Registered Git location is unavailable"));
            }
            let listed =
                crate::identity_git::git(&path, &arguments, true, false).map_err(io_error)?;
            let Some(listed) = listed else {
                if lexists(&path.join(".git"))? || path.file_name() == Some(OsStr::new(".git")) {
                    return Err(err(
                        "Registered Git location cannot list native bank worktrees",
                    ));
                }
                roots.insert(path);
                continue;
            };
            let listed = crate::host::text(&listed, "Git worktree list")?;
            for line in listed.lines() {
                if let Some(worktree) = line.strip_prefix("worktree ") {
                    let worktree = PathBuf::from(worktree);
                    if lexists(&worktree)? {
                        roots.insert(worktree);
                    }
                }
            }
        }
    }
    if let Some(installation) = installation {
        roots.extend(installation_repositories(
            installation,
            &["rollback_repositories", "observed_repositories"],
        )?);
        roots.extend(binding_rows(installation, "bank_bindings")?);
    }
    Ok(roots)
}

/// Baseline `_omp_index(repo)`: the validated omp references of the shared index.
pub fn omp_index(repo: &Path, runtime_root: &Path) -> Result<Vec<(PathBuf, Value)>> {
    let directory = base(repo)?.join("turn-sources-v2").join("pending");
    if !lexists(&directory)? {
        return Ok(vec![]);
    }
    fs::trust::state(&directory)?;
    let context = scope::private_enrollment_context(repo, runtime_root)?;
    let refusal = || err("Unverifiable shared source index inventory");
    let names = listing(&directory, turn_sources::MAX_TURNS)?.ok_or_else(refusal)?;
    let mut result = Vec::new();
    for name in names {
        let name = name.to_str().ok_or_else(refusal)?;
        let number = name.strip_suffix(".json").ok_or_else(refusal)?;
        let canonical = number == "0"
            || (!number.starts_with('0')
                && !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit()));
        if !canonical
            || number
                .parse::<usize>()
                .map_or(true, |n| n >= turn_sources::MAX_TURNS)
        {
            return Err(refusal());
        }
        let path = directory.join(name);
        let value = turn_sources::read(&path, MAX_RECORD)?.ok_or_else(refusal)?;
        turn_sources::validate_reference(&value, context.root(), context.project_id())?;
        if text(&value, "harness") == Some("omp") {
            result.push((path, value));
        }
    }
    scope::recheck_private_enrollment(&context)?;
    Ok(result)
}

/// Baseline `omp_transport.directories(path, '[0-9a-f]{64}')`.
fn hash_directories(path: &Path) -> Result<Vec<PathBuf>> {
    if !lexists(path)? {
        return Ok(vec![]);
    }
    fs::trust::state(path)?;
    let refusal = || err("Unknown or oversized native source inventory");
    let names = listing(path, MAX_NAMESPACES)?.ok_or_else(refusal)?;
    let mut values = Vec::with_capacity(names.len());
    for name in names {
        let name = name
            .to_str()
            .filter(|name| hex64(name))
            .ok_or_else(refusal)?;
        let child = path.join(name);
        fs::trust::state(&child)?;
        values.push(child);
    }
    Ok(values)
}

fn omp_namespace(directory: &Path, refusal: &str) -> Result<bool> {
    for i in 0..turn_sources::MAX_SOURCES {
        let path = directory.join(format!("source-{i}.json"));
        let Some(candidate) = turn_sources::read(&path, MAX_RECORD)? else {
            continue;
        };
        if candidate.as_object().is_none() {
            return Err(err(refusal));
        }
        if text(&candidate, "harness") == Some("omp") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The complete sorted evidence walk of one drain root.
fn walk_evidence(
    root: &Path,
    base: &Path,
    maximum: usize,
    count: &mut usize,
    pins: &mut Vec<Value>,
) -> Result<()> {
    fs::trust::state(root)?;
    let unreadable = || err("Source drain inventory is unreadable; preserve all private evidence");
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        fs::trust::state(&directory)?;
        let names = listing(&directory, MAX_DRAIN_FILES)
            .map_err(|_| unreadable())?
            .ok_or_else(|| err("Source drain inventory exceeds bound"))?;
        let chain = DirectoryChain::open(&directory).map_err(|_| unreadable())?;
        let mut directories = Vec::new();
        let mut files = Vec::new();
        for name in names {
            let info = chain.directory().stat_at(&name).map_err(|_| unreadable())?;
            if info.is_directory() {
                directories.push(directory.join(&name));
            } else {
                files.push(name);
            }
        }
        for child in &directories {
            fs::trust::state(child)?;
        }
        for name in files {
            *count += 1;
            if *count > MAX_DRAIN_FILES {
                return Err(err("Source drain inventory exceeds bound"));
            }
            // Lock files are metadata, never evidence of completion.
            if name.as_bytes().ends_with(b".lock") {
                continue;
            }
            let path = directory.join(&name);
            let raw = turn_sources::read_bytes(&path, maximum)?.ok_or_else(unreadable)?;
            pins.push(Value::Array(vec![
                relative(&path, base)?,
                string(sha256_hex(&raw)),
            ]));
        }
        // Top-down, sorted: the first child directory is visited next.
        for child in directories.into_iter().rev() {
            stack.push(child);
        }
    }
    Ok(())
}

/// Baseline `_drain_inventory(repo)`: the bound complete evidence enumeration,
/// including orphan shared namespaces, and its digest.
pub fn drain_inventory(repo: &Path, runtime_root: &Path) -> Result<(Value, String)> {
    let transport = omp_transport_provider()?;
    let base = base(repo)?;
    let (mut rows, faults) = transport.pending_inventory(repo, runtime_root)?;
    let mut pins = Vec::new();
    let binding_path = base.join(BINDING_FILE);
    if lexists(&binding_path)? {
        bank_binding(&base)?;
        pins.push(Value::Array(vec![
            string(BINDING_FILE),
            string(sha_of(
                &binding_path,
                MAX_RECORD,
                "Unknown native bank binding",
            )?),
        ]));
    }
    for (path, reference) in omp_index(repo, runtime_root)? {
        pins.push(Value::Array(vec![
            relative(&path, &base)?,
            string(sha_of(
                &path,
                MAX_RECORD,
                "Unverifiable shared source index inventory",
            )?),
        ]));
        let session = text(&reference, "session").unwrap_or_default();
        let lifecycle = text(&reference, "lifecycle").unwrap_or_default();
        let state = turn_sources::inspect(repo, "omp", session, lifecycle, runtime_root)?;
        if !truthy(get(&state, "complete")) {
            rows.push(object([
                ("session", string(session)),
                ("lifecycle", string(lifecycle)),
                (
                    "source_reference",
                    string(format!(
                        "omp:index:{}",
                        text(&reference, "turn").unwrap_or_default()
                    )),
                ),
                ("status", string("indexed-pending")),
            ]));
        }
    }
    let mut roots = BTreeSet::from([base.join("omp-source-v1")]);
    for (_, cursor) in transport.all_segments(repo, runtime_root)? {
        let session =
            text(&cursor, "session").ok_or_else(|| err("Unverifiable drain source namespace"))?;
        let lifecycle =
            text(&cursor, "lifecycle").ok_or_else(|| err("Unverifiable drain source namespace"))?;
        let (namespace, _) =
            turn_sources::private_namespace(repo, "omp", session, lifecycle, false, runtime_root)?;
        roots.insert(namespace.directory);
    }
    // Orphan shared namespaces still participate even without a transport receipt.
    for directory in hash_directories(&base.join("turn-sources-v2").join("turns"))? {
        if omp_namespace(&directory, "Unverifiable drain source namespace")? {
            roots.insert(directory);
        }
    }
    let maximum = transport.max_payload();
    let mut count = 0;
    for root in roots {
        if !lexists(&root)? {
            continue;
        }
        walk_evidence(&root, &base, maximum, &mut count, &mut pins)?;
    }
    let pending: Vec<Value> = rows
        .into_iter()
        .filter(|row| text(row, "status") != Some("completed"))
        .collect();
    let binding = object([
        ("repository", path_value(&fs::canonical_path(repo, false)?)?),
        ("private_state", path_value(&base)?),
        ("pins", Value::Array(pins)),
        ("pending", Value::Array(pending)),
        ("faults", Value::Array(faults)),
    ]);
    let digest = sha256_hex(&turn_sources::encoded(&binding)?);
    Ok((binding, digest))
}

/// Baseline `_shared_native_source_evidence(repo, base)`: read shared v2 state
/// without creating locks or a drain requirement.
pub fn shared_native_source_evidence(
    repo: &Path,
    base: &Path,
    runtime_root: &Path,
) -> Result<bool> {
    if !omp_index(repo, runtime_root)?.is_empty() {
        return Err(err(
            "Markerless state still contains indexed omp references",
        ));
    }
    let shared = base.join("turn-sources-v2").join("turns");
    if !lexists(&shared)? {
        return Ok(false);
    }
    fs::trust::state(&shared)?;
    let names = listing(&shared, MAX_NAMESPACES)?
        .ok_or_else(|| err("Source namespace inventory exceeds bound"))?;
    let mut has_evidence = false;
    for name in names {
        let directory = shared.join(name);
        fs::trust::state(&directory)?;
        for i in 0..turn_sources::MAX_SOURCES {
            let path = directory.join(format!("source-{i}.json"));
            let Some(source) = turn_sources::read(&path, MAX_RECORD)? else {
                continue;
            };
            if source.as_object().is_none() {
                return Err(err("Unverifiable source inventory"));
            }
            has_evidence |= text(&source, "harness") == Some("omp");
        }
    }
    Ok(has_evidence)
}

fn drain_proof(binding: &Value, base: &Path, digest: &str) -> Result<Value> {
    Ok(object([
        ("schema", string(DRAIN_SCHEMA)),
        ("repository", get(binding, "repository").clone()),
        ("private_state", path_value(base)?),
        ("inventory_sha256", string(digest)),
        ("writers_quiesced", Value::Bool(true)),
    ]))
}
fn has_streams(base: &Path) -> Result<bool> {
    for name in STREAM_STATE {
        if lexists(&base.join(name))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Baseline `required_source_capabilities(repo)`: a missing marker needs a real
/// bound drain proof, never an empty fallback.
pub fn required_source_capabilities(repo: &Path, runtime_root: &Path) -> Result<BTreeSet<String>> {
    let (root, bank) = bank_writes::bank_path(repo)?;
    let base = base(&root)?;
    let registered = registry()?;
    let saved_base = get(get(&registered, "private_bases"), path_text(&root)?)
        .as_str()
        .map(str::to_owned);
    if saved_base
        .as_deref()
        .is_some_and(|saved| saved != path_text(&base).unwrap_or_default())
    {
        return Err(err(
            "Native protected private-state location differs from its saved binding",
        ));
    }
    let has_binding = lexists(&base.join(BINDING_FILE))?;
    if has_binding {
        let binding = bank_binding(&base)?;
        if text(&binding, "repository") != Some(path_text(&root)?)
            || text(&binding, "bank_path") != Some(path_text(&bank)?)
        {
            return Err(err(
                "Native private source binding differs from the current bank",
            ));
        }
    } else if saved_base.is_some() {
        return Err(err(
            "Saved native bank lacks its protected private-state binding",
        ));
    }
    let current = marker(&base)?;
    let streams = has_streams(&base)?;
    let tokens = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect();
    if let Some(token) = current {
        if !has_binding {
            return Err(err(
                "Native compatibility marker lacks its protected bank binding",
            ));
        }
        if token == HISTORICAL_CAPABILITY {
            if streams {
                return Err(err(
                    "Native stream evidence requires the stored-source reader marker",
                ));
            }
            return Ok(tokens(&[HISTORICAL_CAPABILITY]));
        }
        return Ok(tokens(&[HISTORICAL_CAPABILITY, CAPABILITY]));
    }
    let mut has_evidence = has_native_source_state(&base)?;
    has_evidence |= shared_native_source_evidence(&root, &base, runtime_root)?;
    if !has_evidence {
        return Ok(BTreeSet::new());
    }
    // A valid drain was written under this persistent lock. Inspection may
    // never recreate a missing lock or interpret its loss as empty evidence.
    turn_sources::read_only_locks(|| {
        private_state::observation_barrier(repo, runtime_root, |_| {
            let lock = turn_sources::private_lock(&base.join(TRANSPORT_LOCK), false, true)?;
            let (binding, digest) = drain_inventory(repo, runtime_root)?;
            let proof = turn_sources::read(
                &base.join("omp-drains").join(format!("{digest}.json")),
                MAX_RECORD,
            )?
            .ok_or_else(|| err("Retained omp evidence lacks its bound lifecycle drain proof"))?;
            if !same(&proof, &drain_proof(&binding, &base, &digest)?)
                || truthy(get(&binding, "pending"))
                || truthy(get(&binding, "faults"))
            {
                return Err(err(
                    "Retained omp drain proof differs from current private evidence",
                ));
            }
            if streams {
                verify_stream_drain(repo, &base, runtime_root)?;
            }
            lock.finish()
        })
    })?;
    Ok(BTreeSet::new())
}

fn stream_digest(inventory: &Value) -> Result<&str> {
    text(inventory, "inventory_sha256")
        .filter(|digest| hex64(digest))
        .ok_or_else(|| err("Finite stream inventory is missing, offline or malformed"))
}
/// Baseline `_stream_proof(repo, base, inventory)`.
fn stream_proof(repo: &Path, base: &Path, inventory: &Value) -> Result<Value> {
    Ok(object([
        ("schema", string(STREAM_DRAIN_SCHEMA)),
        ("repository", path_value(&fs::canonical_path(repo, false)?)?),
        ("private_state", path_value(base)?),
        ("inventory_sha256", string(stream_digest(inventory)?)),
        ("writers_quiesced", Value::Bool(true)),
    ]))
}

/// Baseline `_verify_stream_drain(repo, base)`.
fn verify_stream_drain(repo: &Path, base: &Path, runtime_root: &Path) -> Result<Value> {
    let inventory = omp_stream_provider()?.drain_state(repo, base, true, runtime_root)?;
    let expected = stream_proof(repo, base, &inventory)?;
    let target = base
        .join("omp-stream-drains")
        .join(format!("{}.json", stream_digest(&inventory)?));
    let proof = turn_sources::read(&target, MAX_RECORD)?
        .ok_or_else(|| err("Retained native streams lack their bound lifecycle drain proof"))?;
    if truthy(get(&inventory, "pending")) || !same(&proof, &expected) {
        return Err(err(
            "Retained native stream drain proof differs from complete coverage",
        ));
    }
    Ok(inventory)
}

fn evidence_present(base: &Path) -> Result<bool> {
    for name in [
        "omp-source-v1",
        "omp-stream-v1",
        "omp-stream-session-locks",
        "omp-drains",
    ] {
        if lexists(&base.join(name))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Baseline `_read_only_transport_lock(base)`: observe an existing transport
/// lock, or fence its continued absence around `body`.
fn read_only_transport_lock<T>(base: &Path, body: impl FnOnce() -> Result<T>) -> Result<T> {
    let path = base.join(TRANSPORT_LOCK);
    if lexists(&path)? {
        let lock = turn_sources::private_lock(&path, false, true)?;
        let value = body()?;
        lock.finish()?;
        return Ok(value);
    }
    let protected = turn_sources::source_barrier_held(base);
    if !protected && evidence_present(base)? {
        return Err(err(
            "Native evidence lacks its source barrier; preserve private state",
        ));
    }
    // Ordinary refusals are rechecked too; an unwinding panic is not.
    let outcome = body();
    if lexists(&path)? || (!protected && evidence_present(base)?) {
        return Err(err(
            "Native transport appeared during read-only drain observation",
        ));
    }
    outcome
}

/// Baseline cleanup loop: every completion lock of the sorted namespaces is
/// held (nested, in order) while each complete namespace is cleaned.
fn cleanup_completed(namespaces: &[Namespace]) -> Result<()> {
    let Some((first, rest)) = namespaces.split_first() else {
        return Ok(());
    };
    turn_sources::locked(&first.directory, || {
        let state = turn_sources::inventory(&first.directory, &first.identity)?;
        if truthy(get(&state, "complete")) {
            turn_sources::cleanup(first)?;
        }
        cleanup_completed(rest)
    })
}

fn publish_proof(directory: &Path, digest: &str, proof: &Value, conflict: &str) -> Result<()> {
    turn_sources::private_mkdir(directory)?;
    let target = directory.join(format!("{digest}.json"));
    if !turn_sources::exclusive(&target, proof, MAX_RECORD)? {
        let retained = turn_sources::read(&target, MAX_RECORD)?;
        if !retained.is_some_and(|retained| same(&retained, proof)) {
            return Err(err(conflict));
        }
    }
    Ok(())
}

fn drain_body(repo: &Path, base: &Path, check: bool, runtime_root: &Path) -> Result<Value> {
    if !check {
        let mut namespaces = BTreeMap::new();
        for (_, reference) in omp_index(repo, runtime_root)? {
            let (namespace, _) = turn_sources::private_namespace(
                repo,
                "omp",
                text(&reference, "session").unwrap_or_default(),
                text(&reference, "lifecycle").unwrap_or_default(),
                false,
                runtime_root,
            )?;
            namespaces.insert(path_text(&namespace.directory)?.to_owned(), namespace);
        }
        let namespaces: Vec<Namespace> = namespaces.into_values().collect();
        cleanup_completed(&namespaces)?;
    }
    let (binding, digest) = drain_inventory(repo, runtime_root)?;
    let streams = omp_stream_provider()?.drain_state(repo, base, check, runtime_root)?;
    let drainable = !truthy(get(&binding, "pending"))
        && !truthy(get(&binding, "faults"))
        && !truthy(get(&streams, "pending"));
    let mut result = object([
        ("capability", string(CAPABILITY)),
        ("pending", get(&binding, "pending").clone()),
        ("faults", get(&binding, "faults").clone()),
        ("transport_pending", get(&streams, "pending").clone()),
        (
            "stream_inventory_sha256",
            get(&streams, "inventory_sha256").clone(),
        ),
        ("inventory_sha256", string(&digest)),
        ("drainable", Value::Bool(drainable)),
        ("marker_cleared", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]);
    if check || !drainable {
        return Ok(result);
    }
    publish_proof(
        &base.join("omp-drains"),
        &digest,
        &drain_proof(&binding, base, &digest)?,
        "Conflicting retained source drain proof",
    )?;
    if has_streams(base)? {
        publish_proof(
            &base.join("omp-stream-drains"),
            stream_digest(&streams)?,
            &stream_proof(repo, base, &streams)?,
            "Conflicting retained native stream drain proof",
        )?;
        verify_stream_drain(repo, base, runtime_root)?;
    }
    if marker(base)?.is_some() {
        let chain = DirectoryChain::open(base).map_err(io_error)?;
        chain
            .directory()
            .unlink_at(OsStr::new(MARKER_FILE))
            .map_err(io_error)?;
        turn_sources::sync(base)?;
        if lexists(&base.join(MARKER_FILE))? {
            return Err(err("Source marker removal could not be verified"));
        }
    }
    set_field(&mut result, "marker_cleared", Value::Bool(true));
    Ok(result)
}

/// Baseline `drain_source_capabilities(repo, quiesced=…, check=…)`: aware
/// lifecycle only, after binding disable and explicit writer quiescence.
pub fn drain_source_capabilities(
    repo: &Path,
    quiesced: bool,
    check: bool,
    runtime_root: &Path,
) -> Result<Value> {
    if !quiesced {
        return Err(err(
            "Source drain requires disabled bindings and writer quiescence",
        ));
    }
    let base = base(repo)?;
    if check {
        return turn_sources::read_only_locks(|| {
            private_state::observation_barrier(repo, runtime_root, |_| {
                read_only_transport_lock(&base, || drain_body(repo, &base, true, runtime_root))
            })
        });
    }
    let barrier = turn_sources::source_barrier(repo, true, false, None, runtime_root)?;
    let lock = turn_sources::private_lock(&base.join(TRANSPORT_LOCK), false, false)?;
    let result = drain_body(repo, &base, false, runtime_root)?;
    lock.finish()?;
    barrier.recheck()?;
    Ok(result)
}

/// Baseline `_enrolled_repositories(installation)`.
pub fn enrolled_repositories(installation: Option<&Value>) -> Result<Vec<PathBuf>> {
    let registered = registry()?;
    let mut protected: BTreeSet<PathBuf> = strings(
        get(&registered, "repositories"),
        "Unverifiable omp bank registry; preserve private state",
    )?
    .into_iter()
    .map(PathBuf::from)
    .collect();
    protected.extend(source_state_bindings(installation, &registered)?);
    if let Some(installation) = installation {
        protected.extend(installation_repositories(
            installation,
            &["rollback_repositories", "observed_repositories"],
        )?);
        for key in ["banks", "bank_bindings"] {
            protected.extend(binding_rows(installation, key)?);
        }
    }
    let mut result = Vec::new();
    for repo in source_capability_repositories(installation)? {
        if !is_dir(&repo)? {
            if protected.contains(&repo) || lexists(&repo)? {
                return Err(err(
                    "Enrolled native bank is unavailable; compatibility remains unknown",
                ));
            }
            continue;
        }
        if !is_dir(&repository_config::bank_path(&repo)?)? {
            if protected.contains(&repo) {
                return Err(err(
                    "Enrolled native bank has disappeared; compatibility remains unknown",
                ));
            }
            continue;
        }
        result.push(repo);
    }
    Ok(result)
}

fn held_roots() -> Result<BTreeSet<PathBuf>> {
    let bindings = layout_recovery_state::bindings()?;
    let rows = get(&bindings, "bindings")
        .as_object()
        .ok_or_else(|| err("Unknown recovery state binding registry"))?;
    let mut roots = BTreeSet::new();
    for (_, row) in rows {
        if matches!(text(row, "lifecycle_state"), Some("active" | "interrupted")) {
            let root =
                text(row, "repo_root").ok_or_else(|| err("Unknown recovery state binding"))?;
            roots.insert(PathBuf::from(root));
        }
    }
    Ok(roots)
}

fn quiescence_paths(plan: &Value) -> Result<Vec<PathBuf>> {
    let refusal = "Lifecycle quiescence inputs must be explicit protected-file paths";
    let mut paths = match get(plan, "omp_layout_quiescence") {
        Value::String(path) => vec![path.clone()],
        value if !truthy(value) => vec![],
        value => strings(value, refusal)?,
    };
    let layout = get(plan, "layout_quiescence");
    if truthy(layout) {
        paths.extend(strings(layout, refusal)?);
    }
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Baseline `native_activation(plan, check)`: refresh reviewed layout quiescence
/// for the newly enabled native route. With `check`, record the renewals in
/// `plan['native_layout_quiescence_overrides']`.
pub fn native_activation(plan: &mut Value, check: bool, runtime_root: &Path) -> Result<Value> {
    let binding = get(plan, "omp_binding").clone();
    if !truthy(&binding) {
        return Ok(Value::Array(vec![]));
    }
    let old = match get(plan, "old") {
        value if truthy(value) => value.clone(),
        _ => Value::Object(vec![]),
    };
    let fresh_route = !truthy(get(&old, "omp_binding"));
    let upgrading = text(get(&old, "omp_binding"), "peer_reader_capability") != Some(CAPABILITY);
    if text(&binding, "peer_reader_capability") != Some(CAPABILITY) {
        return Err(err(
            "New native activation requires the stored-source reader capability",
        ));
    }
    if upgrading && !truthy(get(plan, "omp_peers_upgraded")) {
        return Err(err(
            "Native source activation requires renewed --omp-peers-upgraded",
        ));
    }
    let digest = get(get(plan, "source"), "artifact_digest").clone();
    if !fresh_route && same(get(get(&old, "source"), "artifact_digest"), &digest) {
        return Ok(Value::Array(vec![]));
    }
    let route = text(&binding, "extension_target")
        .ok_or_else(|| err("New native activation requires the stored-source reader capability"))?
        .to_owned();
    let mut values = Vec::new();
    for path in quiescence_paths(plan)? {
        fs::trust::record(&path)?;
        values.push(
            turn_sources::read(&path, MAX_QUIESCENCE)?
                .ok_or_else(|| err("Reviewed layout quiescence attestation is unavailable"))?,
        );
    }
    let inventory = private_state::repository_inventory(Some(&old), Path::new("."), runtime_root)?;
    if !inventory.unknown.is_empty() {
        return Err(err(
            "Private-state bank inventory is unavailable; preserve the active runtime",
        ));
    }
    let mut repositories: BTreeSet<PathBuf> =
        enrolled_repositories(Some(&old))?.into_iter().collect();
    repositories.extend(held_roots()?);
    if !repositories.is_subset(&inventory.roots) {
        return Err(err(
            "Private-state bank inventory changed during native activation",
        ));
    }
    let mut results = Vec::new();
    let mut renewals = Vec::new();
    for repo in &repositories {
        let barrier = if check {
            None
        } else {
            Some(turn_sources::source_barrier(
                repo,
                true,
                false,
                None,
                runtime_root,
            )?)
        };
        let context = scope::private_enrollment_context(repo, runtime_root)?;
        if context.held() {
            let repo_text = path_text(repo)?;
            let matching: Vec<&Value> = values
                .iter()
                .filter(|item| {
                    item.as_object().is_some()
                        && get(item, "attestation").as_object().is_some()
                        && text(get(item, "attestation"), "repo_root") == Some(repo_text)
                })
                .collect();
            let [selected] = matching.as_slice() else {
                return Err(err(
                    "Native activation under layout hold requires independently reviewed --omp-layout-quiescence covering the new route",
                ));
            };
            results.push(
                bank_writes::recovery_frontier_provider()?.validate_quiescence(
                    repo,
                    selected,
                    digest.as_str(),
                    std::slice::from_ref(&route),
                    !check,
                    runtime_root,
                )?,
            );
            renewals.push((*selected).clone());
        }
        let repo_base = base(repo)?;
        if marker(&repo_base)?.is_some()
            && get(&bank_binding(&repo_base)?, "project_id") != context.project_id()
        {
            return Err(err(
                "Native source activation differs from the checked project binding",
            ));
        }
        if let Some(barrier) = barrier {
            barrier.recheck()?;
        }
    }
    if check {
        set_field(
            plan,
            "native_layout_quiescence_overrides",
            Value::Array(renewals),
        );
    }
    Ok(Value::Array(results))
}

fn activation_roots(receipt: &Value) -> Result<BTreeSet<PathBuf>> {
    let mut roots: BTreeSet<PathBuf> = enrolled_repositories(Some(receipt))?.into_iter().collect();
    roots.extend(held_roots()?);
    Ok(roots)
}
fn target_refs<'a>(receipt: &'a Value, path: Option<&str>) -> Vec<&'a Value> {
    get(receipt, "targets")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter(|target| path.is_none_or(|path| text(target, "path") == Some(path)))
        .collect()
}

/// Baseline `complete_stored_source_activation(state)`: the committed ownership
/// receipt authorizes an idempotent marker upgrade.
pub fn complete_stored_source_activation(
    state: &Path,
    runtime_root: &Path,
) -> installation::Result<Value> {
    let env = Environment::process();
    let services = Services::registered()?;
    let receipt = installation::read_receipt(&env, state)?;
    let Some(receipt) = receipt.filter(|receipt| {
        !truthy(get(receipt, "uninstalled"))
            && truthy(get(receipt, "omp_binding"))
            && text(get(receipt, "omp_binding"), "peer_reader_capability") == Some(CAPABILITY)
    }) else {
        return Ok(object([
            ("status", string("not-required")),
            ("verified", Value::Bool(true)),
        ]));
    };
    let journal = installation::read_journal(&env, state)?;
    let receipt_identity = installation::fingerprint(&state.join("receipt.json"))?;
    if let Some(journal) = &journal {
        match text(journal, "status") {
            Some("complete") => {
                let recorded = get(journal, "receipt_identity");
                if *recorded == Value::Null || !same(&receipt_identity, recorded) {
                    return Err(err(
                        "Native source activation requires the exact committed installation receipt",
                    )
                    .into());
                }
            }
            Some("rolled-back") => {
                if same(&receipt_identity, get(journal, "receipt_identity")) {
                    return Err(err(
                        "Rolled-back transaction cannot authorize native source activation",
                    )
                    .into());
                }
                if truthy(get(journal, "opaque_cache_recovery")) {
                    return Err(err(
                        "Rolled-back ownership remains unverified; native source activation is pending",
                    )
                    .into());
                }
                installation::verify_rolled_back_targets(&services, state, journal)?;
            }
            _ => {
                return Err(err(
                    "Recover the unfinished installation before native source activation",
                )
                .into());
            }
        }
    }
    omp_installation::validate_receipt(&receipt)?;
    installation::verify_targets(
        &services,
        &target_refs(&receipt, None),
        Some(get(&receipt, "source")),
        false,
    )?;
    let roots = activation_roots(&receipt)?;
    // The stored-source upgrade never replaces the guarded target.
    let mut upgrade = || -> Result<Option<Value>> {
        for repo in &roots {
            activate_marker(repo, false, runtime_root)?;
        }
        Ok(None)
    };
    private_state::activation_guard(
        Some(&receipt),
        Some(get(&receipt, "source")),
        "omp-stored-source-activation",
        Path::new("."),
        runtime_root,
        Some(&mut upgrade),
        |_| {
            let current = installation::read_receipt(&env, state).map_err(contract)?;
            if !current.is_some_and(|current| same(&current, &receipt)) {
                return Err(err("Native source activation receipt changed"));
            }
            for repo in &roots {
                verify_marker(repo, runtime_root)?;
            }
            if activation_roots(&receipt)? != roots {
                return Err(err("Native source activation bank inventory changed"));
            }
            Ok(())
        },
    )?;
    Ok(object([
        ("status", string("activated")),
        ("verified", Value::Bool(true)),
        ("banks", Value::Int(roots.len().into())),
    ]))
}

/// `\.agent-memory-disabled-<installation_id>-[0-9a-f-]{36}\.ts\.disabled`.
fn disabled_name(name: Option<&OsStr>, installation_id: &str) -> bool {
    let Some(name) = name.and_then(OsStr::to_str) else {
        return false;
    };
    let Some(rest) = name
        .strip_prefix(".agent-memory-disabled-")
        .and_then(|rest| rest.strip_prefix(installation_id))
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix(".ts.disabled"))
    else {
        return false;
    };
    rest.len() == 36
        && rest
            .bytes()
            .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Nested read-only observation barriers over every repository, then `body`.
fn observed_all<T>(
    repositories: &[PathBuf],
    runtime_root: &Path,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let Some((first, rest)) = repositories.split_first() else {
        return body();
    };
    private_state::observation_barrier(first, runtime_root, |_| {
        observed_all(rest, runtime_root, body)
    })
}
fn exclusive_all(
    repositories: &[PathBuf],
    runtime_root: &Path,
) -> Result<Vec<turn_sources::SourceBarrier>> {
    repositories
        .iter()
        .map(|repo| turn_sources::source_barrier(repo, true, false, None, runtime_root))
        .collect()
}
fn finish_all(barriers: Vec<turn_sources::SourceBarrier>) -> Result<()> {
    for barrier in barriers.iter().rev() {
        barrier.recheck()?;
    }
    Ok(())
}
fn drains(repositories: &[PathBuf], check: bool, runtime_root: &Path) -> Result<Vec<Value>> {
    repositories
        .iter()
        .map(|repo| drain_source_capabilities(repo, true, check, runtime_root))
        .collect()
}
fn path_from(value: &Value, key: &str) -> PathBuf {
    PathBuf::from(text(value, key).unwrap_or_default())
}

/// Baseline `native_transition(plan, check)`: disable only the saved executable
/// binding, journal it, then prove drain. Mutates `plan` exactly as the
/// baseline (`actions`, `omp_disabled_backup`).
pub fn native_transition(
    plan: &mut Value,
    check: bool,
    runtime_root: &Path,
) -> installation::Result<Value> {
    let old = get(plan, "old").clone();
    let selected = get(plan, "omp_transition");
    if !truthy(&old)
        || selected.as_object().is_none()
        || !same(selected, &object([("writers_quiesced", Value::Bool(true))]))
    {
        return Err(err(
            "Native deactivation requires saved ownership and explicit writer quiescence",
        )
        .into());
    }
    omp_installation::validate_receipt(&old)?;
    let repositories = enrolled_repositories(Some(&old))?;
    if !truthy(get(&old, "omp_binding")) {
        // A previously removed binding can leave safely retained snapshots and a
        // separately owned capable reader, without any executable path to guess.
        let results = if check {
            observed_all(&repositories, runtime_root, || {
                drains(&repositories, true, runtime_root)
            })?
        } else {
            let barriers = exclusive_all(&repositories, runtime_root)?;
            let results = drains(&repositories, false, runtime_root)?;
            finish_all(barriers)?;
            results
        };
        if !check
            && results
                .iter()
                .any(|result| !truthy(get(result, "drainable")))
        {
            return Err(err(
                "Historical native sources remain pending; preserve their capable reader",
            )
            .into());
        }
        return Ok(object([
            ("binding_absent_in_saved_ownership", Value::Bool(true)),
            ("drains", Value::Array(results)),
            ("canonical_writes_performed", Value::Bool(false)),
        ]));
    }
    let installation_id = text(&old, "installation_id").unwrap_or_default().to_owned();
    let target = path_from(get(&old, "omp_binding"), "extension_target");
    let target_text = path_text(&target)?.to_owned();
    let owned = target_refs(&old, Some(&target_text))
        .first()
        .map(|owned| (*owned).clone())
        .ok_or_else(|| {
            err("Native binding is modified/unowned; preserve it and refuse compatibility drain")
        })?;
    let owned_identity = get(&owned, "identity").clone();
    let state = path_from(plan, "state");
    let journal_path = state.join(TRANSITION_FILE);
    let identity = installation::fingerprint(&target)?;
    let target_parent = parent(&target)?.to_path_buf();
    let mut backup = target_parent.join(format!(
        ".agent-memory-disabled-{installation_id}-{}.ts.disabled",
        crate::host::uuid4()?
    ));
    let mut journal = object([
        ("schema", string(TRANSITION_SCHEMA)),
        ("installation_id", string(&installation_id)),
        ("source", get(&old, "source").clone()),
        ("extension", string(&target_text)),
        ("identity", owned_identity.clone()),
        ("disabled_path", path_value(&backup)?),
        ("status", string("prepared")),
    ]);
    if lexists(&journal_path)? {
        let retained = installation::load(&journal_path)?;
        let status = text(&retained, "status").unwrap_or_default().to_owned();
        if matches!(status.as_str(), "prepared" | "disabled") {
            backup = path_from(&retained, "disabled_path");
            if backup.parent() != Some(target_parent.as_path())
                || !disabled_name(backup.file_name(), &installation_id)
            {
                return Err(err("Foreign native disabled-binding journal path").into());
            }
            let mut expected = journal.clone();
            set_field(&mut expected, "disabled_path", path_value(&backup)?);
            set_field(&mut expected, "status", string(&status));
            if !same(&retained, &expected) {
                return Err(
                    err("Native disabled-binding journal differs from saved ownership").into(),
                );
            }
            journal = retained;
        }
    }
    if !same(&identity, &owned_identity)
        && !(identity == Value::Null && same(&installation::fingerprint(&backup)?, &owned_identity))
    {
        return Err(err(
            "Native binding is modified/unowned; preserve it and refuse compatibility drain",
        )
        .into());
    }
    for repo in &repositories {
        if !is_dir(repo)? {
            return Err(
                err("Enrolled native bank is unavailable; compatibility remains unknown").into(),
            );
        }
    }
    if check {
        let results = observed_all(&repositories, runtime_root, || {
            drains(&repositories, true, runtime_root)
        })?;
        return Ok(object([
            ("binding_disable_planned", Value::Bool(true)),
            ("drains", Value::Array(results)),
            ("installation_writes_performed", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
        ]));
    }
    let barriers = exclusive_all(&repositories, runtime_root)?;
    installation::replace_record(&journal_path, &journal)?;
    if identity != Value::Null {
        if lexists(&backup)? {
            return Err(
                err("Native disabled-binding backup collision; preserve both paths").into(),
            );
        }
        installation::native_rename(&target, &backup, false)?;
        installation::sync_dir(&target_parent)?;
    }
    if !same(&installation::fingerprint(&backup)?, &owned_identity) || lexists(&target)? {
        return Err(err("Native executable disable readback failed").into());
    }
    set_field(&mut journal, "status", string("disabled"));
    installation::replace_record(&journal_path, &journal)?;
    let results = drains(&repositories, false, runtime_root)?;
    if results
        .iter()
        .any(|result| !truthy(get(result, "drainable")))
    {
        return Err(err(
            "Native sources remain pending; owned extension stays disabled, evidence and transition journal retained",
        )
        .into());
    }
    let mut actions = Vec::new();
    for action in get(plan, "actions").as_array().unwrap_or(&[]) {
        let mut action = action.clone();
        if text(&action, "path") == Some(target_text.as_str()) {
            if *get(&action, "after") == Value::Null {
                continue;
            }
            set_field(&mut action, "before", Value::Null);
            set_field(&mut action, "changed", Value::Bool(true));
        }
        actions.push(action);
    }
    set_field(plan, "actions", Value::Array(actions));
    set_field(
        plan,
        "omp_disabled_backup",
        object([
            ("path", path_value(&backup)?),
            ("original_path", string(&target_text)),
            ("identity", owned_identity),
            ("available", Value::Bool(true)),
            ("kind", string("native-binding-disabled")),
        ]),
    );
    finish_all(barriers)?;
    Ok(object([
        ("binding_disabled", Value::Bool(true)),
        ("drains", Value::Array(results)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}

/// Baseline `finish_native_transition(plan)`.
pub fn finish_native_transition(plan: &Value) -> installation::Result<()> {
    let path = path_from(plan, "state").join(TRANSITION_FILE);
    if !truthy(get(plan, "omp_disabled_backup")) || !lexists(&path)? {
        return Ok(());
    }
    let mut value = installation::load(&path)?;
    let status = text(&value, "status");
    if text(&value, "schema") != Some(TRANSITION_SCHEMA)
        || !matches!(status, Some("disabled" | "completed"))
    {
        return Err(err("Native transition state changed before lifecycle completion").into());
    }
    if status == Some("completed") {
        return Ok(());
    }
    set_field(&mut value, "status", string("completed"));
    installation::replace_record(&path, &value)
}

fn transition_journal(path: &Path) -> installation::Result<Value> {
    let journal = installation::load(path)?;
    let fields = [
        "schema",
        "installation_id",
        "source",
        "extension",
        "identity",
        "disabled_path",
        "status",
    ];
    if journal
        .as_object()
        .is_none_or(|pairs| pairs.len() != fields.len())
        || fields.iter().any(|name| journal.get(name).is_none())
        || text(&journal, "schema") != Some(TRANSITION_SCHEMA)
        || !matches!(
            text(&journal, "status"),
            Some("prepared" | "disabled" | "completed")
        )
    {
        return Err(err("Unverifiable native lifecycle recovery journal").into());
    }
    Ok(journal)
}

fn recover_uninstalled(state: &Path, path: &Path, mut journal: Value) -> installation::Result<()> {
    let historical = installation::load(&state.join("receipt.json"))?;
    let binding = get(&historical, "omp_binding");
    let refusal = || err("Native uninstall recovery has no exact committed receipt");
    if get(&historical, "uninstalled") != &Value::Bool(true)
        || get(&historical, "verified") != &Value::Bool(true)
        || get(&historical, "installation_id") != get(&journal, "installation_id")
        || !same(get(&historical, "source"), get(&journal, "source"))
        || binding.as_object().is_none()
    {
        return Err(refusal().into());
    }
    let agent_dir = text(binding, "agent_dir")
        .ok_or_else(|| err("Native uninstall binding no longer matches its saved root"))?;
    let expected = omp_installation::binding(
        agent_dir,
        truthy(get(&historical, "no_guidance")),
        omp_installation::READER_CAPABILITY,
    )?;
    if !same(binding, &expected) {
        return Err(err("Native uninstall binding no longer matches its saved root").into());
    }
    let target = path_from(binding, "extension_target");
    let backup = path_from(&journal, "disabled_path");
    let installation_id = text(&journal, "installation_id").unwrap_or_default();
    if text(&journal, "extension") != Some(path_text(&target)?)
        || backup.parent() != target.parent()
        || !disabled_name(backup.file_name(), installation_id)
        || installation::fingerprint(&target)? != Value::Null
        || !same(
            &installation::fingerprint(&backup)?,
            get(&journal, "identity"),
        )
    {
        return Err(err(
            "Committed native uninstall differs from its retained disabled executable",
        )
        .into());
    }
    set_field(&mut journal, "status", string("completed"));
    installation::replace_record(path, &journal)
}

/// Baseline `recover_native_binding(state)`: restore a disabled extension only
/// with its actual aware runtime selected.
pub fn recover_native_binding(state: &Path, runtime_root: &Path) -> installation::Result<()> {
    let path = state.join(TRANSITION_FILE);
    if !lexists(&path)? {
        return Ok(());
    }
    let mut journal = transition_journal(&path)?;
    if text(&journal, "status") == Some("completed") {
        return Ok(());
    }
    let env = Environment::process();
    let Some(old) = installation::read_receipt(&env, state)? else {
        return recover_uninstalled(state, &path, journal);
    };
    if !truthy(get(&old, "omp_binding"))
        || get(&old, "installation_id") != get(&journal, "installation_id")
    {
        return Err(err("Native recovery requires its exact saved installation").into());
    }
    let target = path_from(get(&old, "omp_binding"), "extension_target");
    let target_text = path_text(&target)?.to_owned();
    let backup = path_from(&journal, "disabled_path");
    if text(&journal, "extension") != Some(target_text.as_str())
        || backup.parent() != target.parent()
        || !disabled_name(
            backup.file_name(),
            text(&old, "installation_id").unwrap_or_default(),
        )
    {
        return Err(err("Foreign native recovery target").into());
    }
    let services = Services::registered()?;
    if !same(get(&old, "source"), get(&journal, "source")) {
        // A committed update owns the new executable; never overwrite it with old code.
        installation::verify_targets(
            &services,
            &target_refs(&old, Some(&target_text)),
            Some(get(&old, "source")),
            false,
        )?;
    } else {
        let capabilities = get(
            get(get(&old, "source"), "compatibility"),
            "private_state_capabilities",
        );
        let aware = capabilities.as_array().is_some_and(|tokens| {
            tokens
                .iter()
                .any(|token| token.as_str() == Some(CAPABILITY))
        });
        if !aware {
            return Err(err("Native recovery selected an omp-unaware runtime").into());
        }
        let identity = get(&journal, "identity").clone();
        private_state::activation_guard(
            Some(&old),
            Some(get(&old, "source")),
            "recover",
            Path::new("."),
            runtime_root,
            None,
            |_| {
                if installation::fingerprint(&target).map_err(contract)? == Value::Null {
                    if !same(
                        &installation::fingerprint(&backup).map_err(contract)?,
                        &identity,
                    ) {
                        return Err(err(
                            "Disabled native executable no longer matches ownership",
                        ));
                    }
                    installation::native_rename(&backup, &target, false).map_err(contract)?;
                    installation::sync_dir(parent(&target)?).map_err(contract)?;
                }
                if !same(
                    &installation::fingerprint(&target).map_err(contract)?,
                    &identity,
                ) {
                    return Err(err("Native recovery executable readback differs"));
                }
                Ok(())
            },
        )?;
    }
    set_field(&mut journal, "status", string("completed"));
    installation::replace_record(&path, &journal)
}

/// Baseline `native_recovery_expected(state, target)`: one exact restored owned
/// wrapper may differ from a `None` staging before.
pub fn native_recovery_expected(state: &Path, target: &str) -> installation::Result<Option<Value>> {
    let journal_path = state.join(TRANSITION_FILE);
    if !lexists(&journal_path)? {
        return Ok(None);
    }
    let journal = installation::load(&journal_path)?;
    if journal.as_object().is_none()
        || text(&journal, "schema") != Some(TRANSITION_SCHEMA)
        || text(&journal, "status") != Some("completed")
        || text(&journal, "extension") != Some(target)
    {
        return Ok(None);
    }
    let env = Environment::process();
    let Some(receipt) = installation::read_receipt(&env, state)? else {
        return Ok(None);
    };
    let binding = get(&receipt, "omp_binding");
    if !truthy(binding)
        || get(&receipt, "installation_id") != get(&journal, "installation_id")
        || !same(get(&receipt, "source"), get(&journal, "source"))
        || text(binding, "extension_target") != Some(target)
    {
        return Ok(None);
    }
    let owned = target_refs(&receipt, Some(target));
    let [single] = owned.as_slice() else {
        return Err(
            err("Native restored wrapper ownership differs from its recovery journal").into(),
        );
    };
    if text(single, "kind") != Some("wrapper")
        || !same(get(single, "identity"), get(&journal, "identity"))
    {
        return Err(
            err("Native restored wrapper ownership differs from its recovery journal").into(),
        );
    }
    let services = Services::registered()?;
    installation::verify_targets(&services, &owned, Some(get(&receipt, "source")), false)?;
    Ok(Some(get(single, "identity").clone()))
}

/// The production `installation::OmpBoundary` seam: the executing runtime pack
/// is the registered `cli::pack_root`.
pub struct InstallationBoundary;
impl installation::OmpBoundary for InstallationBoundary {
    fn capability(&self) -> &str {
        CAPABILITY
    }
    fn native_transition(&self, plan: &mut Value, check: bool) -> installation::Result<Value> {
        native_transition(plan, check, &runtime_root()?)
    }
    fn native_activation(&self, plan: &mut Value, check: bool) -> installation::Result<Value> {
        Ok(native_activation(plan, check, &runtime_root()?)?)
    }
    fn finish_native_transition(&self, plan: &Value) -> installation::Result<()> {
        finish_native_transition(plan)
    }
    fn recover_native_binding(&self, state: &Path) -> installation::Result<()> {
        recover_native_binding(state, &runtime_root()?)
    }
    fn native_recovery_expected(
        &self,
        state: &Path,
        path: &str,
    ) -> installation::Result<Option<Value>> {
        native_recovery_expected(state, path)
    }
    fn complete_stored_source_activation(&self, state: &Path) -> installation::Result<Value> {
        complete_stored_source_activation(state, &runtime_root()?)
    }
}

/// Baseline `omp_stream.main(argv, startup=stream_startup)`: the fresh-bank
/// enrollment callback is `stream_startup` unchanged.
struct BoundaryStartup;
impl crate::omp_stream::StreamStartup for BoundaryStartup {
    fn stream_startup(
        &self,
        repo: &Path,
        session: &str,
        instance_id: &str,
        lifecycle: &str,
        receipt: &Value,
        runtime_root: &Path,
    ) -> Result<Option<Value>> {
        stream_startup(repo, session, instance_id, lifecycle, receipt, runtime_root)
    }
}

/// The production `OmpStream`: the installed stream authority of the
/// executing runtime and the real finite stream entry.
pub struct InstalledStream;
impl OmpStream for InstalledStream {
    fn drain_state(
        &self,
        repo: &Path,
        base: &Path,
        read_only: bool,
        runtime_root: &Path,
    ) -> Result<Value> {
        crate::omp_stream::drain_state(
            &crate::omp_stream::Installed::new(runtime_root),
            repo,
            Some(base),
            read_only,
        )
    }
    fn main(&self, argv: &[String], prog: &str) -> CommandResult {
        crate::omp_stream::main(argv, prog, &BoundaryStartup)
    }
}

/// One `am omp-boundary` invocation (baseline argparse namespace).
pub struct Request<'a> {
    pub repo: &'a Path,
    pub stdin: bool,
    pub list_pending: bool,
    pub offset: i64,
    pub source: Option<&'a str>,
    pub session: Option<&'a str>,
    pub lifecycle: Option<&'a str>,
    pub quiet: bool,
    pub refresh: bool,
}

fn assessment(request: &Request<'_>, input: &mut dyn Read, runtime_root: &Path) -> Result<Value> {
    let mut raw = Vec::new();
    input
        .take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(io_error)?;
    if raw.len() > MAX_INPUT {
        return Err(err("Native boundary input exceeds bound"));
    }
    let value = crate::developer_identity::load(&raw)?;
    let transport = omp_transport_provider()?;
    if !request.refresh {
        return transport.complete(request.repo, &value, runtime_root);
    }
    let fields = [
        "session",
        "lifecycle",
        "observation_id",
        "source_sha256",
        "outcome",
        "assessment",
    ];
    let exact = value.as_object().is_some_and(|pairs| {
        fields.iter().all(|name| value.get(name).is_some())
            && (pairs.len() == fields.len()
                || (pairs.len() == fields.len() + 1 && value.get("publication_request").is_some()))
    });
    if !exact {
        return Err(err(
            "Native compact assessment accepts exactly one original observation, never an array",
        ));
    }
    crate::turn_check::check(
        request.repo,
        &value,
        |repo, value| Ok(transport.complete(repo, value, runtime_root)?),
        "omp",
    )
}

fn operation(request: &Request<'_>, input: &mut dyn Read) -> Result<Value> {
    let source = request.source.filter(|source| !source.is_empty());
    let modes = usize::from(request.list_pending)
        + usize::from(source.is_some())
        + usize::from(request.stdin);
    if modes != 1 {
        return Err(err(
            "Select exactly one native source, pending or assessment operation",
        ));
    }
    let runtime_root = runtime_root()?;
    if request.list_pending {
        if request.refresh || request.stdin {
            return Err(err("Pending inventory cannot acknowledge or refresh"));
        }
        return omp_transport_provider()?.list_pending(request.repo, request.offset, &runtime_root);
    }
    if let Some(source) = source {
        return omp_transport_provider()?.source(
            request.repo,
            request.session,
            request.lifecycle,
            source,
            &runtime_root,
        );
    }
    assessment(request, input, &runtime_root)
}

/// Baseline `main` (non-stream): every refusal becomes the fixed pending
/// failure, never inferred completion.
pub fn command(request: &Request<'_>, input: &mut dyn Read) -> CommandResult {
    let outcome = operation(request, input)
        .and_then(|result| crate::turn_check::emit(&result, request.quiet));
    match outcome {
        Ok((exit_code, stdout, stderr)) => CommandResult {
            value: None,
            exit_code,
            stdout,
            stderr,
        },
        Err(_) => failure(request.quiet),
    }
}

fn failure(quiet: bool) -> CommandResult {
    if quiet {
        return CommandResult {
            value: None,
            exit_code: 1,
            stdout: String::new(),
            stderr: format!("am: {FAILURE_REASON}\n"),
        };
    }
    let value = object([
        ("verified", Value::Bool(false)),
        ("status", string("pending")),
        ("canonical_writes_performed", Value::Bool(false)),
        ("reason", string(FAILURE_REASON)),
    ]);
    let stdout = json::encode(&value, json::DEFAULT)
        .map(|text| format!("{text}\n"))
        .unwrap_or_default();
    CommandResult {
        value: None,
        exit_code: 1,
        stdout,
        stderr: String::new(),
    }
}

/// `omp-boundary --stream`: delegated to the registered `omp_stream.main`.
pub fn stream(argv: &[String], prog: &str) -> CommandResult {
    match omp_stream_provider() {
        Ok(provider) => provider.main(argv, prog),
        Err(error) => crate::cli::refusal(prog, error.message()),
    }
}
