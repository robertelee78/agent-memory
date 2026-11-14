//! Native public search/get recall routes.
use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;
static PARSER: Parser = Parser {
    description: Some("Project-resolved file/ID/text recall with hard filters before ranking."),
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--request"], "request").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
pub static HELPER: Helper = Helper {
    command: "am search",
    module: "retrieval",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(args: &Matches, _: &Context<'_>) -> CommandResult {
    let result = crate::projections::read_input(
        Path::new(args.text("request").unwrap_or_default()),
        crate::retrieval::MAX_REQUEST,
    )
    .and_then(|request| {
        crate::retrieval::retrieve(
            Path::new(args.text("repo").unwrap_or_default()),
            &request,
            None,
            &[],
        )
    });
    crate::retrieval::command_result(result)
}
