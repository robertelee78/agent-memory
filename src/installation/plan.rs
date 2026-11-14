//! Planning: configuration, presets, preflight, source selection and the
//! complete per-target action list (baseline `configuration`, `presets`,
//! `preflight_path`, `preflight_layout`, `lifecycle_paths`, `source_info`,
//! `prepare`, `public_plan`).
use super::adoption::{python_era_kind, require_hook, require_wrapper};
use super::filesystem::{
    canonical, fingerprint, is_directory, is_regular, lexists, named, read_optional, relative_to,
    same,
};
use super::records::{read_journal, read_receipt};
use super::seams::Services;
use super::transaction::{verify_hook_target, verify_targets};
use super::{
    ADOPTION_GUIDANCE, ATOMICITY, BEGIN, DEFAULT_REMOTE, DEVELOPMENT_BINARY, END, Environment,
    Locations, MANAGED_GUIDANCE, MAX_GUIDANCE, MAX_SHIM, MEMORY_DATA_LOCATION, PLAN_SCHEMA,
    RECEIPT_SCHEMA, RELEASE_BINARY, RELEASE_GUIDANCE, RELEASE_PACK, Result, SHIM, TARGET, absolute,
    path_text, path_value, require, require_layout, sha,
};
use crate::cache_recovery::{self, Qualification};
use crate::client_hooks::{self, EVENT};
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::fs;
use crate::lifecycle::{get, object, string, text};
use crate::omp_installation;
use crate::release_distribution::{self as release, Scratch};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Link,
    Copy,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Copy => "copy",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "link" => Ok(Self::Link),
            "copy" => Ok(Self::Copy),
            _ => Err(ContractError::new("Installation mode must be link or copy").into()),
        }
    }
}

/// The baseline `install` argparse namespace (plus `--bootstrap-binary`).
#[derive(Debug, Clone, Default)]
pub struct InstallRequest {
    pub check: bool,
    pub development_source: Option<PathBuf>,
    pub remote: Option<String>,
    pub reference: Option<String>,
    pub mode: Option<Mode>,
    pub presets: Option<String>,
    pub omp_agent_dir: Option<String>,
    pub omp_peers_upgraded: bool,
    pub omp_layout_quiescence: Vec<PathBuf>,
    pub layout_quiescence: Vec<PathBuf>,
    pub target_dir: Option<Vec<PathBuf>>,
    pub bin_dir: Option<PathBuf>,
    pub no_guidance: bool,
    /// The verified bootstrap binary's own path (install.sh passes it).
    pub bootstrap_binary: Option<PathBuf>,
}

/// One planned owned target.
#[derive(Debug, Clone)]
pub struct Action {
    pub path: PathBuf,
    pub kind: String,
    pub before: Value,
    pub after: Value,
    pub raw: Option<Vec<u8>>,
    pub source_path: Option<PathBuf>,
    pub mode_bits: u32,
    pub changed: bool,
    pub block_hash: Option<String>,
    pub hook_entry: Option<Value>,
    pub hook_original_shape: Option<Value>,
    pub stop_entry: Option<Value>,
    pub stop_original_shape: Option<Value>,
    /// Python-era source record kept with an inert adopted legacy tree.
    pub legacy_source: Option<Value>,
}

/// Fields only `uninstallation` plans carry.
#[derive(Debug, Clone)]
pub struct UninstallFields {
    pub preserved: Value,
    pub reinstall: Value,
    pub preserved_private_state_reader: Option<Value>,
    pub omp_transition: bool,
    pub omp_disabled_backup: Option<Value>,
}

/// Baseline `prepare` result.
pub struct Plan {
    pub operation: String,
    pub state: PathBuf,
    pub source: Value,
    pub mode: String,
    pub presets: String,
    pub custom_targets: Vec<String>,
    pub no_guidance: bool,
    pub bin_dir: String,
    pub adapters: Vec<String>,
    pub actions: Vec<Action>,
    pub restart: Vec<String>,
    pub omp_binding: Option<Value>,
    pub omp_peers_upgraded: bool,
    pub omp_layout_quiescence: Vec<PathBuf>,
    pub layout_quiescence: Vec<PathBuf>,
    pub old: Option<Value>,
    /// The candidate pack and binary whose bytes are installed.
    pub pack_source: PathBuf,
    pub binary_source: PathBuf,
    pub cache_recovery: Option<Qualification>,
    pub rollback_repositories: Vec<String>,
    pub private_state_evidence: Option<Value>,
    /// Python-era adoption evidence (prior schema and receipt digest).
    pub adoption: Option<Value>,
    pub uninstall: Option<UninstallFields>,
}

fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().map(string).collect())
}
fn paths(values: &[PathBuf]) -> Result<Value> {
    Ok(Value::Array(
        values
            .iter()
            .map(|p| path_value(p))
            .collect::<Result<_>>()?,
    ))
}
fn optional(value: &Option<Value>) -> Value {
    value.clone().unwrap_or(Value::Null)
}

impl Action {
    pub fn value(&self) -> Result<Value> {
        let mut fields = vec![
            ("path".to_owned(), path_value(&self.path)?),
            ("kind".to_owned(), string(&self.kind)),
            ("before".to_owned(), self.before.clone()),
            ("after".to_owned(), self.after.clone()),
            ("changed".to_owned(), Value::Bool(self.changed)),
        ];
        if let Some(hash) = &self.block_hash {
            fields.push(("block_hash".into(), string(hash)));
        }
        Ok(Value::Object(fields))
    }
}

impl Plan {
    /// The plan as the dictionary the baseline passed to its lazy callees.
    pub fn value(&self) -> Result<Value> {
        let mut fields = vec![
            ("operation".to_owned(), string(&self.operation)),
            ("state".to_owned(), path_value(&self.state)?),
            ("source".to_owned(), self.source.clone()),
            ("mode".to_owned(), string(&self.mode)),
            ("presets".to_owned(), string(&self.presets)),
            ("custom_targets".to_owned(), strings(&self.custom_targets)),
            ("no_guidance".to_owned(), Value::Bool(self.no_guidance)),
            ("bin_dir".to_owned(), string(&self.bin_dir)),
            ("adapters".to_owned(), strings(&self.adapters)),
            (
                "actions".to_owned(),
                Value::Array(
                    self.actions
                        .iter()
                        .map(Action::value)
                        .collect::<Result<_>>()?,
                ),
            ),
            ("restart".to_owned(), strings(&self.restart)),
            ("omp_binding".to_owned(), optional(&self.omp_binding)),
            (
                "omp_peers_upgraded".to_owned(),
                Value::Bool(self.omp_peers_upgraded),
            ),
            (
                "omp_layout_quiescence".to_owned(),
                paths(&self.omp_layout_quiescence)?,
            ),
            (
                "layout_quiescence".to_owned(),
                paths(&self.layout_quiescence)?,
            ),
            ("old".to_owned(), optional(&self.old)),
            ("pack_source".to_owned(), path_value(&self.pack_source)?),
            (
                "rollback_repositories".to_owned(),
                strings(&self.rollback_repositories),
            ),
        ];
        if let Some(adoption) = &self.adoption {
            fields.push(("adoption".into(), adoption.clone()));
        }
        Ok(Value::Object(fields))
    }
}

/// Baseline `unchanged_parent`.
pub(crate) fn unchanged_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ContractError::new("Installation path requires a parent"))?;
    require(
        canonical(parent)? == parent,
        format!(
            "Installation parent changed or became a symlink: {}",
            parent.display()
        ),
    )
}

/// `os.access(parent, W_OK | X_OK)` decided from the owner bits the trust
/// walk already requires (owner is the user or root).
fn writable_searchable(info: fs::Identity) -> bool {
    let bits = if info.uid == crate::host::effective_uid() {
        0o300
    } else {
        0o003
    };
    info.is_directory() && info.mode & bits == bits && info.mode & 0o222 != 0
}

/// Baseline `preflight_path`.
pub fn preflight_path(env: &Environment, path: &Path) -> Result<()> {
    fs::trust::inspect(
        path,
        fs::trust::InspectOptions {
            allow_final_link: true,
            ..Default::default()
        },
    )?;
    let home = canonical(&env.home()?)?;
    require(
        path != home && path != Path::new("/"),
        format!("Unsafe installation target: {}", path.display()),
    )?;
    unchanged_parent(path)?;
    let mut parent = path.parent();
    while let Some(candidate) = parent {
        if lexists(candidate)? {
            break;
        }
        parent = candidate.parent();
    }
    let parent = parent.unwrap_or(Path::new("/"));
    require(
        named(parent)?.is_some_and(writable_searchable),
        format!(
            "Target ancestor is not writable/searchable: {}",
            parent.display()
        ),
    )?;
    for ancestor in path.ancestors() {
        let memory = ancestor
            .file_name()
            .is_some_and(|name| name == "memory-bank")
            || is_regular(&ancestor.join("MEMORY.md"))?;
        require_layout(
            !memory,
            format!(
                "Installer runtime and targets must not overlap a project Memory Bank: {}",
                path.display()
            ),
        )?;
    }
    Ok(())
}

/// Baseline `_activation_template`: the selected binding's candidate block.
pub fn activation_template(pack: &Path, omp: bool) -> Result<String> {
    let path = pack.join("templates").join(if omp {
        "activation-omp-markdown.example.txt"
    } else {
        "activation-markdown.example.txt"
    });
    let bounded =
        || ContractError::new("Candidate activation shim must be one bounded invocation block");
    let shim = if is_regular(&path)? {
        let raw = read_optional(&path, MAX_SHIM).map_err(|_| bounded())?;
        String::from_utf8(raw.unwrap_or_default()).map_err(|_| bounded())?
    } else {
        require(!omp, "Candidate omp activation template is required")?;
        SHIM.to_owned()
    };
    require(
        shim.starts_with(&format!("{BEGIN}\n"))
            && shim.ends_with(END)
            && shim.matches(BEGIN).count() == 1
            && shim.matches(END).count() == 1
            && shim.len() <= MAX_SHIM,
        "Candidate activation shim must be one bounded invocation block",
    )?;
    Ok(shim)
}

/// Baseline `activation`: replace only the recorded block, or append a new one.
pub fn activation(raw: &[u8], expected: Option<&str>, shim: &str) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(raw).map_err(|_| {
        ContractError::new("Activation file is not UTF-8; preserve and reconcile manually")
    })?;
    let begins = text.matches(BEGIN).count();
    require(
        begins == text.matches(END).count() && begins <= 1,
        "Unbalanced/duplicate activation sentinels; preserve and reconcile manually",
    )?;
    if let (Some(start), Some(end)) = (text.find(BEGIN), text.find(END)) {
        let finish = end + END.len();
        let block = text.get(start..finish);
        require(
            start < end
                && expected.is_some()
                && block.is_some_and(|block| Some(sha(block.as_bytes()).as_str()) == expected),
            "Activation block is unrelated or modified; preserve and reconcile manually",
        )?;
        let mut result = text.get(..start).unwrap_or_default().to_owned();
        result.push_str(shim);
        result.push_str(text.get(finish..).unwrap_or_default());
        return Ok(result.into_bytes());
    }
    require(
        expected.is_none(),
        "Owned activation block is missing; preserve and reconcile manually",
    )?;
    let mut result = raw.to_vec();
    if !raw.is_empty() && !raw.ends_with(b"\n") {
        result.push(b'\n');
    }
    result.extend_from_slice(shim.as_bytes());
    result.push(b'\n');
    Ok(result)
}

/// Baseline `presets`: `(name, skill root, guidance file)` per selected client.
pub fn presets(
    env: &Environment,
    names: &str,
    omp_agent_dir: Option<&str>,
) -> Result<Vec<(String, PathBuf, Option<PathBuf>)>> {
    let home = env.home()?;
    let config = env.directory("XDG_CONFIG_HOME", ".config")?;
    let codex = env.directory("CODEX_HOME", ".codex")?;
    let claude = env.directory("CLAUDE_CONFIG_DIR", ".claude")?;
    let mut values: Vec<(&str, PathBuf, Option<PathBuf>)> = vec![
        ("agents", home.join(".agents/skills"), None),
        ("codex", codex.join("skills"), Some(codex.join("AGENTS.md"))),
        (
            "claude",
            claude.join("skills"),
            Some(claude.join("CLAUDE.md")),
        ),
        (
            "opencode",
            config.join("opencode/skills"),
            Some(config.join("opencode/AGENTS.md")),
        ),
    ];
    let names = match names {
        "all" => "agents,codex,claude,opencode",
        "none" => return Ok(Vec::new()),
        other => other,
    };
    if names.split(',').any(|name| name == "omp") {
        let root = omp_agent_dir.ok_or_else(|| {
            ContractError::new(
                "Explicit omp preset requires its saved or selected absolute agent root",
            )
        })?;
        let native = absolute(env, Path::new(root))?;
        values.push(("omp", native.join("skills"), Some(native.join("AGENTS.md"))));
    }
    let keys: Vec<&str> = names.split(',').collect();
    let mut unique = keys.clone();
    unique.sort_unstable();
    unique.dedup();
    let known = keys
        .iter()
        .all(|key| values.iter().any(|(name, _, _)| name == key));
    require(
        !keys.is_empty() && unique.len() == keys.len() && known,
        "Presets must be all, none, or a unique comma-separated agents,codex,claude,opencode,omp list",
    )?;
    Ok(keys
        .iter()
        .filter_map(|key| values.iter().find(|(name, _, _)| name == key))
        .map(|(name, skills, guidance)| ((*name).to_owned(), skills.clone(), guidance.clone()))
        .collect())
}

/// Baseline `hook_paths`: client-owned configuration paths.
pub fn hook_paths(env: &Environment, name: &str) -> Result<Option<PathBuf>> {
    Ok(match name {
        "codex" => Some(env.directory("CODEX_HOME", ".codex")?.join("hooks.json")),
        "claude" => Some(
            env.directory("CLAUDE_CONFIG_DIR", ".claude")?
                .join("settings.json"),
        ),
        "opencode" => Some(
            env.directory("XDG_CONFIG_HOME", ".config")?
                .join("opencode/plugins/agent-memory.js"),
        ),
        _ => None,
    })
}

/// Baseline `client_hook_entries`, retargeted: the owned prompt and Stop
/// groups invoke the owned Rust binary (`<am> hook <name>`), never Python.
pub fn client_hook_entries(name: &str, binary: &Path) -> Result<(Value, Value)> {
    let (prompt, stop) = if name == "codex" {
        ("codex-prompt", "codex-stop")
    } else {
        ("turn-hook", "claude-stop")
    };
    let prompt = client_hooks::entry(&client_hooks::rust_era_command(binary, prompt)?)?;
    let mut stop = client_hooks::entry(&client_hooks::rust_era_command(binary, stop)?)?;
    if name == "claude" {
        if let Some(Value::Array(hooks)) = stop.get_mut("hooks") {
            if let Some(Value::Object(fields)) = hooks.first_mut() {
                fields.push(("asyncRewake".into(), Value::Bool(true)));
            }
        }
    }
    Ok((prompt, stop))
}

/// Baseline `configuration`.
#[derive(Debug, Clone)]
pub struct Configuration {
    pub mode: String,
    pub presets: String,
    pub custom_targets: Vec<PathBuf>,
    pub no_guidance: bool,
    pub bin_dir: PathBuf,
    pub omp_binding: Option<Value>,
}
impl Configuration {
    fn omp_agent_dir(&self) -> Option<&str> {
        self.omp_binding
            .as_ref()
            .and_then(|binding| text(binding, "agent_dir"))
    }
}

pub fn configuration(
    env: &Environment,
    request: &InstallRequest,
    old: Option<&Value>,
) -> Result<Configuration> {
    let saved = |key: &str| old.and_then(|old| text(old, key));
    let mode = match request.mode {
        Some(mode) => mode.as_str().to_owned(),
        None => saved("mode").unwrap_or("link").to_owned(),
    };
    let presets = request
        .presets
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| saved("presets").map(str::to_owned))
        .unwrap_or_else(|| "all".to_owned());
    let custom_targets = match &request.target_dir {
        Some(values) => values.clone(),
        None => old
            .map(|old| {
                get(old, "custom_targets")
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default(),
    };
    let no_guidance =
        request.no_guidance || old.is_some_and(|old| get(old, "no_guidance") == &Value::Bool(true));
    let bin_dir = match &request.bin_dir {
        Some(path) => path.clone(),
        None => match saved("bin_dir") {
            Some(path) => PathBuf::from(path),
            None => env.home()?.join(".local/bin"),
        },
    };
    let bin_dir = absolute(env, &bin_dir)?;
    let omp_binding = omp_installation::configuration(
        &omp_installation::Configuration {
            omp_agent_dir: request.omp_agent_dir.as_deref(),
            omp_peers_upgraded: request.omp_peers_upgraded,
            omp_layout_quiescence: &request.omp_layout_quiescence,
        },
        old,
        &presets,
        no_guidance,
    )?;
    Ok(Configuration {
        mode,
        presets,
        custom_targets,
        no_guidance,
        bin_dir,
        omp_binding,
    })
}

/// Lifecycle commands whose write set is checked before any write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Install,
    Update,
    Recover,
}

/// Target paths an install of this configuration would own.
fn install_paths(env: &Environment, cfg: &Configuration) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for name in super::COMMAND_NAMES {
        result.push(cfg.bin_dir.join(name));
    }
    for (name, target, guidance) in presets(env, &cfg.presets, cfg.omp_agent_dir())? {
        result.push(absolute(env, &target.join("agent-memory"))?);
        if let Some(guidance) = guidance.filter(|_| !cfg.no_guidance) {
            result.push(absolute(env, &guidance)?);
        }
        if let Some(hook) = hook_paths(env, &name)?.filter(|_| !cfg.no_guidance) {
            result.push(absolute(env, &hook)?);
        }
        if name == "omp" {
            let binding = cfg.omp_binding.as_ref().unwrap_or(&Value::Null);
            result.push(PathBuf::from(
                text(binding, "extension_target").ok_or_else(|| {
                    ContractError::new("Malformed native ownership field: extension_target")
                })?,
            ));
        }
    }
    for target in &cfg.custom_targets {
        result.push(absolute(env, target)?.join("agent-memory"));
    }
    Ok(result)
}

/// Baseline `preflight_layout`: must precede `locked()`.
pub fn preflight_layout(
    env: &Environment,
    services: &Services,
    command: Command,
    request: &InstallRequest,
    locations: &Locations,
    old: Option<&Value>,
) -> Result<()> {
    let state = &locations.state;
    preflight_path(env, state)?;
    for owned in [&locations.code, &locations.runtime] {
        require_layout(
            !relative_to(state, owned) && !relative_to(owned, state),
            "Installation state and managed code paths must be separate",
        )?;
    }
    let mut sources = Vec::new();
    if let Some(executable) = &services.executable {
        if let Some(parent) = canonical(executable)?.parent() {
            sources.push(parent.to_path_buf());
        }
    }
    if let Some(old) = old {
        for target in get(old, "targets").as_array().unwrap_or(&[]) {
            let owned = Path::new(text(target, "path").unwrap_or_default());
            if matches!(text(target, "kind"), Some("source" | "legacy-source")) {
                fs::trust::source(owned)?;
            } else {
                fs::trust::tree(owned, false)?;
            }
        }
        let saved = text(get(old, "source"), "path").unwrap_or_default();
        sources.push(canonical(&absolute(env, Path::new(saved))?)?);
    }
    let journal_path = state.join("journal.json");
    if lexists(&journal_path)? {
        let journal = super::load(&journal_path)?;
        require(
            journal.as_object().is_some()
                && text(&journal, "schema") == Some(super::JOURNAL_SCHEMA),
            "Unsupported lifecycle journal",
        )?;
        let recorded = get(&journal, "receipt");
        require(
            get(recorded, "source").as_object().is_some(),
            "Lifecycle journal has no source identity",
        )?;
        let declared = text(get(recorded, "source"), "path")
            .ok_or_else(|| ContractError::new("Lifecycle journal has no source identity"))?;
        sources.push(canonical(&absolute(env, Path::new(declared))?)?);
    }
    let mut targets = vec![state.clone()];
    match command {
        Command::Install => {
            let cfg = configuration(env, request, old)?;
            targets.extend(install_paths(env, &cfg)?);
            if let Some(development) = &request.development_source {
                sources.push(fs::canonical_path(&absolute(env, development)?, true)?);
            }
        }
        Command::Update => {
            if let Some(old) = old {
                let bin_dir = text(old, "bin_dir").unwrap_or_default();
                for name in super::COMMAND_NAMES {
                    targets.push(absolute(env, Path::new(bin_dir))?.join(name));
                }
                if get(old, "no_guidance") != &Value::Bool(true) {
                    let agent = text(get(old, "omp_binding"), "agent_dir");
                    for (name, _, _) in presets(env, text(old, "presets").unwrap_or("none"), agent)?
                    {
                        if let Some(hook) = hook_paths(env, &name)? {
                            targets.push(absolute(env, &hook)?);
                        }
                    }
                }
            }
        }
        Command::Recover => {}
    }
    for path in targets.iter().skip(1) {
        preflight_path(env, path)?;
        require_layout(
            !relative_to(path, state) && !relative_to(state, path),
            "Installation target overlaps the external receipt directory",
        )?;
    }
    for path in &targets {
        for source in &sources {
            require_layout(
                !relative_to(path, source) && !relative_to(source, path),
                format!(
                    "Installer must not mutate a source package or development source: {}",
                    path.display()
                ),
            )?;
        }
    }
    Ok(())
}

/// Baseline `lifecycle_paths`: the complete possible write set, validated
/// before test faults are enabled.
pub fn lifecycle_paths(
    env: &Environment,
    command: Command,
    request: &InstallRequest,
    locations: &Locations,
    old: Option<&Value>,
) -> Result<Vec<PathBuf>> {
    let mut result = vec![
        locations.state.clone(),
        locations.code.clone(),
        locations.runtime.clone(),
    ];
    if let Some(old) = old {
        for target in get(old, "targets").as_array().unwrap_or(&[]) {
            result.push(PathBuf::from(text(target, "path").unwrap_or_default()));
        }
    }
    if let Some(journal) = read_journal(env, &locations.state)? {
        for action in get(&journal, "actions").as_array().unwrap_or(&[]) {
            for key in ["path", "stage"] {
                result.push(PathBuf::from(text(action, key).unwrap_or_default()));
            }
        }
    }
    if command == Command::Install {
        let cfg = configuration(env, request, old)?;
        result.extend(install_paths(env, &cfg)?);
    }
    Ok(result)
}

/// A selected candidate source: its receipt record and actual bytes.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub value: Value,
    pub root: PathBuf,
    pub pack: PathBuf,
    pub binary: PathBuf,
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    let raw = crate::managed_source::git(root, args, None, 64 * 1024)?;
    String::from_utf8(raw)
        .map(|value| value.trim().to_owned())
        .map_err(|_| ContractError::new("Source Git output is not UTF-8").into())
}

fn package_fields(package: &Value) -> Result<(Value, Value, Value)> {
    let version = get(package, "version").clone();
    let digest = get(package, "artifact_digest").clone();
    let compatibility = get(package, "compatibility").clone();
    require(
        version.as_str().is_some()
            && digest.as_str().is_some()
            && compatibility.as_object().is_some(),
        "Artifact inspection is incomplete",
    )?;
    Ok((version, digest, compatibility))
}

/// Baseline `source_info`, Rust era. `release` verifies the signed runtime
/// root; `development` needs a prebuilt binary; `managed` reproduces the
/// Python-era record for read-only legacy checkout verification.
pub fn source_info(services: &Services, root: &Path, kind: &str) -> Result<SourceInfo> {
    let root = fs::canonical_path(root, true)?;
    fs::trust::source(&root)?;
    let skills = root.join(RELEASE_PACK);
    let pack = if is_directory(&skills)? {
        skills
    } else {
        root.clone()
    };
    let root_value = path_value(&root)?;
    let pack_value = path_value(&pack)?;
    if kind == "release" {
        let binary = root.join(RELEASE_BINARY);
        let manifest = release::installed_with(&root, TARGET, &services.release.signers)?;
        let raw_manifest = release::regular(&root.join(release::MANIFEST), release::MAX_METADATA)?;
        let package = services.artifacts.inspect_runtime(&pack, &binary)?;
        let identity = manifest
            .targets
            .get(TARGET)
            .ok_or_else(|| ContractError::new("Unsupported release target"))?;
        require(
            text(&package, "artifact_digest") == Some(identity.artifact_digest.as_str()),
            "Signed artifact inspection differs",
        )?;
        let (_, _, compatibility) = package_fields(&package)?;
        let value = object([
            ("kind", string("release")),
            ("path", root_value),
            ("pack", pack_value),
            ("remote", string(DEFAULT_REMOTE)),
            ("ref", string("releases/latest")),
            ("revision", string(&manifest.source_commit)),
            ("dirty", Value::Bool(false)),
            ("version", string(&manifest.version)),
            ("artifact_digest", string(&identity.artifact_digest)),
            ("compatibility", compatibility),
            (
                "release_manifest_sha256",
                string(release::digest(&raw_manifest)),
            ),
            ("signature_namespace", string(release::NAMESPACE)),
            ("signature_principal", string(release::PRINCIPAL)),
            ("target", string(TARGET)),
        ]);
        return Ok(SourceInfo {
            value,
            root,
            pack,
            binary,
        });
    }
    let binary = root.join(DEVELOPMENT_BINARY);
    if kind == "development" {
        require(
            is_regular(&binary)?,
            format!(
                "Development source requires a prebuilt am at {}; build it with Cargo first (the installer never invokes Cargo)",
                binary.display()
            ),
        )?;
    }
    let package = if kind == "development" {
        services.artifacts.inspect_runtime(&pack, &binary)?
    } else {
        services.artifacts.inspect(&pack)?
    };
    let (version, digest, compatibility) = package_fields(&package)?;
    let mut revision = "unversioned".to_owned();
    let mut dirty = Value::Null;
    let top = crate::managed_source::git(&root, &["rev-parse", "--show-toplevel"], None, 64 * 1024)
        .ok()
        .and_then(|raw| String::from_utf8(raw).ok());
    // An arbitrary copied package inside an unrelated Git parent is unversioned.
    if let Some(top) = top {
        if canonical(Path::new(top.trim()))? == root {
            revision = git_text(&root, &["rev-parse", "HEAD"])?;
            if kind == "managed" {
                crate::managed_source::verify_payload(&root, &revision)?;
                dirty = Value::Bool(false);
            } else {
                let status = git_text(&root, &["status", "--porcelain", "--untracked-files=all"])?;
                dirty = Value::Bool(!status.is_empty());
            }
        }
    }
    require(
        kind != "managed" || (revision != "unversioned" && dirty != Value::Bool(true)),
        "Managed source must be a clean versioned checkout",
    )?;
    let mut fields = vec![
        ("kind", string(kind)),
        ("path", root_value),
        ("pack", pack_value),
        ("remote", Value::Null),
        ("ref", Value::Null),
        ("revision", string(revision)),
        ("dirty", dirty),
        ("version", version),
        ("artifact_digest", digest),
        ("compatibility", compatibility),
    ];
    if kind == "development" {
        fields.push(("target", string(TARGET)));
    }
    Ok(SourceInfo {
        value: object(fields),
        root,
        pack,
        binary,
    })
}

/// Stage the signed release runtime root from the verified bootstrap binary:
/// signed manifest and archive, then the same bootstrap bytes as `bin/am`.
pub fn stage_release(services: &Services, bootstrap_binary: &Path) -> Result<(Scratch, PathBuf)> {
    let access = &services.release;
    // An absent or non-ordinary bootstrap binary refuses before any download.
    release::regular(bootstrap_binary, release::MAX_BINARY).map_err(|_| {
        ContractError::new(
            "Bootstrap binary is unavailable or not an ordinary file; re-run the trusted bootstrap",
        )
    })?;
    let scratch = Scratch::new("am-preverified-install-")?;
    let root = scratch.path.join("runtime");
    let observed = release::probe_with(
        &access.first_manifest_url,
        &access.assets,
        access.options,
        &access.signers,
    )?;
    let raw = release::bind_bootstrap_binary(
        &observed.manifest,
        bootstrap_binary,
        TARGET,
        &access.version,
    )?;
    release::acquire_with(&root, &observed, access.options)?;
    let directory = fs::Descriptor::open_directory(&root)?;
    directory.create_directory(std::ffi::OsStr::new("bin"), 0o755)?;
    release::write_new(&root.join(RELEASE_BINARY), &raw, 0o755)?;
    Ok((scratch, root))
}

/// Inputs beyond the request that select a candidate (update/cache paths).
#[derive(Default)]
pub struct PrepareOptions<'a> {
    pub candidate_root: Option<&'a Path>,
    pub candidate_kind: Option<&'a str>,
    pub repair_missing_hooks: bool,
    pub cache_recovery: Option<Qualification>,
    /// A release root already staged from the verified bootstrap binary.
    pub bootstrap_root: Option<&'a Path>,
}

struct Builder<'a> {
    env: &'a Environment,
    services: &'a Services,
    prior: BTreeMap<String, Value>,
    old: Option<&'a Value>,
    legacy: bool,
    repair_missing_hooks: bool,
    shim: String,
    omp_binding: Option<Value>,
    pack_source: PathBuf,
    active_pack: PathBuf,
    saved: Option<Value>,
    runtime: PathBuf,
    cache_recovery: Option<Qualification>,
    actions: Vec<Action>,
}

#[derive(Default)]
struct Spec {
    raw: Option<Vec<u8>>,
    source_path: Option<PathBuf>,
    mode_bits: Option<u32>,
    block: bool,
    client: Option<String>,
    hook_entry: Option<Value>,
    stop_entry: Option<Value>,
    /// The Python-era source record kept with an inert legacy tree.
    legacy_source: Option<Value>,
}

fn shape(document: &Value, event: &str) -> Value {
    object([
        (
            "hooks_present",
            Value::Bool(document.get("hooks").is_some()),
        ),
        (
            "event_present",
            Value::Bool(get(document, "hooks").get(event).is_some()),
        ),
    ])
}

fn identity_mode(identity: &Value) -> Option<u32> {
    get(identity, "mode")
        .as_int()
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
}

impl Builder<'_> {
    fn add(&mut self, path: &Path, kind: &str, spec: Spec) -> Result<()> {
        let path = absolute(self.env, path)?;
        preflight_path(self.env, &path)?;
        let before = fingerprint(&path)?;
        let key = path_text(&path)?.to_owned();
        let previous = self.prior.get(&key).cloned();
        let mut raw = spec.raw;
        let mut mode_bits = spec.mode_bits.unwrap_or(0o644);
        let mut block_hash = None;
        let mut hook_shapes = (None, None);
        if spec.block {
            require(
                before == Value::Null || text(&before, "kind") == Some("file"),
                format!("Activation file collision: {}", path.display()),
            )?;
            let omp_target = self
                .omp_binding
                .as_ref()
                .and_then(|binding| text(binding, "activation_target"));
            let template = if omp_target == Some(key.as_str()) {
                activation_template(&self.pack_source, true)?
            } else {
                self.shim.clone()
            };
            let expected = previous.as_ref().and_then(|previous| {
                (text(previous, "kind") == Some("activation")
                    && !(self.repair_missing_hooks && before == Value::Null))
                    .then(|| text(previous, "block_hash"))
                    .flatten()
            });
            let current = if before == Value::Null {
                Vec::new()
            } else {
                read_optional(&path, MAX_GUIDANCE)?.unwrap_or_default()
            };
            raw = Some(activation(&current, expected, &template)?);
            mode_bits = identity_mode(&before).unwrap_or(0o644);
            block_hash = Some(sha(template.as_bytes()));
        } else if kind == "hook-config" {
            require(
                before == Value::Null || text(&before, "kind") == Some("file"),
                format!("Client hook configuration collision: {}", path.display()),
            )?;
            require(
                previous
                    .as_ref()
                    .is_none_or(|previous| text(previous, "kind") == Some(kind)),
                format!("Client hook ownership changed: {}", path.display()),
            )?;
            let missing_owned =
                self.repair_missing_hooks && previous.is_some() && before == Value::Null;
            let (original_raw, document) = match &previous {
                Some(previous) if !missing_owned => {
                    if self.legacy {
                        if let (Some(client), Some(old)) = (&spec.client, self.old) {
                            require_hook(previous, client, &path, old)?;
                        }
                    }
                    let checked = verify_hook_target(previous)?;
                    require(
                        same(&checked.identity, &before),
                        "Client hook configuration changed during planning",
                    )?;
                    (checked.raw, checked.document)
                }
                _ => {
                    if let Some(previous) = &previous {
                        let prompt = get(previous, "hook_entry");
                        let probe = object([(
                            "hooks",
                            object([(EVENT, Value::Array(vec![prompt.clone()]))]),
                        )]);
                        client_hooks::locate(&probe, prompt, EVENT)?;
                        if let Some(stop) = previous.get("stop_entry") {
                            let probe = object([(
                                "hooks",
                                object([("Stop", Value::Array(vec![stop.clone()]))]),
                            )]);
                            client_hooks::locate(&probe, stop, "Stop")?;
                        }
                    }
                    let original = if before == Value::Null {
                        Vec::new()
                    } else {
                        read_optional(&path, client_hooks::MAX_CONFIG)?.unwrap_or_default()
                    };
                    let document = client_hooks::parse(&original)?;
                    (original, document)
                }
            };
            let owned_prompt = previous
                .as_ref()
                .filter(|_| !missing_owned)
                .map(|previous| get(previous, "hook_entry"));
            let original_shape = previous
                .as_ref()
                .map(|previous| {
                    previous
                        .get("hook_original_shape")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Vec::new()))
                })
                .unwrap_or_else(|| shape(&document, EVENT));
            let hook_entry = spec
                .hook_entry
                .clone()
                .ok_or_else(|| ContractError::new("Owned hook entry is required"))?;
            let mut merged = client_hooks::merge(&original_raw, owned_prompt, &hook_entry, EVENT)?;
            let stop_shape = previous
                .as_ref()
                .filter(|previous| previous.get("stop_entry").is_some())
                .map(|previous| {
                    previous
                        .get("stop_original_shape")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Vec::new()))
                })
                .unwrap_or_else(|| shape(&document, "Stop"));
            if let Some(stop_entry) = &spec.stop_entry {
                let owned_stop = previous
                    .as_ref()
                    .filter(|_| !missing_owned)
                    .and_then(|previous| previous.get("stop_entry"));
                merged = client_hooks::merge(&merged, owned_stop, stop_entry, "Stop")?;
            }
            raw = Some(merged);
            mode_bits = identity_mode(&before).unwrap_or(0o644);
            hook_shapes = (Some(original_shape), Some(stop_shape));
        } else if before != Value::Null {
            let recorded = previous.as_ref().map(|previous| get(previous, "identity"));
            let drifted = !recorded.is_some_and(|recorded| same(recorded, &before));
            if drifted && kind == "source" && previous.is_some() {
                match &self.cache_recovery {
                    Some(cache) if path == self.runtime => {
                        cache_recovery::recheck(cache)?;
                        require(
                            same(&cache.source_identity, &before),
                            "Qualified signed source changed before planning",
                        )?;
                    }
                    _ => {
                        let previous = previous.clone().unwrap_or(Value::Null);
                        verify_targets(self.services, &[&previous], self.saved.as_ref(), false)?;
                    }
                }
                require(
                    same(&fingerprint(&path)?, &before),
                    "Managed source changed during ownership verification",
                )?;
            } else if drifted && kind == "legacy-source" && previous.is_some() {
                let legacy = spec.legacy_source.clone().unwrap_or(Value::Null);
                require(
                    text(&legacy, "kind") == Some("managed")
                        && text(&before, "kind") == Some("directory")
                        && recorded.is_some_and(|recorded| {
                            text(recorded, "kind") == Some("directory")
                                && get(recorded, "mode") == get(&before, "mode")
                        }),
                    format!(
                        "Unrelated or user-modified target; preserve: {}",
                        path.display()
                    ),
                )?;
                crate::managed_source::verify(&path, &legacy)?;
                require(
                    same(&fingerprint(&path)?, &before),
                    "Managed source changed during ownership verification",
                )?;
            } else {
                require(
                    previous.is_some() && !drifted,
                    format!(
                        "Unrelated or user-modified target; preserve: {}",
                        path.display()
                    ),
                )?;
            }
            if self.legacy {
                if let (Some(previous), Some(old)) = (&previous, self.old) {
                    if text(previous, "kind") == Some("wrapper") {
                        require_wrapper(previous, &path, old)?;
                    }
                }
            }
        }
        let after = match kind {
            "hook-link" | "command-link" => object([
                ("kind", string("link")),
                (
                    "target",
                    path_value(spec.source_path.as_deref().unwrap_or(Path::new("")))?,
                ),
            ]),
            "link" => object([
                ("kind", string("link")),
                ("target", path_value(&self.active_pack)?),
            ]),
            "copy" | "source" => {
                let source = spec
                    .source_path
                    .as_deref()
                    .ok_or_else(|| ContractError::new("Payload source is required"))?;
                let after = fingerprint(source)?;
                // An unchanged, already-trusted copy needs no permission churn.
                let same_payload = text(&before, "kind") == Some("directory")
                    && get(&before, "hash") == get(&after, "hash");
                let mode = if same_payload {
                    get(&before, "mode").clone()
                } else {
                    Value::Int(0o700u32.into())
                };
                object([
                    ("kind", get(&after, "kind").clone()),
                    ("hash", get(&after, "hash").clone()),
                    ("mode", mode),
                ])
            }
            "legacy-source" => before.clone(),
            _ => object([
                ("kind", string("file")),
                ("hash", string(sha(raw.as_deref().unwrap_or_default()))),
                ("mode", Value::Int(mode_bits.into())),
            ]),
        };
        let changed = !same(&before, &after);
        let legacy_source = spec.legacy_source.filter(|_| kind == "legacy-source");
        self.actions.push(Action {
            path,
            kind: kind.to_owned(),
            before,
            after,
            raw,
            source_path: spec.source_path,
            mode_bits,
            changed,
            block_hash,
            hook_original_shape: spec.hook_entry.as_ref().and(hook_shapes.0),
            stop_original_shape: spec.stop_entry.as_ref().and(hook_shapes.1),
            hook_entry: spec.hook_entry,
            stop_entry: spec.stop_entry,
            legacy_source,
        });
        Ok(())
    }
}

/// The Python-era managed/release source tree an adoption leaves inert, with
/// its Python source record: from a v1/v2 receipt, or kept by a v3 receipt.
fn legacy_source_tree(old: &Value, legacy: bool) -> Option<(PathBuf, Value)> {
    if legacy {
        let source = get(old, "source");
        return matches!(text(source, "kind"), Some("managed" | "release"))
            .then(|| text(source, "path").map(|path| (PathBuf::from(path), source.clone())))
            .flatten();
    }
    get(old, "targets")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .find(|target| text(target, "kind") == Some("legacy-source"))
        .and_then(|target| {
            text(target, "path")
                .map(|path| (PathBuf::from(path), get(target, "legacy_source").clone()))
        })
}

/// Baseline `prepare`: select the candidate and plan every owned target.
pub fn prepare(
    env: &Environment,
    services: &Services,
    request: &InstallRequest,
    locations: &Locations,
    old: Option<&Value>,
    options: PrepareOptions<'_>,
) -> Result<Plan> {
    let state = &locations.state;
    let saved = old.map(|old| get(old, "source").clone());
    let saved_kind = saved.as_ref().and_then(|saved| text(saved, "kind"));
    let legacy = old.is_some_and(python_era_kind);
    let mut cache = options.cache_recovery;
    if let Some(cache) = &cache {
        require(
            old.is_some_and(|old| same(old, &cache.receipt))
                && read_receipt(env, state)?.is_some_and(|current| same(&current, &cache.receipt)),
            "Installation receipt changed before cache recovery planning",
        )?;
        cache_recovery::recheck(cache)?;
    }
    let source_options = request.remote.is_some() || request.reference.is_some();
    let same_channel = saved.as_ref().is_some_and(|saved| {
        request
            .remote
            .as_deref()
            .is_none_or(|remote| text(saved, "remote") == Some(remote))
            && request
                .reference
                .as_deref()
                .is_none_or(|reference| text(saved, "ref") == Some(reference))
    });
    let info = if let Some(candidate) = options.candidate_root {
        require(
            !legacy && saved_kind == Some("release"),
            "Updates require managed ownership",
        )?;
        let root = fs::canonical_path(candidate, true)?;
        source_info(services, &root, options.candidate_kind.unwrap_or("release"))?
    } else if let Some(development) = &request.development_source {
        require(
            !source_options,
            "Choose development source or managed remote/ref, not both",
        )?;
        let root = fs::canonical_path(&absolute(env, development)?, true)?;
        source_info(services, &root, "development")?
    } else if !legacy && saved_kind == Some("development") {
        require(
            !source_options,
            "Changing source channel requires a separate installation",
        )?;
        let path = saved
            .as_ref()
            .and_then(|saved| text(saved, "path"))
            .unwrap_or_default();
        source_info(services, Path::new(path), "development")?
    } else {
        if source_options {
            if !legacy && saved_kind == Some("release") {
                return Err(ContractError::new("Git remote/ref options cannot change a signed release installation; preview an explicit uninstall/reinstall instead").into());
            }
            if legacy && saved_kind == Some("managed") {
                require(
                    same_channel,
                    "Managed channel differs from ownership receipt",
                )?;
            } else {
                return Err(ContractError::new(MANAGED_GUIDANCE).into());
            }
        }
        if let (Some(saved), false) = (&saved, legacy) {
            let updating = services.updating()?;
            match &cache {
                None => cache = updating.qualified_current(state, &locations.runtime)?,
                Some(qualified) => updating.current(state, &locations.runtime, Some(qualified))?,
            }
            require(text(saved, "kind") == Some("release"), MANAGED_GUIDANCE)?;
        } else if old.is_none() {
            require(
                !lexists(&locations.runtime)?,
                format!(
                    "Unrelated runtime collision: {}",
                    locations.runtime.display()
                ),
            )?;
        }
        let root = match (options.bootstrap_root, &saved) {
            (Some(root), _) => root.to_path_buf(),
            (None, Some(saved)) if !legacy && cache.is_none() => {
                PathBuf::from(text(saved, "path").unwrap_or_default())
            }
            _ if legacy => return Err(ContractError::new(ADOPTION_GUIDANCE).into()),
            _ => return Err(ContractError::new(RELEASE_GUIDANCE).into()),
        };
        let info = source_info(services, &root, "release")?;
        if let (Some(saved), false) = (&saved, legacy) {
            let updating = services.updating()?;
            updating.release_transition(saved, &info.value, &root)?;
            updating.compatible_surface(saved, &info.value, cache.as_ref())?;
        }
        info
    };
    let readers = get(
        get(&info.value, "compatibility"),
        "installation_receipt_schemas",
    );
    require(
        readers.as_array().is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_str() == Some(RECEIPT_SCHEMA))
        }),
        "Candidate cannot read the Rust installation receipt-v3",
    )?;
    let root = info.root.clone();
    require(
        !relative_to(state, &root) && !relative_to(&root, state),
        "Installation state must not overlap the selected source",
    )?;
    let pack_source = info.pack.clone();
    let shim = activation_template(&pack_source, false)?;
    let development = text(&info.value, "kind") == Some("development");
    let active_root = if development {
        root.clone()
    } else {
        locations.runtime.clone()
    };
    let relative = pack_source
        .strip_prefix(&root)
        .map_err(|_| ContractError::new("Candidate pack is outside its source"))?;
    let active_pack = active_root.join(relative);
    let active_binary = if development {
        info.binary.clone()
    } else {
        locations.runtime_binary()
    };
    let mut source = info.value.clone();
    if let Value::Object(fields) = &mut source {
        for (key, value) in fields.iter_mut() {
            if key == "path" {
                *value = path_value(&active_root)?;
            } else if key == "pack" {
                *value = path_value(&active_pack)?;
            }
        }
    }
    let cfg = configuration(env, request, old)?;
    let prior: BTreeMap<String, Value> = old
        .map(|old| {
            get(old, "targets")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .filter_map(|target| {
                    text(target, "path").map(|path| (path.to_owned(), target.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut builder = Builder {
        env,
        services,
        prior: prior.clone(),
        old,
        legacy,
        repair_missing_hooks: options.repair_missing_hooks,
        shim,
        omp_binding: cfg.omp_binding.clone(),
        pack_source: pack_source.clone(),
        active_pack: active_pack.clone(),
        saved: saved.clone(),
        runtime: locations.runtime.clone(),
        cache_recovery: cache.clone(),
        actions: Vec::new(),
    };
    let artifact_digest = text(&source, "artifact_digest")
        .unwrap_or_default()
        .to_owned();
    if !development {
        builder.add(
            &locations.runtime,
            "source",
            Spec {
                source_path: Some(root.clone()),
                ..Spec::default()
            },
        )?;
    }
    if let Some((path, record)) = old.and_then(|old| legacy_source_tree(old, legacy)) {
        builder.add(
            &path,
            "legacy-source",
            Spec {
                legacy_source: Some(record),
                ..Spec::default()
            },
        )?;
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut adapters = Vec::new();
    let mode = Mode::parse(&cfg.mode)?;
    for (name, target, guidance) in presets(env, &cfg.presets, cfg.omp_agent_dir())? {
        let skill = absolute(env, &target.join("agent-memory"))?;
        let mut group = vec![path_text(&skill)?.to_owned()];
        adapters.push(name.clone());
        builder.add(
            &skill,
            mode.as_str(),
            Spec {
                source_path: Some(pack_source.clone()),
                ..Spec::default()
            },
        )?;
        if let Some(guidance) = guidance.filter(|_| !cfg.no_guidance) {
            group.push(path_text(&absolute(env, &guidance)?)?.to_owned());
            builder.add(
                &guidance,
                "activation",
                Spec {
                    block: true,
                    ..Spec::default()
                },
            )?;
        }
        let hook = hook_paths(env, &name)?;
        match (name.as_str(), hook) {
            ("codex" | "claude", Some(hook)) if !cfg.no_guidance => {
                let (prompt, stop) = client_hook_entries(&name, &active_binary)?;
                builder.add(
                    &hook,
                    "hook-config",
                    Spec {
                        client: Some(name.clone()),
                        hook_entry: Some(prompt),
                        stop_entry: Some(stop),
                        ..Spec::default()
                    },
                )?;
                group.push(path_text(&absolute(env, &hook)?)?.to_owned());
            }
            ("opencode", Some(hook)) if !cfg.no_guidance => {
                builder.add(
                    &hook,
                    "hook-link",
                    Spec {
                        source_path: Some(active_pack.join("hooks/opencode.js")),
                        ..Spec::default()
                    },
                )?;
                group.push(path_text(&absolute(env, &hook)?)?.to_owned());
            }
            ("omp", _) => {
                let binding = cfg.omp_binding.clone().unwrap_or(Value::Null);
                let extension = text(&binding, "extension_target").ok_or_else(|| {
                    ContractError::new("Malformed native ownership field: extension_target")
                })?;
                let raw = omp_installation::extension_wrapper(
                    &active_pack,
                    &active_binary,
                    &artifact_digest,
                    &binding,
                )?;
                builder.add(
                    Path::new(extension),
                    "wrapper",
                    Spec {
                        raw: Some(raw),
                        ..Spec::default()
                    },
                )?;
                group.push(extension.to_owned());
            }
            _ => {}
        }
        groups.insert(name, group);
    }
    let mut custom = Vec::new();
    for target in &cfg.custom_targets {
        let target = absolute(env, target)?;
        let skill = target.join("agent-memory");
        builder.add(
            &skill,
            mode.as_str(),
            Spec {
                source_path: Some(pack_source.clone()),
                ..Spec::default()
            },
        )?;
        let adapter = format!("custom:{}", path_text(&target)?);
        adapters.push(adapter.clone());
        groups.insert(adapter, vec![path_text(&skill)?.to_owned()]);
        custom.push(path_text(&target)?.to_owned());
    }
    for name in crate::artifact::command_names(get(&source, "compatibility"))? {
        builder.add(
            &cfg.bin_dir.join(&name),
            "command-link",
            Spec {
                source_path: Some(active_binary.clone()),
                ..Spec::default()
            },
        )?;
    }
    let actions = builder.actions;
    for (index, action) in actions.iter().enumerate() {
        let path = &action.path;
        require(
            !relative_to(path, state) && !relative_to(state, path),
            "Installation target overlaps the external receipt directory",
        )?;
        for other in actions.iter().skip(index + 1) {
            require(
                !relative_to(path, &other.path) && !relative_to(&other.path, path),
                format!(
                    "Duplicate or overlapping installation targets: {} / {}",
                    path.display(),
                    other.path.display()
                ),
            )?;
        }
        if development {
            require(
                !relative_to(path, &root) && !relative_to(&root, path),
                "Installer must not mutate a development source",
            )?;
        }
    }
    let planned: Vec<String> = actions
        .iter()
        .map(|action| path_text(&action.path).map(str::to_owned))
        .collect::<Result<_>>()?;
    require(
        prior.keys().all(|path| planned.contains(path)),
        "Changing enabled targets would strand ownership; uninstall first",
    )?;
    let extension = cfg
        .omp_binding
        .as_ref()
        .and_then(|binding| text(binding, "extension_target"))
        .map(str::to_owned);
    let loaded_change = |action: &Action| -> bool {
        let key = action.path.to_str().unwrap_or_default();
        let previous = prior.get(key);
        let digest_changed = previous.is_none_or(|previous| {
            text(previous, "artifact_digest") != Some(artifact_digest.as_str())
        });
        match action.kind.as_str() {
            "copy" | "link" => digest_changed,
            "activation" => previous.is_none_or(|previous| {
                text(previous, "block_hash") != action.block_hash.as_deref()
            }),
            "hook-config" | "hook-link" => action.changed || digest_changed,
            _ if extension.as_deref() == Some(key) => action.changed || digest_changed,
            _ => false,
        }
    };
    let restart = adapters
        .iter()
        .filter(|adapter| {
            groups.get(*adapter).is_some_and(|group| {
                actions.iter().any(|action| {
                    action
                        .path
                        .to_str()
                        .is_some_and(|path| group.iter().any(|owned| owned == path))
                        && loaded_change(action)
                })
            })
        })
        .cloned()
        .collect();
    let adoption = match (legacy, old) {
        (true, Some(old)) => Some(object([
            ("from_schema", get(old, "schema").clone()),
            ("prior_receipt_sha256", string(sha(&super::encoded(old)?))),
            ("rollback_to_python_after_commit", Value::Bool(false)),
        ])),
        _ => None,
    };
    Ok(Plan {
        operation: "install".to_owned(),
        state: state.clone(),
        source,
        mode: cfg.mode.clone(),
        presets: cfg.presets.clone(),
        custom_targets: custom,
        no_guidance: cfg.no_guidance,
        bin_dir: path_text(&cfg.bin_dir)?.to_owned(),
        adapters,
        actions,
        restart,
        omp_binding: cfg.omp_binding,
        omp_peers_upgraded: request.omp_peers_upgraded,
        omp_layout_quiescence: request.omp_layout_quiescence.clone(),
        layout_quiescence: request.layout_quiescence.clone(),
        old: old.cloned(),
        pack_source,
        binary_source: info.binary,
        cache_recovery: cache,
        rollback_repositories: Vec::new(),
        private_state_evidence: None,
        adoption,
        uninstall: None,
    })
}

/// Baseline `public_plan`.
pub fn public_plan(plan: &Plan) -> Result<Value> {
    let mut fields = vec![
        ("schema".to_owned(), string(PLAN_SCHEMA)),
        ("operation".to_owned(), string("install")),
        ("verified".to_owned(), Value::Bool(false)),
        (
            "installation_writes_performed".to_owned(),
            Value::Bool(false),
        ),
        ("source".to_owned(), plan.source.clone()),
        ("mode".to_owned(), string(&plan.mode)),
        ("enabled_adapters".to_owned(), strings(&plan.adapters)),
        ("data_roots".to_owned(), Value::Array(Vec::new())),
        (
            "memory_data_location".to_owned(),
            string(MEMORY_DATA_LOCATION),
        ),
        (
            "receipt_path".to_owned(),
            path_value(&plan.state.join("receipt.json"))?,
        ),
        (
            "targets".to_owned(),
            Value::Array(
                plan.actions
                    .iter()
                    .map(|action| {
                        Ok(object([
                            ("path", path_value(&action.path)?),
                            ("kind", string(&action.kind)),
                            ("before", action.before.clone()),
                            ("after", action.after.clone()),
                            ("changed", Value::Bool(action.changed)),
                        ]))
                    })
                    .collect::<Result<_>>()?,
            ),
        ),
    ];
    if let Some(cache) = &plan.cache_recovery {
        fields.push(("cache_recovery".into(), cache.summary.clone()));
    }
    if let Some(binding) = &plan.omp_binding {
        fields.push(("omp".into(), omp_installation::preview(binding)));
    }
    if let Some(adoption) = &plan.adoption {
        fields.push(("adoption".into(), adoption.clone()));
    }
    fields.push(("restart_clients".into(), strings(&plan.restart)));
    fields.push(("atomicity".into(), string(ATOMICITY)));
    Ok(Value::Object(fields))
}
