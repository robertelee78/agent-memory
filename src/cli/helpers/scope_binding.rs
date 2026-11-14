//! `scope_binding.py` → `am identity`.
use crate::cli::parser::{Arg, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::codec::json::Value;
use crate::lifecycle::CommandResult;
use crate::scope_binding as domain;
use std::path::Path;

static STATUS: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--harness"], "harness")),
        Arg::Opt(Opt::store(&["--session"], "session")),
        Arg::Opt(Opt::append(&["--observe"], "observe").empty()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(
            Opt::store(&["--action"], "action")
                .choices(&["create", "bind", "link", "rebind", "fork"])
                .required(),
        ),
        Arg::Opt(Opt::store(&["--project-id"], "project_id")),
        Arg::Opt(Opt::store(&["--authority"], "authority")),
        Arg::Opt(Opt::store(&["--reason"], "reason").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static REQUIRE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::flag(&["--initialize"], "initialize")),
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
        "Shared attribution/project resolution and explicit, recoverable scope binding.",
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
                name: "require",
                help: None,
                parser: &REQUIRE,
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
    command: "am identity",
    module: "scope_binding",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &crate::cli::parser::Matches, context: &Context<'_>) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let repo = Path::new(args.text("repo").unwrap_or("."));
    let outcome = match command {
        "status" => crate::developer_identity::declared(args.list("observe").unwrap_or(&[]))
            .and_then(|observed| {
                domain::status(
                    repo,
                    observed.as_array().unwrap_or(&[]),
                    args.text("harness"),
                    args.text("session"),
                )
            }),
        "preview" => domain::preview(
            repo,
            args.text("action").unwrap_or_default(),
            args.text("project_id"),
            args.text("reason").unwrap_or_default(),
            args.text("authority"),
        ),
        "require" => domain::require(
            repo,
            None,
            domain::RequireOptions {
                initialize: args.flag("initialize"),
                ..Default::default()
            },
        )
        .map(|scope| {
            domain::result(
                "require",
                [
                    ("context", scope),
                    ("writes_performed", Value::Bool(false)),
                    ("verified", Value::Bool(true)),
                ],
            )
        }),
        "apply" => input::load(args.text("proposal").unwrap_or_default())
            .and_then(|proposal| domain::apply(&proposal)),
        _ => Err(crate::error::ContractError::new(
            "Unknown scope binding operation",
        )),
    };
    domain::command_result(command, outcome)
}
