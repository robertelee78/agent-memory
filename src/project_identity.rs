//! Portable project identity and Python-era compatible local project bindings.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::developer_identity::{digest, encoded, load, text, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::host::{self, Sources};
use crate::identity_git;
use crate::lifecycle::{get, object, string};
use crate::{bank_contract, local_registry as storage, remote_identity, repository_config};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};

pub const ROUTER_MAX_BYTES: usize = 1_048_576;
pub const REGISTRY_MAX_BYTES: usize = crate::developer_identity::REGISTRY_MAX_BYTES;
pub const MARKER_MAX_BYTES: usize = storage::LOCK_MAX_BYTES;
const OPERATIONS: &[&str] = &[
    "project-create",
    "project-bind",
    "project-link",
    "project-rebind",
    "project-fork",
];
pub static SPEC: LazyLock<Value> = LazyLock::new(|| {
    // Invariant: this schema is a committed package asset, not runtime input.
    json::decode(
        include_str!("../skills/agent-memory/schemas/projects.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled project schema")
});

/// Implemented by bank_observation for raw recovery's owned-marker checks.
pub trait BankObservation: Send + Sync {
    fn markers(
        &self,
        bank: &Path,
        owned_lock: Option<&[u8]>,
        owned_transaction: Option<&[u8]>,
    ) -> Result<()>;
}
/// Implemented by bank_writes; its operation set authenticates a canonical writer.
pub trait BankWrites: Send + Sync {
    fn operations(&self) -> &[&str];
}
/// Implemented by layout_recovery_observation; returns independently pinned bytes.
pub trait LayoutObservation: Send + Sync {
    fn read_source(&self, bank: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>>;
}
static BANK_OBSERVATION: RwLock<Option<Arc<dyn BankObservation>>> = RwLock::new(None);
static BANK_WRITES: RwLock<Option<Arc<dyn BankWrites>>> = RwLock::new(None);
static LAYOUT_OBSERVATION: RwLock<Option<Arc<dyn LayoutObservation>>> = RwLock::new(None);
pub fn register_bank_observation(provider: Option<Arc<dyn BankObservation>>) -> Result<()> {
    *BANK_OBSERVATION
        .write()
        .map_err(|_| err("Project bank observation registry is poisoned"))? = provider;
    Ok(())
}
pub fn register_bank_writes(provider: Option<Arc<dyn BankWrites>>) -> Result<()> {
    *BANK_WRITES
        .write()
        .map_err(|_| err("Project bank writes registry is poisoned"))? = provider;
    Ok(())
}
pub fn register_layout_observation(provider: Option<Arc<dyn LayoutObservation>>) -> Result<()> {
    *LAYOUT_OBSERVATION
        .write()
        .map_err(|_| err("Project layout observation registry is poisoned"))? = provider;
    Ok(())
}

pub type MigrationReader<'a> = &'a dyn Fn(&[u8]) -> Result<Option<Value>>;
#[derive(Default, Clone, Copy)]
pub struct DiscoveryOptions<'a> {
    pub owned_lock: Option<&'a [u8]>,
    pub owned_transaction: Option<&'a [u8]>,
    pub migration_reader: Option<MigrationReader<'a>>,
    pub raw_identity: Option<&'a RawIdentityEvidence>,
}

/// Only `observe` can construct this evidence. It cannot be deserialized from a request.
#[derive(Debug, Clone)]
pub struct RawIdentityEvidence {
    repo_root: String,
    bank_path: String,
    router_sha256: Option<String>,
    metadata: Value,
    review: Option<Value>,
}
impl RawIdentityEvidence {
    pub fn observe(repo: &Path, raw: &[u8], review: Option<&Value>) -> Result<Self> {
        let root = fs::canonical_path(repo, false)?;
        let settings = repository_config::recovery_settings(&root)?;
        let bank = root.join(&settings.bank_root);
        if raw.len() > ROUTER_MAX_BYTES {
            return Err(err(
                "Recovery identity requires complete bounded router bytes",
            ));
        }
        let (metadata, retained_review) = match parse_metadata(raw) {
            Ok(metadata) => (metadata, None),
            Err(_) => {
                let review = review.ok_or_else(|| err("pending-identity-review"))?;
                if !fields(
                    review,
                    &[
                        "schema",
                        "review_id",
                        "project_id",
                        "repo_root",
                        "checkout_key",
                        "router_sha256",
                        "committed_router",
                        "committed_router_sha256",
                        "reviewer_developer_id",
                        "reviewer_harness",
                        "reviewer_session",
                        "author_session",
                        "reviewed_at",
                        "rationale",
                        "decision",
                    ],
                ) || get(review, "schema").as_str()
                    != Some("agent-memory/layout-identity-review-v1")
                    || get(review, "decision").as_str() != Some("accept")
                    || !uuid_valid(get(review, "review_id"))
                    || !uuid_valid(get(review, "project_id"))
                    || !uuid_valid(get(review, "reviewer_developer_id"))
                    || get(review, "repo_root").as_str() != Some(path_text(&root)?)
                    || get(review, "router_sha256") != &nullable(digest(Some(raw)))
                    || !truthy(get(review, "reviewer_session"))
                    || equivalent(
                        get(review, "reviewer_session"),
                        get(review, "author_session"),
                    )
                    || !truthy(get(review, "reviewer_harness"))
                    || !truthy(get(review, "rationale"))
                {
                    return Err(err("pending-identity-review"));
                }
                crate::lifecycle::time_value(get(review, "reviewed_at"))?;
                let expression = get(review, "committed_router").as_str().ok_or_else(|| {
                    err("Identity review requires an exact immutable committed router")
                })?;
                let (oid, relative) = expression.split_once(':').ok_or_else(|| {
                    err("Identity review requires an exact immutable committed router")
                })?;
                if !matches!(oid.len(), 40 | 64)
                    || !lower_hex(oid)
                    || relative != format!("{}/MEMORY.md", settings.bank_root)
                {
                    return Err(err(
                        "Identity review requires an exact immutable committed router",
                    ));
                }
                let committed = repository_config::object_router_bytes(
                    &root,
                    OsStr::new(expression),
                    &mut BTreeMap::new(),
                )?;
                let metadata = parse_metadata(&committed)?;
                let common = common_directory(&root)?;
                let key = checkout_key(&common)?;
                let (registry, _) = observe()?;
                let checkout =
                    get(&registry, "checkouts").get(required_text(review, "checkout_key")?);
                if get(review, "committed_router_sha256") != &nullable(digest(Some(&committed)))
                    || get(review, "checkout_key").as_str() != Some(key.as_str())
                    || checkout.is_none()
                    || checkout.is_some_and(|c| get(c, "project_id") != get(review, "project_id"))
                    || get(&metadata, "project-id") != get(review, "project_id")
                {
                    return Err(err(
                        "Identity review does not establish the existing checkout/project",
                    ));
                }
                (metadata, Some(review.clone()))
            }
        };
        Ok(Self {
            repo_root: path_text(&root)?.into(),
            bank_path: path_text(&bank)?.into(),
            router_sha256: digest(Some(raw)),
            metadata,
            review: retained_review,
        })
    }
    pub fn checked(&self, root: &Path, bank: &Path, raw: &[u8]) -> Result<Value> {
        if self.repo_root != path_text(root)?
            || self.bank_path != path_text(bank)?
            || self.router_sha256 != digest(Some(raw))
        {
            return Err(err("Raw recovery identity evidence changed"));
        }
        Ok(self.metadata.clone())
    }
    pub fn repo_root(&self) -> &str {
        &self.repo_root
    }
    pub fn bank_path(&self) -> &str {
        &self.bank_path
    }
    pub fn router_sha256(&self) -> Option<&str> {
        self.router_sha256.as_deref()
    }
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
    pub fn review(&self) -> Option<&Value> {
        self.review.as_ref()
    }
}

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn canonical_writer_error(error: storage::CanonicalWriterError) -> ContractError {
    match error {
        storage::CanonicalWriterError::Busy => ContractError::canonical_writer_busy(),
        storage::CanonicalWriterError::Initializing => {
            ContractError::canonical_writer_initializing()
        }
        storage::CanonicalWriterError::Contract(error) => error,
    }
}
fn nullable(value: Option<String>) -> Value {
    value.map(string).unwrap_or(Value::Null)
}
fn path_text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "project identity path")
}
fn required_text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid project identity evidence"))
}
fn fields(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == names.len() && pairs.iter().all(|(k, _)| names.contains(&k.as_str()))
    })
}
fn spec_fields(value: &Value, key: &str) -> bool {
    get(&SPEC, key).as_array().is_some_and(|names| {
        value.as_object().is_some_and(|pairs| {
            pairs.len() == names.len()
                && pairs
                    .iter()
                    .all(|(k, _)| names.iter().any(|name| name.as_str() == Some(k.as_str())))
        })
    })
}
fn action_valid(action: &str) -> bool {
    get(&SPEC, "actions")
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(action)))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Int(v) => v.as_str() != "0",
        Value::Float(v) => *v != 0.0,
    }
}
/// Python dict equality is independent of serialized insertion order.
fn equivalent(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|other| equivalent(value, other))
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        _ => left == right,
    }
}
fn insert(value: &mut Value, key: &str, item: Value) -> Result<()> {
    let Value::Object(pairs) = value else {
        return Err(err("Invalid project registry maps"));
    };
    if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *old = item;
    } else {
        pairs.push((key.into(), item));
    }
    Ok(())
}
fn append(value: &mut Value, item: Value) -> Result<()> {
    let Value::Array(items) = value else {
        return Err(err("Invalid project binding history"));
    };
    items.push(item);
    Ok(())
}
fn member(value: &Value, item: &Value) -> bool {
    value.as_array().is_some_and(|a| a.contains(item))
}
fn lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn parse_metadata(raw: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(raw).map_err(|_| err("Bank router must be valid UTF-8"))?;
    Ok(bank_contract::parse_router(text)?.0)
}
fn query(root: &Path, args: &[&str], optional: bool) -> identity_git::Result<Option<OsString>> {
    identity_git::git(
        root,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        optional,
        false,
    )
}
fn git_text(value: Option<OsString>) -> Result<Option<String>> {
    value
        .map(|v| host::text(&v, "Git identity output").map(str::to_owned))
        .transpose()
}
fn common_directory(root: &Path) -> Result<std::path::PathBuf> {
    let common = query(root, &["rev-parse", "--git-common-dir"], false)
        .map_err(io_error)?
        .ok_or_else(|| err("Cannot inspect local repository identity evidence"))?;
    fs::canonical_path(&root.join(common), false)
}
fn checkout_key(directory: &Path) -> Result<String> {
    let chain = DirectoryChain::open(directory).map_err(io_error)?;
    let info = chain.directory().stat().map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    Ok(format!("{}:{}", info.dev, info.ino))
}
fn stat_optional(root: &Path, name: &OsStr) -> Result<Option<fs::Identity>> {
    let chain = match DirectoryChain::open(root) {
        Ok(chain) => chain,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let result = match chain.directory().stat_at(name) {
        Ok(info) => Some(info),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(e)),
    };
    chain.recheck().map_err(io_error)?;
    Ok(result)
}
fn read_optional(root: &Path, name: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
    match fs::read_source(root, name, limit) {
        Ok(value) => Ok(Some(value.bytes)),
        Err(fs::ReadError::Io(e)) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(fs::ReadError::SizeLimit { observed, .. }) if limit == REGISTRY_MAX_BYTES => {
            Err(ContractError::local_registry_bound(limit, observed))
        }
        Err(e) => Err(io_error(e)),
    }
}

pub fn discover(
    repo: &Path,
    authority: Option<&str>,
    options: DiscoveryOptions<'_>,
) -> Result<Value> {
    let repo = fs::canonical_path(repo, false)?;
    DirectoryChain::open(&repo)
        .map_err(|_| err("Repository location must be an existing directory"))?;
    let top = query(&repo, &["rev-parse", "--show-toplevel"], true).map_err(io_error)?;
    let is_git = top.is_some();
    let root = match top {
        Some(top) => fs::canonical_path(Path::new(&top), false)?,
        None => repo,
    };
    let common = if is_git {
        common_directory(&root)?
    } else {
        root.clone()
    };
    let key = checkout_key(&common)?;
    let mut issues = Vec::new();
    let mut remotes = object([] as [(&str, Value); 0]);
    let mut configured = None;
    let mut configured_values = Vec::new();
    if is_git {
        let specs = [
            (
                ["config", "--get-all", "agent-memory.authorityRemote"]
                    .map(OsString::from)
                    .to_vec(),
                false,
            ),
            (vec![OsString::from("remote")], false),
        ];
        let (raw_configured, raw_names) = identity_git::_git_group(&root, &specs, |group| {
            let configured = group.value(0, true)?;
            let names = group.value(1, false)?;
            Ok((configured, names))
        })
        .map_err(io_error)?;
        let configured_text = git_text(raw_configured)?;
        if let Some(value) = configured_text.filter(|v| !v.is_empty()) {
            configured_values = value
                .lines()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if configured_values.len() != 1 && authority.is_none() {
                issues.push(string(
                    "Conflicting configured authority remotes; choose one explicitly",
                ));
            }
            configured = value.lines().last().map(str::to_owned);
        }
        let names = git_text(raw_names)?
            .ok_or_else(|| err("Cannot inspect local repository identity evidence"))?;
        for name in names.lines() {
            let mut candidates = Vec::new();
            let mut consistent = true;
            for flags in [
                ["remote", "get-url", "--all"].as_slice(),
                ["remote", "get-url", "--push", "--all"].as_slice(),
            ] {
                let mut args = flags.to_vec();
                args.push(name);
                let values = match query(&root, &args, false) {
                    Ok(value) => git_text(value)?,
                    Err(e) if e.contract().is_some() => {
                        consistent = false;
                        break;
                    }
                    Err(e) => return Err(io_error(e)),
                };
                for value in values.as_deref().unwrap_or("").lines() {
                    match remote_identity::normalize(&string(value), Some(&root)) {
                        Ok(identity) => {
                            let identity = string(identity);
                            if !candidates.contains(&identity) {
                                candidates.push(identity);
                            }
                        }
                        Err(_) => {
                            consistent = false;
                            break;
                        }
                    }
                }
                if !consistent {
                    break;
                }
            }
            if !consistent {
                candidates.clear();
            }
            consistent &= candidates.len() == 1;
            insert(
                &mut remotes,
                name,
                object([
                    ("identities", Value::Array(candidates)),
                    ("consistent", Value::Bool(consistent)),
                ]),
            )?;
        }
    }
    let mut selected = authority
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .or(configured);
    if let Some(name) = &selected {
        text(&string(name), 128)?;
        if name.starts_with('-') || remotes.get(name).is_none() {
            issues.push(string(
                "Configured authority remote is missing; choose an existing remote",
            ));
        }
    } else if let Some(entries) = remotes.as_object().filter(|entries| !entries.is_empty()) {
        let plausible: BTreeSet<_> = entries
            .iter()
            .flat_map(|(_, entry)| {
                get(entry, "identities")
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Value::as_str)
            })
            .collect();
        if remotes.get("origin").is_some()
            && plausible.len() == 1
            && entries
                .iter()
                .all(|(_, e)| get(e, "consistent").as_bool() == Some(true))
        {
            selected = Some("origin".into());
        } else {
            issues.push(string(
                "Multiple plausible remotes or no unambiguous origin; choose an authority remote",
            ));
        }
    }
    let mut canonical = Value::Null;
    if let Some(entry) = selected.as_deref().and_then(|name| remotes.get(name)) {
        if get(entry, "consistent").as_bool() == Some(true) {
            canonical = get(entry, "identities")
                .as_array()
                .and_then(|v| v.first())
                .cloned()
                .ok_or_else(|| err("Invalid remote identity evidence"))?;
        } else {
            issues.push(string("Authority fetch/push URLs do not identify one repository; repair or explicitly bind a consistent remote"));
        }
    }
    if options.raw_identity.is_some() && options.migration_reader.is_some() {
        return Err(err(
            "Recovery identity requires internally observed typed evidence",
        ));
    }
    let migration_probe = |raw: &[u8]| -> Result<Option<bool>> {
        match options.migration_reader {
            Some(reader) => Ok(reader(raw)?.map(|v| truthy(&v))),
            None => Ok(None),
        }
    };
    let settings = if options.raw_identity.is_some() {
        repository_config::recovery_settings(&root)?
    } else {
        repository_config::settings_with(
            &root,
            options
                .migration_reader
                .map(|_| &migration_probe as &dyn Fn(&[u8]) -> Result<Option<bool>>),
            false,
        )?
    };
    let bank = root.join(&settings.bank_root);
    if stat_optional(&root, OsStr::new(&settings.bank_root))?.is_some_and(|s| s.is_symlink())
        || stat_optional(&bank, OsStr::new("MEMORY.md"))?.is_some_and(|s| s.is_symlink())
    {
        return Err(err("Bank/router must not be symlinks"));
    }
    if options.raw_identity.is_some() {
        let provider = BANK_OBSERVATION
            .read()
            .map_err(|_| err("Project bank observation registry is poisoned"))?
            .clone()
            .ok_or_else(|| err("Project bank observation provider is not registered"))?;
        provider.markers(&bank, options.owned_lock, options.owned_transaction)?;
    } else {
        for (name, owned) in [
            (".agent-memory.lock", options.owned_lock),
            (".agent-memory-transaction.md", options.owned_transaction),
        ] {
            let info = stat_optional(&bank, OsStr::new(name))?;
            let Some(info) = info else {
                continue;
            };
            if info.is_symlink() {
                return Err(err(
                    "Bank busy or interrupted; inspect/recover before resolving identity",
                ));
            }
            let bytes = if owned.is_some() {
                read_optional(&bank, Path::new(name), MARKER_MAX_BYTES)?
            } else {
                None
            };
            if owned.is_none() || bytes.as_deref() != owned {
                if name == ".agent-memory.lock" && owned.is_none() {
                    let provider = BANK_WRITES
                        .read()
                        .map_err(|_| err("Project bank writes registry is poisoned"))?
                        .clone()
                        .ok_or_else(|| err("Project bank writes provider is not registered"))?;
                    storage::check_canonical_writer(&bank.join(name), provider.operations())
                        .map_err(canonical_writer_error)?;
                    if stat_optional(&bank, OsStr::new(name))?.is_none() {
                        continue;
                    }
                }
                return Err(err(
                    "Bank busy or interrupted; inspect/recover before resolving identity",
                ));
            }
        }
    }
    let (raw, metadata) = if let Some(identity) = options.raw_identity {
        let provider = LAYOUT_OBSERVATION
            .read()
            .map_err(|_| err("Project layout observation registry is poisoned"))?
            .clone()
            .ok_or_else(|| err("Project layout observation provider is not registered"))?;
        let raw = provider.read_source(&bank, Path::new("MEMORY.md"), ROUTER_MAX_BYTES)?;
        if raw.len() > ROUTER_MAX_BYTES {
            return Err(err(
                "Recovery identity requires complete bounded router bytes",
            ));
        }
        let metadata = identity.checked(&root, &bank, &raw)?;
        (Some(raw), metadata)
    } else {
        let raw = read_optional(&bank, Path::new("MEMORY.md"), ROUTER_MAX_BYTES)?;
        let metadata = match raw.as_deref() {
            Some(raw) => match options.migration_reader {
                Some(reader) => reader(raw)?.ok_or_else(|| {
                    err("Existing bank router is not a supported migration source")
                })?,
                None => parse_metadata(raw)?,
            },
            None => object([] as [(&str, Value); 0]),
        };
        (raw, metadata)
    };
    Ok(object([
        ("repo_root", string(path_text(&root)?)),
        ("is_git", Value::Bool(is_git)),
        (
            "common_git_dir",
            if is_git {
                string(path_text(&common)?)
            } else {
                Value::Null
            },
        ),
        ("checkout_key", string(&key)),
        ("observed_checkout_key", string(key)),
        ("location", string(path_text(&common)?)),
        ("authority_name", nullable(selected)),
        ("canonical_remote", canonical),
        (
            "configured_authority",
            Value::Array(configured_values.into_iter().map(string).collect()),
        ),
        ("remotes", remotes),
        ("bank_project_id", get(&metadata, "project-id").clone()),
        (
            "bank_canonical_remote",
            get(&metadata, "canonical-remote").clone(),
        ),
        (
            "repository_settings",
            object([
                ("bank_root", string(settings.bank_root)),
                ("authority_branch", nullable(settings.authority_branch)),
            ]),
        ),
        ("router_sha256", nullable(digest(raw.as_deref()))),
        ("issues", Value::Array(issues)),
    ]))
}

pub fn canonical(value: &Value) -> Result<()> {
    if value == &Value::Null {
        return Ok(());
    }
    if value.as_str().is_none()
        || remote_identity::normalize(value, None)? != value.as_str().unwrap_or("")
    {
        return Err(err(
            "Registry remote must be a sanitized canonical repository identifier",
        ));
    }
    Ok(())
}
fn key_parts(key: &str) -> Option<(&str, &str)> {
    let (device, inode) = key.split_once(':')?;
    (!device.is_empty()
        && !inode.is_empty()
        && device.bytes().all(|c| c.is_ascii_digit())
        && inode.bytes().all(|c| c.is_ascii_digit()))
    .then_some((device, inode))
}
fn decimal_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
fn key_compare(a: &str, b: &str) -> std::cmp::Ordering {
    match (key_parts(a), key_parts(b)) {
        (Some((ad, ai)), Some((bd, bi))) => {
            decimal_compare(ad, bd).then_with(|| decimal_compare(ai, bi))
        }
        _ => a.cmp(b),
    }
}
pub fn history(event: &Value, data: &Value) -> Result<()> {
    if !fields(
        event,
        &[
            "operation_id",
            "action",
            "project_id",
            "canonical_remote",
            "location",
            "reason",
            "at",
            "previous_project_id",
        ],
    ) || !uuid_valid(get(event, "operation_id"))
        || !get(event, "action").as_str().is_some_and(action_valid)
        || !get(event, "project_id")
            .as_str()
            .is_some_and(|id| get(data, "projects").get(id).is_some())
        || !get(event, "location")
            .as_str()
            .is_some_and(|v| Path::new(v).is_absolute())
    {
        return Err(err("Invalid project binding history"));
    }
    if get(event, "previous_project_id") != &Value::Null
        && !uuid_valid(get(event, "previous_project_id"))
    {
        return Err(err("Invalid previous project identity"));
    }
    canonical(get(event, "canonical_remote"))?;
    text(get(event, "reason"), 512)?;
    crate::lifecycle::time_value(get(event, "at"))?;
    Ok(())
}
pub fn validate(data: &Value) -> Result<&Value> {
    if !spec_fields(data, "registry-fields") || get(data, "schema") != get(&SPEC, "schema") {
        return Err(err(
            "Unknown project registry schema/fields; preserve for explicit repair",
        ));
    }
    if !["projects", "checkouts", "operations"]
        .iter()
        .all(|key| get(data, key).as_object().is_some())
    {
        return Err(err("Invalid project registry maps"));
    }
    let mut remotes = BTreeSet::new();
    for (project, entry) in get(data, "projects").as_object().unwrap_or(&[]) {
        if !uuid_valid(&string(project)) || !spec_fields(entry, "project-fields") {
            return Err(err("Invalid project registry entry"));
        }
        canonical(get(entry, "canonical_remote"))?;
        if let Some(remote) = get(entry, "canonical_remote").as_str() {
            if !remotes.insert(remote) {
                return Err(err("One canonical remote is bound to multiple project IDs"));
            }
        }
        let aliases = get(entry, "aliases")
            .as_array()
            .ok_or_else(|| err("Invalid project alias/history lists"))?;
        let events = get(entry, "history")
            .as_array()
            .ok_or_else(|| err("Invalid project alias/history lists"))?;
        let mut seen = Vec::new();
        for value in aliases {
            canonical(value)?;
            if seen.contains(&value) {
                return Err(err("Duplicate historical remote alias"));
            }
            seen.push(value);
        }
        for event in events {
            history(event, data)?;
        }
    }
    for (key, entry) in get(data, "checkouts").as_object().unwrap_or(&[]) {
        if key_parts(key).is_none() || !spec_fields(entry, "checkout-fields") {
            return Err(err("Invalid checkout binding"));
        }
        let locations = get(entry, "locations").as_array();
        if !get(entry, "project_id")
            .as_str()
            .is_some_and(|id| get(data, "projects").get(id).is_some())
            || locations.is_none_or(|v| v.is_empty())
        {
            return Err(err(
                "Checkout must reference a project and observed locations",
            ));
        }
        for location in locations.unwrap_or(&[]) {
            if !location
                .as_str()
                .is_some_and(|s| Path::new(s).is_absolute())
            {
                return Err(err("Checkout locations must be absolute"));
            }
        }
        if get(entry, "authority_name") != &Value::Null {
            text(get(entry, "authority_name"), 128)?;
        }
        let events = get(entry, "history")
            .as_array()
            .ok_or_else(|| err("Invalid checkout history"))?;
        for event in events {
            history(event, data)?;
        }
    }
    for (operation, value) in get(data, "operations").as_object().unwrap_or(&[]) {
        if !uuid_valid(&string(operation))
            || !fields(value, &["proposal_sha256", "project_id", "at"])
            || !get(value, "project_id")
                .as_str()
                .is_some_and(|id| get(data, "projects").get(id).is_some())
            || !get(value, "proposal_sha256")
                .as_str()
                .is_some_and(|v| v.len() == 64 && lower_hex(v))
        {
            return Err(err("Invalid project operation ledger"));
        }
        crate::lifecycle::time_value(get(value, "at"))?;
    }
    Ok(data)
}
pub fn observe() -> Result<(Value, Option<String>)> {
    let (root, _, _) = storage::paths("projects")?;
    let raw = read_optional(&root, Path::new("projects.json"), REGISTRY_MAX_BYTES)?;
    let data = match raw.as_deref() {
        Some(raw) => {
            let data = load(raw)?;
            validate(&data)?;
            data
        }
        None => object([
            ("schema", get(&SPEC, "schema").clone()),
            ("projects", object([] as [(&str, Value); 0])),
            ("checkouts", object([] as [(&str, Value); 0])),
            ("operations", object([] as [(&str, Value); 0])),
        ]),
    };
    Ok((data, digest(raw.as_deref())))
}

pub fn evidence(
    repo: &Path,
    data: &Value,
    authority: Option<&str>,
    options: DiscoveryOptions<'_>,
) -> Result<Value> {
    let mut initial = discover(repo, authority, options)?;
    let mut key = required_text(&initial, "checkout_key")?.to_owned();
    let mut known = get(data, "checkouts").get(&key);
    let mut historical = false;
    let mut continuity_issues = Vec::new();
    if known.is_none() {
        let (_, inode) = key_parts(&key).ok_or_else(|| err("Invalid checkout binding"))?;
        let mut candidates = get(data, "checkouts")
            .as_object()
            .unwrap_or(&[])
            .iter()
            .filter(|(k, v)| {
                key_parts(k).is_some_and(|(_, i)| i == inode)
                    && member(get(v, "locations"), get(&initial, "location"))
            })
            .collect::<Vec<_>>();
        let decisions: BTreeSet<_> = candidates
            .iter()
            .map(|(_, v)| {
                (
                    get(v, "project_id").as_str(),
                    get(v, "authority_name").as_str(),
                )
            })
            .collect();
        if decisions.len() > 1 {
            continuity_issues.push(string("Conflicting historical checkout bindings; inspect registry history and explicitly link or fork"));
        } else {
            candidates.sort_by(|(a, _), (b, _)| key_compare(a, b));
            if let Some((saved_key, entry)) = candidates.first() {
                key = saved_key.clone();
                known = Some(entry);
                historical = true;
            }
        }
    }
    if authority.is_none() && !truthy(get(&initial, "configured_authority")) {
        if let Some(entry) = known {
            if let Some(saved) = get(entry, "authority_name").as_str() {
                if get(&initial, "authority_name").as_str() != Some(saved) {
                    initial = discover(repo, Some(saved), options)?;
                }
            }
        }
    }
    if historical {
        let known = known.ok_or_else(|| err("Invalid checkout binding"))?;
        let project = get(known, "project_id")
            .as_str()
            .and_then(|id| get(data, "projects").get(id));
        let remote = get(&initial, "canonical_remote");
        if !truthy(get(&initial, "issues"))
            && remote != &Value::Null
            && project.is_some_and(|p| get(p, "canonical_remote") == remote)
            && get(&initial, "bank_project_id") == get(known, "project_id")
            && get(&initial, "bank_canonical_remote") == remote
        {
            insert(&mut initial, "checkout_key", string(key))?;
            insert(
                &mut initial,
                "binding_source",
                string("historical-device-change"),
            )?;
            insert(
                &mut initial,
                "continuity_issues",
                Value::Array(continuity_issues),
            )?;
            return Ok(initial);
        }
        continuity_issues.push(string("Historical checkout evidence does not establish matching bank and authority; restore the registered authority then explicitly link, or explicitly fork after reviewing identity evidence"));
    }
    let exact = get(data, "checkouts")
        .get(required_text(&initial, "checkout_key")?)
        .is_some();
    insert(
        &mut initial,
        "binding_source",
        string(if exact { "exact" } else { "unregistered" }),
    )?;
    insert(
        &mut initial,
        "continuity_issues",
        Value::Array(continuity_issues),
    )?;
    Ok(initial)
}
pub fn result(
    operation: &str,
    values: impl IntoIterator<Item = (impl Into<String>, Value)>,
) -> Value {
    let mut pairs = vec![
        ("schema".into(), string("agent-memory/project-result-v1")),
        ("operation".into(), string(operation)),
        (
            "security_boundary".into(),
            get(&SPEC, "security-boundary").clone(),
        ),
        ("changed_memory_paths".into(), Value::Array(Vec::new())),
    ];
    pairs.extend(values.into_iter().map(|(k, v)| (k.into(), v)));
    Value::Object(pairs)
}
pub fn status(repo: &Path) -> Result<Value> {
    status_with_options(repo, None, DiscoveryOptions::default())
}
pub fn status_with_options(
    repo: &Path,
    authority: Option<&str>,
    options: DiscoveryOptions<'_>,
) -> Result<Value> {
    let (data, sha) = observe()?;
    let proof = evidence(repo, &data, authority, options)?;
    let checkout = get(&data, "checkouts").get(required_text(&proof, "checkout_key")?);
    let project_id = if truthy(get(&proof, "bank_project_id")) {
        get(&proof, "bank_project_id").clone()
    } else {
        checkout
            .map(|c| get(c, "project_id").clone())
            .unwrap_or(Value::Null)
    };
    let mut issues = get(&proof, "issues").as_array().unwrap_or(&[]).to_vec();
    issues.extend_from_slice(get(&proof, "continuity_issues").as_array().unwrap_or(&[]));
    if project_id == Value::Null {
        issues.push(string(
            "No project identity; preview create, then initialize the bank",
        ));
    }
    match checkout {
        None => issues.push(string("Independent checkout is not registered; explicitly bind or link, or fork to a new project")),
        Some(checkout) if get(checkout, "project_id") != &project_id => issues.push(string("Bank ID conflicts with this checkout binding; explicit link/fork decision required")),
        _ => (),
    }
    let entry = project_id
        .as_str()
        .and_then(|id| get(&data, "projects").get(id));
    if entry.is_some_and(|e| get(e, "canonical_remote") != get(&proof, "canonical_remote")) {
        issues.push(string(
            "Unexpected authority remote change; explicit rebind or fork decision required",
        ));
    }
    if get(&proof, "bank_project_id") == &Value::Null {
        issues.push(string("Bank initialization or identity update is pending"));
    } else if get(&proof, "bank_canonical_remote") != get(&proof, "canonical_remote") {
        issues.push(string("Bank authority evidence differs or is missing; explicitly bind/rebind/fork and update bank identity"));
    }
    if entry.is_none() && project_id != Value::Null {
        issues.push(string(
            "Bank project ID is not locally registered; explicit bind or link required",
        ));
    }
    Ok(result(
        "status",
        [
            ("project_id", project_id),
            ("resolved", Value::Bool(issues.is_empty())),
            ("issues", Value::Array(issues)),
            ("evidence", proof.clone()),
            ("project_entry", entry.cloned().unwrap_or(Value::Null)),
            ("checkout_entry", checkout.cloned().unwrap_or(Value::Null)),
            ("observed_sha256", nullable(sha)),
            ("writes_performed", Value::Bool(false)),
        ],
    ))
}
pub fn plan(
    data: &Value,
    proof: &Value,
    action: &str,
    target: Option<&str>,
    reason: &str,
) -> Result<String> {
    plan_with_sources(data, proof, action, target, reason, Sources::default())
}
pub fn plan_with_sources(
    data: &Value,
    proof: &Value,
    action: &str,
    target: Option<&str>,
    reason: &str,
    sources: Sources,
) -> Result<String> {
    if !action_valid(action) || truthy(get(proof, "issues")) {
        return Err(err(
            "Repository authority is ambiguous; choose/repair a consistent authority remote before binding",
        ));
    }
    text(&string(reason), 512)?;
    if get(proof, "binding_source").as_str() == Some("historical-device-change") && action != "link"
    {
        return Err(err(
            "Persist the recovered checkout alias with an explicit link before rebind or fork",
        ));
    }
    let checkout = get(data, "checkouts").get(required_text(proof, "checkout_key")?);
    let bank_id = get(proof, "bank_project_id");
    let remote = get(proof, "canonical_remote");
    let target = if matches!(action, "create" | "fork") {
        let target = match target.filter(|v| !v.is_empty()) {
            Some(target) => target.to_owned(),
            None => (sources.uuid4)()?,
        };
        if !uuid_valid(&string(&target))
            || get(data, "projects").get(&target).is_some()
            || bank_id.as_str() == Some(target.as_str())
        {
            return Err(err("Create/fork requires a new random project ID"));
        }
        Some(target)
    } else {
        target.map(str::to_owned).or_else(|| {
            bank_id
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    checkout
                        .and_then(|c| get(c, "project_id").as_str())
                        .map(str::to_owned)
                })
        })
    };
    let target = target
        .filter(|v| uuid_valid(&string(v)))
        .ok_or_else(|| err("Resolve a stable bank project ID before binding/linking"))?;
    let matches_bank = bank_id.as_str() == Some(target.as_str());
    let registered = get(data, "projects").get(&target);
    if action == "create" && (bank_id != &Value::Null || checkout.is_some()) {
        return Err(err(
            "Existing bank/checkout requires bind, link, rebind or fork, not create",
        ));
    }
    if action == "fork" && bank_id == &Value::Null && checkout.is_none() {
        return Err(err("Fork requires an existing bank or checkout identity"));
    }
    if action == "bind" && (!matches_bank || registered.is_some() || checkout.is_some()) {
        return Err(err(
            "Bind adopts a previously unregistered bank; another independent location requires explicit link or fork",
        ));
    }
    if action == "link" && (registered.is_none() || !matches_bank) {
        return Err(err(
            "Link requires a registered project and matching bank ID",
        ));
    }
    if action == "rebind"
        && (!checkout.is_some_and(|c| get(c, "project_id").as_str() == Some(target.as_str()))
            || !matches_bank)
    {
        return Err(err(
            "Rebind requires the bank and current checkout to identify the same registered project",
        ));
    }
    if !matches!(action, "fork" | "rebind")
        && get(proof, "bank_canonical_remote") != &Value::Null
        && get(proof, "bank_canonical_remote") != remote
    {
        return Err(err(
            "Copied bank authority conflicts with the observed repository; explicit fork or rebind required",
        ));
    }
    if action == "link" && registered.is_some_and(|e| get(e, "canonical_remote") != remote) {
        return Err(err(
            "Linked clone authority differs; explicit rebind or fork required",
        ));
    }
    for (identity, entry) in get(data, "projects").as_object().unwrap_or(&[]) {
        if remote != &Value::Null && get(entry, "canonical_remote") == remote && identity != &target
        {
            return Err(err(
                "Authority already identifies another project; link that project rather than silently split its identity",
            ));
        }
    }
    Ok(target)
}
pub fn preview(
    repo: &Path,
    action: &str,
    target: Option<&str>,
    reason: &str,
    authority: Option<&str>,
) -> Result<Value> {
    preview_with_sources(repo, action, target, reason, authority, Sources::default())
}
pub fn preview_with_sources(
    repo: &Path,
    action: &str,
    target: Option<&str>,
    reason: &str,
    authority: Option<&str>,
    sources: Sources,
) -> Result<Value> {
    let (data, sha) = observe()?;
    let proof = evidence(repo, &data, authority, DiscoveryOptions::default())?;
    let target = plan_with_sources(&data, &proof, action, target, reason, sources)?;
    let (root, registry, _) = storage::paths("projects")?;
    if stat_optional(&root, OsStr::new(".projects.lock"))?.is_some() {
        return Err(err("Project registry is locked; inspect status/recover"));
    }
    let proposal = object([
        ("schema", string("agent-memory/project-proposal-v1")),
        ("operation_id", string((sources.uuid4)()?)),
        ("action", string(action)),
        ("project_id", string(&target)),
        ("reason", string(reason)),
        ("registry_path", string(path_text(&registry)?)),
        ("observed_sha256", nullable(sha)),
        ("evidence", proof.clone()),
        (
            "bank_identity",
            object([
                ("project-id", string(&target)),
                ("canonical-remote", get(&proof, "canonical_remote").clone()),
            ]),
        ),
    ]);
    Ok(result(
        "preview",
        [
            ("project_id", string(target)),
            ("proposal", proposal),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ],
    ))
}
pub fn apply(prepared: &Value) -> Result<Value> {
    apply_with_sources(prepared, Sources::default())
}
pub fn apply_with_sources(prepared: &Value, sources: Sources) -> Result<Value> {
    let proposal = prepared.get("proposal").unwrap_or(prepared);
    let (_, registry, lock) = storage::paths("projects")?;
    if !spec_fields(proposal, "proposal-fields")
        || get(proposal, "schema").as_str() != Some("agent-memory/project-proposal-v1")
        || !uuid_valid(get(proposal, "operation_id"))
        || get(proposal, "registry_path").as_str() != Some(path_text(&registry)?)
        || !get(proposal, "action").as_str().is_some_and(action_valid)
        || get(proposal, "evidence").as_object().is_none()
    {
        return Err(err("Invalid project proposal or config-root binding"));
    }
    let request_sha = digest(Some(&encoded(proposal)?));
    let operation_id = required_text(proposal, "operation_id")?;
    let action = required_text(proposal, "action")?;
    storage::exclusive(
        &lock,
        operation_id,
        &format!("project-{action}"),
        sources,
        |_| {
            let (data, sha) = observe()?;
            if let Some(known) = get(&data, "operations").get(operation_id) {
                if get(known, "proposal_sha256") != &nullable(request_sha.clone()) {
                    return Err(err("Operation ID already used for a different proposal"));
                }
                let current_status = match status(Path::new(required_text(
                    get(proposal, "evidence"),
                    "repo_root",
                )?)) {
                    Ok(status) => status,
                    Err(error) => {
                        let mut status = object([
                            ("resolved", Value::Bool(false)),
                            ("issues", Value::Array(vec![string(error.to_string())])),
                        ]);
                        merge(&mut status, &crate::bank_capacity::details(&error))?;
                        status
                    }
                };
                return Ok(result(
                    "apply",
                    [
                        ("operation_id", string(operation_id)),
                        ("project_id", get(known, "project_id").clone()),
                        ("already_applied", Value::Bool(true)),
                        ("registry_path", string(path_text(&registry)?)),
                        ("writes_performed", Value::Bool(false)),
                        ("verified", Value::Bool(true)),
                        ("binding_status", current_status),
                    ],
                ));
            }
            if nullable(sha.clone()) != *get(proposal, "observed_sha256") {
                return Err(err(
                    "Project registry changed after preview; reread before retrying",
                ));
            }
            let old_proof = get(proposal, "evidence");
            let proof = evidence(
                Path::new(required_text(old_proof, "repo_root")?),
                &data,
                get(old_proof, "authority_name").as_str(),
                DiscoveryOptions::default(),
            )?;
            if !equivalent(&proof, old_proof) {
                return Err(err(
                    "Repository/bank evidence changed after preview; prepare a fresh binding decision",
                ));
            }
            let target = plan_with_sources(
                &data,
                &proof,
                action,
                Some(required_text(proposal, "project_id")?),
                required_text(proposal, "reason")?,
                sources,
            )?;
            if !equivalent(
                get(proposal, "bank_identity"),
                &object([
                    ("project-id", string(&target)),
                    ("canonical-remote", get(&proof, "canonical_remote").clone()),
                ]),
            ) {
                return Err(err(
                    "Proposed bank identity differs from the resolved decision",
                ));
            }
            let mut updated = data.clone();
            let mut entry = get(&updated, "projects")
                .get(&target)
                .cloned()
                .unwrap_or_else(|| {
                    object([
                        ("canonical_remote", get(&proof, "canonical_remote").clone()),
                        ("aliases", Value::Array(Vec::new())),
                        ("history", Value::Array(Vec::new())),
                    ])
                });
            let old_remote = get(&entry, "canonical_remote").clone();
            if action == "rebind" && old_remote != *get(&proof, "canonical_remote") {
                if old_remote != Value::Null && !member(get(&entry, "aliases"), &old_remote) {
                    append(
                        entry
                            .get_mut("aliases")
                            .ok_or_else(|| err("Invalid project alias/history lists"))?,
                        old_remote,
                    )?;
                }
                insert(
                    &mut entry,
                    "canonical_remote",
                    get(&proof, "canonical_remote").clone(),
                )?;
            }
            let previous_project = get(&data, "checkouts")
                .get(required_text(&proof, "checkout_key")?)
                .map(|c| get(c, "project_id"))
                .filter(|v| truthy(v))
                .unwrap_or(get(&proof, "bank_project_id"));
            let event = object([
                ("operation_id", string(operation_id)),
                ("action", string(action)),
                ("project_id", string(&target)),
                ("canonical_remote", get(&proof, "canonical_remote").clone()),
                ("location", get(&proof, "location").clone()),
                ("reason", get(proposal, "reason").clone()),
                ("at", string(storage::now(sources)?)),
                ("previous_project_id", previous_project.clone()),
            ]);
            append(
                entry
                    .get_mut("history")
                    .ok_or_else(|| err("Invalid project alias/history lists"))?,
                event.clone(),
            )?;
            insert(
                updated
                    .get_mut("projects")
                    .ok_or_else(|| err("Invalid project registry maps"))?,
                &target,
                entry,
            )?;
            let mut checkout_key = required_text(&proof, "checkout_key")?;
            if action == "link"
                && get(&proof, "binding_source").as_str() == Some("historical-device-change")
            {
                checkout_key = required_text(&proof, "observed_checkout_key")?;
                if get(&updated, "checkouts").get(checkout_key).is_some() {
                    return Err(err(
                        "Observed checkout alias already exists; preserve its history and prepare a fresh decision",
                    ));
                }
                let prior = get(&updated, "checkouts")
                    .get(required_text(&proof, "checkout_key")?)
                    .cloned()
                    .ok_or_else(|| err("Invalid checkout binding"))?;
                insert(
                    updated
                        .get_mut("checkouts")
                        .ok_or_else(|| err("Invalid project registry maps"))?,
                    checkout_key,
                    prior,
                )?;
            }
            let mut checkout = get(&updated, "checkouts")
                .get(checkout_key)
                .cloned()
                .unwrap_or_else(|| {
                    object([
                        ("project_id", string(&target)),
                        ("locations", Value::Array(Vec::new())),
                        ("authority_name", Value::Null),
                        ("history", Value::Array(Vec::new())),
                    ])
                });
            insert(&mut checkout, "project_id", string(&target))?;
            insert(
                &mut checkout,
                "authority_name",
                get(&proof, "authority_name").clone(),
            )?;
            if !member(get(&checkout, "locations"), get(&proof, "location")) {
                append(
                    checkout
                        .get_mut("locations")
                        .ok_or_else(|| err("Invalid checkout binding"))?,
                    get(&proof, "location").clone(),
                )?;
            }
            append(
                checkout
                    .get_mut("history")
                    .ok_or_else(|| err("Invalid checkout history"))?,
                event.clone(),
            )?;
            insert(
                updated
                    .get_mut("checkouts")
                    .ok_or_else(|| err("Invalid project registry maps"))?,
                checkout_key,
                checkout,
            )?;
            insert(
                updated
                    .get_mut("operations")
                    .ok_or_else(|| err("Invalid project registry maps"))?,
                operation_id,
                object([
                    ("proposal_sha256", nullable(request_sha.clone())),
                    ("project_id", string(&target)),
                    ("at", get(&event, "at").clone()),
                ]),
            )?;
            storage::replace_with_sources(
                &registry,
                &encoded(validate(&updated)?)?,
                sha.as_deref(),
                sources,
            )?;
            Ok(result(
                "apply",
                [
                    ("operation_id", string(operation_id)),
                    ("project_id", string(target)),
                    ("already_applied", Value::Bool(false)),
                    ("registry_path", string(path_text(&registry)?)),
                    ("writes_performed", Value::Bool(true)),
                    ("verified", Value::Bool(true)),
                    (
                        "binding_status",
                        status(Path::new(required_text(&proof, "repo_root")?))?,
                    ),
                ],
            ))
        },
    )
}
fn merge(target: &mut Value, fields: &Value) -> Result<()> {
    for (key, value) in fields
        .as_object()
        .ok_or_else(|| err("Invalid project receipt fields"))?
    {
        insert(target, key, value.clone())?;
    }
    Ok(())
}
pub fn normalize(remote: &Value, base: Option<&Path>) -> Result<Value> {
    Ok(result(
        "normalize",
        [
            (
                "canonical_remote",
                string(remote_identity::normalize(remote, base)?),
            ),
            ("writes_performed", Value::Bool(false)),
        ],
    ))
}
pub fn recover(prepared: Option<&Value>) -> Result<Value> {
    recover_with_sources(prepared, Sources::default())
}
pub fn recover_with_sources(prepared: Option<&Value>, sources: Sources) -> Result<Value> {
    let (_, registry, lock) = storage::paths("projects")?;
    let recovery = storage::recover(
        &registry,
        &lock,
        OPERATIONS,
        prepared,
        "agent-memory/project-lock-recovery-v1",
        sources,
    )?;
    let mut receipt = result("recover", [] as [(&str, Value); 0]);
    merge(&mut receipt, &recovery)?;
    Ok(receipt)
}
/// Exact public CLI receipt, exit code, stdout and stderr for a typed operation.
pub fn command_result(operation: &str, outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let mut value = result(
                operation,
                [
                    ("error", string(error.to_string())),
                    ("verified", Value::Bool(false)),
                ],
            );
            if let Value::Object(fields) = crate::bank_capacity::details(&error) {
                if let Value::Object(pairs) = &mut value {
                    pairs.extend(fields);
                }
            }
            (value, 1, json::DEFAULT)
        }
    };
    match json::encode(&value, style) {
        Ok(stdout) => crate::lifecycle::CommandResult {
            value: Some(value),
            exit_code,
            stdout: stdout + "\n",
            stderr: String::new(),
        },
        Err(error) => crate::lifecycle::CommandResult {
            value: None,
            exit_code: 1,
            stdout: String::new(),
            stderr: error.to_string() + "\n",
        },
    }
}
