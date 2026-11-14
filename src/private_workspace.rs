//! Project-bound private artifacts, never canonical bank writes.
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, load_with_limit};
use crate::error::{ContractError, Result};
use crate::fs::{Descriptor, DirectoryChain, Identity, trust};
use crate::host::{self, Sources};
use crate::lifecycle::{get, object, string};
use crate::{bank_capacity, bank_writes, local_registry as storage, scope_binding};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const MAX_INPUT: usize = 16 * 1024 * 1024;
pub const MAX_BINDING: usize = 4096;
const CHANGED: &str = "Private workspace changed; preserve it for inspection";
const REPAIR: &str =
    "Preserve existing private artifacts and inspect the reported workspace before retrying.";

fn io_error(error: impl std::fmt::Display) -> ContractError {
    ContractError::new(error.to_string())
}
fn path_text(path: &Path) -> Result<&str> {
    host::text(path.as_os_str(), "private workspace path")
}
fn project(context: &Value) -> Result<&str> {
    get(context, "project_id")
        .as_str()
        .ok_or_else(|| ContractError::new("Private workspace does not match this bound repository"))
}

pub fn workspace_root(repo: &Path) -> Result<(PathBuf, Value, PathBuf)> {
    workspace_root_with_sources(repo, Sources::default())
}
fn workspace_root_with_sources(repo: &Path, sources: Sources) -> Result<(PathBuf, Value, PathBuf)> {
    let context = scope_binding::require(repo, None, scope_binding::RequireOptions::default())?;
    let (root, bank) = bank_writes::bank_path(repo)?;
    let (operations, _) = bank_writes::state_paths(&root, &bank, &(sources.uuid4)()?)?;
    let directory = operations
        .parent()
        .ok_or_else(|| ContractError::new("Private workspace operations have no parent"))?
        .join("private-workspaces");
    trust::inspect(&directory, trust::InspectOptions::default())?;
    Ok((root, context, directory))
}
fn binding(root: &Path, context: &Value, workspace: &str) -> Result<Value> {
    Ok(object([
        ("schema", string("agent-memory/private-workspace-v1")),
        ("repo", string(path_text(root)?)),
        ("project_id", string(project(context)?)),
        ("workspace", string(workspace)),
    ]))
}
pub fn create(repo: &Path) -> Result<Value> {
    create_with_sources(repo, Sources::default())
}
pub fn create_with_sources(repo: &Path, sources: Sources) -> Result<Value> {
    let (root, context, base) = workspace_root_with_sources(repo, sources)?;
    storage::private_mkdir(&base)?;
    trust::state(&base)?;
    let name = (sources.uuid4)()?.replace('-', "");
    // Sources supplies a UUID; validate injected sources before using a pathname.
    if !workspace_name(&name) {
        return Err(ContractError::new(
            "Use the returned workspace ID and a simple new JSON filename",
        ));
    }
    let parent = DirectoryChain::open(&base).map_err(io_error)?;
    let directory = parent
        .directory()
        .create_directory(OsStr::new(&name), 0o700)
        .map_err(io_error)?;
    parent.directory().fsync_directory().map_err(io_error)?;
    parent.recheck().map_err(io_error)?;
    let path = base.join(&name);
    trust::state(&path)?;
    let info = directory.stat().map_err(io_error)?;
    check_directory(&path, &info)?;
    write_fd(
        &directory,
        "binding.json",
        &encoded(&binding(&root, &context, &name)?)?,
    )?;
    check_directory(&path, &info)?;
    Ok(object([
        ("operation", string("private-create")),
        ("workspace", string(name)),
        ("path", string(path_text(&path)?)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("verified", Value::Bool(true)),
    ]))
}

pub fn checked_directory(path: &Path) -> Result<(Descriptor, Identity)> {
    trust::state(path)?;
    let directory = Descriptor::open_directory(path).map_err(io_error)?;
    let info = directory.stat().map_err(io_error)?;
    if info.uid != host::effective_uid() || info.mode & 0o7777 != 0o700 {
        return Err(ContractError::new(
            "Private workspace must remain owner-only",
        ));
    }
    check_directory(path, &info)?;
    Ok((directory, info))
}
pub fn check_directory(path: &Path, expected: &Identity) -> Result<()> {
    trust::state(path)?;
    let chain = DirectoryChain::open(path).map_err(|_| ContractError::new(CHANGED))?;
    let observed = chain.directory().stat().map_err(io_error)?;
    if !observed.is_directory()
        || !observed.same_inode(*expected)
        || observed.uid != host::effective_uid()
        || observed.mode & 0o7777 != 0o700
    {
        return Err(ContractError::new(CHANGED));
    }
    chain.recheck().map_err(|_| ContractError::new(CHANGED))?;
    Ok(())
}

pub fn write_fd(directory: &Descriptor, name: &str, raw: &[u8]) -> Result<()> {
    let mut file = directory
        .create_exclusive(OsStr::new(name), libc::O_WRONLY, 0o600)
        .map_err(|error| crate::cli::input::os_error(&error, name))?;
    file.write_all(raw).map_err(io_error)?;
    file.fsync().map_err(io_error)?;
    drop(file);
    directory.fsync_directory().map_err(io_error)
}
pub fn write_exclusive(directory: &Path, name: &str, raw: &[u8]) -> Result<()> {
    let (fd, info) = checked_directory(directory)?;
    write_fd(&fd, name, raw)?;
    check_directory(directory, &info)
}
pub fn read_binding(directory: &Descriptor) -> Result<Vec<u8>> {
    let mut record = directory
        .open_at(
            OsStr::new("binding.json"),
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        )
        .map_err(|error| crate::cli::input::os_error(&error, "binding.json"))?;
    let info = record.stat().map_err(io_error)?;
    if !info.is_regular()
        || info.uid != host::effective_uid()
        || info.nlink != 1
        || info.mode & 0o7777 != 0o600
        || info.size < 0
        || info.size as u128 > MAX_BINDING as u128
    {
        return Err(ContractError::new("Private workspace binding is unsafe"));
    }
    let raw = record.read_prefix(MAX_BINDING + 1).map_err(io_error)?;
    if raw.len() > MAX_BINDING {
        return Err(ContractError::new(
            "Private workspace binding exceeds its bound",
        ));
    }
    Ok(raw)
}
fn workspace_name(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn input_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    !stem.is_empty()
        && stem.len() <= 128
        && stem
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && stem
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        && name != "binding.json"
}
fn binding_matches(value: &Value, expected: &Value) -> bool {
    // Every binding field is a string; dict equality must ignore key order.
    value.as_object().is_some_and(|fields| {
        fields.len() == 4
            && fields
                .iter()
                .all(|(key, item)| expected.get(key) == Some(item))
    })
}
pub fn put(repo: &Path, workspace: &str, name: &str, raw: &[u8]) -> Result<Value> {
    if !workspace_name(workspace) || !input_name(name) {
        return Err(ContractError::new(
            "Use the returned workspace ID and a simple new JSON filename",
        ));
    }
    if raw.len() > MAX_INPUT {
        return Err(ContractError::new("Private input exceeds the 16 MiB bound"));
    }
    let value = load_with_limit(raw, MAX_INPUT)?;
    if !matches!(value, Value::Object(_) | Value::Array(_)) {
        return Err(ContractError::new(
            "Private input must be a JSON object or array",
        ));
    }
    let (root, context, base) = workspace_root(repo)?;
    let path = base.join(workspace);
    let (directory, info) = checked_directory(&path)?;
    let original = read_binding(&directory)?;
    if !binding_matches(
        &load_with_limit(&original, MAX_BINDING)?,
        &binding(&root, &context, workspace)?,
    ) {
        return Err(ContractError::new(
            "Private workspace does not match this bound repository",
        ));
    }
    check_directory(&path, &info)?;
    let options = scope_binding::RequireOptions {
        expected: Some(&context),
        ..Default::default()
    };
    scope_binding::require(&root, Some(project(&context)?), options)?;
    write_fd(&directory, name, raw)?;
    check_directory(&path, &info)?;
    if read_binding(&directory)? != original {
        return Err(ContractError::new(
            "Private workspace binding changed; preserve written evidence",
        ));
    }
    scope_binding::require(&root, Some(project(&context)?), options)?;
    Ok(object([
        ("operation", string("private-put")),
        ("path", string(path_text(&path.join(name))?)),
        ("bytes", Value::Int(raw.len().into())),
        ("canonical_writes_performed", Value::Bool(false)),
        ("verified", Value::Bool(true)),
    ]))
}

pub fn command_result(operation: &str, outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code) = match outcome {
        Ok(value) => (value, 0),
        Err(error) => {
            let mut fields = vec![
                ("operation".into(), string(format!("private-{operation}"))),
                ("verified".into(), Value::Bool(false)),
                ("canonical_writes_performed".into(), Value::Bool(false)),
                ("error".into(), string(error.to_string())),
                ("repair".into(), string(REPAIR)),
            ];
            if let Value::Object(details) = bank_capacity::details(&error) {
                for (key, value) in details {
                    if let Some((_, current)) =
                        fields.iter_mut().find(|(existing, _)| existing == &key)
                    {
                        *current = value;
                    } else {
                        fields.push((key, value));
                    }
                }
            }
            (Value::Object(fields), 1)
        }
    };
    match json::encode(&value, json::DEFAULT) {
        Ok(raw) => crate::lifecycle::CommandResult {
            value: Some(value),
            exit_code,
            stdout: raw + "\n",
            stderr: String::new(),
        },
        Err(error) => command_result(operation, Err(error)),
    }
}
