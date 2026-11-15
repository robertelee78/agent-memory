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
    /// An I/O failure, retaining classification independently of rendered text.
    Io {
        kind: std::io::ErrorKind,
        raw_os_error: Option<i32>,
    },
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
    /// Baseline layout_recovery_temps.ProbeOwnershipError: an unproven metadata probe.
    ProbeOwnership { path: String },
    /// Fresh sharing authority or private observation cleanup failure.
    SharingAuthority {
        code: String,
        component: String,
        repair: String,
    },
    /// Protected admission input was unavailable, redirected or publicly mutable.
    PrivateInput,
    /// Source-bound semantic navigation failed without qualifying guidance.
    DiscoveryFailure { code: String, repair: String },
    /// Explicit private semantic artifact output refused without clobbering.
    PrivateOutput,
    /// Fixed, content-free quick refresh diagnostics.
    RefreshCurrent {
        code: String,
        component: String,
        repair: String,
        path: Option<String>,
    },
    /// A changed outer refresh preparation cannot authorize an inner proposal.
    RefreshPreparedEnvelope,
    /// Untracked memory has no verified complete imported-snapshot witness.
    UntrackedCanonicalMemory,
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

    /// Preserve I/O classification through bounded-reader and lock error wrappers.
    pub fn from_error(error: impl std::error::Error + 'static) -> Self {
        let mut source: &(dyn std::error::Error + 'static) = &error;
        loop {
            if let Some(contract) = source.downcast_ref::<Self>() {
                return contract.clone();
            }
            if let Some(io) = source.downcast_ref::<std::io::Error>() {
                return Self {
                    kind: Kind::Io {
                        kind: io.kind(),
                        raw_os_error: io.raw_os_error(),
                    },
                    message: error.to_string(),
                };
            }
            match source.source() {
                Some(next) => source = next,
                None => return Self::new(error.to_string()),
            }
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

    pub(crate) fn probe_ownership(path: impl Into<String>) -> Self {
        Self {
            kind: Kind::ProbeOwnership { path: path.into() },
            message: "pending-ambiguous-probe-ownership".to_owned(),
        }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ContractError {}

impl ContractError {
    pub(crate) fn sharing_authority(
        code: &str,
        component: &str,
        message: &str,
        repair: &str,
    ) -> Self {
        Self {
            kind: Kind::SharingAuthority {
                code: code.into(),
                component: component.into(),
                repair: repair.into(),
            },
            message: message.into(),
        }
    }
    pub(crate) fn with_message(mut self, message: String) -> Self {
        self.message = message;
        self
    }
    pub(crate) fn private_input(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::PrivateInput,
            message: message.into(),
        }
    }
    pub(crate) fn discovery_failure(code: &str, message: &str, repair: &str) -> Self {
        Self {
            kind: Kind::DiscoveryFailure {
                code: code.into(),
                repair: repair.into(),
            },
            message: message.into(),
        }
    }
}

impl ContractError {
    pub(crate) fn private_output(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::PrivateOutput,
            message: message.into(),
        }
    }
}

impl ContractError {
    pub(crate) fn refresh_current(
        code: &str,
        component: &str,
        message: &str,
        repair: &str,
        path: Option<String>,
    ) -> Self {
        Self {
            kind: Kind::RefreshCurrent {
                code: code.into(),
                component: component.into(),
                repair: repair.into(),
                path,
            },
            message: message.into(),
        }
    }

    pub(crate) fn refresh_prepared_envelope() -> Self {
        Self {
            kind: Kind::RefreshPreparedEnvelope,
            message: "Prepared refresh report metadata is inconsistent with its retained proposal."
                .into(),
        }
    }

    pub(crate) fn untracked_canonical_memory() -> Self {
        Self {
            kind: Kind::UntrackedCanonicalMemory,
            message: "Locally untracked canonical memory requires full review.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContractError, Kind};
    use crate::fs::{LockError, ReadError};
    use std::io;

    #[test]
    fn io_classification_preserves_errno_kind_and_rendering_through_wrappers() {
        for errno in [
            libc::ENOENT,
            libc::EEXIST,
            libc::ENOTDIR,
            libc::ELOOP,
            libc::EXDEV,
            libc::ENOSPC,
            libc::EACCES,
        ] {
            let expected = io::Error::from_raw_os_error(errno);
            let kind = Kind::Io {
                kind: expected.kind(),
                raw_os_error: Some(errno),
            };
            for error in [
                ContractError::from_error(io::Error::from_raw_os_error(errno)),
                ContractError::from_error(ReadError::Io(io::Error::from_raw_os_error(errno))),
                ContractError::from_error(LockError::Io(io::Error::from_raw_os_error(errno))),
            ] {
                assert_eq!(error.kind(), &kind);
                assert_eq!(error.message(), expected.to_string());
                assert_eq!(ContractError::from_error(error.clone()), error);
            }
        }
    }

    #[test]
    fn io_kind_without_errno_does_not_depend_on_message() {
        let error = ContractError::from_error(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "localized collision",
        ));
        assert_eq!(
            error.kind(),
            &Kind::Io {
                kind: io::ErrorKind::AlreadyExists,
                raw_os_error: None,
            }
        );
        assert_eq!(error.message(), "localized collision");
        let refusal = ContractError::new("File exists (os error 17)");
        assert_eq!(refusal.kind(), &Kind::Contract);
        assert_eq!(ContractError::from_error(refusal.clone()), refusal);
    }
}
