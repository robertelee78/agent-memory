//! `tombstones.py` → `am tombstones`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::CommandResult;
use crate::tombstones as domain;
use std::path::Path;

static RENDER: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--input"], "input").required())],
    exclusive: &[],
    subcommands: None,
};
static VALIDATE: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--file"], "file").required())],
    exclusive: &[],
    subcommands: None,
};
static FILTER: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Content-free tombstones and the shared projection anti-resurrection guard."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "render",
                help: None,
                parser: &RENDER,
            },
            Subcommand {
                name: "validate",
                help: None,
                parser: &VALIDATE,
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
    script: "tombstones.py",
    module: "tombstones",
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
        "render" => input::load(args.text("input").unwrap_or_default())
            .and_then(|metadata| domain::render(&metadata)),
        "validate" => input::read_text(args.text("file").unwrap_or_default())
            .and_then(|body| domain::validate(&body)),
        _ => input::load(args.text("input").unwrap_or_default()).and_then(|projection| {
            domain::filter_projection(Path::new(args.text("repo").unwrap_or(".")), &projection)
        }),
    };
    domain::command_result(command, outcome)
}
