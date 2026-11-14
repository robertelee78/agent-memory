//! Source-bound normative guide evidence; changed meaning remains a reviewer assertion.
use agent_memory::codec::json::{self, Value};
use agent_memory::fs::{self, Descriptor, DirectoryChain};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

type Result<T, E = String> = std::result::Result<T, E>;
const SCHEMA: &str = "agent-memory/guide-evidence-v1";
const MAX_GUIDE: usize = 262_144;
const MAX_INPUT: usize = 4_194_304;
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 64;
static TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: this constant expression is a valid ASCII-token regex.
    Regex::new(r"(?-u:\b)(?:MUST[ \t\r\n\x0b\x0c]+NOT|SHALL[ \t\r\n\x0b\x0c]+NOT|SHOULD[ \t\r\n\x0b\x0c]+NOT|NOT[ \t\r\n\x0b\x0c]+RECOMMENDED|MUST|SHALL|REQUIRED|SHOULD|RECOMMENDED|MAY|OPTIONAL)(?-u:\b)").expect("constant token regex")
});
static LINKS: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: this constant expression is a valid Markdown-link regex.
    Regex::new(r"\[[^\]]*\]\(([^\n)]+)\)").expect("constant link regex")
});
fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn number(value: usize) -> Value {
    Value::Int(value.into())
}
fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| format!("missing {key}"))
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .ok_or_else(|| format!("invalid {key}"))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    field(value, key)?
        .as_array()
        .ok_or_else(|| format!("invalid {key}"))
}
fn members(value: &Value) -> Result<&[(String, Value)]> {
    value.as_object().ok_or_else(|| "Expected object".into())
}
fn require(condition: bool) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err("Invalid guide evidence".into())
    }
}
fn hash(raw: &[u8]) -> String {
    Sha256::digest(raw)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn forces(text: &str) -> Value {
    let normalized: String = text
        .chars()
        .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
        .collect();
    Value::Array(
        TOKENS
            .find_iter(&normalized)
            .filter(|matched| {
                let word = |ch: char| ch.is_alphanumeric() || ch == '_';
                !normalized
                    .get(..matched.start())
                    .and_then(|prefix| prefix.chars().next_back())
                    .is_some_and(word)
                    && !normalized
                        .get(matched.end()..)
                        .and_then(|suffix| suffix.chars().next())
                        .is_some_and(word)
            })
            .map(|matched| {
                let token = matched
                    .as_str()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let polarity = if token.contains(' ') {
                    "negative"
                } else {
                    "positive"
                };
                let strength = match token.as_str() {
                    "MUST" | "SHALL" | "REQUIRED" | "MUST NOT" | "SHALL NOT" => 3,
                    "SHOULD" | "RECOMMENDED" | "SHOULD NOT" | "NOT RECOMMENDED" => 2,
                    _ => 1,
                };
                Value::Array(vec![string(polarity), number(strength)])
            })
            .collect(),
    )
}
fn outside_code(text: &str) -> Result<String> {
    let mut lines = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in text.lines() {
        let spaces = line.bytes().take_while(|byte| *byte == b' ').count();
        let candidate = line.get(spaces..).ok_or("Invalid line")?;
        if let Some((character, length)) = fence {
            let count = candidate.chars().take_while(|ch| *ch == character).count();
            if spaces <= 3
                && count >= length
                && candidate
                    .get(count..)
                    .is_some_and(|tail| tail.chars().all(|ch| ch == ' ' || ch == '\t'))
            {
                fence = None;
            }
            lines.push("");
            continue;
        }
        let character = candidate.chars().next().unwrap_or(' ');
        let length = candidate.chars().take_while(|ch| *ch == character).count();
        if spaces <= 3
            && ['`', '~'].contains(&character)
            && length >= 3
            && !(character == '`'
                && candidate
                    .get(length..)
                    .is_some_and(|tail| tail.contains('`')))
        {
            fence = Some((character, length));
            lines.push("");
        } else {
            lines.push(line);
        }
    }
    require(fence.is_none())?;
    Ok(lines.join("\n"))
}
fn paths(
    dir: &Descriptor,
    relative: &Path,
    found: &mut Vec<PathBuf>,
    count: &mut usize,
) -> Result<()> {
    require(relative.components().count() <= MAX_DEPTH)?;
    let before = dir.stat().map_err(|error| error.to_string())?;
    let entries = dir
        .entries_bounded(MAX_ENTRIES.saturating_sub(*count))
        .map_err(|error| error.to_string())?
        .ok_or("Guide inventory exceeds its bound")?;
    *count += entries.len();
    for name in entries {
        let path = relative.join(&name);
        let pin = dir.stat_at(&name).map_err(|error| error.to_string())?;
        if pin.is_directory() {
            let child = dir
                .open_directory_at(&name)
                .map_err(|error| error.to_string())?;
            require(child.stat().map_err(|error| error.to_string())? == pin)?;
            paths(&child, &path, found, count)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            require(pin.is_regular())?;
            found.push(path);
        }
        require(dir.stat_at(&name).map_err(|error| error.to_string())? == pin)?;
    }
    require(dir.stat().map_err(|error| error.to_string())? == before)
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let parent = path.parent().ok_or("Invalid input path")?;
    let name = path.file_name().ok_or("Invalid input name")?;
    fs::read_regular(parent, Path::new(name), maximum)
        .map(|(raw, _)| raw)
        .map_err(|error| error.to_string())
}
fn decode(path: &Path) -> Result<Value> {
    json::decode_bytes(&read(path, MAX_INPUT)?, Default::default())
        .map_err(|error| error.to_string())
}
fn unquote(path: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(path.len());
    let mut source = path.bytes();
    while let Some(byte) = source.next() {
        if byte == b'%' {
            let high = source
                .next()
                .and_then(|byte| (byte as char).to_digit(16))
                .ok_or("Malformed escaped route")?;
            let low = source
                .next()
                .and_then(|byte| (byte as char).to_digit(16))
                .ok_or("Malformed escaped route")?;
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}
pub fn snapshot(pack: &Path) -> Result<Value> {
    let pack = fs::canonical_path(pack, true).map_err(|error| error.to_string())?;
    let chain = DirectoryChain::open(&pack).map_err(|error| error.to_string())?;
    let mut guides = Vec::new();
    paths(chain.directory(), Path::new(""), &mut guides, &mut 0)?;
    guides.sort();
    require(guides.iter().any(|path| path == Path::new("SKILL.md")))?;
    let mut files = Vec::new();
    let mut rules = Vec::new();
    let mut routes = BTreeMap::new();
    let mut total = 0;
    for path in guides {
        let raw = read(&pack.join(&path), MAX_GUIDE)?;
        total += raw.len();
        require(total <= MAX_INPUT)?;
        let text = outside_code(std::str::from_utf8(&raw).map_err(|error| error.to_string())?)?;
        let relative = path.to_str().ok_or("Non-UTF-8 guide path")?.to_owned();
        files.push((relative.clone(), string(hash(&raw))));
        let mut start = 0;
        let mut paragraph = Vec::new();
        let mut collect = |paragraph: &mut Vec<&str>, line: usize| {
            let body = paragraph.join("\n").trim().to_owned();
            paragraph.clear();
            let force = forces(&body);
            if force.as_array().is_some_and(|values| !values.is_empty()) {
                let key = hash(format!("{relative}\0{line}\0{body}").as_bytes());
                rules.push((
                    key,
                    object([
                        ("path", string(&relative)),
                        ("line", number(line)),
                        ("text", string(body)),
                        ("force", force),
                    ]),
                ));
            }
        };
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                collect(&mut paragraph, start);
            } else {
                if paragraph.is_empty() {
                    start = index + 1;
                }
                paragraph.push(line);
            }
        }
        collect(&mut paragraph, start);
        let mut links = Vec::new();
        for matched in LINKS.captures_iter(&text) {
            let whole = matched.get(0).ok_or("Invalid link")?;
            if text
                .get(..whole.start())
                .is_some_and(|prefix| prefix.ends_with('!'))
            {
                continue;
            }
            let raw = matched.get(1).ok_or("Invalid link target")?.as_str().trim();
            let raw = raw
                .strip_prefix('<')
                .and_then(|value| value.strip_suffix('>'))
                .unwrap_or(raw);
            let route = raw.split(['?', '#']).next().unwrap_or("");
            if route.is_empty()
                || route.starts_with("//")
                || route
                    .split('/')
                    .next()
                    .is_some_and(|first| first.contains(':'))
            {
                continue;
            }
            let destination = fs::canonical_path(
                &pack
                    .join(path.parent().ok_or("Invalid guide path")?)
                    .join(unquote(route)?),
                true,
            )
            .map_err(|error| error.to_string())?;
            require(destination.starts_with(&pack))?;
            let parent = DirectoryChain::open(destination.parent().ok_or("Invalid route")?)
                .map_err(|error| error.to_string())?;
            require(
                parent
                    .directory()
                    .stat_at(destination.file_name().ok_or("Invalid route")?)
                    .map_err(|error| error.to_string())?
                    .is_regular(),
            )?;
            if destination
                .extension()
                .is_some_and(|extension| extension == "md")
            {
                links.push(
                    destination
                        .strip_prefix(&pack)
                        .map_err(|error| error.to_string())?
                        .to_str()
                        .ok_or("Non-UTF-8 route")?
                        .to_owned(),
                );
            }
        }
        routes.insert(relative, links);
    }
    let mut reached = BTreeSet::new();
    let mut pending = vec!["SKILL.md".to_owned()];
    while let Some(path) = pending.pop() {
        if reached.insert(path.clone()) {
            pending.extend(
                routes
                    .get(&path)
                    .ok_or("Route is outside the guide inventory")?
                    .iter()
                    .cloned(),
            );
        }
    }
    require(reached.len() == files.len())?;
    chain.recheck().map_err(|error| error.to_string())?;
    Ok(object([
        ("schema", string(SCHEMA)),
        ("files", Value::Object(files)),
        ("rules", Value::Object(rules)),
        (
            "routes",
            Value::Object(
                routes
                    .into_iter()
                    .map(|(key, value)| (key, strings(value)))
                    .collect(),
            ),
        ),
        ("reachable", strings(reached)),
        ("source_origin_authenticated", Value::Bool(false)),
        ("semantic_equivalence_verified", Value::Bool(false)),
    ]))
}
fn sufficient(old: &[Value], new: &[Value]) -> Result<bool> {
    let mut available: BTreeMap<(String, usize), usize> = BTreeMap::new();
    let parse = |value: &Value| -> Result<(String, usize)> {
        let values = value.as_array().ok_or("Invalid force")?;
        require(values.len() == 2)?;
        let polarity = values
            .first()
            .and_then(Value::as_str)
            .ok_or("Invalid polarity")?
            .to_owned();
        let strength = values
            .get(1)
            .and_then(Value::as_int)
            .and_then(|value| value.to_string().parse().ok())
            .ok_or("Invalid force strength")?;
        Ok((polarity, strength))
    };
    for value in new {
        *available.entry(parse(value)?).or_default() += 1;
    }
    let mut required = old.iter().map(parse).collect::<Result<Vec<_>>>()?;
    required.sort_by(|left, right| right.1.cmp(&left.1));
    for (polarity, strength) in required {
        let mut consumed = false;
        for candidate in strength..=3 {
            if let Some(count) = available.get_mut(&(polarity.clone(), candidate))
                && *count > 0
            {
                *count -= 1;
                consumed = true;
                break;
            }
        }
        if !consumed {
            return Ok(false);
        }
    }
    Ok(true)
}
fn check(before: &Value, after: &Value, mapping: &Value) -> Result<Value> {
    require(text(before, "schema")? == SCHEMA)?;
    let old_rules = members(field(before, "rules")?)?;
    let new_rules = field(after, "rules")?;
    require(
        members(mapping)?
            .iter()
            .all(|(key, _)| old_rules.iter().any(|(old, _)| old == key)),
    )?;
    let mut coverage = Vec::new();
    for (key, old) in old_rules {
        let old_text = text(old, "text")?;
        require(members(old)?.len() == 4 && field(old, "force")? == &forces(old_text))?;
        let line = field(old, "line")?
            .as_int()
            .ok_or("Invalid line")?
            .to_string();
        require(*key == hash(format!("{}\0{line}\0{old_text}", text(old, "path")?).as_bytes()))?;
        let (selected, method) = if let Some(entry) = mapping.get(key) {
            require(
                members(entry)?.len() == 3
                    && !text(entry, "reason")?.trim().is_empty()
                    && field(entry, "meaning_preserved")? == &Value::Bool(true),
            )?;
            let selected = array(entry, "new_rules")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or("Invalid new rule".into())
                })
                .collect::<Result<Vec<_>>>()?;
            require(
                !selected.is_empty()
                    && selected.iter().collect::<BTreeSet<_>>().len() == selected.len(),
            )?;
            (selected, "reviewed-wording-map")
        } else {
            let target = members(new_rules)?
                .iter()
                .find(|(_, rule)| rule.get("text").and_then(Value::as_str) == Some(old_text))
                .ok_or("Missing preservation map")?;
            (vec![target.0.clone()], "identical-text")
        };
        let mut force = Vec::new();
        for key in &selected {
            force.extend_from_slice(array(field(new_rules, key)?, "force")?);
        }
        require(sufficient(array(old, "force")?, &force)?)?;
        coverage.push(object([
            ("old_rule", string(key)),
            ("new_rules", strings(selected)),
            ("method", string(method)),
        ]));
    }
    Ok(object([
        ("schema", string("agent-memory/guide-refactor-result-v1")),
        ("passed", Value::Bool(true)),
        ("rule_map", Value::Array(coverage)),
        ("old_rules", number(old_rules.len())),
        ("new_rules", number(members(new_rules)?.len())),
        ("reachable_guides", field(after, "reachable")?.clone()),
        ("semantic_equivalence_verified", Value::Bool(false)),
        ("changed_meaning_is_reviewer_assertion", Value::Bool(true)),
        ("prior_source_origin_authenticated", Value::Bool(false)),
        ("writes_performed", Value::Bool(false)),
    ]))
}
fn command(args: &[String]) -> Result<Value> {
    let (command, rest) = args.split_first().ok_or("Missing operation")?;
    require(rest.len() % 2 == 0)?;
    let mut options = BTreeMap::new();
    for pair in rest.chunks_exact(2) {
        let key = pair.first().ok_or("Missing option")?;
        require(["--pack", "--before", "--before-pack", "--map"].contains(&key.as_str()))?;
        require(
            options
                .insert(
                    key.as_str(),
                    pair.get(1).ok_or("Missing option value")?.as_str(),
                )
                .is_none(),
        )?;
    }
    let required = |key: &str| -> Result<&Path> {
        options
            .get(key)
            .map(Path::new)
            .ok_or_else(|| format!("Missing {key}"))
    };
    let after = snapshot(required("--pack")?)?;
    if command == "snapshot" {
        require(options.len() == 1)?;
        return Ok(after);
    }
    require(command == "check")?;
    let before = decode(required("--before")?)?;
    require(before == snapshot(required("--before-pack")?)?)?;
    let mapping = options
        .get("--map")
        .map(|path| decode(Path::new(path)))
        .transpose()?
        .unwrap_or(Value::Object(vec![]));
    check(&before, &after, &mapping)
}
pub fn run(args: &[String]) -> i32 {
    let (code, value) = match command(args) {
        Ok(value) => (0, value),
        Err(_) => (
            1,
            object([
                ("schema", string("agent-memory/guide-refactor-result-v1")),
                ("passed", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                (
                    "error",
                    string(
                        "Missing, changed, unreachable or weakened guide evidence; preserve source and review the complete map",
                    ),
                ),
            ]),
        ),
    };
    match json::encode(&value, json::INDENT2_ASCII) {
        Ok(raw) => println!("{raw}"),
        Err(error) => {
            eprintln!("am-dev guide-evidence: {error}");
            return 1;
        }
    }
    code
}
