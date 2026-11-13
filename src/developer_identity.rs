//! Developer attribution data contracts.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::LazyLock;

pub static SPEC: LazyLock<Value> = LazyLock::new(|| {
    json::decode(
        include_str!("../skills/agent-memory/schemas/developers.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled developer schema")
});

pub fn uuid_valid(value: &Value) -> bool {
    let Some(s) = value.as_str() else {
        return false;
    };
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            14 => *c == b'4',
            19 => matches!(c, b'8' | b'9' | b'a' | b'b'),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}
pub fn digest(data: Option<&[u8]>) -> Option<String> {
    data.map(|v| crate::validation_memo::hex(&Sha256::digest(v)))
}
pub fn encoded(data: &Value) -> Result<Vec<u8>> {
    let mut s = json::encode(data, json::SORTED_INDENT2_UTF8_FINITE)?;
    s.push('\n');
    Ok(s.into_bytes())
}
pub fn load(data: &[u8]) -> Result<Value> {
    load_with_limit(data, REGISTRY_MAX_BYTES)
}
pub fn load_with_limit(data: &[u8], limit: usize) -> Result<Value> {
    json::decode_bytes(
        data,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            max_bytes: limit,
            ..DecodePolicy::default()
        },
    )
}
pub fn text(value: &Value, limit: usize) -> Result<&str> {
    value
        .as_str()
        .filter(|s| {
            crate::lifecycle::nonempty(s)
                && s.chars().count() <= limit
                && !s.chars().any(|c| (c as u32) < 32)
        })
        .ok_or_else(|| {
            ContractError::new("Identity metadata must be bounded, nonempty single-line text")
        })
}
fn fields(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|o| {
        o.len() == names.len() && o.iter().all(|(k, _)| names.contains(&k.as_str()))
    })
}
fn member(list: &Value, item: &Value) -> bool {
    list.as_array().is_some_and(|a| a.contains(item))
}
pub fn alias(value: &Value) -> Result<&Value> {
    if !fields(value, &["kind", "value"]) || !member(get(&SPEC, "alias-kinds"), get(value, "kind"))
    {
        return Err(ContractError::new(
            "Unknown identity alias fields or kind; harness/session are separate context",
        ));
    }
    text(get(value, "value"), 512)?;
    Ok(value)
}
pub fn aliases(values: &Value) -> Result<&Value> {
    let items = values
        .as_array()
        .ok_or_else(|| ContractError::new("Identity aliases must be a list"))?;
    let mut keys = HashSet::new();
    for v in items {
        alias(v)?;
        if !keys.insert((
            get(v, "kind").as_str().unwrap(),
            get(v, "value").as_str().unwrap(),
        )) {
            return Err(ContractError::new("Duplicate identity aliases"));
        }
    }
    Ok(values)
}
pub fn labels(values: &Value) -> Result<&Value> {
    let items = values
        .as_array()
        .ok_or_else(|| ContractError::new("Identity labels must be a list"))?;
    let mut keys = HashSet::new();
    for v in items {
        let s = text(v, 128)?;
        if !keys.insert(s) {
            return Err(ContractError::new("Duplicate identity labels"));
        }
    }
    Ok(values)
}
pub fn declared(values: &[String]) -> Result<Value> {
    let mut output = Vec::new();
    for item in values {
        let (kind, value) = item
            .split_once('=')
            .ok_or_else(|| ContractError::new("Aliases use kind=value"))?;
        let value = object([("kind", string(kind)), ("value", string(value))]);
        alias(&value)?;
        output.push(value);
    }
    let result = Value::Array(output);
    aliases(&result)?;
    Ok(result)
}
pub fn empty() -> Value {
    object([
        ("schema", get(&SPEC, "schema").clone()),
        ("selected", Value::Null),
        ("developers", object(std::iter::empty::<(&str, Value)>())),
        ("operations", object(std::iter::empty::<(&str, Value)>())),
    ])
}
fn hex64(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn validate(data: &Value) -> Result<&Value> {
    if !fields(data, &["schema", "selected", "developers", "operations"])
        || get(data, "schema") != get(&SPEC, "schema")
    {
        return Err(ContractError::new(
            "Unsupported attribution registry; preserve it for explicit repair",
        ));
    }
    let developers = get(data, "developers")
        .as_object()
        .ok_or_else(|| ContractError::new("Invalid attribution registry maps"))?;
    let operations = get(data, "operations")
        .as_object()
        .ok_or_else(|| ContractError::new("Invalid attribution registry maps"))?;
    for (id, dev) in developers {
        if !uuid_valid(&string(id)) || !fields(dev, &["labels", "aliases"]) {
            return Err(ContractError::new(
                "Registry stores stable developer IDs and identity metadata only",
            ));
        }
        labels(get(dev, "labels"))?;
        aliases(get(dev, "aliases"))?;
    }
    let selected = get(data, "selected");
    if selected != &Value::Null
        && !selected
            .as_str()
            .is_some_and(|s| developers.iter().any(|(id, _)| id == s))
    {
        return Err(ContractError::new("Selected developer is not registered"));
    }
    for (id, op) in operations {
        if !uuid_valid(&string(id))
            || !fields(op, &["proposal_sha256", "developer_id", "action", "at"])
        {
            return Err(ContractError::new("Invalid identity operation history"));
        }
        if !hex64(get(op, "proposal_sha256")) {
            return Err(ContractError::new("Invalid operation proposal hash"));
        }
        if !get(op, "developer_id")
            .as_str()
            .is_some_and(|s| developers.iter().any(|(id, _)| id == s))
            || !member(get(&SPEC, "actions"), get(op, "action"))
        {
            return Err(ContractError::new("Invalid identity operation binding"));
        }
        text(get(op, "at"), 512)?;
    }
    Ok(data)
}
pub fn proposal_value<'a>(proposal: &'a Value, registry: &std::path::Path) -> Result<&'a Value> {
    let p = proposal.get("proposal").unwrap_or(proposal);
    if !fields(
        p,
        &[
            "schema",
            "operation_id",
            "action",
            "developer_id",
            "labels",
            "aliases",
            "observed_sha256",
            "registry_path",
        ],
    ) {
        return Err(ContractError::new("Invalid developer proposal fields"));
    }
    if get(p, "schema").as_str() != Some("agent-memory/developer-proposal-v1")
        || !member(get(&SPEC, "actions"), get(p, "action"))
    {
        return Err(ContractError::new(
            "Unknown developer proposal schema/action",
        ));
    }
    if !uuid_valid(get(p, "operation_id")) || !uuid_valid(get(p, "developer_id")) {
        return Err(ContractError::new(
            "Invalid proposal operation/developer ID",
        ));
    }
    if get(p, "registry_path").as_str() != Some(path_text(registry)?) {
        return Err(ContractError::new(
            "Proposal belongs to a different identity config root",
        ));
    }
    if get(p, "observed_sha256") != &Value::Null && !hex64(get(p, "observed_sha256")) {
        return Err(ContractError::new("Invalid observed registry hash"));
    }
    labels(get(p, "labels"))?;
    aliases(get(p, "aliases"))?;
    Ok(p)
}

use crate::fs::{self, DirectoryChain, FlockGuard, LockError, LockMode, RenameMode};
use crate::host::{self, Sources};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
/// Main-approved local reader limits; oversized files are preserved.
pub const REGISTRY_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const LOCK_MAX_BYTES: usize = 1024 * 1024;

fn io_error(e: impl std::fmt::Display) -> ContractError {
    ContractError::new(e.to_string())
}
fn nullable(s: Option<String>) -> Value {
    s.map(string).unwrap_or(Value::Null)
}
fn path_text(p: &Path) -> Result<&str> {
    host::text(p.as_os_str(), "identity path")
}
fn exists(dir: &fs::Descriptor, name: &OsStr) -> Result<bool> {
    match dir.stat_at(name) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_error(e)),
    }
}

/// Resolve the configured identity root without following any existing symlink.
pub fn paths() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let raw = match std::env::var_os("AGENT_MEMORY_CONFIG_HOME") {
        Some(v) => PathBuf::from(v),
        None => {
            let base = match std::env::var_os("XDG_CONFIG_HOME") {
                Some(v) => PathBuf::from(v),
                None => host::expanduser(Path::new("~"))?.join(".config"),
            };
            base.join("agent-memory")
        }
    };
    let root = host::expanduser(&raw)?;
    if !root.is_absolute() {
        return Err(ContractError::new("Identity config root must be absolute"));
    }
    let mut ancestors: Vec<_> = root.ancestors().filter(|p| p.parent().is_some()).collect();
    ancestors.reverse();
    for part in ancestors {
        let parent = part.parent().unwrap();
        let chain = match DirectoryChain::open(parent) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => return Err(io_error(e)),
        };
        let Some(name) = part.file_name() else {
            continue;
        };
        match chain.directory().stat_at(name) {
            Ok(info) if info.is_symlink() => {
                return Err(ContractError::new(
                    "Identity config roots must not use symlinks; select an explicit canonical root",
                ));
            }
            Ok(info) if part == root && !info.is_directory() => {
                return Err(ContractError::new(
                    "Identity config root is not a directory",
                ));
            }
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => return Err(io_error(e)),
        }
    }
    let registry = root.join(get(&SPEC, "registry-file").as_str().unwrap());
    let lock = root.join(".developers.lock");
    match DirectoryChain::open(&root) {
        Ok(chain) => {
            for path in [&registry, &lock] {
                match chain.directory().stat_at(path.file_name().unwrap()) {
                    Ok(v) if v.is_symlink() => {
                        return Err(ContractError::new(
                            "Identity registry and lock must not be symlinks",
                        ));
                    }
                    Ok(_) => (),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(io_error(e)),
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(io_error(e)),
    }
    Ok((root, registry, lock))
}

fn read_named(root: &Path, name: &OsStr) -> Result<Option<Vec<u8>>> {
    let chain = match DirectoryChain::open(root) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let mut file = match chain
        .directory()
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
    {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let raw = file.read_bounded(REGISTRY_MAX_BYTES).map_err(|e| match e {
        fs::ReadError::SizeLimit { observed, .. } => {
            ContractError::local_registry_bound(REGISTRY_MAX_BYTES, observed)
        }
        other => io_error(other),
    })?;
    chain.recheck().map_err(io_error)?;
    Ok(Some(raw))
}
pub fn observe() -> Result<(Value, Option<String>)> {
    let (root, registry, _) = paths()?;
    let raw = read_named(&root, registry.file_name().unwrap())?;
    let data = match raw.as_deref() {
        Some(bytes) => {
            let v = load(bytes)?;
            validate(&v)?;
            v
        }
        None => empty(),
    };
    Ok((data, digest(raw.as_deref())))
}
pub fn result(
    operation: &str,
    fields: impl IntoIterator<Item = (impl Into<String>, Value)>,
) -> Value {
    let mut pairs = vec![
        ("schema".into(), string("agent-memory/developer-result-v1")),
        ("operation".into(), string(operation)),
        (
            "security_boundary".into(),
            get(&SPEC, "security-boundary").clone(),
        ),
        ("changed_memory_paths".into(), Value::Array(vec![])),
    ];
    pairs.extend(fields.into_iter().map(|(k, v)| (k.into(), v)));
    Value::Object(pairs)
}
fn lock_present(root: &Path, lock: &Path) -> Result<bool> {
    match DirectoryChain::open(root) {
        Ok(chain) => exists(chain.directory(), lock.file_name().unwrap()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_error(e)),
    }
}
pub fn status(
    observations: &[Value],
    harness: Option<&str>,
    session: Option<&str>,
    repo: &Path,
) -> Result<Value> {
    let (data, sha) = observe()?;
    let (root, registry, lock) = paths()?;
    let mut observed = observations.to_vec();
    let args: Vec<OsString> = ["config", "--null", "--get-regexp", r"^user\.(email|name)$"]
        .iter()
        .map(OsString::from)
        .collect();
    let configured = crate::identity_git::git(repo, &args, true, false).map_err(io_error)?;
    let mut email = String::new();
    let mut name = String::new();
    if let Some(configured) = configured {
        let configured = host::text(&configured, "Git attribution config evidence")?;
        let records = configured
            .strip_suffix('\0')
            .ok_or_else(|| ContractError::new("Incomplete Git attribution config evidence"))?;
        for record in records.split('\0') {
            let (key, value) = record.split_once('\n').unwrap_or((record, ""));
            match key {
                "user.email" => email = value.to_owned(),
                "user.name" => name = value.to_owned(),
                _ => {
                    return Err(ContractError::new(
                        "Unknown Git attribution config evidence",
                    ));
                }
            }
        }
    }
    let user = match std::env::var_os("USER") {
        Some(value) => host::text(&value, "OS user")?.to_owned(),
        None => String::new(),
    };
    for (kind, value) in [
        ("git-author-email", email),
        ("git-author-name", name),
        ("os-user", user),
    ] {
        if !value.is_empty() {
            let item = object([("kind", string(kind)), ("value", string(value))]);
            alias(&item)?;
            if !observed.contains(&item) {
                observed.push(item);
            }
        }
    }
    let mut evidence = Vec::new();
    let mut conflicts = Vec::new();
    for item in observed {
        let matches: Vec<Value> = get(&data, "developers")
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, entry)| get(entry, "aliases").as_array().unwrap().contains(&item))
            .map(|(id, _)| string(id))
            .collect();
        let state = if matches.is_empty() {
            "unknown"
        } else if matches.len() == 1 && matches[0] == *get(&data, "selected") {
            "matches-selected"
        } else {
            "conflicting"
        };
        let mut fields = item
            .as_object()
            .ok_or_else(|| {
                ContractError::new(
                    "Unknown identity alias fields or kind; harness/session are separate context",
                )
            })?
            .to_vec();
        fields.push(("matching_developer_ids".into(), Value::Array(matches)));
        fields.push(("state".into(), string(state)));
        let row = Value::Object(fields);
        if state == "conflicting" {
            conflicts.push(row.clone());
        }
        evidence.push(row);
    }
    Ok(result(
        "status",
        [
            ("developer_id", get(&data, "selected").clone()),
            ("registered_developers", get(&data, "developers").clone()),
            ("observed_sha256", nullable(sha)),
            ("registry_path", string(path_text(&registry)?)),
            ("observations", Value::Array(evidence)),
            ("attribution_conflicts", Value::Array(conflicts)),
            (
                "setup_required",
                Value::Bool(get(&data, "selected") == &Value::Null),
            ),
            ("identity_authenticated", Value::Bool(false)),
            ("lock_present", Value::Bool(lock_present(&root, &lock)?)),
            (
                "session_context",
                object([
                    ("harness", nullable(harness.map(str::to_owned))),
                    ("session", nullable(session.map(str::to_owned))),
                ]),
            ),
            ("writes_performed", Value::Bool(false)),
        ],
    ))
}
pub fn preview(
    action: &str,
    developer_id: Option<&str>,
    new_labels: &Value,
    new_aliases: &Value,
) -> Result<Value> {
    preview_with_sources(
        action,
        developer_id,
        new_labels,
        new_aliases,
        Sources::default(),
    )
}
pub fn preview_with_sources(
    action: &str,
    developer_id: Option<&str>,
    new_labels: &Value,
    new_aliases: &Value,
    sources: Sources,
) -> Result<Value> {
    let (data, sha) = observe()?;
    let (root, registry, lock) = paths()?;
    if lock_present(&root, &lock)? {
        return Err(ContractError::new(
            "Attribution registry is locked; inspect status/recover before retrying",
        ));
    }
    let id = if action == "create" {
        string((sources.uuid4)()?)
    } else {
        developer_id.map(string).unwrap_or(Value::Null)
    };
    if !uuid_valid(&id) {
        return Err(ContractError::new(
            "Developer ID must be a canonical random UUIDv4",
        ));
    }
    if action == "select"
        && (get(&data, "developers").get(id.as_str().unwrap()).is_none()
            || new_labels.as_array().is_none_or(|v| !v.is_empty())
            || new_aliases.as_array().is_none_or(|v| !v.is_empty()))
    {
        return Err(ContractError::new(
            "Selection requires an existing ID and does not modify aliases or labels",
        ));
    }
    let proposal = object([
        ("schema", string("agent-memory/developer-proposal-v1")),
        ("operation_id", string((sources.uuid4)()?)),
        ("action", string(action)),
        ("developer_id", id.clone()),
        ("labels", labels(new_labels)?.clone()),
        ("aliases", aliases(new_aliases)?.clone()),
        ("observed_sha256", nullable(sha)),
        ("registry_path", string(path_text(&registry)?)),
    ]);
    Ok(result(
        "preview",
        [
            ("developer_id", id),
            ("proposal", proposal),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ],
    ))
}

struct OwnedLock<'a> {
    dir: &'a fs::Descriptor,
    name: &'a OsStr,
    inode: fs::Identity,
    bytes: Vec<u8>,
    finished: bool,
}
impl OwnedLock<'_> {
    fn finish(&mut self) -> Result<()> {
        self.finished = true;
        let named = match self.dir.stat_at(self.name) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io_error(e)),
        };
        if named.is_symlink() || !named.same_inode(self.inode) {
            return Ok(());
        }
        let mut current = self
            .dir
            .open_at(self.name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            .map_err(io_error)?;
        if !current.stat().map_err(io_error)?.same_inode(self.inode) {
            return Ok(());
        }
        let bytes = match current.read_bounded(self.bytes.len()) {
            Ok(v) => v,
            Err(fs::ReadError::SizeLimit { .. }) => return Ok(()),
            Err(e) => return Err(io_error(e)),
        };
        if bytes == self.bytes {
            self.dir.unlink_at(self.name).map_err(io_error)?;
        }
        Ok(())
    }
}
impl Drop for OwnedLock<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.finish();
        }
    }
}
struct Temp<'a> {
    dir: &'a fs::Descriptor,
    name: OsString,
    present: bool,
}
impl Temp<'_> {
    fn finish(&mut self) -> Result<()> {
        if self.present {
            self.present = false;
            match self.dir.unlink_at(&self.name) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(io_error(e)),
            }
        }
        Ok(())
    }
}
impl Drop for Temp<'_> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
fn map_mut(value: &mut Value) -> &mut Vec<(String, Value)> {
    match value {
        Value::Object(v) => v,
        _ => unreachable!("validated registry object"),
    }
}
fn insert(value: &mut Value, key: &str, item: Value) {
    let map = map_mut(value);
    if let Some((_, v)) = map.iter_mut().find(|(k, _)| k == key) {
        *v = item;
    } else {
        map.push((key.to_owned(), item));
    }
}
pub fn apply(proposal: &Value) -> Result<Value> {
    apply_with_sources(proposal, Sources::default())
}
pub fn apply_with_sources(proposal: &Value, sources: Sources) -> Result<Value> {
    let (root, registry, lock) = paths()?;
    let p = proposal_value(proposal, &registry)?;
    let request_sha = digest(Some(&encoded(p)?)).unwrap();
    fs::private_mkdir(&root)?;
    let owner = object([
        ("operation_id", get(p, "operation_id").clone()),
        ("host", string(host::hostname()?)),
        ("pid", Value::Int(host::pid().into())),
        (
            "operation",
            string(format!("developer-{}", get(p, "action").as_str().unwrap())),
        ),
        ("start", string((sources.utc_now)()?)),
    ]);
    let bytes = encoded(&owner)?;
    let chain = DirectoryChain::open(&root).map_err(io_error)?;
    let dir = chain.directory();
    let lock_name = lock.file_name().unwrap();
    let file = dir
        .create_exclusive(lock_name, libc::O_WRONLY, 0o600)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                ContractError::new(
                    "Attribution registry is locked; stop and inspect status/recover",
                )
            } else {
                io_error(e)
            }
        })?;
    let inode = file.stat().map_err(io_error)?;
    let mut owned = OwnedLock {
        dir,
        name: lock_name,
        inode,
        bytes: bytes.clone(),
        finished: false,
    };
    let mut file = Some(file);
    let mut guard = None;
    let outcome = (|| -> Result<Value> {
        file.as_mut().unwrap().write_all(&bytes).map_err(io_error)?;
        file.as_ref().unwrap().fsync().map_err(io_error)?;
        match FlockGuard::try_acquire_owned(file.take().unwrap(), LockMode::Exclusive) {
            Ok(value) => guard = Some(value),
            Err((descriptor, error)) => {
                file = Some(descriptor);
                return Err(io_error(error));
            }
        }
        let (data, before) = observe()?;
        let op_id = get(p, "operation_id").as_str().unwrap();
        if let Some(known) = get(&data, "operations").get(op_id) {
            if get(known, "proposal_sha256").as_str() != Some(&request_sha) {
                return Err(ContractError::new(
                    "Operation ID was already used for a different proposal",
                ));
            }
            return Ok(result(
                "apply",
                [
                    ("operation_id", string(op_id)),
                    ("developer_id", get(&data, "selected").clone()),
                    ("operation_developer_id", get(known, "developer_id").clone()),
                    ("observed_sha256", nullable(before.clone())),
                    ("resulting_sha256", nullable(before)),
                    ("registry_path", string(path_text(&registry)?)),
                    ("already_applied", Value::Bool(true)),
                    ("writes_performed", Value::Bool(false)),
                    ("verified", Value::Bool(true)),
                ],
            ));
        }
        if get(p, "observed_sha256") != &nullable(before.clone()) {
            return Err(ContractError::new(
                "Attribution registry changed after preview; reread and prepare a fresh proposal",
            ));
        }
        let target = get(p, "developer_id").as_str().unwrap();
        let action = get(p, "action").as_str().unwrap();
        let existing = get(&data, "developers").get(target);
        if action == "create" && existing.is_some() {
            return Err(ContractError::new(
                "Create cannot replace a registered developer",
            ));
        }
        if action == "select"
            && (existing.is_none()
                || !get(p, "labels").as_array().unwrap().is_empty()
                || !get(p, "aliases").as_array().unwrap().is_empty())
        {
            return Err(ContractError::new(
                "Selection requires an existing ID and cannot change metadata",
            ));
        }
        let mut updated = data.clone();
        if action != "select" {
            let mut entry = existing.cloned().unwrap_or_else(|| {
                object([
                    ("labels", Value::Array(vec![])),
                    ("aliases", Value::Array(vec![])),
                ])
            });
            for key in ["labels", "aliases"] {
                let Value::Array(items) = entry.get_mut(key).unwrap() else {
                    unreachable!()
                };
                for item in get(p, key).as_array().unwrap() {
                    if !items.contains(item) {
                        items.push(item.clone());
                    }
                }
            }
            insert(updated.get_mut("developers").unwrap(), target, entry);
        }
        insert(&mut updated, "selected", string(target));
        let history = object([
            ("proposal_sha256", string(request_sha)),
            ("developer_id", string(target)),
            ("action", string(action)),
            ("at", string((sources.utc_now)()?)),
        ]);
        insert(updated.get_mut("operations").unwrap(), op_id, history);
        validate(&updated)?;
        let payload = encoded(&updated)?;
        let temp_name = OsString::from(format!(".developers-{}", (sources.uuid4)()?));
        let mut temp_file = dir
            .create_exclusive(&temp_name, libc::O_WRONLY, 0o600)
            .map_err(io_error)?;
        let mut temp = Temp {
            dir,
            name: temp_name,
            present: true,
        };
        let write_outcome = (|| -> Result<Value> {
            temp_file.write_all(&payload).map_err(io_error)?;
            temp_file.fsync().map_err(io_error)?;
            if observe()?.1 != before {
                return Err(ContractError::new(
                    "External registry edit observed immediately before replacement",
                ));
            }
            chain.recheck().map_err(io_error)?;
            fs::rename_at(
                dir,
                &temp.name,
                dir,
                registry.file_name().unwrap(),
                RenameMode::Replace,
            )
            .map_err(io_error)?;
            temp.present = false;
            dir.fsync_directory().map_err(io_error)?;
            let actual = read_named(&root,registry.file_name().unwrap())?.ok_or_else(|| ContractError::new("Attribution write needs inspection: readback differs; replay the same proposal"))?;
            if actual != payload {
                return Err(ContractError::new(
                    "Attribution write needs inspection: readback differs; replay the same proposal",
                ));
            }
            Ok(result(
                "apply",
                [
                    ("operation_id", string(op_id)),
                    ("developer_id", string(target)),
                    ("observed_sha256", nullable(before)),
                    ("resulting_sha256", nullable(digest(Some(&actual)))),
                    ("registry_path", string(path_text(&registry)?)),
                    ("already_applied", Value::Bool(false)),
                    ("writes_performed", Value::Bool(true)),
                    ("verified", Value::Bool(true)),
                ],
            ))
        })();
        temp.finish()?;
        write_outcome
    })();
    let cleanup = owned.finish();
    drop(guard);
    drop(file);
    cleanup?;
    outcome
}
pub fn recover(prepared: Option<&Value>) -> Result<Value> {
    recover_with_sources(prepared, Sources::default())
}
pub fn recover_with_sources(prepared: Option<&Value>, sources: Sources) -> Result<Value> {
    let (root, registry, lock) = paths()?;
    if !lock_present(&root, &lock)? {
        return Err(ContractError::new("No developer registry lock to recover"));
    }
    let chain = DirectoryChain::open(&root).map_err(io_error)?;
    let dir = chain.directory();
    let name = lock.file_name().unwrap();
    let file = dir
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    let mut guard = FlockGuard::try_acquire(file, LockMode::Exclusive).map_err(|e| match e {
        LockError::Busy => ContractError::new("Lock is held by a writer or another recovery; stop"),
        other => io_error(other),
    })?;
    let inode = guard.descriptor().stat().map_err(io_error)?;
    if !dir.stat_at(name).map_err(io_error)?.same_inode(inode) {
        return Err(ContractError::new("Lock changed before recovery inventory"));
    }
    let raw = guard
        .descriptor_mut()
        .read_bounded(LOCK_MAX_BYTES)
        .map_err(|e| match e {
            fs::ReadError::SizeLimit { .. } => ContractError::new(
                "Unrecognized developer lock requires operator review; preserve it",
            ),
            other => io_error(other),
        })?;
    let owner = load_with_limit(&raw, LOCK_MAX_BYTES)?;
    let operation = get(&owner, "operation")
        .as_str()
        .and_then(|s| s.strip_prefix("developer-"));
    if !fields(
        &owner,
        &["operation_id", "host", "pid", "operation", "start"],
    ) || !uuid_valid(get(&owner, "operation_id"))
        || !operation.is_some_and(|s| member(get(&SPEC, "actions"), &string(s)))
    {
        return Err(ContractError::new(
            "Unrecognized developer lock requires operator review; preserve it",
        ));
    }
    crate::lifecycle::time_value(get(&owner, "start"))?;
    let pid = get(&owner, "pid")
        .as_int()
        .and_then(|v| v.as_str().parse::<i32>().ok())
        .filter(|v| *v > 0);
    if get(&owner, "host").as_str() != Some(host::hostname()?.as_str()) || pid.is_none() {
        return Err(ContractError::new(
            "Foreign-host or malformed lock needs explicit operator review; no automatic removal",
        ));
    }
    match host::pid_state(pid.unwrap())? {
        host::PidState::Gone => (),
        host::PidState::Denied => {
            return Err(ContractError::new("Cannot prove lock owner is gone"));
        }
        host::PidState::Alive => {
            return Err(ContractError::new(
                "Lock owner is still alive; do not remove its lock",
            ));
        }
    }
    let hash = digest(Some(&raw)).unwrap();
    let path = path_text(&registry)?;
    let Some(prepared) = prepared else {
        return Ok(result(
            "recover-preview",
            [
                (
                    "proposal",
                    object([
                        ("schema", string("agent-memory/developer-lock-recovery-v1")),
                        ("registry_path", string(path)),
                        ("observed_lock_sha256", string(hash)),
                        ("operation_id", string((sources.uuid4)()?)),
                    ]),
                ),
                ("lock_owner", owner),
                ("writes_performed", Value::Bool(false)),
                ("verified", Value::Bool(false)),
            ],
        ));
    };
    let p = prepared.get("proposal").unwrap_or(prepared);
    if !fields(
        p,
        &[
            "schema",
            "registry_path",
            "observed_lock_sha256",
            "operation_id",
        ],
    ) || get(p, "schema").as_str() != Some("agent-memory/developer-lock-recovery-v1")
        || !uuid_valid(get(p, "operation_id"))
        || get(p, "registry_path").as_str() != Some(path)
        || get(p, "observed_lock_sha256").as_str() != Some(hash.as_str())
    {
        return Err(ContractError::new(
            "Lock changed or recovery proposal is invalid; inspect it again",
        ));
    }
    let current = dir.stat_at(name).map_err(io_error)?;
    let mut check = dir
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    if !current.same_inode(inode)
        || !check.stat().map_err(io_error)?.same_inode(inode)
        || check.read_bounded(LOCK_MAX_BYTES).map_err(|e| match e {
            fs::ReadError::SizeLimit { .. } => {
                ContractError::new("Lock changed immediately before recovery")
            }
            other => io_error(other),
        })? != raw
    {
        return Err(ContractError::new(
            "Lock changed immediately before recovery",
        ));
    }
    chain.recheck().map_err(io_error)?;
    dir.unlink_at(name).map_err(io_error)?;
    let replacement = match dir.stat_at(name) {
        Ok(v) => Some(v),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(e)),
    };
    Ok(result(
        "recover",
        [
            ("operation_id", get(p, "operation_id").clone()),
            ("writes_performed", Value::Bool(true)),
            (
                "replacement_lock_present",
                Value::Bool(replacement.is_some()),
            ),
            (
                "verified",
                Value::Bool(replacement.is_none_or(|v| !v.same_inode(inode))),
            ),
        ],
    ))
}

/// Turn a typed command's result into the baseline CLI receipt and output.
pub fn command_result(operation: &str, outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(e) => (
            result(
                operation,
                [
                    ("error", string(e.to_string())),
                    ("verified", Value::Bool(false)),
                ],
            ),
            1,
            json::DEFAULT,
        ),
    };
    let stdout = json::encode(&value, style).expect("developer result encoding") + "\n";
    crate::lifecycle::CommandResult {
        value: Some(value),
        exit_code,
        stdout,
        stderr: String::new(),
    }
}
