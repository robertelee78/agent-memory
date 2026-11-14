//! `bank_layout.py` → `am bank-layout`.

use crate::bank_layout as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::CommandResult;
use std::path::Path;

static MODEL: Parser = Parser::EMPTY;
static RENDER_INDEX: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--input"], "input").required())],
    exclusive: &[],
    subcommands: None,
};
static VALIDATE_INDEX: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--file"], "file").required()),
        Arg::Opt(
            Opt::store(&["--path"], "path")
                .required()
                .help("Canonical bank-relative location of this index"),
        ),
    ],
    exclusive: &[],
    subcommands: None,
};
static INSPECT: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--repo"], "repo").default("."))],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Pure optional-record inventory and bounded topic-index contract."),
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
                name: "render-index",
                help: None,
                parser: &RENDER_INDEX,
            },
            Subcommand {
                name: "validate-index",
                help: None,
                parser: &VALIDATE_INDEX,
            },
            Subcommand {
                name: "inspect",
                help: None,
                parser: &INSPECT,
            },
        ],
    }),
};

pub static HELPER: Helper = Helper {
    command: "am bank-layout",
    module: "bank_layout",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let outcome = match matches.command() {
        Some(("model", _)) => Ok(domain::model()),
        Some(("inspect", args)) => domain::inspect(Path::new(args.text("repo").unwrap_or("."))),
        Some(("render-index", args)) => input::load(args.text("input").unwrap_or_default())
            .and_then(|request| domain::render_index(&request)),
        Some((_, args)) => input::read_bytes(args.text("file").unwrap_or_default())
            .and_then(|raw| domain::validate_index(args.text("path").unwrap_or_default(), &raw)),
        None => {
            return crate::cli::refusal(
                context.prog,
                "the following arguments are required: command",
            );
        }
    };
    domain::command_result(outcome)
}
