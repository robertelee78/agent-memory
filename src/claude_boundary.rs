//! Acknowledge one source-bound Claude boundary without inventing publication.
//!
//! Baseline `claude_boundary.py`. Observation-bound assessments are completed
//! by `claude_turns`; the legacy `source_uuid` acknowledgment of a retained
//! `claude-boundaries/<key>.pending` record is implemented here and shared
//! with `codex_boundary` (`acknowledge` with per-harness `Legacy` constants).
//!
//! Errors keep the baseline split: `CompletionError::Contract` carries a
//! reviewed refusal reason, `CompletionError::Unexpected` stands for the
//! baseline's `OSError`/`ValueError` family (unavailable or undecodable
//! private evidence), whose text is never exposed.
use crate::codec::json::{self, Value};
use crate::developer_identity::{self, sha256_hex};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{Descriptor, Identity};
use crate::lifecycle::{CommandResult, get, nonempty, object, string, text};
use crate::scope_binding::{self, RequireOptions};
use crate::turn_check::{self, CompletionError, CompletionResult};
use crate::{
    bank_capacity, bank_writes, boundary_assessment, claude_turns, host, memory_publication,
    sharing_limits, sharing_recovery, trust_paths,
};
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Byte bound of one pending or completed legacy boundary record.
pub const MAX_RECORD: usize = 4096;
/// Byte bound of an assessment read from stdin.
pub const MAX_STDIN: usize = 65536;
const LEGACY_FIELDS: [&str; 6] = [
    "session",
    "prompt",
    "source_uuid",
    "source_sha256",
    "outcome",
    "assessment",
];
const GENERIC_REASON: &str = "Original source or pending record is unavailable or unsafe; preserve the retained observation.";
const REPAIR: &str = "Inspect am claude-boundary --list-pending. Current hook observations require top-level session,prompt,observation_id,source_sha256,outcome,assessment; assessment.source_id is observation_id. Legacy source_uuid applies only to legacy boundaries. Preserve original sources; read turn/GUIDE.md before retrying.";

/// Per-harness constants of the legacy source-bound acknowledgment.
pub(crate) struct Legacy {
    /// Private-state directory beside the bank's operations directory.
    pub directory: &'static str,
    pub pending_schema: &'static str,
    pub completed_schema: &'static str,
    /// Refusal when the encoded completion exceeds `MAX_RECORD`.
    pub bound_refusal: &'static str,
    /// The pending record also binds `session` and `turn` (Codex).
    pub binds_turn: bool,
}

const CLAUDE: Legacy = Legacy {
    directory: "claude-boundaries",
    pending_schema: "agent-memory/claude-boundary-pending-v1",
    completed_schema: "agent-memory/claude-boundary-completed-v1",
    bound_refusal: "Legacy completion exceeds its reader bound; preserve pending evidence",
    binds_turn: false,
};

fn refuse(message: &str) -> CompletionError {
    CompletionError::Contract(ContractError::new(message))
}
fn unavailable<E>(_: E) -> CompletionError {
    CompletionError::Unexpected
}
/// `std::io::Error`'s rendering of an OS error: `… (os error N)`.
fn os_error_text(message: &str) -> bool {
    message.split("(os error ").skip(1).any(|rest| {
        let after = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        after.len() < rest.len() && after.starts_with(')')
    })
}
/// The baseline handler's classification: only a contract refusal keeps its
/// reason. Ported callees report an `OSError` as a plain contract error that
/// carries the `std::io::Error` text; that is the baseline's `OSError` and
/// gets the fixed reason, never its system detail.
fn classified(error: CompletionError) -> CompletionError {
    match error {
        CompletionError::Contract(contract)
            if contract.kind() == &Kind::Contract && os_error_text(contract.message()) =>
        {
            CompletionError::Unexpected
        }
        other => other,
    }
}

/// `[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\Z`.
pub(crate) fn uuid_text(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
fn uuid(value: &Value) -> bool {
    value.as_str().is_some_and(uuid_text)
}
/// `[0-9a-f]{64}` (full match).
pub(crate) fn digest_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(digest_text)
}
/// `set(value) == expected` or `expected | {'publication_request'}`.
pub(crate) fn exact_fields(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        let optional = usize::from(value.get("publication_request").is_some());
        pairs.len() == expected.len() + optional
            && expected.iter().all(|name| value.get(name).is_some())
    })
}

/// Baseline `os.path.lexists`-style `lstat`: the final component is not followed.
pub(crate) fn lstat(path: &Path) -> io::Result<Identity> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
    Descriptor::pin_directory(parent)?.stat_at(name)
}

/// Owner-only, single-link ordinary file of at most `MAX_RECORD` bytes.
pub(crate) fn safe_record(info: &Identity) -> bool {
    info.is_regular()
        && info.uid == host::effective_uid()
        && info.nlink == 1
        && info.mode & 0o077 == 0
        && info.size <= MAX_RECORD as i64
}
/// `(st_dev, st_ino, st_size, st_mtime_ns)`.
fn same_observation(left: &Identity, right: &Identity) -> bool {
    left.same_inode(*right) && left.size == right.size && left.mtime_ns == right.mtime_ns
}

/// Open `name` below `directory` read-only and non-blocking without
/// following a link; `Ok(None)` only when it is absent.
fn open_record(directory: &Descriptor, name: &str) -> CompletionResult<Option<Descriptor>> {
    match directory.open_at(OsStr::new(name), libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(descriptor) => Ok(Some(descriptor)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(unavailable(error)),
    }
}
/// Check an opened record and read it within its bound (`read(4097)`).
fn read_record(
    descriptor: &mut Descriptor,
    unsafe_refusal: &str,
) -> CompletionResult<(Identity, Vec<u8>)> {
    let info = descriptor.stat().map_err(unavailable)?;
    if !safe_record(&info) {
        return Err(refuse(unsafe_refusal));
    }
    let raw = descriptor
        .read_prefix(MAX_RECORD + 1)
        .map_err(unavailable)?;
    Ok((info, raw))
}
fn load_record(raw: &[u8]) -> CompletionResult<Value> {
    developer_identity::load(raw).map_err(unavailable)
}

/// The exact legacy request shape and its bounded assessment.
fn legacy_request(value: &Value) -> CompletionResult<()> {
    let outcome = text(value, "outcome");
    if !exact_fields(value, &LEGACY_FIELDS)
        || !["session", "prompt", "source_uuid"]
            .iter()
            .all(|key| uuid(get(value, key)))
        || !digest(get(value, "source_sha256"))
        || !matches!(outcome, Some("no-learning" | "published"))
    {
        return Err(refuse("Use an explicit bounded source-bound assessment"));
    }
    let assessment = get(value, "assessment");
    if outcome == Some("no-learning") {
        boundary_assessment::validate(assessment, text(value, "source_uuid").unwrap_or_default())?;
    } else if !assessment
        .as_str()
        .is_some_and(|text| nonempty(text) && text.len() <= 2048)
    {
        return Err(refuse("Published outcome requires a bounded assessment"));
    }
    if (outcome == Some("published")) != value.get("publication_request").is_some() {
        return Err(refuse(
            "Published completion requires the original publication request",
        ));
    }
    Ok(())
}

/// The independently witnessed authority commit of a published outcome, or
/// `null` for no-learning.
fn witnessed(root: &Path, value: &Value) -> CompletionResult<Value> {
    if text(value, "outcome") != Some("published") {
        return Ok(Value::Null);
    }
    let request = get(value, "publication_request");
    memory_publication::request_value(root, request)?;
    let proof = get(request, "proof");
    if crate::omp_projection::truthy(get(proof, "blocked_paths"))
        || crate::omp_projection::truthy(get(proof, "blocked_components"))
    {
        return Err(refuse("Conflicted publication remains pending"));
    }
    let commits = object([
        ("base", get(request, "base").clone()),
        ("candidate", get(request, "candidate").clone()),
    ]);
    let quarantine_records =
        text(proof, "schema") == Some("agent-memory/semantic-sharing-proof-v2");
    let replay = sharing_recovery::replay(root, &commits, quarantine_records)?;
    if !crate::omp_projection::truthy(get(&replay, "intended_delta_landed")) {
        return Err(refuse(
            "Publication is not independently witnessed on authority",
        ));
    }
    replay
        .get("witnessed_commit")
        .cloned()
        .ok_or(CompletionError::Unexpected)
}

/// The retained legacy boundary namespace of one bank and harness.
struct Namespace {
    root: PathBuf,
    context: Value,
    directory: PathBuf,
}
fn namespace(repo: &Path, legacy: &Legacy) -> CompletionResult<Namespace> {
    let (root, bank) = bank_writes::bank_path(repo)?;
    let context = scope_binding::require(&root, None, RequireOptions::default())?;
    let (operations, _) = bank_writes::state_paths(&root, &bank, &host::uuid4()?)?;
    let directory = operations
        .parent()
        .ok_or(CompletionError::Unexpected)?
        .join(legacy.directory);
    trust_paths::state(&directory)?;
    Ok(Namespace {
        root,
        context,
        directory,
    })
}
fn recheck_scope(ns: &Namespace) -> CompletionResult<()> {
    let project = text(&ns.context, "project_id").ok_or(CompletionError::Unexpected)?;
    scope_binding::require(
        &ns.root,
        Some(project),
        RequireOptions {
            expected: Some(&ns.context),
            ..RequireOptions::default()
        },
    )?;
    Ok(())
}

/// The retried acknowledgment of an already completed boundary: its pending
/// record is gone and the completed record must match this exact request.
fn already_completed(
    ns: &Namespace,
    directory: &Descriptor,
    done_name: &str,
    value: &Value,
    request_hash: &str,
    legacy: &Legacy,
) -> CompletionResult<Value> {
    let mut done = open_record(directory, done_name)?.ok_or(CompletionError::Unexpected)?;
    let (_, raw) = read_record(&mut done, "Completed boundary evidence is unsafe")?;
    drop(done);
    let previous = load_record(&raw)?;
    if previous.as_object().is_none()
        || text(&previous, "schema") != Some(legacy.completed_schema)
        || !crate::installation::same(get(&previous, "project_id"), get(&ns.context, "project_id"))
        || !crate::installation::same(get(&previous, "source_uuid"), get(value, "source_uuid"))
        || !crate::installation::same(get(&previous, "source_sha256"), get(value, "source_sha256"))
        || text(&previous, "input_sha256") != Some(request_hash)
        || !crate::installation::same(get(&previous, "outcome"), get(value, "outcome"))
    {
        return Err(refuse("Completed boundary differs from requested retry"));
    }
    let witnessed = witnessed(&ns.root, value)?;
    recheck_scope(ns)?;
    Ok(object([
        ("verified", Value::Bool(true)),
        ("already_completed", Value::Bool(true)),
        ("outcome", get(value, "outcome").clone()),
        ("assessment_is_agent_assertion", Value::Bool(true)),
        ("source_publication_link_verified", Value::Bool(false)),
        (
            "publication_verified",
            Value::Bool(witnessed != Value::Null),
        ),
        ("witnessed_commit", witnessed),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}

/// Publish the completed record exclusively, or accept an identical one left
/// by an interrupted earlier acknowledgment.
fn publish_completion(
    directory: &Descriptor,
    done_name: &str,
    record: &Value,
    output: &[u8],
) -> CompletionResult<()> {
    match directory.create_exclusive(OsStr::new(done_name), libc::O_WRONLY, 0o600) {
        Ok(mut done) => {
            done.write_all(output).map_err(unavailable)?;
            done.fsync().map_err(unavailable)?;
            drop(done);
            directory.fsync_directory().map_err(unavailable)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let mut done = open_record(directory, done_name)?.ok_or(CompletionError::Unexpected)?;
            let (_, existing) = read_record(&mut done, "Completed boundary evidence is unsafe")?;
            drop(done);
            if !crate::installation::same(&load_record(&existing)?, record) {
                return Err(refuse("Completed boundary differs; preserve both records"));
            }
            Ok(())
        }
        Err(error) => Err(unavailable(error)),
    }
}

/// The baseline legacy acknowledgment body (`claude_boundary.complete`
/// after its observation dispatch; `codex_boundary.legacy_complete`).
pub(crate) fn acknowledge(repo: &Path, value: &Value, legacy: &Legacy) -> CompletionResult<Value> {
    legacy_request(value)?;
    let ns = namespace(repo, legacy)?;
    let session = text(value, "session").unwrap_or_default();
    let prompt = text(value, "prompt").unwrap_or_default();
    let key = sha256_hex(format!("{session}:{prompt}").as_bytes());
    let request_hash = sha256_hex(json::encode(value, json::SORTED_COMPACT_UTF8)?.as_bytes());
    let pending_name = format!("{key}.pending");
    let done_name = format!("{key}.done");
    let directory = Descriptor::open_directory(&ns.directory).map_err(unavailable)?;
    let directory_identity = directory.stat().map_err(unavailable)?;
    let Some(mut pending_fd) = open_record(&directory, &pending_name)? else {
        return already_completed(&ns, &directory, &done_name, value, &request_hash, legacy);
    };
    let (info, raw) = read_record(&mut pending_fd, "Pending boundary is unsafe")?;
    drop(pending_fd);
    let pending = load_record(&raw)?;
    if pending.as_object().is_none()
        || text(&pending, "schema") != Some(legacy.pending_schema)
        || !crate::installation::same(get(&pending, "project_id"), get(&ns.context, "project_id"))
        || (legacy.binds_turn
            && (!crate::installation::same(get(&pending, "session"), get(value, "session"))
                || !crate::installation::same(get(&pending, "turn"), get(value, "prompt"))))
        || !crate::installation::same(get(&pending, "source_uuid"), get(value, "source_uuid"))
        || !crate::installation::same(get(&pending, "source_sha256"), get(value, "source_sha256"))
    {
        return Err(refuse(
            "Pending boundary does not match the assessed source",
        ));
    }
    let witnessed = witnessed(&ns.root, value)?;
    recheck_scope(&ns)?;
    let after = directory
        .stat_at(OsStr::new(&pending_name))
        .map_err(unavailable)?;
    if !same_observation(&after, &info) {
        return Err(refuse("Pending boundary changed during acknowledgment"));
    }
    let record = object([
        ("schema", string(legacy.completed_schema)),
        ("project_id", get(&ns.context, "project_id").clone()),
        ("source_uuid", get(value, "source_uuid").clone()),
        ("source_sha256", get(value, "source_sha256").clone()),
        ("outcome", get(value, "outcome").clone()),
        ("assessment", get(value, "assessment").clone()),
        ("assessment_is_agent_assertion", Value::Bool(true)),
        ("source_publication_link_verified", Value::Bool(false)),
        ("witnessed_commit", witnessed.clone()),
        ("pending_sha256", string(sha256_hex(&raw))),
        ("input_sha256", string(request_hash)),
    ]);
    let output = json::encode(&record, json::SORTED_UTF8)?;
    if output.len() > MAX_RECORD {
        return Err(refuse(legacy.bound_refusal));
    }
    publish_completion(&directory, &done_name, &record, output.as_bytes())?;
    trust_paths::state(&ns.directory)?;
    let current_directory = lstat(&ns.directory).map_err(unavailable)?;
    let current_pending = directory
        .stat_at(OsStr::new(&pending_name))
        .map_err(unavailable)?;
    if !current_directory.same_inode(directory_identity)
        || !same_observation(&current_pending, &info)
    {
        return Err(refuse("Boundary state changed; preserve pending evidence"));
    }
    directory
        .unlink_at(OsStr::new(&pending_name))
        .map_err(unavailable)?;
    directory.fsync_directory().map_err(unavailable)?;
    Ok(object([
        ("verified", Value::Bool(true)),
        ("outcome", get(value, "outcome").clone()),
        ("assessment_is_agent_assertion", Value::Bool(true)),
        (
            "publication_verified",
            Value::Bool(witnessed != Value::Null),
        ),
        ("source_publication_link_verified", Value::Bool(false)),
        ("witnessed_commit", witnessed),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}

/// The installed runtime pack, resolved only where a source operation needs
/// it (`runtime_root` given by the caller, else `cli::pack_root()`).
pub(crate) fn runtime_root(supplied: Option<&Path>) -> Result<PathBuf> {
    match supplied {
        Some(root) => Ok(root.to_path_buf()),
        None => crate::cli::pack_root(),
    }
}

/// Baseline `complete(repo, value)`: an observation-bound assessment is
/// completed by `claude_turns`; otherwise the exact legacy source-bound
/// acknowledgment. `runtime_root` is the installed runtime pack; `None`
/// resolves it (`cli::pack_root()`) only for observation-bound completion.
pub fn complete(
    repo: &Path,
    value: &Value,
    runtime_root: Option<&Path>,
) -> CompletionResult<Value> {
    scope_binding::require(repo, None, RequireOptions::default())?;
    if value.as_object().is_some() && value.get("observation_id").is_some() {
        let root = self::runtime_root(runtime_root)?;
        return Ok(claude_turns::complete(repo, value, &root)?);
    }
    acknowledge(repo, value, &CLAUDE)
}

/// A parsed argparse `type=int` page offset: its Python truthiness and its
/// value for the bounded backlog, where a negative or out-of-range integer
/// becomes `usize::MAX` so the backlog refuses it as the baseline did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageOffset(Option<usize>);
/// argparse `default=0`.
impl Default for PageOffset {
    fn default() -> Self {
        Self(Some(0))
    }
}
impl PageOffset {
    /// Python `int(text)` over ASCII: surrounding whitespace, one optional
    /// sign, digits with single underscores between them. `None` = invalid.
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed =
            text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        let (negative, digits) = match trimmed.as_bytes().first() {
            Some(b'-') => (true, trimmed.get(1..).unwrap_or_default()),
            Some(b'+') => (false, trimmed.get(1..).unwrap_or_default()),
            _ => (false, trimmed),
        };
        let valid = !digits.is_empty()
            && !digits.starts_with('_')
            && !digits.ends_with('_')
            && !digits.contains("__")
            && digits.bytes().all(|b| b.is_ascii_digit() || b == b'_');
        if !valid {
            return None;
        }
        let digits: String = digits.chars().filter(|c| *c != '_').collect();
        if digits.bytes().all(|b| b == b'0') {
            return Some(Self(Some(0)));
        }
        Some(Self(if negative { None } else { digits.parse().ok() }))
    }
    pub fn is_zero(self) -> bool {
        self.0 == Some(0)
    }
    pub fn value(self) -> usize {
        self.0.unwrap_or(usize::MAX)
    }
}

/// One `am claude-boundary` invocation (baseline argparse namespace).
pub struct Request<'a> {
    pub repo: &'a Path,
    pub input: Option<&'a str>,
    pub list_pending: bool,
    pub session: Option<&'a str>,
    pub prompt: Option<&'a str>,
    pub quiet: bool,
    pub refresh: bool,
    pub offset: PageOffset,
    pub legacy_offset: PageOffset,
    pub enrollment_offset: PageOffset,
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|text| !text.is_empty())
}

/// `stdin.buffer.read(65537)`, refused beyond `MAX_STDIN`.
pub(crate) fn read_stdin(input: &mut dyn Read, refusal: &str) -> CompletionResult<Vec<u8>> {
    let mut raw = Vec::new();
    input
        .take(MAX_STDIN as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(unavailable)?;
    if raw.len() > MAX_STDIN {
        return Err(refuse(refusal));
    }
    Ok(raw)
}
/// `load(sharing_limits.read_bytes(path, MAX_PRIVATE))`: only the encoded
/// budget is a reviewed refusal; unavailable or undecodable input is not.
pub(crate) fn read_input(path: &str) -> CompletionResult<Value> {
    let resolved = crate::fs::canonical_path(Path::new(path), true).map_err(unavailable)?;
    let parent = resolved.parent().ok_or(CompletionError::Unexpected)?;
    let name = resolved.file_name().ok_or(CompletionError::Unexpected)?;
    let raw = match crate::fs::read_source(parent, Path::new(name), sharing_limits::MAX_PRIVATE) {
        Ok(observation) => observation.bytes,
        Err(crate::fs::ReadError::SizeLimit { .. }) => {
            return Err(refuse(
                "Complete private input exceeds its explicit encoded budget; no truncation",
            ));
        }
        Err(error) => return Err(unavailable(error)),
    };
    developer_identity::load_with_limit(&raw, sharing_limits::MAX_PRIVATE).map_err(unavailable)
}

fn operation(request: &Request<'_>, input: &mut dyn Read) -> CompletionResult<Value> {
    if request.list_pending {
        if present(request.input) || request.quiet || request.refresh {
            return Err(refuse("Select pending inspection or acknowledgment"));
        }
        let root = runtime_root(None)?;
        return Ok(claude_turns::pending(
            request.repo,
            request.session,
            request.prompt,
            (
                request.offset.value(),
                request.legacy_offset.value(),
                request.enrollment_offset.value(),
            ),
            &root,
        )?);
    }
    if !present(request.input)
        || present(request.session)
        || present(request.prompt)
        || !request.offset.is_zero()
        || !request.legacy_offset.is_zero()
        || !request.enrollment_offset.is_zero()
    {
        return Err(refuse(
            "Acknowledgment requires its exact input, not pending-inventory page offsets",
        ));
    }
    let path = request.input.unwrap_or_default();
    let value = if path == "-" {
        let raw = read_stdin(input, "Stdin assessment exceeds64KiB")?;
        let value = load_record(&raw)?;
        if !request.refresh && text(&value, "outcome") != Some("no-learning") {
            return Err(refuse(
                "Stdin supports no-learning only; published requests require private file input",
            ));
        }
        value
    } else {
        read_input(path)?
    };
    if request.refresh {
        return Ok(turn_check::check(
            request.repo,
            &value,
            |repo, value| complete(repo, value, None),
            "claude",
        )?);
    }
    complete(request.repo, &value, None)
}

/// The refusal reason and its bounded capacity details (`bank_capacity.details`).
pub(crate) fn failure_value(reason: &str, repair: &str, error: &CompletionError) -> Value {
    let mut fields = vec![
        ("verified".to_owned(), Value::Bool(false)),
        ("publication_verified".to_owned(), Value::Bool(false)),
        ("canonical_writes_performed".to_owned(), Value::Bool(false)),
        ("reason".to_owned(), string(reason)),
        ("repair".to_owned(), string(repair)),
    ];
    if let CompletionError::Contract(error) = error {
        if let Value::Object(details) = bank_capacity::details(error) {
            for (key, field) in details {
                match fields.iter_mut().find(|(name, _)| *name == key) {
                    Some(slot) => slot.1 = field,
                    None => fields.push((key, field)),
                }
            }
        }
    }
    Value::Object(fields)
}
pub(crate) fn emitted(result: Result<(i32, String, String)>) -> CompletionResult<CommandResult> {
    let (exit_code, stdout, stderr) = result?;
    Ok(CommandResult {
        value: None,
        exit_code,
        stdout,
        stderr,
    })
}
pub(crate) fn printed(value: &Value) -> String {
    json::encode(value, json::DEFAULT)
        .map(|text| format!("{text}\n"))
        .unwrap_or_default()
}

/// Baseline `main` after argument parsing: the emitted result, or the fixed
/// pending failure (on stderr only for `--refresh --quiet`), exit 1.
pub fn command(request: &Request<'_>, input: &mut dyn Read) -> CommandResult {
    let outcome = operation(request, input)
        .and_then(|result| emitted(turn_check::emit(&result, request.quiet)));
    let error = match outcome {
        Ok(result) => return result,
        Err(error) => error,
    };
    let error = classified(error);
    let reason = match &error {
        CompletionError::Contract(error) => error.message(),
        CompletionError::Unexpected => GENERIC_REASON,
    };
    let failure = failure_value(reason, REPAIR, &error);
    if request.refresh && request.quiet {
        let repair = text(&failure, "repair").unwrap_or_default();
        return CommandResult {
            value: None,
            exit_code: 1,
            stdout: String::new(),
            stderr: format!("am: source assessment pending: {reason} {repair}\n"),
        };
    }
    CommandResult {
        value: None,
        exit_code: 1,
        stdout: printed(&failure),
        stderr: String::new(),
    }
}
