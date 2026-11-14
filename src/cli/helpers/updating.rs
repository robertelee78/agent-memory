//! `installer.py` native subcommand owned by updating: `am update`.

use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Native};
use crate::installation::{Environment, Services};
use crate::lifecycle::CommandResult;
use crate::updating::{self as domain, UpdateRequest};
use std::path::PathBuf;

/// Baseline `update` declaration plus the re-run bootstrap's `--bootstrap-binary`.
static UPDATE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::flag(&["--check"], "check")),
        Arg::Opt(Opt::append(&["--repo"], "repo").empty()),
        Arg::Opt(Opt::flag(
            &["--omp-writers-quiesced"],
            "omp_writers_quiesced",
        )),
        Arg::Opt(Opt::flag(&["--omp-peers-upgraded"], "omp_peers_upgraded")),
        Arg::Opt(Opt::append(
            &["--omp-layout-quiescence"],
            "omp_layout_quiescence",
        )),
        Arg::Opt(Opt::append(&["--layout-quiescence"], "layout_quiescence").empty()),
        Arg::Opt(Opt::store(&["--bootstrap-binary"], "bootstrap_binary")),
    ],
    exclusive: &[],
    subcommands: None,
};

pub const UPDATE_NATIVE: Native = Native {
    name: "update",
    help: "Check or activate a verified managed-channel update",
    parser: &UPDATE,
    run: update,
};

fn paths(values: Option<&[String]>) -> Vec<PathBuf> {
    values.unwrap_or(&[]).iter().map(PathBuf::from).collect()
}

fn update(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let request = UpdateRequest {
        check: matches.flag("check"),
        repo: matches.list("repo").unwrap_or(&[]).to_vec(),
        omp_writers_quiesced: matches.flag("omp_writers_quiesced"),
        omp_peers_upgraded: matches.flag("omp_peers_upgraded"),
        omp_layout_quiescence: matches
            .list("omp_layout_quiescence")
            .map(|values| paths(Some(values))),
        layout_quiescence: paths(matches.list("layout_quiescence")),
        bootstrap_binary: matches.text("bootstrap_binary").map(PathBuf::from),
    };
    match Services::registered() {
        Ok(services) => domain::update_command(&Environment::process(), &services, &request),
        Err(error) => crate::cli::refusal(context.prog, &error.to_string()),
    }
}
