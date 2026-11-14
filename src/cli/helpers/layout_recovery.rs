//! `recover_layout.py` → `am bank recover-layout` and `am recover-layout`.
use crate::cli::parser::{Arg, Matches, Opt, Parser, Subcommand, Subcommands};
use crate::cli::{Context, Helper};
use crate::layout_recovery::{self as domain, Command};
use crate::lifecycle::CommandResult;
use std::path::Path;
const COMMON: &[Arg] = &[
    Arg::Opt(Opt::store(&["--repo"], "repo").required()),
    Arg::Opt(Opt::store(&["--operation"], "operation")),
    Arg::Opt(Opt::store(&["--input"], "input")),
    Arg::Opt(Opt::store(&["--proposal"], "proposal")),
    Arg::Opt(Opt::store(&["--output"], "output")),
];
static ORDINARY: Parser = Parser {
    description: None,
    args: COMMON,
    exclusive: &[],
    subcommands: None,
};
static INSPECT: Parser = Parser {
    description: None,
    args: &[
        COMMON[0],
        COMMON[1],
        COMMON[2],
        COMMON[3],
        COMMON[4],
        Arg::Opt(Opt::store(&["--page"], "page")),
        Arg::Opt(Opt::store(&["--identity-review"], "identity_review")),
    ],
    exclusive: &[],
    subcommands: None,
};
static RECOVER: Parser = Parser {
    description: None,
    args: &[
        COMMON[0],
        COMMON[1],
        COMMON[2],
        COMMON[3],
        COMMON[4],
        Arg::Opt(Opt::store(&["--mode"], "mode").choices(&["finish", "rollback"])),
    ],
    exclusive: &[],
    subcommands: None,
};
static NESTED: Parser = Parser {
    description: None,
    args: &[
        COMMON[0],
        COMMON[1],
        COMMON[2],
        COMMON[3],
        COMMON[4],
        Arg::Opt(Opt::store(&["--mode"], "mode").choices(&["finish", "rollback"])),
        Arg::Opt(Opt::store(&["--nested"], "nested")),
    ],
    exclusive: &[],
    subcommands: None,
};
static RESOLVE: Parser = Parser {
    description: None,
    args: &[
        COMMON[0],
        COMMON[1],
        COMMON[2],
        COMMON[3],
        COMMON[4],
        Arg::Opt(Opt::store(&["--unit"], "unit")),
    ],
    exclusive: &[],
    subcommands: None,
};
static RETENTION: Parser = Parser {
    description: None,
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "stage",
        required: true,
        commands: &[
            Subcommand {
                name: "inspect",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "apply",
                help: None,
                parser: &ORDINARY,
            },
        ],
    }),
};
static PUBLICATION: Parser = Parser {
    description: None,
    args: &[],
    exclusive: &[],
    subcommands: Some(Subcommands {
        dest: "stage",
        required: true,
        commands: &[
            Subcommand {
                name: "inspect",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "prepare",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "land",
                help: None,
                parser: &ORDINARY,
            },
        ],
    }),
};
pub static PARSER: Parser = Parser {
    description: Some(
        "Source-preserving layout recovery with immutable reviews and typed journals.",
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
                name: "prepare",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "apply",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "status",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "recover",
                help: None,
                parser: &RECOVER,
            },
            Subcommand {
                name: "rollback",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "resolve",
                help: None,
                parser: &RESOLVE,
            },
            Subcommand {
                name: "revise-units",
                help: None,
                parser: &ORDINARY,
            },
            Subcommand {
                name: "recover-nested",
                help: None,
                parser: &NESTED,
            },
            Subcommand {
                name: "retention",
                help: None,
                parser: &RETENTION,
            },
            Subcommand {
                name: "publish",
                help: None,
                parser: &PUBLICATION,
            },
        ],
    }),
};
pub static HELPER: Helper = Helper {
    script: "recover_layout.py",
    module: "layout_recovery",
    parser: &PARSER,
    routes: &[],
    run,
};
fn run(matches: &Matches, context: &Context<'_>) -> CommandResult {
    let Some((operation, args)) = matches.command() else {
        return crate::cli::refusal(
            context.prog,
            "the following arguments are required: command",
        );
    };
    let (stage, args) = if ["retention", "publish"].contains(&operation) {
        match args.command() {
            Some((stage, args)) => (Some(stage), args),
            None => {
                return crate::cli::refusal(
                    context.prog,
                    "the following arguments are required: stage",
                );
            }
        }
    } else {
        (None, args)
    };
    let page = match args.text("page") {
        None => None,
        Some(text) => match text.parse::<i128>() {
            Ok(number) => Some(number),
            Err(_) => {
                return crate::cli::refusal(context.prog, "argument --page: invalid int value");
            }
        },
    };
    let runtime_root = crate::cli::runtime_root();
    let runtime_root = match runtime_root {
        Ok(root) => root,
        Err(error) => return crate::cli::refusal(context.prog, &error.to_string()),
    };
    domain::command(
        Command {
            operation,
            stage,
            repo: Path::new(args.text("repo").unwrap_or_default()),
            operation_id: args.text("operation"),
            input: args.text("input").map(Path::new),
            proposal: args.text("proposal").map(Path::new),
            output: args.text("output").map(Path::new),
            page,
            identity_review: args.text("identity_review").map(Path::new),
            mode: args.text("mode"),
            nested: args.text("nested"),
            unit: args.text("unit"),
        },
        &runtime_root,
    )
}
