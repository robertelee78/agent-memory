//! The `layout_recovery_publication` surface that layout recovery and
//! retention call. The three pure value helpers are defined here once (the
//! publication port reuses them); every operation that needs the publication
//! module itself goes through the [`LayoutPublication`] registration, and an
//! unregistered provider is a typed MissingProvider refusal.
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::layout_resolution::{err, has_keys, with};
use crate::lifecycle::{object, string};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Baseline `layout_recovery_publication.MAX_FILE` (the raw-source bound).
pub const MAX_FILE: usize = crate::layout_recovery_observation::MAX_FILE;

/// `layout_recovery_publication.operation_evidence` result:
/// `(root, bank, base, directory, terminal)`.
pub type OperationEvidence = (PathBuf, PathBuf, PathBuf, PathBuf, Value);

/// The layout_recovery_publication functions these modules call.
pub trait LayoutPublication: Send + Sync {
    /// Baseline `operation_evidence(repo, operation_id, owner=)`.
    fn operation_evidence(
        &self,
        repo: &Path,
        operation_id: &str,
        owner: Option<&[u8]>,
        runtime_root: &Path,
    ) -> Result<OperationEvidence>;
    /// Baseline `inspect(repo, operation_id, input)`.
    fn inspect(
        &self,
        repo: &Path,
        operation_id: &str,
        input: Option<&Value>,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// Baseline `prepare(repo, operation_id, input)`.
    fn prepare(
        &self,
        repo: &Path,
        operation_id: &str,
        input: &Value,
        runtime_root: &Path,
    ) -> Result<Value>;
    /// Baseline `land(repo, operation_id, input)`.
    fn land(
        &self,
        repo: &Path,
        operation_id: &str,
        input: &Value,
        runtime_root: &Path,
    ) -> Result<Value>;
}

static PUBLICATION: RwLock<Option<Arc<dyn LayoutPublication>>> = RwLock::new(None);

pub fn register_layout_publication(provider: Option<Arc<dyn LayoutPublication>>) {
    *PUBLICATION
        .write()
        .unwrap_or_else(|error| error.into_inner()) = provider;
}

pub(crate) fn layout_publication_provider() -> Result<Arc<dyn LayoutPublication>> {
    PUBLICATION
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or_else(|| {
            ContractError::missing_provider("LayoutPublication provider is not registered")
        })
}

/// Baseline `exact(value, fields, schema)`.
pub fn exact(value: &Value, fields: &[&str], schema: Option<&str>) -> Result<()> {
    let typed =
        schema.is_none_or(|schema| value.get("schema").and_then(Value::as_str) == Some(schema));
    if !has_keys(value, fields) || !typed {
        return Err(err(
            "Use the exact typed private recovery publication artifact",
        ));
    }
    Ok(())
}

/// Baseline `text(value, maximum=4096)`.
pub fn text(value: &Value, maximum: usize) -> Result<()> {
    let bounded = value
        .as_str()
        .is_some_and(|body| !crate::bank_contract::trim(body).is_empty() && body.len() <= maximum);
    if !bounded {
        return Err(err(
            "A bounded explicit source-preservation judgment is required",
        ));
    }
    Ok(())
}

/// Baseline `text` default bound.
pub const TEXT_MAXIMUM: usize = 4096;

/// Baseline `result(operation, operation_id, status, **values)`.
pub fn result(
    operation: &str,
    operation_id: &str,
    status: &str,
    values: Vec<(&'static str, Value)>,
) -> Value {
    let base = object([
        ("schema", string("agent-memory/layout-recovery-result-v1")),
        ("operation", string(operation)),
        ("operation_id", string(operation_id)),
        ("state", string(status)),
        ("publication_verified", Value::Bool(false)),
        ("verified", Value::Bool(!status.starts_with("pending"))),
        ("canonical_eligibility", string("unavailable")),
        ("semantic_recovery_complete", Value::Bool(false)),
        ("pending_unit_ids", Value::Array(Vec::new())),
        ("pending_units", Value::Int(0.into())),
        ("required_actions", Value::Array(Vec::new())),
        ("head_writes_performed", Value::Bool(false)),
        ("feature_writes_performed", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("original_index_writes_performed", Value::Bool(false)),
        ("history_writes_performed", Value::Bool(false)),
        ("private_artifact_writes_performed", Value::Bool(false)),
        ("feature_code_freshness_verified", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
        ("other_machine_import_verified", Value::Bool(false)),
    ]);
    with(base, values)
}
