//! Explicit source-bound format conversion; runtime updates never invoke it.
//!
//! Baseline `migration.py`. The baseline located its trusted package from
//! `__file__`; this runtime is the running `am` binary plus the portable pack
//! it self-locates ([`Runtime`]). Migration codecs are compiled into that
//! binary (`migration-catalog-v2`), so a plan binds the runtime artifact
//! digest (pack inventory plus `bin/am`), and the retained recovery package
//! holds both the pack and the binary.
use crate::artifact::MigrationContract;
use crate::codec::json::Value;
use crate::developer_identity::{encoded, load_with_limit, sha256_hex, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{DirectoryChain, ReadError};
use crate::lifecycle::{get, string, text};
use crate::migration_contract::{Codecs, route};
use crate::pack_validation::CommandSurface;
use crate::{bank_contract as bank, bank_transactions as tx, scope_binding};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

mod command;
mod plan;
mod recovery;

pub use command::{Request, command, input_value};
pub use plan::{
    apply, check_code, check_source, converge, historical, marker_value, owned_marker, partial,
    preview, proposal_value, read_intent, read_journal, receipt_value, restoration_payload,
    restore_preview, stage,
};
pub use recovery::{RECOVERY_FIELDS, recover, recover_lock, recovery_view};

pub const SCHEMA: &str = "agent-memory/migration-proposal-v1";
pub const MAX_FILES: usize = 100_000;
pub const MAX_FILE: usize = 65536;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PLAN: usize = 32 * 1024 * 1024;
pub const MAX_RECORD: usize = 8 * MAX_BYTES + MAX_PLAN;
/// Baseline `router_metadata` header bound.
pub const MAX_ROUTER: usize = 16384;
pub const FIELDS: &[&str] = &[
    "schema",
    "operation_id",
    "repo_root",
    "bank_root",
    "project_id",
    "scope_context",
    "source_schema",
    "target_schema",
    "artifact_digest",
    "catalog_sha256",
    "chain",
    "source_hashes",
    "source_modes",
    "resulting_hashes",
    "resulting_modes",
    "preservation_sha256",
    "kind",
    "parent",
];
/// The retained trusted package below an operation's private directory.
pub const RETAINED: &str = "trusted-package";
/// Release layout of a runtime root: the portable pack and the binary.
pub const PACK_ENTRY: &str = "skills/agent-memory";
pub const BINARY_ENTRY: &str = "bin/am";
/// A development source keeps its prebuilt binary here.
pub const DEVELOPMENT_BINARY: &str = "target/release/am";
const LOCK: &str = ".agent-memory.lock";
const BUSY: &str = "Bank is busy/interrupted; use the matching owned recovery operation";

/// The trusted runtime a migration runs under (baseline `PACK`): a runtime
/// root holding the portable pack, the binary, and the codecs compiled into
/// that binary.
#[derive(Clone)]
pub struct Runtime {
    /// Root containing `skills/agent-memory` (the baseline `runtime_root`).
    pub root: PathBuf,
    pub pack: PathBuf,
    pub binary: PathBuf,
    codecs: Arc<dyn Codecs + Send + Sync>,
    surface: Option<Arc<dyn CommandSurface>>,
}

/// Catalog inspection with exactly the codecs this runtime invokes.
struct Catalog(Arc<dyn Codecs + Send + Sync>);

impl MigrationContract for Catalog {
    fn inspect(&self, root: &Path, compatibility: &Value, files: &Value) -> Result<Value> {
        crate::migration_contract::inspect(root, compatibility, files, self.0.as_ref())
    }
}

impl Runtime {
    /// A runtime from explicit parts; `surface` validates documented commands.
    pub fn new(
        root: &Path,
        binary: &Path,
        codecs: Arc<dyn Codecs + Send + Sync>,
        surface: Option<Arc<dyn CommandSurface>>,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            pack: root.join(PACK_ENTRY),
            binary: binary.to_path_buf(),
            codecs,
            surface,
        }
    }

    /// Self-locate the running executable's runtime: a release root
    /// (`<root>/bin/am`) or a development source (`<root>/target/release/am`).
    pub fn locate(
        executable: &Path,
        codecs: Arc<dyn Codecs + Send + Sync>,
        surface: Option<Arc<dyn CommandSurface>>,
    ) -> Result<Self> {
        let (root, binary) = layout(executable)?;
        Ok(Self::new(&root, &binary, codecs, surface))
    }

    /// The same codecs and surface for another (retained) runtime root.
    fn retained(&self, root: &Path) -> Self {
        Self::new(
            root,
            &root.join(BINARY_ENTRY),
            self.codecs.clone(),
            self.surface.clone(),
        )
    }

    /// `artifact.inspect(PACK)` of this runtime, binary included.
    pub fn inspect(&self) -> Result<Value> {
        let catalog = Catalog(self.codecs.clone());
        Ok(crate::artifact::inspect_runtime_with(
            &self.pack,
            &self.binary,
            self.surface.as_deref(),
            Some(&catalog),
        )?)
    }
}

/// `(runtime root, canonical binary)` of an executable in a runtime layout.
fn layout(executable: &Path) -> Result<(PathBuf, PathBuf)> {
    let binary = crate::fs::canonical_path(executable, true)?;
    let parent = binary.parent();
    let root = if binary.ends_with(BINARY_ENTRY) {
        parent.and_then(Path::parent)
    } else if binary.ends_with(DEVELOPMENT_BINARY) {
        parent.and_then(Path::parent).and_then(Path::parent)
    } else {
        None
    };
    let root = root.map(Path::to_path_buf).ok_or_else(|| {
        err("This executable is not inside a runtime with a portable pack; run the installed or retained am")
    })?;
    Ok((root, binary))
}

/// The runtime root of the running executable (baseline `runtime_root`).
pub fn executable_root() -> Result<PathBuf> {
    let executable =
        std::env::current_exe().map_err(|error| ContractError::new(error.to_string()))?;
    Ok(layout(&executable)?.0)
}

pub(crate) fn err(message: &str) -> ContractError {
    ContractError::new(message)
}

pub(crate) fn require(condition: bool, message: &str) -> Result<()> {
    if condition { Ok(()) } else { Err(err(message)) }
}

pub(crate) fn sha(raw: &[u8]) -> String {
    sha256_hex(raw)
}

/// Python `==` on decoded JSON: object key order is irrelevant.
pub(crate) fn same(left: &Value, right: &Value) -> bool {
    crate::installation::same(left, right)
}

pub(crate) fn hex64(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

pub(crate) fn entries(value: &Value) -> &[(String, Value)] {
    value.as_object().unwrap_or_default()
}

pub(crate) fn keys(value: &Value) -> BTreeSet<&str> {
    entries(value).iter().map(|(key, _)| key.as_str()).collect()
}

pub(crate) fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| ContractError::non_utf8("migration path"))
}

/// `target.relative_to(root).as_posix()`.
pub(crate) fn relative_text(root: &Path, target: &Path) -> Result<String> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| err("Migration bank must remain inside its repository"))?;
    Ok(path_text(relative)?.to_owned())
}

/// `result(operation, **values)`.
pub fn result(operation: &str, values: Vec<(&str, Value)>) -> Value {
    let mut fields = vec![
        (
            "schema".to_owned(),
            string("agent-memory/migration-result-v1"),
        ),
        ("operation".to_owned(), string(operation)),
        ("publication_verified".to_owned(), Value::Bool(false)),
        ("identity_authenticated".to_owned(), Value::Bool(false)),
        ("semantic_truth_verified".to_owned(), Value::Bool(false)),
    ];
    for (key, value) in values {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, old)) => *old = value,
            None => fields.push((key.to_owned(), value)),
        }
    }
    Value::Object(fields)
}

/// Locate a format-family header, not validate or adopt its memory.
pub fn router_metadata(raw: &[u8]) -> Result<Option<Value>> {
    crate::updating::router_metadata(raw).map_err(|error| match error {
        crate::installation::Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    })
}

fn router_probe(raw: &[u8]) -> Result<Option<bool>> {
    Ok(router_metadata(raw)?.map(|metadata| !entries(&metadata).is_empty()))
}

/// `(repository root, existing canonical bank)`; never initializes a bank.
pub fn locate(repo: &Path) -> Result<(PathBuf, PathBuf)> {
    let expanded = crate::host::expanduser(repo)?;
    let root = crate::fs::canonical_path(&expanded, true)?;
    let settings = crate::repository_config::settings_with(&root, Some(&router_probe), false)?;
    let target = root.join(&settings.bank_root);
    bank::bank_root(&target)?;
    let directory = lstat(&target)?.is_some_and(|info| info.is_directory());
    require(
        directory,
        "Migration requires the existing canonical bank; it never initializes one",
    )?;
    Ok((root, target))
}

/// A complete, bounded file map whose paths form a valid bank topology.
pub fn bounded_files(files: &Value) -> Result<&Value> {
    let listed = files.as_object();
    let bodies: Option<Vec<&str>> =
        listed.and_then(|pairs| pairs.iter().map(|(_, body)| body.as_str()).collect());
    let bounded = match (listed, bodies) {
        (Some(pairs), Some(bodies)) => {
            !pairs.is_empty()
                && files.get("MEMORY.md").is_some()
                && pairs.len() <= MAX_FILES
                && bodies.iter().all(|body| body.len() <= MAX_FILE)
                && bodies.iter().map(|body| body.len()).sum::<usize>() <= MAX_BYTES
        }
        _ => false,
    };
    require(
        bounded,
        "Complete migration source/result exceeds explicit file/byte bounds; nothing is truncated",
    )?;
    for (path, _) in entries(files) {
        if path != "MEMORY.md" {
            bank::path_value(&string(path.as_str()))?;
        }
    }
    bank::file_topology(entries(files).iter().map(|(path, _)| path.as_str()))?;
    Ok(files)
}

fn read_refusal(error: ReadError) -> ContractError {
    match error {
        ReadError::Io(error) => ContractError::new(error.to_string()),
        ReadError::InvalidPath => err("Migration paths must remain relative to their exact root"),
        ReadError::SourceChanged | ReadError::ParentChanged => {
            err("Migration source changed during its bounded read")
        }
        ReadError::NotRegular
        | ReadError::MultipleLinks
        | ReadError::InvalidSize
        | ReadError::LimitOverflow
        | ReadError::SizeLimit { .. } => {
            err("Migration reads bounded ordinary single-link files only")
        }
    }
}

/// Read through pinned no-follow directory descriptors, including ancestors:
/// `(bytes, permission bits)` of a bounded ordinary single-link file.
pub fn read_regular(directory: &Path, relative: &str, limit: usize) -> Result<(Vec<u8>, u32)> {
    let path = Path::new(relative);
    let normal = path
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    require(
        !relative.is_empty() && normal,
        "Migration paths must remain relative to their exact root",
    )?;
    crate::fs::read_regular(directory, path, limit).map_err(read_refusal)
}

/// `lstat` without following the final component; `None` when absent.
pub(crate) fn lstat(path: &Path) -> Result<Option<crate::fs::Identity>> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(err("Migration path has no parent directory"));
    };
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(error) if matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ENOTDIR)) => {
            return Ok(None);
        }
        Err(error) => return Err(ContractError::new(error.to_string())),
    };
    match chain.directory().stat_at(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(None),
        Err(error) => Err(ContractError::new(error.to_string())),
    }
}

pub(crate) fn lexists(path: &Path) -> Result<bool> {
    Ok(lstat(path)?.is_some())
}

/// Unlink one name through its pinned parent and flush the parent.
pub(crate) fn unlink(path: &Path) -> Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(err("Migration path has no parent directory"));
    };
    let chain = DirectoryChain::open(parent).map_err(|e| ContractError::new(e.to_string()))?;
    chain
        .directory()
        .unlink_at(name)
        .map_err(|e| ContractError::new(e.to_string()))?;
    tx::flush_directory(parent)
}

struct Inventory {
    files: Vec<(String, Value)>,
    modes: Vec<(String, Value)>,
    physical: Vec<String>,
    total: usize,
}

fn unreadable() -> ContractError {
    err("Migration inventory is unreadable; no source absence is inferred")
}

fn walk(target: &Path, relative: &str, inventory: &mut Inventory) -> Result<()> {
    let directory = if relative.is_empty() {
        target.to_path_buf()
    } else {
        target.join(relative)
    };
    let chain = DirectoryChain::open(&directory).map_err(|_| unreadable())?;
    let names = chain
        .directory()
        .entries_bounded(MAX_FILES)
        .map_err(|_| unreadable())?
        .ok_or_else(|| err("Migration source inventory exceeds its complete bound"))?;
    let mut directories = Vec::new();
    let mut leaves = Vec::new();
    for name in &names {
        let name = crate::host::text(name, "bank path")?;
        let path = if relative.is_empty() {
            name.to_owned()
        } else {
            format!("{relative}/{name}")
        };
        let info = chain
            .directory()
            .stat_at(OsStr::new(name))
            .map_err(|_| unreadable())?;
        require(!info.is_symlink(), "Migration cannot follow bank symlinks")?;
        inventory.physical.push(path.clone());
        require(
            inventory.physical.len() <= MAX_FILES,
            "Migration source inventory exceeds its complete bound",
        )?;
        if info.is_directory() {
            directories.push(path);
        } else {
            leaves.push(path);
        }
    }
    chain.recheck().map_err(|_| unreadable())?;
    for path in leaves {
        if bank::runtime_path(&path) {
            continue; // Preserved runtime aids are never canonical learning.
        }
        if path != "MEMORY.md" {
            bank::path_value(&string(path.as_str()))?;
        }
        let (raw, mode) = read_regular(target, &path, MAX_FILE)?;
        inventory.total += raw.len();
        let body = String::from_utf8(raw).map_err(|_| ContractError::non_utf8("bank source"))?;
        inventory.files.push((path.clone(), Value::String(body)));
        inventory.modes.push((path, Value::Int(mode.into())));
        require(
            inventory.files.len() <= MAX_FILES && inventory.total <= MAX_BYTES,
            "Migration source inventory exceeds its complete bound",
        )?;
    }
    for path in directories {
        if !bank::runtime_path(&path) {
            walk(target, &path, inventory)?;
        }
    }
    Ok(())
}

/// The complete canonical inventory `(files, modes)` of a quiescent bank, or
/// of one whose lock/marker bytes are the caller's own.
pub fn snapshot(
    target: &Path,
    owner: Option<&[u8]>,
    marker: Option<&[u8]>,
) -> Result<(Value, Value)> {
    for (name, expected) in [(LOCK, owner), (tx::MARKER, marker)] {
        if lexists(&target.join(name))? {
            let (raw, _) = read_regular(target, name, MAX_FILE)?;
            require(expected.is_some_and(|expected| raw == expected), BUSY)?;
        }
    }
    let mut inventory = Inventory {
        files: Vec::new(),
        modes: Vec::new(),
        physical: Vec::new(),
        total: 0,
    };
    walk(target, "", &mut inventory)?;
    bank::portable_aliases(inventory.physical.iter().map(String::as_str))?;
    let mut files = inventory.files;
    let mut modes = inventory.modes;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    modes.sort_by(|a, b| a.0.cmp(&b.0));
    let files = Value::Object(files);
    bounded_files(&files)?;
    Ok((files, Value::Object(modes)))
}

/// The trusted runtime's inspection; refuses a runtime without a catalog.
pub fn catalog(runtime: &Runtime) -> Result<Value> {
    crate::fs::trust::source(&runtime.pack)?;
    let measured = runtime.inspect()?;
    require(
        get(&measured, "migration_catalog") != &Value::Null,
        "This trusted runtime bundles no explicit migration catalog",
    )?;
    Ok(measured)
}

#[derive(Clone, Copy)]
enum Role {
    Validator,
    Converter,
}

/// Run one compiled codec named by an inspected catalog entry.
fn invoke(runtime: &Runtime, entry: &Value, role: Role, files: &Value) -> Result<Value> {
    let name = text(entry, "codec").unwrap_or_default();
    let function = match role {
        Role::Validator => runtime.codecs.validator(name),
        Role::Converter => runtime.codecs.converter(name),
    };
    let function = function.ok_or_else(|| err("Bundled migration entry is not callable"))?;
    function(files)
}

fn catalog_codecs(measured: &Value) -> &Value {
    get(get(measured, "migration_catalog"), "codecs")
}

/// Validate `files` as `schema` with its bundled validator; returns the
/// `{project_id, preservation_sha256}` projection.
pub fn validate_codec(
    runtime: &Runtime,
    measured: &Value,
    schema: &str,
    files: &Value,
) -> Result<Value> {
    bounded_files(files)?;
    let codec = catalog_codecs(measured)
        .get(schema)
        .ok_or_else(|| err("No bundled validator for the requested migration schema"))?;
    let value = invoke(runtime, codec, Role::Validator, files)?;
    let router = text(files, "MEMORY.md").unwrap_or_default();
    let header = router_metadata(router.as_bytes())?;
    let exact = value.as_object().is_some_and(|pairs| pairs.len() == 2)
        && value.get("project_id").is_some()
        && value.get("preservation_sha256").is_some();
    let valid = header.as_ref().is_some_and(|header| {
        text(header, "schema") == Some(schema)
            && exact
            && get(&value, "project_id") == get(header, "project-id")
            && uuid_valid(get(&value, "project_id"))
            && hex64(get(&value, "preservation_sha256"))
    });
    require(
        valid,
        "Migration codec did not validate exact schema, project and preservation projection",
    )?;
    Ok(value)
}

/// Convert `files` from `source` to `target` along the single bundled chain,
/// proving every step preserves the original projection.
pub fn transform(
    runtime: &Runtime,
    measured: &Value,
    source: &str,
    target: &str,
    files: &Value,
) -> Result<(Value, Vec<Value>, Value)> {
    let chain = route(get(measured, "migration_catalog"), source, target)?.ok_or_else(|| {
        err("No complete bundled migration chain supports this source and target")
    })?;
    let original = validate_codec(runtime, measured, source, files)?;
    let mut current = files.clone();
    for step in &chain {
        current = invoke(runtime, get(step, "converter"), Role::Converter, &current)?;
        let schema = text(step, "to").unwrap_or_default();
        let projection = validate_codec(runtime, measured, schema, &current)?;
        require(
            same(&projection, &original),
            "Migration changes project identity or its complete preservation projection",
        )?;
    }
    Ok((current, chain, original))
}

/// The resolved scope, read through the migration router reader.
pub fn context(
    root: &Path,
    project: &str,
    expected: Option<&Value>,
    owner: Option<&[u8]>,
    marker: Option<&[u8]>,
) -> Result<Value> {
    let reader = |raw: &[u8]| router_metadata(raw);
    scope_binding::require(
        root,
        Some(project),
        scope_binding::RequireOptions {
            expected,
            initialize: false,
            owned_lock: owner,
            owned_transaction: marker,
            migration_reader: Some(&reader),
        },
    )
}

pub fn hashes(files: &Value) -> Value {
    Value::Object(
        entries(files)
            .iter()
            .map(|(path, body)| {
                let raw = body.as_str().unwrap_or_default();
                (path.clone(), string(sha(raw.as_bytes())))
            })
            .collect(),
    )
}

/// Create missing private 0700 directories (each flushed) under a trusted
/// state path.
pub fn private_directory(path: &Path) -> Result<()> {
    crate::fs::trust::state(path)?;
    crate::fs::private_mkdir_with_sync(path, true)?;
    crate::fs::trust::state(path)?;
    Ok(())
}

fn installation_error(error: crate::installation::Error) -> ContractError {
    match error {
        crate::installation::Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    }
}

/// The private directory of one operation, outside the repository and every
/// installer-owned or executing runtime.
pub fn locations(
    runtime: &Runtime,
    root: &Path,
    target: &Path,
    operation_id: &Value,
) -> Result<PathBuf> {
    locations_at(&runtime.root, root, target, operation_id)
}

/// `locations` for the runtime rooted at `runtime_root`.
pub fn locations_at(
    runtime_root: &Path,
    root: &Path,
    target: &Path,
    operation_id: &Value,
) -> Result<PathBuf> {
    let operation_id = operation_id
        .as_str()
        .filter(|_| uuid_valid(operation_id))
        .ok_or_else(|| err("Migration operation requires its original canonical UUID"))?;
    let (operations, _) = crate::bank_writes::state_paths(root, target, operation_id)?;
    let state = operations
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| err("Migration recovery state has no state root"))?;
    let directory = state
        .join("migrations")
        .join(sha(path_text(target)?.as_bytes()))
        .join(operation_id);
    require(
        !directory.starts_with(root) && !root.starts_with(&directory),
        "Migration recovery state must remain outside the repository",
    )?;
    // All modes can write audits, including historical readback. Check actual
    // destinations before any bank lock, mkdir or record write. A retained
    // helper may share a state ancestor with new sibling operations.
    let env = crate::installation::Environment::process();
    let installed = crate::installation::locations(&env).map_err(installation_error)?;
    let mut runtime_roots = vec![
        runtime_root.to_path_buf(),
        installed.code,
        installed.runtime,
    ];
    let previous =
        crate::installation::read_receipt(&env, &installed.state).map_err(installation_error)?;
    if let Some(previous) = previous {
        for owned in get(&previous, "targets").as_array().unwrap_or_default() {
            let kind = text(owned, "kind");
            if matches!(kind, Some("source" | "copy" | "link"))
                && let Some(path) = text(owned, "path")
            {
                runtime_roots.push(PathBuf::from(path));
            }
        }
    }
    let audit_directory = operations
        .parent()
        .ok_or_else(|| err("Migration audit has no state directory"))?
        .join("security-audit");
    let records = ["intent.json", "backup.json", "journal.json", "receipt.json"]
        .map(|name| directory.join(name));
    let inside = records.iter().chain([&audit_directory]).any(|path| {
        runtime_roots
            .iter()
            .any(|protected| path.starts_with(protected))
    });
    require(
        !inside,
        "Migration records and audits must remain outside installer-owned and executing runtime",
    )?;
    let retained = directory.join(RETAINED);
    let overlaps = retained != runtime_root
        && runtime_roots
            .iter()
            .any(|protected| retained.starts_with(protected) || protected.starts_with(&retained));
    require(
        !overlaps,
        "Retained recovery code must not overwrite a trusted runtime",
    )?;
    crate::fs::trust::state(&audit_directory)?;
    crate::fs::trust::state(&directory)?;
    Ok(directory)
}

/// `(decoded record, exact bytes)` of a private state record.
pub fn read_record(path: &Path, limit: usize) -> Result<(Value, Vec<u8>)> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(err("Migration record has no parent directory"));
    };
    crate::fs::trust::state(parent)?;
    crate::fs::trust::record(path)?;
    let name = crate::host::text(name, "migration record")?;
    let (raw, _) = read_regular(parent, name, limit)?;
    Ok((load_with_limit(&raw, limit)?, raw))
}

/// Write a record once; an existing record must be byte-identical.
/// Returns the sha256 of its exact bytes.
pub fn keep_record(path: &Path, value: &Value) -> Result<String> {
    let payload = encoded(value)?;
    require(
        payload.len() <= MAX_RECORD,
        "Complete migration recovery record exceeds its bound",
    )?;
    let parent = path
        .parent()
        .ok_or_else(|| err("Migration record has no parent directory"))?;
    private_directory(parent)?;
    if lexists(path)? {
        let (_, raw) = read_record(path, MAX_RECORD)?;
        require(
            raw == payload,
            "Existing migration recovery record differs; preserve it",
        )?;
    } else {
        keep_blob(path, &payload, 0o600)?;
    }
    Ok(sha(&payload))
}

/// Durably publish `raw` with `mode` under a fresh name, never replacing an
/// existing node; an existing node must already hold the same bytes and mode.
pub fn keep_blob(path: &Path, raw: &[u8], mode: u32) -> Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(err("Migration record has no parent directory"));
    };
    private_directory(parent)?;
    if lexists(path)? {
        let name = crate::host::text(name, "migration record")?;
        let (actual, actual_mode) = read_regular(parent, name, MAX_RECORD.max(raw.len()))?;
        require(
            actual == raw && actual_mode == mode,
            "Existing migration backup/runtime file differs; preserve it",
        )?;
        return Ok(());
    }
    let io = |error: std::io::Error| ContractError::new(error.to_string());
    let chain = DirectoryChain::open(parent).map_err(io)?;
    let directory = chain.directory();
    let temporary = format!(".agent-memory-migration-{}", crate::host::uuid4()?);
    let temporary = OsStr::new(&temporary);
    let published = (|| -> Result<()> {
        let mut file = directory
            .create_exclusive(temporary, libc::O_WRONLY, mode)
            .map_err(io)?;
        file.write_all(raw).map_err(io)?;
        file.fsync().map_err(io)?;
        chain.recheck().map_err(io)?;
        // Existing native no-replace primitive.
        crate::fs::rename_at(
            directory,
            temporary,
            directory,
            name,
            crate::fs::RenameMode::NoReplace,
        )
        .map_err(io)
    })();
    if published.is_err() {
        let _ = directory.unlink_at(temporary);
    }
    published?;
    tx::flush_directory(parent)
}

/// Copy the measured trusted runtime (pack and binary) into the operation's
/// private directory once and prove it measures as the same artifact.
pub fn retain_runtime(runtime: &Runtime, measured: &Value, directory: &Path) -> Result<PathBuf> {
    let retained = directory.join(RETAINED);
    if retained != runtime.root {
        private_directory(&retained)?;
        for (name, identity) in entries(get(measured, "files")) {
            let (raw, mode) = if name == crate::artifact::BINARY_ENTRY {
                let parent = runtime
                    .binary
                    .parent()
                    .ok_or_else(|| err("Runtime binary has no parent directory"))?;
                let file = runtime
                    .binary
                    .file_name()
                    .ok_or_else(|| err("Runtime binary has no name"))?;
                let file = crate::host::text(file, "runtime binary")?;
                let limit = usize::try_from(crate::artifact::BINARY_MAX_BYTES)
                    .map_err(|_| err("Runtime binary bound exceeds this platform"))?;
                read_regular(parent, file, limit)?
            } else {
                read_regular(&runtime.pack, name, MAX_BYTES)?
            };
            let unchanged = text(identity, "sha256") == Some(sha(&raw).as_str())
                && text(identity, "mode") == Some(format!("0o{mode:o}").as_str());
            require(
                unchanged,
                "Trusted migration package changed before recovery retention",
            )?;
            let destination = if name == crate::artifact::BINARY_ENTRY {
                retained.join(BINARY_ENTRY)
            } else {
                retained.join(PACK_ENTRY).join(name)
            };
            keep_blob(&destination, &raw, mode)?;
        }
    }
    let inspected = runtime.retained(&retained).inspect()?;
    require(
        get(&inspected, "artifact_digest") == get(measured, "artifact_digest"),
        "Retained recovery package does not match the original trusted artifact",
    )?;
    Ok(retained)
}

/// The retained helper a caller runs without a global install.
pub fn retained_helper(directory: &Path) -> Result<Value> {
    Ok(string(path_text(
        &directory.join(RETAINED).join(BINARY_ENTRY),
    )?))
}

/// Baseline `security.audit(..., ['external'], action=..., bank_location=target)`.
pub(crate) fn audit(
    root: &Path,
    operation_id: &str,
    category: &str,
    phase: &str,
    scope: &Value,
    action: &str,
    target: &Path,
) -> Result<Value> {
    crate::security::audit(
        root,
        operation_id,
        category,
        phase,
        scope,
        &["external"],
        &[],
        &[],
        action,
        Some(target),
    )
}

/// Security's migration audit check: the bank is the actual existing
/// locator's and the operation's private locations are safe for the
/// runtime rooted at `runtime_root` (the running executable's when `None`).
pub struct LocationAudit {
    runtime_root: Option<PathBuf>,
}

impl LocationAudit {
    /// The production check for the running executable's runtime.
    pub fn executable() -> Self {
        Self { runtime_root: None }
    }

    /// The check for an explicitly selected runtime root.
    pub fn at(runtime_root: &Path) -> Self {
        Self {
            runtime_root: Some(runtime_root.to_path_buf()),
        }
    }
}

impl crate::security::MigrationAudit for LocationAudit {
    fn validate_location(&self, root: &Path, bank: &Path, operation_id: &str) -> Result<()> {
        let (located_root, located_bank) = locate(root)?;
        require(
            located_root == root && located_bank == bank,
            "Migration audit must use the actual existing bank locator",
        )?;
        let runtime_root = match &self.runtime_root {
            Some(runtime_root) => runtime_root.clone(),
            None => executable_root()?,
        };
        locations_at(&runtime_root, root, bank, &string(operation_id)).map(|_| ())
    }
}
