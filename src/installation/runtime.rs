//! Independently verified lifecycle bytes and their transaction-spanning lifetime.
use super::filesystem::{copy_node, fingerprint, same};
use super::{
    DEVELOPMENT_BINARY, Environment, Locations, RELEASE_BINARY, Result, Services, SourceInfo,
    absolute, read_receipt, require, source_info,
};
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::lifecycle::{get, text};
use crate::release_distribution::Scratch;
use std::path::{Path, PathBuf};

/// A pair verified from one source observation. Retained copies outlive every
/// activation/rollback exchange and the private-state guard's final proof.
pub struct LifecycleRuntime {
    pack: PathBuf,
    binary: PathBuf,
    source: Value,
    root: PathBuf,
    // A measured legacy inspection pair, not a fabricated native source record.
    legacy_identity: Option<Value>,
    _scratch: Option<Scratch>,
}
impl LifecycleRuntime {
    pub fn pair(&self) -> (&Path, &Path) {
        (&self.pack, &self.binary)
    }
    pub(crate) fn observation(&self) -> &Value {
        &self.source
    }

    /// A complete, independently selected helper, never a replacement chosen
    /// after a failed verification of an existing runtime.
    pub(crate) fn executing(services: &Services) -> Result<Self> {
        let executable = services
            .executable
            .as_deref()
            .ok_or_else(|| ContractError::new("Removal needs a trusted complete outside helper"))?;
        verified_binary_source(services, executable)
    }

    pub(super) fn from_info(info: SourceInfo) -> Self {
        Self {
            pack: info.pack,
            binary: info.binary,
            source: info.value,
            root: info.root,
            legacy_identity: None,
            _scratch: None,
        }
    }

    pub(crate) fn retain(self, services: &Services) -> Result<Self> {
        // Development sources are external to owned runtime exchanges. Do not
        // snapshot their checkout, Git database or build caches.
        let kind = text(&self.source, "kind").unwrap_or_default();
        let legacy = self.legacy_identity.is_some();
        if kind == "development" && !legacy {
            return Ok(self);
        }
        let scratch = Scratch::new("am-lifecycle-runtime-")?;
        let root = scratch.path.join("runtime");
        let directory = crate::fs::Descriptor::open_directory(&scratch.path)?;
        let runtime = directory.create_directory(std::ffi::OsStr::new("runtime"), 0o700)?;
        let skills = runtime.create_directory(std::ffi::OsStr::new("skills"), 0o700)?;
        let bin = runtime.create_directory(std::ffi::OsStr::new("bin"), 0o700)?;
        drop((skills, bin));
        // Only the verified pack, binary and signature identity metadata are
        // retained. Unrelated runtime-root files can never become authority.
        for (source, target) in [
            (self.pack.clone(), root.join(super::RELEASE_PACK)),
            (self.binary.clone(), root.join(RELEASE_BINARY)),
        ]
        .into_iter()
        .chain(
            [
                crate::release_distribution::MANIFEST,
                crate::release_distribution::SIGNATURE,
            ]
            .into_iter()
            .filter(|_| !legacy)
            .map(|name| (self.root.join(name), root.join(name))),
        ) {
            let before = fingerprint(&source)?;
            copy_node(&source, &target, false)?;
            require(
                same(&fingerprint(&target)?, &before) && same(&fingerprint(&source)?, &before),
                "Lifecycle runtime changed while retaining verified bytes",
            )?;
        }
        if let Some(identity) = self.legacy_identity {
            let pack = root.join(super::RELEASE_PACK);
            let binary = root.join(RELEASE_BINARY);
            require(
                same(
                    &services.artifacts.inspect_runtime(&pack, &binary)?,
                    &identity,
                ),
                "Retained legacy inspection pair differs from verified bytes",
            )?;
            return Ok(Self {
                pack,
                binary,
                source: self.source,
                root,
                legacy_identity: Some(identity),
                _scratch: Some(scratch),
            });
        }
        let info = source_info(services, &root, kind)?;
        require_identity(&info.value, &self.source)?;
        Ok(Self {
            pack: info.pack,
            binary: info.binary,
            source: self.source,
            root,
            legacy_identity: None,
            _scratch: Some(scratch),
        })
    }
}

fn require_identity(observed: &Value, recorded: &Value) -> Result<()> {
    let fields = recorded.as_object().ok_or_else(|| {
        ContractError::new("Lifecycle source identity is missing; preserve the receipt and journal")
    })?;
    require(
        fields.iter().all(|(key, value)| {
            matches!(key.as_str(), "path" | "pack") || same(get(observed, key), value)
        }) && [
            "kind",
            "version",
            "artifact_digest",
            "compatibility",
            "target",
        ]
        .iter()
        .all(|key| {
            get(recorded, key) != &Value::Null && same(get(observed, key), get(recorded, key))
        }),
        "Actual lifecycle runtime differs from its recorded source; preserve recovery evidence",
    )
}

fn verified_source(
    env: &Environment,
    services: &Services,
    source: &Value,
    root: &Path,
    relocated: bool,
) -> Result<LifecycleRuntime> {
    let kind = text(source, "kind")
        .filter(|kind| matches!(*kind, "release" | "development"))
        .ok_or_else(|| {
            ContractError::new("Recorded source cannot supply verified Rust lifecycle authority")
        })?;
    require(
        root.is_absolute() && absolute(env, root)? == root,
        "Lifecycle runtime source path is not canonical",
    )?;
    let info = source_info(services, root, kind)?;
    require_identity(&info.value, source)?;
    if !relocated {
        require(
            get(&info.value, "path") == get(source, "path")
                && get(&info.value, "pack") == get(source, "pack"),
            "Lifecycle source paths differ from the recorded source",
        )?;
    }
    Ok(LifecycleRuntime::from_info(info))
}

/// A Python-era pack is inspection input, never a Rust runtime identity.
/// Its recorded portable digest and owned source bytes must verify; the binary
/// is selected independently from the executing Rust recovery helper.
fn verified_legacy_runtime(
    env: &Environment,
    services: &Services,
    receipt: &Value,
    root: &Path,
) -> Result<LifecycleRuntime> {
    let source = get(receipt, "source");
    let recorded_root = text(source, "path")
        .ok_or_else(|| ContractError::new("Legacy receipt has no source path"))?;
    let recorded_pack = text(source, "pack")
        .ok_or_else(|| ContractError::new("Legacy receipt has no pack path"))?;
    require(
        root.is_absolute() && absolute(env, root)? == root,
        "Legacy lifecycle source path is not canonical",
    )?;
    let relative = Path::new(recorded_pack)
        .strip_prefix(recorded_root)
        .map_err(|_| ContractError::new("Legacy pack is outside its recorded source"))?;
    require(
        matches!(
            text(source, "kind"),
            Some("managed" | "release" | "development")
        ) && !relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir)),
        "Legacy lifecycle source is malformed; preserve recovery evidence",
    )?;
    let pack = root.join(relative);
    require(
        crate::fs::canonical_path(&pack, true)? == pack,
        "Legacy lifecycle pack path is not canonical",
    )?;
    crate::fs::trust::source(root)?;
    for target in get(receipt, "targets").as_array().unwrap_or(&[]) {
        if text(target, "kind") == Some("source") && text(target, "path") == Some(recorded_root) {
            let mut selected = target.clone();
            if let Some(path) = selected.get_mut("path") {
                *path = super::path_value(root)?;
            }
            super::verify_targets(services, &[&selected], Some(source), false)?;
        }
    }
    let package = services.artifacts.inspect(&pack)?;
    let recorded_digest = text(source, "artifact_digest").or_else(|| {
        get(receipt, "targets")
            .as_array()?
            .iter()
            .find(|target| {
                text(target, "kind") == Some("link")
                    && text(get(target, "identity"), "target") == Some(recorded_pack)
            })
            .and_then(|target| text(target, "artifact_digest"))
    });
    require(
        recorded_digest.is_some()
            && text(&package, "artifact_digest") == recorded_digest
            && ["version", "compatibility"].iter().all(|key| {
                source
                    .get(key)
                    .is_none_or(|value| same(value, get(&package, key)))
            }),
        "Legacy portable pack differs from its recorded identity; preserve recovery evidence",
    )?;
    let executable = match services.executable.as_deref() {
        Some(executable) => executable.to_path_buf(),
        None => std::env::current_exe()?,
    };
    let binary = crate::fs::canonical_path(&executable, true)?;
    crate::fs::trust::inspect(&binary, Default::default())?;
    // This measures the actual pair without attributing the executing binary
    // or a native target/capability to the old source record.
    let identity = services.artifacts.inspect_runtime(&pack, &binary)?;
    let portable = get(&package, "files")
        .as_object()
        .ok_or_else(|| ContractError::new("Legacy portable inventory is missing"))?;
    let measured = get(&identity, "files");
    require(
        measured
            .as_object()
            .is_some_and(|files| files.len() == portable.len() + 1)
            && portable
                .iter()
                .all(|(path, entry)| same(get(measured, path), entry)),
        "Legacy portable pack changed while measuring its inspection pair",
    )?;
    Ok(LifecycleRuntime {
        pack,
        binary,
        source: source.clone(),
        root: root.to_path_buf(),
        legacy_identity: Some(identity),
        _scratch: None,
    })
}

/// Inspect the receipt's actual source, never the executing binary or caches.
pub fn verified_receipt_runtime(
    env: &Environment,
    services: &Services,
    receipt: &Value,
) -> Result<LifecycleRuntime> {
    let source = get(receipt, "source");
    let root = text(source, "path")
        .ok_or_else(|| ContractError::new("Receipt has no lifecycle runtime source"))?;
    if super::python_era_kind(receipt) {
        return verified_legacy_runtime(env, services, receipt, Path::new(root));
    }
    verified_source(env, services, source, Path::new(root), false)
}

fn verified_binary_source(services: &Services, binary: &Path) -> Result<LifecycleRuntime> {
    let binary = crate::fs::canonical_path(binary, true)?;
    let (kind, suffix) = if binary.ends_with(DEVELOPMENT_BINARY) {
        ("development", DEVELOPMENT_BINARY)
    } else if binary.ends_with(RELEASE_BINARY) {
        ("release", RELEASE_BINARY)
    } else {
        return Err(ContractError::new("Helper is not a complete Rust runtime source").into());
    };
    let root = binary
        .ancestors()
        .nth(Path::new(suffix).components().count())
        .ok_or_else(|| ContractError::new("Helper binary has no runtime root"))?;
    let info = source_info(services, root, kind)?;
    require(
        crate::fs::canonical_path(&info.binary, true)? == binary,
        "Selected helper binary differs from its verified source",
    )?;
    Ok(LifecycleRuntime::from_info(info))
}

pub(crate) fn verified_plan_runtime(
    env: &Environment,
    services: &Services,
    plan: &super::Plan,
) -> Result<LifecycleRuntime> {
    if plan.operation == "uninstall" {
        let runtime = verified_binary_source(services, &plan.binary_source)?;
        let recorded = text(&plan.source, "path")
            .ok_or_else(|| ContractError::new("Removal source path is missing"))?;
        if fingerprint(Path::new(recorded))? != Value::Null {
            require_identity(runtime.observation(), &plan.source)?;
        }
        require(
            runtime.pair() == plan.runtime(),
            "Removal pair differs from its verified helper",
        )?;
        return Ok(runtime);
    }
    let kind = text(&plan.source, "kind").unwrap_or_default();
    let suffix = if kind == "release" {
        RELEASE_BINARY
    } else {
        DEVELOPMENT_BINARY
    };
    let root = plan
        .binary_source
        .ancestors()
        .nth(Path::new(suffix).components().count())
        .ok_or_else(|| ContractError::new("Plan binary has no runtime root"))?;
    let runtime = verified_source(
        env,
        services,
        &plan.source,
        root,
        text(&plan.source, "path") != root.to_str(),
    )?;
    require(
        runtime.pair() == plan.runtime(),
        "Plan pair differs from its verified source",
    )?;
    Ok(runtime)
}

/// Select from journal phase and exact fingerprints *before* verification.
/// Verification failure never switches to another source or mutates evidence.
pub fn recovery_runtime(
    env: &Environment,
    services: &Services,
    loc: &Locations,
    journal: &Value,
) -> Result<LifecycleRuntime> {
    let committed = super::transaction::receipt_committed(&loc.state, journal)?;
    let status = text(journal, "status").unwrap_or_default();
    if committed && get(get(journal, "receipt"), "uninstalled") != &Value::Bool(true) {
        return verified_receipt_runtime(env, services, get(journal, "receipt"));
    }
    let previous = match journal
        .get("previous_source")
        .filter(|source| **source != Value::Null)
    {
        Some(source) => Some(source.clone()),
        None => read_receipt(env, &loc.state)?.map(|receipt| get(&receipt, "source").clone()),
    };
    let candidate = get(get(journal, "receipt"), "source");
    let actions = get(journal, "actions").as_array().unwrap_or(&[]);
    let select = |source: &Value, previous: bool| -> Result<Option<PathBuf>> {
        let root = text(source, "path").ok_or_else(|| {
            ContractError::new(
                "Journal has no exact lifecycle source; preserve its stages and backups",
            )
        })?;
        let action = actions.iter().find(|action| {
            text(action, "kind") == Some("source") && text(action, "path") == Some(root)
        });
        if let Some(action) = action {
            let expected = get(action, if previous { "before" } else { "after" });
            if *expected == Value::Null {
                return Ok(None);
            }
            for key in ["path", "stage", "backup"] {
                if let Some(path) = text(action, key) {
                    if same(&fingerprint(Path::new(path))?, expected) {
                        return Ok(Some(PathBuf::from(path)));
                    }
                }
            }
            return Ok(None);
        }
        if fingerprint(Path::new(root))? != Value::Null {
            return Ok(Some(PathBuf::from(root)));
        }
        Ok(None)
    };
    if super::python_era_kind(get(journal, "receipt")) {
        let receipt = read_receipt(env, &loc.state)?
            .filter(super::python_era_kind)
            .unwrap_or_else(|| get(journal, "receipt").clone());
        let source = get(&receipt, "source");
        let root = select(source, !committed)?.ok_or_else(|| {
            ContractError::new("No exact legacy source remains; preserve stages and backups")
        })?;
        let runtime = verified_legacy_runtime(env, services, &receipt, &root)?;
        return runtime.retain(services);
    }
    // An opaque rollback or Python-era predecessor preserves old bytes, but
    // cannot authenticate a Rust reader. Keep using the journal's exact,
    // independently verified candidate/acquisition source for inspection.
    let opaque = !matches!(
        get(journal, "opaque_cache_recovery"),
        Value::Null | Value::Bool(false)
    );
    let restoring =
        (status == "rolled-back" && !opaque) || text(journal, "operation") == Some("uninstall");
    let candidate_action = actions.iter().any(|action| {
        text(action, "kind") == Some("source") && text(action, "path") == text(candidate, "path")
    });
    let restoring_native = restoring
        && previous
            .as_ref()
            .is_some_and(|source| text(source, "target").is_some());
    let selected = if restoring_native {
        let source = previous
            .as_ref()
            .ok_or_else(|| ContractError::new("Missing prior source"))?;
        let prior = select(source, true)?;
        if prior.is_none() && text(journal, "operation") == Some("uninstall") {
            // A removal can start with an already-absent runtime. Its verified
            // outside helper was recorded before any owned target changed.
            let helper = get(journal, "runtime_source");
            (helper, select(helper, false)?)
        } else {
            (source, prior)
        }
    } else {
        let selected = if candidate_action {
            select(candidate, false)?
        } else {
            None
        };
        if selected.is_some() {
            (candidate, selected)
        } else {
            // A crash can precede or interrupt the payload copy. The actual
            // acquisition root was recorded before staging; never invent it.
            let acquisition = journal
                .get("runtime_source")
                .map(|source| select(source, false))
                .transpose()?
                .flatten();
            if acquisition.is_some() {
                (get(journal, "runtime_source"), acquisition)
            } else if let Some(source) = previous.as_ref() {
                // Selection uses phase/fingerprints, never verification failure.
                (source, select(source, true)?)
            } else {
                (candidate, select(candidate, false)?)
            }
        }
    };
    let (source, selected) = selected;
    let selected = selected.ok_or_else(|| {
        ContractError::new(
            "No exact journal runtime fingerprint remains; preserve stages and backups",
        )
    })?;
    let root = text(source, "path").ok_or_else(|| ContractError::new("Missing selected source"))?;
    let runtime = verified_source(
        env,
        services,
        source,
        &selected,
        selected != Path::new(root),
    )?;
    let exchanged = actions.iter().any(|action| {
        ["path", "stage"]
            .iter()
            .any(|key| text(action, key).is_some_and(|path| selected.starts_with(Path::new(path))))
    });
    if exchanged && !committed && !matches!(status, "complete" | "rolled-back") {
        runtime.retain(services)
    } else {
        Ok(runtime)
    }
}
