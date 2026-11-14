//! Bounded project policy checks; labels and UUIDs never authenticate a person.
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, sha256_hex, uuid_valid};
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_contract, bank_writes, repository_config, scope_binding, sharing_limits, tombstones,
    validation_memo,
};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::{Arc, RwLock};

pub const CLASSES: &[&str] = &[
    "assistant",
    "external",
    "tool",
    "user",
    "verified-repository",
];
pub const CATEGORIES: &[&str] = &["admission", "mutation", "retrieval", "promotion"];
pub const MIGRATION_ACTIONS: &[&str] = &[
    "migrate",
    "migration-restore",
    "migration-finish",
    "migration-rollback",
];
pub static ACTIONS: std::sync::LazyLock<BTreeSet<&'static str>> = std::sync::LazyLock::new(|| {
    [
        "policy-check",
        "initialize",
        "checkpoint",
        "recover-finish",
        "recover-rollback",
    ]
    .into_iter()
    .chain(bank_writes::OPERATIONS.iter().copied())
    .chain(MIGRATION_ACTIONS.iter().copied())
    .collect()
});
pub const LIMIT: usize = 16384;
pub const REVIEW_LIMIT: usize = 65536;
pub const AUDIT_LIMIT: usize = 65536;
pub const REQUEST_LIMIT: usize = 1024 * 1024;
pub const REQUEST: &[&str] = &[
    "project_id",
    "category",
    "source_class",
    "content_kind",
    "text",
    "record_ids",
    "automatic",
    "repository_evidence",
];
pub const REASONS: &[&str] = &[
    "private-key",
    "token",
    "credential",
    "authorization-header",
    "url-credential",
    "authority-override",
    "credential-disclosure",
    "identity-permission-claim",
    "raw-conversation",
    "terminal-deletion",
    "explicit-procedure-promotion-required",
    "operator-approval-required",
    "source-review-required",
];

// CPython's Unicode whitespace set (not Rust's evolving Unicode `\s`).
const SPACE_INNER: &str = r"\x09-\x0d\x1c-\x20\x{85}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}";
const REGEX_SIZE_LIMIT: usize = 4 * 1024 * 1024;
#[derive(Clone, Copy)]
enum Boundary {
    None,
    Start,
    Token,
}
struct Pattern {
    reason: &'static str,
    expression: regex::Regex,
    boundary: Boundary,
    leading_prefix: Option<regex::Regex>,
}
struct Patterns {
    secrets: Vec<Pattern>,
    suspicious: Vec<Pattern>,
}

/// Python IGNORECASE on these ASCII-only vocabulary literals is simple
/// one-scalar case matching, including the four documented non-ASCII letters.
/// It is deliberately not casefold/normalization of the source content.
fn insensitive(literal: &str) -> String {
    let mut expression = String::with_capacity(literal.len() * 4);
    for character in literal.chars() {
        if character.is_ascii_alphabetic() {
            expression.push('[');
            expression.push(character.to_ascii_lowercase());
            expression.push(character.to_ascii_uppercase());
            match character.to_ascii_lowercase() {
                'i' => expression.push_str("İı"),
                's' => expression.push('ſ'),
                'k' => expression.push('K'),
                _ => (),
            }
            expression.push(']');
        } else {
            expression.push(character);
        }
    }
    expression
}
fn alternatives(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| insensitive(word))
        .collect::<Vec<_>>()
        .join("|")
}
fn compile_expression(expression: &str) -> Result<regex::Regex> {
    regex::RegexBuilder::new(expression)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|_| err("Bundled security pattern cannot be compiled"))
}
fn compile(reason: &'static str, expression: &str, boundary: Boundary) -> Result<Pattern> {
    Ok(Pattern {
        reason,
        expression: compile_expression(expression)?,
        boundary,
        leading_prefix: None,
    })
}
fn patterns() -> Result<Patterns> {
    use std::fmt::Write;
    let ws = format!("[{SPACE_INNER}]");
    // The baseline table, not regex's broader Unicode word class (marks,
    // connector punctuation and joiners are not Python word characters).
    let mut word = String::from("_");
    for &(first, last) in crate::unicode::ALNUM {
        write!(word, r"\x{{{first:x}}}-\x{{{last:x}}}").map_err(io_error)?;
    }
    let nonword = format!("[^{word}]");
    // Baseline .{0,64} between two word boundaries: the gap is 1..64
    // scalars, begins/ends nonword, and may include newlines. The 1-scalar
    // alternative keeps the full 64 budget rather than charging twice.
    let gap = format!(r"(?:{nonword}|{nonword}(?s:.{{0,62}}){nonword})");
    let end = format!(r"(?:\z|{nonword})");
    let keys = format!(
        "{}|{}|{}|{}|{}[_-]?(?:{}|{})|{}[_-]?{}|{}[_-]?{}|{}[_-]?{}|{}",
        insensitive("password"),
        insensitive("passwd"),
        insensitive("token"),
        insensitive("secret"),
        insensitive("api"),
        insensitive("key"),
        insensitive("token"),
        insensitive("access"),
        insensitive("token"),
        insensitive("client"),
        insensitive("secret"),
        insensitive("secret"),
        insensitive("key"),
        insensitive("aws_secret_access_key")
    );
    let mut credential = compile(
        "credential",
        &format!(
            r#"\A["']?(?P<label>{keys})["']?{ws}*[:=]{ws}*["']?[^{SPACE_INNER}"'`,;}}\]]{{6,}}"#
        ),
        Boundary::None,
    )?;
    credential.leading_prefix = Some(compile_expression(&format!(r#"["']?(?P<label>{keys})"#))?);
    let mut token = compile(
        "token",
        r"\A(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}|AKIA[A-Z0-9]{16})",
        Boundary::Token,
    )?;
    token.leading_prefix = Some(compile_expression(r"(?:gh[pousr]_|github_pat_|sk-|AKIA)")?);
    let mut authorization = compile(
        "authorization-header",
        &format!(
            r"\A{}[ \t]*:[ \t]*(?:{})[ \t]+[A-Za-zİıſK0-9._~+/=-]{{6,}}",
            insensitive("Authorization"),
            alternatives(&["Bearer", "Basic"])
        ),
        Boundary::Start,
    )?;
    authorization.leading_prefix = Some(compile_expression(&insensitive("Authorization"))?);
    let secrets = vec![
        compile(
            "private-key",
            r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----(?s:.*?)(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|\z)",
            Boundary::None,
        )?,
        token,
        credential,
        authorization,
        compile(
            "url-credential",
            &format!(
                r"(?:{}|{}|{}|{}|{}|{})://[^/{SPACE_INNER}:@]+:[^@{SPACE_INNER}/]+@",
                insensitive("http"),
                insensitive("https"),
                format!("{}(?:{})?", insensitive("postgres"), insensitive("ql")),
                insensitive("mysql"),
                format!("{}(?:\\+{})?", insensitive("mongodb"), insensitive("srv")),
                format!(
                    "{}|{}|{}",
                    insensitive("redis"),
                    insensitive("amqp"),
                    insensitive("amqps")
                )
            ),
            Boundary::None,
        )?,
    ];
    let target = format!(
        "{}|{}{ws}+(?:{})",
        alternatives(&[
            "system",
            "developer",
            "previous",
            "instructions",
            "safety",
            "rules",
            "operator"
        ]),
        insensitive("current"),
        alternatives(&["request", "task"])
    );
    let suspicious = vec![
        compile(
            "authority-override",
            &format!(
                "(?:{}){gap}(?:{target}){end}",
                alternatives(&["ignore", "disregard", "override", "bypass"])
            ),
            Boundary::Start,
        )?,
        compile(
            "credential-disclosure",
            &format!(
                "(?:{}){gap}(?:{}){end}",
                alternatives(&["reveal", "exfiltrate", "upload", "send"]),
                alternatives(&["credentials", "secrets", "tokens", "passwords"])
            ),
            Boundary::Start,
        )?,
        compile(
            "identity-permission-claim",
            &format!(
                "(?:(?:{}){ws}*[:=]{ws}*(?:{})|{}(?:{})?(?:{})){end}",
                alternatives(&["authenticated", "administrator", "admin", "root"]),
                alternatives(&["true", "yes", "1"]),
                insensitive("I am "),
                insensitive("the "),
                alternatives(&["administrator", "authenticated operator"])
            ),
            Boundary::Start,
        )?,
        compile(
            "raw-conversation",
            &format!(
                r"(?m)^(?:{}){ws}*:{ws}*.*\n(?:(?s:.*)\n)?(?:{}){ws}*:",
                alternatives(&["user", "assistant", "system", "developer"]),
                alternatives(&["user", "assistant", "system", "developer"])
            ),
            Boundary::None,
        )?,
    ];
    Ok(Patterns {
        secrets,
        suspicious,
    })
}
static PATTERNS: std::sync::LazyLock<Result<Patterns>> = std::sync::LazyLock::new(patterns);

fn word_boundary(text: &str, position: usize) -> bool {
    let left = text
        .get(..position)
        .and_then(|s| s.chars().next_back())
        .is_some_and(crate::unicode::is_word);
    let right = text
        .get(position..)
        .and_then(|s| s.chars().next())
        .is_some_and(crate::unicode::is_word);
    left != right
}
fn match_end(pattern: &Pattern, text: &str, start: usize, mut end: usize) -> Option<usize> {
    if matches!(pattern.boundary, Boundary::None) {
        return Some(end);
    }
    if !word_boundary(text, start) {
        return None;
    }
    if matches!(pattern.boundary, Boundary::Start) {
        return Some(end);
    }
    let matched = text.get(start..end)?;
    if matched.starts_with("sk-") && !word_boundary(text, end) {
        // Python greedily backtracks this token's hyphen-containing body
        // until its terminal \b succeeds. Other token bodies are all word.
        for (offset, _) in matched.char_indices().rev() {
            if offset < 23 {
                break;
            } // sk- plus at least 20 body scalars.
            let candidate = start + offset;
            if word_boundary(text, candidate) {
                end = candidate;
                break;
            }
        }
    }
    word_boundary(text, end).then_some(end)
}
fn find_prefixed(
    pattern: &Pattern,
    prefix: &regex::Regex,
    text: &str,
    mut position: usize,
) -> Option<(usize, usize)> {
    while let Some(captures) = prefix.captures_at(text, position) {
        let candidate = captures.get(0)?;
        let start = candidate.start();
        let label_start = captures.name("label").map_or(start, |label| label.start());
        // Reject the original leading boundary before any unbounded body scan.
        // For credentials the bounded prefix may select a shorter key; the
        // anchored full match resolves alternatives and implies the end boundary.
        if word_boundary(text, label_start) {
            if let Some(matched) = pattern.expression.find(text.get(start..)?) {
                let full_start = start + matched.start();
                if let Some(end) = match_end(pattern, text, full_start, start + matched.end()) {
                    return Some((full_start, end));
                }
            }
        }
        // A rejected api-token prefix may contain a valid inner token label.
        // Never enumerate non-overlapping prefixes or skip the rejected span.
        position = start + text.get(start..)?.chars().next()?.len_utf8();
    }
    None
}
fn find_valid(pattern: &Pattern, text: &str, mut position: usize) -> Option<(usize, usize)> {
    if let Some(prefix) = &pattern.leading_prefix {
        return find_prefixed(pattern, prefix, text, position);
    }
    while let Some(matched) = pattern.expression.find_at(text, position) {
        if let Some(end) = match_end(pattern, text, matched.start(), matched.end()) {
            return Some((matched.start(), end));
        }
        // Rejected candidates must not skip an overlapping valid candidate.
        position = matched.start() + text.get(matched.start()..)?.chars().next()?.len_utf8();
    }
    None
}
fn redact(pattern: &Pattern, text: &str) -> Result<Option<String>> {
    let Some((start, end)) = find_valid(pattern, text, 0) else {
        return Ok(None);
    };
    let mut output = String::with_capacity(text.len());
    let mut previous = 0;
    let mut found = Some((start, end));
    while let Some((start, end)) = found {
        output.push_str(
            text.get(previous..start)
                .ok_or_else(|| err("Invalid security match boundary"))?,
        );
        output.push_str("[REDACTED:");
        output.push_str(pattern.reason);
        output.push(']');
        previous = end;
        found = find_valid(pattern, text, end);
    }
    output.push_str(
        text.get(previous..)
            .ok_or_else(|| err("Invalid security match boundary"))?,
    );
    Ok(Some(output))
}
fn scan_uncached(content: &Value, maximum: usize) -> Result<Value> {
    if ![LIMIT, REVIEW_LIMIT].contains(&maximum) {
        return Err(err(
            "Security scan needs the fixed source or review-page budget",
        ));
    }
    let content = content
        .as_str()
        .filter(|text| {
            !text.trim().is_empty()
                && text.len() <= maximum
                && !text
                    .chars()
                    .any(|c| c < ' ' && !['\n', '\r', '\t'].contains(&c))
        })
        .ok_or_else(|| {
            err("Security content must be useful bounded UTF8 text without control separators")
        })?;
    // Bound and validate content before the once-only regex initialization.
    let patterns = PATTERNS.as_ref().map_err(Clone::clone)?;
    let mut redacted = std::borrow::Cow::Borrowed(content);
    let mut reasons = BTreeSet::new();
    for pattern in &patterns.secrets {
        if let Some(replacement) = redact(pattern, &redacted)? {
            redacted = std::borrow::Cow::Owned(replacement);
            reasons.insert(pattern.reason.to_owned());
        }
    }
    let redaction_required = !reasons.is_empty();
    let mut quarantine_required = false;
    for pattern in &patterns.suspicious {
        if find_valid(pattern, &redacted, 0).is_some() {
            reasons.insert(pattern.reason.to_owned());
            quarantine_required = true;
        }
    }
    Ok(object([
        ("redacted_text", Value::String(redacted.into_owned())),
        ("redaction_required", Value::Bool(redaction_required)),
        (
            "reasons",
            Value::Array(reasons.into_iter().map(Value::String).collect()),
        ),
        ("quarantine_required", Value::Bool(quarantine_required)),
        ("content_is_untrusted_data", Value::Bool(true)),
        ("exhaustive_detection_claimed", Value::Bool(false)),
    ]))
}

/// Migration owns both the actual existing locator and its location checks.
pub trait MigrationAudit: Send + Sync {
    fn validate_location(&self, root: &Path, bank: &Path, operation_id: &str) -> Result<()>;
}
static MIGRATION: RwLock<Option<Arc<dyn MigrationAudit>>> = RwLock::new(None);
pub fn register_migration_audit(provider: Option<Arc<dyn MigrationAudit>>) {
    *MIGRATION.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_error(error: impl std::fmt::Display) -> ContractError {
    err(&error.to_string())
}
fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|rows| {
        rows.len() == fields.len() && rows.iter().all(|(key, _)| fields.contains(&key.as_str()))
    })
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid security request metadata"))
}
fn sorted(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().map(string).collect())
}
fn action_valid(action: &str) -> bool {
    ACTIONS.contains(action)
}

pub fn scan(content: &Value, maximum: usize) -> Result<Value> {
    let key = validation_memo::current()
        .and_then(|_| validation_memo::text_key(content, &Value::Int(maximum.into())));
    match key {
        Some(key) => validation_memo::checked(
            "security-scan",
            &key.memo_key(),
            || scan_uncached(content, maximum),
            None,
        ),
        None => scan_uncached(content, maximum),
    }
}

pub fn audit(
    repo: &Path,
    operation_id: &str,
    category: &str,
    phase: &str,
    context: &Value,
    source_classes: &[&str],
    reasons: &[String],
    record_ids: &[String],
    action: &str,
    bank_location: Option<&Path>,
) -> Result<Value> {
    if !uuid_valid(&string(operation_id))
        || !CATEGORIES.contains(&category)
        || !action_valid(action)
        || !["checked", "started", "completed", "refused"].contains(&phase)
        || source_classes.is_empty()
        || source_classes.iter().any(|s| !CLASSES.contains(s))
        || record_ids.iter().any(|id| !uuid_valid(&string(id)))
        || reasons
            .iter()
            .any(|reason| !REASONS.contains(&reason.as_str()))
    {
        return Err(err("Invalid security audit metadata"));
    }
    if context.as_object().is_none()
        || !uuid_valid(get(context, "developer_id"))
        || !uuid_valid(get(context, "project_id"))
    {
        return Err(err(
            "Audit requires checked attribution and project identifiers",
        ));
    }
    let root = fs::canonical_path(repo, false)?;
    let bank = if let Some(bank) = bank_location {
        if !MIGRATION_ACTIONS.contains(&action) {
            return Err(err(
                "Migration audit must use the actual existing bank locator",
            ));
        }
        let provider = MIGRATION
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| err("Migration audit provider is not registered"))?;
        provider.validate_location(&root, bank, operation_id)?;
        bank.to_path_buf()
    } else {
        repository_config::bank_path(&root)?
    };
    let (operations, _) = bank_writes::state_paths(&root, &bank, operation_id)?;
    let directory = operations
        .parent()
        .ok_or_else(|| err("Security audit location must remain a canonical private directory"))?
        .join("security-audit");
    fs::private_mkdir(&directory)
        .map_err(|_| err("Security audit location must remain a canonical private directory"))?;
    let chain = DirectoryChain::open(&directory).map_err(io_error)?;
    let name = OsString::from(format!(
        "{operation_id}.{action}.{phase}.{}.json",
        text(context, "developer_id")?
    ));
    let path = directory.join(&name);
    let event = object([
        ("schema", string("agent-memory/security-audit-v1")),
        ("operation_id", string(operation_id)),
        ("project_id", get(context, "project_id").clone()),
        ("developer_id", get(context, "developer_id").clone()),
        ("category", string(category)),
        ("phase", string(phase)),
        ("action", string(action)),
        (
            "source_classes",
            strings(&sorted(source_classes.iter().map(|s| (*s).to_owned()))),
        ),
        ("reasons", strings(&sorted(reasons.iter().cloned()))),
        ("record_ids", strings(&sorted(record_ids.iter().cloned()))),
        ("identity_authenticated", Value::Bool(false)),
    ]);
    let payload = encoded(&event)?;
    if payload.len() > AUDIT_LIMIT {
        return Err(err("Invalid security audit metadata"));
    }
    publish_audit(&chain, &name, &payload)?;
    Ok(object([
        (
            "path",
            string(crate::host::text(path.as_os_str(), "security audit path")?),
        ),
        ("verified", Value::Bool(true)),
        ("phase", string(phase)),
    ]))
}

fn read_event(chain: &DirectoryChain, name: &OsStr) -> Result<Vec<u8>> {
    let directory = chain.directory();
    let mut file = directory
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(|_| err("Security audit must not follow a symlink"))?;
    let before = file.stat().map_err(io_error)?;
    if !before.is_regular() {
        return Err(err("Security audit readback failed; do not proceed"));
    }
    let raw = file.read_bounded(AUDIT_LIMIT).map_err(io_error)?;
    if !directory
        .stat_at(name)
        .map_err(io_error)?
        .same_binding(before)
    {
        return Err(err("Security audit readback failed; do not proceed"));
    }
    chain.recheck().map_err(io_error)?;
    Ok(raw)
}
struct Temporary<'a> {
    directory: &'a fs::Descriptor,
    name: OsString,
    present: bool,
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if self.present {
            let _ = self.directory.unlink_at(&self.name);
        }
    }
}
fn publish_audit(chain: &DirectoryChain, name: &OsStr, payload: &[u8]) -> Result<()> {
    let directory = chain.directory();
    if directory.stat_at(name).is_ok_and(|info| info.is_symlink()) {
        return Err(err("Security audit must not follow a symlink"));
    }
    let temporary = OsString::from(format!(".security-audit-{}", crate::host::uuid4()?));
    let mut file = directory
        .create_exclusive(&temporary, libc::O_WRONLY, 0o600)
        .map_err(io_error)?;
    let mut cleanup = Temporary {
        directory,
        name: temporary,
        present: true,
    };
    file.write_all(payload).map_err(io_error)?;
    file.fsync().map_err(io_error)?;
    chain.recheck().map_err(io_error)?;
    match fs::rename_at(
        directory,
        &cleanup.name,
        directory,
        name,
        fs::RenameMode::NoReplace,
    ) {
        Ok(()) => cleanup.present = false,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_event(chain, name).map_or(true, |bytes| bytes != payload) {
                return Err(err(
                    "Security operation ID has different metadata; preserve the audit",
                ));
            }
        }
        Err(error) => return Err(io_error(error)),
    }
    directory.fsync_directory().map_err(io_error)?;
    if cleanup.present {
        directory.unlink_at(&cleanup.name).map_err(io_error)?;
        cleanup.present = false;
    }
    if read_event(chain, name)? != payload {
        return Err(err("Security audit readback failed; do not proceed"));
    }
    Ok(())
}

pub fn content_reasons(contents: &Value) -> Result<Vec<String>> {
    let rows = contents
        .as_object()
        .filter(|rows| rows.len() <= 64)
        .ok_or_else(|| err("Governed mutation exceeds its bounded memory content batch"))?;
    let mut bytes = 0usize;
    for (_, body) in rows {
        let body = body
            .as_str()
            .ok_or_else(|| err("Governed mutation exceeds its bounded memory content batch"))?;
        bytes = bytes.saturating_add(body.len());
        if bytes > REVIEW_LIMIT {
            return Err(err(
                "Governed mutation exceeds its bounded memory content batch",
            ));
        }
    }
    collect_reasons(rows.iter().map(|(_, body)| scan(body, LIMIT)))
}
fn collect_reasons(scans: impl IntoIterator<Item = Result<Value>>) -> Result<Vec<String>> {
    let mut reasons = BTreeSet::new();
    for result in scans {
        let result = result?;
        for reason in get(&result, "reasons")
            .as_array()
            .ok_or_else(|| err("Invalid security scan result"))?
        {
            reasons.insert(
                reason
                    .as_str()
                    .ok_or_else(|| err("Invalid security scan result"))?
                    .to_owned(),
            );
        }
    }
    Ok(reasons.into_iter().collect())
}
fn safe(reasons: &[String]) -> Result<()> {
    if !reasons.is_empty() {
        return Err(err(&format!(
            "Security policy quarantined proposed memory; redact or review before writing: {}",
            reasons.join(", ")
        )));
    }
    Ok(())
}
pub fn ensure_safe(contents: &Value) -> Result<()> {
    safe(&content_reasons(contents)?)
}
pub fn complete_refresh_reasons(contents: &Value) -> Result<Vec<String>> {
    sharing_limits::bank_size(contents)?;
    let rows = contents
        .as_object()
        .ok_or_else(|| err("Invalid complete refresh content"))?;
    for (path, _) in rows {
        if path != "MEMORY.md" {
            bank_contract::path_value(&string(path))?;
        }
    }
    collect_reasons(rows.iter().map(|(path, body)| {
        let body = body
            .as_str()
            .ok_or_else(|| err("Invalid complete refresh content"))?;
        scan(&string(format!("{path}\n{body}")), REVIEW_LIMIT)
    }))
}
pub fn guarded_start(
    repo: &Path,
    operation_id: &str,
    context: &Value,
    contents: &Value,
    source_classes: &[&str],
    action: &str,
    complete_refresh: bool,
) -> Result<Value> {
    if complete_refresh && action != "refresh" {
        return Err(err(
            "Complete refresh scanning cannot widen a capture or another mutation",
        ));
    }
    let reasons = if complete_refresh {
        complete_refresh_reasons(contents)?
    } else {
        content_reasons(contents)?
    };
    let event = audit(
        repo,
        operation_id,
        "mutation",
        if reasons.is_empty() {
            "started"
        } else {
            "refused"
        },
        context,
        source_classes,
        &reasons,
        &[],
        action,
        None,
    )?;
    safe(&reasons)?;
    Ok(event)
}

pub fn repository_source(root: &Path, evidence: &Value) -> Result<()> {
    let valid_hash = get(evidence, "sha256").as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    });
    if !exact(evidence, &["path", "sha256"])
        || get(evidence, "path").as_str().is_none()
        || !valid_hash
    {
        return Err(err(
            "Verified repository evidence needs an explicit path and SHA256 observation",
        ));
    }
    let path = text(evidence, "path")?;
    let relative = Path::new(path);
    if path.is_empty()
        || relative.is_absolute()
        || path
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.'))
        || relative.starts_with(repository_config::relative(root)?)
        || path.chars().any(|c| c < ' ')
    {
        return Err(err(
            "Repository evidence must be canonical local source, not memory or private metadata",
        ));
    }
    source_symlinks(root, path)?;
    let args = [
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
    ]
    .map(OsString::from);
    let listed = crate::identity_git::git(root, &args, true, false).map_err(io_error)?;
    use std::os::unix::ffi::OsStrExt;
    if !listed.as_ref().is_some_and(|names| {
        names
            .as_os_str()
            .as_bytes()
            .split(|b| *b == 0)
            .any(|name| name == path.as_bytes())
    }) {
        return Err(err(
            "Repository evidence is ignored or not in the explicit Git source inventory",
        ));
    }
    let raw = fs::read_source(root, relative, REVIEW_LIMIT).map_err(|error| match &error {
        fs::ReadError::Io(io)
            if io.raw_os_error() == Some(libc::ELOOP)
                || io.raw_os_error() == Some(libc::ENOTDIR) =>
        {
            err("Repository evidence must not follow source symlinks")
        }
        _ => err("Repository evidence changed or exceeds the bounded reader"),
    })?;
    if get(evidence, "sha256").as_str() != Some(sha256_hex(&raw.bytes).as_str()) {
        return Err(err(
            "Repository evidence changed or exceeds the bounded reader",
        ));
    }
    Ok(())
}
fn source_symlinks(root: &Path, path: &str) -> Result<()> {
    let mut chain = DirectoryChain::open(root).map_err(io_error)?;
    let mut prefix = root.to_path_buf();
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        let info = match chain.directory().stat_at(OsStr::new(part)) {
            Ok(info) => info,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(io_error(error)),
        };
        if info.is_symlink() {
            return Err(err("Repository evidence must not follow source symlinks"));
        }
        chain.recheck().map_err(io_error)?;
        if parts.peek().is_none() || !info.is_directory() {
            break;
        }
        prefix.push(part);
        chain = DirectoryChain::open(&prefix)
            .map_err(|_| err("Repository evidence must not follow source symlinks"))?;
    }
    Ok(())
}

pub fn approval_value(approval: &Value, request: &Value) -> Result<()> {
    let fields = [
        "schema",
        "project_id",
        "content_sha256",
        "source_version",
        "ref",
        "decision",
    ];
    let version = get(approval, "source_version")
        .as_int()
        .is_some_and(|s| !s.as_str().starts_with('-') && s.as_str().bytes().any(|b| b != b'0'));
    let useful_ref = get(approval, "ref")
        .as_str()
        .is_some_and(|s| !s.trim().is_empty() && !s.chars().any(|c| c < ' '));
    if !exact(approval, &fields)
        || get(approval, "schema").as_str() != Some("agent-memory/operator-procedure-approval-v1")
        || get(approval, "project_id") != get(request, "project_id")
        || get(approval, "content_sha256").as_str()
            != Some(sha256_hex(text(request, "text")?.as_bytes()).as_str())
        || !version
        || get(approval, "decision").as_str() != Some("approved")
        || !useful_ref
    {
        return Err(err(
            "Procedure eligibility requires separate explicit current-operator approval bound to exact content, source version and project",
        ));
    }
    Ok(())
}

pub fn evaluate(
    repo: &Path,
    request: &Value,
    approval: Option<&Value>,
    record_audit: bool,
) -> Result<Value> {
    let approval = approval.filter(|value| *value != &Value::Null);
    let ids = get(request, "record_ids").as_array();
    if !exact(request, REQUEST)
        || !uuid_valid(get(request, "project_id"))
        || !get(request, "category")
            .as_str()
            .is_some_and(|s| CATEGORIES.contains(&s))
        || !get(request, "source_class")
            .as_str()
            .is_some_and(|s| CLASSES.contains(&s))
        || !get(request, "content_kind")
            .as_str()
            .is_some_and(|s| ["summary", "procedure", "conversation"].contains(&s))
        || get(request, "automatic").as_bool().is_none()
        || !ids.is_some_and(|ids| {
            ids.len() <= 64
                && ids.iter().all(uuid_valid)
                && ids
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == ids.len()
        })
    {
        return Err(err(
            "Security request needs explicit project, source class, operation and bounded content/record identities",
        ));
    }
    let root = fs::canonical_path(repo, false)?;
    let category = text(request, "category")?;
    let kind = text(request, "content_kind")?;
    let source = text(request, "source_class")?;
    if category == "promotion" && kind != "procedure" {
        return Err(err(
            "Procedure promotion requires an explicitly typed procedure request",
        ));
    }
    let context = scope_binding::require(
        &root,
        Some(text(request, "project_id")?),
        scope_binding::RequireOptions::default(),
    )?;
    let (project_id, observed, deleted) = tombstones::history(&root)?;
    let evidence = get(request, "repository_evidence");
    if source == "verified-repository" {
        repository_source(&root, evidence)?;
    } else if evidence != &Value::Null {
        return Err(err(
            "Keep repository evidence verification separate from user/assistant/tool/external classification",
        ));
    }
    let mut checked = scan(get(request, "text"), LIMIT)?;
    let mut reasons = get(&checked, "reasons")
        .as_array()
        .ok_or_else(|| err("Invalid security scan result"))?
        .iter()
        .map(|reason| {
            reason
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| err("Invalid security scan result"))
        })
        .collect::<Result<Vec<_>>>()?;
    let record_ids = ids
        .ok_or_else(|| err("Invalid security record identities"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| err("Invalid security record identities"))
        })
        .collect::<Result<Vec<_>>>()?;
    if record_ids.iter().any(|id| deleted.contains_key(id)) {
        reasons.push("terminal-deletion".into());
    }
    if kind == "conversation" {
        reasons.push("raw-conversation".into());
    }
    let procedure = kind == "procedure" || category == "promotion";
    let automatic = get(request, "automatic").as_bool() == Some(true);
    if procedure {
        if automatic || category != "promotion" {
            reasons.push("explicit-procedure-promotion-required".into());
        } else if let Some(approval) = approval {
            approval_value(approval, request)?;
        } else {
            reasons.push("operator-approval-required".into());
        }
    } else if approval.is_some() {
        return Err(err(
            "Approval is for a separate reviewed procedure promotion, never routine capture permission",
        ));
    }
    let weak = ["assistant", "tool", "external"].contains(&source);
    let review = weak && automatic && category == "admission";
    let decision = if !reasons.is_empty() {
        "quarantine"
    } else if review {
        "review-required"
    } else {
        "eligible"
    };
    if review {
        reasons.push("source-review-required".into());
    }
    scope_binding::require(
        &root,
        project_id.as_str(),
        scope_binding::RequireOptions {
            expected: Some(&context),
            ..Default::default()
        },
    )?;
    if tombstones::history(&root)?.1 != observed {
        return Err(err(
            "Canonical bank changed during security evaluation; reread before using the result",
        ));
    }
    if source == "verified-repository" {
        repository_source(&root, evidence)?;
    }
    let event = if record_audit {
        audit(
            &root,
            &crate::host::uuid4()?,
            category,
            "checked",
            &context,
            &[source],
            &reasons,
            &record_ids,
            "policy-check",
            None,
        )?
    } else {
        Value::Null
    };
    if get(&checked, "quarantine_required").as_bool() == Some(true)
        || kind == "conversation"
        || reasons.iter().any(|r| r == "terminal-deletion")
    {
        if let Some(value) = checked.get_mut("redacted_text") {
            *value = Value::Null;
        }
    }
    let mut result = object([
        ("schema", string("agent-memory/security-result-v1")),
        ("decision", string(decision)),
    ]);
    bank_contract::merge(&mut result, &checked)?;
    bank_contract::merge(
        &mut result,
        &object([
            ("repository_evidence", evidence.clone()),
            ("reasons", strings(&sorted(reasons))),
            ("scope_context", context),
            ("source_hashes", observed),
            ("source_class", string(source)),
            ("source_assertions_verified", Value::Bool(false)),
            ("review_required_for_admission", Value::Bool(weak)),
            (
                "procedure_eligibility",
                Value::Bool(procedure && decision == "eligible"),
            ),
            ("procedural_guidance", Value::Bool(false)),
            ("identity_authenticated", Value::Bool(false)),
            ("publication_verified", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
            ("audit", event),
            ("record_ids", get(request, "record_ids").clone()),
            ("content_is_untrusted_data", Value::Bool(true)),
        ]),
    )?;
    Ok(result)
}

/// One production implementation of the three pre-existing security seams.
pub struct Security;
impl repository_config::SecurityScan for Security {
    fn has_reasons(&self, body: &str) -> Result<bool> {
        Ok(!collect_reasons([scan(&string(body), LIMIT)])?.is_empty())
    }
    fn scan(&self, body: &str) -> Result<Value> {
        scan(&string(body), LIMIT)
    }
}
impl crate::sharing_hold_mapping::SecurityScan for Security {
    fn has_reasons(&self, body: &Value) -> Result<bool> {
        Ok(!collect_reasons([scan(body, LIMIT)])?.is_empty())
    }
}
impl bank_writes::Security for Security {
    fn guarded_start(
        &self,
        repo: &Path,
        id: &str,
        context: &Value,
        files: &Value,
        sources: &[&str],
        action: &str,
    ) -> Result<()> {
        guarded_start(repo, id, context, files, sources, action, false).map(|_| ())
    }
    fn audit(
        &self,
        repo: &Path,
        id: &str,
        category: &str,
        phase: &str,
        context: &Value,
        sources: &[&str],
        action: &str,
    ) -> Result<Value> {
        audit(
            repo,
            id,
            category,
            phase,
            context,
            sources,
            &[],
            &[],
            action,
            None,
        )
    }
    fn guarded_start_transaction(
        &self,
        repo: &Path,
        id: &str,
        context: &Value,
        files: &Value,
        sources: &[&str],
        action: &str,
        complete: bool,
    ) -> Result<()> {
        guarded_start(repo, id, context, files, sources, action, complete).map(|_| ())
    }
}

pub fn command_result(outcome: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let mut value = object([
                ("schema", string("agent-memory/security-result-v1")),
                ("error", string(error.to_string())),
                ("verified", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
            ]);
            // Invariant: both values are objects constructed by these helpers.
            bank_contract::merge(&mut value, &crate::bank_capacity::details(&error))
                .expect("capacity fields");
            (value, 1, json::DEFAULT)
        }
    };
    match json::encode(&value, style) {
        Ok(stdout) => crate::lifecycle::CommandResult {
            value: Some(value),
            exit_code,
            stdout: stdout + "\n",
            stderr: String::new(),
        },
        Err(error) => command_result(Err(error)),
    }
}
