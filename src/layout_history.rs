//! Historical receipt reads bind the existing checkout, not current bank health.
use crate::codec::json::Value;
use crate::developer_identity::{encoded, sha256_hex};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::host::Sources;
use crate::lifecycle::{get, object, string};
use crate::{
    identity_git, layout_recovery_state as state, local_registry as storage, project_identity,
    scope_binding,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const CORRUPT: &str = "Known recovery historical receipt is corrupt or request ID was reused";
fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| ContractError::new(CORRUPT))
}

/// `runtime_root` is explicit, as in layout_recovery_state::paths.
pub fn check(repo: &Path, scope: &Value, project_id: &str, runtime_root: &Path) -> Result<PathBuf> {
    let (root, _, _, _) = state::paths(repo, None, runtime_root)?;
    scope_binding::principal(&root, &[], None, None)?;
    let (_, _, lock) = storage::paths("projects")?;
    let parent = lock
        .parent()
        .ok_or_else(|| ContractError::new("Historical project registry is locked"))?;
    let name = lock
        .file_name()
        .ok_or_else(|| ContractError::new("Historical project registry is locked"))?;
    match DirectoryChain::open(parent) {
        Ok(chain) => {
            let present = match chain.directory().stat_at(name) {
                Ok(_) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(io_error(error)),
            };
            chain.recheck().map_err(io_error)?;
            if present {
                return Err(ContractError::new("Historical project registry is locked"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(io_error(error)),
    }
    let (registry, _) = project_identity::observe()?;
    let common = identity_git::git(
        &root,
        &[
            OsString::from("rev-parse"),
            OsString::from("--git-common-dir"),
        ],
        true,
        false,
    )
    .map_err(io_error)?;
    let location = match common {
        Some(common) => fs::canonical_path(&root.join(common), false)?,
        None => root.clone(),
    };
    let chain = DirectoryChain::open(&location).map_err(io_error)?;
    let info = chain.directory().stat().map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    let key = format!("{}:{}", info.dev, info.ino);
    let known = get(&registry, "checkouts").get(&key);
    let location = crate::host::text(location.as_os_str(), "historical checkout path")?;
    if get(scope, "checkout_key").as_str() != Some(&key)
        || known.is_none_or(|known| {
            get(known, "project_id").as_str() != Some(project_id)
                || !get(known, "locations").as_array().is_some_and(|locations| {
                    locations.iter().any(|item| item.as_str() == Some(location))
                })
        })
        || get(&registry, "projects").get(project_id).is_none()
    {
        return Err(ContractError::new(
            "Historical receipt belongs to a different or unregistered checkout",
        ));
    }
    Ok(root)
}

pub fn recovery_receipt(directory: &Path, preview: &Value, canonical: &Value) -> Result<Value> {
    recovery_receipt_with_sources(directory, preview, canonical, Sources::default())
}
pub fn recovery_receipt_with_sources(
    _directory: &Path,
    preview: &Value,
    canonical: &Value,
    sources: Sources,
) -> Result<Value> {
    Ok(object([
        (
            "schema",
            string("agent-memory/layout-known-recovery-receipt-v1"),
        ),
        ("operation_id", required(preview, "operation_id")?.clone()),
        ("recovery_id", required(preview, "recovery_id")?.clone()),
        ("recovery_sha256", string(sha256_hex(&encoded(preview)?))),
        (
            "canonical_receipt_sha256",
            string(sha256_hex(&encoded(canonical)?)),
        ),
        ("outcome", required(canonical, "outcome")?.clone()),
        ("verified", Value::Bool(true)),
        ("readback_at", string(storage::now(sources)?)),
    ]))
}

pub fn completed(directory: &Path, preview: &Value) -> Result<Option<Value>> {
    let recovery = required(preview, "recovery_id")?
        .as_str()
        .ok_or_else(|| ContractError::new(CORRUPT))?;
    // Callers validate their preview; never let a malformed direct call escape its directory.
    if recovery.is_empty() || recovery.contains('/') || recovery.contains('\0') {
        return Err(ContractError::new(CORRUPT));
    }
    let Some(receipt) = state::read(
        &directory
            .join("recovery")
            .join(format!("{recovery}.receipt.json")),
        crate::sharing_limits::MAX_PRIVATE,
        true,
    )?
    else {
        return Ok(None);
    };
    let canonical = state::read(
        &directory.join("receipt.json"),
        crate::sharing_limits::MAX_PRIVATE,
        false,
    )?
    .ok_or_else(|| ContractError::new(CORRUPT))?;
    if get(&receipt, "schema").as_str() != Some("agent-memory/layout-known-recovery-receipt-v1")
        || receipt.get("operation_id") != Some(required(preview, "operation_id")?)
        || receipt.get("recovery_id") != Some(required(preview, "recovery_id")?)
        || get(&receipt, "recovery_sha256").as_str() != Some(&sha256_hex(&encoded(preview)?))
        || get(&receipt, "canonical_receipt_sha256").as_str()
            != Some(&sha256_hex(&encoded(&canonical)?))
        || get(&receipt, "verified").as_bool() != Some(true)
        || get(&canonical, "verified").as_bool() != Some(true)
    {
        return Err(ContractError::new(CORRUPT));
    }
    Ok(Some(canonical))
}
