//! `refresh_memory.py` → `am refresh` and `am boundary`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use crate::memory_refresh as domain;
use std::path::Path;

const REPO: Arg = Arg::Opt(Opt::store(&["--repo"], "repo").required());
const INPUT: Arg = Arg::Opt(Opt::store(&["--input"], "input").required());
const OUTPUT: Arg = Arg::Opt(Opt::store(&["--output"], "output"));
const V2: Arg = Arg::Opt(Opt::flag(&["--v2"], "v2"));
static INSPECT: Parser = Parser {
    description: None,
    args: &[REPO, Arg::Opt(Opt::store(&["--base"], "base")), OUTPUT, V2],
    exclusive: &[],
    subcommands: None,
};
static CURRENT: Parser = Parser {
    description: None,
    args: &[REPO],
    exclusive: &[],
    subcommands: None,
};
static PREPARE: Parser = Parser {
    description: None,
    args: &[REPO, INPUT, OUTPUT, V2],
    exclusive: &[],
    subcommands: None,
};
static APPLY: Parser = Parser {
    description: None,
    args: &[INPUT, OUTPUT, V2],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Reconcile landed repository memory without importing feature code or changing its index.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "operation",
        required: true,
        commands: &[
            Subcommand {
                name: "inspect",
                help: None,
                parser: &INSPECT,
            },
            Subcommand {
                name: "current",
                help: None,
                parser: &CURRENT,
            },
            Subcommand {
                name: "prepare",
                help: None,
                parser: &PREPARE,
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
    script: "refresh_memory.py",
    module: "memory_refresh",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: operation",
        );
    };
    let output = args.text("output").filter(|s| !s.is_empty()).map(Path::new);
    let runtime = if output.is_some() {
        crate::cli::runtime_root()
    } else {
        Err(crate::error::ContractError::new(
            "Runtime location is not needed",
        ))
    };
    domain::command_result(
        operation,
        args.text("repo").map(Path::new),
        args.text("input").map(Path::new),
        args.text("base"),
        output,
        args.flag("v2"),
        runtime,
    )
}
