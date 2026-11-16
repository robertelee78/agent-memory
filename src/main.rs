//! `am` (and its alias `agent-memory`).
use agent_memory::install::{self, release};
use agent_memory::memory::{self, Proposal, Remembered};
use agent_memory::util::{Error, Result};
use agent_memory::{bail, bank, doctor, hook, repo, share, util};
use std::path::PathBuf;

const HELP: &str = "am — shared project memory for coding agents

Memory:
  am recall [QUERY]        Print the project summary, or the notes matching QUERY
  am remember [OPTIONS]    Save a note the user approved (flags, or JSON on stdin)
      --title T --body B|- --kind fix|dead-end|correction|decision|fact|procedure|reference
      --evidence E (repeatable) --read-when R --scope S --reason R --harness H --session S
  am init                  Create memory-bank/ from the template
  am share [-m MESSAGE]    Commit only memory-bank/ changes and push them (or open a PR)
  am refresh               Bring others' shared memory into this working tree

Health:
  am status                Installed? Hooks present? Bank healthy?
  am doctor                Every check, explained; never rewrites memory

Installation:
  am install [--development-source PATH] [--mode link|copy] [--presets all|none|LIST]
             [--target-dir PATH]... [--bin-dir PATH] [--no-guidance] [--omp-agent-dir PATH] [--dry-run]
  am update [--check]
  am uninstall [--dry-run]
  am --version

Run am COMMAND --help for one command's usage.
";

/// `am <command> --help`.
const USAGES: [(&str, &str); 12] = [
    (
        "recall",
        "Usage: am recall [QUERY] [--limit N]\n\nWithout QUERY, print the project summary (context files and lesson titles).\nWith QUERY, print only the notes that contain every query word, best first\n(N at most, default 5). If no note has every word, the closest ones are shown\nand the output says so.",
    ),
    (
        "remember",
        "Usage: am remember --title TITLE --body BODY|- [--kind KIND] [OPTIONS]\n       am remember < note.json\n\nSave one note the user explicitly approved, link it in MEMORY.md and skip duplicates.\n\n  --kind KIND        fix, dead-end, correction, decision, fact, procedure or reference (default fact)\n  --evidence E       where the lesson comes from (repeatable)\n  --read-when TEXT   when a future agent should read it\n  --scope S          what it applies to (default repository)\n  --reason TEXT      why it was approved\n  --harness NAME     your client: claude, codex or omp (default unknown)\n  --session ID       the session id, if known\n  --body -           read the body from stdin\n\nExample:\n  am remember --kind decision --title \"Staging relay port\" --body \"The staging relay port is 7443.\"",
    ),
    (
        "init",
        "Usage: am init\n\nCreate memory-bank/ from the template. On a repository that already has a bank it\nchanges nothing.",
    ),
    (
        "share",
        "Usage: am share [-m MESSAGE]\n\nCommit only memory-bank/ changes on top of the shared branch and push them. A\nprotected branch gets a memory branch and a pull request. Afterwards a plain\n`git pull` works (merge or rebase): am moves the branch over memory-only commits,\nadds one commit that changes only memory-bank/, or stages it, and says which.",
    ),
    (
        "refresh",
        "Usage: am refresh\n\nMerge teammates' shared memory into this working tree, keeping your edits. Code is\nuntouched. Afterwards a plain `git pull` works (merge or rebase): am moves the\nbranch over memory-only commits, adds one commit that changes only memory-bank/,\nor stages it, and says which.",
    ),
    (
        "status",
        "Usage: am status\n\nInstalled? Hooks present? Bank healthy? Short form of am doctor.",
    ),
    (
        "doctor",
        "Usage: am doctor\n\nEvery installation and bank check, explained in plain words. Exits 1 when it finds\na problem. It never rewrites memory.",
    ),
    (
        "install",
        "Usage: am install [--development-source PATH] [--mode link|copy] [--presets all|none|LIST]\n                  [--target-dir PATH]... [--bin-dir PATH] [--no-guidance] [--omp-agent-dir PATH] [--dry-run]\n\nInstall or repair the skill, activation blocks, session-start hooks and the am command.\nPresets: agents, codex, claude, opencode and omp. all (the default) includes omp when\nomp is on this machine: its agent directory ($PI_CODING_AGENT_DIR, else ~/.omp/agent)\nexists. --omp-agent-dir PATH sets up omp in PATH instead.\nA signed release installs through install.sh; --development-source installs a checkout\nwith a built target/release/am.",
    ),
    (
        "update",
        "Usage: am update [--check]\n\nInstall the newest signed release; --check only reports whether one is available.\nAlso sets up clients installed since (omp, with the default presets).",
    ),
    (
        "uninstall",
        "Usage: am uninstall [--dry-run]\n\nRemove exactly the files the installation receipt owns. Memory banks are not touched.",
    ),
    (
        "hook",
        "Usage: am hook session-start --client claude|codex|omp\n\nPrint the session-start context for the client (run by the installed hooks).",
    ),
    ("help", "Usage: am help [COMMAND]"),
];

fn usage(command: &str) -> Option<&'static str> {
    USAGES
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, text)| *text)
}

/// Simple flag reader over the remaining arguments.
struct Args {
    command: String,
    rest: Vec<String>,
}

impl Args {
    fn flag(&mut self, names: &[&str]) -> bool {
        let before = self.rest.len();
        self.rest.retain(|a| !names.contains(&a.as_str()));
        self.rest.len() != before
    }
    fn values(&mut self, name: &str) -> Result<Vec<String>> {
        let mut found = Vec::new();
        let mut index = 0;
        while index < self.rest.len() {
            let arg = &self.rest[index];
            if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
                found.push(value.to_owned());
                self.rest.remove(index);
            } else if arg == name {
                if index + 1 >= self.rest.len() {
                    bail!("{name} needs a value");
                }
                found.push(self.rest.remove(index + 1));
                self.rest.remove(index);
            } else {
                index += 1;
            }
        }
        Ok(found)
    }
    fn value(&mut self, name: &str) -> Result<Option<String>> {
        let mut values = self.values(name)?;
        if values.len() > 1 {
            bail!("{name} is given more than once");
        }
        Ok(values.pop())
    }
    fn done(self) -> Result<Vec<String>> {
        if let Some(unknown) = self.rest.iter().find(|a| a.starts_with('-') && a.len() > 1) {
            bail!("Unknown option {unknown}. Run am {} --help.", self.command);
        }
        Ok(self.rest)
    }
    fn none(self) -> Result<()> {
        let command = self.command.clone();
        let rest = self.done()?;
        if let Some(extra) = rest.first() {
            bail!("Unexpected argument {extra:?}. Run am {command} --help.");
        }
        Ok(())
    }
}

fn install_options(args: &mut Args) -> Result<install::Options> {
    Ok(install::Options {
        development_source: args.value("--development-source")?.map(PathBuf::from),
        bootstrap_binary: args.value("--bootstrap-binary")?.map(PathBuf::from),
        mode: args.value("--mode")?,
        presets: args.value("--presets")?,
        target_dirs: args
            .values("--target-dir")?
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        bin_dir: args.value("--bin-dir")?.map(PathBuf::from),
        no_guidance: args.flag(&["--no-guidance"]),
        omp_agent_dir: args.value("--omp-agent-dir")?.map(PathBuf::from),
        dry_run: args.flag(&["--dry-run", "--check"]),
    })
}

fn remember(mut args: Args) -> Result<String> {
    let title = args.value("--title")?;
    let body = args.value("--body")?;
    let kind = args.value("--kind")?;
    let evidence = args.values("--evidence")?;
    let scope = args.value("--scope")?;
    let read_when = args.value("--read-when")?;
    let reason = args.value("--reason")?;
    let harness = args.value("--harness")?;
    let session = args.value("--session")?;
    let from_stdin = args.flag(&["--stdin"]);
    args.none()?;
    let mut proposal = if from_stdin || (title.is_none() && body.is_none()) {
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            bail!(
                "Give --title and --body, or pipe the note as JSON on stdin. Run am remember --help."
            );
        }
        Proposal::from_json(&memory::read_stdin()?)?
    } else {
        Proposal::default()
    };
    let body = match body.as_deref() {
        Some("-") => Some(String::from_utf8_lossy(&memory::read_stdin()?).into_owned()),
        other => other.map(str::to_owned),
    };
    let set = |slot: &mut String, value: Option<String>| {
        if let Some(value) = value {
            *slot = value;
        }
    };
    set(&mut proposal.title, title);
    set(&mut proposal.body, body);
    set(&mut proposal.kind, kind);
    set(&mut proposal.scope, scope);
    set(&mut proposal.read_when, read_when);
    set(&mut proposal.reason, reason);
    set(&mut proposal.harness, harness);
    set(&mut proposal.session, session);
    if !evidence.is_empty() {
        proposal.evidence = evidence;
    }
    Ok(match memory::remember(proposal)? {
        Remembered::Written(path) => {
            format!("Remembered: {path}\nRun `am share` when the user wants it shared.")
        }
        Remembered::Duplicate(path) => format!("Already remembered (skipped duplicate): {path}"),
    })
}

fn init(args: Args) -> Result<String> {
    args.none()?;
    let repo = repo::Repo::current()?;
    if bank::Bank::new(&repo.bank).exists() {
        return Ok(format!(
            "This project already has a memory bank at {}/; nothing was changed. Context files it lacks are optional: add them when there is something useful to say.",
            repo.bank_rel
        ));
    }
    let remote = repo.remote_url().and_then(repo::canonical_remote);
    let written = bank::init(&repo.bank, &util::uuid4(), remote.as_deref(), &util::now())?;
    Ok(format!(
        "Created {}/ ({}).\nNext: fill projectbrief.md and techContext.md with what this project is and how it is built, then `am share`.",
        repo.bank_rel,
        written.join(", ")
    ))
}

fn dispatch(argv: Vec<String>) -> Result<(String, i32)> {
    let mut iter = argv.into_iter();
    let command = iter.next().unwrap_or_else(|| "help".into());
    let mut args = Args {
        command: command.clone(),
        rest: iter.collect(),
    };
    if args.rest.iter().any(|a| a == "--help" || a == "-h") {
        if let Some(text) = usage(&command) {
            return Ok((text.to_owned(), 0));
        }
    }
    let text = match command.as_str() {
        "help" | "--help" | "-h" => match args.rest.first() {
            Some(topic) => usage(topic)
                .ok_or_else(|| Error::new(format!("Unknown command {topic:?}. Run am help.")))?
                .to_owned(),
            None => HELP.to_owned(),
        },
        "--version" | "-V" | "version" => format!("am {}", release::VERSION),
        "status" | "doctor" => {
            args.none()?;
            let (text, healthy) = doctor::run(command == "doctor");
            return Ok((text, i32::from(!healthy && command == "doctor")));
        }
        "init" => init(args)?,
        "recall" => {
            let limit = args
                .value("--limit")?
                .map(|n| n.parse().unwrap_or(5))
                .unwrap_or(5);
            let query = args.done()?.join(" ");
            memory::recall(&query, limit)?
        }
        "remember" => remember(args)?,
        "share" => {
            let message = args.value("-m")?.or(args.value("--message")?);
            args.none()?;
            share::share(message.as_deref())?
        }
        "refresh" => {
            args.none()?;
            share::refresh()?
        }
        "install" => {
            let options = install_options(&mut args)?;
            args.none()?;
            install::install(&options)?
        }
        "update" => {
            let check = args.flag(&["--check"]);
            let bootstrap = args.value("--bootstrap-binary")?.map(PathBuf::from);
            args.none()?;
            install::update(check, bootstrap.as_deref())?
        }
        "uninstall" => {
            let dry_run = args.flag(&["--dry-run"]);
            args.none()?;
            install::uninstall(dry_run)?
        }
        "hook" => {
            let client = args.value("--client")?.unwrap_or_else(|| "unknown".into());
            let rest = args.done()?;
            if rest.first().map(String::as_str) != Some("session-start") {
                bail!("Usage: am hook session-start --client claude|codex|omp");
            }
            return Ok((String::new(), hook::session_start(&client)));
        }
        other => {
            return Err(Error::new(format!(
                "Unknown command {other:?}. Run am help."
            )));
        }
    };
    Ok((text, 0))
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(argv) {
        Ok((text, code)) => {
            if !text.is_empty() {
                println!("{}", text.trim_end());
            }
            std::process::exit(code);
        }
        Err(error) => {
            eprintln!("am: {error}");
            std::process::exit(1);
        }
    }
}
