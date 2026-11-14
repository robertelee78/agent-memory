//! `publish_memory.py` → `am publish`.

use crate::cli::parser::{Arg, Matches, Opt, Parser, Positional};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use crate::memory_publication as domain;
use std::path::Path;

static PARSER: Parser = Parser {
    description: Some(
        "Publish reviewed memory independently of feature code; verify actual authority trees.",
    ),
    args: &[
        Arg::Pos(Positional {
            dest: "operation",
            choices: &["prepare", "land"],
            help: None,
        }),
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--output"], "output")),
        Arg::Opt(Opt::flag(&["--v2"], "v2").help(
            "Read an explicitly v2 proof/request with the complete private-operation budget",
        )),
    ],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    script: "publish_memory.py",
    module: "memory_publication",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, _context: &Context<'_>) -> CommandResult {
    let operation = matches.text("operation").unwrap_or("prepare");
    let repo = Path::new(matches.text("repo").unwrap_or_default());
    let input = Path::new(matches.text("input").unwrap_or_default());
    let output = matches
        .text("output")
        .filter(|o| !o.is_empty())
        .map(Path::new);
    // The pack root is needed only to keep a private output outside the runtime.
    let pack = match output {
        Some(_) => crate::cli::pack_root(),
        None => Err(crate::error::ContractError::new(
            "Pack location is not needed",
        )),
    };
    domain::command_result(operation, repo, input, output, matches.flag("v2"), pack)
}
