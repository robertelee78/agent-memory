//! Logical lifecycle write boundaries, enabled only in explicit disposable tests.
//!
//! A payload stage (including its file copies/fsyncs) is one logical write; Git's
//! private candidate clone is temporary input. Trace logging is test telemetry,
//! not another lifecycle mutation. Production does not create a trace.
//!
//! The ambient environment can enable tracing or injected failures only when it
//! also names a valid disposable test World (`AGENT_MEMORY_TEST_ROOT` directly
//! below the canonical `/tmp`, an `agent-memory-test-*` name and a marker whose
//! token equals `AGENT_MEMORY_TEST_TOKEN`); otherwise the operation refuses.
use crate::bank_writes::fault;
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, Value};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::lifecycle::{get, object, string};
use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};

/// Explicit reader bound for the disposable World marker.
pub const MAX_WORLD_MARKER: usize = 1024 * 1024;
const STAGE: &str = "AGENT_MEMORY_FAULT_STAGE";
const TRACE: &str = "AGENT_MEMORY_FAULT_TRACE";
const ERROR_STAGE: &str = "AGENT_MEMORY_FAULT_ERROR_STAGE";

#[derive(Clone)]
struct Current {
    name: String,
    step: u64,
    testing: bool,
    trace: Option<PathBuf>,
}

thread_local! {
    static CURRENT: RefCell<Option<Current>> = const { RefCell::new(None) };
}

/// Restores the enclosing operation even when the body unwinds.
struct Restore(Option<Current>);
impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        CURRENT.with(|current| *current.borrow_mut() = previous);
    }
}
fn replace(value: Option<Current>) -> Restore {
    Restore(CURRENT.with(|current| current.replace(value)))
}

fn environment(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn world_error() -> ContractError {
    ContractError::new("Lifecycle fault tracing requires an explicit disposable World")
}

fn is_symlink(path: &Path) -> Result<bool> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok(false);
    };
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Ok(false),
        Err(error) => return Err(ContractError::new(error.to_string())),
    };
    match chain.directory().stat_at(name) {
        Ok(info) => Ok(info.is_symlink()),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(false),
        Err(error) => Err(ContractError::new(error.to_string())),
    }
}

/// Baseline `test_root`: the validated canonical disposable World root.
pub fn test_root() -> Result<PathBuf> {
    let root = PathBuf::from(
        std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap_or_else(|| "/missing-test-world".into()),
    );
    let token = environment("AGENT_MEMORY_TEST_TOKEN").ok_or_else(world_error)?;
    if !root.is_absolute() || is_symlink(&root).map_err(|_| world_error())? {
        return Err(world_error());
    }
    let resolved = fs::canonical_path(&root, false).map_err(|_| world_error())?;
    let temporary = fs::canonical_path(Path::new("/tmp"), false).map_err(|_| world_error())?;
    let named = root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("agent-memory-test-"));
    if resolved.parent() != Some(temporary.as_path()) || !named {
        return Err(world_error());
    }
    let (raw, _) = fs::read_regular(&root, Path::new(".test-world.json"), MAX_WORLD_MARKER)
        .map_err(|_| world_error())?;
    let marker = json::decode_bytes(
        &raw,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::LastWins,
            max_bytes: MAX_WORLD_MARKER,
            ..DecodePolicy::default()
        },
    )
    .map_err(|_| world_error())?;
    if get(&marker, "token").as_str() != Some(token.as_str()) {
        return Err(world_error());
    }
    Ok(resolved)
}

fn inside(path: &Path, root: &Path) -> Result<bool> {
    let resolved = fs::canonical_path(path, false)?;
    Ok(resolved.starts_with(root) && resolved != root)
}

fn enter(name: &str, paths: &[PathBuf]) -> Result<Restore> {
    let testing = environment(STAGE).is_some()
        || environment(TRACE).is_some()
        || environment(ERROR_STAGE).is_some();
    let root = if testing { Some(test_root()?) } else { None };
    if let Some(root) = &root {
        for path in paths {
            if !inside(path, root)? {
                return Err(ContractError::new(
                    "Lifecycle fault tests must not mutate paths outside their World",
                ));
            }
        }
    }
    let trace = environment(TRACE).map(PathBuf::from);
    if let Some(trace) = &trace {
        let contained = match &root {
            Some(root) => fs::canonical_path(trace, false)?.starts_with(root),
            None => false,
        };
        if !trace.is_absolute()
            || trace
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            || !contained
            || is_symlink(trace)?
        {
            return Err(ContractError::new(
                "Lifecycle trace must remain inside its disposable World",
            ));
        }
    }
    Ok(replace(Some(Current {
        name: name.to_owned(),
        step: 0,
        testing,
        trace,
    })))
}

/// Baseline `operation`: validate the complete write set, then number writes.
pub fn operation<T, E: From<ContractError>>(
    name: &str,
    paths: &[PathBuf],
    body: impl FnOnce() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    let _restore = enter(name, paths)?;
    body()
}

/// Baseline `temporary_probe`: writes inside the body are not lifecycle steps.
pub fn temporary_probe<T>(body: impl FnOnce() -> T) -> T {
    let _restore = replace(None);
    body()
}

fn append_trace(trace: &Path, line: &str) -> Result<()> {
    let invalid = || ContractError::new("Lifecycle trace must remain inside its disposable World");
    let parent = trace.parent().ok_or_else(invalid)?;
    let name = trace.file_name().ok_or_else(invalid)?;
    let chain = DirectoryChain::open(parent).map_err(|e| ContractError::new(e.to_string()))?;
    let (mut file, _) = chain
        .directory()
        .open_or_create(name, libc::O_WRONLY | libc::O_APPEND, 0o600)
        .map_err(|e| ContractError::new(e.to_string()))?;
    if !file
        .stat()
        .map_err(|e| ContractError::new(e.to_string()))?
        .is_regular()
    {
        return Err(invalid());
    }
    file.write_all(line.as_bytes())
        .map_err(|e| ContractError::new(e.to_string()))
}

fn boundary(current: &Current, step: u64, kind: &str, phase: &str) -> Result<()> {
    let point = format!("{}-write-{step}-{phase}", current.name);
    if let Some(trace) = &current.trace {
        let row = object([
            ("point", string(point.as_str())),
            ("step", Value::Int(step.into())),
            ("phase", string(phase)),
            ("kind", string(kind)),
        ]);
        append_trace(trace, &(json::encode(&row, json::DEFAULT)? + "\n"))?;
    }
    fault(&point)?;
    if environment(ERROR_STAGE).as_deref() == Some(point.as_str()) {
        return Err(ContractError::new(format!(
            "Injected logical write failure at {point}"
        )));
    }
    Ok(())
}

/// Baseline `mutate`: one numbered logical write with before/after boundaries.
pub fn mutate<T, E: From<ContractError>>(
    kind: &str,
    action: impl FnOnce() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    let selected = CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let state = current.as_mut().filter(|state| state.testing)?;
        let step = state.step;
        state.step += 1;
        Some((state.clone(), step))
    });
    let Some((current, step)) = selected else {
        return action();
    };
    boundary(&current, step, kind, "before")?;
    let result = action()?;
    boundary(&current, step, kind, "after")?;
    Ok(result)
}

/// Lose only the response, after real public-operation verification/commit.
pub fn response(name: &str, result: &Value) -> Result<()> {
    if get(result, "verified") == &Value::Bool(true) {
        fault(&format!("{name}-response-lost"))?;
    }
    Ok(())
}
