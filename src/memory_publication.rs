//! Publish reviewed memory independently of feature code; verify actual authority trees.
use crate::bank_contract as bank;
use crate::bank_transactions as tx;
use crate::bank_writes as writes;
use crate::codec::json::{self, DecodePolicy, Value};
use crate::developer_identity::encoded;
use crate::error::{ContractError, Kind, Result};
use crate::fs::Descriptor;
use crate::layout_recovery_publication::RecoveryPublicationContext;
use crate::lifecycle::{CommandResult, get, object, string};
use crate::process::{self, Input, Spec, Status, Target};
use crate::scope_binding::{self, RequireOptions};
use crate::sharing_limits::{self as limits, truthy};
use crate::sharing_recovery as recovery;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SCHEMA: &str = "agent-memory/publication-request-v1";
pub const FIELDS: &[&str] = &[
    "schema",
    "repo_root",
    "project_id",
    "base",
    "candidate",
    "proof",
];
pub const LIMIT: usize = 1048576;
pub const API_VERSION: &str = "2026-03-10";
/// Baseline `subprocess.run(..., timeout=30)` for every publication transport child.
pub const TIMEOUT: Duration = Duration::from_secs(30);
/// Bound for hashing the original Git index (the baseline read it whole).
pub const MAX_INDEX_BYTES: usize = limits::MAX_PRIVATE;
const PROOF_V2: &str = "agent-memory/semantic-sharing-proof-v2";
const PUSH_REPAIR: &str = "Retain local learning and pending sources; inspect the authenticated GitHub user and branch allowed users. Team/App-only grants require a supported repository workflow; do not loosen protection or switch credentials automatically.";
const GRAPHQL: &str = "query($owner:String!,$repo:String!,$number:Int!){repository(owner:$owner,name:$repo){pullRequest(number:$number){state isDraft headRefOid baseRefOid reviewDecision mergeStateStatus commits(last:1){nodes{commit{oid statusCheckRollup{state}}}} reviewThreads(first:100){pageInfo{hasNextPage} nodes{isResolved}}}}}";
const PR_BODY: &str = "Memory-only learning prepared from the current authority tree. Conditions, evidence and provenance remain in the Markdown bank. Required reviews and checks must complete; no bypass or automatic approval is requested.";

static STATUS: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: the literal pattern is valid.
    Regex::new(r"^HTTP/[0-9.]+ ([0-9]{3})(?-u:\b)").expect("static status pattern")
});
static LINK_PART: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: the literal pattern is valid.
    Regex::new(r#"^(?-u:\s)*<[^>]+>;(?-u:\s)*rel="(?:next|prev|first|last)"(?-u:\s)*$"#)
        .expect("static link pattern")
});
static NEXT_LINK: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: the literal pattern is valid.
    Regex::new(r#"<([^>]+)>;(?-u:\s)*rel="next""#).expect("static next pattern")
});
static LOGIN: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: the literal pattern is valid.
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9-]{0,38}$").expect("static login pattern")
});
static RULE_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: the literal pattern is valid.
    Regex::new(r"^[a-z_]{1,64}$").expect("static rule pattern")
});
static GITHUB_REMOTE: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: the literal pattern is valid.
    Regex::new(r"^repo://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)$")
        .expect("static remote pattern")
});

fn err(message: impl Into<String>) -> ContractError {
    ContractError::new(message)
}
fn text<'a>(value: &'a Value, message: &str) -> Result<&'a str> {
    value.as_str().ok_or_else(|| err(message))
}
fn path_text<'a>(path: &'a Path, what: &str) -> Result<&'a str> {
    crate::host::text(path.as_os_str(), what)
}
/// Python dict/list equality of decoded JSON values, independent of key order.
pub(crate) fn same(left: &Value, right: &Value) -> Result<bool> {
    Ok(encoded(left)? == encoded(right)?)
}
fn sha(raw: &[u8]) -> String {
    crate::validation_memo::hex(&Sha256::digest(raw))
}
fn fields(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    object(pairs)
}
fn merged(mut base: Value, values: Value) -> Result<Value> {
    bank::merge(&mut base, &values)?;
    Ok(base)
}

/// Allowed caller Git configuration plus fixed no-lazy-fetch/no-trace settings.
pub fn environment() -> BTreeMap<OsString, OsString> {
    let allowed = [
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_TERMINAL_PROMPT",
        "GIT_ASKPASS",
        "GIT_ALLOW_PROTOCOL",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ];
    let mut env: BTreeMap<OsString, OsString> = std::env::vars_os()
        .filter(|(key, _)| {
            !key.as_encoded_bytes().starts_with(b"GIT_")
                || allowed.iter().any(|name| key == OsStr::new(name))
        })
        .collect();
    for (key, value) in [
        ("GIT_NO_LAZY_FETCH", "1"),
        ("GIT_OPTIONAL_LOCKS", "0"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_TRACE", "0"),
        ("GIT_TRACE2", "0"),
        ("GIT_TRACE2_EVENT", "0"),
        ("GIT_TRACE2_PERF", "0"),
    ] {
        env.insert(key.into(), value.into());
    }
    // Never emit credentials or response bodies through gh debug logging.
    env.remove(OsStr::new("GH_DEBUG"));
    env
}

/// A held write-only `/dev/null`; child stderr is never captured or disclosed.
pub(crate) fn null_sink() -> Result<Descriptor> {
    let unavailable =
        || err("Publication transport unavailable or interrupted; inventory before retry");
    let dev = Descriptor::open_directory(Path::new("/dev")).map_err(|_| unavailable())?;
    dev.open_at(OsStr::new("null"), libc::O_WRONLY, 0)
        .map_err(|_| unavailable())
}

/// One owned direct child with the fixed 30 second deadline and a 1 MiB reader.
pub fn command(
    argv: &[OsString],
    env: &BTreeMap<OsString, OsString>,
    payload: Option<&[u8]>,
) -> Result<(i32, Vec<u8>)> {
    let unavailable =
        || err("Publication transport unavailable or interrupted; inventory before retry");
    let env: Vec<(OsString, OsString)> = env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let null = null_sink()?;
    let deadline = crate::clock::Deadline::after(TIMEOUT).map_err(|_| unavailable())?;
    let output = process::run(&Spec {
        argv,
        search_path: true,
        env: &env,
        cwd: None,
        stdin: payload.map(Input::Bytes).unwrap_or(Input::Null),
        stdout: Target::Capture { limit: LIMIT },
        stderr: Target::Descriptor(null.as_fd()),
        deadline,
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .map_err(|failure| {
        if matches!(failure.reason, process::Refusal::CaptureLimit { .. }) {
            err("Publication response exceeds its complete bounded reader")
        } else {
            unavailable()
        }
    })?;
    let code = match output.status {
        Status::Code(code) => code,
        Status::Signal(signal) => -signal,
    };
    Ok((code, output.stdout))
}

fn git_argv(root: &Path, args: &[&str]) -> Vec<OsString> {
    let mut argv: Vec<OsString> = [
        "git",
        "--no-pager",
        "--no-replace-objects",
        "--literal-pathspecs",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-C",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    argv.push(root.as_os_str().to_owned());
    argv.extend(args.iter().map(OsString::from));
    argv
}

/// Options of one publication Git write; the default is a local, required call.
#[derive(Default, Clone, Copy)]
pub struct GitWrite<'a> {
    pub payload: Option<&'a [u8]>,
    pub index: Option<&'a Path>,
    pub network: bool,
    pub optional: bool,
}

/// A normal push retains the repository's pre-push hook. No --force, admin,
/// lease override, filter, checkout, reset or original-index mutation exists.
pub fn git_write(root: &Path, args: &[&str], options: GitWrite<'_>) -> Result<(i32, String)> {
    let mut env = environment();
    if let Some(index) = options.index {
        env.insert("GIT_INDEX_FILE".into(), index.as_os_str().to_owned());
    }
    if !options.network {
        env.insert("GIT_ALLOW_PROTOCOL".into(), "".into());
    }
    let (code, raw) = command(&git_argv(root, args), &env, options.payload)?;
    if code != 0 && !options.optional {
        return Err(err(
            "Memory publication Git operation failed; retain local learning and inventory",
        ));
    }
    let text = String::from_utf8(raw)
        .map_err(|_| err("Memory publication Git output is not UTF-8; inventory before retry"))?;
    Ok((code, text))
}

/// Baseline `identity_git.git(root, *args, optional=...)`.
pub(crate) fn git(root: &Path, args: &[&str], optional: bool) -> Result<Option<String>> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let value =
        crate::identity_git::git(root, &args, optional, false).map_err(|error| {
            match error.contract() {
                Some(contract) => contract.clone(),
                None => err(error.to_string()),
            }
        })?;
    value
        .map(|raw| {
            raw.into_string()
                .map_err(|_| err("Publication Git output is not UTF-8"))
        })
        .transpose()
}

/// Python `str.splitlines()` line boundaries, after CRLF normalisation.
fn splitlines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text
        .split(|c| {
            matches!(
                c,
                '\n' | '\r'
                    | '\u{0b}'
                    | '\u{0c}'
                    | '\u{1c}'
                    | '\u{1d}'
                    | '\u{1e}'
                    | '\u{85}'
                    | '\u{2028}'
                    | '\u{2029}'
            )
        })
        .collect();
    if text.ends_with([
        '\n', '\r', '\u{0b}', '\u{0c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ]) {
        lines.pop();
    }
    lines
}

pub type Headers = BTreeMap<String, String>;

/// One `gh api` exchange. `--include` distinguishes an observed 404 from
/// auth/permission/network failure. `None` only for an allowed missing 404.
pub fn api(
    endpoint: &str,
    method: &str,
    body: Option<&Value>,
    allow_missing: bool,
) -> Result<Option<(Value, Headers)>> {
    let version = format!("X-GitHub-Api-Version: {API_VERSION}");
    let mut argv: Vec<OsString> = [
        "gh",
        "api",
        "--hostname",
        "github.com",
        "--include",
        "--method",
        method,
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        &version,
        endpoint,
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if body.is_some() {
        argv.push("--input".into());
        argv.push("-".into());
    }
    let payload = body.map(encoded).transpose()?;
    let (code, raw) = command(&argv, &environment(), payload.as_deref())?;
    let unavailable =
        || err("GitHub rules or permission observation unavailable; publication remains pending");
    let incomplete = || err("Incomplete GitHub policy response; publication remains pending");
    let text = String::from_utf8(raw)
        .map_err(|_| incomplete())?
        .replace("\r\n", "\n");
    let (header, content, separated) = match text.split_once("\n\n") {
        Some((header, content)) => (header, content, true),
        None => (text.as_str(), "", false),
    };
    let status = STATUS
        .captures(header)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<u16>().ok());
    if status == Some(404) && allow_missing {
        return Ok(None);
    }
    let allowed: &[u16] = if method == "GET" { &[200] } else { &[200, 201] };
    if code != 0 || !status.is_some_and(|s| allowed.contains(&s)) || !separated {
        return Err(unavailable());
    }
    let value = json::decode(
        content,
        DecodePolicy {
            max_bytes: LIMIT,
            ..DecodePolicy::default()
        },
    )
    .map_err(|_| incomplete())?;
    let mut headers = Headers::new();
    for line in splitlines(header).into_iter().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.to_lowercase(), value.trim().to_owned());
        }
    }
    Ok(Some((value, headers)))
}

fn api_value(endpoint: &str) -> Result<Value> {
    api(endpoint, "GET", None, false)?
        .map(|(value, _)| value)
        .ok_or_else(|| {
            err("GitHub rules or permission observation unavailable; publication remains pending")
        })
}
fn api_with_headers(endpoint: &str) -> Result<(Value, Headers)> {
    api(endpoint, "GET", None, false)?.ok_or_else(|| {
        err("GitHub rules or permission observation unavailable; publication remains pending")
    })
}
fn api_post(endpoint: &str, body: &Value) -> Result<Value> {
    api(endpoint, "POST", Some(body), false)?
        .map(|(value, _)| value)
        .ok_or_else(|| {
            err("GitHub rules or permission observation unavailable; publication remains pending")
        })
}

/// Read every page within a finite bound; never assume the first page is all rules.
pub fn collection(endpoint: &str, key: Option<&str>) -> Result<Vec<Value>> {
    let mut found = Vec::new();
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    for page in 1..5 {
        let (value, headers) =
            api_with_headers(&format!("{endpoint}{separator}per_page=100&page={page}"))?;
        let rows = match (key, &value) {
            (Some(key), Value::Object(_)) => value.get(key).cloned().unwrap_or(Value::Null),
            _ => value,
        };
        let Value::Array(rows) = rows else {
            return Err(err("Incomplete paginated GitHub policy/check inventory"));
        };
        if rows.len() > 100 {
            return Err(err("Incomplete paginated GitHub policy/check inventory"));
        }
        found.extend(rows);
        if !headers
            .get("link")
            .is_some_and(|link| link.contains("rel=\"next\""))
        {
            return Ok(found);
        }
    }
    Err(err(
        "Complete GitHub rule/check inventory exceeds its review bound",
    ))
}

pub fn repository(root: &Path) -> Result<String> {
    let status = crate::project_identity::status(root)?;
    let identity = get(get(&status, "evidence"), "canonical_remote")
        .as_str()
        .unwrap_or("");
    let captures = GITHUB_REMOTE.captures(identity).ok_or_else(|| {
        err("Automatic landing requires observable GitHub rules; retain local memory for other hosts")
    })?;
    let owner = captures.get(1).map_or("", |m| m.as_str());
    let name = captures.get(2).map_or("", |m| m.as_str());
    Ok(format!("{owner}/{name}"))
}

/// Python `urllib.parse.quote(value, safe='')`.
pub(crate) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Python `urllib.parse.unquote_plus` over ASCII percent escapes; invalid
/// escapes stay literal, invalid UTF-8 is refused.
fn unquote_plus(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let hex = |offset: usize| {
            bytes
                .get(index + offset)
                .and_then(|b| (*b as char).to_digit(16))
        };
        match (byte, hex(1), hex(2)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            (b'+', _, _) => {
                out.push(b' ');
                index += 1;
            }
            _ => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out)
        .map_err(|_| err("Allowed push-user pagination chain changed or is incomplete"))
}

/// Python `urlsplit` components needed by the pagination check.
struct Split<'a> {
    scheme: &'a str,
    netloc: &'a str,
    path: &'a str,
    query: &'a str,
    fragment: Option<&'a str>,
}
fn urlsplit(url: &str) -> Split<'_> {
    let (rest, fragment) = match url.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (url, None),
    };
    let (scheme, rest) = match rest.split_once(':') {
        Some((scheme, rest))
            if scheme
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic())
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) =>
        {
            (scheme, rest)
        }
        _ => ("", rest),
    };
    let (netloc, rest) = match rest.strip_prefix("//") {
        Some(after) => {
            let end = after.find(['/', '?']).unwrap_or(after.len());
            after.split_at(end)
        }
        None => ("", rest),
    };
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    Split {
        scheme,
        netloc,
        path,
        query,
        fragment: fragment.filter(|f| !f.is_empty()),
    }
}
/// Python `parse_qs(query)` (blank values dropped, `&` separators).
fn parse_qs(query: &str) -> Result<BTreeMap<String, Vec<String>>> {
    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for part in query.split('&').filter(|part| !part.is_empty()) {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        values
            .entry(unquote_plus(name)?)
            .or_default()
            .push(unquote_plus(value)?);
    }
    Ok(values)
}

fn push_user(value: &Value) -> Result<Value> {
    let malformed = || err("Push-actor user identity is incomplete or malformed");
    let id = value
        .get("id")
        .and_then(Value::as_int)
        .and_then(|id| id.as_i128())
        .ok_or_else(malformed)?;
    let login = value
        .get("login")
        .and_then(Value::as_str)
        .ok_or_else(malformed)?;
    if id <= 0
        || value.get("type").and_then(Value::as_str) != Some("User")
        || !LOGIN.is_match(login)
    {
        return Err(malformed());
    }
    Ok(fields([
        ("id", value.get("id").cloned().unwrap_or(Value::Null)),
        ("login", string(login.to_lowercase())),
        ("type", string("User")),
    ]))
}
fn user_id(value: &Value) -> i128 {
    get(value, "id")
        .as_int()
        .and_then(|id| id.as_i128())
        .unwrap_or(0)
}

fn push_actor_observation(endpoint: &str, restrictions: &Value) -> Result<Value> {
    let complete = restrictions.as_object().is_some()
        && ["users", "teams", "apps"]
            .iter()
            .all(|key| matches!(restrictions.get(key), Some(Value::Array(_))));
    if !complete {
        return Err(err(
            "Classic push-actor restriction is incomplete or malformed",
        ));
    }
    let actor = push_user(&api_value("user")?)?;
    let mut users: Vec<Value> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut complete_chain = false;
    for page in 1..5 {
        let (rows, headers) = api_with_headers(&format!("{endpoint}?per_page=100&page={page}"))?;
        let Value::Array(rows) = rows else {
            return Err(err(
                "Allowed push-user inventory is incomplete or malformed",
            ));
        };
        if rows.len() > 100 {
            return Err(err(
                "Allowed push-user inventory is incomplete or malformed",
            ));
        }
        for row in &rows {
            let value = push_user(row)?;
            if !seen.insert(user_id(&value)) {
                return Err(err(
                    "Allowed push-user inventory has duplicate or conflicting IDs",
                ));
            }
            users.push(value);
        }
        let link = headers.get("link").map_or("", String::as_str);
        if !link.is_empty() && !link.split(',').all(|part| LINK_PART.is_match(part)) {
            return Err(err("Allowed push-user pagination header is malformed"));
        }
        if !link.contains("rel=\"next\"") {
            complete_chain = true;
            break;
        }
        let matches: Vec<&str> = NEXT_LINK
            .captures_iter(link)
            .filter_map(|captures| captures.get(1).map(|m| m.as_str()))
            .collect();
        let [next] = matches.as_slice() else {
            return Err(err("Allowed push-user pagination chain is malformed"));
        };
        let parsed = urlsplit(next);
        let expected = BTreeMap::from([
            ("page".to_owned(), vec![(page + 1).to_string()]),
            ("per_page".to_owned(), vec!["100".to_owned()]),
        ]);
        if parsed.scheme != "https"
            || parsed.netloc != "api.github.com"
            || parsed.path != format!("/{endpoint}")
            || parsed.fragment.is_some()
            || parse_qs(parsed.query)? != expected
        {
            return Err(err(
                "Allowed push-user pagination chain changed or is incomplete",
            ));
        }
    }
    if !complete_chain {
        return Err(err(
            "Allowed push-user inventory exceeds four complete pages",
        ));
    }
    users.sort_by_key(user_id);
    let listed = users.iter().any(|user| user == &actor);
    let mut observation = vec![
        ("actor".to_owned(), actor),
        ("users".to_owned(), Value::Array(users)),
    ];
    if !listed {
        observation.extend([
            ("eligible".to_owned(), Value::Bool(false)),
            (
                "reason".to_owned(),
                string("Authenticated GitHub user is not an explicitly allowed push user."),
            ),
            ("repair".to_owned(), string(PUSH_REPAIR)),
        ]);
    } else {
        observation.push(("eligible".to_owned(), Value::Bool(true)));
    }
    Ok(Value::Object(observation))
}

/// Observe an explicit user grant; API eligibility never bypasses Git policy.
pub fn restricted_push_actor(endpoint: &str, restrictions: &Value) -> Value {
    push_actor_observation(endpoint, restrictions).unwrap_or_else(|error| {
        fields([
            ("eligible", Value::Bool(false)),
            (
                "reason",
                string(format!(
                    "Explicit push-user eligibility is unverified: {}",
                    error.message()
                )),
            ),
            ("repair", string(PUSH_REPAIR)),
        ])
    })
}

/// Python `value > 0` for a decoded JSON number or boolean.
fn positive(value: &Value) -> Result<bool> {
    match value {
        Value::Int(number) => Ok(!number.as_str().starts_with('-') && number.as_str() != "0"),
        Value::Bool(flag) => Ok(*flag),
        Value::Float(number) => Ok(*number > 0.0),
        _ => Err(err(
            "Incomplete active branch rule inventory; publication remains pending",
        )),
    }
}
/// Python `mapping.get(key, {})` that must itself be a mapping.
fn mapping<'a>(value: &'a Value, key: &str) -> Result<Option<&'a Value>> {
    match value.get(key) {
        None => Ok(None),
        Some(found @ Value::Object(_)) => Ok(Some(found)),
        Some(_) => Err(err("Incomplete classic branch protection response")),
    }
}
/// Python `(mapping or {}).get(key) or {}`.
fn falsy_default<'a>(value: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>> {
    let Some(value) = value.filter(|v| truthy(v)) else {
        return Ok(None);
    };
    match value.get(key) {
        Some(found) if !truthy(found) => Ok(None),
        Some(found @ Value::Object(_)) => Ok(Some(found)),
        Some(_) => Err(err("Incomplete classic branch protection response")),
        None => Ok(None),
    }
}
fn enabled(value: &Value, key: &str) -> Result<bool> {
    Ok(mapping(value, key)?.and_then(|m| m.get("enabled")) == Some(&Value::Bool(true)))
}
fn items<'a>(value: Option<&'a Value>) -> Result<&'a [Value]> {
    match value {
        None => Ok(&[]),
        Some(Value::Array(rows)) => Ok(rows),
        Some(_) => Err(err("Unknown required status check identity")),
    }
}

type Check = (String, Option<i128>);
fn add_check(checks: &mut BTreeSet<Check>, context: &Value, app: Option<&Value>) -> Result<()> {
    let unknown = || err("Unknown required status check identity");
    let context = context.as_str().ok_or_else(unknown)?;
    if context.is_empty() || context.len() > 256 {
        return Err(unknown());
    }
    let app = match app {
        None | Some(Value::Null) => None,
        Some(Value::Int(number)) => {
            let number = number.as_i128().ok_or_else(unknown)?;
            if number < -1 {
                return Err(unknown());
            }
            (number != -1).then_some(number)
        }
        Some(_) => return Err(unknown()),
    };
    checks.insert((context.to_owned(), app));
    Ok(())
}

/// The complete observed repository/branch rule inventory for one landing.
pub fn policy(root: &Path, binding: &Value) -> Result<Value> {
    let name = repository(root)?;
    let endpoint = format!("repos/{name}");
    let branch_name = text(
        get(binding, "authority_branch"),
        "Current repository identity/default/permission does not permit publication",
    )?;
    let repo = api_value(&endpoint)?;
    let permitted = repo.as_object().is_some()
        && repo
            .get("full_name")
            .map_or(Some(""), Value::as_str)
            .is_some_and(|full| full.to_lowercase() == name.to_lowercase())
        && repo.get("default_branch").and_then(Value::as_str) == Some(branch_name)
        && mapping(&repo, "permissions")
            .ok()
            .flatten()
            .and_then(|p| p.get("push"))
            == Some(&Value::Bool(true))
        && repo.get("archived") == Some(&Value::Bool(false))
        && repo.get("disabled") == Some(&Value::Bool(false));
    if !permitted {
        return Err(err(
            "Current repository identity/default/permission does not permit publication",
        ));
    }
    let branch = quote(branch_name);
    let branch_data = api_value(&format!("{endpoint}/branches/{branch}"))?;
    let rules = collection(&format!("{endpoint}/rules/branches/{branch}"), None)?;
    let protected_flag = branch_data.get("protected").and_then(Value::as_bool);
    if branch_data.as_object().is_none()
        || protected_flag.is_none()
        || branch_data.get("name").and_then(Value::as_str) != Some(branch_name)
        || rules.iter().any(|rule| {
            rule.as_object().is_none()
                || !rule
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| RULE_TYPE.is_match(kind))
        })
    {
        return Err(err(
            "Incomplete active branch rule inventory; publication remains pending",
        ));
    }
    let branch_protected = protected_flag == Some(true);
    let rule_type = |rule: &Value| get(rule, "type").as_str().unwrap_or("").to_owned();
    let known = [
        "pull_request",
        "required_status_checks",
        "non_fast_forward",
        "deletion",
    ];
    let mut unsupported: Vec<String> = rules
        .iter()
        .map(rule_type)
        .filter(|kind| !known.contains(&kind.as_str()))
        .collect();
    let mut classic = None;
    let mut push_actor = Value::Null;
    if branch_protected {
        classic = api(
            &format!("{endpoint}/branches/{branch}/protection"),
            "GET",
            None,
            true,
        )?
        .map(|(value, _)| value);
        if classic.is_none() && rules.is_empty() {
            return Err(err(
                "Protected branch has no complete readable rule inventory",
            ));
        }
        if let Some(protection) = &classic {
            if protection.as_object().is_none() {
                return Err(err("Incomplete classic branch protection response"));
            }
            for key in [
                "required_linear_history",
                "required_signatures",
                "lock_branch",
            ] {
                if enabled(protection, key)? {
                    unsupported.push(key.to_owned());
                }
            }
            if let Some(restrictions) = protection
                .get("restrictions")
                .filter(|r| **r != Value::Null)
            {
                push_actor = restricted_push_actor(
                    &format!("{endpoint}/branches/{branch}/protection/restrictions/users"),
                    restrictions,
                );
                if get(&push_actor, "eligible") != &Value::Bool(true) {
                    unsupported.push("restricted-push-actor-unverified".to_owned());
                }
            }
        }
    }
    let requirements = falsy_default(classic.as_ref(), "required_pull_request_reviews")?;
    let mut review = match requirements {
        None => false,
        Some(requirements) => {
            positive(
                requirements
                    .get("required_approving_review_count")
                    .unwrap_or(&Value::Int(0.into())),
            )? || requirements.get("require_code_owner_reviews") == Some(&Value::Bool(true))
                || requirements.get("require_last_push_approval") == Some(&Value::Bool(true))
        }
    };
    let mut checks = BTreeSet::new();
    if let Some(required) = falsy_default(classic.as_ref(), "required_status_checks")? {
        for context in items(required.get("contexts"))? {
            add_check(&mut checks, context, None)?;
        }
        for check in items(required.get("checks"))? {
            let context = check
                .get("context")
                .ok_or_else(|| err("Unknown required status check identity"))?;
            add_check(&mut checks, context, check.get("app_id"))?;
        }
    }
    for rule in &rules {
        match rule_type(rule).as_str() {
            "pull_request" => {
                let parameters = match rule.get("parameters") {
                    None => Value::Object(vec![]),
                    Some(value @ Value::Object(_)) => value.clone(),
                    Some(_) => {
                        return Err(err(
                            "Incomplete active branch rule inventory; publication remains pending",
                        ));
                    }
                };
                if let Some(methods) = parameters.get("allowed_merge_methods") {
                    let merge = match methods {
                        Value::Array(rows) => rows.iter().any(|m| m.as_str() == Some("merge")),
                        Value::Object(pairs) => pairs.iter().any(|(k, _)| k == "merge"),
                        Value::String(s) => s.contains("merge"),
                        _ => {
                            return Err(err(
                                "Incomplete active branch rule inventory; publication remains pending",
                            ));
                        }
                    };
                    if !merge {
                        unsupported.push("pull-request-merge-method".to_owned());
                    }
                }
                let mut requires = positive(
                    parameters
                        .get("required_approving_review_count")
                        .unwrap_or(&Value::Int(0.into())),
                )? || parameters.get("require_code_owner_review")
                    == Some(&Value::Bool(true))
                    || parameters.get("require_last_push_approval") == Some(&Value::Bool(true));
                if !requires {
                    for reviewer in items(parameters.get("required_reviewers"))? {
                        let minimum = reviewer.get("minimum_approvals").ok_or_else(|| {
                            err("Incomplete active branch rule inventory; publication remains pending")
                        })?;
                        if positive(minimum)? {
                            requires = true;
                            break;
                        }
                    }
                }
                review |= requires;
            }
            "required_status_checks" => {
                let incomplete =
                    || err("Incomplete active branch rule inventory; publication remains pending");
                let listed = rule
                    .get("parameters")
                    .and_then(|p| p.get("required_status_checks"))
                    .and_then(Value::as_array)
                    .ok_or_else(incomplete)?;
                for check in listed {
                    let context = check.get("context").ok_or_else(incomplete)?;
                    add_check(&mut checks, context, check.get("integration_id"))?;
                }
            }
            _ => {}
        }
    }
    let protected = branch_protected
        || rules
            .iter()
            .any(|rule| !matches!(rule_type(rule).as_str(), "non_fast_forward" | "deletion"));
    if protected && repo.get("allow_merge_commit") != Some(&Value::Bool(true)) {
        unsupported.push("merge-commit-method-disabled".to_owned());
    }
    let mut ordered: Vec<&Check> = checks.iter().collect();
    ordered.sort_by(|(left, left_app), (right, right_app)| {
        (left, left_app.unwrap_or(0), left_app.is_some()).cmp(&(
            right,
            right_app.unwrap_or(0),
            right_app.is_some(),
        ))
    });
    let required_checks = Value::Array(
        ordered
            .into_iter()
            .map(|(context, app)| {
                fields([
                    ("context", string(context.clone())),
                    (
                        "app_id",
                        app.map_or(Value::Null, |a| {
                            json::Integer::new(&a.to_string())
                                .map(Value::Int)
                                .unwrap_or(Value::Null)
                        }),
                    ),
                ])
            })
            .collect(),
    );
    let conversation = match classic.as_ref().filter(|c| truthy(c)) {
        None => false,
        Some(protection) => mapping(protection, "required_conversation_resolution")?
            .and_then(|m| m.get("enabled"))
            .is_some_and(truthy),
    };
    let unsupported: BTreeSet<String> = unsupported.into_iter().collect();
    let mut sorted_rules = rules.clone();
    let mut keyed = Vec::with_capacity(sorted_rules.len());
    for rule in sorted_rules.drain(..) {
        keyed.push((encoded(&rule)?, rule));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let repo_fields = Value::Object(
        [
            "full_name",
            "default_branch",
            "permissions",
            "archived",
            "disabled",
            "allow_merge_commit",
        ]
        .iter()
        .map(|key| {
            (
                (*key).to_owned(),
                repo.get(key).cloned().unwrap_or(Value::Null),
            )
        })
        .collect(),
    );
    let observation = fields([
        ("repo", repo_fields),
        (
            "branch",
            fields([
                ("name", get(&branch_data, "name").clone()),
                ("protected", get(&branch_data, "protected").clone()),
            ]),
        ),
        (
            "rules",
            Value::Array(keyed.into_iter().map(|(_, rule)| rule).collect()),
        ),
        ("classic", classic.clone().unwrap_or(Value::Null)),
        ("push_actor", push_actor.clone()),
    ]);
    Ok(fields([
        ("repository", string(name)),
        ("protected", Value::Bool(protected)),
        ("review_required", Value::Bool(review)),
        ("push_actor", push_actor),
        ("required_checks", required_checks),
        (
            "conversation_resolution_required",
            Value::Bool(conversation),
        ),
        (
            "unsupported_rules",
            Value::Array(unsupported.into_iter().map(string).collect()),
        ),
        ("observation_sha256", string(sha(&encoded(&observation)?))),
    ]))
}

/// `agent-memory/publication-result-v1`; `values` override base fields in place.
pub fn result(operation: &str, state: &str, values: Value) -> Result<Value> {
    merged(
        fields([
            ("schema", string("agent-memory/publication-result-v1")),
            ("operation", string(operation)),
            ("sharing_state", string(state)),
            ("publication_verified", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
            ("original_index_writes_performed", Value::Bool(false)),
            ("feature_code_freshness_verified", Value::Bool(false)),
            ("local_memory_available", Value::Bool(true)),
        ]),
        values,
    )
}

/// sha256 of the original index bytes, or null when the index is absent.
pub fn index_bytes(root: &Path) -> Result<Value> {
    let location = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        false,
    )?
    .ok_or_else(|| err("Publication refuses a redirected original index"))?;
    let path = PathBuf::from(location);
    let parent = path
        .parent()
        .ok_or_else(|| err("Publication refuses a redirected original index"))?;
    let name = path
        .file_name()
        .ok_or_else(|| err("Publication refuses a redirected original index"))?;
    let io = |error: std::io::Error| err(format!("Original index observation failed: {error}"));
    let directory = Descriptor::open_directory(parent).map_err(io)?;
    let info = match directory.stat_at(name) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Value::Null),
        Err(error) => return Err(io(error)),
    };
    if info.is_symlink() {
        return Err(err("Publication refuses a redirected original index"));
    }
    let mut file = directory
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io)?;
    if !file.stat().map_err(io)?.is_regular() {
        return Err(err("Publication refuses a nonordinary original index"));
    }
    let mut hasher = Sha256::new();
    let mut block = vec![0u8; 65536];
    let mut measured = 0usize;
    loop {
        let amount = file.read_chunk(&mut block).map_err(io)?;
        if amount == 0 {
            break;
        }
        measured = measured.saturating_add(amount);
        if measured > MAX_INDEX_BYTES {
            return Err(err(
                "Original index exceeds its bounded publication observation",
            ));
        }
        hasher.update(
            block
                .get(..amount)
                .ok_or_else(|| io(std::io::Error::other("read")))?,
        );
    }
    Ok(string(crate::validation_memo::hex(&hasher.finalize())))
}

/// A private, uniquely named temporary directory removed after its body.
pub(crate) struct TemporaryDirectory {
    parent: Descriptor,
    name: OsString,
    path: PathBuf,
}
impl TemporaryDirectory {
    pub(crate) fn new(parent: &Path, prefix: &str) -> Result<Self> {
        let io =
            |error: std::io::Error| err(format!("Private publication workspace failed: {error}"));
        let directory = Descriptor::open_directory(parent).map_err(io)?;
        let name = OsString::from(format!("{prefix}{}", crate::host::uuid4()?));
        directory.create_directory(&name, 0o700).map_err(io)?;
        let path = parent.join(&name);
        Ok(Self {
            parent: directory,
            name,
            path,
        })
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    fn remove(&self) -> std::io::Result<()> {
        let inner = self.parent.open_directory_at(&self.name)?;
        let entries = inner
            .entries_bounded(64)?
            .ok_or_else(|| std::io::Error::other("Unexpected private publication workspace"))?;
        for entry in entries {
            inner.unlink_at(&entry)?;
        }
        self.parent.remove_directory_at(&self.name)
    }
    /// Remove the workspace; a cleanup failure never replaces the body's refusal.
    pub(crate) fn finish<T>(self, outcome: Result<T>) -> Result<T> {
        let removed = self.remove();
        let value = outcome?;
        removed.map_err(|error| {
            err(format!(
                "Private publication workspace cleanup failed: {error}"
            ))
        })?;
        Ok(value)
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if self.parent.stat_at(&self.name).is_ok() {
            let _ = self.remove();
        }
    }
}

/// Commit the exact reviewed files over the fresh authority tree.
pub fn candidate(
    root: &Path,
    binding: &Value,
    files: &Value,
    quarantine_records: bool,
) -> Result<Option<String>> {
    let head = text(
        get(binding, "authority_head"),
        "Publication binding is incomplete",
    )?;
    let prefix = text(
        get(binding, "bank_root"),
        "Publication binding is incomplete",
    )?;
    let before = recovery::tree(root, head, prefix, quarantine_records)?;
    if same(files, &before)? {
        return Ok(None);
    }
    // This index contains the fresh authority tree, not the feature HEAD/index.
    // Desired blobs are read from the exact reviewed result, never git add -A.
    let (temporary_root, _) =
        writes::state_paths(root, &root.join(prefix), &crate::host::uuid4()?)?;
    crate::local_registry::private_mkdir(&temporary_root)?;
    let workspace = TemporaryDirectory::new(&temporary_root, "publication-")?;
    let index = workspace.path().join("index");
    let outcome = (|| {
        let local = GitWrite {
            index: Some(&index),
            ..GitWrite::default()
        };
        git_write(root, &["read-tree", head], local)?;
        let files_map = files
            .as_object()
            .ok_or_else(|| err("Publication files must be a mapping"))?;
        let before_map = before
            .as_object()
            .ok_or_else(|| err("Publication files must be a mapping"))?;
        let paths: BTreeSet<&str> = files_map
            .iter()
            .chain(before_map)
            .map(|(path, _)| path.as_str())
            .collect();
        for path in paths {
            let relative = format!("{prefix}/{path}");
            match files.get(path) {
                None => {
                    git_write(
                        root,
                        &["update-index", "--force-remove", "--", &relative],
                        local,
                    )?;
                }
                Some(body) if Some(body) != before.get(path) => {
                    let body = text(body, "Publication file bodies must be text")?;
                    let (_, blob) = git_write(
                        root,
                        &["hash-object", "-w", "--stdin"],
                        GitWrite {
                            payload: Some(body.as_bytes()),
                            ..GitWrite::default()
                        },
                    )?;
                    let info = format!("100644,{},{relative}", blob.trim());
                    git_write(
                        root,
                        &["update-index", "--add", "--cacheinfo", &info],
                        local,
                    )?;
                }
                Some(_) => {}
            }
        }
        let (_, tree) = git_write(root, &["write-tree"], local)?;
        let signing = git(root, &["config", "--bool", "commit.gpgsign"], true)?;
        let message = format!(
            "Publish reviewed project memory\n\nSemantic-proof: {}",
            sha(&encoded(files)?)
        );
        let mut args = vec!["commit-tree", tree.trim(), "-p", head];
        if signing.as_deref() == Some("true") {
            args.push("-S");
        }
        args.extend(["-m", message.as_str()]);
        let (_, commit) = git_write(root, &args, GitWrite::default())?;
        Ok(commit)
    })();
    let commit = workspace.finish(outcome)?;
    Ok(Some(recovery::oid(commit.trim())?.to_owned()))
}

/// The registered semantic_sharing provider (typed refusal when absent).
fn semantic() -> Result<&'static dyn crate::sharing_hold_mapping::SemanticSharing> {
    crate::sharing_hold_mapping::semantic_sharing_provider()
}
fn require_complete(repo: &Path) -> Result<Value> {
    writes::recovery_frontier_provider()?.require_complete(repo, None, None)
}
fn quarantine(proof: &Value) -> bool {
    get(proof, "schema").as_str() == Some(PROOF_V2)
}

pub fn prepare(repo: &Path, proof: &Value) -> Result<Value> {
    require_complete(repo)?;
    let reviewed = semantic()?.check(repo, proof, None)?;
    let binding = get(get(proof, "judgment"), "binding");
    let root = PathBuf::from(text(
        get(binding, "repo_root"),
        "Publication proof binding is incomplete",
    )?);
    let original_index = index_bytes(&root)?;
    let observed_policy = policy(&root, binding)?;
    if truthy(get(&observed_policy, "unsupported_rules")) {
        return result(
            "prepare",
            "pending-policy",
            fields([
                (
                    "reason",
                    string("Repository rules need an explicit supported landing method"),
                ),
                (
                    "unsupported_rules",
                    get(&observed_policy, "unsupported_rules").clone(),
                ),
                (
                    "policy_failure",
                    get(&observed_policy, "push_actor").clone(),
                ),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        );
    }
    let proposed = candidate(
        &root,
        binding,
        get(&reviewed, "publish_files"),
        quarantine(proof),
    )?;
    semantic()?.final_sources(&root, binding, binding, None)?;
    if index_bytes(&root)? != original_index {
        return Err(err(
            "Original index changed during publication preparation; inspect without overwriting it",
        ));
    }
    let Some(proposed) = proposed else {
        return result(
            "prepare",
            "no-publishable-delta",
            fields([
                ("conflicts", get(&reviewed, "conflicts").clone()),
                ("blocked_paths", get(&reviewed, "blocked_paths").clone()),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        );
    };
    let request = fields([
        ("schema", string(SCHEMA)),
        ("repo_root", string(path_text(&root, "repository path")?)),
        ("project_id", get(binding, "project_id").clone()),
        ("base", get(binding, "authority_head").clone()),
        ("candidate", string(proposed)),
        ("proof", proof.clone()),
    ]);
    result(
        "prepare",
        "prepared",
        fields([
            ("request", request),
            ("repository_policy", observed_policy),
            ("conflicts", get(&reviewed, "conflicts").clone()),
            ("blocked_paths", get(&reviewed, "blocked_paths").clone()),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]),
    )
}

/// Validate the exact private publication request in its original repository;
/// returns the source proof binding.
pub fn request_value(root: &Path, value: &Value) -> Result<Value> {
    let exact = value.as_object().is_some_and(|pairs| {
        pairs.len() == FIELDS.len() && pairs.iter().all(|(key, _)| FIELDS.contains(&key.as_str()))
    });
    if !exact
        || get(value, "schema").as_str() != Some(SCHEMA)
        || get(value, "repo_root").as_str() != Some(path_text(root, "repository path")?)
    {
        return Err(err(
            "Use the exact private publication request in its original repository",
        ));
    }
    let proof = get(value, "proof");
    let binding = get(get(proof, "judgment"), "binding");
    let mismatch =
        || err("Publication request and source proof do not share the same project/base");
    if binding.as_object().is_none()
        || get(value, "project_id") != get(binding, "project_id")
        || get(value, "base") != get(binding, "authority_head")
        || get(binding, "repo_root").as_str() != Some(path_text(root, "repository path")?)
    {
        return Err(mismatch());
    }
    let candidate = text(
        get(value, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let base = text(
        get(value, "base"),
        "An immutable full Git object ID is required",
    )?;
    recovery::commit(root, candidate)?;
    recovery::commit(root, base)?;
    let parents = recovery::command(
        root,
        &["rev-list", "--parents", "-n", "1", candidate],
        false,
        recovery::MAX_BYTES,
        false,
    )?;
    if parents.split_whitespace().skip(1).collect::<Vec<_>>() != [base] {
        return Err(err(
            "Memory candidate must have only the freshly reviewed authority parent",
        ));
    }
    let prefix = text(
        get(binding, "bank_root"),
        "Publication binding is incomplete",
    )?;
    let after = recovery::tree(root, candidate, prefix, quarantine(proof))?;
    if get(&bank::validate_files(&after, None)?, "project_id") != get(value, "project_id")
        || !same(&tx::file_hashes(&after)?, get(proof, "publish_hashes"))?
    {
        return Err(err(
            "Actual candidate bank does not equal the exact reviewed publication result",
        ));
    }
    let changed = recovery::command(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--no-renames",
            "--name-only",
            "-r",
            "-z",
            base,
            candidate,
        ],
        false,
        recovery::MAX_BYTES,
        false,
    )?;
    let owned = format!("{prefix}/");
    let paths: Vec<&str> = changed.split('\0').collect();
    if !paths.iter().any(|path| !path.is_empty())
        || paths
            .iter()
            .any(|path| !path.is_empty() && !path.starts_with(&owned))
    {
        return Err(err(
            "Publication contains feature code or no actual memory delta",
        ));
    }
    let project = text(
        get(value, "project_id"),
        "Publication request project is invalid",
    )?;
    scope_binding::require(
        root,
        Some(project),
        RequireOptions {
            expected: Some(get(binding, "scope_context")),
            ..RequireOptions::default()
        },
    )?;
    Ok(binding.clone())
}

/// Source guard before any remote write: the semantic proof's final sources,
/// or the internally verified recovery publication context.
pub fn publication_sources(
    root: &Path,
    binding: &Value,
    recovery_context: Option<&RecoveryPublicationContext>,
) -> Result<()> {
    match recovery_context {
        None => semantic()?.final_sources(root, binding, binding, None),
        Some(context) => context.check(root, binding),
    }
}

/// Create/reuse a memory-only PR; never approve it or enable an unpinned merge.
pub fn publication_pr(
    root: &Path,
    request: &Value,
    observed_policy: &Value,
    recovery_context: Option<&RecoveryPublicationContext>,
) -> Result<Value> {
    let binding = get(get(get(request, "proof"), "judgment"), "binding");
    let name = text(
        get(observed_policy, "repository"),
        "Returned PR is not this repository memory-only candidate",
    )?;
    let owner = name.split('/').next().unwrap_or("");
    let project = text(
        get(request, "project_id"),
        "Publication request project is invalid",
    )?;
    let candidate = text(
        get(request, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let authority_branch = text(
        get(binding, "authority_branch"),
        "Publication binding is incomplete",
    )?;
    let remote = text(
        get(binding, "authority_remote"),
        "Publication binding is incomplete",
    )?;
    let branch = format!("agent-memory/{project}/{candidate}");
    let endpoint = format!("repos/{name}/pulls");
    let existing = api_value(&format!(
        "{endpoint}?state=all&base={}&head={}&per_page=2",
        quote(authority_branch),
        quote(&format!("{owner}:{branch}"))
    ))?;
    let Value::Array(existing) = existing else {
        return Err(err(
            "Publication PR identity is ambiguous; inspect before retry",
        ));
    };
    if existing.len() > 1 {
        return Err(err(
            "Publication PR identity is ambiguous; inspect before retry",
        ));
    }
    let value = match existing.into_iter().next() {
        Some(value) => value,
        None => {
            if recovery_context.is_some() {
                publication_sources(root, binding, recovery_context)?;
            }
            let refspec = format!("{candidate}:refs/heads/{branch}");
            git_write(
                root,
                &[
                    "push",
                    "--porcelain",
                    "--no-follow-tags",
                    "--recurse-submodules=no",
                    "--",
                    remote,
                    &refspec,
                ],
                GitWrite {
                    network: true,
                    ..GitWrite::default()
                },
            )?;
            publication_sources(root, binding, recovery_context)?;
            // A lost create response is recovered by the exact same head/base query.
            api_post(
                &endpoint,
                &fields([
                    ("title", string("Maintain shared project memory")),
                    ("head", string(branch.clone())),
                    ("base", string(authority_branch)),
                    ("body", string(PR_BODY)),
                ]),
            )?
        }
    };
    let full_name = |side: &str| {
        get(get(get(&value, side), "repo"), "full_name")
            .as_str()
            .map(str::to_lowercase)
    };
    let valid = value.as_object().is_some()
        && matches!(value.get("number"), Some(Value::Int(_)))
        && get(get(&value, "head"), "sha").as_str() == Some(candidate)
        && get(get(&value, "head"), "ref").as_str() == Some(branch.as_str())
        && get(get(&value, "base"), "ref").as_str() == Some(authority_branch)
        && full_name("base") == Some(name.to_lowercase())
        && full_name("head") == Some(name.to_lowercase());
    if !valid {
        return Err(err(
            "Returned PR is not this repository memory-only candidate",
        ));
    }
    Ok(value)
}

pub type StatusInventory = (BTreeMap<String, Value>, Vec<Value>);
pub fn check_inventory(name: &str, sha: &str) -> Result<StatusInventory> {
    let endpoint = format!("repos/{name}/commits/{}", recovery::oid(sha)?);
    let statuses = collection(&format!("{endpoint}/statuses"), None)?;
    let checks = collection(
        &format!("{endpoint}/check-runs?filter=latest"),
        Some("check_runs"),
    )?;
    let bad_status = statuses.iter().any(|status| {
        status.as_object().is_none()
            || status.get("context").and_then(Value::as_str).is_none()
            || !matches!(status.get("id"), Some(Value::Int(_)))
    });
    let bad_check = checks.iter().any(|check| {
        check.as_object().is_none()
            || check.get("head_sha").and_then(Value::as_str) != Some(sha)
            || check.get("name").and_then(Value::as_str).is_none()
    });
    if bad_status || bad_check {
        return Err(err("Incomplete or wrong-commit required check evidence"));
    }
    let mut latest: BTreeMap<String, Value> = BTreeMap::new();
    for status in statuses {
        let context = get(&status, "context").as_str().unwrap_or("").to_owned();
        let newer = latest
            .get(&context)
            .is_none_or(|current| user_id(&status) > user_id(current));
        if newer {
            latest.insert(context, status);
        }
    }
    Ok((latest, checks))
}

/// Within the last 7 days and at most 5 minutes in the future.
fn recent(value: &Value) -> bool {
    let Some(raw) = value.as_str() else {
        return false;
    };
    let normalized = string(raw.replace('Z', "+00:00"));
    let Ok((seconds, fraction)) = crate::lifecycle::time_value(&normalized) else {
        return false;
    };
    let nanos: i128 = fraction
        .chars()
        .chain(std::iter::repeat('0'))
        .take(9)
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    let at = i128::from(seconds) * 1_000_000_000 + nanos;
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return false;
    };
    let now = now.as_nanos() as i128;
    let day = 86_400 * 1_000_000_000i128;
    now - 7 * day <= at && at <= now + 300 * 1_000_000_000
}

pub fn required_checks_pass(
    name: &str,
    head: &str,
    merged: &str,
    requirements: &Value,
) -> Result<bool> {
    let requirements = requirements.as_array().unwrap_or(&[]);
    if requirements.is_empty() {
        return Ok(true);
    }
    let (merge_statuses, merge_checks) = check_inventory(name, merged)?;
    let (statuses, checks) = if !merge_statuses.is_empty() || !merge_checks.is_empty() {
        (merge_statuses, merge_checks)
    } else {
        check_inventory(name, head)?
    };
    for required in requirements {
        let context = get(required, "context").as_str().unwrap_or("");
        let app = get(required, "app_id");
        let matching: Vec<&Value> = checks
            .iter()
            .filter(|check| {
                get(check, "name").as_str() == Some(context)
                    && (*app == Value::Null || get(get(check, "app"), "id") == app)
            })
            .collect();
        let status = statuses.get(context);
        // A plain status does not establish a configured GitHub App identity.
        if matching.is_empty() && (status.is_none() || *app != Value::Null) {
            return Ok(false);
        }
        if let Some(status) = status {
            if get(status, "state").as_str() != Some("success")
                || !recent(get(status, "created_at"))
            {
                return Ok(false);
            }
        }
        if matching.iter().any(|check| {
            get(check, "status").as_str() != Some("completed")
                || !matches!(
                    get(check, "conclusion").as_str(),
                    Some("success" | "neutral" | "skipped")
                )
                || !recent(get(check, "completed_at"))
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `(proposed merge, pull request URL, pending reason)`.
pub type Protected = (Option<String>, String, Option<String>);

pub fn protected_candidate(
    root: &Path,
    request: &Value,
    observed_policy: &Value,
    recovery_context: Option<&RecoveryPublicationContext>,
) -> Result<Protected> {
    let pr = publication_pr(root, request, observed_policy, recovery_context)?;
    let number = get(&pr, "number").clone();
    let name = text(
        get(observed_policy, "repository"),
        "Returned PR is not this repository memory-only candidate",
    )?;
    let (owner, repo_name) = name
        .split_once('/')
        .filter(|(_, rest)| !rest.contains('/'))
        .ok_or_else(|| err("Returned PR is not this repository memory-only candidate"))?;
    let data = api_post(
        "graphql",
        &fields([
            ("query", string(GRAPHQL)),
            (
                "variables",
                fields([
                    ("owner", string(owner)),
                    ("repo", string(repo_name)),
                    ("number", number.clone()),
                ]),
            ),
        ]),
    )?;
    let incomplete = || err("Incomplete pull request rule evaluation");
    if data.as_object().is_none() || data.get("errors").is_some_and(truthy) {
        return Err(incomplete());
    }
    let current = data
        .get("data")
        .and_then(|d| d.get("repository"))
        .and_then(|r| r.get("pullRequest"))
        .filter(|c| c.as_object().is_some())
        .ok_or_else(incomplete)?;
    let binding = get(get(get(request, "proof"), "judgment"), "binding");
    let candidate = text(
        get(request, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let base = text(
        get(request, "base"),
        "An immutable full Git object ID is required",
    )?;
    let number_text = number
        .as_int()
        .map(|n| n.as_str().to_owned())
        .ok_or_else(incomplete)?;
    let url = format!("https://github.com/{name}/pull/{number_text}");
    let field = |key: &str| current.get(key).ok_or_else(incomplete);
    if field("state")?.as_str() != Some("OPEN")
        || truthy(field("isDraft")?)
        || field("headRefOid")?.as_str() != Some(candidate)
        || field("baseRefOid")?.as_str() != Some(base)
    {
        return Ok((
            None,
            url,
            Some("PR source/base changed or is not open; reconcile current authority".to_owned()),
        ));
    }
    let decision = field("reviewDecision")?;
    if matches!(
        decision.as_str(),
        Some("REVIEW_REQUIRED" | "CHANGES_REQUESTED")
    ) || (truthy(get(observed_policy, "review_required"))
        && decision.as_str() != Some("APPROVED"))
    {
        return Ok((None, url, Some("Required review remains pending; ask the detecting developer through the normal repository workflow".to_owned())));
    }
    let threads = field("reviewThreads")?;
    let checks = field("commits")?
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(incomplete)?;
    // Same left-to-right short-circuit as the baseline condition.
    let head_differs = || -> Result<bool> {
        let [single] = checks else {
            return Ok(true);
        };
        let oid = single
            .get("commit")
            .and_then(|c| c.get("oid"))
            .ok_or_else(incomplete)?;
        Ok(oid.as_str() != Some(candidate))
    };
    let more_pages = || -> Result<bool> {
        threads
            .get("pageInfo")
            .and_then(|p| p.get("hasNextPage"))
            .map(truthy)
            .ok_or_else(incomplete)
    };
    let unresolved = || -> Result<bool> {
        let rows = threads
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(incomplete)?;
        for row in rows {
            if !truthy(row.get("isResolved").ok_or_else(incomplete)?) {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if field("mergeStateStatus")?.as_str() != Some("CLEAN")
        || head_differs()?
        || more_pages()?
        || unresolved()?
    {
        return Ok((
            None,
            url,
            Some("Required checks, conversations or merge requirements remain pending".to_owned()),
        ));
    }
    // GitHub's generated merge must have this exact reviewed base and PR head.
    // A regular fast-forward push then rejects any competing authority advance.
    let remote = text(
        get(binding, "authority_remote"),
        "Publication binding is incomplete",
    )?;
    let reference = format!("refs/agent-memory/observations/{}", crate::host::uuid4()?);
    let outcome = (|| {
        let refspec = format!("refs/pull/{number_text}/merge:{reference}");
        recovery::command(
            root,
            &[
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "--no-recurse-submodules",
                "--",
                remote,
                &refspec,
            ],
            true,
            recovery::MAX_BYTES,
            false,
        )?;
        let merged =
            git(root, &["rev-parse", "--verify", &reference], false)?.ok_or_else(|| {
                err("Generated merge base/head advanced; repeat semantic reconciliation")
            })?;
        let parents = recovery::command(
            root,
            &["rev-list", "--parents", "-n", "1", &merged],
            false,
            recovery::MAX_BYTES,
            false,
        )?;
        if parents.split_whitespace().skip(1).collect::<Vec<_>>() != [base, candidate] {
            return Err(err(
                "Generated merge base/head advanced; repeat semantic reconciliation",
            ));
        }
        let tree = |revision: &str| {
            recovery::command(
                root,
                &["rev-parse", &format!("{revision}^{{tree}}")],
                false,
                recovery::MAX_BYTES,
                false,
            )
            .map(|value| value.trim().to_owned())
        };
        if tree(&merged)? != tree(candidate)? {
            return Err(err(
                "Generated merge changes more than the exact reviewed candidate tree",
            ));
        }
        if !required_checks_pass(
            name,
            candidate,
            &merged,
            get(observed_policy, "required_checks"),
        )? {
            return Ok((
                None,
                url.clone(),
                Some("Exact required status/check results or their configured App identities remain pending".to_owned()),
            ));
        }
        Ok((Some(merged), url.clone(), None))
    })();
    let cleanup = recovery::command(
        root,
        &["update-ref", "-d", &reference],
        false,
        recovery::MAX_BYTES,
        false,
    );
    match cleanup {
        Err(error) => Err(error),
        Ok(_) => outcome,
    }
}

/// HoldFailure, capacity and a missing provider stay typed; other source
/// changes become pending reconciliation.
fn propagates(error: &ContractError) -> bool {
    matches!(
        error.kind(),
        Kind::HoldFailure { .. } | Kind::BankCapacity { .. } | Kind::MissingProvider
    )
}

pub fn land(repo: &Path, request: &Value) -> Result<Value> {
    require_complete(repo)?;
    let (root, _) = writes::bank_path(repo)?;
    let binding = request_value(&root, request)?;
    let original_index = index_bytes(&root)?;
    let proof = get(request, "proof");
    let pin = fields([
        ("base", get(request, "base").clone()),
        ("candidate", get(request, "candidate").clone()),
    ]);
    // Inventory before any retry. A prior success stays historical if later
    // corrected; neither a new proof nor a duplicate push is needed for readback.
    let readback = recovery::replay(&root, &pin, quarantine(proof))?;
    if truthy(get(&readback, "intended_delta_landed")) {
        return result(
            "land",
            "already-landed",
            fields([
                ("publication_verified", Value::Bool(true)),
                (
                    "intended_delta_current",
                    get(&readback, "intended_delta_current").clone(),
                ),
                (
                    "witnessed_commit",
                    get(&readback, "witnessed_commit").clone(),
                ),
                ("remote_freshness_verified", Value::Bool(true)),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        );
    }
    let reviewed = match semantic()?.check(&root, proof, None) {
        Ok(reviewed) => reviewed,
        Err(error) if propagates(&error) => return Err(error),
        Err(_) => {
            return result(
                "land",
                "pending-reconciliation",
                fields([
                    (
                        "reason",
                        string(
                            "Sources or authority changed; inventory, deduplicate and review the new source set",
                        ),
                    ),
                    ("git_metadata_writes_performed", Value::Bool(true)),
                ]),
            );
        }
    };
    let observed_policy = policy(&root, &binding)?;
    let policy_pending = || {
        result(
            "land",
            "pending-policy",
            fields([
                (
                    "unsupported_rules",
                    get(&observed_policy, "unsupported_rules").clone(),
                ),
                (
                    "reason",
                    string("Repository requires an explicitly supported landing method"),
                ),
                (
                    "policy_failure",
                    get(&observed_policy, "push_actor").clone(),
                ),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        )
    };
    if truthy(get(&observed_policy, "unsupported_rules")) {
        return policy_pending();
    }
    let rules_pending = |reason: Option<String>, url: String| {
        result(
            "land",
            "pending-repository-rules",
            fields([
                ("reason", reason.map_or(Value::Null, string)),
                ("pull_request", string(url)),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        )
    };
    let candidate = text(
        get(request, "candidate"),
        "An immutable full Git object ID is required",
    )?;
    let protected = truthy(get(&observed_policy, "protected"));
    let mut proposed = candidate.to_owned();
    let mut pr_url = Value::Null;
    if protected {
        let (merged, url, reason) = protected_candidate(&root, request, &observed_policy, None)?;
        let Some(merged) = merged else {
            return rules_pending(reason, url);
        };
        proposed = merged;
        pr_url = string(url);
    }
    writes::fault("publication-before-final-review")?;
    // Repeat semantic comparison/deduplication and policy immediately before push.
    semantic()?.check(&root, proof, None)?;
    if !same(&policy(&root, &binding)?, &observed_policy)? || index_bytes(&root)? != original_index
    {
        return Err(err(
            "Repository rules or original index changed; inspect before publication",
        ));
    }
    if protected {
        let (merged, url, reason) = protected_candidate(&root, request, &observed_policy, None)?;
        let Some(merged) = merged else {
            return rules_pending(reason, url);
        };
        proposed = merged;
        pr_url = string(url);
    }
    semantic()?.final_sources(&root, &binding, &binding, None)?;
    writes::fault("publication-before-push")?;
    let remote = text(
        get(&binding, "authority_remote"),
        "Publication binding is incomplete",
    )?;
    let branch = text(
        get(&binding, "authority_branch"),
        "Publication binding is incomplete",
    )?;
    let refspec = format!("{proposed}:refs/heads/{branch}");
    let (code, _) = git_write(
        &root,
        &[
            "push",
            "--porcelain",
            "--no-follow-tags",
            "--recurse-submodules=no",
            "--",
            remote,
            &refspec,
        ],
        GitWrite {
            network: true,
            optional: true,
            ..GitWrite::default()
        },
    )?;
    writes::fault("publication-after-push")?;
    let readback = recovery::replay(&root, &pin, quarantine(proof))?;
    if index_bytes(&root)? != original_index {
        return Err(err(
            "Original feature index changed externally during publication; preserve and inspect it",
        ));
    }
    if !truthy(get(&readback, "intended_delta_landed")) {
        let reason = if code != 0 {
            "Push was rejected; API and Git transport may use different credentials, or repository rules/authority changed. Inspect both identities and current rules; preserve the candidate and local learning before retrying."
        } else {
            "Authority advanced or landing readback is unavailable; preserve the candidate and reconcile before retrying"
        };
        return result(
            "land",
            "pending-reconciliation",
            fields([
                ("reason", string(reason)),
                ("transport_exit", Value::Int(i64::from(code).into())),
                ("pull_request", pr_url),
                ("git_metadata_writes_performed", Value::Bool(true)),
            ]),
        );
    }
    result(
        "land",
        "landed",
        fields([
            ("publication_verified", Value::Bool(true)),
            (
                "witnessed_commit",
                get(&readback, "witnessed_commit").clone(),
            ),
            (
                "intended_delta_current",
                get(&readback, "intended_delta_current").clone(),
            ),
            ("pull_request", pr_url),
            ("remote_freshness_verified", Value::Bool(true)),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("blocked_paths", get(&reviewed, "blocked_paths").clone()),
            ("conflicts", get(&reviewed, "conflicts").clone()),
        ]),
    )
}

/// The `am publish` operation outcome before printing.
enum Failure {
    Output(ContractError),
    Other(ContractError),
}

fn run(
    operation: &str,
    repo: &Path,
    input: &Path,
    output: Option<&Path>,
    version2: bool,
    pack: Result<PathBuf>,
) -> std::result::Result<Value, Failure> {
    let pack = match output {
        Some(path) => {
            let pack = pack.map_err(Failure::Output)?;
            limits::preflight_private_output(path, repo, &pack).map_err(Failure::Output)?;
            Some(pack)
        }
        None => None,
    };
    let value = semantic()
        .and_then(|provider| provider.read_input(input, version2))
        .map_err(Failure::Other)?;
    let outcome = if operation == "prepare" {
        prepare(repo, &value)
    } else {
        land(repo, &value)
    }
    .map_err(Failure::Other)?;
    match (output, pack) {
        (Some(path), Some(pack)) => {
            limits::write_private(path, &outcome, repo, &pack).map_err(Failure::Other)
        }
        _ => Ok(outcome),
    }
}

fn failure_result(operation: &str, failure: Failure) -> Result<Value> {
    match failure {
        Failure::Other(error) if matches!(error.kind(), Kind::BankCapacity { .. }) => {
            let mut values = fields([
                ("reason", string(error.message())),
                ("git_metadata_writes_performed", Value::Null),
                ("remote_freshness_verified", Value::Bool(false)),
            ]);
            bank::merge(&mut values, &crate::bank_capacity::details(&error))?;
            result(operation, "pending-inspection", values)
        }
        Failure::Other(error) if matches!(error.kind(), Kind::HoldFailure { .. }) => {
            let (code, repair) = match error.kind() {
                Kind::HoldFailure { code, repair } => (code.clone(), repair.clone()),
                _ => (String::new(), String::new()),
            };
            result(
                operation,
                "pending-inspection",
                fields([
                    ("reason", string(error.message())),
                    ("failure_component", string("known-dispute-state")),
                    ("failure_code", string(code)),
                    ("repair", string(repair)),
                    ("git_metadata_writes_performed", Value::Null),
                    ("remote_freshness_verified", Value::Bool(false)),
                ]),
            )
        }
        Failure::Output(error) => result(
            operation,
            "pending-private-output",
            fields([
                ("failure_code", string("private-output-invalid")),
                ("reason", string(error.message())),
                ("repair", string(error.message())),
                ("git_metadata_writes_performed", Value::Bool(false)),
                ("remote_freshness_verified", Value::Bool(false)),
            ]),
        ),
        Failure::Other(_) => result(
            operation,
            "pending-inspection",
            fields([
                ("failure_code", string("publication-unverified")),
                (
                    "reason",
                    string(
                        "Unresolved, unsafe, offline, changed or interrupted publication; retain local memory and inspect before retry",
                    ),
                ),
                (
                    "repair",
                    string(
                        "Preserve local memory and original pending evidence; inspect before retrying.",
                    ),
                ),
                ("git_metadata_writes_performed", Value::Null),
                ("remote_freshness_verified", Value::Bool(false)),
            ]),
        ),
    }
}

/// Baseline `memory_publication.main`: pretty JSON and exit 0 on success,
/// compact JSON and exit 1 on every refusal.
pub fn command_result(
    operation: &str,
    repo: &Path,
    input: &Path,
    output: Option<&Path>,
    version2: bool,
    pack: Result<PathBuf>,
) -> CommandResult {
    crate::validation_memo::operation(|_| {
        match run(operation, repo, input, output, version2, pack) {
            Ok(value) => CommandResult::json(value, 0, json::INDENT2_ASCII),
            Err(failure) => match failure_result(operation, failure) {
                Ok(value) => CommandResult::json(value, 1, json::DEFAULT),
                Err(error) => CommandResult {
                    value: None,
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("{error}\n"),
                },
            },
        }
    })
}

/// `turn_sources::PublicationGate`: the real request validation, and
/// authority-mainline replay through `sharing_recovery`.
pub struct Gate;
impl crate::turn_sources::PublicationGate for Gate {
    fn request_value(&self, repo: &Path, request: &Value) -> Result<()> {
        request_value(repo, request).map(|_| ())
    }
    fn replay(&self, repo: &Path, request: &Value, quarantine_records: bool) -> Result<Value> {
        recovery::replay(repo, request, quarantine_records)
    }
}
