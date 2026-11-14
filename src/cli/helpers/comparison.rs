//! Source-bound comparison; private frontiers never disclose bodies on stdout.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::{comparison as domain, sharing_limits};
use std::path::Path;
const REPO: Arg = Arg::Opt(Opt::store(&["--repo"], "repo").required());
const REQUEST: Arg = Arg::Opt(Opt::store(&["--request"], "request").required());
const OUTPUT: Arg = Arg::Opt(Opt::store(&["--private-output"], "private_output"));
static INSPECT: Parser = Parser {
    description: None,
    args: &[REPO, REQUEST, OUTPUT],
    exclusive: &[],
    subcommands: None,
};
static REVIEW: Parser = Parser {
    description: None,
    args: &[
        REPO,
        REQUEST,
        OUTPUT,
        Arg::Opt(Opt::store(&["--judgment"], "judgment").required()),
        Arg::Opt(Opt::store(&["--coverage-review"], "coverage_review")),
    ],
    exclusive: &[],
    subcommands: None,
};
static CHECK: Parser = Parser {
    description: None,
    args: &[
        REPO,
        REQUEST,
        OUTPUT,
        Arg::Opt(Opt::store(&["--proof"], "proof").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Source-bound search/read and explicit agent judgment before durable learning.",
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
                name: "review",
                help: None,
                parser: &REVIEW,
            },
            Subcommand {
                name: "check",
                help: None,
                parser: &CHECK,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    script: "compare_learning.py",
    module: "comparison",
    parser: &PARSER,
    routes: &[],
    run,
};
fn input(m: &Matches, key: &str) -> Result<Value> {
    domain::read_input(Path::new(
        m.text(key)
            .ok_or_else(|| ContractError::new("Missing comparison input"))?,
    ))
}
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, m)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let outcome = (|| {
        let repo = Path::new(
            m.text("repo")
                .ok_or_else(|| ContractError::new("Missing comparison repository"))?,
        );
        let pack = crate::cli::pack_root()?;
        let output = m.text("private_output");
        if let Some(output) = output {
            sharing_limits::preflight_private_output(Path::new(output), repo, &pack)?;
        }
        let token = crate::bank_writes::recovery_frontier_provider()?.observe(repo)?;
        let units = get(&token, "unit_ids")
            .as_array()
            .ok_or_else(|| ContractError::new("Invalid recovery frontier"))?;
        if !units.is_empty() && output.is_none() {
            return Err(ContractError::new(
                "A pending recovery frontier requires checked --private-output; bodies cannot be printed",
            ));
        }
        let request = input(m, "request")?;
        let value = match operation {
            "inspect" => domain::inspect(repo, &request, None)?,
            "review" => domain::review(
                repo,
                &request,
                &domain::cli_judgment(&input(m, "judgment")?),
                None,
                m.text("coverage_review")
                    .map(|_| input(m, "coverage_review"))
                    .transpose()?
                    .as_ref(),
            )?,
            "check" => domain::check(repo, &request, &input(m, "proof")?, None)?,
            _ => return Err(ContractError::new("Unknown comparison operation")),
        };
        let current = get(
            get(get(&value, "observation"), "recovery_frontier"),
            "unit_ids",
        )
        .as_array()
        .ok_or_else(|| ContractError::new("Invalid comparison frontier"))?;
        if output.is_some() || !current.is_empty() {
            let output = output.ok_or_else(|| {
                ContractError::new("Comparison requires checked --private-output")
            })?;
            let artifact = sharing_limits::write_private(Path::new(output), &value, repo, &pack)?;
            Ok(object([
                ("schema", string("agent-memory/comparison-result-v2")),
                ("operation", string(operation)),
                ("private_artifact", artifact),
                ("operation_ids", get(&token, "operation_ids").clone()),
                ("pending_units", Value::Int(units.len().into())),
                ("canonical_writes_performed", Value::Bool(false)),
            ]))
        } else {
            Ok(value)
        }
    })();
    domain::command_result(outcome)
}
