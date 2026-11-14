//! Finite FIFO loss/phase/coverage boundaries; no native-runtime qualification.
//!
//! The baseline contracts replace `_base`, `source_barrier`, `_verify_binding`,
//! `_attest_binding` and `omp_transport` with in-process controls. Here the
//! same points are one explicit `StreamAuthority`: a real descriptor-level
//! source barrier at a private base, an attestation counter, a binding check
//! that can refuse or defer, and a transport whose disposition set the test
//! controls. Records, locks and inventory are the real implementation.
use agent_memory::clock::monotonic_ns;
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::error::{ContractError, Kind, Result};
use agent_memory::omp_stream::{
    self as stream, Attempt, Budget, ClockWindows, Coverage, FiniteExecutor, FrameProof,
    FrameReader, ImageProbe, ImageStat, LinuxNativeImage, NativeImage, ObserveCall, ObserveError,
    Observer, OmpTransport, Output, Platform, RecoveryEvidence, Responder, ResponseWriter,
    RunOptions, Sent, StreamAuthority, TimeNamespace, Timing,
};
use agent_memory::turn_sources::{self, SourceBarrier};
use std::collections::{BTreeSet, VecDeque};
use std::io::{Cursor, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------- helpers

fn uid() -> String {
    agent_memory::host::uuid4().unwrap()
}
fn json(text: &str) -> Value {
    json::decode(text, DecodePolicy::default()).unwrap()
}
fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap_or_default()
}
fn set(value: &mut Value, key: &str, item: Value) {
    if let Some(existing) = value.get_mut(key) {
        *existing = item;
    } else if let Value::Object(pairs) = value {
        pairs.push((key.to_owned(), item));
    }
}
fn remove(value: &mut Value, key: &str) {
    if let Value::Object(pairs) = value {
        pairs.retain(|(k, _)| k != key);
    }
}
fn encoded(value: &Value) -> Vec<u8> {
    turn_sources::encoded(value).unwrap()
}
fn digest(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(encoded(value))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn int(value: &Value) -> u64 {
    value.as_int().and_then(|v| v.as_u64()).unwrap()
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}
fn read(path: &Path) -> Value {
    turn_sources::read(path, stream::MAX_METADATA)
        .unwrap()
        .unwrap()
}
fn codes(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|item| text(item, "code").to_owned())
        .collect()
}
fn busy(error: &ContractError) -> bool {
    matches!(error.kind(), Kind::SourceTransportBusy { .. })
}

/// A genuine typed busy refusal from real kernel lock contention.
fn busy_error() -> ContractError {
    static ERROR: Mutex<Option<ContractError>> = Mutex::new(None);
    let mut cached = ERROR.lock().unwrap();
    if let Some(error) = cached.as_ref() {
        return error.clone();
    }
    let world = World::new();
    let path = world.path().join("contended.lock");
    let _held = turn_sources::private_lock(&path, false, false).unwrap();
    let error = turn_sources::private_lock(&path, false, false).unwrap_err();
    assert!(busy(&error));
    *cached = Some(error.clone());
    error
}

struct World(PathBuf);
impl World {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("am-omp-stream-{}", uid()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(agent_memory::fs::canonical_path(&path, false).unwrap())
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn private_dir(path: &Path) {
    std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}
fn write_private(path: &Path, raw: &[u8]) {
    std::fs::write(path, raw).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

// ------------------------------------------------------------- authority

enum Dispositions {
    Unconfigured,
    Set(BTreeSet<String>),
    Fail(String),
}
struct TestTransport {
    dispositions: Mutex<Dispositions>,
    disposition_calls: AtomicUsize,
    observer: Mutex<Option<Arc<Observer>>>,
    drains: AtomicUsize,
}
impl OmpTransport for TestTransport {
    fn observe(
        &self,
        repo: &Path,
        request: &Value,
        call: ObserveCall<'_, '_>,
    ) -> std::result::Result<Value, ObserveError> {
        let observer = self.observer.lock().unwrap().clone();
        match observer {
            Some(observer) => observer(repo, request, call),
            None => Err(ObserveError::Unexpected("no test observer".into())),
        }
    }
    fn drain_after_response(
        &self,
        _repo: &Path,
        _receipt: Option<&Value>,
        _proof: &FrameProof,
    ) -> Result<()> {
        self.drains.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn native_disposition_ids(&self, _repo: &Path, _session: &str) -> Result<BTreeSet<String>> {
        self.disposition_calls.fetch_add(1, Ordering::SeqCst);
        match &*self.dispositions.lock().unwrap() {
            Dispositions::Unconfigured => Err(ContractError::new("unexpected disposition join")),
            Dispositions::Set(values) => Ok(values.clone()),
            Dispositions::Fail(message) => Err(ContractError::new(message.clone())),
        }
    }
}

struct TestAuthority {
    base: PathBuf,
    attests: AtomicUsize,
    verifies: AtomicUsize,
    refusal: Mutex<Option<String>>,
    busy_entries: Mutex<Vec<usize>>,
    busy_exits: Mutex<Vec<usize>>,
    transport: Arc<TestTransport>,
}
impl TestAuthority {
    fn new(base: &Path) -> Arc<Self> {
        Arc::new(Self {
            base: base.to_path_buf(),
            attests: AtomicUsize::new(0),
            verifies: AtomicUsize::new(0),
            refusal: Mutex::new(None),
            busy_entries: Mutex::new(Vec::new()),
            busy_exits: Mutex::new(Vec::new()),
            transport: Arc::new(TestTransport {
                dispositions: Mutex::new(Dispositions::Unconfigured),
                disposition_calls: AtomicUsize::new(0),
                observer: Mutex::new(None),
                drains: AtomicUsize::new(0),
            }),
        })
    }
    fn attests(&self) -> usize {
        self.attests.load(Ordering::SeqCst)
    }
    fn verifies(&self) -> usize {
        self.verifies.load(Ordering::SeqCst)
    }
    fn dispositions(&self, value: Dispositions) {
        *self.transport.dispositions.lock().unwrap() = value;
    }
    fn disposition_calls(&self) -> usize {
        self.transport.disposition_calls.load(Ordering::SeqCst)
    }
    fn observe_with(&self, observer: Arc<Observer>) {
        *self.transport.observer.lock().unwrap() = Some(observer);
    }
}
impl StreamAuthority for TestAuthority {
    fn base(&self, _repo: &Path) -> Result<PathBuf> {
        Ok(self.base.clone())
    }
    fn source_barrier(&self, _repo: &Path) -> Result<SourceBarrier> {
        turn_sources::source_barrier_at(&self.base, false, false)
    }
    fn observation_barrier(
        &self,
        _repo: &Path,
        body: &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value> {
        body()
    }
    fn attest(&self, _binding: &Value, _receipt: Option<&Value>) -> Result<FrameProof> {
        self.attests.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(()))
    }
    fn verify(
        &self,
        _repo: &Path,
        binding: &Value,
        _receipt: Option<&Value>,
        _proof: &FrameProof,
        _held: Option<&SourceBarrier>,
        body: &mut dyn FnMut(&Value) -> Result<()>,
    ) -> Result<()> {
        let call = self.verifies.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(message) = self.refusal.lock().unwrap().clone() {
            return Err(ContractError::new(message));
        }
        if self.busy_entries.lock().unwrap().contains(&call) {
            return Err(busy_error());
        }
        let context = json(&format!(
            r#"{{"root":{},"bank":{},"project_id":{},"scope":{{"checkout_key":{}}},"held":false}}"#,
            String::from_utf8(encoded(get(binding, "repo_root"))).unwrap(),
            String::from_utf8(encoded(get(binding, "bank_path"))).unwrap(),
            String::from_utf8(encoded(get(binding, "project_id"))).unwrap(),
            String::from_utf8(encoded(get(binding, "checkout_key"))).unwrap(),
        ));
        body(&context)?;
        if self.busy_exits.lock().unwrap().contains(&call) {
            return Err(busy_error());
        }
        Ok(())
    }
    fn transport(&self) -> Result<Arc<dyn OmpTransport>> {
        Ok(self.transport.clone())
    }
}

// ------------------------------------------------------------- responses

#[derive(Default)]
struct Responses {
    dead: AtomicBool,
    reject_observed: AtomicBool,
    values: Mutex<Vec<(String, u64, Value)>>,
}
impl Responses {
    fn values(&self) -> Vec<(String, u64, Value)> {
        self.values.lock().unwrap().clone()
    }
    fn kinds(&self) -> Vec<String> {
        self.values().into_iter().map(|(k, _, _)| k).collect()
    }
}
impl Responder for Responses {
    fn send(&self, kind: &str, sequence: u64, value: Vec<(String, Value)>) -> Sent {
        self.values
            .lock()
            .unwrap()
            .push((kind.to_owned(), sequence, Value::Object(value)));
        if self.dead.load(Ordering::SeqCst)
            || (kind == "OBSERVED" && self.reject_observed.load(Ordering::SeqCst))
        {
            Sent::Dropped
        } else {
            Sent::Queued
        }
    }
    fn live(&self) -> bool {
        !self.dead.load(Ordering::SeqCst)
    }
    fn finish(&self) {}
}

/// Scripted monotonic time; the last value repeats. Sleeps are recorded only.
struct Script {
    values: Mutex<VecDeque<u64>>,
    fixed: AtomicU64,
    sleeps: Mutex<Vec<Duration>>,
}
impl Script {
    fn new(values: &[u64]) -> Arc<Self> {
        Arc::new(Self {
            values: Mutex::new(values.iter().copied().collect()),
            fixed: AtomicU64::new(values.last().copied().unwrap_or(0)),
            sleeps: Mutex::new(Vec::new()),
        })
    }
    fn now(&self) -> u64 {
        match self.values.lock().unwrap().pop_front() {
            Some(value) => {
                self.fixed.store(value, Ordering::SeqCst);
                value
            }
            None => self.fixed.load(Ordering::SeqCst),
        }
    }
    fn at(&self, value: u64) {
        self.values.lock().unwrap().clear();
        self.fixed.store(value, Ordering::SeqCst);
    }
    fn queue(&self, values: &[u64]) {
        self.values.lock().unwrap().extend(values.iter().copied());
    }
    fn timing(self: &Arc<Self>) -> Timing {
        let (now, sleeps) = (Arc::clone(self), Arc::clone(self));
        Timing::new(
            Arc::new(move || now.now()),
            Arc::new(move |d| sleeps.sleeps.lock().unwrap().push(d)),
        )
    }
    fn monotonic(self: &Arc<Self>) -> stream::Monotonic {
        let now = Arc::clone(self);
        Arc::new(move || now.now())
    }
}
/// Real monotonic time with recorded, skipped sleeps.
fn quick_timing() -> (Timing, Arc<Mutex<Vec<Duration>>>) {
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&sleeps);
    (
        Timing::new(
            Arc::new(monotonic_ns),
            Arc::new(move |d| recorded.lock().unwrap().push(d)),
        ),
        sleeps,
    )
}

fn observer(
    f: impl Fn(&Path, &Value, ObserveCall<'_, '_>) -> std::result::Result<Value, ObserveError>
    + Send
    + Sync
    + 'static,
) -> Arc<Observer> {
    Arc::new(f)
}
fn verified() -> Arc<Observer> {
    observer(|_, _, call| {
        (call.on_started)()?;
        Ok(json(r#"{"verified":true,"status":"pending"}"#))
    })
}
/// Counts calls and returns `result` without starting.
fn counting(calls: Arc<AtomicUsize>, result: std::result::Result<Value, String>) -> Arc<Observer> {
    observer(move |_, _, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        match &result {
            Ok(value) => Ok(value.clone()),
            Err(message) => Err(ObserveError::Unexpected(message.clone())),
        }
    })
}
fn counting_verified(calls: Arc<AtomicUsize>) -> Arc<Observer> {
    observer(move |_, _, call| {
        calls.fetch_add(1, Ordering::SeqCst);
        (call.on_started)()?;
        Ok(json(r#"{"verified":true,"status":"pending"}"#))
    })
}
fn never() -> (Arc<Observer>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    (
        observer(move |_, _, _| {
            counted.fetch_add(1, Ordering::SeqCst);
            Err(ObserveError::Unexpected(
                "hint reached source executor".into(),
            ))
        }),
        calls,
    )
}

fn darwin_clocks(instance: &str, eligible: Arc<dyn Fn() -> bool + Send + Sync>) -> ClockWindows {
    ClockWindows::new(
        instance,
        eligible,
        Platform::Darwin,
        Arc::new(monotonic_ns),
        None,
    )
}

// --------------------------------------------------------------- fixture

struct Fixture {
    _world: World,
    root: PathBuf,
    base: PathBuf,
    session: String,
    instance: String,
    lifecycle: String,
    binding: Value,
    authority: Arc<TestAuthority>,
    coverage: Coverage,
    responses: Arc<Responses>,
    clocks: ClockWindows,
}
impl Fixture {
    fn new() -> Self {
        let world = World::new();
        let root = world.path().to_path_buf();
        let base = root.join("private");
        private_dir(&base);
        let bank = root.join("memory-bank");
        private_dir(&bank);
        let (session, instance, lifecycle) = (uid(), uid(), uid());
        let binding = json(&format!(
            r#"{{"project_id":"{}","repo_root":"{}","bank_path":"{}","checkout_key":"checkout","artifact_digest":"{}","installation_id":"{}","initial_lifecycle":"{}"}}"#,
            uid(),
            root.display(),
            bank.display(),
            "a".repeat(64),
            uid().replace('-', ""),
            lifecycle
        ));
        let authority = TestAuthority::new(&base);
        let coverage = Coverage::open(
            authority.clone(),
            &root,
            &session,
            &instance,
            &lifecycle,
            &binding,
            None,
        )
        .unwrap();
        let responses = Arc::new(Responses::default());
        let live = Arc::clone(&responses);
        let clocks = darwin_clocks(&instance, Arc::new(move || live.live()));
        Self {
            _world: world,
            root,
            base,
            session,
            instance,
            lifecycle,
            binding,
            authority,
            coverage,
            responses,
            clocks,
        }
    }
    fn known(&self, native: &[&str]) -> Value {
        let native: Vec<&str> = if native.is_empty() {
            vec![]
        } else {
            native.to_vec()
        };
        let mut sorted = native.clone();
        sorted.sort_unstable();
        json(&format!(
            r#"{{"view_id":"{}","leaf_id":{},"native_ids":{},"tool_ids":[]}}"#,
            uid(),
            native
                .last()
                .map_or("null".to_owned(), |leaf| format!("\"{leaf}\"")),
            String::from_utf8(encoded(&Value::Array(
                sorted.iter().map(|s| string(s)).collect()
            )))
            .unwrap()
        ))
    }
    fn request_with(&self, action: &str, known: &Value) -> Value {
        let parents: Vec<Value> = get(known, "native_ids")
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                json(&format!(
                    r#"{{"id":{},"parent_id":null}}"#,
                    String::from_utf8(encoded(id)).unwrap()
                ))
            })
            .collect();
        let mut view = json(r#"{"source_kind":"session-manager"}"#);
        set(&mut view, "leaf_id", get(known, "leaf_id").clone());
        set(&mut view, "native_parents", Value::Array(parents));
        let mut payload = json(r#"{"entries":[]}"#);
        set(&mut payload, "view", view);
        let mut request = json(&format!(
            r#"{{"schema":"agent-memory/omp-observer-request-v2","action":"{action}","session":"{}","lifecycle":"{}"}}"#,
            self.session, self.lifecycle
        ));
        set(&mut request, "payload", payload);
        let mut value = json(r#"{"event_ordinal":1}"#);
        set(&mut value, "request", request);
        set(&mut value, "identity", known.clone());
        value
    }
    fn request(&self) -> Value {
        self.request_with("ingest", &self.known(&["native-a"]))
    }
    fn hint(&self, leaf: Option<&str>, ordinal: u64) -> Value {
        let leaf = leaf.map_or("null".to_owned(), |l| format!("\"{l}\""));
        let known = json(&format!(
            r#"{{"kind":"hint","session":"{s}","header_id":"{s}","cwd":"{}","leaf_id":{leaf}}}"#,
            self.root.display(),
            s = self.session
        ));
        let payload = json(&format!(
            r#"{{"session":"{s}","header_id":"{s}","cwd":"{}","leaf_id":{leaf}}}"#,
            self.root.display(),
            s = self.session
        ));
        let mut request = json(&format!(
            r#"{{"schema":"agent-memory/omp-observer-request-v2","action":"hint","session":"{}","lifecycle":"{}"}}"#,
            self.session, self.lifecycle
        ));
        set(&mut request, "payload", payload);
        let mut value = json(&format!(r#"{{"event_ordinal":{ordinal}}}"#));
        set(&mut value, "request", request);
        set(&mut value, "identity", known);
        value
    }
    fn other_coverage(&self, session: Option<&str>) -> Coverage {
        Coverage::open(
            self.authority.clone(),
            &self.root,
            session.unwrap_or(&self.session),
            &uid(),
            &self.lifecycle,
            &self.binding,
            None,
        )
        .unwrap()
    }
    fn executor(&self, observer: Arc<Observer>, timing: Timing) -> FiniteExecutor {
        FiniteExecutor::new(
            self.responses.clone(),
            self.clocks.clone(),
            None,
            Some(observer),
            timing,
        )
    }
    fn execute(&mut self, observer: Arc<Observer>, value: &Value, sequence: u64) -> Result<()> {
        let executor = self.executor(observer, Timing::system());
        executor.execute(
            &mut self.coverage,
            "REQUEST",
            sequence,
            None,
            &encoded(value),
        )
    }
    fn execute_raw(&mut self, observer: Arc<Observer>, raw: &[u8], sequence: u64) -> Result<()> {
        let executor = self.executor(observer, Timing::system());
        executor.execute(&mut self.coverage, "REQUEST", sequence, None, raw)
    }
    fn execute_on(&self, coverage: &mut Coverage, observer: Arc<Observer>, value: &Value) {
        let live = Arc::clone(&self.responses);
        let clocks = darwin_clocks(coverage.instance(), Arc::new(move || live.live()));
        let executor = FiniteExecutor::new(
            self.responses.clone(),
            clocks,
            None,
            Some(observer),
            Timing::system(),
        );
        executor
            .execute(coverage, "REQUEST", 1, None, &encoded(value))
            .unwrap();
    }
    fn last(count: u64, refused: u64, ordinal: u64, transition: Option<Value>) -> Value {
        let mut value = json(&format!(
            r#"{{"last_attempt_seq":{count},"refused_count":{refused},"last_event_ordinal":{ordinal}}}"#
        ));
        set(&mut value, "transition", transition.unwrap_or(Value::Null));
        value
    }
    fn final_(&self) -> Value {
        Self::last(1, 0, 1, None)
    }
    fn outcome(&self, sequence: u64) -> Value {
        read(
            &self
                .coverage
                .directory()
                .join(format!("outcomes/{sequence}.json")),
        )
    }
    fn inventory(&self) -> Result<Value> {
        stream::inventory(&*self.authority, &self.root, Some(&self.base), false)
    }
    fn pending(&self) -> Result<Value> {
        Ok(get(&self.inventory()?, "pending").clone())
    }
    fn transition(&self, last_view: &Value, target_session: &str, target_instance: &str) -> Value {
        let mut value = json(&format!(
            r#"{{"transition_id":"{}","target_session":"{target_session}","target_instance":"{target_instance}","reason":"transition-final-view-unavailable"}}"#,
            uid()
        ));
        set(&mut value, "last_view", last_view.clone());
        value
    }
    fn retire(&self) -> Vec<Value> {
        stream::retire_for_recovery(&*self.authority, &self.root, &self.session, None).unwrap()
    }
    fn resolve(
        &self,
        fences: &[Value],
        refs: &Value,
        complete: bool,
        leaf: Option<&str>,
        evidence: Option<&stream::NonlearningTailEvidence>,
        view_id: &str,
    ) -> Result<Vec<Value>> {
        let native = strings(&["native-a"]);
        stream::resolve_recovery(
            &*self.authority,
            &self.root,
            fences,
            RecoveryEvidence {
                view_id,
                native_ids: &native,
                tool_ids: &[],
                payload_native_ids: &native,
                source_references: refs,
                complete,
                leaf_id: leaf,
                nonlearning_evidence: evidence,
            },
        )
    }
    fn run(&self, instance: &str, session: &str, data: Vec<u8>, startup_value: Value) -> i32 {
        self.run_with(
            instance,
            session,
            data,
            startup_value,
            Arc::new(Responses::default()),
        )
    }
    fn run_with(
        &self,
        instance: &str,
        session: &str,
        data: Vec<u8>,
        startup_value: Value,
        writer: Arc<dyn Responder>,
    ) -> i32 {
        let mut startup = |_: &Path, _: &str, _: &str, _: &str| -> Result<Option<Value>> {
            Ok(Some(startup_value.clone()))
        };
        stream::run(
            &self.root,
            session,
            instance,
            &self.lifecycle,
            &mut startup,
            RunOptions {
                authority: self.authority.clone(),
                receipt: None,
                input: Box::new(Cursor::new(data)),
                writer,
                clocks: darwin_clocks,
                timing: Timing::system(),
            },
        )
        .unwrap()
    }
    fn directory(&self, instance: &str) -> PathBuf {
        stream::directory(&self.base, &self.session, instance).unwrap()
    }
}
fn frame(kind: &str, sequence: u64, value: &Value) -> Vec<u8> {
    stream::encode_frame(kind, sequence, value, None).unwrap()
}
fn refs(session: &str, lifecycle: &str) -> Value {
    json(&format!(
        r#"[{{"session":"{session}","lifecycle":"{lifecycle}","observation_id":"{}","source_sha256":"{}"}}]"#,
        uid(),
        "b".repeat(64)
    ))
}

// ---------------------------------------------------------------- Framing

struct TinyReads(Cursor<Vec<u8>>);
impl Read for TinyReads {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let one = buffer.len().min(1);
        self.0.read(&mut buffer[..one])
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_multibyte_utf8_crosses_every_read_boundary
#[test]
fn multibyte_utf8_crosses_every_read_boundary() {
    let value = json(r#"{"text":"\u03bb\ud83c\udf0d"}"#);
    let budget = Arc::new(Budget::new());
    let raw = frame("REQUEST", 1, &value);
    let mut reader = FrameReader::new(TinyReads(Cursor::new(raw)), budget.clone());
    let frame = reader.read().unwrap().unwrap();
    assert_eq!(
        (frame.kind.as_str(), frame.sequence, frame.nonce.as_deref()),
        ("REQUEST", 1, None)
    );
    let decoded = json(std::str::from_utf8(&frame.raw).unwrap());
    assert_eq!(encoded(&decoded), encoded(&value));
    assert_eq!(budget.usage().1, frame.raw.len());
    budget.release(frame.raw.len());
    assert!(reader.read().unwrap().is_none());
    assert_eq!(budget.usage(), (0, 0));
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_partial_header_and_body_never_parse_prefix
#[test]
fn partial_header_and_body_never_parse_prefix() {
    for raw in [&b"AM249 REQ"[..], &b"AM249 REQUEST 1 3 -\n{}"[..]] {
        let budget = Arc::new(Budget::new());
        assert!(
            FrameReader::new(Cursor::new(raw.to_vec()), budget.clone())
                .read()
                .is_err()
        );
        assert_eq!(budget.usage(), (0, 0));
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_noncanonical_header_and_oversize_refuse
#[test]
fn noncanonical_header_and_oversize_refuse() {
    let long = vec![b'x'; 257];
    for raw in [
        &b"AM249 REQUEST 01 2 -\n{}"[..],
        &b"AM249 REQUEST 1 16777217 -\n"[..],
        &b"AM249 UNKNOWN 1 0 -\n"[..],
        &b"AM249 CLOCK 1 0 -\n"[..],
        &long[..],
    ] {
        let budget = Arc::new(Budget::new());
        assert!(
            FrameReader::new(Cursor::new(raw.to_vec()), budget)
                .read()
                .is_err()
        );
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_budget_counts_inflight_partial_and_queue
#[test]
fn budget_counts_inflight_partial_and_queue() {
    let budget = Budget::with_limits(2, 8);
    budget.reserve(4).unwrap();
    budget.reserve(4).unwrap();
    assert!(budget.reserve(0).is_err());
    budget.release(4);
    assert!(budget.reserve(5).is_err());
    budget.reserve(4).unwrap();
    assert_eq!(budget.usage(), (2, 8));
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_cancel_control_has_capacity_even_when_source_fifo_is_full
#[test]
fn cancel_control_has_capacity_even_when_source_fifo_is_full() {
    let budget = Arc::new(Budget::with_limits(1, 0));
    budget.reserve(0).unwrap();
    assert!(budget.reserve(0).is_err());
    let raw = frame("CANCEL", 0, &json(&format!(r#"{{"nonce":"{}"}}"#, uid())));
    let read = FrameReader::new(TinyReads(Cursor::new(raw)), budget.clone())
        .read()
        .unwrap()
        .unwrap();
    assert_eq!(read.kind, "CANCEL");
    assert_eq!(budget.usage(), (1, 0));
}

fn body(raw: &[u8]) -> Value {
    let split = raw.iter().position(|b| *b == b'\n').unwrap();
    json(std::str::from_utf8(&raw[split + 1..]).unwrap())
}
fn result_field() -> Vec<(String, Value)> {
    vec![("result".into(), json(r#"{"verified":true}"#))]
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_response_pressure_discards_received_before_priority
#[test]
fn response_pressure_discards_received_before_priority() {
    let writer = ResponseWriter::unstarted(
        &uid(),
        vec![
            ("RECEIVED".into(), b"ack".to_vec()),
            ("OBSERVED".into(), b"one".to_vec()),
            ("CLOCK-REPLY".into(), b"two".to_vec()),
            ("OBSERVED".into(), b"three".to_vec()),
        ],
    );
    assert_eq!(writer.send("OBSERVED", 5, result_field()), Sent::Queued);
    assert!(writer.live());
    let queue = writer.queued();
    assert_eq!(queue.len(), stream::MAX_RESPONSES);
    assert!(!queue.iter().any(|(kind, _)| kind == "RECEIVED"));
    assert!(!writer.loss_pending());
    assert!(queue.last().unwrap().1.starts_with(b"AM249 OBSERVED 5 "));
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_priority_pressure_coalesces_bounded_loss_notice_without_disconnect
#[test]
fn priority_pressure_coalesces_bounded_loss_notice_without_disconnect() {
    let writer = ResponseWriter::unstarted(
        &uid(),
        vec![("OBSERVED".into(), b"priority".to_vec()); stream::MAX_RESPONSES],
    );
    assert_eq!(writer.send("OBSERVED", 7, result_field()), Sent::Dropped);
    assert_eq!(writer.send("OBSERVED", 8, result_field()), Sent::Dropped);
    assert!(writer.live());
    assert!(writer.loss_pending());
    assert_eq!(writer.queued().len(), stream::MAX_RESPONSES);
    writer.pop_front();
    let queue = writer.queued();
    assert_eq!(queue.len(), stream::MAX_RESPONSES);
    assert!(!writer.loss_pending());
    let notice = body(&queue.last().unwrap().1);
    assert_eq!(text(&notice, "code"), "response-pressure");
    assert!(notice.get("result").is_none());
    writer.clear();
    assert_eq!(writer.send("OBSERVED", 9, result_field()), Sent::Queued);
    assert!(writer.live());
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_oversize_response_becomes_fixed_notice_and_later_delivery_stays_live
#[test]
fn oversize_response_becomes_fixed_notice_and_later_delivery_stays_live() {
    let writer = ResponseWriter::unstarted(&uid(), Vec::new());
    let big = vec![(
        "result".to_owned(),
        json(&format!(
            r#"{{"text":"{}"}}"#,
            "x".repeat(stream::MAX_RESPONSE)
        )),
    )];
    assert_eq!(writer.send("OBSERVED", 1, big), Sent::Substituted);
    assert!(writer.live());
    let raw = writer.queued().last().unwrap().1.clone();
    assert!(raw.len() <= stream::MAX_RESPONSE);
    assert!(raw.starts_with(b"AM249 OBSERVED 1 "));
    let notice = body(&raw);
    assert_eq!(text(&notice, "code"), "result-oversized");
    assert!(notice.get("result").is_none());
    assert_eq!(writer.send("OBSERVED", 2, result_field()), Sent::Queued);
}

// obligation: tests/contracts/test_omp_finite_stream.py::Framing.test_actual_descriptor_failure_revokes_recipient
#[test]
fn actual_descriptor_failure_revokes_recipient() {
    let (reader, sender) = std::io::pipe().unwrap();
    drop(reader);
    let writer = ResponseWriter::start(Output::Descriptor(OwnedFd::from(sender)), &uid());
    assert_eq!(writer.send("OBSERVED", 1, result_field()), Sent::Queued);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while writer.live() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!writer.live());
    assert_eq!(writer.send("OBSERVED", 2, result_field()), Sent::Dropped);
}

/// Descriptor delivery writes the exact frame bytes to the recipient.
#[test]
fn descriptor_output_delivers_exact_frames_in_order() {
    let (mut reader, sender) = std::io::pipe().unwrap();
    let instance = uid();
    let writer = ResponseWriter::start(Output::Descriptor(OwnedFd::from(sender)), &instance);
    assert_eq!(writer.send("RECEIVED", 1, Vec::new()), Sent::Queued);
    assert_eq!(writer.send("OBSERVED", 1, result_field()), Sent::Queued);
    writer.finish();
    drop(writer);
    let mut raw = Vec::new();
    reader.read_to_end(&mut raw).unwrap();
    let received = format!("{{\"instance_id\":\"{instance}\"}}");
    let expected_received = format!("AM249 RECEIVED 1 {} -\n{received}", received.len());
    assert!(raw.starts_with(expected_received.as_bytes()));
    let rest = &raw[expected_received.len()..];
    let observed = format!("{{\"instance_id\":\"{instance}\",\"result\":{{\"verified\":true}}}}");
    assert_eq!(
        rest,
        format!("AM249 OBSERVED 1 {} -\n{observed}", observed.len()).as_bytes()
    );
}

/// The header grammar and body bytes are exactly the TypeScript peer's.
#[test]
fn encoded_frames_are_byte_exact() {
    let nonce = uid();
    let raw =
        stream::encode_frame("REQUEST", 3, &json(r#"{"b":1,"a":"\u00e9"}"#), Some(&nonce)).unwrap();
    let body = "{\"a\":\"\u{e9}\",\"b\":1}";
    assert_eq!(
        raw,
        format!("AM249 REQUEST 3 {} {nonce}\n{body}", body.len()).into_bytes()
    );
    assert!(stream::encode_frame("REQUEST", 1 << 53, &json("{}"), None).is_err());
    assert!(stream::encode_frame("REQUEST", 1, &json("{}"), Some("not-a-uuid")).is_err());
}

// ------------------------------------------------------- CoverageFixture

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_verified_pending_is_terminal_transport_not_learning_completion
#[test]
fn verified_pending_is_terminal_transport_not_learning_completion() {
    let mut f = Fixture::new();
    let request = f.request();
    f.execute(verified(), &request, 1).unwrap();
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
    assert!(f.coverage.close(&f.final_()).unwrap());
    let inventory = f.inventory().unwrap();
    assert_eq!(get(&inventory, "pending"), &Value::Array(vec![]));
    assert_eq!(
        get(&get(&inventory, "streams").as_array().unwrap()[0], "closed"),
        &Value::Bool(true)
    );
    assert!(
        !String::from_utf8(encoded(&inventory))
            .unwrap()
            .contains("completed")
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_busy_before_started_retries_same_attempt_only
#[test]
fn busy_before_started_retries_same_attempt_only() {
    let mut f = Fixture::new();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&calls);
    let observe = observer(move |_, request, call| {
        let mut seen = seen.lock().unwrap();
        seen.push(request as *const Value as usize);
        if seen.len() == 1 {
            return Err(ObserveError::Contract(busy_error()));
        }
        (call.on_started)()?;
        Ok(json(r#"{"verified":true}"#))
    });
    let (timing, sleeps) = quick_timing();
    let request = f.request();
    f.executor(observe, timing)
        .execute(&mut f.coverage, "REQUEST", 1, None, &encoded(&request))
        .unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(
        *sleeps.lock().unwrap().last().unwrap(),
        stream::BUSY_BACKOFF
    );
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
    assert_eq!(f.coverage.last_sequence(), 1);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_busy_after_started_never_replays
#[test]
fn busy_after_started_never_replays() {
    let mut f = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let observe = observer(move |_, _, call| {
        counted.fetch_add(1, Ordering::SeqCst);
        (call.on_started)()?;
        Err(ObserveError::Contract(busy_error()))
    });
    let request = f.request();
    f.execute(observe, &request, 1).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(text(&f.outcome(1), "code"), "unknown-after-start");
    assert!(!f.coverage.close(&f.final_()).unwrap());
    assert!(codes(&f.pending().unwrap()).contains(&"open-stream".to_owned()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_busy_exhaustion_is_open_not_durable_refusal
#[test]
fn busy_exhaustion_is_open_not_durable_refusal() {
    let mut f = Fixture::new();
    let script = Script::new(&[0, 30_000_000_000]);
    let observe = observer(|_, _, _| Err(ObserveError::Contract(busy_error())));
    let request = f.request();
    f.executor(observe, script.timing())
        .execute(&mut f.coverage, "REQUEST", 1, None, &encoded(&request))
        .unwrap();
    assert_eq!(text(&f.outcome(1), "outcome"), "unknown");
    assert_eq!(text(&f.outcome(1), "code"), "busy-exhausted");
    assert!(!f.coverage.close(&f.final_()).unwrap());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_metadata_busy_after_verified_never_replays_observer
#[test]
fn metadata_busy_after_verified_never_replays_observer() {
    let mut f = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    // Binding checks: attempt, started, then the first outcome write defers.
    let before = f.authority.verifies();
    f.authority.busy_entries.lock().unwrap().push(before + 3);
    let (timing, sleeps) = quick_timing();
    let request = f.request();
    f.executor(counting_verified(calls.clone()), timing)
        .execute(&mut f.coverage, "REQUEST", 1, None, &encoded(&request))
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.authority.verifies() - before, 4);
    assert!(!sleeps.lock().unwrap().is_empty());
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_arbitrary_exception_is_not_verified_receipt_or_refusal
#[test]
fn arbitrary_exception_is_not_verified_receipt_or_refusal() {
    let mut f = Fixture::new();
    let request = f.request();
    let calls = Arc::new(AtomicUsize::new(0));
    f.execute(
        counting(calls, Err("sensitive body must not persist".into())),
        &request,
        1,
    )
    .unwrap();
    let value = f.outcome(1);
    assert_eq!(
        (text(&value, "outcome"), text(&value, "code")),
        ("unknown", "unknown-outcome")
    );
    assert!(
        !String::from_utf8(encoded(&value))
            .unwrap()
            .contains("sensitive")
    );
    assert!(
        !f.responses
            .values()
            .iter()
            .any(|(_, _, v)| v.get("result").is_some())
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_fixed_prestart_refusal_closes_but_blocks_drain
#[test]
fn fixed_prestart_refusal_closes_but_blocks_drain() {
    let mut f = Fixture::new();
    let request = f.request();
    let refuse = observer(|_, _, _| {
        Err(ObserveError::Contract(ContractError::new(
            "arbitrary diagnostic not retained",
        )))
    });
    f.execute(refuse, &request, 1).unwrap();
    assert_eq!(text(&f.outcome(1), "outcome"), "durable-refusal");
    assert!(f.coverage.close(&f.final_()).unwrap());
    let inventory = f.inventory().unwrap();
    assert_eq!(
        get(&get(&inventory, "streams").as_array().unwrap()[0], "closed"),
        &Value::Bool(true)
    );
    assert!(stream::validate_drain(&*f.authority, &f.root, &inventory, Some(&f.base)).is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_removed_marker_refuses_open_and_later_metadata_producers
#[test]
fn removed_marker_refuses_open_and_later_metadata_producers() {
    let mut f = Fixture::new();
    let instance = uid();
    *f.authority.refusal.lock().unwrap() = Some("activation required".into());
    assert!(
        Coverage::open(
            f.authority.clone(),
            &f.root,
            &f.session,
            &instance,
            &f.lifecycle,
            &f.binding,
            None
        )
        .is_err()
    );
    let known = f.known(&["native-a"]);
    assert!(
        f.coverage
            .attempt(Attempt {
                sequence: 1,
                action: "ingest",
                ordinal: 1,
                disposition: "accepted",
                reason: None,
                known: &known,
                source_kind: None,
                frame_kind: "REQUEST",
            })
            .is_err()
    );
    *f.authority.refusal.lock().unwrap() = None;
    assert!(!f.directory(&instance).exists());
    assert!(!f.coverage.directory().join("attempts").exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_verified_without_on_started_is_not_accepted
#[test]
fn verified_without_on_started_is_not_accepted() {
    let mut f = Fixture::new();
    let request = f.request();
    let calls = Arc::new(AtomicUsize::new(0));
    f.execute(
        counting(calls, Ok(json(r#"{"verified":true}"#))),
        &request,
        1,
    )
    .unwrap();
    assert_eq!(text(&f.outcome(1), "outcome"), "unknown");
    assert!(!f.coverage.close(&f.final_()).unwrap());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_bad_schema_session_and_identity_are_counted_without_dropping_later_source
#[test]
fn bad_schema_session_and_identity_are_counted_without_dropping_later_source() {
    let mut f = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    for (sequence, mutate) in [(1, "schema"), (2, "session"), (3, "identity")] {
        let mut value = f.request();
        match mutate {
            "schema" => set(
                value.get_mut("request").unwrap(),
                "schema",
                string("agent-memory/omp-observer-request-v1"),
            ),
            "session" => set(value.get_mut("request").unwrap(), "session", string(&uid())),
            _ => {
                let identity = value.get_mut("identity").unwrap();
                set(identity, "native_ids", json(r#"["other"]"#));
                set(identity, "leaf_id", string("other"));
            }
        }
        f.execute(counting_verified(calls.clone()), &value, sequence)
            .unwrap();
        assert_eq!(text(&f.outcome(sequence), "code"), "invalid-request");
        assert_eq!(text(&f.outcome(sequence), "outcome"), "durable-refusal");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let request = f.request();
    f.execute(counting_verified(calls.clone()), &request, 4)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(text(&f.outcome(4), "outcome"), "verified");
    assert_eq!(f.coverage.refused_count(), 0);
    assert!(f.coverage.close(&Fixture::last(4, 0, 1, None)).unwrap());
    assert!(codes(&f.pending().unwrap()).contains(&"invalid-request".to_owned()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_invalid_utf8_and_partial_json_are_refused_without_losing_following_frame
#[test]
fn invalid_utf8_and_partial_json_are_refused_without_losing_following_frame() {
    let mut f = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut utf16 = vec![0xff, 0xfe];
    for unit in "{\"text\":\"ok\"}".encode_utf16() {
        utf16.extend(unit.to_le_bytes());
    }
    let cases = [b"{\"text\":\"\xff\"}".to_vec(), utf16, b"{".to_vec()];
    for (sequence, raw) in (1..).zip(cases) {
        f.execute_raw(counting_verified(calls.clone()), &raw, sequence)
            .unwrap();
        assert_eq!(text(&f.outcome(sequence), "code"), "invalid-request");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let request = f.request();
    f.execute(counting_verified(calls.clone()), &request, 4)
        .unwrap();
    assert_eq!(text(&f.outcome(4), "outcome"), "verified");
    assert!(f.coverage.close(&Fixture::last(4, 0, 1, None)).unwrap());
}

fn invalid_attempt_value(f: &Fixture) -> Value {
    json(&format!(
        r#"{{"schema":"agent-memory/omp-stream-attempt-v1","open_sha256":"{}","instance_id":"{}","sequence":1,"action":"hint","event_ordinal":0,"disposition":"refused","reason":"invalid-request","identity":{{"kind":"unresolvable"}},"source_kind":null,"frame_kind":"REQUEST"}}"#,
        f.coverage.open_digest(),
        f.instance
    ))
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_matching_preexisting_invalid_attempt_is_not_a_live_retry_witness
#[test]
fn matching_preexisting_invalid_attempt_is_not_a_live_retry_witness() {
    let mut f = Fixture::new();
    let path = f.coverage.directory().join("attempts/1.json");
    let value = invalid_attempt_value(&f);
    stream::retain(&path, &value, stream::MAX_IDENTITY).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(f.coverage.refuse_invalid(1, "REQUEST", "hint", 0).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        (f.coverage.last_sequence(), f.coverage.refused_count()),
        (0, 0)
    );
    assert!(!f.coverage.invalid_attempt_pending());
    std::fs::remove_file(&path).unwrap();
    // The precheck succeeds, then its exit recheck defers with typed busy.
    let next = f.authority.verifies() + 1;
    f.authority.busy_exits.lock().unwrap().push(next);
    let deferred = f
        .coverage
        .refuse_invalid(1, "REQUEST", "hint", 0)
        .unwrap_err();
    assert!(busy(&deferred));
    stream::retain(&path, &value, stream::MAX_IDENTITY).unwrap();
    let refused = f
        .coverage
        .refuse_invalid(1, "REQUEST", "hint", 0)
        .unwrap_err();
    assert!(!busy(&refused));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        (f.coverage.last_sequence(), f.coverage.refused_count()),
        (0, 0)
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_partial_immutable_attempt_is_never_replaced_by_refusal
#[test]
fn partial_immutable_attempt_is_never_replaced_by_refusal() {
    let mut f = Fixture::new();
    let path = f.coverage.directory().join("attempts/1.json");
    turn_sources::private_mkdir(path.parent().unwrap()).unwrap();
    write_private(&path, b"{");
    let request = f.request();
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(
        f.execute(counting(calls, Ok(Value::Null)), &request, 1)
            .is_err()
    );
    assert_eq!(
        turn_sources::read_bytes(&path, stream::MAX_METADATA)
            .unwrap()
            .unwrap(),
        b"{"
    );
    assert!(!f.coverage.directory().join("outcomes/1.json").exists());
    assert!(!f.responses.kinds().contains(&"REFUSED".to_owned()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_real_io_failure_does_not_manufacture_invalid_request_refusal
#[test]
fn real_io_failure_does_not_manufacture_invalid_request_refusal() {
    // A real persistence failure after the attempt is retained: the outcome
    // namespace is occupied by a regular file, so publishing it fails.
    let mut f = Fixture::new();
    write_private(&f.coverage.directory().join("outcomes"), b"");
    let request = f.request();
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(
        f.execute(counting(calls, Ok(Value::Null)), &request, 1)
            .is_err()
    );
    let attempt = read(&f.coverage.directory().join("attempts/1.json"));
    assert_eq!(text(&attempt, "disposition"), "accepted");
    assert!(!f.coverage.directory().join("outcomes/1.json").exists());
    assert!(!f.responses.kinds().contains(&"REFUSED".to_owned()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_source_kind_validation_failure_accounts_slot_and_continues
#[test]
fn source_kind_validation_failure_accounts_slot_and_continues() {
    let mut f = Fixture::new();
    let mut malformed = f.request();
    let view = malformed
        .get_mut("request")
        .and_then(|r| r.get_mut("payload"))
        .and_then(|p| p.get_mut("view"))
        .unwrap();
    set(view, "source_kind", string("unrecognized"));
    f.execute(
        counting(Arc::new(AtomicUsize::new(0)), Ok(Value::Null)),
        &malformed,
        1,
    )
    .unwrap();
    assert_eq!(text(&f.outcome(1), "code"), "invalid-request");
    let request = f.request();
    f.execute(verified(), &request, 2).unwrap();
    assert_eq!(text(&f.outcome(2), "outcome"), "verified");
}

fn attempt<'a>(
    sequence: u64,
    ordinal: u64,
    disposition: &'a str,
    reason: Option<&'a str>,
    known: &'a Value,
) -> Attempt<'a> {
    Attempt {
        sequence,
        action: "ingest",
        ordinal,
        disposition,
        reason,
        known,
        source_kind: None,
        frame_kind: "REQUEST",
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_unknown_identity_breaks_delta_chain
#[test]
fn unknown_identity_breaks_delta_chain() {
    let mut f = Fixture::new();
    let first = f.known(&["native-a"]);
    let second = f.known(&["native-a", "native-b"]);
    f.coverage
        .attempt(attempt(1, 1, "accepted", None, &first))
        .unwrap();
    f.coverage
        .attempt(attempt(
            2,
            2,
            "refused",
            Some("capture-refused"),
            &Value::Null,
        ))
        .unwrap();
    f.coverage
        .attempt(attempt(3, 3, "accepted", None, &second))
        .unwrap();
    let value = read(&f.coverage.directory().join("attempts/3.json"));
    assert_eq!(text(get(&value, "identity"), "kind"), "baseline");
    assert!(get(&value, "identity").get("native_added").is_none());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_lost_refusal_notice_cannot_match_final
#[test]
fn lost_refusal_notice_cannot_match_final() {
    let mut f = Fixture::new();
    let request = f.request();
    f.execute(verified(), &request, 1).unwrap();
    assert!(f.coverage.close(&Fixture::last(2, 1, 1, None)).is_err());
    assert!(!f.coverage.directory().join("closed.json").exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_retirement_fence_prevents_old_start
#[test]
fn retirement_fence_prevents_old_start() {
    let mut f = Fixture::new();
    let known = f.known(&["native-a"]);
    let digest = f
        .coverage
        .attempt(attempt(1, 1, "accepted", None, &known))
        .unwrap();
    f.coverage.close_live();
    let fences = f.retire();
    assert_eq!(fences.len(), 1);
    assert!(f.coverage.started(1, &digest).is_err());
    assert!(!f.coverage.directory().join("started/1.json").exists());
    assert_eq!(
        encoded(&Value::Array(f.retire())),
        encoded(&Value::Array(fences))
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_retirement_skips_live_worker_even_while_stream_metadata_is_locked
#[test]
fn retirement_skips_live_worker_even_while_stream_metadata_is_locked() {
    let mut f = Fixture::new();
    {
        let _held = turn_sources::private_lock(f.coverage.lock_path(), false, false).unwrap();
        assert!(f.retire().is_empty());
    }
    assert!(!f.coverage.directory().join("retirement.json").exists());
    let request = f.request();
    f.execute(verified(), &request, 1).unwrap();
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
    assert!(f.coverage.close(&f.final_()).unwrap());
    assert!(f.retire().is_empty());
    f.coverage.close_live();
    assert_eq!(f.retire().len(), 1);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_exact_recovery_does_not_clear_open_unknown_tail
#[test]
fn exact_recovery_does_not_clear_open_unknown_tail() {
    let mut f = Fixture::new();
    let request = f.request();
    let refuse = observer(|_, _, _| Err(ObserveError::Contract(ContractError::new("refused"))));
    f.execute(refuse, &request, 1).unwrap();
    f.coverage.close_live();
    let fences = f.retire();
    let references = refs(&f.session, &f.lifecycle);
    let recovered = f
        .resolve(&fences, &references, false, None, None, &uid())
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(codes(&f.pending().unwrap()), vec!["open-stream"]);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_pre_id_refusal_cannot_be_resolved_by_counts
#[test]
fn pre_id_refusal_cannot_be_resolved_by_counts() {
    let mut f = Fixture::new();
    let digest = f
        .coverage
        .attempt(attempt(
            1,
            1,
            "refused",
            Some("capture-refused"),
            &Value::Null,
        ))
        .unwrap();
    f.coverage
        .outcome(1, &digest, "durable-refusal", "capture-refused", &[])
        .unwrap();
    f.coverage.close_live();
    let fences = f.retire();
    let recovered = f
        .resolve(&fences, &json("[]"), true, None, None, &uid())
        .unwrap();
    assert!(recovered.is_empty());
    assert!(codes(&f.pending().unwrap()).contains(&"unresolvable-identity".to_owned()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_transition_binds_last_old_view_and_remains_pending_after_close
#[test]
fn transition_binds_last_old_view_and_remains_pending_after_close() {
    let mut f = Fixture::new();
    let value = f.request();
    f.execute(verified(), &value, 1).unwrap();
    let transition = f.transition(get(&value, "identity"), &uid(), &uid());
    assert!(
        f.coverage
            .close(&Fixture::last(1, 0, 1, Some(transition)))
            .unwrap()
    );
    assert_eq!(
        codes(&f.pending().unwrap()),
        vec!["transition-final-view-unavailable"]
    );
    let inventory = f.inventory().unwrap();
    assert_eq!(
        get(&get(&inventory, "streams").as_array().unwrap()[0], "closed"),
        &Value::Bool(true)
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_failed_retired_view_does_not_replace_precommit_transition_anchor
#[test]
fn failed_retired_view_does_not_replace_precommit_transition_anchor() {
    let mut f = Fixture::new();
    let original = f.request();
    f.execute(verified(), &original, 1).unwrap();
    let known = f.known(&["native-a", "native-b"]);
    let mut recovery = f.request_with("ingest", &known);
    let view = recovery
        .get_mut("request")
        .and_then(|r| r.get_mut("payload"))
        .and_then(|p| p.get_mut("view"))
        .unwrap();
    set(view, "source_kind", string("retired-file"));
    set(&mut recovery, "event_ordinal", json("2"));
    f.execute(verified(), &recovery, 2).unwrap();
    let transition = f.transition(get(&original, "identity"), &uid(), &uid());
    assert!(
        f.coverage
            .close(&Fixture::last(2, 0, 2, Some(transition)))
            .unwrap()
    );
    assert_eq!(
        codes(&f.pending().unwrap()),
        vec!["transition-final-view-unavailable"]
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_null_leaf_hint_is_fast_neutral_metadata_and_never_observes
#[test]
fn null_leaf_hint_is_fast_neutral_metadata_and_never_observes() {
    let mut f = Fixture::new();
    let (observe, calls) = never();
    let initial = f.authority.attests();
    let hint = f.hint(None, 1);
    f.execute(observe, &hint, 1).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.authority.attests(), initial);
    assert_eq!(text(&f.outcome(1), "code"), "hint-recorded");
    assert!(!f.coverage.directory().join("started").exists());
    assert!(!f.base.join("omp-source-v1").exists());
    assert!(f.coverage.close(&f.final_()).unwrap());
    assert_eq!(f.pending().unwrap(), Value::Array(vec![]));
    assert_eq!(f.authority.disposition_calls(), 0);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_failed_hint_identity_copy_is_fast_counted_refusal_not_neutral_null_leaf
#[test]
fn failed_hint_identity_copy_is_fast_counted_refusal_not_neutral_null_leaf() {
    let mut f = Fixture::new();
    let (observe, calls) = never();
    let initial = f.authority.attests();
    let value =
        json(r#"{"action":"hint","event_ordinal":1,"reason":"capture-refused","identity":null}"#);
    f.executor(observe, Timing::system())
        .execute(
            &mut f.coverage,
            "REFUSED-ATTEMPT",
            1,
            None,
            &encoded(&value),
        )
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.authority.attests(), initial);
    assert_eq!(f.coverage.refused_count(), 1);
    assert_eq!(text(&f.outcome(1), "outcome"), "durable-refusal");
    assert!(f.coverage.close(&Fixture::last(1, 1, 1, None)).unwrap());
    assert!(codes(&f.pending().unwrap()).contains(&"capture-refused".to_owned()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_hint_cannot_carry_source_fields_or_create_human_authority
#[test]
fn hint_cannot_carry_source_fields_or_create_human_authority() {
    let mut f = Fixture::new();
    let mut value = f.hint(None, 1);
    let payload = value
        .get_mut("request")
        .and_then(|r| r.get_mut("payload"))
        .unwrap();
    set(payload, "role", string("user"));
    let (observe, calls) = never();
    f.execute(observe, &value, 1).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(text(&f.outcome(1), "code"), "invalid-request");
    assert!(!f.base.join("omp-source-v1").exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_hints_preserve_full_delta_baseline_and_transition_anchor
#[test]
fn hints_preserve_full_delta_baseline_and_transition_anchor() {
    let mut f = Fixture::new();
    let first = f.request();
    f.execute(verified(), &first, 1).unwrap();
    let before = f.coverage.previous_identity().cloned();
    let (observe, calls) = never();
    let hint = f.hint(Some("native-a"), 1);
    f.execute(observe.clone(), &hint, 2).unwrap();
    assert_eq!(f.coverage.previous_identity().cloned(), before);
    assert_eq!(
        encoded(f.coverage.last_checked_identity().unwrap()),
        encoded(get(&first, "identity"))
    );
    let known = f.known(&["native-a", "native-b"]);
    let second = f.request_with("ingest", &known);
    f.execute(verified(), &second, 3).unwrap();
    let recorded = read(&f.coverage.directory().join("attempts/3.json"));
    assert_eq!(text(get(&recorded, "identity"), "kind"), "delta");
    assert_eq!(
        get(get(&recorded, "identity"), "native_added"),
        &json(r#"["native-b"]"#)
    );
    let hint = f.hint(Some("native-b"), 1);
    f.execute(observe, &hint, 4).unwrap();
    let transition = f.transition(get(&second, "identity"), &uid(), &uid());
    assert!(
        f.coverage
            .close(&Fixture::last(4, 0, 1, Some(transition)))
            .unwrap()
    );
    assert_eq!(
        codes(&f.pending().unwrap()),
        vec!["transition-final-view-unavailable"]
    );
    assert_eq!(f.authority.disposition_calls(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_unknown_hint_leaf_resolves_from_fresh_exact_native_disposition_join_once
#[test]
fn unknown_hint_leaf_resolves_from_fresh_exact_native_disposition_join_once() {
    let mut f = Fixture::new();
    let (observe, calls) = never();
    let initial = f.authority.attests();
    for sequence in 1..9 {
        let hint = f.hint(Some("native-a"), sequence);
        f.execute(observe.clone(), &hint, sequence).unwrap();
    }
    assert_eq!(f.authority.attests(), initial);
    assert!(f.coverage.close(&Fixture::last(8, 0, 8, None)).unwrap());
    let path = f.coverage.directory().join("attempts/1.json");
    let original = turn_sources::read_bytes(&path, stream::MAX_METADATA).unwrap();
    f.authority.dispositions(Dispositions::Set(BTreeSet::new()));
    let before = f.inventory().unwrap();
    assert_eq!(f.authority.disposition_calls(), 1);
    assert_eq!(codes(get(&before, "pending")), vec!["hint-uncovered"; 8]);
    f.authority
        .dispositions(Dispositions::Set(BTreeSet::from(["native-a".to_owned()])));
    let after = f.inventory().unwrap();
    assert_eq!(f.authority.disposition_calls(), 2);
    assert_eq!(get(&after, "pending"), &Value::Array(vec![]));
    assert_ne!(
        get(&before, "inventory_sha256"),
        get(&after, "inventory_sha256")
    );
    assert_eq!(
        turn_sources::read_bytes(&path, stream::MAX_METADATA).unwrap(),
        original
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_earlier_instance_verified_full_view_covers_later_instance_hint
#[test]
fn earlier_instance_verified_full_view_covers_later_instance_hint() {
    let mut f = Fixture::new();
    let request = f.request();
    f.execute(verified(), &request, 1).unwrap();
    assert!(f.coverage.close(&f.final_()).unwrap());
    let mut later = f.other_coverage(None);
    let (observe, calls) = never();
    f.execute_on(&mut later, observe, &f.hint(Some("native-a"), 1));
    assert!(later.close(&f.final_()).unwrap());
    assert_eq!(f.pending().unwrap(), Value::Array(vec![]));
    assert_eq!(f.authority.disposition_calls(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_later_instance_full_view_resolves_earlier_hint
#[test]
fn later_instance_full_view_resolves_earlier_hint() {
    let mut f = Fixture::new();
    let (observe, _) = never();
    let hint = f.hint(Some("native-a"), 1);
    f.execute(observe, &hint, 1).unwrap();
    assert!(f.coverage.close(&f.final_()).unwrap());
    f.authority.dispositions(Dispositions::Set(BTreeSet::new()));
    assert_eq!(codes(&f.pending().unwrap()), vec!["hint-uncovered"]);
    let mut later = f.other_coverage(None);
    f.execute_on(&mut later, verified(), &f.request());
    assert!(later.close(&f.final_()).unwrap());
    let calls = f.authority.disposition_calls();
    f.authority.dispositions(Dispositions::Unconfigured);
    assert_eq!(f.pending().unwrap(), Value::Array(vec![]));
    assert_eq!(f.authority.disposition_calls(), calls);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_refused_and_unknown_full_frames_do_not_cover_hint_merely_by_native_ids
#[test]
fn refused_and_unknown_full_frames_do_not_cover_hint_merely_by_native_ids() {
    let mut f = Fixture::new();
    let (observe, _) = never();
    let hint = f.hint(Some("native-a"), 1);
    f.execute(observe, &hint, 1).unwrap();
    assert!(f.coverage.close(&f.final_()).unwrap());
    for started in [false, true] {
        let mut later = f.other_coverage(None);
        let observe = observer(move |_, _, call| {
            if started {
                (call.on_started)()?;
                return Err(ObserveError::Unexpected(
                    "unknown after write boundary".into(),
                ));
            }
            Err(ObserveError::Contract(ContractError::new(
                "prestart refusal",
            )))
        });
        f.execute_on(&mut later, observe, &f.request());
    }
    f.authority.dispositions(Dispositions::Set(BTreeSet::new()));
    assert!(codes(&f.pending().unwrap()).contains(&"hint-uncovered".to_owned()));
    assert_eq!(f.authority.disposition_calls(), 1);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_another_session_full_view_does_not_cover_hint_and_source_errors_refuse
#[test]
fn another_session_full_view_does_not_cover_hint_and_source_errors_refuse() {
    let mut f = Fixture::new();
    let (observe, _) = never();
    let hint = f.hint(Some("native-a"), 1);
    f.execute(observe, &hint, 1).unwrap();
    let other_session = uid();
    let mut other = f.other_coverage(Some(&other_session));
    let mut value = f.request();
    set(
        value.get_mut("request").unwrap(),
        "session",
        string(&other_session),
    );
    f.execute_on(&mut other, verified(), &value);
    f.authority.dispositions(Dispositions::Set(BTreeSet::new()));
    assert!(codes(&f.pending().unwrap()).contains(&"hint-uncovered".to_owned()));
    f.authority
        .dispositions(Dispositions::Fail("unverifiable source state".into()));
    assert!(f.pending().is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_same_session_different_instance_transition_is_valid_but_self_target_refuses
#[test]
fn same_session_different_instance_transition_is_valid_but_self_target_refuses() {
    let mut f = Fixture::new();
    let value = f.request();
    f.execute(verified(), &value, 1).unwrap();
    let transition = f.transition(get(&value, "identity"), &f.session, &f.instance);
    assert!(
        f.coverage
            .close(&Fixture::last(1, 0, 1, Some(transition.clone())))
            .is_err()
    );
    let mut transition = transition;
    set(&mut transition, "target_instance", string(&uid()));
    assert!(
        f.coverage
            .close(&Fixture::last(1, 0, 1, Some(transition)))
            .unwrap()
    );
    assert_eq!(
        codes(&f.pending().unwrap()),
        vec!["transition-final-view-unavailable"]
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_empty_refs_cannot_resolve_learning_attempt_or_transition
#[test]
fn empty_refs_cannot_resolve_learning_attempt_or_transition() {
    let mut f = Fixture::new();
    let value = f.request();
    f.execute(verified(), &value, 1).unwrap();
    let transition = f.transition(get(&value, "identity"), &uid(), &uid());
    f.coverage
        .close(&Fixture::last(1, 0, 1, Some(transition)))
        .unwrap();
    f.coverage.close_live();
    let fences = f.retire();
    assert!(
        f.resolve(&fences, &json("[]"), true, None, None, &uid())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        codes(&f.pending().unwrap()),
        vec!["transition-final-view-unavailable"]
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_internal_nonlearning_evidence_resolves_only_anchored_closed_transition
#[test]
fn internal_nonlearning_evidence_resolves_only_anchored_closed_transition() {
    // Unmintable and unserializable by type: NonlearningTailEvidence has private
    // fields, no Clone and no serialization, and resolve_recovery accepts only it.
    let mut f = Fixture::new();
    let value = f.request();
    f.execute(verified(), &value, 1).unwrap();
    let transition = f.transition(get(&value, "identity"), &uid(), &uid());
    f.coverage
        .close(&Fixture::last(1, 0, 1, Some(transition)))
        .unwrap();
    f.coverage.close_live();
    let fences = f.retire();
    let view_id = uid();
    let evidence =
        stream::nonlearning_tail_evidence(&f.session, &view_id, &strings(&["native-a"])).unwrap();
    assert!(
        f.resolve(
            &fences,
            &json("[]"),
            true,
            Some("native-a"),
            Some(&evidence),
            &view_id
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(f.pending().unwrap(), Value::Array(vec![]));
    assert!(!f.coverage.directory().join("resolutions").exists());
    assert!(stream::nonlearning_tail_evidence(&f.session, &uid(), &[]).is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_inventory_refuses_unknown_private_state
#[test]
fn inventory_refuses_unknown_private_state() {
    let f = Fixture::new();
    turn_sources::exclusive(
        &f.coverage.directory().join("outcomes-bogus.json"),
        &json(r#"{"x":1}"#),
        4096,
    )
    .unwrap();
    assert!(f.inventory().is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_inventory_accepts_symlinked_repository_alias
#[test]
fn inventory_accepts_symlinked_repository_alias() {
    let f = Fixture::new();
    let alias = f.root.parent().unwrap().join(format!(
        "{}-alias-{}",
        f.root.file_name().unwrap().to_str().unwrap(),
        uid()
    ));
    std::os::unix::fs::symlink(&f.root, &alias).unwrap();
    let value = stream::inventory(&*f.authority, &alias, Some(&f.base), false);
    std::fs::remove_file(&alias).unwrap();
    let value = value.unwrap();
    assert_eq!(
        text(&get(&value, "streams").as_array().unwrap()[0], "session"),
        f.session
    );
}

fn row<'a>(inventory: &'a Value, instance: &str) -> &'a Value {
    get(inventory, "streams")
        .as_array()
        .unwrap()
        .iter()
        .find(|row| text(row, "instance_id") == instance)
        .unwrap()
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_unpublished_instance_stays_pending_while_later_valid_source_closes
#[test]
fn unpublished_instance_stays_pending_while_later_valid_source_closes() {
    let mut f = Fixture::new();
    let abandoned_id = uid();
    let abandoned = f.directory(&abandoned_id);
    turn_sources::private_mkdir(&abandoned).unwrap();
    drop(turn_sources::private_lock(&abandoned.join("live.lock"), false, false).unwrap());
    let value = f.inventory().unwrap();
    let found = row(&value, &abandoned_id);
    assert_eq!(get(found, "session"), &Value::Null);
    assert_eq!(
        text(found, "session_sha256"),
        abandoned
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!(
        get(found, "pending"),
        &json(r#"[{"code":"startup-incomplete"}]"#)
    );
    let before = text(&value, "inventory_sha256").to_owned();
    let authority = f.authority.clone();
    let observe = observer(move |repo, request, call| {
        let handles = stream::retire_for_recovery(
            &*authority,
            repo,
            text(request, "session"),
            Some(call.instance_id),
        )
        .map_err(ObserveError::Contract)?;
        assert!(handles.is_empty());
        (call.on_started)()?;
        Ok(json(r#"{"verified":true}"#))
    });
    let request = f.request();
    f.execute(observe, &request, 1).unwrap();
    assert!(f.coverage.close(&f.final_()).unwrap());
    let after = f.inventory().unwrap();
    assert_ne!(before, text(&after, "inventory_sha256"));
    assert_eq!(
        get(row(&after, &abandoned_id), "pending"),
        &json(r#"[{"code":"startup-incomplete"}]"#)
    );
    assert!(stream::validate_drain(&*f.authority, &f.root, &after, Some(&f.base)).is_err());
    assert!(
        stream::inventory(&*f.authority, &f.root, Some(&f.base.join("offline")), false).is_err()
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_partial_open_is_pinned_as_unpublished_without_trusting_its_binding
#[test]
fn partial_open_is_pinned_as_unpublished_without_trusting_its_binding() {
    let f = Fixture::new();
    let abandoned_id = uid();
    let abandoned = f.directory(&abandoned_id);
    turn_sources::private_mkdir(&abandoned).unwrap();
    let partial = b"{\"schema\":\"agent-memory/omp-stream-open-v1\",\"project_id\":";
    write_private(&abandoned.join("open.json"), partial);
    let value = f.inventory().unwrap();
    let found = row(&value, &abandoned_id);
    assert_eq!(get(found, "session"), &Value::Null);
    assert_eq!(get(found, "open_sha256"), &Value::Null);
    assert_eq!(
        get(found, "pending"),
        &json(r#"[{"code":"startup-incomplete"}]"#)
    );
    assert!(f.retire().is_empty());
    assert_eq!(
        turn_sources::read_bytes(&abandoned.join("open.json"), stream::MAX_METADATA)
            .unwrap()
            .unwrap(),
        partial
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_session_creation_contention_is_typed_busy_without_publishing_instance
#[test]
fn session_creation_contention_is_typed_busy_without_publishing_instance() {
    let f = Fixture::new();
    let instance = uid();
    let session_sha = f
        .coverage
        .directory()
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let path = f
        .base
        .join("omp-stream-session-locks")
        .join(format!("{session_sha}.lock"));
    let _held = turn_sources::private_lock(&path, false, false).unwrap();
    let error = Coverage::open(
        f.authority.clone(),
        &f.root,
        &f.session,
        &instance,
        &f.lifecycle,
        &f.binding,
        None,
    )
    .err()
    .unwrap();
    assert!(busy(&error));
    assert!(!f.directory(&instance).exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_missing_open_with_source_records_is_not_an_unpublished_startup
#[test]
fn missing_open_with_source_records_is_not_an_unpublished_startup() {
    let f = Fixture::new();
    let abandoned = f.directory(&uid());
    turn_sources::private_mkdir(&abandoned.join("attempts")).unwrap();
    turn_sources::exclusive(
        &abandoned.join("attempts/1.json"),
        &json(r#"{"unknown":true}"#),
        4096,
    )
    .unwrap();
    assert!(f.inventory().is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_empty_session_or_orphan_lock_qualification_survives_later_same_session_start
#[test]
fn empty_session_or_orphan_lock_qualification_survives_later_same_session_start() {
    let f = Fixture::new();
    f.authority.observe_with(verified());
    for initial in ["empty-session-directory", "orphan-creation-lock"] {
        let session = uid();
        let directory = stream::directory(&f.base, &session, &uid())
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let name = directory.file_name().unwrap().to_str().unwrap().to_owned();
        if initial == "empty-session-directory" {
            turn_sources::private_mkdir(&directory).unwrap();
        } else {
            drop(
                turn_sources::private_lock(
                    &f.base
                        .join("omp-stream-session-locks")
                        .join(format!("{name}.lock")),
                    false,
                    false,
                )
                .unwrap(),
            );
        }
        let before = f.inventory().unwrap();
        assert!(
            get(&before, "streams")
                .as_array()
                .unwrap()
                .iter()
                .any(|row| {
                    text(row, "session_sha256") == name
                        && get(row, "pending") == &json(r#"[{"code":"startup-incomplete"}]"#)
                })
        );
        let instance = uid();
        let mut value = f.request();
        set(
            value.get_mut("request").unwrap(),
            "session",
            string(&session),
        );
        let mut data = frame("REQUEST", 1, &value);
        data.extend(frame("FINAL", 2, &f.final_()));
        assert_eq!(f.run(&instance, &session, data, f.binding.clone()), 0);
        let marker = read(&directory.join("startup-incomplete.json"));
        assert_eq!(
            encoded(&marker),
            encoded(&json(&format!(
                r#"{{"schema":"agent-memory/omp-session-startup-incomplete-v1","session_sha256":"{name}"}}"#
            )))
        );
        let after = f.inventory().unwrap();
        let healthy = row(&after, &instance);
        assert_eq!(get(healthy, "closed"), &Value::Bool(true));
        assert_eq!(get(healthy, "pending"), &Value::Array(vec![]));
        assert!(
            get(&after, "streams")
                .as_array()
                .unwrap()
                .iter()
                .any(|row| {
                    text(row, "session_sha256") == name
                        && get(row, "instance_id") == &Value::Null
                        && get(row, "pending") == &json(r#"[{"code":"startup-incomplete"}]"#)
                })
        );
        assert!(
            get(&after, "pending")
                .as_array()
                .unwrap()
                .iter()
                .any(|item| text(item, "session_sha256") == name)
        );
        assert!(stream::validate_drain(&*f.authority, &f.root, &after, Some(&f.base)).is_err());
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_startup_accepts_real_32_hex_installation_id_and_refuses_uuid_form
#[test]
fn startup_accepts_real_32_hex_installation_id_and_refuses_uuid_form() {
    let f = Fixture::new();
    let accepted =
        stream::binding(&f.binding, &f.root, &f.session, &f.instance, &f.lifecycle).unwrap();
    assert!(std::ptr::eq(accepted, &f.binding));
    let mut invalid = f.binding.clone();
    set(&mut invalid, "installation_id", string(&uid()));
    assert!(stream::binding(&invalid, &f.root, &f.session, &f.instance, &f.lifecycle).is_err());
    let mut open = f.coverage.open_value().clone();
    set(
        &mut open,
        "installation_id",
        get(&invalid, "installation_id").clone(),
    );
    assert!(stream::open_value(&open, f.coverage.directory()).is_err());
    let instance = uid();
    assert_eq!(f.run(&instance, &f.session, Vec::new(), invalid), 1);
    assert!(!f.directory(&instance).exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_inventory_refuses_orphan_outcome
#[test]
fn inventory_refuses_orphan_outcome() {
    let f = Fixture::new();
    let path = f.coverage.directory().join("outcomes");
    turn_sources::private_mkdir(&path).unwrap();
    turn_sources::exclusive(&path.join("1.json"), &json(r#"{"unbound":true}"#), 4096).unwrap();
    assert!(f.inventory().is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_malformed_request_then_valid_source_and_sender_refusal_all_close_accounted
#[test]
fn malformed_request_then_valid_source_and_sender_refusal_all_close_accounted() {
    let f = Fixture::new();
    let valid = f.request();
    let mut refused = json(r#"{"action":"ingest","event_ordinal":1,"reason":"capture-refused"}"#);
    set(&mut refused, "identity", f.known(&["native-a"]));
    let mut data = b"AM249 REQUEST 1 1 -\n{".to_vec();
    data.extend(frame("REQUEST", 2, &valid));
    data.extend(frame("REFUSED-ATTEMPT", 3, &refused));
    data.extend(frame("FINAL", 4, &Fixture::last(3, 1, 1, None)));
    let instance = uid();
    let calls = Arc::new(AtomicUsize::new(0));
    f.authority.observe_with(counting_verified(calls.clone()));
    let initial = f.authority.attests();
    assert_eq!(f.run(&instance, &f.session, data, f.binding.clone()), 0);
    let directory = f.directory(&instance);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // OPEN + 3 source frames + FINAL.
    assert_eq!(f.authority.attests(), initial + 5);
    let first = read(&directory.join("attempts/1.json"));
    let third = read(&directory.join("attempts/3.json"));
    assert_eq!(
        (text(&first, "disposition"), text(&first, "frame_kind")),
        ("refused", "REQUEST")
    );
    assert_eq!(
        (text(&third, "disposition"), text(&third, "frame_kind")),
        ("refused", "REFUSED-ATTEMPT")
    );
    assert_eq!(
        text(&read(&directory.join("outcomes/2.json")), "outcome"),
        "verified"
    );
    assert_eq!(
        int(get(&read(&directory.join("closed.json")), "refused_count")),
        1
    );
    let inventory = f.inventory().unwrap();
    let pending = codes(get(row(&inventory, &instance), "pending"));
    assert!(pending.contains(&"invalid-request".to_owned()));
    assert!(pending.contains(&"capture-refused".to_owned()));
    // EOF/drain exit released the lifetime owner lock.
    drop(turn_sources::private_lock(&directory.join("live.lock"), false, false).unwrap());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_run_drains_accepted_frames_only_after_valid_final_and_eof
#[test]
fn run_drains_accepted_frames_only_after_valid_final_and_eof() {
    let f = Fixture::new();
    f.authority.observe_with(verified());
    let mut data = frame("REQUEST", 1, &f.request());
    data.extend(frame("FINAL", 2, &f.final_()));
    let instance = uid();
    assert_eq!(f.run(&instance, &f.session, data, f.binding.clone()), 0);
    assert!(f.directory(&instance).join("closed.json").exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_startup_typed_busy_retries_before_open_and_ready
#[test]
fn startup_typed_busy_retries_before_open_and_ready() {
    let f = Fixture::new();
    let instance = uid();
    let directory = f.directory(&instance);
    let mut events = 0;
    let binding = f.binding.clone();
    let mut startup = |_: &Path, _: &str, _: &str, _: &str| -> Result<Option<Value>> {
        events += 1;
        if events == 1 {
            return Err(busy_error());
        }
        assert!(!directory.join("open.json").exists());
        Ok(Some(binding.clone()))
    };
    let (timing, sleeps) = quick_timing();
    let data = frame("FINAL", 1, &Fixture::last(0, 0, 0, None));
    let result = stream::run(
        &f.root,
        &f.session,
        &instance,
        &f.lifecycle,
        &mut startup,
        RunOptions {
            authority: f.authority.clone(),
            receipt: None,
            input: Box::new(Cursor::new(data)),
            writer: Arc::new(Responses::default()),
            clocks: darwin_clocks,
            timing,
        },
    )
    .unwrap();
    assert_eq!(result, 0);
    assert_eq!(events, 2);
    assert_eq!(
        *sleeps.lock().unwrap().last().unwrap(),
        stream::BUSY_BACKOFF
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_startup_busy_exhaustion_is_pending_without_open_or_close
#[test]
fn startup_busy_exhaustion_is_pending_without_open_or_close() {
    let f = Fixture::new();
    let instance = uid();
    let responses = Arc::new(Responses::default());
    let script = Script::new(&[0, 30_000_000_000]);
    let mut startup =
        |_: &Path, _: &str, _: &str, _: &str| -> Result<Option<Value>> { Err(busy_error()) };
    let result = stream::run(
        &f.root,
        &f.session,
        &instance,
        &f.lifecycle,
        &mut startup,
        RunOptions {
            authority: f.authority.clone(),
            receipt: None,
            input: Box::new(Cursor::new(Vec::new())),
            writer: responses.clone(),
            clocks: darwin_clocks,
            timing: script.timing(),
        },
    )
    .unwrap();
    assert_eq!(result, 1);
    let values = responses.values();
    assert_eq!(values.len(), 1);
    assert_eq!((values[0].0.as_str(), values[0].1), ("REFUSED", 0));
    assert_eq!(
        encoded(&values[0].2),
        encoded(&json(
            r#"{"code":"busy-exhausted","outcome":"unknown","status":"pending"}"#
        ))
    );
    assert!(!f.directory(&instance).exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_run_eof_partial_or_after_final_cannot_close
#[test]
fn run_eof_partial_or_after_final_cannot_close() {
    let f = Fixture::new();
    f.authority.observe_with(verified());
    let value = f.request();
    let request = frame("REQUEST", 1, &value);
    let last = frame("FINAL", 2, &f.final_());
    let cases = [
        request.clone(),
        [request.clone(), last[..last.len() - 1].to_vec()].concat(),
        [request.clone(), last.clone(), request.clone()].concat(),
        [frame("REQUEST", 2, &value), last.clone()].concat(),
    ];
    for data in cases {
        let instance = uid();
        assert_eq!(f.run(&instance, &f.session, data, f.binding.clone()), 1);
        assert!(!f.directory(&instance).join("closed.json").exists());
    }
}

struct BrokenOutput;
impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_broken_output_does_not_prevent_accepted_source_drain
#[test]
fn broken_output_does_not_prevent_accepted_source_drain() {
    let f = Fixture::new();
    f.authority.observe_with(verified());
    let mut data = frame("REQUEST", 1, &f.request());
    data.extend(frame("FINAL", 2, &f.final_()));
    let instance = uid();
    let writer = Arc::new(ResponseWriter::start(
        Output::Stream(Box::new(BrokenOutput)),
        &instance,
    ));
    assert_eq!(
        f.run_with(&instance, &f.session, data, f.binding.clone(), writer),
        0
    );
    assert!(f.directory(&instance).join("closed.json").exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_invalid_clock_control_revokes_wake_but_keeps_reading_source
#[test]
fn invalid_clock_control_revokes_wake_but_keeps_reading_source() {
    let f = Fixture::new();
    f.authority.observe_with(verified());
    let mut data = frame(
        "CLOCK-ACK",
        0,
        &json(&format!(r#"{{"nonce":"{}","j0":"1","j3":"2"}}"#, uid())),
    );
    data.extend(frame("REQUEST", 1, &f.request()));
    data.extend(frame("FINAL", 2, &f.final_()));
    let instance = uid();
    assert_eq!(f.run(&instance, &f.session, data, f.binding.clone()), 0);
    let directory = f.directory(&instance);
    assert_eq!(
        text(&read(&directory.join("outcomes/1.json")), "outcome"),
        "verified"
    );
    assert!(directory.join("closed.json").exists());
}

// obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_cancelled_superseded_and_invalid_clock_acks_never_pin_source_close
#[test]
fn cancelled_superseded_and_invalid_clock_acks_never_pin_source_close() {
    let f = Fixture::new();
    let (first, second) = (uid(), uid());
    let control = |kind: &str, body: String| frame(kind, 0, &json(&body));
    let clock =
        |nonce: &str| format!(r#"{{"nonce":"{nonce}","j0":"100","j_deadline":"950000100"}}"#);
    let ack = |nonce: &str, j3: &str| format!(r#"{{"nonce":"{nonce}","j0":"100","j3":"{j3}"}}"#);
    let mut data = [
        control("CLOCK", clock(&first)),
        control("CANCEL", format!(r#"{{"nonce":"{first}"}}"#)),
        control("CLOCK-ACK", ack(&first, "200")),
        control("CLOCK", clock(&first)),
        control("CLOCK", clock(&second)),
        control("CLOCK-ACK", ack(&first, "200")),
        control("CLOCK-ACK", ack(&second, "99")),
    ]
    .concat();
    data.extend(frame("REQUEST", 1, &f.request()));
    data.extend(frame("FINAL", 2, &f.final_()));
    let calls = Arc::new(AtomicUsize::new(0));
    let windows = Arc::new(Mutex::new(Vec::new()));
    let (counted, seen) = (calls.clone(), windows.clone());
    f.authority.observe_with(observer(move |_, _, call| {
        counted.fetch_add(1, Ordering::SeqCst);
        seen.lock().unwrap().push(call.delivery_window.is_none());
        (call.on_started)()?;
        Ok(json(r#"{"verified":true}"#))
    }));
    let instance = uid();
    assert_eq!(f.run(&instance, &f.session, data, f.binding.clone()), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(*windows.lock().unwrap(), vec![true]);
    assert!(f.directory(&instance).join("closed.json").exists());
}

// ----------------------------------------------------- LinuxImageControls

struct Probe {
    clock: String,
    allowed: bool,
    mode: u32,
    size: u64,
    reads: Mutex<VecDeque<Vec<u8>>>,
    changed: bool,
    fstats: AtomicUsize,
    namespaces: Mutex<VecDeque<TimeNamespace>>,
    closed: Mutex<Vec<i32>>,
    close_during_hash: bool,
    image: Mutex<Option<std::sync::Weak<LinuxNativeImage<Probe>>>>,
    // valid() controls
    exited: AtomicBool,
    parent: AtomicUsize,
    start: AtomicU64,
    own_ns: Mutex<TimeNamespace>,
    parent_ns: Mutex<TimeNamespace>,
    fresh_ino: AtomicU64,
}
impl Probe {
    fn new() -> Self {
        Self {
            clock: stream::QUALIFIED_CLOCK.into(),
            allowed: true,
            mode: 0o100755,
            size: 3,
            reads: Mutex::new(VecDeque::from([b"abc".to_vec(), Vec::new()])),
            changed: false,
            fstats: AtomicUsize::new(0),
            namespaces: Mutex::new(VecDeque::new()),
            closed: Mutex::new(Vec::new()),
            close_during_hash: false,
            image: Mutex::new(None),
            exited: AtomicBool::new(false),
            parent: AtomicUsize::new(44),
            start: AtomicU64::new(99),
            own_ns: Mutex::new(TimeNamespace::Time(1, 2)),
            parent_ns: Mutex::new(TimeNamespace::Time(1, 3)),
            fresh_ino: AtomicU64::new(2),
        }
    }
    fn stat(&self, mtime: i128, ino: u64) -> ImageStat {
        ImageStat {
            mode: self.mode,
            size: self.size,
            dev: 1,
            ino,
            mtime_ns: mtime,
            ctime_ns: 4,
        }
    }
}
fn sha256_abc() -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b"abc")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl ImageProbe for Probe {
    type Handle = i32;
    fn clock_identity(&self) -> &str {
        &self.clock
    }
    fn allowed(&self, digest: &str) -> bool {
        self.allowed && digest == sha256_abc()
    }
    fn parent_pid(&self) -> i32 {
        self.parent.load(Ordering::SeqCst) as i32
    }
    fn own_pid(&self) -> i32 {
        45
    }
    fn pidfd_open(&self, _pid: i32) -> std::io::Result<i32> {
        Ok(102)
    }
    fn pidfd_exited(&self, _pidfd: &i32) -> std::io::Result<bool> {
        Ok(self.exited.load(Ordering::SeqCst))
    }
    fn process_start(&self, _pid: i32) -> std::io::Result<u64> {
        Ok(self.start.load(Ordering::SeqCst))
    }
    fn time_namespace(&self, pid: i32) -> std::io::Result<TimeNamespace> {
        if let Some(value) = self.namespaces.lock().unwrap().pop_front() {
            return Ok(value);
        }
        Ok(if pid == 45 {
            *self.own_ns.lock().unwrap()
        } else {
            *self.parent_ns.lock().unwrap()
        })
    }
    fn open_image(&self, _pid: i32) -> std::io::Result<i32> {
        Ok(101)
    }
    fn fstat(&self, _handle: &i32) -> std::io::Result<ImageStat> {
        let call = self.fstats.fetch_add(1, Ordering::SeqCst);
        let mtime = if self.changed && call == 1 { 5 } else { 3 };
        Ok(self.stat(mtime, 2))
    }
    fn stat_image(&self, _pid: i32) -> std::io::Result<ImageStat> {
        Ok(self.stat(3, self.fresh_ino.load(Ordering::SeqCst)))
    }
    fn read(&self, _handle: &mut i32, _count: usize) -> std::io::Result<Vec<u8>> {
        if self.close_during_hash {
            if let Some(image) = self
                .image
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|w| w.upgrade())
            {
                image.close();
            }
        }
        Ok(self.reads.lock().unwrap().pop_front().unwrap_or_default())
    }
    fn close(&self, handle: i32) {
        self.closed.lock().unwrap().push(handle);
    }
}
fn observe(probe: Probe) -> (Arc<LinuxNativeImage<Probe>>, Arc<Probe>) {
    let probe = Arc::new(probe);
    let image = Arc::new(LinuxNativeImage::unobserved(probe.clone()));
    *probe.image.lock().unwrap() = Some(Arc::downgrade(&image));
    image.observe();
    (image, probe)
}
fn closed(probe: &Probe) -> Vec<i32> {
    let mut values = probe.closed.lock().unwrap().clone();
    values.sort_unstable();
    values
}

// obligation: tests/contracts/test_omp_finite_stream.py::LinuxImageControls.test_self_check_rejects_unsupported_interpreters_and_clocks
#[test]
fn self_check_rejects_unsupported_clocks() {
    // The CPython interpreter/version gate is replaced by the Rust clock identity.
    for clock in [
        "macos:mach_absolute_time",
        "linux:clock_gettime(CLOCK_HIGHRES)",
        "linux:clock_gettime(CLOCK_MONOTONIC_RAW)",
        "",
    ] {
        let mut probe = Probe::new();
        probe.clock = clock.into();
        let (image, probe) = observe(probe);
        assert!(!image.observed());
        assert!(closed(&probe).is_empty());
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::LinuxImageControls.test_image_hash_size_type_and_changes_refuse
#[test]
fn image_hash_size_type_and_changes_refuse() {
    let cases: Vec<Box<dyn Fn(&mut Probe)>> = vec![
        Box::new(|p| p.allowed = false),
        Box::new(|p| p.mode = 0o040755),
        Box::new(|p| p.size = 0),
        Box::new(|p| p.size = stream::MAX_NATIVE_IMAGE_BYTES + 1),
        Box::new(|p| *p.reads.lock().unwrap() = VecDeque::from([b"ab".to_vec(), Vec::new()])),
        Box::new(|p| *p.reads.lock().unwrap() = VecDeque::from([b"abc".to_vec(), b"x".to_vec()])),
        Box::new(|p| p.changed = true),
    ];
    for case in cases {
        let mut probe = Probe::new();
        case(&mut probe);
        let (image, probe) = observe(probe);
        assert!(!image.observed());
        assert_eq!(closed(&probe), vec![101, 102]);
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::LinuxImageControls.test_namespace_classification_and_mixed_absence
#[test]
fn namespace_classification_and_mixed_absence() {
    use std::io::{Error, ErrorKind};
    let present = |_: &str| Ok((7, 8));
    assert_eq!(
        stream::time_namespace_with(present).unwrap(),
        TimeNamespace::Time(7, 8)
    );
    let absent = |name: &str| {
        if name == "mnt" {
            Ok((7, 8))
        } else {
            Err(Error::from(ErrorKind::NotFound))
        }
    };
    assert_eq!(
        stream::time_namespace_with(absent).unwrap(),
        TimeNamespace::Absent
    );
    let ambiguous = |name: &str| {
        if name == "time" {
            Err(Error::from(ErrorKind::NotFound))
        } else {
            Ok((7, 8))
        }
    };
    assert_eq!(
        stream::time_namespace_with(ambiguous).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let denied = |_: &str| Err(Error::from(ErrorKind::PermissionDenied));
    assert_eq!(
        stream::time_namespace_with(denied).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    let probe = Probe::new();
    *probe.namespaces.lock().unwrap() =
        VecDeque::from([TimeNamespace::Absent, TimeNamespace::Time(1, 3)]);
    let (image, probe) = observe(probe);
    assert!(!image.observed());
    assert_eq!(closed(&probe), vec![102]);
}

// obligation: tests/contracts/test_omp_finite_stream.py::LinuxImageControls.test_valid_rechecks_process_namespaces_and_image
#[test]
fn valid_rechecks_process_namespaces_and_image() {
    let (image, probe) = observe(Probe::new());
    assert!(image.observed());
    for case in [
        "ok",
        "exited",
        "reparented",
        "start",
        "own-ns",
        "parent-ns",
        "image",
    ] {
        probe.exited.store(case == "exited", Ordering::SeqCst);
        probe
            .parent
            .store(if case == "reparented" { 43 } else { 44 }, Ordering::SeqCst);
        probe
            .start
            .store(if case == "start" { 100 } else { 99 }, Ordering::SeqCst);
        *probe.own_ns.lock().unwrap() =
            TimeNamespace::Time(1, if case == "own-ns" { 9 } else { 2 });
        *probe.parent_ns.lock().unwrap() =
            TimeNamespace::Time(1, if case == "parent-ns" { 9 } else { 3 });
        probe
            .fresh_ino
            .store(if case == "image" { 9 } else { 2 }, Ordering::SeqCst);
        assert_eq!(image.valid(), case == "ok", "{case}");
        if case == "ok" {
            // Arithmetic control only: the stand-in is not provenance.
            let script = Script::new(&[11, 11]);
            let owner = ClockWindows::new(
                &uid(),
                Arc::new(|| true),
                Platform::Linux,
                script.monotonic(),
                Some(image.clone()),
            );
            let nonce = uid();
            let responses = Responses::default();
            let deadline = stream::MARGIN_NS + 100;
            owner
                .control(
                    "CLOCK",
                    &json(&format!(
                        r#"{{"nonce":"{nonce}","j0":"0","j_deadline":"{deadline}"}}"#
                    )),
                    &responses,
                )
                .unwrap();
            owner
                .control(
                    "CLOCK-ACK",
                    &json(&format!(r#"{{"nonce":"{nonce}","j0":"0","j3":"1"}}"#)),
                    &responses,
                )
                .unwrap();
            let window = owner.window(&nonce).unwrap();
            assert_eq!(window.lower_offset(), 10);
            script.at(110);
            assert!(!window.reserve());
            script.at(109);
            assert!(window.reserve());
        }
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::LinuxImageControls.test_close_during_hash_does_not_publish_or_leak
#[test]
fn close_during_hash_does_not_publish_or_leak() {
    let mut probe = Probe::new();
    probe.close_during_hash = true;
    let (image, probe) = observe(probe);
    assert!(!image.observed());
    assert!(image.closed());
    assert_eq!(closed(&probe), vec![101, 102]);
}

// -------------------------------------------------------- DeadlineWindows

struct Windows {
    instance: String,
    nonce: String,
    responses: Arc<Responses>,
    eligible: Arc<AtomicBool>,
    script: Arc<Script>,
    clocks: ClockWindows,
}
impl Windows {
    fn new(platform: Platform, image: Option<Arc<dyn NativeImage>>) -> Self {
        let responses = Arc::new(Responses::default());
        let eligible = Arc::new(AtomicBool::new(true));
        let script = Script::new(&[]);
        let (live, flag) = (responses.clone(), eligible.clone());
        let instance = uid();
        let clocks = ClockWindows::new(
            &instance,
            Arc::new(move || flag.load(Ordering::SeqCst) && live.live()),
            platform,
            script.monotonic(),
            image,
        );
        Self {
            instance,
            nonce: uid(),
            responses,
            eligible,
            script,
            clocks,
        }
    }
    fn darwin() -> Self {
        Self::new(Platform::Darwin, None)
    }
    fn clock(&self, nonce: &str, j0: &str, deadline: &str) -> Result<()> {
        self.clocks.control(
            "CLOCK",
            &json(&format!(
                r#"{{"nonce":"{nonce}","j0":"{j0}","j_deadline":"{deadline}"}}"#
            )),
            &*self.responses,
        )
    }
    fn ack(&self, nonce: &str, j0: &str, j3: &str) -> Result<()> {
        self.clocks.control(
            "CLOCK-ACK",
            &json(&format!(r#"{{"nonce":"{nonce}","j0":"{j0}","j3":"{j3}"}}"#)),
            &*self.responses,
        )
    }
    fn cancel(&self, nonce: &str) -> Result<()> {
        self.clocks.control(
            "CANCEL",
            &json(&format!(r#"{{"nonce":"{nonce}"}}"#)),
            &*self.responses,
        )
    }
    fn calibrated(
        &self,
        j3: &str,
        deadline: &str,
    ) -> Option<Arc<stream::VerifiedNativeDeliveryWindow>> {
        self.script.queue(&[1000, 1010]);
        self.clock(&self.nonce, "100", deadline).unwrap();
        self.ack(&self.nonce, "100", j3).unwrap();
        self.clocks.window(&self.nonce)
    }
    fn window(&self) -> Arc<stream::VerifiedNativeDeliveryWindow> {
        self.calibrated("200", "950000100").unwrap()
    }
}
struct StandIn {
    observed: bool,
    valid: bool,
}
impl NativeImage for StandIn {
    fn observed(&self) -> bool {
        self.observed
    }
    fn valid(&self) -> bool {
        self.valid
    }
    fn close(&self) {}
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_different_epochs_use_lower_offset_and_exact_rate_subtraction
#[test]
fn different_epochs_use_lower_offset_and_exact_rate_subtraction() {
    let w = Windows::darwin();
    let window = w.window();
    assert_eq!(window.lower_offset(), 810);
    let remaining = window.j_deadline() - window.j3();
    let drift = (164 * remaining + 9999) / 10000;
    let deadline = (window.j_deadline() as i128 + window.lower_offset() - drift as i128) as u64;
    w.script.at(deadline - stream::MARGIN_NS as u64);
    assert!(!window.reserve());
    w.script.at(deadline - stream::MARGIN_NS as u64 - 1);
    assert!(window.reserve());
    assert!(!window.reserve());
    assert_eq!(window.support(), "provisional-unqualified");
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_cancel_eof_broken_output_and_recipient_revoke_window
#[test]
fn cancel_eof_broken_output_and_recipient_revoke_window() {
    let w = Windows::darwin();
    let window = w.window();
    w.script.at(0);
    w.eligible.store(false, Ordering::SeqCst);
    assert!(!window.reserve());
    w.eligible.store(true, Ordering::SeqCst);
    w.responses.dead.store(true, Ordering::SeqCst);
    assert!(!window.reserve());
    w.responses.dead.store(false, Ordering::SeqCst);
    w.cancel(&w.nonce).unwrap();
    assert!(!window.reserve());
    w.clocks.end();
    assert!(w.clocks.window(&w.nonce).is_none());
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_late_calibration_and_nonce_mismatch_cannot_mint
#[test]
fn late_calibration_and_nonce_mismatch_cannot_mint() {
    let w = Windows::darwin();
    assert!(w.calibrated("950000101", "950000100").is_none());
    assert!(w.ack(&uid(), "100", "200").is_err());
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_unverified_linux_image_cannot_reserve
#[test]
fn unverified_linux_image_cannot_reserve() {
    // An unverified Linux image cannot borrow the provisional Darwin bound.
    let w = Windows::new(
        Platform::Linux,
        Some(Arc::new(StandIn {
            observed: true,
            valid: false,
        })),
    );
    let window = w.window();
    w.script.at(0);
    assert!(!window.reserve());
    // Without an observed image no Linux window is minted at all.
    let w = Windows::new(Platform::Linux, None);
    assert!(w.calibrated("200", "950000100").is_none());
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_linux_same_clock_envelope_is_strict_and_single_use
#[test]
fn linux_same_clock_envelope_is_strict_and_single_use() {
    // Arithmetic control only: the stand-in is not provenance or qualification.
    let w = Windows::new(
        Platform::Linux,
        Some(Arc::new(StandIn {
            observed: true,
            valid: true,
        })),
    );
    let window = w.window();
    let boundary =
        (window.j_deadline() as i128 + window.lower_offset()) as u64 - stream::MARGIN_NS as u64;
    w.script.at(boundary);
    assert!(!window.reserve());
    w.script.at(boundary - 1);
    assert!(window.reserve());
    assert!(!window.reserve());
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_same_clock_causal_envelope_never_overstates_remaining_time
#[test]
fn same_clock_causal_envelope_never_overstates_remaining_time() {
    // Every original integer schedule through the real clock/window. The image
    // stand-in qualifies arithmetic only, not Linux provenance.
    let unit = stream::MARGIN_NS as i64;
    let epoch = 10 * unit;
    let nonce = uid();
    let responses = Responses::default();
    for offset in -3i64..4 {
        for j0 in 0i64..4 {
            for p1_at in j0..5 {
                for p2_at in p1_at..5 {
                    for j3 in p2_at..5 {
                        let lower = p2_at + offset - j3;
                        let upper = p1_at + offset - j0;
                        assert!(lower <= upper);
                        for now in j3..6 {
                            for deadline in j3..7 {
                                if now + offset + 1 < deadline + lower {
                                    assert!(now + 1 < deadline);
                                }
                                let script = Script::new(&[
                                    (epoch + (p1_at + offset) * unit) as u64,
                                    (epoch + (p2_at + offset) * unit) as u64,
                                ]);
                                let owner = ClockWindows::new(
                                    &uid(),
                                    Arc::new(|| true),
                                    Platform::Linux,
                                    script.monotonic(),
                                    Some(Arc::new(StandIn {
                                        observed: true,
                                        valid: true,
                                    })),
                                );
                                owner
                                    .control(
                                        "CLOCK",
                                        &json(&format!(
                                            r#"{{"nonce":"{nonce}","j0":"{}","j_deadline":"{}"}}"#,
                                            j0 * unit,
                                            deadline * unit
                                        )),
                                        &responses,
                                    )
                                    .unwrap();
                                owner
                                    .control(
                                        "CLOCK-ACK",
                                        &json(&format!(
                                            r#"{{"nonce":"{nonce}","j0":"{}","j3":"{}"}}"#,
                                            j0 * unit,
                                            j3 * unit
                                        )),
                                        &responses,
                                    )
                                    .unwrap();
                                let window = owner.window(&nonce).unwrap();
                                script.at((epoch + (now + offset) * unit) as u64);
                                let reserved = window.reserve();
                                assert_eq!(reserved, now + offset + 1 < deadline + lower);
                                if reserved {
                                    assert!(now + 1 < deadline);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_replayed_old_nonce_cannot_remint_after_new_clock
#[test]
fn replayed_old_nonce_cannot_remint_after_new_clock() {
    let w = Windows::darwin();
    let window = w.window();
    w.script.queue(&[1000, 1010, 1020]);
    w.clock(&uid(), "100", "950000100").unwrap();
    assert!(w.clock(&w.nonce, "100", "950000100").is_err());
    w.script.at(0);
    assert!(!window.reserve());
}

struct PausedProbe {
    started: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl ImageProbe for PausedProbe {
    type Handle = i32;
    fn clock_identity(&self) -> &str {
        stream::QUALIFIED_CLOCK
    }
    fn allowed(&self, _: &str) -> bool {
        false
    }
    fn parent_pid(&self) -> i32 {
        44
    }
    fn own_pid(&self) -> i32 {
        45
    }
    fn pidfd_open(&self, _: i32) -> std::io::Result<i32> {
        if let Some(started) = self.started.lock().unwrap().take() {
            let _ = started.send(());
        }
        let _ = self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10));
        Ok(102)
    }
    fn pidfd_exited(&self, _: &i32) -> std::io::Result<bool> {
        Ok(true)
    }
    fn process_start(&self, _: i32) -> std::io::Result<u64> {
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn time_namespace(&self, _: i32) -> std::io::Result<TimeNamespace> {
        Ok(TimeNamespace::Absent)
    }
    fn open_image(&self, _: i32) -> std::io::Result<i32> {
        Ok(101)
    }
    fn fstat(&self, _: &i32) -> std::io::Result<ImageStat> {
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn stat_image(&self, _: i32) -> std::io::Result<ImageStat> {
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn read(&self, _: &mut i32, _: usize) -> std::io::Result<Vec<u8>> {
        Ok(Vec::new())
    }
    fn close(&self, _: i32) {}
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_linux_hash_in_progress_does_not_block_clock_reply
#[test]
fn linux_hash_in_progress_does_not_block_clock_reply() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let image = LinuxNativeImage::start(Arc::new(PausedProbe {
        started: Mutex::new(Some(started_tx)),
        release: Mutex::new(release_rx),
    }));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let w = Windows::new(Platform::Linux, Some(image));
    assert!(w.calibrated("200", "950000100").is_none());
    assert!(w.responses.kinds().contains(&"CLOCK-REPLY".to_owned()));
    release_tx.send(()).unwrap();
    w.clocks.end();
}

fn replace_or_revoke(w: &Windows, control: &str) -> Option<String> {
    match control {
        "CANCEL" => {
            w.cancel(&w.nonce).unwrap();
            None
        }
        "CLOCK" => {
            let replacement = uid();
            w.clock(&replacement, "100", "950000100").unwrap();
            w.ack(&replacement, "100", "200").unwrap();
            Some(replacement)
        }
        "revoke" => {
            w.clocks.revoke();
            None
        }
        _ => {
            w.clocks.end();
            None
        }
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_cancel_replacement_revoke_and_end_before_reservation_refuse
#[test]
fn cancel_replacement_revoke_and_end_before_reservation_refuse() {
    for control in ["CANCEL", "CLOCK", "revoke", "end"] {
        let w = Windows::darwin();
        let window = w.window();
        w.script.at(0);
        let replacement = replace_or_revoke(&w, control);
        assert!(!window.reserve(), "{control}");
        if let Some(replacement) = replacement {
            assert!(w.clocks.window(&replacement).unwrap().reserve());
        }
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_cancel_replacement_revoke_and_end_after_reservation_do_not_reopen
#[test]
fn cancel_replacement_revoke_and_end_after_reservation_do_not_reopen() {
    for control in ["CANCEL", "CLOCK", "revoke", "end"] {
        let w = Windows::darwin();
        let window = w.window();
        w.script.at(0);
        assert!(window.reserve());
        let replacement = replace_or_revoke(&w, control);
        assert!(!window.reserve(), "{control}");
        if let Some(replacement) = replacement {
            assert!(w.clocks.window(&replacement).unwrap().reserve());
        }
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_concurrent_reservations_spend_window_once
#[test]
fn concurrent_reservations_spend_window_once() {
    let w = Windows::darwin();
    let window = w.window();
    w.script.at(0);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let (window, barrier) = (window.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                window.reserve()
            })
        })
        .collect();
    let mut results: Vec<bool> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    results.sort_unstable();
    assert_eq!(results, vec![false, true]);
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_nonce_cannot_be_replayed_after_cancel_or_reservation
#[test]
fn nonce_cannot_be_replayed_after_cancel_or_reservation() {
    for action in ["cancel", "reserve", "revoke"] {
        let w = Windows::darwin();
        let window = w.window();
        w.script.at(0);
        match action {
            "cancel" => w.cancel(&w.nonce).unwrap(),
            "reserve" => assert!(window.reserve()),
            _ => w.clocks.revoke(),
        }
        assert!(w.clock(&w.nonce, "100", "950000100").is_err());
        assert!(!window.reserve());
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::DeadlineWindows.test_windows_cannot_be_minted_or_serialized_by_json
#[test]
fn windows_cannot_be_minted_or_serialized_by_json() {
    // VerifiedNativeDeliveryWindow has no public constructor, Clone or
    // serialization (a compile_fail doctest pins construction); only the
    // owner's exact nonce lookup returns it.
    let w = Windows::darwin();
    let _ = w.window();
    assert!(w.clocks.window(&uid()).is_none());
    assert_eq!(w.clocks.instance(), w.instance);
}

// ---------------------------------------------------------- WakeDecisions

impl Fixture {
    fn seal(&self, armed: bool, ordinal: u64) -> Value {
        let mut value = self.request_with("seal", &self.known(&["native-a"]));
        set(&mut value, "event_ordinal", json(&ordinal.to_string()));
        let payload = value
            .get_mut("request")
            .and_then(|r| r.get_mut("payload"))
            .unwrap();
        set(payload, "request_wake", Value::Bool(armed));
        value
    }
    fn execute_seal(
        &mut self,
        observe: Arc<Observer>,
        value: &Value,
        sequence: u64,
        nonce: Option<&str>,
    ) {
        let executor = self.executor(observe, Timing::system());
        executor
            .execute(
                &mut self.coverage,
                "REQUEST",
                sequence,
                nonce,
                &encoded(value),
            )
            .unwrap();
    }
    fn decision_path(&self, sequence: u64) -> PathBuf {
        stream::wake_decision_directory(self.coverage.directory())
            .unwrap()
            .join(format!("{sequence}.json"))
    }
    fn readback(&self) -> Result<Value> {
        let inventory = self.inventory()?;
        stream::wake_decisions(&*self.authority, &self.root, &inventory, Some(&self.base))
    }
    fn count(&self, status: &str) -> u64 {
        int(get(get(&self.readback().unwrap(), "counts"), status))
    }
    fn roots(&self) -> PathBuf {
        let session = self
            .coverage
            .directory()
            .parent()
            .unwrap()
            .file_name()
            .unwrap();
        self.base
            .join("omp-source-v1")
            .join(session)
            .join("root-wakes")
    }
}
fn returning(result: Value) -> Arc<Observer> {
    observer(move |_, _, call| {
        (call.on_started)()?;
        Ok(result.clone())
    })
}
/// The transport's role in an offered continuation, reduced to its retained
/// root record: write the root wake and return the continuation result.
fn offering(roots: PathBuf, session: String, mismatch: bool) -> Arc<Observer> {
    observer(move |_, _, call| {
        (call.on_started)()?;
        let root = uid();
        let request_id = uid();
        let path = roots.join(format!("{root}.json"));
        turn_sources::private_mkdir(&roots)?;
        turn_sources::exclusive(
            &path,
            &json(&format!(
                r#"{{"schema":"agent-memory/omp-root-wake-v1","session":"{session}","human_root":"{root}","native_user_entry":"native-a","request_id":"{request_id}","status":"requested"}}"#
            )),
            4096,
        )?;
        if let Some(completion) = call.source_completion {
            completion.offer_root_wake(&path);
        }
        let returned = if mismatch { uid() } else { request_id };
        Ok(json(&format!(
            r#"{{"verified":true,"continue":true,"status":"observed","request_id":"{returned}"}}"#
        )))
    })
}
impl Fixture {
    fn offered(&mut self) {
        let observe = offering(self.roots(), self.session.clone(), false);
        let seal = self.seal(true, 1);
        self.execute_seal(observe, &seal, 1, Some(&uid()));
    }
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_offered_binds_actual_root_after_response_without_changing_old_evidence
#[test]
fn offered_binds_actual_root_after_response_without_changing_old_evidence() {
    let mut f = Fixture::new();
    f.offered();
    let decision = read(&f.decision_path(1));
    let keys: BTreeSet<&str> = decision
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "schema",
            "open_sha256",
            "attempt_sha256",
            "outcome_sha256",
            "sequence",
            "status",
            "continuation_offered",
            "human_root",
            "root_wake_sha256"
        ])
    );
    let root_path = std::fs::read_dir(f.roots())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let actual = read(&root_path);
    assert_eq!(get(&decision, "human_root"), get(&actual, "human_root"));
    assert_eq!(text(&decision, "root_wake_sha256"), digest(&actual));
    assert_eq!(
        text(&decision, "open_sha256"),
        digest(f.coverage.open_value())
    );
    assert_eq!(
        text(&decision, "attempt_sha256"),
        digest(&read(&f.coverage.directory().join("attempts/1.json")))
    );
    assert_eq!(text(&decision, "outcome_sha256"), digest(&f.outcome(1)));
    assert_eq!(text(&decision, "status"), "offered");
    assert_eq!(get(&decision, "continuation_offered"), &Value::Bool(true));
    let meta = std::fs::metadata(f.decision_path(1)).unwrap();
    assert!(meta.len() <= 4096);
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let parent = std::fs::metadata(f.decision_path(1).parent().unwrap()).unwrap();
    assert_eq!(parent.permissions().mode() & 0o777, 0o700);
    // The response preceded the optional record.
    assert!(f.responses.kinds().contains(&"OBSERVED".to_owned()));
    assert!(f.coverage.close(&f.final_()).unwrap());
    let inventory = f.inventory().unwrap();
    let summary = f.readback().unwrap();
    assert_eq!(int(get(get(&summary, "counts"), "offered")), 1);
    assert_eq!(
        encoded(get(&summary, "rows")),
        encoded(&json(&format!(
            r#"[{{"instance_id":"{}","sequence":1,"status":"offered","continuation_offered":true}}]"#,
            f.instance
        )))
    );
    assert_eq!(text(&summary, "delivery"), "not-observed");
    assert_eq!(get(&summary, "assessment_authority"), &Value::Bool(false));
    assert_eq!(get(&summary, "completion_inferred"), &Value::Bool(false));
    assert_eq!(encoded(&f.inventory().unwrap()), encoded(&inventory));
    let optional = f.decision_path(1).parent().unwrap().to_path_buf();
    let hidden = optional.with_file_name("hidden-optional-records");
    std::fs::rename(&optional, &hidden).unwrap();
    assert_eq!(encoded(&f.inventory().unwrap()), encoded(&inventory));
    assert_eq!(f.count("unrecorded"), 1);
    std::fs::rename(&hidden, &optional).unwrap();
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_not_armed_is_distinct_from_armed_without_nonce
#[test]
fn not_armed_is_distinct_from_armed_without_nonce() {
    let mut f = Fixture::new();
    let first = f.seal(false, 1);
    f.execute_seal(
        returning(json(
            r#"{"verified":true,"continue":false,"status":"pending"}"#,
        )),
        &first,
        1,
        None,
    );
    let second = f.seal(true, 2);
    f.execute_seal(
        returning(json(
            r#"{"verified":true,"continue":false,"status":"pending-undeliverable-window"}"#,
        )),
        &second,
        2,
        None,
    );
    let (first, second) = (read(&f.decision_path(1)), read(&f.decision_path(2)));
    assert_eq!(text(&first, "status"), "not-armed");
    assert_eq!(text(&second, "status"), "pending-undeliverable-window");
    for decision in [&first, &second] {
        assert_eq!(get(decision, "continuation_offered"), &Value::Bool(false));
        assert_eq!(get(decision, "human_root"), &Value::Null);
        assert_eq!(get(decision, "root_wake_sha256"), &Value::Null);
    }
    assert!(!f.roots().exists());
    assert_eq!(f.count("not-armed"), 1);
    assert_eq!(f.count("pending-undeliverable-window"), 1);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_empty_verified_seal_reports_wake_not_evaluated
#[test]
fn empty_verified_seal_reports_wake_not_evaluated() {
    let mut f = Fixture::new();
    let seal = f.seal(true, 1);
    f.execute_seal(
        returning(json(r#"{"verified":true,"status":"observed"}"#)),
        &seal,
        1,
        None,
    );
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
    assert_eq!(
        text(&read(&f.decision_path(1)), "status"),
        "wake-not-evaluated"
    );
    assert_eq!(f.count("wake-not-evaluated"), 1);
    assert!(!f.roots().exists());
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_offered_writer_checks_actual_request_without_cursor_or_root_scan
#[test]
fn offered_writer_checks_actual_request_without_cursor_or_root_scan() {
    let mut f = Fixture::new();
    f.offered();
    assert_eq!(text(&read(&f.decision_path(1)), "status"), "offered");
    let observe = offering(f.roots(), f.session.clone(), true);
    let seal = f.seal(true, 2);
    f.execute_seal(observe, &seal, 2, Some(&uid()));
    assert_eq!(std::fs::read_dir(f.roots()).unwrap().count(), 2);
    assert!(!f.decision_path(2).exists());
    assert_eq!(f.count("unrecorded"), 1);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_spoofed_record_keys_types_digests_and_status_are_unbound_without_contents
#[test]
fn spoofed_record_keys_types_digests_and_status_are_unbound_without_contents() {
    let mut f = Fixture::new();
    f.offered();
    let actual = read(&f.decision_path(1));
    let inventory = f.inventory().unwrap();
    let mut variants = Vec::new();
    for (key, item) in [
        ("private_payload", string("do-not-disclose")),
        ("status", string("do-not-disclose")),
        ("continuation_offered", json("1")),
        ("sequence", Value::Bool(true)),
        ("open_sha256", string(&"0".repeat(64))),
        ("attempt_sha256", string(&"0".repeat(64))),
        ("outcome_sha256", string(&"0".repeat(64))),
        ("root_wake_sha256", string(&"0".repeat(64))),
        ("human_root", string(&uid())),
    ] {
        let mut value = actual.clone();
        set(&mut value, key, item);
        variants.push(value);
    }
    let mut pending = actual.clone();
    set(&mut pending, "status", string("pending"));
    set(&mut pending, "continuation_offered", Value::Bool(false));
    variants.push(pending);
    let mut missing = actual.clone();
    remove(&mut missing, "schema");
    variants.push(missing);
    for value in variants {
        std::fs::write(f.decision_path(1), encoded(&value)).unwrap();
        let summary = f.readback().unwrap();
        assert_eq!(int(get(get(&summary, "counts"), "unbound")), 1);
        let first = &get(&summary, "rows").as_array().unwrap()[0];
        assert_eq!(text(first, "status"), "unbound");
        assert_eq!(get(first, "continuation_offered"), &Value::Bool(false));
        assert!(
            !String::from_utf8(encoded(&summary))
                .unwrap()
                .contains("do-not-disclose")
        );
        assert_eq!(encoded(&f.inventory().unwrap()), encoded(&inventory));
    }
    let pretty = json::encode(&actual, json::INDENT2_UTF8).unwrap();
    std::fs::write(f.decision_path(1), pretty).unwrap();
    assert_eq!(f.count("offered"), 1);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_canonical_parsed_evidence_digests_survive_noncanonical_json_encoding
#[test]
fn canonical_parsed_evidence_digests_survive_noncanonical_json_encoding() {
    let mut f = Fixture::new();
    f.offered();
    let inventory = f.inventory().unwrap();
    let mut paths: Vec<PathBuf> = [
        "open.json",
        "attempts/1.json",
        "started/1.json",
        "outcomes/1.json",
    ]
    .iter()
    .map(|name| f.coverage.directory().join(name))
    .collect();
    paths.extend(
        std::fs::read_dir(f.roots())
            .unwrap()
            .map(|e| e.unwrap().path()),
    );
    for path in paths {
        let value = read(&path);
        std::fs::write(&path, json::encode(&value, json::INDENT2_UTF8).unwrap()).unwrap();
    }
    assert_eq!(encoded(&f.inventory().unwrap()), encoded(&inventory));
    assert_eq!(f.count("offered"), 1);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_malformed_real_result_shapes_are_unknown_without_inventing_a_cause
#[test]
fn malformed_real_result_shapes_are_unknown_without_inventing_a_cause() {
    let mut f = Fixture::new();
    let shapes = [
        r#"{"verified":true,"continue":"yes","status":"pending"}"#,
        r#"{"verified":true,"continue":false,"status":"do-not-disclose"}"#,
        r#"{"verified":true,"continue":true,"request_id":null}"#,
        r#"{"verified":true,"continue":false,"status":["do-not-disclose"]}"#,
    ];
    for (sequence, shape) in (1..).zip(shapes) {
        let seal = f.seal(true, sequence);
        f.execute_seal(returning(json(shape)), &seal, sequence, None);
        let decision = read(&f.decision_path(sequence));
        assert_eq!(text(&decision, "status"), "unknown");
        assert_eq!(get(&decision, "human_root"), &Value::Null);
    }
    let actual: Vec<Vec<u8>> = f
        .responses
        .values()
        .into_iter()
        .filter(|(kind, _, _)| kind == "OBSERVED")
        .map(|(_, _, value)| encoded(get(&value, "result")))
        .collect();
    let expected: Vec<Vec<u8>> = shapes.iter().map(|shape| encoded(&json(shape))).collect();
    assert_eq!(actual, expected);
    let summary = f.readback().unwrap();
    assert_eq!(
        int(get(get(&summary, "counts"), "unknown")),
        shapes.len() as u64
    );
    assert!(
        !String::from_utf8(encoded(&summary))
            .unwrap()
            .contains("do-not-disclose")
    );
    assert!(!f.roots().exists());
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_nonseal_and_failed_source_attempts_do_not_acquire_diagnostic_rows
#[test]
fn nonseal_and_failed_source_attempts_do_not_acquire_diagnostic_rows() {
    let mut f = Fixture::new();
    let ingest = f.request();
    f.execute_seal(
        returning(json(r#"{"verified":true,"status":"observed"}"#)),
        &ingest,
        1,
        None,
    );
    let failed = observer(|_, _, call| {
        (call.on_started)()?;
        Err(ObserveError::Unexpected(
            "controlled source failure before verification".into(),
        ))
    });
    let seal = f.seal(true, 2);
    f.execute_seal(failed, &seal, 2, None);
    assert!(!f.decision_path(1).exists());
    assert!(!f.decision_path(2).exists());
    assert_eq!(text(&f.outcome(2), "outcome"), "unknown");
    let summary = f.readback().unwrap();
    assert_eq!(get(&summary, "rows"), &Value::Array(vec![]));
    let total: u64 = get(&summary, "counts")
        .as_object()
        .unwrap()
        .iter()
        .map(|(_, v)| int(v))
        .sum();
    assert_eq!(total, 0);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_conflicting_existing_record_is_never_rewritten_or_replayed
#[test]
fn conflicting_existing_record_is_never_rewritten_or_replayed() {
    let mut f = Fixture::new();
    let path = f.decision_path(1);
    turn_sources::private_mkdir(path.parent().unwrap()).unwrap();
    let conflict = br#"{"arbitrary":"do-not-disclose"}"#;
    write_private(&path, conflict);
    f.offered();
    assert_eq!(std::fs::read(&path).unwrap(), conflict);
    assert_eq!(std::fs::read_dir(f.roots()).unwrap().count(), 1);
    let summary = f.readback().unwrap();
    assert_eq!(int(get(get(&summary, "counts"), "unbound")), 1);
    assert!(
        !String::from_utf8(encoded(&summary))
            .unwrap()
            .contains("do-not-disclose")
    );
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_substituted_oversize_response_never_records_an_offered_continuation
#[test]
fn substituted_oversize_response_never_records_an_offered_continuation() {
    let mut f = Fixture::new();
    let writer = Arc::new(ResponseWriter::unstarted(&f.instance, Vec::new()));
    let offer = offering(f.roots(), f.session.clone(), false);
    let oversized = observer(move |repo, request, call| {
        let mut result = offer(repo, request, call)?;
        set(
            &mut result,
            "faults",
            Value::Array(vec![string(&"x".repeat(stream::MAX_RESPONSE))]),
        );
        Ok(result)
    });
    let executor = FiniteExecutor::new(
        writer.clone(),
        f.clocks.clone(),
        None,
        Some(oversized),
        Timing::system(),
    );
    let seal = f.seal(true, 1);
    executor
        .execute(&mut f.coverage, "REQUEST", 1, Some(&uid()), &encoded(&seal))
        .unwrap();
    let notice = body(&writer.queued().last().unwrap().1);
    assert_eq!(text(&notice, "code"), "result-oversized");
    assert!(notice.get("result").is_none());
    assert!(notice.get("continue").is_none());
    assert!(!f.decision_path(1).exists());
    assert_eq!(std::fs::read_dir(f.roots()).unwrap().count(), 1);
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
    assert_eq!(f.count("unrecorded"), 1);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_false_observed_send_leaves_decision_unrecorded
#[test]
fn false_observed_send_leaves_decision_unrecorded() {
    let mut f = Fixture::new();
    f.responses.reject_observed.store(true, Ordering::SeqCst);
    let seal = f.seal(true, 1);
    f.execute_seal(
        returning(json(r#"{"verified":true,"status":"observed"}"#)),
        &seal,
        1,
        None,
    );
    assert!(!f.decision_path(1).exists());
    assert_eq!(text(&f.outcome(1), "outcome"), "verified");
    assert_eq!(f.count("unrecorded"), 1);
}

// obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_bounded_summary_orphans_and_unreadable_namespace_do_not_change_doctor_health
#[test]
fn bounded_summary_orphans_and_unreadable_namespace_are_bounded_and_refused() {
    let mut f = Fixture::new();
    for sequence in 1..67 {
        let seal = f.seal(false, sequence);
        f.execute_seal(
            returning(json(r#"{"verified":true,"status":"observed"}"#)),
            &seal,
            sequence,
            None,
        );
    }
    assert!(f.coverage.close(&Fixture::last(66, 0, 66, None)).unwrap());
    let inventory = f.inventory().unwrap();
    let summary = f.readback().unwrap();
    assert_eq!(get(&summary, "rows").as_array().unwrap().len(), 64);
    assert_eq!(get(&summary, "truncated"), &Value::Bool(true));
    assert_eq!(int(get(get(&summary, "counts"), "not-armed")), 66);
    let optional = f.decision_path(1).parent().unwrap().to_path_buf();
    let orphan = optional.parent().unwrap().join(uid());
    turn_sources::private_mkdir(&orphan).unwrap();
    std::fs::write(orphan.join("not-a-diagnostic"), "do-not-disclose").unwrap();
    assert_eq!(encoded(&f.readback().unwrap()), encoded(&summary));
    let hidden = optional.with_file_name("hidden-owned-diagnostics");
    std::fs::rename(&optional, &hidden).unwrap();
    write_private(&optional, b"do-not-disclose");
    // The diagnostic caller reports the unavailable summary for this refusal.
    let refused = f.readback().unwrap_err();
    assert!(!refused.message().contains("do-not-disclose"));
    let unavailable = stream::wake_decision_summary(false);
    assert_eq!(
        text(&unavailable, "availability"),
        "unavailable-or-unverifiable"
    );
    assert_eq!(get(&unavailable, "rows"), &Value::Array(vec![]));
    assert_eq!(encoded(&f.inventory().unwrap()), encoded(&inventory));
}
