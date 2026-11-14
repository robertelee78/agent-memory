//! `developer_identity.py` → `am developer`.

use crate::cli::parser::{Arg, Exclusive, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::codec::json::Value;
use crate::developer_identity as domain;
use crate::lifecycle::CommandResult;
use std::path::Path;

static STATUS: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::append(&["--observe"], "observe").empty()),
        Arg::Opt(Opt::store(&["--harness"], "harness")),
        Arg::Opt(Opt::store(&["--session"], "session")),
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
    ],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::flag(&["--create"], "create")),
        Arg::Opt(Opt::store(&["--adopt"], "adopt")),
        Arg::Opt(Opt::store(&["--select"], "select")),
        Arg::Opt(Opt::append(&["--label"], "label").empty()),
        Arg::Opt(Opt::append(&["--alias"], "alias").empty()),
    ],
    exclusive: &[Exclusive {
        required: true,
        dests: &["create", "adopt", "select"],
    }],
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
    args: &[Arg::Opt(Opt::store(&["--proposal"], "proposal"))],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Explicit stable developer attribution; independent of installation and banks.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "status",
                help: None,
                parser: &STATUS,
            },
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
    script: "developer_identity.py",
    module: "developer_identity",
    parser: &PARSER,
    routes: &[],
    run,
};

fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn run(matches: &crate::cli::parser::Matches, context: &Context<'_>) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let outcome = match command {
        "status" => domain::declared(args.list("observe").unwrap_or(&[])).and_then(|observed| {
            domain::status(
                observed.as_array().unwrap_or(&[]),
                args.text("harness"),
                args.text("session"),
                Path::new(args.text("repo").unwrap_or(".")),
            )
        }),
        "preview" => {
            // Python truthiness: an empty --adopt/--select value is absent.
            let adopt = args.text("adopt").filter(|v| !v.is_empty());
            let select = args.text("select").filter(|v| !v.is_empty());
            let action = if args.flag("create") {
                "create"
            } else if adopt.is_some() {
                "adopt"
            } else {
                "select"
            };
            let labels = strings(args.list("label").unwrap_or(&[]));
            domain::declared(args.list("alias").unwrap_or(&[]))
                .and_then(|aliases| domain::preview(action, adopt.or(select), &labels, &aliases))
        }
        "apply" => input::load(args.text("proposal").unwrap_or_default())
            .and_then(|proposal| domain::apply(&proposal)),
        _ => match args.text("proposal").filter(|v| !v.is_empty()) {
            Some(path) => input::load(path).and_then(|proposal| domain::recover(Some(&proposal))),
            None => domain::recover(None),
        },
    };
    domain::command_result(command, outcome)
}
