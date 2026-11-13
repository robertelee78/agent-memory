//! Bounded checkpoint evidence qualification; references never grant outside access.
use crate::codec::json::Value;
use crate::developer_identity::{digest, load};
use crate::error::{ContractError, Result};
use crate::fs::Descriptor;
use crate::lifecycle::{get, object, string};
use crate::{identity_git, scopes, unicode};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path};
use std::sync::{Arc, RwLock};
use unicode_normalization::UnicodeNormalization;

pub const SECTION: &str = "Checkpoint evidence observations";
pub const SCHEMA: &str = "agent-memory/checkpoint-evidence-v1";
pub const MAX_SOURCE: usize = 65536;

/// Offsets returned by the checkpoint parser are UTF-8 byte offsets into `body`.
pub trait Checkpoint: Send + Sync {
    fn headings(&self, body: &str) -> Result<Vec<(usize, usize, String)>>;
    fn fields(&self) -> Vec<String>;
    fn extra(&self) -> Vec<String>;
    fn list_text(&self, references: &[String]) -> String;
}
static CHECKPOINT: RwLock<Option<Arc<dyn Checkpoint>>> = RwLock::new(None);
pub fn register_checkpoint(provider: Option<Arc<dyn Checkpoint>>) {
    *CHECKPOINT.write().unwrap() = provider;
}
fn checkpoint() -> Result<Arc<dyn Checkpoint>> {
    CHECKPOINT
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| ContractError::new("Checkpoint dependency is not registered"))
}

fn invalid_authority(url: &str) -> bool {
    let authority = url
        .strip_prefix("//")
        .map(|s| s.split(['/', '?', '#']).next().unwrap_or(""));
    let Some(authority) = authority else {
        return false;
    };
    let left = authority.find('[');
    let right = authority.find(']');
    if left.is_some() != right.is_some() {
        return true;
    }
    if let (Some(l), Some(r)) = (left, right) {
        if r < l {
            return true;
        }
        let host = &authority[l + 1..r];
        if host.starts_with('v') {
            let Some((version, rest)) = host[1..].split_once('.') else {
                return true;
            };
            if version.is_empty()
                || !version.bytes().all(|b| b.is_ascii_hexdigit())
                || rest.is_empty()
            {
                return true;
            }
        } else {
            let (address, zone) = host
                .split_once('%')
                .map_or((host, None), |(a, z)| (a, Some(z)));
            if address.parse::<std::net::Ipv6Addr>().is_err()
                || zone.is_some_and(|z| z.is_empty() || z.contains('%'))
            {
                return true;
            }
        }
    }
    let stripped: String = authority
        .chars()
        .filter(|c| !matches!(c, '@' | ':' | '#' | '?'))
        .collect();
    let normalized: String = stripped.nfkc().collect();
    normalized != stripped
        && normalized
            .chars()
            .any(|c| matches!(c, '/' | '?' | '#' | '@' | ':'))
}

pub fn reference_path(reference: &str) -> (Option<String>, Option<&'static str>) {
    let mut value = reference;
    if let Some(rest) = reference.strip_prefix('[') {
        if let Some((label, target)) = rest.split_once("](") {
            if !label.contains(['\n', ']']) && target.ends_with(')') {
                let inner = &target[..target.len() - 1];
                if let Some(angle) = inner.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                    if !angle.is_empty() && !angle.contains(['<', '>']) {
                        value = angle;
                    }
                } else if !inner.is_empty()
                    && !inner
                        .chars()
                        .any(|c| c == '(' || c == ')' || c.is_whitespace())
                {
                    value = inner;
                }
            }
        }
    }
    if value.chars().any(|c| c < ' ') {
        return (None, Some("unavailable"));
    }
    // urlsplit strips leading C0 controls/spaces, and removes ASCII tab/newline (already refused).
    let url = value.trim_start_matches(|c: char| c <= ' ');
    let scheme = url.find(':').filter(|&i| {
        i > 0
            && url.as_bytes()[0].is_ascii_alphabetic()
            && url[..i]
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
            && !url[..i].contains(['/', '?', '#'])
    });
    let remainder = scheme.map_or(url, |i| &url[i + 1..]);
    if invalid_authority(remainder) {
        return (None, Some("unavailable"));
    }
    if let Some(i) = scheme {
        let s = url[..i].to_ascii_lowercase();
        return (
            None,
            Some(if s == "http" || s == "https" {
                "external-unverified"
            } else {
                "outside-uninspected"
            }),
        );
    }
    if value.starts_with('/') {
        return (None, Some("outside-uninspected"));
    }
    let path = url.split(['?', '#']).next().unwrap_or("");
    let decoded = crate::pack_validation::percent_decode(path);
    if Path::new(&decoded).is_absolute() {
        (None, Some("outside-uninspected"))
    } else {
        (Some(decoded), None)
    }
}
fn git_os(root: &Path, args: &[OsString], optional: bool) -> Result<Option<OsString>> {
    identity_git::git(root, args, optional, false).map_err(|e| ContractError::new(e.to_string()))
}
fn git(root: &Path, args: &[&str], optional: bool) -> Result<Option<OsString>> {
    git_os(
        root,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        optional,
    )
}
fn git_text(root: &Path, args: &[&str], optional: bool, what: &str) -> Result<Option<String>> {
    git(root, args, optional)?
        .map(|v| crate::host::text(&v, what).map(str::to_owned))
        .transpose()
}

pub fn read_inside(
    root: &Path,
    bank: &Path,
    relative: &str,
) -> Result<(Option<Vec<u8>>, Option<OsString>, Option<&'static str>)> {
    let unavailable = || (None, None, Some("unavailable"));
    if Path::new(relative).is_absolute() {
        return Ok((None, None, Some("outside-uninspected")));
    }
    let bank_rel = match bank.strip_prefix(root) {
        Ok(p) => p,
        Err(_) => return Ok((None, None, Some("outside-uninspected"))),
    };
    let mut parts: Vec<OsString> = bank_rel
        .components()
        .chain(Path::new(relative).components())
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_owned()),
            Component::ParentDir => Some(OsString::from("..")),
            _ => None,
        })
        .collect();
    let opened = if let Some(name) = root.file_name() {
        let parent = root
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        crate::fs::canonical_path(parent, true)
            .ok()
            .and_then(|p| Descriptor::open_directory(&p).ok())
            .and_then(|d| d.open_directory_at(name).ok())
    } else {
        Descriptor::open_directory(root).ok()
    };
    let mut dirs = match opened {
        Some(d) => vec![d],
        None => return Ok(unavailable()),
    };
    let mut names: Vec<OsString> = Vec::new();
    let count = parts.len();
    for (index, part) in parts.drain(..).enumerate() {
        if part == ".." {
            if dirs.len() == 1 {
                return Ok((None, None, Some("outside-uninspected")));
            }
            dirs.pop();
            names.pop();
            continue;
        }
        let name = part.as_bytes();
        if name.starts_with(b".") || name.iter().any(|c| *c < 32 || *c == 127) {
            return Ok(unavailable());
        }
        let final_part = index + 1 == count;
        if !final_part {
            match dirs.last().unwrap().open_directory_at(&part) {
                Ok(d) => {
                    dirs.push(d);
                    names.push(part);
                }
                Err(_) => return Ok(unavailable()),
            }
            continue;
        }
        let mut path = std::path::PathBuf::new();
        for item in names.iter().chain(std::iter::once(&part)) {
            path.push(item);
        }
        if path.starts_with(bank_rel) {
            return Ok((None, None, Some("canonical-memory-reference")));
        }
        let args: Vec<OsString> = [
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "--error-unmatch",
            "-z",
            "--",
        ]
        .into_iter()
        .map(OsString::from)
        .chain(std::iter::once(path.as_os_str().to_owned()))
        .collect();
        let allowed = git_os(root, &args, true)?;
        if !allowed.as_ref().is_some_and(|s| {
            s.as_bytes()
                .split(|b| *b == 0)
                .any(|s| s == path.as_os_str().as_bytes())
        }) {
            return Ok(unavailable());
        }
        let mut file =
            match dirs
                .last()
                .unwrap()
                .open_at(&part, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            {
                Ok(d) => d,
                Err(_) => return Ok(unavailable()),
            };
        let before = match file.stat() {
            Ok(v) => v,
            Err(_) => return Ok(unavailable()),
        };
        if !before.is_regular() || before.nlink != 1 || before.size > MAX_SOURCE as i64 {
            return Ok(unavailable());
        }
        let bytes = match file.read_bounded(MAX_SOURCE) {
            Ok(v) => v,
            Err(crate::fs::ReadError::SizeLimit { .. } | crate::fs::ReadError::SourceChanged) => {
                return Err(ContractError::new(
                    "Referenced evidence changed while being inspected",
                ));
            }
            Err(_) => return Ok(unavailable()),
        };
        let after = match file.stat() {
            Ok(v) => v,
            Err(_) => return Ok(unavailable()),
        };
        if (before.size, before.mtime_ns, before.ctime_ns)
            != (after.size, after.mtime_ns, after.ctime_ns)
        {
            return Err(ContractError::new(
                "Referenced evidence changed while being inspected",
            ));
        }
        return Ok((Some(bytes), Some(path.into_os_string()), None));
    }
    Ok(unavailable())
}
fn opt(v: Option<String>) -> Value {
    v.map(string).unwrap_or(Value::Null)
}
fn oid(v: &Value, lengths: &[usize]) -> bool {
    v.as_str().is_some_and(|s| {
        lengths.contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.iter().any(|(j, w)| k == j && equal(v, w)))
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| equal(a, b))
        }
        _ => a == b,
    }
}
fn fields(v: &Value, names: &[&str]) -> bool {
    v.as_object().is_some_and(|o| {
        o.len() == names.len() && o.iter().all(|(k, _)| names.contains(&k.as_str()))
    })
}

pub fn observe(repo: &Path, bank: &Path, references: &[String]) -> Result<Value> {
    // Git output remains raw bytes until a contract field requires text.
    let head = git_text(
        repo,
        &["rev-parse", "--verify", "HEAD"],
        true,
        "checkpoint HEAD",
    )?;
    let branch = git_text(
        repo,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        true,
        "checkpoint branch",
    )?;
    let mut rows = Vec::new();
    for reference in references {
        let (relative, mut kind) = reference_path(reference);
        let (raw, path) = if let Some(relative) = relative {
            let (raw, path, k) = read_inside(repo, bank, &relative)?;
            kind = k;
            (raw, path)
        } else {
            (None, None)
        };
        let mut hash = Value::Null;
        let mut size = Value::Null;
        let mut commit = Value::Null;
        let mut blob_value = Value::Null;
        if let Some(raw) = raw {
            hash = opt(digest(Some(&raw)));
            size = Value::Int(raw.len().into());
            if let (Some(head), Some(path)) = (&head, &path) {
                let mut spec = OsString::from(format!("{head}:"));
                spec.push(path);
                let args = [
                    OsString::from("rev-parse"),
                    OsString::from("--verify"),
                    spec,
                ];
                if let Some(blob) = git_os(repo, &args, true)?
                    .map(|s| crate::host::text(&s, "checkpoint blob").map(str::to_owned))
                    .transpose()?
                {
                    if oid(&string(&blob), &[40, 64]) {
                        let len = git_text(
                            repo,
                            &["cat-file", "-s", &blob],
                            false,
                            "checkpoint blob size",
                        )?
                        .unwrap_or_default();
                        if !len.is_empty() && len.bytes().all(|b| b.is_ascii_digit()) {
                            if let Ok(n) = len.parse::<usize>() {
                                if n <= MAX_SOURCE {
                                    let mut committed =
                                        git(repo, &["cat-file", "blob", &blob], false)?
                                            .unwrap_or_default()
                                            .into_vec();
                                    if committed.len() + 1 == n {
                                        committed.push(b'\n');
                                    }
                                    if committed.len() == n && committed == raw {
                                        kind = Some("repository-committed");
                                        commit = string(head);
                                        blob_value = string(blob);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        rows.push(object([
            ("ref", string(reference)),
            ("kind", string(kind.unwrap_or("local-unpublished"))),
            ("sha256", hash),
            ("bytes", size),
            ("commit", commit),
            ("blob", blob_value),
        ]));
    }
    if git_text(
        repo,
        &["rev-parse", "--verify", "HEAD"],
        true,
        "checkpoint HEAD",
    )? != head
        || git_text(
            repo,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            true,
            "checkpoint branch",
        )? != branch
    {
        return Err(ContractError::new(
            "Checkout changed during evidence observation",
        ));
    }
    Ok(object([
        ("schema", string(SCHEMA)),
        ("references", Value::Array(rows)),
        (
            "observed_checkout",
            object([("head", opt(head)), ("branch", opt(branch))]),
        ),
        ("publication_verified", Value::Bool(false)),
        ("identity_authenticated", Value::Bool(false)),
    ]))
}
pub fn validate_observation(value: &Value) -> Result<&Value> {
    let err = |s| ContractError::new(s);
    if !fields(
        value,
        &[
            "schema",
            "references",
            "observed_checkout",
            "publication_verified",
            "identity_authenticated",
        ],
    ) || get(value, "schema").as_str() != Some(SCHEMA)
        || get(value, "publication_verified") != &Value::Bool(false)
        || get(value, "identity_authenticated") != &Value::Bool(false)
        || !get(value, "references")
            .as_array()
            .is_some_and(|a| (1..=16).contains(&a.len()))
        || !fields(get(value, "observed_checkout"), &["head", "branch"])
    {
        return Err(err("Unknown checkpoint evidence observation"));
    }
    let checkout = get(value, "observed_checkout");
    let head = get(checkout, "head");
    let branch = get(checkout, "branch");
    if (head != &Value::Null && !oid(head, &[40, 64]))
        || (branch != &Value::Null
            && !branch
                .as_str()
                .is_some_and(|s| s.chars().count() <= 4096 && !s.chars().any(|c| c < ' ')))
    {
        return Err(err("Invalid observed checkpoint checkout"));
    }
    for row in get(value, "references").as_array().unwrap() {
        if !fields(row, &["ref", "kind", "sha256", "bytes", "commit", "blob"])
            || !get(row, "ref").as_str().is_some_and(|s| s.len() <= 1024)
            || !matches!(
                get(row, "kind").as_str(),
                Some(
                    "repository-committed"
                        | "local-unpublished"
                        | "external-unverified"
                        | "outside-uninspected"
                        | "unavailable"
                        | "canonical-memory-reference"
                )
            )
        {
            return Err(err("Invalid checkpoint evidence reference"));
        }
        let hash = get(row, "sha256");
        if hash != &Value::Null && !oid(hash, &[64]) {
            return Err(err("Invalid checkpoint evidence hash"));
        }
        if hash == &Value::Null {
            if ["bytes", "commit", "blob"]
                .iter()
                .any(|k| get(row, k) != &Value::Null)
            {
                return Err(err(
                    "Uninspected references cannot contain invented byte evidence",
                ));
            }
        } else if !matches!(get(row,"bytes"),Value::Int(n) if n.as_str().parse::<usize>().is_ok_and(|n|n<=MAX_SOURCE))
        {
            return Err(err("Invalid checkpoint evidence byte count"));
        }
        for k in ["commit", "blob"] {
            let v = get(row, k);
            if v != &Value::Null && !oid(v, &[40, 64]) {
                return Err(err("Invalid checkpoint committed evidence"));
            }
        }
        if get(row, "kind").as_str() == Some("repository-committed")
            && ["sha256", "commit", "blob"]
                .iter()
                .any(|k| get(row, k) == &Value::Null)
        {
            return Err(err(
                "Committed evidence requires its actual byte/blob observation",
            ));
        }
    }
    Ok(value)
}
pub fn check_pins(repo: &Path, bank: &Path, references: &[String], expected: &Value) -> Result<()> {
    validate_observation(expected)?;
    if !equal(&observe(repo, bank, references)?, expected) {
        return Err(ContractError::new(
            "Checkpoint source evidence changed; preserve the proposal and inspect/review again",
        ));
    }
    Ok(())
}

/// Returns UTF-8 byte offsets of reserved labels outside fenced/indented examples.
pub fn managed_cues(body: &str, names: &[String]) -> BTreeSet<usize> {
    let names: HashSet<String> = names.iter().map(|s| unicode::casefold(s)).collect();
    let mut cues = BTreeSet::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut start = 0;
    while start < body.len() {
        let mut end = body.len();
        for (i, c) in body[start..].char_indices() {
            if matches!(
                c,
                '\n' | '\r'
                    | '\u{b}'
                    | '\u{c}'
                    | '\u{1c}'
                    | '\u{1d}'
                    | '\u{1e}'
                    | '\u{85}'
                    | '\u{2028}'
                    | '\u{2029}'
            ) {
                end = start + i + c.len_utf8();
                if c == '\r' && body.as_bytes().get(end) == Some(&b'\n') {
                    end += 1;
                }
                break;
            }
        }
        let raw = body[start..end].trim_end_matches(['\r', '\n']);
        let trimmed = raw.trim_start_matches(' ');
        if raw.len() - trimmed.len() <= 3 {
            if let Some(marker) = trimmed.bytes().next().filter(|c| *c == b'`' || *c == b'~') {
                let n = trimmed.bytes().take_while(|c| *c == marker).count();
                if n >= 3 {
                    let rest = &trimmed[n..];
                    if let Some((c, m)) = fence {
                        if marker == c && n >= m && rest.trim().is_empty() {
                            fence = None;
                        }
                    } else if marker != b'`' || !rest.contains('`') {
                        fence = Some((marker, n));
                    }
                    start = end;
                    continue;
                }
            }
        }
        if fence.is_none() {
            let mut col = 0;
            for c in raw.chars() {
                if c == ' ' {
                    col += 1
                } else if c == '\t' {
                    col += 4 - col % 4
                } else {
                    break;
                }
            }
            if col < 4 {
                let mut title = String::new();
                let mut tail = raw;
                while let Some(pos) = tail.find('<') {
                    title.push_str(&tail[..pos]);
                    if let Some(close) = tail[pos + 1..].find('>') {
                        let inside = &tail[pos + 1..pos + 1 + close];
                        if !inside.contains('<') {
                            tail = &tail[pos + close + 2..];
                            continue;
                        }
                    }
                    title.push('<');
                    tail = &tail[pos + 1..];
                }
                title.push_str(tail);
                let mut title = title.trim_matches([' ', '\t']);
                title = title
                    .trim_start_matches('#')
                    .trim_start_matches([' ', '\t']);
                let no_space = title.trim_end_matches([' ', '\t']);
                let no_hash = no_space.trim_end_matches('#');
                if no_hash.len() != no_space.len()
                    && (no_hash.ends_with(' ') || no_hash.ends_with('\t'))
                {
                    title = no_hash.trim_end_matches([' ', '\t']);
                }
                title = title.trim_matches([' ', '\t', '*', '_', '~']);
                if names.contains(&unicode::casefold(title)) {
                    cues.insert(start);
                }
            }
        }
        start = end;
    }
    cues
}
pub fn sections_with(body: &str, checkpoint: &dyn Checkpoint) -> Result<BTreeMap<String, String>> {
    let mut expected: HashSet<String> = checkpoint
        .fields()
        .into_iter()
        .chain(checkpoint.extra())
        .collect();
    expected.insert(SECTION.to_owned());
    let cues = managed_cues(body, &expected.iter().cloned().collect::<Vec<_>>());
    if cues.is_empty() {
        return Ok(BTreeMap::new());
    }
    let parsed = checkpoint.headings(body)?;
    let starts: BTreeSet<usize> = parsed
        .iter()
        .filter(|(_, _, t)| expected.contains(t))
        .map(|(s, _, _)| *s)
        .collect();
    if cues != starts {
        return Err(ContractError::new(
            "Every reserved checkpoint label requires exact managed heading framing",
        ));
    }
    let mut found = BTreeMap::new();
    for (start, end, title) in parsed {
        if expected.contains(&title) {
            let section = body
                .get(start..end)
                .and_then(|s| s.split_once('\n'))
                .map(|(_, s)| s.trim())
                .ok_or_else(|| {
                    ContractError::new(
                        "Every reserved checkpoint label requires exact managed heading framing",
                    )
                })?;
            if found.insert(title, section.to_owned()).is_some() {
                return Err(ContractError::new("Duplicate checkpoint section"));
            }
        }
    }
    Ok(found)
}
pub fn sections(body: &str) -> Result<BTreeMap<String, String>> {
    sections_with(body, checkpoint()?.as_ref())
}
pub fn json_section(value: &str) -> Result<Value> {
    let inner = value
        .strip_prefix("```json\n")
        .and_then(|s| s.strip_suffix("\n```"))
        .ok_or_else(|| {
            ContractError::new("Checkpoint conditions/evidence require unambiguous JSON framing")
        })?;
    load(inner.as_bytes())
}
pub fn reusable_with(base: &str, current: &Value, provider: &dyn Checkpoint) -> Value {
    let prior = (|| -> Result<Value> {
        let sections = sections_with(base, provider)?;
        let v = json_section(sections.get(SECTION).map(String::as_str).unwrap_or(""))?;
        validate_observation(&v)?;
        Ok(v)
    })();
    if let Ok(prior) = prior {
        let identity = |v: &Value| -> Option<Vec<Value>> {
            get(v, "references")
                .as_array()?
                .iter()
                .map(|row| {
                    let fields = row.as_object()?;
                    let mut identity = Vec::new();
                    for key in ["ref", "kind", "sha256", "bytes", "blob"] {
                        let value = fields.iter().find(|(name, _)| name == key)?;
                        identity.push((key.to_owned(), value.1.clone()));
                    }
                    Some(Value::Object(identity))
                })
                .collect()
        };
        if let (Some(a), Some(b)) = (identity(&prior), identity(current)) {
            if equal(&Value::Array(a), &Value::Array(b)) {
                return prior;
            }
        }
    }
    current.clone()
}
pub fn reusable(base: &str, current: &Value) -> Result<Value> {
    Ok(reusable_with(base, current, checkpoint()?.as_ref()))
}
pub fn assess_with(
    body: &str,
    project: &Value,
    context: &Value,
    repo: &Path,
    bank: &Path,
    inspect_only: bool,
    provider: &dyn Checkpoint,
) -> Result<Value> {
    let qualified = (|| -> Result<_> {
        let found = sections_with(body, provider)?;
        if found.is_empty() {
            return Ok(None);
        }
        let conditions = scopes::conditions(
            &json_section(
                found
                    .get("Checkpoint conditions")
                    .map(String::as_str)
                    .unwrap_or(""),
            )?,
            true,
        )?;
        let decision = scopes::decide(&object([
            ("bank_project_id", project.clone()),
            ("project_id", project.clone()),
            ("conditions", conditions.clone()),
            ("context", context.clone()),
        ]))?;
        Ok(Some((found, conditions, decision)))
    })();
    let (found, conditions, decision) = match qualified {
        Ok(None) => {
            return Ok(object([
                ("managed_checkpoint", Value::Bool(false)),
                ("applicable", Value::Null),
                ("assignment", Value::Bool(false)),
                ("statement_applicability", string("unassessed orientation")),
            ]));
        }
        Err(_) => {
            return Ok(object([
                ("managed_checkpoint", Value::Bool(true)),
                ("applicable", Value::Bool(false)),
                ("assignment", Value::Bool(false)),
                ("conditions", Value::Null),
                (
                    "reasons",
                    Value::Array(vec![string("unqualified-checkpoint-framing")]),
                ),
            ]));
        }
        Ok(Some(v)) => v,
    };
    let mut pairs = vec![
        ("managed_checkpoint".to_owned(), Value::Bool(true)),
        ("applicable".into(), get(&decision, "applicable").clone()),
        ("assignment".into(), Value::Bool(false)),
        ("conditions".into(), conditions),
        ("reasons".into(), get(&decision, "reasons").clone()),
        (
            "evidence_status".into(),
            string("not-inspected-inapplicable"),
        ),
    ];
    if get(&decision, "applicable") == &Value::Bool(false) && !inspect_only {
        return Ok(Value::Object(pairs));
    }
    let evidence = (|| -> Result<Vec<Value>> {
        let stored = json_section(found.get(SECTION).map(String::as_str).unwrap_or(""))?;
        validate_observation(&stored)?;
        let rows = get(&stored, "references").as_array().unwrap();
        let refs: Vec<String> = rows
            .iter()
            .map(|r| get(r, "ref").as_str().unwrap().to_owned())
            .collect();
        if refs.iter().collect::<HashSet<_>>().len() != refs.len()
            || found.get("Checkpoint evidence").map(String::as_str)
                != Some(provider.list_text(&refs).as_str())
        {
            return Err(ContractError::new(
                "Evidence list differs from its historical observation",
            ));
        }
        let current = observe(repo, bank, &refs)?;
        Ok(rows
            .iter()
            .zip(get(&current, "references").as_array().unwrap())
            .map(|(old, new)| {
                let a = get(old, "sha256");
                let b = get(new, "sha256");
                let status = if a != &Value::Null && a == b {
                    "matching-local-bytes"
                } else if a != &Value::Null && b != &Value::Null {
                    "changed-local-bytes"
                } else if a != &Value::Null {
                    "unavailable-here"
                } else {
                    "original-byte-binding-unverified"
                };
                object([
                    ("ref", get(old, "ref").clone()),
                    ("recorded", old.clone()),
                    ("current", new.clone()),
                    ("qualification", string(status)),
                ])
            })
            .collect())
    })();
    match evidence {
        Ok(rows) => {
            pairs[5].1 = string("qualified-observations");
            let matches = rows
                .iter()
                .all(|r| get(r, "qualification").as_str() == Some("matching-local-bytes"));
            pairs.push(("evidence".into(), Value::Array(rows)));
            pairs.push(("evidence_bytes_match".into(), Value::Bool(matches)));
        }
        Err(e) if e.message().starts_with("Non-UTF-8 checkpoint ") => return Err(e),
        Err(_) => pairs[5].1 = string("historical-or-unqualified-evidence"),
    }
    Ok(Value::Object(pairs))
}
pub fn assess(
    body: &str,
    project: &Value,
    context: &Value,
    repo: &Path,
    bank: &Path,
    inspect_only: bool,
) -> Result<Value> {
    assess_with(
        body,
        project,
        context,
        repo,
        bank,
        inspect_only,
        checkpoint()?.as_ref(),
    )
}
