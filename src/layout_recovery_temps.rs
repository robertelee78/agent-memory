//! Recognize only journal-derived temporary names and exact reviewed payloads.
use crate::codec::json::Value;
use crate::developer_identity::{encoded, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{Descriptor, Identity};
use crate::layout_resolution::{
    digest, err, has_keys, int, io_error, keys, lstat, number, same, sha as value_digest, with,
};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_observation, bank_writes as writes, host, layout_recovery_observation as raw,
    layout_recovery_state as state, sharing_limits,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

const PROBE_PREFIX: &str = ".agent-memory-probe-";
const ATTEMPTS: &str = "metadata-probe-attempts";
const ATTEMPT_SCHEMA: &str = "agent-memory/layout-metadata-probe-attempt-v1";
const COMPLETION_SCHEMA: &str = "agent-memory/layout-metadata-probe-completion-v1";
/// Baseline probe nonces are exactly 32 bytes; one more byte detects growth.
const NONCE_BYTES: usize = 32;

fn ownership(path: &str) -> ContractError {
    ContractError::probe_ownership(path)
}
fn is_ownership(error: &ContractError) -> bool {
    matches!(error.kind(), Kind::ProbeOwnership { .. })
}
fn hex(bytes: &[u8]) -> String {
    crate::validation_memo::hex(bytes)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid metadata probe journal"))
}
fn hex_bytes(value: &Value) -> Option<Vec<u8>> {
    let body = value.as_str()?;
    if body.len() % 2 != 0 {
        return None;
    }
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    body.as_bytes()
        .chunks(2)
        .map(|pair| match pair {
            [high, low] => Some(digit(*high)? << 4 | digit(*low)?),
            _ => None,
        })
        .collect()
}
fn s_imode(mode: i128) -> i128 {
    mode & 0o7777
}

/// `str(Path(path).parent)`: `.` for a top-level name.
fn parent_of(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent,
        _ => ".",
    }
}
/// `Path(path).name`.
fn name_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}
/// `str(Path(parent) / name)`.
fn join(parent: &str, name: &str) -> String {
    if parent == "." || parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}
fn parts(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

pub fn source_name(operation_id: &str, path: &str) -> String {
    let digest = digest(path.as_bytes());
    // A SHA-256 hex digest always has 64 ASCII characters.
    let short = &digest[..16];
    join(
        parent_of(path),
        &format!(".agent-memory-layout-{operation_id}-{short}.tmp"),
    )
}

type Allowed = (String, i128, i128, i128, Option<Value>);

fn facts_tuple(sha: &str, facts: &Value) -> Result<Allowed> {
    let field = |key: &str| {
        int(get(facts, key))
            .ok_or_else(|| err("Unknown, partial or changed transaction temporary; preserve it for explicit inspection"))
    };
    Ok((
        sha.to_owned(),
        field("uid")?,
        field("gid")?,
        s_imode(field("mode")?),
        Some(get(facts, "xattrs").clone()),
    ))
}

/// Recognize existing journal-derived temporaries; anything else is preserved evidence.
pub fn entries(
    bank: &Path,
    operation_id: &str,
    actions: &[Value],
    base_view: &[Value],
    final_facts: &BTreeMap<String, Value>,
    marker_payloads: &[Vec<u8>],
) -> Result<Vec<Value>> {
    let base: BTreeMap<&str, &Value> = base_view
        .iter()
        .filter_map(|row| Some((get(row, "path").as_str()?, row)))
        .collect();
    let mut candidates: BTreeMap<String, Vec<Allowed>> = BTreeMap::new();
    let bank_stat = Descriptor::pin_directory(bank)
        .and_then(|directory| directory.stat())
        .map_err(io_error)?;
    let inherited_gid = if !cfg!(target_os = "linux") || bank_stat.mode & libc::S_ISGID as u32 != 0
    {
        bank_stat.gid
    } else {
        host::effective_gid()
    };
    for action in actions {
        let path = text(action, "path")?;
        let mut values = Vec::new();
        let pairs = [
            (get(action, "before"), base.get(path).copied()),
            (get(action, "after"), final_facts.get(path)),
        ];
        for (sha, facts) in pairs {
            if let (Some(sha), Some(facts)) = (sha.as_str(), facts) {
                values.push(facts_tuple(sha, facts)?);
            }
        }
        candidates.insert(source_name(operation_id, path), values);
    }
    for payload in marker_payloads {
        let marker = crate::developer_identity::load(payload)?;
        let identity = text(&marker, "operation_id")?;
        let path = format!(".agent-memory-layout-marker-{identity}.tmp");
        candidates.entry(path).or_default().push((
            digest(payload),
            i128::from(host::effective_uid()),
            i128::from(inherited_gid),
            0o600,
            None,
        ));
    }
    let mut result = Vec::new();
    for (path, allowed) in candidates {
        let target = bank.join(&path);
        if lstat(&target)?.is_none() {
            continue;
        }
        let (body, facts) = raw::read_source(bank, &path, raw::MAX_FILE)?;
        let xattrs = raw::metadata_supported(&target)?;
        let observed = (
            digest(&body),
            int(get(&facts, "uid")),
            int(get(&facts, "gid")),
            int(get(&facts, "mode")).map(s_imode),
        );
        let known = allowed.iter().any(|item| {
            (observed.0.as_str(), observed.1, observed.2, observed.3)
                == (item.0.as_str(), Some(item.1), Some(item.2), Some(item.3))
                && item
                    .4
                    .as_ref()
                    .is_none_or(|expected| same(&xattrs, expected))
        });
        if !known {
            return Err(err(
                "Unknown, partial or changed transaction temporary; preserve it for explicit inspection",
            ));
        }
        result.push(object([
            ("path", string(path)),
            ("sha256", string(digest(&body))),
            ("facts", facts),
            ("xattrs", xattrs),
        ]));
    }
    Ok(result)
}

/// Open the bank and every parent of `path` without following links.
fn open_parents(bank: &Path, path: &str) -> io::Result<Vec<Descriptor>> {
    let mut descriptors = vec![Descriptor::open_directory(bank)?];
    let names = parts(path);
    for part in names.iter().take(names.len().saturating_sub(1)) {
        let next = descriptors
            .last()
            .ok_or_else(|| io::Error::other("Missing parent descriptor"))?
            .open_directory_at(OsStr::new(part))?;
        descriptors.push(next);
    }
    Ok(descriptors)
}
fn last(descriptors: &[Descriptor]) -> Result<&Descriptor> {
    descriptors
        .last()
        .ok_or_else(|| err("Metadata probe parent binding changed"))
}

pub fn clean(
    bank: &Path,
    pins: &[Value],
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<()> {
    for pin in pins {
        bank_observation::markers(bank, owner, transaction)?;
        let path = text(pin, "path")?;
        let (body, facts) = raw::read_source(bank, path, raw::MAX_FILE)?;
        if get(pin, "sha256").as_str() != Some(digest(&body).as_str())
            || !same(&facts, get(pin, "facts"))
            || !same(
                &raw::metadata_supported(&bank.join(path))?,
                get(pin, "xattrs"),
            )
        {
            return Err(err("Known recovery temporary changed after preview"));
        }
        let descriptors = open_parents(bank, path).map_err(io_error)?;
        let parent = last(&descriptors)?;
        let name = OsStr::new(name_of(path));
        let named = parent.stat_at(name).map_err(io_error)?;
        if !same(&raw::signature(&named), &facts) {
            return Err(err(
                "Known temporary binding changed immediately before cleanup",
            ));
        }
        parent.unlink_at(name).map_err(io_error)?;
        parent.fsync_directory().map_err(io_error)?;
    }
    Ok(())
}

fn probe_facts(value: &Value) -> Result<&Value> {
    let ordinary = |key: &str| int(get(value, key)).is_some_and(|number| number >= 0);
    if !has_keys(value, &["uid", "gid", "mode", "xattrs"])
        || !ordinary("uid")
        || !ordinary("gid")
        || !ordinary("mode")
        || int(get(value, "mode")).is_some_and(|mode| mode > 0o7777)
        || !raw::valid_xattrs(get(value, "xattrs"))
    {
        return Err(err("Invalid metadata probe facts"));
    }
    Ok(value)
}

fn probe_name(operation_id: &str, label: &Value) -> Result<String> {
    let digest = value_digest(label)?;
    // A SHA-256 hex digest always has 64 ASCII characters.
    Ok(format!("{PROBE_PREFIX}{operation_id}-{}", &digest[..20]))
}

fn route(
    operation_id: &str,
    planned: &[(String, Value)],
    target: &str,
    value: &Value,
    kind: &str,
) -> Result<Value> {
    raw::path_value(target)?;
    probe_facts(value)?;
    let planned_facts = |path: &str| {
        planned
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, v)| v)
    };
    let parent = parent_of(target);
    let parent_parts = parts(parent);
    let ancestors: Vec<String> = (1..=parent_parts.len())
        .map(|count| {
            parent_parts
                .iter()
                .take(count)
                .copied()
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    let mut candidates: Vec<String> = ancestors
        .into_iter()
        .filter(|path| planned_facts(path).is_some())
        .collect();
    if kind == "directory" {
        candidates.push(target.to_owned());
    }
    let mut routes: Vec<Vec<Value>> = Vec::new();
    if kind == "file" {
        routes.push(Vec::new());
    }
    for first in &candidates {
        let mut shadow_parent = parent_of(first).to_owned();
        let mut shadow = Vec::new();
        let chain = candidates
            .iter()
            .filter(|path| *path == first || path.starts_with(&format!("{first}/")));
        for directory in chain {
            let name = probe_name(
                operation_id,
                &object([
                    ("directory", string(directory.as_str())),
                    ("parent", string(shadow_parent.as_str())),
                ]),
            )?;
            shadow_parent = join(&shadow_parent, &name);
            let facts = planned_facts(directory)
                .ok_or_else(|| err("Invalid metadata probe facts"))?
                .clone();
            shadow.push(object([
                ("path", string(shadow_parent.as_str())),
                ("type", string("directory")),
                ("target", string(directory.as_str())),
                ("facts", facts),
            ]));
        }
        routes.push(shadow);
    }
    let mut result = Vec::new();
    for shadow in routes {
        let mut nodes = shadow.clone();
        if kind == "file" {
            let destination = match shadow.last() {
                Some(node) => text(node, "path")?.to_owned(),
                None => parent.to_owned(),
            };
            let name = probe_name(
                operation_id,
                &object([
                    ("target", string(target)),
                    ("facts", value.clone()),
                    ("parent", string(destination.as_str())),
                ]),
            )?;
            nodes.push(object([
                ("path", string(join(&destination, &name))),
                ("type", string("file")),
                ("target", string(target)),
                ("facts", value.clone()),
            ]));
        }
        result.push(Value::Array(nodes));
    }
    Ok(object([
        ("type", string(kind)),
        ("target", string(target)),
        ("facts", value.clone()),
        ("routes", Value::Array(result)),
    ]))
}

/// Derive every probe name before any bank mutation, including shadow parents.
pub fn probe_plan(
    operation_id: &str,
    requirements: &[Value],
    directories: &Value,
) -> Result<Value> {
    if !uuid_valid(&string(operation_id)) {
        return Err(err("Invalid metadata probe operation identity"));
    }
    let planned = directories
        .as_object()
        .ok_or_else(|| err("Invalid metadata probe facts"))?
        .to_vec();
    for (path, value) in &planned {
        raw::path_value(path)?;
        probe_facts(value)?;
    }
    let mut tasks = Vec::new();
    let mut ordered = planned.clone();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, value) in &ordered {
        tasks.push(route(operation_id, &planned, path, value, "directory")?);
    }
    let mut sorted = Vec::new();
    for requirement in requirements {
        sorted.push((encoded(requirement)?, requirement));
    }
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, requirement) in sorted {
        if !has_keys(requirement, &["path", "facts"]) {
            return Err(err("Invalid metadata probe requirement"));
        }
        let target = get(requirement, "path")
            .as_str()
            .ok_or_else(|| err("Unsafe raw-source path; preserve the physical bank"))?;
        tasks.push(route(
            operation_id,
            &planned,
            target,
            get(requirement, "facts"),
            "file",
        )?);
    }
    let plan = object([
        ("operation_id", string(operation_id)),
        ("directories", Value::Object(planned)),
        ("tasks", Value::Array(tasks)),
    ]);
    sharing_limits::private_bytes(&plan)?;
    Ok(plan)
}

fn file_requirements(plan: &Value) -> Result<Vec<Value>> {
    let tasks = get(plan, "tasks")
        .as_array()
        .ok_or_else(|| err("Invalid metadata probe journal"))?;
    let mut requirements = Vec::new();
    for task in tasks {
        if task.as_object().is_none() || task.get("type").is_none() {
            return Err(err("Invalid metadata probe journal"));
        }
        if get(task, "type").as_str() == Some("file") {
            let (Some(target), Some(facts)) = (task.get("target"), task.get("facts")) else {
                return Err(err("Invalid metadata probe journal"));
            };
            requirements.push(object([("path", target.clone()), ("facts", facts.clone())]));
        }
    }
    Ok(requirements)
}

fn probe_validate(plan: &Value) -> Result<()> {
    let requirements = file_requirements(plan)?;
    let (Some(operation_id), Some(directories)) = (
        plan.get("operation_id").and_then(Value::as_str),
        plan.get("directories"),
    ) else {
        return Err(err("Invalid metadata probe journal"));
    };
    let expected = probe_plan(operation_id, &requirements, directories)?;
    if !same(plan, &expected) {
        return Err(err(
            "Metadata probe journal differs from its deterministic plan",
        ));
    }
    Ok(())
}

pub fn validate_probe_plan(
    plan: &Value,
    requirements: &[Value],
    directories: &Value,
) -> Result<()> {
    let operation_id = get(plan, "operation_id")
        .as_str()
        .ok_or_else(|| err("Invalid metadata probe operation identity"))?;
    if !same(plan, &probe_plan(operation_id, requirements, directories)?) {
        return Err(err(
            "Metadata probe plan differs from authorized requirements",
        ));
    }
    Ok(())
}

fn task_requirement(task: &Value) -> Result<Vec<u8>> {
    encoded(&object([
        ("path", get(task, "target").clone()),
        ("facts", get(task, "facts").clone()),
    ]))
}

fn probe_nodes(
    plan: &Value,
    requirements: Option<&Value>,
    directories: Option<&Value>,
) -> Result<Vec<Value>> {
    probe_validate(plan)?;
    let selected = match requirements.filter(|value| !matches!(value, Value::Null)) {
        None => None,
        Some(value) => {
            let items = value
                .as_array()
                .ok_or_else(|| err("Metadata probe selection is outside the journal"))?;
            let mut set = BTreeSet::new();
            for item in items {
                set.insert(encoded(item)?);
            }
            Some(set)
        }
    };
    let selected_dirs = match directories.filter(|value| !matches!(value, Value::Null)) {
        None => None,
        Some(value) => {
            let items = value
                .as_array()
                .ok_or_else(|| err("Metadata directory probe selection is outside the journal"))?;
            let mut set = BTreeSet::new();
            for item in items {
                set.insert(
                    item.as_str()
                        .ok_or_else(|| {
                            err("Metadata directory probe selection is outside the journal")
                        })?
                        .to_owned(),
                );
            }
            Some(set)
        }
    };
    let tasks = get(plan, "tasks").as_array().unwrap_or_default();
    let mut available = BTreeSet::new();
    for task in tasks {
        if get(task, "type").as_str() == Some("file") {
            available.insert(task_requirement(task)?);
        }
    }
    if selected
        .as_ref()
        .is_some_and(|set| !set.is_subset(&available))
    {
        return Err(err("Metadata probe selection is outside the journal"));
    }
    let planned: BTreeSet<String> = keys(get(plan, "directories"))
        .into_iter()
        .map(str::to_owned)
        .collect();
    if selected_dirs
        .as_ref()
        .is_some_and(|set| !set.is_subset(&planned))
    {
        return Err(err(
            "Metadata directory probe selection is outside the journal",
        ));
    }
    let mut nodes: BTreeMap<String, Value> = BTreeMap::new();
    for task in tasks {
        if get(task, "type").as_str() == Some("file") {
            if let Some(selected) = &selected {
                if !selected.contains(&task_requirement(task)?) {
                    continue;
                }
            }
        } else if let Some(selected_dirs) = &selected_dirs {
            if !get(task, "target")
                .as_str()
                .is_some_and(|target| selected_dirs.contains(target))
            {
                continue;
            }
        }
        for route in get(task, "routes").as_array().unwrap_or_default() {
            let route = route.as_array().unwrap_or_default();
            if let Some(selected_dirs) = &selected_dirs {
                let foreign = route.iter().any(|node| {
                    get(node, "type").as_str() == Some("directory")
                        && !get(node, "target")
                            .as_str()
                            .is_some_and(|target| selected_dirs.contains(target))
                });
                if foreign {
                    continue;
                }
            }
            for node in route {
                let path = text(node, "path")?.to_owned();
                match nodes.get(&path) {
                    Some(previous) if !same(previous, node) => {
                        return Err(err("Conflicting metadata probe names"));
                    }
                    Some(_) => {}
                    None => {
                        nodes.insert(path, node.clone());
                    }
                }
            }
        }
    }
    let mut ordered: Vec<(usize, String, Value)> = nodes
        .into_iter()
        .map(|(path, node)| (parts(&path).len(), path, node))
        .collect();
    ordered.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(ordered.into_iter().map(|(_, _, node)| node).collect())
}

/// Failure classes the baseline distinguished by exception type.
enum Fault {
    Missing(io::Error),
    Os(io::Error),
    Contract(ContractError),
}
impl From<io::Error> for Fault {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Fault::Missing(error)
        } else {
            Fault::Os(error)
        }
    }
}
impl From<ContractError> for Fault {
    fn from(error: ContractError) -> Self {
        Fault::Contract(error)
    }
}
impl Fault {
    fn contract(self) -> ContractError {
        match self {
            Fault::Missing(error) | Fault::Os(error) => io_error(error),
            Fault::Contract(error) => error,
        }
    }
    /// Baseline `except OSError: raise ProbeOwnershipError(path)`.
    fn owned(self, path: &str) -> ContractError {
        match self {
            Fault::Missing(_) | Fault::Os(_) => ownership(path),
            Fault::Contract(error) => error,
        }
    }
}

fn probe_parent(bank: &Path, path: &str) -> std::result::Result<Vec<Descriptor>, Fault> {
    let descriptors = open_parents(bank, path)?;
    probe_bound(bank, path, &descriptors)?;
    Ok(descriptors)
}

fn probe_bound(
    bank: &Path,
    path: &str,
    descriptors: &[Descriptor],
) -> std::result::Result<(), Fault> {
    let root = lstat(bank)?.ok_or_else(|| err("Metadata probe root binding changed"))?;
    let opened = descriptors
        .first()
        .ok_or_else(|| err("Metadata probe root binding changed"))?
        .stat()?;
    if !root.same_inode(opened) {
        return Err(err("Metadata probe root binding changed").into());
    }
    let names = parts(path);
    for (index, part) in names.iter().take(names.len().saturating_sub(1)).enumerate() {
        let (Some(parent), Some(child)) = (descriptors.get(index), descriptors.get(index + 1))
        else {
            return Err(err("Metadata probe parent binding changed").into());
        };
        let named = parent.stat_at(OsStr::new(part))?;
        let opened = child.stat()?;
        if !named.is_directory() || !named.same_inode(opened) {
            return Err(err("Metadata probe parent binding changed").into());
        }
    }
    Ok(())
}

fn probe_xattrs(descriptor: &Descriptor, path: &str) -> Result<Value> {
    let names = crate::fs::list_xattrs(descriptor).map_err(|_| ownership(path))?;
    let mut values = BTreeMap::new();
    for name in names {
        let body = crate::fs::read_xattr(descriptor, &name).map_err(|_| ownership(path))?;
        let decoded = String::from_utf8(name).map_err(|_| ownership(path))?;
        if body.len() > 16384 {
            return Err(ownership(path));
        }
        values.insert(decoded, string(hex(&body)));
    }
    Ok(Value::Object(values.into_iter().collect()))
}

fn pair(info: &Identity) -> Value {
    object([("dev", number(info.dev)), ("ino", number(info.ino))])
}

fn probe_pin(bank: &Path, node: &Value) -> std::result::Result<Value, Fault> {
    let path = text(node, "path")?;
    let kind = text(node, "type")?;
    let descriptors = probe_parent(bank, path)?;
    let parent = last(&descriptors)?;
    let name = OsStr::new(name_of(path));
    let mut descriptor = parent.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
    let info = descriptor.stat()?;
    let flags = raw::inode_flags(&descriptor)?;
    if (kind == "file"
        && (!info.is_regular() || info.nlink != 1 || info.size != NONCE_BYTES as i64))
        || (kind == "directory" && !info.is_directory())
    {
        return Err(ownership(path).into());
    }
    let nonce = if kind == "file" {
        let body = descriptor
            .read_prefix(NONCE_BYTES + 1)
            .map_err(|_| ownership(path))?;
        if body.len() != NONCE_BYTES {
            return Err(ownership(path).into());
        }
        Some(body)
    } else {
        None
    };
    let attrs = probe_xattrs(&descriptor, path)?;
    let named = parent.stat_at(name)?;
    probe_bound(bank, path, &descriptors)?;
    if named != info || descriptor.stat()? != info || raw::inode_flags(&descriptor)? != flags {
        return Err(ownership(path).into());
    }
    let parent_info = parent.stat()?;
    Ok(object([
        ("path", string(path)),
        ("type", string(kind)),
        ("facts", raw::signature(&info)),
        ("flags", number(flags)),
        ("xattrs", attrs),
        ("parent", pair(&parent_info)),
        (
            "nonce_sha256",
            nonce.map_or(Value::Null, |body| string(digest(&body))),
        ),
    ]))
}

fn pin_matches(actual: &Value, expected: &Value, directory_children: bool) -> bool {
    let fixed = ["path", "type", "flags", "xattrs", "parent", "nonce_sha256"];
    if fixed.iter().any(|key| {
        expected
            .get(key)
            .is_none_or(|value| !same(get(actual, key), value))
    }) {
        return false;
    }
    let directory = get(actual, "type").as_str() == Some("directory");
    let facts = get(actual, "facts");
    let selected: Vec<&str> = if directory_children && directory {
        vec!["dev", "ino", "mode", "uid", "gid"]
    } else if directory {
        vec!["dev", "ino", "mode", "uid", "gid", "nlink"]
    } else {
        keys(facts).into_iter().collect()
    };
    let expected_facts = get(expected, "facts");
    selected.iter().all(|key| {
        expected_facts
            .get(key)
            .is_some_and(|value| same(get(facts, key), value))
    })
}

type Budget = BTreeMap<PathBuf, Identity>;

fn budget_visit(
    parent: &Descriptor,
    name: &OsStr,
    path: &Path,
    level: usize,
    found: &mut Budget,
    total: &mut u64,
) -> Result<()> {
    let info = parent.stat_at(name).map_err(io_error)?;
    let directory = level < 2 || (level == 2 && name == "pins");
    if (directory && !info.is_directory()) || (!directory && !info.is_regular()) {
        return Err(err("Unknown metadata probe attempt evidence"));
    }
    found.insert(path.to_path_buf(), info);
    if found.len() > raw::MAX_ENTRIES {
        return Err(err("Metadata probe attempt capacity exceeded"));
    }
    if !directory {
        *total += u64::try_from(info.size).unwrap_or(u64::MAX);
        if *total > sharing_limits::MAX_PRIVATE as u64 {
            return Err(err("Metadata probe attempt capacity exceeded"));
        }
        return Ok(());
    }
    let opened = parent.open_directory_at(name).map_err(io_error)?;
    if !opened.stat().map_err(io_error)?.same_inode(info) {
        return Err(err("Unknown metadata probe attempt evidence"));
    }
    let names = opened
        .entries_bounded(raw::MAX_ENTRIES)
        .map_err(io_error)?
        .ok_or_else(|| err("Metadata probe attempt capacity exceeded"))?;
    for child in names {
        let label = child.to_str().unwrap_or_default();
        let valid = match level {
            0 => uuid_valid(&string(label)),
            1 => matches!(label, "plan.json" | "completion.json" | "pins"),
            _ => label
                .strip_prefix(PROBE_PREFIX)
                .and_then(|rest| rest.strip_suffix(".json"))
                .is_some_and(|identity| uuid_valid(&string(identity))),
        };
        if !valid {
            return Err(err("Unknown metadata probe attempt evidence"));
        }
        budget_visit(&opened, &child, &path.join(&child), level + 1, found, total)?;
    }
    Ok(())
}

/// Bound and pin all attempt evidence before loading any record.
fn attempt_budget(base: &Path) -> Result<Budget> {
    if lstat(base)?.is_none() {
        return Ok(Budget::new());
    }
    let (Some(parent), Some(name)) = (base.parent(), base.file_name()) else {
        return Err(err("Unknown metadata probe attempt evidence"));
    };
    let parent = Descriptor::pin_directory(parent).map_err(io_error)?;
    let mut found = Budget::new();
    let mut total = 0;
    budget_visit(&parent, name, base, 0, &mut found, &mut total)?;
    Ok(found)
}

fn attempt_store(directory: &Path, path: &Path, value: &Value) -> Result<()> {
    let base = directory.join(ATTEMPTS);
    let snapshot = attempt_budget(&base)?;
    let body = sharing_limits::private_bytes(value)?;
    if lstat(path)?.is_none() {
        let mut missing = 1;
        let stop = base.parent();
        let mut parent = path.parent();
        while let Some(current) = parent {
            if snapshot.contains_key(current) || Some(current) == stop {
                break;
            }
            missing += 1;
            parent = current.parent();
        }
        let total: u64 = snapshot
            .values()
            .filter(|info| info.is_regular())
            .map(|info| u64::try_from(info.size).unwrap_or(u64::MAX))
            .sum();
        if snapshot.len() + missing > raw::MAX_ENTRIES
            || total + body.len() as u64 > sharing_limits::MAX_PRIVATE as u64
        {
            return Err(err("Metadata probe attempt capacity exceeded"));
        }
    }
    state::write_once(path, value)?;
    Ok(())
}

fn node_path<'a>(node: &'a Value) -> Result<&'a str> {
    text(node, "path")
}

fn attempt(
    bank: &Path,
    plan: &Value,
    directory: &Path,
    requirements: Option<&Value>,
    directories: Option<&Value>,
) -> Result<(PathBuf, Value)> {
    let templates = probe_nodes(plan, requirements, directories)?;
    let mut indexes = BTreeMap::new();
    for (index, node) in probe_nodes(plan, None, None)?.iter().enumerate() {
        indexes.insert(node_path(node)?.to_owned(), index);
    }
    let attempt_id = host::uuid4()?;
    let mut mapped: BTreeMap<String, Value> = BTreeMap::new();
    let mut nodes = Vec::new();
    for template in templates {
        let template_path = node_path(&template)?.to_owned();
        let old_parent = parent_of(&template_path).to_owned();
        let parent_ref = mapped
            .get(&old_parent)
            .map(|node| node_path(node).map(str::to_owned))
            .transpose()?;
        let parent = parent_ref.clone().unwrap_or(old_parent);
        let path = join(&parent, &format!("{PROBE_PREFIX}{}", host::uuid4()?));
        let mut parent_pin = Value::Null;
        if parent_ref.is_none() {
            match probe_parent(bank, &path) {
                Ok(descriptors) => {
                    parent_pin = pair(&last(&descriptors)?.stat().map_err(io_error)?);
                }
                Err(Fault::Missing(_)) => {}
                Err(other) => return Err(other.contract()),
            }
        }
        let nonce = if get(&template, "type").as_str() == Some("file") {
            let mut bytes = [0u8; NONCE_BYTES];
            host::random_bytes(&mut bytes)?;
            string(hex(&bytes))
        } else {
            Value::Null
        };
        let index = *indexes
            .get(&template_path)
            .ok_or_else(|| err("Metadata probe attempt is outside its authorized plan"))?;
        let node = with(
            template,
            [
                ("template", string(template_path.as_str())),
                ("path", string(path.as_str())),
                ("index", number(index)),
                ("parent_ref", parent_ref.map_or(Value::Null, string)),
                ("parent_pin", parent_pin),
                ("nonce", nonce),
            ],
        );
        mapped.insert(template_path, node.clone());
        nodes.push(node);
    }
    let value = object([
        ("schema", string(ATTEMPT_SCHEMA)),
        ("operation_id", get(plan, "operation_id").clone()),
        ("plan_sha256", string(value_digest(plan)?)),
        ("attempt_id", string(attempt_id.as_str())),
        ("requirements", requirements.cloned().unwrap_or(Value::Null)),
        ("directories", directories.cloned().unwrap_or(Value::Null)),
        ("nodes", Value::Array(nodes)),
    ]);
    let attempt_dir = directory.join(ATTEMPTS).join(&attempt_id);
    attempt_store(directory, &attempt_dir.join("plan.json"), &value)?;
    writes::fault("layout-recovery-probe-after-attempt-journal")?;
    Ok((attempt_dir, value))
}

fn nonce_digest(node: &Value) -> Option<String> {
    hex_bytes(get(node, "nonce")).map(|nonce| digest(&nonce))
}

fn attempt_value(path: &Path, plan: &Value) -> Result<Value> {
    let value = state::read(&path.join("plan.json"), sharing_limits::MAX_PRIVATE, false)?
        .ok_or_else(|| err("Invalid metadata probe attempt"))?;
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    if !has_keys(
        &value,
        &[
            "schema",
            "operation_id",
            "plan_sha256",
            "attempt_id",
            "requirements",
            "directories",
            "nodes",
        ],
    ) || get(&value, "schema").as_str() != Some(ATTEMPT_SCHEMA)
        || !same(get(&value, "operation_id"), get(plan, "operation_id"))
        || get(&value, "plan_sha256").as_str() != Some(value_digest(plan)?.as_str())
        || get(&value, "attempt_id").as_str() != Some(name)
        || !uuid_valid(&string(name))
    {
        return Err(err("Invalid metadata probe attempt"));
    }
    let templates = probe_nodes(
        plan,
        Some(get(&value, "requirements")),
        Some(get(&value, "directories")),
    )?;
    let mut indexes = BTreeMap::new();
    for (index, node) in probe_nodes(plan, None, None)?.iter().enumerate() {
        indexes.insert(node_path(node)?.to_owned(), index);
    }
    let nodes = get(&value, "nodes").as_array().unwrap_or_default();
    if templates.len() != nodes.len() || get(&value, "nodes").as_array().is_none() {
        return Err(err("Metadata probe attempt is outside its authorized plan"));
    }
    let mut mapped: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for (template, node) in templates.iter().zip(nodes) {
        let mut expected = keys(template);
        expected.extend(["template", "index", "parent_ref", "parent_pin", "nonce"]);
        if node.as_object().is_none() || keys(node) != expected {
            return Err(err("Invalid metadata probe attempt node"));
        }
        let template_path = node_path(template)?;
        let old_parent = parent_of(template_path);
        let reference = mapped.get(old_parent).cloned();
        let parent = reference.clone().unwrap_or_else(|| old_parent.to_owned());
        let changed = || err("Metadata probe attempt paths changed");
        let path = get(node, "path").as_str().ok_or_else(changed)?;
        let name = name_of(path);
        let index_ok = indexes
            .get(template_path)
            .is_some_and(|index| int(get(node, "index")) == Some(*index as i128));
        let reference_value = reference.as_deref().map_or(Value::Null, string);
        if get(node, "template").as_str() != Some(template_path)
            || !same(get(node, "type"), get(template, "type"))
            || !same(get(node, "target"), get(template, "target"))
            || !same(get(node, "facts"), get(template, "facts"))
            || !index_ok
            || parent_of(path) != parent
            || !name
                .strip_prefix(PROBE_PREFIX)
                .is_some_and(|identity| uuid_valid(&string(identity)))
            || seen.contains(path)
            || !same(get(node, "parent_ref"), &reference_value)
        {
            return Err(changed());
        }
        if get(node, "type").as_str() == Some("file") {
            let nonce =
                hex_bytes(get(node, "nonce")).ok_or_else(|| err("Invalid metadata probe nonce"))?;
            if nonce.len() != NONCE_BYTES
                || get(node, "nonce").as_str() != Some(hex(&nonce).as_str())
            {
                return Err(err("Invalid metadata probe nonce"));
            }
        } else if !matches!(get(node, "nonce"), Value::Null) {
            return Err(err("Invalid metadata directory probe nonce"));
        }
        let parent_pin = get(node, "parent_pin");
        if reference.is_some() && !matches!(parent_pin, Value::Null) {
            return Err(err("Invalid metadata probe parent reference"));
        }
        if !matches!(parent_pin, Value::Null)
            && (!has_keys(parent_pin, &["dev", "ino"])
                || int(get(parent_pin, "dev")).is_none()
                || int(get(parent_pin, "ino")).is_none())
        {
            return Err(err("Invalid metadata probe parent pin"));
        }
        seen.insert(path.to_owned());
        mapped.insert(template_path.to_owned(), path.to_owned());
    }
    let listing = Descriptor::open_directory(path)
        .and_then(|directory| directory.entries_bounded(raw::MAX_ENTRIES))
        .map_err(io_error)?
        .ok_or_else(|| err("Unknown metadata probe attempt evidence"))?;
    if listing.iter().any(|entry| {
        !matches!(
            entry.to_str(),
            Some("plan.json" | "pins" | "completion.json")
        )
    }) {
        return Err(err("Unknown metadata probe attempt evidence"));
    }
    let completion = state::read(
        &path.join("completion.json"),
        sharing_limits::MAX_PRIVATE,
        true,
    )?;
    if let Some(completion) = completion {
        let expected = object([
            ("schema", string(COMPLETION_SCHEMA)),
            ("operation_id", get(plan, "operation_id").clone()),
            ("attempt_id", get(&value, "attempt_id").clone()),
            ("plan_sha256", string(value_digest(&value)?)),
        ]);
        if !same(&completion, &expected) {
            return Err(err("Invalid metadata probe completion"));
        }
    }
    let pins_dir = path.join("pins");
    if let Some(info) = lstat(&pins_dir)? {
        if info.is_symlink() || !info.is_directory() {
            return Err(err("Invalid metadata probe pin directory"));
        }
        let entries = Descriptor::open_directory(&pins_dir)
            .and_then(|directory| directory.entries_bounded(raw::MAX_ENTRIES))
            .map_err(io_error)?
            .ok_or_else(|| err("Unknown metadata probe pin evidence"))?;
        let mut expected_names = BTreeMap::new();
        for node in nodes {
            expected_names.insert(format!("{}.json", name_of(node_path(node)?)), node);
        }
        let mut known = Vec::new();
        for entry in &entries {
            let node = entry
                .to_str()
                .and_then(|label| expected_names.get(label))
                .ok_or_else(|| err("Unknown metadata probe pin evidence"))?;
            known.push((entry, *node));
        }
        for (entry, node) in known {
            let node_path = node_path(node)?;
            let pin = state::read(&pins_dir.join(entry), sharing_limits::MAX_PRIVATE, false)
                .ok()
                .flatten()
                .ok_or_else(|| ownership(node_path))?;
            let expected_nonce = if matches!(get(node, "nonce"), Value::Null) {
                Value::Null
            } else {
                nonce_digest(node).map_or(Value::Null, string)
            };
            if !has_keys(
                &pin,
                &[
                    "path",
                    "type",
                    "facts",
                    "flags",
                    "xattrs",
                    "parent",
                    "nonce_sha256",
                ],
            ) || get(&pin, "path").as_str() != Some(node_path)
                || !same(get(&pin, "type"), get(node, "type"))
                || get(&pin, "facts").as_object().is_none()
                || !has_keys(get(&pin, "parent"), &["dev", "ino"])
                || get(&pin, "xattrs").as_object().is_none()
                || !same(get(&pin, "nonce_sha256"), &expected_nonce)
            {
                return Err(ownership(node_path));
            }
        }
    }
    Ok(value)
}

fn progress_path(attempt_dir: &Path, node: &Value) -> Result<PathBuf> {
    Ok(attempt_dir
        .join("pins")
        .join(format!("{}.json", name_of(node_path(node)?))))
}

fn sorted_attempts(base: &Path) -> Result<Vec<PathBuf>> {
    let names = Descriptor::open_directory(base)
        .and_then(|directory| directory.entries_bounded(raw::MAX_ENTRIES))
        .map_err(io_error)?
        .ok_or_else(|| err("Metadata probe attempt capacity exceeded"))?;
    Ok(names.into_iter().map(|name| base.join(name)).collect())
}

fn parent_pin_from_record(record: &Value) -> Value {
    let facts = get(record, "facts");
    object([
        ("dev", get(facts, "dev").clone()),
        ("ino", get(facts, "ino").clone()),
    ])
}

fn checked_entry(
    bank: &Path,
    attempt_dir: &Path,
    node: &Value,
    by_path: &BTreeMap<String, &Value>,
    known: &BTreeSet<String>,
) -> Result<Option<Value>> {
    let path = node_path(node)?;
    let record = progress_path(attempt_dir, node)?;
    let actual = match probe_pin(bank, node) {
        Ok(actual) => actual,
        Err(Fault::Missing(_)) => return Ok(None),
        Err(other) => return Err(other.owned(path)),
    };
    let checked = || -> Result<(Value, Value)> {
        let pin = state::read(&record, sharing_limits::MAX_PRIVATE, true)?
            .filter(|pin| pin_matches(&actual, pin, true))
            .ok_or_else(|| ownership(path))?;
        let mut parent_pin = get(node, "parent_pin").clone();
        if let Some(reference) = get(node, "parent_ref").as_str() {
            let parent_node = by_path.get(reference).ok_or_else(|| ownership(path))?;
            let parent = state::read(
                &progress_path(attempt_dir, parent_node)?,
                sharing_limits::MAX_PRIVATE,
                true,
            )?;
            parent_pin = parent.as_ref().map_or(Value::Null, parent_pin_from_record);
        }
        Ok((pin, parent_pin))
    };
    let (pin, parent_pin) = checked().map_err(|_| ownership(path))?;
    if !same(get(&pin, "parent"), &parent_pin) {
        return Err(ownership(path));
    }
    if get(node, "type").as_str() == Some("file")
        && get(&pin, "nonce_sha256").as_str() != nonce_digest(node).as_deref()
    {
        return Err(ownership(path));
    }
    if get(node, "type").as_str() == Some("directory") {
        let descriptors = probe_parent(bank, path).map_err(Fault::contract)?;
        let opened = last(&descriptors)?
            .open_directory_at(OsStr::new(name_of(path)))
            .map_err(io_error)?;
        let children = opened
            .entries_bounded(raw::MAX_ENTRIES)
            .map_err(io_error)?
            .ok_or_else(|| ownership(path))?;
        for name in children {
            let child = join(path, &name.to_string_lossy());
            if !known.contains(&child) {
                return Err(ownership(&child));
            }
        }
        probe_bound(bank, path, &descriptors).map_err(Fault::contract)?;
        let info = opened.stat().map_err(io_error)?;
        let facts = get(&pin, "facts");
        if int(get(facts, "dev")) != Some(i128::from(info.dev))
            || int(get(facts, "ino")) != Some(i128::from(info.ino))
        {
            return Err(ownership(path));
        }
    }
    let record_text = host::text(record.as_os_str(), "metadata probe record path")?;
    let index = get(node, "index").clone();
    Ok(Some(with(
        pin,
        [("record", string(record_text)), ("index", index)],
    )))
}

pub fn probe_entries(bank: &Path, plan: &Value, directory: &Path) -> Result<Vec<Value>> {
    let base = directory.join(ATTEMPTS);
    let snapshot = attempt_budget(&base)?;
    if snapshot.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for attempt_dir in sorted_attempts(&base)? {
        let attempt = attempt_value(&attempt_dir, plan)?;
        let nodes = get(&attempt, "nodes").as_array().unwrap_or_default();
        let mut known = BTreeSet::new();
        let mut by_path = BTreeMap::new();
        for node in nodes {
            known.insert(node_path(node)?.to_owned());
            by_path.insert(node_path(node)?.to_owned(), node);
        }
        for node in nodes {
            if let Some(entry) = checked_entry(bank, &attempt_dir, node, &by_path, &known)? {
                result.push(entry);
            }
        }
    }
    if attempt_budget(&base)? != snapshot {
        return Err(err(
            "Metadata probe attempt evidence changed during observation",
        ));
    }
    Ok(result)
}

pub fn probe_clean(
    bank: &Path,
    pins: &[Value],
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<()> {
    let mut ordered = Vec::new();
    for pin in pins {
        let path = node_path(pin)?;
        ordered.push((std::cmp::Reverse(parts(path).len()), path.to_owned(), pin));
    }
    ordered.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    for (_, path, pin) in ordered {
        bank_observation::markers(bank, owner, transaction)?;
        let record_path = text(pin, "record")?;
        let record = state::read(Path::new(record_path), sharing_limits::MAX_PRIVATE, false)?
            .unwrap_or(Value::Null);
        let expected = Value::Object(
            pin.as_object()
                .unwrap_or_default()
                .iter()
                .filter(|(key, _)| key != "record" && key != "index")
                .cloned()
                .collect(),
        );
        if !same(&record, &expected) {
            return Err(ownership(&path));
        }
        let actual = probe_pin(bank, pin).map_err(|fault| fault.owned(&path))?;
        if !pin_matches(&actual, &expected, false) {
            return Err(ownership(&path));
        }
        let descriptors = probe_parent(bank, &path).map_err(Fault::contract)?;
        probe_bound(bank, &path, &descriptors).map_err(Fault::contract)?;
        let parent = last(&descriptors)?;
        let name = OsStr::new(name_of(&path));
        let info = parent.stat_at(name).map_err(io_error)?;
        let facts = get(pin, "facts");
        if int(get(facts, "dev")) != Some(i128::from(info.dev))
            || int(get(facts, "ino")) != Some(i128::from(info.ino))
        {
            return Err(ownership(&path));
        }
        if get(pin, "type").as_str() == Some("directory") {
            parent.remove_directory_at(name).map_err(io_error)?;
        } else {
            parent.unlink_at(name).map_err(io_error)?;
        }
        parent.fsync_directory().map_err(io_error)?;
        let index = index_text(pin);
        writes::fault(&format!("layout-recovery-probe-removed:{index}"))?;
    }
    Ok(())
}

pub fn probe_complete(bank: &Path, plan: &Value, directory: &Path) -> Result<()> {
    if !probe_entries(bank, plan, directory)?.is_empty() {
        return Err(err("Metadata probes remain pending"));
    }
    let base = directory.join(ATTEMPTS);
    if lstat(&base)?.is_none() {
        return Ok(());
    }
    for attempt_dir in sorted_attempts(&base)? {
        let attempt = attempt_value(&attempt_dir, plan)?;
        let completion = object([
            ("schema", string(COMPLETION_SCHEMA)),
            ("operation_id", get(plan, "operation_id").clone()),
            ("attempt_id", get(&attempt, "attempt_id").clone()),
            ("plan_sha256", string(value_digest(&attempt)?)),
        ]);
        attempt_store(directory, &attempt_dir.join("completion.json"), &completion)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn probe_clean_live(
    bank: &Path,
    node: &Value,
    descriptors: &[Descriptor],
    descriptor: &mut Descriptor,
    original: Identity,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<()> {
    let path = node_path(node)?;
    let run = |descriptor: &mut Descriptor| -> std::result::Result<(), Fault> {
        bank_observation::markers(bank, owner, transaction)?;
        probe_bound(bank, path, descriptors)?;
        let parent = last(descriptors)?;
        let name = OsStr::new(name_of(path));
        let info = descriptor.stat()?;
        let named = parent.stat_at(name)?;
        if !info.same_inode(original) || info != named {
            return Err(ownership(path).into());
        }
        if get(node, "type").as_str() == Some("file") {
            let body = hex_bytes(get(node, "nonce")).ok_or_else(|| ownership(path))?;
            let observed = descriptor
                .read_prefix(NONCE_BYTES + 1)
                .map_err(|_| ownership(path))?;
            let size = usize::try_from(info.size).unwrap_or(usize::MAX);
            if !info.is_regular()
                || info.nlink != 1
                || size > body.len()
                || Some(observed.as_slice()) != body.get(..size)
            {
                return Err(ownership(path).into());
            }
        } else {
            let empty = descriptor
                .entries_bounded(0)?
                .is_some_and(|names| names.is_empty());
            if !info.is_directory() || !empty {
                return Err(ownership(path).into());
            }
        }
        if descriptor.stat()? != info {
            return Err(ownership(path).into());
        }
        if get(node, "type").as_str() == Some("directory") {
            parent.remove_directory_at(name)?;
        } else {
            parent.unlink_at(name)?;
        }
        parent.fsync_directory()?;
        Ok(())
    };
    run(descriptor).map_err(|fault| fault.owned(path))
}

fn create_probe(
    parent: &Descriptor,
    node: &Value,
    path: &str,
) -> std::result::Result<Descriptor, Fault> {
    let name = OsStr::new(name_of(path));
    let mode = int(get(get(node, "facts"), "mode"))
        .and_then(|mode| u32::try_from(mode).ok())
        .ok_or_else(|| err("Invalid metadata probe facts"))?;
    let exists = |error: io::Error| -> Fault {
        if error.kind() == io::ErrorKind::AlreadyExists {
            Fault::Contract(ownership(path))
        } else {
            error.into()
        }
    };
    if get(node, "type").as_str() == Some("directory") {
        parent.make_directory_at(name, mode).map_err(exists)?;
        Ok(parent.open_directory_at(name)?)
    } else {
        parent
            .open_at(name, libc::O_CREAT | libc::O_EXCL | libc::O_RDWR, mode)
            .map_err(exists)
    }
}

/// Create, journal-pin and (on failure) live-clean one owned probe.
#[allow(clippy::too_many_arguments)]
fn place_probe(
    bank: &Path,
    directory: &Path,
    attempt_dir: &Path,
    node: &Value,
    descriptors: &[Descriptor],
    parent_pin: &Value,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<Value> {
    let path = node_path(node)?;
    bank_observation::markers(bank, owner, transaction)?;
    probe_bound(bank, path, descriptors).map_err(Fault::contract)?;
    let parent = last(descriptors)?;
    if !same(&pair(&parent.stat().map_err(io_error)?), parent_pin) {
        return Err(ownership(path));
    }
    let mut descriptor = create_probe(parent, node, path).map_err(Fault::contract)?;
    let mut original = None;
    let index = index_text(node);
    let mut body = |descriptor: &mut Descriptor| -> std::result::Result<Value, Fault> {
        let opened = descriptor.stat()?;
        original = Some(opened);
        if get(node, "type").as_str() == Some("file") {
            let nonce = hex_bytes(get(node, "nonce")).ok_or_else(|| ownership(path))?;
            descriptor.write_all(&nonce)?;
            descriptor.fsync()?;
        }
        let created = descriptor.stat()?;
        let named = parent.stat_at(OsStr::new(name_of(path)))?;
        if created != named {
            return Err(ownership(path).into());
        }
        parent.fsync_directory()?;
        writes::fault(&format!("layout-recovery-probe-before-pin:{index}"))?;
        let pin = probe_pin(bank, node)?;
        let facts = get(&pin, "facts");
        let nonce_ok = get(node, "type").as_str() != Some("file")
            || get(&pin, "nonce_sha256").as_str() == nonce_digest(node).as_deref();
        if !same(get(&pin, "parent"), parent_pin)
            || int(get(facts, "dev")) != Some(i128::from(created.dev))
            || int(get(facts, "ino")) != Some(i128::from(created.ino))
            || !nonce_ok
        {
            return Err(ownership(path).into());
        }
        attempt_store(directory, &progress_path(attempt_dir, node)?, &pin)?;
        Ok(pin)
    };
    match body(&mut descriptor) {
        Ok(pin) => {
            writes::fault(&format!("layout-recovery-probe-after-pin:{index}"))?;
            Ok(pin)
        }
        Err(fault) => {
            let Some(original) = original else {
                return Err(ownership(path));
            };
            probe_clean_live(
                bank,
                node,
                descriptors,
                &mut descriptor,
                original,
                owner,
                transaction,
            )?;
            Err(fault.contract())
        }
    }
}

fn index_text(node: &Value) -> String {
    match get(node, "index") {
        Value::Int(index) => index.to_string(),
        _ => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn probe_attempt(
    bank: &Path,
    plan: &Value,
    directory: &Path,
    requirements: Option<&Value>,
    directories: Option<&Value>,
    attempt_dir: &Path,
    attempt: &Value,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<()> {
    let attempt_dir = attempt_dir.to_path_buf();
    let mut covered_files = BTreeSet::new();
    let mut covered_directories = BTreeSet::new();
    let mut required_files = BTreeSet::new();
    match requirements {
        Some(Value::Array(items)) => {
            for item in items {
                required_files.insert(encoded(item)?);
            }
        }
        _ => {
            for task in get(plan, "tasks").as_array().unwrap_or_default() {
                if get(task, "type").as_str() == Some("file") {
                    required_files.insert(task_requirement(task)?);
                }
            }
        }
    }
    let required_directories: BTreeSet<String> = match directories {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => keys(get(plan, "directories"))
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    let nodes = get(&attempt, "nodes").as_array().unwrap_or_default();
    let mut by_path = BTreeMap::new();
    for node in nodes {
        by_path.insert(node_path(node)?.to_owned(), node);
    }
    for node in nodes {
        let path = node_path(node)?;
        let mut parent_pin = get(node, "parent_pin").clone();
        if let Some(reference) = get(node, "parent_ref").as_str() {
            let parent_node = by_path
                .get(reference)
                .ok_or_else(|| err("Invalid metadata probe parent reference"))?;
            let Some(parent_record) = state::read(
                &progress_path(&attempt_dir, parent_node)?,
                sharing_limits::MAX_PRIVATE,
                true,
            )?
            else {
                continue;
            };
            parent_pin = parent_pin_from_record(&parent_record);
        }
        if matches!(parent_pin, Value::Null) {
            continue;
        }
        let descriptors = probe_parent(bank, path).map_err(Fault::contract)?;
        let pin = place_probe(
            bank,
            directory,
            &attempt_dir,
            node,
            &descriptors,
            &parent_pin,
            owner,
            transaction,
        )?;
        drop(descriptors);
        let facts = get(&pin, "facts");
        let actual = object([
            ("uid", get(facts, "uid").clone()),
            ("gid", get(facts, "gid").clone()),
            (
                "mode",
                int(get(facts, "mode")).map_or(Value::Null, |mode| number(s_imode(mode))),
            ),
            ("xattrs", raw::metadata_supported(&bank.join(path))?),
        ]);
        if !same(&actual, get(node, "facts")) {
            return Err(err("pending-metadata-preservation"));
        }
        if get(node, "type").as_str() == Some("directory") {
            covered_directories.insert(text(node, "target")?.to_owned());
        } else {
            covered_files.insert(task_requirement(node)?);
        }
    }
    if !required_files.is_subset(&covered_files)
        || !required_directories.is_subset(&covered_directories)
    {
        return Err(err("Metadata probe parent is unavailable"));
    }
    Ok(())
}

/// Prove the filesystem reproduces each reviewed owner/group/mode/xattr set
/// with owned temporary probes, then remove every probe before mutation.
pub fn probe(
    bank: &Path,
    plan: &Value,
    directory: &Path,
    requirements: Option<&Value>,
    directories: Option<&Value>,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<()> {
    if !probe_entries(bank, plan, directory)?.is_empty() {
        return Err(err("Interrupted metadata probes require checked cleanup"));
    }
    let (attempt_dir, journal) = attempt(bank, plan, directory, requirements, directories)?;
    let outcome = probe_attempt(
        bank,
        plan,
        directory,
        requirements,
        directories,
        &attempt_dir,
        &journal,
        owner,
        transaction,
    );
    let cleanup = || -> Result<()> {
        let pins = probe_entries(bank, plan, directory)?;
        if !pins.is_empty() {
            probe_clean(bank, &pins, owner, transaction)?;
        }
        probe_complete(bank, plan, directory)?;
        writes::fault("layout-recovery-probe-directories-removed")?;
        Ok(())
    };
    match (outcome, cleanup()) {
        (_, Err(error)) if is_ownership(&error) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), _) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
