//! `bank_state.py` → `am inspect`.
use crate::bank_state as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;

static INSPECT: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--path"], "path")),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Inspect canonical current files and optional local Git facts without runtime state.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[Subcommand {
            name: "inspect",
            help: None,
            parser: &INSPECT,
        }],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am inspect",
    module: "bank_state",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    match matches.command() {
        Some((_, args)) => domain::command_inspect(
            Path::new(args.text("repo").unwrap_or(".")),
            args.text("path"),
        ),
        None => crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        ),
    }
}
