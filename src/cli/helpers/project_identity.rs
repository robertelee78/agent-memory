//! `project_identity.py` → `am project`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Positional, Subcommand, Subcommands};
use crate::cli::{Context, Helper, input};
use crate::codec::json::Value;
use crate::lifecycle::CommandResult;
use crate::project_identity as domain;
use std::path::Path;

static NORMALIZE: Parser = Parser {
    description: None,
    args: &[
        Arg::Pos(Positional {
            dest: "remote",
            choices: &[],
            help: None,
        }),
        Arg::Opt(Opt::store(&["--base"], "base")),
    ],
    exclusive: &[],
    subcommands: None,
};
static STATUS: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--authority"], "authority")),
    ],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--authority"], "authority")),
        Arg::Opt(
            Opt::store(&["--action"], "action")
                .choices(&["create", "bind", "link", "rebind", "fork"])
                .required(),
        ),
        Arg::Opt(Opt::store(&["--project-id"], "project_id")),
        Arg::Opt(Opt::store(&["--reason"], "reason").required()),
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
    args: &[Arg::Opt(Opt::store(&["--proposal"], "proposal"))],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "One portable project identity algorithm for repositories, worktrees and clones.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "normalize",
                help: None,
                parser: &NORMALIZE,
            },
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
    script: "project_identity.py",
    module: "project_identity",
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
        "normalize" => domain::normalize(
            &Value::String(args.text("remote").unwrap_or_default().to_owned()),
            args.text("base").map(Path::new),
        ),
        "status" => domain::status_with_options(
            repo,
            args.text("authority"),
            domain::DiscoveryOptions::default(),
        ),
        "preview" => domain::preview(
            repo,
            args.text("action").unwrap_or_default(),
            args.text("project_id"),
            args.text("reason").unwrap_or_default(),
            args.text("authority"),
        ),
        "apply" => input::load(args.text("proposal").unwrap_or_default())
            .and_then(|proposal| domain::apply(&proposal)),
        _ => match args.text("proposal").filter(|path| !path.is_empty()) {
            Some(path) => input::load(path).and_then(|proposal| domain::recover(Some(&proposal))),
            None => domain::recover(None),
        },
    };
    domain::command_result(command, outcome)
}
