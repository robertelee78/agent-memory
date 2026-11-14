//! Protected admission commands; inputs never pass through the generic reader.
use crate::admission as domain;
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object};
use std::path::Path;

const REPO: Arg = Arg::Opt(Opt::store(&["--repo"], "repo").required());
const INPUT: Arg = Arg::Opt(Opt::store(&["--input"], "input").required());
static DRAFT: Parser = Parser {
    description: None,
    args: &[REPO, INPUT],
    exclusive: &[],
    subcommands: None,
};
static CAPTURE: Parser = Parser {
    description: None,
    args: &[
        REPO,
        Arg::Opt(Opt::store(&["--request"], "request").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PREVIEW: Parser = Parser {
    description: None,
    args: &[
        REPO,
        Arg::Opt(Opt::store(&["--capture"], "capture").required()),
        Arg::Opt(Opt::store(&["--request"], "request").required()),
        Arg::Opt(Opt::store(&["--proof"], "proof").required()),
        Arg::Opt(Opt::store(&["--navigation"], "navigation")),
        Arg::Opt(Opt::store(&["--operation-id"], "operation_id")),
        Arg::Opt(Opt::store(&["--recorded-at"], "recorded_at")),
        Arg::Opt(Opt::store(&["--private-output"], "private_output")),
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
    description: Some("Small source-bound capture and governed admission, without publication."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "draft",
                help: None,
                parser: &DRAFT,
            },
            Subcommand {
                name: "start",
                help: None,
                parser: &DRAFT,
            },
            Subcommand {
                name: "capture",
                help: None,
                parser: &CAPTURE,
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
    command: "am admission",
    module: "admission",
    parser: &PARSER,
    routes: &[],
    run,
};
fn input(m: &Matches, name: &str, maximum: usize) -> Result<Value> {
    let path = m
        .text(name)
        .ok_or_else(|| ContractError::new("Missing admission input"))?;
    domain::read_input(Path::new(path), maximum)
}
pub(crate) fn private_preview(repo: &Path, value: Value, output: Option<&str>) -> Result<Value> {
    let token = crate::bank_writes::recovery_frontier_provider()?.observe(repo)?;
    let units = get(&token, "unit_ids")
        .as_array()
        .ok_or_else(|| ContractError::new("Invalid recovery frontier"))?;
    if output.is_some()
        || get(&value, "output_plan") != &Value::Null
        || (get(&value, "proposal") != &Value::Null && !units.is_empty())
    {
        let output = output.ok_or_else(|| ContractError::new("Governed output plans and recovery-bound proposals require checked --private-output"))?;
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
        "draft" => domain::draft(repo, &input(m, "input", domain::REQUEST_LIMIT)?),
        "start" => domain::start(repo, &input(m, "input", domain::REQUEST_LIMIT)?),
        "capture" => domain::capture(repo, &input(m, "request", domain::REQUEST_LIMIT)?),
        "preview" => private_preview(
            repo,
            domain::preview(
                repo,
                &input(m, "capture", domain::INPUT_LIMIT)?,
                &input(m, "request", domain::INPUT_LIMIT)?,
                &input(m, "proof", domain::INPUT_LIMIT)?,
                m.text("navigation")
                    .map(|_| input(m, "navigation", domain::REQUEST_LIMIT))
                    .transpose()?
                    .as_ref(),
                m.text("operation_id"),
                m.text("recorded_at"),
            )?,
            m.text("private_output"),
        ),
        "apply" => domain::apply(&input(m, "proposal", crate::sharing_limits::MAX_PRIVATE)?),
        _ => Err(ContractError::new("Unknown admission operation")),
    })();
    domain::command_result(operation, outcome)
}
