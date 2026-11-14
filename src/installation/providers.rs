//! Installation's side of other modules' seams: the CLI pack root, the
//! console receipt reader, private-state installation inventory, the
//! installation half of `OmpActivation`, `managed_source::Installation` and
//! `cache_recovery::Installation`.
use super::filesystem::canonical;
use super::plan::{activation, activation_template, source_info};
use super::records::{read_receipt, receipt_is_rust_era};
use super::seams::{RegisteredArtifacts, Services};
use super::transaction::verify_targets;
use super::{Environment, Result, locations, source_binary};
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::lifecycle::{get, text};
use crate::omp_installation::{
    self, HeldSource, NativeDiagnostics, OmpActivation, RuntimeLocations,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The current (Rust-era) installed receipt, if any.
fn rust_receipt(env: &Environment) -> Result<(PathBuf, Value)> {
    let state = locations(env)?.state;
    let receipt = read_receipt(env, &state)?
        .filter(receipt_is_rust_era)
        .ok_or_else(|| {
            ContractError::new(
                "No installed Rust runtime receipt owns this executable; pass --pack PATH",
            )
        })?;
    Ok((state, receipt))
}

/// The installed pack of the running binary, from the receipt: the
/// executable must be the receipt's owned runtime binary.
pub fn pack_root(env: &Environment, executable: &Path) -> Result<PathBuf> {
    let (_, receipt) = rust_receipt(env)?;
    let source = get(&receipt, "source");
    let binary = source_binary(source)?;
    super::require(
        canonical(executable)? == canonical(&binary)?,
        "This executable is not the installed agent-memory runtime; pass --pack PATH",
    )?;
    Ok(PathBuf::from(text(source, "pack").unwrap_or_default()))
}

/// `cli::PackRoot` for the running executable in the process environment.
pub struct RuntimePackRoot;
impl crate::cli::PackRoot for RuntimePackRoot {
    fn pack_root(&self) -> crate::error::Result<PathBuf> {
        let executable =
            std::env::current_exe().map_err(|error| ContractError::new(error.to_string()))?;
        pack_root(&Environment::process(), &executable).map_err(into_contract)
    }
}

pub(crate) fn into_contract(error: super::Error) -> ContractError {
    match error {
        super::Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    }
}

/// `console::InstalledReceipt`: `locations()` + `read_receipt(state)`.
pub struct InstalledReceiptProvider;
impl crate::console::InstalledReceipt for InstalledReceiptProvider {
    fn receipt(&self) -> crate::error::Result<Option<Value>> {
        let env = Environment::process();
        let state = locations(&env).map_err(into_contract)?.state;
        read_receipt(&env, &state).map_err(into_contract)
    }
}

/// `PrivateStateHooks` installation inventory: the saved bank inventory
/// fields of the current receipt (`private_state.repository_inventory`).
pub fn installation_inventory(env: &Environment) -> Result<Option<Value>> {
    let state = locations(env)?.state;
    let Some(receipt) = read_receipt(env, &state)? else {
        return Ok(None);
    };
    let mut fields = Vec::new();
    for key in [
        "presets",
        "rollback_repositories",
        "observed_repositories",
        "banks",
        "bank_bindings",
        "source_bindings",
    ] {
        if let Some(value) = receipt.get(key) {
            fields.push((key.to_owned(), value.clone()));
        }
    }
    Ok(Some(Value::Object(fields)))
}

/// Installation half of `OmpActivation::locations`.
pub fn omp_locations(env: &Environment, executable: &Path) -> Result<RuntimeLocations> {
    let (state, receipt) = rust_receipt(env)?;
    let pack = pack_root(env, executable)?;
    let binary = source_binary(get(&receipt, "source"))?;
    Ok(RuntimeLocations {
        state,
        pack: canonical(&pack)?,
        binary,
    })
}

/// Installation half of `OmpActivation::verify_targets`.
pub fn omp_verify_targets(services: &Services, targets: &[&Value], source: &Value) -> Result<()> {
    verify_targets(services, targets, Some(source), false).map(|_| ())
}

/// Installation half of `OmpActivation::activation_template`.
pub fn omp_activation_template(pack: &Path) -> Result<String> {
    activation_template(pack, true)
}

/// The T2f half of the frozen activation interface (`omp_boundary`,
/// `omp_projection`, `omp_stream`, `omp_transport`, `turn_sources`).
pub trait OmpHarness: Send + Sync {
    fn diagnostics(&self, repo: &Path) -> omp_installation::Result<NativeDiagnostics>;
    fn observe_marker(
        &self,
        repo: &Path,
        held: Option<&dyn HeldSource>,
        operation: &mut dyn FnMut(&Value) -> omp_installation::Result<()>,
    ) -> omp_installation::Result<()>;
    fn instruction_uri(&self, relative: &str) -> Option<String>;
}

/// The composite `OmpActivation`: installation's four methods plus a
/// registered harness half. CLI wiring registers it once.
pub struct InstallationActivation<H: OmpHarness> {
    pub env: Environment,
    pub services: Services,
    pub executable: PathBuf,
    pub harness: H,
}
fn omp_error(error: super::Error) -> omp_installation::Error {
    match error {
        super::Error::Unregistered(_) => omp_installation::Error::MissingActivation,
        other => omp_installation::Error::Contract(into_contract(other)),
    }
}
impl<H: OmpHarness> OmpActivation for InstallationActivation<H> {
    fn locations(&self) -> omp_installation::Result<RuntimeLocations> {
        omp_locations(&self.env, &self.executable).map_err(omp_error)
    }
    fn verify_targets(&self, targets: &[&Value], source: &Value) -> omp_installation::Result<()> {
        omp_verify_targets(&self.services, targets, source).map_err(omp_error)
    }
    fn activation_template(&self, pack: &Path) -> omp_installation::Result<String> {
        omp_activation_template(pack).map_err(omp_error)
    }
    fn activation(
        &self,
        raw: &[u8],
        expected: &str,
        template: &str,
    ) -> omp_installation::Result<Vec<u8>> {
        activation(raw, Some(expected), template).map_err(omp_error)
    }
    fn diagnostics(&self, repo: &Path) -> omp_installation::Result<NativeDiagnostics> {
        self.harness.diagnostics(repo)
    }
    fn observe_marker(
        &self,
        repo: &Path,
        held: Option<&dyn HeldSource>,
        operation: &mut dyn FnMut(&Value) -> omp_installation::Result<()>,
    ) -> omp_installation::Result<()> {
        self.harness.observe_marker(repo, held, operation)
    }
    fn instruction_uri(&self, relative: &str) -> Option<String> {
        self.harness.instruction_uri(relative)
    }
}

/// `managed_source::Installation`: Python-era managed source records,
/// verified (payload before cleanliness) for read-only legacy checkouts.
pub struct ManagedSourceInfo;
impl crate::managed_source::Installation for ManagedSourceInfo {
    fn source_info(
        &self,
        root: &Path,
        kind: &str,
        remote: &str,
        reference: &str,
    ) -> crate::error::Result<Value> {
        let services = Services {
            updating: None,
            private_state: None,
            omp_boundary: None,
            artifacts: Arc::new(RegisteredArtifacts),
            release: Default::default(),
            executable: None,
        };
        let info = source_info(&services, root, kind).map_err(into_contract)?;
        let mut value = info.value;
        if let Value::Object(fields) = &mut value {
            for (key, field) in fields.iter_mut() {
                if key == "remote" {
                    *field = Value::String(remote.to_owned());
                } else if key == "ref" {
                    *field = Value::String(reference.to_owned());
                }
            }
        }
        Ok(value)
    }
}
/// The one `'static` managed-source provider the composition root registers.
pub static MANAGED_SOURCE_INFO: ManagedSourceInfo = ManagedSourceInfo;

/// `cache_recovery::Installation`: fault points and clean-pack inspection.
pub struct CacheInstallation;
impl crate::cache_recovery::Installation for CacheInstallation {
    fn fault(&self, point: &str) -> crate::release_distribution::Result<()> {
        Ok(crate::bank_writes::fault(point)?)
    }
    fn inspect_package(&self, pack: &Path) -> crate::release_distribution::Result<Value> {
        Ok(crate::artifact::inspect(pack)?)
    }
}
