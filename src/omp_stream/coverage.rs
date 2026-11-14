//! Per-instance finite coverage: OPEN, attempt, start, outcome, transition
//! and close records under the source barrier and the stream lock.
use super::executor::{ObserveCall, ObserveError};
use super::{
    ACTIONS, MAX_IDENTITY, MAX_IDS, MAX_METADATA, REFUSAL_CODES, absolute_without_parent, digest,
    directory, err, exact, hint_identity, identity, installation_id, integer, is_hint, lexists,
    object, opt_string, parent, read_required, retain, same, session_creation_lock,
    session_instances, sha, startup_marker, string, strings, uuid, with_lock,
};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::fs::{self, PrivateLock};
use crate::lifecycle::get;
use crate::omp_installation::{self, HeldSource, ObserverBinding};
use crate::turn_sources::{self, SourceBarrier};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The per-frame executable attestation (production: an
/// `omp_installation::ObserverBinding`). Opaque, never serialized.
pub type FrameProof = Arc<dyn Any + Send + Sync>;

/// Everything the finite stream needs from its installed environment: the
/// private base, the source barrier, the read-only observation barrier, the
/// executable attestation, the marker/scope check and the transport.
pub trait StreamAuthority: Send + Sync {
    /// Private state base of `repo` (the baseline `_base`).
    fn base(&self, repo: &Path) -> Result<PathBuf>;
    /// Shared source barrier of `repo`.
    fn source_barrier(&self, repo: &Path) -> Result<SourceBarrier>;
    /// Read-only lifecycle observation barrier (never creates a source barrier).
    fn observation_barrier(
        &self,
        repo: &Path,
        body: &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value>;
    /// One full executable attestation per OPEN or source frame, outside transport.
    fn attest(&self, binding: &Value, receipt: Option<&Value>) -> Result<FrameProof>;
    /// Fast same-frame executable, marker and scope check: run `body` with the
    /// marker context and recheck everything at exit, including on refusal.
    fn verify(
        &self,
        repo: &Path,
        binding: &Value,
        receipt: Option<&Value>,
        proof: &FrameProof,
        held: Option<&SourceBarrier>,
        body: &mut dyn FnMut(&Value) -> Result<()>,
    ) -> Result<()>;
    /// The native transport (observer, post-response drain, dispositions).
    fn transport(&self) -> Result<Arc<dyn OmpTransport>>;
}

/// Seam to `omp_transport` (not yet ported): the native observer, the
/// post-response source drain, and the fresh verified native disposition set.
pub trait OmpTransport: Send + Sync {
    fn observe(
        &self,
        repo: &Path,
        request: &Value,
        call: ObserveCall<'_, '_>,
    ) -> std::result::Result<Value, ObserveError>;
    fn drain_after_response(
        &self,
        repo: &Path,
        receipt: Option<&Value>,
        proof: &FrameProof,
    ) -> Result<()>;
    fn native_disposition_ids(&self, repo: &Path, session: &str) -> Result<BTreeSet<String>>;
}

fn installation_error(error: omp_installation::Error) -> ContractError {
    match error {
        omp_installation::Error::Contract(error) => error,
        omp_installation::Error::MissingActivation => {
            ContractError::missing_provider(omp_installation::Error::MissingActivation.to_string())
        }
    }
}
impl HeldSource for SourceBarrier {
    fn recheck(&self) -> omp_installation::Result<()> {
        SourceBarrier::recheck(self).map_err(omp_installation::Error::Contract)
    }
}

/// The installed authority: real receipt attestation, marker/scope binding,
/// source barrier, read-only private-state barrier and registered transport.
pub struct Installed {
    runtime_root: PathBuf,
}
impl Installed {
    pub fn new(runtime_root: &Path) -> Self {
        Self {
            runtime_root: runtime_root.to_path_buf(),
        }
    }
}
fn receipt_required(receipt: Option<&Value>) -> Result<&Value> {
    receipt
        .filter(|r| r.as_object().is_some() && get(r, "omp_binding").as_object().is_some())
        .ok_or_else(|| err("Finite stream requires its original installed observer receipt"))
}
impl StreamAuthority for Installed {
    fn base(&self, repo: &Path) -> Result<PathBuf> {
        let (root, bank) = crate::bank_writes::bank_path(repo)?;
        let (operations, _) =
            crate::bank_writes::state_paths(&root, &bank, &crate::host::uuid4()?)?;
        Ok(parent(&operations)?.to_path_buf())
    }
    fn source_barrier(&self, repo: &Path) -> Result<SourceBarrier> {
        turn_sources::source_barrier(repo, false, false, None, &self.runtime_root)
    }
    fn observation_barrier(
        &self,
        repo: &Path,
        body: &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value> {
        crate::private_state::observation_barrier(repo, &self.runtime_root, |_| body())
    }
    fn attest(&self, binding: &Value, receipt: Option<&Value>) -> Result<FrameProof> {
        let receipt = receipt_required(receipt)?;
        if get(receipt, "installation_id") != get(binding, "installation_id")
            || get(get(receipt, "source"), "artifact_digest") != get(binding, "artifact_digest")
        {
            return Err(err(
                "Finite stream requires its original installed observer receipt",
            ));
        }
        let artifact = get(binding, "artifact_digest")
            .as_str()
            .ok_or_else(|| err("Finite stream requires its original installed observer receipt"))?;
        let agent_dir = get(get(receipt, "omp_binding"), "agent_dir")
            .as_str()
            .ok_or_else(|| err("Finite stream requires its original installed observer receipt"))?;
        let proof = omp_installation::verified_observer_binding(artifact, agent_dir, Some(receipt))
            .map_err(installation_error)?;
        Ok(Arc::new(proof))
    }
    fn verify(
        &self,
        repo: &Path,
        binding: &Value,
        receipt: Option<&Value>,
        proof: &FrameProof,
        held: Option<&SourceBarrier>,
        body: &mut dyn FnMut(&Value) -> Result<()>,
    ) -> Result<()> {
        let receipt = receipt_required(receipt)?;
        let proof = proof
            .downcast_ref::<ObserverBinding>()
            .ok_or_else(|| err("Native observer requires its exact internally verified binding"))?;
        let held = held.map(|barrier| barrier as &dyn HeldSource);
        omp_installation::observe_observer_binding(repo, receipt, proof, held, |context| {
            check_scope(context, binding).map_err(omp_installation::Error::Contract)?;
            body(context).map_err(omp_installation::Error::Contract)
        })
        .map_err(installation_error)
    }
    fn transport(&self) -> Result<Arc<dyn OmpTransport>> {
        Ok(Arc::new(crate::omp_transport::Transport::installed()))
    }
}
/// The marker context must still name the OPEN's project, root, bank and checkout.
pub(crate) fn check_scope(context: &Value, binding: &Value) -> Result<()> {
    if get(context, "project_id") != get(binding, "project_id")
        || get(context, "root") != get(binding, "repo_root")
        || get(context, "bank") != get(binding, "bank_path")
        || get(get(context, "scope"), "checkout_key") != get(binding, "checkout_key")
    {
        return Err(err("Finite stream project/bank/checkout binding changed"));
    }
    Ok(())
}

/// Run `body` under the authority's shared source barrier with its exit recheck.
pub(crate) fn barrier<T>(
    authority: &dyn StreamAuthority,
    repo: &Path,
    body: impl FnOnce(&SourceBarrier) -> Result<T>,
) -> Result<T> {
    let held = authority.source_barrier(repo)?;
    let value = body(&held)?;
    held.recheck()?;
    Ok(value)
}
/// `authority.verify` with a single-use body and its typed result.
pub(crate) fn verified<T>(
    authority: &dyn StreamAuthority,
    repo: &Path,
    binding: &Value,
    receipt: Option<&Value>,
    proof: &FrameProof,
    held: Option<&SourceBarrier>,
    body: impl FnOnce(&Value) -> Result<T>,
) -> Result<T> {
    let mut body = Some(body);
    let mut output = None;
    authority.verify(repo, binding, receipt, proof, held, &mut |context| {
        let call = body
            .take()
            .ok_or_else(|| err("Finite stream binding check ran its operation twice"))?;
        output = Some(call(context)?);
        Ok(())
    })?;
    output.ok_or_else(|| err("Finite stream binding check did not run its operation"))
}

/// Startup binding returned by `stream_startup`, checked before OPEN.
pub fn binding<'a>(
    value: &'a Value,
    repo: &Path,
    _session: &str,
    _instance: &str,
    lifecycle: &str,
) -> Result<&'a Value> {
    let fields = [
        "project_id",
        "repo_root",
        "bank_path",
        "checkout_key",
        "artifact_digest",
        "installation_id",
        "initial_lifecycle",
    ];
    exact(value, &fields)?;
    for key in fields {
        turn_sources::bounded(get(value, key), 4096)?;
    }
    uuid(get(value, "project_id"))?;
    installation_id(get(value, "installation_id"))?;
    uuid(get(value, "initial_lifecycle"))?;
    sha(get(value, "artifact_digest"))?;
    let text = |key| get(value, key).as_str().unwrap_or_default();
    let resolved = fs::canonical_path(repo, false)?;
    if text("initial_lifecycle") != lifecycle || !resolved.starts_with(text("repo_root")) {
        return Err(err("Foreign finite stream startup binding"));
    }
    for key in ["repo_root", "bank_path"] {
        let path = Path::new(text(key));
        if !absolute_without_parent(path) || fs::canonical_path(path, false)? != path {
            return Err(err("Redirected finite stream startup scope"));
        }
    }
    if !Path::new(text("bank_path")).starts_with(text("repo_root")) {
        return Err(err("Foreign finite stream bank"));
    }
    Ok(value)
}

/// Original actual source references: exact four-field items only.
pub(crate) fn source_refs(values: &Value) -> Result<Vec<Value>> {
    let items = values
        .as_array()
        .filter(|items| items.len() <= MAX_IDS)
        .ok_or_else(|| err("Invalid original source reference inventory"))?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        exact(
            item,
            &["session", "lifecycle", "observation_id", "source_sha256"],
        )?;
        turn_sources::bounded(get(item, "session"), 512)?;
        uuid(get(item, "lifecycle"))?;
        uuid(get(item, "observation_id"))?;
        sha(get(item, "source_sha256"))?;
        result.push(item.clone());
    }
    if turn_sources::encoded(&Value::Array(result.clone()))?.len() > MAX_IDENTITY {
        return Err(err("Original source references exceed bound"));
    }
    Ok(result)
}
pub(crate) fn returned_refs(result: &Value) -> Vec<Value> {
    match result.get("source_references") {
        None => Vec::new(),
        Some(refs) => source_refs(refs).unwrap_or_default(),
    }
}

/// The envelope identity must equal the stored view's exact native graph and tools.
pub(crate) fn request_identity(request: &Value, known: &Value) -> Result<()> {
    identity(known)?;
    if matches!(known, Value::Null) {
        return Ok(());
    }
    let payload = get(request, "payload");
    let view = get(payload, "view");
    if payload.as_object().is_none() || view.as_object().is_none() {
        return Err(err("Checked identity lacks stored source view"));
    }
    let parents = get(view, "native_parents").as_array().filter(|items| {
        items
            .iter()
            .all(|item| item.as_object().is_some() && get(item, "id").as_str().is_some())
    });
    let Some(parents) = parents else {
        return Err(err("Checked identity lacks native graph"));
    };
    let native: Vec<&str> = parents
        .iter()
        .filter_map(|item| get(item, "id").as_str())
        .collect();
    let mut sorted = native.clone();
    sorted.sort_unstable();
    let unique = sorted.windows(2).all(|pair| pair.first() != pair.get(1));
    if !unique
        || !same(&strings(sorted.iter().copied()), get(known, "native_ids"))
        || !same(get(view, "leaf_id"), get(known, "leaf_id"))
    {
        return Err(err("Envelope native identities differ from stored view"));
    }
    let entries = get(payload, "entries")
        .as_array()
        .ok_or_else(|| err("Checked identity lacks source entries"))?;
    let mut tools = BTreeSet::new();
    for entry in entries {
        let message = get(entry, "message");
        if message.as_object().is_none() {
            continue;
        }
        if get(message, "role").as_str() == Some("toolResult") {
            if let Some(id) = get(message, "toolCallId").as_str() {
                tools.insert(id.to_owned());
            }
        }
        if let Some(content) = get(message, "content").as_array() {
            for block in content {
                if get(block, "type").as_str() == Some("toolCall") {
                    if let Some(id) = get(block, "id").as_str() {
                        tools.insert(id.to_owned());
                    }
                }
            }
        }
    }
    if !same(&strings(tools), get(known, "tool_ids")) {
        return Err(err("Envelope tool identities differ from stored source"));
    }
    Ok(())
}

/// Failure of a source attempt record: invalid frame metadata (refused as
/// `invalid-request`) is distinct from a filesystem or binding refusal.
#[derive(Debug)]
pub enum AttemptError {
    Invalid,
    Failed(ContractError),
}
impl From<ContractError> for AttemptError {
    fn from(error: ContractError) -> Self {
        Self::Failed(error)
    }
}
impl AttemptError {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Failed(error) if super::busy(error))
    }
}

/// One source attempt as described by its frame.
pub struct Attempt<'a> {
    pub sequence: u64,
    pub action: &'a str,
    pub ordinal: u64,
    pub disposition: &'a str,
    pub reason: Option<&'a str>,
    pub known: &'a Value,
    pub source_kind: Option<&'a str>,
    pub frame_kind: &'a str,
}

type InvalidKey = (String, String, u64, String, String, u64);

pub struct Coverage {
    authority: Arc<dyn StreamAuthority>,
    pub(crate) repo: PathBuf,
    pub(crate) session: String,
    pub(crate) instance: String,
    pub(crate) receipt: Option<Value>,
    pub(crate) binding: Value,
    pub(crate) base: PathBuf,
    pub(crate) directory: PathBuf,
    pub(crate) lock_path: PathBuf,
    live_guard: Option<PrivateLock>,
    pub(crate) frame_source: Option<SourceBarrier>,
    previous_identity: Option<Value>,
    last_checked_identity: Option<Value>,
    last_sequence: u64,
    refused_count: u64,
    last_ordinal: u64,
    invalid_attempt_retry: RefCell<Option<InvalidKey>>,
    invalid_attempt_retained: Cell<bool>,
    pub(crate) open: Value,
    pub(crate) open_digest: String,
    pub(crate) frame_proof: FrameProof,
}

impl Coverage {
    /// Publish this instance's OPEN under the source barrier, the session
    /// creation lock and its own live/stream locks.
    pub fn open(
        authority: Arc<dyn StreamAuthority>,
        repo: &Path,
        session: &str,
        instance: &str,
        lifecycle: &str,
        binding: &Value,
        receipt: Option<&Value>,
    ) -> Result<Self> {
        let _ = lifecycle;
        let base = authority.base(repo)?;
        let directory = directory(&base, session, instance)?;
        let mut pairs = vec![(
            "schema".to_owned(),
            string("agent-memory/omp-stream-open-v1"),
        )];
        pairs.extend(binding.as_object().unwrap_or_default().iter().cloned());
        pairs.push(("session".into(), string(session)));
        pairs.push(("instance_id".into(), string(instance)));
        pairs.push((
            "opened_at".into(),
            string(crate::host::time_ns()?.to_string()),
        ));
        let open = Value::Object(pairs);
        let frame_proof = authority.attest(binding, receipt)?;
        let lock_path = directory.join("stream.lock");
        let session_dir = parent(&directory)?.to_path_buf();
        let session_sha = super::name(&session_dir)?.to_owned();
        let live_guard = barrier(&*authority, repo, |_| {
            verified(
                &*authority,
                repo,
                binding,
                receipt,
                &frame_proof,
                None,
                |_| {
                    session_creation_lock(&base, &session_sha, |fresh_lock| {
                        let existed = lexists(&session_dir)?;
                        let instances = if existed {
                            session_instances(&session_dir)?.0
                        } else {
                            Vec::new()
                        };
                        turn_sources::private_mkdir(&session_dir)?;
                        if (existed && instances.is_empty()) || (!existed && !fresh_lock) {
                            retain(
                                &session_dir.join("startup-incomplete.json"),
                                &startup_marker(&session_sha),
                                4096,
                            )?;
                        }
                        turn_sources::private_mkdir(&directory)?;
                        let guard =
                            turn_sources::private_lock(&directory.join("live.lock"), true, false)?;
                        with_lock(&lock_path, false, false, || {
                            if !turn_sources::exclusive(
                                &directory.join("open.json"),
                                &open,
                                MAX_METADATA,
                            )? {
                                return Err(err("Finite stream OPEN already exists"));
                            }
                            Ok(())
                        })?;
                        Ok(guard)
                    })
                },
            )
        })?;
        let open_digest = digest(&open)?;
        Ok(Self {
            authority,
            repo: repo.to_path_buf(),
            session: session.to_owned(),
            instance: instance.to_owned(),
            receipt: receipt.cloned(),
            binding: binding.clone(),
            base,
            directory,
            lock_path,
            live_guard: Some(live_guard),
            frame_source: None,
            previous_identity: None,
            last_checked_identity: None,
            last_sequence: 0,
            refused_count: 0,
            last_ordinal: 0,
            invalid_attempt_retry: RefCell::new(None),
            invalid_attempt_retained: Cell::new(false),
            open,
            open_digest,
            frame_proof,
        })
    }

    pub fn authority(&self) -> &Arc<dyn StreamAuthority> {
        &self.authority
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn base(&self) -> &Path {
        &self.base
    }
    pub fn repo(&self) -> &Path {
        &self.repo
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    pub fn instance(&self) -> &str {
        &self.instance
    }
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
    pub fn open_value(&self) -> &Value {
        &self.open
    }
    pub fn open_digest(&self) -> &str {
        &self.open_digest
    }
    pub fn previous_identity(&self) -> Option<&Value> {
        self.previous_identity.as_ref()
    }
    pub fn last_checked_identity(&self) -> Option<&Value> {
        self.last_checked_identity.as_ref()
    }
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }
    pub fn refused_count(&self) -> u64 {
        self.refused_count
    }
    pub fn last_ordinal(&self) -> u64 {
        self.last_ordinal
    }
    pub fn invalid_attempt_pending(&self) -> bool {
        self.invalid_attempt_retry.borrow().is_some()
    }

    pub fn attest_frame(&mut self) -> Result<()> {
        self.frame_proof = self
            .authority
            .attest(&self.binding, self.receipt.as_ref())?;
        Ok(())
    }
    /// Release the lifetime owner lock: recovery may now retire this instance.
    pub fn close_live(&mut self) {
        self.live_guard = None;
    }

    /// Binding check, then the stream lock, then an unchanged OPEN.
    pub(crate) fn control<T>(&self, body: impl FnOnce() -> Result<T>) -> Result<T> {
        let result = verified(
            &*self.authority,
            &self.repo,
            &self.binding,
            self.receipt.as_ref(),
            &self.frame_proof,
            self.frame_source.as_ref(),
            |_| {
                with_lock(&self.lock_path, false, false, || {
                    let current = read_required(&self.directory.join("open.json"), MAX_METADATA)?;
                    if !same(&current, &self.open) {
                        return Err(err("Finite stream OPEN binding changed"));
                    }
                    body()
                })
            },
        );
        #[cfg(feature = "dev-tools")]
        if result.as_ref().is_err_and(super::busy) {
            crate::bank_writes::fault("omp.metadata.after-busy")?;
        }
        result
    }

    fn attempt_value(&self, attempt: &Attempt<'_>) -> Result<(Value, bool)> {
        let known = attempt.known;
        let hint = is_hint(known);
        if hint {
            hint_identity(known, Some(&self.session), Some(&self.repo))?;
            if attempt.action != "hint"
                || attempt.disposition != "accepted"
                || attempt.source_kind.is_some()
            {
                return Err(err("Hint metadata is not a full stored view"));
            }
        } else {
            identity(known)?;
            if attempt.action == "hint" && attempt.disposition == "accepted" {
                return Err(err("Neutral hint requires its closed metadata identity"));
            }
        }
        super::check_counter(attempt.ordinal)?;
        if !ACTIONS.contains(&attempt.action)
            || !matches!(attempt.disposition, "accepted" | "refused")
        {
            return Err(err("Invalid finite source attempt"));
        }
        if !matches!(
            attempt.source_kind,
            None | Some("session-manager" | "retired-file")
        ) {
            return Err(err("Unknown checked source-view kind"));
        }
        if attempt.disposition == "refused" && attempt.source_kind.is_some() {
            return Err(err("Refused projection is not a checked stored view"));
        }
        if Some(attempt.sequence) != self.last_sequence.checked_add(1)
            || attempt.ordinal < self.last_ordinal
        {
            return Err(err("Noncontiguous finite source attempts"));
        }
        if !matches!(attempt.frame_kind, "REQUEST" | "REFUSED-ATTEMPT")
            || (attempt.frame_kind == "REFUSED-ATTEMPT" && attempt.disposition != "refused")
        {
            return Err(err("Invalid source attempt wire type"));
        }
        let reason_ok = match (attempt.disposition, attempt.reason) {
            ("accepted", None) => true,
            ("refused", Some(reason)) => REFUSAL_CODES.contains(&reason),
            _ => false,
        };
        if !reason_ok {
            return Err(err("Invalid finite source refusal code"));
        }
        let inventory = if hint {
            known.clone()
        } else if matches!(known, Value::Null) {
            object([("kind", string("unresolvable"))])
        } else if let Some(previous) = self.previous_identity.as_ref() {
            let set = |value: &Value, key: &str| -> BTreeSet<String> {
                get(value, key)
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            };
            let (now_native, old_native) = (set(known, "native_ids"), set(previous, "native_ids"));
            let (now_tool, old_tool) = (set(known, "tool_ids"), set(previous, "tool_ids"));
            object([
                ("kind", string("delta")),
                ("view_id", get(known, "view_id").clone()),
                ("leaf_id", get(known, "leaf_id").clone()),
                (
                    "native_added",
                    strings(now_native.difference(&old_native).cloned()),
                ),
                (
                    "native_removed",
                    strings(old_native.difference(&now_native).cloned()),
                ),
                (
                    "tool_added",
                    strings(now_tool.difference(&old_tool).cloned()),
                ),
                (
                    "tool_removed",
                    strings(old_tool.difference(&now_tool).cloned()),
                ),
            ])
        } else {
            let mut pairs = vec![("kind".to_owned(), string("baseline"))];
            pairs.extend(known.as_object().unwrap_or_default().iter().cloned());
            Value::Object(pairs)
        };
        let value = object([
            ("schema", string("agent-memory/omp-stream-attempt-v1")),
            ("open_sha256", string(&self.open_digest)),
            ("instance_id", string(&self.instance)),
            ("sequence", super::int(attempt.sequence)),
            ("action", string(attempt.action)),
            ("event_ordinal", super::int(attempt.ordinal)),
            ("disposition", string(attempt.disposition)),
            ("reason", opt_string(attempt.reason)),
            ("identity", inventory),
            ("source_kind", opt_string(attempt.source_kind)),
            ("frame_kind", string(attempt.frame_kind)),
        ]);
        if turn_sources::encoded(&value)?.len() > MAX_IDENTITY {
            return Err(err("Finite attempt identity record exceeds bound"));
        }
        Ok((value, hint))
    }

    fn invalid_key(
        &self,
        sequence: u64,
        frame_kind: &str,
        action: &str,
        ordinal: u64,
    ) -> InvalidKey {
        (
            self.open_digest.clone(),
            self.instance.clone(),
            sequence,
            frame_kind.to_owned(),
            action.to_owned(),
            ordinal,
        )
    }

    /// Validate and durably record one source attempt; counters advance only
    /// after every exit recheck succeeded.
    pub fn attempt(&mut self, attempt: Attempt<'_>) -> std::result::Result<String, AttemptError> {
        let (value, hint) = self
            .attempt_value(&attempt)
            .map_err(|_| AttemptError::Invalid)?;
        let digest = self.control(|| {
            if lexists(&self.directory.join("retirement.json"))? {
                return Err(err("Finite stream is retired"));
            }
            let path = self
                .directory
                .join("attempts")
                .join(format!("{}.json", attempt.sequence));
            let key = self.invalid_key(
                attempt.sequence,
                attempt.frame_kind,
                attempt.action,
                attempt.ordinal,
            );
            let invalid_retry = self.invalid_attempt_retry.borrow().as_ref() == Some(&key);
            if invalid_retry && !self.invalid_attempt_retained.get() && lexists(&path)? {
                return Err(err(
                    "Original finite attempt exists; preserve its unknown outcome",
                ));
            }
            let digest = retain(&path, &value, MAX_IDENTITY)?;
            #[cfg(feature = "dev-tools")]
            crate::bank_writes::fault("omp.attempt.after-retain")?;
            if invalid_retry {
                self.invalid_attempt_retained.set(true);
            }
            Ok(digest)
        })?;
        if !hint {
            self.previous_identity =
                (!matches!(attempt.known, Value::Null)).then(|| attempt.known.clone());
        }
        if attempt.source_kind == Some("session-manager") {
            self.last_checked_identity =
                (!matches!(attempt.known, Value::Null)).then(|| attempt.known.clone());
        }
        self.last_sequence = attempt.sequence;
        self.last_ordinal = attempt.ordinal;
        self.refused_count += u64::from(attempt.frame_kind == "REFUSED-ATTEMPT");
        Ok(digest)
    }

    /// Only this live coverage instance can resume its own exact invalid
    /// metadata operation after a typed exit-recheck deferral. A serialized,
    /// accepted, partial or unrelated preexisting slot is never adopted.
    pub fn refuse_invalid(
        &mut self,
        sequence: u64,
        frame_kind: &str,
        action: &str,
        ordinal: u64,
    ) -> Result<String> {
        let key = self.invalid_key(sequence, frame_kind, action, ordinal);
        let current = self.invalid_attempt_retry.borrow().clone();
        let retry = current.as_ref() == Some(&key);
        if current.is_some() && !retry {
            return Err(err(
                "Different invalid frame cannot resume pending finite metadata",
            ));
        }
        let result = self.refuse_invalid_once(sequence, frame_kind, action, ordinal, retry, key);
        if !result.as_ref().err().is_some_and(super::busy) {
            *self.invalid_attempt_retry.borrow_mut() = None;
            self.invalid_attempt_retained.set(false);
        }
        result
    }
    fn refuse_invalid_once(
        &mut self,
        sequence: u64,
        frame_kind: &str,
        action: &str,
        ordinal: u64,
        retry: bool,
        key: InvalidKey,
    ) -> Result<String> {
        self.control(|| {
            let path = self
                .directory
                .join("attempts")
                .join(format!("{sequence}.json"));
            if lexists(&path)? && !(retry && self.invalid_attempt_retained.get()) {
                return Err(err(
                    "Original finite attempt exists; preserve its unknown outcome",
                ));
            }
            if !retry {
                *self.invalid_attempt_retry.borrow_mut() = Some(key);
            }
            Ok(())
        })?;
        // `retain` still requires exact immutable bytes on a retry. The attempt
        // advances in-memory counts only after all exit rechecks succeed.
        self.attempt(Attempt {
            sequence,
            action,
            ordinal,
            disposition: "refused",
            reason: Some("invalid-request"),
            known: &Value::Null,
            source_kind: None,
            frame_kind,
        })
        .map_err(|error| match error {
            AttemptError::Invalid => err("Invalid finite source attempt metadata"),
            AttemptError::Failed(error) => error,
        })
    }

    /// Called inside source barrier -> omp-transport lock, never acquires transport.
    pub fn started(&self, sequence: u64, attempt_digest: &str) -> Result<()> {
        self.control_without_open(|| {
            if lexists(&self.directory.join("retirement.json"))? {
                return Err(err("Finite stream retirement fence prevents execution"));
            }
            retain(
                &self
                    .directory
                    .join("started")
                    .join(format!("{sequence}.json")),
                &object([
                    ("schema", string("agent-memory/omp-stream-started-v1")),
                    ("open_sha256", string(&self.open_digest)),
                    ("attempt_sha256", string(attempt_digest)),
                    ("sequence", super::int(sequence)),
                ]),
                4096,
            )?;
            Ok(())
        })
    }
    fn control_without_open<T>(&self, body: impl FnOnce() -> Result<T>) -> Result<T> {
        verified(
            &*self.authority,
            &self.repo,
            &self.binding,
            self.receipt.as_ref(),
            &self.frame_proof,
            self.frame_source.as_ref(),
            |_| with_lock(&self.lock_path, false, false, body),
        )
    }

    pub fn outcome(
        &self,
        sequence: u64,
        attempt_digest: &str,
        outcome: &str,
        code: &str,
        references: &[Value],
    ) -> Result<()> {
        if !matches!(outcome, "verified" | "durable-refusal" | "unknown") {
            return Err(err("Invalid finite stream outcome"));
        }
        let value = object([
            ("schema", string("agent-memory/omp-stream-outcome-v1")),
            ("open_sha256", string(&self.open_digest)),
            ("attempt_sha256", string(attempt_digest)),
            ("sequence", super::int(sequence)),
            ("outcome", string(outcome)),
            ("code", string(code)),
            (
                "source_references",
                Value::Array(source_refs(&Value::Array(references.to_vec()))?),
            ),
        ]);
        #[cfg(feature = "dev-tools")]
        crate::bank_writes::fault("omp.outcome.before-guard")?;
        self.control(|| {
            #[cfg(feature = "dev-tools")]
            crate::bank_writes::fault("omp.outcome.before-commit")?;
            retain(
                &self
                    .directory
                    .join("outcomes")
                    .join(format!("{sequence}.json")),
                &value,
                MAX_METADATA,
            )?;
            #[cfg(feature = "dev-tools")]
            crate::bank_writes::fault("omp.outcome.after-commit")?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn transition(&self, value: &Value) -> Result<()> {
        exact(
            value,
            &[
                "transition_id",
                "target_session",
                "target_instance",
                "last_view",
                "reason",
            ],
        )?;
        uuid(get(value, "transition_id"))?;
        uuid(get(value, "target_instance"))?;
        turn_sources::bounded(get(value, "target_session"), 512)?;
        if (get(value, "target_session").as_str() == Some(&self.session)
            && get(value, "target_instance").as_str() == Some(&self.instance))
            || get(value, "reason").as_str() != Some("transition-final-view-unavailable")
        {
            return Err(err("Invalid committed source transition"));
        }
        identity(get(value, "last_view"))?;
        if !super::same_opt(
            Some(get(value, "last_view")),
            self.last_checked_identity.as_ref(),
        ) {
            return Err(err("Transition does not bind last checked old view"));
        }
        let mut pairs = vec![
            (
                "schema".to_owned(),
                string("agent-memory/omp-transition-tail-v1"),
            ),
            ("open_sha256".into(), string(&self.open_digest)),
            ("project_id".into(), get(&self.open, "project_id").clone()),
            (
                "checkout_key".into(),
                get(&self.open, "checkout_key").clone(),
            ),
            ("session".into(), string(&self.session)),
            ("instance_id".into(), string(&self.instance)),
        ];
        pairs.extend(value.as_object().unwrap_or_default().iter().cloned());
        let record = Value::Object(pairs);
        let id = get(value, "transition_id").as_str().unwrap_or_default();
        self.control(|| {
            retain(
                &self
                    .directory
                    .join("transitions")
                    .join(format!("{id}.json")),
                &record,
                MAX_METADATA,
            )
            .map(|_| ())
        })
    }

    /// FINAL: account every attempt, bind an optional transition, and close
    /// only when no attempt outcome is missing or unknown.
    pub fn close(&self, last: &Value) -> Result<bool> {
        exact(
            last,
            &[
                "last_attempt_seq",
                "refused_count",
                "last_event_ordinal",
                "transition",
            ],
        )?;
        let counts = (
            integer(get(last, "last_attempt_seq"))?,
            integer(get(last, "refused_count"))?,
            integer(get(last, "last_event_ordinal"))?,
        );
        if counts != (self.last_sequence, self.refused_count, self.last_ordinal) {
            return Err(err("FINAL does not account all source attempts"));
        }
        let transition = get(last, "transition");
        if !matches!(transition, Value::Null) {
            self.transition(transition)?;
        }
        self.control(|| {
            let (row, pins) = super::inventory::inspect(&self.directory, &self.open, false)?;
            let pins = super::inventory::close_pins(&pins);
            let unknown = row.pending.iter().any(|item| {
                matches!(
                    get(item, "code").as_str(),
                    Some(
                        "missing-outcome"
                            | "unknown-after-start"
                            | "busy-exhausted"
                            | "unknown-outcome"
                    )
                )
            });
            if unknown {
                return Ok(false);
            }
            retain(
                &self.directory.join("closed.json"),
                &object([
                    ("schema", string("agent-memory/omp-stream-close-v1")),
                    ("open_sha256", string(&self.open_digest)),
                    ("last_attempt_seq", super::int(self.last_sequence)),
                    ("refused_count", super::int(self.refused_count)),
                    ("last_event_ordinal", super::int(self.last_ordinal)),
                    ("inventory_sha256", string(digest(&pins)?)),
                ]),
                4096,
            )?;
            #[cfg(feature = "dev-tools")]
            crate::bank_writes::fault("omp.close.after-retain")?;
            Ok(true)
        })
    }
}
