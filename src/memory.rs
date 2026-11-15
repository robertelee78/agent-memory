//! `am recall`, `am remember` and the session-start summary.
use crate::bail;
use crate::bank::{self, Bank, Meta, Note, Route};
use crate::json::{self, Value};
use crate::repo::Repo;
use crate::util::{Error, Result, clip, io_at, now, uuid_for, uuid4, write_atomic};
use std::fmt::Write as _;
use std::io::Read;

/// Context files the summary shows, in order.
const SUMMARY_FILES: [&str; 3] = ["projectbrief.md", "techContext.md", "activeContext.md"];

/// A context file's body without its title and template comments, its
/// headings nested under the summary's; `None` while still the template.
fn filled(bank: &Bank, name: &str) -> Option<String> {
    let text = bank.read(name).ok()??;
    let (_, body) = bank::split_frontmatter(&text).ok()?;
    let body = bank::without_comments(body);
    let content: Vec<String> = body
        .lines()
        .filter(|l| !l.starts_with("# "))
        .map(|l| {
            if l.starts_with('#') {
                format!("#{l}")
            } else {
                l.to_owned()
            }
        })
        .collect();
    let content = content.join("\n").trim().to_owned();
    (!content.is_empty()).then_some(content)
}

fn lessons(bank: &Bank) -> Vec<Note> {
    bank.notes().0.into_iter().filter(Note::active).collect()
}

/// The project summary: filled context files, then lesson titles.
pub fn summary(bank: &Bank, per_file: usize, max_lessons: usize) -> String {
    let mut out = String::new();
    for name in SUMMARY_FILES {
        if let Some(content) = filled(bank, name) {
            let _ = write!(out, "## {name}\n\n{}", clip(&content, per_file));
            if content.len() > per_file {
                out.push_str(" …");
            }
            out.push_str("\n\n");
        }
    }
    if out.is_empty() {
        out.push_str("The context files are still unfilled templates; fill projectbrief.md and techContext.md as you learn the project.\n\n");
    }
    if max_lessons == 0 {
        return out;
    }
    let mut notes = lessons(bank);
    let stamp = |note: &Note| {
        bank::meta_str(&note.meta, "recorded-at")
            .unwrap_or("")
            .to_owned()
    };
    notes.sort_by_key(|note| std::cmp::Reverse(stamp(note)));
    let _ = writeln!(out, "## Lessons ({})\n", notes.len());
    for note in notes.iter().take(max_lessons) {
        let _ = writeln!(
            out,
            "- {} ({}, {})",
            note.title,
            note.kind(),
            note.recorded()
        );
    }
    if notes.len() > max_lessons {
        let _ = writeln!(
            out,
            "- … {} more; use am recall QUERY",
            notes.len() - max_lessons
        );
    }
    out
}

/// `am recall [QUERY]`.
pub fn recall(query: &str, limit: usize) -> Result<String> {
    let repo = Repo::current()?;
    let bank = Bank::new(&repo.bank);
    if !bank.exists() {
        bail!(
            "No memory bank at {}/. Run `am init` to create one.",
            repo.bank_rel
        );
    }
    let mut out = format!("# Project memory ({}/)\n\n", repo.bank_rel);
    let terms: Vec<String> = bank::words(query)
        .into_iter()
        .filter(|w| w.len() > 1)
        .collect();
    if terms.is_empty() {
        out.push_str(&summary(&bank, 4000, 50));
        return Ok(out);
    }
    out.push_str(&summary(&bank, 1500, 0));
    let router = bank.router()?;
    let mut scored: Vec<(usize, Note)> = lessons(&bank)
        .into_iter()
        .map(|note| {
            let route = router.routes.iter().find(|r| r.path == note.rel);
            let heading = bank::words(&note.title);
            let routed = route
                .map(|r| bank::words(&format!("{} {}", r.purpose, r.read_when)))
                .unwrap_or_default();
            let body = bank::words(&note.body);
            let score = terms
                .iter()
                .map(|t| {
                    let hits = |words: &[String]| {
                        words.iter().filter(|w| w.starts_with(t.as_str())).count()
                    };
                    hits(&heading) * 5 + hits(&routed) * 3 + hits(&body).min(5)
                })
                .sum();
            (score, note)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.recorded().cmp(a.1.recorded())));
    let _ = writeln!(
        out,
        "## Notes matching \"{query}\" ({} found)\n",
        scored.len()
    );
    for (_, note) in scored.iter().take(limit) {
        let evidence = note.evidence();
        let _ = writeln!(
            out,
            "### {}/{} ({}, {})",
            repo.bank_rel,
            note.rel,
            note.kind(),
            note.recorded()
        );
        if !evidence.is_empty() {
            let _ = writeln!(out, "Evidence: {}", evidence.join(", "));
        }
        let body = note.body.trim();
        let _ = writeln!(out, "\n{}\n", clip(body, 4000));
    }
    if scored.is_empty() {
        out.push_str("No lesson matched. Try fewer or different words.\n");
    }
    Ok(out)
}

/// What the user approved, from flags or stdin JSON.
#[derive(Default)]
pub struct Proposal {
    pub title: String,
    pub body: String,
    pub kind: String,
    pub evidence: Vec<String>,
    pub scope: String,
    pub read_when: String,
    pub reason: String,
    pub harness: String,
    pub session: String,
    pub conditions: Option<Value>,
}

/// Friendly kind names mapped to the record kinds.
pub fn kind(name: &str) -> Result<String> {
    let mapped = match name {
        "fix" | "result" => "result",
        "dead-end" | "attempt" | "episodic-attempt" => "episodic-attempt",
        "correction" | "feedback" | "preference" | "user-feedback" => "user-feedback",
        "decision" | "project-decision" => "project-decision",
        "fact" | "semantic-fact" | "lesson" => "semantic-fact",
        "procedure" | "how-to" | "procedural-candidate" => "procedural-candidate",
        "reference" => "reference",
        other => bail!(
            "Unknown kind {other:?}. Use fix, dead-end, correction, decision, fact, procedure or reference."
        ),
    };
    Ok(mapped.to_owned())
}

impl Proposal {
    /// Read a JSON proposal: `{title, body, kind, evidence, scope, read_when,
    /// reason, harness, session, conditions}`, or a record proposal
    /// `{metadata: {...}, body}`.
    pub fn from_json(raw: &[u8]) -> Result<Proposal> {
        let value = json::parse(raw).map_err(|e| Error::new(format!("stdin: {e}")))?;
        let meta = value.get("metadata");
        let field = |key: &str| -> Option<&Value> {
            value
                .get(key)
                .or_else(|| meta.and_then(|m| m.get(key)))
                .or_else(|| value.get(&key.replace('_', "-")))
        };
        let text = |key: &str| field(key).and_then(Value::as_str).unwrap_or("").to_owned();
        let evidence = match field("evidence") {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        };
        let origin = meta.and_then(|m| m.get("origin"));
        Ok(Proposal {
            title: text("title"),
            body: text("body"),
            kind: text("kind"),
            evidence,
            scope: text("scope"),
            read_when: text("read_when"),
            reason: text("reason"),
            harness: origin
                .and_then(|o| o.str("harness"))
                .map(str::to_owned)
                .unwrap_or_else(|| text("harness")),
            session: origin
                .and_then(|o| o.str("session"))
                .map(str::to_owned)
                .unwrap_or_else(|| text("session")),
            conditions: field("conditions").cloned(),
        })
    }
}

pub fn read_stdin() -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    std::io::stdin().read_to_end(&mut raw)?;
    Ok(raw)
}

/// Outcome of `am remember`.
pub enum Remembered {
    Written(String),
    Duplicate(String),
}

/// `am remember`: write one approved note and link it in `MEMORY.md`.
pub fn remember(mut proposal: Proposal) -> Result<Remembered> {
    let repo = Repo::current()?;
    let bank = Bank::new(&repo.bank);
    if !bank.exists() {
        bail!("No memory bank at {}/. Run `am init` first.", repo.bank_rel);
    }
    // The body may carry its own `# Title`.
    let body_title = bank::title_of(&proposal.body);
    if proposal.title.trim().is_empty() {
        proposal.title = body_title.clone();
    }
    let title = proposal.title.trim().to_owned();
    if title.is_empty() || proposal.body.trim().is_empty() {
        bail!("A note needs a title and a body (--title and --body, or JSON title/body).");
    }
    let content = if body_title.is_empty() {
        proposal.body.trim().to_owned()
    } else {
        proposal
            .body
            .trim()
            .lines()
            .skip_while(|l| !l.starts_with("# "))
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    };
    let kind = kind(if proposal.kind.is_empty() {
        "fact"
    } else {
        &proposal.kind
    })?;
    let router = bank.router()?;
    let project = router
        .project_id()
        .filter(|id| crate::util::is_uuid(id))
        .ok_or_else(|| {
            Error::new(format!(
                "{}/MEMORY.md has no valid project-id",
                repo.bank_rel
            ))
        })?
        .to_owned();
    let developer = uuid_for(&format!(
        "agent-memory-developer:{}",
        repo.email()
            .map(str::to_lowercase)
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_default())
    ));
    let id = uuid4();
    let stamp = now();
    let evidence = if proposal.evidence.is_empty() {
        vec!["user-approved in session".to_owned()]
    } else {
        proposal.evidence.clone()
    };
    let or = |value: &str, default: &str| {
        if value.trim().is_empty() {
            default.to_owned()
        } else {
            value.trim().to_owned()
        }
    };
    let reason = or(&proposal.reason, "approved in session");
    let meta: Meta = vec![
        ("schema".into(), json::s(bank::RECORD_SCHEMA)),
        ("id".into(), json::s(&id)),
        ("version".into(), Value::Int(1)),
        ("project-id".into(), json::s(&project)),
        ("kind".into(), json::s(&kind)),
        ("status".into(), json::s("active")),
        ("scope".into(), json::s(or(&proposal.scope, "repository"))),
        ("applicability".into(), json::s("known")),
        (
            "conditions".into(),
            proposal
                .conditions
                .clone()
                .filter(|c| c.as_object().is_some())
                .unwrap_or(Value::Object(Vec::new())),
        ),
        (
            "source".into(),
            json::obj([
                ("actor", json::s("user")),
                ("kind", json::s("user-approved")),
            ]),
        ),
        ("evidence".into(), json::strings(evidence)),
        (
            "origin".into(),
            json::obj([
                ("developer-id", json::s(developer)),
                ("harness", json::s(or(&proposal.harness, &harness()))),
                ("session", json::s(or(&proposal.session, "unknown"))),
                ("code-state", json::s("unknown")),
            ]),
        ),
        ("recorded-at".into(), json::s(&stamp)),
        ("updated".into(), json::s(&stamp)),
        ("valid-from".into(), Value::Null),
        ("valid-until".into(), Value::Null),
        ("derived-from".into(), Value::Array(Vec::new())),
        ("corrects".into(), Value::Array(Vec::new())),
        ("supersedes".into(), Value::Array(Vec::new())),
        ("trust".into(), json::s("reviewed")),
        ("visibility".into(), json::s("visible")),
        (
            "admission-reason".into(),
            json::s(format!("user-approved: {reason}")),
        ),
    ];
    let mut text = bank::render_frontmatter(&meta);
    let _ = write!(text, "# {title}\n\n{content}\n");
    let route = Route {
        path: format!("{}/{id}.md", bank::TOPICS),
        purpose: title.clone(),
        scope: or(&proposal.scope, "repository"),
        lifecycle: "active".into(),
        read_when: or(&proposal.read_when, "When this lesson applies"),
    };

    let read_router = router.text.clone();
    let _lock = crate::util::lock(&repo.bank)?;
    // Duplicates are judged under the lock so two writers cannot both add one.
    let title_words = bank::words(&title);
    let content_words = bank::words(&content);
    for note in bank.notes().0.iter().filter(|n| n.active()) {
        let other_content: Vec<String> = bank::words(&note.body)
            .into_iter()
            .skip(bank::words(&note.title).len())
            .collect();
        if bank::words(&note.title) == title_words || other_content == content_words {
            return Ok(Remembered::Duplicate(format!(
                "{}/{}",
                repo.bank_rel, note.rel
            )));
        }
    }
    let topics = repo.bank.join(bank::TOPICS);
    io_at(std::fs::create_dir_all(&topics), &topics)?;
    let note_path = repo.bank.join(&route.path);
    write_atomic(&note_path, text.as_bytes(), 0o644)?;
    let router_path = repo.bank.join(bank::ROUTER);
    let current = std::fs::read_to_string(&router_path).unwrap_or_default();
    if current != read_router {
        let _ = std::fs::remove_file(&note_path);
        bail!(
            "MEMORY.md changed while the note was being written; nothing was saved. Run am remember again."
        );
    }
    if let Err(e) = write_atomic(
        &router_path,
        router.with_route(&route, &stamp).as_bytes(),
        0o644,
    ) {
        let _ = std::fs::remove_file(&note_path);
        return Err(e);
    }
    Ok(Remembered::Written(format!(
        "{}/{}",
        repo.bank_rel, route.path
    )))
}

/// The harness running `am`, from its environment.
fn harness() -> String {
    let set = |key: &str| std::env::var_os(key).is_some();
    if let Ok(name) = std::env::var("AM_HARNESS") {
        name
    } else if set("CLAUDECODE") {
        "claude".into()
    } else if set("CODEX_THREAD_ID") || set("CODEX_SANDBOX") || set("CODEX_MANAGED_BY_NPM") {
        "codex".into()
    } else if set("OMP_SESSION_ID") || set("PI_CODING_AGENT_DIR") {
        "omp".into()
    } else {
        "unknown".into()
    }
}
