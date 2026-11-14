//! Source-bound qualification of opaque legacy cache additions, never cache execution.
use crate::codec::json::{self, Value};
use crate::error::ContractError;
use crate::fs::{Descriptor, DirectoryChain, Identity};
use crate::lifecycle::{get, object, string, text};
use crate::release_distribution as release;
use crate::release_distribution::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

pub const MAX_CACHES: usize = 256;
pub const MAX_CACHE_FILE: usize = 2 * 1024 * 1024;
pub const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
/// Fault points and clean-pack inspection, passed explicitly to the `*_with`
/// forms; tests supply fixture packs and concurrent writers.
pub trait Installation: Send + Sync {
    fn fault(&self, point: &str) -> Result<()>;
    fn inspect_package(&self, pack: &Path) -> Result<Value>;
}
/// The running installation: the dev-tools fault points of `bank_writes` and
/// `artifact`'s clean-pack inspection.
pub struct Installed;
impl Installation for Installed {
    fn fault(&self, point: &str) -> Result<()> {
        Ok(crate::bank_writes::fault(point)?)
    }
    fn inspect_package(&self, pack: &Path) -> Result<Value> {
        Ok(crate::artifact::inspect(pack)?)
    }
}
fn require(condition: bool, reason: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        let guidance = concat!(
            "Preserve the source and receipts; use the trusted signed installer ",
            "to diagnose this exact path, not cache deletion or receipt editing."
        );
        let message = format!("Signed runtime cache recovery refused: {reason}. {guidance}");
        Err(ContractError::new(message).into())
    }
}
pub fn encoded(value: &Value) -> Result<Vec<u8>> {
    let mut raw = json::encode(value, json::SORTED_INDENT2_ASCII)?.into_bytes();
    raw.push(b'\n');
    Ok(raw)
}
#[derive(Debug, Clone)]
pub struct Entry {
    pub pin: Identity,
    pub raw: Option<Vec<u8>>,
}
#[derive(Debug, Clone)]
pub struct Observation {
    pub root_mode: u32,
    pub entries: BTreeMap<String, Entry>,
    pub tree_pins: BTreeMap<String, Identity>,
}
impl Observation {
    pub fn identity(&self, excluded: &BTreeSet<String>) -> Result<Value> {
        let mut entries = Vec::new();
        for (name, item) in &self.entries {
            if excluded.contains(name) {
                continue;
            }
            let mode = Value::Int((item.pin.mode & 0o7777).into());
            let value = match &item.raw {
                Some(raw) => object([
                    ("kind", string("file")),
                    ("hash", string(release::digest(raw))),
                    ("mode", mode),
                ]),
                None => object([("kind", string("directory")), ("mode", mode)]),
            };
            entries.push((name.clone(), value));
        }
        Ok(object([
            ("kind", string("directory")),
            (
                "hash",
                string(release::digest(&encoded(&Value::Object(entries))?)),
            ),
            ("mode", Value::Int(self.root_mode.into())),
        ]))
    }
}
pub fn observe(root: &Path) -> Result<Observation> {
    observe_with(root, &Installed)
}
pub fn observe_with(root: &Path, install: &dyn Installation) -> Result<Observation> {
    require(
        root.is_absolute() && !root.components().any(|c| matches!(c, Component::ParentDir)),
        "noncanonical source path",
    )?;
    let chain = DirectoryChain::open(root)?;
    let mut protected = false;
    let mut node = PathBuf::from("/");
    let parts: Vec<_> = root
        .components()
        .filter_map(|c| {
            if let Component::Normal(n) = c {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    let mut ancestor_pins = Vec::new();
    for (index, descriptor) in chain.descriptors().iter().enumerate() {
        if index > 0 {
            node.push(parts.get(index - 1).ok_or_else(|| {
                ContractError::new("Source ancestor chain differs from its path")
            })?);
        }
        let pin = descriptor.stat()?;
        protected = crate::fs::trust::inspect_entry(&node, &pin, protected, false, false)?.0;
        ancestor_pins.push(pin);
    }
    let root_pin = chain.directory().stat()?;
    let mut observation = Observation {
        root_mode: root_pin.mode & 0o7777,
        entries: BTreeMap::new(),
        tree_pins: BTreeMap::from([(".".into(), root_pin)]),
    };
    let mut held = Vec::new();
    let mut total = 0;
    fn walk(
        dir: &Descriptor,
        root: &Path,
        relative: &str,
        barrier: bool,
        observation: &mut Observation,
        held: &mut Vec<(PathBuf, Descriptor, Identity)>,
        total: &mut usize,
    ) -> Result<()> {
        require(
            relative.split('/').filter(|v| !v.is_empty()).count() <= 64,
            "source directory depth exceeds 64 components",
        )?;
        let names = dir.entries_bounded(release::MAX_ENTRIES-observation.entries.len())?.ok_or_else(|| ContractError::new("Signed runtime cache recovery refused: complete source tree exceeds 4096 entries"))?;
        for name in names {
            require(
                observation.entries.len() < release::MAX_ENTRIES,
                "complete source tree exceeds 4096 entries",
            )?;
            let name_text = crate::host::text(&name, "cache source entry")?;
            let rel = if relative.is_empty() {
                name_text.to_owned()
            } else {
                format!("{relative}/{name_text}")
            };
            let before = dir.stat_at(&name)?;
            require(
                before.is_directory() || before.is_regular(),
                &format!("unsafe file type at {rel}"),
            )?;
            let mut child = dir.open_at(
                &name,
                libc::O_RDONLY
                    | if before.is_directory() {
                        libc::O_DIRECTORY
                    } else {
                        libc::O_NONBLOCK
                    },
                0,
            )?;
            let pin = child.stat()?;
            require(before == pin, &format!("source binding changed at {rel}"))?;
            let next_barrier = crate::fs::trust::inspect_entry(
                &root.join(&rel),
                &pin,
                barrier,
                !pin.is_directory(),
                false,
            )?
            .0;
            observation.tree_pins.insert(rel.clone(), pin);
            if pin.is_directory() {
                observation
                    .entries
                    .insert(rel.clone(), Entry { pin, raw: None });
                walk(&child, root, &rel, next_barrier, observation, held, total)?;
                held.push((PathBuf::from(&rel), child, pin));
            } else {
                let maximum = if rel == release::MANIFEST {
                    release::MAX_MANIFEST
                } else {
                    release::MAX_FILE
                };
                require(
                    pin.nlink == 1 && pin.size >= 0 && pin.size as usize <= maximum,
                    &format!("linked or oversized file at {rel}"),
                )?;
                let raw = child
                    .read_bounded(maximum)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                *total += raw.len();
                require(
                    *total <= release::MAX_BYTES + MAX_CACHE_BYTES + release::MAX_MANIFEST + 4096,
                    "source byte inventory exceeds its bound",
                )?;
                require(
                    child.stat()? == pin && dir.stat_at(&name)? == pin,
                    &format!("source bytes changed at {rel}"),
                )?;
                observation.entries.insert(
                    rel,
                    Entry {
                        pin,
                        raw: Some(raw),
                    },
                );
            }
        }
        Ok(())
    }
    walk(
        chain.directory(),
        root,
        "",
        protected,
        &mut observation,
        &mut held,
        &mut total,
    )?;
    install.fault("cache-recovery-before-final-recheck")?;
    chain.recheck()?;
    require(
        chain.directory().stat()? == root_pin,
        "source root changed during observation",
    )?;
    for (descriptor, pin) in chain.descriptors().iter().zip(ancestor_pins) {
        require(
            descriptor.stat()?.same_binding(pin),
            "source ancestor changed during observation",
        )?;
    }
    for (rel, descriptor, pin) in &held {
        require(
            descriptor.stat()? == *pin,
            "source named binding changed during observation",
        )?;
        let parent = DirectoryChain::open(&root.join(rel).parent().unwrap_or(root))?;
        require(
            parent.directory().stat_at(
                rel.file_name()
                    .ok_or_else(|| ContractError::new("Invalid cache directory"))?,
            )? == *pin,
            "source named binding changed during observation",
        )?;
    }
    // Every leaf's binding is checked again after the fault point; no changed leaf is qualified.
    for (rel, entry) in &observation.entries {
        let path = root.join(rel);
        let parent = DirectoryChain::open(path.parent().unwrap_or(root))?;
        require(
            parent.directory().stat_at(
                path.file_name()
                    .ok_or_else(|| ContractError::new("Invalid cache entry"))?,
            )? == entry.pin,
            "source named binding changed during observation",
        )?;
    }
    Ok(observation)
}
#[derive(Debug, Clone)]
pub struct Qualification {
    pub root: PathBuf,
    pub source: Value,
    pub source_identity: Value,
    pub receipt: Value,
    pub tree_pins: BTreeMap<String, Identity>,
    pub files: release::Inventory,
    pub summary: Value,
}
pub fn qualify(root: &Path, receipt: &Value) -> Result<Qualification> {
    qualify_with(root, receipt, &Installed, release::ALLOWED_SIGNERS)
}
fn cache_module(name: &str) -> Option<String> {
    let path = Path::new(name);
    if path.parent()?.file_name()? != "__pycache__" {
        return None;
    }
    let stem = path.file_name()?.to_str()?.strip_suffix(".pyc")?;
    let (module, tag) = stem.rsplit_once(".cpython-3")?;
    if module.is_empty() {
        return None;
    }
    let (digits, opt) = tag
        .split_once(".opt-")
        .map_or((tag, None), |(a, b)| (a, Some(b)));
    if digits.len() != 2
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || opt.is_some_and(|v| v.len() != 1 || !v.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(
        path.parent()?
            .parent()?
            .join(format!("{module}.py"))
            .to_str()?
            .to_owned(),
    )
}
pub fn qualify_with(
    root: &Path,
    receipt: &Value,
    install: &dyn Installation,
    signers: &str,
) -> Result<Qualification> {
    let root_text = crate::host::text(root.as_os_str(), "cache source path")?;
    let pack_path = root.join("skills/agent-memory");
    let source = get(receipt, "source");
    require(
        text(source, "kind") == Some("release")
            && text(source, "path") == Some(root_text)
            && text(source, "pack") == pack_path.to_str(),
        "source is not the recorded signed runtime",
    )?;
    let observed = observe_with(root, install)?;
    let file = |name: &str| -> Result<&[u8]> {
        Ok(observed
            .entries
            .get(name)
            .and_then(|e| e.raw.as_deref())
            .ok_or_else(|| {
                ContractError::new(format!(
                    "Signed runtime cache recovery refused: missing ordinary signed metadata {name}"
                ))
            })?)
    };
    let raw_manifest = file(release::MANIFEST)?;
    let manifest =
        release::verify_manifest_with(raw_manifest, file(release::SIGNATURE)?, None, signers)?;
    // Receipt-v3 `artifact_files` is the runtime inventory: the pack plus `bin/am`.
    // Its digest is the per-target runtime digest; without `bin/am` it is the
    // signed pack-only inventory.
    let runtime_files = get(receipt, "artifact_files");
    let runtime_pairs = runtime_files
        .as_object()
        .ok_or_else(|| ContractError::new("Receipt payload inventory must be an object"))?;
    let target_triple = text(source,"target").ok_or_else(|| ContractError::new("Signed runtime cache recovery requires the receipt target triple; re-bootstrap legacy receipts"))?;
    let binary = manifest
        .targets
        .get(target_triple)
        .ok_or_else(|| ContractError::new("Unsupported release target"))?;
    let pack_only = Value::Object(
        runtime_pairs
            .iter()
            .filter(|(name, _)| name != crate::artifact::BINARY_ENTRY)
            .cloned()
            .collect(),
    );
    let expected = &pack_only;
    let expected_pairs = expected
        .as_object()
        .ok_or_else(|| ContractError::new("Receipt payload inventory must be an object"))?;
    let binary_entry_declared = runtime_files.get(crate::artifact::BINARY_ENTRY);
    require(
        !expected_pairs.is_empty()
            && expected_pairs.len() <= release::MAX_ENTRIES
            && runtime_pairs.len() == expected_pairs.len() + 1
            && binary_entry_declared.is_some_and(|entry| {
                release::canonical(entry).ok()
                    == release::canonical(&crate::pack_validation::file_entry(
                        &binary.sha256,
                        0o755,
                    ))
                    .ok()
            })
            && release::inventory_digest(runtime_files)? == binary.artifact_digest
            && release::inventory_digest(expected)? == manifest.artifact_digest,
        "receipt payload inventory is not authenticated by the signed manifest",
    )?;
    require(
        text(source, "release_manifest_sha256") == Some(release::digest(raw_manifest).as_str())
            && text(source, "artifact_digest") == Some(binary.artifact_digest.as_str())
            && text(source, "revision") == Some(manifest.source_commit.as_str())
            && text(source, "version") == Some(manifest.version.as_str()),
        "authenticated manifest differs from the receipt",
    )?;
    let signed: BTreeSet<_> = expected_pairs
        .iter()
        .map(|(name, _)| format!("{}{name}", release::PREFIX))
        .collect();
    let mut wanted = signed.clone();
    wanted.extend([
        release::MANIFEST.to_owned(),
        release::SIGNATURE.to_owned(),
        "bin/am".into(),
    ]);
    let mut directories = BTreeSet::new();
    for name in &wanted {
        let path = Path::new(name);
        require(
            !path.is_absolute() && path.components().all(|p| matches!(p, Component::Normal(_))),
            "signed path escapes the source",
        )?;
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            directories
                .insert(crate::host::text(parent.as_os_str(), "signed source path")?.to_owned());
        }
    }
    let mut caches = BTreeSet::new();
    let mut extra_dirs = BTreeSet::new();
    for (name, item) in &observed.entries {
        if directories.contains(name) {
            require(
                item.raw.is_none(),
                &format!("signed directory is missing or changed at {name}"),
            )?;
        } else if wanted.contains(name) {
            require(
                item.raw.is_some(),
                &format!("signed file is missing or changed at {name}"),
            )?;
        } else if item.raw.is_none() {
            let path = Path::new(name);
            require(
                path.file_name().is_some_and(|n| n == "__pycache__")
                    && name.starts_with(release::PREFIX)
                    && item.pin.uid == crate::host::effective_uid()
                    && signed.iter().any(|value| {
                        Path::new(value).parent() == path.parent() && value.ends_with(".py")
                    }),
                &format!("unrelated or unsafe extra directory {name}"),
            )?;
            extra_dirs.insert(name.clone());
        } else {
            require(
                cache_module(name).is_some_and(|module| signed.contains(&module))
                    && item.pin.uid == crate::host::effective_uid()
                    && item.pin.nlink == 1
                    && [0o600, 0o644].contains(&(item.pin.mode & 0o7777)),
                &format!("unrelated or unsafe opaque cache extra {name}"),
            )?;
            require(
                item.raw
                    .as_ref()
                    .is_some_and(|raw| raw.len() <= MAX_CACHE_FILE),
                &format!("opaque cache file exceeds 2 MiB at {name}"),
            )?;
            caches.insert(name.clone());
        }
    }
    for name in wanted.union(&directories) {
        require(
            observed.entries.contains_key(name),
            &format!("signed payload path is missing: {name}"),
        )?;
    }
    require(
        !caches.is_empty() || !extra_dirs.is_empty(),
        "no eligible cache drift or more than 256 caches",
    )?;
    require(
        caches.len() <= MAX_CACHES,
        "no eligible cache drift or more than 256 caches",
    )?;
    let cache_bytes = caches
        .iter()
        .try_fold(0usize, |total, name| -> Result<usize> {
            Ok(total + file(name)?.len())
        })?;
    require(
        cache_bytes <= MAX_CACHE_BYTES,
        "opaque cache bytes exceed 16 MiB",
    )?;
    let mut files = release::Inventory::new();
    for name in &signed {
        let item = observed
            .entries
            .get(name)
            .ok_or_else(|| ContractError::new("Signed payload file is missing"))?;
        let relative = name
            .strip_prefix(release::PREFIX)
            .ok_or_else(|| ContractError::new("Signed path escapes the source"))?;
        files.insert(
            relative.into(),
            (
                item.raw
                    .clone()
                    .ok_or_else(|| ContractError::new("Missing signed file"))?,
                item.pin.mode & 0o7777,
            ),
        );
    }
    require(
        release::canonical(&release::inventory_value(&files))? == release::canonical(expected)?,
        "signed payload bytes, names or modes changed",
    )?;
    let raw_binary = file("bin/am")?;
    let binary_entry = observed
        .entries
        .get("bin/am")
        .ok_or_else(|| ContractError::new("Signed runtime binary is missing"))?;
    let runtime_digest = crate::artifact::runtime_digest(expected, &binary.sha256, 0o755)?.1;
    require(
        raw_binary.len() == binary.size
            && release::digest(raw_binary) == binary.sha256
            && binary_entry.pin.mode & 0o7777 == 0o755
            && runtime_digest == binary.artifact_digest,
        "signed runtime binary changed",
    )?;
    let excluded = caches.union(&extra_dirs).cloned().collect();
    let target = get(receipt, "targets").as_array().and_then(|targets| {
        targets
            .iter()
            .find(|t| text(t, "kind") == Some("source") && text(t, "path") == Some(root_text))
    });
    let target =
        target.ok_or_else(|| ContractError::new("Recorded signed source target is missing"))?;
    require(
        release::canonical(&observed.identity(&excluded)?)?
            == release::canonical(get(target, "identity"))?,
        "original signed source metadata, directories or modes changed",
    )?;
    let scratch = release::Scratch::new("am-clean-old-payload-")?;
    for (name, (raw, mode)) in &files {
        let destination = scratch.path.join(name);
        DirectoryChain::create(
            destination
                .parent()
                .ok_or_else(|| ContractError::new("Invalid signed file path"))?,
            false,
        )?;
        release::write_new(&destination, raw, *mode)?;
    }
    let package = install.inspect_package(&scratch.path)?;
    require(
        text(&package, "version") == Some(manifest.version.as_str())
            && release::canonical(get(source, "compatibility"))?
                == release::canonical(get(&package, "compatibility"))?,
        "authenticated source fields differ from the receipt",
    )?;
    let actual_source = object([
        ("kind", string("release")),
        ("path", string(root_text)),
        ("pack", string(pack_path.to_string_lossy())),
        (
            "remote",
            string("https://github.com/robertelee78/agent-memory.git"),
        ),
        ("ref", string("releases/latest")),
        ("revision", string(&manifest.source_commit)),
        ("dirty", Value::Bool(false)),
        ("version", string(&manifest.version)),
        ("artifact_digest", string(&binary.artifact_digest)),
        ("compatibility", get(&package, "compatibility").clone()),
        (
            "release_manifest_sha256",
            string(release::digest(raw_manifest)),
        ),
        ("signature_namespace", string(release::NAMESPACE)),
        ("signature_principal", string(release::PRINCIPAL)),
        ("target", string(target_triple)),
    ]);
    require(
        release::canonical(&actual_source)? == release::canonical(source)?,
        "authenticated source fields differ from the receipt",
    )?;
    let identity = observed.identity(&BTreeSet::new())?;
    let summary = object([
        ("schema", string("agent-memory/opaque-cache-recovery-v1")),
        ("planned", Value::Bool(true)),
        ("cache_count", Value::Int(caches.len().into())),
        ("cache_bytes", Value::Int(cache_bytes.into())),
        (
            "opaque_cache_files",
            Value::Object(
                caches
                    .iter()
                    .map(|name| -> Result<(String, Value)> {
                        Ok((name.clone(), string(release::digest(file(name)?))))
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
        ),
        ("old_source_identity", identity.clone()),
        (
            "receipt_sha256",
            string(release::digest(&encoded(receipt)?)),
        ),
        ("manifest_sha256", string(release::digest(raw_manifest))),
        (
            "authenticated_inventory_digest",
            string(&manifest.artifact_digest),
        ),
        ("cache_validity_verified", Value::Bool(false)),
        ("cache_code_executed", Value::Bool(false)),
    ]);
    Ok(Qualification {
        root: root.into(),
        source: actual_source,
        source_identity: identity,
        receipt: receipt.clone(),
        tree_pins: observed.tree_pins,
        files,
        summary,
    })
}
pub fn recheck(qualification: &Qualification) -> Result<()> {
    recheck_with(qualification, &Installed)
}
pub fn recheck_with(qualification: &Qualification, install: &dyn Installation) -> Result<()> {
    let observed = observe_with(&qualification.root, install)?;
    require(
        observed.identity(&BTreeSet::new())? == qualification.source_identity
            && observed.tree_pins == qualification.tree_pins,
        "qualified source changed; obtain a fresh trusted-source retry",
    )
}
