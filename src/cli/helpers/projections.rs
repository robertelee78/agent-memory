//! Disposable canonical-reference projection commands.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;
static OPERATION: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Disposable canonical-reference projections; no independent memory bodies."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "build",
                help: None,
                parser: &OPERATION,
            },
            Subcommand {
                name: "check",
                help: None,
                parser: &OPERATION,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    script: "projection.py",
    module: "projections",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let maximum = if operation == "build" {
        crate::projections::MAX_BUILD_INPUT
    } else {
        crate::projections::MAX_BYTES
    };
    let result =
        crate::projections::read_input(Path::new(args.text("input").unwrap_or_default()), maximum)
            .and_then(|request| {
                let repo = Path::new(args.text("repo").unwrap_or_default());
                if operation == "build" {
                    crate::projections::build(repo, &request)
                } else {
                    crate::projections::check(repo, &request)
                }
            });
    crate::projections::command_result(operation, result)
}
