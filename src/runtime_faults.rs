//! Lifecycle boundaries are transparent unless `dev-tools` is explicitly enabled.

#[cfg(feature = "dev-tools")]
mod dev;
#[cfg(feature = "dev-tools")]
pub use dev::*;

#[cfg(not(feature = "dev-tools"))]
use crate::codec::json::Value;
#[cfg(not(feature = "dev-tools"))]
use crate::error::{ContractError, Result};
#[cfg(not(feature = "dev-tools"))]
use std::path::PathBuf;

#[cfg(not(feature = "dev-tools"))]
#[inline(always)]
pub fn operation<T, E: From<ContractError>>(
    _name: &str,
    _paths: &[PathBuf],
    body: impl FnOnce() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    body()
}

#[cfg(not(feature = "dev-tools"))]
#[inline(always)]
pub fn temporary_probe<T>(body: impl FnOnce() -> T) -> T {
    body()
}

#[cfg(not(feature = "dev-tools"))]
#[inline(always)]
pub fn mutate<T, E: From<ContractError>>(
    _kind: &str,
    action: impl FnOnce() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    action()
}

#[cfg(not(feature = "dev-tools"))]
#[inline(always)]
pub fn response(_name: &str, _result: &Value) -> Result<()> {
    Ok(())
}
