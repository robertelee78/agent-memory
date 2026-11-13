//! Name resolution only; a resolved name grants no filesystem trust.
use crate::error::{ContractError, Result};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub fn canonical_path(path: &Path, strict: bool) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| ContractError::new(e.to_string()))?
            .join(path)
    };
    let mut pending: VecDeque<OsString> = absolute
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_owned()),
            Component::ParentDir => Some(OsString::from("..")),
            _ => None,
        })
        .collect();
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
                if links > 4096 {
                    return Err(ContractError::new(
                        "Symlink loop or excessive name resolution",
                    ));
                }
                let target = std::fs::read_link(&candidate)
                    .map_err(|e| ContractError::new(e.to_string()))?;
                if target.is_absolute() {
                    resolved = PathBuf::from("/");
                }
                let components: Vec<_> = target
                    .components()
                    .filter_map(|c| match c {
                        Component::Normal(s) => Some(s.to_owned()),
                        Component::ParentDir => Some(OsString::from("..")),
                        _ => None,
                    })
                    .collect();
                for component in components.into_iter().rev() {
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
