//! Explicit receipt-owned OMP authority. Disk ownership never proves native loading.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::ContractError;
use crate::fs::{self, Descriptor, DirectoryChain, Fingerprint, Identity};
use crate::lifecycle::{get, object, string, text};
use crate::pack_validation::sha256;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const READER_CAPABILITY: &str = "omp-stored-source-v2";
pub const HISTORICAL_READER_CAPABILITY: &str = "omp-harness-v1";
/// The one stock omp release the native integration targets (tag commit
/// b7fe8c7f); `hooks/omp.ts` gates on the same exact `VERSION` string.
pub const SUPPORTED_VERSION: &str = "18.20.1";
pub const MAX_RECEIPT: usize = 1024 * 1024;
pub const MAX_MANUAL: usize = 16 * 1024 * 1024;
pub const MAX_ACTIVATION: usize = 2 * 1024 * 1024;
const FIELDS: [&str; 6] = [
    "agent_dir",
    "skill_target",
    "extension_target",
    "activation_target",
    "peer_reader_capability",
    "rollout_attested",
];

#[derive(Debug)]
pub enum Error {
    MissingActivation,
    Contract(ContractError),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingActivation => f.write_str(
                "OMP activation interface is not registered; native ownership verification refuses",
            ),
            Self::Contract(error) => error.fmt(f),
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
        ContractError::new(error.to_string()).into()
    }
}
fn require(condition: bool, message: &str) -> Result<()> {
    crate::artifact::require(condition, message).map_err(Into::into)
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    text(value, key).ok_or_else(|| {
        ContractError::new(format!("Malformed native ownership field: {key}")).into()
    })
}
fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| ContractError::new("Native ownership path is not UTF-8").into())
}
fn canonical_absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::Prefix(_)))
}
fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn same_value(left: &Value, right: &Value) -> Result<bool> {
    Ok(json::encode(left, json::SORTED_COMPACT_UTF8)?
        == json::encode(right, json::SORTED_COMPACT_UTF8)?)
}

pub fn binding(agent_dir: &str, no_guidance: bool, reader_capability: &str) -> Result<Value> {
    let root = Path::new(agent_dir);
    require(
        canonical_absolute(root),
        "Explicit --omp-agent-dir must be an absolute canonical root",
    )?;
    let normalized: PathBuf = root.components().collect();
    let root = normalized.as_path();
    require(
        fs::canonical_path(root, false).is_ok_and(|p| p == root),
        "omp agent root must not contain symlinks",
    )?;
    require(
        matches!(
            reader_capability,
            READER_CAPABILITY | HISTORICAL_READER_CAPABILITY
        ),
        "Unknown native reader capability",
    )?;
    Ok(object([
        ("agent_dir", string(path_text(root)?)),
        (
            "skill_target",
            string(path_text(&root.join("skills/agent-memory"))?),
        ),
        (
            "extension_target",
            string(path_text(&root.join("extensions/agent-memory.ts"))?),
        ),
        (
            "activation_target",
            if no_guidance {
                Value::Null
            } else {
                string(path_text(&root.join("AGENTS.md"))?)
            },
        ),
        ("peer_reader_capability", string(reader_capability)),
        ("rollout_attested", Value::Bool(true)),
    ]))
}

#[derive(Default)]
pub struct Configuration<'a> {
    pub omp_agent_dir: Option<&'a str>,
    pub omp_peers_upgraded: bool,
    pub omp_layout_quiescence: &'a [PathBuf],
}
pub fn configuration(
    args: &Configuration<'_>,
    old: Option<&Value>,
    selected: &str,
    no_guidance: bool,
) -> Result<Option<Value>> {
    let previous = old
        .map(|v| get(v, "omp_binding"))
        .filter(|v| **v != Value::Null);
    let enabled = selected.split(',').any(|v| v == "omp");
    require(
        previous.is_none() || enabled,
        "Removing saved omp selection requires explicit owned uninstall/reinstall; all excludes omp",
    )?;
    if !enabled {
        require(
            args.omp_agent_dir.is_none()
                && !args.omp_peers_upgraded
                && args.omp_layout_quiescence.is_empty(),
            "omp root/rollout attestation requires an explicit omp preset",
        )?;
        return Ok(None);
    }
    if let Some(previous) = previous {
        let root = required(previous, "agent_dir")?;
        require(
            args.omp_agent_dir.is_none_or(|v| v == root),
            "omp root differs from saved ownership; uninstall/reinstall explicitly",
        )?;
        let capability = required(previous, "peer_reader_capability")?;
        require(
            same_value(previous, &binding(root, no_guidance, capability)?)?,
            "omp saved binding changed; preserve owned targets",
        )?;
        if capability != READER_CAPABILITY {
            require(
                args.omp_peers_upgraded,
                "Stored-source authority upgrade requires renewed --omp-peers-upgraded for omp-stored-source-v2",
            )?;
        }
        // Python-era adoption requires a fresh attestation; Rust re-issues
        // retain the baseline reuse of an already-current reader capability.
        if old.is_some_and(|receipt| {
            matches!(
                text(receipt, "schema"),
                Some("agent-memory/install-receipt-v1" | "agent-memory/install-receipt-v2")
            )
        }) {
            require(
                args.omp_peers_upgraded,
                "Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded after quiescing/upgrading bank peers",
            )?;
        }
        return binding(root, no_guidance, READER_CAPABILITY).map(Some);
    }
    require(
        args.omp_agent_dir.is_some() && args.omp_peers_upgraded,
        "Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded after quiescing/upgrading bank peers",
    )?;
    let root = args.omp_agent_dir.ok_or_else(|| {
        ContractError::new("Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded after quiescing/upgrading bank peers")
    })?;
    binding(root, no_guidance, READER_CAPABILITY).map(Some)
}

fn targets(receipt: &Value) -> Result<&[Value]> {
    get(receipt, "targets")
        .as_array()
        .ok_or_else(|| ContractError::new("Malformed native ownership targets").into())
}
fn target<'a>(receipt: &'a Value, path: &str) -> Result<&'a Value> {
    targets(receipt)?
        .iter()
        .find(|v| text(v, "path") == Some(path))
        .ok_or_else(|| ContractError::new("Native target is absent from ownership receipt").into())
}
pub fn validate_receipt(receipt: &Value) -> Result<()> {
    let schema = text(receipt, "schema");
    require(
        matches!(
            schema,
            Some(
                "agent-memory/install-receipt-v1"
                    | "agent-memory/install-receipt-v2"
                    | "agent-memory/install-receipt-v3"
            )
        ),
        "Unknown installation receipt schema",
    )?;
    // Rust-era receipt-v3 records the compiled runtime target of its source.
    let rust_era = schema == Some("agent-memory/install-receipt-v3");
    require(
        !rust_era || text(get(receipt, "source"), "target").is_some(),
        "Rust-era installation receipt lacks its runtime target",
    )?;
    let value = get(receipt, "omp_binding");
    let installed = *value != Value::Null;
    require(
        text(receipt, "presets")
            .unwrap_or("")
            .split(',')
            .any(|v| v == "omp")
            == installed,
        "omp preset and receipt binding differ",
    )?;
    require(
        get(receipt, "enabled_adapters")
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some("omp")))
            == installed,
        "Native enabled adapter differs from ownership binding",
    )?;
    if !installed {
        return require(
            rust_era || schema == Some("agent-memory/install-receipt-v1"),
            "Non-omp installs retain receipt-v1",
        );
    }
    require(
        (rust_era || schema == Some("agent-memory/install-receipt-v2"))
            && value.as_object().is_some_and(|v| {
                v.len() == FIELDS.len() && FIELDS.iter().all(|k| value.get(k).is_some())
            }),
        "Malformed native ownership receipt",
    )?;
    require(
        get(value, "rollout_attested") == &Value::Bool(true)
            && text(receipt, "installation_id").is_some_and(|v| hexadecimal(v, 32)),
        "Invalid native rollout/installation identity",
    )?;
    require(
        same_value(
            value,
            &binding(
                required(value, "agent_dir")?,
                get(receipt, "no_guidance") == &Value::Bool(true),
                required(value, "peer_reader_capability")?,
            )?,
        )?,
        "Native ownership targets differ from canonical saved binding",
    )?;
    for field in ["skill_target", "extension_target", "activation_target"] {
        if let Some(path) = text(value, field) {
            target(receipt, path)?;
        }
    }
    let digest = get(get(receipt, "source"), "artifact_digest");
    let extension = target(receipt, required(value, "extension_target")?)?;
    require(
        text(extension, "kind") == Some("wrapper") && get(extension, "artifact_digest") == digest,
        "Native extension artifact identity differs from owned runtime",
    )?;
    let skill = target(receipt, required(value, "skill_target")?)?;
    require(
        matches!(text(skill, "kind"), Some("copy" | "link"))
            && get(skill, "artifact_digest") == digest,
        "Native skill target differs from installed artifact",
    )?;
    if let Some(path) = text(value, "activation_target") {
        require(
            text(target(receipt, path)?, "kind") == Some("activation"),
            "Native activation target differs from owned Markdown block",
        )?;
    }
    Ok(())
}

pub fn extension_wrapper(
    active_pack: &Path,
    binary: &Path,
    artifact_digest: &str,
    value: &Value,
) -> Result<Vec<u8>> {
    require(
        canonical_absolute(binary),
        "Native extension executable must be an absolute canonical path",
    )?;
    let source = json::encode(
        &string(path_text(&active_pack.join("hooks/omp.ts"))?),
        json::DEFAULT,
    )?;
    let options = json::encode(
        &object([
            ("artifact", string(artifact_digest)),
            ("executable", string(path_text(binary)?)),
            ("agentDir", string(required(value, "agent_dir")?)),
            (
                "readerCapability",
                string(required(value, "peer_reader_capability")?),
            ),
            ("rolloutAttested", Value::Bool(true)),
        ]),
        json::COMPACT_ASCII,
    )?;
    Ok(format!("// Receipt-owned native executable. Never edit provider or approval settings.\nimport nativeMemory from {source};\nexport default pi => nativeMemory(pi, {options});\n").into_bytes())
}
pub fn preview(value: &Value) -> Value {
    object([
        ("binding", value.clone()),
        (
            "executable_authority",
            string("stored native source observation and bounded advisory wakes only"),
        ),
        (
            "canonical_operations",
            string("ordinary approved native tools; no callback mutation or permission bypass"),
        ),
        (
            "private_retention",
            string(
                "native stored user/tool plaintext after shared screening; immutable evidence retained on uninstall; no automatic purge or global quota",
            ),
        ),
        (
            "compatibility_limit",
            string(
                "unresolved source gaps/registration faults can require omp-aware readers indefinitely; old/manual/remote binaries outside local inventory are not fenced",
            ),
        ),
        (
            "qualification",
            string(
                "disk ownership is not native loading, source completion or platform/provider acceptance",
            ),
        ),
    ])
}

/// One frozen T2e -> T2f interface, registered once by trusted CLI wiring.
/// `locations` pins this running executable's pack and owned binary, not caller input.
#[derive(Debug, Clone)]
pub struct RuntimeLocations {
    pub state: PathBuf,
    pub pack: PathBuf,
    pub binary: PathBuf,
}
/// Implemented by turn_sources; borrowing must recheck the held native barrier.
pub trait HeldSource {
    fn recheck(&self) -> Result<()>;
}
#[derive(Debug)]
pub struct StreamDiagnostics {
    pub instances: usize,
    pub pending: Value,
    pub wake_decisions: Value,
}
#[derive(Debug)]
pub struct NativeDiagnostics {
    pub pending: Option<Value>,
    pub streams: Option<StreamDiagnostics>,
    pub stored_source_verified: bool,
}
pub trait OmpActivation: Send + Sync {
    fn locations(&self) -> Result<RuntimeLocations>;
    /// Full receipt ownership checks, including wrapper bytes and activation blocks.
    fn verify_targets(&self, targets: &[&Value], source: &Value) -> Result<()>;
    fn activation_template(&self, pack: &Path) -> Result<String>;
    fn activation(&self, raw: &[u8], expected: &str, template: &str) -> Result<Vec<u8>>;
    /// Read-only locks around pending/stream inventory, then existing-only source
    /// barrier + marker verification. Wake failures use the unavailable summary;
    /// marker refusal yields false, never enrollment or promotion.
    fn diagnostics(&self, repo: &Path) -> Result<NativeDiagnostics>;
    /// Acquire/borrow source barrier, observe marker, run callback, and freshly
    /// recheck marker/bank binding at exit even when callback refuses. No writes.
    fn observe_marker(
        &self,
        repo: &Path,
        held: Option<&dyn HeldSource>,
        operation: &mut dyn FnMut(&Value) -> Result<()>,
    ) -> Result<()>;
    /// Canonical projection instruction URI, never a normalized alias.
    fn instruction_uri(&self, relative: &str) -> Option<String>;
}
static ACTIVATION: RwLock<Option<Arc<dyn OmpActivation>>> = RwLock::new(None);
pub fn register_activation(provider: Option<Arc<dyn OmpActivation>>) -> Result<()> {
    *ACTIVATION
        .write()
        .map_err(|_| ContractError::new("OMP activation registry is poisoned"))? = provider;
    Ok(())
}
fn activation_provider() -> Result<Arc<dyn OmpActivation>> {
    ACTIVATION
        .read()
        .map_err(|_| ContractError::new("OMP activation registry is poisoned"))?
        .clone()
        .ok_or(Error::MissingActivation)
}

fn answer_first_qualification(provider: &dyn OmpActivation, receipt: &Value) -> Value {
    let selected = get(receipt, "omp_binding");
    let mut state = "unverified";
    if get(receipt, "no_guidance") == &Value::Bool(true)
        || get(selected, "activation_target") == &Value::Null
    {
        state = "no_guidance";
    } else {
        let verify = || -> Result<bool> {
            let path = required(selected, "activation_target")?;
            let target = target(receipt, path)?;
            let template = provider
                .activation_template(Path::new(required(get(receipt, "source"), "pack")?))?;
            let expected = sha256(template.as_bytes());
            let path = Path::new(path);
            let raw = fs::read_source(
                path.parent()
                    .ok_or_else(|| ContractError::new("Malformed activation path"))?,
                Path::new(
                    path.file_name()
                        .ok_or_else(|| ContractError::new("Malformed activation path"))?,
                ),
                MAX_ACTIVATION,
            )
            .map_err(|e| ContractError::new(e.to_string()))?
            .bytes;
            Ok(text(target, "kind") == Some("activation")
                && text(target, "block_hash") == Some(expected.as_str())
                && provider.activation(&raw, &expected, &template)? == raw)
        };
        if verify().unwrap_or(false) {
            state = "receipt-owned";
        }
    }
    object([
        ("activation", string(state)),
        ("automation", string("unqualified")),
        ("native_loading", string("not-observed")),
    ])
}
pub fn diagnostics(repo: &Path, receipt: Option<&Value>) -> Result<Value> {
    let Some(receipt) = receipt.filter(|v| get(v, "omp_binding") != &Value::Null) else {
        return Ok(object([
            ("binding", string("not-installed")),
            ("native_loading", string("not-observed")),
            ("source_assessment", string("not-inferred")),
            ("publication", string("not-inferred")),
        ]));
    };
    let provider = activation_provider()?;
    let selected = get(receipt, "omp_binding");
    let owned: Vec<_> = targets(receipt)?
        .iter()
        .filter(|t| {
            ["skill_target", "extension_target", "activation_target"]
                .iter()
                .any(|f| get(selected, f) == get(t, "path"))
        })
        .collect();
    provider.verify_targets(&owned, get(receipt, "source"))?;
    let mut result = vec![
        ("binding".into(), selected.clone()),
        ("owned_disk_integrity".into(), string("verified")),
        ("native_loading".into(), string("not-live-verified")),
        (
            "supported_version_targets".into(),
            Value::Array(vec![string(SUPPORTED_VERSION)]),
        ),
        (
            "source_assessment".into(),
            string("not-inferred-from-loading"),
        ),
        ("refresh".into(), string("not-inferred-from-assessment")),
        (
            "publication".into(),
            string("requires-witnessed-shared-publication"),
        ),
        (
            "retention".into(),
            string("private-native-snapshots-preserved-on-uninstall"),
        ),
        (
            "answer_first".into(),
            answer_first_qualification(provider.as_ref(), receipt),
        ),
    ];
    let native = provider.diagnostics(repo)?;
    result.push((
        "observer_evidence".into(),
        string(if native.pending.is_some() {
            "retained"
        } else {
            "not-observed"
        }),
    ));
    if let Some(pending) = native.pending {
        result.push(("pending".into(), pending));
    }
    if let Some(streams) = native.streams {
        result.push((
            "transport_qualification".into(),
            object([
                ("instances", Value::Int(streams.instances.into())),
                ("pending", streams.pending),
                ("assessment_authority", Value::Bool(false)),
                ("automatic_wake_support", string("unqualified")),
            ]),
        ));
        result.push(("wake_decisions".into(), streams.wake_decisions));
    }
    result.push((
        "stored_source_activation".into(),
        string(if native.stored_source_verified {
            "verified"
        } else {
            "required-or-unverified"
        }),
    ));
    Ok(Value::Object(result))
}

fn receipt_encoding(receipt: &Value) -> Result<Vec<u8>> {
    Ok((json::encode(receipt, json::SORTED_INDENT2_ASCII)? + "\n").into_bytes())
}
fn transport_digest(value: &Value) -> Result<String> {
    Ok(sha256(
        json::encode(value, json::SORTED_COMPACT_UTF8)?.as_bytes(),
    ))
}
fn read_receipt(state: &Path) -> Result<Option<Value>> {
    fs::trust::state(state)?;
    let path = state.join("receipt.json");
    if !fs::trust::record(&path)?.present {
        return Ok(None);
    }
    let raw = fs::read_source(state, Path::new("receipt.json"), MAX_RECEIPT).map_err(|_| {
        ContractError::new(format!(
            "Missing, linked or oversized installation record: {}",
            path.display()
        ))
    })?;
    let value = json::decode_bytes(
        &raw.bytes,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Accept,
            max_bytes: MAX_RECEIPT,
            ..DecodePolicy::default()
        },
    )?;
    require(
        value.as_object().is_some()
            && matches!(
                text(&value, "schema"),
                Some(
                    "agent-memory/install-receipt-v1"
                        | "agent-memory/install-receipt-v2"
                        | "agent-memory/install-receipt-v3"
                )
            )
            && get(&value, "verified") == &Value::Bool(true)
            && text(&value, "installation_id").is_some()
            && get(&value, "targets").as_array().is_some()
            && get(&value, "source").as_object().is_some(),
        &format!("Invalid installation ownership receipt: {}", path.display()),
    )?;
    if get(&value, "uninstalled") == &Value::Bool(true) {
        return Ok(None);
    }
    for item in targets(&value)? {
        // The installation receipt schema owns the target-kind set (v3 adds
        // command links and legacy sources); never a second, stale list.
        require(
            item.as_object().is_some()
                && text(item, "kind")
                    .is_some_and(|kind| crate::installation::TARGET_KINDS.contains(&kind)),
            "Malformed target ownership receipt",
        )?;
        let normalized = if let Some(path) = text(item, "path").map(Path::new) {
            require(
                path.is_absolute(),
                &format!("Installation paths must be absolute: {}", path.display()),
            )?;
            let parent = fs::canonical_path(path.parent().unwrap_or(path), false)?;
            path.components()
                .next_back()
                .is_some_and(|name| parent.join(name.as_os_str()) == path)
        } else {
            false
        };
        require(
            normalized && item.get("identity").is_some(),
            "Malformed target ownership receipt",
        )?;
    }
    validate_receipt(&value)?;
    Ok(Some(value))
}
fn observer_binding_with(
    provider: &dyn OmpActivation,
    artifact_digest: &str,
    agent_dir: &str,
) -> Result<Value> {
    let locations = provider.locations()?;
    let receipt = read_receipt(&locations.state)?.ok_or_else(|| {
        ContractError::new("Native observer is not the current explicit owned binding")
    })?;
    require(
        get(&receipt, "omp_binding") != &Value::Null
            && text(get(&receipt, "source"), "artifact_digest") == Some(artifact_digest)
            && text(get(&receipt, "omp_binding"), "agent_dir") == Some(agent_dir)
            && fs::canonical_path(Path::new(required(get(&receipt, "source"), "pack")?), false)?
                == locations.pack,
        "Native observer is not the current explicit owned binding",
    )?;
    let owned = target(
        &receipt,
        required(get(&receipt, "omp_binding"), "extension_target")?,
    )?;
    provider.verify_targets(std::slice::from_ref(&owned), get(&receipt, "source"))?;
    let pack = Path::new(required(get(&receipt, "source"), "pack")?);
    let inspected = crate::artifact::inspect_runtime(pack, &locations.binary)
        .map_err(|e| ContractError::new(e.to_string()))?;
    require(
        text(&inspected, "artifact_digest") == Some(artifact_digest),
        "Native observer executable closure differs from the installed artifact",
    )?;
    // Runtime closure is the complete portable inventory plus bin/am. The shared
    // artifact verifier reads every entry; no Python module list survives.
    require(
        same_value(get(&receipt, "artifact_files"), get(&inspected, "files"))?,
        "Native observer executable differs from installed artifact",
    )?;
    Ok(receipt)
}
pub fn observer_binding(artifact_digest: &str, agent_dir: &str) -> Result<Value> {
    observer_binding_with(activation_provider()?.as_ref(), artifact_digest, agent_dir)
}

/// Unforgeable, immutable, non-serializable in-process attestation. No Clone.
pub struct ObserverBinding {
    receipt_sha: String,
    receipt_identity: Fingerprint,
    receipt_path: PathBuf,
    agent_dir: String,
    artifact_digest: String,
    pack: String,
    provider: Arc<dyn OmpActivation>,
}
pub fn verified_observer_binding(
    artifact_digest: &str,
    agent_dir: &str,
    receipt: Option<&Value>,
) -> Result<ObserverBinding> {
    let provider = activation_provider()?;
    let current = observer_binding_with(provider.as_ref(), artifact_digest, agent_dir)?;
    if let Some(receipt) = receipt {
        require(
            receipt_encoding(&current)? == receipt_encoding(receipt)?,
            "Native observer installation changed; start a new instance",
        )?;
    }
    let locations = provider.locations()?;
    let path = locations.state.join("receipt.json");
    let identity = fs::fingerprint(&path)?
        .ok_or_else(|| ContractError::new("Observer ownership receipt is unavailable"))?;
    require(
        matches!(identity, Fingerprint::File { .. }),
        "Observer ownership receipt is unavailable",
    )?;
    require(
        read_receipt(&locations.state)?.is_some_and(|v| same_value(&v, &current).unwrap_or(false))
            && fs::fingerprint(&path)? == Some(identity.clone()),
        "Native observer receipt changed during verification",
    )?;
    Ok(ObserverBinding {
        receipt_sha: sha256(&receipt_encoding(&current)?),
        receipt_identity: identity,
        receipt_path: path,
        agent_dir: agent_dir.into(),
        artifact_digest: artifact_digest.into(),
        pack: required(get(&current, "source"), "pack")?.into(),
        provider,
    })
}
pub fn observe_observer_binding<T>(
    repo: &Path,
    receipt: &Value,
    proof: &ObserverBinding,
    held_source: Option<&dyn HeldSource>,
    operation: impl FnOnce(&Value) -> Result<T>,
) -> Result<T> {
    require(
        receipt.as_object().is_some()
            && get(receipt, "omp_binding").as_object().is_some()
            && sha256(&receipt_encoding(receipt)?) == proof.receipt_sha
            && text(get(receipt, "omp_binding"), "agent_dir") == Some(proof.agent_dir.as_str())
            && text(get(receipt, "source"), "artifact_digest")
                == Some(proof.artifact_digest.as_str())
            && text(get(receipt, "source"), "pack") == Some(proof.pack.as_str()),
        "Native observer proof differs from its original receipt",
    )?;
    if let Some(held) = held_source {
        held.recheck()?;
    }
    let mut operation = Some(operation);
    let mut output = None;
    proof.provider.observe_marker(repo, held_source, &mut |context| {
        let locations = proof.provider.locations()?;
        require(locations.state.join("receipt.json") == proof.receipt_path, "Native observer ownership location changed")?;
        fs::trust::state(&locations.state)?;
        fs::trust::record(&proof.receipt_path)?;
        require(fs::fingerprint(&proof.receipt_path)? == Some(proof.receipt_identity.clone()), "Native observer installation changed; preserve pending work and start a new instance")?;
        let callback = operation.take().ok_or_else(|| ContractError::new("Native observer activation invoked its operation more than once"))?;
        output = Some(callback(context)?);
        Ok(())
    })?;
    output.ok_or_else(|| {
        ContractError::new("Native observer activation did not invoke its operation").into()
    })
}
pub fn verify_observer_binding(
    repo: &Path,
    receipt: &Value,
    proof: &ObserverBinding,
) -> Result<Value> {
    observe_observer_binding(repo, receipt, proof, None, |context| Ok(context.clone()))
}

#[derive(Debug, PartialEq)]
pub struct ManualComparison {
    pub text: String,
    pub header_path: String,
    pub installation_id: String,
    pub observed_receipt_sha256: String,
    pub artifact_digest: String,
    pub manual_file_sha256: String,
}
/// Fresh evidence is privately minted, and every use revalidates ownership.
pub struct OwnedManual {
    receipt_sha: String,
    artifact: String,
    agent_dir: String,
    relative: String,
    comparison: ManualComparison,
    provider: Arc<dyn OmpActivation>,
}
impl OwnedManual {
    pub fn verify(&self) -> Result<Option<ManualComparison>> {
        let current = manual_comparison(
            self.provider.as_ref(),
            &self.receipt_sha,
            &self.artifact,
            &self.agent_dir,
            &self.relative,
        )?;
        Ok(current.filter(|v| *v == self.comparison))
    }
    pub fn revalidate(&self) -> Result<bool> {
        Ok(self.verify()?.is_some())
    }
    pub fn relative_path(&self) -> &str {
        &self.relative
    }
    pub fn text(&self) -> &str {
        &self.comparison.text
    }
    pub fn header_path(&self) -> &str {
        &self.comparison.header_path
    }
    pub fn installation_id(&self) -> &str {
        &self.comparison.installation_id
    }
    pub fn observed_receipt_sha256(&self) -> &str {
        &self.comparison.observed_receipt_sha256
    }
    pub fn artifact_digest(&self) -> &str {
        &self.comparison.artifact_digest
    }
    pub fn manual_file_sha256(&self) -> &str {
        &self.comparison.manual_file_sha256
    }
}

/// Pins directory identities and their trust policy, not sibling timestamps.
struct TrustedParents {
    chain: DirectoryChain,
    pins: Vec<Identity>,
    path: PathBuf,
    protected: bool,
}
impl TrustedParents {
    fn open(path: &Path) -> Result<Self> {
        require(
            canonical_absolute(path),
            "Native manual path must be absolute without parent traversal",
        )?;
        let chain = DirectoryChain::open(path)?;
        let mut protected = false;
        let mut node = PathBuf::from("/");
        let mut pins = Vec::new();
        let mut names = path.components().filter_map(|c| {
            if let Component::Normal(n) = c {
                Some(n)
            } else {
                None
            }
        });
        for (i, descriptor) in chain.descriptors().iter().enumerate() {
            if i > 0 {
                node.push(names.next().ok_or_else(|| {
                    ContractError::new("Native manual parent chain differs from its path")
                })?);
            }
            let identity = descriptor.stat()?;
            protected = fs::trust::inspect_entry(&node, &identity, protected, false, false)?.0;
            pins.push(identity);
        }
        Ok(Self {
            chain,
            pins,
            path: path.into(),
            protected,
        })
    }
    fn recheck(&self) -> Result<()> {
        self.chain.recheck()?;
        for (fd, pin) in self.chain.descriptors().iter().zip(&self.pins) {
            require(
                fd.stat()?.same_binding(*pin),
                "Native manual parent changed during observation",
            )?;
        }
        // Reopening checks the anchor and each named parent against held pins.
        let current = DirectoryChain::open(&self.path)?;
        for (fd, pin) in current.descriptors().iter().zip(&self.pins) {
            require(
                fd.stat()?.same_binding(*pin),
                "Native manual parent changed during observation",
            )?;
        }
        Ok(())
    }
    fn directory(&self) -> &Descriptor {
        self.chain.directory()
    }
}
fn manual_comparison_inner(
    provider: &dyn OmpActivation,
    receipt_sha: &str,
    artifact_digest: &str,
    agent_dir: &str,
    relative: &str,
) -> Result<ManualComparison> {
    require(
        provider.instruction_uri(relative).is_some(),
        "Native manual is not a canonical instruction file",
    )?;
    let receipt = observer_binding_with(provider, artifact_digest, agent_dir)?;
    require(
        transport_digest(&receipt)? == receipt_sha,
        "Native manual receipt changed",
    )?;
    let skill_path = required(get(&receipt, "omp_binding"), "skill_target")?;
    let mut matching = targets(&receipt)?
        .iter()
        .filter(|t| text(t, "path") == Some(skill_path));
    let target = matching
        .next()
        .ok_or_else(|| ContractError::new("Native manual requires one measured owned target"))?;
    require(
        matching.next().is_none() && matches!(text(target, "kind"), Some("copy" | "link")),
        "Native manual requires one measured owned target",
    )?;
    provider.verify_targets(std::slice::from_ref(&target), get(&receipt, "source"))?;
    let selected = get(get(&receipt, "artifact_files"), relative);
    require(
        selected.as_object().is_some_and(|v| v.len() == 2)
            && text(selected, "sha256").is_some_and(|v| hexadecimal(v, 64))
            && text(selected, "mode").is_some(),
        "Malformed native manual inventory entry",
    )?;
    let expected = required(selected, "sha256")?;
    let logical = Path::new(skill_path).join(relative);
    let mut link_pin = None;
    let physical = if text(target, "kind") == Some("link") {
        let link = Path::new(skill_path);
        let destination = Path::new(required(get(&receipt, "source"), "pack")?);
        require(
            canonical_absolute(link)
                && canonical_absolute(destination)
                && same_value(
                    get(target, "identity"),
                    &object([
                        ("kind", string("link")),
                        ("target", string(path_text(destination)?)),
                    ]),
                )?,
            "Native manual link differs from owned target",
        )?;
        let parent = TrustedParents::open(
            link.parent()
                .ok_or_else(|| ContractError::new("Malformed native manual link"))?,
        )?;
        let name = link
            .file_name()
            .ok_or_else(|| ContractError::new("Malformed native manual link"))?
            .to_owned();
        let info = parent.directory().stat_at(&name)?;
        fs::trust::inspect_entry(link, &info, parent.protected, true, true)?;
        require(
            info.is_symlink() && parent.directory().read_link_at(&name)? == destination,
            "Native manual link differs from owned target",
        )?;
        link_pin = Some((parent, name, info, destination.to_path_buf()));
        destination.join(relative)
    } else {
        logical.clone()
    };
    let parents = TrustedParents::open(
        physical
            .parent()
            .ok_or_else(|| ContractError::new("Malformed native manual path"))?,
    )?;
    let name = physical
        .file_name()
        .ok_or_else(|| ContractError::new("Malformed native manual path"))?;
    let mut fd = parents
        .directory()
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
    let before = fd.stat()?;
    fs::trust::inspect_entry(&physical, &before, parents.protected, true, false)?;
    require(
        before.is_regular()
            && before.nlink == 1
            && format!("0o{:o}", before.mode & 0o7777) == required(selected, "mode")?
            && before.size >= 0
            && before.size as u128 <= MAX_MANUAL as u128,
        "Native manual file identity differs from artifact",
    )?;
    let raw = fd
        .read_prefix(MAX_MANUAL + 1)
        .map_err(|e| ContractError::new(e.to_string()))?;
    require(
        raw.len() <= MAX_MANUAL
            && raw.len() as u128 == before.size as u128
            && sha256(&raw) == expected,
        "Native manual bytes differ from artifact",
    )?;
    let text =
        String::from_utf8(raw).map_err(|_| ContractError::new("Native manual is not UTF-8"))?;
    provider.verify_targets(std::slice::from_ref(&target), get(&receipt, "source"))?;
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.manual.before-final-receipt")?;
    require(
        transport_digest(&observer_binding_with(
            provider,
            artifact_digest,
            agent_dir,
        )?)? == receipt_sha,
        "Native manual receipt changed",
    )?;
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.manual.after-final-receipt")?;
    parents.recheck()?;
    require(
        fd.stat()? == before && parents.directory().stat_at(name)? == before,
        "Native manual changed during observation",
    )?;
    if let Some((parent, name, info, destination)) = link_pin {
        parent.recheck()?;
        require(
            parent.directory().stat_at(&name)? == info
                && parent.directory().read_link_at(&name)? == destination,
            "Native manual link changed during observation",
        )?;
    }
    Ok(ManualComparison {
        text,
        header_path: path_text(&logical)?.into(),
        installation_id: required(&receipt, "installation_id")?.into(),
        observed_receipt_sha256: receipt_sha.into(),
        artifact_digest: artifact_digest.into(),
        manual_file_sha256: expected.into(),
    })
}
fn manual_comparison(
    provider: &dyn OmpActivation,
    receipt_sha: &str,
    artifact_digest: &str,
    agent_dir: &str,
    relative: &str,
) -> Result<Option<ManualComparison>> {
    match manual_comparison_inner(provider, receipt_sha, artifact_digest, agent_dir, relative) {
        Ok(value) => Ok(Some(value)),
        Err(Error::MissingActivation) => Err(Error::MissingActivation),
        Err(Error::Contract(_)) => Ok(None),
    }
}
pub fn owned_manual(receipt: &Value, relative: &str) -> Result<Option<OwnedManual>> {
    let provider = activation_provider()?;
    let mint = || -> Result<Option<OwnedManual>> {
        let receipt_sha = transport_digest(receipt)?;
        let artifact = required(get(receipt, "source"), "artifact_digest")?;
        let agent_dir = required(get(receipt, "omp_binding"), "agent_dir")?;
        let Some(comparison) = manual_comparison(
            provider.as_ref(),
            &receipt_sha,
            artifact,
            agent_dir,
            relative,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(OwnedManual {
            receipt_sha,
            artifact: artifact.into(),
            agent_dir: agent_dir.into(),
            relative: relative.into(),
            comparison,
            provider: Arc::clone(&provider),
        }))
    };
    match mint() {
        Err(Error::Contract(_)) => Ok(None),
        result => result,
    }
}

#[cfg(test)]
mod receipt_reader_tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    struct State(PathBuf);

    impl State {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("am-omp-receipt-{}", crate::host::uuid4().unwrap()));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(fs::canonical_path(&path, true).unwrap())
        }

        fn write(&self, value: &Value) {
            // Receipts are 0600 lifecycle records; never depend on the umask.
            let path = self.0.join("receipt.json");
            std::fs::write(&path, receipt_encoding(value).unwrap()).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    impl Drop for State {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn receipt() -> Value {
        object([
            ("schema", string("agent-memory/install-receipt-v1")),
            ("verified", Value::Bool(true)),
            ("installation_id", string("baseline-string")),
            ("targets", Value::Array(vec![])),
            ("source", object([] as [(&str, Value); 0])),
        ])
    }

    #[test]
    fn baseline_uninstall_receipt_stops_before_native_validation_or_hashing() {
        let state = State::new();
        let mut value = receipt();
        *value.get_mut("schema").unwrap() = string("agent-memory/install-receipt-v2");
        // Baseline uninstall retains the OMP binding but removes owned files.
        // Absent runtime bytes and targets prove this returns before validation.
        if let Value::Object(fields) = &mut value {
            fields.extend([
                (
                    "omp_binding".to_owned(),
                    binding(state.0.to_str().unwrap(), false, READER_CAPABILITY).unwrap(),
                ),
                ("uninstalled".to_owned(), Value::Bool(true)),
                (
                    "artifact_files".to_owned(),
                    object([] as [(&str, Value); 0]),
                ),
            ]);
        }
        state.write(&value);
        assert_eq!(read_receipt(&state.0).unwrap(), None);
        assert!(!state.0.join("bin/am").exists());
    }

    #[test]
    fn receipt_reader_enforces_one_mib_and_verified_before_native_validation() {
        let state = State::new();
        let mut value = receipt();
        *value.get_mut("verified").unwrap() = Value::Bool(false);
        state.write(&value);
        assert_eq!(
            read_receipt(&state.0).unwrap_err().to_string(),
            format!(
                "Invalid installation ownership receipt: {}",
                state.0.join("receipt.json").display()
            )
        );
        let mut raw = receipt_encoding(&receipt()).unwrap();
        raw.resize(1536 * 1024, b' ');
        std::fs::write(state.0.join("receipt.json"), &raw).unwrap();
        assert_eq!(
            read_receipt(&state.0).unwrap_err().to_string(),
            format!(
                "Missing, linked or oversized installation record: {}",
                state.0.join("receipt.json").display()
            )
        );
        raw.truncate(MAX_RECEIPT);
        std::fs::write(state.0.join("receipt.json"), raw).unwrap();
        assert_eq!(
            receipt_encoding(&read_receipt(&state.0).unwrap().unwrap()).unwrap(),
            receipt_encoding(&receipt()).unwrap()
        );
    }

    #[test]
    fn receipt_reader_checks_top_level_shape_and_each_target() {
        let state = State::new();
        let base = receipt();
        for (key, invalid) in [
            ("schema", string("unknown")),
            ("installation_id", Value::Null),
            ("targets", Value::Null),
            ("source", Value::Array(vec![])),
        ] {
            let mut value = base.clone();
            *value.get_mut(key).unwrap() = invalid;
            state.write(&value);
            assert_eq!(
                read_receipt(&state.0).unwrap_err().to_string(),
                format!(
                    "Invalid installation ownership receipt: {}",
                    state.0.join("receipt.json").display()
                )
            );
        }
        for kind in [
            "copy",
            "link",
            "wrapper",
            "activation",
            "source",
            "hook-config",
            "hook-link",
        ] {
            let mut value = base.clone();
            *value.get_mut("targets").unwrap() = Value::Array(vec![object([
                ("kind", string(kind)),
                ("path", string(state.0.join("owned").to_str().unwrap())),
                ("identity", Value::Null),
            ])]);
            state.write(&value);
            assert_eq!(
                receipt_encoding(&read_receipt(&state.0).unwrap().unwrap()).unwrap(),
                receipt_encoding(&value).unwrap()
            );
        }
        for target in [
            Value::Null,
            object([
                ("kind", string("unknown")),
                ("path", string("/owned")),
                ("identity", Value::Null),
            ]),
            object([("kind", string("copy")), ("path", string("/owned"))]),
        ] {
            let mut value = base.clone();
            *value.get_mut("targets").unwrap() = Value::Array(vec![target]);
            state.write(&value);
            assert_eq!(
                read_receipt(&state.0).unwrap_err().to_string(),
                "Malformed target ownership receipt"
            );
        }
        let mut relative = base.clone();
        *relative.get_mut("targets").unwrap() = Value::Array(vec![object([
            ("kind", string("copy")),
            ("path", string("relative")),
            ("identity", Value::Null),
        ])]);
        state.write(&relative);
        assert_eq!(
            read_receipt(&state.0).unwrap_err().to_string(),
            "Installation paths must be absolute: relative"
        );
    }

    #[test]
    fn duplicate_observer_and_answer_first_targets_select_the_first_entry() {
        let first = object([("path", string("/owned")), ("kind", string("wrapper"))]);
        let last = object([("path", string("/owned")), ("kind", string("copy"))]);
        let value = object([("targets", Value::Array(vec![first.clone(), last]))]);
        assert_eq!(target(&value, "/owned").unwrap(), &first);
    }
}
