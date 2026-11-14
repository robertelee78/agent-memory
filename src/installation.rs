//! Install a portable runtime with external ownership and recoverable activation.
//!
//! Baseline `installation.py`. The Rust runtime installs one owned `am` binary
//! inside an owned runtime root (`$XDG_DATA_HOME/agent-memory/runtime`: `bin/am`,
//! the signed release manifest and its signature, `skills/agent-memory`), owns
//! `bin_dir/am` and `bin_dir/agent-memory` as links to that binary, and adopts
//! Python-era (receipt v1/v2) state inside the same journaled transaction.
//! Directory replacement uses native exchange, never a rename-away visibility gap.

mod adoption;
mod filesystem;
mod lock;
mod plan;
mod providers;
mod records;
pub(crate) mod seams;
mod transaction;

pub use adoption::{
    legacy_extension_wrapper, legacy_source_only_wrapper, legacy_wrapper, python_era_kind,
};
pub use filesystem::{fingerprint, same};
pub use lock::{Locked, locked, read_owner};
pub use plan::{
    Action, Command, Configuration, InstallRequest, Mode, Plan, PrepareOptions, SourceInfo,
    UninstallFields, activation, activation_template, client_hook_entries, configuration,
    hook_paths, lifecycle_paths, preflight_layout, preflight_path, prepare, presets, public_plan,
    source_info, stage_release,
};
pub use providers::{
    CacheInstallation, InstallationActivation, InstalledReceiptProvider, MANAGED_SOURCE_INFO,
    ManagedSourceInfo, OmpHarness, RuntimePackRoot, installation_inventory,
    omp_activation_template, omp_locations, omp_verify_targets, pack_root,
};
pub use records::{encoded, load, read_journal, read_receipt, receipt_is_rust_era};
pub use seams::{
    ArtifactInspection, OmpBoundary, PrivateState, RegisteredArtifacts, ReleaseAccess, Services,
    Updating, register_omp_boundary, register_private_state, register_updating,
};
pub use transaction::{
    ActivationHooks, HookVerification, failure_details, failure_repair,
    finish_stored_source_activation, install, install_command, recover, recover_command, rollback,
    verify_hook_target, verify_targets,
};

use crate::codec::json::Value;
use crate::error::ContractError;
use crate::host;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Rust-era receipts; the Python tree refuses this unknown schema.
pub const RECEIPT_SCHEMA: &str = "agent-memory/install-receipt-v3";
/// Python-era receipt schemas: read-only adoption inputs.
pub const LEGACY_RECEIPT_SCHEMAS: [&str; 2] = [
    "agent-memory/install-receipt-v1",
    "agent-memory/install-receipt-v2",
];
pub const JOURNAL_SCHEMA: &str = "agent-memory/install-journal-v1";
pub const OWNER_SCHEMA: &str = "agent-memory/lifecycle-owner-v1";
pub const LOCK_SCHEMA: &str = "agent-memory/lifecycle-lock-v1";
pub const PLAN_SCHEMA: &str = "agent-memory/install-plan-v1";
pub const BEGIN: &str = "<!-- agent-memory:activation:begin -->";
pub const END: &str = "<!-- agent-memory:activation:end -->";
pub const SHIM: &str =
    include_str!("../skills/agent-memory/templates/activation-markdown.example.txt");
pub const DEFAULT_REMOTE: &str = "https://github.com/robertelee78/agent-memory.git";
pub const COMMAND_NAMES: [&str; 2] = ["am", "agent-memory"];
pub const ATOMICITY: &str = "per target; journaled rollback across targets";
pub const MEMORY_DATA_LOCATION: &str = "repository-local memory-bank/; not installer-owned";
/// Baseline `load`: installation records are at most 1 MiB.
pub const MAX_RECORD: usize = 1024 * 1024;
/// Baseline lock read: `os.read(fd, 65537)` with a 65,536-byte bound.
pub const MAX_LOCK: usize = 65536;
/// Candidate activation shims are one bounded block.
pub const MAX_SHIM: usize = 4096;
/// Guidance files holding an activation block (shared with OMP verification).
pub const MAX_GUIDANCE: usize = crate::omp_installation::MAX_ACTIVATION;
/// Baseline fingerprint/copy bounds.
pub const MAX_MANAGED_FILE: usize = 64 * 1024 * 1024;
pub const MAX_MANAGED_ENTRIES: usize = 40000;
pub const MAX_MANAGED_BYTES: u64 = 512 * 1024 * 1024;
/// Baseline rollback repository inventory bound.
pub const MAX_REPOSITORIES: usize = 1024;
/// Development sources supply a prebuilt binary here; the installer never runs Cargo.
pub const DEVELOPMENT_BINARY: &str = "target/release/am";
/// Release runtime layout (release_distribution `installed`).
pub const RELEASE_PACK: &str = "skills/agent-memory";
pub const RELEASE_BINARY: &str = "bin/am";
/// Every receipt-v3 target kind; v1/v2 never contain the last two.
pub const TARGET_KINDS: [&str; 9] = [
    "copy",
    "link",
    "wrapper",
    "activation",
    "source",
    "hook-config",
    "hook-link",
    "command-link",
    "legacy-source",
];
pub const LEGACY_TARGET_KINDS: [&str; 7] = [
    "copy",
    "link",
    "wrapper",
    "activation",
    "source",
    "hook-config",
    "hook-link",
];
pub const MANAGED_GUIDANCE: &str = "Managed Git remote/ref channels are not installable or updatable by the Rust runtime; re-run the trusted bootstrap without --remote/--ref to install the signed release. An existing managed checkout is adopted read-only and left in place.";
pub const ADOPTION_GUIDANCE: &str = "Python-era installation adoption requires the verified bootstrap binary; re-run the trusted bootstrap. Preserve the source and receipts.";
pub const RELEASE_GUIDANCE: &str = "Signed release installation requires the verified bootstrap binary; run the trusted bootstrap (install.sh).";
pub const RESTART_NOTICE: &str = "Restart the listed clients: running sessions keep their previous hook, skill and extension targets until they restart. A running omp.ts keeps its old spawn target until the harness restarts.";

/// The compiled target triple recorded as receipt `source.target`.
pub const TARGET: &str = if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
    "aarch64-apple-darwin"
} else if cfg!(all(
    target_arch = "x86_64",
    target_os = "linux",
    target_env = "musl"
)) {
    "x86_64-unknown-linux-musl"
} else if cfg!(all(
    target_arch = "aarch64",
    target_os = "linux",
    target_env = "musl"
)) {
    "aarch64-unknown-linux-musl"
} else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
    "x86_64-unknown-linux-gnu"
} else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
    "aarch64-unknown-linux-gnu"
} else {
    "unsupported"
};

/// Baseline `HookOwnershipError` codes; bounded and non-sensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookCode {
    InsufficientReceipt,
    UnsafePath,
    UnsafeType,
    ModeChanged,
    ConfigUnavailable,
    MalformedJson,
    DuplicateJsonKey,
    UnsupportedShape,
    OwnedGroupMissingOrChanged,
    OwnedGroupDuplicate,
    ConcurrentChange,
}
impl HookCode {
    /// Unknown codes map to `config_unavailable`, as the baseline constructor does.
    pub fn from_code(code: &str) -> Self {
        match code {
            "insufficient_receipt" => Self::InsufficientReceipt,
            "unsafe_path" => Self::UnsafePath,
            "unsafe_type" => Self::UnsafeType,
            "mode_changed" => Self::ModeChanged,
            "malformed_json" => Self::MalformedJson,
            "duplicate_json_key" => Self::DuplicateJsonKey,
            "unsupported_shape" => Self::UnsupportedShape,
            "owned_group_missing_or_changed" => Self::OwnedGroupMissingOrChanged,
            "owned_group_duplicate" => Self::OwnedGroupDuplicate,
            "concurrent_change" => Self::ConcurrentChange,
            _ => Self::ConfigUnavailable,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::InsufficientReceipt => "insufficient_receipt",
            Self::UnsafePath => "unsafe_path",
            Self::UnsafeType => "unsafe_type",
            Self::ModeChanged => "mode_changed",
            Self::ConfigUnavailable => "config_unavailable",
            Self::MalformedJson => "malformed_json",
            Self::DuplicateJsonKey => "duplicate_json_key",
            Self::UnsupportedShape => "unsupported_shape",
            Self::OwnedGroupMissingOrChanged => "owned_group_missing_or_changed",
            Self::OwnedGroupDuplicate => "owned_group_duplicate",
            Self::ConcurrentChange => "concurrent_change",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::InsufficientReceipt => "The receipt lacks sufficient owned-hook evidence.",
            Self::UnsafePath => "The hook path does not meet the ownership trust policy.",
            Self::UnsafeType => "The hook target is not an ordinary file.",
            Self::ModeChanged => "The hook target mode differs from its receipt.",
            Self::ConfigUnavailable => {
                "The hook configuration is unavailable or exceeds its bound."
            }
            Self::MalformedJson => "The hook configuration is not valid JSON.",
            Self::DuplicateJsonKey => "The hook configuration contains duplicate JSON keys.",
            Self::UnsupportedShape => "The hook configuration has an unsupported shape.",
            Self::OwnedGroupMissingOrChanged => "A recorded hook group is missing or changed.",
            Self::OwnedGroupDuplicate => "A recorded hook group is duplicated.",
            Self::ConcurrentChange => "The hook configuration changed during verification.",
        }
    }
}

/// Installation refusals. Callers match variants, never message text.
#[derive(Debug)]
pub enum Error {
    /// A violated contract, including `Kind::TrustPath` refusals.
    Contract(ContractError),
    /// Unsafe storage layout; recovery must not be suggested under it.
    Layout(String),
    /// A bounded owned-hook verification failure.
    Hook(HookCode),
    /// Signed release verification, transport or download failure.
    Release(crate::release_distribution::Error),
    /// A lazy callee (named) is not registered; the operation refuses.
    Unregistered(&'static str),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(error) => error.fmt(f),
            Self::Layout(message) => f.write_str(message),
            Self::Hook(code) => f.write_str(code.message()),
            Self::Release(error) => error.fmt(f),
            Self::Unregistered(name) => write!(
                f,
                "{name} is not registered for this runtime; the lifecycle operation refuses"
            ),
        }
    }
}
impl std::error::Error for Error {}
impl From<ContractError> for Error {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Contract(ContractError::new(error.to_string()))
    }
}
impl From<crate::release_distribution::Error> for Error {
    fn from(error: crate::release_distribution::Error) -> Self {
        match error {
            crate::release_distribution::Error::Contract(error) => Self::Contract(error),
            other => Self::Release(other),
        }
    }
}
impl From<crate::omp_installation::Error> for Error {
    fn from(error: crate::omp_installation::Error) -> Self {
        match error {
            crate::omp_installation::Error::MissingActivation => {
                Self::Unregistered("OMP activation interface")
            }
            crate::omp_installation::Error::Contract(error) => Self::Contract(error),
        }
    }
}
impl From<crate::managed_source::Error> for Error {
    fn from(error: crate::managed_source::Error) -> Self {
        match error {
            crate::managed_source::Error::Contract(error) => Self::Contract(error),
            other => Self::Contract(ContractError::new(other.to_string())),
        }
    }
}
impl From<crate::pack_validation::Invalid> for Error {
    fn from(error: crate::pack_validation::Invalid) -> Self {
        Self::Contract(error.into())
    }
}
impl From<crate::fs::ReadError> for Error {
    fn from(error: crate::fs::ReadError) -> Self {
        Self::Contract(ContractError::new(error.to_string()))
    }
}
impl From<crate::fs::LockError> for Error {
    fn from(error: crate::fs::LockError) -> Self {
        match error {
            crate::fs::LockError::Unsafe(error) => Self::Contract(error),
            other => Self::Contract(ContractError::new(other.to_string())),
        }
    }
}

pub(crate) fn require(condition: bool, message: impl AsRef<str>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::new(message.as_ref()).into())
    }
}
pub(crate) fn require_layout(condition: bool, message: impl AsRef<str>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Layout(message.as_ref().to_owned()))
    }
}
pub(crate) fn sha(raw: &[u8]) -> String {
    crate::pack_validation::sha256(raw)
}
pub(crate) fn path_text(path: &Path) -> Result<&str> {
    Ok(host::text(path.as_os_str(), "installation path")?)
}
pub(crate) fn path_value(path: &Path) -> Result<Value> {
    Ok(Value::String(path_text(path)?.to_owned()))
}

/// The process environment, injectable so isolated tests never mutate it.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    values: BTreeMap<OsString, OsString>,
}
impl Environment {
    pub fn process() -> Self {
        Self {
            values: std::env::vars_os().collect(),
        }
    }
    pub fn from_pairs(pairs: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        Self {
            values: pairs.into_iter().collect(),
        }
    }
    pub fn var(&self, key: &str) -> Option<&OsStr> {
        self.values.get(OsStr::new(key)).map(OsString::as_os_str)
    }
    pub fn pairs(&self) -> impl Iterator<Item = (&OsString, &OsString)> {
        self.values.iter()
    }
    /// Python `Path.home()`: `$HOME`, otherwise the passwd entry.
    pub fn home(&self) -> Result<PathBuf> {
        Ok(host::expanduser_with_home(
            Path::new("~"),
            self.var("HOME"),
        )?)
    }
    /// `Path(os.environ.get(key, default))`.
    pub fn directory(&self, key: &str, home_relative: &str) -> Result<PathBuf> {
        match self.var(key) {
            Some(value) => Ok(PathBuf::from(value)),
            None => Ok(self.home()?.join(home_relative)),
        }
    }
    pub fn expanduser(&self, path: &Path) -> Result<PathBuf> {
        Ok(host::expanduser_with_home(path, self.var("HOME"))?)
    }
}

/// Baseline `absolute`: expanded, absolute, no parent traversal; ancestors
/// resolved while the final node is retained so a collision cannot hide.
pub fn absolute(env: &Environment, value: &Path) -> Result<PathBuf> {
    let path = env.expanduser(value)?;
    require(
        path.is_absolute(),
        format!("Installation paths must be absolute: {}", path.display()),
    )?;
    require(
        !path
            .components()
            .any(|part| matches!(part, Component::ParentDir)),
        "Installation paths must not contain parent traversal",
    )?;
    let normalized: PathBuf = path.components().collect();
    match (normalized.parent(), normalized.file_name()) {
        (Some(parent), Some(name)) => Ok(crate::fs::canonical_path(parent, false)?.join(name)),
        _ => Ok(normalized),
    }
}

/// Owned state, legacy managed-source and Rust runtime locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    /// `$XDG_STATE_HOME/agent-memory/install`: receipt, journal, lock, owner.
    pub state: PathBuf,
    /// `$XDG_DATA_HOME/agent-memory/source`: Python-era managed/release tree (inert).
    pub code: PathBuf,
    /// `$XDG_DATA_HOME/agent-memory/runtime`: the owned Rust runtime root.
    pub runtime: PathBuf,
}
impl Locations {
    pub fn runtime_binary(&self) -> PathBuf {
        self.runtime.join(RELEASE_BINARY)
    }
    pub fn runtime_pack(&self) -> PathBuf {
        self.runtime.join(RELEASE_PACK)
    }
}

/// Baseline `locations`, plus the new owned runtime root.
pub fn locations(env: &Environment) -> Result<Locations> {
    let state_home = env.directory("XDG_STATE_HOME", ".local/state")?;
    let data_home = env.directory("XDG_DATA_HOME", ".local/share")?;
    Ok(Locations {
        state: absolute(env, &state_home.join("agent-memory/install"))?,
        code: absolute(env, &data_home.join("agent-memory/source"))?,
        runtime: absolute(env, &data_home.join("agent-memory/runtime"))?,
    })
}

/// The binary a recorded source executes: release root `bin/am`, or a
/// development source's prebuilt `target/release/am`.
pub fn source_binary(source: &Value) -> Result<PathBuf> {
    let root = source
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ContractError::new("Installed source has no runtime path"))?;
    match source.get("kind").and_then(Value::as_str) {
        Some("release") => Ok(Path::new(root).join(RELEASE_BINARY)),
        Some("development") => Ok(Path::new(root).join(DEVELOPMENT_BINARY)),
        _ => Err(ContractError::new("Installed source kind has no Rust runtime binary").into()),
    }
}

// Internal lifecycle primitives shared with `updating`, `uninstallation`,
// `purging` and `doctor_fix` (baseline cross-module `install.*` calls).
pub(crate) use filesystem::{
    is_directory, is_regular, lexists, named, native_rename, read_optional, replace_record,
    sync_dir,
};
pub(crate) use plan::unchanged_parent;
pub(crate) use transaction::{receipt_committed, require_replannable_journal};

// omp_boundary (baseline `install._verify_rolled_back_targets`).
pub(crate) use transaction::verify_rolled_back_targets;
