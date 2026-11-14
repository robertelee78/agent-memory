//! Native client hook commands: `am hook <name>`.
//!
//! Baseline: the portable pack's `hooks/*.py` entry scripts, which Codex,
//! Claude and OpenCode started through the pack's interpreter launcher. Each hook
//! here keeps its baseline stdin bound, stdout/stderr bytes and exit status;
//! no hook output is ever a capture, admission or publication receipt.
//!
//! `turn-hook` (`turn_hook.py`), `claude-prompt` (`claude_prompt.py`),
//! `claude-stop` (`claude_stop.py` with `claude_source.py`), `codex-prompt`
//! (`codex_prompt.py`), `codex-stop` (`codex_stop.py`) and `boundary-ledger`
//! (`boundary_ledger.py`, the OpenCode adapter's private ledger helper).

pub mod boundary_ledger;
mod claude_prompt;
pub mod claude_source;
mod claude_stop;
mod codex_prompt;
mod codex_stop;
mod turn_hook;

use crate::clock::Deadline;
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::process::{self, Input, Spec, Status, Target};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Hook names in `am hook <name>`, in baseline file order.
pub const NAMES: [&str; 6] = [
    "turn-hook",
    "claude-prompt",
    "claude-stop",
    "codex-prompt",
    "codex-stop",
    "boundary-ledger",
];
/// Baseline `MAX_INPUT`: every harness event is at most 64 KiB.
pub const MAX_EVENT: usize = 64 * 1024;
/// The ledger request had no baseline bound (`json.load(sys.stdin)`); it
/// shares the CLI's explicit input bound.
pub const MAX_LEDGER_REQUEST: usize = crate::cli::input::INPUT_MAX_BYTES;
/// Baseline `git rev-parse` calls use `timeout=5`.
const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const GIT_OUTPUT_MAX: usize = 65536;
/// The pack's shared boundary guidance (`hooks/boundary-prompt.txt`), the
/// same bytes the OpenCode adapters read at runtime.
const BOUNDARY_PROMPT: &str = include_str!("../skills/agent-memory/hooks/boundary-prompt.txt");

/// One hook invocation's exact process result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    fn stdout(exit_code: i32, text: String) -> Self {
        Self {
            exit_code,
            stdout: text,
            stderr: String::new(),
        }
    }
    fn quiet() -> Self {
        Self::stdout(0, String::new())
    }
}

/// The executing runtime's pack root (baseline `Path(__file__).parent.parent`),
/// resolved only when a hook reaches private source state.
pub type Runtime<'a> = &'a dyn Fn() -> Result<PathBuf>;

/// The stdin byte count a hook reads: its bound plus one byte proving excess.
pub fn input_limit(name: &str) -> Option<usize> {
    match name {
        "boundary-ledger" => Some(MAX_LEDGER_REQUEST + 1),
        name if NAMES.contains(&name) => Some(MAX_EVENT + 1),
        _ => None,
    }
}

/// Run hook `name` on its raw stdin bytes.
pub fn run(name: &str, input: &[u8], runtime: Runtime<'_>) -> Option<Output> {
    Some(match name {
        "turn-hook" => turn_hook::main(input, runtime),
        "claude-prompt" => claude_prompt::main(None, input, runtime),
        "claude-stop" => claude_stop::main(input, runtime),
        "codex-prompt" => codex_prompt::main(input, runtime),
        "codex-stop" => codex_stop::main(input, runtime),
        "boundary-ledger" => boundary_ledger::main(input),
        _ => return None,
    })
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

/// `print(json.dumps(value))`.
fn line(value: &Value) -> String {
    match json::encode(value, json::DEFAULT) {
        Ok(text) => format!("{text}\n"),
        // Every hook value is built from strings, booleans and decoded JSON.
        Err(_) => String::from("{}\n"),
    }
}

fn object(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}

/// `json.loads(raw)` of a harness event: plain decoding, last duplicate wins.
fn event(raw: &[u8]) -> Result<Value> {
    json::decode_bytes(raw, DecodePolicy::default())
}

/// `{'hookSpecificOutput': {'hookEventName': 'UserPromptSubmit', 'additionalContext': text}}`.
fn prompt_context(text: &str) -> String {
    line(&object([(
        "hookSpecificOutput",
        object([
            ("hookEventName", string("UserPromptSubmit")),
            ("additionalContext", string(text)),
        ]),
    )]))
}

fn system_message(text: &str) -> String {
    line(&object([("systemMessage", string(text))]))
}

fn block(reason: &str) -> String {
    line(&object([
        ("decision", string("block")),
        ("reason", string(reason)),
    ]))
}

/// Python `str.strip()`: `str.isspace` includes the 0x1c–0x1f separators.
fn strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// `(pack/'hooks/boundary-prompt.txt').read_text().strip()`.
fn boundary_prompt() -> &'static str {
    strip(BOUNDARY_PROMPT)
}

/// The stripped guidance, refused when empty or over 4,096 bytes.
fn checked_boundary_prompt() -> Result<&'static str> {
    let prompt = boundary_prompt();
    if prompt.is_empty() || prompt.len() > 4096 {
        return Err(err("invalid instruction"));
    }
    Ok(prompt)
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::validation_memo::hex(&Sha256::digest(bytes))
}

/// Python `len(text)`: code points.
fn chars(text: &str) -> usize {
    text.chars().count()
}

fn hex_digit(byte: u8, upper: bool) -> bool {
    byte.is_ascii_digit()
        || (b'a'..=b'f').contains(&byte)
        || (upper && (b'A'..=b'F').contains(&byte))
}

/// `[0-9a-f]{64}` (full match).
fn digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| hex_digit(b, false))
}

/// `[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}` (full match).
fn uuid_text(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                hex_digit(byte, true)
            }
        })
}

/// Python truthiness of a decoded JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(pairs) => !pairs.is_empty(),
        Value::Int(number) => number.as_str() != "0",
        Value::Float(number) => *number != 0.0,
        _ => true,
    }
}

fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}

/// `event[key]` as text: a missing key or another type is a refusal.
fn required_text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| err("Hook event field is missing or not text"))
}

/// `os.path.lexists`: any observation failure means absent.
fn lexists(path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return path == Path::new("/");
    };
    let Ok(parent) = fs::canonical_path(parent, false) else {
        return false;
    };
    let Ok(chain) = DirectoryChain::open(&parent) else {
        return false;
    };
    chain.directory().stat_at(name).is_ok()
}

/// `Path(event['cwd']).resolve(strict=True)`.
fn working_directory(event: &Value) -> Result<PathBuf> {
    fs::canonical_path(Path::new(required_text(event, "cwd")?), true)
}

/// `subprocess.run(['git', *args], capture_output=True, text=True, timeout=5)`:
/// the exit status and the decoded, stripped stdout.
fn git(args: &[&std::ffi::OsStr]) -> Result<(bool, String)> {
    let mut argv = vec![OsString::from("git")];
    argv.extend(args.iter().map(|arg| arg.to_os_string()));
    let env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let spec = Spec {
        argv: &argv,
        search_path: true,
        env: &env,
        cwd: None,
        stdin: Input::Null,
        stdout: Target::Capture {
            limit: GIT_OUTPUT_MAX,
        },
        stderr: Target::Capture {
            limit: GIT_OUTPUT_MAX,
        },
        deadline: Deadline::after(GIT_TIMEOUT)?,
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    let output = process::run(&spec).map_err(|error| ContractError::new(error.to_string()))?;
    let stdout =
        String::from_utf8(output.stdout).map_err(|_| ContractError::non_utf8("Git output"))?;
    Ok((output.status == Status::Code(0), strip(&stdout).to_owned()))
}

/// The baseline checkout probe: `git -C cwd rev-parse --show-toplevel`.
/// `None` is "no repository"; a failed probe inside an apparent repository
/// (any `.git` entry at or above `cwd`) is a refusal.
fn checkout(cwd: &Path) -> Result<Option<PathBuf>> {
    let (found, stdout) = git(&[
        "-C".as_ref(),
        cwd.as_os_str(),
        "rev-parse".as_ref(),
        "--show-toplevel".as_ref(),
    ])?;
    if !found {
        if cwd.ancestors().any(|parent| lexists(&parent.join(".git"))) {
            return Err(err("repository unavailable"));
        }
        return Ok(None);
    }
    Ok(Some(fs::canonical_path(Path::new(&stdout), true)?))
}

/// `os.path.lexists(repository_config.bank_path(repo))`.
fn bank_exists(repo: &Path) -> Result<bool> {
    Ok(lexists(&crate::repository_config::bank_path(repo)?))
}

/// `hashlib.sha256((a + ':' + b).encode()).hexdigest()`.
fn pair_key(first: &str, second: &str) -> String {
    sha256_hex(format!("{first}:{second}").as_bytes())
}

/// Exact `set(value) == fields` for a decoded object.
fn exact_fields(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len() && fields.iter().all(|field| value.get(field).is_some())
    })
}

/// The receipt a legacy boundary completion implies, for
/// `turn_sources.validate_receipt`.
fn legacy_receipt(completed: &Value, observation_id: &str, source_sha256: &Value) -> Value {
    let mut pairs: Vec<(String, Value)> = [
        "outcome",
        "assessment",
        "assessment_is_agent_assertion",
        "source_publication_link_verified",
        "witnessed_commit",
        "input_sha256",
    ]
    .iter()
    .map(|key| ((*key).to_owned(), get(completed, key).clone()))
    .collect();
    pairs.push(("observation_id".into(), string(observation_id)));
    pairs.push(("source_sha256".into(), source_sha256.clone()));
    pairs.push((
        "publication_verified".into(),
        Value::Bool(get(completed, "outcome").as_str() == Some("published")),
    ));
    pairs.push(("canonical_writes_performed".into(), Value::Bool(false)));
    Value::Object(pairs)
}

/// `bool(turn_sources.pending_notice(...))`.
fn notice(
    repo: &Path,
    harness: &str,
    session: &str,
    lifecycle: &str,
    runtime_root: &Path,
) -> Result<bool> {
    Ok(
        crate::turn_sources::pending_notice(repo, harness, session, lifecycle, runtime_root)?
            .is_some_and(|value| truthy(&value)),
    )
}

/// A buffered `read(maximum)`: up to `maximum` bytes or end of file.
fn read_up_to(file: &mut fs::Descriptor, maximum: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = vec![0u8; maximum];
    let mut filled = 0;
    while let Some(rest) = bytes.get_mut(filled..).filter(|rest| !rest.is_empty()) {
        let count = file.read_chunk(rest)?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    bytes.truncate(filled);
    Ok(bytes)
}

/// `Path.exists()`: the path resolves through any symlinks.
fn exists(path: &Path) -> bool {
    fs::canonical_path(path, true).is_ok()
}
