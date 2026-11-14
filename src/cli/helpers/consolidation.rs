//! Governed maintenance with protected, bounded proposal inputs.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object};
use crate::{admission, consolidation as domain};
use std::path::Path;
const REPO: Arg = Arg::Opt(Opt::store(&["--repo"], "repo").required());
const INPUT: Arg = Arg::Opt(Opt::store(&["--input"], "input").required());
const OUTPUT: Arg = Arg::Opt(Opt::store(&["--private-output"], "private_output"));
static PLAN: Parser = Parser {
    description: None,
    args: &[REPO, INPUT, OUTPUT],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        REPO,
        INPUT,
        OUTPUT,
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
        "Source-bound, meaning-preserving maintenance; no daemon or procedure promotion.",
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
    command: "am compact",
    module: "consolidation",
    parser: &PARSER,
    routes: &[],
    run,
};
fn input(m: &Matches, name: &str, maximum: usize) -> Result<Value> {
    admission::read_input(
        Path::new(
            m.text(name)
                .ok_or_else(|| ContractError::new("Missing maintenance input"))?,
        ),
        maximum,
    )
}
fn private_preview(repo: &Path, value: Value, output: Option<&str>) -> Result<Value> {
    let token = crate::bank_writes::recovery_frontier_provider()?.observe(repo)?;
    let units = get(&token, "unit_ids")
        .as_array()
        .ok_or_else(|| ContractError::new("Invalid recovery frontier"))?;
    if output.is_some()
        || get(&value, "output_plan") != &Value::Null
        || (get(&value, "proposal") != &Value::Null && !units.is_empty())
    {
        let output=output.ok_or_else(|| ContractError::new("Governed output plans and recovery-bound proposals require checked --private-output"))?;
        let artifact = crate::sharing_limits::write_private(
            Path::new(output),
            &value,
            repo,
            &crate::cli::pack_root()?,
        )?;
        return Ok(object([
            ("schema", get(&value, "schema").clone()),
            ("operation", get(&value, "operation").clone()),
            ("private_artifact", artifact),
            ("pending_recovery_units", Value::Int(units.len().into())),
            ("canonical_writes_performed", Value::Bool(false)),
        ]));
    }
    Ok(value)
}
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, m)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let repo = Path::new(m.text("repo").unwrap_or("."));
    let outcome = (|| match operation {
        "plan" => private_preview(
            repo,
            domain::plan(repo, &input(m, "input", domain::INPUT_LIMIT)?)?,
            m.text("private_output"),
        ),
        "preview" => private_preview(
            repo,
            domain::preview(
                repo,
                &input(m, "input", domain::INPUT_LIMIT)?,
                m.text("plan")
                    .map(|_| input(m, "plan", crate::sharing_limits::MAX_PRIVATE))
                    .transpose()?
                    .as_ref(),
                m.text("operation_id"),
                m.text("recorded_at"),
            )?,
            m.text("private_output"),
        ),
        "apply" => domain::apply(&input(m, "proposal", crate::sharing_limits::MAX_PRIVATE)?),
        _ => Err(ContractError::new("Unknown maintenance operation")),
    })();
    domain::command_result(operation, outcome)
}
