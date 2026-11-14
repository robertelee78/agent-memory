//! Checkpoint active context through one identity-bound observed replacement.
use crate::codec::json::{self, Value};
use crate::error::{ContractError, Result};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::{
    bank_contract as bank, bank_transactions as transactions, bank_writes as writes, handoff,
    repository_config, scope_binding, scopes,
};
use std::collections::BTreeSet;
use std::path::Path;

pub const FIELDS: &[(&str, &str)] = &[
    ("focus", "Current focus"),
    ("recent_changes", "Recent changes"),
    ("open_questions", "Open questions"),
    ("next_step", "Next step"),
];
pub const INPUT: &[&str] = &[
    "focus",
    "recent_changes",
    "open_questions",
    "next_step",
    "conditions",
    "evidence",
    "harness",
    "session",
    "observed_active_sha256",
];
pub const EXTRA: &[&str] = &[
    "Checkpoint conditions",
    "Checkpoint evidence",
    "Checkpoint provenance",
    handoff::SECTION,
];
pub const LIMIT: usize = 16384;
pub const MAX_ACTIVE: usize = 65536;
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn str_value<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    get(value, key)
        .as_str()
        .ok_or_else(|| err("Invalid checkpoint field type"))
}
fn put(value: &mut Value, key: &str, item: Value) -> Result<()> {
    let Value::Object(fields) = value else {
        return Err(err("Invalid checkpoint object"));
    };
    if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
        *old = item;
    } else {
        fields.push((key.to_owned(), item));
    }
    Ok(())
}
pub fn result(operation: &str, fields: Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/checkpoint-result-v1")),
        ("operation", string(operation)),
        ("publication_verified", Value::Bool(false)),
        ("identity_authenticated", Value::Bool(false)),
        ("source_assertions_verified", Value::Bool(false)),
    ]);
    for (key, item) in fields
        .as_object()
        .ok_or_else(|| err("Invalid checkpoint result"))?
    {
        put(&mut value, key, item.clone())?;
    }
    Ok(value)
}
fn reserved(title: &str) -> bool {
    FIELDS.iter().any(|(_, label)| *label == title) || EXTRA.contains(&title)
}
/// Returns UTF-8 byte ranges; an empty title represents a non-H2 delimiter.
pub fn headings(body: &str) -> Result<Vec<(usize, usize, String)>> {
    if body.chars().any(|c| {
        (c < ' ' && !matches!(c, '\n' | '\r' | '\t'))
            || matches!(c, '\u{7f}' | '\u{85}' | '\u{2028}' | '\u{2029}')
    }) {
        return Err(err(
            "Unsupported control or Unicode line separator; inspect context before checkpointing",
        ));
    }
    let mut output: Vec<(usize, usize, String)> = Vec::new();
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut list_seen = false;
    for line in body.split_inclusive(['\n', '\r']) {
        let raw = line.trim_end_matches(['\r', '\n']);
        let indent = raw.bytes().take_while(|b| *b == b' ').count();
        let unindented = raw.get(indent..).unwrap_or("");
        let marker = unindented.as_bytes().first().copied();
        let count = marker.map_or(0, |m| unindented.bytes().take_while(|b| *b == m).count());
        if indent <= 3 && matches!(marker, Some(b'`' | b'~')) && count >= 3 {
            let rest = unindented.get(count..).unwrap_or("");
            if let Some((m, n)) = fence {
                if marker == Some(m) && count >= n && rest.trim().is_empty() {
                    fence = None;
                }
            } else if marker != Some(b'`') || !rest.contains('`') {
                fence = marker.map(|m| (m, count));
            }
        } else if fence.is_none() && !indented_code(raw) {
            if indent <= 3 {
                let boundary = unindented.trim_end_matches([' ', '\t']);
                if !boundary.is_empty()
                    && (boundary.bytes().all(|b| b == b'-') || boundary.bytes().all(|b| b == b'='))
                {
                    return Err(err(
                        "Setext/underline boundary requires explicit inspection before checkpointing; preserve the original context",
                    ));
                }
            }
            if raw.match_indices('<').any(|(at, _)| {
                let rest = raw.get(at + 1..).unwrap_or("");
                rest.starts_with("!--")
                    || rest.starts_with(['?', '!'])
                    || rest
                        .strip_prefix('/')
                        .unwrap_or(rest)
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_alphabetic)
            }) {
                return Err(err(
                    "Raw HTML requires explicit inspection or a complete fenced example before checkpointing",
                ));
            }
            let hashes = unindented.bytes().take_while(|b| *b == b'#').count();
            let rest = unindented.get(hashes..).unwrap_or("");
            if indent <= 3
                && (1..=6).contains(&hashes)
                && (rest.is_empty() || rest.starts_with([' ', '\t']))
            {
                if indent > 0 && list_seen {
                    return Err(err(
                        "Ambiguous nested-list heading requires explicit inspection before checkpointing",
                    ));
                }
                let mut title = rest.trim_matches([' ', '\t']);
                let stripped = title.trim_end_matches('#');
                if stripped.len() != title.len()
                    && (stripped.is_empty() || stripped.ends_with([' ', '\t']))
                {
                    title = stripped.trim_end_matches([' ', '\t']);
                }
                if hashes != 2 && reserved(title) {
                    return Err(err(
                        "Reserved checkpoint section names require level-two headings; clarify the existing structure",
                    ));
                }
                if let Some(previous) = output.last_mut() {
                    previous.1 = offset;
                }
                output.push((
                    offset,
                    body.len(),
                    if hashes == 2 {
                        title.to_owned()
                    } else {
                        String::new()
                    },
                ));
                list_seen = false;
            } else if indent <= 3 && list_marker(unindented) {
                list_seen = true;
            }
        }
        offset += line.len();
    }
    if fence.is_some() {
        return Err(err(
            "Active context/input has an unterminated Markdown fence; inspect it before checkpointing",
        ));
    }
    Ok(output)
}
fn indented_code(raw: &str) -> bool {
    let mut column = 0;
    for byte in raw.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += 4 - column % 4,
            _ => break,
        }
        if column >= 4 {
            return true;
        }
    }
    false
}
fn list_marker(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.first().is_some_and(|b| b"-+*".contains(b)) {
        return bytes.get(1).is_some_and(|b| matches!(b, b' ' | b'\t'));
    }
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    (1..=9).contains(&digits)
        && bytes.get(digits).is_some_and(|b| matches!(b, b'.' | b')'))
        && bytes
            .get(digits + 1)
            .is_some_and(|b| matches!(b, b' ' | b'\t'))
}
pub fn text(value: &Value, limit: usize) -> Result<&str> {
    text_str(
        value.as_str().ok_or_else(|| {
            err("Checkpoint fields require bounded useful Markdown without new sections")
        })?,
        limit,
    )
}
fn text_str(value: &str, limit: usize) -> Result<&str> {
    if value.trim().is_empty()
        || value.len() > limit
        || value.chars().any(|c| c < ' ' && !matches!(c, '\n' | '\t'))
    {
        return Err(err(
            "Checkpoint fields require bounded useful Markdown without new sections",
        ));
    }
    let normalized = value.trim();
    if !headings(normalized)?.is_empty() {
        return Err(err(
            "Checkpoint fields must not create unfenced headings after normalization",
        ));
    }
    Ok(normalized)
}
pub fn list_text(items: &[String]) -> Result<String> {
    let mut output = String::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str("- ");
        for (line_number, line) in text_str(item, 1024)?.split('\n').enumerate() {
            if line_number > 0 {
                output.push_str("\n    ");
            }
            output.push_str(line);
        }
    }
    Ok(output)
}
fn strings(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| err("Checkpoint lists are bounded to sixteen concise items"))?
        .iter()
        .map(|v| {
            v.as_str().map(str::to_owned).ok_or_else(|| {
                err("Checkpoint fields require bounded useful Markdown without new sections")
            })
        })
        .collect()
}
pub fn input_value(value: &Value) -> Result<&Value> {
    if crate::records::fields(value, INPUT, INPUT).is_err() {
        return Err(err(
            "Checkpoint input requires four reviewed fields, conditions, evidence, harness/session and observed active hash",
        ));
    }
    for field in ["focus", "next_step", "harness", "session"] {
        text(get(value, field), if field == "focus" { 2048 } else { 512 })?;
    }
    for field in ["recent_changes", "open_questions", "evidence"] {
        let items = get(value, field)
            .as_array()
            .filter(|v| v.len() <= 16)
            .ok_or_else(|| err("Checkpoint lists are bounded to sixteen concise items"))?;
        for item in items {
            text(item, 1024)?;
        }
    }
    let evidence = strings(get(value, "evidence"))?;
    if evidence.is_empty() || evidence.iter().collect::<BTreeSet<_>>().len() != evidence.len() {
        return Err(err(
            "Checkpoint evidence must contain unique source references, not a raw transcript",
        ));
    }
    scopes::conditions(get(value, "conditions"), true)?;
    let sha = get(value, "observed_active_sha256");
    if sha != &Value::Null
        && !sha.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err(err(
            "Observed active context must be SHA256, or null only for separate missing-context preparation",
        ));
    }
    Ok(value)
}
pub fn render(
    base: &str,
    value: &Value,
    developer_id: &Value,
    observation: Option<&Value>,
) -> Result<String> {
    input_value(value)?;
    let mut desired = Vec::with_capacity(8);
    for (field, title) in FIELDS {
        let content = if matches!(*field, "focus" | "next_step") {
            text(get(value, field), 2048)?.to_owned()
        } else {
            let content = list_text(&strings(get(value, field))?)?;
            if content.is_empty() {
                "None supplied.".to_owned()
            } else {
                content
            }
        };
        desired.push((*title, content));
    }
    let fenced = |v: &Value| {
        json::encode(v, json::SORTED_INDENT2_ASCII).map(|s| format!("```json\n{s}\n```"))
    };
    desired.push(("Checkpoint conditions", fenced(get(value, "conditions"))?));
    desired.push((
        "Checkpoint evidence",
        list_text(&strings(get(value, "evidence"))?)?,
    ));
    desired.push((
        "Checkpoint provenance",
        fenced(&object([
            ("developer_id", developer_id.clone()),
            ("harness", get(value, "harness").clone()),
            ("session", get(value, "session").clone()),
            ("identity_authenticated", Value::Bool(false)),
        ]))?,
    ));
    if let Some(observation) = observation {
        desired.push((
            handoff::SECTION,
            fenced(&handoff::reusable(
                base,
                handoff::validate_observation(observation)?,
            ))?,
        ));
    }
    let mut seen = BTreeSet::new();
    let mut body = String::with_capacity(base.len());
    let mut cursor = 0;
    for (start, end, title) in headings(base)? {
        if let Some((_, content)) = desired.iter().find(|(name, _)| *name == title) {
            if !seen.insert(title.clone()) {
                return Err(err(
                    "Duplicate checkpoint section requires clarification; do not choose one silently",
                ));
            }
            body.push_str(
                base.get(cursor..start)
                    .ok_or_else(|| err("Invalid checkpoint section boundary"))?,
            );
            body.push_str(&format!("## {title}\n\n{content}\n\n"));
            cursor = end;
        }
    }
    body.push_str(
        base.get(cursor..)
            .ok_or_else(|| err("Invalid checkpoint section boundary"))?,
    );
    for (title, content) in &desired {
        if !seen.contains(*title) {
            if !body.ends_with("\n\n") {
                body.push_str(if body.ends_with('\n') { "\n" } else { "\n\n" });
            }
            body.push_str(&format!("## {title}\n\n{content}\n\n"));
        }
    }
    if body.len() > LIMIT {
        return Err(err(
            "Checkpoint exceeds 16 KiB; consolidate safely without dropping distinct evidence or conditions",
        ));
    }
    let final_sections = headings(&body)?;
    if desired.iter().any(|(title, _)| {
        final_sections
            .iter()
            .filter(|(_, _, name)| name == title)
            .count()
            != 1
    }) {
        return Err(err(
            "Rendered checkpoint has ambiguous section boundaries; preserve the original context",
        ));
    }
    Ok(body)
}
fn read(bank: &Path, path: &str, limit: usize) -> Result<String> {
    let raw = crate::fs::PinnedRead::open(bank, Path::new(path), limit)
        .and_then(crate::fs::PinnedRead::read)
        .map_err(|error| err(&error.to_string()))?
        .bytes;
    let body = String::from_utf8(raw).map_err(|_| err("Checkpoint source is not UTF-8"))?;
    Ok(body)
}
fn equal(a: &Value, b: &Value) -> Result<bool> {
    Ok(
        json::encode(a, json::SORTED_COMPACT_ASCII)?
            == json::encode(b, json::SORTED_COMPACT_ASCII)?,
    )
}
fn hash(body: &str) -> Value {
    string(crate::developer_identity::sha256_hex(body.as_bytes()))
}
fn ensure_safe(files: &Value) -> Result<()> {
    let fields = files
        .as_object()
        .ok_or_else(|| err("Governed mutation exceeds its bounded memory content batch"))?;
    if fields.len() > 64
        || fields
            .iter()
            .try_fold(0usize, |total, (_, body)| {
                body.as_str().and_then(|body| total.checked_add(body.len()))
            })
            .is_none_or(|size| size > 65536)
    {
        return Err(err(
            "Governed mutation exceeds its bounded memory content batch",
        ));
    }
    let mut reasons = BTreeSet::new();
    for (_, content) in fields {
        let content = content
            .as_str()
            .ok_or_else(|| err("Invalid checkpoint content"))?;
        let scanned = crate::security::scan(&string(content), crate::security::LIMIT)?;
        for reason in strings(get(&scanned, "reasons"))? {
            reasons.insert(reason);
        }
    }
    if !reasons.is_empty() {
        return Err(err(&format!(
            "Security policy quarantined proposed memory; redact or review before writing: {}",
            reasons.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(())
}
pub fn inspect(repo: &Path) -> Result<Value> {
    let (root, bank_path) = writes::bank_path(repo)?;
    let (checked, observed) = writes::snapshot(&root, None)?;
    let coverage = writes::recovery_frontier_provider()?.observe(&root)?;
    let body = if observed.get("activeContext.md").is_some() {
        Some(read(&bank_path, "activeContext.md", MAX_ACTIVE)?)
    } else {
        None
    };
    if !equal(&writes::snapshot(&root, None)?.1, &observed)? {
        return Err(err("Bank changed during checkpoint inspection; reread"));
    }
    let route = get(&checked, "routes")
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| get(row, "path").as_str() == Some("activeContext.md"))
        .cloned()
        .unwrap_or(Value::Null);
    let public = bank::recovery_coverage_provider()?.public_coverage(&coverage)?;
    let mut fields = object([
        (
            "repo_root",
            string(crate::host::text(
                root.as_os_str(),
                "checkpoint repository",
            )?),
        ),
        ("project_id", get(&checked, "project_id").clone()),
        (
            "active_context",
            body.as_ref().map(string).unwrap_or(Value::Null),
        ),
        ("route", route),
    ]);
    for (key, item) in public
        .as_object()
        .ok_or_else(|| err("Invalid recovery coverage"))?
    {
        put(&mut fields, key, item.clone())?;
    }
    for (key, item) in [
        (
            "observed_active_sha256",
            get(&observed, "activeContext.md").clone(),
        ),
        ("source_hashes", observed),
        ("preparation_required", Value::Bool(body.is_none())),
        ("content_is_untrusted_data", Value::Bool(true)),
        ("writes_performed", Value::Bool(false)),
    ] {
        put(&mut fields, key, item)?;
    }
    result("inspect", fields)
}
pub fn preview(repo: &Path, value: &Value, prepare: bool) -> Result<Value> {
    input_value(value)?;
    let developer = scope_binding::principal(repo, &[], None, None)?;
    let context = writes::recovery_frontier_provider()?.require_scope(repo, &Value::Null, None)?;
    if get(&context, "developer_id") != get(&developer, "developer_id")
        || get(&context, "developer_sha256") != get(&developer, "observed_sha256")
    {
        return Err(err(
            "Developer identity changed during checkpoint resolution; retain original provenance and prepare again",
        ));
    }
    let current = inspect(repo)?;
    if get(&current, "observed_active_sha256") != get(value, "observed_active_sha256") {
        return Err(err(
            "Active context changed or was not read; inspect it and preserve compatible evidence before a fresh proposal",
        ));
    }
    let (root, bank_path) = writes::bank_path(repo)?;
    let observation = handoff::observe(&root, &bank_path, &strings(get(value, "evidence"))?)?;
    if prepare {
        if get(&current, "preparation_required").as_bool() != Some(true) {
            return Err(err(
                "Active context already exists; use the single-file checkpoint",
            ));
        }
        let mut files = transactions::files_at(&bank_path)?;
        let body = render(
            "# Active context\n",
            value,
            get(&context, "developer_id"),
            Some(&observation),
        )?;
        ensure_safe(&object([("activeContext.md", string(&body))]))?;
        put(&mut files, "activeContext.md", string(body))?;
        let router = bank::add_context_routes(
            str_value(&files, "MEMORY.md")?,
            &["activeContext.md".to_owned()],
            &string(crate::local_registry::now(crate::host::Sources::default())?),
        )?;
        put(&mut files, "MEMORY.md", string(router))?;
        let preview = transactions::preview(&root, &files, "replace")?;
        let mut proposal = get(&preview, "proposal").clone();
        put(
            &mut proposal,
            "checkpoint_evidence",
            object([
                ("input", value.clone()),
                ("observation", observation),
                ("base_text", string("# Active context\n")),
                ("base_router", string(current_router(&current)?)),
            ]),
        )?;
        if !equal(
            get(&proposal, "observed_hashes"),
            get(&current, "source_hashes"),
        )? || !equal(get(&proposal, "scope_context"), &context)?
        {
            return Err(err(
                "Bank or identity changed during context preparation; reread",
            ));
        }
        validate_preparation(&proposal, true)?;
        return result(
            "prepare",
            object([
                ("separate_router_operation", Value::Bool(true)),
                ("transaction", proposal),
                ("writes_performed", Value::Bool(false)),
                ("verified", Value::Bool(false)),
            ]),
        );
    }
    if get(&current, "preparation_required").as_bool() == Some(true) {
        return Err(err(
            "Active context is omitted; prepare its useful content and route as a separate observed transaction first",
        ));
    }
    let base = str_value(&current, "active_context")?;
    let body = render(
        base,
        value,
        get(&context, "developer_id"),
        Some(&observation),
    )?;
    ensure_safe(&object([("activeContext.md", string(&body))]))?;
    let mut prepared = get(
        &writes::preview(repo, "activeContext.md", &body, "checkpoint")?,
        "proposal",
    )
    .clone();
    put(
        &mut prepared,
        "checkpoint_evidence",
        object([
            ("input", value.clone()),
            ("base_text", string(base)),
            ("observation", observation),
        ]),
    )?;
    if !equal(
        get(&prepared, "observed_hashes"),
        get(&current, "source_hashes"),
    )? || !equal(get(&prepared, "scope_context"), &context)?
    {
        return Err(err(
            "Bank or identity changed during checkpoint preparation; inspect again",
        ));
    }
    result(
        "preview",
        object([
            (
                "proposal",
                object([
                    ("schema", string("agent-memory/checkpoint-proposal-v1")),
                    ("input", value.clone()),
                    ("base_text", string(base)),
                    ("write", prepared),
                ]),
            ),
            (
                "changed_memory_paths",
                Value::Array(if body == base {
                    vec![]
                } else {
                    vec![string(format!(
                        "{}/activeContext.md",
                        repository_config::relative(repo)?
                    ))]
                }),
            ),
            ("writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(false)),
        ]),
    )
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = prepared.get("proposal").unwrap_or(prepared);
    let fields = ["schema", "input", "base_text", "write"];
    if crate::records::fields(p, &fields, &fields).is_err()
        || get(p, "schema").as_str() != Some("agent-memory/checkpoint-proposal-v1")
    {
        return Err(err(
            "Invalid checkpoint proposal; router preparation is a separate transaction",
        ));
    }
    let value = input_value(get(p, "input"))?;
    let write = writes::proposal_value(get(p, "write"))?;
    let proof = write.get("checkpoint_evidence");
    if let Some(proof) = proof {
        if !equal(get(proof, "input"), value)? || get(proof, "base_text") != get(p, "base_text") {
            return Err(err(
                "Checkpoint wrapper differs from its source-bound write",
            ));
        }
    }
    if get(write, "path").as_str() != Some("activeContext.md")
        || get(write, "operation").as_str() != Some("checkpoint")
        || get(get(write, "observed_hashes"), "activeContext.md")
            != get(value, "observed_active_sha256")
    {
        return Err(err(
            "Checkpoint may replace only the observed activeContext.md",
        ));
    }
    let base = get(p, "base_text").as_str().ok_or_else(|| err("Checkpoint content must match the reviewed fields and preserve unrelated existing sections"))?;
    if hash(base) != *get(value, "observed_active_sha256")
        || get(write, "scope_context").as_object().is_none()
        || render(
            base,
            value,
            get(get(write, "scope_context"), "developer_id"),
            proof.map(|p| get(p, "observation")),
        )? != str_value(write, "content")?
    {
        return Err(err(
            "Checkpoint content must match the reviewed fields and preserve unrelated existing sections",
        ));
    }
    let receipt = writes::apply(write)?;
    let verified = get(&receipt, "verified").as_bool() == Some(true)
        && get(&receipt, "still_current").as_bool() == Some(true);
    let writes_performed = get(&receipt, "writes_performed").clone();
    let changed_memory_paths = get(&receipt, "changed_memory_paths").clone();
    result(
        "apply",
        object([
            ("write", receipt),
            ("verified", Value::Bool(verified)),
            ("writes_performed", writes_performed),
            ("changed_memory_paths", changed_memory_paths),
            ("sharing", string("local-or-pending-sharing")),
        ]),
    )
}
pub fn current_router(current: &Value) -> Result<String> {
    let (_, bank_path) = writes::bank_path(Path::new(str_value(current, "repo_root")?))?;
    let body = read(&bank_path, "MEMORY.md", bank::MAX_ROUTER_BYTES)?;
    if hash(&body) != *get(get(current, "source_hashes"), "MEMORY.md") {
        return Err(err("Router changed during checkpoint preparation"));
    }
    Ok(body)
}
pub fn validate_write(write: &Value, sources: bool) -> Result<()> {
    let proof = get(write, "checkpoint_evidence");
    let fields = ["input", "base_text", "observation"];
    if crate::records::fields(proof, &fields, &fields).is_err()
        || get(write, "operation").as_str() != Some("checkpoint")
        || get(write, "path").as_str() != Some("activeContext.md")
    {
        return Err(err("Invalid source-bound checkpoint write"));
    }
    let value = input_value(get(proof, "input"))?;
    let base = get(proof, "base_text").as_str().ok_or_else(|| {
        err("Checkpoint write differs from reviewed fields and source observation")
    })?;
    if hash(base) != *get(value, "observed_active_sha256")
        || get(value, "observed_active_sha256")
            != get(get(write, "observed_hashes"), "activeContext.md")
        || render(
            base,
            value,
            get(get(write, "scope_context"), "developer_id"),
            Some(get(proof, "observation")),
        )? != str_value(write, "content")?
    {
        return Err(err(
            "Checkpoint write differs from reviewed fields and source observation",
        ));
    }
    if sources {
        let (root, bank_path) = writes::bank_path(Path::new(str_value(write, "repo_root")?))?;
        handoff::check_pins(
            &root,
            &bank_path,
            &strings(get(value, "evidence"))?,
            get(proof, "observation"),
        )?;
    }
    Ok(())
}
pub fn validate_preparation(tx: &Value, sources: bool) -> Result<()> {
    let proof = get(tx, "checkpoint_evidence");
    let fields = ["input", "base_text", "base_router", "observation"];
    let observed = get(tx, "observed_hashes");
    if crate::records::fields(proof, &fields, &fields).is_err()
        || get(tx, "operation").as_str() != Some("replace")
        || observed.get("activeContext.md").is_some()
    {
        return Err(err(
            "Checkpoint preparation may only add missing active context and its route",
        ));
    }
    let value = input_value(get(proof, "input"))?;
    let base_router = str_value(proof, "base_router")?;
    if get(value, "observed_active_sha256") != &Value::Null
        || get(proof, "base_text").as_str() != Some("# Active context\n")
        || hash(base_router) != *get(observed, "MEMORY.md")
    {
        return Err(err(
            "Checkpoint preparation base differs from its inspected bank",
        ));
    }
    let expected = render(
        "# Active context\n",
        value,
        get(get(tx, "scope_context"), "developer_id"),
        Some(get(proof, "observation")),
    )?;
    let files = get(tx, "files");
    let (metadata, _, _) = bank::parse_router(str_value(files, "MEMORY.md")?)?;
    let router = bank::add_context_routes(
        base_router,
        &["activeContext.md".to_owned()],
        get(&metadata, "router-updated"),
    )?;
    let observed_fields = observed
        .as_object()
        .ok_or_else(|| err("Invalid checkpoint observation"))?;
    let file_fields = files
        .as_object()
        .ok_or_else(|| err("Invalid checkpoint files"))?;
    if get(files, "activeContext.md").as_str() != Some(&expected)
        || get(files, "MEMORY.md").as_str() != Some(&router)
        || file_fields.len() != observed_fields.len() + 1
        || observed_fields.iter().any(|(path, sha)| {
            files.get(path).is_none()
                || (path != "MEMORY.md"
                    && files.get(path).and_then(Value::as_str).map(hash).as_ref() != Some(sha))
        })
    {
        return Err(err(
            "Checkpoint preparation must preserve other files and existing routing",
        ));
    }
    if sources {
        let (root, bank_path) = writes::bank_path(Path::new(str_value(tx, "repo_root")?))?;
        handoff::check_pins(
            &root,
            &bank_path,
            &strings(get(value, "evidence"))?,
            get(proof, "observation"),
        )?;
    }
    Ok(())
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> CommandResult {
    let (value, exit_code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let mut fields = object([
                ("error", string(error.to_string())),
                ("verified", Value::Bool(false)),
                ("inspection_required", Value::Bool(true)),
            ]);
            if let (Value::Object(fields), Value::Object(details)) =
                (&mut fields, crate::bank_capacity::details(&error))
            {
                fields.extend(details);
            }
            match result(operation, fields) {
                Ok(value) => (value, 1, json::DEFAULT),
                Err(error) => return crate::cli::refusal("checkpoint", &error.to_string()),
            }
        }
    };
    match json::encode(&value, style) {
        Ok(stdout) => CommandResult {
            value: Some(value),
            exit_code,
            stdout: stdout + "\n",
            stderr: String::new(),
        },
        Err(error) => crate::cli::refusal("checkpoint", &error.to_string()),
    }
}
