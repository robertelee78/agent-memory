//! Complete-bank and private-operation budgets.
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use std::path::{Component, Path, PathBuf};
pub const MAX_FILES: usize = 65536;
pub const MAX_BANK: usize = 64 * 1024 * 1024;
pub const MAX_FILE: usize = 16384;
pub const MAX_PRIVATE: usize = 512 * 1024 * 1024;
pub const MAX_PAGE: usize = 65536;
pub fn bank_size(files: &Value) -> Result<()> {
    crate::bank_capacity::check_files(files)
}
pub fn private_bytes(value: &Value) -> Result<Vec<u8>> {
    let raw = crate::developer_identity::encoded(value)?;
    if raw.len() > MAX_PRIVATE {
        return Err(ContractError::new(
            "Complete private operation exceeds 512MiB; preserve sources and recovery evidence",
        ));
    }
    Ok(raw)
}
pub fn read_bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let resolved = crate::fs::canonical_path(path, true)?;
    let parent = resolved
        .parent()
        .ok_or_else(|| ContractError::new("Invalid private input path"))?;
    let name = resolved
        .file_name()
        .ok_or_else(|| ContractError::new("Invalid private input path"))?;
    crate::fs::read_source(parent, Path::new(name), limit)
        .map(|r| r.bytes)
        .map_err(|e| match e {
            crate::fs::ReadError::SizeLimit { .. } => ContractError::new(
                "Complete private input exceeds its explicit encoded budget; no truncation",
            ),
            _ => ContractError::new(e.to_string()),
        })
}
pub fn read_json(path: &Path, limit: usize) -> Result<Value> {
    crate::developer_identity::load_with_limit(&read_bytes(path, limit)?, limit)
}

fn output_path(path: &Path, repo: &Path, pack: &Path) -> Result<PathBuf> {
    let supplied = crate::host::expanduser(path)?;
    if !supplied.is_absolute()
        || supplied
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(ContractError::new(
            "Select an absolute owner-only output outside the repository",
        ));
    }
    let parent = supplied
        .parent()
        .ok_or_else(|| ContractError::new("Invalid private output"))?;
    let parent = crate::fs::trust::git_pointer(
        Path::new("/"),
        crate::host::text(parent.as_os_str(), "private output path")?,
    )?;
    crate::fs::trust::state(&parent)?;
    let target = parent.join(
        supplied
            .file_name()
            .ok_or_else(|| ContractError::new("Invalid private output"))?,
    );
    crate::fs::trust::record(&target)?;
    let repo = crate::fs::canonical_path(repo, false)?;
    let pack = crate::fs::canonical_path(pack, false)?;
    let source = if pack
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "skills")
    {
        pack.parent().and_then(Path::parent).unwrap_or(&pack)
    } else {
        &pack
    };
    if target.starts_with(repo) || target.starts_with(source) || target.starts_with(&pack) {
        return Err(ContractError::new(
            "Private review output must stay outside the repository and executing runtime/source",
        ));
    }
    Ok(target)
}
pub fn preflight_private_output(path: &Path, repo: &Path, pack: &Path) -> Result<PathBuf> {
    output_path(path,repo,pack).map_err(|_|ContractError::new("Choose a new absolute output path in an existing owner-only directory outside the repository and installed runtime; preserve existing files."))
}

fn pinned_output_parent(target: &Path) -> Result<crate::fs::DirectoryChain> {
    let parent = target
        .parent()
        .ok_or_else(|| ContractError::new("Invalid private output"))?;
    let chain =
        crate::fs::DirectoryChain::open(parent).map_err(|e| ContractError::new(e.to_string()))?;
    let mut protected = false;
    let mut path = PathBuf::from("/");
    let mut parts = parent.components().filter_map(|part| match part {
        Component::Normal(name) => Some(name),
        _ => None,
    });
    for (index, descriptor) in chain.descriptors().iter().enumerate() {
        if index > 0 {
            path.push(parts.next().expect("directory chain matches path"));
        }
        let identity = descriptor
            .stat()
            .map_err(|e| ContractError::new(e.to_string()))?;
        protected = crate::fs::trust::inspect_entry(&path, &identity, protected, false, false)?.0;
    }
    let identity = chain
        .directory()
        .stat()
        .map_err(|e| ContractError::new(e.to_string()))?;
    if identity.uid != crate::host::effective_uid() || identity.mode & 0o077 != 0 {
        return Err(ContractError::new(
            "Private output directory is not owner-only",
        ));
    }
    chain
        .recheck()
        .map_err(|e| ContractError::new(e.to_string()))?;
    Ok(chain)
}

fn publish_private(target: &Path, raw: &[u8]) -> Result<()> {
    let chain = pinned_output_parent(target)?;
    let dir = chain.directory();
    let name = target
        .file_name()
        .ok_or_else(|| ContractError::new("Invalid private output"))?;
    match dir.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(mut file) => {
            let identity = file.stat().map_err(|e| ContractError::new(e.to_string()))?;
            if !identity.is_regular()
                || identity.uid != crate::host::effective_uid()
                || identity.nlink != 1
                || identity.mode & 0o077 != 0
            {
                return Err(ContractError::new(
                    "Preserve existing private output; choose a new path",
                ));
            }
            let existing = file
                .read_bounded(MAX_PRIVATE)
                .map_err(|e| ContractError::new(e.to_string()))?;
            let named = dir
                .stat_at(name)
                .map_err(|e| ContractError::new(e.to_string()))?;
            if existing != raw || named != identity {
                return Err(ContractError::new(
                    "Preserve existing private output; choose a new path",
                ));
            }
            chain
                .recheck()
                .map_err(|e| ContractError::new(e.to_string()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let temp = format!(".private-output-{}", crate::host::uuid4()?);
            let temp = std::ffi::OsStr::new(&temp);
            let mut file = dir
                .create_exclusive(temp, libc::O_WRONLY, 0o600)
                .map_err(|e| ContractError::new(e.to_string()))?;
            let identity = file.stat().map_err(|e| ContractError::new(e.to_string()))?;
            let result = (|| -> Result<()> {
                file.write_all(raw)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                file.fsync()
                    .map_err(|e| ContractError::new(e.to_string()))?;
                chain
                    .recheck()
                    .map_err(|e| ContractError::new(e.to_string()))?;
                if !dir
                    .stat_at(temp)
                    .is_ok_and(|current| current.same_inode(identity))
                {
                    return Err(ContractError::new(
                        "Private output temporary file changed; preserve existing files",
                    ));
                }
                crate::fs::rename_at(dir, temp, dir, name, crate::fs::RenameMode::NoReplace)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                dir.fsync_directory()
                    .map_err(|e| ContractError::new(e.to_string()))?;
                chain
                    .recheck()
                    .map_err(|e| ContractError::new(e.to_string()))?;
                let mut actual = dir
                    .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                let opened = actual
                    .stat()
                    .map_err(|e| ContractError::new(e.to_string()))?;
                let bytes = actual
                    .read_bounded(MAX_PRIVATE)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                let named = dir
                    .stat_at(name)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                if !opened.same_inode(identity) || named != opened || bytes != raw {
                    return Err(ContractError::new(
                        "Registry readback differs; inspect and replay the same operation",
                    ));
                }
                chain
                    .recheck()
                    .map_err(|e| ContractError::new(e.to_string()))?;
                Ok(())
            })();
            if dir
                .stat_at(temp)
                .is_ok_and(|current| current.same_inode(identity))
            {
                dir.unlink_at(temp)
                    .map_err(|e| ContractError::new(e.to_string()))?;
            }
            result?;
        }
        Err(e) => return Err(ContractError::new(e.to_string())),
    }
    Ok(())
}
fn inspection_len(value: &Value) -> Result<usize> {
    match value {
        Value::String(value) => Ok(value.chars().count()),
        Value::Array(value) => Ok(value.len()),
        Value::Object(value) => Ok(value.len()),
        _ => Err(ContractError::new("Invalid private inspection")),
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Int(value) => value.as_str() != "0",
        Value::Float(value) => *value != 0.0,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

pub fn write_private(path: &Path, value: &Value, repo: &Path, pack: &Path) -> Result<Value> {
    let target = output_path(path, repo, pack)?;
    let raw = private_bytes(value)?;
    publish_private(&target, &raw)?;
    let mut out = object([
        ("schema", string("agent-memory/private-review-artifact-v2")),
        (
            "path",
            string(crate::host::text(
                target.as_os_str(),
                "private output path",
            )?),
        ),
        (
            "sha256",
            crate::developer_identity::digest(Some(&raw))
                .map(string)
                .unwrap_or(Value::Null),
        ),
        ("bytes", Value::Int(raw.len().into())),
    ]);
    if let Value::Object(pairs) = &mut out {
        for key in [
            "operation",
            "sharing_state",
            "refresh_applied",
            "publication_verified",
            "remote_freshness_verified",
            "conflicts_pending",
        ] {
            if let Some(v) = value.get(key) {
                pairs.push((key.into(), v.clone()));
            }
        }
        if get(value, "schema").as_str() == Some("agent-memory/semantic-sharing-inspection-v2") {
            for (key, source) in [
                ("changed_components", "changed"),
                ("review_pages", "pages"),
                ("selection_required", "pending_selections"),
            ] {
                let count = inspection_len(get(value, source))?;
                pairs.push((key.into(), Value::Int(count.into())));
            }
            let causes = value
                .get("known_causes")
                .ok_or_else(|| ContractError::new("Invalid private inspection"))?;
            let pending = if truthy(causes) {
                true
            } else {
                truthy(
                    value
                        .get("withheld_sources")
                        .ok_or_else(|| ContractError::new("Invalid private inspection"))?,
                )
            };
            if let Some((_, v)) = pairs.iter_mut().find(|(k, _)| k == "conflicts_pending") {
                *v = Value::Bool(pending);
            } else {
                pairs.push(("conflicts_pending".into(), Value::Bool(pending)));
            }
        }
        pairs.push((
            "canonical_writes_performed".into(),
            value
                .get("canonical_writes_performed")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ));
    }
    Ok(out)
}
