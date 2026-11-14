//! Explicit public identity setup for mediated-writer fixtures.
use super::world::{Result, World, WorldError};
use agent_memory::codec::json::{self, Value};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

fn command(world: &World, executable: &Path, command: &str, args: &[OsString]) -> Result<Value> {
    let output = world.run(
        [executable.as_os_str().to_owned(), "--json".into()]
            .into_iter()
            .chain(super::world::native_arguments(command, args)?),
    )?;
    Ok(json::decode_bytes(
        &output.stdout,
        json::DecodePolicy::default(),
    )?)
}

fn apply(world: &World, executable: &Path, command: &str, proposal: &Value) -> Result<Value> {
    let path = world.inside("fixture-identity-proposal.json")?;
    fs::write(&path, json::encode(proposal, json::DEFAULT)?)?;
    self::command(
        world,
        executable,
        command,
        &["apply".into(), "--proposal".into(), path.into_os_string()],
    )
}

/// Bind through the genuinely installed command, with its own strict locator.
pub fn bind_with_command(world: &World, repo: &Path, executable: &Path) -> Result<Value> {
    let status_args = [
        OsString::from("status"),
        "--repo".into(),
        repo.as_os_str().to_owned(),
    ];
    let developer = command(world, executable, "am developer", &status_args)?;
    if boolean(&developer, "setup_required")? {
        let proposal = command(
            world,
            executable,
            "am developer",
            &[
                "preview".into(),
                "--create".into(),
                "--label".into(),
                "Disposable test developer".into(),
            ],
        )?;
        apply(world, executable, "am developer", &proposal)?;
    }
    let current = command(world, executable, "am identity", &status_args)?;
    if !boolean(&current, "resolved")? {
        let proposal = command(
            world,
            executable,
            "am identity",
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
        let receipt = apply(world, executable, "am identity", &proposal)?;
        if !boolean(&receipt, "verified")? {
            return Err(WorldError::invalid(format!(
                "Identity fixture adoption was not verified: {}",
                json::encode(&receipt, json::DEFAULT)?
            )));
        }
    }
    command(world, executable, "am identity", &status_args)
}

fn boolean(value: &Value, key: &str) -> Result<bool> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| WorldError::invalid(format!("Identity fixture result has no boolean {key}")))
}
