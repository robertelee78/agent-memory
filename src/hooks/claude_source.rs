//! Baseline `hooks/claude_source.py`: private source-bound Claude Stop
//! identity; no memory admission is implied.
use super::{get, sha256_hex, string, uuid_text};
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use std::path::Path;

/// Baseline `LIMIT`: the transcript read bound.
pub const LIMIT: usize = 16 * 1024 * 1024;
const MAX_ID: usize = 512;
const MAX_ASSISTANT: usize = 65536;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

fn io(error: std::io::Error) -> ContractError {
    ContractError::new(error.to_string())
}

/// A bounded native identifier without control characters.
fn identifier(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= MAX_ID && !v.chars().any(|c| (c as u32) < 32))
}

/// The transcript bytes, read once through a no-follow descriptor that must
/// stay bound to the named path for the whole observation.
fn transcript_bytes(transcript: &str) -> Result<Vec<u8>> {
    let path = Path::new(transcript);
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(err("missing transcript"));
    };
    let parent = fs::canonical_path(parent, true)?;
    let chain = DirectoryChain::open(&parent).map_err(io)?;
    let mut file = chain
        .directory()
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io)?;
    let info = file.stat().map_err(io)?;
    if !info.is_regular()
        || info.uid != crate::host::effective_uid()
        || info.size < 0
        || info.size as u128 > LIMIT as u128
    {
        return Err(err("unsafe transcript"));
    }
    let mut raw = Vec::new();
    let mut block = vec![0u8; 65536];
    while raw.len() <= LIMIT {
        let want = block.len().min(LIMIT + 1 - raw.len());
        let Some(buffer) = block.get_mut(..want) else {
            return Err(err("oversized transcript"));
        };
        let count = file.read_chunk(buffer).map_err(io)?;
        if count == 0 {
            break;
        }
        raw.extend_from_slice(buffer.get(..count).unwrap_or_default());
    }
    if raw.len() > LIMIT {
        return Err(err("oversized transcript"));
    }
    let after = file.stat().map_err(io)?;
    let named = chain.directory().stat_at(name).map_err(io)?;
    if (after.dev, after.ino, after.size, after.mtime_ns)
        != (info.dev, info.ino, info.size, info.mtime_ns)
        || !named.same_inode(info)
    {
        return Err(err("transcript changed during observation"));
    }
    chain.recheck().map_err(io)?;
    Ok(raw)
}

/// Python `bytes.splitlines()`: `\n`, `\r\n` and `\r` end a line.
fn lines(raw: &[u8]) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while let Some(&byte) = raw.get(index) {
        if byte == b'\n' || byte == b'\r' {
            result.push(raw.get(start..index).unwrap_or_default());
            if byte == b'\r' && raw.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            start = index + 1;
        }
        index += 1;
    }
    if start < raw.len() {
        result.push(raw.get(start..).unwrap_or_default());
    }
    result
}

/// Baseline `source(event, *, require_final=True)`: the one exact native user
/// message for the event's session and prompt in its transcript.
pub fn source(event: &Value, require_final: bool) -> Result<Value> {
    let (Some(session), Some(prompt)) = (
        identifier(get(event, "session_id")),
        identifier(get(event, "prompt_id")),
    ) else {
        return Err(err("missing prompt identity"));
    };
    if require_final && (!uuid_text(session) || !uuid_text(prompt)) {
        return Err(err("unsupported legacy prompt identity"));
    }
    let transcript = get(event, "transcript_path")
        .as_str()
        .filter(|path| Path::new(path).is_absolute())
        .ok_or_else(|| err("missing transcript"))?;
    let raw = transcript_bytes(transcript)?;
    let mut found = Vec::new();
    for line in lines(&raw) {
        let row = json::decode_bytes(line, DecodePolicy::default())?;
        if row.as_object().is_some()
            && get(&row, "type").as_str() == Some("user")
            && get(&row, "sessionId").as_str() == Some(session)
            && get(&row, "promptId").as_str() == Some(prompt)
            && get(&row, "isMeta") != &Value::Bool(true)
            && row.get("message").is_some_and(|message| {
                message.as_object().is_some()
                    && message.get("content").and_then(Value::as_str).is_some()
            })
        {
            found.push(row);
        }
    }
    let [row] = found.as_slice() else {
        return Err(err("source user message is missing or ambiguous"));
    };
    let Some(uuid) = get(row, "uuid").as_str().filter(|v| uuid_text(v)) else {
        return Err(err("source user message is missing or ambiguous"));
    };
    let last = get(event, "last_assistant_message");
    if require_final && last.as_str().is_none_or(str::is_empty) {
        return Err(err("final assistant message is unavailable"));
    }
    if last != &Value::Null && last.as_str().is_none_or(|v| v.len() > MAX_ASSISTANT) {
        return Err(err("invalid assistant message"));
    }
    let content = get(get(row, "message"), "content")
        .as_str()
        .unwrap_or_default();
    let assistant = match last.as_str() {
        Some(text) if !text.is_empty() => string(sha256_hex(text.as_bytes())),
        _ => Value::Null,
    };
    Ok(super::object([
        ("session", string(session)),
        ("prompt", string(prompt)),
        ("source_uuid", string(uuid)),
        ("source_sha256", string(sha256_hex(content.as_bytes()))),
        ("assistant_sha256", assistant),
        ("transcript", string(transcript)),
    ]))
}
