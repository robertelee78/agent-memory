//! Export checked source-bound pages to exclusive owner-only private artifacts.
use crate::codec::json::Value;
use crate::developer_identity::{encoded, sha256_hex};
use crate::error::{ContractError, Result};
use crate::fs::{Descriptor, Identity};
use crate::lifecycle::{get, object, string};
use crate::semantic_sharing::{array, err, hash, text};
use crate::{private_workspace as private, sharing_frontier as frontier, sharing_limits as limits};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
pub const SCHEMA: &str = "agent-memory/semantic-sharing-page-export-v1";
fn io(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn parent(path: &Path) -> Result<&Path> {
    path.parent()
        .ok_or_else(|| err("Missing private output parent"))
}
fn name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| err("Invalid private output name"))
}
fn absent_at(directory: &Descriptor, name: &str) -> Result<()> {
    match directory.stat_at(OsStr::new(name)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(error)),
        Ok(_) => Err(ContractError::private_output(
            "Preserve existing private output; choose new paths",
        )),
    }
}
pub fn absent(path: &Path) -> Result<()> {
    let (directory, _) = private::checked_directory(parent(path)?)?;
    absent_at(&directory, name(path)?)
}
pub fn preflight(repo: &Path, output: &Path, pages_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    if !output.is_absolute() || !pages_dir.is_absolute() {
        return Err(ContractError::private_output(
            "Choose new absolute private output paths",
        ));
    }
    let runtime = crate::cli::pack_root()?;
    let manifest = limits::preflight_private_output(output, repo, &runtime)
        .map_err(|error| ContractError::private_output(error.to_string()))?;
    let directory = limits::preflight_private_output(pages_dir, repo, &runtime)
        .map_err(|error| ContractError::private_output(error.to_string()))?;
    if manifest.starts_with(&directory) || directory.starts_with(&manifest) {
        return Err(ContractError::private_output(
            "Choose distinct, non-overlapping private outputs",
        ));
    }
    let root = crate::fs::canonical_path(repo, true)?;
    let pack = crate::fs::canonical_path(&runtime, true)?;
    let source = if pack.parent().and_then(Path::file_name) == Some(OsStr::new("skills")) {
        pack.parent().and_then(Path::parent).unwrap_or(&pack)
    } else {
        &pack
    };
    if [&root, &pack, source]
        .into_iter()
        .any(|root| root.starts_with(&directory))
    {
        return Err(ContractError::private_output(
            "Private export must not overlap repository or runtime",
        ));
    }
    absent(&manifest)?;
    absent(&directory)?;
    for path in [&manifest, &directory] {
        private::checked_directory(parent(path)?).map_err(|_| {
            ContractError::private_output("Choose existing owner-only private output directories")
        })?;
    }
    Ok((manifest, directory))
}
fn stable(a: Identity, b: Identity) -> bool {
    a.dev == b.dev
        && a.ino == b.ino
        && a.mode == b.mode
        && a.uid == b.uid
        && a.nlink == b.nlink
        && a.size == b.size
        && a.mtime_ns == b.mtime_ns
        && a.ctime_ns == b.ctime_ns
}
pub fn readback(directory: &Descriptor, name: &str, size: usize, expected: &str) -> Result<()> {
    if size > limits::MAX_PRIVATE {
        return Err(err("Private export file exceeds its bound"));
    }
    let mut file = directory
        .open_at(
            OsStr::new(name),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0,
        )
        .map_err(io)?;
    let before = file.stat().map_err(io)?;
    if !before.is_regular()
        || before.uid != crate::host::effective_uid()
        || before.mode & 0o7777 != 0o600
        || before.nlink != 1
        || before.size < 0
        || before.size as u64 != size as u64
    {
        return Err(err("Unsafe or changed private export file"));
    }
    let mut hasher = Sha256::new();
    let mut count = 0usize;
    let mut buffer = [0u8; 65536];
    loop {
        let bound = (size - count + 1).min(buffer.len());
        let chunk = buffer
            .get_mut(..bound)
            .ok_or_else(|| err("Invalid export read bound"))?;
        let read = file.read_chunk(chunk).map_err(io)?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read)
            .ok_or_else(|| err("Private export file changed during readback"))?;
        if count > size {
            return Err(err("Private export file changed during readback"));
        }
        hasher.update(
            chunk
                .get(..read)
                .ok_or_else(|| err("Invalid export read size"))?,
        );
    }
    let actual = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if count != size
        || actual != expected
        || !stable(before, file.stat().map_err(io)?)
        || !stable(before, directory.stat_at(OsStr::new(name)).map_err(io)?)
    {
        return Err(err("Private export readback did not match"));
    }
    Ok(())
}
pub fn write_checked(directory: &Descriptor, name: &str, raw: &[u8]) -> Result<()> {
    private::write_fd(directory, name, raw)?;
    readback(directory, name, raw.len(), &sha256_hex(raw))
}
pub fn export(
    repo: &Path,
    output: &Path,
    pages_dir: &Path,
    request: Option<&Value>,
) -> Result<Value> {
    let (manifest_path, directory) = preflight(repo, output, pages_dir)?;
    let (parent_fd, parent_info) = private::checked_directory(parent(&directory)?)?;
    let (manifest_fd, manifest_parent_info) = private::checked_directory(parent(&manifest_path)?)?;
    absent_at(&parent_fd, name(&directory)?)?;
    absent_at(&manifest_fd, name(&manifest_path)?)?;
    let created = parent_fd
        .create_directory(OsStr::new(name(&directory)?), 0o700)
        .map_err(io)?;
    parent_fd.fsync_directory().map_err(io)?;
    let (directory_fd, directory_info) = private::checked_directory(&directory)?;
    if !created.stat().map_err(io)?.same_inode(directory_info) {
        return Err(err("Private export directory changed during creation"));
    }
    private::check_directory(parent(&directory)?, &parent_info)?;
    let frontier = crate::bank_writes::recovery_frontier_provider()?;
    let token = frontier.observe(repo)?;
    let ((root, observation, binding, _, _), _, manifest, pages) =
        frontier::build(repo, request, None, None)?;
    let binding_sha = hash(&binding)?;
    let descriptors = array(get(&manifest, "pages"))?;
    if pages.len() != descriptors.len() {
        return Err(err("Checked page inventory changed"));
    }
    let mut total = 0usize;
    for (index, (page, descriptor)) in pages.iter().zip(descriptors).enumerate() {
        let raw = encoded(page)?;
        total = total
            .checked_add(raw.len())
            .ok_or_else(|| err("Checked page does not match its bounded manifest descriptor"))?;
        if total > limits::MAX_PRIVATE
            || raw.len() > limits::MAX_PAGE
            || get(page, "schema").as_str() != Some("agent-memory/semantic-sharing-page-v2")
            || get(page, "index") != &Value::Int(index.into())
            || get(page, "binding_sha256") != &string(&binding_sha)
            || get(descriptor, "index") != &Value::Int(index.into())
            || get(descriptor, "bytes") != &Value::Int(raw.len().into())
            || get(descriptor, "sha256") != &string(sha256_hex(&raw))
        {
            return Err(err(
                "Checked page does not match its bounded manifest descriptor",
            ));
        }
        write_checked(&directory_fd, &format!("{index}.json"), &raw)?;
    }
    directory_fd.fsync_directory().map_err(io)?;
    let manifest_raw = limits::private_bytes(&manifest)?;
    let manifest_sha = sha256_hex(&manifest_raw);
    write_checked(&manifest_fd, name(&manifest_path)?, &manifest_raw)?;
    let check_dirs = || -> Result<()> {
        private::check_directory(parent(&manifest_path)?, &manifest_parent_info)?;
        private::check_directory(&directory, &directory_info)?;
        private::check_directory(parent(&directory)?, &parent_info)
    };
    check_dirs()?;
    crate::semantic_sharing::final_sources(&root, &observation, &binding, None)?;
    let known = crate::sharing_holds::load(
        &root,
        text(get(&observation, "project_id"))?,
        &crate::cli::pack_root()?,
        None,
    )?;
    if hash(&Value::Array(known))? != text(get(&manifest, "known_inventory_sha256"))? {
        return Err(err("Known hold inventory changed; inspect again"));
    }
    frontier.check(&token, repo, None, None)?;
    let expected = (0..pages.len())
        .map(|index| format!("{index}.json"))
        .collect::<BTreeSet<_>>();
    let actual = directory_fd
        .entries_bounded(pages.len() + 1)
        .map_err(io)?
        .ok_or_else(|| err("Unexpected private export directory contents"))?
        .into_iter()
        .map(|name| {
            name.into_string()
                .map_err(|_| err("Unexpected private export directory contents"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if !actual.is_subset(&expected) {
        return Err(err("Unexpected private export directory contents"));
    }
    if actual != expected {
        return Err(err("Missing private export page"));
    }
    for (index, descriptor) in descriptors.iter().enumerate() {
        let size = get(descriptor, "bytes")
            .as_int()
            .and_then(|n| n.to_string().parse::<usize>().ok())
            .ok_or_else(|| err("Invalid page descriptor size"))?;
        readback(
            &directory_fd,
            &format!("{index}.json"),
            size,
            text(get(descriptor, "sha256"))?,
        )?;
    }
    readback(
        &manifest_fd,
        name(&manifest_path)?,
        manifest_raw.len(),
        &manifest_sha,
    )?;
    check_dirs()?;
    let marker = encoded(&object([
        ("schema", string(SCHEMA)),
        ("manifest_sha256", get(&manifest, "manifest_sha256").clone()),
        ("manifest_artifact_sha256", string(&manifest_sha)),
        ("binding_sha256", string(binding_sha)),
        ("page_count", Value::Int(pages.len().into())),
    ]))?;
    write_checked(&directory_fd, "complete.json", &marker)?;
    readback(
        &manifest_fd,
        name(&manifest_path)?,
        manifest_raw.len(),
        &manifest_sha,
    )?;
    check_dirs()?;
    directory_fd.fsync_directory().map_err(io)?;
    parent_fd.fsync_directory().map_err(io)?;
    Ok(object([
        ("schema", string("agent-memory/private-review-artifact-v2")),
        (
            "path",
            string(crate::host::text(
                manifest_path.as_os_str(),
                "private manifest path",
            )?),
        ),
        ("sha256", string(manifest_sha)),
        ("bytes", Value::Int(manifest_raw.len().into())),
        (
            "publication_verified",
            get(&manifest, "publication_verified").clone(),
        ),
        (
            "changed_components",
            Value::Int(array(get(&manifest, "changed"))?.len().into()),
        ),
        ("review_pages", Value::Int(descriptors.len().into())),
        (
            "selection_required",
            Value::Int(array(get(&manifest, "pending_selections"))?.len().into()),
        ),
        (
            "conflicts_pending",
            Value::Bool(
                !array(get(&manifest, "known_causes"))?.is_empty()
                    || !array(get(&manifest, "withheld_sources"))?.is_empty(),
            ),
        ),
        (
            "canonical_writes_performed",
            manifest
                .get("canonical_writes_performed")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        (
            "pages_dir",
            string(crate::host::text(
                directory.as_os_str(),
                "private pages path",
            )?),
        ),
        ("page_export_sha256", string(sha256_hex(&marker))),
        ("page_count", Value::Int(pages.len().into())),
    ]))
}
