//! Name resolution only; a resolved name grants no filesystem trust.
use crate::error::{ContractError, Result};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Symlink expansions per resolution (Linux `MAXSYMLINKS`); a longer
/// non-looping chain is refused although CPython's `realpath` resolves it.
pub const MAX_SYMLINKS: usize = 40;
/// Queued name components; bounds a self-amplifying target such as `a -> a/a/…/a`.
pub const MAX_PENDING_COMPONENTS: usize = 131_072;

fn loop_refusal() -> ContractError {
    ContractError::new("Symlink loop or excessive name resolution")
}

fn components(path: &Path) -> Vec<OsString> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_owned()),
            Component::ParentDir => Some(OsString::from("..")),
            _ => None,
        })
        .collect()
}

pub fn canonical_path(path: &Path, strict: bool) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| ContractError::new(e.to_string()))?
            .join(path)
    };
    let mut pending: VecDeque<OsString> = components(&absolute).into();
    let mut resolved = PathBuf::from("/");
    let mut links = 0usize;
    while let Some(part) = pending.pop_front() {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            resolved.pop();
            continue;
        }
        let candidate = resolved.join(&part);
        match std::fs::symlink_metadata(&candidate) {
            Ok(meta) if meta.file_type().is_symlink() => {
                links += 1;
                if links > MAX_SYMLINKS {
                    return Err(loop_refusal());
                }
                let target = std::fs::read_link(&candidate)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                let expansion = components(&target);
                if pending.len() + expansion.len() > MAX_PENDING_COMPONENTS {
                    return Err(loop_refusal());
                }
                if target.is_absolute() {
                    resolved = PathBuf::from("/");
                }
                for component in expansion.into_iter().rev() {
                    pending.push_front(component);
                }
            }
            Ok(_) => resolved.push(part),
            Err(error) if strict => return Err(ContractError::new(error.to_string())),
            Err(_) => resolved.push(part),
        }
    }
    Ok(resolved)
}
