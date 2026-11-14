//! One coherent seam per lazily imported, not-yet-ported callee. A missing
//! registration is a typed `Error::Unregistered` refusal, never a silent pass.
//! `Services` carries the seams explicitly, so isolated tests never share
//! process-global registrations; production uses `Services::registered`.
use super::{Error, Result};
use crate::cache_recovery::Qualification;
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::release_distribution::{self as release, DownloadOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Baseline `updating` callees used by installation (`banks`,
/// `qualified_current`, `current`, `release_transition`, `compatible_surface`).
/// Implemented by `updating`.
pub trait Updating: Send + Sync {
    /// `updating.banks(extra, compatibility, require_readable=…)`: the checked
    /// bank inventory (`observed_repositories` …) or the baseline refusal.
    fn banks(
        &self,
        repositories: &[String],
        compatibility: &Value,
        require_readable: bool,
    ) -> Result<Value>;
    /// `updating.qualified_current(state, code)`: the current-runtime check and
    /// its optional opaque-cache qualification.
    fn qualified_current(&self, state: &Path, runtime: &Path) -> Result<Option<Qualification>>;
    /// `updating.current(state, code, cache_recovery=…)`.
    fn current(&self, state: &Path, runtime: &Path, cache: Option<&Qualification>) -> Result<()>;
    /// `updating.release_transition(saved, source, root)`.
    fn release_transition(&self, saved: &Value, source: &Value, root: &Path) -> Result<()>;
    /// `updating.compatible_surface(saved, source, cache_recovery=…)`.
    fn compatible_surface(
        &self,
        saved: &Value,
        source: &Value,
        cache: Option<&Qualification>,
    ) -> Result<()>;
}

/// Baseline `private_state` callees (`activation_guard`, `prepare_lifecycle`,
/// `require_compatible`, `public_summary`). Implemented by `private_state`.
pub trait PrivateState: Send + Sync {
    /// `activation_guard(installation, target, operation, transition)`: runs
    /// `transition` (which may replace the target compatibility) and then
    /// `body` with the guard's proof, holding the private-state barriers.
    fn activation_guard(
        &self,
        installation: &Value,
        target: Option<&Value>,
        operation: &str,
        transition: &mut dyn FnMut() -> Result<Option<Value>>,
        body: &mut dyn FnMut(&Value) -> Result<Value>,
    ) -> Result<Value>;
    fn prepare_lifecycle(&self, plan: &Value, check: bool) -> Result<Value>;
    fn require_compatible(
        &self,
        installation: &Value,
        target: &Value,
        operation: &str,
    ) -> Result<Value>;
    fn public_summary(&self, value: &Value) -> Result<Value>;
}

/// Baseline `omp_boundary` callees used by installation. Implemented by
/// `omp_boundary`.
pub trait OmpBoundary: Send + Sync {
    /// `omp_boundary.CAPABILITY`.
    fn capability(&self) -> &str;
    fn native_transition(&self, plan: &Value, check: bool) -> Result<()>;
    fn native_activation(&self, plan: &Value, check: bool) -> Result<Value>;
    fn finish_native_transition(&self, plan: &Value) -> Result<()>;
    fn recover_native_binding(&self, state: &Path) -> Result<()>;
    fn native_recovery_expected(&self, state: &Path, path: &str) -> Result<Option<Value>>;
    fn complete_stored_source_activation(&self, state: &Path) -> Result<Value>;
}

/// Artifact inspection of an installed pack plus its runtime binary.
/// Production delegates to `artifact` with its registered providers.
pub trait ArtifactInspection: Send + Sync {
    /// `artifact::inspect_runtime(pack, binary)`: the one artifact digest.
    fn inspect_runtime(&self, pack: &Path, binary: &Path) -> Result<Value>;
    /// `artifact::inspect(pack)`: portable-only inspection (Python-era identities).
    fn inspect(&self, pack: &Path) -> Result<Value>;
}

/// The production artifact inspection (registered command surface and migration catalog).
pub struct RegisteredArtifacts;
impl ArtifactInspection for RegisteredArtifacts {
    fn inspect_runtime(&self, pack: &Path, binary: &Path) -> Result<Value> {
        Ok(crate::artifact::inspect_runtime(pack, binary)?)
    }
    fn inspect(&self, pack: &Path) -> Result<Value> {
        Ok(crate::artifact::inspect(pack)?)
    }
}

/// Where and with which trust roots the signed release is fetched. Production
/// is the pinned GitHub release with the compiled allowed-signers.
#[derive(Clone)]
pub struct ReleaseAccess {
    pub first_manifest_url: String,
    pub assets: String,
    pub options: DownloadOptions,
    pub signers: String,
    /// Compiled version the bootstrap binary must equal.
    pub version: String,
}
impl Default for ReleaseAccess {
    fn default() -> Self {
        Self {
            first_manifest_url: format!(
                "https://github.com/{}/releases/latest/download/{}",
                release::REPOSITORY,
                release::MANIFEST
            ),
            assets: release::ASSETS.to_owned(),
            options: DownloadOptions::default(),
            signers: release::ALLOWED_SIGNERS.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// Everything installation reaches outside its own module.
#[derive(Clone)]
pub struct Services {
    pub updating: Option<Arc<dyn Updating>>,
    pub private_state: Option<Arc<dyn PrivateState>>,
    pub omp_boundary: Option<Arc<dyn OmpBoundary>>,
    pub artifacts: Arc<dyn ArtifactInspection>,
    pub release: ReleaseAccess,
    /// The running executable; its directory is never an installation target.
    pub executable: Option<PathBuf>,
}

static UPDATING: RwLock<Option<Arc<dyn Updating>>> = RwLock::new(None);
static PRIVATE_STATE: RwLock<Option<Arc<dyn PrivateState>>> = RwLock::new(None);
static OMP_BOUNDARY: RwLock<Option<Arc<dyn OmpBoundary>>> = RwLock::new(None);

fn poisoned() -> Error {
    Error::Contract(ContractError::new("Installation seam registry is poisoned"))
}
pub fn register_updating(provider: Option<Arc<dyn Updating>>) -> Result<()> {
    *UPDATING.write().map_err(|_| poisoned())? = provider;
    Ok(())
}
pub fn register_private_state(provider: Option<Arc<dyn PrivateState>>) -> Result<()> {
    *PRIVATE_STATE.write().map_err(|_| poisoned())? = provider;
    Ok(())
}
pub fn register_omp_boundary(provider: Option<Arc<dyn OmpBoundary>>) -> Result<()> {
    *OMP_BOUNDARY.write().map_err(|_| poisoned())? = provider;
    Ok(())
}

impl Services {
    /// The process registrations made by trusted CLI wiring.
    pub fn registered() -> Result<Self> {
        Ok(Self {
            updating: UPDATING.read().map_err(|_| poisoned())?.clone(),
            private_state: PRIVATE_STATE.read().map_err(|_| poisoned())?.clone(),
            omp_boundary: OMP_BOUNDARY.read().map_err(|_| poisoned())?.clone(),
            artifacts: Arc::new(RegisteredArtifacts),
            release: ReleaseAccess::default(),
            executable: std::env::current_exe().ok(),
        })
    }
    pub(crate) fn updating(&self) -> Result<&dyn Updating> {
        self.updating
            .as_deref()
            .ok_or(Error::Unregistered("Updating (bank compatibility)"))
    }
    pub(crate) fn private_state(&self) -> Result<&dyn PrivateState> {
        self.private_state
            .as_deref()
            .ok_or(Error::Unregistered("Private-state lifecycle guard"))
    }
    pub(crate) fn omp_boundary(&self) -> Result<&dyn OmpBoundary> {
        self.omp_boundary
            .as_deref()
            .ok_or(Error::Unregistered("OMP native boundary"))
    }
}
