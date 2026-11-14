//! Public recoverable complete-bank transactions.
use crate::bank_transactions as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;
const OPERATIONS: &[&str] = &[
    "bind",
    "capture",
    "checkpoint",
    "compact",
    "correct",
    "forget",
    "initialize",
    "recover",
    "refresh",
    "replace",
    "revoke",
    "supersede",
];
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--files"], "files").required()),
        Arg::Opt(
            Opt::store(&["--operation"], "operation")
                .choices(OPERATIONS)
                .default("compact"),
        ),
    ],
    exclusive: &[],
    subcommands: None,
};
static APPLY: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--proposal"], "proposal").required())],
    exclusive: &[],
    subcommands: None,
};
static RECOVER: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--mode"], "mode").choices(&["finish", "rollback"])),
        Arg::Opt(Opt::store(&["--proposal"], "proposal")),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Observed complete-bank transactions with verified finish/rollback recovery.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "preview",
                help: None,
                parser: &PREVIEW,
            },
            Subcommand {
                name: "apply",
                help: None,
                parser: &APPLY,
            },
            Subcommand {
                name: "recover",
                help: None,
                parser: &RECOVER,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    script: "bank_transactions.py",
    module: "bank_transactions",
    parser: &PARSER,
    routes: &[],
    run,
};
fn load(path: &str) -> crate::error::Result<crate::codec::json::Value> {
    crate::sharing_limits::read_json(Path::new(path), crate::sharing_limits::MAX_PRIVATE)
}
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let repo = Path::new(args.text("repo").unwrap_or("."));
    let outcome = match command {
        "preview" => load(args.text("files").unwrap_or_default()).and_then(|files| {
            domain::preview(repo, &files, args.text("operation").unwrap_or("compact"))
        }),
        "apply" => load(args.text("proposal").unwrap_or_default())
            .and_then(|proposal| domain::apply(&proposal)),
        _ => match args.text("proposal").filter(|v| !v.is_empty()) {
            Some(path) => load(path)
                .and_then(|proposal| domain::recover(repo, args.text("mode"), Some(&proposal))),
            None => domain::recover(repo, args.text("mode"), None),
        },
    };
    domain::command_result(command, outcome)
}
