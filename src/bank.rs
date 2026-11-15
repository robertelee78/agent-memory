//! The Markdown bank: `MEMORY.md` route map, six context files and lesson
//! notes under `topics/` with record frontmatter.
use crate::bail;
use crate::json::{self, Value};
use crate::util::{Error, Result, io_at, read_optional};
use std::path::{Path, PathBuf};

pub const ROUTER: &str = "MEMORY.md";
pub const ROUTER_SCHEMA: &str = "agent-memory/v1";
pub const RECORD_SCHEMA: &str = "agent-memory/record-v1";
pub const TOPICS: &str = "topics";
/// The six context files with their route purpose and read-when text.
pub const CONTEXTS: [(&str, &str, &str); 6] = [
    (
        "projectbrief.md",
        "What the project is, why it exists, its boundaries",
        "At session start to identify this project",
    ),
    (
        "productContext.md",
        "Users, workflow and scope exclusions",
        "When deciding product scope",
    ),
    (
        "activeContext.md",
        "Current focus, open questions and next step",
        "When continuing current work",
    ),
    (
        "systemPatterns.md",
        "Architecture and established patterns",
        "When changing design or structure",
    ),
    (
        "techContext.md",
        "Stack, build and test commands, constraints",
        "When building, testing or setting up",
    ),
    (
        "progress.md",
        "What works, what remains, known problems",
        "When choosing what to do next",
    ),
];
pub const KINDS: [&str; 7] = [
    "semantic-fact",
    "user-feedback",
    "project-decision",
    "episodic-attempt",
    "result",
    "reference",
    "procedural-candidate",
];
const TEMPLATES: [(&str, &str); 6] = [
    (
        "projectbrief.md",
        include_str!("../skills/agent-memory/templates/memory-bank/projectbrief.md"),
    ),
    (
        "productContext.md",
        include_str!("../skills/agent-memory/templates/memory-bank/productContext.md"),
    ),
    (
        "activeContext.md",
        include_str!("../skills/agent-memory/templates/memory-bank/activeContext.md"),
    ),
    (
        "systemPatterns.md",
        include_str!("../skills/agent-memory/templates/memory-bank/systemPatterns.md"),
    ),
    (
        "techContext.md",
        include_str!("../skills/agent-memory/templates/memory-bank/techContext.md"),
    ),
    (
        "progress.md",
        include_str!("../skills/agent-memory/templates/memory-bank/progress.md"),
    ),
];

pub type Meta = Vec<(String, Value)>;

pub fn meta_str<'a>(meta: &'a Meta, key: &str) -> Option<&'a str> {
    meta.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
}
fn meta_get<'a>(meta: &'a Meta, key: &str) -> Option<&'a Value> {
    meta.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Split `---` frontmatter (one `key: value` per line, values JSON or bare)
/// from the body. Text without frontmatter has empty metadata.
pub fn split_frontmatter(text: &str) -> Result<(Meta, &str)> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok((Vec::new(), text));
    };
    let (head, body) = match rest.find("\n---\n") {
        Some(end) => (&rest[..end], &rest[end + 5..]),
        None => match rest.strip_suffix("\n---") {
            Some(head) => (head, ""),
            None => bail!("frontmatter has no closing ---"),
        },
    };
    let mut meta = Vec::new();
    for line in head.lines().filter(|l| !l.trim().is_empty()) {
        let Some((key, value)) = line.split_once(':') else {
            bail!("frontmatter line is not `key: value`: {line}");
        };
        let value = value.trim();
        let parsed = json::parse(value.as_bytes()).unwrap_or_else(|_| {
            let bare = value
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .unwrap_or(value);
            Value::String(bare.to_owned())
        });
        meta.push((key.trim().to_owned(), parsed));
    }
    Ok((meta, body))
}

pub fn render_frontmatter(meta: &Meta) -> String {
    let mut out = String::from("---\n");
    for (key, value) in meta {
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&json::encode(value, json::LINE));
        out.push('\n');
    }
    out.push_str("---\n");
    out
}

/// One `MEMORY.md` route row.
#[derive(Debug, Clone)]
pub struct Route {
    pub path: String,
    pub purpose: String,
    pub scope: String,
    pub lifecycle: String,
    pub read_when: String,
}

impl Route {
    pub fn row(&self) -> String {
        let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
        format!(
            "| [{p}](<{p}>) | {} | {} | {} | {} |",
            cell(&self.purpose),
            cell(&self.scope),
            cell(&self.lifecycle),
            cell(&self.read_when),
            p = self.path
        )
    }
}

fn cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|');
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = vec![String::new()];
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cells.last_mut().map(|s| s.push('|'));
            }
            '|' => cells.push(String::new()),
            c => {
                cells.last_mut().map(|s| s.push(c));
            }
        }
    }
    cells.into_iter().map(|c| c.trim().to_owned()).collect()
}

/// Parse a route row; `None` for any other line.
pub fn parse_route(line: &str) -> Option<Route> {
    let line = line.trim();
    if !line.starts_with("| [") {
        return None;
    }
    let cells = cells(line);
    let link = cells.first()?;
    let target = link.split_once("](")?.1.strip_suffix(')')?;
    let path = target.trim_start_matches('<').trim_end_matches('>');
    let at = |i: usize| cells.get(i).cloned().unwrap_or_default();
    Some(Route {
        path: path.to_owned(),
        purpose: at(1),
        scope: at(2),
        lifecycle: at(3),
        read_when: at(4),
    })
}

/// The bank's route map, kept as text so edits preserve everything else.
pub struct Router {
    pub text: String,
    pub meta: Meta,
    pub routes: Vec<Route>,
}

impl Router {
    pub fn parse(text: String) -> Result<Router> {
        let (meta, body) =
            split_frontmatter(&text).map_err(|e| Error::new(format!("{ROUTER}: {e}")))?;
        let routes = body.lines().filter_map(parse_route).collect();
        Ok(Router { meta, routes, text })
    }
    pub fn project_id(&self) -> Option<&str> {
        meta_str(&self.meta, "project-id")
    }
    /// The text with `route` appended after the last route row and the
    /// router time refreshed.
    pub fn with_route(&self, route: &Route, now: &str) -> String {
        let mut lines: Vec<String> = self.text.lines().map(str::to_owned).collect();
        let last_row = lines.iter().rposition(|l| parse_route(l).is_some());
        let separator = lines.iter().position(|l| {
            l.trim_start().starts_with("| ---") || l.trim_start().starts_with("|---")
        });
        match last_row.or(separator) {
            Some(index) => lines.insert(index + 1, route.row()),
            None => {
                lines.push(String::new());
                lines.push("## Routes".into());
                lines.push(String::new());
                lines.push("| Path | Purpose | Scope | Lifecycle | Read when |".into());
                lines.push("| --- | --- | --- | --- | --- |".into());
                lines.push(route.row());
            }
        }
        let mut text = lines.join("\n");
        text.push('\n');
        with_router_time(&text, now)
    }
}

/// Replace the frontmatter `router-updated` value.
pub fn with_router_time(text: &str, now: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_front = false;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if line.trim_end() == "---" {
            in_front = index == 0;
        }
        if in_front && line.starts_with("router-updated:") {
            out.push_str(&format!("router-updated: \"{now}\"\n"));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// A lesson note under `topics/`.
pub struct Note {
    /// Bank-relative path, e.g. `topics/<id>.md`.
    pub rel: String,
    pub meta: Meta,
    pub title: String,
    pub body: String,
}

impl Note {
    pub fn status(&self) -> &str {
        meta_str(&self.meta, "status").unwrap_or("active")
    }
    pub fn kind(&self) -> &str {
        meta_str(&self.meta, "kind").unwrap_or("note")
    }
    pub fn active(&self) -> bool {
        self.status() == "active"
    }
    pub fn recorded(&self) -> &str {
        meta_str(&self.meta, "recorded-at")
            .map(|t| t.get(..10).unwrap_or(t))
            .unwrap_or("")
    }
    pub fn evidence(&self) -> Vec<String> {
        meta_get(&self.meta, "evidence")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// First `# heading` of a Markdown body.
pub fn title_of(body: &str) -> String {
    body.lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or("")
        .trim()
        .to_owned()
}

/// Body text with HTML comments removed (template guidance).
pub fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The bank in a repository.
pub struct Bank {
    pub dir: PathBuf,
}

impl Bank {
    pub fn new(dir: &Path) -> Bank {
        Bank {
            dir: dir.to_path_buf(),
        }
    }
    pub fn exists(&self) -> bool {
        self.dir.join(ROUTER).is_file()
    }
    pub fn router(&self) -> Result<Router> {
        let path = self.dir.join(ROUTER);
        let raw = io_at(std::fs::read(&path), &path)?;
        let text = String::from_utf8(raw)
            .map_err(|_| Error::new(format!("{} is not UTF-8 text", path.display())))?;
        Router::parse(text)
    }
    /// A bank file's text, `None` when absent.
    pub fn read(&self, rel: &str) -> Result<Option<String>> {
        Ok(read_optional(&self.dir.join(rel))?
            .map(|raw| String::from_utf8_lossy(&raw).into_owned()))
    }
    /// Every Markdown note under `topics/`, sorted by path. Unreadable notes
    /// are returned as errors next to the good ones.
    pub fn notes(&self) -> (Vec<Note>, Vec<String>) {
        let mut notes = Vec::new();
        let mut problems = Vec::new();
        let mut stack = vec![self.dir.join(TOPICS)];
        let mut files = Vec::new();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(t) if t.is_dir() => stack.push(path),
                    Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "md") => {
                        files.push(path)
                    }
                    _ => {}
                }
            }
        }
        files.sort();
        for path in files {
            let rel = path
                .strip_prefix(&self.dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    let (meta, body) = split_frontmatter(&text).map_err(|e| e.0)?;
                    Ok(Note {
                        rel: rel.clone(),
                        title: title_of(body),
                        body: body.to_owned(),
                        meta,
                    })
                });
            match parsed {
                Ok(note) => notes.push(note),
                Err(e) => problems.push(format!("{rel}: {e}")),
            }
        }
        (notes, problems)
    }
}

/// Create the bank from the templates. Returns the files written.
pub fn init(bank: &Path, project_id: &str, remote: Option<&str>, now: &str) -> Result<Vec<String>> {
    let existing = Bank::new(bank);
    if existing.exists() {
        bail!(
            "{} already exists; this project already has a memory bank.",
            bank.join(ROUTER).display()
        );
    }
    io_at(std::fs::create_dir_all(bank.join(TOPICS)), bank)?;
    let mut written = Vec::new();
    for (name, template) in TEMPLATES {
        let path = bank.join(name);
        if path.exists() {
            continue; // Keep whatever the project already wrote.
        }
        io_at(std::fs::write(&path, template), &path)?;
        written.push(name.to_owned());
    }
    // Git does not track empty directories; keep topics/ visible to clones.
    let keep = bank.join(TOPICS).join(".gitkeep");
    if !keep.exists() {
        io_at(std::fs::write(&keep, ""), &keep)?;
    }
    let mut meta: Meta = vec![
        ("schema".into(), json::s(ROUTER_SCHEMA)),
        ("project-id".into(), json::s(project_id)),
        ("router-updated".into(), json::s(now)),
    ];
    if let Some(remote) = remote {
        meta.push(("canonical-remote".into(), json::s(remote)));
    }
    let mut text = render_frontmatter(&meta);
    text.push_str("# Project memory\n\nRouter time describes this map, not every file's revision.\n\n## Routes\n\n| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n");
    for (name, purpose, read_when) in CONTEXTS {
        let route = Route {
            path: name.into(),
            purpose: purpose.into(),
            scope: "repository".into(),
            lifecycle: "active".into(),
            read_when: read_when.into(),
        };
        text.push_str(&route.row());
        text.push('\n');
    }
    text.push_str("\n## Omitted contexts\n\nNone.\n");
    let router = bank.join(ROUTER);
    io_at(std::fs::write(&router, text), &router)?;
    written.push(ROUTER.to_owned());
    Ok(written)
}

/// Words used for duplicate detection and search.
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}
