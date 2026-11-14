//! `lifecycle_model.py` → `am lifecycle-model`.

use crate::cli::parser::{Arg, Matches, Opt, Parser, Positional};
use crate::cli::{Context, Helper, input};
use crate::lifecycle::{self as domain, CommandResult};

static PARSER: Parser = Parser {
    description: Some(
        "Shared deterministic lifecycle decisions; never performs a memory mutation.",
    ),
    args: &[
        Arg::Pos(Positional {
            dest: "operation",
            choices: &["definition", "guide", "current", "transition"],
            help: None,
        }),
        Arg::Opt(Opt::store(&["--input"], "input").help("JSON request path, or - for stdin")),
    ],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    command: "am lifecycle-model",
    module: "lifecycle",
    parser: &PARSER,
    routes: &[],
    run,
};

fn run(matches: &Matches, _context: &Context<'_>) -> CommandResult {
    let operation = matches.text("operation").unwrap_or_default();
    // Only current/transition read --input; an empty value is absent.
    let request = match matches.text("input").filter(|path| !path.is_empty()) {
        Some(path) if matches!(operation, "current" | "transition") => {
            input::load_text_or_stdin(path).map(Some)
        }
        _ => Ok(None),
    };
    domain::command(operation, request)
}
