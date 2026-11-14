//! Governed lifecycle operations; inputs use the protected admission reader.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::governance as domain;
use crate::lifecycle::CommandResult;
use std::path::Path;

const REPO: Arg = Arg::Opt(Opt::store(&["--repo"], "repo").required());
const REQUEST: Arg = Arg::Opt(Opt::store(&["--request"], "request").required());
const PRIVATE_OUTPUT: Arg = Arg::Opt(Opt::store(&["--private-output"], "private_output"));
static PLAN: Parser = Parser {
    description: None,
    args: &[REPO, REQUEST, PRIVATE_OUTPUT],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        REPO,
        REQUEST,
        PRIVATE_OUTPUT,
        Arg::Opt(Opt::store(&["--plan"], "plan")),
        Arg::Opt(Opt::store(&["--operation-id"], "operation_id")),
        Arg::Opt(Opt::store(&["--recorded-at"], "recorded_at")),
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
        "Evidence-bound lifecycle governance, with no age-based or implicit deletion.",
    ),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "plan",
                help: None,
                parser: &PLAN,
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
    script: "govern_memory.py",
    module: "governance",
    parser: &PARSER,
    routes: &[],
    run,
};
fn input(m: &Matches, name: &str, maximum: usize) -> Result<Value> {
    let path = m
        .text(name)
        .ok_or_else(|| ContractError::new("Missing governance input"))?;
    crate::admission::read_input(Path::new(path), maximum)
}
fn execute(operation: &str, m: &Matches, context: &Context<'_>) -> Result<Value> {
    let preparing = matches!(operation, "plan" | "preview");
    let prepared = if preparing {
        input(m, "request", domain::REQUEST_INPUT_LIMIT)?
    } else {
        input(m, "proposal", domain::PROPOSAL_INPUT_LIMIT)?
    };
    domain::require_intent(&prepared, preparing, context.required_operations)?;
    let repo = Path::new(m.text("repo").unwrap_or("."));
    let value = match operation {
        "plan" => domain::plan(repo, &prepared)?,
        "preview" => {
            let plan = m
                .text("plan")
                .map(|_| input(m, "plan", domain::PROPOSAL_INPUT_LIMIT))
                .transpose()?;
            domain::preview(
                repo,
                &prepared,
                plan.as_ref(),
                m.text("operation_id"),
                m.text("recorded_at"),
            )?
        }
        "apply" => return domain::apply(&prepared),
        _ => return Err(ContractError::new("Unknown governance operation")),
    };
    super::admission::private_preview(repo, value, m.text("private_output"))
}
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, m)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    domain::command_result(operation, execute(operation, m, context))
}
