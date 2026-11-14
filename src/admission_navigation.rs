//! Agent-reviewed navigation captions and placement, without replacing payloads.
use crate::codec::{frontmatter, json::Value};
use crate::developer_identity::encoded;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::{bank_contract as bank, bank_layout as layout, scopes, security};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CAPTION_BYTES: usize = 512;
pub const MAX_NAVIGATION_BYTES: usize = 16_384;
pub const MAX_INDEXES: usize = 8;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    get(value, field)
        .as_str()
        .ok_or_else(|| err("Invalid navigation text"))
}
fn exact(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == fields.len()
            && fields
                .iter()
                .all(|key| pairs.iter().filter(|(k, _)| k == key).count() == 1)
    })
}
pub fn caption(value: &Value) -> Result<&str> {
    let caption = value
        .as_str()
        .filter(|s| !bank::trim(s).is_empty() && s.len() <= MAX_CAPTION_BYTES)
        .ok_or_else(|| err("Navigation needs a bounded meaningful subject and read trigger"))?;
    if get(&security::scan(value, security::LIMIT)?, "reasons")
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        return Err(err("Unsafe route caption needs clarification"));
    }
    Ok(caption)
}
pub fn placement(value: &Value, index: bool) -> Result<&Value> {
    let fields: &[&str] = if index {
        &["parent", "purpose", "read-when", "path", "scope"]
    } else {
        &["parent", "purpose", "read-when"]
    };
    if !exact(value, fields) {
        return Err(err(
            "Use explicit parent, purpose/read trigger and native index declarations",
        ));
    }
    if get(value, "parent").as_str() != Some("MEMORY.md") {
        bank::path_value(get(value, "parent"))?;
    }
    caption(get(value, "purpose"))?;
    caption(get(value, "read-when"))?;
    if index {
        let path = bank::path_value(get(value, "path"))?;
        if !path.starts_with("topics/") || bank::CONTEXTS.contains(&path) {
            return Err(err(
                "New topic nodes stay in the native topic-index namespace",
            ));
        }
        let scope = get(value, "scope");
        if scopes::parse_scope(scope)?.is_none() && scope.as_str() != Some("repository") {
            return Err(err("Declare an actual inherited topic scope"));
        }
    }
    Ok(value)
}
pub fn navigation_value(value: &Value) -> Result<&Value> {
    if !exact(value, &["learning", "history", "indexes"])
        || !get(value, "indexes")
            .as_array()
            .is_some_and(|a| a.len() <= MAX_INDEXES)
        || encoded(value)?.len() > MAX_NAVIGATION_BYTES
    {
        return Err(err(
            "Use a small explicit reviewed learning/history navigation plan",
        ));
    }
    placement(get(value, "learning"), false)?;
    placement(get(value, "history"), false)?;
    let mut paths = BTreeSet::new();
    for node in get(value, "indexes").as_array().into_iter().flatten() {
        placement(node, true)?;
        if !paths.insert(text(node, "path")?) {
            return Err(err("New topic nodes require distinct canonical paths"));
        }
    }
    Ok(value)
}
pub fn route(path: &str, plan: &Value, scope: &Value, state: &str) -> Value {
    object([
        ("path", string(path)),
        ("purpose", get(plan, "purpose").clone()),
        ("scope", scope.clone()),
        ("lifecycle", string(state)),
        ("read-when", get(plan, "read-when").clone()),
    ])
}
fn add(tables: &mut BTreeMap<String, Vec<Value>>, parent: &str, item: Value) -> Result<()> {
    let table = tables.get_mut(parent).ok_or_else(|| {
        err("The chosen parent must be an actual existing or declared topic node")
    })?;
    if table
        .iter()
        .any(|row| get(row, "path") == get(&item, "path"))
    {
        return Err(err("A new route must not replace an existing route"));
    }
    table.push(item);
    Ok(())
}
fn set(metadata: &mut frontmatter::Metadata, key: &str, value: Value) {
    if let Some((_, prior)) = metadata.iter_mut().find(|(k, _)| k == key) {
        *prior = value;
    } else {
        metadata.push((key.into(), value));
    }
}
pub fn apply(
    files: &Value,
    navigation: &Value,
    learning: &str,
    history: &str,
    metadata: &Value,
    stamp: &str,
) -> Result<Value> {
    navigation_value(navigation)?;
    let mut current = files.clone();
    let (root_meta, root_routes, _) = bank::parse_router(text(files, "MEMORY.md")?)?;
    let mut headers = BTreeMap::from([("MEMORY.md".to_owned(), root_meta)]);
    let Value::Object(current_pairs) = &mut current else {
        return Err(err("A complete bank requires MEMORY.md"));
    };
    let mut tables = BTreeMap::from([("MEMORY.md".to_owned(), root_routes)]);
    let report = bank::validate_files(files, None)?;
    let index_fields: Vec<&str> = get(&layout::SPEC, "properties")
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.as_str())
        .collect();
    for entry in get(&report, "entries").as_array().into_iter().flatten() {
        if get(entry, "format").as_str() == Some("topic-index") {
            let path = text(entry, "path")?;
            let (meta, body) = frontmatter::read(text(files, path)?, &index_fields)?;
            let meta = Value::Object(meta);
            tables.insert(path.into(), layout::parse_index(path, &meta, body)?);
            headers.insert(path.into(), meta);
        }
    }
    let mut new = BTreeSet::new();
    let indexes = get(navigation, "indexes")
        .as_array()
        .ok_or_else(|| err("Invalid navigation indexes"))?;
    for node in indexes {
        let path = text(node, "path")?;
        if files.get(path).is_some() || tables.contains_key(path) {
            return Err(err(
                "New index declaration must not replace an existing source",
            ));
        }
        headers.insert(
            path.into(),
            object([
                ("schema", get(&layout::SPEC, "$id").clone()),
                ("project-id", get(metadata, "project-id").clone()),
                ("updated", string(stamp)),
            ]),
        );
        tables.insert(path.into(), Vec::new());
        new.insert(path.to_owned());
    }
    let mut touched = new.clone();
    for node in indexes {
        let parent = text(node, "parent")?;
        add(
            &mut tables,
            parent,
            route(text(node, "path")?, node, get(node, "scope"), "active"),
        )?;
        touched.insert(parent.into());
    }
    for (key, path, scope) in [
        ("learning", learning, get(metadata, "scope").clone()),
        ("history", history, string("repository")),
    ] {
        let plan = get(navigation, key);
        let parent = text(plan, "parent")?;
        add(&mut tables, parent, route(path, plan, &scope, "active"))?;
        touched.insert(parent.into());
    }
    let allowed: BTreeSet<&str> = get(&bank::SPEC, "properties")
        .as_object()
        .into_iter()
        .flatten()
        .chain(
            get(&layout::SPEC, "properties")
                .as_object()
                .into_iter()
                .flatten(),
        )
        .map(|(k, _)| k.as_str())
        .collect();
    let allowed: Vec<&str> = allowed.into_iter().collect();
    for path in touched {
        let rows = tables
            .get(&path)
            .ok_or_else(|| err("Missing navigation table"))?;
        let content = if new.contains(&path) {
            layout::index_text(
                &path,
                headers
                    .get(&path)
                    .ok_or_else(|| err("Missing navigation header"))?,
                rows,
            )?
        } else {
            let (mut meta, body) = frontmatter::read(text(files, &path)?, &allowed)?;
            let (prefix, tail) = body
                .split_once("\n## Routes\n")
                .ok_or_else(|| err("Missing navigation Routes section"))?;
            let body = if path == "MEMORY.md" {
                let (_, omitted_tail) = tail
                    .split_once("\n## Omitted contexts\n")
                    .ok_or_else(|| err("Missing navigation Omitted contexts section"))?;
                set(&mut meta, "router-updated", string(stamp));
                format!(
                    "{prefix}\n## Routes\n\n{}\n## Omitted contexts\n{omitted_tail}",
                    bank::route_table(rows, &path)?
                )
            } else {
                set(&mut meta, "updated", string(stamp));
                format!("{prefix}\n## Routes\n\n{}", bank::route_table(rows, &path)?)
            };
            frontmatter::write(&meta)? + &body
        };
        if let Some((_, value)) = current_pairs.iter_mut().find(|(k, _)| k == &path) {
            *value = string(content);
        } else {
            current_pairs.push((path, string(content)));
        }
    }
    Ok(current)
}
