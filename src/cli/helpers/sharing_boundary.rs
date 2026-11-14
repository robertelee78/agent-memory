//! `sharing_boundary.py` → `am sharing`.
use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;

static PARSER: Parser = Parser {
    description: Some(
        "Inspect the configured memory-only Git boundary without publishing anything.",
    ),
    args: &[Arg::Opt(Opt::store(&["--repo"], "repo").required())],
    exclusive: &[],
    subcommands: None,
};
pub static HELPER: Helper = Helper {
    script: "sharing_boundary.py",
    module: "sharing_boundary",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &Matches, _: &Context<'_>) -> CommandResult {
    let outcome = matches
        .text("repo")
        .ok_or_else(|| crate::error::ContractError::new("Repository is required"))
        .and_then(|repo| crate::sharing_boundary::inspect(Path::new(repo), None));
    crate::sharing_boundary::command_result(outcome)
}
