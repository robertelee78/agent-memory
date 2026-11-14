//! `authority_policy.py` → `am authority-policy`.

use crate::authority as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::CommandResult;

static MODEL: Parser = Parser::EMPTY;
static ASSESS: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--input"], "input").required())],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Read-only authority triage of explicitly reviewed, comparable assertions."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "model",
                help: None,
                parser: &MODEL,
            },
            Subcommand {
                name: "assess",
                help: None,
                parser: &ASSESS,
            },
        ],
    }),
};

pub static HELPER: Helper = Helper {
    script: "authority_policy.py",
    module: "authority",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    match matches.command() {
        Some(("model", _)) => domain::command_model(),
        Some((_, args)) => {
            domain::command_assess(input::load(args.text("input").unwrap_or_default()))
        }
        None => crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        ),
    }
}
