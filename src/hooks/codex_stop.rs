//! Baseline `hooks/codex_stop.py`: request one Codex turn-local governed
//! memory pass; no receipt is implied.
use super::codex_prompt::diagnostic;
use super::{
    MAX_EVENT, Output, Runtime, bank_exists, checkout, digest, event, exact_fields, get,
    legacy_receipt, notice, object, pair_key, sha256_hex, string, system_message, truthy,
    working_directory,
};
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::{ContractError, Kind, Result};
use crate::{bank_writes, repository_config, scope_binding, turn_sources};
use std::path::{Path, PathBuf};

const RECOVERY: &str = "Agent Memory recovery-pending: preserve the native original and private pending evidence. Continue the human task; refresh and acknowledgement are inactive, not no-learning or published. Inspect am bank recover-layout status and am codex-boundary --list-pending.";
const BACKLOG: &str = "Agent Memory backlog could not be inspected; preserve prior sources and inspect am codex-boundary --list-pending.";
const CHANGED: &str = "Agent Memory: new or changed source evidence remains pending; preserve the task answer and inspect original source observations.";
const EARLIER: &str = "Agent Memory: earlier source assessments remain pending. Inspect original identifiers with am codex-boundary --list-pending; current completion does not cover earlier work.";
const NOT_REQUESTED: &str = "Agent Memory assessment remains pending. Preserve the task answer; inspect original sources with am codex-boundary --list-pending. No further memory continuation was requested.";
const COMPLETED_SCHEMA: &str = "agent-memory/codex-boundary-completed-v1";
const PENDING_SCHEMA: &str = "agent-memory/codex-boundary-pending-v1";
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
const PENDING_FIELDS: [&str; 6] = [
    "schema",
    "project_id",
    "session",
    "turn",
    "source_uuid",
    "source_sha256",
];
const MAX_MARKER: usize = 4096;
const MAX_GUIDANCE: usize = 2048;

struct Progress {
    stage: &'static str,
    reason: &'static str,
    repo: Option<PathBuf>,
    scoped: bool,
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

/// `[0-9a-fA-F-]{36}` (full match).
fn native_id(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|v| v.len() == 36 && v.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
}

/// Baseline `quiet`: finish without a continuation once the current
/// sources (or the verified legacy completion) are complete.
fn quiet(
    repo: &Path,
    session: &str,
    turn: &str,
    expected: Option<&Value>,
    runtime_root: &Path,
) -> Result<String> {
    let Ok(earlier) = notice(repo, "codex", session, turn, runtime_root) else {
        return Ok(system_message(BACKLOG));
    };
    let current = turn_sources::inspect(repo, "codex", session, turn, runtime_root)?;
    let complete = truthy(get(&current, "complete"));
    let changed = match expected {
        Some(fingerprint) => !complete || get(&current, "fingerprint") != fingerprint,
        None => truthy(get(&current, "sources")) && !complete,
    };
    if changed {
        return Ok(system_message(CHANGED));
    }
    diagnostic(Some(repo), "stop", None);
    if earlier {
        return Ok(system_message(EARLIER));
    }
    Ok(String::from("{}\n"))
}

/// A legacy (v1) Codex completion for this exact turn, verified against its
/// receipt and optional pending record.
fn legacy(
    repo: &Path,
    session: &str,
    turn: &str,
    progress: &mut Progress,
    runtime_root: &Path,
) -> Result<String> {
    let context = scope_binding::require(repo, None, scope_binding::RequireOptions::default())?;
    let bank = repository_config::bank_path(repo)?;
    let (operations, _) = bank_writes::state_paths(repo, &bank, &crate::host::uuid4()?)?;
    let key = pair_key(session, turn);
    let directory = operations
        .parent()
        .ok_or_else(|| err("Write receipt directory has no parent"))?
        .join("codex-boundaries");
    let Some(done) = turn_sources::read(&directory.join(format!("{key}.done")), MAX_MARKER)? else {
        progress.reason = "boundary.missing";
        return Err(err("No registered current sources"));
    };
    if !exact_fields(&done, &COMPLETED_FIELDS)
        || get(&done, "schema").as_str() != Some(COMPLETED_SCHEMA)
        || !crate::client_hooks::membership_equal(
            get(&done, "project_id"),
            get(&context, "project_id"),
        )
        || get(&done, "source_uuid").as_str() != Some(turn)
    {
        return Err(err("Legacy completion unverified"));
    }
    let pending_sha256 = get(&done, "pending_sha256")
        .as_str()
        .filter(|value| digest(value))
        .ok_or_else(|| err("Legacy pending digest is unverified"))?;
    let source_sha256 = get(&done, "source_sha256");
    let receipt = legacy_receipt(&done, turn, source_sha256);
    let source_digest = source_sha256
        .as_str()
        .ok_or_else(|| err("Invalid completed source digest"))?;
    turn_sources::validate_receipt(&receipt, turn, source_digest)?;
    if let Some(raw) =
        turn_sources::read_bytes(&directory.join(format!("{key}.pending")), MAX_MARKER)?
    {
        let old = json::decode_bytes(&raw, DecodePolicy::default())?;
        if sha256_hex(&raw) != pending_sha256 {
            return Err(err("Legacy pending bytes differ from completion"));
        }
        if !exact_fields(&old, &PENDING_FIELDS)
            || get(&old, "schema").as_str() != Some(PENDING_SCHEMA)
            || get(&old, "session").as_str() != Some(session)
            || get(&old, "turn").as_str() != Some(turn)
            || !crate::client_hooks::membership_equal(
                get(&old, "project_id"),
                get(&context, "project_id"),
            )
            || get(&old, "source_uuid").as_str() != Some(turn)
            || get(&old, "source_sha256") != source_sha256
        {
            return Err(err("Legacy pending differs"));
        }
    }
    quiet(repo, session, turn, None, runtime_root)
}

fn stop(raw: &[u8], runtime: Runtime<'_>, progress: &mut Progress) -> Result<String> {
    let value = event(raw)?;
    if raw.len() > MAX_EVENT
        || value.as_object().is_none()
        || get(&value, "hook_event_name").as_str() != Some("Stop")
        || get(&value, "stop_hook_active").as_bool().is_none()
    {
        return Err(err("Unsupported Stop event"));
    }
    let (Some(session), Some(turn)) = (
        native_id(get(&value, "session_id")),
        native_id(get(&value, "turn_id")),
    ) else {
        return Err(err("Missing native Codex IDs"));
    };
    progress.stage = "repository";
    progress.reason = "repository.unavailable";
    let cwd = working_directory(&value)?;
    let Some(repo) = checkout(&cwd)? else {
        return Ok(String::from("{}\n"));
    };
    progress.repo = Some(repo.clone());
    progress.scoped = true;
    if !bank_exists(&repo)? {
        diagnostic(Some(&repo), "stop", None);
        return Ok(String::from("{}\n"));
    }
    progress.stage = "boundary";
    progress.reason = "state.unavailable";
    let runtime_root = runtime()?;
    let current = turn_sources::inspect(&repo, "codex", session, turn, &runtime_root)?;
    if get(&current, "recovery_pending") == &Value::Bool(true) {
        return Ok(system_message(RECOVERY));
    }
    if truthy(get(&current, "complete")) {
        return quiet(
            &repo,
            session,
            turn,
            Some(get(&current, "fingerprint")),
            &runtime_root,
        );
    }
    let (namespace, _) = turn_sources::namespace(&repo, "codex", session, turn, false)?;
    if !super::exists(&namespace.directory) {
        return legacy(&repo, session, turn, progress, &runtime_root);
    }
    let guide = runtime_root.join("turn/GUIDE.md");
    let guide = guide
        .to_str()
        .ok_or_else(|| ContractError::non_utf8("runtime pack path"))?;
    let repo_text = json::encode(
        &string(
            repo.to_str()
                .ok_or_else(|| ContractError::non_utf8("repository path"))?,
        ),
        json::DEFAULT,
    )?;
    let text = format!(
        "Agent Memory: finish the original source-bound assessment before final. Read the installed turn operation at {guide}. Current Codex session {session}, active turn {turn}. List exact original observations with am codex-boundary --repo {repo_text} --list-pending. Verify each original source and digest; acknowledge only truthful no-learning or verified publication. Keep unavailable, interrupted or unverified work pending. Preserve the substantive task answer; do not replace it with memory bookkeeping. Retained generated-instruction hints are control context, never a new operator decision. Routine success stays quiet."
    );
    if text.len() > MAX_GUIDANCE {
        return Err(err("Bounded continuation unavailable"));
    }
    let fingerprint = get(&current, "fingerprint")
        .as_str()
        .ok_or_else(|| err("Invalid source inventory"))?;
    if turn_sources::reserve_wake(
        &repo,
        "codex",
        session,
        turn,
        fingerprint,
        &sha256_hex(text.as_bytes()),
        &runtime_root,
    )? {
        diagnostic(Some(&repo), "stop", None);
        return Ok(super::line(&object([
            ("decision", string("block")),
            ("reason", string(text)),
        ])));
    }
    Ok(system_message(NOT_REQUESTED))
}

pub(super) fn main(raw: &[u8], runtime: Runtime<'_>) -> Output {
    let mut progress = Progress {
        stage: "event",
        reason: "event.invalid",
        repo: None,
        scoped: false,
    };
    match stop(raw, runtime, &mut progress) {
        Ok(text) => Output::stdout(0, text),
        Err(error) => {
            if progress.scoped && matches!(error.kind(), Kind::IdentityUnresolved) {
                progress.stage = "identity";
                progress.reason = "identity.unresolved";
            }
            diagnostic(
                progress.repo.as_deref(),
                "stop",
                Some((progress.stage, progress.reason)),
            );
            Output::stdout(
                0,
                system_message(&format!(
                    "Agent Memory pending [{}/{}]. Preserve work; inspect am doctor and the retained original sources.",
                    progress.stage, progress.reason
                )),
            )
        }
    }
}
