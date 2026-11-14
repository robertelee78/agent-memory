//! `migrate_bank.py` → `am migrate`.

use crate::cli::parser::{self, Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::error::{ContractError, Result};
use crate::lifecycle::CommandResult;
use crate::migration::{self as domain, Request, Runtime};
use std::sync::Arc;

static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--to"], "to").required()),
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
        Arg::Opt(Opt::store(&["--operation"], "operation")),
        Arg::Opt(Opt::store(&["--proposal"], "proposal")),
    ],
    exclusive: &[],
    subcommands: None,
};
static RECOVER_LOCK: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--proposal"], "proposal")),
    ],
    exclusive: &[],
    subcommands: None,
};
static RESTORE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").default(".")),
        Arg::Opt(Opt::store(&["--operation"], "operation")),
        Arg::Opt(Opt::store(&["--proposal"], "proposal")),
    ],
    exclusive: &[],
    subcommands: None,
};
pub static PARSER: Parser = Parser {
    description: Some("Explicit source-bound format conversion; runtime updates never invoke it."),
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
            Subcommand {
                name: "recover-lock",
                help: None,
                parser: &RECOVER_LOCK,
            },
            Subcommand {
                name: "restore",
                help: None,
                parser: &RESTORE,
            },
        ],
    }),
};

pub static HELPER: Helper = Helper {
    script: "migrate_bank.py",
    module: "migration",
    parser: &PARSER,
    routes: &[],
    run,
};

/// The running executable's self-located runtime with the compiled codecs
/// and this binary's own command surface.
pub fn current_runtime() -> Result<Runtime> {
    let executable =
        std::env::current_exe().map_err(|error| ContractError::new(error.to_string()))?;
    let surface = crate::cli::surface::Declared::current()?;
    Runtime::locate(
        &executable,
        Arc::new(crate::migration_codecs::Compiled),
        Some(Arc::new(surface)),
    )
}

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    run_with(matches, context.prog, current_runtime)
}

/// Run parsed `migrate` arguments under the runtime `runtime` supplies.
pub fn run_with(
    matches: &Matches,
    prog: &str,
    runtime: impl FnOnce() -> Result<Runtime>,
) -> CommandResult {
    let Some((command, args)) = matches.command() else {
        return crate::cli::refusal(prog, "the following arguments are required: command");
    };
    let request = Request {
        command,
        repo: args.text("repo").unwrap_or("."),
        to: args.text("to"),
        proposal: args.text("proposal"),
        mode: args.text("mode"),
        operation: args.text("operation"),
    };
    domain::command(runtime(), &request)
}

/// Parse `args` with the `migrate` parser and run them under `runtime`
/// (an isolated test runtime, or a retained trusted package).
pub fn dispatch_with(
    args: &[String],
    prog: &str,
    runtime: impl FnOnce() -> Result<Runtime>,
) -> CommandResult {
    match parser::parse(&PARSER, prog, args) {
        Ok(matches) => run_with(&matches, prog, runtime),
        Err(exit) => CommandResult {
            value: None,
            exit_code: exit.code,
            stdout: exit.stdout,
            stderr: exit.stderr,
        },
    }
}
