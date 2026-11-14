//! Compact refresh and explicit source-assessment orchestration.
use crate::bank_capacity;
use crate::codec::json::{self, Value};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

pub const SCHEMA: &str = "agent-memory/turn-check-v1";
const NOTICE_SCHEMA: &str = "agent-memory/refresh-notice-v1";
const NOTICE_LIMIT: usize = 4096;

/// Contract refusals retain their reviewed reason; ordinary unavailable input
/// and I/O failures must never expose arbitrary exception text.
#[derive(Debug, Clone)]
pub enum CompletionError {
    Contract(ContractError),
    Unexpected,
}
pub type CompletionResult<T> = std::result::Result<T, CompletionError>;
impl From<ContractError> for CompletionError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
impl From<CompletionError> for ContractError {
    fn from(error: CompletionError) -> Self {
        match error {
            CompletionError::Contract(error) => error,
            CompletionError::Unexpected => {
                ContractError::new("Original source or private assessment state is unavailable.")
            }
        }
    }
}

/// Implemented by memory_refresh.
pub trait MemoryRefresh: Send + Sync {
    fn current_result(&self, repo: &Path) -> CompletionResult<Value>;
    /// Baseline `memory_refresh.local_prerequisites(repo)` (used by diagnostics):
    /// the observed prerequisites, or its typed `CurrentFailure` refusal.
    fn local_prerequisites(
        &self,
        repo: &Path,
    ) -> Result<std::result::Result<Value, CurrentFailure>>;
}
static REFRESH: OnceLock<Box<dyn MemoryRefresh>> = OnceLock::new();
pub fn register_memory_refresh(value: Box<dyn MemoryRefresh>) -> Result<()> {
    REFRESH
        .set(value)
        .map_err(|_| ContractError::new("memory_refresh is already registered"))
}
fn refresh_backend() -> Result<&'static dyn MemoryRefresh> {
    REFRESH
        .get()
        .map(Box::as_ref)
        .ok_or_else(|| ContractError::new("memory_refresh is not registered"))
}

pub fn requests(value: &Value) -> Result<Vec<Value>> {
    let values = if get(value, "schema").as_str() == Some("agent-memory/turn-check-request-v1") {
        let invalid = || {
            ContractError::new(
                "Use one to 128 exact source assessments within the bounded compact request",
            )
        };
        let pairs = value.as_object().ok_or_else(invalid)?;
        let values = value
            .get("assessments")
            .and_then(Value::as_array)
            .filter(|values| (1..=128).contains(&values.len()))
            .ok_or_else(invalid)?;
        if pairs.len() != 2
            || pairs
                .iter()
                .any(|(key, _)| key != "schema" && key != "assessments")
        {
            return Err(invalid());
        }
        if values.iter().any(|source| {
            source
                .get("observation_id")
                .and_then(Value::as_str)
                .is_none()
        }) {
            return Err(ContractError::new(
                "A batch requires distinct local observation IDs",
            ));
        }
        let mut ids = HashSet::new();
        for source in values {
            let id = source
                .get("observation_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ContractError::new("A batch requires distinct local observation IDs")
                })?;
            if !ids.insert(id) {
                return Err(ContractError::new(
                    "Do not repeat an observation inside one compact request",
                ));
            }
        }
        let first = values.first().ok_or_else(invalid)?;
        if values.iter().any(|source| {
            !crate::client_hooks::membership_equal(get(source, "session"), get(first, "session"))
                || !crate::client_hooks::membership_equal(
                    get(source, "prompt"),
                    get(first, "prompt"),
                )
        }) {
            return Err(ContractError::new(
                "A compact batch must retain one actual session and lifecycle; recover earlier contexts separately",
            ));
        }
        values.to_vec()
    } else {
        vec![value.clone()]
    };
    if values.iter().any(|source| {
        source.as_object().is_none() || get(source, "outcome").as_str() != Some("no-learning")
    }) {
        return Err(ContractError::new(
            "The compact check accepts explicit no-learning assessments only; use the governed publication route for durable learning",
        ));
    }
    Ok(values)
}
fn synchronized(refresh: &Value) -> bool {
    get(refresh, "sharing_state").as_str() == Some("bank-synchronized")
        && get(refresh, "remote_freshness_verified") == &Value::Bool(true)
}
fn hash(value: &Value, style: json::Style) -> Result<String> {
    let bytes = json::encode(value, style)?;
    Ok(crate::validation_memo::hex(&Sha256::digest(
        bytes.as_bytes(),
    )))
}

/// The cache is advisory: storage failures cannot suppress an actionable notice.
pub fn notice_required(
    repo: &Path,
    harness: &str,
    session: &Value,
    refresh: &Value,
    failures: &[Value],
) -> Result<bool> {
    let Some(session) = session.as_str().filter(|s| !s.is_empty() && s.len() <= 512) else {
        return Ok(true);
    };
    let mut action = |store: &mut crate::turn_sources::NoticeStore<'_>| -> Result<bool> {
        let old = store.read(NOTICE_LIMIT)?;
        if let Some(old) = &old {
            if old.as_object().map_or(true, |p| p.len() != 2)
                || get(old, "schema").as_str() != Some(NOTICE_SCHEMA)
                || old.get("fingerprint").is_none()
            {
                return Ok(true);
            }
        }
        if synchronized(refresh) && failures.is_empty() {
            if old.is_some() {
                store.remove()?;
            }
            return Ok(true);
        }
        let cause_refresh = if synchronized(refresh) {
            Value::Null
        } else {
            object(
                [
                    "sharing_state",
                    "failure_component",
                    "failure_code",
                    "failure_path",
                    "reason",
                    "repair",
                ]
                .into_iter()
                .map(|key| (key, get(refresh, key).clone())),
            )
        };
        let cause = object([
            ("refresh", cause_refresh),
            ("source_assessments", Value::Array(failures.to_vec())),
        ]);
        let value = object([
            ("schema", string(NOTICE_SCHEMA)),
            (
                "fingerprint",
                string(hash(&cause, json::SORTED_COMPACT_UTF8)?),
            ),
        ]);
        if old.as_ref() == Some(&value) {
            return Ok(false);
        }
        store.replace(&value, NOTICE_LIMIT)?;
        Ok(true)
    };
    Ok(crate::turn_sources::with_notice_store(repo, harness, session, &mut action).unwrap_or(true))
}

pub fn check(
    repo: &Path,
    value: &Value,
    complete: impl Fn(&Path, &Value) -> CompletionResult<Value>,
    harness: &str,
) -> Result<Value> {
    let values = requests(value)?;
    // Resolve the registration before any completion can mutate private state.
    let refresh_api = refresh_backend()?;
    let runtime_root = crate::cli::pack_root()?;
    let context = crate::scope_binding::private_enrollment_context(repo, &runtime_root)?;
    let held = context.held();
    let refresh = if held {
        object([
            ("sharing_state", string("recovery-pending")),
            ("active", Value::Bool(false)),
            ("remote_freshness_verified", Value::Bool(false)),
            ("failure_component", string("layout-recovery")),
            ("failure_code", string("incomplete-recovery")),
            (
                "reason",
                string(
                    "A verified private recovery hold keeps refresh and source acknowledgement inactive.",
                ),
            ),
            (
                "repair",
                string(
                    "Continue the human task; retain original pending observations and inspect am bank recover-layout status.",
                ),
            ),
            ("canonical_writes_performed", Value::Bool(false)),
        ])
    } else {
        refresh_api.current_result(repo)?
    };
    if refresh.as_object().is_none() {
        return Err(ContractError::new(
            "The refresh observation is unavailable; preserve original sources",
        ));
    }
    let mut receipts = Vec::with_capacity(values.len());
    for source in &values {
        let receipt = if held {
            Err(CompletionError::Contract(ContractError::new(
                "Recovery-pending: the original source is retained; acknowledgement is inactive, not no-learning or published",
            )))
        } else {
            complete(repo, source)
        };
        receipts.push(match receipt {
            Ok(value) => value,
            Err(error) => {
                let error = ContractError::from(error);
                let mut pairs = vec![("verified".into(), Value::Bool(false)), ("outcome".into(), string("pending")),
                    ("failure_component".into(), string("source-assessment")), ("failure_code".into(), string("assessment-unverified")),
                    ("reason".into(), string(error.message())),
                    ("repair".into(), string("Preserve the original observation and inspect the matching boundary pending inventory before retrying.")),
                    ("canonical_writes_performed".into(), Value::Bool(false))];
                if let Value::Object(details) = bank_capacity::details(&error) { pairs.extend(details); }
                Value::Object(pairs)
            }
        });
    }
    let assessment = if receipts.len() == 1 {
        receipts
            .first()
            .cloned()
            .ok_or_else(|| ContractError::new("Missing source assessment"))?
    } else {
        object([
            (
                "verified",
                Value::Bool(
                    receipts
                        .iter()
                        .all(|r| get(r, "verified") == &Value::Bool(true)),
                ),
            ),
            ("results", Value::Array(receipts.clone())),
        ])
    };
    let mut failures = Vec::new();
    for (source, receipt) in values.iter().zip(&receipts) {
        if get(receipt, "verified") == &Value::Bool(true) {
            continue;
        }
        let source_id = source
            .get("observation_id")
            .or_else(|| source.get("source_uuid"))
            .or_else(|| source.get("prompt"))
            .cloned()
            .unwrap_or(Value::Null);
        failures.push(object([
            ("source_id", source_id),
            (
                "request_sha256",
                string(hash(source, json::SORTED_COMPACT_ASCII)?),
            ),
            (
                "failure_code",
                receipt
                    .get("failure_code")
                    .cloned()
                    .unwrap_or_else(|| string("assessment-unverified")),
            ),
            ("reason", get(receipt, "reason").clone()),
        ]));
    }
    let mut result = combined(refresh.clone(), assessment);
    let notice = notice_required(
        repo,
        harness,
        get(
            values
                .first()
                .ok_or_else(|| ContractError::new("Missing source assessment"))?,
            "session",
        ),
        &refresh,
        &failures,
    )?;
    if let Value::Object(pairs) = &mut result {
        pairs.push(("assessment_count".into(), Value::Int(values.len().into())));
        pairs.push(("notice_required".into(), Value::Bool(notice)));
    }
    crate::scope_binding::recheck_private_enrollment(&context)?;
    Ok(result)
}

pub fn combined(refresh: Value, assessment: Value) -> Value {
    let refreshed = synchronized(&refresh);
    let assessed = get(&assessment, "verified") == &Value::Bool(true);
    object([
        ("schema", string(SCHEMA)),
        ("operation", string("turn-check")),
        (
            "verification_scope",
            string("mechanical-refresh-and-source-assessment; not semantic completeness"),
        ),
        ("verified", Value::Bool(refreshed && assessed)),
        (
            "outcome",
            string(if refreshed && assessed {
                "finished"
            } else if !refreshed {
                "pending-refresh"
            } else {
                "pending-assessment"
            }),
        ),
        ("refresh_verified", Value::Bool(refreshed)),
        ("assessment_verified", Value::Bool(assessed)),
        ("refresh", refresh),
        ("assessment", assessment),
        ("admission_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
    ])
}
pub fn exit_code(result: &Value) -> i32 {
    if get(result, "schema").as_str() == Some(SCHEMA) {
        i32::from(get(result, "verified") != &Value::Bool(true))
    } else {
        i32::from(get(result, "verified") == &Value::Bool(false))
    }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Int(n) => n.to_string() != "0",
        Value::Float(n) => *n != 0.0,
        _ => true,
    }
}
fn display(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Ok("None".into()),
        Value::Bool(b) => Ok(if *b { "True" } else { "False" }.into()),
        _ => json::encode(v, json::DEFAULT),
    }
}
fn default_field(value: &Value, key: &str, default: &str) -> Result<String> {
    value
        .get(key)
        .map(display)
        .transpose()
        .map(|value| value.unwrap_or_else(|| default.to_owned()))
}
pub fn failure_notice(result: &Value) -> Result<String> {
    let mut notices = Vec::new();
    let is_check = get(result, "schema").as_str() == Some(SCHEMA);
    if is_check && !truthy(get(result, "refresh_verified")) {
        let refresh = get(result, "refresh");
        let component = default_field(refresh, "failure_component", "refresh")?;
        let failure = refresh
            .get("failure_code")
            .or_else(|| refresh.get("sharing_state"))
            .map(display)
            .transpose()?
            .unwrap_or_else(|| "unverified".into());
        let reason = if truthy(get(refresh, "reason")) {
            display(get(refresh, "reason"))?
        } else {
            "Shared memory needs source-bound review before it is current.".into()
        };
        let repair = if truthy(get(refresh, "repair")) {
            display(get(refresh, "repair"))?
        } else {
            "Preserve work; inspect the refresh result and complete the required review.".into()
        };
        notices.push(format!(
            "memory refresh pending [{component}/{failure}]: {reason} {repair}"
        ));
    }
    if !is_check || !truthy(get(result, "assessment_verified")) {
        let assessment = result.get("assessment").unwrap_or(result);
        let assessment = assessment
            .get("results")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|v| get(v, "verified") != &Value::Bool(true))
            })
            .unwrap_or(assessment);
        notices.push(format!(
            "source assessment pending: {} {}",
            default_field(assessment, "reason", "Original evidence is unavailable.")?,
            default_field(
                assessment,
                "repair",
                "Preserve evidence and inspect the matching boundary operation."
            )?
        ));
    }
    Ok(format!("am: {}", notices.join("; ").replace('\n', " ")))
}
/// Return the exit status and exact stdout/stderr, leaving I/O to the CLI.
pub fn emit(result: &Value, quiet: bool) -> Result<(i32, String, String)> {
    let code = exit_code(result);
    let stdout = if quiet {
        String::new()
    } else {
        format!("{}\n", json::encode(result, json::DEFAULT)?)
    };
    let stderr = if quiet && code != 0 && get(result, "notice_required") != &Value::Bool(false) {
        format!("{}\n", failure_notice(result)?)
    } else {
        String::new()
    };
    Ok((code, stdout, stderr))
}

/// Baseline `memory_refresh.CurrentFailure`: a bounded local-prerequisite
/// refusal (`code`, message, `repair`, optional `path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentFailure {
    pub code: String,
    pub message: String,
    pub repair: String,
    pub path: Option<String>,
}

/// The registered memory_refresh provider (diagnostics' local prerequisites).
pub(crate) fn memory_refresh_provider() -> Result<&'static dyn MemoryRefresh> {
    refresh_backend()
}
