//! `scope_policy.py` → `am scope-policy`.

use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::CommandResult;
use crate::scopes as domain;

static MODEL: Parser = Parser::EMPTY;
static DECIDE: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--input"], "input").required())],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Shared repository namespace and applicability decisions, never an access ACL.",
    ),
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
                name: "decide",
                help: None,
                parser: &DECIDE,
            },
        ],
    }),
};

pub static HELPER: Helper = Helper {
    script: "scope_policy.py",
    module: "scopes",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    match matches.command() {
        Some(("model", _)) => domain::command_model(),
        Some((_, args)) => {
            domain::command_decide(input::load(args.text("input").unwrap_or_default()))
        }
        None => crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        ),
    }
}
