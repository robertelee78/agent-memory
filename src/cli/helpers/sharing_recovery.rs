//! `sharing_recovery.py` → `am exchange`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use crate::sharing_recovery as domain;
use std::path::Path;
static INSPECT: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::flag(&["--fetch"], "fetch")),
    ],
    exclusive: &[],
    subcommands: None,
};
static RECONCILE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--base"], "base")),
    ],
    exclusive: &[],
    subcommands: None,
};
static REPLAY: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--request"], "request").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Fresh Git observations, publication replay readback and deletion-aware merge plans.",
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
                name: "reconcile",
                help: None,
                parser: &RECONCILE,
            },
            Subcommand {
                name: "replay",
                help: None,
                parser: &REPLAY,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am exchange",
    module: "sharing_recovery",
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
    let repo = Path::new(args.text("repo").unwrap_or_default());
    let fetch = args.flag("fetch");
    let outcome = match operation {
        "inspect" => domain::inspect(repo, fetch),
        "reconcile" => domain::reconcile(repo, args.text("base")),
        _ => crate::sharing_limits::read_json(
            Path::new(args.text("request").unwrap_or_default()),
            domain::MAX_REQUEST,
        )
        .and_then(|request| domain::replay(repo, &request, false)),
    };
    domain::command_result(operation, fetch, outcome)
}
