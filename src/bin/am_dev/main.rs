//! Developer-only tooling; never shipped. Built with `--features dev-tools`.
#[allow(dead_code)]
mod obligations;

use agent_memory::error::ContractError;
use agent_memory::release_builder as release;
use agent_memory::release_distribution::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The release builder inspects what it publishes with the same command
/// surface and migration contract the shipped binary uses.
fn register_release_providers() -> Result<()> {
    let surface = agent_memory::cli::surface::Declared::current()
        .map_err(|error| ContractError::new(error.detail))?;
    release::register_providers(Some(release::Providers {
        surface: Arc::new(surface),
        migration: Arc::new(agent_memory::migration_contract::Bundled),
        command_surface: agent_memory::cli::surface::declaration(),
    }))
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some((command, rest)) = arguments.split_first()
        && command == "obligations"
    {
        let code = obligations::run(rest, Path::new(env!("CARGO_MANIFEST_DIR")));
        std::process::exit(code);
    }
    match register_release_providers().and_then(|()| entry()) {
        Ok(value) => match agent_memory::codec::json::encode(
            &value,
            agent_memory::codec::json::INDENT2_ASCII,
        ) {
            Ok(raw) => println!("{raw}"),
            Err(error) => {
                eprintln!("am release: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("am release: {error}");
            std::process::exit(1);
        }
    }
}
fn entry() -> Result<agent_memory::codec::json::Value> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("release") {
        let usage = concat!(
            "Usage: am-dev release build|publish --private-key PATH ",
            "--binary TRIPLE=PATH (four times) [--output PATH | ",
            "--notes-file PATH --integration-branch integrate/NAME] [--root PATH]"
        );
        return Err(ContractError::new(usage).into());
    }
    let operation = args
        .next()
        .ok_or_else(|| ContractError::new("A release operation is required"))?;
    let mut options = BTreeMap::new();
    let mut binaries = BTreeMap::new();
    while let Some(key) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| ContractError::new("A release option value is required"))?;
        if key == "--binary" {
            let (triple, path) = value
                .split_once('=')
                .ok_or_else(|| ContractError::new("Binary must be TRIPLE=PATH"))?;
            if binaries
                .insert(triple.to_owned(), PathBuf::from(path))
                .is_some()
            {
                return Err(ContractError::new("Duplicate release binary").into());
            }
        } else {
            if ![
                "--root",
                "--private-key",
                "--output",
                "--notes-file",
                "--integration-branch",
            ]
            .contains(&key.as_str())
                || options.insert(key, value).is_some()
            {
                return Err(ContractError::new("Unknown or duplicate release option").into());
            }
        }
    }
    let root = PathBuf::from(
        options
            .get("--root")
            .map(String::as_str)
            .unwrap_or(env!("CARGO_MANIFEST_DIR")),
    );
    let required = |key: &str| -> Result<&str> {
        Ok(options
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| ContractError::new(format!("{key} is required")))?)
    };
    let key = PathBuf::from(required("--private-key")?);
    let providers = release::registered_providers()?;
    let inputs = release::BuildInputs {
        root,
        key,
        binaries,
    };
    match operation.as_str() {
        "build" => Ok(release::build(
            &inputs,
            &PathBuf::from(required("--output")?),
            providers.surface.as_ref(),
            providers.migration.as_ref(),
            &providers.command_surface,
        )?
        .value()),
        "publish" => release::publish(
            &inputs,
            &PathBuf::from(required("--notes-file")?),
            required("--integration-branch")?,
            providers.surface.as_ref(),
            providers.migration.as_ref(),
            &providers.command_surface,
        ),
        _ => Err(ContractError::new("Unknown release operation").into()),
    }
}
