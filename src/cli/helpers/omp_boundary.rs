//! `omp_boundary.py` → `am omp-boundary`.
//!
//! The baseline removed `--stream` from argv before parsing and handed the
//! rest to `omp_stream.main`. Rust declares the stream options on this one
//! parser so `--stream` is accepted in any position; without `--stream` they
//! remain unrecognized arguments, as before.
use crate::cli::parser::{self, Arg, Matches, Opt, Parser};
use crate::cli::{Context, Helper};
use crate::lifecycle::CommandResult;
use crate::omp_boundary as domain;
use std::path::Path;

static PARSER: Parser = Parser {
    description: Some("Native omp boundary CLI and conservative bank-wide lifecycle coordination."),
    args: &[
        Arg::Opt(Opt::store(&["--repo"], "repo").required()),
        Arg::Opt(Opt::flag(&["--stdin"], "stdin")),
        Arg::Opt(Opt::flag(&["--list-pending"], "list_pending")),
        Arg::Opt(Opt::store(&["--offset"], "offset")),
        Arg::Opt(Opt::store(&["--source"], "source")),
        Arg::Opt(Opt::store(&["--session"], "session")),
        Arg::Opt(Opt::store(&["--lifecycle"], "lifecycle")),
        Arg::Opt(Opt::flag(&["--quiet"], "quiet")),
        Arg::Opt(Opt::flag(&["--refresh"], "refresh")),
        Arg::Opt(Opt::flag(&["--stream"], "stream")),
        Arg::Opt(Opt::store(&["--instance"], "instance")),
        Arg::Opt(Opt::store(&["--artifact"], "artifact")),
        Arg::Opt(Opt::store(
            &["--omp-agent-dir", "--agent-dir"],
            "omp_agent_dir",
        )),
    ],
    exclusive: &[],
    subcommands: None,
};

pub static HELPER: Helper = Helper {
    script: "omp_boundary.py",
    module: "omp_boundary",
    parser: &PARSER,
    routes: &[],
    run,
};

/// Options only the finite stream parser declares.
const STREAM_OPTIONS: [(&str, &str); 3] = [
    ("--instance", "instance"),
    ("--artifact", "artifact"),
    ("--omp-agent-dir", "omp_agent_dir"),
];
/// Options only the boundary parser declares.
const BOUNDARY_VALUES: [(&str, &str); 2] = [("--offset", "offset"), ("--source", "source")];
const BOUNDARY_FLAGS: [(&str, &str); 5] = [
    ("--stdin", "stdin"),
    ("--list-pending", "list_pending"),
    ("--quiet", "quiet"),
    ("--refresh", "refresh"),
    ("--stream", "stream"),
];

fn exit(exit: parser::Exit) -> CommandResult {
    CommandResult {
        value: None,
        exit_code: exit.code,
        stdout: exit.stdout,
        stderr: exit.stderr,
    }
}

/// The finite stream argv without `--stream`, in a fixed order; options the
/// stream parser does not declare are kept so it refuses them.
fn stream_argv(matches: &Matches) -> Vec<String> {
    let mut argv = Vec::new();
    for (flag, dest) in [
        ("--repo", "repo"),
        ("--session", "session"),
        ("--instance", "instance"),
        ("--lifecycle", "lifecycle"),
        ("--artifact", "artifact"),
        ("--omp-agent-dir", "omp_agent_dir"),
    ]
    .into_iter()
    .chain(BOUNDARY_VALUES)
    {
        if let Some(value) = matches.text(dest) {
            argv.push(flag.to_owned());
            argv.push(value.to_owned());
        }
    }
    for (flag, dest) in BOUNDARY_FLAGS {
        if dest != "stream" && matches.flag(dest) {
            argv.push(flag.to_owned());
        }
    }
    argv
}

/// Python `int(text)`: optional sign and ASCII digits.
fn integer(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    if matches.flag("stream") {
        return domain::stream(&stream_argv(matches), context.prog);
    }
    let extras: Vec<String> = STREAM_OPTIONS
        .iter()
        .filter_map(|(flag, dest)| matches.text(dest).map(|value| format!("{flag} {value}")))
        .collect();
    if !extras.is_empty() {
        let message = format!("unrecognized arguments: {}", extras.join(" "));
        return exit(parser::error(&PARSER, context.prog, &message));
    }
    let offset = match matches.text("offset") {
        None => 0,
        Some(text) => match integer(text) {
            Some(offset) => offset,
            None => {
                let message = format!(
                    "argument --offset: invalid int value: {}",
                    crate::console::repr(text)
                );
                return exit(parser::error(&PARSER, context.prog, &message));
            }
        },
    };
    let request = domain::Request {
        repo: Path::new(matches.text("repo").unwrap_or_default()),
        stdin: matches.flag("stdin"),
        list_pending: matches.flag("list_pending"),
        offset,
        source: matches.text("source"),
        session: matches.text("session"),
        lifecycle: matches.text("lifecycle"),
        quiet: matches.flag("quiet"),
        refresh: matches.flag("refresh"),
    };
    domain::command(&request, &mut std::io::stdin().lock())
}
