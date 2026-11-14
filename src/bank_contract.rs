//! Read-only rendering and validation of the canonical router/file contract.
use crate::codec::{
    frontmatter,
    json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value},
};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::{bank_capacity, bank_layout, fs, repository_config, validation_memo};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};
use unicode_normalization::UnicodeNormalization;

pub static SPEC: LazyLock<Value> = LazyLock::new(|| {
    // Invariant: the compile-time router schema is reviewed, valid JSON.
    json::decode(
        include_str!("../skills/agent-memory/schemas/bank-router.json"),
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        },
    )
    .expect("bundled bank router schema")
});
pub static CONTEXTS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    // Invariant: the compile-time router schema declares an x-contexts object.
    get(&SPEC, "x-contexts")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect()
});
pub static FIELDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    // Invariant: the compile-time router schema declares five string route fields.
    get(&SPEC, "x-route-fields")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
});
pub const MAX_ROUTER_BYTES: usize = 16_384;
pub const MAX_ROUTES: usize = 64;
pub const MAX_PHYSICAL_PATHS: usize = 1_000_000;
pub const MAX_PHYSICAL_DEPTH: usize = 256;
pub type ParsedRecords = BTreeMap<String, (String, Value, String)>;
pub type ParsedRouter = (Value, Vec<Value>, Value);

/// A source-stable observation, implemented by bank_observation.
#[derive(Clone, Debug)]
pub struct Observation {
    pub checked: Value,
    pub hashes: Value,
    pub sources: BTreeMap<String, Vec<u8>>,
    pub recovery_frontier: Value,
}
pub trait BankObservation: Send + Sync {
    fn observe(
        &self,
        repo: &Path,
        owner: Option<&[u8]>,
        transaction: Option<&[u8]>,
    ) -> Result<Observation>;
    fn scan_hashes(&self, bank: &Path) -> Result<Value>;
    /// Baseline scan(read_sources=True, require_router=False): physical map
    /// (including runtime entries, path -> directory/file/other), observed hashes,
    /// and exact canonical source bytes, with no router required.
    fn scan_sources(&self, bank: &Path) -> Result<(Value, Value, BTreeMap<String, Vec<u8>>)>;
}
pub trait RecoveryCoverage: Send + Sync {
    fn public_coverage(&self, frontier: &Value) -> Result<Value>;
}
/// CLI-owned recover-layout route; domain code never imports or launches a script.
pub trait RouteTable: Send + Sync {
    fn execute(&self, route: &str, arguments: &[String])
    -> Result<crate::lifecycle::CommandResult>;
}
static OBSERVATION: RwLock<Option<Arc<dyn BankObservation>>> = RwLock::new(None);
static COVERAGE: RwLock<Option<Arc<dyn RecoveryCoverage>>> = RwLock::new(None);
static ROUTES: RwLock<Option<Arc<dyn RouteTable>>> = RwLock::new(None);
pub fn register_bank_observation(provider: Option<Arc<dyn BankObservation>>) {
    *OBSERVATION.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub(crate) fn bank_observation_provider() -> Result<Arc<dyn BankObservation>> {
    OBSERVATION
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| missing("bank_observation provider is not registered"))
}
pub fn register_recovery_coverage(provider: Option<Arc<dyn RecoveryCoverage>>) {
    *COVERAGE.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub(crate) fn recovery_coverage_provider() -> Result<Arc<dyn RecoveryCoverage>> {
    COVERAGE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| missing("recovery_frontier coverage provider is not registered"))
}
pub fn register_route_table(provider: Option<Arc<dyn RouteTable>>) {
    *ROUTES.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
pub fn observe(
    repo: &Path,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<Observation> {
    bank_observation_provider()?.observe(repo, owner, transaction)
}
pub fn scan_hashes(bank: &Path) -> Result<Value> {
    bank_observation_provider()?.scan_hashes(bank)
}
pub fn scan_sources(bank: &Path) -> Result<(Value, Value, BTreeMap<String, Vec<u8>>)> {
    bank_observation_provider()?.scan_sources(bank)
}
pub fn recover_layout(arguments: &[String]) -> Result<crate::lifecycle::CommandResult> {
    let provider = ROUTES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| missing("RouteTable provider is not registered"))?;
    provider.execute("recover-layout", arguments)
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn missing(message: &str) -> ContractError {
    ContractError::missing_provider(message)
}
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
fn framing(c: char) -> bool {
    c < ' ' || matches!(c, '\u{7f}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}
pub fn timestamp(value: &Value) -> Result<()> {
    crate::lifecycle::time_value(value)
        .map(|_| ())
        .map_err(|_| err("Invalid router-updated time"))
}
pub fn header(metadata: &Value) -> Result<()> {
    if crate::records::fields(
        metadata,
        &["schema", "project-id", "router-updated"],
        repository_config::ROUTER_FIELDS,
    )
    .is_err()
    {
        return Err(err(
            "Router requires schema, project-id and router-updated; only known fields are allowed",
        ));
    }
    if let Some(remote) = metadata.get("canonical-remote") {
        let spelling = remote.as_str().ok_or_else(|| {
            err("canonical-remote must be a sanitized canonical repository identifier")
        })?;
        if crate::remote_identity::normalize(remote, None)? != spelling {
            return Err(err(
                "canonical-remote must be a sanitized canonical repository identifier",
            ));
        }
    }
    if get(metadata, "schema").as_str() != Some("agent-memory/v1") {
        return Err(err(
            "Unsupported bank schema; no mutation or implicit migration is allowed. Inspect am migrate preview for an explicitly bundled chain.",
        ));
    }
    if !crate::developer_identity::uuid_valid(get(metadata, "project-id")) {
        return Err(err("project-id must be a canonical random UUIDv4"));
    }
    timestamp(get(metadata, "router-updated"))
}
pub fn path_value(value: &Value) -> Result<&str> {
    let value = value
        .as_str()
        .filter(|s| !s.is_empty() && !s.chars().any(|c| framing(c) || "\\|[]()<>#%".contains(c)))
        .ok_or_else(|| err("Route path must be an unambiguous relative Markdown path"))?;
    if value.starts_with('/') || value.split('/').any(|p| p.is_empty() || p.starts_with('.')) {
        return Err(err("Route escapes the bank or names runtime/hidden state"));
    }
    if !value.ends_with(".md") || value.rsplit('/').next() == Some(".md") || value == "MEMORY.md" {
        return Err(err(
            "Routes must name Markdown targets other than the router",
        ));
    }
    let first = value
        .split('/')
        .next()
        .ok_or_else(|| err("Route path must be an unambiguous relative Markdown path"))?;
    if !CONTEXTS.contains(&value)
        && (!value.contains('/')
            || !get(&SPEC, "x-optional-directories")
                .as_array()
                .ok_or_else(|| err("Invalid bundled optional-directory contract"))?
                .iter()
                .any(|v| v.as_str() == Some(first)))
    {
        return Err(err(
            "Routes must name context files or optional project-record directories",
        ));
    }
    Ok(value)
}
pub fn text_cell(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| {
            !trim(s).is_empty() && !s.chars().any(|c| framing(c) || matches!(c, '|' | '\\'))
        })
        .map(trim)
        .ok_or_else(|| {
            err("Route cells require nonempty single-line text without pipes or backslashes")
        })
}
pub fn route_value(route: &Value) -> Result<Value> {
    if crate::records::fields(route, &FIELDS, &FIELDS).is_err() {
        return Err(err(
            "Routes require path, purpose, scope, lifecycle and read-when",
        ));
    }
    let route = Value::Object(
        FIELDS
            .iter()
            .map(|k| Ok(((*k).to_owned(), string(text_cell(get(route, k))?))))
            .collect::<Result<_>>()?,
    );
    path_value(get(&route, "path"))?;
    crate::scopes::parse_scope(get(&route, "scope"))?;
    if !get(&crate::lifecycle::MODEL, "states")
        .as_array()
        .ok_or_else(|| err("Invalid bundled lifecycle contract"))?
        .contains(get(&route, "lifecycle"))
    {
        return Err(err("Unknown route lifecycle"));
    }
    Ok(route)
}
fn relative_target(path: &str, source: &str) -> String {
    let target: Vec<_> = path.split('/').collect();
    let source: Vec<_> = source
        .rsplit_once('/')
        .map(|(parent, _)| parent.split('/').collect())
        .unwrap_or_default();
    let common = source
        .iter()
        .zip(&target)
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts = vec![".."; source.len() - common];
    parts.extend(target.iter().skip(common).copied());
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}
pub fn route_table(routes: &[Value], source: &str) -> Result<String> {
    if routes.len() > MAX_ROUTES {
        return Err(err("Route limit exceeded; use a bounded topic index"));
    }
    let mut text =
        "| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n"
            .to_owned();
    for route in routes {
        let route = route_value(route)?;
        let path = required_text(
            &route,
            "path",
            "Routes require path, purpose, scope, lifecycle and read-when",
        )?;
        text.push_str(&format!("| [{path}](<{}>)", relative_target(path, source)));
        for key in FIELDS.iter().skip(1) {
            text.push_str(" | ");
            text.push_str(required_text(
                &route,
                key,
                "Routes require path, purpose, scope, lifecycle and read-when",
            )?);
        }
        text.push_str(" |\n");
    }
    Ok(text)
}
/// Python `str.splitlines`, including its CRLF coalescing and terminal-break rule.
fn split_lines(text: &str, keepends: bool) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let boundary = rest.char_indices().find(|(_, c)| {
            matches!(
                c,
                '\n' | '\r'
                    | '\u{b}'
                    | '\u{c}'
                    | '\u{1c}'
                    | '\u{1d}'
                    | '\u{1e}'
                    | '\u{85}'
                    | '\u{2028}'
                    | '\u{2029}'
            )
        });
        let Some((start, character)) = boundary else {
            let line = rest;
            rest = "";
            return Some(line);
        };
        let mut end = start + character.len_utf8();
        // Invariant: char_indices and len_utf8 delimit complete UTF-8 characters.
        if character == '\r' && rest.get(end..).is_some_and(|tail| tail.starts_with('\n')) {
            end += 1;
        }
        let line = rest.get(..if keepends { end } else { start })?;
        rest = rest.get(end..)?;
        Some(line)
    })
}
pub fn parse_table(text: &str, source: &str) -> Result<Vec<Value>> {
    let rows: Vec<_> = split_lines(text, false)
        .map(trim)
        .filter(|s| !s.is_empty())
        .collect();
    if rows.first().copied() != Some("| Path | Purpose | Scope | Lifecycle | Read when |")
        || rows.get(1).copied() != Some("| --- | --- | --- | --- | --- |")
    {
        return Err(err("Invalid route table header"));
    }
    if rows.len() - 2 > MAX_ROUTES {
        return Err(err("Route limit exceeded; use a bounded topic index"));
    }
    let mut routes = Vec::new();
    for row in rows.iter().skip(2) {
        let row = if *row == "|" {
            ""
        } else {
            row.strip_prefix('|')
                .and_then(|s| s.strip_suffix('|'))
                .ok_or_else(|| err("Invalid route table row"))?
        };
        let mut cells: Vec<_> = row.split('|').map(trim).collect();
        if cells.len() != FIELDS.len() {
            return Err(err("Route row must have five columns"));
        }
        let link = || err("Route requires an explicit relative Markdown link");
        // Invariant: this function checked that cells contains exactly five fields.
        let (label, destination) = cells[0]
            .strip_prefix('[')
            .and_then(|s| s.split_once("]("))
            .ok_or_else(link)?;
        let destination = destination.strip_suffix(')').ok_or_else(link)?;
        let destination = if let Some(s) = destination.strip_prefix('<') {
            s.strip_suffix('>')
                .filter(|s| !s.is_empty() && !s.contains(['<', '>']))
                .ok_or_else(link)?
        } else {
            if destination.is_empty()
                || destination.chars().any(|c| {
                    c.is_whitespace()
                        || ('\u{1c}'..='\u{1f}').contains(&c)
                        || matches!(c, '<' | '>')
                })
            {
                return Err(link());
            }
            destination
        };
        if label.is_empty() || label.contains(']') {
            return Err(link());
        }
        path_value(&string(label))?;
        if destination != relative_target(label, source) {
            return Err(err("Route label and relative target must agree"));
        }
        cells[0] = label;
        let route = route_value(&Value::Object(
            FIELDS
                .iter()
                .zip(cells)
                .map(|(k, v)| ((*k).to_owned(), string(v)))
                .collect(),
        ))?;
        routes.push(route);
    }
    let mut seen = HashSet::new();
    for route in &routes {
        if !seen.insert(required_text(route, "path", "Duplicate route path")?) {
            return Err(err("Duplicate route path"));
        }
    }
    Ok(routes)
}
pub fn router_text(metadata: &Value, routes: &[Value], omitted: &Value) -> Result<String> {
    header(metadata)?;
    if routes.len() > MAX_ROUTES {
        return Err(err(
            "Router route limit exceeded; use a bounded topic index",
        ));
    }
    let routes = routes.iter().map(route_value).collect::<Result<Vec<_>>>()?;
    let paths: HashSet<_> = routes
        .iter()
        .map(|r| {
            required_text(
                r,
                "path",
                "Routes require path, purpose, scope, lifecycle and read-when",
            )
        })
        .collect::<Result<_>>()?;
    if paths.len() != routes.len() {
        return Err(err("Duplicate route path"));
    }
    let expected: HashSet<_> = CONTEXTS
        .iter()
        .copied()
        .filter(|p| !paths.contains(p))
        .collect();
    let omitted = omitted
        .as_object()
        .ok_or_else(|| err("Every absent context needs exactly one omission reason"))?;
    if omitted.len() != expected.len()
        || omitted.iter().any(|(k, _)| !expected.contains(k.as_str()))
    {
        return Err(err(
            "Every absent context needs exactly one omission reason",
        ));
    }
    let mut text = frontmatter::write(metadata.as_object().ok_or_else(|| {
        err("Router requires schema, project-id and router-updated; only known fields are allowed")
    })?)?;
    text.push_str("# Project memory\n\nRouter time describes this map, not every file's revision.\n\n## Routes\n\n");
    text.push_str(&route_table(&routes, "MEMORY.md")?);
    text.push_str("\n## Omitted contexts\n\n");
    if omitted.is_empty() {
        text.push_str("None.\n");
    } else {
        for path in CONTEXTS.iter() {
            if let Some((_, reason)) = omitted.iter().find(|(k, _)| k == path) {
                text.push_str(&format!("- `{path}`: {}\n", text_cell(reason)?));
            }
        }
    }
    if text.len() > MAX_ROUTER_BYTES {
        return Err(err(
            "Router byte limit exceeded; move detail into project context",
        ));
    }
    Ok(text)
}
pub fn parse_router(text: &str) -> Result<ParsedRouter> {
    if text.len() > MAX_ROUTER_BYTES {
        return Err(err("Router byte limit exceeded"));
    }
    let (metadata, body) = frontmatter::read(text, repository_config::ROUTER_FIELDS)?;
    let metadata = Value::Object(metadata);
    header(&metadata)?;
    if body.matches("\n## Routes\n").count() != 1
        || body.matches("\n## Omitted contexts\n").count() != 1
    {
        return Err(err(
            "Router needs exactly one Routes and Omitted contexts section",
        ));
    }
    let (_, rest) = body
        .split_once("\n## Routes\n")
        .ok_or_else(|| err("Router needs exactly one Routes and Omitted contexts section"))?;
    let (table, omissions) = rest
        .split_once("\n## Omitted contexts\n")
        .ok_or_else(|| err("Router needs exactly one Routes and Omitted contexts section"))?;
    let routes = parse_table(table, "MEMORY.md")?;
    let mut omitted = Vec::new();
    if trim(omissions) != "None." {
        for line in split_lines(trim(omissions), false) {
            let (path, reason) = line
                .strip_prefix("- `")
                .and_then(|s| s.split_once("`: "))
                .filter(|(p, r)| !p.is_empty() && !p.contains('`') && !r.is_empty())
                .ok_or_else(|| err("Invalid or duplicated context omission"))?;
            if omitted.iter().any(|(p, _)| p == path) {
                return Err(err("Invalid or duplicated context omission"));
            }
            omitted.push((path.to_owned(), string(text_cell(&string(reason))?)));
        }
    }
    let omitted = Value::Object(omitted);
    router_text(&metadata, &routes, &omitted)?;
    Ok((metadata, routes, omitted))
}
pub fn context_route(path: &str) -> Result<Value> {
    let (purpose, trigger) = match path {
        "projectbrief.md" => (
            "Project purpose and authority boundaries",
            "At session start to identify the project",
        ),
        "productContext.md" => (
            "Users, workflow and scope exclusions",
            "When deciding product scope or comparing requirements",
        ),
        "activeContext.md" => (
            "Scoped handoff, unresolved questions and next step",
            "When resuming the matching task or investigating its questions",
        ),
        "systemPatterns.md" => (
            "Architecture and established project patterns",
            "When changing design, integration or project conventions",
        ),
        "techContext.md" => (
            "Qualified implementation and technical source evidence",
            "When working on the relevant implementation or tooling",
        ),
        "progress.md" => (
            "Delivery evidence and work-source references",
            "When checking delivery claims or release scope",
        ),
        _ => return Err(err("Navigation defaults require a known ordinary context")),
    };
    Ok(object([
        ("path", string(path)),
        ("purpose", string(purpose)),
        ("scope", string("repository")),
        ("lifecycle", string("active")),
        ("read-when", string(trigger)),
    ]))
}
pub fn add_context_routes(text: &str, paths: &[String], updated: &Value) -> Result<String> {
    let (_, mut routes, omitted) = parse_router(text)?;
    timestamp(updated)?;
    let distinct: HashSet<_> = paths.iter().collect();
    if distinct.len() != paths.len() || paths.iter().any(|p| omitted.get(p).is_none()) {
        return Err(err("Only missing ordinary context routes may be added"));
    }
    if paths.is_empty() {
        return Ok(text.to_owned());
    }
    let new_routes = paths
        .iter()
        .map(|p| context_route(p))
        .collect::<Result<Vec<_>>>()?;
    let end = text
        .get(4..)
        .and_then(|text| text.find("\n---\n"))
        .ok_or_else(|| err("Missing frontmatter terminator"))?
        + 4;
    let stamp = json::encode(updated, json::DEFAULT)?;
    let prefix = text
        .get(..end)
        .ok_or_else(|| err("Missing frontmatter terminator"))?
        .split_inclusive('\n')
        .map(|line| {
            if line.starts_with("router-updated:") {
                format!(
                    "router-updated: {stamp}{}",
                    if line.ends_with('\n') { "\n" } else { "" }
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<String>();
    let (intro, rest) = text
        .get(end..)
        .and_then(|text| text.split_once("\n## Routes\n"))
        .ok_or_else(|| err("Router needs exactly one Routes and Omitted contexts section"))?;
    let (table, omissions) = rest
        .split_once("\n## Omitted contexts\n")
        .ok_or_else(|| err("Router needs exactly one Routes and Omitted contexts section"))?;
    let lines: Vec<_> = split_lines(table, true).collect();
    let mut insertion = lines.len();
    while lines
        .get(insertion.wrapping_sub(1))
        .is_some_and(|line| trim(line).is_empty())
    {
        insertion -= 1;
    }
    let mut table = lines.iter().take(insertion).copied().collect::<String>();
    if !table.is_empty() && !table.ends_with('\n') {
        table.push('\n');
    }
    table.extend(split_lines(&route_table(&new_routes, "MEMORY.md")?, true).skip(2));
    table.extend(lines.iter().skip(insertion).copied());
    let omissions = if omitted
        .as_object()
        .ok_or_else(|| err("Every absent context needs exactly one omission reason"))?
        .len()
        == paths.len()
    {
        "\n\nNone.\n".to_owned()
    } else {
        split_lines(omissions, true)
            .filter(|line| {
                !paths
                    .iter()
                    .any(|p| trim(line).starts_with(&format!("- `{p}`: ")))
            })
            .collect()
    };
    let result = format!("{prefix}{intro}\n## Routes\n{table}\n## Omitted contexts\n{omissions}");
    routes.extend(new_routes);
    if parse_router(&result)?.1 != routes {
        return Err(err("Existing route metadata must be preserved"));
    }
    Ok(result)
}
pub fn render(proposal: &Value) -> Result<Value> {
    let required = ["project-id", "router-updated", "contexts", "omitted"];
    if crate::records::fields(
        proposal,
        &required,
        &[
            "project-id",
            "router-updated",
            "contexts",
            "omitted",
            "canonical-remote",
        ],
    )
    .is_err()
    {
        return Err(err(
            "Render input needs project-id, router-updated, contexts and omitted",
        ));
    }
    let contexts = get(proposal, "contexts")
        .as_object()
        .filter(|o| !o.is_empty() && o.iter().all(|(k, _)| CONTEXTS.contains(&k.as_str())))
        .ok_or_else(|| err("Provide at least one useful recognizable context file"))?;
    if get(proposal, "omitted").as_object().is_none() {
        return Err(err("omitted must map absent contexts to explicit reasons"));
    }
    let mut metadata = vec![
        ("schema".into(), string("agent-memory/v1")),
        ("project-id".into(), get(proposal, "project-id").clone()),
        (
            "router-updated".into(),
            get(proposal, "router-updated").clone(),
        ),
    ];
    if let Some(remote) = proposal.get("canonical-remote") {
        metadata.push(("canonical-remote".into(), remote.clone()));
    }
    let metadata = Value::Object(metadata);
    let mut files = Vec::new();
    let mut routes = Vec::new();
    for path in CONTEXTS.iter() {
        if let Some((_, context)) = contexts.iter().find(|(p, _)| p == path) {
            if crate::records::fields(
                context,
                &["body", "purpose", "scope", "lifecycle", "read-when"],
                &["body", "purpose", "scope", "lifecycle", "read-when"],
            )
            .is_err()
            {
                return Err(err(
                    "Each context needs body, purpose, scope, lifecycle and read-when",
                ));
            }
            let body = get(context, "body")
                .as_str()
                .filter(|s| !trim(s).is_empty())
                .ok_or_else(|| err("Empty context placeholders are forbidden"))?;
            files.push((
                (*path).to_owned(),
                string(format!(
                    "{}\n",
                    body.trim_end_matches(
                        |c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
                    )
                )),
            ));
            let mut route = vec![("path".into(), string(*path))];
            route.extend(
                FIELDS
                    .iter()
                    .skip(1)
                    .map(|k| ((*k).to_owned(), get(context, k).clone())),
            );
            routes.push(Value::Object(route));
        }
    }
    files.insert(
        0,
        (
            "MEMORY.md".into(),
            string(router_text(&metadata, &routes, get(proposal, "omitted"))?),
        ),
    );
    let files = Value::Object(files);
    validate_files(&files, None)?;
    Ok(object([
        ("schema", string("agent-memory/bank-contract-result-v1")),
        ("operation", string("render")),
        ("bank_path", get(&SPEC, "x-bank-path").clone()),
        ("project_id", get(&metadata, "project-id").clone()),
        ("files", files),
        ("writes_performed", Value::Bool(false)),
    ]))
}
pub fn portable_key(value: &str) -> String {
    crate::unicode::nfc_casefold(value).nfc().collect()
}
pub fn portable_aliases<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let mut seen = HashMap::<String, String>::new();
    for path in paths {
        let absolute = path.starts_with('/');
        let mut spelling = if absolute {
            "/".to_owned()
        } else {
            String::new()
        };
        if absolute {
            seen.entry("/".into()).or_insert_with(|| "/".into());
        }
        for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
            if !spelling.is_empty() && !spelling.ends_with('/') {
                spelling.push('/');
            }
            spelling.push_str(part);
            if seen.get(&spelling) == Some(&spelling) {
                continue;
            }
            let key = portable_key(&spelling);
            if seen.get(&key).is_some_and(|old| old != &spelling) {
                return Err(err(
                    "Bank paths have case or Unicode component aliases; preserve the physical entries",
                ));
            }
            seen.insert(key, spelling.clone());
        }
    }
    Ok(())
}
pub fn runtime_path(path: &str) -> bool {
    path.split('/').any(|p| p.starts_with(".agent-memory"))
}
pub fn bank_root(bank: &Path) -> Result<()> {
    let Some(parent) = bank.parent() else {
        return Ok(());
    };
    let chain = match fs::DirectoryChain::open(parent) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(err(&e.to_string())),
    };
    let name = bank
        .file_name()
        .ok_or_else(|| err("Bank root requires a directory name"))?;
    let name = crate::host::text(name, "bank root")?;
    for entry in chain
        .directory()
        .entries_bounded(MAX_PHYSICAL_PATHS)
        .map_err(|e| err(&e.to_string()))?
        .ok_or_else(|| {
            err("Bank root parent inventory exceeds its bound; preserve the existing layout")
        })?
    {
        // A name that is not UTF-8 can never be a spelling alias of the
        // UTF-8 bank name; the baseline compares and moves on.
        let Some(entry) = entry.to_str() else {
            continue;
        };
        if portable_key(entry) == portable_key(name) && entry != name {
            return Err(err(
                "Bank root has a case or Unicode spelling alias; explicit repair is required",
            ));
        }
    }
    chain.recheck().map_err(|e| err(&e.to_string()))
}
pub fn physical_paths(bank: &Path) -> Result<Vec<String>> {
    fn walk(
        dir: &fs::Descriptor,
        prefix: &str,
        paths: &mut Vec<String>,
        count: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_PHYSICAL_DEPTH {
            return Err(err(
                "Bank path traversal depth exceeds its bound; preserve the existing layout",
            ));
        }
        let before = dir.stat().map_err(|_| {
            err("Cannot completely inspect bank paths; preserve the existing layout")
        })?;
        for name in dir
            .entries_bounded(MAX_PHYSICAL_PATHS - *count)
            .map_err(|_| err("Cannot completely inspect bank paths; preserve the existing layout"))?
            .ok_or_else(|| {
                err("Bank physical path inventory exceeds its bound; preserve the existing layout")
            })?
        {
            *count += 1;
            if *count > MAX_PHYSICAL_PATHS {
                return Err(err(
                    "Bank physical path inventory exceeds its bound; preserve the existing layout",
                ));
            }
            let info = dir
                .stat_at(&name)
                .map_err(|_| err("Cannot inspect bank path; preserve the existing layout"))?;
            if info.is_symlink() {
                return Err(err("Bank paths must not be symlinks"));
            }
            let name_text = crate::host::text(&name, "bank path")?;
            let path = if prefix.is_empty() {
                name_text.to_owned()
            } else {
                format!("{prefix}/{name_text}")
            };
            if !runtime_path(&path) {
                paths.push(path.clone());
            }
            if info.is_directory() {
                let child = dir.open_directory_at(&name).map_err(|_| {
                    err("Cannot completely inspect bank paths; preserve the existing layout")
                })?;
                if child.stat().map_err(|_| err("Bank paths changed during inspection, including runtime lock state; preserve the existing layout and retry when writers are idle"))? != info {
                    return Err(err(
                        "Bank paths changed during inspection, including runtime lock state; preserve the existing layout and retry when writers are idle",
                    ));
                }
                walk(&child, &path, paths, count, depth + 1)?;
            }
            if dir.stat_at(&name).map_err(|_| err("Bank paths changed during inspection, including runtime lock state; preserve the existing layout and retry when writers are idle"))? != info {
                return Err(err(
                    "Bank paths changed during inspection, including runtime lock state; preserve the existing layout and retry when writers are idle",
                ));
            }
        }
        if dir.stat().map_err(|e| err(&e.to_string()))? != before {
            return Err(err(
                "Bank paths changed during inspection, including runtime lock state; preserve the existing layout and retry when writers are idle",
            ));
        }
        Ok(())
    }
    let chain = fs::DirectoryChain::open(bank)
        .map_err(|_| err("Cannot completely inspect bank paths; preserve the existing layout"))?;
    let mut paths = Vec::new();
    walk(chain.directory(), "", &mut paths, &mut 0, 0)?;
    chain.recheck().map_err(|e| err(&e.to_string()))?;
    Ok(paths)
}
pub fn file_topology<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let paths: HashSet<_> = paths.into_iter().collect();
    portable_aliases(paths.iter().copied())?;
    for path in &paths {
        let mut parent = *path;
        while let Some((p, _)) = parent.rsplit_once('/') {
            if paths.contains(p) {
                return Err(err("A canonical file cannot also be a parent directory"));
            }
            parent = p;
        }
    }
    Ok(())
}
pub fn validate_files(files: &Value, parsed_records: Option<&mut ParsedRecords>) -> Result<Value> {
    bank_capacity::check_files(files)?;
    if parsed_records.as_ref().is_some_and(|p| !p.is_empty()) {
        return Err(err(
            "Parsed-record output must start empty and cannot supply validation authority",
        ));
    }
    let produce = || -> Result<Value> {
        let mut parsed = ParsedRecords::new();
        let report = validate_files_uncached(files, Some(&mut parsed))?;
        let companion = Value::Object(
            parsed
                .into_iter()
                .map(|(p, (kind, meta, body))| {
                    (p, Value::Array(vec![string(kind), meta, string(body)]))
                })
                .collect(),
        );
        Ok(Value::Array(vec![report, companion]))
    };
    let result = if validation_memo::current().is_some() {
        if let Some((key, _)) = validation_memo::map_input(files) {
            let mut guard = || {
                parse_router(
                    get(files, "MEMORY.md")
                        .as_str()
                        .ok_or_else(|| err("A complete bank requires MEMORY.md"))?,
                )
                .map(|_| ())
            };
            validation_memo::checked("bank-format", &key, produce, Some(&mut guard))?
        } else {
            produce()?
        }
    } else {
        produce()?
    };
    let [report, companion] = result
        .as_array()
        .ok_or_else(|| err("Invalid bank-format validation memo entry"))?
    else {
        return Err(err("Invalid bank-format validation memo entry"));
    };
    if let Some(parsed) = parsed_records {
        let mut checked = ParsedRecords::new();
        for (path, entry) in companion
            .as_object()
            .ok_or_else(|| err("Invalid bank-format validation memo entry"))?
        {
            let [kind, metadata, body] = entry
                .as_array()
                .ok_or_else(|| err("Invalid bank-format validation memo entry"))?
            else {
                return Err(err("Invalid bank-format validation memo entry"));
            };
            checked.insert(
                path.clone(),
                (
                    kind.as_str()
                        .ok_or_else(|| err("Invalid bank-format validation memo entry"))?
                        .to_owned(),
                    metadata.clone(),
                    body.as_str()
                        .ok_or_else(|| err("Invalid bank-format validation memo entry"))?
                        .to_owned(),
                ),
            );
        }
        parsed.extend(checked);
    }
    Ok(report.clone())
}
pub fn validate_files_uncached(
    files: &Value,
    mut parsed_records: Option<&mut ParsedRecords>,
) -> Result<Value> {
    let files_map = files
        .as_object()
        .filter(|_| files.get("MEMORY.md").is_some())
        .ok_or_else(|| err("A complete bank requires MEMORY.md"))?;
    if parsed_records.as_ref().is_some_and(|p| !p.is_empty()) {
        return Err(err(
            "Parsed-record output must start empty and cannot supply validation authority",
        ));
    }
    for (path, body) in files_map {
        if path != "MEMORY.md" {
            path_value(&string(path))?;
        }
        if body.as_str().is_none_or(|s| trim(s).is_empty()) {
            return Err(ContractError::new(format!(
                "Missing or empty routed file: {path}"
            )));
        }
    }
    file_topology(files_map.iter().map(|(p, _)| p.as_str()))?;
    let (metadata, routes, omitted) = parse_router(required_text(
        files,
        "MEMORY.md",
        "A complete bank requires MEMORY.md",
    )?)?;
    for route in &routes {
        let path = required_text(
            route,
            "path",
            "Routes require path, purpose, scope, lifecycle and read-when",
        )?;
        if files.get(path).is_none() {
            return Err(ContractError::new(format!(
                "Missing or empty routed file: {path}"
            )));
        }
    }
    for (path, _) in omitted
        .as_object()
        .ok_or_else(|| err("Every absent context needs exactly one omission reason"))?
    {
        if files.get(path).is_some() {
            return Err(ContractError::new(format!(
                "Context exists but is marked omitted: {path}"
            )));
        }
    }
    let entries = bank_layout::inventory(files, &metadata, &routes, parsed_records.take())?;
    Ok(object([
        ("schema", string("agent-memory/bank-contract-result-v1")),
        ("operation", string("validate")),
        ("valid", Value::Bool(true)),
        (
            "validation_scope",
            string("native-layout-and-source-capacity"),
        ),
        ("project_id", get(&metadata, "project-id").clone()),
        ("router_updated", get(&metadata, "router-updated").clone()),
        ("routes", Value::Array(routes)),
        ("omitted", omitted),
        ("entries", Value::Array(entries)),
        ("writes_performed", Value::Bool(false)),
    ]))
}
pub fn validate(
    repo: &Path,
    owned_lock: Option<&[u8]>,
    owned_transaction: Option<&[u8]>,
) -> Result<Value> {
    let observed = observe(repo, owned_lock, owned_transaction)?;
    let provider = recovery_coverage_provider()?;
    let mut report = observed.checked;
    *report
        .get_mut("validation_scope")
        .ok_or_else(|| err("Observation lacks a validation scope"))? =
        string("canonical-bank-eligibility");
    merge(
        &mut report,
        &provider.public_coverage(&observed.recovery_frontier)?,
    )?;
    merge(
        &mut report,
        &object([("source_stability_verified", Value::Bool(true))]),
    )?;
    Ok(report)
}
pub(crate) fn merge(target: &mut Value, fields: &Value) -> Result<()> {
    let Value::Object(target) = target else {
        return Err(err("Result must be an object"));
    };
    for (k, v) in fields
        .as_object()
        .ok_or_else(|| err("Result fields must be an object"))?
    {
        if let Some((_, old)) = target.iter_mut().find(|(key, _)| key == k) {
            *old = v.clone();
        } else {
            target.push((k.clone(), v.clone()));
        }
    }
    Ok(())
}
pub fn command_render(input: Result<Value>) -> crate::lifecycle::CommandResult {
    command_result("render", input.and_then(|v| render(&v)))
}
pub fn command_validate(repo: &Path) -> crate::lifecycle::CommandResult {
    command_result("validate", validate(repo, None, None))
}
fn command_result(operation: &str, result: Result<Value>) -> crate::lifecycle::CommandResult {
    let (value, exit_code, style) = match result {
        Ok(v) => (v, 0, json::INDENT2_UTF8),
        Err(e) => {
            let mut v = object([
                ("schema", string("agent-memory/bank-contract-result-v1")),
                ("operation", string(operation)),
                ("valid", Value::Bool(false)),
                ("error", string(e.to_string())),
                ("writes_performed", Value::Bool(false)),
            ]);
            // Invariant: both merge operands are constructed objects in this branch.
            merge(&mut v, &bank_capacity::details(&e)).expect("capacity result fields");
            (v, 1, json::DEFAULT)
        }
    };
    let stdout = match json::encode(&value, style) {
        Ok(encoded) => encoded + "\n",
        Err(error) => return command_result(operation, Err(error)),
    };
    crate::lifecycle::CommandResult {
        stdout,
        value: Some(value),
        exit_code,
        stderr: String::new(),
    }
}

pub(crate) fn required_text<'a>(value: &'a Value, key: &str, message: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| err(message))
}
