//! `installer.py` native subcommands owned by diagnostics: `am version`,
//! `am status`, `am doctor` (baseline `diagnose.py` / `diagnostics.main`).

use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Native};
use crate::diagnostics::{self as domain, Request};
use crate::installation::{Environment, Services};
use crate::lifecycle::CommandResult;
use std::path::PathBuf;

const HELP: &str = "Read-only runtime and bank diagnostics";

static VERSION: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::flag(&["--json"], "json"))],
    exclusive: &[],
    subcommands: None,
};

static STATUS: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::flag(&["--json"], "json")),
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
    ],
    exclusive: &[],
    subcommands: None,
};

static DOCTOR: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::flag(&["--json"], "json")),
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--projection"], "projection")),
        Arg::Opt(
            Opt::flag(&["--fix"], "fix")
                .help("Repair safely identified owned runtime faults and recheck"),
        ),
    ],
    exclusive: &[],
    subcommands: None,
};

pub const VERSION_NATIVE: Native = Native {
    name: "version",
    help: HELP,
    parser: &VERSION,
    run: version,
};

pub const STATUS_NATIVE: Native = Native {
    name: "status",
    help: HELP,
    parser: &STATUS,
    run: status,
};

pub const DOCTOR_NATIVE: Native = Native {
    name: "doctor",
    help: HELP,
    parser: &DOCTOR,
    run: doctor,
};

fn execute(operation: &str, matches: &Matches, context: &Context<'_>) -> CommandResult {
    let request = Request {
        operation: operation.to_owned(),
        repo: PathBuf::from(matches.text("repo").unwrap_or(".")),
        projection: matches.text("projection").map(PathBuf::from),
        fix: matches.flag("fix"),
    };
    match Services::registered() {
        Ok(services) => domain::command(&Environment::process(), &services, &request),
        Err(error) => crate::cli::refusal(context.prog, &error.to_string()),
    }
}

fn version(matches: &Matches, context: &Context<'_>) -> CommandResult {
    execute("version", matches, context)
}

fn status(matches: &Matches, context: &Context<'_>) -> CommandResult {
    execute("status", matches, context)
}

fn doctor(matches: &Matches, context: &Context<'_>) -> CommandResult {
    execute("doctor", matches, context)
}
