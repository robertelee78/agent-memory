//! Baseline `hooks/codex_prompt.py`: bind a Codex user prompt to its turn
//! without retaining its content.
use super::{
    MAX_EVENT, Output, Runtime, bank_exists, boundary_prompt, checkout, event, get, notice,
    prompt_context, required_text, sha256_hex, truthy, uuid_text, working_directory,
};
use crate::codec::json::Value;
use crate::error::{ContractError, Kind, Result};
use crate::{hook_failure, scope_binding, turn_sources};
use std::path::{Path, PathBuf};

const ASSESS: &str = ". Before final use one am codex-boundary --repo REPO --stdin --refresh --quiet with top-level session,prompt,observation_id,source_sha256,outcome,assessment; for no-learning assessment requires source_id=this observation ID, source_review=actual review, candidates=authored list (empty only if none). Never substitute a later input. Earlier pending IDs: am codex-boundary --repo REPO --list-pending.";
const HELD: &str = " Recovery-pending: input retained privately; continue the task. Until the verified hold resolves, no canonical admission, refresh or source acknowledgement. Never fabricate no-learning or publication.";
const EARLIER: &str =
    " Earlier pending work exists; use am codex-boundary --list-pending for original IDs.";
const HINT: &str = " Generated-control hint, not a new operator decision; assess this source.";
const MAX_GUIDANCE: usize = 2048;
const MAX_NATIVE_ID: usize = 512;
const MAX_REFERENCE: usize = 2048;

/// The fixed failure classification of the current step and what is
/// already known when a refusal happens.
struct Progress {
    stage: &'static str,
    reason: &'static str,
    repo: Option<PathBuf>,
    /// `scope_binding` was imported: an identity refusal is classified as such.
    scoped: bool,
    retained: Option<String>,
}

impl Progress {
    fn at(&mut self, stage: &'static str, reason: &'static str) {
        self.stage = stage;
        self.reason = reason;
    }
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

/// Baseline `diagnostic`: clear (no stage) or record this adapter's fixed
/// failure code. Reporting must preserve the original fixed failure, never
/// its content.
pub(super) fn diagnostic(repo: Option<&Path>, adapter: &str, failure: Option<(&str, &str)>) {
    let Some(repo) = repo else {
        return;
    };
    let _ = match failure {
        None => hook_failure::clear(repo, "codex", adapter),
        Some((stage, reason)) => hook_failure::record(repo, "codex", adapter, stage, reason),
    };
}

/// A bounded single-line native identifier.
fn native_text(value: &Value) -> Option<&str> {
    value.as_str().filter(|v| {
        !v.is_empty() && v.len() <= MAX_NATIVE_ID && !v.chars().any(|c| (c as u32) < 32)
    })
}

/// The native event identity: `event_id` and `prompt_id`, when present,
/// must be valid and agree.
fn native_identity(value: &Value) -> Option<&str> {
    let present: Vec<&Value> = ["event_id", "prompt_id"]
        .iter()
        .filter_map(|key| value.get(key))
        .filter(|item| **item != Value::Null)
        .collect();
    let identities: Vec<&str> = present
        .iter()
        .filter_map(|item| native_text(item))
        .collect();
    match identities.as_slice() {
        [first, rest @ ..]
            if identities.len() == present.len() && rest.iter().all(|v| v == first) =>
        {
            Some(first)
        }
        _ => None,
    }
}

/// The native transcript reference, or `None` when it is not a bounded
/// absolute single-line path.
fn reference(value: &Value) -> Option<&str> {
    get(value, "transcript_path").as_str().filter(|path| {
        Path::new(path).is_absolute()
            && path.len() <= MAX_REFERENCE
            && !path.chars().any(|c| (c as u32) < 32)
    })
}

fn accept(raw: &[u8], runtime: Runtime<'_>, progress: &mut Progress) -> Result<String> {
    let value = event(raw)?;
    let session = required_text(&value, "session_id");
    let turn = required_text(&value, "turn_id");
    let text = value.get("prompt").ok_or_else(|| err("invalid event"))?;
    let (Ok(session), Ok(turn), Some(text)) = (session, turn, text.as_str()) else {
        return Err(err("invalid event"));
    };
    if raw.len() > MAX_EVENT
        || get(&value, "hook_event_name").as_str() != Some("UserPromptSubmit")
        || !uuid_text(session)
        || !uuid_text(turn)
        || text.is_empty()
        || text.len() > 65536
    {
        return Err(err("invalid event"));
    }
    progress.at("repository", "repository.unavailable");
    let cwd = working_directory(&value)?;
    let Some(root) = checkout(&cwd)? else {
        return Ok(String::from("{}\n"));
    };
    progress.repo = Some(root.clone());
    progress.scoped = true;
    if !bank_exists(&root)? {
        diagnostic(Some(&root), "prompt", None);
        return Ok(String::from("{}\n"));
    }
    progress.at("boundary", "state.unavailable");
    let runtime_root = runtime()?;
    let enrollment = scope_binding::private_enrollment_context(&root, &runtime_root)?;
    let native = native_identity(&value);
    let reference = reference(&value);
    if enrollment.held() && native.is_none() {
        let retained = turn_sources::retain_unidentified_input(
            &root,
            "codex",
            session,
            turn,
            text.as_bytes(),
            reference,
            &runtime_root,
        )?;
        progress.retained = Some(required_text(&retained, "retention_id")?.to_owned());
        progress.at("identity", "identity.unresolved");
        return Err(err(
            "Retained source lacks an unambiguous native event identity",
        ));
    }
    let observation = turn_sources::register(
        &root,
        "codex",
        session,
        turn,
        &sha256_hex(text.as_bytes()),
        native,
        reference,
        None,
        &runtime_root,
    )?;
    progress.at("guidance", "guidance.unavailable");
    let mut guidance = format!(
        "{} Observation {}; session={session}; prompt={turn}; source_sha256={}{ASSESS}",
        boundary_prompt(),
        required_text(&observation, "observation_id")?,
        required_text(&observation, "source_sha256")?,
    );
    if enrollment.held() {
        guidance.push_str(HELD);
    }
    // Optional notice bookkeeping cannot veto durable source registration.
    if notice(&root, "codex", session, turn, &runtime_root).unwrap_or(true) {
        guidance.push_str(EARLIER);
    }
    if truthy(get(&observation, "control_origin_hint")) {
        guidance.push_str(HINT);
    }
    if guidance.len() > MAX_GUIDANCE {
        return Err(err("guidance exceeds bound"));
    }
    progress.at("boundary", "state.unavailable");
    scope_binding::recheck_private_enrollment(&enrollment)?;
    diagnostic(Some(&root), "prompt", None);
    Ok(prompt_context(&guidance))
}

pub(super) fn main(raw: &[u8], runtime: Runtime<'_>) -> Output {
    let mut progress = Progress {
        stage: "event",
        reason: "event.invalid",
        repo: None,
        scoped: false,
        retained: None,
    };
    match accept(raw, runtime, &mut progress) {
        Ok(text) => Output::stdout(0, text),
        Err(error) => {
            if progress.scoped && matches!(error.kind(), Kind::IdentityUnresolved) {
                progress.at("identity", "identity.unresolved");
            }
            diagnostic(
                progress.repo.as_deref(),
                "prompt",
                Some((progress.stage, progress.reason)),
            );
            let retained = progress.retained.map_or_else(String::new, |id| {
                format!(
                    " Unidentified input retained privately as {id}; it is not an observation. Restore the original native event ID/reference before enrollment."
                )
            });
            Output::stdout(
                0,
                super::block(&format!(
                    "Agent Memory input not accepted [{}/{}]. Preserve work; run am doctor and am codex-boundary --list-pending for this repository, then retry this input after resolving the fault.{retained}",
                    progress.stage, progress.reason
                )),
            )
        }
    }
}
