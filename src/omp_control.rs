//! Conservative stored-batch classification; this never authorizes tool execution.
//!
//! Baseline `omp_control.py`.
use crate::codec::json::Value;
use crate::lifecycle::{get, nonempty};
use crate::omp_projection;
use std::collections::BTreeSet;

/// One stored entry with its projected snapshot.
pub type Projected = Vec<(Value, Value)>;

/// Baseline `shlex.split(command)` (POSIX mode, no comments, whitespace
/// split). `None` is the baseline `ValueError` (unclosed quote or a trailing
/// escape).
pub fn shell_split(command: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Space,
        Word,
        Single,
        Double,
        Escape,
    }
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut state = State::Space;
    let mut escaped_from = State::Word;
    for character in command.chars() {
        match state {
            State::Space | State::Word => match character {
                ' ' | '\t' | '\r' | '\n' => {
                    if state == State::Word || !token.is_empty() || quoted {
                        if !token.is_empty() || quoted {
                            tokens.push(std::mem::take(&mut token));
                        }
                        quoted = false;
                    }
                    state = State::Space;
                }
                '\\' => {
                    escaped_from = State::Word;
                    state = State::Escape;
                }
                '\'' => state = State::Single,
                '"' => state = State::Double,
                _ => {
                    token.push(character);
                    state = State::Word;
                }
            },
            State::Single => {
                quoted = true;
                if character == '\'' {
                    state = State::Word;
                } else {
                    token.push(character);
                }
            }
            State::Double => {
                quoted = true;
                match character {
                    '"' => state = State::Word,
                    '\\' => {
                        escaped_from = State::Double;
                        state = State::Escape;
                    }
                    _ => token.push(character),
                }
            }
            State::Escape => {
                // In POSIX quotes only the quote itself or the escape may be escaped.
                if escaped_from == State::Double && character != '"' && character != '\\' {
                    token.push('\\');
                }
                token.push(character);
                state = escaped_from;
            }
        }
    }
    match state {
        State::Single | State::Double | State::Escape => None,
        State::Space | State::Word => {
            if !token.is_empty() || quoted {
                tokens.push(token);
            }
            Some(tokens)
        }
    }
}

/// Only `am|agent-memory omp-boundary …` without shell metacharacters.
fn boundary_command(block: &Value) -> Option<()> {
    let arguments = get(block, "arguments");
    let command = get(arguments, "command").as_str()?;
    let command = shell_split(command)?;
    let program = command.first()?;
    let operation = command.get(1)?;
    if !matches!(program.as_str(), "am" | "agent-memory")
        || operation != "omp-boundary"
        || command
            .iter()
            .any(|token| token.chars().any(|c| ";&|<>$`".contains(c)))
    {
        return None;
    }
    Some(())
}

/// Baseline `pure_batch`: only one exact reserved hidden message, no
/// host/foreign companions.
///
/// Visible assistant text or any unrecognized invocation reclassifies the whole
/// interval as substantive. This is metadata classification, not permission or
/// a promise that even a valid native tool is actually invoked/completed.
pub fn pure_batch(
    entries: &[Value],
    guidance: Option<&str>,
    continuing: bool,
    tool_ids: &[String],
) -> Option<(Projected, Vec<String>)> {
    let guidance = guidance.filter(|text| !text.is_empty())?;
    let mut own = 0;
    let mut tools: BTreeSet<String> = tool_ids.iter().cloned().collect();
    let mut projected = Vec::new();
    for entry in entries {
        if !matches!(
            get(entry, "type").as_str(),
            Some("message" | "custom_message")
        ) {
            return None;
        }
        let result = omp_projection::project(entry);
        let snapshot = result.get("snapshot")?.clone();
        let role = get(&snapshot, "role").as_str();
        if get(&snapshot, "carrier").as_str() == Some("custom_message")
            || matches!(role, Some("custom" | "hookMessage"))
        {
            if get(&snapshot, "attribution").as_str() != Some("agent")
                || get(&snapshot, "content").as_str() != Some(guidance)
                || own > 0
                || continuing
            {
                return None;
            }
            own += 1;
        } else if role == Some("assistant") {
            let content = match get(&snapshot, "content") {
                Value::String(text) if text.is_empty() => <&[Value]>::default(),
                Value::Array(items) => items.as_slice(),
                _ => return None,
            };
            for block in content {
                let kind = get(block, "type").as_str();
                if kind == Some("text") && !get(block, "text").as_str().is_some_and(nonempty) {
                    continue;
                }
                let name = get(block, "name").as_str();
                if kind != Some("toolCall") || !matches!(name, Some("bash" | "functions.bash")) {
                    return None;
                }
                boundary_command(block)?;
                tools.insert(get(block, "id").as_str()?.to_owned());
            }
        } else if role == Some("toolResult")
            && get(&snapshot, "toolCallId")
                .as_str()
                .is_some_and(|id| tools.contains(id))
        {
            if let Some(id) = get(&snapshot, "toolCallId").as_str() {
                tools.remove(id);
            }
        } else {
            return None;
        }
        projected.push((entry.clone(), snapshot));
    }
    if own != 1 && !continuing {
        return None;
    }
    Some((projected, tools.into_iter().collect()))
}
