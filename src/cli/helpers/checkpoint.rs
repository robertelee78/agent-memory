//! `checkpoint.py` → `am checkpoint`.
use crate::checkpoint as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::error::ContractError;
use crate::lifecycle::CommandResult;
use std::path::Path;
static INSPECT: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--repo"], "repo").default("."))],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static APPLY: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--proposal"], "proposal").required())],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Checkpoint active context through one identity-bound observed replacement."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "inspect",
                help: None,
                parser: &INSPECT,
            },
            Subcommand {
                name: "preview",
                help: None,
                parser: &PREVIEW,
            },
            Subcommand {
                name: "prepare",
                help: None,
                parser: &PREVIEW,
            },
            Subcommand {
                name: "apply",
                help: None,
                parser: &APPLY,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    script: "checkpoint.py",
    module: "checkpoint",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let repo = Path::new(args.text("repo").unwrap_or("."));
    let outcome = match command {
        "inspect" => domain::inspect(repo),
        "preview" | "prepare" => input::load(args.text("input").unwrap_or_default())
            .and_then(|value| domain::preview(repo, &value, command == "prepare")),
        "apply" => input::load(args.text("proposal").unwrap_or_default())
            .and_then(|value| domain::apply(&value)),
        _ => Err(ContractError::new("Unknown checkpoint operation")),
    };
    domain::command_result(command, outcome)
}
