//! `am hook session-start`: inject the project summary at session start.
//! It never fails the session: any problem prints nothing and exits 0.
use crate::bank::Bank;
use crate::json::{self, Value};
use crate::repo::Repo;
use std::io::Read;
use std::path::PathBuf;

/// How to use memory; `{client}` is the hook's client.
const USAGE: &str = "Agent Memory: this repository keeps shared project memory in memory-bank/. \
Run `am recall QUERY` to read the notes on a topic before relying on assumptions.
Saving needs the user's own explicit yes. When a fix is confirmed, a decision is made, \
the user corrects you or a task is done, propose one short memory and ask \
\"Save this to project memory?\". Agreeing a decision (\"that's final\", \"we decided\") \
is not permission to save. Only after a separate yes, run for example:
  am remember --harness {client} --kind decision --title \"Staging relay port\" --body \"The staging relay port is 7443.\"
(kinds: fix, dead-end, correction, decision, fact). Run `am share` only when the user asks.";

/// Upper bound on the injected session-start text, in bytes.
const MAX_CONTEXT: usize = 8 * 1024;

/// The context text for the repository at `cwd`; `None` without a bank.
pub fn context(cwd: &std::path::Path, client: &str) -> Option<String> {
    let repo = Repo::discover(cwd).ok()??;
    let bank = Bank::new(&repo.bank);
    if !bank.exists() {
        return None;
    }
    let text = format!(
        "{}\n\n# Project memory ({}/)\n\n{}",
        USAGE.replace("{client}", client),
        repo.bank_rel,
        crate::memory::summary(&bank, 1500, 15)
    );
    // Session context stays small however large the bank's files are.
    Some(match crate::util::clip(&text, MAX_CONTEXT) {
        clipped if clipped.len() < text.len() => format!("{clipped} …\n(cut; run am recall)"),
        _ => text,
    })
}

/// Run the hook for `client` (`claude`, `codex` or `omp`).
pub fn session_start(client: &str) -> i32 {
    let client = match client {
        "claude" | "codex" | "omp" => client,
        _ => "unknown",
    };
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
    let Some(text) = cwd.and_then(|cwd| context(&cwd, client)) else {
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
