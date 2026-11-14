//! Genuine installed runtime bytes for native consumer journeys.
use agent_memory::codec::json::{self, Value};
use agent_memory::installation;
use agent_memory::process::{Output, Status};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub struct NativeRuntime {
    pub source: PathBuf,
    pub pack: PathBuf,
    pub binary: PathBuf,
    pub command: PathBuf,
}

impl NativeRuntime {
    pub fn pair(&self) -> (&Path, &Path) {
        (&self.pack, &self.binary)
    }
}

/// The runner owns the isolated environment and bounded child deadline. It
/// receives a complete argv and must not modify process-global providers/env.
pub fn install(root: &Path, mut run: impl FnMut(&[OsString]) -> Output) -> NativeRuntime {
    for relative in [
        "cache",
        "credentials",
        "config/agent-memory",
        "state/agent-memory",
        "backups",
    ] {
        let path = root.join(relative);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let source = root.join("dev-source");
    let pack = source.join("skills/agent-memory");
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
    for (relative, (bytes, permissions)) in
        agent_memory::pack_validation::inventory(&shipped).unwrap()
    {
        let path = pack.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(permissions)).unwrap();
    }
    let binary = source.join("target/release/am");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = root.join("home/.local/bin/am");
    let output = run(&[
        binary.as_os_str().to_owned(),
        "--json".into(),
        "install".into(),
        "--development-source".into(),
        source.as_os_str().to_owned(),
        "--mode".into(),
        "link".into(),
        "--presets".into(),
        "none".into(),
        "--no-guidance".into(),
        "--bin-dir".into(),
        command.parent().unwrap().as_os_str().to_owned(),
    ]);
    assert_eq!(
        output.status,
        Status::Code(0),
        "installer: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    let result = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(
        result.get("verified"),
        Some(&Value::Bool(true)),
        "{result:?}"
    );
    let receipt = json::decode_bytes(
        &std::fs::read(root.join("state/agent-memory/install/receipt.json")).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        receipt.get("schema").and_then(Value::as_str),
        Some("agent-memory/install-receipt-v3"),
    );
    let recorded = receipt.get("source").unwrap();
    assert_eq!(installation::source_binary(recorded).unwrap(), binary);
    assert_eq!(recorded.get("pack").and_then(Value::as_str), pack.to_str());
    assert_eq!(std::fs::canonicalize(&command).unwrap(), binary);
    NativeRuntime {
        source,
        pack,
        binary,
        command,
    }
}

fn component_runtime() -> agent_memory::error::Result<(PathBuf, PathBuf)> {
    use agent_memory::error::ContractError;
    let root =
        super::require_current_world().map_err(|error| ContractError::new(error.to_string()))?;
    installation::executing_runtime(
        &installation::Environment::process(),
        &root.join("home/.local/bin/am"),
    )
    .map_err(|error| match error {
        installation::Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    })
}

/// Select the genuinely installed command for isolated component consumers.
/// The dev-tools pack override is locator input, not native execution proof;
/// native children must invoke `NativeRuntime::command` with their strict locator.
pub fn register_component_runtime() {
    use std::sync::Arc;
    let (pack, _) = component_runtime().unwrap();
    agent_memory::cli::use_development_pack(Some(pack));
    let frontier = Arc::new(agent_memory::recovery_frontier::FrontierProvider::new(
        component_runtime,
    ));
    agent_memory::bank_writes::register_recovery_frontier(Some(frontier.clone()));
    agent_memory::bank_contract::register_recovery_coverage(Some(frontier));
}
