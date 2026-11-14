//! Baseline `hooks/boundary_ledger.py`: private OpenCode hook bookkeeping;
//! queue attempts are not completion receipts.
//!
//! State lives under `<root>/experimental-boundary/<sha256(session)>/`, one
//! `<turn key>.*` family per native source: `.intent` (enrollment),
//! `.attempt` (legacy queue attempt), `.wake-lock`, `.wake-0`/`.wake-1`
//! (bounded wakes) and `.assessed` (completion). Every walk is descriptor
//! based from `/`, with no-follow components and identity rechecks.
use super::{chars, checkout, digest, get, object, sha256_hex, string, truthy};
use crate::client_hooks::membership_equal;
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::ContractError;
use crate::fs::{self, Descriptor, FlockGuard, LockMode};
use crate::{bank_writes, boundary_assessment, scope_binding, turn_check, turn_sources};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

const INTENT_V1: &str = "agent-memory/experimental-boundary-intent-v1";
const INTENT_V2: &str = "agent-memory/opencode-source-intent-v2";
const WAKE: &str = "agent-memory/opencode-wake-v1";
/// Marker read bound (`info.st_size > 4096`, `os.read(fd, 4097)`).
const MAX_MARKER: usize = 4096;
const MAX_ID_CHARS: usize = 512;
const PAGE: usize = 16;
const MAX_OFFSET: i64 = 4096;
const MAX_SESSIONS: usize = 4096;
const MAX_SESSION_ENTRIES: usize = 20000;
const MAX_BACKLOG_SOURCES: usize = 4096;
const LIMIT_VARIABLE: &str = "AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT";
const DEFAULT_LIMIT: i64 = 20000;
const MIN_LIMIT: i64 = 2;
const MAX_LIMIT: i64 = 1_000_000;
const ATTEMPT_MARKER: &[u8] = b"queue-attempt-uncertain\n";

/// The baseline exception classes the ledger distinguishes.
#[derive(Debug)]
pub enum Error {
    /// `FileNotFoundError`.
    Missing(io::Error),
    /// Any other `OSError`, including a busy non-blocking lock.
    Os(io::Error),
    /// A ledger `ValueError`.
    Value(String),
    /// A refusal of a ported module (`ContractError`).
    Contract(ContractError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(error) | Self::Os(error) => fmt::Display::fmt(error, f),
            Self::Value(message) => f.write_str(message),
            Self::Contract(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for Error {}

impl From<ContractError> for Error {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Self::Missing(error)
        } else {
            Self::Os(error)
        }
    }
}

impl From<fs::LockError> for Error {
    fn from(error: fs::LockError) -> Self {
        match error {
            fs::LockError::Busy => Self::Os(io::Error::from(io::ErrorKind::WouldBlock)),
            fs::LockError::Io(error) => Self::from(error),
            fs::LockError::Unsafe(error) => Self::Contract(error),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn value(message: &str) -> Error {
    Error::Value(message.to_owned())
}

/// Optional request members of `reserve`; `None` is an absent (or JSON
/// null) member, exactly as the baseline keyword defaults.
#[derive(Debug, Default, Clone, Copy)]
pub struct Options<'a> {
    pub repo: Option<&'a Value>,
    pub publication_request: Option<&'a Value>,
    pub assessment: Option<&'a Value>,
    pub fingerprint: Option<&'a Value>,
    /// `None` is the default offset 0; JSON null is an invalid offset.
    pub offset: Option<&'a Value>,
}

fn name(text: String) -> OsString {
    OsString::from(text)
}

fn private(info: fs::Identity) -> bool {
    info.uid == crate::host::effective_uid() && info.mode & 0o777 == 0o700
}

/// A marker that may be trusted as private single-link evidence.
fn safe_marker(info: fs::Identity) -> bool {
    info.is_regular()
        && info.uid == crate::host::effective_uid()
        && info.nlink == 1
        && info.mode & 0o077 == 0
        && info.size <= MAX_MARKER as i64
}

/// `json.loads(os.read(fd, 4097))`: one read of the marker bytes.
fn read_once(file: &mut Descriptor) -> Result<Value> {
    let mut buffer = [0u8; MAX_MARKER + 1];
    let count = file.read_chunk(&mut buffer)?;
    let bytes = buffer.get(..count).unwrap_or_default();
    json::decode_bytes(bytes, DecodePolicy::default()).map_err(|e| Error::Value(e.to_string()))
}

/// Baseline `read_marker`.
fn read_marker(directory: &Descriptor, marker: &str) -> Result<Value> {
    let mut item = directory.open_at(OsStr::new(marker), libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
    if !safe_marker(item.stat()?) {
        return Err(value("unsafe source evidence"));
    }
    read_once(&mut item)
}

/// `json.dumps([session, source], ensure_ascii=True, separators=(',', ':'))`.
fn turn_key(session: &str, source: &str) -> Result<String> {
    let encoded = json::encode(
        &Value::Array(vec![string(session), string(source)]),
        json::COMPACT_ASCII,
    )?;
    Ok(sha256_hex(encoded.as_bytes()))
}

/// Baseline `valid_intent`.
fn valid_intent(
    intent: &Value,
    key: &str,
    repository: &Value,
    session: &str,
    source: &str,
) -> bool {
    let legacy = ["schema", "turn_key", "repository"];
    let native = [
        "schema",
        "turn_key",
        "repository",
        "session_id",
        "source_id",
    ];
    intent.as_object().is_some()
        && get(intent, "turn_key").as_str() == Some(key)
        && intent
            .get("repository")
            .is_some_and(|value| membership_equal(value, repository))
        && ((super::exact_fields(intent, &legacy)
            && get(intent, "schema").as_str() == Some(INTENT_V1))
            || (super::exact_fields(intent, &native)
                && get(intent, "schema").as_str() == Some(INTENT_V2)
                && get(intent, "session_id").as_str() == Some(session)
                && get(intent, "source_id").as_str() == Some(source)))
}

/// `[0-9a-f]{40}|[0-9a-f]{64}` (full match).
fn commit(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64)
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Baseline `valid_completion`.
fn valid_completion(record: &Value, repository: &Value) -> bool {
    let status = get(record, "status").as_str();
    let witnessed = get(record, "witnessed_commit");
    super::exact_fields(
        record,
        &["status", "repository", "request_hash", "witnessed_commit"],
    ) && membership_equal(get(record, "repository"), repository)
        && matches!(status, Some("published" | "assessed-no-learning"))
        && get(record, "request_hash").as_str().is_some_and(digest)
        && ((status == Some("assessed-no-learning") && witnessed == &Value::Null)
            || (status == Some("published") && witnessed.as_str().is_some_and(commit)))
}

fn completed_status(record: &Value) -> Value {
    object([(
        "status",
        string(if get(record, "status").as_str() == Some("published") {
            "already-published"
        } else {
            "assessed-no-learning"
        }),
    )])
}

/// Baseline `_open_wake_lock`: create once or open the existing lock without
/// a create-or-open race. Darwin openat(O_CREAT) can lose its dirfd during a
/// racing first create (golang/go#81246); ENOENT is never retried or hidden.
fn open_wake_lock(directory: &Descriptor, key: &str) -> Result<Descriptor> {
    let lock = name(format!("{key}.wake-lock"));
    let flags = libc::O_RDWR | libc::O_NONBLOCK;
    match directory.open_at(&lock, flags | libc::O_CREAT | libc::O_EXCL, 0o600) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            crate::bank_writes::fault("boundary-ledger-wake-lock-reopen")?;
            Ok(directory.open_at(&lock, flags, 0)?)
        }
        Err(error) => Err(error.into()),
    }
}

/// The exclusive non-blocking lock on a verified private wake-lock file.
fn lock(directory: &Descriptor, key: &str, unsafe_message: &str) -> Result<FlockGuard> {
    let file = open_wake_lock(directory, key)?;
    let info = file.stat()?;
    if !info.is_regular()
        || info.uid != crate::host::effective_uid()
        || info.nlink != 1
        || info.mode & 0o077 != 0
    {
        return Err(value(unsafe_message));
    }
    Ok(FlockGuard::try_acquire(file, LockMode::Exclusive)?)
}

/// Create, write once and sync one new marker.
fn create_marker(directory: &Descriptor, marker: &str, payload: &[u8], short: &str) -> Result<()> {
    let mut item = directory.open_at(
        OsStr::new(marker),
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    if item.write_once(payload)? != payload.len() {
        return Err(Error::Os(io::Error::other(short.to_owned())));
    }
    item.fsync()?;
    drop(item);
    Ok(())
}

/// Baseline `wake`: at most two wakes per source, one per fingerprint.
fn wake(
    directory: &Descriptor,
    key: &str,
    repository: &Value,
    session: &str,
    source: &str,
    fingerprint: Option<&Value>,
) -> Result<Value> {
    let Some(fingerprint) = fingerprint.and_then(Value::as_str).filter(|v| digest(v)) else {
        return Err(value("invalid wake fingerprint"));
    };
    let _lock = lock(directory, key, "unsafe wake lock")?;
    match read_marker(directory, &format!("{key}.assessed")) {
        Ok(record) => {
            if !valid_completion(&record, repository) {
                return Err(value("invalid completion"));
            }
            return Ok(completed_status(&record));
        }
        Err(Error::Missing(_)) => {}
        Err(error) => return Err(error),
    }
    if !valid_intent(
        &read_marker(directory, &format!("{key}.intent"))?,
        key,
        repository,
        session,
        source,
    ) {
        return Err(value("source not enrolled"));
    }
    let mut attempt = None;
    for number in 0..2usize {
        let marker = format!("{key}.wake-{number}");
        let previous = match read_marker(directory, &marker) {
            Ok(previous) => previous,
            Err(Error::Missing(_)) => {
                attempt = Some((number, marker));
                break;
            }
            Err(error) => return Err(error),
        };
        if !super::exact_fields(&previous, &["schema", "fingerprint"])
            || get(&previous, "schema").as_str() != Some(WAKE)
            || !get(&previous, "fingerprint").as_str().is_some_and(digest)
        {
            return Err(value("unsafe wake evidence"));
        }
        if get(&previous, "fingerprint").as_str() == Some(fingerprint) {
            return Ok(object([
                ("status", string("pending")),
                ("reason", string("wake-already-attempted")),
            ]));
        }
    }
    let Some((number, marker)) = attempt else {
        return Ok(object([
            ("status", string("pending")),
            ("reason", string("wake-cap")),
        ]));
    };
    let payload = json::encode(
        &object([
            ("schema", string(WAKE)),
            ("fingerprint", string(fingerprint)),
        ]),
        json::DEFAULT,
    )?;
    create_marker(directory, &marker, payload.as_bytes(), "short wake write")?;
    directory.fsync_directory()?;
    Ok(object([
        ("status", string("reserved")),
        ("turn_key", string(key)),
        ("attempt", Value::Int((number + 1).into())),
    ]))
}

/// `[0-9a-f]{64}\.intent` (full match).
fn intent_name(entry: &str) -> bool {
    entry.strip_suffix(".intent").is_some_and(digest)
}

fn capacity() -> Value {
    object([
        ("status", string("pending")),
        ("reason", string("backlog-capacity")),
    ])
}

/// Baseline `backlog`: the bounded inventory of every enrolled native source
/// still pending for this repository, sixteen per page.
fn backlog(parent: &Descriptor, repository: &Value, offset: Option<&Value>) -> Result<Value> {
    let offset = match offset {
        None => 0,
        Some(Value::Int(number)) => number
            .as_i64()
            .filter(|n| (0..=MAX_OFFSET).contains(n))
            .ok_or_else(|| value("invalid backlog offset"))?,
        Some(_) => return Err(value("invalid backlog offset")),
    };
    let Some(directories) = parent.entries_bounded(MAX_SESSIONS)? else {
        return Ok(capacity());
    };
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut legacy = 0u64;
    for directory_name in &directories {
        let child = parent.open_directory_at(directory_name)?;
        if !private(child.stat()?) {
            return Err(value("unsafe backlog directory"));
        }
        let Some(entries) = child.entries_bounded(MAX_SESSION_ENTRIES)? else {
            return Ok(capacity());
        };
        for entry in &entries {
            let Some(entry) = entry.to_str().filter(|text| intent_name(text)) else {
                continue;
            };
            let intent = read_marker(&child, entry)?;
            if intent.as_object().is_none() {
                return Err(value("invalid pending source"));
            }
            if !membership_equal(get(&intent, "repository"), repository) {
                continue;
            }
            if get(&intent, "schema").as_str() == Some(INTENT_V1) {
                legacy += 1;
                continue;
            }
            let (Some(session), Some(source)) = (
                get(&intent, "session_id")
                    .as_str()
                    .filter(|v| !v.is_empty() && chars(v) <= MAX_ID_CHARS),
                get(&intent, "source_id")
                    .as_str()
                    .filter(|v| !v.is_empty() && chars(v) <= MAX_ID_CHARS),
            ) else {
                return Err(value("invalid native identifiers"));
            };
            let key = turn_key(session, source)?;
            if directory_name.to_str() != Some(sha256_hex(session.as_bytes()).as_str())
                || entry != format!("{key}.intent")
                || !valid_intent(&intent, &key, repository, session, source)
            {
                return Err(value("invalid native source binding"));
            }
            sources.push((session.to_owned(), source.to_owned()));
            if sources.len() > MAX_BACKLOG_SOURCES {
                return Ok(capacity());
            }
        }
    }
    sources.sort();
    let rows: Vec<Value> = sources
        .iter()
        .map(|(session, source)| {
            object([
                ("session_id", string(session.as_str())),
                ("source_id", string(source.as_str())),
            ])
        })
        .collect();
    let start = usize::try_from(offset).unwrap_or(usize::MAX);
    let next = start + PAGE;
    let next_offset = if next < rows.len() {
        Value::Int(next.into())
    } else {
        Value::Null
    };
    let inventory = json::encode(
        &object([
            ("sources", Value::Array(rows.clone())),
            ("legacy", Value::Int(legacy.into())),
        ]),
        json::SORTED_COMPACT_ASCII,
    )?;
    let page: Vec<Value> = rows.iter().skip(start).take(PAGE).cloned().collect();
    Ok(object([
        ("status", string("backlog")),
        ("sources", Value::Array(page)),
        ("pending_count", Value::Int(rows.len().into())),
        ("more_pending", Value::Bool(next_offset != Value::Null)),
        ("offset", Value::Int(offset.into())),
        ("next_offset", next_offset),
        ("inventory_sha256", string(sha256_hex(inventory.as_bytes()))),
        ("legacy_pending_count", Value::Int(legacy.into())),
        ("publication_verified", Value::Bool(false)),
    ]))
}

/// `os.path.normpath(root).split(os.sep)[1:]` for an absolute root.
fn normalized_parts(root: &str) -> Vec<String> {
    let initial = if root.starts_with("//") && !root.starts_with("///") {
        2
    } else {
        1
    };
    let mut parts: Vec<&str> = Vec::new();
    for component in root.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let mut result = vec![String::new(); initial - 1];
    if parts.is_empty() {
        result.push(String::new());
    }
    result.extend(parts.into_iter().map(str::to_owned));
    result
}

fn display(parts: &[String]) -> PathBuf {
    PathBuf::from(format!("/{}", parts.join("/")))
}

/// The walked private state namespace: the session directory, its parent
/// and every component's identity.
struct Namespace {
    parts: Vec<String>,
    identities: Vec<(u64, u64)>,
    parent: Descriptor,
    directory: Descriptor,
}

/// Walk from `/` to `<root>/experimental-boundary/<session hash>`, creating
/// the last two components privately and applying the shared trust policy.
fn open_namespace(root: &str, session: &str) -> Result<Namespace> {
    let mut parts = normalized_parts(root);
    parts.push(String::from("experimental-boundary"));
    parts.push(sha256_hex(session.as_bytes()));
    let count = parts.len();
    let uid = crate::host::effective_uid();
    let mut directory = Descriptor::open_directory(Path::new("/"))?;
    let (mut protected, _) =
        fs::trust::inspect_entry(Path::new("/"), &directory.stat()?, false, false, false)?;
    let mut identities = Vec::with_capacity(count);
    let mut parent = None;
    for (index, part) in parts.iter().enumerate() {
        let component = OsStr::new(part);
        if index + 2 >= count {
            match directory.make_directory_at(component, 0o700) {
                Ok(()) => directory.fsync_directory()?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        let next = directory.open_directory_at(component)?;
        parent = Some(std::mem::replace(&mut directory, next));
        let info = directory.stat()?;
        identities.push((info.dev, info.ino));
        let path = display(parts.get(..=index).unwrap_or_default());
        (protected, _) = fs::trust::inspect_entry(&path, &info, protected, false, false)?;
        if index + 3 == count && info.uid != uid {
            return Err(value("state root must be owned by the executing user"));
        }
        if index + 2 >= count && !private(info) {
            return Err(value("boundary state directory is not private and owned"));
        }
    }
    let parent = parent.ok_or_else(|| value("absolute state root and turn IDs required"))?;
    Ok(Namespace {
        parts,
        identities,
        parent,
        directory,
    })
}

/// Baseline `verify_namespace`: the walked path still names the same
/// private directories.
fn verify_namespace(namespace: &Namespace, message: &str, trust: bool) -> Result<()> {
    let count = namespace.parts.len();
    let mut check = Descriptor::open_directory(Path::new("/"))?;
    let (mut protected, _) =
        fs::trust::inspect_entry(Path::new("/"), &check.stat()?, false, false, false)?;
    for (index, (part, identity)) in namespace
        .parts
        .iter()
        .zip(&namespace.identities)
        .enumerate()
    {
        check = check.open_directory_at(OsStr::new(part))?;
        let info = check.stat()?;
        if (info.dev, info.ino) != *identity {
            return Err(value(message));
        }
        if trust {
            let path = display(namespace.parts.get(..=index).unwrap_or_default());
            (protected, _) = fs::trust::inspect_entry(&path, &info, protected, false, false)?;
            if index + 2 >= count && !private(info) {
                return Err(value("boundary state directory is not private and owned"));
            }
        }
    }
    Ok(())
}

/// The checked session repository, or `None` when the ledger does not apply.
fn session_repository(location: &Value, root: &str, action: &str) -> Result<Option<PathBuf>> {
    let location = location
        .as_str()
        .ok_or_else(|| value("session location unavailable"))?;
    let cwd = fs::canonical_path(Path::new(location), true)?;
    if !crate::fs::trust::named_stat(&cwd)?.is_directory() {
        return Err(value("session location unavailable"));
    }
    let tolerated = matches!(action, "reserve" | "enroll" | "backlog");
    let checkout = match checkout(&cwd) {
        Ok(Some(checkout)) => checkout,
        Ok(None) if tolerated => return Ok(None),
        Ok(None) => return Err(value("session repository unavailable for acknowledgment")),
        Err(_) => return Err(value("repository cannot be inspected")),
    };
    if !super::bank_exists(&checkout)? {
        if tolerated {
            return Ok(None);
        }
        return Err(value("session bank unavailable for acknowledgment"));
    }
    let (found, common) = super::git(&[
        "-C".as_ref(),
        checkout.as_os_str(),
        "rev-parse".as_ref(),
        "--git-common-dir".as_ref(),
    ])?;
    if !found {
        return Err(value("repository namespace unavailable"));
    }
    let common = fs::canonical_path(&checkout.join(common), true)?;
    let private = fs::canonical_path(Path::new(root), false)?;
    if private.starts_with(&checkout)
        || private.starts_with(&common)
        || (common.file_name() == Some(OsStr::new(".git"))
            && common
                .parent()
                .is_some_and(|parent| private.starts_with(parent)))
    {
        return Err(value(
            "boundary ledger state must stay outside repository and common Git data",
        ));
    }
    Ok(Some(checkout))
}

fn not_applicable() -> Value {
    object([("status", string("not-applicable"))])
}

/// The bound repository's identity, recorded in every intent and completion.
struct Bound {
    repository: PathBuf,
    context: Value,
    value: Value,
}

fn bind(repo: &Path) -> Result<Bound> {
    let (repository, _) = bank_writes::bank_path(repo)?;
    let context =
        scope_binding::require(&repository, None, scope_binding::RequireOptions::default())?;
    let text = repository
        .to_str()
        .ok_or_else(|| ContractError::non_utf8("repository path"))?;
    let value = object([
        ("repo", string(text)),
        ("project_id", get(&context, "project_id").clone()),
    ]);
    Ok(Bound {
        repository,
        context,
        value,
    })
}

fn recheck_scope(bound: &Bound) -> Result<()> {
    let project = get(&bound.context, "project_id").as_str();
    scope_binding::require(
        &bound.repository,
        project,
        scope_binding::RequireOptions {
            expected: Some(&bound.context),
            ..scope_binding::RequireOptions::default()
        },
    )?;
    Ok(())
}

/// Python `int(text)` for the ledger limit: optional surrounding whitespace,
/// sign and ASCII digits with single underscores between digits.
fn integer(text: &str) -> Option<i64> {
    let text = super::strip(text);
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned.parse().ok()
}

/// Exactly-once checked turn markers (`.intent` required, `.attempt`
/// optional) for an inspected or completing source.
fn check_markers(
    directory: &Descriptor,
    key: &str,
    repository: &Value,
    session: &str,
    source: &str,
    unsafe_message: &str,
    changed_message: &str,
) -> Result<()> {
    for suffix in [".intent", ".attempt"] {
        let mut existing = match directory.open_at(
            &name(format!("{key}{suffix}")),
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        ) {
            Ok(existing) => existing,
            Err(error) if error.kind() == io::ErrorKind::NotFound && suffix == ".attempt" => {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if !safe_marker(existing.stat()?) {
            return Err(value(unsafe_message));
        }
        if suffix == ".intent"
            && !valid_intent(&read_once(&mut existing)?, key, repository, session, source)
        {
            return Err(value(changed_message));
        }
    }
    Ok(())
}

/// Baseline `reserve`: persist native enrollment and bounded wakes; preserve
/// legacy reservations. `root` is the private state root (absolute).
pub fn reserve(
    root: &str,
    session: &str,
    source: &str,
    action: &str,
    options: Options<'_>,
) -> Result<Value> {
    if !root.starts_with('/')
        || session.is_empty()
        || source.is_empty()
        || chars(session) > MAX_ID_CHARS
        || chars(source) > MAX_ID_CHARS
    {
        return Err(value("absolute state root and turn IDs required"));
    }
    let mut repo = None;
    if let Some(location) = options.repo {
        match session_repository(location, root, action)? {
            Some(checkout) => repo = Some(checkout),
            None => return Ok(not_applicable()),
        }
    }
    let namespace = open_namespace(root, session)?;
    let fd = &namespace.directory;
    let bound = repo.as_deref().map(bind).transpose()?;
    let repository = bound
        .as_ref()
        .map_or(Value::Null, |bound| bound.value.clone());
    let key = turn_key(session, source)?;
    let done = format!("{key}.assessed");
    if action == "backlog" {
        return backlog(&namespace.parent, &repository, options.offset);
    }
    let verify_enrollment = || -> Result<()> {
        crate::bank_writes::fault("boundary-ledger-enroll-verify")?;
        if !valid_intent(
            &read_marker(fd, &format!("{key}.intent"))?,
            &key,
            &repository,
            session,
            source,
        ) {
            return Err(value("native enrollment readback differs"));
        }
        verify_namespace(
            &namespace,
            "state namespace changed; source remains pending",
            true,
        )?;
        if let Some(bound) = &bound {
            recheck_scope(bound)?;
        }
        Ok(())
    };
    let _completion_lock = if matches!(action, "inspect" | "assessed-no-learning" | "published") {
        completion_lock(fd, &key)?
    } else {
        None
    };
    let mut request_hash = Value::Null;
    let mut witnessed = Value::Null;
    if action == "assessed-no-learning" {
        let assessment = options.assessment.unwrap_or(&Value::Null);
        boundary_assessment::validate(assessment, source)?;
        request_hash = string(sha256_hex(
            json::encode(assessment, json::SORTED_COMPACT_UTF8)?.as_bytes(),
        ));
    }
    if action == "published" {
        let gate = turn_sources::publication_gate_provider()?;
        let (Some(bound), Some(request)) = (
            bound.as_ref(),
            options
                .publication_request
                .filter(|v| v.as_object().is_some()),
        ) else {
            return Err(value("publication requires bound repository and request"));
        };
        request_hash = string(sha256_hex(
            json::encode(request, json::SORTED_COMPACT_UTF8)?.as_bytes(),
        ));
        gate.request_value(&bound.repository, request)?;
        let proof = request
            .get("proof")
            .filter(|proof| proof.as_object().is_some())
            .ok_or_else(|| value("publication proof unavailable"))?;
        if truthy(get(proof, "blocked_paths")) || truthy(get(proof, "blocked_components")) {
            return Err(value("conflicted publication remains pending"));
        }
        let (Some(base), Some(candidate)) = (request.get("base"), request.get("candidate")) else {
            return Err(value("publication request is incomplete"));
        };
        let replay = gate.replay(
            &bound.repository,
            &object([("base", base.clone()), ("candidate", candidate.clone())]),
            get(proof, "schema").as_str() == Some("agent-memory/semantic-sharing-proof-v2"),
        )?;
        if !truthy(get(&replay, "intended_delta_landed")) {
            return Err(value("publication not witnessed"));
        }
        recheck_scope(bound)?;
        witnessed = replay
            .get("witnessed_commit")
            .cloned()
            .ok_or_else(|| value("publication not witnessed"))?;
    }
    match fd.open_at(&name(done.clone()), libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(mut completed) => {
            if !safe_marker(completed.stat()?) {
                return Err(value("invalid assessment marker"));
            }
            let record = read_once(&mut completed)?;
            drop(completed);
            if !valid_completion(&record, &repository) {
                return Err(value("invalid or different completion marker"));
            }
            if matches!(action, "reserve" | "enroll" | "wake") {
                return Ok(completed_status(&record));
            }
            if action == "inspect" {
                return Ok(object([("status", string("completed-source"))]));
            }
            if get(&record, "status").as_str() != Some(action)
                || (matches!(action, "published" | "assessed-no-learning")
                    && get(&record, "request_hash") != &request_hash)
            {
                return Err(value("completion differs from original assessment"));
            }
            return Ok(object([
                ("status", string(action)),
                ("already_completed", Value::Bool(true)),
                ("publication_verified", Value::Bool(action == "published")),
                ("witnessed_commit", witnessed),
                ("source_publication_link_verified", Value::Bool(false)),
            ]));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if action == "inspect" {
        check_markers(
            fd,
            &key,
            &repository,
            session,
            source,
            "invalid pending turn marker",
            "pending turn binding changed",
        )?;
        return Ok(object([("status", string("pending-turn"))]));
    }
    if matches!(action, "assessed-no-learning" | "published") {
        check_markers(
            fd,
            &key,
            &repository,
            session,
            source,
            "invalid turn marker",
            "session repository or project changed",
        )?;
        let payload = json::encode(
            &object([
                ("status", string(action)),
                ("repository", repository.clone()),
                ("request_hash", request_hash),
                ("witnessed_commit", witnessed.clone()),
            ]),
            json::SORTED_ASCII,
        )?;
        create_marker(fd, &done, payload.as_bytes(), "short assessment write")?;
        fd.fsync_directory()?;
        for suffix in [".intent", ".attempt"] {
            match fd.unlink_at(&name(format!("{key}{suffix}"))) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound && suffix == ".attempt" => {}
                Err(error) => return Err(error.into()),
            }
        }
        fd.fsync_directory()?;
        return Ok(object([
            ("status", string(action)),
            ("publication_verified", Value::Bool(action == "published")),
            ("witnessed_commit", witnessed),
            ("source_publication_link_verified", Value::Bool(false)),
        ]));
    }
    if action == "wake" {
        return wake(fd, &key, &repository, session, source, options.fingerprint);
    }
    if !matches!(action, "reserve" | "enroll") {
        return Err(value("unsupported acknowledgment"));
    }
    // An operator can raise this bound without deleting unresolved intents.
    let limit = match std::env::var(LIMIT_VARIABLE) {
        Ok(text) => integer(&text),
        Err(std::env::VarError::NotPresent) => Some(DEFAULT_LIMIT),
        Err(std::env::VarError::NotUnicode(_)) => None,
    };
    let Some(limit) = limit.filter(|limit| (MIN_LIMIT..=MAX_LIMIT).contains(limit)) else {
        return Ok(object([
            ("status", string("pending")),
            ("reason", string("ledger-limit-invalid")),
        ]));
    };
    let below = usize::try_from(limit - 1).unwrap_or(0);
    if fd.entries_bounded(below)?.is_none() {
        return Ok(object([
            ("status", string("pending")),
            ("reason", string("ledger-capacity")),
        ]));
    }
    let intent = format!("{key}.intent");
    let mut item = match fd.open_at(
        &name(intent.clone()),
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    ) {
        Ok(item) => item,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if action == "enroll" {
                verify_enrollment()?;
                return Ok(object([
                    ("status", string("enrolled")),
                    ("already_enrolled", Value::Bool(true)),
                ]));
            }
            return Ok(object([
                ("status", string("pending")),
                ("reason", string("existing-intent")),
            ]));
        }
        Err(error) => return Err(error.into()),
    };
    let mut record = vec![
        ("schema".to_owned(), string(INTENT_V1)),
        ("turn_key".to_owned(), string(key.as_str())),
        ("repository".to_owned(), repository.clone()),
    ];
    if action == "enroll" {
        if let Some((_, schema)) = record.first_mut() {
            *schema = string(INTENT_V2);
        }
        record.push(("session_id".to_owned(), string(session)));
        record.push(("source_id".to_owned(), string(source)));
    }
    let payload = json::encode(&Value::Object(record), json::DEFAULT)?;
    if payload.len() > MAX_MARKER {
        return Err(value("native source evidence exceeds bound"));
    }
    if item.write_once(payload.as_bytes())? != payload.len() {
        return Err(Error::Os(io::Error::other("short intent write")));
    }
    item.fsync()?;
    drop(item);
    fd.fsync_directory()?;
    if action == "enroll" {
        verify_enrollment()?;
        return Ok(object([
            ("status", string("enrolled")),
            ("turn_key", string(key.as_str())),
        ]));
    }
    create_marker(
        fd,
        &format!("{key}.attempt"),
        ATTEMPT_MARKER,
        "short attempt write",
    )?;
    fd.fsync_directory()?;
    // Detect namespace rebinding observed during the reservation. This is not
    // atomic against an uncooperative same-UID process after the final check.
    crate::bank_writes::fault("boundary-ledger-reserve-recheck")?;
    verify_namespace(
        &namespace,
        "state namespace changed; queue attempt pending",
        false,
    )?;
    Ok(object([
        ("status", string("reserved")),
        ("turn_key", string(key.as_str())),
    ]))
}

fn pending() -> Value {
    object([
        ("status", string("pending")),
        ("reason", string("ledger-unavailable")),
    ])
}

/// `request.get(key)` without JSON null (`is not None`).
fn present<'a>(request: &'a Value, key: &str) -> Option<&'a Value> {
    request.get(key).filter(|value| **value != Value::Null)
}

/// The request's `reserve` call, as the baseline `complete` closure: the
/// result plus whether its status is the requested action.
fn complete(request: &Value) -> Result<Value> {
    let root = get(request, "root")
        .as_str()
        .ok_or_else(|| value("absolute state root and turn IDs required"))?;
    let (Some(session), Some(source)) = (
        request
            .get("session")
            .ok_or_else(|| value("missing session"))?
            .as_str(),
        request
            .get("source")
            .ok_or_else(|| value("missing source"))?
            .as_str(),
    ) else {
        return Err(value("absolute state root and turn IDs required"));
    };
    // `request.get('action', 'reserve')`: any non-text action is unsupported.
    let action = match request.get("action") {
        None => "reserve",
        Some(action) => action.as_str().unwrap_or_default(),
    };
    let result = reserve(
        root,
        session,
        source,
        action,
        Options {
            repo: present(request, "repo"),
            publication_request: present(request, "publication_request"),
            assessment: present(request, "assessment"),
            fingerprint: present(request, "fingerprint"),
            offset: request.get("offset"),
        },
    )?;
    let verified = get(&result, "status") == request.get("action").unwrap_or(&Value::Null);
    let mut pairs = match result {
        Value::Object(pairs) => pairs,
        _ => return Err(value("invalid ledger result")),
    };
    pairs.retain(|(key, _)| key != "verified");
    pairs.push(("verified".to_owned(), Value::Bool(verified)));
    Ok(Value::Object(pairs))
}

/// The combined native check: refresh freshness plus this source's exact
/// no-learning completion, without echoing the source review.
fn combined(request: &Value) -> Result<Value> {
    if get(request, "action").as_str() != Some("assessed-no-learning")
        || !truthy(get(request, "repo"))
    {
        return Err(value(
            "combined native check requires an actual no-learning assessment and repository",
        ));
    }
    let repo = get(request, "repo")
        .as_str()
        .ok_or_else(|| value("combined native check requires a repository path"))?;
    let check = object([
        (
            "session",
            request
                .get("session")
                .cloned()
                .ok_or_else(|| value("missing session"))?,
        ),
        (
            "prompt",
            request
                .get("source")
                .cloned()
                .ok_or_else(|| value("missing source"))?,
        ),
        ("outcome", string("no-learning")),
        ("assessment", get(request, "assessment").clone()),
    ]);
    let mut result = turn_check::check(
        Path::new(repo),
        &check,
        |_, _| {
            complete(request).map_err(|error| match error {
                Error::Contract(error) => turn_check::CompletionError::Contract(error),
                _ => turn_check::CompletionError::Unexpected,
            })
        },
        "opencode",
    )?;
    if get(&result, "verified") != &Value::Bool(true)
        && get(&result, "notice_required") != &Value::Bool(false)
    {
        let notice = turn_check::failure_notice(&result)?;
        if let Value::Object(pairs) = &mut result {
            pairs.push(("failure_notice".to_owned(), string(notice)));
        }
    }
    // Native routine responses retain stage truth without echoing source reviews.
    let assessment = match get(&result, "assessment") {
        Value::Object(pairs) => Value::Object(
            pairs
                .iter()
                .filter(|(key, _)| {
                    matches!(key.as_str(), "verified" | "status" | "reason" | "repair")
                })
                .cloned()
                .collect(),
        ),
        _ => return Err(value("invalid combined assessment")),
    };
    if let Some(slot) = result.get_mut("assessment") {
        *slot = assessment;
    }
    Ok(result)
}

fn respond(raw: &[u8]) -> Result<Value> {
    if raw.len() > super::MAX_LEDGER_REQUEST {
        return Err(value("ledger request exceeds its bound"));
    }
    let request = json::decode_bytes(raw, DecodePolicy::default())?;
    if request.as_object().is_none() {
        return Err(value("ledger request must be an object"));
    }
    if get(&request, "refresh_current") == &Value::Bool(true) {
        return combined(&request);
    }
    let mut result = complete(&request)?;
    if let Value::Object(pairs) = &mut result {
        pairs.retain(|(key, _)| key != "verified");
    }
    Ok(result)
}

/// The `am hook boundary-ledger` request/response contract: one JSON request
/// on stdin, one JSON line on stdout; any failure is pending with status 1.
pub(super) fn main(raw: &[u8]) -> super::Output {
    match respond(raw) {
        Ok(result) => super::Output::stdout(0, super::line(&result)),
        Err(_) => super::Output::stdout(1, super::line(&pending())),
    }
}

/// A native source's completion runs under its wake lock: present when the
/// intent is native (v2) or a wake lock already exists.
fn completion_lock(fd: &Descriptor, key: &str) -> Result<Option<FlockGuard>> {
    let mut native = match read_marker(fd, &format!("{key}.intent")) {
        Ok(intent) => {
            if intent.as_object().is_none() {
                return Err(value("invalid native intent"));
            }
            get(&intent, "schema").as_str() == Some(INTENT_V2)
        }
        Err(Error::Missing(_)) => false,
        Err(error) => return Err(error),
    };
    match fd.stat_at(&name(format!("{key}.wake-lock"))) {
        Ok(_) => native = true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if !native {
        return Ok(None);
    }
    Ok(Some(lock(fd, key, "unsafe completion lock")?))
}
