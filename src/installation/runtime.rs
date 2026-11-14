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
    _scratch: Option<Scratch>,
}
impl LifecycleRuntime {
    pub fn pair(&self) -> (&Path, &Path) {
        (&self.pack, &self.binary)
    }

    fn from_info(info: SourceInfo) -> Self {
        Self {
            pack: info.pack,
            binary: info.binary,
            source: info.value,
            root: info.root,
            _scratch: None,
        }
    }

    pub(crate) fn retain(self, services: &Services) -> Result<Self> {
        // Development sources are external to owned runtime exchanges. Do not
        // snapshot their checkout, Git database or build caches.
        let kind = text(&self.source, "kind").unwrap_or_default();
        if kind == "development" {
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
            (
                self.root.join(crate::release_distribution::MANIFEST),
                root.join(crate::release_distribution::MANIFEST),
            ),
            (
                self.root.join(crate::release_distribution::SIGNATURE),
                root.join(crate::release_distribution::SIGNATURE),
            ),
        ] {
            let before = fingerprint(&source)?;
            copy_node(&source, &target, false)?;
            require(
                same(&fingerprint(&target)?, &before) && same(&fingerprint(&source)?, &before),
                "Lifecycle runtime changed while retaining verified bytes",
            )?;
        }
        let info = source_info(services, &root, kind)?;
        require_identity(&info.value, &self.source)?;
        Ok(Self {
            pack: info.pack,
            binary: info.binary,
            source: self.source,
            root,
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

/// Inspect the receipt's actual source, never the executing binary or caches.
pub fn verified_receipt_runtime(
    env: &Environment,
    services: &Services,
    receipt: &Value,
) -> Result<LifecycleRuntime> {
    let source = get(receipt, "source");
    let root = text(source, "path")
        .ok_or_else(|| ContractError::new("Receipt has no lifecycle runtime source"))?;
    verified_source(env, services, source, Path::new(root), false)
}
pub(crate) fn plan_source(plan: &super::Plan) -> Result<Value> {
    let suffix = if text(&plan.source, "kind") == Some("release") {
        RELEASE_BINARY
    } else {
        DEVELOPMENT_BINARY
    };
    let root = plan
        .binary_source
        .ancestors()
        .nth(Path::new(suffix).components().count())
        .ok_or_else(|| ContractError::new("Plan binary has no runtime root"))?;
    let mut source = plan.source.clone();
    if let Value::Object(fields) = &mut source {
        for (key, value) in fields {
            if key == "path" {
                *value = super::path_value(root)?;
            } else if key == "pack" {
                *value = super::path_value(&plan.pack_source)?;
            }
        }
    }
    Ok(source)
}

pub(crate) fn verified_plan_runtime(
    env: &Environment,
    services: &Services,
    plan: &super::Plan,
) -> Result<LifecycleRuntime> {
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
    // An opaque rollback preserved old bytes, but never authenticated them.
    // Its retained signed candidate remains the inspection authority.
    let opaque = !matches!(
        get(journal, "opaque_cache_recovery"),
        Value::Null | Value::Bool(false)
    );
    let restoring =
        (status == "rolled-back" && !opaque) || text(journal, "operation") == Some("uninstall");
    let candidate_action = actions.iter().any(|action| {
        text(action, "kind") == Some("source") && text(action, "path") == text(candidate, "path")
    });
    let selected = if restoring && previous.is_some() {
        let source = previous
            .as_ref()
            .ok_or_else(|| ContractError::new("Missing prior source"))?;
        (source, select(source, true)?)
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
