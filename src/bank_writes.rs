//! Common lock and observed single-file replacement, independent of admission.
use crate::bank_contract::{self as contract, Observation};
use crate::codec::json::{self, Value};
use crate::developer_identity::{digest, encoded, load_with_limit, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::host::{self, Sources};
use crate::lifecycle::{get, object, string};
use crate::{bank_capacity, local_registry as storage, repository_config};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub const OPERATIONS: &[&str] = &[
    "replace",
    "checkpoint",
    "initialize",
    "bind",
    "capture",
    "compact",
    "correct",
    "supersede",
    "revoke",
    "forget",
    "refresh",
    "recover",
];
pub const PROPOSAL_FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "operation",
    "repo_root",
    "project_id",
    "observed_hashes",
    "path",
    "content",
    "scope_context",
];
pub const MAX_RECEIPT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_FAULT_WORLD_BYTES: usize = 1024 * 1024;
pub const FAULT_PAUSE_SECONDS: f64 = 10.0;

pub trait RecoveryFrontier: Send + Sync {
    fn capture_scope(&self, repo: &Path) -> Result<Value>;
    fn observe(&self, repo: &Path) -> Result<Value>;
    /// Baseline recovery_frontier.observe(repo, owner, transaction).
    fn observe_owned(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
    ) -> Result<Value>;
    /// Baseline recovery_frontier.verified_hold(repo, owner), returning held.marker.
    fn verified_hold(&self, repo: &Path, owner: Option<&[u8]>) -> Result<Vec<u8>>;
    /// Baseline recovery_frontier.check(frontier, repo, owner, transaction).
    fn check(
        &self,
        frontier: &Value,
        repo: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
    ) -> Result<()>;
    /// Baseline recovery_frontier.require_scope(repo, project, expected).
    fn require_scope(
        &self,
        repo: &Path,
        project: &Value,
        expected: Option<&Value>,
    ) -> Result<Value>;
    /// Read-only frontier observation at an already checked bank location.
    fn observe_bank(
        &self,
        repo: &Path,
        bank: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
        require_identity: bool,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// Exact retained ledger: operation directory, head and current generation.
    fn ledger(
        &self,
        repo: &Path,
        operation_id: &str,
        row: &Value,
        bank: &Path,
        runtime_root: &Path,
    ) -> Result<(std::path::PathBuf, Value, Value)>;
    fn evidence(
        &self,
        directory: &Path,
        current: &Value,
        row: &Value,
        runtime_root: &Path,
    ) -> Result<(Value, Value)>;
    fn validate_quiescence(
        &self,
        repo: &Path,
        value: &Value,
        runtime_artifact_digest: Option<&str>,
        required_routes: &[String],
        publish: bool,
        runtime_root: &Path,
    ) -> Result<Value>;
    fn verified_hold_for_runtime(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Vec<u8>>;
    /// Full verified hold observation at an independently located bank.
    fn verified_enrollment_hold_at_bank(
        &self,
        root: &Path,
        bank: &Path,
        runtime_root: &Path,
    ) -> Result<Option<crate::scope_binding::VerifiedEnrollmentHold>>;
    /// Observe the private frontier, including removed holds and its private index.
    fn observe_enrollment_bank(
        &self,
        root: &Path,
        bank: &Path,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// Guarded source-marker reads; the guard restores prior state on Drop.
    fn observe_source_markers(
        &self,
    ) -> Result<Box<dyn crate::scope_binding::SourceMarkerObservation>>;
    /// Baseline require_scope with the caller's held writer and transaction marker.
    fn require_scope_owned(
        &self,
        repo: &Path,
        project: &Value,
        expected: Option<&Value>,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
    ) -> Result<Value>;
    /// Baseline recovery_frontier.marker_bytes(bank, name, expected=...).
    fn marker_bytes(
        &self,
        bank: &Path,
        name: &str,
        expected: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>>;
    /// Baseline recovery_frontier.marker_value(raw): one exact typed marker.
    fn marker_value(&self, raw: &[u8]) -> Result<Value>;
    /// Baseline recovery_frontier.invalidate(repo): derived-cache status text.
    fn invalidate(&self, repo: &Path) -> Result<String>;
    /// Baseline recovery_frontier.range_value(value).
    fn range_value(&self, value: &Value) -> Result<()>;
    /// Baseline recovery_frontier.verified_hold(repo, owner, transaction): the full hold.
    fn verified_hold_value(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Option<crate::scope_binding::VerifiedEnrollmentHold>>;
    /// Baseline recovery_frontier.pending_rows(repo, token, owner, transaction).
    fn pending_rows(
        &self,
        repo: &Path,
        token: &Value,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Vec<Value>>;
    /// Baseline recovery_frontier.successor_token(token, ids, operation_id=, generation_change=).
    fn successor_token(
        &self,
        token: &Value,
        pending_unit_ids: &[String],
        operation_id: &str,
        generation_change: bool,
    ) -> Result<Value>;
    /// Baseline recovery_frontier.reservation(files, pending, token).
    fn reservation(&self, files: &Value, pending: &[Value], token: &Value) -> Result<Value>;
    /// Baseline recovery_frontier.claim_value(claim, row).
    fn claim_value(&self, claim: &Value, row: &Value) -> Result<()>;
    /// Baseline recovery_frontier.resolve_reference(repo, reference).
    fn resolve_reference(
        &self,
        repo: &Path,
        reference: &Value,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// Baseline recovery_frontier.validate_comparison_review(repo, proof, owner, transaction=).
    fn validate_comparison_review(
        &self,
        repo: &Path,
        proof: &Value,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Option<Value>>;
    /// Baseline recovery_frontier.require_complete(repo, owner, transaction): the
    /// observed token, refused unless coverage is complete and no hold is present.
    fn require_complete(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
    ) -> Result<Value>;
    /// Exact verified hold of a completed rollback, or None.
    fn rolled_back_predecessor(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Option<crate::scope_binding::VerifiedEnrollmentHold>>;
    fn inherited_pending_rows(&self, repo: &Path, runtime_root: &Path) -> Result<Vec<Value>>;
    fn prospective_token(
        &self,
        repo: &Path,
        manifest: &Value,
        pending_units: &[Value],
        predecessor: Option<&crate::scope_binding::VerifiedEnrollmentHold>,
        owner: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<Value>;
    #[allow(clippy::too_many_arguments)]
    fn install(
        &self,
        repo: &Path,
        operation_id: &str,
        proposal: &Value,
        pending_ledger: &Value,
        hold_id: &str,
        owner: &[u8],
        predecessor: Option<&crate::scope_binding::VerifiedEnrollmentHold>,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// Baseline recovery_frontier._observe_index, validating a projected retained index.
    fn observe_index(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
        indexed: &Value,
        runtime_root: &Path,
    ) -> Result<Value>;
}
/// Baseline recovery_frontier marker schemas; recovery_frontier reuses these.
pub const HOLD: &str = "agent-memory/layout-frontier-hold-v1";
pub const NESTED: &str = "agent-memory/layout-frontier-transaction-v1";
pub const TRANSACTION: &str = "agent-memory/layout-recovery-transaction-v1";
pub const SENTINEL: &str = "agent-memory/layout-recovery-sentinel-v1";
static FRONTIER: RwLock<Option<Arc<dyn RecoveryFrontier>>> = RwLock::new(None);
pub fn register_recovery_frontier(provider: Option<Arc<dyn RecoveryFrontier>>) {
    *FRONTIER.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub(crate) fn recovery_frontier_provider() -> Result<Arc<dyn RecoveryFrontier>> {
    frontier()
}
fn frontier() -> Result<Arc<dyn RecoveryFrontier>> {
    FRONTIER
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| missing("recovery_frontier provider is not registered"))
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn missing(message: &str) -> ContractError {
    ContractError::missing_provider(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn path_text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "write path")
}
fn sha(bytes: &[u8]) -> String {
    // Invariant: digest(Some(bytes)) always returns Some, including empty bytes.
    digest(Some(bytes)).expect("present digest input")
}
fn hash_value(bytes: &[u8]) -> Value {
    string(sha(bytes))
}
fn hashes_equal(a: &Value, b: &Value) -> bool {
    // Python dict equality is independent of insertion order.
    match (a.as_object(), b.as_object()) {
        (Some(a), Some(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.iter().any(|(kb, vb)| k == kb && v == vb))
        }
        _ => a == b,
    }
}
pub fn bank_path(repo: &Path) -> Result<(PathBuf, PathBuf)> {
    let root = fs::canonical_path(repo, false)?;
    let bank = repository_config::bank_path(&root)?;
    contract::bank_root(&bank)?;
    DirectoryChain::open(&bank).map_err(|_| err("An existing canonical bank directory is required; initialization uses the multi-file protocol"))?.recheck().map_err(io_error)?;
    Ok((root, bank))
}
pub fn runtime(path: &str) -> bool {
    contract::runtime_path(path)
}
pub fn hashes(bank: &Path) -> Result<Value> {
    crate::bank_observation::scan_hashes(bank)
}
pub fn snapshot(repo: &Path, owned_lock: Option<&[u8]>) -> Result<(Value, Value)> {
    let observed = crate::bank_observation::observe(repo, owned_lock, None)?;
    Ok((observed.checked, observed.hashes))
}
/// Explicit environment injection avoids global environment mutation in callers/tests.
#[derive(Clone, Debug)]
pub struct StateEnvironment {
    pub state_home: Option<OsString>,
    pub xdg_state_home: Option<OsString>,
    pub home: Option<OsString>,
}
impl StateEnvironment {
    pub fn current() -> Self {
        Self {
            state_home: std::env::var_os("AGENT_MEMORY_STATE_HOME"),
            xdg_state_home: std::env::var_os("XDG_STATE_HOME"),
            home: std::env::var_os("HOME"),
        }
    }
}
pub fn state_root(repo: Option<&Path>) -> Result<PathBuf> {
    state_root_with(repo, &StateEnvironment::current())
}
pub fn state_root_with(repo: Option<&Path>, environment: &StateEnvironment) -> Result<PathBuf> {
    let raw = if let Some(raw) = environment.state_home.as_ref().filter(|s| !s.is_empty()) {
        PathBuf::from(raw)
    } else {
        let base = match &environment.xdg_state_home {
            Some(base) => PathBuf::from(base),
            None => host::expanduser_with_home(
                Path::new("~/.local/state"),
                environment.home.as_deref(),
            )?,
        };
        base.join("agent-memory")
    };
    let root = host::expanduser_with_home(&raw, environment.home.as_deref())?;
    if !root.is_absolute()
        || root.components().any(|p| matches!(p, Component::ParentDir))
        || repo.is_some_and(|repo| root.starts_with(repo))
    {
        return Err(err(
            "Write receipts require a canonical absolute state root outside the repository",
        ));
    }
    validate_directories(
        &root,
        "Write receipts require a canonical absolute state root outside the repository",
    )?;
    Ok(root)
}
fn validate_directories(path: &Path, symlink_refusal: &str) -> Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        let Component::Normal(name) = component else {
            continue;
        };
        let parent = DirectoryChain::open(prefix.parent().ok_or_else(|| {
            err("Write receipt directories must be canonical directories without symlinks")
        })?)
        .map_err(io_error)?;
        let info = match parent.directory().stat_at(name) {
            Ok(info) => info,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(io_error(e)),
        };
        if info.is_symlink() {
            return Err(err(symlink_refusal));
        }
        if !info.is_directory() {
            return Err(err(
                "Write receipt directories must be canonical directories without symlinks",
            ));
        }
        parent.recheck().map_err(io_error)?;
        DirectoryChain::open(&prefix)
            .map_err(io_error)?
            .recheck()
            .map_err(io_error)?;
    }
    Ok(())
}
pub fn state_paths(repo: &Path, bank: &Path, operation_id: &str) -> Result<(PathBuf, PathBuf)> {
    state_paths_with(repo, bank, operation_id, &StateEnvironment::current())
}
pub fn state_paths_with(
    repo: &Path,
    bank: &Path,
    operation_id: &str,
    environment: &StateEnvironment,
) -> Result<(PathBuf, PathBuf)> {
    if !uuid_valid(&string(operation_id)) {
        return Err(err("Invalid observed-write operation ID"));
    }
    let root = state_root_with(Some(repo), environment)?;
    let directory = root
        .join("writes")
        .join(sha(path_text(bank)?.as_bytes()))
        .join("operations");
    validate_directories(
        &directory,
        "Write receipt directories must be canonical directories without symlinks",
    )?;
    let receipt = directory.join(format!("{operation_id}.json"));
    match DirectoryChain::open(&directory) {
        Ok(chain) => {
            match chain.directory().stat_at(
                receipt
                    .file_name()
                    .ok_or_else(|| err("Write receipt has no filename"))?,
            ) {
                Ok(info) if info.is_symlink() => {
                    return Err(err("Write receipt must not be a symlink"));
                }
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(io_error(e)),
            }
            chain.recheck().map_err(io_error)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(io_error(e)),
    }
    Ok((directory, receipt))
}
pub fn result(operation: &str, fields: &Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/write-result-v1")),
        ("operation", string(operation)),
    ]);
    contract::merge(&mut value, fields)?;
    Ok(value)
}
pub fn validate_replacement(observation: &Observation, path: &str, content: &str) -> Result<()> {
    let mut files = Value::Object(
        observation
            .sources
            .iter()
            .map(|(p, raw)| {
                Ok((
                    p.clone(),
                    string(
                        std::str::from_utf8(raw)
                            .map_err(|_| ContractError::non_utf8("canonical source"))?,
                    ),
                ))
            })
            .collect::<Result<_>>()?,
    );
    contract::merge(&mut files, &object([(path, string(content))]))?;
    contract::validate_files(&files, None)?;
    Ok(())
}
pub fn preview(repo: &Path, path: &str, content: &str, operation: &str) -> Result<Value> {
    preview_with_sources(repo, path, content, operation, Sources::default())
}
pub fn preview_with_sources(
    repo: &Path,
    path: &str,
    content: &str,
    operation: &str,
    sources: Sources,
) -> Result<Value> {
    let (root, _) = bank_path(repo)?;
    let observed = crate::bank_observation::observe(&root, None, None)?.into_contract();
    let proposal = object([
        ("schema", string("agent-memory/write-proposal-v1")),
        ("operation_id", string((sources.uuid4)()?)),
        ("operation", string(operation)),
        ("repo_root", string(path_text(&root)?)),
        ("project_id", get(&observed.checked, "project_id").clone()),
        ("observed_hashes", observed.hashes.clone()),
        ("path", string(path)),
        ("content", string(content)),
        ("scope_context", frontier()?.capture_scope(&root)?),
    ]);
    proposal_value(&proposal)?;
    validate_replacement(&observed, path, content)?;
    result(
        "preview",
        &object([
            ("proposal", proposal),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
            ("changed_memory_paths", Value::Array(vec![])),
        ]),
    )
}
pub fn proposal_value(prepared: &Value) -> Result<&Value> {
    let proposal = prepared.get("proposal").unwrap_or(prepared);
    let invalid = || err("Invalid observed-write proposal");
    let mut allowed = PROPOSAL_FIELDS.to_vec();
    allowed.push("checkpoint_evidence");
    if crate::records::fields(proposal, PROPOSAL_FIELDS, &allowed).is_err()
        || get(proposal, "schema").as_str() != Some("agent-memory/write-proposal-v1")
        || !uuid_valid(get(proposal, "operation_id"))
        || !uuid_valid(get(proposal, "project_id"))
        || !get(proposal, "operation")
            .as_str()
            .is_some_and(|s| OPERATIONS.contains(&s))
    {
        return Err(invalid());
    }
    let root = get(proposal, "repo_root").as_str().ok_or_else(invalid)?;
    if !Path::new(root).is_absolute()
        || fs::canonical_path(Path::new(root), true)
            .map_or(true, |p| p.as_os_str() != OsStr::new(root))
    {
        return Err(invalid());
    }
    contract::path_value(get(proposal, "path"))?;
    crate::scope_binding::context_value(get(proposal, "scope_context"), false)?;
    if get(proposal, "path").as_str() == Some("MEMORY.md") {
        return Err(err(
            "Router/identity changes require the governed multi-file or binding operation",
        ));
    }
    if get(proposal, "content")
        .as_str()
        .is_none_or(|s| contract::trim(s).is_empty())
    {
        return Err(err(
            "Single-file replacement requires nonempty Markdown; deletion uses the governed forget operation",
        ));
    }
    let observed = get(proposal, "observed_hashes");
    let path = proposal_text(proposal, "path")?;
    if observed.as_object().is_none()
        || observed.get(path).is_none()
        || observed.get("MEMORY.md").is_none()
    {
        return Err(err(
            "Proposal must include the complete observed bank and an existing target",
        ));
    }
    for (path, sha) in observed.as_object().ok_or_else(|| {
        err("Proposal must include the complete observed bank and an existing target")
    })? {
        if path != "MEMORY.md" {
            contract::path_value(&string(path))?;
        }
        if !sha.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err(err("Observed hashes must be SHA256 values"));
        }
    }
    if proposal.get("checkpoint_evidence").is_some() {
        crate::checkpoint::validate_write(proposal, false)?;
    }
    Ok(proposal)
}
/// Faults are opt-in, token-bound disposable worlds; Markdown never enables them.
pub fn fault(stage: &str) -> Result<()> {
    if std::env::var("AGENT_MEMORY_FAULT_STAGE").ok().as_deref() != Some(stage) {
        return Ok(());
    }
    let root = PathBuf::from(
        std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap_or_else(|| "/missing-test-root".into()),
    );
    let token = std::env::var("AGENT_MEMORY_TEST_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let invalid = || err("Fault injection requires an explicit disposable test world");
    let actual = fs::canonical_path(&root, false).map_err(|_| invalid())?;
    let temporary = fs::canonical_path(Path::new("/tmp"), false).map_err(|_| invalid())?;
    if token.is_none()
        || !root.is_absolute()
        || actual.parent() != Some(temporary.as_path())
        || !root.file_name().is_some_and(|s| {
            s.to_str()
                .is_some_and(|s| s.starts_with("agent-memory-test-"))
        })
    {
        return Err(invalid());
    }
    let world_chain = DirectoryChain::open(&root).map_err(|_| invalid())?;
    world_chain.recheck().map_err(|_| invalid())?;
    let raw = fs::read_regular(&root, Path::new(".test-world.json"), MAX_FAULT_WORLD_BYTES)
        .map_err(|_| invalid())?
        .0;
    if get(
        &load_with_limit(&raw, MAX_FAULT_WORLD_BYTES).map_err(|_| invalid())?,
        "token",
    )
    .as_str()
        != token.as_deref()
    {
        return Err(invalid());
    }
    match std::env::var("AGENT_MEMORY_FAULT_MODE").ok().as_deref() {
        Some("exit") => std::process::exit(86),
        Some("pause") => {
            let directory = PathBuf::from(
                std::env::var_os("AGENT_MEMORY_FAULT_DIR")
                    .ok_or_else(|| err("Fault rendezvous must stay in the disposable world"))?,
            );
            // Baseline `Path.resolve()` is non-strict: the rendezvous directory
            // may not exist yet; it is created below after these checks.
            let canonical = fs::canonical_path(&directory, false)?;
            if !directory.is_absolute()
                || !canonical.starts_with(&actual)
                || directory
                    .components()
                    .any(|p| matches!(p, Component::ParentDir))
            {
                return Err(err("Fault rendezvous must stay in the disposable world"));
            }
            validate_directories(
                &directory,
                "Fault rendezvous must stay in the disposable world",
            )?;
            let chain = DirectoryChain::create(&directory, false).map_err(io_error)?;
            let leaf = stage.replace('/', "%2F");
            let ready = OsString::from(format!("{leaf}.ready"));
            let release = OsString::from(format!("{leaf}.release"));
            let (file, _) = chain
                .directory()
                .open_or_create(&ready, libc::O_WRONLY, 0o600)
                .map_err(io_error)?;
            if !file.stat().map_err(io_error)?.is_regular() {
                return Err(err("Fault rendezvous requires ordinary files"));
            }
            let deadline = crate::clock::Deadline::from_seconds(FAULT_PAUSE_SECONDS)?;
            loop {
                chain.recheck().map_err(io_error)?;
                match chain.directory().stat_at(&release) {
                    Ok(info) if info.is_regular() => break,
                    Ok(_) => return Err(err("Fault rendezvous requires ordinary files")),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(io_error(e)),
                }
                if deadline.expired() {
                    return Err(err("Isolated fault rendezvous timed out"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        }
        _ => Err(ContractError::new(format!(
            "Injected write failure at {stage}"
        ))),
    }
}
pub fn record(receipt_path: &Path, payload: &[u8]) -> Result<()> {
    storage::private_mkdir(
        receipt_path
            .parent()
            .ok_or_else(|| err("Write receipt has no directory"))?,
    )?;
    storage::replace(receipt_path, payload, None)
}
fn exists(path: &Path) -> Result<bool> {
    let Some(parent) = path.parent() else {
        return Err(err("Write path has no parent"));
    };
    let Some(name) = path.file_name() else {
        return Err(err("Write path has no filename"));
    };
    match DirectoryChain::open(parent) {
        Ok(chain) => {
            let result = match chain.directory().stat_at(name) {
                Ok(info) if info.is_symlink() => {
                    return Err(err("Write receipt must not be a symlink"));
                }
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(io_error(e)),
            };
            chain.recheck().map_err(io_error)?;
            Ok(result)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_error(e)),
    }
}
fn checkpoint_sources(proposal: &Value) -> Result<()> {
    if proposal.get("checkpoint_evidence").is_some() {
        let (root, bank) = bank_path(Path::new(proposal_text(proposal, "repo_root")?))?;
        let (_, receipt) = state_paths(&root, &bank, proposal_text(proposal, "operation_id")?)?;
        crate::checkpoint::validate_write(proposal, !exists(&receipt)?)?;
    }
    Ok(())
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let proposal = proposal_value(prepared)?;
    let root = Path::new(proposal_text(proposal, "repo_root")?);
    let frontier = frontier()?.observe(root)?;
    if frontier
        .get("hold_sha256")
        .ok_or_else(|| err("Recovery frontier lacks hold coverage"))?
        != &Value::Null
    {
        return crate::recovery_transactions::apply_checkpoint(
            proposal,
            &crate::cli::runtime_root()?,
        );
    }
    if get(proposal, "scope_context") == &Value::Null {
        return Err(err(
            "Proposal has no resolved scope; finish identity setup and preview again",
        ));
    }
    let require = |expected: &Value, owned_lock: Option<&[u8]>| -> Result<Value> {
        crate::scope_binding::require(
            root,
            crate::scope_binding::optional_project(get(proposal, "project_id"))?,
            crate::scope_binding::RequireOptions {
                expected: (expected != &Value::Null).then_some(expected),
                owned_lock,
                ..Default::default()
            },
        )
    };
    let context = require(get(proposal, "scope_context"), None)?;
    checkpoint_sources(proposal)?;
    let operation = proposal_text(proposal, "operation")?;
    let operation_id = proposal_text(proposal, "operation_id")?;
    let check = |owner: &[u8]| -> Result<()> {
        require(&context, Some(owner))?;
        checkpoint_sources(proposal)?;
        if operation == "checkpoint" {
            crate::security::guarded_start(
                root,
                operation_id,
                &context,
                &object([(
                    proposal_text(proposal, "path")?,
                    get(proposal, "content").clone(),
                )]),
                &["assistant"],
                "checkpoint",
                false,
            )?;
        } else {
            crate::security::audit(
                root,
                operation_id,
                "mutation",
                "started",
                &context,
                &["external"],
                &[],
                &[],
                operation,
                None,
            )?;
        }
        Ok(())
    };
    let mut receipt = apply_with_identity(proposal, check)?;
    let audit = crate::security::audit(
        root,
        operation_id,
        "mutation",
        "completed",
        &context,
        if operation == "checkpoint" {
            &["assistant"]
        } else {
            &["external"]
        },
        &[],
        &[],
        operation,
        None,
    )?;
    contract::merge(
        &mut receipt,
        &object([("resolved_scope", context), ("security_audit", audit)]),
    )?;
    Ok(receipt)
}
struct Temporary<'a> {
    directory: &'a fs::Descriptor,
    name: OsString,
    identity: fs::Identity,
    present: bool,
}
impl Temporary<'_> {
    fn finish(&mut self) -> Result<()> {
        if !self.present {
            return Ok(());
        }
        let current = match self.directory.stat_at(&self.name) {
            Ok(current) => current,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.present = false;
                return Ok(());
            }
            Err(error) => return Err(io_error(error)),
        };
        if !current.same_inode(self.identity) {
            self.present = false;
            return Err(err("Write temporary changed; preserve it for inspection"));
        }
        self.directory.unlink_at(&self.name).map_err(io_error)?;
        self.present = false;
        Ok(())
    }
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if self.present {
            let _ = self.finish();
        }
    }
}
/// Internal observed replacement protocol. Admission callers supply their full identity guard.
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
    let proposal = proposal_value(prepared)?;
    let (root, bank) = bank_path(Path::new(proposal_text(proposal, "repo_root")?))?;
    let (_, receipt_path) = state_paths_with(
        &root,
        &bank,
        proposal_text(proposal, "operation_id")?,
        environment,
    )?;
    let request_sha = sha(&encoded(proposal)?);
    let path = proposal_text(proposal, "path")?;
    let content = proposal_text(proposal, "content")?;
    let target = bank.join(path);
    let mut expected = get(proposal, "observed_hashes").clone();
    contract::merge(
        &mut expected,
        &object([(path, hash_value(content.as_bytes()))]),
    )?;
    // No inventory or receipt is read before canonical barrier + exclusive bank lock.
    let mut barrier =
        crate::turn_sources::canonical_writer_barrier(&root, None, &crate::cli::pack_root()?)?;
    let outcome = storage::exclusive(
        &bank.join(".agent-memory.lock"),
        proposal_text(proposal, "operation_id")?,
        &format!("bank-{}", proposal_text(proposal, "operation")?),
        sources,
        |owner| {
            fault("locked-before-inventory")?;
            check_identity(owner)?;
            let observation =
                crate::bank_observation::observe(&root, Some(owner), None)?.into_contract();
            if get(&observation.checked, "project_id") != get(proposal, "project_id") {
                return Err(err("Bank project identity changed after preview"));
            }
            if exists(&receipt_path)? {
                let raw = fs::read_source_with_options(
                    receipt_path
                        .parent()
                        .ok_or_else(|| err("Write receipt has no directory"))?,
                    Path::new(
                        receipt_path
                            .file_name()
                            .ok_or_else(|| err("Write receipt has no filename"))?,
                    ),
                    MAX_RECEIPT_BYTES,
                    fs::ReadOptions::default(),
                )
                .map_err(io_error)?
                .bytes;
                let known = load_receipt(&raw)?;
                if known.as_object().is_none()
                    || get(&known, "schema").as_str() != Some("agent-memory/write-receipt-v1")
                    || get(&known, "operation_id") != get(proposal, "operation_id")
                    || get(&known, "request_sha256").as_str() != Some(request_sha.as_str())
                    || get(&known, "project_id") != get(proposal, "project_id")
                    || get(&known, "bank_path").as_str() != Some(path_text(&bank)?)
                    || !hashes_equal(get(&known, "resulting_hashes"), &expected)
                    || get(&known, "verified").as_bool() != Some(true)
                {
                    return Err(err(
                        "Unknown receipt or operation ID reused for another proposal; preserve it",
                    ));
                }
                let mut fields = Value::Object(
                    known
                        .as_object()
                        .ok_or_else(|| err("Unknown receipt or operation ID reused for another proposal; preserve it"))?
                        .iter()
                        .filter(|(k, _)| k != "schema")
                        .cloned()
                        .collect(),
                );
                contract::merge(
                    &mut fields,
                    &object([
                        ("already_applied", Value::Bool(true)),
                        ("writes_performed", Value::Bool(false)),
                        (
                            "still_current",
                            Value::Bool(hashes_equal(&observation.hashes, &expected)),
                        ),
                        ("receipt_path", string(path_text(&receipt_path)?)),
                    ]),
                )?;
                return result("apply", &fields);
            }
            let already_present = hashes_equal(&observation.hashes, &expected);
            if !hashes_equal(&observation.hashes, get(proposal, "observed_hashes"))
                && !already_present
            {
                return Err(err(
                    "Bank changed after preview; validate the external edit and prepare a fresh proposal",
                ));
            }
            if !already_present {
                validate_replacement(&observation, path, content)?;
            }
            let mut wrote = false;
            if !already_present {
                let parent = target
                    .parent()
                    .ok_or_else(|| err("Canonical replacement target has no parent"))?;
                let chain = DirectoryChain::open(parent).map_err(io_error)?;
                let directory = chain.directory();
                let name = target
                    .file_name()
                    .ok_or_else(|| err("Canonical replacement target has no filename"))?;
                let before = directory.stat_at(name).map_err(io_error)?;
                if !before.is_regular() {
                    return Err(err(
                        "Canonical replacement target must remain an ordinary file",
                    ));
                }
                let temp_name =
                    OsString::from(format!(".agent-memory-write-{}.tmp", (sources.uuid4)()?));
                let mut file = directory
                    .create_exclusive(&temp_name, libc::O_WRONLY, before.mode & 0o777)
                    .map_err(io_error)?;
                let identity = match file.stat() {
                    Ok(identity) => identity,
                    Err(error) => {
                        // No initial identity is available: match baseline cleanup.
                        let _ = directory.unlink_at(&temp_name);
                        return Err(io_error(error));
                    }
                };
                let mut temporary = Temporary {
                    directory,
                    name: temp_name,
                    identity,
                    present: true,
                };
                let replacement = (|| -> Result<()> {
                    file.write_all(content.as_bytes()).map_err(io_error)?;
                    file.fsync().map_err(io_error)?;
                    fault("before-recheck")?;
                    if proposal.get("checkpoint_evidence").is_some() {
                        check_identity(owner)?;
                    }
                    if !hashes_equal(
                        &snapshot(&root, Some(owner))?.1,
                        get(proposal, "observed_hashes"),
                    ) {
                        return Err(err(
                            "External edit observed immediately before replacement; preserve it",
                        ));
                    }
                    chain.recheck().map_err(io_error)?;
                    if directory.stat_at(name).map_err(io_error)? != before {
                        return Err(err(
                            "External edit observed immediately before replacement; preserve it",
                        ));
                    }
                    if !directory
                        .stat_at(&temporary.name)
                        .map_err(io_error)?
                        .same_inode(temporary.identity)
                    {
                        return Err(err("Write temporary changed; preserve it for inspection"));
                    }
                    fs::rename_at(
                        directory,
                        &temporary.name,
                        directory,
                        name,
                        fs::RenameMode::Replace,
                    )
                    .map_err(io_error)?;
                    temporary.present = false;
                    wrote = true;
                    directory.fsync_directory().map_err(io_error)?;
                    chain.recheck().map_err(io_error)?;
                    fault("after-replace")?;
                    Ok(())
                })();
                let cleanup = temporary.finish();
                replacement?;
                cleanup?;
            }
            if !hashes_equal(&snapshot(&root, Some(owner))?.1, &expected) {
                return Err(err(
                    "Post-write bank readback differs; operation may have changed memory, inspect/replay",
                ));
            }
            fault("before-receipt")?;
            if !hashes_equal(&snapshot(&root, Some(owner))?.1, &expected) {
                return Err(err(
                    "Bank changed before receipt; do not claim a verified current write",
                ));
            }
            let receipt = object([
                ("schema", string("agent-memory/write-receipt-v1")),
                ("operation_id", get(proposal, "operation_id").clone()),
                ("project_id", get(proposal, "project_id").clone()),
                ("bank_path", string(path_text(&bank)?)),
                ("request_sha256", string(&request_sha)),
                ("observed_hashes", get(proposal, "observed_hashes").clone()),
                ("resulting_hashes", expected.clone()),
                (
                    "changed_memory_paths",
                    Value::Array(
                        if !hashes_equal(&expected, get(proposal, "observed_hashes")) {
                            vec![string(format!(
                                "{}/{path}",
                                repository_config::relative(&root)?
                            ))]
                        } else {
                            vec![]
                        },
                    ),
                ),
                (
                    "outcome",
                    string(if wrote { "replaced" } else { "already-present" }),
                ),
                ("readback_at", string((sources.utc_now)()?)),
                ("verified", Value::Bool(true)),
            ]);
            record(&receipt_path, &encoded(&receipt)?)?;
            fault("after-receipt")?;
            let mut fields = Value::Object(
                receipt
                    .as_object()
                    .ok_or_else(|| err("Write receipt must be an object"))?
                    .iter()
                    .filter(|(k, _)| k != "schema")
                    .cloned()
                    .collect(),
            );
            contract::merge(
                &mut fields,
                &object([
                    ("already_applied", Value::Bool(false)),
                    ("writes_performed", Value::Bool(wrote)),
                    ("still_current", Value::Bool(true)),
                    ("receipt_path", string(path_text(&receipt_path)?)),
                ]),
            )?;
            result("apply", &fields)
        },
    );
    if outcome.is_ok() {
        barrier.finish()?;
    }
    outcome
}
pub fn recover(repo: &Path, prepared: Option<&Value>) -> Result<Value> {
    let (_, bank) = bank_path(repo)?;
    let operations: Vec<_> = OPERATIONS.iter().map(|op| format!("bank-{op}")).collect();
    let borrowed: Vec<_> = operations.iter().map(String::as_str).collect();
    let recovered = storage::recover(
        &bank.join("MEMORY.md"),
        &bank.join(".agent-memory.lock"),
        &borrowed,
        prepared,
        "agent-memory/bank-lock-recovery-v1",
        Sources::default(),
    )?;
    result("recover", &recovered)
}
pub fn inspect(repo: &Path) -> Result<Value> {
    let (checked, observed) = snapshot(repo, None)?;
    let relative = repository_config::relative(repo)?;
    let shared = Value::Array(
        observed
            .as_object()
            .ok_or_else(|| err("Observed bank hashes must be an object"))?
            .iter()
            .map(|(p, _)| string(format!("{relative}/{p}")))
            .collect(),
    );
    result(
        "inspect",
        &object([
            ("project_id", get(&checked, "project_id").clone()),
            ("observed_hashes", observed),
            ("shared_memory_paths", shared),
            ("writes_performed", Value::Bool(false)),
        ]),
    )
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(v) => (v, 0, json::INDENT2_UTF8),
        Err(e) => {
            let mut fields = object([
                ("error", string(e.to_string())),
                ("verified", Value::Bool(false)),
                ("inspection_required", Value::Bool(operation == "apply")),
            ]);
            // Invariant: fields and capacity details are constructed objects.
            contract::merge(&mut fields, &bank_capacity::details(&e))
                .expect("capacity result fields");
            // Invariant: this branch constructs an object for result's fields argument.
            (
                result(operation, &fields).expect("write error result"),
                1,
                json::DEFAULT,
            )
        }
    };
    let stdout = match json::encode(&value, style) {
        Ok(encoded) => encoded + "\n",
        Err(error) => return command_result(operation, Err(error)),
    };
    crate::lifecycle::CommandResult {
        stdout,
        value: Some(value),
        exit_code,
        stderr: String::new(),
    }
}

fn proposal_text<'a>(proposal: &'a Value, key: &str) -> Result<&'a str> {
    contract::required_text(proposal, key, "Invalid observed-write proposal")
}

/// Bounded strict decoding used for an existing observed-write receipt.
pub fn load_receipt(raw: &[u8]) -> Result<Value> {
    load_with_limit(raw, MAX_RECEIPT_BYTES)
}
