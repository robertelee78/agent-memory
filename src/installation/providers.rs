//! Installation functions other modules call directly: the CLI pack root, the
//! console receipt reader, private-state installation inventory, the
//! installation half of `OmpActivation` and managed-source records.
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
use crate::scope_binding::PrivateEnrollmentContext;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RUNTIME_RECEIPT_GUIDANCE: &str =
    "No installed Rust runtime receipt owns this executable; use the trusted bootstrap installer";

/// The executing pack and binary selected together from one installed v3 receipt.
/// The executable is checked against that receipt, never used as a fallback.
pub fn executing_runtime(env: &Environment, executable: &Path) -> Result<(PathBuf, PathBuf)> {
    executing_runtime_if_installed(env, executable)?
        .ok_or_else(|| ContractError::new(RUNTIME_RECEIPT_GUIDANCE).into())
}

/// Resolve one current installed receipt, checking its selected executable.
/// `None` means no receipt or a valid uninstalled receipt, as in `read_receipt`.
/// Present invalid receipts, legacy receipts and executable mismatches are errors.
pub fn executing_runtime_if_installed(
    env: &Environment,
    executable: &Path,
) -> Result<Option<(PathBuf, PathBuf)>> {
    let state = locations(env)?.state;
    let Some(receipt) = read_receipt(env, &state)? else {
        return Ok(None);
    };
    super::require(receipt_is_rust_era(&receipt), RUNTIME_RECEIPT_GUIDANCE)?;
    let source = get(&receipt, "source");
    let binary = source_binary(source)?;
    super::require(
        canonical(executable)? == canonical(&binary)?,
        "This executable is not the installed agent-memory runtime; use the trusted bootstrap installer",
    )?;
    Ok(Some((
        PathBuf::from(text(source, "pack").unwrap_or_default()),
        binary,
    )))
}

/// The installed pack of the running binary, from the same runtime authority.
pub fn pack_root(env: &Environment, executable: &Path) -> Result<PathBuf> {
    executing_runtime(env, executable).map(|(pack, _)| pack)
}

/// The installed pack of the running executable in the process environment.
pub(crate) fn runtime_pack_root() -> crate::error::Result<PathBuf> {
    let executable = crate::host::current_executable()?;
    pack_root(&Environment::process(), &executable).map_err(into_contract)
}

pub(crate) fn into_contract(error: super::Error) -> ContractError {
    match error {
        super::Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    }
}

/// The installed receipt the console bootstrap consults: `locations()` +
/// `read_receipt(state)`.
pub(crate) fn installed_receipt() -> crate::error::Result<Option<Value>> {
    let env = Environment::process();
    let state = locations(&env).map_err(into_contract)?.state;
    read_receipt(&env, &state).map_err(into_contract)
}

/// Private-state installation inventory: the saved bank inventory
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
    let state = locations(env)?.state;
    let (pack, binary) = executing_runtime(env, executable)?;
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
        operation: &mut dyn FnMut(&PrivateEnrollmentContext) -> omp_installation::Result<()>,
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
        operation: &mut dyn FnMut(&PrivateEnrollmentContext) -> omp_installation::Result<()>,
    ) -> omp_installation::Result<()> {
        self.harness.observe_marker(repo, held, operation)
    }
    fn instruction_uri(&self, relative: &str) -> Option<String> {
        self.harness.instruction_uri(relative)
    }
}

/// Python-era managed source records, verified (payload before cleanliness)
/// for read-only legacy checkouts.
pub(crate) fn managed_source_info(
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
