//! Adapter that connects installation's lifecycle registration point to
//! `private_state`. It adds no behaviour: each method forwards to the
//! baseline-named function of the implementing module.
use crate::codec::json::Value;

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
