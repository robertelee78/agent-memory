use agent_memory::codec::json::{self, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

const LEDGER: &str = "tests/obligations/python-baseline-v1.json";
const MAX_LEDGER: u64 = 16 * 1024 * 1024;
const MAX_SOURCE: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Annotation {
    pub file: String,
    pub line: usize,
    pub id: String,
    pub attached: bool,
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{}: exceeds byte limit {limit}", path.display()));
    }
    Ok(bytes)
}

pub fn parse_ledger(bytes: &[u8]) -> Result<Value, String> {
    let policy = json::DecodePolicy {
        duplicate_keys: json::DuplicateKeys::Reject,
        non_finite: json::NonFinite::Reject,
        max_bytes: MAX_LEDGER as usize,
        ..Default::default()
    };
    json::decode_bytes(bytes, policy).map_err(|e| e.to_string())
}

fn members<'a>(ledger: &'a Value, key: &str) -> Result<&'a [Value], String> {
    ledger
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("ledger {key} must be an array"))
}

pub fn annotations(file: &str, source: &str) -> Vec<Annotation> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(id) = line.trim().strip_prefix("// obligation:") else {
            continue;
        };
        let mut attached = false;
        let mut attribute_depth = 0i32;
        for next in lines.iter().skip(index + 1) {
            let text = next.trim();
            if attribute_depth > 0 || text.starts_with("#[") {
                attribute_depth += text.chars().filter(|c| *c == '[').count() as i32;
                attribute_depth -= text.chars().filter(|c| *c == ']').count() as i32;
                continue;
            }
            if text.is_empty() || text.starts_with("//") {
                continue;
            }
            attached = text.starts_with("fn ") || text.starts_with("async fn ");
            break;
        }
        result.push(Annotation {
            file: file.to_owned(),
            line: index + 1,
            id: id.trim().to_owned(),
            attached,
        });
    }
    result
}

pub fn check(ledger: &Value, annotations: &[Annotation]) -> Vec<String> {
    let mut findings = Vec::new();
    if ledger.get("schema").and_then(Value::as_str) != Some("agent-memory/port-obligations-v1") {
        findings.push("invalid ledger schema".into());
    }
    let values = match members(ledger, "status_values") {
        Ok(v) => v,
        Err(e) => {
            findings.push(e);
            return findings;
        }
    };
    let mut allowed = BTreeSet::new();
    for value in values {
        if let Some(s) = value.as_str() {
            allowed.insert(s);
        } else {
            findings.push("non-string status_values entry".into());
        }
    }
    let methods = match members(ledger, "methods") {
        Ok(v) => v,
        Err(e) => {
            findings.push(e);
            return findings;
        }
    };
    let mut ids = BTreeMap::new();
    let mut totals = BTreeMap::<&str, usize>::new();
    let mut registered = 0usize;
    for (index, method) in methods.iter().enumerate() {
        let Some(id) = method.get("id").and_then(Value::as_str) else {
            findings.push(format!("methods[{index}]: missing id"));
            continue;
        };
        let Some(status) = method.get("status").and_then(Value::as_str) else {
            findings.push(format!("{id}: missing status"));
            continue;
        };
        if ids.insert(id, status).is_some() {
            findings.push(format!("{id}: duplicate ledger id"));
        }
        if !allowed.contains(status) {
            findings.push(format!("{id}: invalid status {status}"));
        }
        *totals.entry(status).or_default() += 1;
        // A not-applicable obligation names why no Rust test can exist.
        if status == "not-applicable"
            && !method
                .get("note")
                .and_then(Value::as_str)
                .is_some_and(|note| !note.trim().is_empty())
        {
            findings.push(format!("{id}: not-applicable entry needs a note"));
        }
        match method.get("registered").and_then(Value::as_bool) {
            Some(true) => registered += 1,
            Some(false) => {}
            None => findings.push(format!("{id}: invalid registered")),
        }
    }
    let mut annotated = BTreeSet::new();
    for annotation in annotations {
        annotated.insert(annotation.id.as_str());
        let location = format!("{}:{}: {}", annotation.file, annotation.line, annotation.id);
        if !annotation.attached {
            findings.push(format!(
                "{location}: annotation not directly followed by a function"
            ));
        }
        match ids.get(annotation.id.as_str()) {
            None => findings.push(format!("{location}: unknown annotation id")),
            Some(&"unported") => findings.push(format!("{location}: annotated id is unported")),
            Some(&"not-applicable") => {
                findings.push(format!("{location}: annotated id is not-applicable"))
            }
            _ => {}
        }
    }
    for (id, status) in &ids {
        if matches!(*status, "ported" | "partial") && !annotated.contains(id) {
            findings.push(format!("{id}: {status} entry has no annotation"));
        }
    }
    match ledger.get("counts").and_then(Value::as_object) {
        None => findings.push("counts must be an object".into()),
        Some(counts) => {
            let get = |key: &str| {
                counts
                    .iter()
                    .find(|(k, _)| k == key)
                    .and_then(|(_, v)| v.as_int())
                    .and_then(|n| n.to_string().parse::<usize>().ok())
            };
            if get("registered") != Some(registered) {
                findings.push(format!(
                    "counts.registered disagrees: expected {registered}"
                ));
            }
            let unregistered = methods.len().saturating_sub(registered);
            if get("unregistered") != Some(unregistered) {
                findings.push(format!(
                    "counts.unregistered disagrees: expected {unregistered}"
                ));
            }
            if get("static_methods")
                .zip(get("generated_selectors"))
                .and_then(|(a, b)| a.checked_add(b))
                != Some(methods.len())
            {
                findings.push(format!(
                    "counts static_methods + generated_selectors disagrees: expected {}",
                    methods.len()
                ));
            }
            match ledger
                .get("counts")
                .and_then(|v| v.get("by_status"))
                .and_then(Value::as_object)
            {
                None => findings.push("counts.by_status must be an object".into()),
                Some(actual) => {
                    for status in totals
                        .keys()
                        .copied()
                        .chain(actual.iter().map(|(s, _)| s.as_str()))
                        .collect::<BTreeSet<_>>()
                    {
                        let expected = totals.get(status).copied().unwrap_or(0);
                        let found = actual
                            .iter()
                            .find(|(s, _)| s == status)
                            .and_then(|(_, v)| v.as_int())
                            .and_then(|n| n.to_string().parse::<usize>().ok());
                        if found.unwrap_or(0) != expected
                            || (found.is_none() && actual.iter().any(|(s, _)| s == status))
                        {
                            findings.push(format!(
                                "counts.by_status.{status} disagrees: expected {expected}"
                            ));
                        }
                    }
                }
            }
        }
    }
    findings
}

pub fn load_annotations(manifest: &Path) -> Result<Vec<Annotation>, String> {
    let tests = manifest.join("tests");
    let mut paths = Vec::new();
    for entry in fs::read_dir(&tests).map_err(|e| format!("{}: {e}", tests.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().is_some_and(|s| s == "rs") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let mut result = Vec::new();
    for path in paths {
        let bytes = bounded_read(&path, MAX_SOURCE)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .strip_prefix(manifest)
            .map_err(|e| e.to_string())?
            .display()
            .to_string();
        result.extend(annotations(&name, text));
    }
    Ok(result)
}

pub fn encode_ledger(ledger: &Value) -> Result<String, String> {
    let encoded = json::encode(ledger, json::INDENT2_ASCII).map_err(|e| e.to_string())?;
    let mut result = String::with_capacity(encoded.len());
    for line in encoded.lines() {
        let spaces = line.bytes().take_while(|b| *b == b' ').count();
        result.push_str(&" ".repeat(spaces / 2));
        result.push_str(line.trim_start_matches(' '));
        result.push('\n');
    }
    Ok(result)
}

const USAGE: &str = "usage: obligations summary | obligations mark --status <ported|partial> --ids-file <path> | obligations mark --status not-applicable --ids-file <path> --note <reason>";

fn execute(args: &[String], manifest: &Path) -> Result<(), String> {
    let path = manifest.join(LEDGER);
    let bytes = bounded_read(&path, MAX_LEDGER)?;
    let mut ledger = parse_ledger(&bytes)?;
    match args {
        [command] if command == "summary" => {
            let mut statuses = BTreeMap::<&str, usize>::new();
            let mut files = BTreeMap::<&str, usize>::new();
            for method in members(&ledger, "methods")? {
                let id = method
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("method missing id")?;
                let status = method
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or("method missing status")?;
                *statuses.entry(status).or_default() += 1;
                *files
                    .entry(id.split_once("::").map_or(id, |(file, _)| file))
                    .or_default() += 1;
            }
            println!("By status:");
            for (status, count) in &statuses {
                println!("  {status}: {count}");
            }
            println!("By contract file:");
            for (file, count) in files {
                println!("  {file}: {count}");
            }
            println!(
                "Unported list size: {}",
                statuses.get("unported").copied().unwrap_or(0)
            );
        }
        [command, ..] if command == "mark" => {
            let (status, ids_path, note) = match args {
                [_, flag1, status, flag2, ids_path]
                    if flag1 == "--status"
                        && flag2 == "--ids-file"
                        && matches!(status.as_str(), "ported" | "partial") =>
                {
                    (status, ids_path, None)
                }
                [_, flag1, status, flag2, ids_path, flag3, note]
                    if flag1 == "--status"
                        && status == "not-applicable"
                        && flag2 == "--ids-file"
                        && flag3 == "--note"
                        && !note.trim().is_empty() =>
                {
                    (status, ids_path, Some(note))
                }
                _ => return Err(USAGE.into()),
            };
            let ids_bytes = bounded_read(Path::new(ids_path), MAX_LEDGER)?;
            let ids_text = std::str::from_utf8(&ids_bytes).map_err(|e| e.to_string())?;
            let ids: BTreeSet<&str> = ids_text
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let methods = ledger
                .get_mut("methods")
                .and_then(|v| {
                    if let Value::Array(a) = v {
                        Some(a)
                    } else {
                        None
                    }
                })
                .ok_or("methods must be an array")?;
            let mut found = BTreeSet::new();
            for method in methods.iter_mut() {
                let id = method
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("method missing id")?
                    .to_owned();
                if ids.contains(id.as_str()) {
                    found.insert(id);
                    *method.get_mut("status").ok_or("method missing status")? =
                        Value::String(status.clone());
                    if let Some(note) = note {
                        *method.get_mut("note").ok_or("method missing note")? =
                            Value::String(note.clone());
                    }
                }
            }
            let missing: Vec<_> = ids
                .iter()
                .filter(|id| !found.contains(**id))
                .copied()
                .collect();
            if !missing.is_empty() {
                return Err(format!("unknown ids: {}", missing.join(", ")));
            }
            let mut totals = BTreeMap::<String, usize>::new();
            for method in methods {
                let s = method
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or("method missing status")?;
                *totals.entry(s.to_owned()).or_default() += 1;
            }
            let counts = ledger
                .get_mut("counts")
                .and_then(|v| v.get_mut("by_status"))
                .ok_or("missing counts.by_status")?;
            *counts = Value::Object(
                totals
                    .into_iter()
                    .map(|(s, n)| (s, Value::Int(n.into())))
                    .collect(),
            );
            let output = encode_ledger(&ledger)?;
            if output.as_bytes() != bytes {
                let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)
                    .map_err(|e| format!("{}: {e}", temp.display()))?;
                let write_result = (|| {
                    file.set_permissions(fs::metadata(&path)?.permissions())?;
                    file.write_all(output.as_bytes())?;
                    file.sync_all()?;
                    fs::rename(&temp, &path)
                })();
                if let Err(e) = write_result {
                    let _ = fs::remove_file(&temp);
                    return Err(format!("{}: {e}", path.display()));
                }
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

pub fn run(args: &[String], manifest_dir: &Path) -> i32 {
    match execute(args, manifest_dir) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("obligations: {e}");
            1
        }
    }
}
