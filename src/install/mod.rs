//! `am install`, `am update` and `am uninstall`: own exactly the files a
//! receipt records, replace them only while they are unchanged, and take
//! over earlier (v0.0.14 Python-era and Rust) receipts the same way.
pub mod release;

use crate::bail;
use crate::json::{self, Value, obj, s};
use crate::util::{
    Error, Result, env_dir, home, io_at, lock, read_optional, sha256_hex, temp_for, uuid4,
    write_atomic, write_atomic_via,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const RECEIPT_SCHEMA: &str = "agent-memory/install-receipt-v4";
/// Receipts this runtime can take over: Python-era v1/v2, Rust v3 and its own.
const READABLE: [&str; 4] = [
    "agent-memory/install-receipt-v1",
    "agent-memory/install-receipt-v2",
    "agent-memory/install-receipt-v3",
    RECEIPT_SCHEMA,
];
pub const BEGIN: &str = "<!-- agent-memory:activation:begin -->";
pub const END: &str = "<!-- agent-memory:activation:end -->";
const SHIM: &str = include_str!("../../skills/agent-memory/templates/activation-markdown.txt");
pub const COMMANDS: [&str; 2] = ["am", "agent-memory"];
pub const HOOK_EVENT: &str = "SessionStart";
const DEVELOPMENT_BINARY: &str = "target/release/am";
const PACK: &str = "skills/agent-memory";

/// Installer-owned state and runtime locations.
pub struct Locations {
    /// `$XDG_STATE_HOME/agent-memory/install`: receipt and lock.
    pub state: PathBuf,
    /// `$XDG_DATA_HOME/agent-memory`.
    pub data: PathBuf,
    /// `$XDG_DATA_HOME/agent-memory/runtime`: signed release runtime root.
    pub runtime: PathBuf,
}

pub fn locations() -> Result<Locations> {
    let data = env_dir("XDG_DATA_HOME", ".local/share")?.join("agent-memory");
    Ok(Locations {
        state: env_dir("XDG_STATE_HOME", ".local/state")?.join("agent-memory/install"),
        runtime: data.join("runtime"),
        data,
    })
}

pub fn read_receipt(locations: &Locations) -> Result<Option<Value>> {
    let path = locations.state.join("receipt.json");
    let Some(raw) = read_optional(&path)? else {
        return Ok(None);
    };
    let value = json::parse(&raw).map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
    let schema = value.str("schema").unwrap_or("");
    if !READABLE.contains(&schema) {
        bail!(
            "{} has unknown schema {schema:?}; it was written by a newer am.",
            path.display()
        );
    }
    Ok(Some(value))
}

fn targets(receipt: &Value) -> &[Value] {
    receipt
        .get("targets")
        .and_then(Value::as_array)
        .unwrap_or(&[])
}

/// Command-line choices for install/update.
#[derive(Default)]
pub struct Options {
    pub development_source: Option<PathBuf>,
    pub bootstrap_binary: Option<PathBuf>,
    pub mode: Option<String>,
    pub presets: Option<String>,
    pub target_dirs: Vec<PathBuf>,
    pub bin_dir: Option<PathBuf>,
    pub no_guidance: bool,
    pub omp_agent_dir: Option<PathBuf>,
    pub dry_run: bool,
}

struct Config {
    mode: String,
    presets: String,
    custom: Vec<PathBuf>,
    no_guidance: bool,
    bin_dir: PathBuf,
    omp_agent_dir: Option<PathBuf>,
}

fn absolute(path: &Path) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    let path = match text.strip_prefix("~/") {
        Some(rest) => home()?.join(rest),
        None => path.to_path_buf(),
    };
    if !path.is_absolute()
        || path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        bail!(
            "Installation paths must be absolute without `..`: {}",
            path.display()
        );
    }
    Ok(path.components().collect())
}

/// New choices override the prior receipt's, which override defaults.
fn config(options: &Options, prior: Option<&Value>) -> Result<Config> {
    let saved = |key: &str| prior.and_then(|p| p.str(key)).map(str::to_owned);
    let mode = options
        .mode
        .clone()
        .or_else(|| saved("mode"))
        .unwrap_or_else(|| "link".into());
    if mode != "link" && mode != "copy" {
        bail!("--mode must be link or copy");
    }
    let custom = if options.target_dirs.is_empty() {
        prior
            .and_then(|p| p.get("custom_targets"))
            .and_then(Value::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .collect()
    } else {
        options
            .target_dirs
            .iter()
            .map(|p| absolute(p))
            .collect::<Result<_>>()?
    };
    let omp_agent_dir = match &options.omp_agent_dir {
        Some(dir) => Some(absolute(dir)?),
        None => prior.and_then(|p| {
            p.str("omp_agent_dir")
                .or_else(|| p.get("omp_binding").and_then(|b| b.str("agent_dir")))
                .map(PathBuf::from)
        }),
    };
    Ok(Config {
        mode,
        presets: options
            .presets
            .clone()
            .or_else(|| saved("presets"))
            .unwrap_or_else(|| "all".into()),
        custom,
        no_guidance: options.no_guidance
            || prior.is_some_and(|p| p.get("no_guidance") == Some(&Value::Bool(true))),
        bin_dir: match &options.bin_dir {
            Some(dir) => absolute(dir)?,
            None => saved("bin_dir")
                .map(PathBuf::from)
                .unwrap_or(home()?.join(".local/bin")),
        },
        omp_agent_dir,
    })
}

/// A client preset: skill root, guidance file and session-start hook.
struct Preset {
    name: &'static str,
    skills: PathBuf,
    guidance: Option<PathBuf>,
    hook_config: Option<PathBuf>,
    extension: Option<PathBuf>,
}

fn presets(cfg: &Config) -> Result<Vec<Preset>> {
    let codex = env_dir("CODEX_HOME", ".codex")?;
    let claude = env_dir("CLAUDE_CONFIG_DIR", ".claude")?;
    let config = env_dir("XDG_CONFIG_HOME", ".config")?;
    let names = match cfg.presets.as_str() {
        "all" => "agents,codex,claude,opencode",
        "none" => "",
        other => other,
    };
    let mut result = Vec::new();
    for name in names.split(',').filter(|n| !n.is_empty()) {
        let preset = match name {
            "agents" => Preset {
                name: "agents",
                skills: home()?.join(".agents/skills"),
                guidance: None,
                hook_config: None,
                extension: None,
            },
            "codex" => Preset {
                name: "codex",
                skills: codex.join("skills"),
                guidance: Some(codex.join("AGENTS.md")),
                hook_config: Some(codex.join("hooks.json")),
                extension: None,
            },
            "claude" => Preset {
                name: "claude",
                skills: claude.join("skills"),
                guidance: Some(claude.join("CLAUDE.md")),
                hook_config: Some(claude.join("settings.json")),
                extension: None,
            },
            "opencode" => Preset {
                name: "opencode",
                skills: config.join("opencode/skills"),
                guidance: Some(config.join("opencode/AGENTS.md")),
                hook_config: None,
                extension: None,
            },
            "omp" => {
                let Some(root) = &cfg.omp_agent_dir else {
                    bail!(
                        "The omp preset needs --omp-agent-dir ABSOLUTE_ROOT (your omp agent directory)."
                    );
                };
                Preset {
                    name: "omp",
                    skills: root.join("skills"),
                    guidance: Some(root.join("AGENTS.md")),
                    hook_config: None,
                    extension: Some(root.join("extensions/agent-memory.ts")),
                }
            }
            other => bail!(
                "Unknown preset {other:?}; use all, none or a comma list of agents,codex,claude,opencode,omp"
            ),
        };
        if result.iter().any(|p: &Preset| p.name == preset.name) {
            bail!("Preset {name} is listed twice");
        }
        result.push(preset);
    }
    Ok(result)
}

/// The runtime an install activates.
struct Source {
    kind: &'static str,
    /// Active root: the runtime directory or the development checkout.
    root: PathBuf,
    pack: PathBuf,
    binary: PathBuf,
    version: String,
    manifest_sha256: Option<String>,
    /// A staged release root to move into place as the runtime.
    staged: Option<release::Scratch>,
}

fn development_source(path: &Path) -> Result<Source> {
    let root = io_at(absolute(path)?.canonicalize(), path)?;
    let pack = root.join(PACK);
    let binary = root.join(DEVELOPMENT_BINARY);
    if !pack.join("SKILL.md").is_file() {
        bail!(
            "{} is not an agent-memory checkout (no {PACK}/SKILL.md)",
            root.display()
        );
    }
    if !binary.is_file() {
        bail!(
            "{} is missing; build it first (cargo build --release). The installer never runs Cargo.",
            binary.display()
        );
    }
    Ok(Source {
        kind: "development",
        root,
        pack,
        binary,
        version: release::VERSION.into(),
        manifest_sha256: None,
        staged: None,
    })
}

fn release_source(
    locations: &Locations,
    observed: &release::Observation,
    binary: &[u8],
) -> Result<Source> {
    io_at(std::fs::create_dir_all(&locations.data), &locations.data)?;
    let staged = release::Scratch::under(&locations.data, ".staging-")?;
    release::stage(&staged.0, observed, binary)?;
    let mode = std::fs::Permissions::from_mode(0o755);
    io_at(std::fs::set_permissions(&staged.0, mode), &staged.0)?;
    Ok(Source {
        kind: "release",
        root: locations.runtime.clone(),
        pack: locations.runtime.join(PACK),
        binary: locations.runtime.join(release::BINARY),
        version: observed.manifest.version.clone(),
        manifest_sha256: Some(sha256_hex(&observed.raw)),
        staged: Some(staged),
    })
}

/// The installed release runtime, verified, without restaging.
fn current_release(locations: &Locations) -> Result<Source> {
    let manifest = release::installed(&locations.runtime).map_err(|e| {
        Error::new(format!(
            "The installed runtime at {} does not verify ({e}). Re-run install.sh to repair it.",
            locations.runtime.display()
        ))
    })?;
    let raw = io_at(
        std::fs::read(locations.runtime.join(release::MANIFEST)),
        &locations.runtime,
    )?;
    Ok(Source {
        kind: "release",
        root: locations.runtime.clone(),
        pack: locations.runtime.join(PACK),
        binary: locations.runtime.join(release::BINARY),
        version: manifest.version,
        manifest_sha256: Some(sha256_hex(&raw)),
        staged: None,
    })
}

// ------------------------------------------------------------ fingerprints

/// Receipt identity of a path: link target, file hash and mode, or the
/// directory digest (sorted-key indented JSON of every entry, as v0.0.14 wrote).
pub fn fingerprint(path: &Path) -> Result<Option<Value>> {
    let meta = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => bail!("{}: {e}", path.display()),
    };
    let mode = Value::Int((meta.permissions().mode() & 0o7777) as i64);
    if meta.is_dir() {
        let mut entries = Vec::new();
        let mut stack = vec![(path.to_path_buf(), String::new())];
        while let Some((dir, prefix)) = stack.pop() {
            for entry in io_at(std::fs::read_dir(&dir), &dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let relative = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                let child = entry.path();
                let child_meta = io_at(child.symlink_metadata(), &child)?;
                if child_meta.is_dir() {
                    let mode = Value::Int((child_meta.permissions().mode() & 0o7777) as i64);
                    entries.push((
                        relative.clone(),
                        obj([("kind", s("directory")), ("mode", mode)]),
                    ));
                    stack.push((child, relative));
                } else {
                    let leaf = fingerprint(&child)?.unwrap_or(Value::Null);
                    entries.push((relative, leaf));
                }
            }
        }
        let mut encoded = json::encode(&Value::Object(entries), json::SORTED_PRETTY_ASCII);
        encoded.push('\n');
        return Ok(Some(obj([
            ("kind", s("directory")),
            ("hash", s(sha256_hex(encoded.as_bytes()))),
            ("mode", mode),
        ])));
    }
    if meta.file_type().is_symlink() {
        let target = io_at(std::fs::read_link(path), path)?;
        return Ok(Some(obj([
            ("kind", s("link")),
            ("target", s(target.to_string_lossy())),
        ])));
    }
    if meta.is_file() {
        let raw = io_at(std::fs::read(path), path)?;
        return Ok(Some(obj([
            ("kind", s("file")),
            ("hash", s(sha256_hex(&raw))),
            ("mode", mode),
        ])));
    }
    bail!("{} is not a file, directory or link", path.display())
}

fn identity_matches(target: &Value, path: &Path) -> bool {
    match (target.get("identity"), fingerprint(path)) {
        (Some(expected), Ok(Some(actual))) => json::same(expected, &actual),
        _ => false,
    }
}

// ------------------------------------------------------------ text edits

/// Insert or replace the owned activation block.
fn with_activation(text: &str, owned_hash: Option<&str>) -> Result<String> {
    let begins = text.matches(BEGIN).count();
    if begins != text.matches(END).count() || begins > 1 {
        bail!("has unbalanced or duplicate agent-memory activation markers; fix them by hand");
    }
    if let (Some(start), Some(end)) = (text.find(BEGIN), text.find(END)) {
        let finish = end + END.len();
        let block = &text[start..finish.max(start)];
        let ours = owned_hash.is_some_and(|h| sha256_hex(block.as_bytes()) == h)
            || block == SHIM.trim_end();
        if start > end || !ours {
            bail!(
                "holds an agent-memory activation block that was edited by hand; remove it or restore it, then retry"
            );
        }
        return Ok(format!(
            "{}{}{}",
            &text[..start],
            SHIM.trim_end(),
            &text[finish..]
        ));
    }
    let mut out = text.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(SHIM.trim_end());
    out.push('\n');
    Ok(out)
}

/// Remove the owned block; `None` when it is missing or edited.
fn without_activation(text: &str, owned_hash: &str) -> Option<String> {
    let start = text.find(BEGIN)?;
    let finish = text.find(END)? + END.len();
    if start > finish || sha256_hex(text[start..finish].as_bytes()) != owned_hash {
        return None;
    }
    let mut rest = &text[finish..];
    if text[..start].is_empty() || text[..start].ends_with('\n') {
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    Some(format!("{}{rest}", &text[..start]))
}

fn hook_entry(binary: &Path, client: &str) -> Value {
    let quoted = shell_quote(&binary.to_string_lossy());
    obj([(
        "hooks",
        Value::Array(vec![obj([
            ("type", s("command")),
            (
                "command",
                s(format!("{quoted} hook session-start --client {client}")),
            ),
        ])]),
    )])
}

pub fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn hook_groups<'a>(document: &'a mut Value, event: &str) -> Option<&'a mut Vec<Value>> {
    match document.get_mut("hooks")?.get_mut(event)? {
        Value::Array(groups) => Some(groups),
        _ => None,
    }
}

/// Remove one owned group; restore the original shape of `hooks`/`event`.
fn remove_group(document: &mut Value, entry: &Value, event: &str, shape: Option<&Value>) -> bool {
    let Some(groups) = hook_groups(document, event) else {
        return false;
    };
    let Some(index) = groups.iter().position(|g| json::same(g, entry)) else {
        return false;
    };
    groups.remove(index);
    let empty = groups.is_empty();
    let flag = |key: &str| shape.and_then(|s| s.get(key)) == Some(&Value::Bool(true));
    if empty && !flag("event_present") {
        if let Some(hooks) = document.get_mut("hooks") {
            hooks.remove(event);
        }
    }
    if document
        .get("hooks")
        .and_then(Value::as_object)
        .is_some_and(<[_]>::is_empty)
        && !flag("hooks_present")
    {
        document.remove("hooks");
    }
    true
}

/// A client's hook configuration after removing prior groups and adding
/// the desired one; `(bytes or None to delete, receipt fields)`. `created`:
/// an interrupted earlier install created the file (recorded proof).
fn edit_hooks(
    path: &Path,
    prior: Option<&Value>,
    desired: Option<&Value>,
    created: bool,
) -> Result<(Option<Vec<u8>>, Value)> {
    let raw = read_optional(path)?;
    let mut document = match &raw {
        Some(raw) if !raw.iter().all(u8::is_ascii_whitespace) => {
            json::parse(raw).map_err(|e| Error::new(format!("{}: {e}", path.display())))?
        }
        _ => Value::Object(Vec::new()),
    };
    if document.as_object().is_none()
        || document
            .get("hooks")
            .is_some_and(|h| h.as_object().is_none())
    {
        bail!(
            "{} has an unsupported shape; fix it by hand",
            path.display()
        );
    }
    let mut originally_absent = raw.is_none() || created;
    if let Some(prior) = prior {
        let event = prior.str("event").unwrap_or("UserPromptSubmit");
        if let Some(entry) = prior.get("hook_entry") {
            remove_group(
                &mut document,
                entry,
                event,
                prior.get("hook_original_shape"),
            );
        }
        if let Some(entry) = prior.get("stop_entry").filter(|e| **e != Value::Null) {
            remove_group(
                &mut document,
                entry,
                "Stop",
                prior.get("stop_original_shape"),
            );
        }
        originally_absent = prior.get("originally_absent") == Some(&Value::Bool(true));
    }
    let mut fields = Value::Object(Vec::new());
    if let Some(entry) = desired {
        // An identical group left by an interrupted install is ours, not
        // part of the file's original shape.
        remove_group(&mut document, entry, HOOK_EVENT, None);
        let shape = obj([
            (
                "event_present",
                Value::Bool(
                    document
                        .get("hooks")
                        .and_then(|h| h.get(HOOK_EVENT))
                        .is_some(),
                ),
            ),
            (
                "hooks_present",
                Value::Bool(document.get("hooks").is_some()),
            ),
        ]);
        if document.get("hooks").is_none() {
            document.set("hooks", Value::Object(Vec::new()));
        }
        if let Some(hooks) = document.get_mut("hooks") {
            if hooks.get(HOOK_EVENT).is_none() {
                hooks.set(HOOK_EVENT, Value::Array(Vec::new()));
            }
        }
        let Some(groups) = hook_groups(&mut document, HOOK_EVENT) else {
            bail!(
                "{} has an unsupported hooks.{HOOK_EVENT} shape",
                path.display()
            );
        };
        groups.push(entry.clone());
        fields = obj([
            ("event", s(HOOK_EVENT)),
            ("hook_entry", entry.clone()),
            ("hook_original_shape", shape),
            ("originally_absent", Value::Bool(originally_absent)),
        ]);
    }
    // Only a file the installer created, left with nothing but empty
    // objects and arrays, is removed; anything else keeps the user's file.
    let empty = originally_absent && no_content(&document);
    let bytes = (!empty).then(|| json::pretty_file(&document));
    Ok((bytes, fields))
}

fn no_content(value: &Value) -> bool {
    match value {
        Value::Object(pairs) => pairs.iter().all(|(_, v)| no_content(v)),
        Value::Array(items) => items.iter().all(no_content),
        _ => false,
    }
}

/// The file listing what an install/uninstall in progress created, so a
/// killed run's leftovers are cleaned up by proof, never by name pattern.
const LEFTOVERS: &str = "leftovers";

/// Finish what a killed run recorded: remove its temporary files, put
/// moved-aside directories back (or drop them once replaced), and return
/// the files it created (they count as created by this install).
fn recover_leftovers(state: &Path) -> BTreeSet<PathBuf> {
    let mut created = BTreeSet::new();
    let record = state.join(LEFTOVERS);
    let text = std::fs::read_to_string(&record).unwrap_or_default();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["temp", temp] => {
                if Path::new(temp)
                    .symlink_metadata()
                    .is_ok_and(|m| m.is_file())
                {
                    let _ = std::fs::remove_file(temp);
                }
            }
            ["aside", original, aside] if Path::new(aside).is_dir() => {
                let _ = if Path::new(original).symlink_metadata().is_ok() {
                    std::fs::remove_dir_all(aside)
                } else {
                    std::fs::rename(aside, original)
                };
            }
            ["created", path] if Path::new(path).exists() => {
                created.insert(PathBuf::from(path));
            }
            _ => {}
        }
    }
    // The receipt's own temporary lives in am's state directory.
    for entry in std::fs::read_dir(state).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name
            .strip_prefix(".receipt.json.am-")
            .is_some_and(crate::util::is_uuid)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let _ = std::fs::remove_file(&record);
    created
}

/// `.<name>.am-*` files beside `path` that no am run recorded: possibly a
/// user's, so they are left in place and only reported.
pub fn lookalikes(path: &Path) -> Vec<PathBuf> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Vec::new();
    };
    let prefix = format!(".{}.am-", name.to_string_lossy());
    std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect()
}

// ------------------------------------------------------------ transactions

enum Before {
    Absent,
    File(Vec<u8>, u32),
    Link(PathBuf),
    /// A directory moved to this sibling path.
    Aside(PathBuf),
}

/// Applied changes, undone in reverse when a later step fails. Every
/// temporary, moved-aside or created path is written to the leftovers
/// record first, so a killed run can be finished safely.
struct Journal {
    entries: Vec<(PathBuf, Before)>,
    record: PathBuf,
    /// Files an interrupted earlier run created.
    created_before: BTreeSet<PathBuf>,
}

impl Journal {
    fn open(state: &Path) -> Result<Journal> {
        io_at(std::fs::create_dir_all(state), state)?;
        let journal = Journal {
            entries: Vec::new(),
            record: state.join(LEFTOVERS),
            created_before: recover_leftovers(state),
        };
        for path in &journal.created_before {
            journal.note(&["created", &path.to_string_lossy()])?;
        }
        Ok(journal)
    }
    fn note(&self, fields: &[&str]) -> Result<()> {
        use std::io::Write;
        if fields.iter().any(|f| f.contains(['\t', '\n'])) {
            bail!("Installation paths must not contain tabs or newlines");
        }
        let mut file = io_at(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.record),
            &self.record,
        )?;
        io_at(writeln!(file, "{}", fields.join("\t")), &self.record)?;
        io_at(file.sync_all(), &self.record)
    }
    /// Move whatever is at `path` out of the way, remembering it.
    fn clear(&mut self, path: &Path) -> Result<()> {
        let before = match path.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Before::Absent,
            Err(e) => bail!("{}: {e}", path.display()),
            Ok(meta) if meta.is_dir() => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let aside = path.with_file_name(format!(".{name}.am-old-{}", uuid4()));
                self.note(&["aside", &path.to_string_lossy(), &aside.to_string_lossy()])?;
                io_at(std::fs::rename(path, &aside), path)?;
                Before::Aside(aside)
            }
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = io_at(std::fs::read_link(path), path)?;
                io_at(std::fs::remove_file(path), path)?;
                Before::Link(target)
            }
            Ok(meta) => {
                let raw = io_at(std::fs::read(path), path)?;
                io_at(std::fs::remove_file(path), path)?;
                Before::File(raw, meta.permissions().mode() & 0o7777)
            }
        };
        self.entries.push((path.to_path_buf(), before));
        Ok(())
    }
    /// Replace a text file in place (atomically), remembering the old bytes.
    /// A linked file (`~/.claude/settings.json -> dotfiles`) is edited at
    /// its target so the link stays.
    fn write(&mut self, path: &Path, raw: Option<&[u8]>) -> Result<()> {
        let resolved;
        let path = if is_link(path) && raw.is_some() {
            resolved = io_at(path.canonicalize(), path)?;
            resolved.as_path()
        } else {
            path
        };
        let old = read_optional(path)?;
        let mode = path
            .metadata()
            .map(|m| m.permissions().mode() & 0o7777)
            .unwrap_or(0o644);
        let absent = old.is_none();
        let before = match old {
            Some(raw) => Before::File(raw, mode),
            None => Before::Absent,
        };
        self.entries.push((path.to_path_buf(), before));
        match raw {
            Some(raw) => {
                if let Some(parent) = path.parent() {
                    io_at(std::fs::create_dir_all(parent), parent)?;
                }
                let temp = temp_for(path);
                self.note(&["temp", &temp.to_string_lossy()])?;
                if absent {
                    self.note(&["created", &path.to_string_lossy()])?;
                }
                write_atomic_via(path, &temp, raw, mode)
            }
            None => io_at(std::fs::remove_file(path), path),
        }
    }
    fn rollback(self) {
        let record = self.record.clone();
        for (path, before) in self.entries.into_iter().rev() {
            match path.symlink_metadata() {
                Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&path)),
                Ok(_) => drop(std::fs::remove_file(&path)),
                Err(_) => {}
            }
            let _ = match before {
                Before::Absent => Ok(()),
                Before::File(raw, mode) => std::fs::write(&path, raw).and_then(|()| {
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                }),
                Before::Link(target) => std::os::unix::fs::symlink(target, &path),
                Before::Aside(aside) => std::fs::rename(aside, &path),
            };
        }
        let _ = std::fs::remove_file(record);
    }
    fn commit(self) {
        for (_, before) in self.entries {
            if let Before::Aside(aside) = before {
                let _ = std::fs::remove_dir_all(aside);
            }
        }
        let _ = std::fs::remove_file(self.record);
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    io_at(std::fs::create_dir(to), to)?;
    for entry in io_at(std::fs::read_dir(from), from)? {
        let entry = entry?;
        let destination = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else {
            io_at(std::fs::copy(entry.path(), &destination), &destination)?;
        }
    }
    Ok(())
}

/// What one owned path should become.
enum Want {
    /// Move the staged release root into place, or keep the current one.
    Runtime(Option<PathBuf>),
    Link(&'static str, PathBuf),
    Copy(PathBuf),
    File(Vec<u8>),
    Activation,
    Hooks(String, Value),
}

struct Item {
    path: PathBuf,
    want: Option<Want>,
    prior: Option<Value>,
}

impl Item {
    fn kind(&self) -> &str {
        match &self.want {
            Some(Want::Runtime(_)) => "source",
            Some(Want::Link(kind, _)) => kind,
            Some(Want::Copy(_)) => "copy",
            Some(Want::File(_)) => "wrapper",
            Some(Want::Activation) => "activation",
            Some(Want::Hooks(..)) => "hook-config",
            None => self
                .prior
                .as_ref()
                .and_then(|p| p.str("kind"))
                .unwrap_or("file"),
        }
    }
    /// Does the path already hold exactly what is wanted?
    fn already(&self) -> bool {
        self.want
            .as_ref()
            .is_some_and(|want| item_already(&self.path, want))
    }
}

fn plan(source: &Source, cfg: &Config, prior: Option<&Value>) -> Result<Vec<Item>> {
    let mut wants: Vec<(PathBuf, Want)> = Vec::new();
    if source.kind == "release" {
        wants.push((
            source.root.clone(),
            Want::Runtime(source.staged.as_ref().map(|s| s.0.clone())),
        ));
    }
    let skill = |path: PathBuf| -> (PathBuf, Want) {
        if cfg.mode == "copy" {
            (path, Want::Copy(source.pack.clone()))
        } else {
            (path, Want::Link("link", source.pack.clone()))
        }
    };
    for preset in presets(cfg)? {
        wants.push(skill(preset.skills.join("agent-memory")));
        if cfg.no_guidance {
            continue;
        }
        if let Some(guidance) = preset.guidance {
            wants.push((guidance, Want::Activation));
        }
        if let Some(config) = preset.hook_config {
            wants.push((
                config,
                Want::Hooks(preset.name.into(), hook_entry(&source.binary, preset.name)),
            ));
        }
        if let Some(extension) = preset.extension {
            let module = json::encode(
                &s(source.pack.join("hooks/omp.ts").to_string_lossy()),
                json::LINE,
            );
            let options = json::encode(
                &obj([("executable", s(source.binary.to_string_lossy()))]),
                json::LINE,
            );
            let raw = format!(
                "// Owned by agent-memory (install receipt). Session-start project memory only.\nimport agentMemory from {module};\nexport default pi => agentMemory(pi, {options});\n"
            );
            wants.push((extension, Want::File(raw.into_bytes())));
        }
    }
    for dir in &cfg.custom {
        wants.push(skill(dir.join("agent-memory")));
    }
    for name in COMMANDS {
        wants.push((
            cfg.bin_dir.join(name),
            Want::Link("command-link", source.binary.clone()),
        ));
    }
    let mut prior_targets: Vec<Value> = prior.map(|p| targets(p).to_vec()).unwrap_or_default();
    let mut items = Vec::new();
    for (path, want) in wants {
        if items.iter().any(|i: &Item| i.path == path) {
            bail!("Two installation targets share {}", path.display());
        }
        // Receipts may record a resolved parent (`~/.claude -> dotfiles`).
        let here = resolve_parent(&path);
        let index = prior_targets.iter().position(|t| {
            t.str("path")
                .is_some_and(|p| Path::new(p) == path || resolve_parent(Path::new(p)) == here)
        });
        items.push(Item {
            prior: index.map(|i| prior_targets.remove(i)),
            path,
            want: Some(want),
        });
    }
    // Prior-only targets are released last, links before their sources.
    for target in prior_targets.into_iter().rev() {
        if let Some(path) = target.str("path") {
            items.push(Item {
                path: PathBuf::from(path),
                want: None,
                prior: Some(target),
            });
        }
    }
    Ok(items)
}

/// `path` with its parent directory resolved (links followed), when it exists.
fn resolve_parent(path: &Path) -> PathBuf {
    match (path.parent().map(Path::canonicalize), path.file_name()) {
        (Some(Ok(parent)), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

/// Check every item before any change; returns notes about targets left in place.
fn preflight(items: &[Item], releasing: bool) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    for item in items {
        let path = item.path.display();
        match (&item.want, &item.prior) {
            (Some(Want::Activation), prior) => {
                let text =
                    read_optional(&item.path)?.map(|r| String::from_utf8_lossy(&r).into_owned());
                let hash = prior.as_ref().and_then(|p| p.str("block_hash"));
                if let Err(e) = with_activation(text.as_deref().unwrap_or(""), hash) {
                    problems.push(format!("{path} {e}"));
                }
            }
            (Some(Want::Hooks(..)), prior) => {
                if let Err(e) = edit_hooks(&item.path, prior.as_ref(), None, false) {
                    problems.push(e.0);
                }
            }
            (Some(Want::Runtime(None)), _) => {}
            (Some(_), Some(prior)) => {
                if !identity_matches(prior, &item.path) && !item.already() {
                    problems.push(format!("{path} changed since agent-memory installed it; move it away or restore it, then retry"));
                }
            }
            (Some(_), None) => {
                if item.path.symlink_metadata().is_ok() && !item.already() {
                    problems.push(format!("{path} already exists and is not owned by agent-memory; move it away, then retry"));
                }
            }
            (None, Some(prior)) => {
                let owned = match item.kind() {
                    "activation" => read_optional(&item.path)?.is_some_and(|raw| {
                        without_activation(
                            &String::from_utf8_lossy(&raw),
                            prior.str("block_hash").unwrap_or(""),
                        )
                        .is_some()
                    }),
                    "hook-config" => edit_hooks(&item.path, Some(prior), None, false).is_ok(),
                    _ => {
                        item.path.symlink_metadata().is_err() || identity_matches(prior, &item.path)
                    }
                };
                if !owned {
                    let verb = if releasing {
                        "uninstall"
                    } else {
                        "this install"
                    };
                    notes.push(format!(
                        "Left in place (changed since install, so {verb} does not own it): {path}"
                    ));
                }
            }
            (None, None) => {}
        }
    }
    if !problems.is_empty() {
        bail!("Nothing was changed:\n  {}", problems.join("\n  "));
    }
    Ok(notes)
}

/// Apply one item; returns its receipt target, if it is owned afterwards.
fn apply(item: &mut Item, journal: &mut Journal) -> Result<Option<Value>> {
    let path_text = item.path.to_string_lossy().into_owned();
    let kind = item.kind().to_owned();
    let record = |fields: Value| -> Value {
        let mut target = obj([("kind", s(&kind)), ("path", s(&path_text))]);
        for (key, value) in fields.as_object().unwrap_or(&[]) {
            target.set(key, value.clone());
        }
        target
    };
    let identity = |path: &Path| -> Result<Value> {
        Ok(obj([(
            "identity",
            fingerprint(path)?.unwrap_or(Value::Null),
        )]))
    };
    let prior = item.prior.take();
    match item.want.take() {
        Some(Want::Activation) => {
            let text = read_optional(&item.path)?.map(|r| String::from_utf8_lossy(&r).into_owned());
            let hash = prior.as_ref().and_then(|p| p.str("block_hash"));
            let updated = with_activation(text.as_deref().unwrap_or(""), hash)?;
            // Created by this install, or by an interrupted one (recorded).
            let originally_absent = match &prior {
                Some(p) => p.get("originally_absent") == Some(&Value::Bool(true)),
                None => text.is_none() || journal.created_before.contains(&item.path),
            };
            if text.as_deref() != Some(updated.as_str()) {
                journal.write(&item.path, Some(updated.as_bytes()))?;
            }
            Ok(Some(record(obj([
                ("block_hash", s(sha256_hex(SHIM.trim_end().as_bytes()))),
                ("originally_absent", Value::Bool(originally_absent)),
            ]))))
        }
        Some(Want::Hooks(client, entry)) => {
            let created = journal.created_before.contains(&item.path);
            let (raw, mut fields) = edit_hooks(&item.path, prior.as_ref(), Some(&entry), created)?;
            if read_optional(&item.path)? != raw {
                journal.write(&item.path, raw.as_deref())?;
            }
            fields.set("client", s(client));
            Ok(Some(record(fields)))
        }
        Some(Want::Runtime(None)) => Ok(Some(record(identity(&item.path)?))),
        Some(want) => {
            if !item_already(&item.path, &want) {
                journal.clear(&item.path)?;
                if let Some(parent) = item.path.parent() {
                    io_at(std::fs::create_dir_all(parent), parent)?;
                }
                match &want {
                    Want::Runtime(Some(staged)) => {
                        io_at(std::fs::rename(staged, &item.path), &item.path)?
                    }
                    Want::Link(_, to) => {
                        io_at(std::os::unix::fs::symlink(to, &item.path), &item.path)?
                    }
                    Want::Copy(from) => copy_tree(from, &item.path)?,
                    Want::File(raw) => journal.write(&item.path, Some(raw))?,
                    _ => {}
                }
            }
            Ok(Some(record(identity(&item.path)?)))
        }
        None => {
            let Some(prior) = prior else { return Ok(None) };
            match kind.as_str() {
                "activation" => {
                    if let Some(raw) = read_optional(&item.path)? {
                        let text = String::from_utf8_lossy(&raw);
                        if let Some(rest) =
                            without_activation(&text, prior.str("block_hash").unwrap_or(""))
                        {
                            // Delete the whole file only when the installer
                            // created it and nothing else is left in it.
                            let created =
                                prior.get("originally_absent") == Some(&Value::Bool(true));
                            let delete = created && rest.trim().is_empty() && !is_link(&item.path);
                            journal.write(&item.path, (!delete).then_some(rest.as_bytes()))?;
                        }
                    }
                }
                "hook-config" => {
                    if let Ok((raw, _)) = edit_hooks(&item.path, Some(&prior), None, false) {
                        // A linked config is kept (as `{}`) rather than unlinked.
                        let raw = raw.or_else(|| is_link(&item.path).then(|| b"{}\n".to_vec()));
                        if read_optional(&item.path)? != raw {
                            journal.write(&item.path, raw.as_deref())?;
                        }
                    }
                }
                _ => {
                    if item.path.symlink_metadata().is_ok() && identity_matches(&prior, &item.path)
                    {
                        journal.clear(&item.path)?;
                    }
                }
            }
            Ok(None)
        }
    }
}

fn is_link(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
}

fn item_already(path: &Path, want: &Want) -> bool {
    match want {
        Want::Link(_, to) => std::fs::read_link(path).is_ok_and(|t| t == *to),
        Want::File(raw) => std::fs::read(path).is_ok_and(|r| r == *raw),
        _ => false,
    }
}

fn describe(item: &Item) -> String {
    let path = item.path.display();
    match &item.want {
        Some(Want::Runtime(Some(_))) => format!("install signed runtime  {path}"),
        Some(Want::Runtime(None)) => format!("keep runtime            {path}"),
        Some(Want::Link(_, to)) => format!("link                    {path} -> {}", to.display()),
        Some(Want::Copy(from)) => format!("copy                    {path} <- {}", from.display()),
        Some(Want::File(_)) => format!("write omp extension     {path}"),
        Some(Want::Activation) => format!("activation block in     {path}"),
        Some(Want::Hooks(..)) => format!("SessionStart hook in    {path}"),
        None => format!("release earlier target  {path}"),
    }
}

/// Plan and apply an installation; the receipt is written last.
fn activate(
    locations: &Locations,
    source: Source,
    cfg: Config,
    prior: Option<&Value>,
    dry_run: bool,
) -> Result<String> {
    let mut items = plan(&source, &cfg, prior)?;
    let notes = preflight(&items, false)?;
    let mut out = String::new();
    if dry_run {
        let _ = writeln!(
            out,
            "Would install agent-memory {} ({}):",
            source.version, source.kind
        );
        for item in &items {
            let _ = writeln!(out, "  {}", describe(item));
        }
        for note in &notes {
            let _ = writeln!(out, "  {note}");
        }
        return Ok(out);
    }
    // Finishes what an interrupted run recorded (we hold the install lock).
    let mut journal = Journal::open(&locations.state)?;
    let mut owned = Vec::new();
    let mut lines = Vec::new();
    for item in &mut items {
        lines.push(describe(item));
        match apply(item, &mut journal) {
            Ok(Some(target)) => owned.push(target),
            Ok(None) => {}
            Err(e) => {
                journal.rollback();
                bail!("Installation failed and was rolled back: {e}");
            }
        }
    }
    let mut receipt = obj([
        ("schema", s(RECEIPT_SCHEMA)),
        ("installed_at", s(crate::util::now())),
        ("version", s(&source.version)),
        (
            "source",
            obj([
                ("kind", s(source.kind)),
                ("path", s(source.root.to_string_lossy())),
                ("pack", s(source.pack.to_string_lossy())),
                ("binary", s(source.binary.to_string_lossy())),
                ("version", s(&source.version)),
            ]),
        ),
        ("bin_dir", s(cfg.bin_dir.to_string_lossy())),
        ("mode", s(&cfg.mode)),
        ("presets", s(&cfg.presets)),
        ("no_guidance", Value::Bool(cfg.no_guidance)),
        (
            "custom_targets",
            json::strings(cfg.custom.iter().map(|p| p.to_string_lossy())),
        ),
        ("targets", Value::Array(owned)),
    ]);
    if let Some(sha) = &source.manifest_sha256 {
        if let Some(src) = receipt.get_mut("source") {
            src.set("release_manifest_sha256", s(sha));
        }
    }
    if let Some(dir) = &cfg.omp_agent_dir {
        receipt.set("omp_agent_dir", s(dir.to_string_lossy()));
    }
    let written =
        io_at(std::fs::create_dir_all(&locations.state), &locations.state).and_then(|()| {
            write_atomic(
                &locations.state.join("receipt.json"),
                &json::pretty_file(&receipt),
                0o600,
            )
        });
    if let Err(e) = written {
        journal.rollback();
        bail!("Could not write the installation receipt, so the installation was rolled back: {e}");
    }
    journal.commit();
    if let Some(staged) = source.staged {
        staged.keep(); // Moved into place as the runtime.
    }
    let _ = writeln!(
        out,
        "Installed agent-memory {} ({}).",
        source.version, source.kind
    );
    for line in lines {
        let _ = writeln!(out, "  {line}");
    }
    for note in notes {
        let _ = writeln!(out, "  {note}");
    }
    if prior.is_some_and(|p| p.str("schema") != Some(RECEIPT_SCHEMA)) {
        let _ = writeln!(
            out,
            "Took over the earlier installation recorded by {}.",
            prior.and_then(|p| p.str("schema")).unwrap_or("")
        );
    }
    let _ = writeln!(
        out,
        "Restart running Claude, Codex and omp sessions to load the session-start hook."
    );
    Ok(out)
}

/// The version a receipt records (v0.0.14 keeps it under `source`).
fn installed_version(receipt: &Value) -> Option<&str> {
    receipt
        .str("version")
        .or_else(|| receipt.get("source").and_then(|s| s.str("version")))
}

/// Remove `.staging-<uuid>` runtime roots an interrupted install left.
fn sweep_staging(data: &Path) {
    for entry in std::fs::read_dir(data).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name
            .strip_prefix(".staging-")
            .is_some_and(crate::util::is_uuid)
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `am install`.
pub fn install(options: &Options) -> Result<String> {
    let locations = locations()?;
    io_at(std::fs::create_dir_all(&locations.state), &locations.state)?;
    let _lock = lock(&locations.state.join("lock"))?;
    let prior = read_receipt(&locations)?;
    if !options.dry_run {
        sweep_staging(&locations.data); // am's own data directory only
    }
    let prior_kind = prior
        .as_ref()
        .and_then(|p| p.get("source"))
        .and_then(|s| s.str("kind"));
    let rust_era = prior
        .as_ref()
        .is_some_and(|p| p.str("schema") == Some(RECEIPT_SCHEMA));
    let source = if let Some(dir) = &options.development_source {
        development_source(dir)?
    } else if let Some(binary) = &options.bootstrap_binary {
        let observed = release::probe()?;
        let installed = prior
            .as_ref()
            .and_then(installed_version)
            .unwrap_or("0.0.0");
        let latest = &observed.manifest.version;
        if release::version_key(latest) < release::version_key(installed) {
            bail!(
                "The latest release ({latest}) is older than the installed {installed}; nothing changed."
            );
        }
        let raw = release::bind_bootstrap(&observed.manifest, binary)?;
        release_source(&locations, &observed, &raw)?
    } else if rust_era && prior_kind == Some("development") {
        let path = prior
            .as_ref()
            .and_then(|p| p.get("source"))
            .and_then(|s| s.str("path"))
            .unwrap_or("");
        development_source(Path::new(path))?
    } else if rust_era && prior_kind == Some("release") {
        current_release(&locations)?
    } else {
        bail!(
            "A signed release installs through the bootstrap: curl -fsSL https://github.com/robertelee78/agent-memory/releases/latest/download/install.sh | sh"
        );
    };
    let cfg = config(options, prior.as_ref())?;
    activate(&locations, source, cfg, prior.as_ref(), options.dry_run)
}

/// `am update [--check]`.
pub fn update(check: bool, bootstrap: Option<&Path>) -> Result<String> {
    let locations = locations()?;
    io_at(std::fs::create_dir_all(&locations.state), &locations.state)?;
    let _lock = lock(&locations.state.join("lock"))?;
    let Some(prior) = read_receipt(&locations)? else {
        bail!("agent-memory is not installed; run install.sh first.");
    };
    if prior.str("schema") != Some(RECEIPT_SCHEMA) {
        bail!("This is an earlier agent-memory installation; re-run install.sh to take it over.");
    }
    let kind = prior
        .get("source")
        .and_then(|s| s.str("kind"))
        .unwrap_or("");
    let cfg = config(&Options::default(), Some(&prior))?;
    if kind == "development" {
        let path = prior
            .get("source")
            .and_then(|s| s.str("path"))
            .unwrap_or("");
        if check {
            return Ok(format!(
                "Development install from {path}; rebuild it to update."
            ));
        }
        return activate(
            &locations,
            development_source(Path::new(path))?,
            cfg,
            Some(&prior),
            false,
        );
    }
    let installed = prior.str("version").unwrap_or("0.0.0").to_owned();
    let observed = release::probe()?;
    let latest = observed.manifest.version.clone();
    let newer = release::version_key(&latest) > release::version_key(&installed);
    let intact = release::installed(&locations.runtime).is_ok();
    if check {
        return Ok(if newer {
            format!("agent-memory {latest} is available (installed {installed}); run am update.")
        } else {
            format!("agent-memory {installed} is up to date.")
        });
    }
    if !newer && intact && bootstrap.is_none() {
        return Ok(format!("agent-memory {installed} is up to date."));
    }
    if release::version_key(&latest) < release::version_key(&installed) {
        bail!(
            "The latest release ({latest}) is older than the installed {installed}; nothing changed."
        );
    }
    let binary = match bootstrap {
        Some(path) => release::bind_bootstrap(&observed.manifest, path)?,
        None => release::fetch_binary(&observed)?,
    };
    let source = release_source(&locations, &observed, &binary)?;
    activate(&locations, source, cfg, Some(&prior), false)
}

/// `am uninstall`: release every owned target, then the receipt.
pub fn uninstall(dry_run: bool) -> Result<String> {
    let locations = locations()?;
    let Some(prior) = read_receipt(&locations)? else {
        return Ok("agent-memory is not installed; nothing to remove.".into());
    };
    let _lock = lock(&locations.state.join("lock"))?;
    let mut items: Vec<Item> = targets(&prior)
        .iter()
        .rev()
        .filter_map(|t| {
            t.str("path").map(|p| Item {
                path: PathBuf::from(p),
                want: None,
                prior: Some(t.clone()),
            })
        })
        .collect();
    let notes = preflight(&items, true)?;
    let mut out = String::new();
    if dry_run {
        out.push_str("Would remove:\n");
        for item in &items {
            let _ = writeln!(out, "  {} {}", item.kind(), item.path.display());
        }
    } else {
        sweep_staging(&locations.data);
        let mut journal = Journal::open(&locations.state)?;
        for item in &mut items {
            if let Err(e) = apply(item, &mut journal) {
                journal.rollback();
                bail!("Uninstall failed and was rolled back: {e}");
            }
        }
        journal.commit();
        let receipt = locations.state.join("receipt.json");
        io_at(std::fs::remove_file(&receipt), &receipt)?;
        let _ = std::fs::remove_file(locations.state.join("lock"));
        let _ = std::fs::remove_dir(&locations.state);
        if let Some(parent) = locations.state.parent() {
            let _ = std::fs::remove_dir(parent); // Only when empty.
        }
        let _ = std::fs::remove_dir(&locations.data);
        out.push_str("Uninstalled agent-memory. Repository memory banks were not touched.\n");
    }
    for note in notes {
        let _ = writeln!(out, "  {note}");
    }
    Ok(out)
}

/// A receipt target and whether it still holds what was installed.
pub struct TargetCheck {
    pub kind: String,
    pub path: String,
    /// The client a session-start hook serves (`claude`, `codex`, `omp`).
    pub client: Option<String>,
    pub ok: bool,
}

/// One check per receipt target.
pub fn check_targets(receipt: &Value) -> Vec<TargetCheck> {
    targets(receipt)
        .iter()
        .filter_map(|target| {
            let path = PathBuf::from(target.str("path")?);
            let ok = match target.str("kind")? {
                "activation" => read_optional(&path).ok().flatten().is_some_and(|raw| {
                    without_activation(
                        &String::from_utf8_lossy(&raw),
                        target.str("block_hash").unwrap_or(""),
                    )
                    .is_some()
                }),
                "hook-config" => read_optional(&path).ok().flatten().is_some_and(|raw| {
                    let mut doc = json::parse(&raw).unwrap_or(Value::Null);
                    let event = target.str("event").unwrap_or("UserPromptSubmit");
                    target
                        .get("hook_entry")
                        .is_some_and(|e| remove_group(&mut doc, e, event, None))
                }),
                _ => identity_matches(target, &path),
            };
            let kind = target.str("kind")?.to_owned();
            let client = match kind.as_str() {
                "hook-config" => target.str("client").map(str::to_owned),
                "wrapper" if path.ends_with("extensions/agent-memory.ts") => Some("omp".into()),
                _ => None,
            };
            Some(TargetCheck {
                kind,
                path: path.display().to_string(),
                client,
                ok,
            })
        })
        .collect()
}
