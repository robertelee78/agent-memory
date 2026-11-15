//! `am hook session-start`: inject the project summary at session start.
//! It never fails the session: any problem prints nothing and exits 0.
use crate::bank::Bank;
use crate::json::{self, Value};
use crate::repo::Repo;
use std::io::Read;
use std::path::PathBuf;

pub const USAGE: &str = "Agent Memory: this repository keeps shared project memory in Markdown. \
Use `am recall QUERY` to read the notes on a topic before relying on assumptions. \
When a fix is confirmed, a decision is made, the user corrects you or a task is done, \
suggest one short memory, ask the user to approve it, then save it with `am remember`. \
Run `am share` when the user wants memory shared with the team.";

/// The context text for the repository at `cwd`; `None` without a bank.
pub fn context(cwd: &std::path::Path) -> Option<String> {
    let repo = Repo::discover(cwd).ok()??;
    let bank = Bank::new(&repo.bank);
    if !bank.exists() {
        return None;
    }
    Some(format!(
        "{USAGE}\n\n# Project memory ({}/)\n\n{}",
        repo.bank_rel,
        crate::memory::summary(&bank, 1500, 15)
    ))
}

/// Run the hook for `client` (`claude`, `codex` or `omp`).
pub fn session_start(client: &str) -> i32 {
    let mut raw = Vec::new();
    // Claude and Codex send the event on stdin; omp passes nothing.
    if client != "omp" {
        let _ = std::io::stdin().take(1 << 20).read_to_end(&mut raw);
    }
    let event = json::parse(&raw).unwrap_or(Value::Null);
    let cwd = event
        .str("cwd")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let Some(text) = cwd.and_then(|cwd| context(&cwd)) else {
        return 0;
    };
    if client == "omp" {
        print!("{text}");
    } else {
        let output = json::obj([(
            "hookSpecificOutput",
            json::obj([
                ("hookEventName", json::s("SessionStart")),
                ("additionalContext", json::s(text)),
            ]),
        )]);
        println!("{}", json::encode(&output, json::LINE));
    }
    0
}
