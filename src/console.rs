//! Human CLI presentation; the typed command results remain the JSON authority.
//!
//! Ports baseline `console.py`: the installed-name rule, `safe`, the top-level
//! help text, the `--json`/`--verbose` selectors, the guarded bootstrap
//! rewrite and the human rendering of lifecycle and diagnostic results.

use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::{ContractError, Result};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::{Arc, RwLock};

pub const NAMES: [&str; 2] = ["am", "agent-memory"];
pub const HUMAN: [&str; 8] = [
    "install",
    "update",
    "uninstall",
    "version",
    "status",
    "doctor",
    "recover",
    "migrate",
];

/// Which stream a piece of console output belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stdout,
    Stderr,
}

/// Console output in the order the baseline printed it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    pub parts: Vec<(Channel, String)>,
}

impl Transcript {
    pub fn write(&mut self, channel: Channel, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            self.parts.push((channel, text));
        }
    }
    fn line(&mut self, channel: Channel, text: impl Into<String>) {
        let mut text = text.into();
        text.push('\n');
        self.parts.push((channel, text));
    }
    pub fn text(&self, channel: Channel) -> String {
        self.parts
            .iter()
            .filter(|(c, _)| *c == channel)
            .map(|(_, text)| text.as_str())
            .collect()
    }
}

/// The installed command name when the executable was invoked as `am` or
/// `agent-memory`. There is no interpreter, so only argv0's file name counts.
pub fn installed_name(argv0: &OsStr) -> Option<&'static str> {
    let name = Path::new(argv0).file_name()?;
    NAMES
        .into_iter()
        .find(|candidate| OsStr::new(candidate) == name)
}

/// Python `str.isprintable` for one character: false for Cc, Cf, Cs, Co, Cn,
/// Zl, Zp and Zs other than space. Rust's debug-escape table is generated
/// from exactly those categories (Unicode version of the Rust toolchain).
pub fn printable(c: char) -> bool {
    if matches!(c, ' ' | '\\' | '\'' | '"') {
        return true;
    }
    let mut buffer = [0u8; 8];
    // Fixed 8-byte buffer: index 0 and the 7-byte tail always exist (UTF-8 ≤ 4 bytes).
    buffer[0] = b'a';
    let width = 1 + c.encode_utf8(&mut buffer[1..]).len();
    // `a` followed by one encoded char is valid UTF-8 within the 8-byte buffer.
    let Some(text) = buffer
        .get(..width)
        .and_then(|b| std::str::from_utf8(b).ok())
    else {
        return false;
    };
    let mut escaped = text.escape_debug();
    let _ = escaped.next();
    escaped.next() == Some(c) && escaped.next().is_none()
}

/// Python `repr(str)`.
pub fn repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut output = String::with_capacity(text.len() + 2);
    output.push(quote);
    for c in text.chars() {
        match c {
            '\\' => output.push_str("\\\\"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            c if c == quote => {
                output.push('\\');
                output.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                output.push_str(&format!("\\x{:02x}", c as u32));
            }
            c if (c as u32) < 0x7f || printable(c) => output.push(c),
            c if (c as u32) <= 0xff => output.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) <= 0xffff => output.push_str(&format!("\\u{:04x}", c as u32)),
            c => output.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    output.push(quote);
    output
}

/// Python `repr` of a JSON-decoded value.
fn repr_value(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(integer) => integer.as_str().to_owned(),
        Value::Float(float) if float.is_nan() => "nan".into(),
        Value::Float(float) if float.is_infinite() && *float < 0.0 => "-inf".into(),
        Value::Float(float) if float.is_infinite() => "inf".into(),
        Value::Float(_) => json::encode(value, json::DEFAULT).unwrap_or_default(),
        Value::String(text) => repr(text),
        Value::Array(values) => {
            let items: Vec<String> = values.iter().map(repr_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(pairs) => {
            let items: Vec<String> = pairs
                .iter()
                .map(|(key, value)| format!("{}: {}", repr(key), repr_value(value)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// Python `str(value)` of a JSON-decoded value.
pub fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => repr_value(other),
    }
}

/// Escape every non-printable character; `lines` keeps LF and TAB.
pub fn safe(value: &str, lines: bool) -> String {
    let mut output = String::with_capacity(value.len());
    for c in value.chars() {
        if printable(c) || (lines && matches!(c, '\n' | '\t')) {
            output.push(c);
        } else if (c as u32) <= 255 {
            output.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            output.push_str(&format!("\\u{:04x}", c as u32));
        }
    }
    output
}

fn safe_value(value: &Value) -> String {
    safe(&text(value), false)
}

pub fn help_text(prog: &str) -> String {
    format!(
        "Agent Memory — shared repository memory.

Usage:
  {prog} install [--check] [installation options]
  {prog} update [--check]
  {prog} uninstall [--check]
  {prog} --version
  {prog} version
  {prog} -V
  {prog} status [--repo PATH]
  {prog} doctor [--repo PATH] [--fix]
  {prog} recover
  {prog} migrate preview --repo PATH --to SCHEMA
  {prog} migrate apply --proposal FILE
  {prog} migrate recover --repo PATH --mode finish|rollback
  {prog} migrate restore --repo PATH --operation FINISHED_OPERATION
  {prog} help [COMMAND]

Lifecycle and diagnostics print readable results by default.
Use --json for complete machine results; --verbose adds diagnostic detail.
Memory commands retain structured proposals/results: init, search, context,
capture, checkpoint, publish, refresh and the other documented operations.
Native omp is explicitly opt-in: install --presets omp --omp-agent-dir ABSOLUTE_ROOT
--omp-peers-upgraded. It is excluded from all; source work uses omp-boundary.
Use {prog} COMMAND --help for its options. Uninstall preserves project memory.
"
    )
}

/// Remove the `--json` and `--verbose`/`-v` selectors before the literal `--` tail.
pub fn options(argv: Vec<String>) -> (Vec<String>, bool, bool) {
    let mut args = Vec::with_capacity(argv.len());
    let mut machine = false;
    let mut verbose = false;
    let mut values = argv.into_iter();
    while let Some(arg) = values.next() {
        if arg == "--" {
            args.push(arg);
            args.extend(values);
            return (args, machine, verbose);
        }
        if arg == "--json" {
            machine = true;
        } else if arg == "--verbose" || (arg == "-v" && !args.is_empty()) {
            verbose = true;
        } else {
            args.push(arg);
        }
    }
    (args, machine, verbose)
}

/// The installation receipt the bootstrap rewrite consults. Implemented by
/// `installation` (`locations()` + `read_receipt(state)`); an `Err` is the
/// baseline's caught OSError/ValueError/TypeError/KeyError.
pub trait InstalledReceipt: Send + Sync {
    fn receipt(&self) -> Result<Option<Value>>;
}

static RECEIPT: RwLock<Option<Arc<dyn InstalledReceipt>>> = RwLock::new(None);

pub fn register_installed_receipt(provider: Option<Arc<dyn InstalledReceipt>>) {
    *RECEIPT.write().unwrap_or_else(|e| e.into_inner()) = provider;
}

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object().and(value.get(key))
}

/// Python truthiness of an optional JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Int(integer)) => integer.as_str().trim_start_matches('-') != "0",
        Some(Value::Float(float)) => *float != 0.0,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(values)) => !values.is_empty(),
        Some(Value::Object(pairs)) => !pairs.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

/// The default repeated one-liner is a guarded update; explicit target
/// reconfiguration keeps the install contract. Source options may identify the
/// same channel, never silently switch it. `--bootstrap-binary PATH` is the
/// Rust bootstrap's own argument and never reconfigures a target.
pub fn bootstrap_command(args: Vec<String>) -> Result<Vec<String>> {
    if args.first().map(String::as_str) != Some("install") {
        return Ok(args);
    }
    let tail = args.get(1..).unwrap_or(&[]);
    let mut overrides: Vec<(&str, &str)> = Vec::new();
    let mut index = 0;
    while let Some(arg) = tail.get(index) {
        let next = tail.get(index + 1);
        if arg == "--check" || arg == "--dry-run" {
            index += 1;
        } else if arg == "--bootstrap-binary" && next.is_some() {
            index += 2;
        } else if (arg == "--remote" || arg == "--ref") && next.is_some() {
            let key = arg.trim_start_matches('-');
            overrides.push((key, next.map_or("", String::as_str)));
            index += 2;
        } else if let Some((key, value)) = arg
            .strip_prefix("--")
            .and_then(|rest| rest.split_once('='))
            .filter(|(key, _)| matches!(*key, "remote" | "ref"))
        {
            overrides.push((key, value));
            index += 1;
        } else {
            return Ok(args);
        }
    }
    let provider = RECEIPT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| {
            ContractError::new(
                "Installed receipt provider is not registered; bootstrap cannot select update",
            )
        })?;
    let old = match provider.receipt() {
        Ok(old) => old,
        Err(_) => return Ok(args),
    };
    let Some(source) = old
        .as_ref()
        .filter(|old| truthy(Some(old)))
        .and_then(|old| get(old, "source"))
    else {
        return Ok(args);
    };
    // Only a Rust-era receipt (v3) selects the guarded update. Python-era v1/v2
    // receipts and unknown schemas fall through to install, which adopts or refuses.
    let rust_era = old
        .as_ref()
        .and_then(|old| get(old, "schema"))
        .and_then(Value::as_str)
        == Some("agent-memory/install-receipt-v3");
    let channel = matches!(
        get(source, "kind").and_then(Value::as_str),
        Some("release" | "managed")
    );
    let same = overrides
        .iter()
        .all(|(key, value)| get(source, key).and_then(Value::as_str) == Some(*value));
    if rust_era && channel && same {
        let check = tail.iter().any(|v| v == "--check" || v == "--dry-run");
        let mut update = vec!["update".to_owned()];
        if check {
            update.push("--check".to_owned());
        }
        return Ok(update);
    }
    Ok(args)
}

fn object_or_empty(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| matches!(v, Value::Object(_)))
}

fn field<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value.and_then(|v| get(v, key))
}

fn field_text(value: Option<&Value>, key: &str, default: &str) -> String {
    match field(value, key) {
        Some(found) => safe_value(found),
        None => safe(default, false),
    }
}

fn prefix(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn items(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).unwrap_or(&[])
}

/// Human rendering of a lifecycle or diagnostic result (baseline `render`).
/// Malformed shapes the baseline would have crashed on render as absent.
pub fn render(data: &Value, command: &str, prog: &str, code: i32, out: &mut Transcript) {
    let data = Some(data);
    let receipt = object_or_empty(field(data, "receipt"));
    let source = [
        field(receipt, "source"),
        field(data, "executing_source"),
        field(data, "source"),
    ]
    .into_iter()
    .find(|candidate| truthy(*candidate))
    .flatten();
    let source = object_or_empty(source);
    let version = field_text(source, "version", "unknown");
    let revision_value = field(source, "revision").filter(|v| truthy(Some(v)));
    let revision = revision_value.map_or_else(|| "unversioned".to_owned(), text);
    let qualification = match field(source, "cleanliness").filter(|v| truthy(Some(v))) {
        Some(value) => text(value),
        None => match field(source, "dirty") {
            Some(Value::Bool(true)) => "dirty".into(),
            Some(Value::Bool(false)) => "clean".into(),
            _ => "unversioned".into(),
        },
    };
    let has_source = truthy(source);
    if code != 0 {
        let error = field(data, "error").filter(|v| truthy(Some(v)));
        let message = error.map_or_else(|| format!("{command} could not be verified"), text);
        out.line(Channel::Stderr, safe(&format!("{prog}: {message}"), false));
    }
    let preview = truthy(field(data, "check_complete"))
        || field(data, "schema").and_then(Value::as_str) == Some("agent-memory/install-plan-v1");
    if command == "version" && has_source {
        out.line(Channel::Stdout, format!("{prog} {version}"));
        out.line(
            Channel::Stdout,
            format!(
                "revision {} ({})",
                safe(&revision, false),
                safe(&qualification, false)
            ),
        );
        out.line(
            Channel::Stdout,
            format!("source {}", field_text(source, "path", "unavailable")),
        );
    } else if command == "update" && code == 0 {
        render_update(data, prog, &version, &revision, out);
    } else if (command == "install" || command == "uninstall") && code == 0 {
        render_installation(
            data,
            command,
            prog,
            preview,
            &version,
            &revision,
            &qualification,
            source,
            receipt,
            out,
        );
    } else if command == "status" || command == "doctor" {
        render_diagnostics(data, command, prog, code, out);
    } else if command == "migrate" && code == 0 {
        render_migration(data, prog, out);
    } else if command == "recover" && code == 0 {
        let message = match field(data, "status").and_then(Value::as_str) {
            Some("no-pending-transaction") => "No pending runtime transaction.",
            Some("complete") => "Verified committed runtime transaction.",
            Some("rolled-back") => "Rolled back interrupted runtime activation.",
            _ => "Runtime recovery verified.",
        };
        out.line(Channel::Stdout, message);
    }
    for (level, channel) in [("failures", Channel::Stderr), ("warnings", Channel::Stdout)] {
        let label = level.trim_end_matches('s');
        for item in items(field(data, level)) {
            let detail = get(item, "detail").unwrap_or(item);
            out.line(channel, format!("{label}: {}", safe_value(detail)));
            if let Some(repair) = get(item, "repair").filter(|v| truthy(Some(v))) {
                out.line(channel, format!("  {}", safe_value(repair)));
            }
        }
    }
    if code != 0 {
        if let Some(repair) = field(data, "repair").filter(|v| truthy(Some(v))) {
            out.line(Channel::Stderr, safe_value(repair));
        }
    }
    if code == 0 && truthy(field(data, "restart_clients")) && !preview {
        out.line(
            Channel::Stdout,
            "Restart running agent sessions to load the changed skill.",
        );
    }
}

fn render_update(
    data: Option<&Value>,
    prog: &str,
    version: &str,
    revision: &str,
    out: &mut Transcript,
) {
    let current = field(data, "current");
    let available = field(data, "available");
    let current_revision = field(current, "revision").map_or_else(String::new, text);
    let recovery = field(data, "cache_recovery");
    if truthy(field(data, "check_complete")) {
        if truthy(field(data, "update_available")) {
            out.line(
                Channel::Stdout,
                format!(
                    "{prog} update available: {} -> {} ({} -> {}).",
                    field_text(current, "version", "None"),
                    field_text(available, "version", "None"),
                    safe(&prefix(&current_revision, 12), false),
                    safe(
                        &prefix(
                            &field(available, "revision").map_or_else(String::new, text),
                            12
                        ),
                        false
                    ),
                ),
            );
        } else {
            let shown_version =
                field(current, "version").map_or_else(|| version.to_owned(), safe_value);
            let shown_revision =
                field(current, "revision").map_or_else(|| safe(revision, false), safe_value);
            out.line(
                Channel::Stdout,
                format!("{prog} {shown_version} is current at {shown_revision}."),
            );
        }
        if truthy(field(recovery, "planned")) {
            out.line(
                Channel::Stdout,
                "Cache-only runtime repair planned; the previous runtime will be preserved.",
            );
        }
        out.line(Channel::Stdout, "Preview only; no installation changes.");
    } else if truthy(field(data, "changed")) {
        if truthy(field(data, "update_available")) {
            out.line(
                Channel::Stdout,
                format!(
                    "Updated {prog} {} -> {version} ({} -> {}).",
                    field_text(current, "version", "None"),
                    safe(&prefix(&current_revision, 12), false),
                    safe(&prefix(revision, 12), false),
                ),
            );
        } else if truthy(recovery) {
            out.line(
                Channel::Stdout,
                format!(
                    "Repaired {prog} {version} cache-only runtime drift ({}).",
                    safe(&prefix(revision, 12), false)
                ),
            );
        } else {
            out.line(
                Channel::Stdout,
                format!(
                    "Reconciled {prog} {version} installation ({}).",
                    safe(&prefix(revision, 12), false)
                ),
            );
        }
        if truthy(field(recovery, "old_runtime_preserved")) {
            out.line(
                Channel::Stdout,
                "Previous runtime and cache evidence preserved in the installation backup.",
            );
        }
    } else {
        out.line(Channel::Stdout, format!("{prog} {version} is up to date."));
    }
}

#[allow(clippy::too_many_arguments)]
fn render_installation(
    data: Option<&Value>,
    command: &str,
    prog: &str,
    preview: bool,
    version: &str,
    revision: &str,
    qualification: &str,
    source: Option<&Value>,
    receipt: Option<&Value>,
    out: &mut Transcript,
) {
    if preview {
        if command == "install" {
            out.line(
                Channel::Stdout,
                "Preview only: this command does not install am. Rerun the same installer without --check or --dry-run to apply.",
            );
        }
        let title = if command == "install" {
            "Installation"
        } else {
            "Uninstall"
        };
        out.line(
            Channel::Stdout,
            format!("{title} preview; no changes made:"),
        );
        for target in items(field(data, "targets")) {
            let action = match (truthy(get(target, "changed")), command == "install") {
                (true, true) => "change",
                (true, false) => "remove",
                (false, _) => "keep",
            };
            out.line(
                Channel::Stdout,
                format!("  {action} {}", field_text(Some(target), "path", "None")),
            );
        }
    } else if command == "install" {
        let verb = if truthy(field(data, "changed")) {
            "Installed"
        } else {
            "Already installed:"
        };
        out.line(
            Channel::Stdout,
            format!(
                "{verb} {prog} {version} ({}).",
                safe(&prefix(revision, 12), false)
            ),
        );
        out.line(
            Channel::Stdout,
            format!(
                "source {} ({})",
                field_text(source, "path", "unavailable"),
                safe(qualification, false)
            ),
        );
        out.line(
            Channel::Stdout,
            format!(
                "artifact {}",
                field_text(source, "artifact_digest", "unavailable")
            ),
        );
        let receipt = receipt.or(field(data, "receipt"));
        for target in items(field(receipt, "targets")) {
            if get(target, "kind").and_then(Value::as_str) == Some("wrapper") {
                out.line(
                    Channel::Stdout,
                    format!("command {}", field_text(Some(target), "path", "None")),
                );
            }
        }
        let adapters: Vec<String> = items(field(receipt, "enabled_adapters"))
            .iter()
            .map(text)
            .collect();
        let adapters = adapters.join(", ");
        let clients = if adapters.is_empty() {
            "none".to_owned()
        } else {
            adapters
        };
        out.line(
            Channel::Stdout,
            format!("clients {}", safe(&clients, false)),
        );
        out.line(
            Channel::Stdout,
            "memory repository-local; not installer-owned",
        );
    } else if truthy(field(data, "preserved")) {
        out.line(
            Channel::Stdout,
            "Uninstall finished with preserved targets; removal is not complete.",
        );
    } else {
        out.line(
            Channel::Stdout,
            "Uninstalled owned runtime and client integration.",
        );
    }
    if command == "uninstall" {
        out.line(Channel::Stdout, "Project memory and user data preserved.");
    }
    for target in items(field(data, "preserved")) {
        out.line(
            Channel::Stdout,
            format!(
                "preserved {}: {}",
                field_text(Some(target), "path", "None"),
                field_text(Some(target), "reason", "user changes")
            ),
        );
    }
}

fn render_diagnostics(
    data: Option<&Value>,
    command: &str,
    prog: &str,
    code: i32,
    out: &mut Transcript,
) {
    let label = if code != 0 {
        "checks failed"
    } else if truthy(field(data, "warnings")) {
        "checks passed with warnings"
    } else {
        "checks passed"
    };
    let suffix = if command == "doctor" {
        format!(": {label}")
    } else {
        String::new()
    };
    out.line(Channel::Stdout, format!("{prog} {command}{suffix}"));
    out.line(
        Channel::Stdout,
        format!(
            "runtime {}",
            field_text(field(data, "installation"), "state", "unknown")
        ),
    );
    out.line(
        Channel::Stdout,
        format!(
            "repository {}",
            field_text(data, "repository", "unavailable")
        ),
    );
    let bank = field(data, "bank");
    let path = field(bank, "path").filter(|v| truthy(Some(v)));
    let located = path.map_or_else(String::new, |p| format!(" — {}", safe_value(p)));
    out.line(
        Channel::Stdout,
        format!(
            "memory {}{located}",
            field_text(bank, "state", "not inspected")
        ),
    );
    let setup = field(data, "setup");
    if truthy(setup) {
        out.line(
            Channel::Stdout,
            format!("setup {}", field_text(setup, "state", "None")),
        );
        out.line(
            Channel::Stdout,
            format!("next {}", field_text(setup, "next_step", "None")),
        );
    }
    for item in items(field(data, "repairs")) {
        out.line(
            Channel::Stdout,
            format!(
                "repair {}: {}",
                field_text(Some(item), "state", "unknown"),
                field_text(Some(item), "detail", "")
            ),
        );
        if let Some(repair) = get(item, "repair").filter(|v| truthy(Some(v))) {
            out.line(Channel::Stdout, format!("  {}", safe_value(repair)));
        }
    }
    if field(data, "operation").and_then(Value::as_str) == Some("doctor-fix")
        && !truthy(field(data, "repairs"))
    {
        out.line(
            Channel::Stdout,
            "No automatic runtime repair was performed.",
        );
    }
}

fn render_migration(data: Option<&Value>, prog: &str, out: &mut Transcript) {
    out.line(
        Channel::Stdout,
        format!("{prog} migrate: {}", field_text(data, "state", "inspected")),
    );
    if truthy(field(data, "source_schema")) {
        out.line(
            Channel::Stdout,
            format!(
                "format {} -> {}",
                field_text(data, "source_schema", "None"),
                field_text(data, "target_schema", "None")
            ),
        );
    }
    let receipt = field(data, "receipt");
    if field(data, "state").and_then(Value::as_str) == Some("already-current") {
        out.line(Channel::Stdout, "No format change or backup needed.");
    } else if truthy(field(data, "proposal")) {
        out.line(
            Channel::Stdout,
            "Preview only; save --json output privately and review before applying.",
        );
    } else if truthy(receipt) {
        let message = if truthy(field(data, "already_applied")) {
            "Bank bytes unchanged by this readback."
        } else {
            "Verified local outcome; Git publication remains separate."
        };
        out.line(Channel::Stdout, message);
    }
    if truthy(field(data, "available_modes")) {
        out.line(
            Channel::Stdout,
            "Choose finish or rollback and review its recovery preview.",
        );
    }
    if truthy(field(data, "backup_path")) {
        let planned = matches!(field(data, "backup_created"), Some(Value::Bool(false)));
        let label = if planned {
            "planned backup "
        } else {
            "recorded backup "
        };
        out.line(
            Channel::Stdout,
            format!("{label}{}", field_text(data, "backup_path", "None")),
        );
    }
    let unreadable_target = matches!(
        field(data, "target_directly_readable"),
        Some(Value::Bool(false))
    );
    let unreadable_outcome = truthy(receipt)
        && !truthy(field(data, "receipt_is_historical"))
        && matches!(
            field(receipt, "outcome_directly_readable"),
            Some(Value::Bool(false))
        );
    if unreadable_target || unreadable_outcome {
        out.line(
            Channel::Stdout,
            "The outcome format is not declared directly readable by this runtime; use a compatible runtime.",
        );
    }
}

/// Console entry (baseline `console.main`): help, version aliases, the
/// bootstrap rewrite, captured dispatch, then raw JSON or human rendering.
pub fn main(argv: Vec<String>, prog: &str, bootstrap: bool) -> (i32, Transcript) {
    let mut out = Transcript::default();
    let (mut args, machine, verbose) = options(argv);
    let help_only = args.is_empty()
        || (args.len() == 1
            && matches!(
                args.first().map(String::as_str),
                Some("help" | "--help" | "-h")
            ));
    if help_only {
        out.write(Channel::Stdout, help_text(prog));
        return (0, out);
    }
    if args.first().map(String::as_str) == Some("help") {
        args.remove(0);
        args.push("--help".into());
    }
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        args.remove(0);
        args.insert(0, "version".into());
    }
    if bootstrap {
        args = match bootstrap_command(args) {
            Ok(args) => args,
            Err(error) => {
                out.line(Channel::Stderr, safe(&format!("{prog}: {error}"), false));
                return (1, out);
            }
        };
    }
    let command = args.first().cloned().unwrap_or_default();
    let dispatched = crate::cli::dispatch(&args, prog);
    let result = dispatched.result;
    if command == "omp-boundary" && args.iter().skip(1).any(|arg| arg == "--stream") {
        // The private framed worker is a byte stream, never one printable response.
        out.write(Channel::Stderr, result.stderr);
        out.write(Channel::Stdout, result.stdout);
        return (result.exit_code, out);
    }
    if dispatched.exited {
        out.write(Channel::Stdout, safe(&result.stdout, true));
        out.write(Channel::Stderr, safe(&result.stderr, true));
        return (result.exit_code, out);
    }
    let code = result.exit_code;
    out.write(Channel::Stderr, safe(&result.stderr, true));
    if machine || !HUMAN.contains(&command.as_str()) {
        out.write(Channel::Stdout, result.stdout);
        return (code, out);
    }
    let data = match json::decode(&result.stdout, DecodePolicy::default()) {
        Ok(data) => data,
        Err(_) => {
            out.line(
                Channel::Stderr,
                format!("{prog}: cannot interpret the operation result; inspect --json output."),
            );
            return (if code != 0 { code } else { 1 }, out);
        }
    };
    render(&data, &command, prog, code, &mut out);
    if verbose {
        let channel = if code != 0 {
            Channel::Stderr
        } else {
            Channel::Stdout
        };
        match json::encode(&data, json::INDENT2_ASCII) {
            Ok(text) => out.line(channel, text),
            Err(error) => out.line(Channel::Stderr, safe(&format!("{prog}: {error}"), false)),
        }
    }
    (code, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_matches_python_categories() {
        for c in ['a', ' ', 'é', '\\', '\'', '"', '漢'] {
            assert!(printable(c), "{c:?}");
        }
        for c in [
            '\n', '\t', '\0', '\u{7f}', '\u{85}', '\u{a0}', '\u{200b}', '\u{2028}', '\u{e000}',
            '\u{1b}',
        ] {
            assert!(!printable(c), "{c:?}");
        }
    }

    #[test]
    fn repr_and_safe_follow_python() {
        assert_eq!(repr("bogus"), "'bogus'");
        assert_eq!(repr("it's"), "\"it's\"");
        assert_eq!(repr("a'\""), "'a\\'\"'");
        assert_eq!(repr("\u{1b}\n\u{a0}é"), "'\\x1b\\n\\xa0é'");
        assert_eq!(safe("\u{1b}[31m\n\t", false), "\\x1b[31m\\x0a\\x09");
        assert_eq!(safe("x\n\ty\u{2028}", true), "x\n\ty\\u2028");
        assert_eq!(
            text(
                &json::decode(
                    r#"{"a": [1, true, null, 1.5, "x"]}"#,
                    DecodePolicy::default()
                )
                .unwrap()
            ),
            "{'a': [1, True, None, 1.5, 'x']}"
        );
    }
}
