//! `bank_contract.py` → `am bank`.

use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::CommandResult;
use std::path::Path;

static RENDER: Parser = Parser {
    description: None,
    args: &[Arg::Opt(
        Opt::store(&["--input"], "input")
            .required()
            .help("JSON file, or - for stdin"),
    )],
    exclusive: &[],
    subcommands: None,
};
static VALIDATE: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--repo"], "repo").default("."))],
    exclusive: &[],
    subcommands: None,
};
// Recovery is dispatched to its typed helper before parsing bank commands.
static PARSER: Parser = Parser {
    description: Some(
        "Read-only rendering and validation of the first-release router/file contract.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "recover-layout",
                help: Some("source-preserving reviewed layout recovery"),
                parser: &super::layout_recovery::PARSER,
            },
            Subcommand {
                name: "render",
                help: Some("emit a file proposal; write nothing"),
                parser: &RENDER,
            },
            Subcommand {
                name: "validate",
                help: Some("inspect the repository-root memory-bank"),
                parser: &VALIDATE,
            },
        ],
    }),
};

pub static HELPER: Helper = Helper {
    command: "am bank",
    module: "bank_contract",
    parser: &PARSER,
    routes: &[crate::cli::Route {
        name: "recover-layout",
        helper: &super::layout_recovery::HELPER,
        prefix: &[],
    }],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    match matches.command() {
        Some(("render", args)) => crate::bank_contract::command_render(input::load_text_or_stdin(
            args.text("input").unwrap_or_default(),
        )),
        Some((_, args)) => {
            crate::bank_contract::command_validate(Path::new(args.text("repo").unwrap_or(".")))
        }
        None => crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        ),
    }
}
