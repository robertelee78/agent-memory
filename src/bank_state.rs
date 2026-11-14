//! Canonical current Markdown and optional, read-only local Git facts.
use crate::bank_capacity;
use crate::bank_contract as contract;
use crate::bank_writes as writes;
use crate::codec::json::{self, Value};
use crate::developer_identity::digest;
use crate::error::{ContractError, Result};
use crate::identity_git;
use crate::lifecycle::{CommandResult, get, object, string};
use crate::process::{self, Input, Output, Refusal, Spec, Status, Target};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub const MAX_GIT_CONTEXT_OUTPUT: usize = 16 * 1024 * 1024;
pub const GIT_CONTEXT_TIMEOUT: Duration = Duration::from_secs(10);
/// Recall's retrieval implementation resolves checkout context from fresh evidence.
/// Shared upward seam: comparison/layout/frontier callers extend this one definition.
pub trait ComparisonView: Send + Sync {
    /// Baseline retrieval.observed_context(root, request).
    fn observed_context(&self, root: &Path, request: &Value) -> Result<(Value, Value)>;
    /// Baseline recall_catalog.invalidate_frontier_transition.
    fn invalidate_frontier_transition(&self, repo: &Path) -> Result<String>;
    /// Baseline comparison.safe_text with the caller's fixed review bound.
    fn safe_text(&self, value: &Value, maximum: usize) -> Result<()>;
}
static COMPARISON: RwLock<Option<Arc<dyn ComparisonView>>> = RwLock::new(None);
pub fn register_comparison_view(provider: Option<Arc<dyn ComparisonView>>) {
    *COMPARISON
        .write()
        .unwrap_or_else(|error| error.into_inner()) = provider;
}
pub(crate) fn comparison_view_provider() -> Result<Arc<dyn ComparisonView>> {
    COMPARISON
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or_else(|| err("ComparisonView provider is not registered"))
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn unavailable(reason: &str) -> Value {
    object([
        ("status", string("unavailable")),
        ("reason", string(reason)),
    ])
}
fn context_unavailable() -> Value {
    unavailable("Local Git inspection unavailable; current Markdown remains the source")
}
fn oid_valid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn git_run(
    root: &Path,
    args: &[&str],
    environment: &[(OsString, OsString)],
) -> std::result::Result<Output, process::ProcessError> {
    let args: Vec<_> = args.iter().map(OsString::from).collect();
    let invocation =
        identity_git::invocation_with_env(root, &args, false, environment.iter().cloned());
    let spec = Spec {
        argv: &invocation.argv,
        env: &invocation.env,
        cwd: None,
        search_path: true,
        stdin: Input::Inherit,
        stdout: Target::Capture {
            limit: MAX_GIT_CONTEXT_OUTPUT,
        },
        stderr: Target::Capture {
            limit: MAX_GIT_CONTEXT_OUTPUT,
        },
        deadline: crate::clock::Deadline::from_absolute_ns(0),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    process::run_for(&spec, GIT_CONTEXT_TIMEOUT)
}
pub fn git_context(root: &Path, observed: &Value) -> Result<Value> {
    git_context_with_environment(root, observed, &std::env::vars_os().collect::<Vec<_>>())
}
pub fn git_context_with_environment(
    root: &Path,
    observed: &Value,
    environment: &[(OsString, OsString)],
) -> Result<Value> {
    let top = match git_run(root, &["rev-parse", "--show-toplevel"], environment) {
        Ok(output) => output,
        Err(error)
            if error.reason == Refusal::Spawn
                && error
                    .source
                    .as_ref()
                    .is_some_and(|source| source.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(unavailable(
                "Git is not installed; current Markdown remains readable",
            ));
        }
        Err(_) => return Ok(context_unavailable()),
    };
    if top.status != Status::Code(0) {
        return Ok(object([
            ("status", string("not-discovered")),
            (
                "reason",
                string("No usable Git checkout was discovered; no Git audit claim"),
            ),
        ]));
    }
    let top_text = match std::str::from_utf8(&top.stdout) {
        Ok(text) => text.strip_suffix('\n').unwrap_or(text),
        Err(_) => return Ok(context_unavailable()),
    };
    let top_root = match crate::fs::canonical_path(Path::new(top_text), false) {
        Ok(path) => path,
        Err(_) => return Ok(context_unavailable()),
    };
    if top_root != root {
        return Ok(object([
            ("status", string("different-root")),
            (
                "reason",
                string("Selected bank root is not the Git checkout root; resolve explicitly"),
            ),
        ]));
    }
    let head = match git_run(root, &["rev-parse", "--verify", "HEAD"], environment) {
        Ok(output) => output,
        Err(_) => return Ok(context_unavailable()),
    };
    let oid = if head.status == Status::Code(0) {
        let text = match std::str::from_utf8(&head.stdout) {
            Ok(text) if text.is_ascii() => text.trim(),
            _ => return Ok(context_unavailable()),
        };
        if !oid_valid(text) {
            return Err(err("Invalid Git context reference"));
        }
        string(text)
    } else {
        Value::Null
    };
    let bank_relative = crate::repository_config::relative(root)?;
    let index = match git_run(
        root,
        &["ls-files", "--stage", "-z", "--", &bank_relative],
        environment,
    ) {
        Ok(output) => output,
        Err(_) => return Ok(context_unavailable()),
    };
    if index.status != Status::Code(0) {
        return Ok(unavailable(
            "Git index could not be inspected; no Git audit claim",
        ));
    }
    let Some(observed) = observed.as_object() else {
        return Ok(context_unavailable());
    };
    let prefix = bank_relative + "/";
    let mut entries = Vec::new();
    let mut indexed = BTreeSet::new();
    let mut unmerged = BTreeSet::new();
    for record in index
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            return Ok(context_unavailable());
        };
        let Some(metadata) = record
            .get(..tab)
            .and_then(|raw| std::str::from_utf8(raw).ok())
            .filter(|text| text.is_ascii())
        else {
            return Ok(context_unavailable());
        };
        let Some(path) = record
            .get(tab + 1..)
            .and_then(|raw| std::str::from_utf8(raw).ok())
        else {
            return Ok(context_unavailable());
        };
        let mut fields = metadata.split_whitespace();
        let (Some(mode), Some(object_id), Some(stage), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Ok(context_unavailable());
        };
        let stage = match stage.parse::<i64>() {
            Ok(stage) => stage,
            Err(_) => return Ok(context_unavailable()),
        };
        let Some(path) = path.strip_prefix(&prefix) else {
            continue;
        };
        if !observed.iter().any(|(name, _)| name == path) {
            continue;
        }
        entries.push(object([
            ("path", string(path)),
            ("mode", string(mode)),
            ("object_id", string(object_id)),
            ("stage", Value::Int(stage.into())),
        ]));
        if stage == 0 {
            indexed.insert(path.to_owned());
        } else {
            unmerged.insert(path.to_owned());
        }
    }
    let not_indexed: BTreeSet<_> = observed
        .iter()
        .map(|(path, _)| path.clone())
        .filter(|path| !indexed.contains(path))
        .collect();
    Ok(object([
        ("status", string("checkout")),
        ("context_head", oid),
        ("index_entries", Value::Array(entries)),
        (
            "indexed_paths",
            Value::Array(indexed.into_iter().map(string).collect()),
        ),
        (
            "not_indexed_paths",
            Value::Array(not_indexed.into_iter().map(string).collect()),
        ),
        (
            "unmerged_paths",
            Value::Array(unmerged.into_iter().map(string).collect()),
        ),
        ("current_bytes_committed", string("not-assessed")),
    ]))
}
pub fn inspect(repo: &Path, path: Option<&str>) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    let provider = writes::recovery_frontier_provider()?;
    let frontier = provider.observe(&root)?;
    let (checked, observed) = writes::snapshot(&root, None)?;
    if let Some(path) = path {
        return inspect_context(&root, &bank, &checked, &observed, path);
    }
    let git = git_context(&root, &observed)?;
    let (latest, hashes) = writes::snapshot(&root, None)?;
    if latest != checked || hashes != observed || git_context(&root, &hashes)? != git {
        return Err(err(
            "Bank or local Git context changed during inspection; reread before use",
        ));
    }
    provider.check(&frontier, &root, None, None)?;
    let routes = get(&checked, "routes")
        .as_array()
        .ok_or_else(|| err("Invalid canonical route inventory"))?;
    let contexts = object(contract::CONTEXTS.iter().map(|name| {
        let route = routes
            .iter()
            .find(|route| get(route, "path").as_str() == Some(*name))
            .cloned()
            .unwrap_or(Value::Null);
        (
            *name,
            object([
                ("present", Value::Bool(observed.get(name).is_some())),
                ("route", route),
                (
                    "omission_reason",
                    get(get(&checked, "omitted"), name).clone(),
                ),
            ]),
        )
    }));
    let mut value = object([
        ("schema", string("agent-memory/current-bank-view-v1")),
        ("operation", string("inspect")),
    ]);
    contract::merge(
        &mut value,
        &contract::recovery_coverage_provider()?.public_coverage(&frontier)?,
    )?;
    let operation_ids = get(&frontier, "operation_ids")
        .as_array()
        .ok_or_else(|| err("Invalid recovery frontier operation inventory"))?;
    contract::merge(
        &mut value,
        &object([
            ("project_id", get(&checked, "project_id").clone()),
            ("canonical_source", string("Markdown")),
            ("contexts", contexts),
            ("routes", get(&checked, "routes").clone()),
            ("source_hashes", observed),
            ("git", git),
            (
                "sharing",
                object([
                    ("state", string("local-or-pending-sharing")),
                    ("publication_verified", Value::Bool(false)),
                    ("freshness_verified", Value::Bool(false)),
                ]),
            ),
            (
                "reads_memory_generated_state",
                Value::Bool(!operation_ids.is_empty()),
            ),
            ("writes_performed", Value::Bool(false)),
            (
                "validation_scope",
                string("current-file-inventory-and-local-git-context"),
            ),
        ]),
    )?;
    Ok(value)
}
pub fn inspect_context(
    root: &Path,
    bank: &Path,
    checked: &Value,
    observed: &Value,
    path: &str,
) -> Result<Value> {
    let provider = writes::recovery_frontier_provider()?;
    let frontier = provider.observe(root)?;
    if !contract::CONTEXTS.contains(&path) || observed.get(path).is_none() {
        return Err(err(
            "Explicit historical inspection selects an existing ordinary context file only",
        ));
    }
    let scope = provider.require_scope(root, get(checked, "project_id"), None)?;
    let comparison = comparison_view_provider()?;
    let request = Value::Object(Vec::new());
    let (context, _) = comparison.observed_context(root, &request)?;
    let raw = crate::bank_observation::read_source(bank, path, crate::sharing_limits::MAX_FILE)
        .map_err(|error| {
            if matches!(error.kind(), crate::error::Kind::BankCapacity { .. }) {
                err("Context is oversized or changed during inspection")
            } else {
                error
            }
        })?;
    if digest(Some(&raw)).as_deref() != get(observed, path).as_str() {
        return Err(err("Context is oversized or changed during inspection"));
    }
    let body = std::str::from_utf8(&raw).map_err(|_| err("Canonical context must be UTF-8"))?;
    let scanned = crate::repository_config::security_scan_provider()?.scan(body)?;
    let mut value = object([
        ("schema", string("agent-memory/context-inspection-v1")),
        ("operation", string("inspect")),
    ]);
    contract::merge(
        &mut value,
        &contract::recovery_coverage_provider()?.public_coverage(&frontier)?,
    )?;
    contract::merge(
        &mut value,
        &object([
            ("project_id", get(checked, "project_id").clone()),
            ("inspection_class", string("historical-orientation")),
            ("assignment", Value::Bool(false)),
            ("usable_as_current_guidance", Value::Bool(false)),
            ("source_assertions_verified", Value::Bool(false)),
            ("identity_authenticated", Value::Bool(false)),
            ("publication_verified", Value::Bool(false)),
            ("writes_performed", Value::Bool(false)),
        ]),
    )?;
    let reasons = get(&scanned, "reasons")
        .as_array()
        .ok_or_else(|| err("Invalid security scan reasons"))?;
    if !reasons.is_empty() {
        contract::merge(
            &mut value,
            &object([
                ("withheld", Value::Bool(true)),
                ("reasons", Value::Array(reasons.to_vec())),
            ]),
        )?;
    } else {
        let assessment = if path == "activeContext.md" {
            crate::handoff::assess(body, get(checked, "project_id"), &context, root, bank, true)?
        } else {
            Value::Null
        };
        contract::merge(
            &mut value,
            &object([
                ("path", string(path)),
                ("body", string(body)),
                ("source_sha256", get(observed, path).clone()),
                ("untrusted_data", Value::Bool(true)),
                ("trust", Value::Null),
                ("context_assessment", assessment),
            ]),
        )?;
    }
    if writes::snapshot(root, None)?.1 != *observed
        || comparison.observed_context(root, &request)?.0 != context
    {
        return Err(err("Bank or checkout changed during historical inspection"));
    }
    provider.require_scope(root, get(checked, "project_id"), Some(&scope))?;
    provider.check(&frontier, root, None, None)?;
    Ok(value)
}
pub fn command_inspect(repo: &Path, path: Option<&str>) -> CommandResult {
    command_result(inspect(repo, path))
}
pub fn command_result(result: Result<Value>) -> CommandResult {
    let (value, exit_code, style) = match result {
        Ok(value) => (value, 0, json::INDENT2_ASCII),
        Err(error) => {
            let mut value = object([
                ("schema", string("agent-memory/current-bank-view-v1")),
                ("valid", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                ("error", string(error.to_string())),
            ]);
            if let (Value::Object(fields), Value::Object(details)) =
                (&mut value, bank_capacity::details(&error))
            {
                fields.extend(details);
            }
            (value, 1, json::DEFAULT)
        }
    };
    let stdout = match json::encode(&value, style) {
        Ok(text) => text + "\n",
        Err(error) => return command_result(Err(error)),
    };
    CommandResult {
        value: Some(value),
        exit_code,
        stdout,
        stderr: String::new(),
    }
}
