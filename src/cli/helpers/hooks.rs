//! `am hook <name>`: the client hook commands installed into Codex, Claude
//! and OpenCode configuration (baseline `hooks/*.py`, started through
//! `hooks/python3.sh`). Each reads its bounded event on stdin.
use crate::cli::parser::{Arg, Matches, Parser, Positional};
use crate::cli::{Context, Native};
use crate::hooks;
use crate::lifecycle::CommandResult;
use std::io::Read;

static PARSER: Parser = Parser {
    description: Some("Run one installed client hook on its harness event (stdin)."),
    args: &[Arg::Pos(Positional {
        dest: "name",
        choices: &hooks::NAMES,
        help: None,
    })],
    exclusive: &[],
    subcommands: None,
};

pub const HOOK_NATIVE: Native = Native {
    name: "hook",
    help: "Client hook adapter; reads one harness event on stdin",
    parser: &PARSER,
    run,
};

/// `sys.stdin.buffer.read(limit)`: up to `limit` bytes, never draining more.
fn input(limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(u64::try_from(limit).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let name = matches.text("name").unwrap_or_default();
    let Some(limit) = hooks::input_limit(name) else {
        return crate::cli::refusal(context.prog, "unknown hook");
    };
    // An unreadable stdin is an invalid event: each hook reports it in its
    // own fixed failure form.
    let raw = input(limit).unwrap_or_default();
    let Some(output) = hooks::run(name, &raw, &crate::cli::pack_root) else {
        return crate::cli::refusal(context.prog, "unknown hook");
    };
    CommandResult {
        value: None,
        exit_code: output.exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
    }
}
