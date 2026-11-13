//! Refusals shared by every domain. Messages are bounded contract text and
//! never carry memory bodies, credentials or remote URLs.

use std::fmt;
use std::path::{Path, PathBuf};

/// Why an operation was refused. Domains add variants; callers match on the
/// variant, never on message text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// A violated data or protocol contract.
    Contract,
    /// An unsafe lifecycle trust path: the path is preserved, not repaired.
    TrustPath { path: PathBuf, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    kind: Kind,
    message: String,
}

pub type Result<T, E = ContractError> = std::result::Result<T, E>;

impl ContractError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { kind: Kind::Contract, message: message.into() }
    }

    /// Same text as the prior runtime: `Unsafe lifecycle trust path: <path>: <reason>`.
    pub fn trust_path(path: &Path, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        let message = format!("Unsafe lifecycle trust path: {}: {}", path.display(), reason);
        Self { kind: Kind::TrustPath { path: path.to_path_buf(), reason }, message }
    }

    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ContractError {}
