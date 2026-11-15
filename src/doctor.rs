//! `am status` and `am doctor`: installed? hooks present? bank healthy?
//! Read-only; problems are explained in plain words.
use crate::bank::{self, Bank};
use crate::install::{self, release};
use crate::repo::Repo;
use crate::util::is_uuid;
use std::collections::BTreeSet;
use std::fmt::Write as _;

const STATUSES: [&str; 6] = [
    "candidate",
    "quarantined",
    "active",
    "superseded",
    "revoked",
    "expired",
];
const REQUIRED: [&str; 19] = [
    "schema",
    "id",
    "version",
    "project-id",
    "kind",
    "status",
    "scope",
    "applicability",
    "conditions",
    "source",
    "evidence",
    "origin",
    "recorded-at",
    "updated",
    "valid-from",
    "valid-until",
    "derived-from",
    "corrects",
    "supersedes",
];

#[derive(Default)]
struct Report {
    lines: String,
    problems: Vec<String>,
}

impl Report {
    fn line(&mut self, text: impl AsRef<str>) {
        self.lines.push_str(text.as_ref());
        self.lines.push('\n');
    }
    fn problem(&mut self, text: impl Into<String>) {
        self.problems.push(text.into());
    }
}

fn installation(report: &mut Report, deep: bool) {
    let receipt = install::locations().and_then(|l| Ok((install::read_receipt(&l)?, l)));
    let (receipt, locations) = match receipt {
        Ok((Some(receipt), locations)) => (receipt, locations),
        Ok((None, _)) => {
            report.line("Installed: no");
            report.problem("agent-memory is not installed. Install it with install.sh (or am install --development-source PATH).");
            return;
        }
        Err(e) => {
            report.line("Installed: unknown");
            report.problem(format!("The installation receipt cannot be read: {e}"));
            return;
        }
    };
    let schema = receipt.str("schema").unwrap_or("");
    let source = receipt.get("source");
    let kind = source.and_then(|s| s.str("kind")).unwrap_or("?");
    let version = receipt
        .str("version")
        .or_else(|| source.and_then(|s| s.str("version")))
        .unwrap_or("?");
    report.line(format!(
        "Installed: {version} ({kind}) from {}",
        source.and_then(|s| s.str("path")).unwrap_or("?")
    ));
    if schema != install::RECEIPT_SCHEMA {
        report.problem("This is an earlier agent-memory installation (v0.0.14 or older). Run install.sh to update; it takes the old installation over.");
        return;
    }
    let checks = install::check_targets(&receipt);
    let hooks: Vec<String> = checks
        .iter()
        .filter_map(|check| {
            let client = check.client.as_deref()?;
            Some(format!(
                "{client} {}",
                if check.ok { "ok" } else { "MISSING" }
            ))
        })
        .collect();
    if hooks.is_empty() {
        report.line("Session-start hooks: none installed (no Claude, Codex or omp preset)");
    } else {
        report.line(format!("Session-start hooks: {}", hooks.join(", ")));
    }
    for check in &checks {
        if !check.ok {
            report.problem(format!(
                "{} {} no longer holds what agent-memory installed. Run `am install` to repair it.",
                check.kind, check.path
            ));
        } else if deep {
            report.line(format!("  ok  {} {}", check.kind, check.path));
        }
    }
    let binary = source.and_then(|s| s.str("binary")).unwrap_or("");
    match crate::util::which("am") {
        Some(found) => {
            let resolved = found.canonicalize().unwrap_or(found.clone());
            let expected = std::path::Path::new(binary)
                .canonicalize()
                .unwrap_or_default();
            if resolved != expected {
                report.problem(format!(
                    "`am` on PATH is {}, not the installed {binary}. Put {} first on PATH.",
                    found.display(),
                    receipt
                        .str("bin_dir")
                        .unwrap_or("the install bin directory")
                ));
            }
        }
        None => report.problem(format!(
            "`am` is not on PATH. Add {} to PATH.",
            receipt
                .str("bin_dir")
                .unwrap_or("the install bin directory")
        )),
    }
    if deep && kind == "release" {
        if let Err(e) = release::installed(&locations.runtime) {
            report.problem(format!(
                "The installed runtime does not match its signed release: {e}. Re-run install.sh."
            ));
        }
    }
}

fn bank_health(report: &mut Report, deep: bool) {
    let repo = match std::env::current_dir()
        .map_err(Into::into)
        .and_then(|cwd| Repo::discover(&cwd))
    {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            report.line("Repository: none (not inside a Git checkout)");
            return;
        }
        Err(e) => {
            report.problem(format!("{e}"));
            return;
        }
    };
    report.line(format!("Repository: {}", repo.root.display()));
    let bank = Bank::new(&repo.bank);
    if !bank.exists() {
        report.line(format!(
            "Memory bank: none at {}/ (run `am init` to create one)",
            repo.bank_rel
        ));
        return;
    }
    let router = match bank.router() {
        Ok(router) => router,
        Err(e) => {
            report.problem(format!("{}/MEMORY.md cannot be read: {e}", repo.bank_rel));
            return;
        }
    };
    let (notes, unreadable) = bank.notes();
    let active = notes.iter().filter(|n| n.active()).count();
    report.line(format!(
        "Memory bank: {}/ with {} routes and {active} active lessons",
        repo.bank_rel,
        router.routes.len()
    ));
    let project = router.project_id().unwrap_or("");
    if bank::meta_str(&router.meta, "schema") != Some(bank::ROUTER_SCHEMA) {
        report.problem("MEMORY.md frontmatter lacks `schema: \"agent-memory/v1\"`.");
    }
    if !is_uuid(project) {
        report.problem("MEMORY.md frontmatter has no valid project-id; `am remember` needs one.");
    }
    for problem in unreadable {
        report.problem(format!("Unreadable note {problem}"));
    }
    for route in &router.routes {
        if !repo.bank.join(&route.path).exists() {
            report.problem(format!(
                "MEMORY.md links {} but the file is missing.",
                route.path
            ));
        }
    }
    if !deep {
        return;
    }
    // The bank grows with the project: absent context files are normal.
    let absent: Vec<&str> = bank::CONTEXTS
        .iter()
        .map(|(name, _, _)| *name)
        .filter(|name| !repo.bank.join(name).exists())
        .collect();
    if !absent.is_empty() {
        report.line(format!(
            "Not written yet (fine; add them when there is something useful to say): {}",
            absent.join(", ")
        ));
    }
    let routed: BTreeSet<&str> = router.routes.iter().map(|r| r.path.as_str()).collect();
    let mut ids = BTreeSet::new();
    for note in &notes {
        let rel = &note.rel;
        let missing: Vec<&str> = REQUIRED
            .iter()
            .copied()
            .filter(|key| !note.meta.iter().any(|(k, _)| k == key))
            .collect();
        if !missing.is_empty() {
            report.problem(format!(
                "{rel} lacks frontmatter fields: {}.",
                missing.join(", ")
            ));
        }
        let field = |key: &str| bank::meta_str(&note.meta, key).unwrap_or("");
        if !missing.contains(&"schema") && field("schema") != bank::RECORD_SCHEMA {
            report.problem(format!(
                "{rel} has schema {:?}, expected {}.",
                field("schema"),
                bank::RECORD_SCHEMA
            ));
        }
        let id = field("id");
        if !id.is_empty() && !ids.insert(id.to_owned()) {
            report.problem(format!("{rel} reuses id {id}."));
        }
        if !field("kind").is_empty() && !bank::KINDS.contains(&field("kind")) {
            report.problem(format!("{rel} has unknown kind {:?}.", field("kind")));
        }
        if !STATUSES.contains(&note.status()) {
            report.problem(format!("{rel} has unknown status {:?}.", note.status()));
        }
        if !missing.contains(&"evidence") && note.evidence().is_empty() {
            report.problem(format!("{rel} needs at least one evidence entry."));
        }
        if !project.is_empty() && !field("project-id").is_empty() && field("project-id") != project
        {
            report.problem(format!(
                "{rel} belongs to another project-id ({}).",
                field("project-id")
            ));
        }
        if note.body.contains("\n<<<<<<< ") || note.body.starts_with("<<<<<<< ") {
            report.problem(format!(
                "{rel} still has merge conflict markers; resolve them."
            ));
        }
        if note.active() && !routed.contains(rel.as_str()) {
            report.problem(format!(
                "{rel} is not linked from MEMORY.md, so agents may not find it."
            ));
        }
    }
}

/// `am status` (short) or `am doctor` (every check). Returns (text, healthy).
pub fn run(deep: bool) -> (String, bool) {
    let mut report = Report::default();
    report.line(format!(
        "agent-memory {} ({})",
        release::VERSION,
        release::TARGET
    ));
    installation(&mut report, deep);
    bank_health(&mut report, deep);
    let mut out = report.lines;
    if report.problems.is_empty() {
        out.push_str("No problems found.\n");
    } else {
        let _ = writeln!(out, "\nProblems ({}):", report.problems.len());
        for problem in &report.problems {
            let _ = writeln!(out, "  - {problem}");
        }
        if !deep {
            out.push_str("Run `am doctor` for every check.\n");
        }
    }
    (out, report.problems.is_empty())
}
