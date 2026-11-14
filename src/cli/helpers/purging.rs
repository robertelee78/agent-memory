//! `installer.py` native subcommand owned by purging: `am purge-data`.

use crate::cli::parser::{Arg, Exclusive, Matches, Opt, Parser};
use crate::cli::{Context, Native};
use crate::installation::{Environment, Services};
use crate::lifecycle::CommandResult;
use crate::purging::{self as domain, PurgeRequest};
use std::path::PathBuf;

/// Baseline `purge-data` declaration.
static PURGE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--path"], "path").required()),
        Arg::Opt(Opt::flag(&["--check"], "check")),
        Arg::Opt(Opt::store(&["--confirm"], "confirm")),
    ],
    exclusive: &[Exclusive {
        required: true,
        dests: &["check", "confirm"],
    }],
    subcommands: None,
};

pub const PURGE_NATIVE: Native = Native {
    name: "purge-data",
    help: "Explicitly preview or confirm deletion of one exact local project bank",
    parser: &PURGE,
    run: purge,
};

fn purge(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let request = PurgeRequest {
        repo: PathBuf::from(matches.text("repo").unwrap_or_default()),
        path: PathBuf::from(matches.text("path").unwrap_or_default()),
        check: matches.flag("check"),
        confirm: matches.text("confirm").map(str::to_owned),
    };
    match Services::registered() {
        Ok(services) => domain::purge_command(&Environment::process(), &services, &request),
        Err(error) => crate::cli::refusal(context.prog, &error.to_string()),
    }
}
