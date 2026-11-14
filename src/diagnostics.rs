//! Read-only runtime and repository diagnostics, without setup or repair writes.
//!
//! Baseline `diagnostics.py` (`am version`, `am status`, `am doctor`). The
//! executing package is the receipt-owned runtime of this executable or a
//! development source build (`<source>/target/release/am`); the Rust-era
//! receipt (v3) is audited target by target, a Python-era receipt (v1/v2) is
//! reported as requiring adoption. `projections` is reached through the
//! [`Projections`] seam and `memory_refresh` through `turn_check::MemoryRefresh`;
//! an unregistered callee is a reported component failure, never a pass.

use crate::codec::json::{self, Value};
use crate::error::{ContractError, Kind};
use crate::fs::{self, DirectoryChain, FlockGuard, LockError, LockMode};
use crate::installation::{self as install, Environment, HookCode, Services};
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::sharing_limits::truthy;
use crate::updating::merged;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "agent-memory/diagnostic-result-v1";
pub const RECOVER: &str = "Use a trusted complete package outside the affected targets: run its verified am binary with recover; inspect its result before retrying.";
/// Baseline `read` default bound.
pub const MAX_READ: usize = 1 << 20;
/// Bounded router and lock reads (baseline `read(path, 16384)` / `65536`).
pub const MAX_ROUTER: usize = 16384;
pub const MAX_LOCK: usize = 65536;
/// Purge journal inventory bound.
pub const MAX_PURGES: usize = 1024;
/// Bounded Git output for source identity queries.
const MAX_GIT: usize = 64 * 1024;
const GENERIC: &str = "Cannot verify this component from its current local evidence.";
const UNLOCATED: &str = "The executing runtime is neither a receipt-owned installed runtime nor a development source build; its package cannot be located.";
const COMPONENT_PERMISSIONS: &str = "Review this directory’s ownership and permissions; grant only the intended developer access, then rerun doctor.";
const TARGET_REPAIR: &str = "Preserve user edits and backups; compare this path with the ownership receipt. Preview reinstall from a trusted helper after explicit reconciliation; do not force replacement.";

// ----- failures -----

/// Why a component could not be verified. Only `Fixed` text (baseline
/// `DiagnosticFailure`) is exposed; every other refusal reports a bounded
/// component message, except validated bank capacity metadata.
#[derive(Debug)]
pub enum Failure {
    Fixed(String),
    Error(install::Error),
}
pub type Outcome<T> = std::result::Result<T, Failure>;

impl From<install::Error> for Failure {
    fn from(error: install::Error) -> Self {
        Self::Error(error)
    }
}
impl From<ContractError> for Failure {
    fn from(error: ContractError) -> Self {
        Self::Error(install::Error::Contract(error))
    }
}
impl From<crate::identity_git::Error> for Failure {
    fn from(error: crate::identity_git::Error) -> Self {
        match error.contract() {
            Some(contract) => contract.clone().into(),
            None => ContractError::new(error.to_string()).into(),
        }
    }
}
impl From<crate::hook_failure::Error> for Failure {
    fn from(error: crate::hook_failure::Error) -> Self {
        match error {
            crate::hook_failure::Error::NoRepository(error)
            | crate::hook_failure::Error::Contract(error) => error.into(),
        }
    }
}
impl From<crate::omp_installation::Error> for Failure {
    fn from(error: crate::omp_installation::Error) -> Self {
        Self::Error(error.into())
    }
}
impl From<crate::managed_source::Error> for Failure {
    fn from(error: crate::managed_source::Error) -> Self {
        Self::Error(error.into())
    }
}
impl From<crate::pack_validation::Invalid> for Failure {
    fn from(error: crate::pack_validation::Invalid) -> Self {
        Self::Error(error.into())
    }
}
impl From<fs::ReadError> for Failure {
    fn from(error: fs::ReadError) -> Self {
        Self::Error(error.into())
    }
}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Error(error.into())
    }
}
impl From<LockError> for Failure {
    fn from(error: LockError) -> Self {
        Self::Error(error.into())
    }
}
impl From<Failure> for install::Error {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Fixed(text) => install::Error::Contract(ContractError::new(text)),
            Failure::Error(error) => error,
        }
    }
}

fn fixed(text: impl Into<String>) -> Failure {
    Failure::Fixed(text.into())
}

fn require(condition: bool, message: &str) -> Outcome<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::new(message).into())
    }
}

// ----- small helpers -----

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn path_string(path: &Path) -> Value {
    string(display(path))
}

/// Python `dict[key] = value`: replace in place, otherwise append.
fn set(target: &mut Value, key: &str, value: Value) {
    if let Value::Object(fields) = target {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = value,
            None => fields.push((key.to_owned(), value)),
        }
    }
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// `identity_git.git(...)` as text; non-UTF-8 output is unavailable evidence.
fn git(root: &Path, args: &[&str], optional: bool) -> Outcome<Option<String>> {
    let output = crate::identity_git::git(root, &os(args), optional, false)?;
    output
        .map(|value| {
            value.into_string().map_err(|_| {
                Failure::from(ContractError::new(
                    "Repository memory inventory is unavailable; do not infer absence",
                ))
            })
        })
        .transpose()
}

/// Python `Path.exists()`: follows links; any resolution failure is absence.
fn exists(path: &Path) -> bool {
    fs::canonical_path(path, true).is_ok()
}

fn lexists(path: &Path) -> Outcome<bool> {
    Ok(install::lexists(path)?)
}

fn is_symlink(path: &Path) -> Outcome<bool> {
    if path.parent().is_none() {
        return Ok(false);
    }
    Ok(install::named(path)?.is_some_and(|info| info.is_symlink()))
}

/// Python `Counter(...)` in first-seen order.
fn counted<'a>(values: impl Iterator<Item = &'a Value>) -> Value {
    let mut counts: Vec<(String, u64)> = Vec::new();
    for value in values {
        let key = match value.as_str() {
            Some(text) => text.to_owned(),
            None => json::encode(value, json::DEFAULT).unwrap_or_default(),
        };
        match counts.iter_mut().find(|(name, _)| *name == key) {
            Some((_, count)) => *count += 1,
            None => counts.push((key, 1)),
        }
    }
    Value::Object(
        counts
            .into_iter()
            .map(|(key, count)| (key, Value::Int(count.into())))
            .collect(),
    )
}

/// Python `os.access(path, R_OK | W_OK | X_OK)` from the observed owner and
/// mode bits (group membership is not resolved; see the lane report).
fn accessible(info: &fs::Identity) -> bool {
    let uid = crate::host::effective_uid();
    if uid == 0 {
        return info.mode & 0o111 != 0;
    }
    let bits = if info.uid == uid {
        (info.mode >> 6) & 0o7
    } else {
        info.mode & 0o7
    };
    bits == 0o7
}

/// Python `shutil.which(name)` over the injected `PATH`.
pub(crate) fn which(env: &Environment, name: &str) -> bool {
    let Some(path) = env.var("PATH") else {
        return false;
    };
    std::env::split_paths(path).any(|directory| {
        let candidate = directory.join(name);
        fs::canonical_path(&candidate, true)
            .ok()
            .and_then(|resolved| install::named(&resolved).ok().flatten())
            .is_some_and(|info| info.is_regular() && info.mode & 0o111 != 0)
    })
}

// ----- bounded reads -----

/// Baseline `read`: never follow a redirected diagnostic record or load an
/// unlimited file.
pub fn read(path: &Path, maximum: usize) -> Outcome<Vec<u8>> {
    for node in path.ancestors() {
        require(!is_symlink(node)?, "Diagnostic path crosses a symlink")?;
    }
    require(
        install::is_regular(path)?,
        "Diagnostic input is not a regular file",
    )?;
    let (parent, name) = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => (parent, name),
        _ => return Err(ContractError::new("Diagnostic input is not a regular file").into()),
    };
    match fs::read_source(parent, Path::new(name), maximum) {
        Ok(observation) => Ok(observation.bytes),
        Err(fs::ReadError::SizeLimit { .. }) => {
            Err(ContractError::new("Diagnostic input exceeds its reader bound").into())
        }
        Err(error) => Err(error.into()),
    }
}

/// Baseline `record`: a bounded unique-key JSON record.
pub fn record(path: &Path, maximum: usize) -> Outcome<Value> {
    Ok(crate::developer_identity::load(&read(path, maximum)?)?)
}

// ----- sources -----

/// The executing package (baseline `artifact.PACK`) and its binary: the
/// receipt-owned runtime of this executable, otherwise a development source
/// build at `<source>/target/release/am` beside `<source>/skills/agent-memory`.
pub fn executing_package(env: &Environment, services: &Services) -> Outcome<(PathBuf, PathBuf)> {
    let executable = services
        .executable
        .as_deref()
        .ok_or_else(|| fixed(UNLOCATED))?;
    let executable = fs::canonical_path(executable, true)?;
    if let Ok(pack) = install::pack_root(env, &executable) {
        return Ok((pack, executable));
    }
    if executable.ends_with(install::DEVELOPMENT_BINARY) {
        if let Some(root) = executable.ancestors().nth(3) {
            let pack = root.join(install::RELEASE_PACK);
            if install::is_directory(&pack)? {
                return Ok((pack, executable));
            }
        }
    }
    Err(fixed(UNLOCATED))
}

/// Baseline `source`: identity of a runtime package. `binary` binds the
/// runtime binary into the artifact digest; `None` inspects the pack only
/// (legacy managed checkouts have no Rust binary).
pub fn source(
    services: &Services,
    pack: &Path,
    binary: Option<&Path>,
    managed: bool,
) -> Outcome<Value> {
    let pack = fs::canonical_path(pack, true)?;
    let skills = pack
        .parent()
        .filter(|parent| parent.file_name() == Some(OsStr::new("skills")));
    let root = match skills.and_then(Path::parent) {
        Some(root) => root.to_path_buf(),
        None => pack.clone(),
    };
    fs::trust::source(&root)?;
    if !lexists(&root.join(".git"))? && lexists(&root.join(crate::release_distribution::MANIFEST))?
    {
        let info = install::source_info(services, &root, "release")?;
        return Ok(merged(
            info.value,
            object([("cleanliness", string("verified_release"))]),
        ));
    }
    let inspected = match binary {
        Some(binary) => services.artifacts.inspect_runtime(&pack, binary)?,
        None => services.artifacts.inspect(&pack)?,
    };
    let local = |args: &[&str], optional: bool| -> Outcome<Option<String>> {
        if !managed {
            return git(&root, args, optional);
        }
        match crate::managed_source::git(&root, args, None, MAX_GIT) {
            Ok(raw) => String::from_utf8(raw)
                .map(|value| Some(value.trim().to_owned()))
                .map_err(|_| ContractError::new("Source Git output is not UTF-8").into()),
            Err(_) if optional => Ok(None),
            Err(error) => Err(error.into()),
        }
    };
    let top = local(&["rev-parse", "--show-toplevel"], true)?;
    let versioned = top.as_deref().is_some_and(|top| {
        fs::canonical_path(Path::new(top.trim()), false).is_ok_and(|top| top == root)
    });
    let unavailable = top.is_none() && lexists(&root.join(".git"))?;
    let revision = if versioned {
        local(&["rev-parse", "--verify", "HEAD"], false)?
    } else {
        None
    };
    let dirty = match (&revision, versioned, managed) {
        (Some(revision), true, true) => {
            match crate::managed_source::verify_payload(&root, revision) {
                Ok(_) => Some(false),
                Err(crate::managed_source::Error::PayloadMismatch(_)) => Some(true),
                Err(error) => return Err(error.into()),
            }
        }
        (_, true, _) => Some(
            !local(&["status", "--porcelain", "--untracked-files=all"], false)?
                .unwrap_or_default()
                .is_empty(),
        ),
        _ => None,
    };
    let cleanliness = if unavailable {
        "unavailable"
    } else if dirty == Some(true) {
        "dirty"
    } else if versioned {
        "clean"
    } else {
        "unversioned"
    };
    Ok(object([
        ("path", path_string(&root)),
        ("pack", path_string(&pack)),
        ("version", get(&inspected, "version").clone()),
        ("revision", revision.map_or(Value::Null, string)),
        ("dirty", dirty.map_or(Value::Null, Value::Bool)),
        ("cleanliness", string(cleanliness)),
        (
            "artifact_digest",
            get(&inspected, "artifact_digest").clone(),
        ),
        ("compatibility", get(&inspected, "compatibility").clone()),
    ]))
}

/// Baseline `channel`: never echo a credential-bearing remote URL.
pub fn channel(value: &Value) -> Outcome<Value> {
    let remote = match value.get("remote") {
        None | Some(Value::Null) => Value::Null,
        Some(remote) => string(crate::remote_identity::normalize(remote, None)?),
    };
    Ok(object([
        ("kind", get(value, "kind").clone()),
        ("ref", get(value, "ref").clone()),
        ("remote", remote),
    ]))
}

fn unverified(selection: &str) -> Value {
    object([
        ("publisher_authentication", string("not_established")),
        ("signature_verification_performed", Value::Bool(false)),
        ("recorded_source_selection", string(selection)),
        ("executing_verifier_authenticated", Value::Bool(false)),
        ("platform_acls_audited", Value::Bool(false)),
    ])
}

/// Baseline `security_summary`: verify the pinned signature and actual bytes;
/// never promote receipt claims. A Python-era receipt is never verified here.
pub fn security_summary(services: &Services, active: Option<&Value>) -> Outcome<Value> {
    let Some(active) = active else {
        return Ok(unverified("unavailable"));
    };
    let expected = get(active, "source");
    let kind = text(expected, "kind");
    if kind == Some("release") && !install::python_era_kind(active) {
        let root = PathBuf::from(text(expected, "path").unwrap_or_default());
        let observed = install::source_info(services, &root, "release")?;
        require(
            install::same(&observed.value, expected),
            "Signed source differs from its recorded activation",
        )?;
        return Ok(object([
            ("publisher_authentication", string("verified")),
            ("signature_verification_performed", Value::Bool(true)),
            ("recorded_source_selection", string("pinned_signed_release")),
            ("executing_verifier_authenticated", Value::Bool(false)),
            ("platform_acls_audited", Value::Bool(false)),
        ]));
    }
    let selection = match kind {
        Some("managed") => "unsigned_managed_git",
        Some("development") => "operator_selected_development",
        _ => "unavailable",
    };
    Ok(unverified(selection))
}

/// Baseline `distribution_notice`.
pub fn distribution_notice(selection: &str) -> &'static str {
    match selection {
        "pinned_signed_release" => {
            "The pinned publisher signature and installed portable bytes verify. Already-loaded verifier code and platform ACLs are not authenticated by this observation."
        }
        "unsigned_managed_git" => {
            "The receipt records unsigned managed Git. Artifact digests establish byte identity, not publisher authentication."
        }
        "operator_selected_development" => {
            "The receipt records an operator-selected development/custom source. Local selection and a digest do not establish publisher authentication."
        }
        _ => {
            "Publisher authentication is not established from the available installation evidence. The executing helper must be trusted separately."
        }
    }
}

// ----- report -----

/// Baseline `Report`: the diagnostic result under construction.
pub struct Report {
    value: Value,
}

impl Report {
    pub fn new(operation: &str) -> Self {
        Self {
            value: object([
                ("schema", string(SCHEMA)),
                ("operation", string(operation)),
                ("writes_performed", Value::Bool(false)),
                ("network_performed", Value::Bool(false)),
                ("repairs_performed", Value::Bool(false)),
                ("checks", Value::Array(Vec::new())),
                ("security", unverified("unavailable")),
            ]),
        }
    }

    /// Baseline `Report.add`.
    pub fn add(
        &mut self,
        component: &str,
        state: &str,
        detail: &str,
        repair: Option<&str>,
        path: Option<&Path>,
    ) {
        let mut item = object([
            ("component", string(component)),
            ("state", string(state)),
            ("detail", string(detail)),
        ]);
        if let Some(repair) = repair.filter(|repair| !repair.is_empty()) {
            set(&mut item, "repair", string(repair));
        }
        if let Some(path) = path {
            set(&mut item, "path", path_string(path));
        }
        if let Some(Value::Array(checks)) = self.value.get_mut("checks") {
            checks.push(item);
        }
    }

    fn last_check(&mut self) -> Option<&mut Value> {
        match self.value.get_mut("checks") {
            Some(Value::Array(checks)) => checks.last_mut(),
            _ => None,
        }
    }

    fn checks(&self) -> &[Value] {
        get(&self.value, "checks").as_array().unwrap_or(&[])
    }

    fn failed(&self, component: &str) -> bool {
        self.checks().iter().any(|check| {
            text(check, "component") == Some(component) && text(check, "state") == Some("failure")
        })
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    fn set(&mut self, key: &str, value: Value) {
        set(&mut self.value, key, value);
    }

    /// `report.value[outer][key] = value`; the outer object must exist.
    fn set_in(&mut self, outer: &str, key: &str, value: Value) -> Outcome<()> {
        match self.value.get_mut(outer) {
            Some(target @ Value::Object(_)) => {
                set(target, key, value);
                Ok(())
            }
            _ => Err(ContractError::new("Diagnostic report section is missing").into()),
        }
    }

    /// Baseline `Report.finish`.
    pub fn finish(mut self) -> Value {
        let select = |state: &str| -> Value {
            Value::Array(
                self.checks()
                    .iter()
                    .filter(|check| text(check, "state") == Some(state))
                    .cloned()
                    .collect(),
            )
        };
        let failures = select("failure");
        let warnings = select("warning");
        let ok = failures.as_array().is_some_and(<[Value]>::is_empty);
        self.set("failures", failures);
        self.set("warnings", warnings);
        self.set("ok", Value::Bool(ok));
        self.value
    }
}

fn capacity(error: &install::Error) -> Option<&ContractError> {
    match error {
        install::Error::Contract(error) if matches!(error.kind(), Kind::BankCapacity { .. }) => {
            Some(error)
        }
        _ => None,
    }
}

/// Baseline `Report.attempt`: a component failure never aborts the report.
pub fn attempt<T>(
    report: &mut Report,
    component: &str,
    repair: &str,
    path: Option<&Path>,
    body: impl FnOnce(&mut Report) -> Outcome<T>,
) -> Option<T> {
    match body(report) {
        Ok(value) => Some(value),
        Err(failure) => {
            failed(report, component, repair, path, failure);
            None
        }
    }
}

fn failed(
    report: &mut Report,
    component: &str,
    repair: &str,
    path: Option<&Path>,
    failure: Failure,
) {
    match failure {
        Failure::Fixed(detail) => report.add(component, "failure", &detail, Some(repair), path),
        Failure::Error(error) => {
            if let Some(error) = capacity(&error) {
                report.add(
                    component,
                    "failure",
                    error.message(),
                    Some(crate::bank_capacity::REPAIR),
                    path,
                );
                let details = crate::bank_capacity::details(error);
                if let Some(check) = report.last_check() {
                    for (key, value) in details.as_object().unwrap_or(&[]) {
                        set(check, key, value.clone());
                    }
                }
                return;
            }
            let detail = match &error {
                // A missing provider is a fixed, content-free refusal.
                install::Error::Unregistered(name) => format!(
                    "{name} is not registered for this runtime; this component cannot be inspected."
                ),
                install::Error::Contract(error)
                    if matches!(error.kind(), Kind::MissingProvider) =>
                {
                    error.message().to_owned()
                }
                _ => GENERIC.to_owned(),
            };
            report.add(component, "failure", &detail, Some(repair), path);
        }
    }
}

// ----- installation receipt -----

/// Baseline `receipt_view`: observe the receipt and journal again; never
/// carry a formerly applicable selection through a failed reread.
pub fn receipt_view(
    report: &mut Report,
    env: &Environment,
    services: &Services,
) -> Outcome<Option<Value>> {
    report.set("security", unverified("unavailable"));
    let state = install::locations(env)?.state;
    fs::trust::state(&state)?;
    let path = state.join("receipt.json");
    let mut active = None;
    report.set(
        "installation",
        object([
            ("state", string("not_installed")),
            ("receipt_path", path_string(&path)),
            ("receipt_schema", Value::Null),
            ("mode", Value::Null),
            ("enabled_adapters", Value::Array(Vec::new())),
            ("health", string("not_audited")),
            ("update_availability", string("not_cached_or_fetched")),
            ("purge_transactions", string("not_scanned")),
        ]),
    );
    if lexists(&path)? {
        fs::trust::record(&path)?;
        let raw = record(&path, MAX_READ)?;
        require(raw.as_object().is_some(), "Invalid receipt")?;
        active = install::read_receipt(env, &state)?;
        let schema = text(&raw, "schema");
        require(
            schema.is_some_and(|schema| {
                schema == install::RECEIPT_SCHEMA
                    || install::LEGACY_RECEIPT_SCHEMAS.contains(&schema)
            }),
            "Unknown receipt schema",
        )?;
        let source = raw
            .get("source")
            .ok_or_else(|| ContractError::new("Invalid receipt"))?;
        require(source.as_object().is_some(), "Invalid receipt")?;
        let preserved = match raw.get("preserved") {
            None => 0,
            Some(Value::Array(items)) => items.len(),
            Some(Value::Object(items)) => items.len(),
            Some(_) => return Err(ContractError::new("Invalid receipt").into()),
        };
        let recorded = object(
            [
                "path",
                "pack",
                "version",
                "revision",
                "artifact_digest",
                "dirty",
            ]
            .map(|key| (key, get(source, key).clone())),
        );
        let adapters = if active.is_some() {
            raw.get("enabled_adapters")
                .cloned()
                .unwrap_or(Value::Array(Vec::new()))
        } else {
            Value::Array(Vec::new())
        };
        let state_name = if active.is_some() {
            "active"
        } else {
            "uninstalled"
        };
        let fields = [
            ("state", string(state_name)),
            ("receipt_schema", get(&raw, "schema").clone()),
            ("mode", get(&raw, "mode").clone()),
            ("enabled_adapters", adapters),
            ("channel", channel(source)?),
            ("recorded_source", recorded),
            ("preserved_target_count", Value::Int(preserved.into())),
        ];
        for (key, value) in fields {
            report.set_in("installation", key, value)?;
        }
    }
    let journal = state.join("journal.json");
    if lexists(&journal)? {
        read(&journal, MAX_READ)?;
        let data = install::read_journal(env, &state)?
            .ok_or_else(|| ContractError::new("Unsupported lifecycle journal"))?;
        let status = text(&data, "status").unwrap_or_default().to_owned();
        report.set_in(
            "installation",
            "transaction",
            object([
                ("operation", get(&data, "operation").clone()),
                ("state", string(&status)),
            ]),
        )?;
        if !matches!(status.as_str(), "complete" | "rolled-back") {
            report.add(
                "runtime.transaction",
                "warning",
                "Runtime activation is pending; targets may be partially switched.",
                Some(RECOVER),
                Some(&journal),
            );
        } else if status == "complete"
            && !install::same(
                &install::fingerprint(&path)?,
                get(&data, "receipt_identity"),
            )
        {
            report.add(
                "runtime.receipt",
                "failure",
                "Receipt differs from its completed transaction.",
                Some("Preserve the receipt, journal and backups; reconcile ownership before install/update/removal."),
                Some(&path),
            );
        }
    }
    if let Some(active) = &active {
        if !report.failed("runtime.receipt") {
            let summary = security_summary(services, Some(active))?;
            report.set("security", summary);
        }
    }
    Ok(active)
}

/// Baseline `hook_migration`: receipt-level gaps, without trusting client
/// configuration or writing it.
pub fn hook_migration(
    report: &mut Report,
    env: &Environment,
    active: Option<&Value>,
) -> Outcome<()> {
    let Some(active) = active else {
        return Ok(());
    };
    if truthy(get(active, "no_guidance")) {
        return Ok(());
    }
    let targets = get(active, "targets").as_array().unwrap_or(&[]);
    for name in get(active, "enabled_adapters").as_array().unwrap_or(&[]) {
        let Some(name) = name
            .as_str()
            .filter(|name| matches!(*name, "codex" | "claude" | "opencode"))
        else {
            continue;
        };
        let Some(path) = install::hook_paths(env, name)? else {
            continue;
        };
        let absolute = display(&install::absolute(env, &path)?);
        let target = targets
            .iter()
            .find(|target| text(target, "path") == Some(absolute.as_str()));
        let kind = if name == "opencode" {
            "hook-link"
        } else {
            "hook-config"
        };
        let mut missing = target.is_none_or(|target| text(target, "kind") != Some(kind));
        if let (false, Some(target), "hook-config") = (missing, target, kind) {
            let binary = install::source_binary(get(active, "source"))?;
            let (prompt, stop) = install::client_hook_entries(name, &binary)?;
            missing = !install::same(get(target, "hook_entry"), &prompt)
                || !install::same(get(target, "stop_entry"), &stop);
        }
        if missing {
            report.add(
                &format!("runtime.hook_migration.{name}"),
                "warning",
                &format!("{name} hook activation is missing or outdated in the installation receipt; turn-boundary automation is not established."),
                Some("Run am install to reconcile the existing installation, then restart the agent client. If it refuses, follow the reported ownership or recovery guidance."),
                Some(&path),
            );
        }
    }
    Ok(())
}

// ----- bank locators -----

fn names(listing: &str) -> impl Iterator<Item = &str> {
    listing.split('\0').filter(|name| !name.is_empty())
}

fn hidden_or_traversing(path: &str) -> (bool, bool) {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let hidden = parts.iter().any(|part| part.starts_with('.'));
    let traversing = path.starts_with('/') || parts.contains(&"..");
    (hidden, traversing)
}

fn file_name(path: &str) -> &str {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or_default()
}

/// Python `PurePosixPath(path).parent.as_posix()`.
fn parent_posix(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    match parts.split_last() {
        Some((_, [])) | None => ".".to_owned(),
        Some((_, parents)) => parents.join("/"),
    }
}

fn under(root: &Path, relative: &str) -> PathBuf {
    if relative == "." {
        root.to_path_buf()
    } else {
        root.join(relative)
    }
}

/// Baseline `tracked_bank_roots`: positive history only.
pub fn tracked_bank_roots(root: &Path) -> Outcome<std::collections::BTreeSet<String>> {
    let indexed = git(root, &["ls-files", "--cached", "-z"], false)?.unwrap_or_default();
    let head = git(root, &["rev-parse", "--verify", "HEAD"], true)?;
    if head.is_none() {
        let symbolic = git(root, &["symbolic-ref", "--quiet", "HEAD"], true)?;
        let unborn = match &symbolic {
            Some(symbolic) => git(
                root,
                &["for-each-ref", "--format=%(refname)", symbolic.trim()],
                false,
            )?
            .unwrap_or_default()
            .is_empty(),
            None => false,
        };
        require(unborn, "HEAD is unavailable rather than an unborn branch")?;
    }
    let committed = match &head {
        Some(head) => git(
            root,
            &["ls-tree", "-r", "--name-only", "-z", head.trim()],
            false,
        )?
        .unwrap_or_default(),
        None => String::new(),
    };
    let sides: [(Option<&str>, std::collections::BTreeSet<&str>); 2] = [
        (None, names(&indexed).collect()),
        (head.as_deref().map(str::trim), names(&committed).collect()),
    ];
    let mut paths = std::collections::BTreeSet::new();
    for (_, listed) in &sides {
        for path in listed {
            if file_name(path) == "MEMORY.md" && !hidden_or_traversing(path).0 {
                paths.insert(*path);
            }
        }
    }
    require(
        paths.len() <= crate::repository_config::MAX_ROUTERS,
        "Tracked bank locator bound exceeded",
    )?;
    let mut roots = std::collections::BTreeSet::new();
    let mut cache = BTreeMap::new();
    for path in paths {
        require(!hidden_or_traversing(path).1, "Unsafe tracked bank path")?;
        for (revision, listed) in &sides {
            if listed.contains(path) {
                let expression = format!("{}:{path}", revision.unwrap_or_default());
                let raw = crate::repository_config::object_router_bytes(
                    root,
                    OsStr::new(&expression),
                    &mut cache,
                )?;
                if crate::repository_config::router_kind(&raw)?.unwrap_or(false) {
                    roots.insert(parent_posix(path));
                }
            }
        }
    }
    Ok(roots)
}

/// Baseline `lightweight_bank`: candidate locators only, not full-bank
/// validity or proof of absence.
pub fn lightweight_bank(root: &Path) -> Outcome<(PathBuf, Value)> {
    let top = git(root, &["rev-parse", "--show-toplevel"], true)?;
    let root = match &top {
        Some(top) => fs::canonical_path(Path::new(top.trim()), false)?,
        None => root.to_path_buf(),
    };
    let explicit = if top.is_some() {
        crate::repository_config::local_value(&root, "agent-memory.bankRoot")?
    } else {
        None
    };
    let mut candidates = std::collections::BTreeSet::new();
    candidates.insert(explicit.clone().unwrap_or_else(|| "memory-bank".to_owned()));
    if let Some(explicit) = &explicit {
        crate::repository_config::validate_root(explicit)?;
    }
    let known = if top.is_some() {
        tracked_bank_roots(&root)?
    } else {
        std::collections::BTreeSet::new()
    };
    require(
        known.len() <= 1
            && explicit
                .as_ref()
                .is_none_or(|explicit| known.is_empty() || known.iter().all(|k| k == explicit)),
        "Competing tracked bank locators",
    )?;
    candidates.extend(known.iter().cloned());
    let listed = if top.is_some() {
        git(
            &root,
            &[
                "ls-files",
                "--cached",
                "-z",
                "--",
                "*/MEMORY.md",
                "MEMORY.md",
            ],
            false,
        )?
        .unwrap_or_default()
    } else {
        String::new()
    };
    let paths: Vec<&str> = names(&listed).collect();
    require(
        paths.len() <= crate::repository_config::MAX_ROUTERS,
        "Too many possible bank locators",
    )?;
    for name in paths {
        let (hidden, traversing) = hidden_or_traversing(name);
        require(!traversing, "Unsafe router path")?;
        if file_name(name) == "MEMORY.md" && !hidden {
            let path = root.join(name);
            if exists(&path)
                && crate::repository_config::router_kind(&read(&path, MAX_ROUTER)?)?
                    .unwrap_or(false)
            {
                candidates.insert(parent_posix(name));
            }
        }
    }
    let mut found = Vec::new();
    for relative in &candidates {
        let router = under(&root, relative).join("MEMORY.md");
        if lexists(&router)? {
            let raw = read(&router, MAX_ROUTER)?;
            let body = String::from_utf8(raw)
                .map_err(|_| ContractError::new("Router is not UTF-8 text"))?;
            let (metadata, routes, _) = crate::bank_contract::parse_router(&body)?;
            found.push((relative.clone(), metadata, routes));
        }
    }
    require(
        found.len() <= 1
            && explicit.as_ref().is_none_or(|explicit| {
                found
                    .first()
                    .is_none_or(|(relative, _, _)| relative == explicit)
            }),
        "Competing bank locators",
    )?;
    let selected = explicit
        .clone()
        .or_else(|| known.iter().next().cloned())
        .unwrap_or_else(|| "memory-bank".to_owned());
    crate::repository_config::validate_root(&selected)?;
    let state = if !known.is_empty() && found.is_empty() {
        "missing_known_bank"
    } else {
        "not_located"
    };
    let mut bank = object([
        ("state", string(state)),
        ("path", path_string(&under(&root, &selected))),
        ("known_tracked_router", Value::Bool(!known.is_empty())),
        ("schema", Value::Null),
        ("project_id", Value::Null),
        ("counts_by_lifecycle", Value::Null),
        ("counts_scope", string("not_scanned")),
        (
            "discovery",
            string("configured/default-and-indexed-routers-only"),
        ),
        ("absence_verified", Value::Bool(false)),
        ("uniqueness_verified", Value::Bool(false)),
        ("pending_capture", string("not_assessed")),
        (
            "projection_position",
            string("no_global_projection_registry"),
        ),
        ("index_freshness", string("not_assessed")),
    ]);
    if let Some((relative, metadata, routes)) = found.first() {
        let fields = [
            ("state", string("router_observed")),
            ("path", path_string(&under(&root, relative))),
            ("schema", get(metadata, "schema").clone()),
            ("project_id", get(metadata, "project-id").clone()),
            ("router_updated", get(metadata, "router-updated").clone()),
            (
                "root_routes_by_lifecycle",
                counted(routes.iter().map(|route| get(route, "lifecycle"))),
            ),
        ];
        for (key, value) in fields {
            set(&mut bank, key, value);
        }
    }
    let path = PathBuf::from(text(&bank, "path").unwrap_or_default());
    let pending = lexists(&path.join(crate::bank_transactions::MARKER))?;
    let writer = lexists(&path.join(".agent-memory.lock"))?;
    set(&mut bank, "pending_transaction", Value::Bool(pending));
    set(&mut bank, "writer_marker_present", Value::Bool(writer));
    Ok((root, bank))
}

// ----- identities -----

fn developer(report: &mut Report) -> Outcome<()> {
    let (_, path, lock) = crate::developer_identity::paths()?;
    let data = if exists(&path) {
        let value = record(&path, MAX_READ)?;
        crate::developer_identity::validate(&value)?.clone()
    } else {
        crate::developer_identity::empty()
    };
    let selected = get(&data, "selected").clone();
    let resolved = selected != Value::Null;
    report.set(
        "developer",
        object([
            ("id", selected),
            ("resolved", Value::Bool(resolved)),
            ("pending_operation", Value::Bool(lexists(&lock)?)),
        ]),
    );
    if !resolved {
        report.add(
            "identity.developer",
            "warning",
            "No developer is selected; file diagnostics remain available.",
            Some(
                "Use the identity operation to preview and select an explicit stable developer ID.",
            ),
            None,
        );
    }
    Ok(())
}

fn checkout_key(root: &Path) -> Outcome<String> {
    let common = git(root, &["rev-parse", "--git-common-dir"], true)?;
    let location = match common {
        Some(common) => fs::canonical_path(&root.join(common.trim()), true)?,
        None => root.to_path_buf(),
    };
    let identity = fs::Descriptor::pin_directory(&location)?.stat()?;
    Ok(format!("{}:{}", identity.dev, identity.ino))
}

fn project(report: &mut Report, root: &Path, bank: &Value) -> Outcome<()> {
    let (_, path, lock) = crate::local_registry::paths("projects")?;
    let data = if exists(&path) {
        let value = record(&path, MAX_READ)?;
        crate::project_identity::validate(&value)?.clone()
    } else {
        object([
            ("projects", Value::Object(Vec::new())),
            ("checkouts", Value::Object(Vec::new())),
        ])
    };
    let observed_key = checkout_key(root)?;
    let mut entry = get(&data, "checkouts").get(&observed_key).cloned();
    let project_id = if truthy(get(bank, "project_id")) {
        get(bank, "project_id").clone()
    } else {
        entry
            .as_ref()
            .map_or(Value::Null, |entry| get(entry, "project_id").clone())
    };
    let mut historical = false;
    let mut binding_source = string(if entry.is_some() {
        "exact"
    } else {
        "unregistered"
    });
    // Quick diagnostics keep malformed/busy bank evidence independent of
    // local registration. Only the shared resolver may establish continuity.
    if entry.is_none() && truthy(&project_id) {
        if let Ok(resolved) = crate::project_identity::status(root) {
            if get(&resolved, "resolved") == &Value::Bool(true) {
                entry = Some(get(&resolved, "checkout_entry").clone());
                let source = get(get(&resolved, "evidence"), "binding_source");
                historical = source.as_str() == Some("historical-device-change");
                binding_source = source.clone();
            }
        }
    }
    let matches = entry.as_ref().is_some_and(|entry| {
        &project_id == get(entry, "project_id")
            && project_id
                .as_str()
                .is_some_and(|id| get(&data, "projects").get(id).is_some())
    });
    let binding_state = if matches {
        "matches"
    } else if entry.is_none() {
        "missing"
    } else {
        "mismatch"
    };
    report.set(
        "project",
        object([
            ("id", project_id.clone()),
            ("checkout_binding_matches", Value::Bool(matches)),
            ("binding_state", string(binding_state)),
            ("authority_verified", Value::Bool(false)),
            ("pending_operation", Value::Bool(lexists(&lock)?)),
            ("binding_source", binding_source),
            ("observed_checkout_key", string(observed_key)),
        ]),
    );
    if historical && matches {
        report.add(
            "identity.checkout_continuity",
            "warning",
            "Recognized the previously adopted checkout after its filesystem device number changed; memory operations can continue.",
            Some("Optional: am doctor --fix persists the current binding. This invalidates previously prepared proposals; prepare them again."),
            None,
        );
    }
    if !matches {
        if entry.is_some() {
            report.add(
                "identity.project",
                "warning",
                "Project binding differs from the observed bank; both identities are preserved.",
                Some("Use am identity status and review repository evidence before explicit binding reconciliation."),
                None,
            );
        } else if truthy(get(bank, "project_id")) {
            report.add(
                "identity.project",
                "warning",
                "This bank has no local checkout binding; its identity has not been adopted.",
                Some("Use am identity status and review repository evidence before explicit link/bind/fork."),
                None,
            );
        }
    }
    Ok(())
}

/// Baseline `identities`: bounded registries; selection is observed, never bootstrapped.
pub fn identities(report: &mut Report, root: &Path, bank: &Value) {
    attempt(
        report,
        "identity.developer",
        "Preserve the local developer registry; inspect it through the identity operation.",
        None,
        developer,
    );
    attempt(
        report,
        "identity.project",
        "Preserve the project registry and bank; use the identity operation for explicit reconciliation.",
        None,
        |report| project(report, root, bank),
    );
}

/// Baseline `permission_check`: existing nodes only; no mkdir, probe, lock or chmod.
pub fn permission_check(report: &mut Report, path: &Path, component: &str) -> Outcome<()> {
    for node in path.ancestors() {
        require(!is_symlink(node)?, "Redirected directory")?;
    }
    let mut node = path.to_path_buf();
    while !exists(&node) {
        match node.parent() {
            Some(parent) => node = parent.to_path_buf(),
            None => return Err(ContractError::new("Redirected directory").into()),
        }
    }
    let info = install::named(&node)?.ok_or_else(|| ContractError::new("Redirected directory"))?;
    let mode = info.mode;
    if !info.is_directory()
        || mode & 0o444 == 0
        || mode & 0o222 == 0
        || mode & 0o111 == 0
        || !accessible(&info)
    {
        report.add(
            component,
            "failure",
            "Existing directory/ancestor is not readable, writable and searchable.",
            Some(COMPONENT_PERMISSIONS),
            Some(&node),
        );
    } else {
        report.add(
            component,
            "pass",
            "Existing directory/ancestor has read, write and search access; no write probe performed.",
            None,
            Some(&node),
        );
    }
    Ok(())
}

// ----- runtime doctor -----

fn path_of(value: &Value, key: &str) -> PathBuf {
    PathBuf::from(text(value, key).unwrap_or_default())
}

fn same_list(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| install::same(a, b))
}

fn check_source(report: &mut Report, services: &Services, active: &Value) -> Outcome<()> {
    let expected = get(active, "source");
    let kind = text(expected, "kind").unwrap_or_default();
    let managed = kind == "managed";
    let binary = if managed {
        None
    } else {
        Some(install::source_binary(expected)?)
    };
    let observed = source(
        services,
        &path_of(expected, "pack"),
        binary.as_deref(),
        managed,
    )?;
    report.set("installed_source", observed.clone());
    let compare = |keys: &[&str]| {
        keys.iter()
            .all(|key| install::same(get(&observed, key), get(expected, key)))
    };
    let mut matches = compare(&[
        "path",
        "pack",
        "version",
        "revision",
        "dirty",
        "artifact_digest",
    ]);
    // Unversioned source uses the install receipt's explicit marker.
    if get(&observed, "revision") == &Value::Null
        && text(expected, "revision") == Some("unversioned")
    {
        matches = compare(&["path", "pack", "version", "dirty", "artifact_digest"]);
    }
    if managed {
        let root = path_of(expected, "path");
        let output = |args: &[&str]| -> Outcome<String> {
            let raw = crate::managed_source::git(&root, args, None, MAX_GIT)?;
            Ok(String::from_utf8_lossy(&raw).trim().to_owned())
        };
        require(
            Some(output(&["remote", "get-url", "origin"])?.as_str()) == text(expected, "remote")
                && Some(output(&["symbolic-ref", "--short", "HEAD"])?.as_str())
                    == text(expected, "ref"),
            "Managed source channel differs",
        )?;
    }
    let path = path_of(expected, "path");
    if !matches {
        report.add(
            "runtime.source",
            if kind == "development" {
                "warning"
            } else {
                "failure"
            },
            "Source differs from its recorded installation.",
            Some("For a developer source, inspect edits and preview reinstall; for managed source, preserve edits and reconcile the recorded channel before update."),
            Some(&path),
        );
        return Ok(());
    }
    let owned: Vec<&Value> = get(active, "targets")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter(|target| text(target, "kind") == Some("source"))
        .collect();
    let qualified = install::verify_targets(services, &owned, Some(expected), false)?;
    let detail = if qualified.is_empty() {
        "Source revision, artifact and recorded channel match."
    } else {
        "Legacy Git payload and package modes match; raw directory identity differs."
    };
    report.add("runtime.source", "pass", detail, None, Some(&path));
    Ok(())
}

fn wrapper_mode(observed: &Value, identity: &Value) -> Option<String> {
    let expected = get(identity, "mode").as_int()?.as_u64()?;
    let actual = get(observed, "mode").as_int()?.as_u64()?;
    (text(observed, "kind") == Some("file") && expected <= 0o7777 && actual != expected)
        .then(|| format!("Wrapper mode is 0o{actual:o}; recorded mode is 0o{expected:o}."))
}

fn command_roles(active: &Value) -> Vec<PathBuf> {
    let bin = path_of(active, "bin_dir");
    install::COMMAND_NAMES
        .iter()
        .map(|name| bin.join(name))
        .collect()
}

fn check_target(
    report: &mut Report,
    services: &Services,
    active: &Value,
    target: &Value,
) -> Outcome<()> {
    let kind = text(target, "kind").unwrap_or_default();
    let path = path_of(target, "path");
    let source = get(active, "source");
    if kind != "hook-config" {
        fs::trust::tree(&path, false)?;
    }
    match kind {
        "activation" => {
            let raw = read(&path, install::MAX_GUIDANCE)?;
            if install::activation(&raw, text(target, "block_hash"), install::SHIM).is_err() {
                return Err(fixed(
                    "Activation block is missing, malformed, duplicated or differs from its recorded block hash; outside text is preserved.",
                ));
            }
        }
        "hook-config" => match install::verify_hook_target(target) {
            Ok(_) => {}
            Err(install::Error::Hook(code)) => {
                hook_failure(report, &path, code);
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        },
        "legacy-source" => {
            if install::verify_targets(services, &[target], Some(source), false).is_err() {
                return Err(fixed(
                    "Owned legacy-source target is missing, redirected, or has different bytes or modes than its recorded identity.",
                ));
            }
        }
        _ => {
            let observed = install::fingerprint(&path)?;
            let identity = get(target, "identity");
            if !install::same(&observed, identity) {
                if kind == "wrapper" {
                    if let Some(message) = wrapper_mode(&observed, identity) {
                        return Err(fixed(message));
                    }
                }
                return Err(fixed(format!(
                    "Owned {kind} target is missing, redirected, or has different bytes or modes than its recorded identity."
                )));
            }
            if matches!(kind, "link" | "copy") {
                let rust_era = text(source, "target").is_some();
                let inspected = if rust_era {
                    services
                        .artifacts
                        .inspect_runtime(&path, &install::source_binary(source)?)?
                } else {
                    services.artifacts.inspect(&path)?
                };
                if get(&inspected, "artifact_digest") != get(target, "artifact_digest") {
                    return Err(fixed(
                        "Client artifact digest differs from the installation receipt.",
                    ));
                }
            }
            if kind == "wrapper" {
                let binding = get(active, "omp_binding");
                if binding != &Value::Null
                    && text(target, "path") == text(binding, "extension_target")
                {
                    let expected = crate::omp_installation::extension_wrapper(
                        &path_of(source, "pack"),
                        &install::source_binary(source)?,
                        text(source, "artifact_digest").unwrap_or_default(),
                        binding,
                    )?;
                    require(
                        read(&path, MAX_READ)? == expected,
                        "Native extension differs from its owned entrypoint",
                    )?;
                } else {
                    return Err(fixed(
                        "Wrapper path has no recognized role in the installation receipt.",
                    ));
                }
            }
            if kind == "command-link" && !command_roles(active).contains(&path) {
                return Err(fixed(
                    "Command link path has no recognized role in the installation receipt.",
                ));
            }
        }
    }
    if kind == "hook-config" {
        report.add(
            "runtime.target.hook-config",
            "pass",
            "Receipt-owned prompt and recorded Stop entries are intact; other configuration may differ.",
            None,
            Some(&path),
        );
        if let Some(check) = report.last_check() {
            set(check, "verification_scope", string("receipt-owned-entries"));
        }
    } else {
        report.add(
            &format!("runtime.target.{kind}"),
            "pass",
            "Owned target is intact; surrounding activation text may differ.",
            None,
            Some(&path),
        );
    }
    Ok(())
}

fn hook_failure(report: &mut Report, path: &Path, code: HookCode) {
    report.add(
        "runtime.target.hook-config",
        "failure",
        code.message(),
        Some("Preserve the configuration and receipt; inspect ownership evidence before explicit reconciliation."),
        Some(path),
    );
    if let Some(check) = report.last_check() {
        set(check, "failure_code", string(code.code()));
    }
}

fn purge_entries(directory: &Path, message: &str) -> Outcome<Vec<PathBuf>> {
    let descriptor = fs::Descriptor::open_directory(directory)?;
    let entries = descriptor
        .entries_bounded(MAX_PURGES)?
        .ok_or_else(|| ContractError::new(message))?;
    let mut paths: Vec<PathBuf> = entries
        .into_iter()
        .map(|name| directory.join(name))
        .collect();
    paths.sort();
    Ok(paths)
}

fn is_json(path: &Path) -> bool {
    path.extension() == Some(OsStr::new("json"))
}

fn purge_status(value: &Value, message: &str) -> Outcome<String> {
    let status = text(value, "status").unwrap_or_default();
    require(
        text(value, "schema") == Some(crate::purging::SCHEMA)
            && matches!(status, "staging" | "detached" | "complete"),
        message,
    )?;
    Ok(status.to_owned())
}

fn check_purge(report: &mut Report, path: &Path) -> Outcome<()> {
    let value = record(path, crate::purging::MAX_RECORD)?;
    let status = purge_status(&value, "Unknown purge journal")?;
    if status != "complete" {
        let preview = get(&value, "preview");
        require(preview.as_object().is_some(), "Unknown purge journal")?;
        let pending = object([
            ("journal_path", path_string(path)),
            ("repository", get(preview, "repository").clone()),
            ("path", get(preview, "path").clone()),
            ("state", string(&status)),
        ]);
        if let Some(Value::Array(items)) = report.value.get_mut("pending_purges") {
            items.push(pending);
        }
        report.add(
            "data.purge",
            "warning",
            "A separately confirmed purge is unfinished; runtime recovery does not resume data deletion.",
            Some("Inspect the original exact preview and retained journal. Resume only the same previously authorized purge confirmation; doctor grants no deletion consent."),
            Some(path),
        );
    } else {
        report.add(
            "data.purge",
            "pass",
            "Historical purge journal is complete; no data deletion performed.",
            None,
            Some(path),
        );
    }
    Ok(())
}

/// The shared lifecycle lock, held (shared, nonblocking) for the inspection.
enum LockState {
    Absent,
    Held(FlockGuard),
    Busy,
}

fn shared_lock(state: &Path, lock: &Path) -> Outcome<LockState> {
    if !lexists(lock)? {
        return Ok(LockState::Absent);
    }
    fs::trust::record(lock)?;
    read(lock, MAX_LOCK)?;
    let chain = DirectoryChain::open(state)?;
    let descriptor = chain.directory().open_at(
        OsStr::new("lock"),
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0,
    )?;
    let info = descriptor.stat()?;
    require(
        info.is_regular() && info.nlink == 1,
        "Unsafe lifecycle lock",
    )?;
    match FlockGuard::try_acquire(descriptor, LockMode::Shared) {
        Ok(guard) => Ok(LockState::Held(guard)),
        Err(LockError::Busy) => Ok(LockState::Busy),
        Err(error) => Err(error.into()),
    }
}

fn audit_active(report: &mut Report, services: &Services, active: &Value) {
    attempt(
        report,
        "runtime.source",
        "Use a trusted outside source helper; preserve the source and receipt and preview reinstall after reconciling the exact channel.",
        text(get(active, "source"), "path").map(Path::new),
        |report| check_source(report, services, active),
    );
    for target in get(active, "targets").as_array().unwrap_or(&[]) {
        let kind = text(target, "kind").unwrap_or_default();
        if kind == "source" {
            // Source identity and channel are checked above, without copying Git metadata.
            continue;
        }
        let path = path_of(target, "path");
        attempt(
            report,
            &format!("runtime.target.{kind}"),
            TARGET_REPAIR,
            Some(&path),
            |report| check_target(report, services, active, target),
        );
        if kind != "activation" {
            let parent = path.parent().unwrap_or(Path::new("/")).to_path_buf();
            attempt(
                report,
                "runtime.target_permissions",
                "Inspect this target parent’s ownership and access; preserve existing files and preview reinstall after resolving permissions.",
                Some(&parent),
                |report| permission_check(report, &parent, "runtime.target_permissions"),
            );
        }
    }
    for backup in get(active, "backups").as_array().unwrap_or(&[]) {
        let parent = path_of(backup, "path")
            .parent()
            .unwrap_or(Path::new("/"))
            .to_path_buf();
        attempt(
            report,
            "runtime.backup_permissions",
            "Preserve existing backups; restore intended owner access to their directory before lifecycle changes.",
            Some(&parent),
            |report| permission_check(report, &parent, "runtime.backup_permissions"),
        );
    }
}

/// Baseline `runtime_doctor`, under the existing shared lifecycle lock.
pub fn runtime_doctor(
    report: &mut Report,
    env: &Environment,
    services: &Services,
    active: Option<&Value>,
) -> Outcome<()> {
    let state = install::locations(env)?.state;
    let trusted = attempt(
        report,
        "runtime.path_trust",
        "Use a trusted outside helper; preserve the state and choose private current-user-owned paths. Unix ownership/mode checks do not authenticate already-loaded code or audit platform ACLs.",
        Some(&state),
        |_| Ok(fs::trust::state(&state)?),
    );
    let Some(trusted) = trusted else {
        return Ok(());
    };
    let prefix = if trusted.present {
        "Installation state satisfies the Unix owner/ancestor/private-directory policy. "
    } else {
        "Existing ancestors permit private installation state; no state was created. "
    };
    report.add(
        "runtime.path_trust",
        "pass",
        &format!("{prefix}Platform ACLs and already-loaded verifier code are not authenticated."),
        None,
        Some(&state),
    );
    permission_check(report, &state, "runtime.permissions")?;
    let lock = state.join("lock");
    report.set("pending_purges", Value::Array(Vec::new()));
    require(
        active.is_none() || lexists(&lock)?,
        "An active installation is missing its lifecycle lock",
    )?;
    let _guard = match shared_lock(&state, &lock)? {
        LockState::Busy => {
            report.add(
                "runtime.lock",
                "warning",
                "A lifecycle writer is active; runtime ownership checks were deferred.",
                Some("Wait for the lifecycle operation to finish, then rerun doctor. Do not steal the lock."),
                Some(&lock),
            );
            return Ok(());
        }
        LockState::Held(guard) => {
            require(
                text(&record(&lock, MAX_READ)?, "schema") == Some(install::LOCK_SCHEMA),
                "Unknown lock schema",
            )?;
            Some(guard)
        }
        LockState::Absent => None,
    };
    report.add(
        "runtime.lock",
        "pass",
        "No lifecycle writer held the existing lock during inspection.",
        None,
        Some(&lock),
    );
    // Re-read under the existing shared lock, then compare again before releasing it.
    // Never combine ownership from a receipt that changed across this inspection.
    let records = ["receipt.json", "journal.json"];
    let mut before = Vec::new();
    for name in records {
        before.push(install::fingerprint(&state.join(name))?);
    }
    let active = receipt_view(report, env, services)?;
    let owner_path = state.join("owner.json");
    if lexists(&owner_path)? {
        attempt(
            report,
            "runtime.owner",
            "Preserve this owner record and the lifecycle lock. Reconcile the malformed or redirected metadata before retrying a lifecycle operation; do not reset or steal ownership based on PID or age.",
            Some(&owner_path),
            |report| {
                install::read_owner(&state)?;
                read(&owner_path, MAX_READ)?;
                report.add(
                    "runtime.owner",
                    "pass",
                    "Existing lifecycle owner metadata is readable and accepted by the lifecycle lock protocol; PID and age do not grant lock ownership.",
                    None,
                    Some(&owner_path),
                );
                Ok(())
            },
        );
    }
    match &active {
        Some(active) if install::python_era_kind(active) => report.add(
            "runtime.ownership",
            "failure",
            "A Python-era installation receipt is active; its owned targets are verified only after adoption by the verified bootstrap binary.",
            Some(install::ADOPTION_GUIDANCE),
            Some(&state.join("receipt.json")),
        ),
        Some(active) => audit_active(report, services, active),
        None => report.add(
            "runtime.ownership",
            "warning",
            "No active installation receipt; source and bank diagnostics still work.",
            Some("Use the installation operation to preview an installation if an installed CLI is wanted."),
            None,
        ),
    }
    let purges = state.join("purges");
    if lexists(&purges)? {
        require(
            install::is_directory(&purges)?,
            "Unsafe purge record directory",
        )?;
        for path in purge_entries(&purges, "Purge journal inventory exceeds its bound")? {
            if !is_json(&path) {
                continue;
            }
            attempt(
                report,
                "data.purge",
                "Preserve this purge record and quarantine. Review the original confirmation before resuming; do not remove or reset it.",
                Some(&path),
                |report| check_purge(report, &path),
            );
        }
    }
    let purge_failed = report.failed("data.purge");
    let pending = get(&report.value, "pending_purges")
        .as_array()
        .is_some_and(|items| !items.is_empty());
    let summary = if purge_failed {
        "inspection_failed"
    } else if pending {
        "pending"
    } else {
        "none_pending"
    };
    report.set_in("installation", "purge_transactions", string(summary))?;
    let mut after = Vec::new();
    for name in records {
        after.push(install::fingerprint(&state.join(name))?);
    }
    require(
        same_list(&before, &after),
        "Lifecycle records changed during inspection",
    )?;
    Ok(())
}

// ----- bank doctor -----

fn refresh_prerequisites(report: &mut Report, root: &Path) -> Outcome<()> {
    let provider = crate::turn_check::memory_refresh_provider()
        .map_err(|_| install::Error::Unregistered("Memory refresh (local prerequisites)"))?;
    match provider.local_prerequisites(root)? {
        Err(failure) => {
            report.set(
                "refresh",
                object([
                    ("local_prerequisites", string("pending")),
                    ("remote_check", string("not-exercised")),
                    ("remote_freshness_verified", Value::Bool(false)),
                    ("failure_code", string(&failure.code)),
                ]),
            );
            report.add(
                "bank.refresh_prerequisites",
                "failure",
                &format!("{}: {}", failure.code, failure.message),
                Some(&failure.repair),
                failure.path.as_deref().map(Path::new),
            );
        }
        Ok(observed) => {
            report.set("refresh", observed);
            report.add(
                "bank.refresh_prerequisites",
                "pass",
                "Local transaction-path protection and retained recovery inventory were inspected; remote freshness and complete refresh were not exercised.",
                None,
                None,
            );
        }
    }
    Ok(())
}

fn entry_key(entry: &Value) -> Value {
    Value::Array(vec![
        get(entry, "id").clone(),
        get(entry, "version").clone(),
        get(entry, "path").clone(),
    ])
}

fn check_projection(
    report: &mut Report,
    projection: &Path,
    checked: &Value,
    hashes: &Value,
) -> Outcome<()> {
    let data = record(projection, crate::projections::MAX_BYTES)?;
    let value = crate::projections::value(&data)?.clone();
    let active: Vec<Value> = get(checked, "entries")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter(|entry| text(entry, "lifecycle") == Some("active"))
        .map(entry_key)
        .collect();
    let current = install::same(get(&value, "project_id"), get(checked, "project_id"))
        && install::same(get(&value, "source_hashes"), hashes)
        && get(&value, "records").as_array().is_some_and(|records| {
            records.iter().all(|record| {
                let key = entry_key(record);
                active.iter().any(|active| install::same(active, &key))
            })
        });
    require(
        current,
        "Derived projection is stale or contains inactive/foreign references",
    )?;
    report.add(
        "bank.projection",
        "pass",
        "Explicit projection binds current source hashes and active canonical references; applicability remains a retrieval decision.",
        None,
        Some(projection),
    );
    report.set_in(
        "bank",
        "projection_position",
        string("explicit_projection_current"),
    )
}

/// Baseline `bank_doctor`: full canonical validation of the located bank.
pub fn bank_doctor(report: &mut Report, root: &Path, projection: Option<&Path>) -> Outcome<()> {
    let root_text = display(root);
    let purge_pending = get(&report.value, "pending_purges")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .any(|item| text(item, "repository") == Some(root_text.as_str()));
    if purge_pending {
        report.set_in("bank", "state", string("purge_pending"))?;
        report.add(
            "bank.purge",
            "warning",
            "This repository has an unfinished separately confirmed data purge; bank validation and initialization are deferred.",
            Some("Preserve the original bank/quarantine and purge journal; follow only the original explicitly authorized purge operation."),
            None,
        );
        return Ok(());
    }
    let bank = crate::repository_config::bank_path(root)?;
    report.set_in("bank", "path", path_string(&bank))?;
    report.set_in("bank", "uniqueness_verified", Value::Bool(true))?;
    permission_check(report, &bank, "bank.permissions")?;
    let router = bank.join("MEMORY.md");
    let known = get(get(&report.value, "bank"), "known_tracked_router") == &Value::Bool(true);
    if !exists(&router) && !exists(&bank) {
        let state = if known {
            "missing_known_bank"
        } else {
            "not_initialized"
        };
        report.set_in("bank", "state", string(state))?;
        report.set_in("bank", "absence_verified", Value::Bool(true))?;
        let repair = if known {
            "Inspect Git state and restore/reconcile the known bank; do not initialize a replacement."
        } else {
            "Inspect useful project evidence, then use am init; diagnostics do not create identity or memory."
        };
        report.add(
            "bank.setup",
            "warning",
            "No bank was found by the established repository locator.",
            Some(repair),
            None,
        );
        require(
            projection.is_none(),
            "A selected projection has no canonical bank to verify against",
        )?;
        return Ok(());
    }
    if !lexists(&router)? && known {
        report.set_in("bank", "state", string("missing_known_bank"))?;
        report.set_in("bank", "absence_verified", Value::Bool(false))?;
        report.add(
            "bank.setup",
            "warning",
            "A tracked router is missing from this existing bank directory.",
            Some("Inspect Git state and restore/reconcile the known bank; preserve remaining files and do not initialize a replacement."),
            None,
        );
        return Ok(());
    }
    attempt(
        report,
        "bank.refresh_prerequisites",
        "Preserve private state and original recovery evidence. Inspect the reported local refresh prerequisite before retrying; no remote refresh was performed.",
        None,
        |report| refresh_prerequisites(report, root),
    );
    let transaction = bank.join(crate::bank_transactions::MARKER);
    let lock = bank.join(".agent-memory.lock");
    let marked = lexists(&transaction)?;
    if marked || lexists(&lock)? {
        report.add(
            "bank.transaction",
            "warning",
            "Bank is busy or interrupted; no stable content inventory was claimed.",
            Some("Wait for a live writer. For an interrupted write, follow the writes recovery operation with its original operation and receipt; preserve markers and edits."),
            Some(if marked { &transaction } else { &lock }),
        );
        report.set_in("bank", "state", string("busy_or_interrupted"))?;
        return Ok(());
    }
    let (checked, hashes) = crate::bank_writes::snapshot(root, None)?;
    let entries = get(&checked, "entries").as_array().unwrap_or(&[]);
    let files = hashes.as_object().map_or(0, <[(String, Value)]>::len);
    let fields = [
        ("state", string("inspecting")),
        ("source_stability_verified", Value::Bool(false)),
        ("schema", string("agent-memory/v1")),
        ("project_id", get(&checked, "project_id").clone()),
        (
            "counts_by_lifecycle",
            counted(entries.iter().map(|entry| get(entry, "lifecycle"))),
        ),
        ("counts_scope", string("validated_canonical_entries")),
        ("source_files", Value::Int(files.into())),
        (
            "index_freshness",
            string("canonical_topic_routes_validated"),
        ),
        ("pending_capture", string("not_centrally_registered")),
    ];
    for (key, value) in fields {
        report.set_in("bank", key, value)?;
    }
    report.add(
        "bank.schema_and_readability",
        "pass",
        "Native schema, routes, topic indices, record/history/tombstone formats and readable canonical files are valid.",
        None,
        Some(&bank),
    );
    match projection {
        Some(projection) => {
            attempt(
                report,
                "bank.projection",
                "Discard this derived projection and rebuild from current canonical Markdown through the projections operation; retain canonical memory and do not edit cached references into validity.",
                Some(projection),
                |report| check_projection(report, projection, &checked, &hashes),
            );
        }
        None => report.add(
            "bank.projection",
            "not_enabled",
            "No projection file was selected; derived references have no global authoritative registry.",
            None,
            None,
        ),
    }
    let (_, again) = crate::bank_writes::snapshot(root, None)?;
    require(
        install::same(&again, &hashes),
        "Bank changed during diagnostics",
    )?;
    report.set_in("bank", "state", string("validated"))?;
    report.set_in("bank", "source_stability_verified", Value::Bool(true))
}

/// Baseline `setup_assessment`: bank setup evidence, without certifying
/// runtime health or semantic truth.
pub fn setup_assessment(report: &mut Report) {
    let bank = get(&report.value, "bank").clone();
    let state = text(&bank, "state").unwrap_or_default();
    let project = get(&report.value, "project");
    let binding = text(project, "binding_state");
    let failures = report.checks().iter().any(|check| {
        let component = text(check, "component").unwrap_or_default();
        text(check, "state") == Some("failure")
            && (component.starts_with("bank.")
                || component.starts_with("identity.")
                || component == "data.purge")
    });
    let absence = get(&bank, "absence_verified") == &Value::Bool(true);
    let mut evidence = Vec::new();
    let (assessment, action) = if failures || state == "unresolved" {
        (
            "inspection_required",
            "Preserve bank/configuration evidence and inspect the failed component with am doctor.",
        )
    } else if state == "purge_pending" {
        (
            "purge_pending",
            "Preserve quarantine and journal; follow only the original explicitly authorized purge operation.",
        )
    } else if state == "busy_or_interrupted"
        || truthy(get(&bank, "pending_transaction"))
        || truthy(get(&bank, "writer_marker_present"))
    {
        (
            "busy_or_interrupted",
            "Wait for a live writer; preserve unknown markers and use the writes recovery operation for an interrupted write.",
        )
    } else if state == "missing_known_bank" {
        evidence.push(string("recognized-router-in-index-or-HEAD"));
        (
            "missing_known_bank",
            "Inspect Git state and restore/reconcile the known bank; do not initialize a replacement.",
        )
    } else if matches!(state, "not_initialized" | "not_located") {
        if absence {
            (
                "not_initialized",
                "Inspect useful project evidence, then use am init; identity and memory are not created by diagnostics.",
            )
        } else {
            (
                "setup_needed",
                "Run am doctor to inspect existing bank locations before initialization; review useful project evidence.",
            )
        }
    } else if binding == Some("mismatch") {
        (
            "binding_mismatch",
            "Use am identity status; preserve both identities and review evidence before explicit reconciliation.",
        )
    } else if binding == Some("missing") {
        (
            "binding_required",
            "Use am identity status; review repository evidence before explicit link/bind/fork.",
        )
    } else if binding == Some("matches") {
        let verified = state == "validated" && truthy(get(project, "authority_verified"));
        if verified {
            (
                "ready",
                "Bank and project authority verified; qualify task applicability before using memory.",
            )
        } else {
            (
                "bound_not_audited",
                "Run am doctor for complete bank and project-authority checks.",
            )
        }
    } else {
        (
            "inspection_required",
            "Use am doctor to inspect project identity and bank evidence.",
        )
    };
    report.set(
        "setup",
        object([
            ("state", string(assessment)),
            ("evidence", Value::Array(evidence)),
            ("absence_verified", Value::Bool(absence)),
            (
                "scope",
                string("bank-and-project-setup; runtime health and semantic truth are separate"),
            ),
            ("next_step", string(action)),
        ]),
    );
}

/// Baseline `shallow_purge_state`: bounded existing purge metadata only;
/// never resume a deletion.
pub fn shallow_purge_state(report: &mut Report, env: &Environment, root: &Path) -> Outcome<()> {
    let state = install::locations(env)?.state;
    fs::trust::state(&state)?;
    let directory = state.join("purges");
    if !lexists(&directory)? {
        return Ok(());
    }
    require(
        install::is_directory(&directory)?,
        "Unsafe purge journal directory",
    )?;
    let root_text = display(root);
    for path in purge_entries(&directory, "Purge inventory exceeds its bound")? {
        if !is_json(&path) {
            continue;
        }
        let value = record(&path, crate::purging::MAX_RECORD)?;
        let status = purge_status(&value, "Unknown purge journal")?;
        let preview = get(&value, "preview");
        if status != "complete" {
            require(preview.as_object().is_some(), "Unknown purge journal")?;
            if text(preview, "repository") == Some(root_text.as_str()) {
                report.set_in("bank", "state", string("purge_pending"))?;
                report.add(
                    "data.purge",
                    "warning",
                    "This repository has an unfinished separately authorized purge.",
                    Some("Preserve the original journal/quarantine; follow only the original explicitly authorized purge operation."),
                    Some(&path),
                );
            }
        }
    }
    Ok(())
}

// ----- run -----

fn hook_outcomes(report: &mut Report, root: &Path) -> Outcome<()> {
    let mut unreadable = Vec::new();
    let failures = crate::hook_failure::read(root, Some(&mut unreadable))?;
    report.set("hook_failures", Value::Array(failures.clone()));
    report.set("hook_diagnostic_errors", Value::Array(unreadable.clone()));
    for item in &unreadable {
        let component = format!(
            "hook.{}.{}",
            text(item, "harness").unwrap_or_default(),
            text(item, "adapter").unwrap_or_default()
        );
        report.add(
            &component,
            "warning",
            "Private hook failure metadata could not be safely read: state.unavailable.",
            Some("Preserve redirected or foreign files. A safely owned malformed record heals on the next successful adapter invocation; inspect ownership before manual recovery."),
            None,
        );
    }
    for item in &failures {
        let field = |key: &str| text(item, key).unwrap_or_default().to_owned();
        report.add(
            &format!("hook.{}.{}", field("harness"), field("adapter")),
            "warning",
            &format!(
                "Hook failure observed at {}: {} / {}",
                field("timestamp"),
                field("stage"),
                field("reason")
            ),
            Some("Inspect the project identity findings and resolve the reported stage; submit a new real prompt afterward. Pending capture is not publication."),
            None,
        );
    }
    Ok(())
}

fn full_identity(report: &mut Report, root: &Path) -> Outcome<()> {
    let value = crate::project_identity::status(root)?;
    let resolved = get(&value, "resolved").clone();
    report.set_in("project", "authority_verified", resolved.clone())?;
    if resolved != Value::Bool(true) {
        report.add(
            "identity.authority",
            "warning",
            "Full repository/project authority remains unresolved.",
            Some("Use the identity operation to inspect sanitized repository evidence and explicitly reconcile binding or authority; do not infer a new project."),
            None,
        );
    } else {
        report.add(
            "identity.authority",
            "pass",
            "Local registry, checkout, bank and canonical remote identify one project.",
            None,
            None,
        );
    }
    Ok(())
}

fn doctor_checks(
    report: &mut Report,
    env: &Environment,
    services: &Services,
    active: Option<&Value>,
    root: &Path,
    bank_path: Option<PathBuf>,
    projection: Option<&Path>,
) -> Outcome<()> {
    attempt(
        report,
        "hook.outcomes",
        "Preserve private hook failure metadata; inspect its ownership and the installed adapter.",
        None,
        |report| hook_outcomes(report, root),
    );
    let git_available = which(env, "git");
    report.add(
        "prerequisite.git",
        if git_available { "pass" } else { "failure" },
        if git_available {
            "Git is available."
        } else {
            "Git is unavailable."
        },
        Some("Install Git for governed repository operations; Markdown remains readable."),
        None,
    );
    let native = cfg!(any(target_os = "macos", target_os = "linux"));
    report.add(
        "filesystem.native_api",
        if native { "pass" } else { "failure" },
        if native {
            "Native no-replace/exchange API is available; actual per-filesystem support is established by lifecycle activation, not a diagnostic write probe."
        } else {
            "Required native no-replace/exchange API is unavailable."
        },
        Some("Use a supported macOS/Linux runtime and filesystem; preserve receipt and backups. Do not replace activation with a rename-away fallback."),
        None,
    );
    crate::bank_writes::fault("diagnostic-before-runtime-lock")?;
    attempt(report, "runtime.inspection", RECOVER, None, |report| {
        runtime_doctor(report, env, services, active)
    });
    let security = get(&report.value, "security");
    let verified = text(security, "publisher_authentication") == Some("verified");
    let notice =
        distribution_notice(text(security, "recorded_source_selection").unwrap_or_default());
    report.add(
        "distribution.trust",
        if verified { "pass" } else { "warning" },
        notice,
        Some("Use an operator-trusted source. A verified lifecycle receipt is not a publisher signature; diagnostics cannot authenticate already-loaded verifier code."),
        None,
    );
    attempt(
        report,
        "bank.inspection",
        "Preserve canonical files, markers and operation receipts. Follow the bank inspection/writes operation; rebuild a selected stale projection from current canonical sources instead of repairing cached content.",
        bank_path.as_deref(),
        |report| bank_doctor(report, root, projection),
    );
    if text(get(&report.value, "bank"), "state") == Some("validated") {
        attempt(
            report,
            "identity.authority",
            "Inspect explicit project binding and authority with the identity operation; preserve existing memory.",
            None,
            |report| full_identity(report, root),
        );
    }
    for (name, detail) in [
        (
            "authoritative_event_log",
            "Markdown/Git is authoritative; optional historical records are validated with the bank.",
        ),
        (
            "vector_index",
            "Native recall reads canonical Markdown; no vector service is enabled.",
        ),
        (
            "migration",
            "Native v1 is directly readable; the production catalog has no conversion steps. Runtime operations never migrate memory.",
        ),
        (
            "optional_services",
            "GitHub publication is opt-in per operation; diagnostics perform no authentication, API calls, or provider availability claim.",
        ),
    ] {
        report.add(name, "not_enabled", detail, None, None);
    }
    Ok(())
}

/// Baseline `run`: `version`, `status` or `doctor`.
pub fn run(
    env: &Environment,
    services: &Services,
    operation: &str,
    repo: &Path,
    projection: Option<&Path>,
) -> Outcome<Value> {
    let mut report = Report::new(operation);
    let active = attempt(
        &mut report,
        "runtime.receipt",
        "Preserve installation state and backups. Inspect the external receipt/journal from a trusted package; do not guess or reset ownership.",
        None,
        |report| receipt_view(report, env, services),
    )
    .flatten();
    if matches!(operation, "version" | "doctor") {
        let location = executing_package(env, services);
        let pack = location.as_ref().ok().map(|(pack, _)| pack.clone());
        let managed = active.as_ref().is_some_and(|active| {
            let recorded = get(active, "source");
            text(recorded, "kind") == Some("managed")
                && pack.as_ref().is_some_and(|pack| {
                    fs::canonical_path(&path_of(recorded, "pack"), false)
                        .ok()
                        .as_ref()
                        == Some(pack)
                })
        });
        let current = attempt(
            &mut report,
            "runtime.executing_artifact",
            "Obtain a complete trusted portable package and run its diagnostic helper; preserve the affected installation.",
            pack.as_deref(),
            |_| {
                let (pack, binary) = location?;
                source(
                    services,
                    &pack,
                    (!managed).then_some(binary.as_path()),
                    managed,
                )
            },
        );
        let found = current.is_some();
        report.set("executing_source", current.unwrap_or(Value::Null));
        if found {
            report.add(
                "runtime.executing_artifact",
                "pass",
                "Executing package inventory, route graph and declared compatibility are valid.",
                None,
                pack.as_deref(),
            );
        }
    }
    if operation == "version" {
        return Ok(report.finish());
    }
    attempt(
        &mut report,
        "runtime.hook_migration",
        "Inspect the installation receipt and retry am install only after resolving the reported ownership issue.",
        None,
        |report| hook_migration(report, env, active.as_ref()),
    );
    let root = fs::canonical_path(&env.expanduser(repo)?, true)?;
    require(
        install::is_directory(&root)?,
        "Repository must be an existing directory",
    )?;
    let located = attempt(
        &mut report,
        "bank.locator",
        "Inspect repository bankRoot configuration and existing routers; preserve all possible banks and use doctor before initialization.",
        Some(&root),
        |_| lightweight_bank(&root),
    );
    let (root, bank) = match located {
        Some(located) => located,
        None => (
            root,
            object([
                ("state", string("unresolved")),
                ("path", Value::Null),
                ("project_id", Value::Null),
                ("counts_by_lifecycle", Value::Null),
            ]),
        ),
    };
    report.set("repository", path_string(&root));
    report.set("bank", bank.clone());
    identities(&mut report, &root, &bank);
    if let Some(active) = active
        .as_ref()
        .filter(|active| get(active, "omp_binding") != &Value::Null)
    {
        attempt(
            &mut report,
            "omp.native_boundary",
            "Inspect saved omp binding and retained source evidence. Disk integrity is not native discovery, assessment, refresh or witnessed publication.",
            None,
            |report| {
                let value = crate::omp_installation::diagnostics(&root, Some(active))?;
                report.set("omp", value);
                Ok(())
            },
        );
    }
    if operation == "status" {
        attempt(
            &mut report,
            "data.purge",
            "Preserve purge metadata and quarantine; inspect through am doctor without resuming deletion.",
            None,
            |report| shallow_purge_state(report, env, &root),
        );
    }
    if operation == "doctor" {
        let bank_path = text(&bank, "path").map(PathBuf::from);
        doctor_checks(
            &mut report,
            env,
            services,
            active.as_ref(),
            &root,
            bank_path,
            projection,
        )?;
    }
    setup_assessment(&mut report);
    let mut result = report.finish();
    if operation == "doctor" {
        let health = if get(&result, "failures")
            .as_array()
            .is_some_and(|failures| !failures.is_empty())
        {
            "failures_detected"
        } else {
            "inspected_with_qualifications"
        };
        match result.get_mut("installation") {
            Some(installation @ Value::Object(_)) => set(installation, "health", string(health)),
            _ => set(
                &mut result,
                "installation",
                object([("health", string(health))]),
            ),
        }
    }
    Ok(result)
}

// ----- command -----

/// One diagnostics invocation (`am version|status|doctor`).
#[derive(Debug, Clone)]
pub struct Request {
    pub operation: String,
    pub repo: PathBuf,
    pub projection: Option<PathBuf>,
    pub fix: bool,
}

fn input_failure(operation: &str, error: &install::Error) -> Value {
    let mut report = Report::new(operation);
    match capacity(error) {
        Some(capacity) => {
            report.add(
                "bank.inspection",
                "failure",
                capacity.message(),
                Some(crate::bank_capacity::REPAIR),
                None,
            );
            let details = crate::bank_capacity::details(capacity);
            if let Some(check) = report.last_check() {
                for (key, value) in details.as_object().unwrap_or(&[]) {
                    set(check, key, value.clone());
                }
            }
        }
        None => report.add(
            "diagnostic.inputs",
            "failure",
            "Diagnostic input or local layout cannot be inspected safely.",
            Some("Use an existing repository directory and canonical readable configuration paths; rerun this helper from a trusted complete package."),
            None,
        ),
    }
    report.finish()
}

/// Baseline `execute`: the report as `json.dumps(value, indent=2)`; exit 0
/// only when no failure was found.
pub fn command(env: &Environment, services: &Services, request: &Request) -> CommandResult {
    let outcome = if request.operation == "doctor" && request.fix {
        crate::doctor_fix::run(env, services, &request.repo, request.projection.as_deref())
    } else {
        run(
            env,
            services,
            &request.operation,
            &request.repo,
            request.projection.as_deref(),
        )
        .map_err(install::Error::from)
    };
    let value = outcome.unwrap_or_else(|error| input_failure(&request.operation, &error));
    let ok = get(&value, "ok") == &Value::Bool(true);
    match json::encode(&value, json::INDENT2_ASCII) {
        Ok(text) => CommandResult {
            value: Some(value),
            exit_code: if ok { 0 } else { 1 },
            stdout: text + "\n",
            stderr: String::new(),
        },
        Err(error) => CommandResult {
            value: Some(value),
            exit_code: 1,
            stdout: String::new(),
            stderr: format!("{error}\n"),
        },
    }
}
