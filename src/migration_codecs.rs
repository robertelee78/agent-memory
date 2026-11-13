//! Production codecs compiled into this runtime; no older or newer bank
//! format is shipped here.
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{object, string};
use crate::migration_contract::{Codecs, Converter, Validator};
use crate::{bank_contract, developer_identity};

/// The catalog names this runtime can execute.
pub struct Compiled;

impl Codecs for Compiled {
    fn validator(&self, name: &str) -> Option<Validator> {
        match name {
            "native" => Some(native),
            _ => None,
        }
    }

    fn converter(&self, _name: &str) -> Option<Converter> {
        None
    }
}

/// Validate a native bank file map. The preservation projection binds every
/// canonical byte; a future reviewed codec must produce the equivalent
/// projection for its own format.
pub fn native(files: &Value) -> Result<Value> {
    let checked = bank_contract::validate_files(files, None)?;
    let project = checked
        .get("project_id")
        .ok_or_else(|| ContractError::new("Validated bank reports no project identity"))?;
    let preservation = developer_identity::digest(Some(&developer_identity::encoded(files)?))
        .ok_or_else(|| ContractError::new("Bank preservation projection is unavailable"))?;
    Ok(object([
        ("project_id", project.clone()),
        ("preservation_sha256", string(preservation)),
    ]))
}
