//! Baseline `hooks/claude_stop.py`: opt-in Claude asyncRewake guidance; not a
//! capture or publication receipt.
//!
//! Register only as an asyncRewake Stop hook. A second Stop in the resulting
//! continuation is ignored; the existing agent performs any permitted work.
use super::claude_source;
use super::{
    MAX_EVENT, Output, Runtime, checked_boundary_prompt, checkout, digest, event, exact_fields,
    get, legacy_receipt, notice, object, pair_key, read_up_to, sha256_hex, string, system_message,
    working_directory,
};
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::{ContractError, Result};
use crate::fs::{self, Descriptor};
use crate::{bank_writes, scope_binding, turn_sources};
use std::ffi::OsStr;
use std::path::Path;

const FAILED: &str = "Agent Memory boundary could not verify its source or private state; capture and publication are pending. Preserve the bank and pending evidence, run am doctor in this repository, and inspect before retrying.";
const RECOVERY: &str = "Agent Memory recovery-pending: preserve the native original and private pending evidence. Continue the human task; refresh and acknowledgement are inactive, not no-learning or published. Inspect am bank recover-layout status and am claude-boundary --list-pending.";
const EARLIER: &str = "Agent Memory has earlier retained pending sources; inspect am claude-boundary --list-pending. Current work may continue.";
const CONTINUED: &str = "Agent Memory remains pending for retained native sources; preserve the task answer and inspect its original observations.";
const ATTEMPTED: &str = "Agent Memory remains pending; its bounded continuation was already attempted. Preserve work and inspect the retained source.";
const RETAINED: &str = "Agent Memory has a retained pending boundary; inspect it before retrying.";
const PASS_TAIL: &str = ". Assess exact original sources; complete only verified work. Preserve the substantive task answer. No-learning success is silent. If tools/evidence are unavailable, leave memory pending. The original answer is already retained. Do not repeat it. End silently after success; report only one brief actionable pending qualification if necessary.";
const LEGACY_TAIL: &str = ". Acknowledge truthful no-learning or verified publication via am claude-boundary. Preserve the substantive task answer; routine bookkeeping stays quiet. Unavailable tools or evidence leave memory pending. The original answer is already retained. Do not repeat it. End silently after success; report only one brief actionable pending qualification if necessary.";
const COMPLETED_SCHEMA: &str = "agent-memory/claude-boundary-completed-v1";
const PENDING_SCHEMA: &str = "agent-memory/claude-boundary-pending-v1";
const COMPLETED_FIELDS: [&str; 11] = [
    "schema",
    "project_id",
    "source_uuid",
    "source_sha256",
    "outcome",
    "assessment",
    "assessment_is_agent_assertion",
    "source_publication_link_verified",
    "witnessed_commit",
    "pending_sha256",
    "input_sha256",
];
const PENDING_FIELDS: [&str; 5] = [
    "schema",
    "project_id",
    "source_uuid",
    "source_sha256",
    "assistant_sha256",
];
/// Bound of retained legacy boundary markers.
const MAX_MARKER: usize = 4096;
/// Unresolved legacy boundaries the ledger retains before refusing new ones.
const MAX_PENDING: usize = 4096;
/// Entries a legacy ledger listing may hold (the baseline scanned without a
/// bound); beyond it the hook refuses rather than reading without limit.
const MAX_LEDGER_ENTRIES: usize = 1_000_000;
const MAX_GUIDANCE: usize = 2048;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

fn io(error: std::io::Error) -> ContractError {
    ContractError::new(error.to_string())
}

/// `pack/'turn/GUIDE.md'` of the executing runtime.
fn guide(runtime: Runtime<'_>) -> Result<String> {
    let path = runtime()?.join("turn/GUIDE.md");
    Ok(path
        .to_str()
        .ok_or_else(|| ContractError::non_utf8("runtime pack path"))?
        .to_owned())
}

fn is_directory(path: &Path) -> Result<bool> {
    Ok(crate::fs::trust::named_stat(path)
        .map_err(io)?
        .is_directory())
}

/// Baseline `observed_stop`: a native prompt already enrolled by v2 can
/// finish before final quietly. `None` continues with legacy handling.
fn observed_stop(repo: &Path, value: &Value, runtime: Runtime<'_>) -> Result<Option<Output>> {
    let (Some(session), Some(prompt)) = (
        get(value, "session_id").as_str(),
        get(value, "prompt_id").as_str(),
    ) else {
        // Legacy source handling still reports unsupported evidence.
        return Ok(None);
    };
    let runtime_root = runtime()?;
    let state = turn_sources::inspect(repo, "claude", session, prompt, &runtime_root)?;
    if get(&state, "recovery_pending") == &Value::Bool(true) {
        return Ok(Some(Output::stdout(0, system_message(RECOVERY))));
    }
    let sources = get(&state, "sources")
        .as_array()
        .ok_or_else(|| err("Invalid source inventory"))?;
    let faults = get(&state, "faults")
        .as_array()
        .ok_or_else(|| err("Invalid source inventory"))?;
    if sources.is_empty()
        && (faults.is_empty()
            || matches!(faults, [only] if only.as_str() == Some("source-missing")))
    {
        return Ok(None);
    }
    if super::truthy(get(&state, "complete")) {
        if notice(repo, "claude", session, prompt, &runtime_root)? {
            return Ok(Some(Output::stdout(0, system_message(EARLIER))));
        }
        return Ok(Some(Output::quiet()));
    }
    let message = format!(
        "Agent Memory pass for native Claude session {session}, prompt {prompt}. Read {}; inspect am claude-boundary --list-pending --session {session} --prompt {prompt}{PASS_TAIL}",
        guide(runtime)?
    );
    if message.len() > MAX_GUIDANCE {
        return Err(err("guidance bound"));
    }
    if get(value, "stop_hook_active") == &Value::Bool(true) {
        return Ok(Some(Output::stdout(0, system_message(CONTINUED))));
    }
    let fingerprint = get(&state, "fingerprint")
        .as_str()
        .ok_or_else(|| err("Invalid source inventory"))?;
    if !turn_sources::reserve_wake(
        repo,
        "claude",
        session,
        prompt,
        fingerprint,
        &sha256_hex(message.as_bytes()),
        &runtime_root,
    )? {
        return Ok(Some(Output::stdout(0, system_message(ATTEMPTED))));
    }
    Ok(Some(Output {
        exit_code: 2,
        stdout: String::new(),
        stderr: format!("{message}\n"),
    }))
}

/// A retained completion of this exact legacy source, checked against its
/// optional pending record; any mismatch is a refusal, never a quiet pass.
fn verify_completed(
    ledger: &Path,
    key: &str,
    mut file: Descriptor,
    context: &Value,
    scoped: &Value,
) -> Result<()> {
    let info = file.stat().map_err(io)?;
    if !info.is_regular()
        || info.uid != crate::host::effective_uid()
        || info.nlink != 1
        || info.mode & 0o077 != 0
        || info.size > MAX_MARKER as i64
    {
        return Err(err("unsafe completed boundary"));
    }
    let raw = read_up_to(&mut file, MAX_MARKER + 1).map_err(io)?;
    drop(file);
    let completed = json::decode_bytes(&raw, DecodePolicy::default())
        .map_err(|_| err("invalid completed boundary"))?;
    if completed.as_object().is_none()
        || get(&completed, "schema").as_str() != Some(COMPLETED_SCHEMA)
        || !crate::client_hooks::membership_equal(
            get(&completed, "project_id"),
            get(context, "project_id"),
        )
        || get(&completed, "source_uuid") != get(scoped, "source_uuid")
        || get(&completed, "source_sha256") != get(scoped, "source_sha256")
    {
        return Err(err("invalid completed boundary"));
    }
    if !exact_fields(&completed, &COMPLETED_FIELDS) {
        return Err(err("malformed completed boundary"));
    }
    let pending_sha256 = get(&completed, "pending_sha256")
        .as_str()
        .filter(|value| digest(value))
        .ok_or_else(|| err("invalid pending digest"))?;
    let source_uuid = get(scoped, "source_uuid").as_str().unwrap_or_default();
    let source_sha256 = get(scoped, "source_sha256");
    let receipt = legacy_receipt(&completed, source_uuid, source_sha256);
    turn_sources::validate_receipt(
        &receipt,
        source_uuid,
        source_sha256.as_str().unwrap_or_default(),
    )?;
    let Some(retained) =
        turn_sources::read_bytes(&ledger.join(format!("{key}.pending")), MAX_MARKER)?
    else {
        return Ok(());
    };
    let old = json::decode_bytes(&retained, DecodePolicy::default())?;
    if sha256_hex(&retained) != pending_sha256 {
        return Err(err("pending source bytes differ"));
    }
    if !exact_fields(&old, &PENDING_FIELDS)
        || get(&old, "schema").as_str() != Some(PENDING_SCHEMA)
        || !crate::client_hooks::membership_equal(
            get(&old, "project_id"),
            get(context, "project_id"),
        )
        || get(&old, "source_uuid") != get(scoped, "source_uuid")
        || get(&old, "source_sha256") != get(scoped, "source_sha256")
    {
        return Err(err("pending source differs"));
    }
    Ok(())
}

/// Retain one legacy pending boundary. `None` means a new record was
/// written; `Some` is the hook's final output for an existing record.
fn retain_pending(
    ledger: &Path,
    key: &str,
    context: &Value,
    scoped: &Value,
) -> Result<Option<Output>> {
    let directory = Descriptor::open_directory(ledger).map_err(io)?;
    let done = OsStr::new(&format!("{key}.done")).to_os_string();
    match directory.open_at(&done, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(file) => {
            verify_completed(ledger, key, file, context, scoped)?;
            return Ok(Some(Output::quiet()));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    // Preserve unresolved evidence and place a finite upper bound on
    // routine bookkeeping; never delete or silently evict pending work.
    let names = directory
        .entries_bounded(MAX_LEDGER_ENTRIES)
        .map_err(io)?
        .ok_or_else(|| err("boundary pending inventory is full"))?;
    let pending = names
        .iter()
        .filter(|name| name.as_encoded_bytes().ends_with(b".pending"))
        .count();
    if pending >= MAX_PENDING {
        return Err(err("boundary pending inventory is full"));
    }
    let name = OsStr::new(&format!("{key}.pending")).to_os_string();
    let mut file =
        match directory.open_at(&name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL, 0o600) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Ok(Some(Output::stdout(0, system_message(RETAINED))));
            }
            Err(error) => return Err(io(error)),
        };
    let record = object([
        ("schema", string(PENDING_SCHEMA)),
        ("project_id", get(context, "project_id").clone()),
        ("source_uuid", get(scoped, "source_uuid").clone()),
        ("source_sha256", get(scoped, "source_sha256").clone()),
        ("assistant_sha256", get(scoped, "assistant_sha256").clone()),
    ]);
    file.write_all(json::encode(&record, json::DEFAULT)?.as_bytes())
        .map_err(io)?;
    file.fsync().map_err(io)?;
    drop(file);
    directory.fsync_directory().map_err(io)?;
    Ok(None)
}

fn stop(raw: &[u8], runtime: Runtime<'_>) -> Result<Output> {
    if raw.len() > MAX_EVENT {
        return Err(err("oversized event"));
    }
    let value = event(raw)?;
    if value.as_object().is_none() || get(&value, "hook_event_name").as_str() != Some("Stop") {
        return Err(err("unsupported event"));
    }
    let Some(active) = get(&value, "stop_hook_active").as_bool() else {
        return Err(err("missing continuation guard"));
    };
    let cwd = working_directory(&value)?;
    if !is_directory(&cwd)? {
        return Err(err("working directory is unavailable"));
    }
    // No repository is not a memory failure.
    let Some(repo) = checkout(&cwd)? else {
        return Ok(Output::quiet());
    };
    // A repository without a bank is not automatically initialized.
    if !super::bank_exists(&repo)? {
        return Ok(Output::quiet());
    }
    let (repo, bank) = bank_writes::bank_path(&repo)?;
    if let Some(decision) = observed_stop(&repo, &value, runtime)? {
        return Ok(decision);
    }
    let context = scope_binding::require(&repo, None, scope_binding::RequireOptions::default())?;
    if active {
        return Ok(Output::quiet());
    }
    let scoped = claude_source::source(&value, false)?;
    let (operations, _) = bank_writes::state_paths(&repo, &bank, &crate::host::uuid4()?)?;
    let ledger = operations
        .parent()
        .ok_or_else(|| err("Write receipt directory has no parent"))?
        .join("claude-boundaries");
    fs::trust::inspect(&ledger, fs::trust::InspectOptions::default())?;
    fs::private_mkdir(&ledger)?;
    fs::trust::state(&ledger)?;
    let session = get(&scoped, "session").as_str().unwrap_or_default();
    let prompt = get(&scoped, "prompt").as_str().unwrap_or_default();
    if let Some(decision) = retain_pending(&ledger, &pair_key(session, prompt), &context, &scoped)?
    {
        return Ok(decision);
    }
    checked_boundary_prompt()?;
    // A legacy v1 turn still receives one short, source-bound continuation.
    let message = format!(
        "Agent Memory: verify exact Claude source session={session} prompt={prompt} source_uuid={} source_sha256={}. Verify that exact source in {}; follow {}{LEGACY_TAIL}",
        get(&scoped, "source_uuid").as_str().unwrap_or_default(),
        get(&scoped, "source_sha256").as_str().unwrap_or_default(),
        get(&scoped, "transcript").as_str().unwrap_or_default(),
        guide(runtime)?,
    );
    if message.len() > MAX_GUIDANCE {
        return Err(err("guidance bound"));
    }
    Ok(Output {
        exit_code: 2,
        stdout: String::new(),
        stderr: format!("{message}\n"),
    })
}

pub(super) fn main(raw: &[u8], runtime: Runtime<'_>) -> Output {
    match stop(raw, runtime) {
        Ok(output) => output,
        // A failed hook is never an admission or publication receipt.
        Err(_) => Output::stdout(0, system_message(FAILED)),
    }
}
