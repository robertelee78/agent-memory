//! `inactive_records.py` → `am inactive-records`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::error::ContractError;
use crate::inactive as domain;
use crate::lifecycle::CommandResult;
use std::path::Path;
static COMPACT: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--file"], "file").required())],
    exclusive: &[],
    subcommands: None,
};
static FILTER: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Minimal inactive representations and canonical invalidation of derived input.\n\nThese helpers do not choose a lifecycle change, admit learning or return recalled\ntext. They keep stale projection bodies from supplying their own replacement.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "compact",
                help: None,
                parser: &COMPACT,
            },
            Subcommand {
                name: "filter",
                help: None,
                parser: &FILTER,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am inactive-records",
    module: "inactive",
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
    let outcome = match command {
        "compact" => domain::bounded_file(Path::new(args.text("file").unwrap_or_default()))
            .and_then(|raw| {
                let content = std::str::from_utf8(&raw)
                    .map_err(|_| ContractError::non_utf8("inactive input"))?;
                domain::compact(content)
            }),
        "filter" => domain::bounded_file(Path::new(args.text("input").unwrap_or_default()))
            .and_then(|raw| crate::developer_identity::load(&raw))
            .and_then(|request| {
                domain::filter_projection(
                    Path::new(args.text("repo").unwrap_or_default()),
                    &request,
                )
            }),
        _ => Err(ContractError::new(
            "Unknown inactive representation operation",
        )),
    };
    domain::command_result(command, outcome)
}
