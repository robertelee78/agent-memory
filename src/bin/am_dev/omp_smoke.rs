//! Installed native OMP smoke; controlled protocol tests are not provider qualification.
use agent_memory::{
    clock::{Deadline, monotonic_ns},
    codec::json::{self, Value},
    fs::{DirectoryChain, ReadOptions, canonical_path, read_source_with_options},
    host,
    process::{self, Input, Spec, Status, Target},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    io::{Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    path::{Path, PathBuf},
    time::Duration,
};

const JSON_LIMIT: usize = 16 * 1024 * 1024;
const BINARY_LIMIT: usize = 512 * 1024 * 1024;
const FRAME_LIMIT: usize = 1024 * 1024;
const FRAME_QUEUE_LIMIT: usize = 4096;
const DIRECTORY_LIMIT: usize = 16_384;
const SUPPORTED_VERSIONS: [&str; 2] = ["18.16.0", "18.19.0"];
pub const HELP: &str = "usage: am-dev omp-smoke [-h] --binary BINARY --am AM --receipt RECEIPT\n                        --repo REPO --output OUTPUT --model MODEL\n                        [--approval-mode APPROVAL_MODE] [--timeout TIMEOUT]\n\nReal installed omp multi-turn smoke. This is not the full #249 release gate.\n\nRequires an explicitly prepared disposable, initialized/bound repository, actual\nprovider access, an installed Rust receipt-v3 binding, and normal native permissions.\nNever installs a fake tool, auto-approves a request, or fabricates a source receipt.\n\noptions:\n  -h, --help            show this help message and exit\n  --binary BINARY\n  --am AM\n  --receipt RECEIPT\n  --repo REPO\n  --output OUTPUT\n  --model MODEL         Actual operator-selected available provider/model\n  --approval-mode APPROVAL_MODE\n                        Optional explicit normal native mode; never auto-approved by this driver\n  --timeout TIMEOUT\n";

#[derive(Debug)]
pub enum Error {
    Pending(String),
    Failure(String),
    Timeout(String),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, message) = match self {
            Self::Pending(s) => ("PendingObservations", s),
            Self::Failure(s) => ("RuntimeError", s),
            Self::Timeout(s) => ("TimeoutError", s),
        };
        write!(f, "{kind}: {message}")
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        fail(value.to_string())
    }
}
impl From<agent_memory::error::ContractError> for Error {
    fn from(value: agent_memory::error::ContractError) -> Self {
        fail(value.to_string())
    }
}
fn fail(value: impl Into<String>) -> Error {
    Error::Failure(value.into())
}
fn pending(value: &str) -> Error {
    Error::Pending(value.into())
}
pub fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
pub fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(values.into_iter().collect())
}
pub fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| fail(format!("Missing or malformed {key}")))
}
fn rows(value: &Value) -> Result<&[Value]> {
    value.as_array().ok_or_else(|| fail("Expected JSON array"))
}
pub fn set(value: &mut Value, key: &str, replacement: Value) {
    if let Value::Object(pairs) = value {
        if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == key) {
            *old = replacement;
        } else {
            pairs.push((key.into(), replacement));
        }
    }
}
fn append(value: &mut Value, key: &str, item: Value) {
    let mut items = get(value, key).as_array().unwrap_or(&[]).to_vec();
    items.push(item);
    set(value, key, Value::Array(items));
}
fn count(value: &mut Value, key: &str, amount: usize) {
    let old = get(value, key).as_i64().unwrap_or(0);
    set(
        value,
        key,
        Value::Int(old.saturating_add(amount as i64).into()),
    );
}
trait Numeric {
    fn as_i64(&self) -> Option<i64>;
}
impl Numeric for Value {
    fn as_i64(&self) -> Option<i64> {
        self.as_int().and_then(json::Integer::as_i64)
    }
}
fn encode(value: &Value, style: json::Style) -> Result<String> {
    Ok(json::encode(value, style)?)
}
fn decode(bytes: &[u8]) -> Result<Value> {
    Ok(json::decode_bytes(
        bytes,
        json::DecodePolicy {
            max_bytes: JSON_LIMIT,
            ..Default::default()
        },
    )?)
}
pub fn sha(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        for nibble in [byte >> 4, byte & 15] {
            output.push(char::from(if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + nibble - 10
            }));
        }
    }
    output
}
pub fn source_digest(value: &Value) -> Result<String> {
    Ok(sha(encode(value, json::SORTED_COMPACT_UTF8)?.as_bytes()))
}
fn same(left: &Value, right: &Value) -> Result<bool> {
    Ok(encode(left, json::SORTED_COMPACT_UTF8)? == encode(right, json::SORTED_COMPACT_UTF8)?)
}
fn canonical(path: &Path) -> Result<PathBuf> {
    Ok(canonical_path(path, true)?)
}
fn path_text(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| fail("Non-UTF-8 path"))
}
fn read_bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .ok_or_else(|| fail("Evidence file has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| fail("Evidence file has no name"))?;
    read_source_with_options(parent, Path::new(name), limit, ReadOptions::default())
        .map(|observation| observation.bytes)
        .map_err(|error| fail(format!("{}: {error}", path.display())))
}
pub fn read_json(path: &Path) -> Result<Value> {
    decode(&read_bytes(path, JSON_LIMIT)?)
}
fn json_lines(path: &Path) -> Result<Vec<Value>> {
    let bytes = read_bytes(path, JSON_LIMIT)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| fail("Native session is not UTF-8"))?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| decode(line.as_bytes()))
        .collect()
}
fn optional_entries(path: &Path) -> Result<Option<Vec<OsString>>> {
    let chain = match DirectoryChain::open(path) {
        Ok(chain) => chain,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let names = chain
        .directory()
        .entries_bounded(DIRECTORY_LIMIT)?
        .ok_or_else(|| fail("Native evidence directory exceeds entry limit"))?;
    chain.recheck()?;
    Ok(Some(names))
}
fn canonical_uuid(value: &str) -> bool {
    agent_memory::omp_transport::canonical_uuid(&s(value)).is_ok()
}
pub fn content_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => blocks
            .iter()
            .map(|block| {
                if get(block, "type").as_str() == Some("text") {
                    get(block, "text").as_str()
                } else {
                    None
                }
            })
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.concat()),
        _ => None,
    }
}

pub fn current_humans(
    repo: &Path,
    root: &Path,
    session: &str,
    session_file: &Path,
    prompts: &[String],
) -> Result<Vec<Value>> {
    if !canonical_uuid(session) || session_file.as_os_str().is_empty() {
        return Err(fail(
            "Current persistent native session identity is unavailable",
        ));
    }
    let path = canonical(session_file)?;
    if !path.starts_with(root.join("sessions")) {
        return Err(fail("Native session file is outside this smoke run"));
    }
    let stored = json_lines(&path)?;
    let headers: Vec<_> = stored
        .iter()
        .enumerate()
        .filter(|(_, e)| get(e, "type").as_str() == Some("session"))
        .collect();
    let [(header_index, header)] = headers.as_slice() else {
        return Err(fail("Native session header differs from current RPC state"));
    };
    if get(header, "id").as_str() != Some(session) {
        return Err(fail("Native session header differs from current RPC state"));
    }
    if stored
        .iter()
        .take(*header_index)
        .any(|e| get(e, "type").as_str() != Some("title"))
    {
        return Err(fail("Unexpected native records precede the session header"));
    }
    if canonical(Path::new(text(header, "cwd")?))? != repo || truthy(get(header, "parentSession")) {
        return Err(fail(
            "Native session is not a fresh session of the selected repository",
        ));
    }
    let humans: Vec<_> = stored
        .iter()
        .skip(header_index + 1)
        .filter(|e| {
            get(e, "type").as_str() == Some("message")
                && get(get(e, "message"), "role").as_str() == Some("user")
                && get(get(e, "message"), "attribution").as_str() == Some("user")
        })
        .cloned()
        .collect();
    let actual: Option<Vec<String>> = humans
        .iter()
        .map(|e| content_text(get(get(e, "message"), "content")))
        .collect();
    if actual.as_deref() != Some(prompts) {
        return Err(fail(
            "Both exact submitted human turns must be retained in the current native session",
        ));
    }
    let mut ids = BTreeSet::new();
    for human in &humans {
        let id = get(human, "id").as_str().filter(|id| !id.is_empty());
        if id.is_none() || !ids.insert(id) {
            return Err(fail("Native human entry IDs are missing or duplicated"));
        }
    }
    Ok(humans)
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Int(_) => value.as_i64() != Some(0),
        _ => true,
    }
}
pub type Environment = BTreeMap<OsString, OsString>;
fn private_base(status: &Value, env: &Environment) -> Result<PathBuf> {
    let state = if let Some(state) = env
        .get(OsStr::new("AGENT_MEMORY_STATE_HOME"))
        .filter(|s| !s.is_empty())
    {
        PathBuf::from(state)
    } else {
        env.get(OsStr::new("XDG_STATE_HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env.get(OsStr::new("HOME"))
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".local/state")
            })
            .join("agent-memory")
    };
    let home = env.get(OsStr::new("HOME")).map(OsString::as_os_str);
    let state = host::expanduser_with_home(&state, home)?;
    Ok(state
        .join("writes")
        .join(sha(text(get(status, "bank"), "path")?.as_bytes())))
}
pub fn bounded_timeout(deadline: Option<Deadline>, maximum: Duration) -> Result<Duration> {
    match deadline {
        Some(deadline) if deadline.expired() => Err(pending("Native readback deadline expired")),
        Some(deadline) => Ok(maximum.min(deadline.remaining())),
        None => Ok(maximum),
    }
}
fn spec<'a>(
    argv: &'a [OsString],
    env: &'a [(OsString, OsString)],
    repo: &'a Path,
    deadline: Deadline,
) -> Spec<'a> {
    Spec {
        argv,
        env,
        cwd: Some(repo),
        search_path: false,
        stdin: Input::Null,
        stdout: Target::Capture { limit: JSON_LIMIT },
        stderr: Target::Capture { limit: JSON_LIMIT },
        deadline,
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    }
}
pub fn run_command(
    argv: &[OsString],
    repo: &Path,
    env: &Environment,
    deadline: Option<Deadline>,
) -> Result<Vec<u8>> {
    let timeout = bounded_timeout(deadline, Duration::from_secs(30))?;
    let env: Vec<_> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let output =
        process::run(&spec(argv, &env, repo, Deadline::after(timeout)?)).map_err(|error| {
            if error.reason == process::Refusal::Deadline {
                Error::Timeout("Command exceeded its native readback deadline".into())
            } else {
                fail(error.to_string())
            }
        })?;
    if output.status == Status::Code(0) {
        return Ok(output.stdout);
    }
    if let Ok(value) = decode(&output.stdout) {
        if get(&value, "verified") == &Value::Bool(false)
            && get(&value, "status").as_str() == Some("pending")
            && get(&value, "canonical_writes_performed") == &Value::Bool(false)
        {
            return Err(pending("Native readback remains pending"));
        }
        if get(&value, "schema").as_str() == Some("agent-memory/diagnostic-result-v1")
            && get(&value, "operation").as_str() == Some("status")
            && get(&value, "failures").as_array().is_some_and(|failures| {
                !failures.is_empty()
                    && failures
                        .iter()
                        .all(|f| get(f, "component").as_str() == Some("omp.native_boundary"))
            })
        {
            return Err(pending("Native diagnostic readback is not stable yet"));
        }
    }
    let code = status_code(output.status);
    let command = argv
        .first()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    Err(fail(format!(
        "Command failed: {command} (exit {code}; retained smoke remains unqualified)"
    )))
}
fn status_code(status: Status) -> i32 {
    match status {
        Status::Code(code) => code,
        Status::Signal(signal) => -signal,
    }
}

pub fn current_observations(
    am: &Path,
    repo: &Path,
    root: &Path,
    session: &str,
    session_file: &Path,
    prompts: &[String],
    status: &Value,
    env: &Environment,
    deadline: Option<Deadline>,
) -> Result<Vec<Value>> {
    let humans = current_humans(repo, root, session, session_file, prompts)?;
    let bank = text(get(status, "bank"), "path")?;
    let base = private_base(status, env)?;
    let binding_path = base.join("omp-bank-binding-v1.json");
    let binding = agent_memory::omp_transport::read_optional(&binding_path, JSON_LIMIT)?
        .ok_or_else(|| pending("Current native observer binding is not published yet"))?;
    if get(&binding, "schema").as_str() != Some("agent-memory/omp-bank-binding-v1")
        || get(&binding, "repository").as_str() != Some(path_text(repo)?)
        || get(&binding, "bank_path").as_str() != Some(bank)
        || get(&binding, "private_base").as_str() != Some(path_text(&base)?)
        || get(&binding, "project_id") != get(get(status, "bank"), "project_id")
    {
        return Err(fail(
            "Retained native sources are not bound to this repository/bank",
        ));
    }
    let directory = base.join("omp-source-v1").join(sha(session.as_bytes()));
    let lifecycles = optional_entries(&directory)?
        .ok_or_else(|| pending("Current native session has no retained observer evidence yet"))?;
    let mut evidence = Vec::new();
    for human in &humans {
        let id = text(human, "id")?;
        let mut matches = Vec::new();
        for lifecycle_dir in &lifecycles {
            let enrollments = directory.join(lifecycle_dir).join("enrollments");
            let Some(names) = optional_entries(&enrollments)? else {
                continue;
            };
            for name in names
                .iter()
                .filter(|name| Path::new(name).extension() == Some(OsStr::new("json")))
            {
                let path = enrollments.join(name);
                let enrollment = read_json(&path)?;
                if get(&enrollment, "kind").as_str() != Some("admission")
                    || get(&enrollment, "native_source_id").as_str() != Some(id)
                {
                    continue;
                }
                let lifecycle = text(&enrollment, "lifecycle")?;
                if !canonical_uuid(lifecycle)
                    || path.parent().and_then(Path::parent)
                        != Some(directory.join(lifecycle).as_path())
                {
                    return Err(fail(
                        "Current human enrollment lifecycle differs from its retained namespace",
                    ));
                }
                let reference = text(&enrollment, "source_reference")?;
                let argv: Vec<OsString> = [
                    am.as_os_str(),
                    OsStr::new("omp-boundary"),
                    OsStr::new("--repo"),
                    repo.as_os_str(),
                    OsStr::new("--source"),
                    OsStr::new(reference),
                    OsStr::new("--session"),
                    OsStr::new(session),
                    OsStr::new("--lifecycle"),
                    OsStr::new(lifecycle),
                ]
                .into_iter()
                .map(OsStr::to_os_string)
                .collect();
                let value = decode(&run_command(&argv, repo, env, deadline)?)?;
                let source = get(&value, "source");
                let digest = source_digest(source)?;
                let origin = get(&value, "origin");
                let captures: Vec<_> = get(&value, "captures")
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|capture| get(capture, "native_entry_id").as_str() == Some(id))
                    .collect();
                let capture = captures.first().copied().unwrap_or(&Value::Null);
                if !truthy(get(&value, "verified"))
                    || truthy(get(&value, "faults"))
                    || !same(get(&value, "enrollment"), &enrollment)?
                    || get(&enrollment, "session").as_str() != Some(session)
                    || get(&enrollment, "source_sha256").as_str() != Some(&digest)
                    || get(source, "carrier").as_str() != Some("message")
                    || get(source, "native_entry_id").as_str() != Some(id)
                    || get(source, "native_parent_id") != get(human, "parentId")
                    || get(source, "role").as_str() != Some("user")
                    || get(source, "attribution").as_str() != Some("user")
                    || content_text(get(source, "content"))
                        != content_text(get(get(human, "message"), "content"))
                    || get(origin, "kind").as_str() != Some("genuine-user")
                    || get(origin, "human_root").as_str() != Some(lifecycle)
                    || !matches!(
                        get(origin, "native_trigger").as_str(),
                        Some("human" | "human-with-own-guidance")
                    )
                    || get(&value, "retained_directory").as_str()
                        != Some(path_text(&directory.join(lifecycle))?)
                    || captures.len() != 1
                    || get(capture, "mapping").as_str() != Some("verified")
                    || get(capture, "snapshot_sha256").as_str() != Some(&digest)
                {
                    return Err(fail(
                        "Current human retained source failed exact source/root/digest binding",
                    ));
                }
                if value.get("observation") == Some(&Value::Null) {
                    return Err(pending(
                        "Verified current source is awaiting shared registration",
                    ));
                }
                let observation = get(&value, "observation");
                if !matches!(observation, Value::Object(_))
                    || get(observation, "harness").as_str() != Some("omp")
                    || get(observation, "session").as_str() != Some(session)
                    || get(observation, "lifecycle").as_str() != Some(lifecycle)
                    || !truthy(get(observation, "observation_id"))
                    || get(observation, "source_reference").as_str() != Some(reference)
                    || get(observation, "source_sha256").as_str() != Some(&digest)
                {
                    return Err(fail(
                        "Current human observer evidence failed exact source/root/digest binding",
                    ));
                }
                matches.push(obj([
                    ("native_entry_id", s(id)),
                    ("human_root", s(lifecycle)),
                    ("observation_id", get(observation, "observation_id").clone()),
                    ("source_reference", s(reference)),
                    ("source_sha256", s(digest)),
                    ("mapping", get(capture, "mapping").clone()),
                ]));
            }
        }
        match matches.len() {
            0 => {
                return Err(pending(
                    "A current human turn has no retained source-bound observation yet",
                ));
            }
            1 => evidence.extend(matches),
            _ => {
                return Err(fail(
                    "Each current human turn needs exactly one retained source-bound observation",
                ));
            }
        }
    }
    if evidence
        .iter()
        .filter_map(|e| get(e, "human_root").as_str())
        .collect::<BTreeSet<_>>()
        .len()
        != humans.len()
    {
        return Err(fail(
            "Distinct current human turns cannot share an observer root",
        ));
    }
    Ok(evidence)
}

pub fn current_native_leaf(stored: &[Value]) -> Result<Option<String>> {
    let mut ids = BTreeSet::new();
    let mut leaf = None;
    let mut newest_self_selected = None;
    for entry in stored
        .iter()
        .filter(|e| !matches!(get(e, "type").as_str(), Some("session" | "title")))
    {
        let identifier = get(entry, "id")
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| fail("Stored native entry identity is unavailable or duplicated"))?;
        if !ids.insert(identifier.to_owned()) {
            return Err(fail(
                "Stored native entry identity is unavailable or duplicated",
            ));
        }
        let selected = entry
            .get("activeLeafId")
            .unwrap_or_else(|| get(entry, "id"));
        leaf = if selected == &Value::Null {
            None
        } else {
            Some(
                selected
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| fail("Stored native active leaf identity is malformed"))?
                    .to_owned(),
            )
        };
        if leaf.as_deref() == Some(identifier) {
            newest_self_selected = leaf.clone();
        }
    }
    Ok(if leaf.as_ref().is_some_and(|id| !ids.contains(id)) {
        newest_self_selected
    } else {
        leaf
    })
}

pub fn current_stream_outcomes(
    repo: &Path,
    session: &str,
    session_file: &Path,
    status: &Value,
    env: &Environment,
    pending_rows: &[Value],
) -> Result<Vec<Value>> {
    for row in pending_rows {
        let qualification = get(row, "qualification");
        if matches!(qualification, Value::Object(_))
            && get(qualification, "session").as_str() == Some(session)
            && get(qualification, "code").as_str() != Some("open-stream")
        {
            return Err(pending(
                "Current finite stream retains unresolved execution/coverage evidence",
            ));
        }
    }
    let directory = private_base(status, env)?
        .join("omp-stream-v1")
        .join(sha(session.as_bytes()));
    let leaf = current_native_leaf(&json_lines(session_file)?)?
        .ok_or_else(|| pending("Current stored native leaf is unavailable"))?;
    let mut instances = optional_entries(&directory)?.unwrap_or_default();
    instances.sort();
    let mut evidence = Vec::new();
    let mut covered = false;
    for instance_dir in instances {
        let instance_path = directory.join(instance_dir);
        let opened_path = instance_path.join("open.json");
        let Some(opened) = agent_memory::omp_transport::read_optional(&opened_path, JSON_LIMIT)?
        else {
            continue;
        };
        if get(&opened, "session").as_str() != Some(session)
            || get(&opened, "repo_root").as_str() != Some(path_text(&canonical(repo)?)?)
        {
            return Err(fail(
                "Finite stream is bound to a foreign native session/repository",
            ));
        }
        let instance = text(&opened, "instance_id")?;
        if instance_path.file_name() != Some(OsStr::new(instance)) {
            return Err(fail("Finite stream instance namespace differs"));
        }
        let attempts_dir = instance_path.join("attempts");
        let attempt_names = optional_entries(&attempts_dir)?.unwrap_or_default();
        let mut attempts = Vec::new();
        for name in attempt_names
            .into_iter()
            .filter(|n| Path::new(n).extension() == Some(OsStr::new("json")))
        {
            let number = Path::new(&name)
                .file_stem()
                .and_then(OsStr::to_str)
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| fail("Finite source attempt/outcome receipts differ"))?;
            attempts.push((number, name));
        }
        attempts.sort_by_key(|(n, _)| *n);
        for (index, (_, name)) in attempts.iter().enumerate() {
            let sequence = index + 1;
            let attempt = read_json(&attempts_dir.join(name))?;
            let outcome_path = instance_path
                .join("outcomes")
                .join(format!("{sequence}.json"));
            let Some(outcome) =
                agent_memory::omp_transport::read_optional(&outcome_path, JSON_LIMIT)?
            else {
                return Err(pending(
                    "A received finite source attempt has no terminal outcome",
                ));
            };
            let opened_digest = source_digest(&opened)?;
            if Path::new(name).file_stem() != Some(OsStr::new(&sequence.to_string()))
                || get(&attempt, "sequence").as_i64() != Some(sequence as i64)
                || get(&attempt, "instance_id").as_str() != Some(instance)
                || get(&attempt, "open_sha256").as_str() != Some(&opened_digest)
                || get(&outcome, "sequence").as_i64() != Some(sequence as i64)
                || get(&outcome, "open_sha256").as_str() != Some(&opened_digest)
                || get(&outcome, "attempt_sha256").as_str() != Some(&source_digest(&attempt)?)
            {
                return Err(fail("Finite source attempt/outcome receipts differ"));
            }
            if get(&outcome, "outcome").as_str() != Some("verified") {
                return Err(pending(
                    "Finite source attempt lacks a verified terminal outcome",
                ));
            }
            let identity = get(&attempt, "identity");
            if get(&attempt, "action").as_str() == Some("seal")
                && get(&attempt, "source_kind").as_str() == Some("session-manager")
                && matches!(get(identity, "kind").as_str(), Some("baseline" | "delta"))
                && get(identity, "leaf_id").as_str() == Some(&leaf)
            {
                covered = true;
            }
        }
        evidence.push(obj([
            ("instance_id", s(instance)),
            ("received_sequence", Value::Int(attempts.len().into())),
            ("terminal_sequence", Value::Int(attempts.len().into())),
        ]));
    }
    if !covered {
        return Err(pending(
            "No verified finite seal covers the current stored native leaf",
        ));
    }
    Ok(evidence)
}

struct Native {
    child: Option<process::OwnedProcess>,
    input: Option<UnixStream>,
    output: UnixStream,
    stderr: UnixStream,
    buffer: Vec<u8>,
    frames: VecDeque<Value>,
    eof: bool,
}
impl Native {
    fn launch(
        argv: &[OsString],
        env: &Environment,
        repo: &Path,
        timeout: Duration,
    ) -> Result<Self> {
        let (input, child_input) = UnixStream::pair()?;
        let (output, child_output) = UnixStream::pair()?;
        let (stderr, child_stderr) = UnixStream::pair()?;
        input.set_nonblocking(true)?;
        output.set_nonblocking(true)?;
        stderr.set_nonblocking(true)?;
        let env: Vec<_> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        // Ready, two turns, state, readback and bounded shutdown are all finite.
        let lifetime = timeout
            .checked_mul(5)
            .and_then(|d| d.checked_add(Duration::from_secs(30)))
            .ok_or_else(|| fail("Native deadline overflows"))?;
        let mut spec = spec(argv, &env, repo, Deadline::after(lifetime)?);
        spec.stdin = Input::Descriptor(child_input.as_fd());
        spec.stdout = Target::Descriptor(child_output.as_fd());
        spec.stderr = Target::Descriptor(child_stderr.as_fd());
        let child = process::spawn_owned(&spec).map_err(|e| fail(e.to_string()))?;
        Ok(Self {
            child: Some(child),
            input: Some(input),
            output,
            stderr,
            buffer: Vec::new(),
            frames: VecDeque::new(),
            eof: false,
        })
    }
    fn send(&mut self, value: &Value, deadline: Deadline) -> Result<()> {
        let mut bytes = encode(value, json::DEFAULT)?.into_bytes();
        bytes.push(b'\n');
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            if deadline.expired() {
                return Err(Error::Timeout("Native RPC input deadline expired".into()));
            }
            let input = self
                .input
                .as_mut()
                .ok_or_else(|| fail("Native RPC input is closed"))?;
            match input.write(remaining) {
                Ok(0) => return Err(fail("Native RPC input closed")),
                Ok(count) => {
                    remaining = remaining
                        .get(count..)
                        .ok_or_else(|| fail("Invalid native RPC write length"))?
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    self.pump()?;
                    std::thread::sleep(Duration::from_millis(5).min(deadline.remaining()));
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn pump(&mut self) -> Result<()> {
        let mut bytes = [0u8; 8192];
        // One bounded batch per sweep prevents a noisy child starving deadlines.
        for _ in 0..32 {
            match self.stderr.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => {} // Never retain arbitrary native display text.
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        for _ in 0..32 {
            match self.output.read(&mut bytes) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(count) => {
                    let chunk = bytes
                        .get(..count)
                        .ok_or_else(|| fail("Invalid native RPC read length"))?;
                    for byte in chunk {
                        if *byte == b'\n' {
                            if self.frames.len() >= FRAME_QUEUE_LIMIT {
                                return Err(fail("Native RPC queue exceeds frame limit"));
                            }
                            self.frames.push_back(
                                decode(&self.buffer)
                                    .unwrap_or_else(|_| obj([("type", s("invalid-frame"))])),
                            );
                            self.buffer.clear();
                        } else {
                            if self.buffer.len() >= FRAME_LIMIT {
                                return Err(fail("Native RPC frame exceeds byte limit"));
                            }
                            self.buffer.push(*byte);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        if self.eof && !self.buffer.is_empty() {
            self.frames.push_back(
                decode(&self.buffer).unwrap_or_else(|_| obj([("type", s("invalid-frame"))])),
            );
            self.buffer.clear();
        }
        Ok(())
    }
    fn until(
        &mut self,
        kind: &str,
        identifier: Option<&str>,
        deadline: Deadline,
        sentinels: &[String],
        report: &mut Value,
    ) -> Result<Value> {
        while !deadline.expired() {
            self.pump()?;
            while let Some(frame) = self.frames.pop_front() {
                self.observe_frame(&frame, deadline, sentinels, report)?;
                if get(&frame, "type").as_str() == Some(kind)
                    && identifier.is_none_or(|id| get(&frame, "id").as_str() == Some(id))
                {
                    return Ok(frame);
                }
                if deadline.expired() {
                    break;
                }
            }
            if self.eof {
                return Err(fail(format!("Real native process exited before {kind}")));
            }
            std::thread::sleep(Duration::from_millis(5).min(deadline.remaining()));
        }
        Err(Error::Timeout(format!(
            "Real native {kind} was not observed"
        )))
    }
    fn observe_frame(
        &mut self,
        frame: &Value,
        deadline: Deadline,
        sentinels: &[String],
        report: &mut Value,
    ) -> Result<()> {
        if get(frame, "type").as_str() == Some("extension_ui_request") {
            let method = get(frame, "method").as_str().unwrap_or("");
            let known = matches!(
                method,
                "select"
                    | "confirm"
                    | "input"
                    | "editor"
                    | "ask"
                    | "cancel"
                    | "notify"
                    | "setStatus"
                    | "setWidget"
                    | "setWorkingMessage"
                    | "setTitle"
                    | "set_editor_text"
                    | "open_url"
            );
            let mut methods = get(report, "native_ui_methods").clone();
            count(&mut methods, if known { method } else { "unknown" }, 1);
            set(report, "native_ui_methods", methods);
            for code in [
                "inactive: unsupported public API or activation binding",
                "duplicate-binding",
                "observer pending",
                "observer result unavailable",
                "observer shutdown flush pending",
            ] {
                let short = format!("am: {code}");
                let long = format!(
                    "Agent Memory: {code}; sources remain pending. Use am status/doctor and am omp-boundary --list-pending."
                );
                if ["message", "text", "statusText"].iter().any(|key| {
                    get(frame, key)
                        .as_str()
                        .is_some_and(|s| s == short || s == long)
                }) && !get(report, "native_adapter_diagnostics")
                    .as_array()
                    .unwrap_or(&[])
                    .contains(&s(code))
                {
                    append(report, "native_adapter_diagnostics", s(code));
                }
            }
            if !known || matches!(method, "select" | "confirm" | "input" | "editor" | "ask") {
                count(report, "refused_ui_requests", 1);
                self.send(
                    &obj([
                        ("type", s("extension_ui_response")),
                        ("id", get(frame, "id").clone()),
                        ("cancelled", Value::Bool(true)),
                    ]),
                    deadline,
                )?;
            }
        }
        if get(frame, "type").as_str() == Some("message_end")
            && get(get(frame, "message"), "role").as_str() == Some("assistant")
        {
            let content = get(get(frame, "message"), "content");
            let text = if let Some(text) = content.as_str() {
                text.to_owned()
            } else {
                content
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|b| get(b, "type").as_str() == Some("text"))
                    .filter_map(|b| get(b, "text").as_str())
                    .collect::<String>()
            };
            let mut counts = get(report, "task_sentinels").clone();
            for sentinel in sentinels {
                count(
                    &mut counts,
                    sentinel,
                    text.matches(sentinel.as_str()).count(),
                );
            }
            set(report, "task_sentinels", counts);
        }
        Ok(())
    }
    fn finish(mut self) -> Result<i32> {
        self.input.take();
        let child = self
            .child
            .take()
            .ok_or_else(|| fail("Native process already closed"))?;
        // Drain/discard output concurrently with natural shutdown, without reader threads.
        let deadline = Deadline::after(Duration::from_secs(15))?;
        while !self.eof && !deadline.expired() {
            self.pump()?;
            self.frames.clear();
            std::thread::sleep(Duration::from_millis(5).min(deadline.remaining()));
        }
        match child.finish(deadline) {
            Ok(output) => Ok(status_code(output.status)),
            Err(error) if error.reason == process::Refusal::Deadline => error
                .evidence
                .and_then(|e| e.status)
                .map(status_code)
                .ok_or_else(|| fail("Native shutdown did not retain a collected exit status")),
            Err(error) => Err(fail(error.to_string())),
        }
    }
}

struct Arguments {
    binary: PathBuf,
    am: PathBuf,
    receipt: PathBuf,
    repo: PathBuf,
    output: PathBuf,
    model: String,
    approval_mode: Option<String>,
    timeout: Duration,
}
impl Arguments {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = BTreeMap::new();
        let mut remaining = args.iter();
        while let Some(key) = remaining.next() {
            let (key, inline) = key
                .split_once('=')
                .map_or((key.as_str(), None), |(key, value)| (key, Some(value)));
            if !matches!(
                key,
                "--binary"
                    | "--am"
                    | "--receipt"
                    | "--repo"
                    | "--output"
                    | "--model"
                    | "--approval-mode"
                    | "--timeout"
            ) {
                return Err(fail(format!("unrecognized arguments: {key}")));
            }
            let value = inline
                .or_else(|| remaining.next().map(String::as_str))
                .ok_or_else(|| fail(format!("argument {key}: expected one argument")))?;
            options.insert(key.to_owned(), value.to_owned());
        }
        let required = |key: &str| {
            options
                .get(key)
                .cloned()
                .ok_or_else(|| fail(format!("the following argument is required: {key}")))
        };
        let seconds = options
            .get("--timeout")
            .map(|s| s.parse::<i64>())
            .transpose()
            .map_err(|_| fail("argument --timeout: invalid int value"))?
            .unwrap_or(240);
        let timeout = Duration::from_secs(
            u64::try_from(seconds).map_err(|_| fail("argument --timeout: must be nonnegative"))?,
        );
        Ok(Self {
            binary: required("--binary")?.into(),
            am: required("--am")?.into(),
            receipt: required("--receipt")?.into(),
            repo: required("--repo")?.into(),
            output: required("--output")?.into(),
            model: required("--model")?,
            approval_mode: options
                .get("--approval-mode")
                .cloned()
                .filter(|s| !s.is_empty()),
            timeout,
        })
    }
}
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|s| matches!(s.as_str(), "--help" | "-h")) {
        print!("{HELP}");
        return 0;
    }
    let args = match Arguments::parse(args) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}\n{HELP}");
            return 2;
        }
    };
    match smoke(args) {
        Ok(qualified) => {
            if qualified {
                0
            } else {
                1
            }
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}
fn smoke(args: Arguments) -> Result<bool> {
    let root = agent_memory::fs::canonical_path(&args.output, false)?;
    let parent = root
        .parent()
        .ok_or_else(|| fail("Output directory has no parent"))?;
    let name = root
        .file_name()
        .ok_or_else(|| fail("Output directory has no name"))?;
    let parent = DirectoryChain::open(parent)?;
    parent.directory().make_directory_at(name, 0o700)?;
    let output_dir = parent.directory().open_directory_at(name)?;
    let repo = canonical(&args.repo)?;
    let binary = canonical(&args.binary)?;
    let receipt_path = canonical(&args.receipt)?;
    let receipt_bytes = read_bytes(&receipt_path, JSON_LIMIT)?;
    let receipt = decode(&receipt_bytes)?;
    if get(&receipt, "schema").as_str() != Some("agent-memory/install-receipt-v3") {
        return Err(fail(
            "not a Rust runtime install: explicit receipt-v3 native binding required",
        ));
    }
    let binding = get(&receipt, "omp_binding");
    if !truthy(binding) {
        return Err(fail("Explicit real installed native binding required"));
    }
    let extension = PathBuf::from(text(binding, "extension_target")?);
    let owned = rows(get(&receipt, "targets"))?
        .iter()
        .find(|t| get(t, "path").as_str() == extension.to_str())
        .ok_or_else(|| fail("Installed native extension is not receipt-owned"))?;
    let extension_digest = text(get(owned, "identity"), "hash")?;
    if sha(&read_bytes(&extension, JSON_LIMIT)?) != extension_digest {
        return Err(fail("Installed native extension differs from receipt"));
    }
    let am = canonical(&args.am)?;
    let bin_dir = PathBuf::from(text(&receipt, "bin_dir")?);
    if ![bin_dir.join("am"), bin_dir.join("agent-memory")].contains(&am) {
        return Err(fail("Smoke must use an exact receipt-owned public command"));
    }
    let mut env: Environment = std::env::vars_os()
        .filter(|(key, _)| {
            let key = key.to_string_lossy();
            ![
                "PI_SESSION",
                "PI_TOOL_BRIDGE",
                "PI_ARTIFACT",
                "PI_EVAL",
                "VOX_",
            ]
            .iter()
            .any(|prefix| key.starts_with(prefix))
        })
        .collect();
    env.insert(
        "PI_CODING_AGENT_DIR".into(),
        text(binding, "agent_dir")?.into(),
    );
    let mut argv: Vec<OsString> = vec![
        binary.clone().into_os_string(),
        "--mode".into(),
        "rpc".into(),
        "--cwd".into(),
        repo.clone().into_os_string(),
        "--session-dir".into(),
        root.join("sessions").into_os_string(),
        "--model".into(),
        args.model.clone().into(),
        "--max-time".into(),
        args.timeout.as_secs().to_string().into(),
    ];
    if let Some(mode) = &args.approval_mode {
        argv.extend(["--approval-mode".into(), mode.into()]);
    }
    let mut report = obj([
        ("schema", s("agent-memory/omp-installed-smoke-v1")),
        ("qualified", Value::Bool(false)),
        ("full_native_qualification", Value::Bool(false)),
        (
            "platform",
            s(if cfg!(target_os = "macos") {
                "darwin"
            } else {
                "linux"
            }),
        ),
        ("binary_sha256", s(sha(&read_bytes(&binary, BINARY_LIMIT)?))),
        ("native_version", Value::Null),
        ("supported_versions", list(SUPPORTED_VERSIONS.map(s))),
        ("installed_source", get(&receipt, "source").clone()),
        ("receipt_sha256", s(sha(&receipt_bytes))),
        ("extension_sha256", s(extension_digest)),
        ("command", list(argv.iter().map(|a| s(a.to_string_lossy())))),
        ("model", s(&args.model)),
        (
            "approval_mode",
            s(args
                .approval_mode
                .as_deref()
                .unwrap_or("native-existing-default")),
        ),
        ("native_requests", list([])),
        ("refused_ui_requests", Value::Int(0.into())),
        ("task_sentinels", obj([])),
        ("failures", list([])),
        ("native_adapter_diagnostics", list([])),
        ("native_ui_methods", obj([])),
        (
            "observer_execution_route",
            obj([
                (
                    "authority",
                    s("receipt-owned native hook spawn; not a public one-shot writer"),
                ),
                (
                    "argv_prefix",
                    list([s(path_text(&am)?), s("omp-boundary"), s("--stream")]),
                ),
                (
                    "evidence",
                    s(
                        "loaded owned extension plus actual finite attempt/outcome receipts; not a separate process-argv trace",
                    ),
                ),
            ]),
        ),
        ("readback_attempts", Value::Int(0.into())),
    ]);
    let sentinels = [
        format!("OMP_SMOKE_FIRST_{}", host::uuid4()?.replace('-', "")),
        format!("OMP_SMOKE_SECOND_{}", host::uuid4()?.replace('-', "")),
    ];
    let mut native = None;
    let journey = (|| -> Result<()> {
        let version = run_command(
            &[binary.into_os_string(), "--version".into()],
            &repo,
            &env,
            None,
        )?;
        let version = std::str::from_utf8(&version)
            .map_err(|_| fail("Native version is not UTF-8"))?
            .trim();
        set(&mut report, "native_version", s(version));
        if !SUPPORTED_VERSIONS
            .iter()
            .any(|v| version == format!("omp/{v}"))
        {
            return Err(fail(format!(
                "Unsupported omp binary: require exactly one of omp/18.16.0, omp/18.19.0, observed {version}"
            )));
        }
        native = Some(Native::launch(&argv, &env, &repo, args.timeout)?);
        let native = native
            .as_mut()
            .ok_or_else(|| fail("Native process is unavailable"))?;
        native.until(
            "ready",
            None,
            Deadline::after(args.timeout)?,
            &sentinels,
            &mut report,
        )?;
        let mut prompts = Vec::new();
        for (index, sentinel) in sentinels.iter().enumerate() {
            let identifier = format!("human-{}", index + 1);
            append(&mut report, "native_requests", s(&identifier));
            let arithmetic = if index == 0 { "19 + 23" } else { "7 * 8" };
            let prompt = format!(
                "This is a disposable real-native Agent Memory smoke, not durable project learning. Calculate {arithmetic} and return exactly {sentinel}. Preserve this substantive answer once. Use the installed Agent Memory skill for ordinary initial recall. Do not manually invoke boundary bookkeeping before answering; allow the installed native adapter to request it. Under normal tool permissions, review actual retained sources before any no-learning assessment; never fake a receipt. If memory is refused/unavailable, finish the task and leave memory pending."
            );
            native.send(
                &obj([
                    ("id", s(&identifier)),
                    ("type", s("prompt")),
                    ("message", s(&prompt)),
                ]),
                Deadline::after(args.timeout)?,
            )?;
            prompts.push(prompt);
            native.until(
                "session_settled",
                None,
                Deadline::after(args.timeout)?,
                &sentinels,
                &mut report,
            )?;
        }
        native.send(
            &obj([("id", s("state")), ("type", s("get_state"))]),
            Deadline::after(args.timeout)?,
        )?;
        let state = native.until(
            "response",
            Some("state"),
            Deadline::after(args.timeout)?,
            &sentinels,
            &mut report,
        )?;
        let data = get(&state, "data");
        let session = get(data, "sessionId").as_str().unwrap_or("");
        let session_file = Path::new(get(data, "sessionFile").as_str().unwrap_or(""));
        set(&mut report, "session_id", get(data, "sessionId").clone());
        set(
            &mut report,
            "native_session_file",
            get(data, "sessionFile").clone(),
        );
        set(
            &mut report,
            "native_is_settled",
            get(data, "isSettled").clone(),
        );
        if !answers_once(&report, &sentinels) {
            return Err(fail("Substantive answer was missing or duplicated"));
        }
        if get(&report, "native_is_settled") != &Value::Bool(true) {
            return Err(fail("Current native session is not settled"));
        }
        verify_owned(&receipt_path, &extension, &report, false)?;
        let waiting = monotonic_ns();
        let deadline = Deadline::after(args.timeout)?;
        let observations = loop {
            count(&mut report, "readback_attempts", 1);
            let attempt = get(&report, "readback_attempts").as_i64().unwrap_or(0);
            let readback = (|| -> Result<Vec<Value>> {
                bounded_timeout(Some(deadline), args.timeout)?;
                let identifier = format!("observer-progress-{attempt}");
                native.send(
                    &obj([("id", s(&identifier)), ("type", s("get_state"))]),
                    deadline,
                )?;
                let progress = native.until(
                    "response",
                    Some(&identifier),
                    deadline,
                    &sentinels,
                    &mut report,
                )?;
                if get(get(&progress, "data"), "sessionId").as_str() != Some(session) {
                    return Err(fail("Native session changed while awaiting its observer"));
                }
                let pending_argv = vec![
                    am.clone().into_os_string(),
                    "omp-boundary".into(),
                    "--repo".into(),
                    repo.clone().into_os_string(),
                    "--list-pending".into(),
                ];
                let pending_page =
                    decode(&run_command(&pending_argv, &repo, &env, Some(deadline))?)?;
                set(&mut report, "pending_readback", pending_page.clone());
                let mut pending_rows = rows(get(&pending_page, "pending"))?.to_vec();
                let mut cursor = get(&pending_page, "next_offset").clone();
                while cursor != Value::Null {
                    let offset = cursor
                        .as_i64()
                        .ok_or_else(|| fail("Native pending inventory cursor is malformed"))?;
                    let mut argv = pending_argv.clone();
                    argv.extend(["--offset".into(), offset.to_string().into()]);
                    let page = decode(&run_command(&argv, &repo, &env, Some(deadline))?)?;
                    let next = get(&page, "next_offset");
                    if next != &Value::Null && next.as_i64().is_none_or(|next| next <= offset) {
                        return Err(fail("Native pending inventory cursor did not advance"));
                    }
                    pending_rows.extend_from_slice(rows(get(&page, "pending"))?);
                    if pending_rows.len() > DIRECTORY_LIMIT {
                        return Err(fail("Native pending inventory exceeds row limit"));
                    }
                    cursor = next.clone();
                }
                let status = decode(&run_command(
                    &[
                        am.clone().into_os_string(),
                        "--json".into(),
                        "status".into(),
                        "--repo".into(),
                        repo.clone().into_os_string(),
                    ],
                    &repo,
                    &env,
                    Some(deadline),
                )?)?;
                set(&mut report, "status_readback", status.clone());
                if !same(get(get(&status, "omp"), "binding"), binding)?
                    || get(get(&status, "omp"), "owned_disk_integrity").as_str() != Some("verified")
                {
                    return Err(fail("Current installed native ownership is not verified"));
                }
                let streams = current_stream_outcomes(
                    &repo,
                    session,
                    session_file,
                    &status,
                    &env,
                    &pending_rows,
                )?;
                set(&mut report, "stream_outcomes", list(streams));
                let observations = current_observations(
                    &am,
                    &repo,
                    &root,
                    session,
                    session_file,
                    &prompts,
                    &status,
                    &env,
                    Some(deadline),
                )?;
                let identifier = format!("observer-final-{attempt}");
                native.send(
                    &obj([("id", s(&identifier)), ("type", s("get_state"))]),
                    deadline,
                )?;
                let latest = native.until(
                    "response",
                    Some(&identifier),
                    deadline,
                    &sentinels,
                    &mut report,
                )?;
                let latest = get(&latest, "data");
                if get(latest, "sessionId").as_str() != Some(session) {
                    return Err(fail("Native session changed before final qualification"));
                }
                set(
                    &mut report,
                    "native_is_settled",
                    get(latest, "isSettled").clone(),
                );
                if !answers_once(&report, &sentinels) {
                    return Err(fail(
                        "Substantive answer was missing or duplicated during observer waiting",
                    ));
                }
                if get(latest, "isSettled") != &Value::Bool(true) {
                    return Err(pending(
                        "Native work resumed while awaiting observer evidence",
                    ));
                }
                if deadline.expired() {
                    return Err(pending(
                        "Native observations completed after the readback deadline",
                    ));
                }
                Ok(observations)
            })();
            match readback {
                Ok(observations) => break observations,
                Err(Error::Pending(_)) => {
                    native.pump()?;
                    if deadline.expired() || native.eof {
                        return Err(pending(
                            "Current human observer evidence did not complete before the readback deadline",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(500).min(deadline.remaining()));
                }
                Err(error) => return Err(error),
            }
        };
        set(
            &mut report,
            "observer_wait_seconds",
            Value::Float(monotonic_ns().saturating_sub(waiting) as f64 / 1_000_000_000.0),
        );
        set(
            &mut report,
            "current_human_observations",
            list(observations),
        );
        verify_owned(&receipt_path, &extension, &report, true)?;
        set(&mut report, "qualified", Value::Bool(true));
        set(
            &mut report,
            "limits",
            list([
                s("Smoke proves only the recorded real installed multi-turn/source journey."),
                s("Inspect individual source assessments, refresh and publication separately."),
                s(
                    "Pending sources/refused tools remain pending; this is not a full source-drain or release pass.",
                ),
                s(
                    "Other modes/platforms/clients, durable capture and peer exchange need the full retained matrix.",
                ),
            ]),
        );
        Ok(())
    })();
    if let Err(error) = journey {
        append(&mut report, "failures", s(error.to_string()));
    }
    if let Some(native) = native {
        match native.finish() {
            Ok(code) => set(&mut report, "native_exit", Value::Int(code.into())),
            Err(error) => {
                append(&mut report, "failures", s(error.to_string()));
                set(&mut report, "qualified", Value::Bool(false));
            }
        }
    }
    let mut report_file =
        output_dir.create_exclusive(OsStr::new("report.json"), libc::O_WRONLY, 0o600)?;
    let mut raw = encode(&report, json::INDENT2_ASCII)?;
    raw.push('\n');
    report_file.write_all(raw.as_bytes())?;
    let qualified = get(&report, "qualified") == &Value::Bool(true);
    println!(
        "{}",
        encode(
            &obj([
                ("report", s(path_text(&root.join("report.json"))?)),
                ("qualified", Value::Bool(qualified)),
                ("full_native_qualification", Value::Bool(false))
            ]),
            json::DEFAULT
        )?
    );
    Ok(qualified)
}
fn answers_once(report: &Value, sentinels: &[String]) -> bool {
    sentinels
        .iter()
        .all(|s| get(get(report, "task_sentinels"), s).as_i64() == Some(1))
}
fn verify_owned(receipt: &Path, extension: &Path, report: &Value, final_check: bool) -> Result<()> {
    let suffix = if final_check {
        "before final qualification"
    } else {
        "during this smoke"
    };
    if get(report, "receipt_sha256").as_str() != Some(&sha(&read_bytes(receipt, JSON_LIMIT)?)) {
        return Err(fail(format!(
            "Installed ownership receipt changed {suffix}"
        )));
    }
    if get(report, "extension_sha256").as_str() != Some(&sha(&read_bytes(extension, JSON_LIMIT)?)) {
        return Err(fail(format!("Installed native extension changed {suffix}")));
    }
    Ok(())
}
