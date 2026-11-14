//! `artifact.py` → `am artifact`.

use crate::cli::parser::{Arg, Matches, Opt, Parser, Positional};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::PathBuf;

static PARSER: Parser = Parser {
    description: Some("Describe a complete portable artifact without executing candidate code."),
    args: &[
        Arg::Pos(Positional {
            dest: "operation",
            choices: &["inspect"],
            help: None,
        }),
        Arg::Opt(Opt::store(&["--pack"], "pack")),
    ],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    script: "artifact.py",
    module: "artifact",
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
    crate::artifact::command(&pack)
}
