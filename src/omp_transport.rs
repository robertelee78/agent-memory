//! Private native evidence transport. Shared turn_sources owns all completion.
//!
//! Baseline `omp_transport.py`. Callees are called directly: `omp_boundary`
//! (private base, marker), `omp_control`, `omp_installation` (observer
//! binding, owned manual), `omp_projection`, `omp_stream` (stream inventory,
//! recovery fences, delivery windows, source completion), `scope_binding`,
//! `turn_sources`. This module implements the `omp_boundary::OmpTransport`
//! and `omp_stream::OmpTransport` seams and the T2f half of the
//! `omp_installation::OmpActivation` interface (`providers`).
use crate::codec::json::Value;
use crate::developer_identity::sha256_hex;
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain, RenameMode};
use crate::lifecycle::{get, object, string};
use crate::omp_boundary;
use crate::omp_installation::{self, OwnedManual};
use crate::omp_projection::{self, exclusions, has_exact, same};
use crate::scope_binding::PrivateEnrollmentContext;
use crate::turn_sources::{self, Namespace};
use sha2::Digest;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod inventory;
mod observe;
mod providers;

pub use inventory::{
    complete, historical_completed_family, list_pending, pending_inventory, segment_inventory,
    source,
};
pub use observe::{
    Observation, ObserverScope, checked_payload_coverage, observe, resolve_tool_result, wake,
};
pub use providers::{Harness, InstalledHarness, InstalledScope, Transport};
pub(crate) use providers::{boundary_all_segments, boundary_pending_inventory, boundary_source};

/// Retained snapshot and manifest read bound (baseline `16 * 1024 * 1024`).
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
/// Captures, gaps and control inventories per segment (baseline `16384`).
pub const MAX_CAPTURES: usize = 16384;
/// Native cursor bound (baseline `1024 * 1024`).
pub const MAX_CURSOR: usize = 1024 * 1024;
/// Small immutable records: captures, gaps, enrollments, attempts (baseline `4096`).
pub const MAX_RECORD: usize = 4096;
/// Session namespaces, segments and root wakes (baseline `65536`).
pub const MAX_ENTRIES: usize = 65536;
pub const TRIGGERS: [&str; 7] = [
    "human",
    "human-with-own-guidance",
    "own-hidden-pure",
    "own-hidden-combined",
    "own-hidden-ambiguous",
    "foreign-hidden",
    "resume",
];
pub const GAPS: [&str; 7] = [
    "unsafe-representation",
    "missing-stored-source",
    "ambiguous-lifecycle",
    "missing-payload",
    "oversized-source",
    "cursor-discontinuity",
    "late-substantive-source",
];
pub const NONLEARNING: [&str; 15] = [
    "model_change",
    "thinking_level_change",
    "label",
    "session_info",
    "credential_pin",
    "custom",
    "compaction",
    "branch_summary",
    "model_usage",
    "service_tier_change",
    "reset_boundary",
    "ttsr_injection",
    "title_change",
    "mode_change",
    "session_init",
];
const HUMAN_TRIGGERS: [&str; 2] = ["human", "human-with-own-guidance"];
const RESOLVABLE: [&str; 3] = [
    "missing-stored-source",
    "missing-payload",
    "cursor-discontinuity",
];
/// RFC 4122 `NAMESPACE_URL`.
const NAMESPACE_URL: [u8; 16] = [
    0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
];

pub(crate) fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

/// Where the transport operates: the repository, the runtime root of its
/// private state, and (inside an observer operation) the held enrollment context.
#[derive(Clone, Copy)]
pub struct Site<'a> {
    pub repo: &'a Path,
    pub runtime_root: &'a Path,
    pub context: Option<&'a PrivateEnrollmentContext>,
}
impl<'a> Site<'a> {
    pub fn new(repo: &'a Path, runtime_root: &'a Path) -> Self {
        Self {
            repo,
            runtime_root,
            context: None,
        }
    }
    pub fn with(self, context: &'a PrivateEnrollmentContext) -> Self {
        Self {
            context: Some(context),
            ..self
        }
    }
    /// Baseline `_base(repo, enrollment_context=…)`.
    pub fn base(&self) -> Result<PathBuf> {
        match self.context {
            Some(context) => omp_boundary::enrollment_base(self.repo, context, self.runtime_root),
            None => omp_boundary::base(self.repo),
        }
    }
}

/// One native segment directory and its live cursor, shared by every
/// inventory that names it (the baseline mutates one cursor dict in place).
pub struct Segment {
    pub directory: PathBuf,
    cursor: RefCell<Value>,
}
pub type Seg = Rc<Segment>;
impl Segment {
    pub fn new(directory: PathBuf, cursor: Value) -> Seg {
        Rc::new(Self {
            directory,
            cursor: RefCell::new(cursor),
        })
    }
    pub fn cursor(&self) -> Result<Ref<'_, Value>> {
        self.cursor
            .try_borrow()
            .map_err(|_| err("Native cursor is already being updated"))
    }
    pub(crate) fn cursor_mut(&self) -> Result<RefMut<'_, Value>> {
        self.cursor
            .try_borrow_mut()
            .map_err(|_| err("Native cursor is already being observed"))
    }
    pub fn snapshot(&self) -> Result<Value> {
        Ok(self.cursor()?.clone())
    }
    fn sealed(&self) -> Result<bool> {
        lexists(&self.directory.join("segment.json"))
    }
    fn bound(&self) -> Result<bool> {
        source_bound_origin(&self.directory, &*self.cursor()?)
    }
    fn lifecycle(&self) -> Result<String> {
        Ok(text(get(&*self.cursor()?, "lifecycle"))?.to_owned())
    }
    fn root(&self) -> Result<String> {
        Ok(text(get(get(&*self.cursor()?, "origin"), "human_root"))?.to_owned())
    }
    fn ordinal(&self) -> Result<u64> {
        ordinal(&*self.cursor()?)
    }
}

pub(crate) fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| err("Native source record has an unexpected value"))
}
/// Baseline `value[key]`: a missing key is a refusal, never a default.
pub(crate) fn field<'v>(value: &'v Value, key: &str) -> Result<&'v Value> {
    value
        .get(key)
        .ok_or_else(|| err("Native source record lacks a required field"))
}
pub(crate) fn field_text<'v>(value: &'v Value, key: &str) -> Result<&'v str> {
    text(field(value, key)?)
}
pub(crate) fn ordinal(cursor: &Value) -> Result<u64> {
    get(cursor, "event_ordinal")
        .as_int()
        .and_then(|n| n.as_u64())
        .ok_or_else(|| err("Invalid native ordinal"))
}
pub(crate) fn set(value: &mut Value, key: &str, item: Value) {
    match value {
        Value::Object(pairs) => match pairs.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = item,
            None => pairs.push((key.to_owned(), item)),
        },
        other => *other = object([(key, item)]),
    }
}
pub(crate) fn remove(value: &mut Value, key: &str) -> Option<Value> {
    let Value::Object(pairs) = value else {
        return None;
    };
    let index = pairs.iter().position(|(k, _)| k == key)?;
    Some(pairs.remove(index).1)
}
/// `cursor['control_interval'] or {}` as an owned object.
pub(crate) fn control_of(cursor: &Value) -> Value {
    match get(cursor, "control_interval") {
        Value::Object(pairs) if !pairs.is_empty() => Value::Object(pairs.clone()),
        _ => Value::Object(Vec::new()),
    }
}
pub(crate) fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Value {
    Value::Array(values.into_iter().map(|v| string(v)).collect())
}
fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| ContractError::non_utf8("native source path"))
}

/// Baseline `digest(value)`: sha256 of the canonical sorted compact encoding.
pub fn digest(value: &Value) -> Result<String> {
    Ok(sha256_hex(&turn_sources::encoded(value)?))
}
pub(crate) fn sha_text(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

fn canonical_uuid_text(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}
/// Baseline `canonical_uuid`: exactly `str(uuid.UUID(value))`.
pub fn canonical_uuid(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|text| canonical_uuid_text(text))
        .ok_or_else(|| err("Use a canonical native lifecycle UUID"))
}
fn uuid_bytes(value: &str) -> Result<[u8; 16]> {
    if !canonical_uuid_text(value) {
        return Err(err("Use a canonical native lifecycle UUID"));
    }
    let digits: Vec<u8> = value.bytes().filter(|byte| *byte != b'-').collect();
    let mut bytes = [0u8; 16];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| err("Invalid native UUID"))?;
        *byte = u8::from_str_radix(pair, 16).map_err(|_| err("Invalid native UUID"))?;
    }
    Ok(bytes)
}
fn uuid5_bytes(namespace: &[u8; 16], name: &str) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(namespace);
    hasher.update(name.as_bytes());
    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    for (byte, value) in bytes.iter_mut().zip(hash.iter()) {
        *byte = *value;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let (a, rest) = hex.split_at(8);
    let (b, rest) = rest.split_at(4);
    let (c, rest) = rest.split_at(4);
    let (d, e) = rest.split_at(4);
    format!("{a}-{b}-{c}-{d}-{e}")
}
/// `str(uuid.uuid5(uuid.UUID(namespace), name))`.
pub fn uuid5(namespace: &str, name: &str) -> Result<String> {
    Ok(uuid5_bytes(&uuid_bytes(namespace)?, name))
}
/// `str(uuid.uuid5(uuid.NAMESPACE_URL, name))`.
pub fn uuid5_url(name: &str) -> String {
    uuid5_bytes(&NAMESPACE_URL, name)
}

/// Baseline `exact(value, fields)`.
pub fn exact<'v>(value: &'v Value, fields: &[&str]) -> Result<&'v Value> {
    if !has_exact(value, fields) {
        return Err(err("Unexpected native observer fields"));
    }
    Ok(value)
}

fn bounded_ids(value: &Value, maximum: usize) -> Result<Vec<String>> {
    let items = value
        .as_array()
        .filter(|items| items.len() <= maximum)
        .ok_or_else(|| err("Invalid native tool invocation metadata"))?;
    items
        .iter()
        .map(|item| Ok(turn_sources::bounded(item, 512)?.to_owned()))
        .collect()
}

/// Baseline `admission_metadata`.
pub fn admission_metadata<'v>(value: &'v Value, session: &Value) -> Result<&'v Value> {
    exact(
        value,
        &[
            "schema",
            "session",
            "lifecycle",
            "capture_id",
            "event_ordinal",
            "role",
            "attribution",
            "tool_call_ids",
        ],
    )?;
    if get(value, "schema").as_str() != Some("agent-memory/omp-admission-v1")
        || get(value, "session") != session
    {
        return Err(err("Foreign native admission metadata"));
    }
    canonical_uuid(get(value, "lifecycle"))?;
    canonical_uuid(get(value, "capture_id"))?;
    if !matches!(get(value, "event_ordinal"), Value::Int(n) if !n.as_str().starts_with('-')) {
        return Err(err("Invalid retained native event ordinal"));
    }
    turn_sources::bounded(get(value, "role"), 512)?;
    turn_sources::bounded(get(value, "attribution"), 512)?;
    let calls = bounded_ids(get(value, "tool_call_ids"), 1024)?;
    if calls.iter().collect::<BTreeSet<_>>().len() != calls.len() {
        return Err(err("Duplicate native tool invocation identity"));
    }
    Ok(value)
}

/// Named-entry presence without following a final link (`os.path.lexists`).
/// Baseline `Path.exists()` checks also use this: a dangling link is present
/// and its later read refuses.
pub(crate) fn lexists(path: &Path) -> Result<bool> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Native source path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Native source path has no name"))?;
    let absent = |error: &std::io::Error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
        )
    };
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(error) if absent(&error) => return Ok(false),
        Err(error) => return Err(ContractError::new(error.to_string())),
    };
    match chain.directory().stat_at(name) {
        Ok(_) => Ok(true),
        Err(error) if absent(&error) => Ok(false),
        Err(error) => Err(ContractError::new(error.to_string())),
    }
}
/// A trusted, bounded, sorted directory listing of UTF-8 names.
pub(crate) fn listing(path: &Path, maximum: usize, message: &str) -> Result<Vec<String>> {
    fs::trust::state(path)?;
    let chain = DirectoryChain::open(path).map_err(|e| ContractError::new(e.to_string()))?;
    let names = chain
        .directory()
        .entries_bounded(maximum)
        .map_err(|e| ContractError::new(e.to_string()))?
        .ok_or_else(|| err(message))?;
    chain
        .recheck()
        .map_err(|e| ContractError::new(e.to_string()))?;
    names
        .into_iter()
        .map(|name| name.into_string().map_err(|_| err(message)))
        .collect()
}
fn modified_ns(path: &Path) -> Result<i128> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Native source path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Native source path has no name"))?;
    let chain = DirectoryChain::open(parent).map_err(|e| ContractError::new(e.to_string()))?;
    let info = chain
        .directory()
        .stat_at(name)
        .map_err(|e| ContractError::new(e.to_string()))?;
    Ok(info.mtime_ns)
}

/// Baseline `read(path, maximum)`; `None` is the baseline FileNotFoundError.
pub fn read_optional(path: &Path, maximum: usize) -> Result<Option<Value>> {
    turn_sources::read(path, maximum)
}
/// Baseline `read(path, maximum)` where absence is a refusal.
pub fn read(path: &Path, maximum: usize) -> Result<Value> {
    read_optional(path, maximum)?.ok_or_else(|| err("Retained native source record is unavailable"))
}

/// Baseline `retain`: immutable evidence; an equal existing record is reused.
pub fn retain(path: &Path, value: &Value, maximum: usize) -> Result<String> {
    let parent = path
        .parent()
        .ok_or_else(|| err("Native source path has no parent"))?;
    turn_sources::private_mkdir(parent)?;
    if !turn_sources::exclusive(path, value, maximum)? && !same(&read(path, maximum)?, value) {
        return Err(err(
            "Immutable native evidence changed; preserve both sources",
        ));
    }
    digest(value)
}

/// Baseline `replace_cursor`: exclusive temporary, native rename, directory
/// fsync and readback; the temporary never survives.
pub fn replace_cursor(path: &Path, value: &Value) -> Result<()> {
    if turn_sources::encoded(value)?.len() > MAX_CURSOR {
        return Err(err("Native cursor capacity reached"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| err("Native source path has no parent"))?;
    let target = path
        .file_name()
        .ok_or_else(|| err("Native source path has no name"))?;
    let name = format!(".cursor-{}", crate::host::uuid4()?);
    let temporary = parent.join(&name);
    turn_sources::exclusive(&temporary, value, MAX_CURSOR)?;
    let outcome = (|| -> Result<()> {
        let chain = DirectoryChain::open(parent).map_err(|e| ContractError::new(e.to_string()))?;
        fs::rename_at(
            chain.directory(),
            OsStr::new(&name),
            chain.directory(),
            target,
            RenameMode::Replace,
        )
        .map_err(|e| ContractError::new(e.to_string()))?;
        chain
            .recheck()
            .map_err(|e| ContractError::new(e.to_string()))?;
        turn_sources::sync(parent)?;
        if !same(&read(path, MAX_CURSOR)?, value) {
            return Err(err("Native cursor durable readback failed"));
        }
        Ok(())
    })();
    if lexists(&temporary)? {
        let chain = DirectoryChain::open(parent).map_err(|e| ContractError::new(e.to_string()))?;
        chain
            .directory()
            .unlink_at(OsStr::new(&name))
            .map_err(|e| ContractError::new(e.to_string()))?;
    }
    outcome
}

/// Baseline `session_dir`.
pub fn session_dir(site: &Site<'_>, session: &str) -> Result<PathBuf> {
    turn_sources::bounded(&string(session), 512)?;
    Ok(site.base()?.join("omp-source-v1").join(sha_text(session)))
}

fn hex64_name(name: &str, suffix: &str) -> bool {
    name.strip_suffix(suffix).is_some_and(|stem| {
        stem.len() == 64
            && stem
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn dash36_name(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|stem| {
        stem.len() == 36
            && stem
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) || byte == b'-')
    })
}

/// Baseline `directories(path, r'[0-9a-f]{64}')`.
pub fn directories(path: &Path) -> Result<Vec<PathBuf>> {
    if !lexists(path)? {
        return Ok(Vec::new());
    }
    let names = listing(
        path,
        MAX_ENTRIES,
        "Unknown or oversized native source inventory",
    )?;
    let mut values = Vec::new();
    for name in names {
        if !hex64_name(&name, "") {
            return Err(err("Unknown or oversized native source inventory"));
        }
        let child = path.join(name);
        fs::trust::state(&child)?;
        values.push(child);
    }
    Ok(values)
}

fn unique_strings(value: &Value, maximum: usize) -> Result<()> {
    let items = value
        .as_array()
        .filter(|items| items.len() <= maximum)
        .ok_or_else(|| err("Invalid native control identity inventory"))?;
    let mut seen = BTreeSet::new();
    for item in items {
        let native = turn_sources::bounded(item, 512)
            .map_err(|_| err("Invalid native control identity inventory"))?;
        if !seen.insert(native) {
            return Err(err("Invalid native control identity inventory"));
        }
    }
    Ok(())
}

fn control_value(control: &Value) -> Result<()> {
    const FIELDS: [&str; 11] = [
        "capture_order",
        "metadata_matches",
        "tool_metadata_matches",
        "metadata_predecessors",
        "pure_entries",
        "pure_mode",
        "pure_tool_ids",
        "native_trigger",
        "generated_guidance",
        "human_entry",
        "absent_native_entries",
    ];
    let pairs = control
        .as_object()
        .filter(|pairs| pairs.iter().all(|(key, _)| FIELDS.contains(&key.as_str())))
        .ok_or_else(|| err("Unknown native control interval fields"))?;
    for (name, values) in pairs {
        match name.as_str() {
            "capture_order" | "absent_native_entries" => unique_strings(values, MAX_CAPTURES)?,
            "pure_tool_ids" => unique_strings(values, 1024)?,
            "metadata_matches" | "tool_metadata_matches" | "metadata_predecessors" => {
                let mapping = values
                    .as_object()
                    .filter(|mapping| mapping.len() <= MAX_CAPTURES)
                    .ok_or_else(|| err("Invalid native metadata mapping inventory"))?;
                for (identifier, native) in mapping {
                    canonical_uuid(&string(identifier))?;
                    if native != &Value::Null {
                        turn_sources::bounded(native, 512)?;
                    }
                }
            }
            "pure_entries" => {
                let entries = values
                    .as_object()
                    .filter(|entries| entries.len() <= MAX_CAPTURES)
                    .ok_or_else(|| err("Invalid native own-control source inventory"))?;
                for (native, item) in entries {
                    turn_sources::bounded(&string(native), 512)?;
                    exact(item, &["snapshot_ref", "snapshot_sha256"])?;
                    snapshot_path(Path::new("."), get(item, "snapshot_ref"))?;
                    if !hex64_name(get(item, "snapshot_sha256").as_str().unwrap_or(""), "")
                        || get(item, "snapshot_sha256").as_str().is_none()
                    {
                        return Err(err("Invalid native own-control snapshot digest"));
                    }
                }
            }
            "pure_mode" => {
                if values.as_bool().is_none() {
                    return Err(err("Invalid native control classification"));
                }
            }
            "native_trigger" => {
                if !values.as_str().is_some_and(|t| TRIGGERS.contains(&t)) {
                    return Err(err("Unknown native control trigger"));
                }
            }
            "human_entry" => {
                turn_sources::bounded(values, 512)?;
            }
            _ => {
                turn_sources::bounded(values, 8192)?;
            }
        }
    }
    Ok(())
}

/// Baseline `cursor_value(value, session, lifecycle)`.
pub fn cursor_value<'v>(value: &'v Value, session: &Value, lifecycle: &str) -> Result<&'v Value> {
    exact(
        value,
        &[
            "schema",
            "session",
            "lifecycle",
            "origin",
            "last_native_entry",
            "event_ordinal",
            "control_interval",
            "tool_lifecycles",
        ],
    )?;
    if get(value, "schema").as_str() != Some("agent-memory/omp-cursor-v1")
        || get(value, "session") != session
        || get(value, "lifecycle").as_str() != Some(lifecycle)
    {
        return Err(err("Foreign native cursor"));
    }
    canonical_uuid(&string(lifecycle))?;
    let origin = exact(
        get(value, "origin"),
        &["kind", "human_root", "native_trigger"],
    )?;
    canonical_uuid(get(origin, "human_root"))?;
    let trigger = get(origin, "native_trigger").as_str();
    let kind = get(origin, "kind").as_str();
    let Some(trigger) = trigger.filter(|t| TRIGGERS.contains(t)) else {
        return Err(err("Unknown native origin"));
    };
    if !matches!(kind, Some("genuine-user" | "foreign-continuation")) {
        return Err(err("Unknown native origin"));
    }
    if (kind == Some("genuine-user")) != HUMAN_TRIGGERS.contains(&trigger) {
        return Err(err("Native origin attribution differs"));
    }
    ordinal(value)?;
    let tools = get(value, "tool_lifecycles")
        .as_object()
        .filter(|tools| tools.len() <= 1024)
        .ok_or_else(|| err("Native active tool binding capacity reached"))?;
    for (key, owner) in tools {
        turn_sources::bounded(&string(key), 512)?;
        canonical_uuid(owner)?;
    }
    let last = get(value, "last_native_entry");
    if last != &Value::Null {
        turn_sources::bounded(last, 512)?;
    }
    let control = get(value, "control_interval");
    if control != &Value::Null {
        control_value(control)?;
    }
    Ok(value)
}

/// Baseline `validate_root_wakes(path, session)`: the native user entries
/// already holding a requested root wake.
pub fn validate_root_wakes(path: &Path, session: Option<&str>) -> Result<BTreeSet<String>> {
    let names = listing(
        path,
        MAX_ENTRIES,
        "Native root wake inventory exceeds bound",
    )?;
    let namespace = path
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .ok_or_else(|| err("Foreign native root wake"))?;
    let mut users = BTreeSet::new();
    for name in names {
        let stem = name
            .strip_suffix(".json")
            .ok_or_else(|| err("Unknown native root wake record"))?;
        let root = canonical_uuid(&string(stem))?.to_owned();
        let value = read(&path.join(&name), MAX_RECORD)?;
        exact(
            &value,
            &[
                "schema",
                "session",
                "human_root",
                "native_user_entry",
                "request_id",
                "status",
            ],
        )?;
        let owner = get(&value, "session").as_str();
        if get(&value, "schema").as_str() != Some("agent-memory/omp-root-wake-v1")
            || get(&value, "human_root").as_str() != Some(root.as_str())
            || get(&value, "status").as_str() != Some("requested")
            || session.is_some_and(|session| owner != Some(session))
            || owner.map(sha_text).as_deref() != Some(namespace)
        {
            return Err(err("Foreign native root wake"));
        }
        let user = turn_sources::bounded(get(&value, "native_user_entry"), 512)?.to_owned();
        canonical_uuid(get(&value, "request_id"))?;
        let parent = path
            .parent()
            .ok_or_else(|| err("Foreign native root wake"))?;
        let cursor = read(&parent.join(&root).join("cursor.json"), MAX_PAYLOAD)?;
        cursor_value(&cursor, get(&value, "session"), &root)?;
        users.insert(user);
    }
    Ok(users)
}

/// Baseline `segments(repo, session)`: ordered by `(event_ordinal, name)`.
pub fn segments(site: &Site<'_>, session: &str) -> Result<Vec<Seg>> {
    let directory = session_dir(site, session)?;
    if !lexists(&directory)? {
        return Ok(Vec::new());
    }
    let names = listing(
        &directory,
        MAX_ENTRIES,
        "Native session inventory exceeds bound",
    )?;
    let mut result = Vec::new();
    for name in names {
        if name == "root-wakes" {
            continue; // Producer source capture is independent of wake-state faults.
        }
        canonical_uuid(&string(&name))?;
        let child = directory.join(&name);
        fs::trust::state(&child)?;
        let cursor = read(&child.join("cursor.json"), MAX_CURSOR)?;
        cursor_value(&cursor, &string(session), &name)?;
        result.push((ordinal(&cursor)?, name, Segment::new(child, cursor)));
    }
    result.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(result.into_iter().map(|(_, _, segment)| segment).collect())
}

/// Baseline `new_segment`.
pub fn new_segment(
    site: &Site<'_>,
    session: &str,
    lifecycle: &str,
    root: &str,
    trigger: &str,
    event_ordinal: u64,
) -> Result<Seg> {
    let namespace = session_dir(site, session)?;
    let directory = namespace.join(lifecycle);
    if root != lifecycle {
        let root_directory = namespace.join(root);
        let root_cursor = read(&root_directory.join("cursor.json"), MAX_PAYLOAD)?;
        cursor_value(&root_cursor, &string(session), root)?;
        if !source_bound_origin(&root_directory, &root_cursor)? {
            return Err(err("Cannot extend an unbound native root family"));
        }
    }
    turn_sources::private_mkdir(&directory)?;
    let kind = if HUMAN_TRIGGERS.contains(&trigger) {
        "genuine-user"
    } else {
        "foreign-continuation"
    };
    let mut cursor = object([
        ("schema", string("agent-memory/omp-cursor-v1")),
        ("session", string(session)),
        ("lifecycle", string(lifecycle)),
        (
            "origin",
            object([
                ("kind", string(kind)),
                ("human_root", string(root)),
                ("native_trigger", string(trigger)),
            ]),
        ),
        ("last_native_entry", Value::Null),
        ("event_ordinal", Value::Int(event_ordinal.into())),
        ("control_interval", Value::Null),
        ("tool_lifecycles", Value::Object(Vec::new())),
    ]);
    let path = directory.join("cursor.json");
    if !lexists(&path)? {
        retain(&path, &cursor, MAX_CURSOR)?;
    } else {
        cursor = read(&path, MAX_PAYLOAD)?;
        cursor_value(&cursor, &string(session), lifecycle)?;
        if !source_bound_origin(&directory, &cursor)? {
            return Err(err("Cannot reuse an unbound native root family"));
        }
    }
    Ok(Segment::new(directory, cursor))
}

/// Baseline `source_bound_origin`: historical notification-only cursors never
/// acquire source authority.
pub fn source_bound_origin(directory: &Path, cursor: &Value) -> Result<bool> {
    let root = field_text(field(cursor, "origin")?, "human_root")?;
    let session = field(cursor, "session")?;
    if Some(root) != get(cursor, "lifecycle").as_str() {
        let parent = directory
            .parent()
            .ok_or_else(|| err("Native segment has no session namespace"))?;
        let root_directory = parent.join(root);
        let Some(root_cursor) = read_optional(&root_directory.join("cursor.json"), MAX_PAYLOAD)?
        else {
            return Ok(false);
        };
        cursor_value(&root_cursor, session, root)?;
        if get(get(&root_cursor, "origin"), "human_root").as_str() != Some(root) {
            return Ok(false);
        }
        return source_bound_origin(&root_directory, &root_cursor);
    }
    if get(get(cursor, "origin"), "kind").as_str() != Some("genuine-user") {
        return Ok(true);
    }
    let native = get(get(cursor, "control_interval"), "human_entry");
    let Some(native) = native.as_str().filter(|n| !n.is_empty()) else {
        return Ok(false);
    };
    let base = directory
        .ancestors()
        .nth(3)
        .ok_or_else(|| err("Native segment has no private base"))?;
    let binding = read(&base.join("omp-bank-binding-v1.json"), MAX_RECORD)?;
    let Some(repository) = get(&binding, "repository")
        .as_str()
        .filter(|r| r.starts_with('/'))
    else {
        return Ok(false);
    };
    let resolved = fs::canonical_path(Path::new(repository), false)?;
    let session = text(session)?;
    let expected = uuid5_url(&format!("{}:omp:{session}:{native}", path_text(&resolved)?));
    if get(cursor, "lifecycle").as_str() != Some(expected.as_str()) {
        return Ok(false);
    }
    let target = directory
        .join("captures")
        .join(format!("{}.json", sha_text(native)));
    if lexists(&target)? {
        let captured = read(&target, MAX_RECORD)?;
        let snapshot = read(
            &snapshot_path(directory, field(&captured, "snapshot_ref")?)?,
            MAX_PAYLOAD,
        )?;
        return Ok(
            get(&captured, "schema").as_str() == Some("agent-memory/omp-capture-v1")
                && get(&captured, "session").as_str() == Some(session)
                && get(&captured, "lifecycle") == get(cursor, "lifecycle")
                && field(&captured, "native_entry_id")?.as_str() == Some(native)
                && get(&snapshot, "native_entry_id").as_str() == Some(native)
                && Some(digest(&snapshot)?.as_str()) == get(&captured, "snapshot_sha256").as_str()
                && get(&snapshot, "role").as_str() == Some("user")
                && get(&snapshot, "attribution").as_str() == Some("user"),
        );
    }
    // A checked stored user can itself be unsafe/unavailable. Its exact-ID gap
    // preserves that real origin without admitting its payload or a metadata root.
    for path in files(directory, "gaps", Names::Dash36)? {
        let value = read(&path, MAX_RECORD)?;
        if get(&value, "schema").as_str() == Some("agent-memory/omp-gap-v1")
            && get(&value, "session").as_str() == Some(session)
            && get(&value, "lifecycle") == get(cursor, "lifecycle")
            && get(&value, "native_entry_id").as_str() == Some(native)
            && matches!(
                get(&value, "reason").as_str(),
                Some("unsafe-representation" | "oversized-source" | "missing-payload")
            )
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The first element with the greatest key (Python `max`).
fn first_max<T: Clone>(items: &[T], key: impl Fn(&T) -> Result<u64>) -> Result<Option<T>> {
    let mut best: Option<(u64, &T)> = None;
    for item in items {
        let value = key(item)?;
        if best.is_none_or(|(current, _)| value > current) {
            best = Some((value, item));
        }
    }
    Ok(best.map(|(_, item)| item.clone()))
}

/// Baseline `active_segment(current, branch_ids)`.
pub fn active_segment(current: &[Seg], branch_ids: Option<&[String]>) -> Result<Option<Seg>> {
    let mut invalid = BTreeSet::new();
    for segment in current {
        if !segment.bound()? {
            invalid.insert(segment.root()?);
        }
    }
    let mut kept = Vec::new();
    for segment in current {
        if !invalid.contains(&segment.root()?) {
            kept.push(Rc::clone(segment));
        }
    }
    if kept.is_empty() {
        return Ok(None);
    }
    let genuine = |segment: &Seg| -> Result<bool> {
        Ok(get(get(&*segment.cursor()?, "origin"), "kind").as_str() == Some("genuine-user"))
    };
    let mut humans = Vec::new();
    for segment in &kept {
        if genuine(segment)? {
            humans.push(Rc::clone(segment));
        }
    }
    if let Some(branch) = branch_ids {
        let mut on_branch = Vec::new();
        for segment in humans {
            let entry = get(get(&*segment.cursor()?, "control_interval"), "human_entry")
                .as_str()
                .map(str::to_owned);
            if entry.is_some_and(|entry| branch.contains(&entry)) {
                on_branch.push(segment);
            }
        }
        humans = on_branch;
        if humans.is_empty() {
            for segment in kept.iter().rev() {
                if !genuine(segment)? {
                    return Ok(Some(Rc::clone(segment)));
                }
            }
            return Ok(None);
        }
    }
    if humans.is_empty() {
        return Ok(kept.last().cloned());
    }
    let Some(latest) = first_max(&humans, |segment| segment.ordinal())? else {
        return Ok(None);
    };
    let root = latest.root()?;
    let mut family = Vec::new();
    for segment in &kept {
        if segment.root()? == root {
            family.push(Rc::clone(segment));
        }
    }
    first_max(&family, |segment| segment.ordinal())
}

pub(crate) fn max_ordinal(current: &[Seg], floor: u64) -> Result<u64> {
    let mut maximum = floor;
    for segment in current {
        maximum = maximum.max(segment.ordinal()?);
    }
    maximum
        .checked_add(1)
        .ok_or_else(|| err("Invalid native ordinal"))
}

/// Baseline `writable_successor`.
pub fn writable_successor(
    site: &Site<'_>,
    session: &str,
    owner: Seg,
    current: &mut Vec<Seg>,
    event_ordinal: u64,
) -> Result<Seg> {
    if !owner.sealed()? {
        return Ok(owner);
    }
    let root = owner.root()?;
    let mut existing = None;
    for segment in current.iter() {
        if segment.root()? == root && segment.bound()? && !segment.sealed()? {
            existing = Some(Rc::clone(segment));
        }
    }
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let successor = new_segment(
        site,
        session,
        &crate::host::uuid4()?,
        &root,
        "foreign-hidden",
        max_ordinal(current, event_ordinal)?,
    )?;
    current.push(Rc::clone(&successor));
    Ok(successor)
}

/// File-name families of a segment's immutable record directories.
#[derive(Clone, Copy)]
pub enum Names {
    /// `[0-9a-f]{64}\.json`
    Hex64,
    /// `[0-9a-f-]{36}\.json`
    Dash36,
}

/// Baseline `_files(directory, child, pattern)`.
pub fn files(directory: &Path, child: &str, names: Names) -> Result<Vec<PathBuf>> {
    let path = directory.join(child);
    if !lexists(&path)? {
        return Ok(Vec::new());
    }
    let message = "Native capture inventory exceeds bound or has unknown entries";
    let mut result = Vec::new();
    for name in listing(&path, MAX_CAPTURES, message)? {
        let known = match names {
            Names::Hex64 => hex64_name(&name, ".json"),
            Names::Dash36 => dash36_name(&name),
        };
        if !known {
            return Err(err(message));
        }
        result.push(path.join(name));
    }
    Ok(result)
}

/// Baseline `_snapshot_path`.
pub fn snapshot_path(directory: &Path, reference: &Value) -> Result<PathBuf> {
    let valid = reference
        .as_str()
        .and_then(|r| r.strip_prefix("snapshots/"))
        .is_some_and(|name| hex64_name(name, ".json"));
    if !valid {
        return Err(err("Invalid native snapshot reference"));
    }
    Ok(directory.join(text(reference)?))
}
fn snapshot_reference(native: &str) -> String {
    format!("snapshots/{}.json", sha_text(native))
}

/// Baseline `native_disposition_ids`: exact safely retained/qualified IDs;
/// neutral hints are never source evidence.
pub fn native_disposition_ids(site: &Site<'_>, session: &str) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for segment in segments(site, session)? {
        let directory = &segment.directory;
        let cursor = segment.cursor()?;
        if !source_bound_origin(directory, &cursor)? {
            continue;
        }
        for path in files(directory, "captures", Names::Hex64)? {
            let captured = read(&path, MAX_RECORD)?;
            let snapshot = read(
                &snapshot_path(directory, field(&captured, "snapshot_ref")?)?,
                MAX_PAYLOAD,
            )?;
            let native = field_text(&captured, "native_entry_id")?;
            if get(&snapshot, "native_entry_id").as_str() != Some(native)
                || Some(digest(&snapshot)?.as_str()) != get(&captured, "snapshot_sha256").as_str()
            {
                return Err(err("Native disposition snapshot identity changed"));
            }
            result.insert(native.to_owned());
        }
        if let Some(entries) = get(&control_of(&cursor), "pure_entries").as_object() {
            for (native, captured) in entries {
                let snapshot = read(
                    &snapshot_path(directory, field(captured, "snapshot_ref")?)?,
                    MAX_PAYLOAD,
                )?;
                if Some(digest(&snapshot)?.as_str()) != get(captured, "snapshot_sha256").as_str() {
                    return Err(err("Native control disposition changed"));
                }
                result.insert(native.clone());
            }
        }
        for path in files(directory, "gaps", Names::Dash36)? {
            let value = read(&path, MAX_RECORD)?;
            if let Some(native) = get(&value, "native_entry_id").as_str() {
                if !native.starts_with("event:")
                    && !native.starts_with("tool:")
                    && get(&value, "session").as_str() == Some(session)
                    && get(&value, "lifecycle") == get(&cursor, "lifecycle")
                    && get(&value, "reason")
                        .as_str()
                        .is_some_and(|reason| GAPS.contains(&reason))
                {
                    result.insert(native.to_owned());
                }
            }
        }
    }
    Ok(result)
}

fn tool_call_ids(snapshot: &Value) -> Result<Vec<Value>> {
    let mut calls = Vec::new();
    if let Some(content) = get(snapshot, "content").as_array() {
        for block in content {
            if block.as_object().is_some() && get(block, "type").as_str() == Some("toolCall") {
                calls.push(field(block, "id")?.clone());
            }
        }
    }
    Ok(calls)
}

/// Baseline `_enroll`.
fn enroll(
    directory: &Path,
    cursor: &Value,
    reference: &str,
    snapshot: &Value,
    kind: &str,
    native: &str,
) -> Result<()> {
    let snapshot_ref = if kind == "segment" {
        "segment.json".to_owned()
    } else {
        snapshot_reference(native)
    };
    let record = object([
        ("schema", string("agent-memory/omp-enrollment-v1")),
        ("session", field(cursor, "session")?.clone()),
        ("lifecycle", field(cursor, "lifecycle")?.clone()),
        ("source_reference", string(reference)),
        ("source_sha256", string(digest(snapshot)?)),
        ("snapshot_ref", string(snapshot_ref)),
        ("kind", string(kind)),
        ("native_source_id", string(native)),
    ]);
    let name = format!("{}.json", digest(&string(reference))?);
    retain(
        &directory.join("enrollments").join(name),
        &record,
        MAX_RECORD,
    )?;
    if kind == "admission" {
        let capture_id = reference
            .rsplit_once(':')
            .map(|(_, id)| id)
            .unwrap_or(reference);
        let admission = object([
            ("schema", string("agent-memory/omp-admission-v1")),
            ("session", field(cursor, "session")?.clone()),
            ("lifecycle", field(cursor, "lifecycle")?.clone()),
            ("capture_id", string(capture_id)),
            ("event_ordinal", field(cursor, "event_ordinal")?.clone()),
            (
                "role",
                snapshot
                    .get("role")
                    .cloned()
                    .unwrap_or_else(|| string("custom")),
            ),
            (
                "attribution",
                snapshot
                    .get("attribution")
                    .cloned()
                    .unwrap_or_else(|| string("unknown")),
            ),
            ("tool_call_ids", Value::Array(tool_call_ids(snapshot)?)),
        ]);
        retain(&directory.join("admission.json"), &admission, MAX_RECORD)?;
    }
    Ok(())
}

/// Baseline `gap(directory, cursor, native_id, reason)`.
pub fn gap(directory: &Path, cursor: &Value, native: Option<&str>, reason: &str) -> Result<Value> {
    if !GAPS.contains(&reason) {
        return Err(err("Unknown coverage gap"));
    }
    let lifecycle = field_text(cursor, "lifecycle")?;
    let gap_id = uuid5(lifecycle, &format!("{}:{reason}", native.unwrap_or("None")))?;
    let value = object([
        ("schema", string("agent-memory/omp-gap-v1")),
        ("session", field(cursor, "session")?.clone()),
        ("lifecycle", string(lifecycle)),
        ("gap_id", string(&gap_id)),
        ("native_entry_id", native.map(string).unwrap_or(Value::Null)),
        ("reason", string(reason)),
    ]);
    retain(
        &directory.join("gaps").join(format!("{gap_id}.json")),
        &value,
        MAX_RECORD,
    )?;
    Ok(value)
}
pub(crate) fn segment_gap(segment: &Seg, native: Option<&str>, reason: &str) -> Result<Value> {
    gap(&segment.directory, &*segment.cursor()?, native, reason)
}

/// Baseline `_retained_gap`: live gaps and immutable manifests, including this sweep.
pub fn retained_gap(current: &[Seg], identities: &[&str]) -> Result<bool> {
    for segment in current {
        for path in files(&segment.directory, "gaps", Names::Dash36)? {
            let value = read(&path, MAX_RECORD)?;
            if field(&value, "native_entry_id")?
                .as_str()
                .is_some_and(|native| identities.contains(&native))
            {
                return Ok(true);
            }
        }
        if segment.sealed()? {
            let manifest = manifest_value(&segment.directory, &*segment.cursor()?)?;
            if manifest_gaps(&manifest)?.iter().any(|item| {
                get(item, "native_entry_id")
                    .as_str()
                    .is_some_and(|native| identities.contains(&native))
            }) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(crate) fn manifest_gaps(manifest: &Value) -> Result<&[Value]> {
    field(manifest, "gaps")?
        .as_array()
        .ok_or_else(|| err("Foreign or oversized native sealed manifest"))
}

/// Same object identity of a boolean flag (`a is b` for True/False/None).
fn identical_flag(left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (Value::Bool(a), Value::Bool(b)) if a == b
    ) || (left == &Value::Null && right == &Value::Null)
}

fn existing_projection(path: &Path, entry: &Value, native: &str, persisted: bool) -> Result<Value> {
    let snapshot = read(path, MAX_PAYLOAD)?;
    if !omp_projection::valid_instruction_context(&snapshot) {
        return Err(err("Invalid retained native instruction context"));
    }
    if snapshot.get("instruction_context").is_some() {
        let message = get(entry, "message");
        if get(entry, "type").as_str() != Some("message")
            || message.as_object().is_none()
            || get(message, "role").as_str() != Some("toolResult")
            || get(&snapshot, "native_entry_id").as_str() != Some(native)
            || get(&snapshot, "native_parent_id") != get(entry, "parentId")
            || get(&snapshot, "toolCallId") != get(message, "toolCallId")
            || get(&snapshot, "toolName") != get(message, "toolName")
            || !identical_flag(get(&snapshot, "isError"), get(message, "isError"))
            || snapshot.get("attribution").is_some() != message.get("attribution").is_some()
            || get(&snapshot, "attribution") != get(message, "attribution")
        {
            return Err(err(
                "Retained instruction context differs from native identity",
            ));
        }
    } else if persisted {
        // Native persistence may truncate otherwise safe bodies. Reuse the
        // already screened immutable snapshot by exact structural identity,
        // never compare/replace it with a persisted rendering.
        let is_message = get(entry, "type").as_str() == Some("message");
        let message = if is_message {
            get(entry, "message")
        } else {
            entry
        };
        let keys = [
            "toolCallId",
            "toolName",
            "isError",
            "attribution",
            "customType",
        ];
        if get(&snapshot, "carrier") != get(entry, "type")
            || get(&snapshot, "native_entry_id").as_str() != Some(native)
            || get(&snapshot, "native_parent_id") != get(entry, "parentId")
            || (is_message && get(&snapshot, "role") != get(message, "role"))
            || keys.iter().any(|key| {
                (snapshot.get(key).is_some() || message.get(key).is_some())
                    && !same(get(&snapshot, key), get(message, key))
            })
        {
            return Err(err("Retained native snapshot identity differs"));
        }
    } else {
        let original = omp_projection::project(entry);
        if !original.get("snapshot").is_some_and(|s| same(s, &snapshot)) {
            return Err(err("Retained native snapshot differs from its projection"));
        }
    }
    Ok(snapshot)
}

/// An exact native full read and its installation witness (baseline
/// `invocation=` / `witness=` of `capture`).
pub struct ManualRead<'a> {
    pub invocation: &'a Value,
    pub witness: &'a OwnedManual,
}

fn manual_projection(entry: &Value, manual: &ManualRead<'_>) -> Result<Option<Value>> {
    let verified = omp_projection::comparison_of(manual.witness);
    let Some(projected) =
        omp_projection::project_owned_manual(entry, manual.invocation, manual.witness, &verified)
    else {
        return Ok(None);
    };
    match manual.witness.verify() {
        Ok(Some(current)) if current == verified => Ok(Some(projected)),
        Ok(_) | Err(omp_installation::Error::Contract(_)) => Ok(None),
        Err(omp_installation::Error::MissingActivation) => Err(ContractError::missing_provider(
            omp_installation::Error::MissingActivation.to_string(),
        )),
    }
}

/// Baseline `capture(directory, cursor, entry, mapping=…, invocation=…,
/// witness=…, persisted=…)`: `None` when the entry became a coverage gap.
pub fn capture(
    directory: &Path,
    cursor: &Value,
    entry: &Value,
    mapping: &str,
    manual: Option<&ManualRead<'_>>,
    persisted: bool,
) -> Result<Option<Value>> {
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.capture.before-commit")?;
    let native = turn_sources::bounded(get(entry, "id"), 512)?.to_owned();
    let name = sha_text(&native);
    let snapshot_ref = snapshot_reference(&native);
    let path = directory.join(&snapshot_ref);
    let sealed = lexists(&directory.join("segment.json"))?;
    let snapshot = if lexists(&path)? {
        existing_projection(&path, entry, &native, persisted)?
    } else {
        let projected = match manual {
            Some(manual) => manual_projection(entry, manual)?,
            None => None,
        };
        let projected = projected.unwrap_or_else(|| omp_projection::project(entry));
        if let Some(reason) = projected.get("gap").and_then(Value::as_str) {
            if !sealed {
                gap(directory, cursor, Some(&native), reason)?;
            }
            return Ok(None);
        }
        field(&projected, "snapshot")?.clone()
    };
    let source_hash = retain(&path, &snapshot, MAX_PAYLOAD)?;
    let lifecycle = field_text(cursor, "lifecycle")?;
    let record = object([
        ("schema", string("agent-memory/omp-capture-v1")),
        ("session", field(cursor, "session")?.clone()),
        ("lifecycle", string(lifecycle)),
        ("capture_id", string(uuid5(lifecycle, &native)?)),
        ("native_entry_id", string(&native)),
        ("native_parent_id", get(entry, "parentId").clone()),
        ("snapshot_ref", string(&snapshot_ref)),
        ("snapshot_sha256", string(&source_hash)),
        ("representation", string("native-stored-learning-v1")),
        ("mapping", string(mapping)),
        ("exclusions", exclusions()),
    ]);
    retain(
        &directory.join("captures").join(format!("{name}.json")),
        &record,
        MAX_RECORD,
    )?;
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.capture.after-commit")?;
    // Only the observer ingest path can resolve a previously sealed gap.
    if sealed {
        let manifest_hash = digest(&read(&directory.join("segment.json"), MAX_PAYLOAD)?)?;
        for gap_path in files(directory, "gaps", Names::Dash36)? {
            let old = read(&gap_path, MAX_RECORD)?;
            let reason = field(&old, "reason")?.as_str();
            if field(&old, "native_entry_id")?.as_str() == Some(native.as_str())
                && reason.is_some_and(|reason| RESOLVABLE.contains(&reason))
            {
                let gap_id = field_text(&old, "gap_id")?;
                let resolution = object([
                    ("schema", string("agent-memory/omp-gap-resolution-v1")),
                    ("manifest_sha256", string(&manifest_hash)),
                    ("gap_id", string(gap_id)),
                    ("recovered_ref", string(&snapshot_ref)),
                    ("recovered_sha256", string(&source_hash)),
                    ("reason", string("recovered-safe-native-source")),
                ]);
                retain(
                    &directory.join("resolutions").join(format!("{gap_id}.json")),
                    &resolution,
                    MAX_RECORD,
                )?;
            }
        }
    }
    Ok(Some(record))
}

/// Baseline `matched_metadata(cursor)`.
pub fn matched_metadata(cursor: &Value) -> BTreeMap<String, Value> {
    let control = control_of(cursor);
    let mut result = BTreeMap::new();
    for name in ["metadata_matches", "tool_metadata_matches"] {
        if let Some(pairs) = get(&control, name).as_object() {
            for (key, value) in pairs {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    result
}

fn captures_with_ref(directory: &Path, reference: &Value) -> Result<Vec<Value>> {
    let mut matching = Vec::new();
    for path in files(directory, "captures", Names::Hex64)? {
        let captured = read(&path, MAX_RECORD)?;
        if same(field(&captured, "snapshot_ref")?, reference) {
            matching.push(captured);
        }
    }
    Ok(matching)
}

/// Baseline `enroll_sealed`.
pub fn enroll_sealed(directory: &Path, cursor: &Value, manifest: &Value) -> Result<()> {
    if !source_bound_origin(directory, cursor)? {
        return Err(err("Cannot enroll an unbound native root family"));
    }
    let reference = field(manifest, "admission_ref")?;
    if reference != &Value::Null {
        let matching = captures_with_ref(directory, reference)?;
        let [admission] = matching.as_slice() else {
            return Err(err("Sealed admission no longer names one retained capture"));
        };
        let snapshot = read(&snapshot_path(directory, reference)?, MAX_PAYLOAD)?;
        enroll(
            directory,
            cursor,
            &format!("omp:capture:{}", field_text(admission, "capture_id")?),
            &snapshot,
            "admission",
            field_text(admission, "native_entry_id")?,
        )?;
    }
    let seal_id = field_text(manifest, "seal_id")?;
    enroll(
        directory,
        cursor,
        &format!("omp:seal:{seal_id}"),
        manifest,
        "segment",
        seal_id,
    )
}

fn read_all(directory: &Path, child: &str, names: Names) -> Result<Vec<Value>> {
    files(directory, child, names)?
        .iter()
        .map(|path| read(path, MAX_RECORD))
        .collect()
}

fn previous_digest(directory: &Path, cursor: &Value) -> Result<Value> {
    let parent = directory
        .parent()
        .ok_or_else(|| err("Native segment has no session namespace"))?;
    let own = directory.file_name().and_then(OsStr::to_str);
    let current = ordinal(cursor)?;
    let mut previous: Option<(u64, String)> = None;
    for name in listing(
        parent,
        MAX_ENTRIES,
        "Native session inventory exceeds bound",
    )? {
        if name == "root-wakes" || Some(name.as_str()) == own {
            continue;
        }
        canonical_uuid(&string(&name))?;
        let other = parent.join(&name);
        let old = read(&other.join("cursor.json"), MAX_PAYLOAD)?;
        cursor_value(&old, field(cursor, "session")?, &name)?;
        if ordinal(&old)? < current && lexists(&other.join("segment.json"))? {
            let candidate = (
                ordinal(&old)?,
                digest(&read(&other.join("segment.json"), MAX_PAYLOAD)?)?,
            );
            if previous.as_ref().is_none_or(|best| candidate > *best) {
                previous = Some(candidate);
            }
        }
    }
    Ok(previous
        .map(|(_, digest)| string(digest))
        .unwrap_or(Value::Null))
}

/// Baseline `seal(directory, cursor)`.
pub fn seal(directory: &Path, cursor: &Value) -> Result<Value> {
    if !source_bound_origin(directory, cursor)? {
        return Err(err(
            "Legacy notification-only native origin cannot be sealed",
        ));
    }
    if lexists(&directory.join("segment.json"))? {
        let manifest = manifest_value(directory, cursor)?;
        enroll_sealed(directory, cursor, &manifest)?;
        return Ok(manifest);
    }
    let mut captures = read_all(directory, "captures", Names::Hex64)?;
    let mut keyed = Vec::new();
    for captured in captures.drain(..) {
        // Retained snapshots carry no ordinal: the baseline key is `(0, native)`.
        read(
            &snapshot_path(directory, field(&captured, "snapshot_ref")?)?,
            MAX_PAYLOAD,
        )?;
        keyed.push((
            field_text(&captured, "native_entry_id")?.to_owned(),
            captured,
        ));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    // Capture sequence is recorded at ingestion, not inferred from lexical native IDs.
    let control = control_of(cursor);
    let sequence: Vec<&str> = get(&control, "capture_order")
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    keyed.sort_by_key(|(native, _)| {
        sequence
            .iter()
            .position(|item| item == native)
            .unwrap_or(sequence.len())
    });
    let captures: Vec<Value> = keyed.into_iter().map(|(_, captured)| captured).collect();
    if let Some(tools) = get(cursor, "tool_lifecycles").as_object() {
        for (call_id, _) in tools {
            gap(
                directory,
                cursor,
                Some(&format!("tool:{call_id}")),
                "missing-stored-source",
            )?;
        }
    }
    let mut gaps = read_all(directory, "gaps", Names::Dash36)?;
    if captures.is_empty() && gaps.is_empty() {
        gap(directory, cursor, None, "missing-stored-source")?;
        gaps = read_all(directory, "gaps", Names::Dash36)?;
    }
    let genuine = get(get(cursor, "origin"), "kind").as_str() == Some("genuine-user");
    let mut admission = None;
    if genuine {
        for captured in &captures {
            let snapshot = read(
                &snapshot_path(directory, field(captured, "snapshot_ref")?)?,
                MAX_PAYLOAD,
            )?;
            if get(&snapshot, "role").as_str() == Some("user")
                && get(&snapshot, "attribution").as_str() == Some("user")
            {
                admission = Some(captured);
                break;
            }
        }
    } else {
        admission = captures.first();
    }
    let lifecycle = field_text(cursor, "lifecycle")?;
    let mut items = Vec::new();
    for captured in &captures {
        items.push(object([
            ("ref", field(captured, "snapshot_ref")?.clone()),
            ("sha256", field(captured, "snapshot_sha256")?.clone()),
        ]));
    }
    let manifest = object([
        ("schema", string("agent-memory/omp-segment-v1")),
        ("session", field(cursor, "session")?.clone()),
        ("lifecycle", string(lifecycle)),
        ("origin", field(cursor, "origin")?.clone()),
        ("seal_id", string(uuid5(lifecycle, "seal")?)),
        (
            "admission_ref",
            match admission {
                Some(captured) => field(captured, "snapshot_ref")?.clone(),
                None => Value::Null,
            },
        ),
        ("captures", Value::Array(items)),
        ("gaps", Value::Array(gaps)),
        ("exclusions", exclusions()),
        ("previous_digest", previous_digest(directory, cursor)?),
    ]);
    retain(&directory.join("segment.json"), &manifest, MAX_PAYLOAD)?;
    enroll_sealed(directory, cursor, &manifest)?;
    Ok(manifest)
}
pub(crate) fn seal_segment(segment: &Seg) -> Result<Value> {
    let cursor = segment.snapshot()?;
    seal(&segment.directory, &cursor)
}

/// Baseline `enroll_admission`.
pub fn enroll_admission(directory: &Path, cursor: &Value, captured: Option<&Value>) -> Result<()> {
    let Some(captured) = captured else {
        return Ok(());
    };
    let snapshot = read(
        &snapshot_path(directory, field(captured, "snapshot_ref")?)?,
        MAX_PAYLOAD,
    )?;
    enroll(
        directory,
        cursor,
        &format!("omp:capture:{}", field_text(captured, "capture_id")?),
        &snapshot,
        "admission",
        field_text(captured, "native_entry_id")?,
    )
}

/// Baseline `_shared`: the shared source namespace of a native segment.
pub(crate) fn shared(site: &Site<'_>, cursor: &Value) -> Result<Namespace> {
    let session = field_text(cursor, "session")?;
    let lifecycle = field_text(cursor, "lifecycle")?;
    match site.context {
        Some(context) => {
            turn_sources::enrollment_namespace(site.repo, "omp", session, lifecycle, context)
        }
        None => Ok(turn_sources::private_namespace(
            site.repo,
            "omp",
            session,
            lifecycle,
            false,
            site.runtime_root,
        )?
        .0),
    }
}

/// Baseline `_shared_inventory`.
pub(crate) fn shared_inventory(site: &Site<'_>, cursor: &Value) -> Result<Value> {
    let session = field_text(cursor, "session")?;
    let lifecycle = field_text(cursor, "lifecycle")?;
    match site.context {
        Some(context) => {
            turn_sources::inspect_enrollment(site.repo, "omp", session, lifecycle, context)
        }
        None => turn_sources::inspect(site.repo, "omp", session, lifecycle, site.runtime_root),
    }
}

fn session_children(
    site: &Site<'_>,
    session_path: &Path,
    result: &mut Vec<Seg>,
    errors: &mut Option<&mut Vec<String>>,
) -> Result<()> {
    let names = listing(
        session_path,
        MAX_ENTRIES,
        "Native session inventory exceeds bound",
    )?;
    for name in names {
        let child = session_path.join(&name);
        let outcome = (|| -> Result<Option<Seg>> {
            if name == "root-wakes" {
                validate_root_wakes(&child, None)?;
                return Ok(None);
            }
            canonical_uuid(&string(&name))?;
            fs::trust::state(&child)?;
            let cursor = read(&child.join("cursor.json"), MAX_CURSOR)?;
            cursor_value(&cursor, get(&cursor, "session"), &name)?;
            if session_dir(site, field_text(&cursor, "session")?)? != session_path {
                return Err(err("Foreign native session namespace"));
            }
            Ok(Some(Segment::new(child.clone(), cursor)))
        })();
        match (outcome, errors.as_deref_mut()) {
            (Ok(Some(segment)), _) => result.push(segment),
            (Ok(None), _) => {}
            (Err(error), None) => return Err(error),
            (Err(_), Some(errors)) => errors.push("native-segment-unverifiable".to_owned()),
        }
    }
    Ok(())
}

/// Baseline `all_segments(repo, errors)`: with `errors`, unverifiable
/// sessions/segments become fault codes instead of refusals.
pub fn all_segments(site: &Site<'_>, mut errors: Option<&mut Vec<String>>) -> Result<Vec<Seg>> {
    let root = site.base()?.join("omp-source-v1");
    if !lexists(&root)? {
        return Ok(Vec::new());
    }
    let sessions = listing(&root, MAX_ENTRIES, "Native session inventory exceeds bound")?;
    let mut result = Vec::new();
    for name in sessions {
        let outcome = (|| -> Result<()> {
            if !hex64_name(&name, "") {
                return Err(err("Unknown native session namespace"));
            }
            let session_path = root.join(&name);
            fs::trust::state(&session_path)?;
            session_children(site, &session_path, &mut result, &mut errors)
        })();
        match (outcome, errors.as_deref_mut()) {
            (Ok(()), _) => {}
            (Err(error), None) => return Err(error),
            (Err(_), Some(errors)) => errors.push("native-session-unverifiable".to_owned()),
        }
    }
    Ok(result)
}

/// A shared observation and its fault codes.
pub type Registration = (Option<Value>, Vec<Value>);

/// Baseline `_registration_lookup`: resolve one reference without observing
/// or mutating its namespace.
pub fn registration_lookup(state: &Value, enrollment: &Value) -> Result<Registration> {
    let reference = field(enrollment, "source_reference")?;
    let sources = field(state, "sources")?
        .as_array()
        .ok_or_else(|| err("Invalid shared source inventory"))?;
    let faults = field(state, "faults")?
        .as_array()
        .ok_or_else(|| err("Invalid shared source inventory"))?;
    let matches: Vec<&Value> = sources
        .iter()
        .filter(|source| get(source, "source_reference") == reference)
        .collect();
    if let Some(first) = matches.first() {
        if matches.len() != 1
            || field(first, "source_sha256")? != field(enrollment, "source_sha256")?
        {
            return Err(err(
                "Native source reference changed digest or is duplicated",
            ));
        }
        return Ok((Some((*first).clone()), faults.to_vec()));
    }
    if !faults.is_empty() && faults != [string("source-missing")] {
        return Ok((None, faults.to_vec()));
    }
    Ok((None, Vec::new()))
}

fn attempt_value(cursor: &Value, enrollment: &Value) -> Result<Value> {
    Ok(object([
        ("schema", string("agent-memory/omp-registration-attempt-v1")),
        ("session", field(cursor, "session")?.clone()),
        ("lifecycle", field(cursor, "lifecycle")?.clone()),
        (
            "source_reference",
            field(enrollment, "source_reference")?.clone(),
        ),
        ("source_sha256", field(enrollment, "source_sha256")?.clone()),
    ]))
}

/// Occupied pending index slots and whether this namespace already holds one.
fn pending_slots(site: &Site<'_>, ns: &Namespace) -> Result<(usize, bool)> {
    let pending = ns.base.join("pending");
    if !lexists(&pending)? {
        return Ok((0, false));
    }
    let message = "Unverifiable pending index capacity";
    let root = fs::canonical_path(site.repo, false)?;
    let expected = turn_sources::reference(&ns.directory, &ns.identity)?;
    let mut occupied = 0;
    let mut exists = false;
    for name in listing(&pending, turn_sources::MAX_TURNS, message)? {
        let number = name.strip_suffix(".json").ok_or_else(|| err(message))?;
        let index: usize = number.parse().map_err(|_| err(message))?;
        if number != index.to_string() || index >= turn_sources::MAX_TURNS {
            return Err(err(message));
        }
        let value = read(&pending.join(&name), MAX_RECORD)?;
        let value =
            turn_sources::validate_reference(&value, &root, get(&ns.identity, "project_id"))?;
        occupied += 1;
        exists |= same(value, &expected);
    }
    Ok((occupied, exists))
}

/// Baseline `registration(repo, directory, cursor, enrollment, perform=…)`.
pub fn registration(
    site: &Site<'_>,
    directory: &Path,
    cursor: &Value,
    enrollment: &Value,
    perform: bool,
) -> Result<Registration> {
    if perform && !source_bound_origin(directory, cursor)? {
        return Err(err("Cannot register an unbound native root family"));
    }
    let state = shared_inventory(site, cursor)?;
    let (observation, faults) = registration_lookup(&state, enrollment)?;
    if observation.is_some() || !faults.is_empty() {
        return Ok((observation, faults));
    }
    let reference = field(enrollment, "source_reference")?;
    let attempt_path = directory
        .join("registration-attempts")
        .join(format!("{}.json", digest(reference)?));
    let attempt = attempt_value(cursor, enrollment)?;
    if lexists(&attempt_path)? {
        if !same(&read(&attempt_path, MAX_RECORD)?, &attempt) {
            return Err(err("Native registration attempt identity changed"));
        }
        return Ok((None, vec![string("registration-fault-retained")]));
    }
    if !perform {
        return Ok((None, Vec::new()));
    }
    let ns = shared(site, cursor)?;
    let (occupied, exists) = pending_slots(site, &ns)?;
    if !exists && occupied >= turn_sources::MAX_TURNS {
        return Ok((None, Vec::new()));
    }
    omp_boundary::verify_marker(site.repo, site.runtime_root)?;
    // Recover successful lost responses above; never repeat an unknown attempt
    // whose shared failure could not itself be durably written (e.g. disk full).
    retain(&attempt_path, &attempt, MAX_RECORD)?;
    let registered = turn_sources::register(
        site.repo,
        "omp",
        field_text(cursor, "session")?,
        field_text(cursor, "lifecycle")?,
        field_text(enrollment, "source_sha256")?,
        Some(field_text(enrollment, "native_source_id")?),
        Some(text(reference)?),
        site.context,
        site.runtime_root,
    );
    let Ok(observation) = registered else {
        // register's durable fault is intentionally never erased or retried.
        return Ok((None, vec![string("registration-fault-retained")]));
    };
    let checked = shared_inventory(site, cursor)?;
    let identifier = field(&observation, "observation_id")?;
    let found: Vec<&Value> = field(&checked, "sources")?
        .as_array()
        .unwrap_or_default()
        .iter()
        .filter(|source| get(source, "observation_id") == identifier)
        .collect();
    let [found] = found.as_slice() else {
        return Err(err("Native registration readback unavailable"));
    };
    let faults = field(&checked, "faults")?
        .as_array()
        .map(<[Value]>::to_vec)
        .unwrap_or_default();
    Ok((Some((*found).clone()), faults))
}

/// Baseline `drain_enrollments`: finish at most eight eligible enrollments,
/// independently of capture time.
///
/// Inventory and each selected registration must finish their durable work:
/// elapsed-time cutoffs after scope/inspection can otherwise starve every
/// callback. The native adapter bounds its await, not this helper's writes.
pub fn drain_enrollments(site: &Site<'_>) -> Result<()> {
    if let Some(context) = site.context {
        crate::scope_binding::enrollment_read_context(site.repo, context)?;
    }
    let mut records = Vec::new();
    let mut faults = Vec::new();
    for segment in all_segments(site, Some(&mut faults))? {
        let cursor = segment.snapshot()?;
        if !source_bound_origin(&segment.directory, &cursor)? {
            continue;
        }
        let collected = (|| -> Result<Vec<(i128, String, Value)>> {
            if segment.sealed()? {
                let manifest = manifest_value(&segment.directory, &cursor)?;
                enroll_sealed(&segment.directory, &cursor, &manifest)?;
            }
            let mut found = Vec::new();
            for path in files(&segment.directory, "enrollments", Names::Hex64)? {
                let enrollment = read(&path, MAX_RECORD)?;
                let reference = field_text(&enrollment, "source_reference")?.to_owned();
                found.push((modified_ns(&path)?, reference, enrollment));
            }
            Ok(found)
        })();
        // Joined inventory retains the exact fault; no retry of register.
        if let Ok(found) = collected {
            for (modified, reference, enrollment) in found {
                records.push((
                    modified,
                    reference,
                    Rc::clone(&segment),
                    cursor.clone(),
                    enrollment,
                ));
            }
        }
    }
    records.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    let mut attempts = 0;
    for (_, _, segment, cursor, enrollment) in records {
        if attempts >= 8 {
            return Ok(());
        }
        let _ = (|| -> Result<()> {
            let (observation, faults) =
                registration(site, &segment.directory, &cursor, &enrollment, false)?;
            if observation.is_none() && faults.is_empty() {
                attempts += 1;
                registration(site, &segment.directory, &cursor, &enrollment, true)?;
            }
            Ok(())
        })();
    }
    Ok(())
}

/// Baseline `_coverage(directory, manifest, historical=…)`.
pub fn coverage(directory: &Path, manifest: &Value, historical: bool) -> Result<bool> {
    for gap_value in manifest_gaps(manifest)? {
        let gap_id = field_text(gap_value, "gap_id")?;
        let Some(resolution) = read_optional(
            &directory.join("resolutions").join(format!("{gap_id}.json")),
            MAX_RECORD,
        )?
        else {
            return Ok(false);
        };
        exact(
            &resolution,
            &[
                "schema",
                "manifest_sha256",
                "gap_id",
                "recovered_ref",
                "recovered_sha256",
                "reason",
            ],
        )?;
        let reason = get(&resolution, "reason").as_str();
        if get(&resolution, "schema").as_str() != Some("agent-memory/omp-gap-resolution-v1")
            || get(&resolution, "manifest_sha256").as_str() != Some(digest(manifest)?.as_str())
            || get(&resolution, "gap_id").as_str() != Some(gap_id)
            || !matches!(
                reason,
                Some(
                    "recovered-safe-native-source"
                        | "recovered-safe-payload"
                        | "verified-native-reassociation"
                )
            )
        {
            return Err(err("Native gap resolution differs from sealed evidence"));
        }
        let recovered = read(
            &snapshot_path(directory, get(&resolution, "recovered_ref"))?,
            MAX_PAYLOAD,
        )?;
        if !omp_projection::valid_instruction_context(&recovered) {
            return Err(err("Invalid recovered native instruction context"));
        }
        let original = field(gap_value, "native_entry_id")?;
        let reassociation = reason == Some("verified-native-reassociation");
        let mut reassociated = false;
        if let Some(event) = original.as_str().and_then(|o| o.strip_prefix("event:")) {
            if reassociation {
                if !historical {
                    return Ok(false); // Role/order metadata is not an exact native-ID witness.
                }
                let cursor = read(&directory.join("cursor.json"), MAX_PAYLOAD)?;
                reassociated = matched_metadata(&cursor)
                    .get(event)
                    .is_some_and(|native| native == get(&recovered, "native_entry_id"));
            }
        }
        if let Some(call) = original.as_str().and_then(|o| o.strip_prefix("tool:")) {
            if reassociation {
                reassociated = get(&recovered, "role").as_str() == Some("toolResult")
                    && get(&recovered, "toolCallId").as_str() == Some(call);
            }
        }
        if get(&resolution, "recovered_sha256").as_str() != Some(digest(&recovered)?.as_str())
            || (get(&recovered, "native_entry_id") != original && !reassociated)
        {
            return Err(err(
                "Recovered source differs from original native identity",
            ));
        }
    }
    Ok(true)
}

/// Baseline `manifest_value(directory, cursor)`.
pub fn manifest_value(directory: &Path, cursor: &Value) -> Result<Value> {
    let manifest = read(&directory.join("segment.json"), MAX_PAYLOAD)?;
    exact(
        &manifest,
        &[
            "schema",
            "session",
            "lifecycle",
            "origin",
            "seal_id",
            "admission_ref",
            "captures",
            "gaps",
            "exclusions",
            "previous_digest",
        ],
    )?;
    let bounded = |key: &str| {
        get(&manifest, key)
            .as_array()
            .is_some_and(|items| items.len() <= MAX_CAPTURES)
    };
    if get(&manifest, "schema").as_str() != Some("agent-memory/omp-segment-v1")
        || get(&manifest, "session") != field(cursor, "session")?
        || get(&manifest, "lifecycle") != field(cursor, "lifecycle")?
        || !same(get(&manifest, "origin"), field(cursor, "origin")?)
        || get(&manifest, "exclusions") != &exclusions()
        || !bounded("captures")
        || !bounded("gaps")
    {
        return Err(err("Foreign or oversized native sealed manifest"));
    }
    canonical_uuid(get(&manifest, "seal_id"))?;
    for item in get(&manifest, "captures").as_array().unwrap_or_default() {
        exact(item, &["ref", "sha256"])?;
        let snapshot = read(&snapshot_path(directory, get(item, "ref"))?, MAX_PAYLOAD)?;
        if get(item, "sha256").as_str() != Some(digest(&snapshot)?.as_str()) {
            return Err(err("Native sealed capture digest changed"));
        }
        if !omp_projection::valid_instruction_context(&snapshot) {
            return Err(err("Invalid sealed native instruction context"));
        }
    }
    for item in manifest_gaps(&manifest)? {
        exact(
            item,
            &[
                "schema",
                "session",
                "lifecycle",
                "gap_id",
                "native_entry_id",
                "reason",
            ],
        )?;
        if get(item, "schema").as_str() != Some("agent-memory/omp-gap-v1")
            || get(item, "session") != field(cursor, "session")?
            || get(item, "lifecycle") != field(cursor, "lifecycle")?
            || !get(item, "reason")
                .as_str()
                .is_some_and(|reason| GAPS.contains(&reason))
        {
            return Err(err("Foreign native coverage gap"));
        }
        canonical_uuid(get(item, "gap_id"))?;
    }
    Ok(manifest)
}
