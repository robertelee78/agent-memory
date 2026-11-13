//! Repository bank location validation and security-review boundary.
use crate::error::{ContractError, Result};
use std::sync::{Arc, RwLock};

pub const MAX_ROUTERS: usize = 64;
pub const MAX_DISCOVERY_ENTRIES: usize = 1_000_000;
pub const ROUTER_FIELDS: &[&str] = &["schema", "project-id", "router-updated", "canonical-remote"];

/// The security tier supplies the same scan used by the Python lazy import.
pub trait SecurityScan: Send + Sync {
    fn has_reasons(&self, value: &str) -> Result<bool>;
}
static SCANNER: RwLock<Option<Arc<dyn SecurityScan>>> = RwLock::new(None);
pub fn register_security_scan(provider: Option<Arc<dyn SecurityScan>>) -> Result<()> {
    *SCANNER
        .write()
        .map_err(|_| ContractError::new("Security scan registry is poisoned"))? = provider;
    Ok(())
}
pub fn safe_label(value: &str) -> Result<()> {
    let scanner = SCANNER
        .read()
        .map_err(|_| ContractError::new("Security scan registry is poisoned"))?
        .clone()
        .ok_or_else(|| {
            ContractError::new("Repository boundary security scan provider is not registered")
        })?;
    if scanner.has_reasons(value)? {
        return Err(ContractError::new(
            "Repository boundary label requires safety review; preserve it without echoing its value",
        ));
    }
    Ok(())
}
pub fn validate_root(relative: &str) -> Result<&str> {
    if relative.is_empty()
        || relative.len() > 256
        || relative.starts_with('/')
        || relative == "."
        || relative
            .split('/')
            .any(|p| p.is_empty() || p.starts_with('.'))
        || relative.chars().any(|c| {
            (c as u32) < 32
                || matches!(
                    c,
                    '\\' | ':' | '\u{7f}' | '\u{85}' | '\u{2028}' | '\u{2029}'
                )
        })
    {
        return Err(ContractError::new(
            "Memory bank root must be a bounded canonical repository-relative directory",
        ));
    }
    safe_label(relative)?;
    if crate::unicode::nfc_casefold(relative) == "memory-bank" && relative != "memory-bank" {
        return Err(ContractError::new(
            "Default memory bank directory has a portable spelling alias",
        ));
    }
    Ok(relative)
}
pub fn validate_branch(branch: &str) -> Result<&str> {
    if branch.is_empty()
        || branch.len() > 256
        || branch.starts_with(['-', '/'])
        || branch.ends_with(['/', '.'])
        || branch == "@"
        || branch.contains("..")
        || branch.contains("@{")
        || branch.contains("//")
        || branch
            .split('/')
            .any(|p| p.starts_with('.') || p.ends_with(".lock"))
        || branch
            .chars()
            .any(|c| c.is_whitespace() || (c as u32) < 32 || "~^:?*[\\\u{7f}".contains(c))
    {
        return Err(ContractError::new(
            "Authority branch must be an explicit safe Git branch name",
        ));
    }
    safe_label(branch)?;
    Ok(branch)
}
pub fn router_kind(raw: &[u8]) -> Result<Option<bool>> {
    let prefix = &raw[..raw.len().min(16384)];
    let marker = prefix.split(|b| *b == b'\n').any(|line| {
        let Some(mut rest) = line.strip_prefix(b"schema:") else {
            return false;
        };
        while rest.first().is_some_and(|b| *b == b' ' || *b == b'\t') {
            rest = &rest[1..];
        }
        if matches!(rest.first(), Some(b'"' | b'\'')) {
            rest = &rest[1..];
        }
        rest.starts_with(b"agent-memory/")
    });
    if !marker {
        return Ok(None);
    }
    if raw.starts_with(b"---\r\n") {
        return Err(ContractError::new(
            "Frontmatter must start at byte zero with LF separators; inspect the existing router",
        ));
    }
    let parsed = std::str::from_utf8(raw)
        .ok()
        .filter(|_| raw.len() <= 16384)
        .and_then(|s| crate::codec::frontmatter::read(s, ROUTER_FIELDS).ok())
        .is_some_and(|(metadata, _)| {
            metadata
                .iter()
                .any(|(key, value)| key == "schema" && value.as_str() == Some("agent-memory/v1"))
        });
    if !parsed {
        return Err(ContractError::new(
            "Possible existing memory needs repair or explicit inspection, not another bank; use am migrate preview to inspect an explicitly bundled chain",
        ));
    }
    Ok(Some(true))
}

use crate::fs::Descriptor;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

type Names = BTreeSet<String>;
type Paths = BTreeSet<OsString>;
type Config = BTreeMap<String, String>;
type Reader<'a> = Option<&'a dyn Fn(&[u8]) -> Result<Option<bool>>>;
const LOCAL: &[&str] = &[
    "config",
    "--local",
    "--null",
    "--get-regexp",
    r"^agent-memory\.(bankroot|authoritybranch)$",
];
const LISTED: &[&str] = &[
    "ls-files",
    "--cached",
    "--others",
    "--exclude-standard",
    "-t",
    "-z",
    "--",
    ":(icase,glob)**/MEMORY.md",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub bank_root: String,
    pub authority_branch: Option<String>,
}
#[derive(Clone)]
struct Inventory {
    listed: Paths,
    indexed: Paths,
    head: Option<String>,
    committed: Paths,
}
thread_local! { static OBSERVED: RefCell<Option<(PathBuf, Settings, Settings)>> = const { RefCell::new(None) }; }

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn git_error(e: crate::identity_git::Error) -> ContractError {
    err(&e.to_string())
}
fn argv(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}
fn query(root: &Path, args: &[&str], optional: bool, glob: bool) -> Result<Option<OsString>> {
    crate::identity_git::git(root, &argv(args), optional, glob).map_err(git_error)
}
fn text(value: OsString) -> Result<String> {
    value
        .into_string()
        .map_err(|_| err("Repository memory inventory is unavailable; do not infer absence"))
}
pub fn local_value(root: &Path, key: &str) -> Result<Option<String>> {
    let Some(raw) = query(
        root,
        &["config", "--local", "--null", "--get-all", key],
        true,
        false,
    )?
    else {
        return Ok(None);
    };
    let raw = crate::host::text(&raw, "repository configuration")?.to_owned();
    let mut values: Vec<_> = raw.split('\0').collect();
    if values.last() == Some(&"") {
        values.pop();
    }
    if values.len() != 1 || values[0].is_empty() {
        return Err(err(
            "Repository memory configuration needs one explicit value per key",
        ));
    }
    Ok(Some(values[0].to_owned()))
}
fn parse_local(raw: Option<OsString>) -> Result<Config> {
    let mut result = Config::new();
    let Some(raw) = raw else { return Ok(result) };
    let raw = crate::host::text(&raw, "repository configuration")?.to_owned();
    if !raw.ends_with('\0') {
        return Err(err(
            "Repository memory configuration inventory is incomplete",
        ));
    }
    for entry in raw[..raw.len() - 1].split('\0') {
        let (key, value) = entry.split_once('\n').ok_or_else(|| {
            err("Repository memory configuration needs one explicit value per key")
        })?;
        if !matches!(
            key,
            "agent-memory.bankroot" | "agent-memory.authoritybranch"
        ) || value.is_empty()
            || result.insert(key.to_owned(), value.to_owned()).is_some()
        {
            return Err(err(
                "Repository memory configuration needs one explicit value per key",
            ));
        }
    }
    Ok(result)
}
pub fn local_settings(root: &Path) -> Result<Config> {
    parse_local(query(root, LOCAL, true, false)?)
}
fn listed_paths(raw: Option<OsString>) -> Result<(Paths, Paths)> {
    let raw = raw
        .ok_or_else(|| err("Repository memory inventory is unavailable; do not infer absence"))?
        .into_vec();
    if !raw.is_empty() && !raw.ends_with(b"\0") {
        return Err(err(
            "Repository memory inventory is unavailable; do not infer absence",
        ));
    }
    let mut listed = Paths::new();
    let mut indexed = Paths::new();
    for entry in raw.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        if entry.len() < 3 || entry[1] != b' ' || !b"HSMRCK?U".contains(&entry[0]) {
            return Err(err(
                "Repository memory inventory has an unknown status; do not infer absence",
            ));
        }
        if !possible_name(&entry[2..]) {
            continue;
        }
        let path = OsString::from_vec(entry[2..].to_vec());
        if entry[0] != b'?' {
            indexed.insert(path.clone());
        }
        listed.insert(path);
    }
    Ok((listed, indexed))
}
fn checked_head(head: Option<String>) -> Result<Option<String>> {
    if head.as_ref().is_some_and(|s| {
        !matches!(s.len(), 40 | 64)
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        return Err(err(
            "Repository HEAD observation is incomplete; do not infer absence",
        ));
    }
    Ok(head)
}
fn possible_name(path: &[u8]) -> bool {
    let end = path
        .split(|b| *b == b'/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or_default();
    if end == b"." {
        return true;
    }
    match std::str::from_utf8(end) {
        Ok(s) => crate::unicode::casefold(s) == "memory.md",
        Err(_) => false,
    }
}
fn names(raw: OsString) -> Result<Paths> {
    let bytes = raw.into_vec();
    Ok(bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty() && possible_name(s))
        .map(|s| OsString::from_vec(s.to_vec()))
        .collect())
}
fn inventory(root: &Path) -> Result<Inventory> {
    let (listed, indexed) = listed_paths(query(root, LISTED, true, true)?)?;
    let head = checked_head(
        query(root, &["rev-parse", "--verify", "HEAD"], true, false)?
            .map(text)
            .transpose()?,
    )?;
    let committed = if let Some(h) = &head {
        names(
            query(
                root,
                &["ls-tree", "-r", "--name-only", "-z", h],
                false,
                false,
            )?
            .ok_or_else(|| {
                err("Repository memory inventory is unavailable; do not infer absence")
            })?,
        )?
    } else {
        Paths::new()
    };
    Ok(Inventory {
        listed,
        indexed,
        head,
        committed,
    })
}
fn inputs(root: &Path) -> Result<(bool, Config, Option<Inventory>)> {
    let (top, head) = crate::identity_git::root_head(root).map_err(git_error)?;
    if top.is_none() && child_exists(root, OsStr::new(".git"))? {
        return Err(err(
            "Existing Git metadata/configuration cannot be inspected; do not guess the bank root",
        ));
    }
    if let Some(top) = &top {
        if crate::fs::canonical_path(Path::new(top), false)? != root {
            return Err(err(
                "Select the actual repository root for its one memory bank",
            ));
        }
    } else {
        let config = local_settings(root)?;
        if let Some(value) = config.get("agent-memory.bankroot") {
            validate_root(value)?;
        }
        return Ok((false, config, None));
    }
    let mut specs = vec![(argv(LOCAL), false), (argv(LISTED), true)];
    if let Some(h) = &head {
        specs.push((argv(&["ls-tree", "-r", "--name-only", "-z", h]), false));
    }
    let (config, inventory) = crate::identity_git::_git_group(root, &specs, |group| {
        let convert = |e: ContractError| crate::identity_git::Error::Contract(e);
        let config = parse_local(group.value(0, true)?).map_err(convert)?;
        if let Some(value) = config.get("agent-memory.bankroot") {
            validate_root(value).map_err(convert)?;
        }
        let (listed, indexed) = listed_paths(group.value(1, true)?).map_err(convert)?;
        let head = checked_head(head.clone()).map_err(convert)?;
        let committed = if head.is_some() {
            names(group.value(2, false)?.ok_or_else(|| {
                convert(err(
                    "Repository memory inventory is unavailable; do not infer absence",
                ))
            })?)
            .map_err(convert)?
        } else {
            Paths::new()
        };
        Ok((
            config,
            Inventory {
                listed,
                indexed,
                head,
                committed,
            },
        ))
    })
    .map_err(git_error)?;
    Ok((true, config, Some(inventory)))
}
fn child_exists(root: &Path, name: &OsStr) -> Result<bool> {
    let dir = Descriptor::open_directory(root).map_err(|e| err(&e.to_string()))?;
    match dir.stat_at(name) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(err(&e.to_string())),
    }
}

pub fn router_bytes(root: &Path, relative: &Path) -> Result<Option<Vec<u8>>> {
    router_probe(root, relative, true)
}
fn router_probe(root: &Path, relative: &Path, read_body: bool) -> Result<Option<Vec<u8>>> {
    let mut directory = Descriptor::open_directory(root).map_err(|e| err(&e.to_string()))?;
    let parts: Vec<_> = relative.components().collect();
    for (i, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            return Err(err("Invalid possible repository memory locator"));
        };
        let info = match directory.stat_at(name) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(err(&e.to_string())),
        };
        if info.is_symlink() {
            return Err(err(
                "A possible memory locator crosses a symlink; inspect without adopting it",
            ));
        }
        if i + 1 == parts.len() {
            if !info.is_regular() {
                return Ok(None);
            }
            if !read_body {
                return Ok(Some(Vec::new()));
            }
            let mut file = directory
                .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
                .map_err(|e| err(&e.to_string()))?;
            if file.stat().map_err(|e| err(&e.to_string()))? != info {
                return Err(err(
                    "Possible memory disappeared during discovery; do not infer absence",
                ));
            }
            let bytes = file.read_prefix(16385).map_err(|e| err(&e.to_string()))?;
            if directory.stat_at(name).ok() != Some(info) {
                return Err(err(
                    "Possible memory disappeared during discovery; do not infer absence",
                ));
            }
            return Ok(Some(bytes));
        }
        if !info.is_directory() {
            return Ok(None);
        }
        directory = directory
            .open_directory_at(name)
            .map_err(|e| err(&e.to_string()))?;
    }
    Ok(None)
}
pub fn object_router_bytes(
    root: &Path,
    expression: &OsStr,
    cache: &mut BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>> {
    let args = vec![
        OsString::from("rev-parse"),
        OsString::from("--verify"),
        expression.to_owned(),
    ];
    let oid = crate::identity_git::git(root, &args, true, false)
        .map_err(git_error)?
        .map(text)
        .transpose()?;
    let oid = checked_head(oid)
        .map_err(|_| err("Tracked memory locator is unavailable; do not infer absence"))?
        .ok_or_else(|| err("Tracked memory locator is unavailable; do not infer absence"))?;
    if let Some(value) = cache.get(&oid) {
        return Ok(value.clone());
    }
    let size = query(root, &["cat-file", "-s", &oid], true, false)?
        .map(text)
        .transpose()?;
    let size = size.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).and_then(|s| s.parse::<usize>().ok()).filter(|n| *n <= 16384).ok_or_else(|| err("Tracked possible memory exceeds its reader or is unavailable; inspect before initialization"))?;
    let mut raw = query(root, &["cat-file", "blob", &oid], true, false)?
        .ok_or_else(|| err("Cannot read the tracked memory locator; do not infer absence"))?
        .into_vec();
    if raw.len() + 1 == size {
        raw.push(b'\n');
    }
    if raw.len() != size {
        return Err(err(
            "Tracked memory locator bytes are incomplete; do not infer absence",
        ));
    }
    cache.insert(oid, raw.clone());
    Ok(raw)
}
fn roots_at(
    root: &Path,
    inventory: &Inventory,
    reader: Reader<'_>,
    raw_locations: bool,
) -> Result<Names> {
    let mut paths = BTreeSet::<PathBuf>::new();
    for name in inventory.listed.iter().chain(inventory.committed.iter()) {
        if !possible_name(name.as_bytes()) {
            continue;
        }
        let path = Path::new(name);
        let parts: Vec<_> = path.components().collect();
        if parts.iter().any(|p| {
            matches!(p, Component::Normal(n) if n.as_bytes().starts_with(b"."))
                || matches!(p, Component::ParentDir)
        }) {
            continue;
        }
        if !path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|s| crate::unicode::casefold(s) == "memory.md")
        {
            continue;
        }
        paths.insert(parts.iter().collect::<PathBuf>());
    }
    if paths.len() > MAX_ROUTERS {
        return Err(err(
            "Memory locator inventory exceeds its bound; incomplete discovery cannot prove absence",
        ));
    }
    let mut roots = Names::new();
    let mut cache = BTreeMap::new();
    for path in paths {
        let relative = path.as_path();
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(err("Invalid possible repository memory locator"));
        }
        if raw_locations {
            let present = router_probe(root, relative, false).map_err(|e| if e.message() == "A possible memory locator crosses a symlink; inspect without adopting it" { err("A raw recovery locator crosses a symlink") } else { e })?.is_some();
            if !present
                && !inventory.indexed.contains(relative.as_os_str())
                && !inventory.committed.contains(relative.as_os_str())
            {
                return Err(err("Existing raw memory locator disappeared"));
            }
            let parent = relative
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            roots.insert(crate::host::text(parent.as_os_str(), "repository bank root")?.to_owned());
            continue;
        }
        let mut versions = Vec::new();
        if let Some(raw) = router_bytes(root, relative)? {
            versions.push(raw);
        }
        if inventory.indexed.contains(relative.as_os_str()) {
            let mut expression = OsString::from(":");
            expression.push(relative.as_os_str());
            versions.push(object_router_bytes(root, &expression, &mut cache)?);
        }
        if inventory.committed.contains(relative.as_os_str()) {
            let mut expression = OsString::from(inventory.head.as_deref().unwrap_or(""));
            expression.push(":");
            expression.push(relative.as_os_str());
            versions.push(object_router_bytes(root, &expression, &mut cache)?);
        }
        if versions.is_empty() {
            return Err(err(
                "Possible memory disappeared during discovery; do not infer absence",
            ));
        }
        for value in versions {
            if reader.unwrap_or(&router_kind)(&value)?.unwrap_or(false) {
                let parent = relative
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                roots.insert(
                    crate::host::text(parent.as_os_str(), "repository bank root")?.to_owned(),
                );
                break;
            }
        }
    }
    Ok(roots)
}
pub fn git_roots(root: &Path, reader: Reader<'_>, raw_locations: bool) -> Result<Names> {
    roots_at(root, &inventory(root)?, reader, raw_locations)
}

fn contexts() -> Result<Names> {
    let schema = crate::codec::json::decode(
        include_str!("../skills/agent-memory/schemas/bank-router.json"),
        Default::default(),
    )?;
    let object = schema
        .get("x-contexts")
        .and_then(|v| v.as_object())
        .ok_or_else(|| err("Bank context schema is unavailable"))?;
    Ok(object.iter().map(|(name, _)| name.clone()).collect())
}
fn discovery_error() -> ContractError {
    err("Repository discovery is incomplete; do not create a bank from an unreadable inventory")
}
pub fn physical_others(
    root: &Path,
    selected: &Path,
    reader: Reader<'_>,
    raw_locations: bool,
) -> Result<()> {
    let contexts = contexts()?;
    let root_dir = Descriptor::open_directory(root).map_err(|_| discovery_error())?;
    let mut pending = vec![(root.to_path_buf(), root_dir)];
    let mut examined = 0usize;
    let mut routers = 0usize;
    while let Some((directory, dir)) = pending.pop() {
        if directory != root {
            match dir.stat_at(OsStr::new(".git")) {
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(discovery_error()),
            }
        }
        let entries = dir.entries().map_err(|_| discovery_error())?;
        examined = examined.checked_add(entries.len()).ok_or_else(|| {
            err("Physical memory discovery exceeds its bound; absence remains unknown")
        })?;
        if examined > MAX_DISCOVERY_ENTRIES {
            return Err(err(
                "Physical memory discovery exceeds its bound; absence remains unknown",
            ));
        }
        let mut files = Vec::new();
        let mut subdirs = Vec::new();
        for name in entries {
            let info = dir.stat_at(&name).map_err(|_| discovery_error())?;
            if info.is_symlink() {
                // os.walk classifies links to directories as directory names but
                // never descends through them with followlinks=False.
                let target_is_directory = crate::fs::canonical_path(&directory.join(&name), true)
                    .ok()
                    .is_some_and(|target| {
                        if let (Some(parent), Some(leaf)) = (target.parent(), target.file_name()) {
                            Descriptor::pin_directory(parent)
                                .ok()
                                .and_then(|dir| dir.stat_at(leaf).ok())
                                .is_some_and(|info| info.is_directory())
                        } else {
                            Descriptor::pin_directory(&target)
                                .ok()
                                .and_then(|dir| dir.stat().ok())
                                .is_some_and(|info| info.is_directory())
                        }
                    });
                if target_is_directory {
                    continue;
                }
            }
            if info.is_directory() {
                subdirs.push(name);
            } else {
                files.push((name, info));
            }
        }
        let count = files
            .iter()
            .filter(|(name, _)| name.to_str().is_some_and(|s| contexts.contains(s)))
            .count();
        let has_router = files.iter().any(|(name, _)| {
            name.to_str()
                .is_some_and(|s| crate::unicode::casefold(s) == "memory.md")
        });
        if directory != selected && count > 0 && (has_router || count >= 2) {
            return Err(err(
                "Possible existing context bank needs explicit inspection; do not create a replacement",
            ));
        }
        for (name, info) in files {
            if !name
                .to_str()
                .is_some_and(|s| crate::unicode::casefold(s) == "memory.md")
            {
                continue;
            }
            routers += 1;
            if routers > MAX_ROUTERS {
                return Err(err("Possible memory locator inventory exceeds its bound"));
            }
            if info.is_symlink() {
                return Err(err(
                    "A possible existing memory locator is a symlink; inspect it explicitly",
                ));
            }
            if raw_locations {
                if directory != selected {
                    return Err(err(
                        "Existing memory lies outside the selected bank; configure its location explicitly",
                    ));
                }
            } else {
                let path = directory.join(&name);
                let relative = path.strip_prefix(root).map_err(|_| discovery_error())?;
                let raw = router_bytes(root, relative)?.ok_or_else(discovery_error)?;
                if reader.unwrap_or(&router_kind)(&raw)?.unwrap_or(false) && directory != selected {
                    return Err(err(
                        "Existing memory lies outside the selected bank; configure its location before initialization",
                    ));
                }
            }
        }
        for name in subdirs.into_iter().rev() {
            let path = directory.join(&name);
            if name.as_bytes().starts_with(b".") || path == selected {
                continue;
            }
            let child = dir
                .open_directory_at(&name)
                .map_err(|_| discovery_error())?;
            pending.push((path, child));
        }
    }
    Ok(())
}
fn settings_at(
    root: &Path,
    is_git: bool,
    config: &Config,
    inventory: Option<&Inventory>,
    reader: Reader<'_>,
    raw_locations: bool,
) -> Result<Settings> {
    let explicit = config.get("agent-memory.bankroot");
    if let Some(value) = explicit {
        validate_root(value)?;
    }
    let roots = if is_git {
        roots_at(
            root,
            inventory.ok_or_else(discovery_error)?,
            reader,
            raw_locations,
        )?
    } else {
        Names::new()
    };
    if roots.len() > 1 || explicit.is_some_and(|value| roots.iter().any(|root| root != value)) {
        return Err(err(
            "Competing memory bank locations require explicit reconciliation; preserve every source",
        ));
    }
    let relative = explicit
        .cloned()
        .or_else(|| roots.into_iter().next())
        .unwrap_or_else(|| "memory-bank".to_owned());
    validate_root(&relative)?;
    let mut current = root.to_path_buf();
    let mut dir = Some(Descriptor::open_directory(root).map_err(|e| err(&e.to_string()))?);
    for part in relative.split('/') {
        if let Some(parent) = dir.take() {
            let key = crate::unicode::nfc_casefold(part);
            for name in parent.entries().map_err(|e| err(&e.to_string()))? {
                if name
                    .to_str()
                    .is_some_and(|s| s != part && crate::unicode::nfc_casefold(s) == key)
                {
                    return Err(err(
                        "Configured bank directory has a portable spelling alias",
                    ));
                }
            }
            match parent.stat_at(OsStr::new(part)) {
                Ok(info) if info.is_symlink() || !info.is_directory() => {
                    return Err(err(
                        "Configured bank directory cannot follow symlinks or occupied files",
                    ));
                }
                Ok(_) => {
                    dir = Some(
                        parent
                            .open_directory_at(OsStr::new(part))
                            .map_err(|e| err(&e.to_string()))?,
                    )
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(err(&e.to_string())),
            }
        }
        current.push(part);
    }
    let selected_router = if let Some(parent) = dir {
        match parent.stat_at(OsStr::new("MEMORY.md")) {
            Ok(info) => info.is_regular(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(err(&e.to_string())),
        }
    } else {
        false
    };
    if !selected_router {
        physical_others(root, &current, reader, raw_locations)?;
    }
    let branch = config.get("agent-memory.authoritybranch").cloned();
    if let Some(value) = &branch {
        validate_branch(value)?;
    }
    Ok(Settings {
        bank_root: relative,
        authority_branch: branch,
    })
}
pub fn settings_with(repo: &Path, reader: Reader<'_>, raw_locations: bool) -> Result<Settings> {
    let root = crate::fs::canonical_path(repo, false)?;
    if reader.is_none() {
        if let Some(value) = OBSERVED.with(|v| {
            v.borrow().as_ref().and_then(|(path, ordinary, raw)| {
                (path == &root).then(|| {
                    if raw_locations {
                        raw.clone()
                    } else {
                        ordinary.clone()
                    }
                })
            })
        }) {
            return Ok(value);
        }
    }
    let (is_git, config, inventory) = inputs(&root)?;
    settings_at(
        &root,
        is_git,
        &config,
        inventory.as_ref(),
        reader,
        raw_locations,
    )
}
pub fn settings(repo: &Path) -> Result<Settings> {
    settings_with(repo, None, false)
}
pub fn recovery_settings(repo: &Path) -> Result<Settings> {
    settings_with(repo, None, true)
}
pub fn bank_path(repo: &Path) -> Result<PathBuf> {
    Ok(crate::fs::canonical_path(repo, false)?.join(settings(repo)?.bank_root))
}
pub fn relative(repo: &Path) -> Result<String> {
    Ok(settings(repo)?.bank_root)
}
fn pair(root: &Path) -> Result<(Settings, Settings)> {
    let (is_git, config, inventory) = inputs(root)?;
    Ok((
        settings_at(root, is_git, &config, inventory.as_ref(), None, false)?,
        settings_at(root, is_git, &config, inventory.as_ref(), None, true)?,
    ))
}
struct RestoreObservation(Option<(PathBuf, Settings, Settings)>);
impl Drop for RestoreObservation {
    fn drop(&mut self) {
        OBSERVED.with(|v| *v.borrow_mut() = self.0.take());
    }
}
/// Observe entry and successful exit independently; nested scopes restore their parent.
pub fn observe_settings<T>(repo: &Path, body: impl FnOnce() -> Result<T>) -> Result<T> {
    let root = crate::fs::canonical_path(repo, false)?;
    let previous = OBSERVED.with(|v| v.borrow_mut().take());
    let _restore = RestoreObservation(previous);
    let initial = pair(&root)?;
    OBSERVED.with(|v| *v.borrow_mut() = Some((root.clone(), initial.0.clone(), initial.1.clone())));
    let result = body()?;
    OBSERVED.with(|v| *v.borrow_mut() = None);
    if pair(&root)? != initial {
        return Err(err(
            "Private enrollment bank configuration changed during observation",
        ));
    }
    Ok(result)
}
