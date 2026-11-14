//! Adapters that connect a registration point to an implementer which
//! already exists as plain functions. They add no behaviour: each method
//! forwards to the baseline-named function of the implementing module.
use crate::codec::json::Value;
use crate::error::Result;
use std::path::Path;

/// `bank_transactions` asks `project_identity.status(repo)`.
pub(super) struct ProjectStatus;

impl crate::bank_transactions::ProjectIdentity for ProjectStatus {
    fn status(&self, repo: &Path) -> Result<Value> {
        crate::project_identity::status(repo)
    }
}

/// `project_identity` asks `bank_observation.markers(bank, lock, transaction)`.
pub(super) struct BankMarkers;

impl crate::project_identity::BankObservation for BankMarkers {
    fn markers(
        &self,
        bank: &Path,
        owned_lock: Option<&[u8]>,
        owned_transaction: Option<&[u8]>,
    ) -> Result<()> {
        crate::bank_observation::markers(bank, owned_lock, owned_transaction)
    }
}

/// `project_identity` reads `bank_writes.OPERATIONS`.
pub(super) struct WriterOperations;

impl crate::project_identity::BankWrites for WriterOperations {
    fn operations(&self) -> &[&str] {
        crate::bank_writes::OPERATIONS
    }
}
