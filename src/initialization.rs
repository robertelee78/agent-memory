//! Evidence-led orientation, preserving existing memory and stronger authorities.
//!
//! Baseline `initialization.py` (`am init inspect|read|preview|apply`).
//! Proposed memory is checked with `security::ensure_safe`; writes go through
//! the bank transaction protocol, which re-validates through [`Validator`].

use crate::bank_contract as bank;
use crate::bank_transactions as transactions;
use crate::codec::json::{self, Value};
use crate::developer_identity::{load, sha256_hex};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, Descriptor};
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::repository_config;
use crate::scope_binding as identity;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/initialize-proposal-v1";
pub const RESULT_SCHEMA: &str = "agent-memory/initialization-result-v1";
pub const AWA_SCHEMA: &str = "agent-memory/awa-initialization-evidence-v1";
pub const EXCLUDED: [&str; 10] = [
    ".git",
    "memory-bank",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
];
pub const CODE: [&str; 15] = [
    ".py", ".js", ".ts", ".tsx", ".jsx", ".go", ".rs", ".c", ".h", ".cpp", ".java", ".rb", ".sh",
    ".swift", ".kt",
];
pub const MAX_SOURCES: usize = 64;
pub const MAX_FILE: usize = 65536;
pub const MAX_DISCOVERED: usize = 4096;
/// Bounded reader of a skill-collected AWA evidence file (baseline unbounded).
pub const MAX_AWA: usize = 1 << 20;
/// Directory nesting walked by bounded discovery.
pub const MAX_DEPTH: usize = bank::MAX_PHYSICAL_DEPTH;
const ORIENTATION_MAX: usize = 16384;
const OMITTED: &str = "No distinct useful source context was established during initialization";

fn err(message: impl Into<String>) -> ContractError {
    ContractError::new(message)
}

fn git_error(error: crate::identity_git::Error) -> ContractError {
    match error.contract() {
        Some(error) => error.clone(),
        None => err(error.to_string()),
    }
}

/// `identity_git.git(..., optional=True)` as text.
fn git(root: &Path, args: &[&str]) -> Result<Option<String>> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let output = crate::identity_git::git(root, &args, true, false).map_err(git_error)?;
    output
        .map(|value| {
            value
                .into_string()
                .map_err(|_| ContractError::non_utf8("Git output"))
        })
        .transpose()
}

fn text_value(value: Option<String>) -> Value {
    value.map_or(Value::Null, string)
}

fn path_text(path: &Path) -> Result<&str> {
    crate::host::text(path.as_os_str(), "repository path")
}

fn is_capacity(error: &ContractError) -> bool {
    matches!(error.kind(), Kind::BankCapacity { .. })
}

fn same(left: &Value, right: &Value) -> bool {
    crate::installation::same(left, right)
}

fn strings(value: &Value) -> Option<Vec<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(key, _)| key.as_str())
        .collect()
}

fn exact_keys(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some() && keys(value) == fields.iter().copied().collect()
}

fn controls(value: &str) -> bool {
    value
        .chars()
        .any(|c| (c as u32) < 32 || matches!(c, '\u{7f}' | '\u{85}' | '\u{2028}' | '\u{2029}'))
}

/// Python `len(str)`.
fn characters(value: &str) -> usize {
    value.chars().count()
}

/// Baseline `result`.
pub fn result(operation: &str, values: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut fields = vec![
        ("schema".to_owned(), string(RESULT_SCHEMA)),
        ("operation".to_owned(), string(operation)),
        ("source_assertions_verified".to_owned(), Value::Bool(false)),
        ("publication_verified".to_owned(), Value::Bool(false)),
    ];
    fields.extend(
        values
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    );
    Value::Object(fields)
}

/// Baseline `root_at`: the Git top level of an existing directory.
pub fn root_at(repo: &Path) -> Result<PathBuf> {
    let repo = fs::canonical_path(repo, false)?;
    let directory = crate::installation::is_directory(&repo).unwrap_or(false);
    if !directory {
        return Err(err(
            "Initialization requires an existing repository directory",
        ));
    }
    match git(&repo, &["rev-parse", "--show-toplevel"])? {
        Some(top) => fs::canonical_path(Path::new(&top), false),
        None => Ok(repo),
    }
}

/// Python `PurePath.suffix` of a final component, without its dot.
fn extension(name: &str) -> Option<&str> {
    let (_, extension) = name.trim_start_matches('.').rsplit_once('.')?;
    (!extension.is_empty()).then_some(extension)
}

fn suffix(name: &str) -> String {
    extension(name).map_or_else(String::new, |extension| format!(".{extension}"))
}

/// Python `PurePath.stem`.
fn stem(name: &str) -> &str {
    extension(name)
        .and_then(|extension| name.strip_suffix(extension))
        .and_then(|stem| stem.strip_suffix('.'))
        .unwrap_or(name)
}

/// Baseline `source_kind` over the components of a repository-relative path.
pub fn source_kind(parts: &[&str]) -> Option<&'static str> {
    let (name, parents) = parts.split_last()?;
    let extension = suffix(name).to_lowercase();
    let within = |names: &[&str]| {
        parents
            .iter()
            .any(|part| names.contains(&part.to_lowercase().as_str()))
    };
    if extension == ".md" || name.to_lowercase().starts_with("readme") {
        return Some(if within(&["adr", "adrs", "design", "prd", "rfcs"]) {
            "planning"
        } else {
            "documentation"
        });
    }
    if CODE.contains(&extension.as_str()) {
        let test = within(&["test", "tests", "__tests__", "spec", "specs"])
            || stem(name).starts_with("test_");
        return Some(if test { "test" } else { "code" });
    }
    None
}

fn components(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

/// Python `PurePosixPath(path).as_posix()`.
fn posix(path: &str) -> String {
    let absolute = path.starts_with('/');
    let parts: Vec<&str> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

fn is_link(path: &Path) -> Result<bool> {
    Ok(crate::installation::named(path)
        .map_err(|error| err(error.to_string()))?
        .is_some_and(|info| info.is_symlink()))
}

/// Baseline `read_source`: one explicit repository-relative source, read as
/// untrusted data.
pub fn read_source(repo: &Path, path: &Value) -> Result<Value> {
    let root = root_at(repo)?;
    let refused = || err("Read an explicit repository-relative source or canonical memory file");
    let path = path.as_str().ok_or_else(refused)?;
    let parts = components(path);
    let excluded = |part: &&str| EXCLUDED.contains(part) && *part != "memory-bank";
    if posix(path) != path
        || path.starts_with('/')
        || parts.is_empty()
        || controls(path)
        || parts
            .iter()
            .any(|part| matches!(*part, "." | "..") || part.starts_with('.'))
        || parts.iter().any(excluded)
    {
        return Err(refused());
    }
    let target = root.join(path);
    let mut node = target.clone();
    for _ in 0..parts.len() {
        if is_link(&node)? {
            return Err(err("Source links must not escape through symlinks"));
        }
        if !node.pop() {
            break;
        }
    }
    let regular = crate::installation::named(&target)
        .map_err(|error| err(error.to_string()))?
        .is_some_and(|info| {
            info.is_regular() && usize::try_from(info.size).is_ok_and(|size| size <= MAX_FILE)
        });
    if !regular {
        return Err(err("Source is absent or exceeds the bounded reader"));
    }
    let raw = match fs::read_source(&root, Path::new(path), MAX_FILE) {
        Ok(observation) => observation.bytes,
        Err(fs::ReadError::SizeLimit { .. }) => {
            return Err(err("Source is absent or exceeds the bounded reader"));
        }
        Err(error) => return Err(err(error.to_string())),
    };
    let content = String::from_utf8(raw).map_err(|_| ContractError::non_utf8("source content"))?;
    let sha = sha256_hex(content.as_bytes());
    Ok(result(
        "read",
        [
            ("ref", string(path)),
            ("sha256", string(sha)),
            ("content", string(content)),
            ("content_is_untrusted_data", Value::Bool(true)),
            ("writes_performed", Value::Bool(false)),
        ],
    ))
}

// ----- discovery -----

struct Discovery<'a> {
    root: &'a Path,
    bank: &'a Path,
    allowed: Option<&'a BTreeSet<String>>,
    output: Vec<Value>,
    gaps: Vec<String>,
    examined: usize,
}

enum Entry {
    Directory(OsString),
    File(OsString, fs::Identity),
}

/// Baseline `os.walk` classification: directories (also through a link,
/// later pruned as a link) or everything else.
fn classify(directory: &Descriptor, path: &Path, name: OsString) -> Result<Entry> {
    let info = directory
        .stat_at(&name)
        .map_err(|error| err(error.to_string()))?;
    if info.is_directory() {
        return Ok(Entry::Directory(name));
    }
    if info.is_symlink() {
        let target = fs::canonical_path(&path.join(&name), true).ok();
        let to_directory = target
            .and_then(|target| crate::installation::named(&target).ok().flatten())
            .is_some_and(|info| info.is_directory());
        if to_directory {
            return Ok(Entry::Directory(name));
        }
    }
    Ok(Entry::File(name, info))
}

impl Discovery<'_> {
    /// Returns `false` once a bound stops discovery.
    fn file(&mut self, directory: &Path, name: &OsStr, info: fs::Identity) -> Result<bool> {
        self.examined += 1;
        if self.examined > MAX_DISCOVERED {
            self.gaps.push(
                "Local source discovery hit its 4096-file bound; inspect additional relevant sources explicitly"
                    .to_owned(),
            );
            return Ok(false);
        }
        let path = directory.join(name);
        let relative = path
            .strip_prefix(self.root)
            .map_err(|_| err("Source discovery left its repository"))?;
        let parts: Option<Vec<&str>> = relative
            .components()
            .map(|part| match part {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect();
        let reference = parts.as_ref().map(|parts| parts.join("/"));
        let kind = match &parts {
            Some(parts) => source_kind(parts),
            // A name that is not UTF-8 is classified by its bytes alone.
            None => byte_kind(name),
        };
        let hidden = name.as_bytes().first() == Some(&b'.');
        let listed = match (self.allowed, &reference) {
            (Some(allowed), Some(reference)) => allowed.contains(reference),
            (Some(_), None) => false,
            (None, _) => true,
        };
        if kind.is_none() || hidden || info.is_symlink() || !info.is_regular() || !listed {
            return Ok(true);
        }
        if self.output.len() >= MAX_SOURCES {
            self.gaps.push(
                "Local source discovery hit its 64-source bound; inspect additional relevant sources explicitly"
                    .to_owned(),
            );
            return Ok(false);
        }
        let shown = reference
            .clone()
            .unwrap_or_else(|| relative.to_string_lossy().into_owned());
        if usize::try_from(info.size).map_or(true, |size| size > MAX_FILE) {
            self.gaps
                .push(format!("Source exceeds the bounded reader: {shown}"));
            return Ok(true);
        }
        let raw = match fs::read_source(directory, Path::new(name), MAX_FILE) {
            Ok(observation) => observation.bytes,
            Err(fs::ReadError::SizeLimit { .. }) => {
                self.gaps
                    .push(format!("Source exceeds the bounded reader: {shown}"));
                return Ok(true);
            }
            Err(error) => return Err(err(error.to_string())),
        };
        if std::str::from_utf8(&raw).is_err() {
            self.gaps.push(format!(
                "Non-UTF8 source requires explicit inspection: {shown}"
            ));
            return Ok(true);
        }
        let Some(reference) = reference.filter(|reference| !controls(reference)) else {
            self.gaps.push(
                "A source path cannot be represented safely; inspect it explicitly".to_owned(),
            );
            return Ok(true);
        };
        self.output.push(object([
            ("ref", string(reference)),
            ("kind", string(kind.unwrap_or("documentation"))),
            ("sha256", string(sha256_hex(&raw))),
            ("bytes", Value::Int(raw.len().into())),
        ]));
        Ok(true)
    }

    /// One `os.walk` level: files first (sorted), then pruned sorted subdirectories.
    fn directory(&mut self, path: &Path, depth: usize) -> Result<bool> {
        let descriptor =
            Descriptor::open_directory(path).map_err(|error| err(error.to_string()))?;
        let names = descriptor
            .entries_bounded(repository_config::MAX_DISCOVERY_ENTRIES)
            .map_err(|error| err(error.to_string()))?
            .ok_or_else(|| err("Repository source inventory exceeds its bound"))?;
        let mut directories = Vec::new();
        let mut files = Vec::new();
        for name in names {
            match classify(&descriptor, path, name)? {
                Entry::Directory(name) => directories.push(name),
                Entry::File(name, info) => files.push((name, info)),
            }
        }
        files.sort_by(|left, right| left.0.cmp(&right.0));
        for (name, info) in files {
            if !self.file(path, &name, info)? {
                return Ok(false);
            }
        }
        directories.sort();
        for name in directories {
            let child = path.join(&name);
            let excluded = name.to_str().is_some_and(|name| EXCLUDED.contains(&name));
            if excluded
                || name.as_bytes().first() == Some(&b'.')
                || is_link(&child)?
                || child == self.bank
            {
                continue;
            }
            if depth >= MAX_DEPTH {
                self.gaps.push(
                    "Local source discovery hit its directory depth bound; inspect deeper relevant sources explicitly"
                        .to_owned(),
                );
                continue;
            }
            if !self.directory(&child, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// `source_kind` for a final name that is not UTF-8 (its parents never select
/// planning/test when undecodable; the source itself is reported as a gap).
fn byte_kind(name: &OsStr) -> Option<&'static str> {
    let lossy = String::from_utf8_lossy(name.as_bytes()).into_owned();
    source_kind(&[lossy.as_str()])
}

fn inventory_names(root: &Path) -> Result<Option<BTreeSet<String>>> {
    let args: Vec<OsString> = [
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let listed = crate::identity_git::git(root, &args, true, false).map_err(git_error)?;
    Ok(listed.map(|listed| {
        listed
            .as_bytes()
            .split(|byte| *byte == 0)
            .filter_map(|name| std::str::from_utf8(name).ok())
            .map(str::to_owned)
            .collect()
    }))
}

/// Python `PurePath(ref).is_relative_to(base)` for relative POSIX paths.
fn relative_to(reference: &str, base: &str) -> bool {
    let reference = components(reference);
    let base = components(base);
    reference.len() >= base.len() && reference.iter().zip(&base).all(|(a, b)| a == b)
}

/// Baseline `local_sources`: a bounded orientation inventory, never an
/// exhaustive repository crawler.
pub fn local_sources(root: &Path, selected: &Value) -> Result<(Vec<Value>, Vec<String>)> {
    let canonical_bank = repository_config::bank_path(root)?;
    let allowed = inventory_names(root)?;
    if selected != &Value::Null {
        let refs = strings(selected).filter(|refs| {
            !refs.is_empty()
                && refs.len() <= MAX_SOURCES
                && refs.iter().collect::<BTreeSet<_>>().len() == refs.len()
        });
        let Some(refs) = refs else {
            return Err(err(
                "Explicit source selection must contain 1-64 unique repository paths",
            ));
        };
        let bank_root = repository_config::relative(root)?;
        let mut output = Vec::new();
        for reference in refs {
            if allowed
                .as_ref()
                .is_some_and(|allowed| !allowed.contains(reference))
            {
                return Err(err(
                    "Explicit source is ignored or outside the Git source inventory; inspect it without publishing private context",
                ));
            }
            let source = read_source(root, &string(reference))?;
            if relative_to(reference, &bank_root) {
                return Err(err(
                    "Existing memory is inventoried separately, never promoted into source authority",
                ));
            }
            let content = text(&source, "content").unwrap_or_default();
            let kind = source_kind(&components(reference)).unwrap_or("documentation");
            output.push(object([
                ("ref", string(reference)),
                ("kind", string(kind)),
                ("sha256", get(&source, "sha256").clone()),
                ("bytes", Value::Int(content.len().into())),
            ]));
        }
        return Ok((output, Vec::new()));
    }
    let mut discovery = Discovery {
        root,
        bank: &canonical_bank,
        allowed: allowed.as_ref(),
        output: Vec::new(),
        gaps: Vec::new(),
        examined: 0,
    };
    discovery.directory(root, 0)?;
    Ok((discovery.output, discovery.gaps))
}

// ----- AWA evidence -----

fn unavailable_awa(availability: &str) -> (Value, Vec<String>) {
    (
        object([
            ("availability", string(availability)),
            ("sources", Value::Array(Vec::new())),
            ("path", Value::Null),
            ("sha256", Value::Null),
        ]),
        vec![format!(
            "AWA project/task/milestone evidence is {availability}; use the AWA skill when available"
        )],
    )
}

/// Baseline `awa_evidence`: skill-collected AWA references for the same project.
pub fn awa_evidence(path: &Value, project_id: &Value) -> Result<(Value, Vec<String>)> {
    let path = match path {
        Value::Null => {
            let found =
                crate::diagnostics::which(&crate::installation::Environment::process(), "awa");
            return Ok(unavailable_awa(if found {
                "not-collected"
            } else {
                "unavailable"
            }));
        }
        Value::String(path) => path,
        _ => return Err(err("AWA evidence path must be a filesystem path")),
    };
    let path = fs::canonical_path(Path::new(path), true)?;
    let raw = crate::sharing_limits::read_bytes(&path, MAX_AWA)?;
    let value = load(&raw)?;
    let fields = ["schema", "project_id", "availability", "reason", "sources"];
    let valid = exact_keys(&value, &fields)
        && text(&value, "schema") == Some(AWA_SCHEMA)
        && get(&value, "project_id") == project_id
        && matches!(
            text(&value, "availability"),
            Some("available" | "unavailable")
        )
        && text(&value, "reason").is_some()
        && get(&value, "sources")
            .as_array()
            .is_some_and(|sources| sources.len() <= 64);
    if !valid {
        return Err(err(
            "AWA evidence requires the same project, explicit availability and a bounded skill-collected source list",
        ));
    }
    let mut seen = BTreeSet::new();
    for source in get(&value, "sources").as_array().unwrap_or(&[]) {
        let reference = get(source, "ref");
        let ok = exact_keys(source, &["kind", "ref"])
            && matches!(text(source, "kind"), Some("project" | "task" | "milestone"))
            && crate::authority::text(reference)
            && reference.as_str().is_some_and(|reference| {
                reference.starts_with("https://") && seen.insert(reference)
            });
        if !ok {
            return Err(err(
                "AWA sources require unique explicit project/task/milestone references",
            ));
        }
    }
    let available = text(&value, "availability") == Some("available");
    let reason = text(&value, "reason").unwrap_or_default();
    let has_sources = !seen.is_empty();
    if !available && (reason.trim().is_empty() || has_sources) {
        return Err(err(
            "Unavailable AWA evidence needs the observed reason, without invented work records",
        ));
    }
    let gaps = if available {
        Vec::new()
    } else {
        vec![format!("AWA evidence unavailable: {reason}")]
    };
    let mut fields = value.as_object().unwrap_or(&[]).to_vec();
    fields.push(("path".to_owned(), string(path_text(&path)?)));
    fields.push(("sha256".to_owned(), string(sha256_hex(&raw))));
    Ok((Value::Object(fields), gaps))
}

// ----- inspect -----

fn local_git_context(root: &Path) -> Result<Value> {
    let branch = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let head = git(root, &["rev-parse", "--verify", "HEAD"])?;
    Ok(object([
        ("branch", text_value(branch)),
        ("head", text_value(head)),
        ("sources_verified_on_main", Value::Bool(false)),
    ]))
}

/// Baseline `inspect`: bounded read-only inventory of existing memory, local
/// sources and AWA evidence.
pub fn inspect(repo: &Path, awa_path: &Value, selected: &Value) -> Result<Value> {
    let root = root_at(repo)?;
    let path = repository_config::bank_path(&root)?;
    bank::bank_root(&path)?;
    let info = crate::installation::named(&path).map_err(|error| err(error.to_string()))?;
    if info.is_some_and(|info| info.is_symlink()) {
        return Err(err(
            "Existing bank is a symlink; preserve it for explicit repair",
        ));
    }
    let mut existing = Value::Object(Vec::new());
    let mut bank_state = "absent";
    let mut project_id = Value::Null;
    let mut error = None;
    if let Some(info) = info {
        if !info.is_directory() {
            return Err(err(
                "Existing bank location is not a directory; preserve it",
            ));
        }
        let occupied = Descriptor::open_directory(&path)
            .and_then(|directory| directory.entries_bounded(1))
            .map_err(|error| err(error.to_string()))?
            .is_none_or(|names| !names.is_empty());
        if occupied {
            match crate::bank_writes::snapshot(&root, None) {
                Ok((checked, hashes)) => {
                    project_id = get(&checked, "project_id").clone();
                    existing = hashes;
                    bank_state = "usable";
                }
                Err(failure) if is_capacity(&failure) => return Err(failure),
                Err(failure) => {
                    bank_state = "needs-inspection";
                    error = Some(failure.message().to_owned());
                }
            }
        } else {
            bank_state = "empty";
        }
    }
    if project_id == Value::Null {
        match crate::project_identity::status(&root) {
            Ok(status) => project_id = get(&status, "project_id").clone(),
            Err(failure) if is_capacity(&failure) => return Err(failure),
            Err(_) => {}
        }
    }
    let (sources, mut gaps) = local_sources(&root, selected)?;
    for kind in ["documentation", "planning", "code", "test"] {
        if !sources
            .iter()
            .any(|source| text(source, "kind") == Some(kind))
        {
            gaps.push(format!(
                "No {kind} sources selected; inspect relevant evidence explicitly if available"
            ));
        }
    }
    let (awa, missing) = awa_evidence(awa_path, &project_id)?;
    let context = local_git_context(&root)?;
    if let Some(error) = error {
        gaps.push(format!(
            "Existing memory needs inspection and will not be overwritten: {error}"
        ));
    }
    gaps.extend(missing);
    Ok(result(
        "inspect",
        [
            ("repo_root", string(path_text(&root)?)),
            ("project_id", project_id),
            ("bank_state", string(bank_state)),
            ("existing_hashes", existing),
            ("sources", Value::Array(sources)),
            ("source_selection", selected.clone()),
            ("awa", awa),
            (
                "evidence_gaps",
                Value::Array(gaps.into_iter().map(string).collect()),
            ),
            ("local_git_context", context),
            (
                "bounds",
                object([
                    ("max_sources", Value::Int(MAX_SOURCES.into())),
                    ("max_file_bytes", Value::Int(MAX_FILE.into())),
                    ("max_discovered_files", Value::Int(MAX_DISCOVERED.into())),
                ]),
            ),
            ("writes_performed", Value::Bool(false)),
        ],
    ))
}

// ----- orientation -----

/// Python `urllib.parse.quote(value, safe)`.
fn quote(value: &str, safe: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || b"_.-~".contains(&byte);
        if unreserved || safe.as_bytes().contains(&byte) {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

/// Baseline `link`: data-only labels and percent-encoded hrefs.
pub fn link(reference: &str, bank_root: &str) -> String {
    let label = reference
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let href = if reference.starts_with("https://") {
        quote(reference, "/:#?=&%")
    } else {
        let depth = components(bank_root).len();
        quote(&format!("{}{reference}", "../".repeat(depth)), "/")
    };
    format!("[{label}]({href})")
}

fn domain(fact: &str) -> Option<&'static Value> {
    get(&crate::authority::MODEL, "domains")
        .as_array()?
        .iter()
        .find(|row| text(row, "fact") == Some(fact))
}

fn is_bool(value: &Value) -> bool {
    matches!(value, Value::Bool(_))
}

fn known_sources(observed: &Value) -> BTreeMap<String, Value> {
    let mut known = BTreeMap::new();
    let local = get(observed, "sources").as_array().unwrap_or(&[]);
    let awa = get(get(observed, "awa"), "sources")
        .as_array()
        .unwrap_or(&[]);
    for source in local.iter().chain(awa) {
        if let Some(reference) = text(source, "ref") {
            known.insert(reference.to_owned(), source.clone());
        }
    }
    known
}

fn valid_claim(claim: &Value, known: &BTreeMap<String, Value>) -> bool {
    let fields = [
        "context",
        "fact",
        "fact_key",
        "summary",
        "value",
        "scope",
        "conditions",
        "evidence",
        "current",
        "applicable",
        "qualified",
        "authority_kind",
    ];
    let names = keys(claim);
    let mut extended: BTreeSet<&str> = fields.iter().copied().collect();
    let base = extended.clone();
    extended.insert("applicability_review");
    if claim.as_object().is_none() || (names != base && names != extended) {
        return false;
    }
    let context_known =
        text(claim, "context").is_some_and(|context| bank::CONTEXTS.contains(&context));
    let fact = text(claim, "fact").and_then(domain);
    let evidence = strings(get(claim, "evidence"));
    let evidence_ok = evidence.as_ref().is_some_and(|evidence| {
        !evidence.is_empty()
            && evidence.iter().collect::<BTreeSet<_>>().len() == evidence.len()
            && evidence
                .iter()
                .all(|reference| known.contains_key(*reference))
    });
    let summary_ok = crate::authority::text(get(claim, "summary"))
        && text(claim, "summary").is_some_and(|summary| characters(summary) <= 512);
    let flags = ["current", "applicable", "qualified"]
        .iter()
        .all(|key| is_bool(get(claim, key)));
    let authority_ok = fact.is_some_and(|row| {
        get(row, "source-kinds")
            .as_array()
            .unwrap_or(&[])
            .iter()
            .any(|kind| kind == get(claim, "authority_kind"))
    });
    context_known
        && fact.is_some()
        && crate::authority::text(get(claim, "fact_key"))
        && summary_ok
        && evidence_ok
        && flags
        && authority_ok
}

fn sorted_union<'a>(lists: impl Iterator<Item = &'a Value>) -> Value {
    let refs: BTreeSet<String> = lists
        .flat_map(|list| strings(list).unwrap_or_default())
        .map(str::to_owned)
        .collect();
    Value::Array(refs.into_iter().map(string).collect())
}

fn set_field(target: &mut Value, key: &str, value: Value) {
    if let Value::Object(fields) = target {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = value,
            None => fields.push((key.to_owned(), value)),
        }
    }
}

type Reviewed = (Vec<Value>, Vec<Value>, Vec<String>);

/// Baseline `review_value`: the exact inspected inventory and a bounded,
/// reviewed source interpretation.
pub fn review_value(
    review: &Value,
    observed: &Value,
    detecting_developer_id: &Value,
) -> Result<Reviewed> {
    let shape = exact_keys(
        review,
        &[
            "inventory",
            "existing_memory_reviewed",
            "claims",
            "questions",
        ],
    ) && same(get(review, "inventory"), observed)
        && is_bool(get(review, "existing_memory_reviewed"))
        && get(review, "claims")
            .as_array()
            .is_some_and(|claims| claims.len() <= 64)
        && get(review, "questions")
            .as_array()
            .is_some_and(|questions| questions.len() <= 16);
    if !shape {
        return Err(err(
            "Initialization needs the exact inspected inventory and a bounded reviewed source interpretation",
        ));
    }
    if text(observed, "bank_state") == Some("usable")
        && get(review, "existing_memory_reviewed") != &Value::Bool(true)
    {
        return Err(err(
            "Read existing applicable memory before creating orientation",
        ));
    }
    let known = known_sources(observed);
    let mut questions = Vec::new();
    let mut groups: Vec<((String, String, String, String), Vec<&Value>)> = Vec::new();
    let mut gaps = Vec::new();
    let context = get(observed, "local_git_context");
    for claim in get(review, "claims").as_array().unwrap_or(&[]) {
        if !valid_claim(claim, &known) {
            return Err(err(
                "Claims require explicit same-question authority, conditions and inspected source references",
            ));
        }
        if let Some(reason) = claim.get("applicability_review") {
            let ok = text(claim, "fact") == Some("implementation")
                && crate::authority::text(reason)
                && reason.as_str().is_some_and(|reason| {
                    characters(reason) <= 512 && !reason.chars().any(|c| (c as u32) < 32)
                });
            if !ok {
                return Err(err(
                    "Implementation applicability review requires a bounded explicit semantic explanation",
                ));
            }
        }
        crate::scopes::parse_scope(get(claim, "scope"))?;
        crate::scopes::conditions(get(claim, "conditions"), true)?;
        crate::authority::comparable(get(claim, "value"))?;
        let conditions = get(claim, "conditions");
        let unresolved = !["current", "applicable", "qualified"]
            .iter()
            .all(|key| get(claim, key) == &Value::Bool(true))
            || conditions
                .as_object()
                .unwrap_or(&[])
                .iter()
                .any(|(_, value)| value == &Value::Null);
        let fact = text(claim, "fact").unwrap_or_default();
        let fact_key = text(claim, "fact_key").unwrap_or_default();
        if unresolved {
            gaps.push(format!("Unresolved source interpretation: {fact_key}"));
            continue;
        }
        let scope = crate::scopes::parse_scope(get(claim, "scope"))?;
        if fact == "implementation" {
            let branch = get(context, "branch");
            let branch_scope = scope.as_ref().is_some_and(|(kind, _)| kind == "branch");
            let conditioned =
                conditions.get("branch").is_some() || conditions.get("revision").is_some();
            if !(branch_scope || conditioned || claim.get("applicability_review").is_some()) {
                return Err(err(
                    "Local implementation observations need branch/revision conditions or an explicit semantic applicability review; initialization does not prove main publication",
                ));
            }
            let scope_mismatch = scope.as_ref().is_some_and(|(kind, value)| {
                kind == "branch" && (branch == &Value::Null || value != branch)
            });
            let condition_mismatch = conditions
                .get("branch")
                .is_some_and(|value| branch == &Value::Null || value != branch);
            if scope_mismatch || condition_mismatch {
                return Err(err(
                    "Implementation branch condition must match the known inspected source branch",
                ));
            }
            let revision = get(context, "head");
            if conditions
                .get("revision")
                .is_some_and(|value| revision == &Value::Null || value != revision)
            {
                return Err(err(
                    "Implementation revision condition must match the known inspected source revision",
                ));
            }
        }
        // Code or work records alone cannot qualify approved product intent.
        let kinds: BTreeSet<&str> = strings(get(claim, "evidence"))
            .unwrap_or_default()
            .iter()
            .filter_map(|reference| {
                known
                    .get(*reference)
                    .and_then(|source| text(source, "kind"))
            })
            .collect();
        let has = |options: &[&str]| options.iter().any(|kind| kinds.contains(kind));
        let unsupported = (matches!(fact, "intent" | "requirement")
            && !has(&["planning", "documentation"]))
            || (fact == "implementation" && !has(&["code", "test"]))
            || (fact == "delivery" && !has(&["project", "task", "milestone"]));
        if unsupported {
            return Err(err(
                "Source domain cannot establish this fact; distinguish intent, implementation and planned delivery",
            ));
        }
        let key = (
            fact.to_owned(),
            fact_key.to_owned(),
            crate::authority::comparable(get(claim, "scope"))?,
            crate::authority::comparable(conditions)?,
        );
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, group)) => group.push(claim),
            None => groups.push((key, vec![claim])),
        }
    }
    let mut claims = Vec::new();
    for ((fact, fact_key, _, _), group) in groups {
        let mut values = BTreeSet::new();
        for claim in &group {
            values.insert(crate::authority::comparable(get(claim, "value"))?);
        }
        let evidence = sorted_union(group.iter().map(|claim| get(claim, "evidence")));
        if values.len() > 1 {
            questions.push(object([
                (
                    "question",
                    string(format!(
                        "Which applicable current source resolves {fact_key}?"
                    )),
                ),
                ("fact", string(fact)),
                ("fact_key", string(fact_key)),
                ("evidence", evidence),
                ("detecting_developer_id", detecting_developer_id.clone()),
            ]));
        } else if let Some(first) = group.first() {
            // Equivalent reviewed assertions form one concise orientation item.
            let mut claim = (*first).clone();
            set_field(&mut claim, "evidence", evidence);
            claims.push(claim);
        }
    }
    for item in get(review, "questions").as_array().unwrap_or(&[]) {
        let ok = exact_keys(item, &["question", "evidence"])
            && crate::authority::text(get(item, "question"))
            && text(item, "question").is_some_and(|question| characters(question) <= 512)
            && strings(get(item, "evidence"))
                .is_some_and(|refs| refs.iter().all(|reference| known.contains_key(*reference)));
        if !ok {
            return Err(err(
                "Product questions must be focused and point to inspected evidence, without invented answers",
            ));
        }
        let mut question = item.clone();
        set_field(
            &mut question,
            "detecting_developer_id",
            detecting_developer_id.clone(),
        );
        questions.push(question);
    }
    Ok((claims, questions, gaps))
}

/// Baseline `implementation_observation`: checkout context only.
pub fn implementation_observation(claim: &Value, observed: &Value) -> Value {
    let sources = known_local(observed);
    let mut bound = Vec::new();
    for reference in strings(get(claim, "evidence")).unwrap_or_default() {
        if let Some(source) = sources.get(reference) {
            bound.push((
                reference.to_owned(),
                object([
                    ("sha256", get(source, "sha256").clone()),
                    ("bytes", get(source, "bytes").clone()),
                ]),
            ));
        }
    }
    let context = get(observed, "local_git_context");
    object([
        ("sources", Value::Object(bound)),
        (
            "observed_checkout",
            object([
                ("branch", get(context, "branch").clone()),
                ("head", get(context, "head").clone()),
            ]),
        ),
        ("checkout_context_only", Value::Bool(true)),
        ("publication_verified", Value::Bool(false)),
    ])
}

fn known_local(observed: &Value) -> BTreeMap<String, Value> {
    get(observed, "sources")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|source| {
            text(source, "ref").map(|reference| (reference.to_owned(), source.clone()))
        })
        .collect()
}

fn backticks(value: &str) -> String {
    value.replace('`', "\\u0060")
}

/// Baseline `claim_lines`.
pub fn claim_lines(
    claim: &Value,
    bank_root: &str,
    observed: Option<&Value>,
) -> Result<Vec<String>> {
    let condition = json::encode(
        &object([
            ("scope", get(claim, "scope").clone()),
            ("conditions", get(claim, "conditions").clone()),
        ]),
        json::SORTED_UTF8,
    )?;
    let value = crate::authority::comparable(get(claim, "value"))?;
    let links: Vec<String> = strings(get(claim, "evidence"))
        .unwrap_or_default()
        .iter()
        .map(|reference| link(reference, bank_root))
        .collect();
    let mut lines = vec![
        text(claim, "summary").unwrap_or_default().to_owned(),
        String::new(),
        format!("Applicability: `{}`.", backticks(&condition)),
        format!("Reviewed value: `{}`.", backticks(&value)),
        format!("Evidence: {}.", links.join(", ")),
    ];
    if let (Some(observed), Some("implementation")) = (observed, text(claim, "fact")) {
        let provenance = json::encode(
            &implementation_observation(claim, observed),
            json::SORTED_UTF8,
        )?;
        lines.push(String::new());
        lines.push(format!(
            "Observation provenance: `{}`.",
            backticks(&provenance)
        ));
        if let Some(reason) = claim
            .get("applicability_review")
            .filter(|reason| crate::sharing_limits::truthy(reason))
        {
            let reason = json::encode(reason, json::UTF8)?;
            lines.push(format!("Applicability review: `{}`.", backticks(&reason)));
        }
    }
    Ok(lines)
}

fn heading(claim: &Value) -> String {
    format!(
        "Reviewed {}: {}",
        text(claim, "fact").unwrap_or_default(),
        text(claim, "fact_key").unwrap_or_default()
    )
}

/// Baseline `orientation`: bounded source-linked context bodies.
pub fn orientation(
    observed: &Value,
    claims: &[Value],
    questions: &[Value],
    gaps: &[String],
) -> Result<Vec<(String, String)>> {
    let repo_root = Path::new(text(observed, "repo_root").unwrap_or_default());
    let bank_root = repository_config::relative(repo_root)?;
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |path: &str, heading: &str, lines: Vec<String>| {
        if lines.is_empty() {
            return;
        }
        let body = format!("## {heading}\n\n{}", lines.join("\n"));
        match sections.iter_mut().find(|(name, _)| name == path) {
            Some((_, bodies)) => bodies.push(body),
            None => sections.push((path.to_owned(), vec![body])),
        }
    };
    let sources = get(observed, "sources").as_array().unwrap_or(&[]);
    for (kind, path, title) in [
        (
            "documentation",
            "projectbrief.md",
            "Documentation to inspect",
        ),
        (
            "planning",
            "systemPatterns.md",
            "Planning sources; approval and current applicability require review",
        ),
        (
            "code",
            "techContext.md",
            "Implementation sources; existence does not prove behavior",
        ),
        (
            "test",
            "techContext.md",
            "Executable tests; existence does not prove they pass",
        ),
    ] {
        let lines = sources
            .iter()
            .filter(|source| text(source, "kind") == Some(kind))
            .map(|source| {
                format!(
                    "- {}",
                    link(text(source, "ref").unwrap_or_default(), &bank_root)
                )
            })
            .take(8)
            .collect();
        add(path, title, lines);
    }
    let awa = get(get(observed, "awa"), "sources")
        .as_array()
        .unwrap_or(&[]);
    let lines = awa
        .iter()
        .map(|source| {
            format!(
                "- {}: {}",
                text(source, "kind").unwrap_or_default(),
                link(text(source, "ref").unwrap_or_default(), &bank_root)
            )
        })
        .take(16)
        .collect();
    add(
        "progress.md",
        "AWA projects, tasks and milestones; references do not prove delivery",
        lines,
    );
    for claim in claims {
        let lines = claim_lines(claim, &bank_root, Some(observed))?;
        add(
            text(claim, "context").unwrap_or_default(),
            &heading(claim),
            lines,
        );
    }
    let lines = questions
        .iter()
        .map(|question| {
            let refs: Vec<String> = strings(get(question, "evidence"))
                .unwrap_or_default()
                .iter()
                .map(|reference| link(reference, &bank_root))
                .collect();
            let evidence = if refs.is_empty() {
                String::new()
            } else {
                format!(" Evidence: {}.", refs.join(", "))
            };
            format!(
                "- {}{evidence}",
                text(question, "question").unwrap_or_default()
            )
        })
        .collect();
    add(
        "activeContext.md",
        "Product questions for the detecting developer",
        lines,
    );
    let observed_gaps = strings(get(observed, "evidence_gaps")).unwrap_or_default();
    let lines = observed_gaps
        .iter()
        .map(|gap| (*gap).to_owned())
        .chain(gaps.iter().cloned())
        .map(|gap| format!("- {gap}"))
        .collect();
    add(
        "activeContext.md",
        "Evidence gaps; no requirements inferred",
        lines,
    );
    let mut result = Vec::new();
    for (path, bodies) in sections {
        let title = path.strip_suffix(".md").unwrap_or(&path).to_owned();
        let body = format!("# {title}\n\n{}\n", bodies.join("\n\n"));
        if body.len() > ORIENTATION_MAX {
            return Err(err(
                "Orientation exceeds its context bound; tighten reviewed summaries and source selection",
            ));
        }
        result.push((path, body));
    }
    Ok(result)
}

/// The `(.+)` capture of `(?m)^Observation provenance: `(.+)`\.$`.
fn recorded_provenance(tail: &str) -> Option<&str> {
    const PREFIX: &str = "Observation provenance: `";
    tail.split('\n').find_map(|line| {
        let inner = line.strip_prefix(PREFIX)?.strip_suffix("`.")?;
        (!inner.is_empty()).then_some(inner)
    })
}

/// Baseline `new_claims`: reviewed assertions not already stored, plus gaps
/// for stored implementation observations whose source bytes differ.
pub fn new_claims(
    claims: Vec<Value>,
    files: &Value,
    bank_root: &str,
    observed: &Value,
) -> Result<(Vec<Value>, Vec<String>)> {
    let existing = files
        .as_object()
        .unwrap_or(&[])
        .iter()
        .filter(|(path, _)| path != "MEMORY.md")
        .map(|(_, body)| body.as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut fresh = Vec::new();
    let mut gaps = Vec::new();
    for claim in claims {
        let block = format!(
            "## {}\n\n{}",
            heading(&claim),
            claim_lines(&claim, bank_root, None)?.join("\n")
        );
        let Some((_, after)) = existing.split_once(&block) else {
            fresh.push(claim);
            continue;
        };
        if text(&claim, "fact") != Some("implementation") {
            continue;
        }
        let tail = after.split_once("\n## ").map_or(after, |(tail, _)| tail);
        let known = recorded_provenance(tail)
            .and_then(|recorded| load(recorded.as_bytes()).ok())
            .and_then(|previous| previous.get("sources").cloned());
        let current = get(&implementation_observation(&claim, observed), "sources").clone();
        let matches = known.as_ref().is_some_and(|known| same(known, &current));
        if !matches {
            let detail = if known.is_none() {
                "recorded byte binding is unknown"
            } else {
                "inspected source bytes differ"
            };
            gaps.push(format!(
                "Existing implementation observation needs evidence review: {}; {detail}",
                text(&claim, "fact_key").unwrap_or_default()
            ));
        }
    }
    Ok((fresh, gaps))
}

fn changed_contents(tx: &Value) -> Result<Value> {
    let files = get(tx, "files");
    let mut contents = Vec::new();
    for path in transactions::changed(tx)? {
        if let Some(body) = files.get(&path) {
            contents.push((path, body.clone()));
        }
    }
    Ok(Value::Object(contents))
}

fn ensure_safe(tx: &Value) -> Result<()> {
    crate::security::ensure_safe(&changed_contents(tx)?)
}

fn now() -> Result<Value> {
    Ok(string(crate::local_registry::now(
        crate::host::Sources::default(),
    )?))
}

fn strings_value(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}

/// Baseline `preview`: an exact initialization transaction, without writes.
pub fn preview(repo: &Path, review: &Value) -> Result<Value> {
    let inventory = get(review, "inventory");
    if review.as_object().is_none() || inventory.as_object().is_none() {
        return Err(err("Inspect and review evidence before initialization"));
    }
    let awa_path = match inventory.get("awa") {
        None => Value::Null,
        Some(awa @ Value::Object(_)) => get(awa, "path").clone(),
        Some(_) => return Err(err("Inspect and review evidence before initialization")),
    };
    let observed = inspect(repo, &awa_path, get(inventory, "source_selection"))?;
    if text(&observed, "bank_state") == Some("needs-inspection") {
        return Err(err(
            "Existing memory is invalid, busy or interrupted; inspect/recover it without overwriting",
        ));
    }
    let empty = matches!(text(&observed, "bank_state"), Some("empty" | "absent"));
    let root = PathBuf::from(text(&observed, "repo_root").unwrap_or_default());
    let context = identity::require(
        &root,
        None,
        identity::RequireOptions {
            initialize: empty,
            ..Default::default()
        },
    )?;
    let (claims, questions, mut gaps) =
        review_value(review, &observed, get(&context, "developer_id"))?;
    let no_local = get(&observed, "sources")
        .as_array()
        .is_none_or(<[Value]>::is_empty);
    let no_awa = get(get(&observed, "awa"), "sources")
        .as_array()
        .is_none_or(<[Value]>::is_empty);
    if no_local && no_awa {
        return Err(err(
            "No useful source evidence supplied; inspect documentation/code/tests/planning before creating a bank",
        ));
    }
    let bank_root = repository_config::relative(&root)?;
    let base = if empty {
        Value::Object(Vec::new())
    } else {
        transactions::files_at(&repository_config::bank_path(&root)?)?
    };
    let (claims, comparison) = new_claims(claims, &base, &bank_root, &observed)?;
    gaps.extend(comparison);
    let bodies = orientation(&observed, &claims, &questions, &gaps)?;
    let files = if empty {
        let mut proposal = object([
            ("project-id", get(&context, "project_id").clone()),
            ("router-updated", now()?),
            ("contexts", Value::Object(Vec::new())),
            ("omitted", Value::Object(Vec::new())),
        ]);
        let status = crate::project_identity::status(&root)?;
        let remote = get(get(&status, "evidence"), "canonical_remote");
        if remote != &Value::Null {
            set_field(&mut proposal, "canonical-remote", remote.clone());
        }
        let mut contexts = Vec::new();
        let mut omitted = Vec::new();
        for path in bank::CONTEXTS.iter() {
            match bodies.iter().find(|(name, _)| name == path) {
                Some((_, body)) => {
                    let mut fields = vec![("body".to_owned(), string(body))];
                    let route = bank::context_route(path)?;
                    fields.extend(
                        route
                            .as_object()
                            .unwrap_or(&[])
                            .iter()
                            .filter(|(key, _)| key != "path")
                            .cloned(),
                    );
                    contexts.push(((*path).to_owned(), Value::Object(fields)));
                }
                None => omitted.push(((*path).to_owned(), string(OMITTED))),
            }
        }
        set_field(&mut proposal, "contexts", Value::Object(contexts));
        set_field(&mut proposal, "omitted", Value::Object(omitted));
        get(&bank::render(&proposal)?, "files").clone()
    } else {
        let mut files = base.clone();
        let mut added = Vec::new();
        for (path, body) in &bodies {
            if base.get(path).is_some() {
                continue; // Existing prose, conditions, sources and provenance stay byte-identical.
            }
            set_field(&mut files, path, string(body));
            added.push(path.clone());
        }
        if !added.is_empty() {
            let router =
                text(&base, "MEMORY.md").ok_or_else(|| err("Existing bank router is missing"))?;
            let updated = bank::add_context_routes(router, &added, &now()?)?;
            set_field(&mut files, "MEMORY.md", string(updated));
        }
        files
    };
    let base_router = get(&base, "MEMORY.md").clone();
    let candidate = object([
        ("schema", string(transactions::SCHEMA)),
        ("operation_id", string(crate::host::uuid4()?)),
        ("operation", string("initialize")),
        ("repo_root", get(&observed, "repo_root").clone()),
        ("project_id", get(&context, "project_id").clone()),
        ("observed_hashes", get(&observed, "existing_hashes").clone()),
        ("files", files),
        ("scope_context", context.clone()),
        (
            "initialization_evidence",
            object([
                ("inventory", observed.clone()),
                (
                    "existing_memory_reviewed",
                    get(review, "existing_memory_reviewed").clone(),
                ),
                ("claims", get(review, "claims").clone()),
                ("questions", get(review, "questions").clone()),
                ("base_router", base_router.clone()),
            ]),
        ),
    ]);
    let tx = transactions::proposal_value(&candidate)?.clone();
    ensure_safe(&tx)?;
    let changed: Vec<Value> = transactions::changed(&tx)?
        .into_iter()
        .map(|path| string(format!("{bank_root}/{path}")))
        .collect();
    let proposal = object([
        ("schema", string(SCHEMA)),
        ("inventory", observed.clone()),
        ("review", review.clone()),
        ("transaction", tx),
        ("base_router", base_router),
    ]);
    let mut evidence_gaps: Vec<String> = strings(get(&observed, "evidence_gaps"))
        .unwrap_or_default()
        .into_iter()
        .map(str::to_owned)
        .collect();
    evidence_gaps.extend(gaps);
    Ok(result(
        "preview",
        [
            ("proposal", proposal),
            ("questions", Value::Array(questions)),
            ("evidence_gaps", strings_value(evidence_gaps)),
            ("changed_memory_paths", Value::Array(changed)),
            ("existing_memory_preserved", Value::Bool(true)),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ],
    ))
}

/// Baseline `check_sources`: the inspected evidence is unchanged.
pub fn check_sources(inventory: &Value) -> Result<()> {
    let root = PathBuf::from(text(inventory, "repo_root").unwrap_or_default());
    let (sources, _) = local_sources(&root, get(inventory, "source_selection"))?;
    let awa = get(inventory, "awa");
    let (observed_awa, _) = awa_evidence(get(awa, "path"), get(inventory, "project_id"))?;
    let context = local_git_context(&root)?;
    if !same(&Value::Array(sources), get(inventory, "sources"))
        || !same(&observed_awa, awa)
        || !same(&context, get(inventory, "local_git_context"))
    {
        return Err(err(
            "Source evidence changed after initialization preview; inspect and review again",
        ));
    }
    Ok(())
}

/// Baseline `transaction_evidence`: the reviewed wrapper inside a transaction.
pub fn transaction_evidence(tx: &Value) -> Result<Value> {
    let proof = get(tx, "initialization_evidence");
    let fields = [
        "inventory",
        "existing_memory_reviewed",
        "claims",
        "questions",
        "base_router",
    ];
    if !exact_keys(proof, &fields) {
        return Err(err(
            "Initialization source proof is missing; preserve the journal and use rollback",
        ));
    }
    let review = object(
        [
            "inventory",
            "existing_memory_reviewed",
            "claims",
            "questions",
        ]
        .map(|key| (key, get(proof, key).clone())),
    );
    Ok(object([
        ("schema", string(SCHEMA)),
        ("inventory", get(proof, "inventory").clone()),
        ("review", review),
        ("transaction", tx.clone()),
        ("base_router", get(proof, "base_router").clone()),
    ]))
}

fn router_updated(router: &str) -> Result<Value> {
    let (metadata, _, _) = bank::parse_router(router)?;
    Ok(get(&metadata, "router-updated").clone())
}

/// Baseline `validate_transaction`: reconstruct the intended files from the
/// original inventory and review; refuse any substitution.
pub fn validate_transaction(tx: &Value) -> Result<Reviewed> {
    let p = transaction_evidence(tx)?;
    let inventory = get(&p, "inventory");
    let consistent = text(tx, "operation") == Some("initialize")
        && get(tx, "repo_root") == get(inventory, "repo_root")
        && get(tx, "project_id") == get(inventory, "project_id")
        && same(
            get(tx, "observed_hashes"),
            get(inventory, "existing_hashes"),
        );
    if !consistent {
        return Err(err(
            "Initialization proposal has a different operation/repository",
        ));
    }
    // Re-read source evidence before every attempt. Recovery preserves the exact
    // original transaction and never silently imports newer source interpretations.
    check_sources(inventory)?;
    let developer = get(get(tx, "scope_context"), "developer_id");
    let (claims, questions, mut gaps) = review_value(get(&p, "review"), inventory, developer)?;
    // Reconstruct the intended files with the original timestamp and inventory,
    // preventing a substituted transaction from replacing existing bank content.
    let original = get(inventory, "existing_hashes").as_object().unwrap_or(&[]);
    let files = get(tx, "files");
    let body_of = |path: &str| text(files, path).unwrap_or_default();
    let prior: Vec<(String, Value)> = original
        .iter()
        .map(|(path, _)| (path.clone(), string(body_of(path))))
        .collect();
    let preserved = |path: &str, sha: &Value| {
        path == "MEMORY.md" || sha.as_str() == Some(sha256_hex(body_of(path).as_bytes()).as_str())
    };
    if !original.iter().all(|(path, sha)| preserved(path, sha)) {
        return Err(err(
            "Initialization must preserve every existing memory file",
        ));
    }
    let repo_root = Path::new(text(tx, "repo_root").unwrap_or_default());
    let bank_root = repository_config::relative(repo_root)?;
    let (claims, comparison) = new_claims(claims, &Value::Object(prior), &bank_root, inventory)?;
    gaps.extend(comparison);
    let bodies = orientation(inventory, &claims, &questions, &gaps)?;
    let router = text(files, "MEMORY.md").ok_or_else(|| {
        err("Initial router must match the resolved identity and useful source-linked contexts")
    })?;
    let base_router = get(&p, "base_router");
    let original_names: BTreeSet<&str> = original.iter().map(|(path, _)| path.as_str()).collect();
    if !original.is_empty() {
        let base = base_router.as_str().filter(|base| {
            original
                .iter()
                .find(|(path, _)| path == "MEMORY.md")
                .is_some_and(|(_, sha)| sha.as_str() == Some(sha256_hex(base.as_bytes()).as_str()))
        });
        let Some(base) = base else {
            return Err(err(
                "Existing router differs from the inspected initialization base",
            ));
        };
        let added: Vec<String> = bodies
            .iter()
            .filter(|(path, _)| !original_names.contains(path.as_str()))
            .map(|(path, _)| path.clone())
            .collect();
        let expected = if added.is_empty() {
            base.to_owned()
        } else {
            bank::add_context_routes(base, &added, &router_updated(router)?)?
        };
        if router != expected {
            return Err(err(
                "Initialization may add missing context routes, never rewrite existing routes or identity",
            ));
        }
    } else if base_router != &Value::Null {
        return Err(err(
            "Empty-bank initialization must not invent prior memory",
        ));
    } else {
        let mut metadata = object([
            ("schema", string("agent-memory/v1")),
            ("project-id", get(tx, "project_id").clone()),
            ("router-updated", router_updated(router)?),
        ]);
        let (data, _) = crate::project_identity::observe()?;
        let project = text(tx, "project_id")
            .and_then(|id| get(&data, "projects").get(id))
            .ok_or_else(|| err("Initialization project is not registered"))?;
        let remote = get(project, "canonical_remote");
        if remote != &Value::Null {
            set_field(&mut metadata, "canonical-remote", remote.clone());
        }
        let mut routes = Vec::new();
        let mut omitted = Vec::new();
        for path in bank::CONTEXTS.iter() {
            if bodies.iter().any(|(name, _)| name == path) {
                routes.push(bank::context_route(path)?);
            } else {
                omitted.push(((*path).to_owned(), string(OMITTED)));
            }
        }
        if router != bank::router_text(&metadata, &routes, &Value::Object(omitted))? {
            return Err(err(
                "Initial router must match the resolved identity and useful source-linked contexts",
            ));
        }
    }
    let mut expected: BTreeSet<&str> = original_names.clone();
    expected.insert("MEMORY.md");
    expected.extend(bodies.iter().map(|(path, _)| path.as_str()));
    if keys(files) != expected {
        return Err(err(
            "Initialization file set differs from the preserved bank and useful reviewed contexts",
        ));
    }
    if !original.iter().all(|(path, sha)| preserved(path, sha)) {
        return Err(err(
            "Initialization must preserve every existing memory file",
        ));
    }
    for (path, body) in files.as_object().unwrap_or(&[]) {
        if path == "MEMORY.md" || original_names.contains(path.as_str()) {
            continue;
        }
        let reviewed = bodies
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, body)| body.as_str());
        if body.as_str() != reviewed {
            return Err(err(
                "New context differs from the reviewed source-linked orientation",
            ));
        }
    }
    ensure_safe(tx)?;
    Ok((claims, questions, gaps))
}

/// Create the bank directory and its missing parents below the repository
/// (baseline `mkdir(parents=True, exist_ok=True)`), never through a link.
fn make_bank(root: &Path, bank: &Path) -> Result<()> {
    let relative = bank
        .strip_prefix(root)
        .map_err(|_| err("Bank location changed after preview; preserve it"))?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(name) = part else {
            return Err(err("Bank location changed after preview; preserve it"));
        };
        let chain = fs::DirectoryChain::open(&current).map_err(|error| err(error.to_string()))?;
        match chain.directory().stat_at(name) {
            Ok(info) if info.is_directory() => {}
            Ok(_) => return Err(err("Bank location changed after preview; preserve it")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                chain
                    .directory()
                    .create_directory(name, 0o755)
                    .map_err(|error| err(error.to_string()))?;
                chain
                    .directory()
                    .fsync_directory()
                    .map_err(|error| err(error.to_string()))?;
            }
            Err(error) => return Err(err(error.to_string())),
        }
        current.push(name);
    }
    Ok(())
}

/// Baseline `apply`: revalidate the original evidence, then write through the
/// bank transaction protocol.
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = prepared.get("proposal").unwrap_or(prepared);
    let fields = [
        "schema",
        "inventory",
        "review",
        "transaction",
        "base_router",
    ];
    if !exact_keys(p, &fields) || text(p, "schema") != Some(SCHEMA) {
        return Err(err("Invalid initialization proposal"));
    }
    let tx = transactions::proposal_value(get(p, "transaction"))?.clone();
    if crate::authority::comparable(p)?
        != crate::authority::comparable(&transaction_evidence(&tx)?)?
    {
        return Err(err(
            "Initialization wrapper differs from the original transaction evidence",
        ));
    }
    let (_, questions, gaps) = validate_transaction(&tx)?;
    let root = PathBuf::from(text(&tx, "repo_root").unwrap_or_default());
    let bank_path = repository_config::bank_path(&root)?;
    bank::bank_root(&bank_path)?;
    let info = crate::installation::named(&bank_path).map_err(|error| err(error.to_string()))?;
    if info.is_some_and(|info| info.is_symlink() || !info.is_directory()) {
        return Err(err("Bank location changed after preview; preserve it"));
    }
    make_bank(&fs::canonical_path(&root, false)?, &bank_path)?;
    let receipt = transactions::apply(&tx)?;
    let verified = get(&receipt, "verified") == &Value::Bool(true)
        && text(&receipt, "outcome") == Some("finished")
        && get(&receipt, "still_current") == &Value::Bool(true);
    let mut evidence_gaps: Vec<String> = strings(get(get(p, "inventory"), "evidence_gaps"))
        .unwrap_or_default()
        .into_iter()
        .map(str::to_owned)
        .collect();
    evidence_gaps.extend(gaps);
    let writes = get(&receipt, "writes_performed").clone();
    Ok(result(
        "apply",
        [
            ("transaction", receipt),
            ("questions", Value::Array(questions)),
            ("evidence_gaps", strings_value(evidence_gaps)),
            ("existing_memory_preserved", Value::Bool(true)),
            ("writes_performed", writes),
            ("verified", Value::Bool(verified)),
            ("sharing", string("local-or-pending-sharing")),
        ],
    ))
}

/// `bank_transactions::TransactionValidator` for `initialize` transactions.
pub struct Validator;

impl transactions::TransactionValidator for Validator {
    fn validate_transaction(&self, proposal: &Value, _owned_lock: Option<&[u8]>) -> Result<()> {
        validate_transaction(proposal).map(|_| ())
    }
    fn check_sources(&self, proposal: &Value, _owner: &[u8], _marker: &[u8]) -> Result<()> {
        let evidence = transaction_evidence(proposal)?;
        check_sources(get(&evidence, "inventory"))
    }
}

/// Baseline `main` output: success as `json.dumps(value, indent=2,
/// ensure_ascii=False)`; a refusal as the compact error envelope, exit 1.
pub fn command_result(operation: &str, outcome: Result<Value>) -> CommandResult {
    match outcome {
        Ok(value) => match json::encode(&value, json::INDENT2_UTF8) {
            Ok(text) => CommandResult {
                value: Some(value),
                exit_code: 0,
                stdout: text + "\n",
                stderr: String::new(),
            },
            Err(error) => command_result(operation, Err(error)),
        },
        Err(error) => {
            let mut value = result(
                operation,
                [
                    ("error", string(error.message())),
                    ("verified", Value::Bool(false)),
                    ("inspection_required", Value::Bool(true)),
                ],
            );
            let details = crate::bank_capacity::details(&error);
            for (key, detail) in details.as_object().unwrap_or(&[]) {
                set_field(&mut value, key, detail.clone());
            }
            let stdout = json::encode(&value, json::DEFAULT).unwrap_or_default() + "\n";
            CommandResult {
                value: Some(value),
                exit_code: 1,
                stdout,
                stderr: String::new(),
            }
        }
    }
}
