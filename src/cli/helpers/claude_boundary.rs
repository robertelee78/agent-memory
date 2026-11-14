//! `claude_boundary.py` → `am claude-boundary`.
//!
//! The three page offsets are argparse `type=int` options; an invalid value
//! is the baseline argparse error, exit 2.
use crate::claude_boundary::{self as domain, PageOffset};
use crate::cli::parser::{self, Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use std::path::Path;

static PARSER: Parser = Parser {
    description: Some(
        "Acknowledge one source-bound Claude boundary without inventing publication.",
    ),
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input")),
        Arg::Opt(Opt::flag(&["--list-pending"], "list_pending")),
        Arg::Opt(Opt::store(&["--session"], "session")),
        Arg::Opt(Opt::store(&["--prompt"], "prompt")),
        Arg::Opt(Opt::flag(&["--quiet"], "quiet")),
        Arg::Opt(Opt::flag(&["--refresh"], "refresh")),
        Arg::Opt(Opt::store(&["--offset"], "offset")),
        Arg::Opt(Opt::store(&["--legacy-offset"], "legacy_offset")),
        Arg::Opt(Opt::store(&["--enrollment-offset"], "enrollment_offset")),
    ],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    script: "claude_boundary.py",
    module: "claude_boundary",
    parser: &PARSER,
    routes: &[],
    run,
};

/// The parsed `type=int` page offset of `flag` (`0` when absent), or the
/// argparse error for an invalid value.
pub(crate) fn offset(
    parser: &Parser<'_>,
    matches: &Matches,
    context: &Context<'_>,
    flag: &str,
    dest: &str,
) -> Result<PageOffset, CommandResult> {
    let Some(text) = matches.text(dest) else {
        return Ok(PageOffset::default());
    };
    PageOffset::parse(text).ok_or_else(|| {
        let message = format!(
            "argument {flag}: invalid int value: {}",
            crate::console::repr(text)
        );
        let exit = parser::error(parser, context.prog, &message);
        CommandResult {
            value: None,
            exit_code: exit.code,
            stdout: exit.stdout,
            stderr: exit.stderr,
        }
    })
}

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let mut offsets = [PageOffset::default(); 3];
    for (slot, (flag, dest)) in offsets.iter_mut().zip([
        ("--offset", "offset"),
        ("--legacy-offset", "legacy_offset"),
        ("--enrollment-offset", "enrollment_offset"),
    ]) {
        match offset(&PARSER, matches, context, flag, dest) {
            Ok(value) => *slot = value,
            Err(result) => return result,
        }
    }
    let [offset, legacy_offset, enrollment_offset] = offsets;
    let request = domain::Request {
        repo: Path::new(matches.text("repo").unwrap_or_default()),
        input: matches.text("input"),
        list_pending: matches.flag("list_pending"),
        session: matches.text("session"),
        prompt: matches.text("prompt"),
        quiet: matches.flag("quiet"),
        refresh: matches.flag("refresh"),
        offset,
        legacy_offset,
        enrollment_offset,
    };
    domain::command(&request, &mut std::io::stdin().lock())
}
