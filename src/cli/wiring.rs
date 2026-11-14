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

/// `turn_check` asks `scope_binding` for a live private-enrollment context;
/// the executing runtime root comes from the installed runtime location.
pub(super) struct EnrollmentScope;

impl crate::turn_check::ScopeBinding for EnrollmentScope {
    fn private_enrollment_context(
        &self,
        repo: &Path,
    ) -> crate::turn_check::CompletionResult<Box<dyn std::any::Any>> {
        let runtime_root = super::pack_root()?;
        let context = crate::scope_binding::private_enrollment_context(repo, &runtime_root)?;
        Ok(Box::new(context))
    }

    fn held(&self, context: &dyn std::any::Any) -> bool {
        context
            .downcast_ref::<crate::scope_binding::PrivateEnrollmentContext>()
            .is_some_and(crate::scope_binding::PrivateEnrollmentContext::held)
    }

    fn recheck_private_enrollment(
        &self,
        context: &dyn std::any::Any,
    ) -> crate::turn_check::CompletionResult<()> {
        let context = context
            .downcast_ref::<crate::scope_binding::PrivateEnrollmentContext>()
            .ok_or_else(|| {
                crate::error::ContractError::new(
                    "Private enrollment requires an internally constructed live context",
                )
            })?;
        crate::scope_binding::recheck_private_enrollment(context)?;
        Ok(())
    }
}

/// `installation` asks `private_state` for `activation_guard`,
/// `prepare_lifecycle`, `require_compatible` and `public_summary`. The current
/// repository is the process directory (baseline `Path.cwd()`); the runtime
/// root is the owned runtime pack the lifecycle activates, as for `updating`
/// and `uninstallation`. JSON null stands for the baseline `None`.
pub(super) struct LifecyclePrivateState;

fn lifecycle_roots() -> crate::installation::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let env = crate::installation::Environment::process();
    let runtime_root = crate::installation::locations(&env)?.runtime_pack();
    let current_repo = std::env::current_dir()?;
    Ok((current_repo, runtime_root))
}

fn present(value: &Value) -> Option<&Value> {
    (*value != Value::Null).then_some(value)
}

impl crate::installation::seams::PrivateState for LifecyclePrivateState {
    fn activation_guard(
        &self,
        installation: &Value,
        target: Option<&Value>,
        operation: &str,
        transition: &mut dyn FnMut() -> crate::installation::Result<Option<Value>>,
        body: &mut dyn FnMut(&Value) -> crate::installation::Result<Value>,
    ) -> crate::installation::Result<Value> {
        let (current_repo, runtime_root) = lifecycle_roots()?;
        crate::private_state::activation_guard(
            present(installation),
            target.and_then(present),
            operation,
            &current_repo,
            &runtime_root,
            Some(transition),
            body,
        )
    }

    fn prepare_lifecycle(&self, plan: &Value, check: bool) -> crate::installation::Result<Value> {
        let (_, runtime_root) = lifecycle_roots()?;
        let prepared = crate::private_state::prepare_lifecycle(plan, check, &runtime_root)?;
        Ok(prepared.unwrap_or(Value::Null))
    }

    fn require_compatible(
        &self,
        installation: &Value,
        target: &Value,
        operation: &str,
    ) -> crate::installation::Result<Value> {
        let (current_repo, runtime_root) = lifecycle_roots()?;
        let proof = crate::private_state::require_compatible(
            present(installation),
            present(target),
            operation,
            None,
            &current_repo,
            &runtime_root,
        )?;
        Ok(proof)
    }

    fn public_summary(&self, value: &Value) -> crate::installation::Result<Value> {
        Ok(crate::private_state::public_summary(value)?)
    }
}
