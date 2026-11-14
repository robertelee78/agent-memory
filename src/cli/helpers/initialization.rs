//! `initialize.py` → `am init`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::codec::json::Value;
use crate::initialization as domain;
use crate::lifecycle::CommandResult;
use std::path::Path;

static INSPECT: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--awa-evidence"], "awa_evidence")),
        Arg::Opt(Opt::append(&["--source"], "source")),
    ],
    exclusive: &[],
    subcommands: None,
};
static READ: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--path"], "path").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--review"], "review").required()),
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
        "Evidence-led orientation, preserving existing memory and stronger authorities.",
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
                name: "read",
                help: None,
                parser: &READ,
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
        ],
    }),
};
pub static HELPER: Helper = Helper {
    script: "initialize.py",
    module: "initialization",
    parser: &PARSER,
    routes: &[],
    run,
};

fn optional_text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |value| Value::String(value.to_owned()))
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
        "inspect" => {
            let selected = args.list("source").map_or(Value::Null, |sources| {
                Value::Array(sources.iter().cloned().map(Value::String).collect())
            });
            domain::inspect(repo, &optional_text(args.text("awa_evidence")), &selected)
        }
        "read" => domain::read_source(repo, &optional_text(args.text("path"))),
        "preview" => input::load(args.text("review").unwrap_or_default())
            .and_then(|review| domain::preview(repo, &review)),
        "apply" => input::load(args.text("proposal").unwrap_or_default())
            .and_then(|proposal| domain::apply(&proposal)),
        _ => Err(crate::error::ContractError::new(
            "Unknown initialization operation",
        )),
    };
    domain::command_result(command, outcome)
}
