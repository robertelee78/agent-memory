//! Explicit changed-learning selection; judgments remain fallible assertions.
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, sha256_hex, uuid_valid};
use crate::error::Result;
use crate::lifecycle::{get, object, string};
use crate::semantic_sharing::{
    self as semantic, Rows, Snapshot, array, container, entries, err, exact, fields, hash,
    metadata, nonnull, set, strings, text,
};
use crate::{
    bank_contract as bank, records, retrieval, security, sharing_hold_mapping as mapping,
    sharing_holds as holds, sharing_limits as limits,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
pub const REQUEST: &str = "agent-memory/semantic-sharing-request-v2";
pub const INSPECTION: &str = "agent-memory/semantic-sharing-inspection-v2";
pub const JUDGMENT: &str = "agent-memory/semantic-sharing-judgment-v2";
pub const PROOF: &str = "agent-memory/semantic-sharing-proof-v2";
pub const MAX_EDGES: usize = 262144;
pub fn add_edges(
    edges: &mut BTreeSet<(String, String)>,
    values: impl IntoIterator<Item = (String, String)>,
) -> Result<()> {
    for edge in values {
        if !edges.contains(&edge) && edges.len() >= MAX_EDGES {
            return Err(err(
                "Complete review exceeds 262,144 interactions; preserve the frontier without truncation",
            ));
        }
        edges.insert(edge);
    }
    Ok(())
}
pub fn semantic_metadata(value: &Value) -> Result<Value> {
    Ok(Value::Object(
        fields(value)?
            .iter()
            .filter(|(key, _)| {
                !["updated", "recorded-at", "router-updated"].contains(&key.as_str())
            })
            .cloned()
            .collect(),
    ))
}
pub fn components(values: &Value, checked: &Value) -> Result<Rows> {
    let mut rows = Rows::new();
    for side in ["base", "local", "landed"] {
        let files = get(values, side);
        if fields(files)?.is_empty() {
            continue;
        }
        let all_entries = entries(get(checked, side))?;
        let mut containers = BTreeMap::new();
        for entry in &all_entries {
            let path = text(get(entry, "path"))?;
            if path == "MEMORY.md" || get(entry, "format").as_str() == Some("topic-index") {
                containers.insert(path.to_owned(), container(path, text(get(files, path))?)?);
            }
        }
        let mut framing = BTreeMap::from([("MEMORY.md".to_owned(), Vec::<Value>::new())]);
        let mut pending = vec!["MEMORY.md".to_owned()];
        let mut visited = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if !visited.insert(parent.clone()) {
                return Err(err("Ambiguous effective semantic framing"));
            }
            let (meta, routes, intro, _) = containers
                .get(&parent)
                .ok_or_else(|| err("Missing semantic container"))?;
            for route in routes {
                let child = text(get(route, "path"))?;
                let mut chain = framing
                    .get(&parent)
                    .ok_or_else(|| err("Missing semantic framing"))?
                    .clone();
                chain.push(object([
                    ("container_path", string(&parent)),
                    ("metadata", semantic_metadata(meta)?),
                    ("introduction", string(intro)),
                    ("route", route.clone()),
                ]));
                if framing.get(child).is_some_and(|old| {
                    !retrieval::same(&Value::Array(old.clone()), &Value::Array(chain.clone()))
                }) {
                    return Err(err("Ambiguous effective semantic framing"));
                }
                framing.insert(child.into(), chain);
                if containers.contains_key(child) {
                    pending.push(child.into());
                }
            }
        }
        for entry in all_entries {
            let path = text(get(&entry, "path"))?;
            let content = text(get(files, path))?;
            let format = text(get(&entry, "format"))?;
            let mut add = |body: String,
                           meta: Value,
                           component: &str,
                           target: Value,
                           context: Value,
                           witness: Value,
                           chain: Value|
             -> Result<()> {
                let logical = object([
                    ("path", string(path)),
                    ("component", string(component)),
                    ("target_path", target.clone()),
                ]);
                let mut effective = framing
                    .get(path)
                    .ok_or_else(|| err("Missing semantic framing"))?
                    .clone();
                if component == "route-prose" {
                    effective.push(object([("route", get(&context, "route").clone())]));
                }
                let effective = Value::Array(effective);
                let meaning = object([
                    ("logical", logical.clone()),
                    ("body", string(&body)),
                    (
                        "metadata",
                        if nonnull(&meta) {
                            semantic_metadata(&meta)?
                        } else {
                            Value::Null
                        },
                    ),
                    ("framing", effective.clone()),
                    ("lifecycle", get(&entry, "lifecycle").clone()),
                    (
                        "component_metadata",
                        if nonnull(&context) {
                            semantic_metadata(get(&context, "metadata"))?
                        } else {
                            Value::Null
                        },
                    ),
                    ("witness", witness.clone()),
                ]);
                let key = hash(&meaning)?;
                let sha = sha256_hex(content.as_bytes());
                let times = if nonnull(&meta) {
                    Value::Object(
                        fields(&meta)?
                            .iter()
                            .filter(|(key, _)| ["updated", "recorded-at"].contains(&key.as_str()))
                            .cloned()
                            .collect(),
                    )
                } else {
                    object([] as [(&str, Value); 0])
                };
                if let Some(row) = rows.get_mut(&key) {
                    let mut sides = array(get(row, "sides"))?.to_vec();
                    sides.push(string(side));
                    set(row, "sides", Value::Array(sides))?;
                    for (name, value) in [
                        ("source_sha256_by_side", string(&sha)),
                        ("scope_chains", chain.clone()),
                        ("container_variants", context.clone()),
                        ("observation_times_by_side", times),
                    ] {
                        let mut map = get(row, name).clone();
                        set(&mut map, side, value)?;
                        set(row, name, map)?;
                    }
                    if side == "local" || !array(get(row, "sides"))?.contains(&string("local")) {
                        for (name, value) in [
                            ("sha256", string(sha)),
                            ("metadata", meta),
                            ("scope_chain", chain),
                            ("container", context),
                        ] {
                            set(row, name, value)?;
                        }
                    }
                } else {
                    let mut row = logical;
                    for (name, value) in [
                        ("key", string(&key)),
                        (
                            "logical_key",
                            string(hash(&object([
                                ("path", string(path)),
                                ("component", string(component)),
                                ("target_path", target),
                            ]))?),
                        ),
                        ("sha256", string(&sha)),
                        ("source_sha256_by_side", object([(side, string(sha))])),
                        ("sides", strings([side])),
                        ("metadata", meta),
                        ("body", string(body)),
                        ("scope_chain", chain.clone()),
                        ("scope_chains", object([(side, chain)])),
                        ("effective_framing", effective),
                        ("lifecycle", get(&entry, "lifecycle").clone()),
                        ("format", string(format)),
                        ("container", context.clone()),
                        ("container_variants", object([(side, context)])),
                        ("witness_metadata", witness),
                        ("observation_times_by_side", object([(side, times)])),
                    ] {
                        set(&mut row, name, value)?;
                    }
                    rows.insert(key, row);
                }
                Ok(())
            };
            let chain = get(&entry, "scope_chain").clone();
            match format {
                "standalone-record" => {
                    let (_, meta, body) = records::parse(content, &[])?;
                    add(
                        body,
                        meta,
                        "body",
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        chain,
                    )?;
                }
                "context-prose" => add(
                    content.into(),
                    Value::Null,
                    "body",
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    chain,
                )?,
                "root-router" | "topic-index" => {
                    let (meta, routes, intro, omitted) = containers
                        .get(path)
                        .ok_or_else(|| err("Missing semantic container"))?;
                    let mut context = object([
                        ("metadata", meta.clone()),
                        ("routes", Value::Array(routes.clone())),
                    ]);
                    if path == "MEMORY.md" {
                        set(&mut context, "omitted", omitted.clone())?;
                    }
                    add(
                        intro.clone(),
                        Value::Null,
                        "introduction",
                        Value::Null,
                        context,
                        Value::Null,
                        chain.clone(),
                    )?;
                    for route in routes {
                        let mut chain = array(&chain)?.to_vec();
                        chain.push(get(route, "scope").clone());
                        add(
                            format!(
                                "Purpose: {}\nRead when: {}\n",
                                text(get(route, "purpose"))?,
                                text(get(route, "read-when"))?
                            ),
                            Value::Null,
                            "route-prose",
                            get(route, "path").clone(),
                            object([("metadata", meta.clone()), ("route", route.clone())]),
                            Value::Null,
                            Value::Array(chain),
                        )?;
                    }
                    for (target, reason) in fields(omitted)? {
                        add(
                            text(reason)?.into(),
                            Value::Null,
                            "omission",
                            string(target),
                            object([("metadata", meta.clone()), ("context_path", string(target))]),
                            Value::Null,
                            chain.clone(),
                        )?;
                    }
                }
                "tombstone" | "optional-history-event" => {
                    let (_, meta, body) = records::parse(content, &[])?;
                    add(
                        body,
                        Value::Null,
                        "body",
                        Value::Null,
                        Value::Null,
                        meta,
                        chain,
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(rows)
}
pub fn changed_components(rows: &Rows, audit_paths: &BTreeSet<String>) -> Result<BTreeSet<String>> {
    let mut logical = BTreeMap::<String, Vec<&String>>::new();
    for (key, row) in rows {
        logical
            .entry(text(get(row, "logical_key"))?.into())
            .or_default()
            .push(key);
    }
    let mut changed = BTreeSet::new();
    for keys in logical.values() {
        let versions = ["base", "local", "landed"].map(|side| {
            keys.iter()
                .filter(|key| {
                    rows.get(**key).is_some_and(|row| {
                        get(row, "sides")
                            .as_array()
                            .is_some_and(|sides| sides.contains(&string(side)))
                    })
                })
                .map(|key| (*key).clone())
                .collect::<BTreeSet<_>>()
        });
        if versions.first() != versions.get(1)
            || versions.first() != versions.get(2)
            || keys.iter().any(|key| {
                rows.get(*key)
                    .and_then(|row| get(row, "path").as_str())
                    .is_some_and(|path| audit_paths.contains(path))
            })
        {
            changed.extend(keys.iter().map(|key| (*key).clone()));
        }
    }
    Ok(changed)
}
pub fn identities(row: &Value) -> Result<BTreeSet<String>> {
    let meta = metadata(row);
    let mut result = BTreeSet::new();
    if let Some(id) = get(meta, "id").as_str() {
        result.insert(id.into());
    }
    for name in ["corrects", "supersedes", "superseded-by", "derived-from"] {
        if let Some(refs) = get(meta, name).as_array() {
            for reference in refs {
                result.insert(text(get(reference, "id"))?.into());
            }
        }
    }
    if let Some(id) = get(get(meta, "record"), "id").as_str() {
        result.insert(id.into());
    }
    let parts: Vec<&str> = text(get(row, "path"))?.split('/').collect();
    if let ["history", "sources", id, _] = parts.as_slice() {
        if uuid_valid(&string(*id)) {
            result.insert((*id).into());
        }
    }
    Ok(result)
}
pub fn request_value(request: Option<&Value>) -> Result<Value> {
    let Some(request) = request.filter(|v| nonnull(v)) else {
        return Ok(object([
            ("schema", string(REQUEST)),
            ("base", Value::Null),
            ("selections", object([] as [(&str, Value); 0])),
            ("audit_paths", strings([] as [&str; 0])),
            ("dependencies", object([] as [(&str, Value); 0])),
            ("consolidation_sources", strings([] as [&str; 0])),
        ]));
    };
    if !exact(
        request,
        &[
            "schema",
            "base",
            "selections",
            "audit_paths",
            "dependencies",
            "consolidation_sources",
        ],
    ) || get(request, "schema").as_str() != Some(REQUEST)
    {
        return Err(err("Use an explicit complete v2 selection request"));
    }
    if get(request, "selections").as_object().is_none()
        || get(request, "audit_paths").as_array().is_none()
        || unique_strings(get(request, "audit_paths")).is_err()
    {
        return Err(err(
            "Review requires unique actual audit paths and per-change selectors",
        ));
    }
    for path in array(get(request, "audit_paths"))? {
        if path.as_str() != Some("MEMORY.md") {
            bank::path_value(path)?;
        }
    }
    if get(request, "dependencies").as_object().is_none()
        || get(request, "consolidation_sources")
            .as_array()
            .is_none_or(|v| v.len() > 8)
    {
        return Err(err(
            "Declare actual dependency paths and consolidation source groups",
        ));
    }
    limits::private_bytes(request)?;
    Ok(request.clone())
}
fn unique_strings(value: &Value) -> Result<BTreeSet<String>> {
    let values = array(value)?;
    let result = values
        .iter()
        .map(|v| Ok(text(v)?.to_owned()))
        .collect::<Result<BTreeSet<_>>>()?;
    if result.len() != values.len() {
        return Err(err("Expected unique exact source selectors"));
    }
    Ok(result)
}
fn edge(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    }
}
// Python repr is part of ordinary-text selector input, not a disclosure codec.
fn repr(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Null => "None".into(),
        Value::Bool(v) => if *v { "True" } else { "False" }.into(),
        Value::String(v) => {
            let quote = if v.contains('\'') && !v.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut out = String::from(quote);
            for c in v.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if c == quote => {
                        out.push('\\');
                        out.push(c);
                    }
                    c => out.push(c),
                }
            }
            out.push(quote);
            out
        }
        Value::Array(v) => format!(
            "[{}]",
            v.iter().map(repr).collect::<Result<Vec<_>>>()?.join(", ")
        ),
        Value::Object(v) => format!(
            "{{{}}}",
            v.iter()
                .map(|(k, v)| Ok(format!("{}: {}", repr(&string(k))?, repr(v)?)))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        _ => json::encode(value, json::DEFAULT)?,
    })
}
pub type Selection = (Rows, Value, Value, Vec<String>);
pub fn selected(rows: &Rows, changed: &BTreeSet<String>, request: &Value) -> Result<Selection> {
    if fields(get(request, "selections"))?
        .iter()
        .any(|(key, _)| !changed.contains(key))
    {
        return Err(err(
            "Selectors cannot substitute an unchanged or absent change reference",
        ));
    }
    let mut logical = BTreeMap::<String, BTreeSet<String>>::new();
    let mut identity = BTreeMap::<String, BTreeSet<String>>::new();
    let mut paths = BTreeMap::<String, BTreeSet<String>>::new();
    for (key, row) in rows {
        logical
            .entry(text(get(row, "logical_key"))?.into())
            .or_default()
            .insert(key.clone());
        paths
            .entry(text(get(row, "path"))?.into())
            .or_default()
            .insert(key.clone());
        for id in identities(row)? {
            identity.entry(id).or_default().insert(key.clone());
        }
    }
    let mut chosen = changed.clone();
    let mut edges = BTreeSet::new();
    let mut entries = Vec::new();
    let mut pending = Vec::new();
    let mut rendered = BTreeMap::<String, String>::new();
    let mut rendered_bytes = 0usize;
    let mut queries = BTreeMap::<String, Vec<String>>::new();
    let mut query_bytes = 0usize;
    for key in changed {
        let row = rows.get(key).ok_or_else(|| err("Missing changed source"))?;
        let mut matches = logical
            .get(text(get(row, "logical_key"))?)
            .cloned()
            .unwrap_or_default();
        for id in identities(row)? {
            matches.extend(identity.get(&id).into_iter().flatten().cloned());
        }
        let selection = get(request, "selections").get(key);
        let descriptor = if let Some(selection) = selection {
            if !exact(
                selection,
                &["searches", "ids", "paths", "complete", "uncertain"],
            ) || get(selection, "complete").as_bool().is_none()
                || get(selection, "uncertain").as_bool().is_none()
                || get(selection, "searches")
                    .as_array()
                    .is_none_or(|v| !(2..=8).contains(&v.len()))
                || encoded(selection)?.len() > limits::MAX_PAGE
            {
                return Err(err(
                    "Each changed meaning needs bounded alternative searches and an explicit completed/uncertain selection assertion",
                ));
            }
            let searches = array(get(selection, "searches"))?
                .iter()
                .map(|v| {
                    Ok(crate::unicode::casefold(semantic::safe_text(v, 256)?)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "))
                })
                .collect::<Result<Vec<_>>>()?;
            if searches.iter().collect::<BTreeSet<_>>().len() != searches.len() {
                return Err(err("Use distinct plausible-match wordings"));
            }
            for name in ["ids", "paths"] {
                if array(get(selection, name)).is_err()
                    || array(get(selection, name))?.len() > 64
                    || unique_strings(get(selection, name)).is_err()
                {
                    return Err(err("Use bounded unique exact source selectors"));
                }
            }
            for id in array(get(selection, "ids"))? {
                if !uuid_valid(id) {
                    return Err(err("Select actual stable record identities"));
                }
                matches.extend(identity.get(text(id)?).into_iter().flatten().cloned());
            }
            for path in array(get(selection, "paths"))? {
                if path.as_str() != Some("MEMORY.md") {
                    bank::path_value(path)?;
                }
                matches.extend(paths.get(text(path)?).into_iter().flatten().cloned());
            }
            for search in searches {
                if let Some(found) = queries.get(&search) {
                    matches.extend(found.iter().cloned());
                    continue;
                }
                let query = object([
                    ("ids", strings([] as [&str; 0])),
                    ("paths", strings([] as [&str; 0])),
                    ("query", string(&search)),
                ]);
                let mut found = Vec::new();
                for (other, value) in rows {
                    let transient;
                    let body = if let Some(body) = rendered.get(other) {
                        body
                    } else {
                        transient = format!(
                            "{}\n{}",
                            text(get(value, "body"))?,
                            repr(get(value, "effective_framing"))?
                        );
                        &transient
                    };
                    if retrieval::matching(
                        &query,
                        &object([("path", get(value, "path").clone()), ("body", string(body))]),
                    )?
                    .is_some()
                    {
                        found.push(other.clone());
                    }
                    if !rendered.contains_key(other)
                        && rendered_bytes.saturating_add(body.len() + other.len() + 128)
                            <= 24 * 1024 * 1024
                    {
                        let body = body.to_owned();
                        rendered_bytes += body.len() + other.len() + 128;
                        rendered.insert(other.clone(), body);
                    }
                }
                matches.extend(found.iter().cloned());
                let charge = search
                    .len()
                    .saturating_add(
                        found
                            .iter()
                            .map(|key| key.len() + std::mem::size_of::<String>())
                            .sum::<usize>(),
                    )
                    .saturating_add(128);
                if queries.len() < 256 && query_bytes.saturating_add(charge) <= 8 * 1024 * 1024 {
                    query_bytes += charge;
                    queries.insert(search, found);
                }
            }
            if get(selection, "complete").as_bool() == Some(false) {
                pending.push(key.clone());
            }
            let mut result = selection.clone();
            set(&mut result, "matched", strings(matches.iter().cloned()))?;
            result
        } else {
            pending.push(key.clone());
            object([
                ("complete", Value::Bool(false)),
                ("matched", strings(matches.iter().cloned())),
            ])
        };
        chosen.extend(matches.iter().cloned());
        add_edges(
            &mut edges,
            matches
                .iter()
                .filter(|other| *other != key)
                .map(|other| edge(key, other)),
        )?;
        entries.push((key.clone(), descriptor));
    }
    for (path, required) in fields(get(request, "dependencies"))? {
        let required = unique_strings(required)
            .map_err(|_| err("Dependencies require actual complete unique source paths"))?;
        if !paths.contains_key(path) || required.iter().any(|v| !paths.contains_key(v)) {
            return Err(err(
                "Dependencies require actual complete unique source paths",
            ));
        }
        let left = paths
            .get(path)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let right = required
            .iter()
            .flat_map(|p| paths.get(p).into_iter().flatten().cloned())
            .collect::<Vec<_>>();
        chosen.extend(left.iter().chain(&right).cloned());
        for a in &left {
            add_edges(
                &mut edges,
                right.iter().filter(|b| *b != a).map(|b| edge(a, b)),
            )?;
        }
    }
    for group in array(get(request, "consolidation_sources"))? {
        let group = unique_strings(group).map_err(|_| {
            err("Consolidation requires actual distinct complete component references")
        })?;
        if !(2..=8).contains(&group.len()) || group.iter().any(|k| !rows.contains_key(k)) {
            return Err(err(
                "Consolidation requires actual distinct complete component references",
            ));
        }
        chosen.extend(group.iter().cloned());
        for (i, a) in group.iter().enumerate() {
            add_edges(&mut edges, group.iter().skip(i + 1).map(|b| edge(a, b)))?;
        }
    }
    Ok((
        chosen
            .into_iter()
            .map(|key| {
                rows.get(&key)
                    .cloned()
                    .map(|row| (key, row))
                    .ok_or_else(|| err("Missing selected source"))
            })
            .collect::<Result<_>>()?,
        Value::Object(entries),
        Value::Array(edges.into_iter().map(|(a, b)| strings([a, b])).collect()),
        pending,
    ))
}
pub fn page_payloads(
    binding: &Value,
    rows: &Rows,
    edges: &Value,
    known: &[Value],
) -> Result<Vec<Value>> {
    let binding_sha = hash(binding)?;
    let mut pages = Vec::new();
    let mut items = Vec::new();
    let payload = |index: usize, items: Vec<Value>| {
        object([
            ("schema", string("agent-memory/semantic-sharing-page-v2")),
            ("binding_sha256", string(&binding_sha)),
            ("index", Value::Int(index.into())),
            ("items", Value::Array(items)),
        ])
    };
    for item in rows
        .values()
        .map(|row| object([("source", row.clone())]))
        .chain(
            known
                .iter()
                .map(|cause| object([("known_hold", cause.clone())])),
        )
        .chain(
            array(edges)?
                .iter()
                .map(|edge| object([("edge", edge.clone())])),
        )
    {
        items.push(item);
        if encoded(&payload(pages.len(), items.clone()))?.len() > limits::MAX_PAGE {
            let item = items.pop().ok_or_else(|| err("Missing page item"))?;
            if items.is_empty() {
                return Err(err(
                    "One complete source/framing exceeds a disclosure page; split/review explicitly without truncation",
                ));
            }
            pages.push(payload(pages.len(), std::mem::take(&mut items)));
            items.push(item);
            if encoded(&payload(pages.len(), items.clone()))?.len() > limits::MAX_PAGE {
                return Err(err("One complete source exceeds its page budget"));
            }
        }
    }
    if !items.is_empty() {
        pages.push(payload(pages.len(), items));
    }
    for page in &pages {
        if !array(get(
            &security::scan(
                &string(
                    String::from_utf8(encoded(page)?)
                        .map_err(|_| err("Invalid review page encoding"))?,
                ),
                limits::MAX_PAGE,
            )?,
            "reasons",
        ))?
        .is_empty()
        {
            return Err(err(
                "Composed review page requires safety clarification; no body is disclosed",
            ));
        }
    }
    Ok(pages)
}
pub type Built = (Snapshot, Rows, Value, Vec<Value>);
pub fn build(
    repo: &Path,
    request: Option<&Value>,
    owned_lock: Option<&[u8]>,
    skip_judgment: Option<&str>,
) -> Result<Built> {
    let mut request = request_value(request)?;
    let snapshot = semantic::snapshots(repo, get(&request, "base").as_str(), owned_lock, true)?;
    let (root, observation, binding, values, checked) = &snapshot;
    let rows = components(values, checked)?;
    let runtime = crate::cli::pack_root()?;
    let known_records = holds::load(
        root,
        text(get(observation, "project_id"))?,
        &runtime,
        skip_judgment,
    )?;
    let mut known = Vec::new();
    let actual_paths = rows
        .values()
        .map(|r| Ok(text(get(r, "path"))?.to_owned()))
        .collect::<Result<BTreeSet<_>>>()?;
    let qualified =
        mapping::qualified_current(&rows, values, root, text(get(observation, "project_id"))?)?;
    let index = mapping::source_index(&rows)?;
    for record in &known_records {
        semantic::safe_text(get(get(record, "cause"), "reason"), 4096)?;
        let mut descriptor = mapping::describe(record, &rows, Some(&index))?;
        let resolved = mapping::cached(&descriptor, &rows, &qualified)?;
        set(&mut descriptor, "resolved", Value::Bool(resolved))?;
        let mut audit = unique_strings(get(&request, "audit_paths"))?;
        for key in array(get(&descriptor, "sources"))? {
            let row = rows
                .get(text(key)?)
                .ok_or_else(|| err("Missing retained source"))?;
            audit.insert(text(get(row, "path"))?.into());
        }
        set(&mut request, "audit_paths", strings(audit))?;
        for (path, required) in fields(get(record, "dependencies"))? {
            let required = unique_strings(required)?;
            if actual_paths.contains(path) && required.iter().all(|v| actual_paths.contains(v)) {
                let mut dependencies = get(&request, "dependencies").clone();
                let mut combined = if let Some(old) = dependencies.get(path) {
                    unique_strings(old)?
                } else {
                    BTreeSet::new()
                };
                combined.extend(required);
                set(&mut dependencies, path, strings(combined))?;
                set(&mut request, "dependencies", dependencies)?;
            }
        }
        known.push(descriptor);
    }
    for path in array(get(&request, "audit_paths"))? {
        if !fields(values)?
            .iter()
            .any(|(_, files)| files.get(path.as_str().unwrap_or_default()).is_some())
        {
            return Err(err("Audit paths must name actual observed source"));
        }
    }
    let changed = changed_components(&rows, &unique_strings(get(&request, "audit_paths"))?)?;
    let (mut chosen, selections, edges, pending) = selected(&rows, &changed, &request)?;
    let mut all_edges = BTreeSet::new();
    for pair in array(&edges)? {
        let mut pair = array(pair)?.iter();
        let a = text(pair.next().ok_or_else(|| err("Missing edge"))?)?;
        let b = text(pair.next().ok_or_else(|| err("Missing edge"))?)?;
        all_edges.insert((a.into(), b.into()));
    }
    for descriptor in &known {
        let mut keys = unique_strings(get(descriptor, "sources"))?;
        if let Some(evidence) = get(get(descriptor, "resolution"), "evidence").as_array() {
            keys.extend(
                evidence
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|k| rows.contains_key(*k))
                    .map(str::to_owned),
            );
        }
        for key in &keys {
            chosen.insert(
                key.clone(),
                rows.get(key)
                    .ok_or_else(|| err("Missing retained source"))?
                    .clone(),
            );
        }
        for (i, a) in keys.iter().enumerate() {
            add_edges(&mut all_edges, keys.iter().skip(i + 1).map(|b| edge(a, b)))?;
        }
    }
    let edges = Value::Array(
        all_edges
            .into_iter()
            .map(|(a, b)| strings([a, b]))
            .collect(),
    );
    let mut deleted = BTreeSet::new();
    for side in ["local", "landed"] {
        for entry in array(get(get(checked, side), "entries"))? {
            if get(entry, "format").as_str() == Some("tombstone") {
                deleted.insert(text(get(entry, "id"))?.to_owned());
            }
        }
    }
    for row in chosen.values_mut() {
        let meta = get(row, "metadata").clone();
        if nonnull(&meta)
            && get(&meta, "id")
                .as_str()
                .is_some_and(|id| deleted.contains(id))
        {
            set(row, "body", string(""))?;
            set(row, "metadata", Value::Null)?;
            set(
                row,
                "witness_metadata",
                object([
                    ("id", get(&meta, "id").clone()),
                    ("version", get(&meta, "version").clone()),
                ]),
            )?;
            set(row, "terminal_body_withheld", Value::Bool(true))?;
        }
    }
    let mut hidden = BTreeSet::new();
    for (key, row) in &chosen {
        if nonnull(get(row, "metadata"))
            && (get(get(row, "metadata"), "visibility").as_str() == Some("hidden")
                || !array(get(&security::scan(get(row, "body"), 16384)?, "reasons"))?.is_empty())
        {
            hidden.insert(key.clone());
        }
    }
    loop {
        let mut linked = BTreeSet::new();
        for key in &hidden {
            linked.extend(identities(
                chosen
                    .get(key)
                    .ok_or_else(|| err("Missing hidden source"))?,
            )?);
        }
        let mut expanded = hidden.clone();
        for (key, row) in &chosen {
            if !identities(row)?.is_disjoint(&linked) {
                expanded.insert(key.clone());
            }
        }
        if expanded == hidden {
            break;
        }
        hidden = expanded;
    }
    for key in &hidden {
        let row = chosen
            .get_mut(key)
            .ok_or_else(|| err("Missing hidden source"))?;
        let meta = get(row, "metadata").clone();
        if nonnull(&meta) {
            set(
                row,
                "witness_metadata",
                object([
                    ("id", get(&meta, "id").clone()),
                    ("version", get(&meta, "version").clone()),
                ]),
            )?;
        }
        set(row, "body", string(""))?;
        set(row, "metadata", Value::Null)?;
        set(row, "content_withheld", Value::Bool(true))?;
    }
    let pages = page_payloads(binding, &chosen, &edges, &known)?;
    let descriptors = pages
        .iter()
        .map(|page| {
            let items = array(get(page, "items"))?;
            Ok(object([
                ("index", get(page, "index").clone()),
                ("sha256", string(hash(page)?)),
                ("bytes", Value::Int(encoded(page)?.len().into())),
                (
                    "source_keys",
                    Value::Array(
                        items
                            .iter()
                            .filter_map(|item| item.get("source"))
                            .map(|row| get(row, "key").clone())
                            .collect(),
                    ),
                ),
                (
                    "edges",
                    Value::Array(
                        items
                            .iter()
                            .filter_map(|item| item.get("edge"))
                            .cloned()
                            .collect(),
                    ),
                ),
            ]))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut manifest = object([
        ("schema", string(INSPECTION)),
        ("request", request),
        ("binding", binding.clone()),
        (
            "inventory_digests",
            Value::Object(
                fields(get(binding, "source_hashes"))?
                    .iter()
                    .map(|(side, hashes)| Ok((side.clone(), string(hash(hashes)?))))
                    .collect::<Result<_>>()?,
            ),
        ),
        ("changed", strings(changed)),
        ("sources", Value::Array(chosen.values().cloned().collect())),
        ("selections", selections),
        ("required_pairs", edges),
        ("pending_selections", strings(pending)),
        ("withheld_sources", strings(hidden)),
        ("known_causes", Value::Array(known)),
        (
            "known_inventory_sha256",
            string(hash(&Value::Array(known_records))?),
        ),
        ("pages", Value::Array(descriptors)),
        ("semantic_completeness_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
    ]);
    let digest = sha256_hex(&limits::private_bytes(&manifest)?);
    set(&mut manifest, "manifest_sha256", string(digest))?;
    semantic::final_sources(root, observation, binding, owned_lock)?;
    Ok((snapshot, chosen, manifest, pages))
}
pub fn inspect(repo: &Path, request: Option<&Value>, owned_lock: Option<&[u8]>) -> Result<Value> {
    Ok(build(repo, request, owned_lock, None)?.2)
}
pub fn page(
    repo: &Path,
    manifest: &Value,
    index: usize,
    owned_lock: Option<&[u8]>,
) -> Result<Value> {
    let current = build(repo, manifest.get("request"), owned_lock, None)?;
    if !retrieval::same(&current.2, manifest) {
        return Err(err(
            "Missing, mixed or stale page observation; inspect actual sources again",
        ));
    }
    current.3.get(index).cloned().ok_or_else(|| {
        err("Missing, mixed or stale page observation; inspect actual sources again")
    })
}
pub fn prepare_review(
    repo: &Path,
    judgment: &Value,
    owned_lock: Option<&[u8]>,
) -> Result<(Snapshot, Rows, Value)> {
    let required = [
        "schema",
        "request",
        "binding",
        "manifest_sha256",
        "pages",
        "pairs",
        "dependencies",
        "consolidations",
        "rationale",
        "recorded_at",
    ];
    if judgment.as_object().is_none_or(|pairs| {
        pairs
            .iter()
            .any(|(k, _)| !required.contains(&k.as_str()) && k != "resolutions")
            || required.iter().any(|k| judgment.get(k).is_none())
    }) || get(judgment, "schema").as_str() != Some(JUDGMENT)
    {
        return Err(err("Use the complete explicit v2 source-bound judgment"));
    }
    let (snapshot, rows, manifest, _) = build(
        repo,
        judgment.get("request"),
        owned_lock,
        Some(&hash(judgment)?),
    )?;
    if !retrieval::same(get(judgment, "binding"), get(&manifest, "binding"))
        || get(judgment, "manifest_sha256") != get(&manifest, "manifest_sha256")
        || !array(get(&manifest, "pending_selections"))?.is_empty()
    {
        return Err(err(
            "Complete all actual changed-learning selections against current sources",
        ));
    }
    let pages = Value::Array(
        array(get(&manifest, "pages"))?
            .iter()
            .map(|p| get(p, "sha256").clone())
            .collect(),
    );
    if get(judgment, "pages") != &pages {
        return Err(err("Acknowledge all exact pages once, in manifest order"));
    }
    let mut actual = Vec::new();
    for group in array(get(judgment, "consolidations"))? {
        actual.push(unique_strings(get(group, "sources"))?);
    }
    actual.sort();
    let mut expected = Vec::new();
    for group in array(get(get(judgment, "request"), "consolidation_sources"))? {
        expected.push(unique_strings(group)?);
    }
    expected.sort();
    if !retrieval::same(
        get(judgment, "dependencies"),
        get(get(judgment, "request"), "dependencies"),
    ) || actual != expected
    {
        return Err(err(
            "Dependencies/consolidation must match the complete reviewed page frontier",
        ));
    }
    Ok((snapshot, rows, manifest))
}
pub fn terminal_relation(left: &Value, right: &Value) -> bool {
    let a = metadata(left);
    let b = metadata(right);
    (get(left, "format").as_str() == Some("tombstone")
        || get(right, "format").as_str() == Some("tombstone"))
        && nonnull(get(a, "id"))
        && get(a, "id") == get(b, "id")
}
