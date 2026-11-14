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
// Baseline also declares `recover-layout` (routed to recover_layout.py) first;
// it is registered with layout recovery, which is not yet ported.
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
    script: "bank_contract.py",
    module: "bank_contract",
    parser: &PARSER,
    routes: &[],
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
