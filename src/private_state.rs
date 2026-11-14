//! Fail-closed union of bank-local private runtime requirements.
use crate::bank_writes::{self, CanonicalWriterBarrier};
use crate::codec::json::Value;
use crate::developer_identity::{digest, encoded};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain, Identity};
use crate::lifecycle::{get, object, string};
use crate::{
    artifact, host, identity_git, layout_recovery_state as state, project_identity,
    repository_config,
};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const TOKENS: &[&str] = &[
    "layout-frontier-v1",
    "omp-harness-v1",
    "omp-stored-source-v2",
];
pub const MAX_REPOSITORIES: usize = 1024;
pub const MAX_RENEWAL_BYTES: usize = 65536;
const LOCATOR_OPERATION: &str = "00000000-0000-4000-8000-000000000000";

/// Composite production wiring: installation (receipt/targets), omp_boundary
/// (protected native inventory/source evidence), admission (protected input).
/// Every method is required: an unavailable native producer is never capable.
pub trait PrivateStateHooks: Send + Sync {
    fn native_registry(&self, runtime_root: &Path) -> Result<Value>;
    fn native_source_state_bindings(
        &self,
        installation: Option<&Value>,
        registered: &Value,
        runtime_root: &Path,
    ) -> Result<BTreeSet<PathBuf>>;
    fn has_native_source_state(&self, base: &Path, runtime_root: &Path) -> Result<bool>;
    fn shared_native_source_evidence(
        &self,
        repo: &Path,
        base: &Path,
        runtime_root: &Path,
    ) -> Result<bool>;
    fn required_source_capabilities(
        &self,
        repo: &Path,
        runtime_root: &Path,
    ) -> Result<BTreeSet<String>>;
    fn read_receipt(&self, state: &Path, runtime_root: &Path) -> Result<Option<Value>>;
    fn verify_targets(&self, targets: &Value, source: &Value, runtime_root: &Path) -> Result<()>;
    fn read_input(&self, path: &Path, maximum: usize, runtime_root: &Path) -> Result<Value>;
}
static HOOKS: RwLock<Option<Arc<dyn PrivateStateHooks>>> = RwLock::new(None);
pub fn register_private_state_hooks(provider: Option<Arc<dyn PrivateStateHooks>>) {
    *HOOKS.write().unwrap_or_else(|error| error.into_inner()) = provider;
}
pub(crate) fn private_state_hooks_provider() -> Result<Arc<dyn PrivateStateHooks>> {
    HOOKS
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or_else(|| refusal("Private-state hooks provider is not registered"))
}

/// Implemented by recovery_quiescence, preserving its exact selected-attestation
/// contract rather than treating an unregistered reader as an absent proof.
pub trait RecoveryQuiescence: Send + Sync {
    fn selected_attestation(
        &self,
        directory: &Path,
        proposal: &Value,
        runtime_digest: &str,
        runtime_root: &Path,
    ) -> Result<Option<Value>>;
    /// Baseline recovery_quiescence.validate; no caller boolean can bypass it.
    fn validate(
        &self,
        repo: &Path,
        value: &Value,
        runtime_artifact_digest: Option<&str>,
        required_routes: &[String],
        publish: bool,
        owner: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Value>;
}
static QUIESCENCE: RwLock<Option<Arc<dyn RecoveryQuiescence>>> = RwLock::new(None);
pub fn register_recovery_quiescence(provider: Option<Arc<dyn RecoveryQuiescence>>) {
    *QUIESCENCE
        .write()
        .unwrap_or_else(|error| error.into_inner()) = provider;
}
pub(crate) fn recovery_quiescence_provider() -> Result<Arc<dyn RecoveryQuiescence>> {
    QUIESCENCE
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or_else(|| refusal("recovery_quiescence provider is not registered"))
}

thread_local! {
    static BARRIERS: RefCell<Option<BTreeSet<PathBuf>>> = const { RefCell::new(None) };
}
struct RestoreBarriers(Option<BTreeSet<PathBuf>>);
impl Drop for RestoreBarriers {
    fn drop(&mut self) {
        BARRIERS.with(|held| *held.borrow_mut() = self.0.take());
    }
}
fn refusal(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "private-state path")
}
fn sha(value: &Value) -> Result<String> {
    digest(Some(&encoded(value)?)).ok_or_else(|| refusal("Private-state digest is unavailable"))
}
fn strings(values: &BTreeSet<String>) -> Value {
    Value::Array(values.iter().map(string).collect())
}
fn paths_value(values: &BTreeSet<PathBuf>) -> Result<Value> {
    Ok(Value::Array(
        values
            .iter()
            .map(|path| text(path).map(string))
            .collect::<Result<_>>()?,
    ))
}
fn set(value: &Value, key: &str, default_empty: bool) -> Result<BTreeSet<String>> {
    match value.get(key) {
        None if default_empty => Ok(BTreeSet::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| refusal("Private-state string inventory is malformed"))
            })
            .collect(),
        _ => Err(refusal("Private-state string inventory is malformed")),
    }
}
fn put(value: &mut Value, key: &str, replacement: Value) -> Result<()> {
    let Value::Object(fields) = value else {
        return Err(refusal(
            "Private-state lifecycle declaration must be an object",
        ));
    };
    if let Some((_, current)) = fields.iter_mut().find(|(name, _)| name == key) {
        *current = replacement;
    } else {
        fields.push((key.to_owned(), replacement));
    }
    Ok(())
}
fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .iter()
                        .any(|(other, candidate)| key == other && equal(value, candidate))
                })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equal(left, right))
        }
        _ => left == right,
    }
}
fn named(path: &Path) -> Result<Option<Identity>> {
    if path == Path::new("/") {
        return DirectoryChain::open(path)
            .and_then(|chain| chain.directory().stat())
            .map(Some)
            .map_err(io_error);
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| refusal("Private-state path has no filename"))?;
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    let result = match chain.directory().stat_at(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    };
    chain.recheck().map_err(io_error)?;
    result
}
pub fn missing_path(path: &Path) -> Result<bool> {
    Ok(named(path)?.is_none())
}
fn ordinary_directory(path: &Path) -> Result<bool> {
    if !path.is_absolute() || !named(path)?.is_some_and(|info| info.is_directory()) {
        return Ok(false);
    }
    DirectoryChain::open(path)
        .and_then(|chain| chain.recheck())
        .map_err(io_error)?;
    Ok(true)
}

pub fn compatibility(target: Option<&Value>) -> Result<BTreeSet<String>> {
    let Some(target) = target else {
        return Ok(BTreeSet::new());
    };
    let value = target.get("compatibility").unwrap_or(target);
    if value.as_object().is_none() {
        return Err(refusal(
            "Target runtime has an unknown private-state compatibility declaration",
        ));
    }
    let empty = Value::Array(vec![]);
    let declared = value.get("private_state_capabilities").unwrap_or(&empty);
    if !artifact::strings(declared, true) {
        return Err(refusal(
            "Target runtime has an unknown private-state compatibility declaration",
        ));
    }
    let tokens = set(value, "private_state_capabilities", true)?;
    if tokens.iter().any(|token| !TOKENS.contains(&token.as_str())) {
        return Err(refusal(
            "Target runtime has an unknown private-state compatibility declaration",
        ));
    }
    Ok(tokens)
}

#[derive(Debug)]
pub struct RepositoryInventory {
    pub roots: BTreeSet<PathBuf>,
    pub unknown: Vec<Value>,
    pub pins: Value,
}
fn git(repo: &Path, args: &[&str]) -> Result<Option<String>> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    identity_git::git(repo, &args, true, false)
        .map_err(io_error)?
        .map(|value| host::text(&value, "private-state Git output").map(str::to_owned))
        .transpose()
}
/// The caller supplies its current repository and executing runtime explicitly;
/// neither discovery nor layout containment guesses a pack or process directory.
pub fn repository_inventory(
    installation: Option<&Value>,
    current_repo: &Path,
    runtime_root: &Path,
) -> Result<RepositoryInventory> {
    let cwd = fs::canonical_path(current_repo, false)?;
    let current = match git(&cwd, &["rev-parse", "--show-toplevel"])? {
        Some(top) if !top.is_empty() => {
            let top = Path::new(&top);
            if !top.is_absolute() {
                return Err(refusal(
                    "Private-state repository discovery returned a non-absolute root",
                ));
            }
            fs::canonical_path(top, false)?
        }
        _ => cwd,
    };
    let mut required = BTreeSet::from([current]);
    let mut roots = required.clone();
    let mut unknown = vec![];
    let mut private_roots = BTreeSet::new();
    let mut historical_gaps = BTreeSet::new();
    let (registry, registry_sha) = project_identity::observe()?;
    let checkouts = get(&registry, "checkouts")
        .as_object()
        .ok_or_else(|| refusal("Project registry checkout inventory is malformed"))?;
    for (_, entry) in checkouts {
        for location in get(entry, "locations")
            .as_array()
            .ok_or_else(|| refusal("Project registry location inventory is malformed"))?
        {
            let location = location
                .as_str()
                .ok_or_else(|| refusal("Project registry location is malformed"))?;
            let path = Path::new(location);
            if missing_path(path)? {
                historical_gaps.insert(path.to_path_buf());
                continue;
            }
            if !ordinary_directory(path)? {
                roots.insert(path.to_path_buf());
                continue;
            }
            match git(path, &["worktree", "list", "--porcelain"])? {
                None => {
                    if named(&path.join(".git"))?.is_some()
                        || path.file_name().is_some_and(|name| name == ".git")
                    {
                        unknown.push(object([
                            ("source", string("project-registry")),
                            ("location", string(location)),
                        ]));
                    } else {
                        roots.insert(path.to_path_buf());
                    }
                }
                Some(listed) => {
                    for line in listed.lines() {
                        if let Some(worktree) = line.strip_prefix("worktree ") {
                            let worktree = PathBuf::from(worktree);
                            if missing_path(&worktree)? {
                                historical_gaps.insert(worktree);
                            } else {
                                roots.insert(worktree);
                            }
                        }
                    }
                }
            }
        }
    }
    let bindings = state::bindings()?;
    for (_, row) in get(&bindings, "bindings")
        .as_object()
        .ok_or_else(|| refusal("Recovery state binding inventory is malformed"))?
    {
        if matches!(
            get(row, "lifecycle_state").as_str(),
            Some("active" | "interrupted")
        ) {
            private_roots.insert(PathBuf::from(
                get(row, "repo_root")
                    .as_str()
                    .ok_or_else(|| refusal("Recovery state repository binding is malformed"))?,
            ));
        }
    }
    if let Some(installation) = installation {
        for key in ["rollback_repositories", "observed_repositories"] {
            let values = set(installation, key, true)
                .map_err(|_| refusal("Saved runtime bank inventory is malformed"))?;
            required.extend(values.into_iter().map(PathBuf::from));
        }
        for key in ["banks", "bank_bindings"] {
            if let Some(value) = installation.get(key) {
                for row in value
                    .as_array()
                    .ok_or_else(|| refusal("Saved runtime bank binding is malformed"))?
                {
                    private_roots.insert(PathBuf::from(
                        get(row, "repository")
                            .as_str()
                            .ok_or_else(|| refusal("Saved runtime bank binding is malformed"))?,
                    ));
                }
            }
        }
    }
    let hooks = private_state_hooks_provider()?;
    let registered = hooks.native_registry(runtime_root)?;
    let native_sha = sha(&registered)?;
    private_roots.extend(
        set(&registered, "repositories", false)?
            .into_iter()
            .map(PathBuf::from),
    );
    private_roots.extend(hooks.native_source_state_bindings(
        installation,
        &registered,
        runtime_root,
    )?);
    required.extend(private_roots.iter().cloned());
    roots.extend(required.iter().cloned());
    if roots.len() > MAX_REPOSITORIES {
        return Err(refusal(
            "Private-state bank inventory exceeds its complete finite bound",
        ));
    }
    for root in &roots {
        if !required.contains(root) && missing_path(root)? {
            historical_gaps.insert(root.clone());
        }
    }
    historical_gaps.retain(|root| !required.contains(root));
    roots.retain(|root| !historical_gaps.contains(root));
    Ok(RepositoryInventory {
        roots,
        unknown,
        pins: object([
            (
                "project_registry_sha256",
                registry_sha.map(string).unwrap_or(Value::Null),
            ),
            ("layout_bindings_sha256", string(sha(&bindings)?)),
            ("native_registry_sha256", string(native_sha)),
            ("required_bank_repositories", paths_value(&private_roots)?),
            ("historical_discovery_gaps", paths_value(&historical_gaps)?),
        ]),
    })
}

/// Advisory absence only: never a held-source token or activation authority.
pub fn native_absence(root: &Path, base: &Path, runtime_root: &Path) -> Result<String> {
    let lock = base.join("source-barrier.lock");
    if named(&lock)?.is_some() {
        return Err(refusal("Native source barrier appeared during observation"));
    }
    let hooks = private_state_hooks_provider()?;
    let registered = hooks.native_registry(runtime_root)?;
    let repositories = set(&registered, "repositories", false)?;
    let private_bases = get(&registered, "private_bases")
        .as_object()
        .ok_or_else(|| refusal("Unverifiable omp bank registry; preserve private state"))?;
    let root_text = text(root)?;
    if repositories.contains(root_text)
        || private_bases.iter().any(|(repo, _)| repo == root_text)
        || hooks.has_native_source_state(base, runtime_root)?
        || (named(&base.join("turn-sources-v2"))?.is_some()
            && hooks.shared_native_source_evidence(root, base, runtime_root)?)
    {
        return Err(refusal(
            "Retained source state lacks its required source barrier",
        ));
    }
    if named(&lock)?.is_some() {
        return Err(refusal("Native source barrier appeared during observation"));
    }
    sha(&registered)
}

enum ObservationBarrier {
    Held(Box<dyn CanonicalWriterBarrier>),
    Absent {
        root: PathBuf,
        base: PathBuf,
        pin: String,
    },
}
impl ObservationBarrier {
    fn acquire(root: &Path, runtime_root: &Path) -> Result<Self> {
        let location = bank_writes::scope_binding_provider()?
            .observe_enrollment_location(root, runtime_root)?;
        let (_, _, recovery_base, _) = match location {
            Some(location) => location,
            None => state::paths(root, None, runtime_root)?,
        };
        let base = recovery_base
            .parent()
            .ok_or_else(|| refusal("Recovery state has no private parent"))?;
        if named(&base.join("source-barrier.lock"))?.is_some() {
            Ok(Self::Held(
                bank_writes::turn_sources_provider()?.source_barrier(
                    root,
                    true,
                    true,
                    runtime_root,
                )?,
            ))
        } else {
            Ok(Self::Absent {
                root: root.to_path_buf(),
                base: base.to_path_buf(),
                pin: native_absence(root, base, runtime_root)?,
            })
        }
    }
    fn pin(&self) -> Option<&str> {
        match self {
            Self::Absent { pin, .. } => Some(pin),
            Self::Held(_) => None,
        }
    }
    fn finish(&mut self, success: bool, runtime_root: &Path) -> Result<()> {
        match self {
            Self::Held(guard) if success => guard.finish(),
            Self::Held(_) => Ok(()),
            Self::Absent { root, base, pin } => {
                if native_absence(root, base, runtime_root)? != *pin {
                    return Err(refusal(
                        "Native registry changed during read-only compatibility observation",
                    ));
                }
                Ok(())
            }
        }
    }
}
/// Read-only observation never creates a source barrier. Ordinary errors still
/// recheck absence; Rust unwinding bypasses rechecks and releases owned guards.
pub fn observation_barrier<T>(
    root: &Path,
    runtime_root: &Path,
    body: impl FnOnce(Option<&str>) -> Result<T>,
) -> Result<T> {
    let mut guard = ObservationBarrier::acquire(root, runtime_root)?;
    let result = body(guard.pin());
    guard.finish(result.is_ok(), runtime_root)?;
    result
}

fn target_routes(target: Option<&Value>) -> Result<Vec<String>> {
    let Some(target) = target else {
        return Ok(vec![]);
    };
    let Some(value) = target.get("layout_required_routes") else {
        return Ok(vec![]);
    };
    value
        .as_array()
        .ok_or_else(|| refusal("Private-state invocation routes are malformed"))?
        .iter()
        .map(|route| {
            route
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| refusal("Private-state invocation routes are malformed"))
        })
        .collect()
}
struct BankRequirement {
    root: PathBuf,
    bank: PathBuf,
    capabilities: BTreeSet<String>,
    token: Value,
}
fn observe_bank(
    supplied_root: &Path,
    target: Option<&Value>,
    native_pin: Option<&str>,
    runtime_root: &Path,
) -> Result<BankRequirement> {
    let (root, bank, _, _) = state::paths(supplied_root, None, runtime_root)?;
    let (operations, _) = bank_writes::state_paths(&root, &bank, LOCATOR_OPERATION)?;
    let base = operations
        .parent()
        .ok_or_else(|| refusal("Private operations have no parent"))?;
    let mut capabilities = BTreeSet::new();
    if native_pin.is_none() {
        capabilities.extend(
            private_state_hooks_provider()?.required_source_capabilities(&root, runtime_root)?,
        );
    }
    let frontier = bank_writes::recovery_frontier_provider()?;
    let token = frontier.observe_bank(&root, &bank, None, None, false, runtime_root)?;
    let indexed = state::_index_for_bank(&root, &bank, runtime_root)?;
    let rows = get(&indexed, "operations")
        .as_object()
        .ok_or_else(|| refusal("Recovery operation index is malformed"))?;
    let mut layout_operations = vec![];
    for (operation, row) in rows {
        if matches!(
            get(row, "state").as_str(),
            Some("active" | "interrupted" | "rolled-back")
        ) {
            let (_, _, value) = frontier.ledger(&root, operation, row, &bank, runtime_root)?;
            let units = get(&value, "units")
                .as_array()
                .ok_or_else(|| refusal("Recovery ledger units are malformed"))?;
            if get(row, "state").as_str() != Some("rolled-back")
                || units
                    .iter()
                    .any(|unit| get(unit, "state").as_str() == Some("pending-admission"))
            {
                layout_operations.push((operation, row));
            }
        }
    }
    if layout_operations
        .iter()
        .any(|(_, row)| matches!(get(row, "state").as_str(), Some("active" | "interrupted")))
    {
        layout_operations.retain(|(_, row)| {
            matches!(get(row, "state").as_str(), Some("active" | "interrupted"))
        });
    }
    if !layout_operations.is_empty() {
        capabilities.insert("layout-frontier-v1".to_owned());
        let target_digest = target
            .and_then(|value| value.get("artifact_digest"))
            .filter(|value| !matches!(value, Value::Null))
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| refusal("Target artifact digest is malformed"))
            })
            .transpose()?;
        let routes = target_routes(target)?;
        for (operation, row) in layout_operations {
            let override_value = target
                .and_then(|value| value.get("layout_quiescence_overrides"))
                .and_then(|value| value.get(operation))
                .filter(|value| !matches!(value, Value::Null));
            if let Some(value) = override_value {
                frontier.validate_quiescence(
                    &root,
                    value,
                    target_digest,
                    &routes,
                    false,
                    runtime_root,
                )?;
            } else if matches!(get(row, "state").as_str(), Some("active" | "interrupted")) {
                frontier.verified_hold_for_runtime(&root, None, runtime_root)?;
            }
            if let Some(target_digest) = target_digest {
                let (directory, _, ledger) =
                    frontier.ledger(&root, operation, row, &bank, runtime_root)?;
                let (_, proposal) = frontier.evidence(&directory, &ledger, row, runtime_root)?;
                let selected = recovery_quiescence_provider()?.selected_attestation(
                    &directory,
                    &proposal,
                    target_digest,
                    runtime_root,
                )?;
                if selected.is_none() && override_value.is_none() {
                    return Err(refusal(
                        "Target capable runtime has no renewed bank-bound quiescence attestation",
                    ));
                }
                if override_value.is_none() {
                    if let Some(selected) = selected {
                        let declared = set(&selected, "declared_invocation_routes", false)?;
                        if routes.iter().any(|route| !declared.contains(route)) {
                            return Err(refusal(
                                "Target capable reader has an invocation route omitted from quiescence",
                            ));
                        }
                    }
                }
            }
        }
    }
    if capabilities
        .iter()
        .any(|token| !TOKENS.contains(&token.as_str()))
    {
        return Err(refusal(
            "A private-state producer returned an unknown capability",
        ));
    }
    if !equal(
        &frontier.observe_bank(&root, &bank, None, None, false, runtime_root)?,
        &token,
    ) {
        return Err(refusal(
            "Private recovery frontier changed during compatibility observation",
        ));
    }
    let (checked_root, checked_bank, _, _) = state::paths(&root, None, runtime_root)?;
    if checked_root != root || checked_bank != bank {
        return Err(refusal(
            "Recovery bank location changed during compatibility observation",
        ));
    }
    if let Some(pin) = native_pin {
        if native_absence(&root, base, runtime_root)? != pin {
            return Err(refusal(
                "Native registry changed during compatibility observation",
            ));
        }
    }
    Ok(BankRequirement {
        root,
        bank,
        capabilities,
        token,
    })
}

pub fn required_private_state_capabilities(
    installation: Option<&Value>,
    target_artifact: Option<&Value>,
    operation: &str,
    current_repo: &Path,
    runtime_root: &Path,
) -> Result<Value> {
    let supported = compatibility(target_artifact)?;
    let RepositoryInventory {
        roots,
        mut unknown,
        pins: inventory_pins,
    } = repository_inventory(installation, current_repo, runtime_root)?;
    let mut requirements = Value::Object(vec![]);
    let mut pins = vec![];
    let mut required = BTreeSet::new();
    let target_compatibility =
        target_artifact.map(|target| target.get("compatibility").unwrap_or(target));
    let receipt_schema = installation
        .and_then(|value| value.get("schema"))
        .and_then(Value::as_str);
    let readable =
        target_compatibility.and_then(|target| target.get("installation_receipt_schemas"));
    if operation != "uninstall"
        && receipt_schema == Some("agent-memory/install-receipt-v2")
        && !readable
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value.as_str() == receipt_schema))
    {
        unknown.push(object([
            ("source", string("installation-receipt")),
            (
                "reason",
                string("target cannot read retained v2 ownership receipt"),
            ),
        ]));
    }
    let private_roots = set(&inventory_pins, "required_bank_repositories", false)?;
    let mut guards = vec![];
    let mut barrier_failed = false;
    // BTreeSet order is the common lexicographic lock order across all banks.
    for root in roots {
        match ordinary_directory(&root) {
            Ok(true) => (),
            Ok(false) => {
                unknown.push(object([
                    ("source", string("bank-inventory")),
                    ("location", string(text(&root)?)),
                ]));
                continue;
            }
            Err(_) => {
                barrier_failed = true;
                break;
            }
        }
        let observed = (|| -> Result<Option<BankRequirement>> {
            let settings = repository_config::settings(&root)?;
            let bank = root.join(settings.bank_root);
            if named(&bank)?.is_none() {
                if private_roots.contains(text(&root)?) {
                    return Err(refusal("Bound private-state bank is unavailable"));
                }
                return Ok(None);
            }
            let held =
                BARRIERS.with(|held| held.borrow().as_ref().map(|roots| roots.contains(&root)));
            if held == Some(false) {
                return Err(refusal(
                    "A bank joined the private-state inventory after lifecycle quiescence",
                ));
            }
            let pin = if held.is_none() {
                guards.push(ObservationBarrier::acquire(&root, runtime_root)?);
                guards.last().and_then(ObservationBarrier::pin)
            } else {
                None
            };
            observe_bank(&root, target_artifact, pin, runtime_root).map(Some)
        })();
        match observed {
            Ok(Some(observed)) => {
                required.extend(observed.capabilities.iter().cloned());
                put(
                    &mut requirements,
                    text(&observed.bank)?,
                    strings(&observed.capabilities),
                )?;
                pins.push(object([
                    ("repo_root", string(text(&observed.root)?)),
                    ("bank_path", string(text(&observed.bank)?)),
                    ("recovery_frontier", observed.token),
                    ("requirements", strings(&observed.capabilities)),
                ]));
            }
            Ok(None) => (),
            Err(_) => unknown.push(object([
                ("source", string("private-state")),
                ("location", string(text(&root)?)),
                (
                    "reason",
                    string(
                        "private state is missing, inaccessible, interrupted, malformed or changed",
                    ),
                ),
            ])),
        }
    }
    // ExitStack checks every successfully entered barrier in reverse order;
    // any exit refusal is an outer synchronization unknown, not a bank result.
    for guard in guards.iter_mut().rev() {
        if guard.finish(!barrier_failed, runtime_root).is_err() {
            barrier_failed = true;
        }
    }
    drop(guards);
    if barrier_failed {
        unknown.push(object([
            ("source", string("private-state-barrier")),
            (
                "reason",
                string("private source synchronization changed during compatibility observation"),
            ),
        ]));
    }
    if let Some(target) = target_artifact
        .filter(|target| get(target, "already_active_verified").as_bool() == Some(true))
    {
        let path = Path::new(get(target, "reader_receipt_path").as_str().ok_or_else(|| {
            refusal("Selected preserved capable runtime receipt changed or is no longer active")
        })?);
        let parent = path.parent().ok_or_else(|| {
            refusal("Selected preserved capable runtime receipt changed or is no longer active")
        })?;
        let hooks = private_state_hooks_provider()?;
        let other = hooks.read_receipt(parent, runtime_root)?;
        let raw = state::read_bytes(path, crate::sharing_limits::MAX_PRIVATE, false)?;
        if other.as_ref().is_none_or(|other| {
            get(get(other, "source"), "artifact_digest") != get(target, "artifact_digest")
        }) || raw.as_deref().and_then(|raw| digest(Some(raw))).as_deref()
            != get(target, "reader_receipt_sha256").as_str()
        {
            return Err(refusal(
                "Selected preserved capable runtime receipt changed or is no longer active",
            ));
        }
        if let Some(other) = other {
            hooks.verify_targets(get(&other, "targets"), get(&other, "source"), runtime_root)?;
        }
    }
    let mut missing: BTreeSet<_> = required.difference(&supported).cloned().collect();
    if operation == "uninstall"
        && target_artifact
            .is_none_or(|target| get(target, "already_active_verified").as_bool() != Some(true))
    {
        missing.extend(required.iter().cloned());
    }
    Ok(object([
        ("requirements", requirements),
        ("required_capabilities", strings(&required)),
        ("target_capabilities", strings(&supported)),
        ("missing_capabilities", strings(&missing)),
        (
            "decision",
            string(if unknown.is_empty() && missing.is_empty() {
                "compatible"
            } else {
                "pending"
            }),
        ),
        ("unknown_state", Value::Array(unknown)),
        (
            "evidence_pins",
            object([("inventory", inventory_pins), ("banks", Value::Array(pins))]),
        ),
        ("operation", string(operation)),
        ("private_evidence_preserved", Value::Bool(true)),
    ]))
}

pub fn require_compatible(
    installation: Option<&Value>,
    target_artifact: Option<&Value>,
    operation: &str,
    expected: Option<&Value>,
    current_repo: &Path,
    runtime_root: &Path,
) -> Result<Value> {
    let current = required_private_state_capabilities(
        installation,
        target_artifact,
        operation,
        current_repo,
        runtime_root,
    )?;
    if get(&current, "decision").as_str() != Some("compatible") {
        return Err(refusal(
            "Private-state runtime compatibility is pending; preserve the capable active runtime and evidence",
        ));
    }
    if expected.is_some_and(|expected| !equal(&current, expected)) {
        return Err(refusal(
            "Private-state inventory changed before runtime activation; quiesce and inspect again",
        ));
    }
    Ok(current)
}

/// Holds all source barriers through transition, activation and exact readback.
/// Nested calls reuse the outer context; Drop restores context and unlocks on
/// every refusal or unwind. No post-body check replaces a panic/fatal exit.
pub fn activation_guard<T>(
    installation: Option<&Value>,
    target_artifact: Option<&Value>,
    operation: &str,
    current_repo: &Path,
    runtime_root: &Path,
    transition: Option<&mut dyn FnMut() -> Result<()>>,
    body: impl FnOnce(&Value) -> Result<T>,
) -> Result<T> {
    let nested = BARRIERS.with(|held| held.borrow().is_some());
    let mut guards = vec![];
    let _restore;
    if !nested {
        let inventory = repository_inventory(installation, current_repo, runtime_root)?;
        if !inventory.unknown.is_empty() {
            return Err(refusal(
                "Private-state bank inventory is unavailable; preserve the active runtime",
            ));
        }
        for root in &inventory.roots {
            if !ordinary_directory(root)? {
                return Err(refusal(
                    "Required private-state repository is unavailable; preserve the active runtime",
                ));
            }
            let settings = repository_config::recovery_settings(root)?;
            if named(&root.join(settings.bank_root))?.is_some_and(|info| info.is_directory()) {
                guards.push(bank_writes::turn_sources_provider()?.source_barrier(
                    root,
                    true,
                    false,
                    runtime_root,
                )?);
            }
        }
        let previous = BARRIERS.with(|held| held.replace(Some(inventory.roots)));
        _restore = Some(RestoreBarriers(previous));
    } else {
        _restore = None;
    }
    if let Some(transition) = transition {
        transition()?;
    }
    let proof = require_compatible(
        installation,
        target_artifact,
        operation,
        None,
        current_repo,
        runtime_root,
    )?;
    let result = body(&proof)?;
    require_compatible(
        installation,
        target_artifact,
        operation,
        Some(&proof),
        current_repo,
        runtime_root,
    )?;
    for guard in guards.iter_mut().rev() {
        guard.finish()?;
    }
    Ok(result)
}

pub fn public_summary(value: &Value) -> Result<Value> {
    let mut result = Value::Object(vec![]);
    for key in [
        "decision",
        "required_capabilities",
        "target_capabilities",
        "missing_capabilities",
    ] {
        put(
            &mut result,
            key,
            value
                .get(key)
                .ok_or_else(|| refusal("Private-state summary is malformed"))?
                .clone(),
        )?;
    }
    let banks = get(value, "requirements")
        .as_object()
        .ok_or_else(|| refusal("Private-state summary is malformed"))?
        .len();
    let unknown = get(value, "unknown_state")
        .as_array()
        .ok_or_else(|| refusal("Private-state summary is malformed"))?
        .len();
    put(&mut result, "observed_banks", Value::Int(banks.into()))?;
    put(
        &mut result,
        "unknown_private_states",
        Value::Int(unknown.into()),
    )?;
    Ok(result)
}

pub fn prepare_lifecycle(plan: &Value, check: bool, runtime_root: &Path) -> Result<Option<Value>> {
    let selected = if get(plan, "operation").as_str() == Some("uninstall") {
        plan.get("preserved_private_state_reader")
    } else {
        Some(
            plan.get("source")
                .ok_or_else(|| refusal("Lifecycle source is missing"))?,
        )
    };
    let target = selected
        .filter(|value| !matches!(value, Value::Null))
        .cloned();
    let supplied = match plan.get("layout_quiescence") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(values)) if values.len() <= MAX_REPOSITORIES => values.as_slice(),
        _ => {
            return Err(refusal(
                "Lifecycle quiescence inputs must be explicit protected-file paths",
            ));
        }
    };
    if supplied.iter().any(|value| value.as_str().is_none()) {
        return Err(refusal(
            "Lifecycle quiescence inputs must be explicit protected-file paths",
        ));
    }
    if target.is_none() && !supplied.is_empty() {
        return Err(refusal(
            "Uninstall renewal needs an actually preserved capable reader",
        ));
    }
    let Some(mut target) = target else {
        return Ok(None);
    };
    let mut routes: Vec<String> = match target.get("reader_invocation_routes") {
        None => vec![],
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| refusal("Lifecycle reader invocation routes are malformed"))
            })
            .collect::<Result<_>>()?,
        _ => return Err(refusal("Lifecycle reader invocation routes are malformed")),
    };
    if routes.is_empty() {
        for action in get(plan, "actions")
            .as_array()
            .ok_or_else(|| refusal("Lifecycle action inventory is malformed"))?
        {
            if get(action, "changed").as_bool() == Some(true)
                && !matches!(get(action, "after"), Value::Null)
                && matches!(
                    get(action, "kind").as_str(),
                    Some("wrapper" | "hook-link" | "hook-config" | "copy" | "link" | "activation")
                )
            {
                routes.push(
                    get(action, "path")
                        .as_str()
                        .ok_or_else(|| refusal("Lifecycle action path is malformed"))?
                        .to_owned(),
                );
            }
        }
    }
    put(
        &mut target,
        "layout_required_routes",
        Value::Array(routes.iter().map(string).collect()),
    )?;
    let mut renewals = vec![];
    for path in supplied {
        let path = Path::new(path.as_str().ok_or_else(|| {
            refusal("Lifecycle quiescence inputs must be explicit protected-file paths")
        })?);
        renewals.push(private_state_hooks_provider()?.read_input(
            path,
            MAX_RENEWAL_BYTES,
            runtime_root,
        )?);
    }
    if check {
        if let Some(values) = plan.get("native_layout_quiescence_overrides") {
            renewals.extend(values.as_array().ok_or_else(|| refusal("Lifecycle renewal uses the exact attestation/independent-review envelope"))?.iter().cloned());
        }
    }
    let mut overrides = Value::Object(vec![]);
    for value in renewals {
        if !value.as_object().is_some_and(|fields| {
            fields.len() == 2 && value.get("attestation").is_some() && value.get("review").is_some()
        }) {
            return Err(refusal(
                "Lifecycle renewal uses the exact attestation/independent-review envelope",
            ));
        }
        let attestation = get(&value, "attestation");
        let operation = get(attestation, "operation_id")
            .as_str()
            .ok_or_else(|| refusal("Lifecycle renewal operation is malformed"))?;
        let repo = Path::new(
            get(attestation, "repo_root")
                .as_str()
                .ok_or_else(|| refusal("Lifecycle renewal repository is malformed"))?,
        );
        if overrides
            .get(operation)
            .is_some_and(|previous| !equal(previous, &value))
        {
            return Err(refusal(
                "Conflicting runtime quiescence declarations cannot select a winner",
            ));
        }
        let target_digest = get(&target, "artifact_digest")
            .as_str()
            .ok_or_else(|| refusal("Lifecycle target artifact digest is missing"))?;
        bank_writes::recovery_frontier_provider()?.validate_quiescence(
            repo,
            &value,
            Some(target_digest),
            &routes,
            !check,
            runtime_root,
        )?;
        put(&mut overrides, operation, value.clone())?;
    }
    if check {
        put(&mut target, "layout_quiescence_overrides", overrides)?;
    }
    Ok(Some(target))
}
