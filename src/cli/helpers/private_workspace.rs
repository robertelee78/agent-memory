//! `private_workspace.py` → `am private`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::error::{ContractError, Result};
use crate::lifecycle::CommandResult;
use crate::private_workspace as domain;
use std::io::{self, Read};
use std::path::Path;

static CREATE: Parser = Parser {
    description: None,
    args: &[Arg::Opt(Opt::store(&["--repo"], "repo").required())],
    exclusive: &[],
    subcommands: None,
};
static PUT: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--workspace"], "workspace").required()),
        Arg::Opt(Opt::store(&["--name"], "name").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Create project-bound private artifacts without shell file operations."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "create",
                help: None,
                parser: &CREATE,
            },
            Subcommand {
                name: "put",
                help: None,
                parser: &PUT,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am private",
    module: "private_workspace",
    parser: &PARSER,
    routes: &[],
    run,
};
fn stdin() -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    io::stdin()
        .lock()
        .take(domain::MAX_INPUT as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|error| ContractError::new(error.to_string()))?;
    Ok(raw)
}
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let repo = Path::new(args.text("repo").unwrap_or_default());
    let outcome = match command {
        "create" => domain::create(repo),
        "put" => stdin().and_then(|raw| {
            domain::put(
                repo,
                args.text("workspace").unwrap_or_default(),
                args.text("name").unwrap_or_default(),
                &raw,
            )
        }),
        _ => Err(ContractError::new("Unknown private workspace command")),
    };
    domain::command_result(command, outcome)
}
