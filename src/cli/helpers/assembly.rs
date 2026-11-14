//! Exact-current bounded context preview and harness-tokenizer qualification.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;
const REPO: Arg = Arg::Opt(Opt::store(&["--repo"], "repo").required());
const REQUEST: Arg = Arg::Opt(Opt::store(&["--request"], "request").required());
static PREVIEW: Parser = Parser {
    description: None,
    args: &[REPO, REQUEST],
    exclusive: &[],
    subcommands: None,
};
static CHECK: Parser = Parser {
    description: None,
    args: &[
        REPO,
        REQUEST,
        Arg::Opt(Opt::store(&["--counter"], "counter").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Fresh bounded context with complete labels and atomic disputed evidence."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "preview",
                help: None,
                parser: &PREVIEW,
            },
            Subcommand {
                name: "check",
                help: None,
                parser: &CHECK,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am context",
    module: "assembly",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(args: &Matches, _: &Context<'_>) -> CommandResult {
    let Some((command, args)) = args.command() else {
        return crate::assembly::command_result(Err(crate::error::ContractError::new(
            "Context operation required",
        )));
    };
    let outcome = crate::assembly::read_input(Path::new(args.text("request").unwrap_or_default()))
        .and_then(|request| {
            let repo = Path::new(args.text("repo").unwrap_or_default());
            if command == "check" {
                crate::assembly::read_input(Path::new(args.text("counter").unwrap_or_default()))
                    .and_then(|counter| crate::assembly::check(repo, &request, &counter))
            } else {
                crate::assembly::assemble(repo, &request)
            }
        });
    crate::assembly::command_result(outcome)
}
