//! `record_contract.py` → `am record-contract`.

use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::codec::json::Value;
use crate::lifecycle::CommandResult;
use crate::records as domain;

/// `POLICY_FIELDS`: the record schema's `x-policy-fields`.
pub fn policy_fields() -> Vec<String> {
    domain::MODEL
        .get("x-policy-fields")
        .and_then(Value::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

const REQUIRE: Opt = Opt::append(&["--require"], "require")
    .empty()
    .computed(policy_fields);

static MODEL: Parser = Parser::EMPTY;
static RENDER: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(REQUIRE),
    ],
    exclusive: &[],
    subcommands: None,
};
static VALIDATE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--file"], "file").required()),
        Arg::Opt(REQUIRE),
    ],
    exclusive: &[],
    subcommands: None,
};
static APPLICABILITY: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--file"], "file").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(REQUIRE),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Read-only standalone Markdown record and optional history format contract."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "model",
                help: None,
                parser: &MODEL,
            },
            Subcommand {
                name: "render",
                help: None,
                parser: &RENDER,
            },
            Subcommand {
                name: "validate",
                help: None,
                parser: &VALIDATE,
            },
            Subcommand {
                name: "applicability",
                help: None,
                parser: &APPLICABILITY,
            },
        ],
    }),
};

pub static HELPER: Helper = Helper {
    command: "am record-contract",
    module: "records",
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
    let required: Vec<&str> = args
        .list("require")
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .collect();
    // render reads --input; validate and applicability read --file first.
    let content = match args.text("file") {
        Some(path) if command != "render" => input::read_text(path).map(Some),
        _ => Ok(None),
    };
    let request = match args.text("input") {
        Some(path) if command != "validate" => input::load(path).map(Some),
        _ => Ok(None),
    };
    domain::command(command, request, content, &required)
}
