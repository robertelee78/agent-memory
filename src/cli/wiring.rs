//! Adapter that connects installation's lifecycle registration point to
//! `private_state`. It adds no behaviour: each method forwards to the
//! baseline-named function of the implementing module.
use crate::codec::json::Value;
use std::path::Path;

/// `installation` asks `private_state` for `activation_guard`,
/// `prepare_lifecycle`, `require_compatible` and `public_summary`. The current
/// repository is the process directory; the runtime pair is independently
/// verified and retained by lifecycle planning. JSON null represents `None`.
pub(super) struct LifecyclePrivateState;

fn present(value: &Value) -> Option<&Value> {
    (*value != Value::Null).then_some(value)
}

impl crate::installation::seams::PrivateState for LifecyclePrivateState {
    fn activation_guard(
        &self,
        installation: &Value,
        target: Option<&Value>,
        operation: &str,
        runtime: (&Path, &Path),
        transition: &mut dyn FnMut() -> crate::installation::Result<Option<Value>>,
        body: &mut dyn FnMut(&Value) -> crate::installation::Result<Value>,
    ) -> crate::installation::Result<Value> {
        let current_repo = std::env::current_dir()?;
        crate::private_state::activation_guard(
            present(installation),
            target.and_then(present),
            operation,
            &current_repo,
            runtime,
            Some(transition),
            body,
        )
    }

    fn prepare_lifecycle(
        &self,
        plan: &Value,
        check: bool,
        runtime: (&Path, &Path),
    ) -> crate::installation::Result<Value> {
        let prepared = crate::private_state::prepare_lifecycle(plan, check, runtime)?;
        Ok(prepared.unwrap_or(Value::Null))
    }

    fn require_compatible(
        &self,
        installation: &Value,
        target: &Value,
        operation: &str,
        runtime: (&Path, &Path),
    ) -> crate::installation::Result<Value> {
        let current_repo = std::env::current_dir()?;
        let proof = crate::private_state::require_compatible(
            present(installation),
            present(target),
            operation,
            None,
            &current_repo,
            runtime,
        )?;
        Ok(proof)
    }

    fn public_summary(&self, value: &Value) -> crate::installation::Result<Value> {
        Ok(crate::private_state::public_summary(value)?)
    }
}
