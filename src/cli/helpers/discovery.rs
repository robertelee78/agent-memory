//! Source-bound semantic discovery, including pending-recovery private output.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object, string};
use std::path::Path;
static START: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--task"], "task").required()),
        Arg::Opt(Opt::store(&["--intent"], "intent").required().choices(&[
            "recall",
            "comparison",
            "sharing",
        ])),
        Arg::Opt(Opt::store(&["--context"], "context")),
        Arg::Opt(Opt::store(&["--private-output"], "private_output")),
    ],
    exclusive: &[],
    subcommands: None,
};
static NODE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--path"], "path").required()),
        Arg::Opt(Opt::store(&["--private-output"], "private_output")),
    ],
    exclusive: &[],
    subcommands: None,
};
static READ: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--request"], "request").required()),
        Arg::Opt(Opt::store(&["--rationale"], "rationale").required()),
        Arg::Opt(Opt::store(&["--private-output"], "private_output")),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some("Source-bound observations for semantic navigation by the invoking harness."),
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "command",
        required: true,
        commands: &[
            Subcommand {
                name: "start",
                help: None,
                parser: &START,
            },
            Subcommand {
                name: "node",
                help: None,
                parser: &NODE,
            },
            Subcommand {
                name: "read",
                help: None,
                parser: &READ,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am discover",
    module: "discovery",
    parser: &PARSER,
    routes: &[],
    run,
};
fn input(path: &str) -> Result<Value> {
    crate::discovery::read_input(Path::new(path))
}
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let mut phase = "input";
    let result = (|| {
        let repo = Path::new(args.text("repo").unwrap_or_default());
        let frontier = crate::bank_writes::recovery_frontier_provider()?.observe(repo)?;
        let private = args.text("private_output").map(Path::new);
        let pack = if private.is_some() {
            Some(crate::cli::pack_root()?)
        } else {
            None
        };
        if let (Some(path), Some(pack)) = (private, pack.as_ref()) {
            crate::sharing_limits::preflight_private_output(path, repo, pack)?;
        }
        let pending = get(&frontier, "unit_ids")
            .as_array()
            .map_or(0, |rows| rows.len());
        if pending > 0 && private.is_none() {
            return Err(ContractError::new(
                "Pending recovery navigation requires private output for its source-bound trace",
            ));
        }
        let result = if operation == "start" {
            let supplied = args
                .text("context")
                .map(input)
                .transpose()?
                .unwrap_or_else(|| object(Vec::<(String, Value)>::new()));
            crate::scopes::conditions(&supplied, false)?;
            phase = "root-observation";
            crate::discovery::start(
                repo,
                &string(args.text("task").unwrap_or_default()),
                args.text("intent").unwrap_or_default(),
                &supplied,
            )?
        } else {
            let input = input(args.text("input").unwrap_or_default())?;
            let trace = input.get("discovery").unwrap_or(&input);
            if operation == "node" {
                phase = "navigation-node";
                crate::discovery::node(repo, trace, args.text("path").unwrap_or_default())?
            } else {
                let request = self::input(args.text("request").unwrap_or_default())?;
                phase = "canonical-qualification";
                crate::discovery::qualified_read(
                    repo,
                    trace,
                    &request,
                    &string(args.text("rationale").unwrap_or_default()),
                )?
            }
        };
        if pending == 0 {
            return Ok(result);
        }
        let path = private
            .ok_or_else(|| ContractError::new("Pending recovery requires private output"))?;
        let pack =
            pack.ok_or_else(|| ContractError::new("Private output requires runtime root"))?;
        Ok(object([
            ("schema", string("agent-memory/discovery-result-v1")),
            ("operation", string(operation)),
            (
                "private_artifact",
                crate::sharing_limits::write_private(path, &result, repo, &pack)?,
            ),
            ("pending_recovery_units", Value::Int(pending.into())),
            ("canonical_writes_performed", Value::Bool(false)),
        ]))
    })();
    crate::discovery::command_result(operation, phase, result)
}
