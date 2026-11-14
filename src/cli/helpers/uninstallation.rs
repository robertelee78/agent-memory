//! `installer.py` native subcommand owned by uninstallation: `am uninstall`.

use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Native};
use crate::installation::{Environment, Services};
use crate::lifecycle::CommandResult;
use crate::uninstallation::{self as domain, UninstallRequest};
use std::path::PathBuf;

/// Baseline `uninstall` declaration.
static UNINSTALL: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::flag(&["--check", "--dry-run"], "check")),
        Arg::Opt(Opt::flag(
            &["--omp-writers-quiesced"],
            "omp_writers_quiesced",
        )),
        Arg::Opt(Opt::store(
            &["--preserve-capable-runtime"],
            "preserve_capable_runtime",
        )),
        Arg::Opt(Opt::append(&["--layout-quiescence"], "layout_quiescence").empty()),
    ],
    exclusive: &[],
    subcommands: None,
};

pub const UNINSTALL_NATIVE: Native = Native {
    name: "uninstall",
    help: "Remove only unchanged owned runtime targets; preserve project memory",
    parser: &UNINSTALL,
    run: uninstall,
};

fn uninstall(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let request = UninstallRequest {
        check: matches.flag("check"),
        omp_writers_quiesced: matches.flag("omp_writers_quiesced"),
        preserve_capable_runtime: matches.text("preserve_capable_runtime").map(PathBuf::from),
        layout_quiescence: matches
            .list("layout_quiescence")
            .unwrap_or(&[])
            .iter()
            .map(PathBuf::from)
            .collect(),
    };
    match Services::registered() {
        Ok(services) => domain::uninstall_command(&Environment::process(), &services, &request),
        Err(error) => crate::cli::refusal(context.prog, &error.to_string()),
    }
}
