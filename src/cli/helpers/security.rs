//! `security_policy.py` → `am security-policy`.
use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use crate::security;
use std::path::Path;

static PARSER: Parser = Parser {
    description: Some(
        "Bounded project policy checks; labels and UUIDs never authenticate a person.",
    ),
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--request"], "request").required()),
        Arg::Opt(Opt::store(&["--operator-approval"], "operator_approval")),
    ],
    exclusive: &[],
    subcommands: None,
};
pub static HELPER: Helper = Helper {
    command: "am security-policy",
    module: "security",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &Matches, _context: &Context<'_>) -> CommandResult {
    security::command_result((|| {
        let request = crate::sharing_limits::read_json(
            Path::new(matches.text("request").unwrap_or_default()),
            security::REQUEST_LIMIT,
        )?;
        let approval = matches
            .text("operator_approval")
            .filter(|path| !path.is_empty())
            .map(|path| crate::sharing_limits::read_json(Path::new(path), security::REQUEST_LIMIT))
            .transpose()?;
        security::evaluate(
            Path::new(matches.text("repo").unwrap_or_default()),
            &request,
            approval.as_ref(),
            true,
        )
    })())
}
