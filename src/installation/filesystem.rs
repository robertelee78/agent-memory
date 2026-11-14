//! Pinned, descriptor-relative lifecycle writes; every one is a numbered
//! `runtime_faults` step exactly where the baseline called `mutate`.
use super::{
    Error, MAX_MANAGED_BYTES, MAX_MANAGED_ENTRIES, MAX_MANAGED_FILE, Result, encoded, require,
};
use crate::codec::json::{self, Value};
use crate::error::ContractError;
use crate::fs::{self, Descriptor, DirectoryChain, Identity, RenameMode};
use crate::runtime_faults::{mutate, temporary_probe};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

fn io_error(error: io::Error) -> Error {
    Error::Contract(ContractError::new(error.to_string()))
}

pub(crate) fn split(path: &Path) -> Result<(&Path, &OsStr)> {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => Ok((parent, name)),
        _ => Err(ContractError::new(format!(
            "Installation path requires a named parent: {}",
            path.display()
        ))
        .into()),
    }
}

/// Baseline `fingerprint`: type, bytes and modes, never following a symlink;
/// `Value::Null` when the node is absent.
pub fn fingerprint(path: &Path) -> Result<Value> {
    Ok(fs::fingerprint(path)?.map_or(Value::Null, |value| value.json_value()))
}

/// Structural JSON equality independent of object key order (Python `==`).
pub fn same(left: &Value, right: &Value) -> bool {
    match (
        json::encode(left, json::SORTED_COMPACT_UTF8),
        json::encode(right, json::SORTED_COMPACT_UTF8),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// `lstat` without following the final component; `None` when absent.
pub(crate) fn named(path: &Path) -> Result<Option<Identity>> {
    let (parent, name) = split(path)?;
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(error) if matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ENOTDIR)) => {
            return Ok(None);
        }
        Err(error) => return Err(io_error(error)),
    };
    match chain.directory().stat_at(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

pub(crate) fn lexists(path: &Path) -> Result<bool> {
    Ok(named(path)?.is_some())
}

/// `Path.is_file()` without following a final symlink.
pub(crate) fn is_regular(path: &Path) -> Result<bool> {
    Ok(named(path)?.is_some_and(Identity::is_regular))
}

pub(crate) fn is_directory(path: &Path) -> Result<bool> {
    Ok(named(path)?.is_some_and(Identity::is_directory))
}

/// One bounded ordinary-file read, `None` when absent.
pub(crate) fn read_optional(path: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
    if !lexists(path)? {
        return Ok(None);
    }
    let (parent, name) = split(path)?;
    Ok(Some(fs::read_source(parent, Path::new(name), limit)?.bytes))
}

pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    Descriptor::open_directory(path)
        .and_then(|directory| directory.fsync_directory())
        .map_err(io_error)
}

fn write_file_now(path: &Path, raw: &[u8], mode: u32) -> Result<()> {
    let (parent, name) = split(path)?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    let mut file = chain
        .directory()
        .create_exclusive(name, libc::O_WRONLY, 0o600)
        .map_err(io_error)?;
    file.write_all(raw).map_err(io_error)?;
    file.fsync().map_err(io_error)?;
    file.set_mode(mode).map_err(io_error)?;
    chain.recheck().map_err(io_error)
}

/// Baseline `write_file`: exclusive, no-follow, private while written, then chmod.
pub(crate) fn write_file(path: &Path, raw: &[u8], mode: u32) -> Result<()> {
    mutate("file-stage", || {
        write_file_now(path, raw, mode).map_err(contract)
    })
    .map_err(Error::from)
}

fn contract(error: Error) -> ContractError {
    match error {
        Error::Contract(error) => error,
        other => ContractError::new(other.to_string()),
    }
}

pub(crate) fn cleanup_file(path: &Path) -> Result<()> {
    if !lexists(path)? {
        return Ok(());
    }
    let (parent, name) = split(path)?;
    mutate("stage-cleanup", || {
        let chain = DirectoryChain::open(parent).map_err(|e| contract(io_error(e)))?;
        chain
            .directory()
            .unlink_at(name)
            .map_err(|e| contract(io_error(e)))?;
        chain
            .directory()
            .fsync_directory()
            .map_err(|e| contract(io_error(e)))
    })?;
    Ok(())
}

pub(crate) fn unique_name(prefix: &str) -> Result<String> {
    Ok(format!(
        "{prefix}{}",
        crate::host::uuid4()?.replace('-', "")
    ))
}

/// Baseline `replace_record`: private stage, atomic replace, stage cleanup.
pub(crate) fn replace_record(path: &Path, value: &Value) -> Result<()> {
    let (parent, name) = split(path)?;
    let stage = parent.join(unique_name(".record-")?);
    let raw = encoded(value)?;
    let published = (|| -> Result<()> {
        write_file(&stage, &raw, 0o600)?;
        mutate("record-publication", || {
            fs::native_rename(&stage, &parent.join(name), RenameMode::Replace)
                .map_err(|e| contract(io_error(e)))
        })?;
        Ok(())
    })();
    let cleaned = cleanup_file(&stage);
    published?;
    cleaned
}

fn rename_mode(exchange: bool) -> RenameMode {
    if exchange {
        RenameMode::Exchange
    } else {
        RenameMode::NoReplace
    }
}

/// Baseline `_native_rename`: native no-replace / exchange; unsupported
/// filesystems fail closed. No lifecycle step (used by the probe).
pub(crate) fn raw_rename(source: &Path, target: &Path, exchange: bool) -> io::Result<()> {
    fs::native_rename(source, target, rename_mode(exchange))
}

/// Baseline `native_rename`: one `atomic-node-switch` step with directory syncs.
pub(crate) fn native_rename(source: &Path, target: &Path, exchange: bool) -> Result<()> {
    mutate("atomic-node-switch", || {
        raw_rename(source, target, exchange).map_err(|e| contract(io_error(e)))
    })?;
    Ok(())
}

struct CopyBudget {
    entries: usize,
    bytes: u64,
}
impl CopyBudget {
    fn add(&mut self, size: u64) -> Result<()> {
        self.entries += 1;
        self.bytes = self.bytes.saturating_add(size);
        require(
            self.entries <= MAX_MANAGED_ENTRIES && self.bytes <= MAX_MANAGED_BYTES,
            "Managed directory inventory exceeds its bound",
        )
    }
}

fn copy_file(
    source: &Descriptor,
    name: &OsStr,
    target: &Descriptor,
    target_name: &OsStr,
    info: Identity,
    path: &Path,
) -> Result<()> {
    require(
        info.size >= 0 && info.size as u128 <= MAX_MANAGED_FILE as u128,
        format!("Oversized managed file: {}", path.display()),
    )?;
    let mut file = source
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io_error)?;
    require(
        file.stat().map_err(io_error)? == info,
        "Managed node changed during payload copy",
    )?;
    let raw = file.read_bounded(MAX_MANAGED_FILE)?;
    require(
        raw.len() as u128 == info.size as u128 && file.stat().map_err(io_error)? == info,
        "Managed node changed during payload copy",
    )?;
    let mut copy = target
        .create_exclusive(target_name, libc::O_WRONLY, 0o600)
        .map_err(io_error)?;
    copy.write_all(&raw).map_err(io_error)?;
    copy.fsync().map_err(io_error)?;
    copy.set_mode(info.mode & 0o7777).map_err(io_error)
}

fn copy_children(
    source: &Descriptor,
    target: &Descriptor,
    path: &Path,
    budget: &mut CopyBudget,
) -> Result<()> {
    let names = source
        .entries_bounded(MAX_MANAGED_ENTRIES)
        .map_err(io_error)?
        .ok_or_else(|| ContractError::new("Managed directory inventory exceeds its bound"))?;
    for name in names {
        let node = path.join(&name);
        let info = source.stat_at(&name).map_err(io_error)?;
        budget.add(if info.is_regular() {
            u64::try_from(info.size).unwrap_or(u64::MAX)
        } else {
            0
        })?;
        if info.is_symlink() {
            let link = source.read_link_at(&name).map_err(io_error)?;
            target
                .symlink_at(&name, link.as_os_str())
                .map_err(io_error)?;
        } else if info.is_directory() {
            let child = source.open_directory_at(&name).map_err(io_error)?;
            require(
                child.stat().map_err(io_error)?.same_inode(info),
                "Managed node changed during payload copy",
            )?;
            let created = target.create_directory(&name, 0o700).map_err(io_error)?;
            copy_children(&child, &created, &node, budget)?;
            created.fsync_directory().map_err(io_error)?;
            created.set_mode(info.mode & 0o7777).map_err(io_error)?;
        } else if info.is_regular() {
            copy_file(source, &name, target, &name, info, &node)?;
        } else {
            return Err(ContractError::new(format!(
                "Unsupported filesystem node: {}",
                node.display()
            ))
            .into());
        }
    }
    target.fsync_directory().map_err(io_error)
}

fn copy_now(source: &Path, target: &Path, private_root: bool) -> Result<()> {
    let (source_parent, source_name) = split(source)?;
    let (target_parent, target_name) = split(target)?;
    let from = DirectoryChain::open(source_parent).map_err(io_error)?;
    let to = DirectoryChain::open(target_parent).map_err(io_error)?;
    let info = from.directory().stat_at(source_name).map_err(io_error)?;
    let mut budget = CopyBudget {
        entries: 0,
        bytes: 0,
    };
    if info.is_symlink() {
        let link = from
            .directory()
            .read_link_at(source_name)
            .map_err(io_error)?;
        to.directory()
            .symlink_at(target_name, link.as_os_str())
            .map_err(io_error)?;
    } else if info.is_directory() {
        // Keep the new top-level staging entry private throughout the copy.
        let directory = from
            .directory()
            .open_directory_at(source_name)
            .map_err(io_error)?;
        let created = to
            .directory()
            .create_directory(target_name, 0o700)
            .map_err(io_error)?;
        copy_children(&directory, &created, source, &mut budget)?;
        if !private_root {
            created.set_mode(info.mode & 0o7777).map_err(io_error)?;
        }
        created.fsync_directory().map_err(io_error)?;
    } else if info.is_regular() {
        copy_file(
            from.directory(),
            source_name,
            to.directory(),
            target_name,
            info,
            source,
        )?;
    } else {
        return Err(ContractError::new(format!(
            "Unsupported filesystem node: {}",
            source.display()
        ))
        .into());
    }
    to.directory().fsync_directory().map_err(io_error)?;
    from.recheck().map_err(io_error)?;
    to.recheck().map_err(io_error)
}

/// Baseline `copy_node`: one `payload-snapshot` logical write.
pub(crate) fn copy_node(source: &Path, target: &Path, private_root: bool) -> Result<()> {
    mutate("payload-snapshot", || {
        copy_now(source, target, private_root).map_err(contract)
    })?;
    Ok(())
}

/// Baseline `sync_tree`: fsync every file and directory below `path`.
pub(crate) fn sync_tree(path: &Path) -> Result<()> {
    let Some(info) = named(path)? else {
        return Ok(());
    };
    if info.is_symlink() {
        return Ok(());
    }
    let (parent, name) = split(path)?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    if info.is_regular() {
        let file = chain
            .directory()
            .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            .map_err(io_error)?;
        return file.fsync().map_err(io_error);
    }
    fn walk(directory: &Descriptor, depth: usize) -> Result<()> {
        require(depth <= 256, "Managed directory nesting exceeds its bound")?;
        let names = directory
            .entries_bounded(MAX_MANAGED_ENTRIES)
            .map_err(io_error)?
            .ok_or_else(|| ContractError::new("Managed directory inventory exceeds its bound"))?;
        for name in names {
            let info = directory.stat_at(&name).map_err(io_error)?;
            if info.is_directory() {
                walk(
                    &directory.open_directory_at(&name).map_err(io_error)?,
                    depth + 1,
                )?;
            } else if info.is_regular() {
                directory
                    .open_at(&name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
                    .and_then(|file| file.fsync())
                    .map_err(io_error)?;
            }
        }
        directory.fsync_directory().map_err(io_error)
    }
    walk(
        &chain
            .directory()
            .open_directory_at(name)
            .map_err(io_error)?,
        0,
    )
}

fn make_directory(path: &Path) -> Result<()> {
    let (parent, name) = split(path)?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    chain
        .directory()
        .create_directory(name, 0o700)
        .map_err(io_error)?;
    chain.directory().fsync_directory().map_err(io_error)
}

/// Baseline `make_state_directory`.
pub(crate) fn make_state_directory(path: &Path) -> Result<()> {
    if let Some(info) = named(path)? {
        return require(
            info.is_directory(),
            "State parent must be an ordinary directory",
        );
    }
    if let Some(parent) = path.parent() {
        make_state_directory(parent)?;
    }
    mutate("state-parent-directory", || {
        make_directory(path).map_err(contract)
    })?;
    Ok(())
}

/// Baseline `make_parents`: each new parent is journaled before it is created.
pub(crate) fn make_parents(
    parent: &Path,
    record: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    if let Some(info) = named(parent)? {
        return require(info.is_directory(), "Target parent is not a directory");
    }
    if let Some(above) = parent.parent() {
        make_parents(above, record)?;
    }
    record(parent)?;
    mutate("target-parent-directory", || {
        make_directory(parent).map_err(contract)
    })?;
    Ok(())
}

/// Exercise the actual parent filesystem (exchange and no-replace) before
/// changing any target; not a lifecycle step.
pub(crate) fn probe(parent: &Path) -> Result<()> {
    temporary_probe(|| {
        let name = OsString::from(unique_name(".agent-memory-probe-")?);
        let chain = DirectoryChain::open(parent).map_err(io_error)?;
        let probe = chain
            .directory()
            .create_directory(&name, 0o700)
            .map_err(io_error)?;
        let root = parent.join(&name);
        let exercised = (|| -> Result<()> {
            let left = probe
                .create_directory(OsStr::new("left"), 0o700)
                .map_err(io_error)?;
            let mut payload = left
                .create_exclusive(OsStr::new("payload"), libc::O_WRONLY, 0o600)
                .map_err(io_error)?;
            payload.write_all(b"left").map_err(io_error)?;
            probe
                .create_directory(OsStr::new("right"), 0o700)
                .map_err(io_error)?;
            raw_rename(&root.join("left"), &root.join("right"), true).map_err(io_error)?;
            raw_rename(&root.join("right"), &root.join("third"), false).map_err(io_error)
        })();
        let removed = remove_probe(&probe);
        let cleaned = removed.and_then(|()| {
            chain
                .directory()
                .remove_directory_at(&name)
                .map_err(io_error)
        });
        exercised?;
        cleaned
    })
}

fn remove_probe(probe: &Descriptor) -> Result<()> {
    for directory in ["left", "right", "third"] {
        let name = OsStr::new(directory);
        match probe.stat_at(name) {
            Ok(info) if info.is_directory() => {
                let child = probe.open_directory_at(name).map_err(io_error)?;
                match child.unlink_at(OsStr::new("payload")) {
                    Ok(()) => {}
                    Err(error) if error.raw_os_error() == Some(libc::ENOENT) => {}
                    Err(error) => return Err(io_error(error)),
                }
                probe.remove_directory_at(name).map_err(io_error)?;
            }
            Ok(_) => return Err(ContractError::new("Probe changed during cleanup").into()),
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

/// Write the symbolic link `path` -> `target` (staging step body).
pub(crate) fn symlink(path: &Path, target: &Path) -> Result<()> {
    let (parent, name) = split(path)?;
    let chain = DirectoryChain::open(parent).map_err(io_error)?;
    chain
        .directory()
        .symlink_at(name, target.as_os_str())
        .map_err(io_error)
}

pub(crate) fn canonical(path: &Path) -> Result<PathBuf> {
    Ok(fs::canonical_path(path, false)?)
}

/// Python `Path.is_relative_to`.
pub(crate) fn relative_to(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

pub(crate) fn mutate_contract<T>(kind: &str, action: impl FnOnce() -> Result<T>) -> Result<T> {
    Ok(mutate(kind, || action().map_err(contract))?)
}
