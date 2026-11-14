//! `bank_writes.py` → `am writes`.

use crate::bank_writes as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::CommandResult;
use std::path::Path;

static INSPECT: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--repo"], "repo").default("."))],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--path"], "path").required()),
        Arg::Opt(Opt::store(&["--content"], "content").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static RECOVER: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--proposal"], "proposal")),
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
static PARSER: Parser = Parser {
    description: Some(
        "Common lock and observed single-file replacement, independent of admission.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "inspect",
                help: None,
                parser: &INSPECT,
            },
            Subcommand {
                name: "preview",
                help: None,
                parser: &PREVIEW,
            },
            Subcommand {
                name: "recover",
                help: None,
                parser: &RECOVER,
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
    script: "bank_writes.py",
    module: "bank_writes",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let repo = Path::new(args.text("repo").unwrap_or("."));
    let outcome = match command {
        "inspect" => domain::inspect(repo),
        "preview" => input::read_text_universal(args.text("content").unwrap_or_default()).and_then(
            |content| {
                domain::preview(
                    repo,
                    args.text("path").unwrap_or_default(),
                    &content,
                    "replace",
                )
            },
        ),
        "apply" => input::load(args.text("proposal").unwrap_or_default())
            .and_then(|proposal| domain::apply(&proposal)),
        _ => match args.text("proposal").filter(|v| !v.is_empty()) {
            Some(path) => {
                input::load(path).and_then(|proposal| domain::recover(repo, Some(&proposal)))
            }
            None => domain::recover(repo, None),
        },
    };
    domain::command_result(command, outcome)
}
