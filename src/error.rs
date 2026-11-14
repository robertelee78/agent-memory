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
    /// A validated canonical bank capacity observation.
    BankCapacity {
        code: crate::bank_capacity::CapacityCode,
        path: PathBuf,
        limit: usize,
        observed: usize,
        measurement: crate::bank_capacity::Measurement,
        unit: &'static str,
    },
    /// A local identity registry exceeded its bounded reader budget.
    LocalRegistryBound { limit: usize, observed: u64 },
    /// A value required as textual metadata is not UTF-8.
    NonUtf8 { what: String },
    /// A held canonical bank writer permits only bounded unwind-and-rediscover.
    CanonicalWriterBusy,
    /// An unpublished canonical writer owner permits bounded retry without authority.
    CanonicalWriterInitializing,
    /// Missing, conflicting or changed independently resolved identity.
    IdentityUnresolved,
    /// Canonical writer contention permits bounded rediscovery, never authority.
    SourceTransportBusy { initializing: bool },
    /// A required runtime provider has not been registered.
    MissingProvider,
    /// Known-dispute state refusal; preserves the baseline failure code and repair.
    HoldFailure { code: String, repair: String },
    /// Published source completion requires the registered T2c authority gate.
    SourcePublicationUnavailable,
    /// An incomplete bounded raw layout-recovery observation.
    RawRecoveryCapacity {
        dimension: String,
        limit: usize,
        measured: u64,
        exact: bool,
    },
    /// The optional derived recall catalog cannot qualify a reusable witness.
    RecallCatalogMiss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    kind: Kind,
    message: String,
}

pub type Result<T, E = ContractError> = std::result::Result<T, E>;

impl ContractError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Contract,
            message: message.into(),
        }
    }

    /// Only bank_capacity may construct this variant after validating its observation.
    pub(crate) fn bank_capacity(
        code: crate::bank_capacity::CapacityCode,
        path: PathBuf,
        limit: usize,
        observed: usize,
        measurement: crate::bank_capacity::Measurement,
        message: String,
    ) -> Self {
        Self {
            kind: Kind::BankCapacity {
                code,
                path,
                limit,
                observed,
                measurement,
                unit: code.unit(),
            },
            message,
        }
    }

    pub(crate) fn local_registry_bound(limit: usize, observed: u64) -> Self {
        Self {
            kind: Kind::LocalRegistryBound { limit, observed },
            message: "Local registry exceeds its reader bound; preserve it and inspect manually"
                .into(),
        }
    }

    pub(crate) fn non_utf8(what: &str) -> Self {
        Self {
            kind: Kind::NonUtf8 { what: what.into() },
            message: format!(
                "Non-UTF-8 {what} cannot be recorded; rename it or configure a UTF-8 value"
            ),
        }
    }

    pub(crate) fn canonical_writer_busy() -> Self {
        Self {
            kind: Kind::CanonicalWriterBusy,
            message: "Canonical writer busy; unwind and rediscover before private source work"
                .into(),
        }
    }

    pub(crate) fn canonical_writer_initializing() -> Self {
        Self {
            kind: Kind::CanonicalWriterInitializing,
            message: "Canonical writer owner is unpublished; bounded retry without authority"
                .into(),
        }
    }

    /// Same text as the prior runtime: `Unsafe lifecycle trust path: <path>: <reason>`.
    pub fn trust_path(path: &Path, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        let message = format!(
            "Unsafe lifecycle trust path: {}: {}",
            path.display(),
            reason
        );
        Self {
            kind: Kind::TrustPath {
                path: path.to_path_buf(),
                reason,
            },
            message,
        }
    }

    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn identity_unresolved(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::IdentityUnresolved,
            message: message.into(),
        }
    }

    pub(crate) fn source_transport_busy(initializing: bool) -> Self {
        Self {
            kind: Kind::SourceTransportBusy { initializing },
            message: "Canonical writer unavailable; preserve pending source work and rediscover"
                .into(),
        }
    }
    pub(crate) fn missing_provider(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::MissingProvider,
            message: message.into(),
        }
    }

    pub(crate) fn hold_failure(
        code: impl Into<String>,
        message: impl Into<String>,
        repair: impl Into<String>,
    ) -> Self {
        Self {
            kind: Kind::HoldFailure {
                code: code.into(),
                repair: repair.into(),
            },
            message: message.into(),
        }
    }

    pub(crate) fn missing_source_publication() -> Self {
        Self {
            kind: Kind::SourcePublicationUnavailable,
            message: "PublicationGate provider is not registered; publication remains pending"
                .into(),
        }
    }

    pub(crate) fn source_lock_busy(message: &str) -> Self {
        Self {
            kind: Kind::SourceTransportBusy {
                initializing: false,
            },
            message: message.into(),
        }
    }
    pub(crate) fn raw_recovery_capacity(
        dimension: &str,
        limit: usize,
        measured: u64,
        exact: bool,
    ) -> Self {
        Self {
            kind: Kind::RawRecoveryCapacity {
                dimension: dimension.to_owned(),
                limit,
                measured,
                exact,
            },
            message: "pending-recovery-capacity".to_owned(),
        }
    }

    pub(crate) fn recall_catalog_miss(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::RecallCatalogMiss,
            message: message.into(),
        }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ContractError {}
