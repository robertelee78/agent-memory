//! Finite private omp FIFO. Coverage metadata is never source completion authority.
//!
//! One serial source executor drains a framed, bounded stdin FIFO in order.
//! Every source attempt, start and outcome is an immutable private record
//! bound to its OPEN; inventory pins them, so an unaccounted, unknown or
//! unpublished attempt keeps lifecycle drain pending. Clock frames only ever
//! narrow optional wake guidance; they never gate ordinary source capture.
//!
//! Clock relation (design "Coexistence, clocks, processes"): the local stamps
//! come from `clock::monotonic_ns` — Linux `clock_gettime(CLOCK_MONOTONIC)`,
//! macOS `mach_absolute_time` scaled by its timebase, the relation CPython
//! used. The CPython interpreter/clock gate becomes a self-check of
//! `clock::clock_identity()`. A Linux window is usable only after the parent
//! image is one of the exact qualified upstream images in the same time
//! namespace (E = 0); Darwin keeps the provisional rate allowance and the
//! `provisional-unqualified` status. READY always advertises
//! `wake_support: unqualified`: the port does not newly qualify wake support.
mod clock_window;
mod coverage;
mod executor;
mod frame;
mod inventory;
mod linux_image;
mod run;
mod wake;

use crate::codec::json::Value;
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, Descriptor, DirectoryChain};
use crate::lifecycle::get;
use crate::turn_sources;
pub use clock_window::{
    ClockWindows, DeliveryWindow, LateNativeDeliveryWindow, Monotonic, NativeImage, Platform,
    VerifiedNativeDeliveryWindow,
};
pub use coverage::{
    Attempt, AttemptError, Coverage, FrameProof, Installed, OmpTransport, StreamAuthority, binding,
};
pub use executor::{FiniteExecutor, ObserveCall, ObserveError, Observer, SourceCompletion};
pub use frame::{
    Budget, Frame, FrameReader, Output, Responder, ResponseWriter, Sent, encode_frame,
};
pub use inventory::{
    RecoveryEvidence, drain_state, inventory, open_value, resolve_recovery, retire_for_recovery,
    validate_drain,
};
pub use linux_image::{
    ImageProbe, ImageStat, LinuxNativeImage, QUALIFIED_CLOCK, TimeNamespace, process_start,
    qualified_images, time_namespace_with,
};
pub use run::{RunOptions, STREAM_PARSER, Startup, StreamStartup, Timing, main, run};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
pub use wake::{wake_decision_directory, wake_decision_summary, wake_decisions};

pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const MAX_RETAINED: usize = 64 * 1024 * 1024;
pub const MAX_FRAMES: usize = 4096;
pub const MAX_HEADER: usize = 256;
pub const READ_CHUNK: usize = 64 * 1024;
pub const MAX_RESPONSE: usize = 64 * 1024;
pub const MAX_RESPONSES: usize = 4;
pub const MAX_IDENTITY: usize = 1024 * 1024;
pub const MAX_IDS: usize = 16384;
pub const MAX_METADATA: usize = MAX_IDENTITY + 8192;
/// Bounded busy retry for metadata and observer acquisition (seconds).
pub const BUSY_SECONDS: u64 = 30;
/// Backoff between busy retries (25 ms).
pub const BUSY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(25);
pub const MARGIN_NS: u128 = 100_000_000;
pub const MAC_PROVISIONAL_RHO_NUMERATOR: u128 = 164;
pub const RATE_DENOMINATOR: u128 = 10000;
pub const WAKE_SUPPORT_QUALIFIED: bool = false;
pub const SOURCE_TYPES: [&str; 3] = ["REQUEST", "REFUSED-ATTEMPT", "FINAL"];
pub const CONTROL_TYPES: [&str; 3] = ["CLOCK", "CLOCK-ACK", "CANCEL"];
pub const ACTIONS: [&str; 4] = ["hint", "ingest", "seal", "wake"];
pub const REFUSAL_CODES: [&str; 12] = [
    "capture-refused",
    "identity-changed",
    "unsafe-view",
    "oversized-source",
    "queue-full",
    "serialization-failed",
    "startup-pending",
    "invalid-request",
    "binding-refused",
    "transition-final-view-unavailable",
    "unsupported-view",
    "missing-payload",
];
/// Exact upstream Linux arm64 images; see the v0.0.15 clock qualification contract.
pub const LINUX_MONOTONIC_IMAGES: [&str; 2] = [
    "c2be1ea85ce17c4270854b57efff2a7ea1d5a03c7557fd08721f2b9bb32698de",
    "61826bf89c2c0512417fee05fa2f486e60ee11b23ea95a17257a6c49edb6b8d5",
];
pub const MAX_NATIVE_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
/// Named bound for every private namespace listing (the baseline listed
/// attempt/outcome/transition directories without a bound).
pub const MAX_ENTRIES: usize = 65536;
const MAX_INTEGER: u64 = (1 << 53) - 1;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
pub(crate) fn busy(error: &ContractError) -> bool {
    matches!(error.kind(), Kind::SourceTransportBusy { .. })
}
fn path_text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "finite stream path")
}
fn leaf(path: &Path) -> Result<&OsStr> {
    path.file_name()
        .ok_or_else(|| err("Finite stream path requires a filename"))
}
fn parent(path: &Path) -> Result<&Path> {
    path.parent()
        .ok_or_else(|| err("Finite stream path requires a parent"))
}
fn name(path: &Path) -> Result<&str> {
    crate::host::text(leaf(path)?, "finite stream entry")
}
fn stem(path: &Path) -> Result<&str> {
    name(path).map(|n| n.strip_suffix(".json").unwrap_or(n))
}
fn object(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn int(value: u64) -> Value {
    Value::Int(value.into())
}
fn opt_string(value: Option<&str>) -> Value {
    value.map_or(Value::Null, string)
}
fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Value {
    Value::Array(values.into_iter().map(|v| string(v)).collect())
}
/// Python dict/list equality: compare canonical sorted encodings.
fn same(left: &Value, right: &Value) -> bool {
    match (turn_sources::encoded(left), turn_sources::encoded(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
fn same_opt(left: Option<&Value>, right: Option<&Value>) -> bool {
    same(left.unwrap_or(&Value::Null), right.unwrap_or(&Value::Null))
}

/// `set(value) == set(fields)`.
fn exact<'a>(value: &'a Value, fields: &[&str]) -> Result<&'a Value> {
    let ok = value.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len() && fields.iter().all(|f| value.get(f).is_some())
    });
    if !ok {
        return Err(err("Malformed finite stream metadata"));
    }
    Ok(value)
}
fn canonical_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
/// `str(uuid.UUID(value)) == value`: canonical lowercase hyphenated form.
fn uuid(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| canonical_uuid(s))
        .ok_or_else(|| err("Noncanonical finite stream identity"))
}
fn uuid_text(value: &str) -> Result<&str> {
    canonical_uuid(value)
        .then_some(value)
        .ok_or_else(|| err("Noncanonical finite stream identity"))
}
fn lower_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn installation_id(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| lower_hex(s, 32))
        .ok_or_else(|| err("Invalid installed runtime identity"))
}
/// `type(value) is int and 0 <= value <= 2**53 - 1`.
fn integer(value: &Value) -> Result<u64> {
    value
        .as_int()
        .and_then(|v| v.as_u64())
        .filter(|v| *v <= MAX_INTEGER)
        .ok_or_else(|| err("Invalid finite stream counter"))
}
fn check_counter(value: u64) -> Result<u64> {
    if value > MAX_INTEGER {
        return Err(err("Invalid finite stream counter"));
    }
    Ok(value)
}
fn digest(value: &Value) -> Result<String> {
    Ok(crate::developer_identity::sha256_hex(
        &turn_sources::encoded(value)?,
    ))
}
fn sha(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| lower_hex(s, 64))
        .ok_or_else(|| err("Invalid finite stream metadata digest"))
}
fn sha_text(value: &str) -> Result<&str> {
    lower_hex(value, 64)
        .then_some(value)
        .ok_or_else(|| err("Invalid finite stream metadata digest"))
}
fn session_sha256(session: &str) -> String {
    crate::developer_identity::sha256_hex(session.as_bytes())
}
/// Sorted, unique, bounded native identity inventory.
fn ids(value: &Value) -> Result<Vec<String>> {
    let items = value
        .as_array()
        .filter(|items| items.len() <= MAX_IDS)
        .ok_or_else(|| err("Unresolvable native identity inventory"))?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| err("Unresolvable native identity inventory"))?;
        result.push(text.to_owned());
    }
    let mut canonical = result.clone();
    canonical.sort();
    canonical.dedup();
    if canonical != result {
        return Err(err("Noncanonical native identity inventory"));
    }
    for item in items {
        turn_sources::bounded(item, 512)?;
    }
    Ok(result)
}
fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute() && !path.components().any(|c| c == Component::ParentDir)
}

/// Closed metadata-only inventory; unknown identities are never truncated.
pub fn identity(value: &Value) -> Result<()> {
    if matches!(value, Value::Null) {
        return Ok(());
    }
    exact(value, &["view_id", "leaf_id", "native_ids", "tool_ids"])?;
    uuid(get(value, "view_id"))?;
    let leaf_value = get(value, "leaf_id");
    if !matches!(leaf_value, Value::Null) {
        turn_sources::bounded(leaf_value, 512)?;
    }
    let native = ids(get(value, "native_ids"))?;
    ids(get(value, "tool_ids"))?;
    if let Some(leaf_id) = leaf_value.as_str() {
        if !native.iter().any(|item| item == leaf_id) {
            return Err(err("Checked leaf absent from native inventory"));
        }
    }
    if turn_sources::encoded(value)?.len() > MAX_IDENTITY {
        return Err(err("Unresolvable oversized native identity inventory"));
    }
    Ok(())
}

fn hint_identity(value: &Value, session: Option<&str>, repo: Option<&Path>) -> Result<()> {
    exact(value, &["kind", "session", "header_id", "cwd", "leaf_id"])?;
    if get(value, "kind").as_str() != Some("hint") {
        return Err(err("Unknown neutral hint identity"));
    }
    turn_sources::bounded(get(value, "session"), 512)?;
    turn_sources::bounded(get(value, "header_id"), 512)?;
    turn_sources::bounded(get(value, "cwd"), 4096)?;
    let hinted = get(value, "session").as_str();
    if get(value, "header_id").as_str() != hinted || session.is_some_and(|s| Some(s) != hinted) {
        return Err(err("Neutral hint session/header identity differs"));
    }
    let cwd = Path::new(get(value, "cwd").as_str().unwrap_or_default());
    if !absolute_without_parent(cwd) {
        return Err(err("Invalid neutral hint scope"));
    }
    if let Some(repo) = repo {
        let scoped = match (
            fs::canonical_path(cwd, false),
            fs::canonical_path(repo, false),
        ) {
            (Ok(cwd), Ok(repo)) => cwd.starts_with(repo),
            _ => false,
        };
        if !scoped {
            return Err(err("Neutral hint belongs to another repository"));
        }
    }
    let leaf_value = get(value, "leaf_id");
    if !matches!(leaf_value, Value::Null) {
        turn_sources::bounded(leaf_value, 512)?;
    }
    Ok(())
}

fn is_hint(value: &Value) -> bool {
    value.as_object().is_some() && get(value, "kind").as_str() == Some("hint")
}

/// Observer-internal evidence that a complete committed old tail held only
/// NONLEARNING entries. No Clone, no serialization, no public fields.
pub struct NonlearningTailEvidence {
    session: String,
    view_id: String,
    native_ids: Vec<String>,
}

/// Trusted ingest only, after complete committed old-file/parser qualification.
///
/// The native projection must independently classify EVERY actual ID as
/// NONLEARNING/payload-not-required and verify writer drain/original anchors.
/// No observer JSON field or caller completeness flag can mint this evidence.
pub fn nonlearning_tail_evidence(
    session: &str,
    view_id: &str,
    native_ids: &[String],
) -> Result<NonlearningTailEvidence> {
    turn_sources::bounded(&string(session), 512)?;
    uuid_text(view_id)?;
    ids(&strings(native_ids.iter().cloned()))?;
    if native_ids.is_empty() {
        return Err(err("Empty source inventory cannot prove absent old tail"));
    }
    Ok(NonlearningTailEvidence {
        session: session.to_owned(),
        view_id: view_id.to_owned(),
        native_ids: native_ids.to_vec(),
    })
}

/// Named-entry presence without following a final link (`os.path.lexists`).
fn lexists(path: &Path) -> Result<bool> {
    let directory = match Descriptor::pin_directory(parent(path)?) {
        Ok(directory) => directory,
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(libc::ENOENT) | Some(libc::ENOTDIR)
            ) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(io(error)),
    };
    match directory.stat_at(leaf(path)?) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io(error)),
    }
}
/// `turn_sources.read` that raises when the record is absent.
fn read_required(path: &Path, maximum: usize) -> Result<Value> {
    turn_sources::read(path, maximum)?
        .ok_or_else(|| err("Finite stream metadata is missing; retain pending evidence"))
}
fn read_bytes_required(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    turn_sources::read_bytes(path, maximum)?
        .ok_or_else(|| err("Finite stream metadata is missing; retain pending evidence"))
}

/// Publish an immutable record, or accept only its exact existing value.
pub fn retain(path: &Path, value: &Value, maximum: usize) -> Result<String> {
    turn_sources::private_mkdir(parent(path)?)?;
    if !turn_sources::exclusive(path, value, maximum)? {
        let existing = read_required(path, maximum)?;
        if !same(&existing, value) {
            return Err(err("Immutable finite stream metadata changed"));
        }
    }
    digest(value)
}

/// `<base>/omp-stream-v1/<sha256(session)>/<instance>`.
pub fn directory(base: &Path, session: &str, instance: &str) -> Result<PathBuf> {
    turn_sources::bounded(&string(session), 512)?;
    uuid_text(instance)?;
    Ok(base
        .join("omp-stream-v1")
        .join(session_sha256(session))
        .join(instance))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pattern {
    /// `[1-9][0-9]*\.json`
    Sequence,
    /// `[0-9a-f-]{36}\.json`
    Transition,
    /// `[0-9a-f]{64}\.lock`
    SessionLock,
    /// `[0-9a-f]{64}`
    Session,
}
impl Pattern {
    fn matches(self, name: &str) -> bool {
        match self {
            Self::Sequence => name.strip_suffix(".json").is_some_and(|s| {
                s.bytes().next().is_some_and(|b| (b'1'..=b'9').contains(&b))
                    && s.bytes().all(|b| b.is_ascii_digit())
            }),
            Self::Transition => name.strip_suffix(".json").is_some_and(|s| {
                s.len() == 36
                    && s.bytes()
                        .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }),
            Self::SessionLock => name.strip_suffix(".lock").is_some_and(|s| lower_hex(s, 64)),
            Self::Session => lower_hex(name, 64),
        }
    }
}
/// The sequence number of an already pattern-checked `<n>.json` entry.
fn sequence_of(path: &Path) -> Result<u64> {
    stem(path)?
        .parse::<u64>()
        .map_err(|_| err("Unknown finite stream inventory entry"))
}

/// Complete bounded inventory of one private namespace, sorted by name.
fn entries(
    path: &Path,
    pattern: Pattern,
    directories: bool,
    maximum: usize,
) -> Result<Vec<PathBuf>> {
    fs::trust::state(path)?;
    let chain = DirectoryChain::open(path).map_err(io)?;
    let names = chain
        .directory()
        .entries_bounded(maximum)
        .map_err(io)?
        .ok_or_else(|| err("Finite stream inventory exceeds bound"))?;
    let mut result = Vec::with_capacity(names.len());
    for entry in names {
        let info = chain.directory().stat_at(&entry).map_err(io)?;
        let text = crate::host::text(&entry, "finite stream entry")?;
        if !pattern.matches(text) || info.is_directory() != directories {
            return Err(err("Unknown finite stream inventory entry"));
        }
        if info.is_symlink() {
            return Err(err("Redirected finite stream inventory"));
        }
        result.push(path.join(&entry));
    }
    chain.recheck().map_err(io)?;
    result.sort();
    Ok(result)
}

const STARTUP_SCHEMA: &str = "agent-memory/omp-session-startup-incomplete-v1";

fn startup_marker(session_sha256: &str) -> Value {
    object([
        ("schema", string(STARTUP_SCHEMA)),
        ("session_sha256", string(session_sha256)),
    ])
}

/// Instance directories of one session namespace plus its startup marker.
fn session_instances(directory: &Path) -> Result<(Vec<PathBuf>, Option<Value>)> {
    fs::trust::state(directory)?;
    let session = sha_text(name(directory)?)?.to_owned();
    let chain = DirectoryChain::open(directory).map_err(io)?;
    let names = chain
        .directory()
        .entries_bounded(MAX_ENTRIES)
        .map_err(io)?
        .ok_or_else(|| err("Finite stream inventory exceeds bound"))?;
    let mut result = Vec::new();
    let mut marker = None;
    for entry in names {
        let info = chain.directory().stat_at(&entry).map_err(io)?;
        if info.is_symlink() {
            return Err(err("Redirected finite session startup state"));
        }
        let path = directory.join(&entry);
        let text = crate::host::text(&entry, "finite stream entry")?;
        if text == "startup-incomplete.json" {
            let value = read_required(&path, 4096)?;
            if !same(&value, &startup_marker(&session)) {
                return Err(err("Unknown finite session startup qualification"));
            }
            marker = Some(value);
        } else {
            uuid_text(text)?;
            if !info.is_directory() {
                return Err(err("Unknown finite session startup entry"));
            }
            result.push(path);
        }
    }
    chain.recheck().map_err(io)?;
    result.sort();
    Ok((result, marker))
}

/// Source barrier -> creation lock -> live/stream locks; never a runtime suffix.
fn session_creation_lock<T>(
    base: &Path,
    session_sha256: &str,
    body: impl FnOnce(bool) -> Result<T>,
) -> Result<T> {
    let directory = base.join("omp-stream-session-locks");
    turn_sources::private_mkdir(&directory)?;
    let file = format!("{}.lock", sha_text(session_sha256)?);
    let path = directory.join(&file);
    let chain = DirectoryChain::open(&directory).map_err(io)?;
    let created = match chain.directory().create_exclusive(
        OsStr::new(&file),
        libc::O_WRONLY | libc::O_NONBLOCK,
        0o600,
    ) {
        Ok(_) => {
            turn_sources::sync(&directory)?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => return Err(io(error)),
    };
    drop(chain);
    let lock = turn_sources::private_lock(&path, false, false)?;
    read_bytes_required(&path, 0)?;
    let value = body(created)?;
    lock.finish()?;
    Ok(value)
}

/// One private lock scope with its explicit successful exit recheck.
fn with_lock<T>(
    path: &Path,
    shared: bool,
    existing_only: bool,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let lock = turn_sources::private_lock(path, shared, existing_only)?;
    let value = body()?;
    lock.finish()?;
    Ok(value)
}

static PRELOADED: Mutex<Option<Option<String>>> = Mutex::new(None);

/// The Rust executable closure is linked before OPEN, so nothing loads after
/// native stdin disappears. One finite worker process still adopts exactly one
/// installed artifact: a later different receipt artifact is refused.
pub fn preload(receipt: Option<&Value>) -> Result<()> {
    let artifact = match receipt {
        Some(receipt) => Some(
            get(get(receipt, "source"), "artifact_digest")
                .as_str()
                .ok_or_else(|| err("Finite worker receipt lacks its artifact"))?
                .to_owned(),
        ),
        None => None,
    };
    let mut preloaded = PRELOADED.lock().unwrap_or_else(|e| e.into_inner());
    match preloaded.as_ref() {
        Some(existing) if *existing != artifact => {
            Err(err("Finite worker cannot adopt another executable closure"))
        }
        Some(_) => Ok(()),
        None => {
            *preloaded = Some(artifact);
            Ok(())
        }
    }
}

/// Seam registration for `omp_transport` (registered by CLI wiring once ported).
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
                "OmpTransport provider is not registered; finite sources remain pending",
            )
        })
}
