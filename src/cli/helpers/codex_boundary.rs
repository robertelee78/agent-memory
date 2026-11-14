//! `codex_boundary.py` → `am codex-boundary`.
use super::claude_boundary::offset;
use crate::claude_boundary::PageOffset;
use crate::cli::parser::{Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::codex_boundary as domain;
use crate::lifecycle::CommandResult;
use std::path::Path;

static PARSER: Parser = Parser {
    description: Some("Acknowledge one source-bound Codex boundary without inventing publication."),
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::store(&["--input"], "input")),
        Arg::Opt(Opt::flag(&["--list-pending"], "list_pending")),
        Arg::Opt(Opt::flag(&["--stdin"], "stdin")),
        Arg::Opt(Opt::store(&["--offset"], "offset")),
        Arg::Opt(Opt::store(&["--legacy-offset"], "legacy_offset")),
        Arg::Opt(Opt::flag(&["--quiet"], "quiet")),
        Arg::Opt(Opt::flag(&["--refresh"], "refresh")),
        Arg::Opt(Opt::store(&["--enrollment-offset"], "enrollment_offset")),
    ],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    script: "codex_boundary.py",
    module: "codex_boundary",
    parser: &PARSER,
    routes: &[],
    run,
};

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
        stdin: matches.flag("stdin"),
        offset,
        legacy_offset,
        quiet: matches.flag("quiet"),
        refresh: matches.flag("refresh"),
        enrollment_offset,
    };
    domain::command(&request, &mut std::io::stdin().lock())
}
