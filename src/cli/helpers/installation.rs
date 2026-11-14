//! `installer.py` native subcommands owned by installation: `am install`, `am recover`.

use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Native};
use crate::installation::{self as domain, Environment, InstallRequest, Mode, Services};
use crate::lifecycle::CommandResult;
use std::path::PathBuf;

/// Baseline `install` declaration plus the bootstrap's `--bootstrap-binary`.
static INSTALL: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::flag(&["--check", "--dry-run"], "check")),
        Arg::Opt(Opt::store(&["--development-source"], "development_source")),
        Arg::Opt(Opt::store(&["--remote"], "remote")),
        Arg::Opt(Opt::store(&["--ref"], "ref")),
        Arg::Opt(Opt::store(&["--mode"], "mode").choices(&["link", "copy"])),
        Arg::Opt(Opt::store(&["--presets"], "presets").help(
            "all, none, or comma-separated agents,codex,claude,opencode,omp (omp excluded from all)",
        )),
        Arg::Opt(Opt::store(&["--omp-agent-dir"], "omp_agent_dir")),
        Arg::Opt(Opt::flag(&["--omp-peers-upgraded"], "omp_peers_upgraded")),
        Arg::Opt(
            Opt::append(&["--omp-layout-quiescence"], "omp_layout_quiescence").help(
                "Refreshed layout hold attestation including the new native invocation route",
            ),
        ),
        Arg::Opt(Opt::append(&["--layout-quiescence"], "layout_quiescence").empty()),
        Arg::Opt(Opt::append(&["--target-dir"], "target_dir")),
        Arg::Opt(Opt::store(&["--bin-dir"], "bin_dir")),
        Arg::Opt(Opt::flag(&["--no-guidance"], "no_guidance")),
        Arg::Opt(Opt::store(&["--bootstrap-binary"], "bootstrap_binary")),
    ],
    exclusive: &[],
    subcommands: None,
};

static RECOVER: Parser = Parser {
    description: None,
    args: &[],
    exclusive: &[],
    subcommands: None,
};

pub const INSTALL_NATIVE: Native = Native {
    name: "install",
    help: "Plan or install owned portable runtime targets",
    parser: &INSTALL,
    run: install,
};

pub const RECOVER_NATIVE: Native = Native {
    name: "recover",
    help: "Verify a committed transaction or roll back interrupted activation",
    parser: &RECOVER,
    run: recover,
};

fn paths(matches: &Matches, dest: &str) -> Vec<PathBuf> {
    matches
        .list(dest)
        .unwrap_or(&[])
        .iter()
        .map(PathBuf::from)
        .collect()
}

fn services(context: &Context<'_>) -> Result<Services, CommandResult> {
    Services::registered().map_err(|error| crate::cli::refusal(context.prog, &error.to_string()))
}

fn install(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let mode = match matches.text("mode").map(Mode::parse).transpose() {
        Ok(mode) => mode,
        Err(error) => return crate::cli::refusal(context.prog, &error.to_string()),
    };
    let request = InstallRequest {
        check: matches.flag("check"),
        development_source: matches.text("development_source").map(PathBuf::from),
        remote: matches.text("remote").map(str::to_owned),
        reference: matches.text("ref").map(str::to_owned),
        mode,
        presets: matches.text("presets").map(str::to_owned),
        omp_agent_dir: matches.text("omp_agent_dir").map(str::to_owned),
        omp_peers_upgraded: matches.flag("omp_peers_upgraded"),
        omp_layout_quiescence: paths(matches, "omp_layout_quiescence"),
        layout_quiescence: paths(matches, "layout_quiescence"),
        target_dir: matches
            .list("target_dir")
            .map(|values| values.iter().map(PathBuf::from).collect()),
        bin_dir: matches.text("bin_dir").map(PathBuf::from),
        no_guidance: matches.flag("no_guidance"),
        bootstrap_binary: matches.text("bootstrap_binary").map(PathBuf::from),
    };
    match services(context) {
        Ok(services) => domain::install_command(&Environment::process(), &services, &request),
        Err(refused) => refused,
    }
}

fn recover(_matches: &Matches, context: &Context<'_>) -> CommandResult {
    match services(context) {
        Ok(services) => domain::recover_command(&Environment::process(), &services),
        Err(refused) => refused,
    }
}
