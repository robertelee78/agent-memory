//! The single CLI dispatch of the port: one declarative command table whose
//! routes reach typed domain functions, never script paths.
//!
//! Baseline: `cli.py` (`arguments`, `execute` with the `intent` mapping) and
//! the `MEMORY_ROUTES` delegation of `installation.main`/`bank_contract.main`.
//! The table lives in [`table`]; one registration line per command.

pub mod helpers;
pub mod input;
pub mod parser;
pub mod surface;
pub mod table;
mod wiring;

use crate::error::{ContractError, Result};
use crate::lifecycle::CommandResult;
use parser::{Matches, Parser};
use std::path::PathBuf;
use std::sync::Arc;

/// A typed handler: the parsed namespace of its helper parser.
pub type Handler = fn(&Matches, &Context<'_>) -> CommandResult;

/// What a handler may know about its invocation.
pub struct Context<'a> {
    /// argparse prog of the helper (`am developer`, `am`, `am bank`).
    pub prog: &'a str,
    /// The `required_operations` keyword the baseline passed for intent routes.
    pub required_operations: Option<&'static [&'static str]>,
}

/// One baseline helper script: its parser declaration and typed handler.
pub struct Helper {
    /// Baseline wrapper file name under `scripts/`, e.g. `developer_identity.py`.
    pub script: &'static str,
    /// Baseline implementation module under `scripts/agent_memory/`.
    pub module: &'static str,
    pub parser: &'static Parser<'static>,
    /// Literal delegated routes of this helper (its own `MEMORY_ROUTES`).
    pub routes: &'static [Route],
    pub run: Handler,
}

/// A delegated route: `name` runs `helper` with `prefix` prepended.
pub struct Route {
    pub name: &'static str,
    pub helper: &'static Helper,
    pub prefix: &'static [&'static str],
}

/// A root subcommand parsed by the root parser itself (installation's own
/// `install`, `update`, … subparsers).
pub struct Native {
    pub name: &'static str,
    pub help: &'static str,
    pub parser: &'static Parser<'static>,
    pub run: Handler,
}

pub enum Entry {
    Route(Route),
    Native(Native),
}

impl Entry {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Route(route) => route.name,
            Self::Native(native) => native.name,
        }
    }
}

/// A dispatched command; `exited` marks argparse's help/usage SystemExit.
pub struct Dispatched {
    pub result: CommandResult,
    pub exited: bool,
}

impl Dispatched {
    fn exit(exit: parser::Exit) -> Self {
        Self {
            result: CommandResult {
                value: None,
                exit_code: exit.code,
                stdout: exit.stdout,
                stderr: exit.stderr,
            },
            exited: true,
        }
    }
    fn completed(result: CommandResult) -> Self {
        Self {
            result,
            exited: false,
        }
    }
}

/// Every public operation already emits JSON; drop its explicit `--json`
/// selector before the literal `--` tail, keeping `--label=--json` intact.
pub fn arguments(argv: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(argv.len());
    for (index, value) in argv.iter().enumerate() {
        if value == "--" {
            result.extend(argv.iter().skip(index).cloned());
            return result;
        }
        if value != "--json" {
            result.push(value.clone());
        }
    }
    result
}

/// The baseline `intent` keyword for governance routes.
pub fn intent(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "correct" => Some(&["correct"]),
        "supersede" => Some(&["supersede"]),
        "revoke" => Some(&["revoke"]),
        "forget" => Some(&["expire", "retention", "delete"]),
        _ => None,
    }
}

/// The root parser: installation's subcommand table.
pub const ROOT_DESCRIPTION: &str =
    "Install a portable runtime with external ownership and recoverable activation.

The source helper remains a recovery entry point even before a wrapper exists.
Directory replacement uses native exchange, never a rename-away visibility gap.";

fn route_help(name: &str) -> String {
    format!("Shared memory operation; use {name} --help")
}

/// Run `args` (command first) against the declared table under `prog`.
pub fn dispatch(args: &[String], prog: &str) -> Dispatched {
    let args = arguments(args);
    if let Some(route) = args.first().and_then(|first| table::route_named(first)) {
        return execute(route, args.get(1..).unwrap_or(&[]), prog);
    }
    let helps: Vec<String> = table::ENTRIES
        .iter()
        .map(|entry| match entry {
            Entry::Route(route) => route_help(route.name),
            Entry::Native(native) => native.help.to_owned(),
        })
        .collect();
    let commands: Vec<parser::Subcommand<'_>> = table::ENTRIES
        .iter()
        .zip(&helps)
        .map(|(entry, help)| parser::Subcommand {
            name: entry.name(),
            help: Some(help.as_str()),
            parser: match entry {
                Entry::Route(_) => &Parser::EMPTY,
                Entry::Native(native) => native.parser,
            },
        })
        .collect();
    let root = Parser {
        description: Some(ROOT_DESCRIPTION),
        args: &[],
        exclusive: &[],
        subcommands: Some(parser::Subcommands {
            dest: "command",
            required: true,
            commands: &commands,
        }),
    };
    let matches = match parser::parse(&root, prog, &args) {
        Ok(matches) => matches,
        Err(exit) => return Dispatched::exit(exit),
    };
    let Some((name, sub)) = matches.command() else {
        return Dispatched::completed(refusal(
            prog,
            "the following arguments are required: command",
        ));
    };
    match table::ENTRIES.iter().find(|entry| entry.name() == name) {
        Some(Entry::Native(native)) => {
            let context = Context {
                prog,
                required_operations: intent(name),
            };
            Dispatched::completed((native.run)(sub, &context))
        }
        // Routes were delegated before parsing; a parsed route name is unreachable.
        Some(Entry::Route(route)) => execute(route, &[], prog),
        None => Dispatched::completed(refusal(prog, "unknown command")),
    }
}

/// `cli.execute`: delegate to the route's helper. Under `am`/`agent-memory`
/// an unprefixed route names its own parser `am <route>`.
pub fn execute(route: &Route, args: &[String], prog: &str) -> Dispatched {
    let prog = if route.prefix.is_empty() && crate::console::NAMES.contains(&prog) {
        format!("{prog} {}", route.name)
    } else {
        prog.to_owned()
    };
    let mut argv: Vec<String> = route.prefix.iter().map(|p| (*p).to_owned()).collect();
    argv.extend_from_slice(args);
    run_helper(route.helper, &argv, &prog, intent(route.name))
}

fn run_helper(
    helper: &Helper,
    argv: &[String],
    prog: &str,
    required_operations: Option<&'static [&'static str]>,
) -> Dispatched {
    if let Some(nested) = argv
        .first()
        .and_then(|first| helper.routes.iter().find(|route| route.name == first))
    {
        return execute(nested, argv.get(1..).unwrap_or(&[]), prog);
    }
    match parser::parse(helper.parser, prog, argv) {
        Ok(matches) => {
            let context = Context {
                prog,
                required_operations,
            };
            Dispatched::completed((helper.run)(&matches, &context))
        }
        Err(exit) => Dispatched::exit(exit),
    }
}

/// A CLI-level refusal (no domain envelope applies): stderr text, status 1.
pub fn refusal(prog: &str, message: &str) -> CommandResult {
    CommandResult {
        value: None,
        exit_code: 1,
        stdout: String::new(),
        stderr: format!("{prog}: {message}\n"),
    }
}

#[cfg(feature = "dev-tools")]
static DEVELOPMENT_PACK: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Dev-tools only: a test binary is not an installed runtime, so its source
/// checkout's `skills/agent-memory` stands in for the installed pack.
#[cfg(feature = "dev-tools")]
pub fn use_development_pack(pack: Option<PathBuf>) {
    *DEVELOPMENT_PACK.write().unwrap_or_else(|e| e.into_inner()) = pack;
}

/// The `--pack` default: the installed runtime pack of the running
/// executable (the baseline derived it from `__file__`), or a typed refusal.
pub fn pack_root() -> Result<PathBuf> {
    #[cfg(feature = "dev-tools")]
    if let Some(pack) = DEVELOPMENT_PACK
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        return Ok(pack);
    }
    crate::installation::runtime_pack_root()
}

/// The executing runtime root: the directory holding `skills/agent-memory`
/// (the registered pack) and the runtime's `bin/am`. Callers that measure
/// the runtime (layout resolution, recovery quiescence) take this, never the
/// pack itself.
pub fn runtime_root() -> Result<PathBuf> {
    let pack = pack_root()?;
    pack.parent()
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| ContractError::new("Cannot locate installed runtime root"))
}

/// A registry that cannot be written (poisoned lock) stops the binary.
fn wired(result: Result<()>) -> std::result::Result<(), crate::pack_validation::Invalid> {
    result.map_err(|error| {
        crate::pack_validation::Invalid::new("malformed-pack", ".", error.to_string())
    })
}

/// Composition root of the binary: the one place where modules written
/// before their callees existed are connected. Every `register_*` function in
/// the crate is either called here or listed, with the module it waits for,
/// in `tests/wiring-pending.txt`; `tests/wiring.rs` enforces that.
/// Later lanes append their provider registrations at the end.
pub fn register_providers() -> std::result::Result<(), crate::pack_validation::Invalid> {
    wired(
        crate::installation::seams::register_updating(Some(Arc::new(
            crate::updating::Runtime::process(),
        )))
        .map_err(|error| ContractError::new(error.to_string())),
    )?;
    wired(
        crate::installation::seams::register_private_state(Some(Arc::new(
            wiring::LifecyclePrivateState,
        )))
        .map_err(|error| ContractError::new(error.to_string())),
    )?;
    wired(
        crate::installation::seams::register_omp_boundary(Some(Arc::new(
            crate::omp_boundary::InstallationBoundary,
        )))
        .map_err(|error| ContractError::new(error.to_string())),
    )?;
    crate::private_state::register_recovery_quiescence(Some(Arc::new(
        crate::recovery_quiescence::RecoveryQuiescence,
    )));
    wired(crate::bank_transactions::register_transaction_validator(
        "initialize",
        Some(Arc::new(crate::initialization::Validator)),
    ))?;
    wired(crate::bank_transactions::register_transaction_validator(
        "capture",
        Some(Arc::new(crate::admission::Admission)),
    ))?;
    // Without a resolvable running executable the activation stays
    // unregistered and every native operation refuses with MissingActivation.
    if let Ok(executable) = crate::host::current_executable() {
        let services = crate::installation::Services::registered()
            .map_err(|error| ContractError::new(error.to_string()));
        let activation = services.map(|services| {
            Arc::new(crate::installation::InstallationActivation {
                env: crate::installation::Environment::process(),
                services,
                executable,
                harness: crate::omp_transport::Harness,
            })
        });
        wired(activation.and_then(|activation| {
            crate::omp_installation::register_activation(Some(activation))
                .map_err(|error| ContractError::new(error.to_string()))
        }))?;
    }
    wired(crate::bank_transactions::register_transaction_validator(
        "compact",
        Some(Arc::new(crate::consolidation::Consolidation)),
    ))?;
    crate::admission::register_comparison(Some(Arc::new(crate::comparison::Comparison)));
    for operation in ["correct", "supersede", "revoke", "forget"] {
        wired(crate::bank_transactions::register_transaction_validator(
            operation,
            Some(Arc::new(crate::governance::Governance)),
        ))?;
    }
    static MEMORY_REFRESH: std::sync::LazyLock<std::result::Result<(), String>> =
        std::sync::LazyLock::new(|| {
            crate::turn_check::register_memory_refresh(Box::new(
                crate::memory_refresh::MemoryRefresh,
            ))
            .map_err(|error| error.to_string())
        });
    wired((*MEMORY_REFRESH).clone().map_err(ContractError::new))?;
    wired(crate::bank_transactions::register_transaction_validator(
        "refresh",
        Some(Arc::new(crate::memory_refresh::MemoryRefresh)),
    ))?;
    Ok(())
}

/// The executable entry: choose console or direct mode from argv0 and run.
/// Console mode for the installed names `am`/`agent-memory`; the bootstrap
/// console for `install --bootstrap-binary …` from install.sh; otherwise the
/// machine (direct) mode of the baseline `installer.py`.
pub fn entry(
    argv0: &std::ffi::OsStr,
    args: Vec<std::ffi::OsString>,
) -> (i32, crate::console::Transcript) {
    use crate::console::{self, Channel, Transcript};
    let name = console::installed_name(argv0);
    let prog = name.unwrap_or(console::NAMES[0]);
    let args: Result<Vec<String>> = args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| ContractError::non_utf8("argument"))
        })
        .collect();
    let args = match args {
        Ok(args) => args,
        Err(error) => {
            let mut out = Transcript::default();
            out.write(
                Channel::Stderr,
                console::safe(&format!("{prog}: error: {error}\n"), true),
            );
            return (2, out);
        }
    };
    if let Some(name) = name {
        return console::main(args, name, false);
    }
    let bootstrap = args.first().map(String::as_str) == Some("install")
        && args.get(1).map(String::as_str) == Some("--bootstrap-binary");
    if bootstrap {
        return console::main(args, prog, true);
    }
    let dispatched = dispatch(&args, prog);
    let mut out = Transcript::default();
    out.write(Channel::Stderr, dispatched.result.stderr);
    out.write(Channel::Stdout, dispatched.result.stdout);
    (dispatched.result.exit_code, out)
}
