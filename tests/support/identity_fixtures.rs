//! Explicit public identity setup for mediated-writer fixtures.
use super::world::{Result, World, WorldError};
use agent_memory::codec::json::{self, Value};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

fn command(world: &World, helper: &str, args: &[OsString]) -> Result<Value> {
    let output = world.helper(helper, args)?;
    Ok(json::decode_bytes(
        &output.stdout,
        json::DecodePolicy::default(),
    )?)
}

fn apply(world: &World, helper: &str, proposal: &Value) -> Result<Value> {
    let path = world.inside("fixture-identity-proposal.json")?;
    fs::write(&path, json::encode(proposal, json::DEFAULT)?)?;
    command(
        world,
        helper,
        &["apply".into(), "--proposal".into(), path.into_os_string()],
    )
}

pub fn bind(world: &World, repo: &Path) -> Result<Value> {
    let status_args = [
        OsString::from("status"),
        "--repo".into(),
        repo.as_os_str().to_owned(),
    ];
    let developer = command(world, "developer_identity.py", &status_args)?;
    if boolean(&developer, "setup_required")? {
        let proposal = command(
            world,
            "developer_identity.py",
            &[
                "preview".into(),
                "--create".into(),
                "--label".into(),
                "Disposable test developer".into(),
            ],
        )?;
        apply(world, "developer_identity.py", &proposal)?;
    }
    let current = command(world, "scope_binding.py", &status_args)?;
    if !boolean(&current, "resolved")? {
        let proposal = command(
            world,
            "scope_binding.py",
            &[
                OsString::from("preview"),
                "--repo".into(),
                repo.as_os_str().to_owned(),
                "--action".into(),
                "bind".into(),
                "--reason".into(),
                "Explicit disposable fixture bank adoption".into(),
            ],
        )?;
        let receipt = apply(world, "scope_binding.py", &proposal)?;
        if !boolean(&receipt, "verified")? {
            return Err(WorldError::invalid(format!(
                "Identity fixture adoption was not verified: {}",
                json::encode(&receipt, json::DEFAULT)?
            )));
        }
    }
    command(world, "scope_binding.py", &status_args)
}

fn boolean(value: &Value, key: &str) -> Result<bool> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| WorldError::invalid(format!("Identity fixture result has no boolean {key}")))
}
