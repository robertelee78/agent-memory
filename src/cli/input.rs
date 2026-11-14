//! Caller-supplied `--input`/`--file`/`--proposal` reads, bounded and with
//! the baseline's `OSError` text (`[Errno N] strerror: 'path'`).
//!
//! These are request documents named by the caller, not trust-relevant
//! state: like the baseline `Path(p).read_bytes()` they follow symlinks.

use crate::codec::json::{self, DecodePolicy, DuplicateKeys, Value};
use crate::console;
use crate::error::{ContractError, Result};
use crate::process;
use std::io::{self, Read};

/// Upper bound for one CLI request document or Markdown file.
pub const INPUT_MAX_BYTES: usize = json::DEFAULT_MAX_BYTES;

/// Python's `str(OSError)` for an error naming `path`.
pub fn os_error(error: &io::Error, path: &str) -> ContractError {
    match error.raw_os_error() {
        Some(errno) => {
            let full = io::Error::from_raw_os_error(errno).to_string();
            let suffix = format!(" (os error {errno})");
            let message = full.strip_suffix(&suffix).unwrap_or(&full);
            ContractError::new(format!(
                "[Errno {errno}] {message}: {}",
                console::repr(path)
            ))
        }
        None => ContractError::new(error.to_string()),
    }
}

fn bounded(mut reader: impl Read, path: &str) -> Result<Vec<u8>> {
    const CHUNK: usize = 64 * 1024;
    let mut bytes = Vec::new();
    // One byte past the bound proves an oversized input.
    while bytes.len() <= INPUT_MAX_BYTES {
        let filled = bytes.len();
        let want = CHUNK.min(INPUT_MAX_BYTES + 1 - filled);
        bytes.resize(filled + want, 0);
        // `filled < bytes.len()` after the resize above.
        let read = reader.read(&mut bytes[filled..]);
        bytes.truncate(filled + *read.as_ref().unwrap_or(&0));
        match read {
            Ok(0) => break,
            Ok(_) => {}
            // SIGINT ends a blocked read (stdin from a terminal) as the
            // baseline's KeyboardInterrupt does; other EINTRs are retried.
            // (`read_to_end` would retry every EINTR and never return.)
            Err(error) if error.kind() == io::ErrorKind::Interrupted && !process::interrupted() => {
            }
            Err(error) => return Err(os_error(&error, path)),
        }
    }
    if bytes.len() > INPUT_MAX_BYTES {
        return Err(ContractError::new(format!(
            "Input exceeds its {INPUT_MAX_BYTES}-byte read bound: {}",
            console::repr(path)
        )));
    }
    Ok(bytes)
}

/// `Path(path).read_bytes()`.
pub fn read_bytes(path: &str) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|error| os_error(&error, path))?;
    bounded(file, path)
}

/// `Path(path).read_bytes().decode('utf-8')`: the bytes unchanged.
pub fn read_text(path: &str) -> Result<String> {
    String::from_utf8(read_bytes(path)?).map_err(|_| ContractError::non_utf8("input text"))
}

/// `Path(path).read_text(encoding='utf-8')`: text mode with `newline=None`,
/// so `\r\n` and then each lone `\r` become `\n`.
pub fn read_text_universal(path: &str) -> Result<String> {
    let text = read_text(path)?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `sys.stdin.read()`.
pub fn read_stdin() -> Result<Vec<u8>> {
    bounded(io::stdin().lock(), "<stdin>")
}

/// `sys.stdin.read() if path == '-' else Path(path).read_text()` then
/// `json.loads(text, object_pairs_hook=unique_object)`.
pub fn load_text_or_stdin(path: &str) -> Result<Value> {
    let bytes = if path == "-" {
        read_stdin()?
    } else {
        read_bytes(path)?
    };
    json::decode_bytes(
        &bytes,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            max_bytes: INPUT_MAX_BYTES,
            ..DecodePolicy::default()
        },
    )
}

/// `developer_identity.load(Path(path).read_bytes())`.
pub fn load(path: &str) -> Result<Value> {
    crate::developer_identity::load(&read_bytes(path)?)
}
