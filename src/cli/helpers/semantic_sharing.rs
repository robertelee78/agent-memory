//! `reconcile_memory.py` → `am semantic`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::codec::json::Value;
use crate::error::Result;
use crate::lifecycle::get;
use crate::semantic_sharing::{self as domain, array, err, set};
use crate::{sharing_frontier as frontier, sharing_limits as limits};
use std::path::Path;
static INSPECT: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--base"], "base")),
        Arg::Opt(Opt::store(&["--output"], "output")),
    ],
    exclusive: &[],
    subcommands: None,
};
static INPUT: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--output"], "output")),
    ],
    exclusive: &[],
    subcommands: None,
};
static CHECK: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--output"], "output")),
        Arg::Opt(Opt::flag(&["--v2"], "v2")),
    ],
    exclusive: &[],
    subcommands: None,
};
static INSPECT_V2: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input")),
        Arg::Opt(Opt::store(&["--output"], "output").required()),
        Arg::Opt(Opt::store(&["--pages-dir"], "pages_dir")),
    ],
    exclusive: &[],
    subcommands: None,
};
static PAGE: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--page"], "page").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static REVIEW_V2: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--output"], "output").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static INDEXED: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--inspection"], "inspection").required()),
        Arg::Opt(Opt::store(&["--input"], "input").required()),
        Arg::Opt(Opt::store(&["--output"], "output").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static RECOVER: Parser = Parser {
    description: None,
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--project-id"], "project_id").required()),
    ],
    exclusive: &[],
    subcommands: None,
};
static PARSER: Parser = Parser {
    description: Some(
        "Source-bound semantic review of repository-local and landed shared learning.",
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
                parser: &INPUT,
            },
            Subcommand {
                name: "check",
                help: None,
                parser: &CHECK,
            },
            Subcommand {
                name: "inspect-v2",
                help: None,
                parser: &INSPECT_V2,
            },
            Subcommand {
                name: "page-v2",
                help: None,
                parser: &PAGE,
            },
            Subcommand {
                name: "worklist-v2",
                help: None,
                parser: &PAGE,
            },
            Subcommand {
                name: "review-v2",
                help: None,
                parser: &REVIEW_V2,
            },
            Subcommand {
                name: "review-indexed-v2",
                help: None,
                parser: &INDEXED,
            },
            Subcommand {
                name: "recover-holds",
                help: None,
                parser: &RECOVER,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    command: "am semantic",
    module: "semantic_sharing",
    parser: &PARSER,
    routes: &[],
    run,
};
fn execute(operation: &str, args: &Matches) -> Result<Value> {
    let repo = Path::new(args.text("repo").unwrap_or_default());
    let output = args.text("output").map(Path::new);
    let pages_dir = args.text("pages_dir").map(Path::new);
    if let (Some(output), Some(pages_dir)) = (output, pages_dir) {
        crate::sharing_page_export::preflight(repo, output, pages_dir)?;
    } else if let Some(output) = output {
        limits::preflight_private_output(output, repo, &crate::cli::pack_root()?)
            .map_err(|error| crate::error::ContractError::private_output(error.to_string()))?;
    }
    let input = || {
        domain::read_input(
            Path::new(args.text("input").unwrap_or_default()),
            operation.ends_with("-v2") || args.flag("v2"),
        )
    };
    let mut value = match operation {
        "recover-holds" => crate::sharing_holds::recover(
            repo,
            args.text("project_id").unwrap_or_default(),
            &crate::cli::pack_root()?,
        )?,
        "review-indexed-v2" => domain::compact_review(
            repo,
            &domain::read_input(Path::new(args.text("inspection").unwrap_or_default()), true)?,
            &limits::read_json(
                Path::new(args.text("input").unwrap_or_default()),
                limits::MAX_PRIVATE,
            )?,
        )?,
        "inspect-v2" => {
            let request = args
                .text("input")
                .map(|path| domain::read_input(Path::new(path), true))
                .transpose()?;
            if let (Some(output), Some(pages_dir)) = (output, pages_dir) {
                crate::sharing_page_export::export(repo, output, pages_dir, request.as_ref())?
            } else {
                frontier::inspect(repo, request.as_ref(), None)?
            }
        }
        "page-v2" | "worklist-v2" => {
            let manifest = input()?;
            let index = args
                .text("page")
                .unwrap_or_default()
                .parse::<usize>()
                .map_err(|_| {
                    err("Missing, mixed or stale page observation; inspect actual sources again")
                })?;
            let mut value = frontier::page(repo, &manifest, index, None)?;
            if operation == "worklist-v2" {
                let descriptor = array(get(&manifest, "pages"))?
                    .get(index)
                    .ok_or_else(|| err("Missing review page descriptor"))?;
                set(&mut value, "page_sha256", get(descriptor, "sha256").clone())?;
                let required = array(get(&manifest, "required_pairs"))?;
                let mut items = Vec::new();
                for item in array(get(&value, "items"))? {
                    let mut item = item.clone();
                    if let Some(edge) = item.get("edge") {
                        let index = required
                            .iter()
                            .position(|pair| pair == edge)
                            .ok_or_else(|| err("Missing required review edge"))?;
                        set(&mut item, "pair_index", Value::Int(index.into()))?;
                    }
                    items.push(item);
                }
                set(&mut value, "items", Value::Array(items))?;
            }
            value
        }
        "review" | "review-v2" => domain::review(repo, &input()?, None)?,
        "inspect" => domain::inspect(repo, args.text("base"), None)?,
        "check" => domain::check(repo, &input()?, None)?,
        _ => return Err(err("Unknown semantic review command")),
    };
    if let Some(output) = output.filter(|_| pages_dir.is_none()) {
        value = limits::write_private(output, &value, repo, &crate::cli::pack_root()?)
            .map_err(|error| crate::error::ContractError::private_output(error.to_string()))?;
    } else if get(get(&value, "proof"), "schema").as_str() == Some(frontier::PROOF) {
        if let Value::Object(pairs) = &mut value {
            pairs.retain(|(name, _)| !["refresh_files", "publish_files"].contains(&name.as_str()));
        }
    }
    Ok(value)
}
fn run(matches: &Matches, context: &Context<'_>) -> crate::lifecycle::CommandResult {
    let Some((operation, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    crate::validation_memo::operation(|_| {
        domain::command_result(operation, execute(operation, args))
    })
}
