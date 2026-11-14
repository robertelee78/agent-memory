//! The serial finite source executor and its one-attempt source completion.
use super::clock_window::{ClockWindows, DeliveryWindow, LateNativeDeliveryWindow};
use super::coverage::{
    Attempt, AttemptError, Coverage, FrameProof, OmpTransport, check_scope, request_identity,
    returned_refs,
};
use super::frame::{Responder, Sent};
use super::run::Timing;
use super::{
    ACTIONS, MAX_METADATA, MAX_RETAINED, busy, err, exact, hint_identity, identity, integer,
    lexists, read_required, same, string, uuid,
};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::get;
use crate::turn_sources::{self, SourceBarrier};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How an observer call failed. A contract refusal before start is a durable
/// observer refusal; any other failure leaves the outcome unknown.
#[derive(Debug)]
pub enum ObserveError {
    Contract(ContractError),
    Unexpected(String),
}
impl From<ContractError> for ObserveError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
impl ObserveError {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Contract(error) if busy(error))
    }
}

/// Everything an observer receives besides the repository and request.
pub struct ObserveCall<'a, 'c> {
    pub receipt: Option<&'a Value>,
    /// Durably record the start; must run before any source write.
    pub on_started: &'a mut dyn FnMut() -> Result<()>,
    pub instance_id: &'a str,
    pub identity: &'a Value,
    pub delivery_window: Option<DeliveryWindow<'a>>,
    pub verified_binding: &'a FrameProof,
    pub source_completion: Option<&'a SourceCompletion<'c>>,
    pub held_source: Option<&'a SourceBarrier>,
}

/// An injected observer (the baseline `observer=`); `None` uses the transport.
pub type Observer = dyn Fn(&Path, &Value, ObserveCall<'_, '_>) -> std::result::Result<Value, ObserveError>
    + Send
    + Sync;

/// One finite attempt's historical source result, not reusable authority.
pub struct SourceCompletion<'c> {
    coverage: &'c Coverage,
    timing: &'c Timing,
    sequence: u64,
    digest: String,
    request: &'c Value,
    window: LateNativeDeliveryWindow,
    started: Cell<bool>,
    attempted: Cell<bool>,
    source_committed: Cell<bool>,
    source_bytes: RefCell<Option<Vec<u8>>>,
    live: Cell<bool>,
    offered_root_wake_path: RefCell<Option<PathBuf>>,
}
impl<'c> SourceCompletion<'c> {
    pub fn window(&self) -> &LateNativeDeliveryWindow {
        &self.window
    }
    pub fn attempted(&self) -> bool {
        self.attempted.get()
    }
    pub fn source_committed(&self) -> bool {
        self.source_committed.get()
    }
    pub fn live(&self) -> bool {
        self.live.get()
    }
    pub fn offered_root_wake_path(&self) -> Option<PathBuf> {
        self.offered_root_wake_path.borrow().clone()
    }
    /// Recorded by the transport after it retained an offered root wake.
    pub fn offer_root_wake(&self, path: &Path) {
        *self.offered_root_wake_path.borrow_mut() = Some(path.to_path_buf());
    }
    pub fn validate(&self, value: &Value, view: &Value) -> Result<()> {
        if !self.live.get()
            || !std::ptr::eq(value, self.request)
            || !self.started.get()
            || get(value, "action").as_str() != Some("seal")
            || get(get(value, "payload"), "request_wake") != &Value::Bool(true)
            || get(view, "source_kind").as_str() != Some("session-manager")
        {
            return Err(err(
                "Source completion requires the exact armed live finite seal",
            ));
        }
        Ok(())
    }
    pub fn source_result(&self) -> Result<Value> {
        let raw = self.source_bytes.borrow();
        let raw = raw
            .as_deref()
            .ok_or_else(|| err("Source result has not completed"))?;
        crate::developer_identity::load_with_limit(raw, MAX_RETAINED)
    }
    pub fn commit_source(&self, _context: &Value, result: &Value) -> Result<()> {
        if !self.live.get()
            || !self.started.get()
            || self.attempted.get()
            || result.get("verified") != Some(&Value::Bool(true))
        {
            return Err(err("Invalid finite source completion"));
        }
        let raw = turn_sources::encoded(result)?;
        if raw.len() > MAX_RETAINED {
            return Err(err(
                "Finite source completion exceeds retained-memory bound",
            ));
        }
        *self.source_bytes.borrow_mut() = Some(raw);
        self.attempted.set(true);
        crate::scope_binding::fresh_enrollment_locations(|| self.write_outcome())?;
        self.source_committed.set(true);
        Ok(())
    }
    fn write_outcome(&self) -> Result<()> {
        let refs = returned_refs(&self.source_result()?);
        self.coverage
            .outcome(self.sequence, &self.digest, "verified", "verified", &refs)
    }
    /// Only called after the observer and transport have completely unwound.
    pub fn retry_busy_outcome(&self) -> Result<()> {
        metadata(self.timing, || self.write_outcome())?;
        self.source_committed.set(true);
        Ok(())
    }
    pub fn before_reserve(&self, context: &Value) -> Result<()> {
        let coverage = self.coverage;
        if !self.live.get() || !self.source_committed.get() {
            return Err(err("Wake requires a committed finite source"));
        }
        #[cfg(feature = "dev-tools")]
        crate::bank_writes::fault("omp.before-reserve.before-open")?;
        super::with_lock(&coverage.lock_path, false, false, || {
            let current = read_required(&coverage.directory.join("open.json"), MAX_METADATA)?;
            if !same(&current, &coverage.open)
                || lexists(&coverage.directory.join("retirement.json"))?
                || check_scope(context, &coverage.binding).is_err()
            {
                return Err(err("Finite stream binding changed before reservation"));
            }
            Ok(())
        })
    }
    pub fn wake_failure(&self, error: &ObserveError) -> Result<Value> {
        let mut result = self.source_result()?;
        let status = if self.window.reservation_started() {
            "pending-wake-unknown-after-reservation"
        } else if error.busy() {
            "pending-wake-busy"
        } else {
            "pending-wake-unavailable"
        };
        set(&mut result, "continue", Value::Bool(false));
        set(&mut result, "status", string(status));
        Ok(result)
    }
    pub fn close(&self) {
        self.live.set(false);
        self.window.close();
    }
}
fn set(value: &mut Value, key: &str, item: Value) {
    if let Value::Object(pairs) = value {
        match pairs.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = item,
            None => pairs.push((key.to_owned(), item)),
        }
    }
}

/// Retry only lock acquisition: every private/source lock has unwound before the wait.
pub(crate) fn metadata<T, E: BusyError>(
    timing: &Timing,
    mut function: impl FnMut() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    let mut deadline = None;
    loop {
        match function() {
            Err(error) if error.is_busy() => {
                let deadline = *deadline.get_or_insert_with(|| {
                    timing
                        .now_ns()
                        .saturating_add(super::BUSY_SECONDS * 1_000_000_000)
                });
                let now = timing.now_ns();
                if now >= deadline {
                    return Err(error);
                }
                let remaining = std::time::Duration::from_nanos(deadline - now);
                timing.sleep(super::BUSY_BACKOFF.min(remaining));
            }
            result => return result,
        }
    }
}
pub(crate) trait BusyError {
    fn is_busy(&self) -> bool;
}
impl BusyError for ContractError {
    fn is_busy(&self) -> bool {
        busy(self)
    }
}
impl BusyError for AttemptError {
    fn is_busy(&self) -> bool {
        self.busy()
    }
}

/// Parsed source frame (the baseline `_execute` try block).
struct Parsed {
    value: Value,
    action: String,
    disposition: &'static str,
    reason: Option<String>,
    known: Value,
    source_kind: Option<String>,
    ordinal: u64,
}

pub struct FiniteExecutor {
    writer: Arc<dyn Responder>,
    clocks: ClockWindows,
    receipt: Option<Value>,
    observer: Option<Arc<Observer>>,
    timing: Timing,
}
impl FiniteExecutor {
    pub fn new(
        writer: Arc<dyn Responder>,
        clocks: ClockWindows,
        receipt: Option<Value>,
        observer: Option<Arc<Observer>>,
        timing: Timing,
    ) -> Self {
        Self {
            writer,
            clocks,
            receipt,
            observer,
            timing,
        }
    }

    fn refuse_invalid(
        &self,
        coverage: &mut Coverage,
        kind: &str,
        sequence: u64,
        value: Option<&Value>,
    ) -> Result<()> {
        let mut action = "hint".to_owned();
        let mut ordinal = coverage.last_ordinal();
        if let Some(value) = value.filter(|v| v.as_object().is_some()) {
            let candidate = if kind == "REQUEST" {
                get(value, "request")
            } else {
                value
            };
            if let Some(found) = get(candidate, "action").as_str() {
                if ACTIONS.contains(&found) {
                    action = found.to_owned();
                }
            }
            if let Ok(candidate) = integer(get(value, "event_ordinal")) {
                ordinal = ordinal.max(candidate);
            }
        }
        let digest = metadata(&self.timing, || {
            coverage.refuse_invalid(sequence, kind, &action, ordinal)
        })?;
        metadata(&self.timing, || {
            coverage.outcome(sequence, &digest, "durable-refusal", "invalid-request", &[])
        })?;
        self.writer.send(
            "REFUSED",
            sequence,
            vec![
                ("code".into(), string("invalid-request")),
                ("outcome".into(), string("durable-refusal")),
            ],
        );
        Ok(())
    }

    /// Execute one source frame under the source barrier. Retry only its
    /// acquisition: once entered, the finite frame is never replayed.
    pub fn execute(
        &self,
        coverage: &mut Coverage,
        kind: &str,
        sequence: u64,
        nonce: Option<&str>,
        raw: &[u8],
    ) -> Result<()> {
        let authority = Arc::clone(coverage.authority());
        let repo = coverage.repo.clone();
        let held = metadata(&self.timing, || authority.source_barrier(&repo))?;
        if coverage.frame_source.is_some() {
            return Err(err(
                "Finite source frame already owns a source barrier observation",
            ));
        }
        coverage.frame_source = Some(held);
        let result = self.execute_held(coverage, kind, sequence, nonce, raw);
        let held = coverage.frame_source.take();
        result?;
        match held {
            Some(held) => held.recheck(),
            None => Err(err("Finite source frame lost its source barrier")),
        }
    }

    fn parse(
        &self,
        coverage: &Coverage,
        kind: &str,
        nonce: Option<&str>,
        value: Value,
    ) -> Result<Parsed> {
        if kind == "REFUSED-ATTEMPT" {
            if nonce.is_some() {
                return Err(err("Refused source attempt cannot carry wake authority"));
            }
            exact(&value, &["action", "event_ordinal", "reason", "identity"])?;
            let known = get(&value, "identity").clone();
            identity(&known)?;
            let ordinal = integer(get(&value, "event_ordinal"))?.max(coverage.last_ordinal());
            return Ok(Parsed {
                action: get(&value, "action")
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                disposition: "refused",
                reason: Some(
                    get(&value, "reason")
                        .as_str()
                        .ok_or_else(|| err("Invalid finite source refusal code"))?
                        .to_owned(),
                ),
                known,
                source_kind: None,
                ordinal,
                value,
            });
        }
        exact(&value, &["request", "event_ordinal", "identity"])?;
        let request = get(&value, "request");
        exact(
            request,
            &["schema", "action", "session", "lifecycle", "payload"],
        )?;
        if get(request, "schema").as_str() != Some("agent-memory/omp-observer-request-v2")
            || get(request, "session").as_str() != Some(coverage.session())
        {
            return Err(err("Foreign finite observer request"));
        }
        uuid(get(request, "lifecycle"))?;
        let action = get(request, "action").as_str().unwrap_or_default();
        if !ACTIONS.contains(&action) || (nonce.is_some() && action != "seal") {
            return Err(err("Invalid finite observer action/window"));
        }
        let known = get(&value, "identity").clone();
        let payload = get(request, "payload");
        let source_kind = if action == "hint" {
            hint_identity(&known, Some(coverage.session()), Some(coverage.repo()))?;
            exact(payload, &["session", "header_id", "cwd", "leaf_id"])?;
            let expected = Value::Object(
                known
                    .as_object()
                    .unwrap_or_default()
                    .iter()
                    .filter(|(k, _)| k != "kind")
                    .cloned()
                    .collect(),
            );
            if !same(payload, &expected) {
                return Err(err("Neutral hint payload differs from its identity"));
            }
            None
        } else {
            request_identity(request, &known)?;
            #[cfg(feature = "dev-tools")]
            crate::bank_writes::fault("omp.request-identity.after-validation")?;
            // A non-text kind is carried as an unknown kind: the attempt
            // validation then refuses it as invalid-request.
            match get(get(payload, "view"), "source_kind") {
                Value::Null => None,
                Value::String(kind) => Some(kind.clone()),
                _ => Some(String::new()),
            }
        };
        let ordinal = integer(get(&value, "event_ordinal"))?.max(coverage.last_ordinal());
        Ok(Parsed {
            action: action.to_owned(),
            disposition: "accepted",
            reason: None,
            known,
            source_kind,
            ordinal,
            value,
        })
    }

    fn execute_held(
        &self,
        coverage: &mut Coverage,
        kind: &str,
        sequence: u64,
        nonce: Option<&str>,
        raw: &[u8],
    ) -> Result<()> {
        if !matches!(kind, "REQUEST" | "REFUSED-ATTEMPT") {
            return Err(err("Invalid source frame type"));
        }
        let decoded = parse_utf8(raw);
        let value = decoded.as_ref().ok().cloned();
        let parsed = decoded.and_then(|value| self.parse(coverage, kind, nonce, value));
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(_) => {
                coverage.attest_frame()?;
                return self.refuse_invalid(coverage, kind, sequence, value.as_ref());
            }
        };
        if parsed.action != "hint"
            || (parsed.disposition == "refused" && !matches!(parsed.known, Value::Null))
        {
            coverage.attest_frame()?;
        }
        let attempt = metadata(&self.timing, || {
            coverage.attempt(Attempt {
                sequence,
                action: &parsed.action,
                ordinal: parsed.ordinal,
                disposition: parsed.disposition,
                reason: parsed.reason.as_deref(),
                known: &parsed.known,
                source_kind: parsed.source_kind.as_deref(),
                frame_kind: kind,
            })
        });
        let digest = match attempt {
            Ok(digest) => digest,
            Err(AttemptError::Invalid) => {
                return self.refuse_invalid(coverage, kind, sequence, Some(&parsed.value));
            }
            Err(AttemptError::Failed(error)) => return Err(error),
        };
        let coverage: &Coverage = coverage;
        if parsed.disposition == "refused" {
            let reason = parsed.reason.as_deref().unwrap_or_default();
            metadata(&self.timing, || {
                coverage.outcome(sequence, &digest, "durable-refusal", reason, &[])
            })?;
            self.writer.send(
                "REFUSED",
                sequence,
                vec![
                    ("code".into(), string(reason)),
                    ("outcome".into(), string("durable-refusal")),
                ],
            );
            return Ok(());
        }
        if parsed.action == "hint" {
            metadata(&self.timing, || {
                coverage.outcome(sequence, &digest, "verified", "hint-recorded", &[])
            })?;
            self.writer.send(
                "OBSERVED",
                sequence,
                vec![(
                    "result".into(),
                    super::object([
                        ("verified", Value::Bool(true)),
                        ("status", string("hint-recorded")),
                        ("canonical_writes_performed", Value::Bool(false)),
                    ]),
                )],
            );
            return Ok(());
        }
        self.writer.send("RECEIVED", sequence, Vec::new());
        self.observe_source(coverage, sequence, nonce, &digest, &parsed)
    }

    fn observe_source(
        &self,
        coverage: &Coverage,
        sequence: u64,
        nonce: Option<&str>,
        digest: &str,
        parsed: &Parsed,
    ) -> Result<()> {
        let request = get(&parsed.value, "request");
        let source_kind = parsed.source_kind.as_deref();
        let armed = parsed.action == "seal"
            && source_kind == Some("session-manager")
            && get(get(request, "payload"), "request_wake") == &Value::Bool(true);
        let completion = match nonce.filter(|_| armed) {
            Some(nonce) => Some(SourceCompletion {
                coverage,
                timing: &self.timing,
                sequence,
                digest: digest.to_owned(),
                request,
                window: LateNativeDeliveryWindow::new(self.clocks.clone(), nonce)?,
                started: Cell::new(false),
                attempted: Cell::new(false),
                source_committed: Cell::new(false),
                source_bytes: RefCell::new(None),
                live: Cell::new(true),
                offered_root_wake_path: RefCell::new(None),
            }),
            None => None,
        };
        let result = self.observe_completion(
            coverage,
            sequence,
            nonce,
            digest,
            parsed,
            completion.as_ref(),
        );
        if let Some(completion) = completion.as_ref() {
            completion.close();
        }
        result
    }

    fn observe_completion(
        &self,
        coverage: &Coverage,
        sequence: u64,
        nonce: Option<&str>,
        digest: &str,
        parsed: &Parsed,
        completion: Option<&SourceCompletion<'_>>,
    ) -> Result<()> {
        let request = get(&parsed.value, "request");
        let started = Cell::new(false);
        let mut on_started = || -> Result<()> {
            if started.get() {
                return Err(err("Finite source attempt started twice"));
            }
            coverage.started(sequence, digest)?;
            started.set(true);
            if let Some(completion) = completion {
                completion.started.set(true);
            }
            Ok(())
        };
        let identity_value = get(&parsed.value, "identity");
        identity(identity_value)?;
        let transport: Option<Arc<dyn OmpTransport>> = match self.observer {
            Some(_) => None,
            None => Some(coverage.authority().transport()?),
        };
        let deadline = self
            .timing
            .now_ns()
            .saturating_add(super::BUSY_SECONDS * 1_000_000_000);
        let (outcome, code, result) = loop {
            let window = match (completion, nonce) {
                (Some(completion), _) => Some(DeliveryWindow::Late(completion.window())),
                (None, Some(nonce)) => self.clocks.window(nonce).map(DeliveryWindow::Verified),
                (None, None) => None,
            };
            let call = ObserveCall {
                receipt: self.receipt.as_ref(),
                on_started: &mut on_started,
                instance_id: coverage.instance(),
                identity: identity_value,
                delivery_window: window,
                verified_binding: &coverage.frame_proof,
                source_completion: completion,
                held_source: coverage.frame_source.as_ref(),
            };
            let observed = match (&self.observer, &transport) {
                (Some(observer), _) => observer(coverage.repo(), request, call),
                (None, Some(transport)) => transport.observe(coverage.repo(), request, call),
                (None, None) => Err(ObserveError::Contract(err(
                    "OmpTransport provider is not registered; finite sources remain pending",
                ))),
            };
            match observed {
                Ok(result) => {
                    let verified = result.get("verified") == Some(&Value::Bool(true));
                    break if verified && started.get() {
                        ("verified", "verified", Some(result))
                    } else if verified {
                        ("unknown", "unknown-outcome", Some(result))
                    } else if started.get() {
                        ("unknown", "unknown-after-start", Some(result))
                    } else {
                        ("durable-refusal", "observer-refused", Some(result))
                    };
                }
                Err(error) => {
                    #[cfg(feature = "dev-tools")]
                    if error.busy() {
                        crate::bank_writes::fault("omp.observer.after-busy-error")?;
                    } else {
                        crate::bank_writes::fault("omp.observer.after-error")?;
                    }
                    if let Some(completion) = completion.filter(|c| c.source_committed()) {
                        break (
                            "verified",
                            "verified",
                            Some(completion.wake_failure(&error)?),
                        );
                    }
                    if let Some(completion) = completion.filter(|c| c.attempted()) {
                        if !error.busy() {
                            return Err(match error {
                                ObserveError::Contract(error) => error,
                                ObserveError::Unexpected(message) => err(&message),
                            });
                        }
                        completion.retry_busy_outcome()?;
                        break (
                            "verified",
                            "verified",
                            Some(completion.wake_failure(&error)?),
                        );
                    }
                    if error.busy() {
                        if started.get() {
                            break ("unknown", "unknown-after-start", None);
                        }
                        let now = self.timing.now_ns();
                        if now >= deadline {
                            break ("unknown", "busy-exhausted", None);
                        }
                        let remaining = std::time::Duration::from_nanos(deadline - now);
                        self.timing.sleep(super::BUSY_BACKOFF.min(remaining));
                        continue;
                    }
                    break match error {
                        ObserveError::Contract(_) if started.get() => {
                            ("unknown", "unknown-after-start", None)
                        }
                        ObserveError::Contract(_) => ("durable-refusal", "observer-refused", None),
                        ObserveError::Unexpected(_) if started.get() => {
                            ("unknown", "unknown-after-start", None)
                        }
                        ObserveError::Unexpected(_) => ("unknown", "unknown-outcome", None),
                    };
                }
            }
        };
        let finished = self.respond(
            coverage, sequence, digest, parsed, completion, &started, outcome, code, result,
        );
        if completion.is_some_and(SourceCompletion::source_committed) {
            let transport = match transport {
                Some(transport) => transport,
                None => coverage.authority().transport()?,
            };
            transport.drain_after_response(
                coverage.repo(),
                self.receipt.as_ref(),
                &coverage.frame_proof,
            )?;
        }
        finished
    }

    #[allow(clippy::too_many_arguments)]
    fn respond(
        &self,
        coverage: &Coverage,
        sequence: u64,
        digest: &str,
        parsed: &Parsed,
        completion: Option<&SourceCompletion<'_>>,
        started: &Cell<bool>,
        outcome: &str,
        code: &str,
        result: Option<Value>,
    ) -> Result<()> {
        // Never overwrite a possibly retained source outcome after a failed exit.
        if !completion.is_some_and(SourceCompletion::source_committed) {
            if completion.is_some_and(SourceCompletion::attempted) {
                return Err(err("Finite source completion did not commit"));
            }
            let refs = match (outcome, result.as_ref()) {
                ("verified", Some(result)) => returned_refs(result),
                _ => Vec::new(),
            };
            metadata(&self.timing, || {
                coverage.outcome(sequence, digest, outcome, code, &refs)
            })?;
        }
        if outcome != "verified" {
            self.writer.send(
                "OBSERVED",
                sequence,
                vec![
                    ("outcome".into(), string(outcome)),
                    ("code".into(), string(code)),
                ],
            );
            return Ok(());
        }
        let result = result.unwrap_or(Value::Null);
        let sent = self.writer.send(
            "OBSERVED",
            sequence,
            vec![("result".into(), result.clone())],
        );
        if sent == Sent::Queued
            && started.get()
            && parsed.action == "seal"
            && parsed.source_kind.as_deref() == Some("session-manager")
        {
            // Optional evidence cannot change the committed source or retry its wake.
            let request = get(&parsed.value, "request");
            let _ = super::wake::retain_wake_decision(
                coverage, sequence, digest, request, &result, completion,
            );
        }
        Ok(())
    }
}

pub(crate) fn parse_utf8(raw: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(raw).map_err(|_| err("Invalid UTF-8 finite stream frame"))?;
    crate::developer_identity::load_with_limit(text.as_bytes(), super::MAX_FRAME)
}
