//! Baseline `hooks/turn_hook.py`: compatibility prompt entrypoint; native
//! Claude inputs enroll before acceptance. Invocation guidance only, never a
//! memory operation or an admission receipt.
use super::{MAX_EVENT, Output, Runtime, checked_boundary_prompt, event, prompt_context};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};

const FAILED: &str =
    "Agent Memory turn hook failed; automatic memory refresh and capture are pending.";

enum Decision {
    Guidance(String),
    Claude(Value),
}

fn decide(raw: &[u8]) -> Result<Decision> {
    if raw.len() > MAX_EVENT {
        return Err(ContractError::new("oversized event"));
    }
    let value = event(raw)?;
    if value.as_object().is_none()
        || value.get("hook_event_name").and_then(Value::as_str) != Some("UserPromptSubmit")
    {
        return Err(ContractError::new("unsupported event"));
    }
    if ["prompt_id", "prompt", "transcript_path"]
        .iter()
        .all(|key| value.get(key).is_some())
    {
        return Ok(Decision::Claude(value));
    }
    Ok(Decision::Guidance(prompt_context(
        checked_boundary_prompt()?
    )))
}

pub(super) fn main(raw: &[u8], runtime: Runtime<'_>) -> Output {
    match decide(raw) {
        Ok(Decision::Claude(value)) => super::claude_prompt::main(Some(value), raw, runtime),
        Ok(Decision::Guidance(text)) => Output::stdout(0, text),
        // No hook error may block the user's turn. Be explicit that memory work
        // remains unverified instead of treating a broken hook as success.
        Err(_) => Output::stdout(0, super::system_message(FAILED)),
    }
}
