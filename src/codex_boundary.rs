//! Acknowledge one source-bound Codex boundary without inventing publication.
//!
//! Baseline `codex_boundary.py`. Observation-bound assessments complete the
//! retained `turn_sources` observation; legacy `source_uuid` assessments
//! acknowledge a retained `codex-boundaries/<key>.pending` record through the
//! shared legacy acknowledgment in `claude_boundary`.
use crate::claude_boundary::{
    self, Legacy, PageOffset, exact_fields, failure_value, read_input, read_stdin, safe_record,
    uuid_text,
};
use crate::codec::json::Value;
use crate::developer_identity::{self, sha256_hex};
use crate::error::ContractError;
use crate::fs::Descriptor;
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::scope_binding::{self, RequireOptions};
use crate::turn_check::{self, CompletionError, CompletionResult};
use crate::{bank_writes, host, trust_paths, turn_sources};
use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Directory entries a legacy pending inventory inspects at most.
pub const MAX_INVENTORY_ENTRIES: usize = 65536;
/// Legacy pending records a legacy pending inventory returns at most.
pub const MAX_PENDING: usize = 4096;
const OBSERVATION_FIELDS: [&str; 6] = [
    "session",
    "prompt",
    "observation_id",
    "source_sha256",
    "outcome",
    "assessment",
];
const PENDING_FIELDS: [&str; 6] = [
    "schema",
    "project_id",
    "session",
    "turn",
    "source_uuid",
    "source_sha256",
];
const GENERIC_REASON: &str =
    "Boundary completion remains pending; preserve evidence and inspect the failure.";
const REPAIR: &str =
    "Preserve boundary records; inspect retained completed and pending evidence before retrying.";

const CODEX: Legacy = Legacy {
    directory: "codex-boundaries",
    pending_schema: "agent-memory/codex-boundary-pending-v1",
    completed_schema: "agent-memory/codex-boundary-completed-v1",
    bound_refusal: "Encoded legacy completion exceeds bound; preserve pending source",
    binds_turn: true,
};

fn refuse(message: &str) -> CompletionError {
    CompletionError::Contract(ContractError::new(message))
}
fn unavailable<E>(_: E) -> CompletionError {
    CompletionError::Unexpected
}

/// Baseline `legacy_complete(repo, value)`: acknowledge one retained legacy
/// Codex boundary with its exact source-bound assessment.
pub fn legacy_complete(repo: &Path, value: &Value) -> CompletionResult<Value> {
    claude_boundary::acknowledge(repo, value, &CODEX)
}

/// Baseline `complete(repo, value)`: complete one retained Codex observation
/// with its verified assessment, or a legacy boundary. `runtime_root` is the
/// installed runtime pack; `None` resolves it (`cli::pack_root()`) only for
/// observation-bound completion.
pub fn complete(
    repo: &Path,
    value: &Value,
    runtime_root: Option<&Path>,
) -> CompletionResult<Value> {
    if value.as_object().is_none() {
        return Err(refuse("Explicit source assessment required"));
    }
    scope_binding::require(repo, None, RequireOptions::default())?;
    if value.get("observation_id").is_none() {
        return legacy_complete(repo, value);
    }
    if !exact_fields(value, &OBSERVATION_FIELDS) {
        return Err(refuse("Use exact observation-bound assessment fields"));
    }
    for key in ["session", "prompt", "observation_id"] {
        if !text(value, key).is_some_and(uuid_text) {
            return Err(refuse(
                "Use actual Codex identity and local observation UUID",
            ));
        }
    }
    let session = text(value, "session").unwrap_or_default();
    let prompt = text(value, "prompt").unwrap_or_default();
    let observation_id = text(value, "observation_id").unwrap_or_default();
    let root = claude_boundary::runtime_root(runtime_root)?;
    let observation =
        turn_sources::get_source(repo, "codex", session, prompt, observation_id, &root)?;
    if !crate::installation::same(
        get(&observation, "source_sha256"),
        get(value, "source_sha256"),
    ) {
        return Err(refuse("Original source digest differs"));
    }
    let receipt = turn_sources::verify_assessment(repo, value, observation_id, &root)?;
    let source_sha256 = text(value, "source_sha256").ok_or(CompletionError::Unexpected)?;
    Ok(turn_sources::complete(
        repo,
        "codex",
        session,
        prompt,
        observation_id,
        source_sha256,
        &receipt,
        &root,
    )?)
}

/// One retained legacy pending record, validated against its file name and
/// the enrolled project. A non-text identifier is unavailable evidence, as
/// the baseline's `TypeError`.
fn legacy_pending_record(
    directory: &Descriptor,
    name: &OsStr,
    project_id: &Value,
) -> CompletionResult<Value> {
    let mut fd = directory
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(unavailable)?;
    let info = fd.stat().map_err(unavailable)?;
    if !safe_record(&info) {
        return Err(refuse("Unsafe pending record"));
    }
    let raw = fd
        .read_prefix(claude_boundary::MAX_RECORD + 1)
        .map_err(unavailable)?;
    drop(fd);
    let record = developer_identity::load(&raw).map_err(unavailable)?;
    let malformed = || refuse("Pending record is malformed or belongs to another project");
    let keys_match = record.as_object().is_some_and(|pairs| {
        pairs.len() == PENDING_FIELDS.len()
            && PENDING_FIELDS.iter().all(|k| record.get(k).is_some())
    });
    if !keys_match
        || text(&record, "schema") != Some(CODEX.pending_schema)
        || !crate::installation::same(get(&record, "project_id"), project_id)
    {
        return Err(malformed());
    }
    let session = text(&record, "session").ok_or(CompletionError::Unexpected)?;
    if !uuid_text(session) {
        return Err(malformed());
    }
    let turn = text(&record, "turn").ok_or(CompletionError::Unexpected)?;
    if !uuid_text(turn)
        || !crate::installation::same(get(&record, "source_uuid"), get(&record, "turn"))
    {
        return Err(malformed());
    }
    let source_sha256 = text(&record, "source_sha256").ok_or(CompletionError::Unexpected)?;
    let expected = format!(
        "{}.pending",
        sha256_hex(format!("{session}:{turn}").as_bytes())
    );
    if !claude_boundary::digest_text(source_sha256) || name.as_bytes() != expected.as_bytes() {
        return Err(malformed());
    }
    Ok(record)
}

/// Baseline `legacy_list_pending(repo)`: every retained legacy Codex pending
/// record of this project, under its private enrollment.
pub fn legacy_list_pending(repo: &Path, runtime_root: &Path) -> CompletionResult<Value> {
    let (root, bank) = bank_writes::bank_path(repo)?;
    let context = scope_binding::private_enrollment_context(&root, runtime_root)?;
    let (operations, _) = bank_writes::state_paths(&root, &bank, &host::uuid4()?)?;
    let directory = operations
        .parent()
        .ok_or(CompletionError::Unexpected)?
        .join(CODEX.directory);
    match claude_boundary::lstat(&directory) {
        Err(error) if matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ENOTDIR)) => {
            scope_binding::recheck_private_enrollment(&context)?;
            return Ok(object([
                ("operation", string("pending")),
                ("verified", Value::Bool(true)),
                ("pending", Value::Array(Vec::new())),
                ("canonical_writes_performed", Value::Bool(false)),
            ]));
        }
        _ => {}
    }
    trust_paths::state(&directory)?;
    let dfd = Descriptor::open_directory(&directory).map_err(unavailable)?;
    let names = dfd
        .entries_bounded(MAX_INVENTORY_ENTRIES)
        .map_err(unavailable)?
        .ok_or_else(|| {
            refuse("Boundary inventory exceeds 65536 entries; preserve evidence for inspection")
        })?;
    let mut entries = Vec::new();
    for name in names {
        if !name.as_bytes().ends_with(b".pending") {
            continue;
        }
        if entries.len() >= MAX_PENDING {
            return Err(refuse(
                "Pending inventory exceeds 4096 entries; preserve evidence for inspection",
            ));
        }
        entries.push(legacy_pending_record(&dfd, &name, context.project_id())?);
    }
    scope_binding::recheck_private_enrollment(&context)?;
    Ok(object([
        ("operation", string("pending")),
        ("verified", Value::Bool(true)),
        ("pending", Value::Array(entries)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("source_completeness_verified", Value::Bool(false)),
    ]))
}

/// Baseline `list_pending(repo, offset, legacy_offset, enrollment_offset)`:
/// the bounded Codex backlog page with its legacy sources as `pending`.
pub fn list_pending(
    repo: &Path,
    offsets: (usize, usize, usize),
    runtime_root: &Path,
) -> CompletionResult<Value> {
    let (offset, legacy_offset, enrollment_offset) = offsets;
    let backlog = turn_sources::backlog(
        repo,
        Some("codex"),
        offset,
        legacy_offset,
        enrollment_offset,
        runtime_root,
    )?;
    let pending = get(&backlog, "legacy")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .map(|row| {
            row.get("source")
                .cloned()
                .ok_or(CompletionError::Unexpected)
        })
        .collect::<CompletionResult<Vec<_>>>()?;
    let mut fields = vec![
        ("operation".to_owned(), string("pending")),
        ("verified".to_owned(), Value::Bool(true)),
        ("pending".to_owned(), Value::Array(pending)),
    ];
    let Value::Object(rows) = backlog else {
        return Err(CompletionError::Unexpected);
    };
    for (key, field) in rows
        .into_iter()
        .chain([("canonical_writes_performed".to_owned(), Value::Bool(false))])
    {
        match fields.iter_mut().find(|(name, _)| *name == key) {
            Some(slot) => slot.1 = field,
            None => fields.push((key, field)),
        }
    }
    Ok(Value::Object(fields))
}

/// One `am codex-boundary` invocation (baseline argparse namespace).
pub struct Request<'a> {
    pub repo: &'a Path,
    pub input: Option<&'a str>,
    pub list_pending: bool,
    pub stdin: bool,
    pub offset: PageOffset,
    pub legacy_offset: PageOffset,
    pub quiet: bool,
    pub refresh: bool,
    pub enrollment_offset: PageOffset,
}

fn operation(request: &Request<'_>, input: &mut dyn Read) -> CompletionResult<Value> {
    let file = request.input.filter(|path| !path.is_empty());
    if request.list_pending {
        if file.is_some() || request.stdin || request.quiet || request.refresh {
            return Err(refuse("Select either pending inventory or acknowledgement"));
        }
        let root = claude_boundary::runtime_root(None)?;
        return list_pending(
            request.repo,
            (
                request.offset.value(),
                request.legacy_offset.value(),
                request.enrollment_offset.value(),
            ),
            &root,
        );
    }
    if file.is_some() == request.stdin {
        return Err(refuse("Select one file or bounded stdin assessment"));
    }
    let value = match file {
        None => {
            let raw = read_stdin(input, "Stdin assessment exceeds bound")?;
            let value = developer_identity::load(&raw).map_err(unavailable)?;
            if !request.refresh && text(&value, "outcome") != Some("no-learning") {
                return Err(refuse("Stdin accepts no-learning only"));
            }
            value
        }
        Some(path) => read_input(path)?,
    };
    if request.refresh {
        return Ok(turn_check::check(
            request.repo,
            &value,
            |repo, value| complete(repo, value, None),
            "codex",
        )?);
    }
    complete(request.repo, &value, None)
}

/// Only a measured bank capacity refusal or a bounded inventory refusal is
/// shown; every other failure keeps the fixed pending reason.
fn reason(error: &CompletionError) -> &str {
    match error {
        CompletionError::Contract(error)
            if matches!(error.kind(), crate::error::Kind::BankCapacity { .. })
                || error.message().contains("inventory exceeds") =>
        {
            error.message()
        }
        _ => GENERIC_REASON,
    }
}

/// Baseline `main` after argument parsing: the emitted result, or the fixed
/// pending failure (stderr under `--quiet`, stdout JSON otherwise), exit 1.
pub fn command(request: &Request<'_>, input: &mut dyn Read) -> CommandResult {
    let outcome = operation(request, input)
        .and_then(|result| claude_boundary::emitted(turn_check::emit(&result, request.quiet)));
    let error = match outcome {
        Ok(result) => return result,
        Err(error) => error,
    };
    let reason = reason(&error);
    let failure = failure_value(reason, REPAIR, &error);
    if request.quiet {
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
        stdout: claude_boundary::printed(&failure),
        stderr: String::new(),
    }
}
