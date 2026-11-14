//! Baseline `migration.main`: one typed request, one JSON document.
use super::plan::{apply, preview, proposal_value, restore_preview};
use super::recovery::{recover, recover_lock};
use super::{MAX_PLAN, Runtime, read_regular, require, result};
use crate::codec::json::{self, Value};
use crate::developer_identity::load_with_limit;
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, string, text};
use std::path::Path;

const ERROR_CHARACTERS: usize = 2048;
const WITHHELD: &str =
    "Migration source, code or recovery validation failed; unsafe details withheld.";
const REPAIR: &str = "Preserve the bank, visible marker, private intent/backups and retained trusted helper. Recover only the owned dead lock, then preview the exact finish/rollback or a new completed-restoration operation. Do not force replacement, rebind identity or initialize another bank.";

/// The parsed `migrate` subcommand: `preview`, `apply`, `recover`,
/// `recover-lock` or `restore`, with its declared options.
pub struct Request<'a> {
    pub command: &'a str,
    pub repo: &'a str,
    pub to: Option<&'a str>,
    pub proposal: Option<&'a str>,
    pub mode: Option<&'a str>,
    pub operation: Option<&'a str>,
}

/// A caller-supplied private JSON record (proposal or recovery preview).
pub fn input_value(path: &str) -> Result<Value> {
    let mut selected = crate::host::expanduser(Path::new(path))?;
    if !selected.is_absolute() {
        let cwd = std::env::current_dir().map_err(|error| ContractError::new(error.to_string()))?;
        selected = cwd.join(selected);
    }
    let parent = selected
        .parent()
        .ok_or_else(|| ContractError::new("Migration input has no parent directory"))?;
    let name = selected
        .file_name()
        .ok_or_else(|| ContractError::new("Migration input has no file name"))?;
    let parent = crate::fs::canonical_path(parent, true)?;
    let selected = parent.join(name);
    crate::fs::trust::record(&selected)?;
    let name = crate::host::text(name, "migration input")?;
    let (raw, _) = read_regular(&parent, name, MAX_PLAN)?;
    load_with_limit(&raw, MAX_PLAN)
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn run(runtime: Result<Runtime>, request: &Request<'_>) -> Result<Value> {
    let runtime = runtime?;
    let repo = Path::new(request.repo);
    let prepared = present(request.proposal).map(input_value).transpose()?;
    match request.command {
        "preview" => preview(&runtime, repo, request.to.unwrap_or_default()),
        "apply" => {
            let proposal = prepared
                .ok_or_else(|| ContractError::new("Migration apply requires --proposal"))?;
            apply(&runtime, &proposal, None)
        }
        "restore" => {
            if let Some(prepared) = prepared {
                let p = proposal_value(&prepared)?;
                let selected = crate::fs::canonical_path(repo, false)?;
                require(
                    text(&p, "kind") == Some("restore")
                        && Some(selected.as_path()) == text(&p, "repo_root").map(Path::new),
                    "Restore apply requires its own explicit proposal and selected repository",
                )?;
                apply(&runtime, &p, None)
            } else if let Some(operation) = present(request.operation) {
                restore_preview(&runtime, repo, operation)
            } else {
                Err(ContractError::new(
                    "Restore needs the original finished operation or its new restoration proposal",
                ))
            }
        }
        "recover-lock" => recover_lock(&runtime, repo, prepared.as_ref()),
        _ => recover(
            &runtime,
            repo,
            request.mode,
            request.operation,
            prepared.as_ref(),
        ),
    }
}

/// First `limit` characters, like Python `str[:limit]`.
fn truncated(message: &str, limit: usize) -> &str {
    match message.char_indices().nth(limit) {
        Some((end, _)) => message.get(..end).unwrap_or(message),
        None => message,
    }
}

/// Refusal text with any scan reason (or an unscannable message) is withheld.
fn safe_message(error: &ContractError) -> String {
    let message = truncated(&error.to_string(), ERROR_CHARACTERS).to_owned();
    let flagged = crate::security::scan(&string(message.as_str()), crate::security::LIMIT)
        .map(|scanned| {
            get(&scanned, "reasons")
                .as_array()
                .is_none_or(|reasons| !reasons.is_empty())
        })
        .unwrap_or(true);
    if flagged {
        WITHHELD.to_owned()
    } else {
        message
    }
}

fn failure(command: &str, request: &Request<'_>, error: &ContractError) -> Value {
    let may_have_changed = matches!(command, "apply" | "recover")
        || (command == "restore" && present(request.proposal).is_some());
    let mut value = result(
        command,
        vec![
            ("state", string("inspection-required")),
            ("verified", Value::Bool(false)),
            ("error", string(safe_message(error))),
            ("error_type", string("ContractError")),
            (
                "canonical_writes_performed",
                if may_have_changed {
                    Value::Null
                } else {
                    Value::Bool(false)
                },
            ),
            (
                "canonical_state_requires_inspection",
                Value::Bool(may_have_changed),
            ),
            ("repair", string(REPAIR)),
        ],
    );
    let details = crate::bank_capacity::details(error);
    if let Value::Object(fields) = &mut value {
        for (key, detail) in super::entries(&details) {
            match fields.iter_mut().find(|(name, _)| name == key) {
                Some((_, old)) => *old = detail.clone(),
                None => fields.push((key.clone(), detail.clone())),
            }
        }
    }
    value
}

/// Run one request under `runtime` and render the baseline output: indented
/// UTF-8 JSON and exit 0, or the compact inspection envelope and exit 1.
pub fn command(runtime: Result<Runtime>, request: &Request<'_>) -> CommandResult {
    match run(runtime, request) {
        Ok(value) => CommandResult::json(value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let value = failure(request.command, request, &error);
            CommandResult::json(value, 1, json::DEFAULT)
        }
    }
}
