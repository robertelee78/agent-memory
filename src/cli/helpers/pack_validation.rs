//! `validate_pack.py` → `am validate-pack`.

use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::PathBuf;

static PARSER: Parser = Parser {
    description: Some(
        "Read-only, non-executing checks of the portable instruction contract.

This checks the supported Markdown/argparse forms, not English equivalence,
program behavior, source trust, or the truth of memory. Candidate code is data.",
    ),
    args: &[Arg::Opt(Opt::store(&["--pack"], "pack"))],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    script: "validate_pack.py",
    module: "pack_validation",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let pack = match matches.text("pack") {
        Some(pack) => PathBuf::from(pack),
        None => match crate::cli::pack_root() {
            Ok(pack) => pack,
            Err(error) => return crate::cli::refusal(context.prog, error.message()),
        },
    };
    crate::pack_validation::command(&pack)
}
