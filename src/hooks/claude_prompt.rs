//! Baseline `hooks/claude_prompt.py`: retain native Claude input before
//! transcript publication or interruption.
use super::{
    MAX_EVENT, Output, Runtime, bank_exists, boundary_prompt, checkout, event, notice,
    prompt_context, required_text, sha256_hex, working_directory,
};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::fs;
use crate::{scope_binding, turn_sources};
use std::path::Path;

const BLOCKED: &str = "Agent Memory could not retain this input safely. Preserve work; run am doctor and inspect pending source capacity/state before retrying.";
const ASSESS: &str = ". Before final submit top-level session,prompt,observation_id,source_sha256,outcome,assessment via one am claude-boundary --repo REPO --input - --refresh --quiet for no-learning; assessment requires source_id=observation_id, source_review=actual review, candidates=authored list (empty only if none). Attribute the actual source. On routine success, output only the task response; do not announce guides, assessments, no-learning or memory status.";
const HELD: &str = " Recovery-pending: input retained privately; continue the task. No refresh, acknowledgement or canonical writes; never fabricate no-learning or publication.";
const PENDING: &str =
    " Pending work: use am claude-boundary --list-pending before assessing the original source.";
/// Bound of the injected guidance.
const MAX_GUIDANCE: usize = 2048;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

/// The printed result: `{}` when no bank applies, else the guidance line.
fn enroll(given: Option<Value>, raw: &[u8], runtime: Runtime<'_>) -> Result<String> {
    let value = match given {
        Some(value) => value,
        None => {
            if raw.len() > MAX_EVENT {
                return Err(err("event bound"));
            }
            event(raw)?
        }
    };
    if value.get("hook_event_name").and_then(Value::as_str) != Some("UserPromptSubmit") {
        return Err(err("event type"));
    }
    let cwd = working_directory(&value)?;
    let Some(repo) = checkout(&cwd)? else {
        return Ok(String::from("{}\n"));
    };
    if !bank_exists(&repo)? {
        return Ok(String::from("{}\n"));
    }
    let text = value
        .get("prompt")
        .ok_or_else(|| err("native source unavailable"))?;
    let session = required_text(&value, "session_id")?;
    let prompt = required_text(&value, "prompt_id")?;
    let reference = value
        .get("transcript_path")
        .ok_or_else(|| err("native source unavailable"))?;
    let (Some(text), Some(reference)) = (text.as_str(), reference.as_str()) else {
        return Err(err("native source unavailable"));
    };
    if text.is_empty() || text.len() > 65536 || !Path::new(reference).is_absolute() {
        return Err(err("native source unavailable"));
    }
    let runtime_root = runtime()?;
    // The transcript may not exist yet. Store its native reference without
    // claiming a transcript UUID or requiring a premature transcript read.
    let enrollment = scope_binding::private_enrollment_context(&repo, &runtime_root)?;
    let reference = fs::canonical_path(Path::new(reference), false)?;
    let reference = reference
        .to_str()
        .ok_or_else(|| ContractError::non_utf8("transcript reference"))?;
    let observation = turn_sources::register(
        &repo,
        "claude",
        session,
        prompt,
        &sha256_hex(text.as_bytes()),
        Some(prompt),
        Some(reference),
        None,
        &runtime_root,
    )?;
    let observation_id = required_text(&observation, "observation_id")?;
    let source_sha256 = required_text(&observation, "source_sha256")?;
    let mut context = format!(
        "{}\nClaude source: session={session} prompt={prompt} observation_id={observation_id} source_sha256={source_sha256}{ASSESS}",
        boundary_prompt()
    );
    if enrollment.held() {
        context.push_str(HELD);
    }
    if notice(&repo, "claude", session, prompt, &runtime_root)? {
        context.push_str(PENDING);
    }
    if context.len() > MAX_GUIDANCE {
        return Err(err("guidance bound"));
    }
    scope_binding::recheck_private_enrollment(&enrollment)?;
    Ok(prompt_context(&context))
}

/// `main(event=None)`; `turn_hook` passes its already decoded event.
pub(super) fn main(given: Option<Value>, raw: &[u8], runtime: Runtime<'_>) -> Output {
    match enroll(given, raw, runtime) {
        Ok(text) => Output::stdout(0, text),
        // Failure to enroll input must not masquerade as successful tracking.
        Err(_) => Output::stdout(0, super::block(BLOCKED)),
    }
}
