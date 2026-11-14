//! Source-bound semantic review. Agent judgments never authenticate truth.
use crate::codec::{frontmatter, json::Value};
use crate::developer_identity::{encoded, sha256_hex};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::{
    bank_contract as bank, bank_layout as layout, bank_transactions as tx, bank_writes as writes,
    lifecycle, records, retrieval, security, sharing_limits as limits,
    sharing_recovery as recovery,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
pub type Rows = BTreeMap<String, Value>;
pub type Snapshot = (PathBuf, Value, Value, Value, Value);
pub const LIMIT: usize = 1048576;
pub const CLASSES: &[&str] = &[
    "compatible",
    "equivalent",
    "contradiction",
    "disjoint-conditions",
    "related",
    "uncertain",
    "lifecycle",
];
pub(crate) fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
pub(crate) fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| err("Expected semantic source text"))
}
pub(crate) fn array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .ok_or_else(|| err("Expected semantic source list"))
}
pub(crate) fn fields(value: &Value) -> Result<&[(String, Value)]> {
    value
        .as_object()
        .ok_or_else(|| err("Expected semantic source object"))
}
pub(crate) fn set(value: &mut Value, key: &str, new: Value) -> Result<()> {
    let Value::Object(pairs) = value else {
        return Err(err("Expected semantic source object"));
    };
    if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *old = new;
    } else {
        pairs.push((key.into(), new));
    }
    Ok(())
}
pub(crate) fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
pub(crate) fn keys(value: &Value) -> Result<BTreeSet<String>> {
    Ok(fields(value)?.iter().map(|(key, _)| key.clone()).collect())
}
pub(crate) fn exact(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|v| {
        v.len() == names.len() && v.iter().all(|(k, _)| names.contains(&k.as_str()))
    })
}
pub(crate) fn hash(value: &Value) -> Result<String> {
    Ok(sha256_hex(&encoded(value)?))
}
pub(crate) fn nonnull(value: &Value) -> bool {
    !matches!(value, Value::Null)
}
pub(crate) fn metadata(row: &Value) -> &Value {
    if nonnull(get(row, "metadata")) {
        get(row, "metadata")
    } else {
        get(row, "witness_metadata")
    }
}
pub(crate) fn header(text: &str) -> Result<(Value, String)> {
    let mut allowed = keys(get(&bank::SPEC, "properties"))?;
    allowed.extend(keys(get(&layout::SPEC, "properties"))?);
    let names: Vec<&str> = allowed.iter().map(String::as_str).collect();
    let (meta, body) = frontmatter::read(text, &names)?;
    Ok((Value::Object(meta), body.into()))
}
pub(crate) fn container(path: &str, text: &str) -> Result<(Value, Vec<Value>, String, Value)> {
    let (metadata, body) = header(text)?;
    let (routes, omitted) = if path == "MEMORY.md" {
        let (_, routes, omitted) = bank::parse_router(text)?;
        (routes, omitted)
    } else {
        (
            layout::parse_index(path, &metadata, &body)?,
            object([] as [(&str, Value); 0]),
        )
    };
    let intro = body
        .split_once("\n## Routes\n")
        .map_or(body.as_str(), |(prefix, _)| prefix)
        .to_owned();
    Ok((metadata, routes, intro, omitted))
}
pub(crate) fn entries(checked: &Value) -> Result<Vec<Value>> {
    let mut entries = vec![object([
        ("path", string("MEMORY.md")),
        ("format", string("root-router")),
        ("scope_chain", strings(["repository"])),
        ("lifecycle", string("active")),
    ])];
    entries.extend(array(get(checked, "entries"))?.iter().cloned());
    Ok(entries)
}
pub fn safe_text(value: &Value, bound: usize) -> Result<&str> {
    let value =
        text(value).map_err(|_| err("A bounded explicit semantic rationale is required"))?;
    if bank::trim(value).is_empty() || value.len() > bound {
        return Err(err("A bounded explicit semantic rationale is required"));
    }
    security::ensure_safe(&object([("review", string(value))]))?;
    Ok(value)
}
pub fn snapshots(
    repo: &Path,
    base: Option<&str>,
    owned_lock: Option<&[u8]>,
    quarantine_records: bool,
) -> Result<Snapshot> {
    writes::recovery_frontier_provider()?.require_complete(repo, owned_lock, None)?;
    let observation = recovery::observe(repo, owned_lock, quarantine_records)?;
    let root = PathBuf::from(text(get(&observation, "repo_root"))?);
    let head = text(get(&observation, "authority_head"))?;
    let common;
    let base = if let Some(base) = base {
        base
    } else {
        common = recovery::command(
            &root,
            &[
                "merge-base",
                "--all",
                text(get(&observation, "local_head"))?,
                head,
            ],
            false,
            limits::MAX_BANK,
            false,
        )?;
        let mut lines = common.lines();
        let base = lines
            .next()
            .ok_or_else(|| err("A lost cursor needs an unambiguous actual common base"))?;
        if lines.next().is_some() {
            return Err(err("A lost cursor needs an unambiguous actual common base"));
        }
        base
    };
    recovery::commit(&root, base)?;
    if !recovery::ancestor(&root, base, head)? {
        return Err(err(
            "The reviewed baseline must remain in authority history",
        ));
    }
    let prefix = text(get(&observation, "bank_root"))?;
    let values = object([
        (
            "base",
            recovery::tree(&root, base, prefix, quarantine_records)?,
        ),
        ("local", tx::files_at(&root.join(prefix))?),
        (
            "landed",
            recovery::tree(&root, head, prefix, quarantine_records)?,
        ),
    ]);
    let checked = Value::Object(
        fields(&values)?
            .iter()
            .map(|(side, files)| {
                Ok((
                    side.clone(),
                    recovery::bounded(files, true, quarantine_records)?,
                ))
            })
            .collect::<Result<_>>()?,
    );
    for (_, value) in fields(&checked)? {
        if nonnull(get(value, "project_id"))
            && get(value, "project_id") != get(&observation, "project_id")
        {
            return Err(err(
                "Sharing review cannot transfer knowledge between projects",
            ));
        }
    }
    let mut binding = Value::Object(
        fields(&observation)?
            .iter()
            .filter(|(key, _)| key != "observed_at")
            .cloned()
            .collect(),
    );
    set(&mut binding, "base", string(base))?;
    set(
        &mut binding,
        "source_hashes",
        Value::Object(
            fields(&values)?
                .iter()
                .map(|(side, files)| Ok((side.clone(), tx::file_hashes(files)?)))
                .collect::<Result<_>>()?,
        ),
    )?;
    Ok((root, observation, binding, values, checked))
}
pub fn final_sources(
    root: &Path,
    observation: &Value,
    binding: &Value,
    owned_lock: Option<&[u8]>,
) -> Result<()> {
    writes::recovery_frontier_provider()?.require_complete(root, owned_lock, None)?;
    crate::scope_binding::require(
        root,
        get(observation, "project_id").as_str(),
        crate::scope_binding::RequireOptions {
            expected: Some(get(observation, "scope_context")),
            owned_lock,
            ..Default::default()
        },
    )?;
    if recovery::server(
        root,
        text(get(observation, "authority_remote"))?,
        text(get(observation, "authority_branch"))?,
    )? != text(get(observation, "authority_head"))?
        || !retrieval::same(
            &writes::snapshot(root, owned_lock)?.1,
            get(get(binding, "source_hashes"), "local"),
        )
        || recovery::command(root, &["rev-parse", "--verify", "HEAD"], false, 4096, false)?
            != text(get(observation, "local_head"))?
    {
        return Err(err(
            "Authority or local source/head changed; review the new source set",
        ));
    }
    Ok(())
}
pub fn units(values: &Value, checked: &Value) -> Result<Rows> {
    Ok(ordered_units(values, checked)?.0)
}
fn ordered_units(values: &Value, checked: &Value) -> Result<(Rows, Vec<String>)> {
    let mut output = Rows::new();
    let mut order = Vec::new();
    for side in ["local", "landed"] {
        let files = get(values, side);
        if fields(files)?.is_empty() {
            continue;
        }
        for entry in entries(get(checked, side))? {
            let format = text(get(&entry, "format"))?;
            if ![
                "context-prose",
                "standalone-record",
                "root-router",
                "topic-index",
            ]
            .contains(&format)
            {
                continue;
            }
            let path = text(get(&entry, "path"))?;
            let content = text(get(files, path))?;
            security::ensure_safe(&object([(path, string(content))]))?;
            let mut add = |body: String,
                           metadata: Value,
                           component: &str,
                           target: Value,
                           container: Value,
                           chain: Value|
             -> Result<()> {
                let sha = sha256_hex(content.as_bytes());
                let identity = if component == "body" {
                    object([("path", string(path)), ("sha256", string(&sha))])
                } else {
                    object([
                        ("path", string(path)),
                        ("component", string(component)),
                        ("target_path", target.clone()),
                        ("body_sha256", string(sha256_hex(body.as_bytes()))),
                    ])
                };
                let key = hash(&identity)?;
                if let Some(row) = output.get_mut(&key) {
                    let mut sides = array(get(row, "sides"))?.to_vec();
                    sides.push(string(side));
                    set(row, "sides", Value::Array(sides))?;
                    for (name, value) in [
                        ("scope_chains", chain),
                        ("source_sha256_by_side", string(sha)),
                        ("container_variants", container),
                    ] {
                        let mut map = get(row, name).clone();
                        set(&mut map, side, value)?;
                        set(row, name, map)?;
                    }
                } else {
                    order.push(key.clone());
                    output.insert(
                        key.clone(),
                        object([
                            ("key", string(key)),
                            ("path", string(path)),
                            ("sha256", string(&sha)),
                            ("source_sha256_by_side", object([(side, string(sha))])),
                            ("sides", strings([side])),
                            ("metadata", metadata),
                            ("body", string(body)),
                            ("scope_chain", chain.clone()),
                            ("scope_chains", object([(side, chain)])),
                            ("lifecycle", get(&entry, "lifecycle").clone()),
                            ("format", string(format)),
                            ("component", string(component)),
                            ("target_path", target),
                            ("container", container.clone()),
                            ("container_variants", object([(side, container)])),
                        ]),
                    );
                }
                Ok(())
            };
            let chain = get(&entry, "scope_chain").clone();
            if format == "standalone-record" {
                let (_, metadata, body) = records::parse(content, &[])?;
                add(body, metadata, "body", Value::Null, Value::Null, chain)?;
            } else if format == "context-prose" {
                add(
                    content.into(),
                    Value::Null,
                    "body",
                    Value::Null,
                    Value::Null,
                    chain,
                )?;
            } else {
                let (meta, routes, intro, omitted) = container(path, content)?;
                let mut context = object([
                    ("metadata", meta.clone()),
                    ("routes", Value::Array(routes.clone())),
                ]);
                if path == "MEMORY.md" {
                    set(&mut context, "omitted", omitted.clone())?;
                }
                add(
                    intro,
                    Value::Null,
                    "introduction",
                    Value::Null,
                    context,
                    chain.clone(),
                )?;
                for route in routes {
                    let mut route_chain = array(&chain)?.to_vec();
                    route_chain.push(get(&route, "scope").clone());
                    add(
                        format!(
                            "Purpose: {}\nRead when: {}\n",
                            text(get(&route, "purpose"))?,
                            text(get(&route, "read-when"))?
                        ),
                        Value::Null,
                        "route-prose",
                        get(&route, "path").clone(),
                        object([("metadata", meta.clone()), ("route", route)]),
                        Value::Array(route_chain),
                    )?;
                }
                for (target, reason) in fields(&omitted)? {
                    add(
                        text(reason)?.into(),
                        Value::Null,
                        "omission",
                        string(target),
                        object([("metadata", meta.clone()), ("context_path", string(target))]),
                        chain.clone(),
                    )?;
                }
            }
        }
    }
    if output.len() > 64
        || encoded(&Value::Object(
            output.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        ))?
        .len()
            > 65536
    {
        return Err(err(
            "Complete semantic source review exceeds its bound; split reviewed independent units without claiming absence",
        ));
    }
    Ok((output, order))
}
pub fn pairs(rows: &Rows) -> Value {
    Value::Array(
        rows.keys()
            .enumerate()
            .flat_map(|(i, left)| {
                rows.keys()
                    .skip(i + 1)
                    .map(move |right| strings([left, right]))
            })
            .collect(),
    )
}
pub fn inspect(repo: &Path, base: Option<&str>, owned_lock: Option<&[u8]>) -> Result<Value> {
    let (root, observation, binding, values, checked) = snapshots(repo, base, owned_lock, false)?;
    let (rows, order) = ordered_units(&values, &checked)?;
    final_sources(&root, &observation, &binding, owned_lock)?;
    let ids = rows
        .values()
        .filter_map(|row| get(get(row, "metadata"), "id").as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let audit = security::audit(
        &root,
        &crate::host::uuid4()?,
        "retrieval",
        "checked",
        get(&observation, "scope_context"),
        &["external"],
        &[],
        &ids.into_iter().collect::<Vec<_>>(),
        "policy-check",
        None,
    )?;
    final_sources(&root, &observation, &binding, owned_lock)?;
    let sources = order
        .iter()
        .map(|key| {
            rows.get(key)
                .cloned()
                .ok_or_else(|| err("Missing ordered source"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(object([
        (
            "schema",
            string("agent-memory/semantic-sharing-inspection-v1"),
        ),
        ("binding", binding),
        ("sources", Value::Array(sources)),
        ("required_pairs", pairs(&rows)),
        ("audit", audit),
        ("audit_writes_performed", Value::Bool(true)),
        (
            "detecting_developer_id",
            get(get(&observation, "scope_context"), "developer_id").clone(),
        ),
        ("semantic_truth_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}

pub fn qualified_sources(files: &Value, root: &Path, project_id: &str) -> Result<BTreeSet<String>> {
    if fields(files)?.is_empty() {
        return Ok(BTreeSet::new());
    }
    let checked = bank::validate_files(files, None)?;
    let (actual, _) = retrieval::observed_context(root, &object([] as [(&str, Value); 0]))?;
    let now = crate::host::utc_now_isoformat()?;
    let mut qualified = BTreeMap::new();
    let mut superseders = BTreeSet::new();
    for entry in array(get(&checked, "entries"))? {
        if get(entry, "format").as_str() != Some("standalone-record") {
            continue;
        }
        let path = text(get(entry, "path"))?;
        let (_, meta, _) = records::parse(text(get(files, path))?, &[])?;
        if !retrieval::scope_reasons(&string(project_id), entry, &actual)?.is_empty() {
            continue;
        }
        let applies = records::applicability(
            &meta,
            &object([
                ("bank_project_id", string(project_id)),
                ("context", actual.clone()),
            ]),
        )?;
        let mut record = meta.clone();
        for (name, default) in [("trust", "reviewed"), ("visibility", "visible")] {
            if record.get(name).is_none() {
                set(&mut record, name, string(default))?;
            }
        }
        let selection = lifecycle::current(
            &record,
            &object([
                ("authorized-scope", Value::Bool(true)),
                ("applicable", get(&applies, "applicable").clone()),
                ("minimum-trust", string("reviewed")),
                ("now", string(&now)),
                ("active-superseders", strings([] as [&str; 0])),
            ]),
        )?;
        if ![
            "authorized-scope",
            "applicable",
            "active",
            "valid-time",
            "sufficient-trust",
        ]
        .iter()
        .all(|key| get(get(&selection, "gates"), key).as_bool() == Some(true))
        {
            continue;
        }
        for name in ["supersedes", "corrects"] {
            for reference in array(get(&meta, name))? {
                superseders.insert((
                    text(get(reference, "id"))?.to_owned(),
                    encoded(get(reference, "version"))?,
                ));
            }
        }
        qualified.insert(path.to_owned(), (meta, selection));
    }
    let mut result = BTreeSet::new();
    for (path, (meta, selection)) in qualified {
        if get(&selection, "current").as_bool() == Some(true)
            && !superseders.contains(&(
                text(get(&meta, "id"))?.to_owned(),
                encoded(get(&meta, "version"))?,
            ))
        {
            result.insert(path);
        }
    }
    Ok(result)
}
pub fn disjoint(left: &Value, right: &Value) -> Result<bool> {
    fn constraints(row: &Value, chain: &Value) -> Result<BTreeMap<String, Vec<Value>>> {
        let mut result = BTreeMap::<String, Vec<Value>>::new();
        if nonnull(get(row, "metadata")) {
            for (key, value) in fields(&layout::constraints(get(row, "metadata"))?)? {
                result.entry(key.clone()).or_default().push(value.clone());
            }
        }
        for scope in array(chain)? {
            if let Some((key, value)) = crate::scopes::parse_scope(scope)? {
                result.entry(key).or_default().push(value);
            }
        }
        Ok(result)
    }
    fn nonoverlap(
        a: &BTreeMap<String, Vec<Value>>,
        b: &BTreeMap<String, Vec<Value>>,
    ) -> Result<bool> {
        for (key, left) in a {
            if let Some(right) = b.get(key) {
                for x in left {
                    for y in right {
                        if !nonnull(x) || !nonnull(y) {
                            continue;
                        }
                        if key == "component" {
                            let a = Path::new(text(x)?);
                            let b = Path::new(text(y)?);
                            if !a.starts_with(b) && !b.starts_with(a) {
                                return Ok(true);
                            }
                        } else if !retrieval::same(x, y) {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
    let (a, b) = (get(left, "metadata"), get(right, "metadata"));
    if nonnull(a) && nonnull(b) {
        for (old, new) in [(a, b), (b, a)] {
            if nonnull(get(old, "valid-until"))
                && nonnull(get(new, "valid-from"))
                && lifecycle::time_value(get(old, "valid-until"))?
                    <= lifecycle::time_value(get(new, "valid-from"))?
            {
                return Ok(true);
            }
        }
    }
    let a = fields(get(left, "scope_chains"))?
        .iter()
        .map(|(_, chain)| constraints(left, chain))
        .collect::<Result<Vec<_>>>()?;
    let b = fields(get(right, "scope_chains"))?
        .iter()
        .map(|(_, chain)| constraints(right, chain))
        .collect::<Result<Vec<_>>>()?;
    if a.is_empty() || b.is_empty() {
        return Ok(false);
    }
    for x in &a {
        for y in &b {
            if !nonoverlap(x, y)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
pub fn introduction(text: &str) -> Result<&str> {
    let (_, body) = text
        .split_once("\n---\n")
        .ok_or_else(|| err("Malformed semantic container"))?;
    Ok(body
        .split_once("\n## Routes\n")
        .map_or(body, |(prefix, _)| prefix))
}
pub fn replace_introduction(text: &str, prefix: &str) -> Result<String> {
    let (header, body) = text
        .split_once("\n---\n")
        .ok_or_else(|| err("Malformed semantic container"))?;
    let (_, tail) = body
        .split_once("\n## Routes\n")
        .ok_or_else(|| err("Malformed semantic container"))?;
    Ok(format!("{header}\n---\n{prefix}\n## Routes\n{tail}"))
}
pub fn container_paths(files: &Value) -> Result<BTreeSet<String>> {
    if fields(files)?.is_empty() {
        return Ok(BTreeSet::new());
    }
    let checked = bank::validate_files(files, None)?;
    let mut result = BTreeSet::from(["MEMORY.md".into()]);
    for entry in array(get(&checked, "entries"))? {
        if get(entry, "format").as_str() == Some("topic-index") {
            result.insert(text(get(entry, "path"))?.into());
        }
    }
    Ok(result)
}
fn prose_lines(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        if matches!(
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
        ) {
            let mut end = offset + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                if let Some((offset, _)) = chars.next() {
                    end = offset + 1;
                }
            }
            if let Some(line) = text.get(start..end) {
                result.push(line);
            }
            start = end;
        }
    }
    if let Some(tail) = text.get(start..).filter(|tail| !tail.is_empty()) {
        result.push(tail);
    }
    result
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    start: usize,
    end: usize,
    replacement: Vec<String>,
}
// SequenceMatcher with autojunk=False: longest block, earliest source, earliest peer.
fn line_edits(base: &[&str], peer: &[&str]) -> Vec<Edit> {
    let mut positions = BTreeMap::<&str, Vec<usize>>::new();
    for (j, line) in peer.iter().enumerate() {
        positions.entry(line).or_default().push(j);
    }
    let mut pending = vec![(0, base.len(), 0, peer.len())];
    let mut blocks = Vec::new();
    while let Some((alo, ahi, blo, bhi)) = pending.pop() {
        let mut best = (alo, blo, 0usize);
        let mut previous = BTreeMap::<usize, usize>::new();
        for i in alo..ahi {
            let mut current = BTreeMap::new();
            if let Some(line) = base.get(i) {
                for j in positions
                    .get(line)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|j| *j >= blo && *j < bhi)
                {
                    let count = j
                        .checked_sub(1)
                        .and_then(|j| previous.get(&j))
                        .copied()
                        .unwrap_or(0)
                        + 1;
                    current.insert(j, count);
                    if count > best.2 {
                        best = (i + 1 - count, j + 1 - count, count);
                    }
                }
            }
            previous = current;
        }
        let (i, j, size) = best;
        if size != 0 {
            blocks.push(best);
            if alo < i && blo < j {
                pending.push((alo, i, blo, j));
            }
            if i + size < ahi && j + size < bhi {
                pending.push((i + size, ahi, j + size, bhi));
            }
        }
    }
    blocks.sort_unstable();
    blocks.push((base.len(), peer.len(), 0));
    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (a, b, size) in blocks {
        if a > i || b > j {
            edits.push(Edit {
                start: i,
                end: a,
                replacement: peer
                    .get(j..b)
                    .unwrap_or_default()
                    .iter()
                    .map(|s| (*s).into())
                    .collect(),
            });
        }
        i = a + size;
        j = b + size;
    }
    edits
}
pub fn compatible_prose(base: &str, local: &str, landed: &str) -> Result<String> {
    let source = prose_lines(base);
    let mut edits = line_edits(&source, &prose_lines(local));
    for edit in line_edits(&source, &prose_lines(landed)) {
        if !edits.contains(&edit) {
            edits.push(edit);
        }
    }
    for (i, left) in edits.iter().enumerate() {
        for right in edits.iter().skip(i + 1) {
            let (a, b, c, d) = (left.start, left.end, right.start, right.end);
            if a.max(c) < b.min(d)
                || a == c
                || (a == b && c < a && a < d)
                || (c == d && a < c && c < b)
            {
                return Err(err(
                    "Overlapping prose changes require an explicit governed correction, not compatible merge",
                ));
            }
        }
    }
    edits.sort_by_key(|edit| (edit.start, edit.end));
    let mut result = String::new();
    let mut cursor = 0;
    for edit in edits {
        for line in source
            .get(cursor..edit.start)
            .ok_or_else(|| err("Invalid prose edit range"))?
        {
            result.push_str(line);
        }
        for line in edit.replacement {
            result.push_str(&line);
        }
        cursor = edit.end;
    }
    for line in source
        .get(cursor..)
        .ok_or_else(|| err("Invalid prose edit range"))?
    {
        result.push_str(line);
    }
    Ok(result)
}
pub fn child_paths(files: &Value, path: &str) -> Result<BTreeSet<String>> {
    if fields(files)?.is_empty() {
        return Ok(BTreeSet::from([path.into()]));
    }
    if path == "MEMORY.md" {
        return keys(files);
    }
    let checked = bank::validate_files(files, None)?;
    let indices = array(get(&checked, "entries"))?
        .iter()
        .filter(|entry| get(entry, "format").as_str() == Some("topic-index"))
        .map(|entry| Ok(text(get(entry, "path"))?.to_owned()))
        .collect::<Result<BTreeSet<_>>>()?;
    let mut found = BTreeSet::new();
    let mut pending = vec![path.to_owned()];
    while let Some(path) = pending.pop() {
        if !found.insert(path.clone()) {
            continue;
        }
        if indices.contains(&path) {
            for route in container(&path, text(get(files, &path))?)?.1 {
                pending.push(text(get(&route, "path"))?.into());
            }
        }
    }
    Ok(found)
}
pub type Held = BTreeSet<(String, String, Option<String>)>;
pub fn mask(files: &Value, baseline: &Value, held: &Held) -> Result<Value> {
    if fields(files)?.is_empty() {
        return Ok(object([] as [(&str, Value); 0]));
    }
    let original = bank::validate_files(files, None)?;
    let old = if fields(baseline)?.is_empty() {
        object([
            ("entries", Value::Array(Vec::new())),
            ("routes", Value::Array(Vec::new())),
        ])
    } else {
        bank::validate_files(baseline, None)?
    };
    let mut current = fields(files)?.iter().cloned().collect::<BTreeMap<_, _>>();
    let mut protected = BTreeSet::new();
    for entry in array(get(&original, "entries"))? {
        if ["tombstone", "optional-history-event"].contains(&text(get(entry, "format"))?)
            || (get(entry, "format").as_str() == Some("standalone-record")
                && get(entry, "lifecycle").as_str() != Some("active"))
        {
            protected.insert(text(get(entry, "path"))?.to_owned());
        }
    }
    let body_paths = held
        .iter()
        .filter(|(path, component, _)| component == "body" && !protected.contains(path))
        .map(|(path, _, _)| path.clone())
        .collect::<BTreeSet<_>>();
    for path in &body_paths {
        if let Some(body) = baseline.get(path) {
            current.insert(path.clone(), body.clone());
        } else {
            current.remove(path);
        }
    }
    let mut tables = BTreeMap::<String, BTreeMap<String, Value>>::new();
    let mut route_order = BTreeMap::<String, Vec<String>>::new();
    let mut headers = BTreeMap::new();
    let mut prefixes = BTreeMap::new();
    let mut omitted = object([] as [(&str, Value); 0]);
    for path in container_paths(files)?
        .union(&container_paths(baseline)?)
        .cloned()
        .collect::<Vec<_>>()
    {
        let Some(content) = current.get(&path) else {
            continue;
        };
        let (header, routes, prefix, reasons) = container(&path, text(content)?)?;
        if path == "MEMORY.md" {
            omitted = reasons;
        }
        headers.insert(path.clone(), header);
        prefixes.insert(path.clone(), prefix);
        let mut rows = BTreeMap::new();
        let mut order = Vec::new();
        for route in routes {
            let target = text(get(&route, "path"))?.to_owned();
            if !body_paths.contains(&target) {
                order.push(target.clone());
                rows.insert(target, route);
            }
        }
        route_order.insert(path.clone(), order);
        tables.insert(path, rows);
    }
    let mut base_routes = BTreeMap::new();
    for route in array(get(&old, "routes"))? {
        base_routes.insert(
            text(get(route, "path"))?.to_owned(),
            ("MEMORY.md".to_owned(), route.clone()),
        );
    }
    for entry in array(get(&old, "entries"))? {
        if get(entry, "format").as_str() == Some("topic-index") {
            let path = text(get(entry, "path"))?;
            for route in container(path, text(get(baseline, path))?)?.1 {
                base_routes.insert(
                    text(get(&route, "path"))?.to_owned(),
                    (path.to_owned(), route),
                );
            }
        }
    }
    let old_containers = container_paths(baseline)?;
    for path in &body_paths {
        if !current.contains_key(path) || path == "MEMORY.md" {
            continue;
        }
        let (parent, row) = base_routes
            .get(path)
            .ok_or_else(|| err("Missing baseline navigation for disputed source"))?;
        let parent = if tables.contains_key(parent) {
            parent.as_str()
        } else {
            if old_containers.contains(path) {
                return Err(err(
                    "A disputed index move needs explicit dependency isolation",
                ));
            }
            "MEMORY.md"
        };
        let rows = tables
            .get_mut(parent)
            .ok_or_else(|| err("Missing project router"))?;
        if !rows.contains_key(path) {
            route_order
                .get_mut(parent)
                .ok_or_else(|| err("Missing navigation order"))?
                .push(path.clone());
        }
        rows.insert(path.clone(), row.clone());
    }
    for (path, component, target) in held {
        let Some(target) = target else {
            continue;
        };
        if component != "route-prose" || !protected.contains(target) {
            continue;
        }
        let Some(row) = tables.get_mut(path).and_then(|rows| rows.get_mut(target)) else {
            continue;
        };
        if let Some((_, previous)) = base_routes.get(target) {
            set(row, "purpose", get(previous, "purpose").clone())?;
            set(row, "read-when", get(previous, "read-when").clone())?;
        } else {
            set(
                row,
                "purpose",
                string("Retained canonical invalidation or history"),
            )?;
            set(
                row,
                "read-when",
                string("Before using or rebuilding affected guidance"),
            )?;
        }
    }
    for (path, component, _) in held {
        if component != "introduction" || !tables.contains_key(path) {
            continue;
        }
        let prefix = if let Some(content) = baseline.get(path) {
            introduction(text(content)?)?.to_owned()
        } else if path != "MEMORY.md" {
            let table = tables
                .get(path)
                .ok_or_else(|| err("Missing container routes"))?;
            let routes = route_order
                .get(path)
                .ok_or_else(|| err("Missing navigation order"))?
                .iter()
                .filter_map(|target| table.get(target))
                .cloned()
                .collect::<Vec<_>>();
            if routes.is_empty() {
                String::new()
            } else {
                introduction(&layout::index_text(
                    path,
                    headers
                        .get(path)
                        .ok_or_else(|| err("Missing index header"))?,
                    &routes,
                )?)?
                .to_owned()
            }
        } else {
            return Err(err(
                "A project router cannot disappear during reconciliation",
            ));
        };
        prefixes.insert(path.clone(), prefix);
    }
    loop {
        let removed = tables
            .iter()
            .filter(|(path, rows)| path.as_str() != "MEMORY.md" && rows.is_empty())
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>();
        if removed.is_empty() {
            break;
        }
        for path in &removed {
            tables.remove(path);
            current.remove(path);
        }
        for rows in tables.values_mut() {
            rows.retain(|path, _| !removed.contains(path));
        }
    }
    for (path, rows) in &tables {
        let routes = route_order
            .get(path)
            .ok_or_else(|| err("Missing navigation order"))?
            .iter()
            .filter(|target| current.contains_key(*target))
            .filter_map(|target| rows.get(target))
            .cloned()
            .collect::<Vec<_>>();
        let mut content = frontmatter::write(fields(
            headers
                .get(path)
                .ok_or_else(|| err("Missing container header"))?,
        )?)?;
        content.push_str(
            prefixes
                .get(path)
                .ok_or_else(|| err("Missing container introduction"))?,
        );
        content.push_str("\n## Routes\n\n");
        content.push_str(&bank::route_table(&routes, path)?);
        if path == "MEMORY.md" {
            let previous = if let Some(content) = baseline.get(path) {
                bank::parse_router(text(content)?)?.2
            } else {
                object([] as [(&str, Value); 0])
            };
            let mut reasons = Vec::new();
            for context in bank::CONTEXTS.iter() {
                if current.contains_key(*context) {
                    continue;
                }
                let reason = if body_paths.contains(*context) {
                    previous.get(context).or_else(|| omitted.get(context))
                } else {
                    omitted.get(context).or_else(|| previous.get(context))
                }
                .ok_or_else(|| err("An omitted context needs its actual source explanation"))?;
                reasons.push(format!("- `{context}`: {}\n", text(reason)?));
            }
            content.push_str("\n## Omitted contexts\n\n");
            if reasons.is_empty() {
                content.push_str("None.\n");
            } else {
                content.push_str(&reasons.concat());
            }
        }
        current.insert(path.clone(), string(content));
    }
    let current = Value::Object(current.into_iter().collect());
    bank::validate_files(&current, None)?;
    Ok(current)
}
pub fn preserve_invalidation(values: &Value, candidate: &Value) -> Result<()> {
    let mut guards = BTreeMap::new();
    let mut inactive = BTreeMap::<String, crate::codec::json::Integer>::new();
    for (_, files) in fields(values)? {
        if fields(files)?.is_empty() {
            continue;
        }
        let checked = bank::validate_files(files, None)?;
        for entry in array(get(&checked, "entries"))? {
            if get(entry, "format").as_str() == Some("tombstone") {
                let path = text(get(entry, "path"))?;
                let content = get(files, path);
                if guards.get(path).is_some_and(|old| old != content) {
                    return Err(err("Conflicting terminal guards require source review"));
                }
                guards.insert(path.to_owned(), content.clone());
            } else if get(entry, "format").as_str() == Some("standalone-record")
                && get(entry, "lifecycle").as_str() != Some("active")
            {
                let id = text(get(entry, "id"))?;
                let version = get(entry, "version")
                    .as_int()
                    .ok_or_else(|| err("Invalid source version"))?
                    .clone();
                if inactive
                    .get(id)
                    .is_none_or(|old| integer_cmp(&version, old).is_gt())
                {
                    inactive.insert(id.into(), version);
                }
            }
        }
    }
    if guards
        .iter()
        .any(|(path, body)| candidate.get(path) != Some(body))
    {
        return Err(err(
            "Semantic deferral cannot discard a terminal guard from the original inputs",
        ));
    }
    for entry in array(get(&bank::validate_files(candidate, None)?, "entries"))? {
        if get(entry, "format").as_str() == Some("standalone-record")
            && get(entry, "lifecycle").as_str() == Some("active")
        {
            let id = text(get(entry, "id"))?;
            let version = get(entry, "version")
                .as_int()
                .ok_or_else(|| err("Invalid source version"))?;
            if inactive
                .get(id)
                .is_some_and(|old| !integer_cmp(version, old).is_gt())
            {
                return Err(err(
                    "Semantic deferral cannot restore an invalidated original version",
                ));
            }
        }
    }
    Ok(())
}
fn integer_cmp(
    a: &crate::codec::json::Integer,
    b: &crate::codec::json::Integer,
) -> std::cmp::Ordering {
    let (a, b) = (a.to_string(), b.to_string());
    a.len().cmp(&b.len()).then_with(|| a.cmp(&b))
}
pub type Decisions = BTreeMap<(String, String), Value>;
pub fn reviewed_value(judgment: &Value, inspection: &Value) -> Result<Decisions> {
    if !exact(
        judgment,
        &[
            "binding",
            "pairs",
            "dependencies",
            "consolidations",
            "rationale",
            "recorded_at",
        ],
    ) || !retrieval::same(get(judgment, "binding"), get(inspection, "binding"))
    {
        return Err(err(
            "Semantic judgment must bind complete actual current sources",
        ));
    }
    safe_text(get(judgment, "rationale"), 4096)?;
    let at = lifecycle::time_value(get(judgment, "recorded_at"))?;
    if at > lifecycle::time_value(&string(crate::host::utc_now_isoformat()?))? {
        return Err(err("Review cannot be future dated"));
    }
    let mut decisions = Decisions::new();
    for decision in
        array(get(judgment, "pairs")).map_err(|_| err("Review every actual source pair"))?
    {
        if !exact(decision, &["left", "right", "classification", "reason"])
            || !get(decision, "classification")
                .as_str()
                .is_some_and(|s| CLASSES.contains(&s))
        {
            return Err(err("Unknown semantic pair decision"));
        }
        let key = (
            text(get(decision, "left"))?.to_owned(),
            text(get(decision, "right"))?.to_owned(),
        );
        safe_text(get(decision, "reason"), 4096)?;
        if decisions.insert(key, decision.clone()).is_some() {
            return Err(err("Do not duplicate source judgments"));
        }
    }
    let mut required = BTreeSet::new();
    for pair in array(get(inspection, "required_pairs"))? {
        let pair = array(pair)?;
        if let [left, right] = pair {
            required.insert((text(left)?.to_owned(), text(right)?.to_owned()));
        } else {
            return Err(err("Invalid required source pair"));
        }
    }
    if decisions.keys().cloned().collect::<BTreeSet<_>>() != required {
        return Err(err(
            "Semantic review must cover cross-file and same-file source pairs completely",
        ));
    }
    Ok(decisions)
}

pub fn consolidate(
    files: &Value,
    groups: &Value,
    rows: &Rows,
    decisions: &Decisions,
    context: &Value,
    operation_id: &str,
    at: &Value,
    blocked: &BTreeSet<String>,
) -> Result<Value> {
    let groups = array(groups).map_err(|_| err("Consolidate bounded explicit duplicate groups"))?;
    if groups.len() > 8 {
        return Err(err("Consolidate bounded explicit duplicate groups"));
    }
    let mut used = BTreeSet::new();
    let mut current = files.clone();
    for group in groups {
        if !exact(group, &["sources", "primary", "body", "reason"]) {
            return Err(err(
                "A duplicate group names distinct complete source keys and an existing primary",
            ));
        }
        let sources = array(get(group, "sources"))?;
        let source_keys = sources
            .iter()
            .map(|key| Ok(text(key)?.to_owned()))
            .collect::<Result<BTreeSet<_>>>()?;
        let primary = text(get(group, "primary"))?;
        if !(2..=8).contains(&sources.len())
            || source_keys.len() != sources.len()
            || !source_keys.contains(primary)
        {
            return Err(err(
                "A duplicate group names distinct complete source keys and an existing primary",
            ));
        }
        safe_text(get(group, "body"), 4096)?;
        safe_text(get(group, "reason"), 4096)?;
        if source_keys
            .iter()
            .any(|key| !rows.contains_key(key) || used.contains(key))
        {
            return Err(err("Source group is missing or overlaps another group"));
        }
        used.extend(source_keys.iter().cloned());
        let selected = sources
            .iter()
            .map(|key| {
                rows.get(text(key)?)
                    .ok_or_else(|| err("Missing duplicate source"))
            })
            .collect::<Result<Vec<_>>>()?;
        if selected.iter().any(|row| {
            get(row, "path")
                .as_str()
                .is_some_and(|path| blocked.contains(path))
        }) {
            continue;
        }
        if selected
            .iter()
            .map(|row| get(row, "path").as_str())
            .collect::<BTreeSet<_>>()
            .len()
            != selected.len()
        {
            return Err(err(
                "Divergent versions of one path require lifecycle review",
            ));
        }
        for (i, left) in source_keys.iter().enumerate() {
            for right in source_keys.iter().skip(i + 1) {
                if decisions
                    .get(&(left.clone(), right.clone()))
                    .and_then(|decision| get(decision, "classification").as_str())
                    != Some("equivalent")
                {
                    return Err(err("Only reviewed equivalent claims can consolidate"));
                }
            }
        }
        let primary = rows
            .get(primary)
            .ok_or_else(|| err("Missing duplicate primary"))?;
        let metadata = get(primary, "metadata");
        if !nonnull(metadata)
            || selected.iter().any(|row| {
                !nonnull(get(row, "metadata")) || get(row, "lifecycle").as_str() != Some("active")
            })
        {
            return Err(err("Duplicate maintenance requires active typed records"));
        }
        if selected.iter().any(|row| {
            crate::consolidation::AXES
                .iter()
                .any(|axis| !retrieval::same(get(get(row, "metadata"), axis), get(metadata, axis)))
        }) {
            return Err(err(
                "Distinct conditions, validity, type, trust and visibility must remain separate",
            ));
        }
        let mut targets = Vec::new();
        let root = Path::new(text(get(context, "repo_root"))?);
        for row in selected {
            let path = text(get(row, "path"))?;
            let Some(body) = current.get(path) else {
                return Err(err(
                    "A duplicate source changed during three-way reconciliation",
                ));
            };
            if sha256_hex(text(body)?.as_bytes()) != text(get(row, "sha256"))? {
                return Err(err(
                    "A duplicate source changed during three-way reconciliation",
                ));
            }
            let meta = get(row, "metadata");
            if !qualified_sources(&current, root, text(get(meta, "project-id"))?)?.contains(path) {
                return Err(err(
                    "Unknown, invalidated or inactive duplicate applicability requires lifecycle review",
                ));
            }
            targets.push(object([
                ("path", string(path)),
                ("id", get(meta, "id").clone()),
                ("version", get(meta, "version").clone()),
                ("sha256", get(row, "sha256").clone()),
                ("required_preserved_metadata", meta.clone()),
                ("scope_chain", get(row, "scope_chain").clone()),
            ]));
        }
        let chosen = targets
            .iter()
            .find(|target| get(target, "path") == get(primary, "path"))
            .ok_or_else(|| err("Missing duplicate primary target"))?
            .clone();
        let evidence = object([
            (
                "comparison_request",
                object([(
                    "candidate",
                    object([
                        ("metadata", metadata.clone()),
                        ("body", get(group, "body").clone()),
                    ]),
                )]),
            ),
            (
                "comparison_proof",
                object([(
                    "judgment",
                    object([("rationale", get(group, "reason").clone())]),
                )]),
            ),
            ("recorded_at", at.clone()),
        ]);
        let plan = object([
            ("primary", chosen),
            ("targets", Value::Array(targets)),
            (
                "qualified",
                object([("scope_context", get(context, "scope_context").clone())]),
            ),
        ]);
        let child = crate::consolidation::child_id(operation_id, &format!("group-{}", used.len()))?;
        if let Some(result) = crate::consolidation::build(&current, &evidence, &child, &plan, None)?
        {
            current = result;
        }
    }
    Ok(current)
}
fn sorted_pair(left: &str, right: &str) -> (String, String) {
    if left < right {
        (left.into(), right.into())
    } else {
        (right.into(), left.into())
    }
}
fn hold(row: &Value, held: &mut Held, blocked: &mut BTreeSet<String>) -> Result<()> {
    let path = text(get(row, "path"))?.to_owned();
    held.insert((
        path.clone(),
        text(get(row, "component"))?.into(),
        get(row, "target_path").as_str().map(str::to_owned),
    ));
    blocked.insert(path);
    Ok(())
}
fn remember(
    causes: &mut BTreeMap<String, Value>,
    keys: impl IntoIterator<Item = String>,
    classification: &str,
    reason: &str,
    origin: &str,
) -> Result<()> {
    let keys = keys.into_iter().collect::<BTreeSet<_>>();
    let cause = object([
        ("sources", strings(keys)),
        ("classification", string(classification)),
        ("reason", string(reason)),
        ("origin", string(origin)),
    ]);
    let id = crate::sharing_holds::cause_id(&cause)?;
    causes.entry(id).or_insert(cause);
    Ok(())
}
fn peers(claims: &[&Value], actor: &str) -> BTreeSet<String> {
    claims
        .iter()
        .filter_map(|row| get(get(get(row, "metadata"), "origin"), "developer-id").as_str())
        .filter(|id| *id != actor)
        .map(str::to_owned)
        .collect()
}
fn source_for<'a>(rows: &'a Rows, path: &str, component: &str, side: &str) -> Result<&'a str> {
    rows.iter()
        .find(|(_, row)| {
            get(row, "path").as_str() == Some(path)
                && get(row, "component").as_str() == Some(component)
                && get(row, "sides")
                    .as_array()
                    .is_some_and(|sides| sides.contains(&string(side)))
        })
        .map(|(key, _)| key.as_str())
        .ok_or_else(|| err("Missing reviewed source variant"))
}
fn operation_id(digest: &str) -> Result<String> {
    let bytes = digest.as_bytes();
    let mut out = String::with_capacity(36);
    for index in 0..32 {
        if [8, 12, 16, 20].contains(&index) {
            out.push('-');
        }
        let c = *bytes
            .get(index)
            .ok_or_else(|| err("Invalid judgment digest"))?;
        if index == 12 {
            out.push('4');
        } else if index == 16 {
            let digit = (c as char)
                .to_digit(16)
                .ok_or_else(|| err("Invalid judgment digest"))?;
            out.push(
                char::from_digit((digit & 3) | 8, 16)
                    .ok_or_else(|| err("Invalid judgment digest"))?,
            );
        } else {
            out.push(c as char);
        }
    }
    Ok(out)
}
pub fn review(repo: &Path, judgment: &Value, owned_lock: Option<&[u8]>) -> Result<Value> {
    if judgment.as_object().is_none() || get(judgment, "binding").as_object().is_none() {
        return Err(err("Review requires an inspection binding"));
    }
    let version2 = get(judgment, "schema").as_str() == Some(crate::sharing_frontier::JUDGMENT);
    let (snapshot, rows, inspection, plain) = if version2 {
        let (snapshot, rows, inspection) =
            crate::sharing_frontier::prepare_review(repo, judgment, owned_lock)?;
        let plain = Value::Object(
            fields(judgment)?
                .iter()
                .filter(|(key, _)| {
                    [
                        "binding",
                        "pairs",
                        "dependencies",
                        "consolidations",
                        "rationale",
                        "recorded_at",
                    ]
                    .contains(&key.as_str())
                })
                .cloned()
                .collect(),
        );
        (snapshot, rows, inspection, plain)
    } else {
        let snapshot = snapshots(
            repo,
            get(get(judgment, "binding"), "base").as_str(),
            owned_lock,
            false,
        )?;
        let rows = units(&snapshot.3, &snapshot.4)?;
        let inspection = object([
            ("binding", snapshot.2.clone()),
            ("sources", Value::Array(rows.values().cloned().collect())),
            ("required_pairs", pairs(&rows)),
        ]);
        (snapshot, rows, inspection, judgment.clone())
    };
    let (root, observation, binding, values, _) = &snapshot;
    let decisions = reviewed_value(&plain, &inspection)?;
    let mut held = Held::new();
    let mut blocked = BTreeSet::new();
    let mut conflicts = Vec::new();
    let mut peer = BTreeSet::new();
    let mut equivalents = BTreeSet::new();
    let mut causes = BTreeMap::new();
    let origin = hash(judgment)?;
    let actor = text(get(get(observation, "scope_context"), "developer_id"))?;
    let mut resolutions = object([] as [(&str, Value); 0]);
    if version2 {
        let proposed = judgment
            .get("resolutions")
            .cloned()
            .unwrap_or_else(|| object([] as [(&str, Value); 0]));
        let known_causes = array(get(&inspection, "known_causes"))?;
        let known_ids = known_causes
            .iter()
            .map(|known| Ok(text(get(known, "id"))?.to_owned()))
            .collect::<Result<BTreeSet<_>>>()?;
        if fields(&proposed).is_err()
            || fields(&proposed)?
                .iter()
                .any(|(key, _)| !known_ids.contains(key))
        {
            return Err(err("Resolve only actual retained detected disputes"));
        }
        let qualified = crate::sharing_hold_mapping::qualified_current(
            &rows,
            values,
            root,
            text(get(observation, "project_id"))?,
        )?;
        for known in known_causes {
            let cause = get(known, "cause");
            let id = text(get(known, "id"))?;
            if let Some(proposal) = proposed.get(id) {
                let resolution = crate::sharing_hold_mapping::validate_resolution(
                    proposal, known, &rows, &qualified, false,
                )?;
                let involved = array(get(known, "sources"))?
                    .iter()
                    .chain(array(get(&resolution, "evidence"))?)
                    .map(|key| Ok(text(key)?.to_owned()))
                    .collect::<Result<BTreeSet<_>>>()?;
                if involved.iter().any(|key| !rows.contains_key(key)) {
                    return Err(err(
                        "Resolution must read and review all actual claims and stronger evidence without unresolved interactions",
                    ));
                }
                for (i, left) in involved.iter().enumerate() {
                    for right in involved.iter().skip(i + 1) {
                        let classification = decisions
                            .get(&sorted_pair(left, right))
                            .and_then(|decision| get(decision, "classification").as_str());
                        if classification
                            .is_none_or(|value| ["uncertain", "contradiction"].contains(&value))
                        {
                            return Err(err(
                                "Resolution must read and review all actual claims and stronger evidence without unresolved interactions",
                            ));
                        }
                    }
                }
                set(&mut resolutions, id, resolution)?;
                continue;
            }
            if get(known, "resolved").as_bool() == Some(true) {
                continue;
            }
            remember(
                &mut causes,
                array(get(cause, "sources"))?
                    .iter()
                    .map(|key| text(key).map(str::to_owned))
                    .collect::<Result<Vec<_>>>()?,
                text(get(cause, "classification"))?,
                text(get(cause, "reason"))?,
                text(get(cause, "origin"))?,
            )?;
            let claims = array(get(known, "sources"))?
                .iter()
                .map(|key| {
                    rows.get(text(key)?)
                        .ok_or_else(|| err("Missing retained claim"))
                })
                .collect::<Result<Vec<_>>>()?;
            for row in &claims {
                hold(row, &mut held, &mut blocked)?;
            }
            let mut missing = Vec::new();
            for key in array(get(known, "missing_sources"))? {
                let reference = get(get(known, "references"), text(key)?);
                hold(reference, &mut held, &mut blocked)?;
                missing.push(reference.clone());
            }
            let others = peers(&claims, actor);
            peer.extend(others.iter().cloned());
            conflicts.push(object([
                ("classification", get(cause, "classification").clone()),
                (
                    "claims",
                    Value::Array(claims.into_iter().cloned().collect()),
                ),
                ("reason", get(cause, "reason").clone()),
                ("missing_source_references", Value::Array(missing)),
                ("other_developer_ids", strings(others)),
                ("detecting_developer_id", string(actor)),
                ("retained_prior_review", Value::Bool(true)),
            ]));
        }
        let withheld = array(get(&inspection, "withheld_sources"))?
            .iter()
            .map(|key| Ok(text(key)?.to_owned()))
            .collect::<Result<BTreeSet<_>>>()?;
        let mut unresolved = withheld.clone();
        for (key, selection) in fields(get(&inspection, "selections"))? {
            if get(selection, "uncertain").as_bool() == Some(true) {
                unresolved.insert(key.clone());
            }
        }
        for key in &unresolved {
            let row = rows
                .get(key)
                .ok_or_else(|| err("Missing unresolved source"))?;
            hold(row, &mut held, &mut blocked)?;
            remember(
                &mut causes,
                [key.clone()],
                if withheld.contains(key) {
                    "withheld"
                } else {
                    "uncertain-selection"
                },
                "Selected evidence remains withheld or plausible-match selection remains uncertain; clarify before relying on it",
                &origin,
            )?;
        }
        for (key, selection) in fields(get(&inspection, "selections"))? {
            if array(get(selection, "matched"))?
                .iter()
                .any(|key| key.as_str().is_some_and(|key| unresolved.contains(key)))
            {
                hold(
                    rows.get(key)
                        .ok_or_else(|| err("Missing affected selected source"))?,
                    &mut held,
                    &mut blocked,
                )?;
            }
        }
    }
    for ((a, b), decision) in &decisions {
        let left = rows
            .get(a)
            .ok_or_else(|| err("Missing reviewed left source"))?;
        let right = rows
            .get(b)
            .ok_or_else(|| err("Missing reviewed right source"))?;
        let classification = text(get(decision, "classification"))?;
        if classification == "disjoint-conditions" && !disjoint(left, right)? {
            return Err(err(
                "Disjoint conditions require an actual known non-overlap; unknown is not disjoint",
            ));
        }
        if classification == "contradiction" && disjoint(left, right)? {
            return Err(err(
                "Disjoint claims need their distinct conditions preserved",
            ));
        }
        if classification == "lifecycle" {
            let (a, b) = (get(left, "metadata"), get(right, "metadata"));
            let terminal = version2 && crate::sharing_frontier::terminal_relation(left, right);
            let linked = if nonnull(a) && nonnull(b) {
                [(a, b), (b, a)].iter().any(|(old, new)| {
                    let old_ref = object([
                        ("id", get(old, "id").clone()),
                        ("version", get(old, "version").clone()),
                    ]);
                    let new_ref = object([
                        ("id", get(new, "id").clone()),
                        ("version", get(new, "version").clone()),
                    ]);
                    ["supersedes", "corrects"].iter().any(|name| {
                        get(new, name)
                            .as_array()
                            .is_some_and(|refs| refs.iter().any(|r| retrieval::same(r, &old_ref)))
                    }) || get(old, "superseded-by")
                        .as_array()
                        .is_some_and(|refs| refs.iter().any(|r| retrieval::same(r, &new_ref)))
                })
            } else {
                false
            };
            if !terminal && !linked {
                return Err(err(
                    "A lifecycle distinction needs actual retained version links",
                ));
            }
        }
        if classification == "equivalent" {
            equivalents.insert((a.clone(), b.clone()));
        }
        if ["contradiction", "uncertain"].contains(&classification) {
            if version2 {
                remember(
                    &mut causes,
                    [a.clone(), b.clone()],
                    classification,
                    text(get(decision, "reason"))?,
                    &origin,
                )?;
            }
            hold(left, &mut held, &mut blocked)?;
            hold(right, &mut held, &mut blocked)?;
            let others = peers(&[left, right], actor);
            peer.extend(others.iter().cloned());
            conflicts.push(object([
                ("classification", string(classification)),
                ("claims", Value::Array(vec![left.clone(), right.clone()])),
                ("reason", get(decision, "reason").clone()),
                ("other_developer_ids", strings(others)),
                ("detecting_developer_id", string(actor)),
            ]));
        }
    }
    let groups = array(get(judgment, "consolidations"))
        .map_err(|_| err("Duplicate groups require an explicit bounded list"))?;
    let dependencies = fields(get(judgment, "dependencies"))
        .map_err(|_| err("Dependent updates need an explicit path graph"))?;
    let mut actual = BTreeSet::new();
    for (_, files) in fields(values)? {
        actual.extend(keys(files)?);
    }
    for (path, required) in dependencies {
        if !actual.contains(path)
            || required.as_array().is_none_or(|rows| {
                rows.iter()
                    .any(|p| p.as_str().is_none_or(|p| !actual.contains(p)))
            })
        {
            return Err(err("Dependencies must name actual bank paths"));
        }
    }
    let mut seeds = held
        .iter()
        .filter_map(|(path, component, target)| {
            if ["route-prose", "omission"].contains(&component.as_str()) {
                target.clone()
            } else if component == "body" {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    loop {
        let mut payload = BTreeSet::new();
        for path in &seeds {
            for (_, files) in fields(values)? {
                payload.extend(child_paths(files, path)?);
            }
        }
        for path in &payload {
            held.insert((path.clone(), "body".into(), None));
        }
        blocked.extend(payload);
        let mut expanded = seeds.clone();
        for (path, required) in dependencies {
            if array(required)?
                .iter()
                .any(|p| p.as_str().is_some_and(|p| blocked.contains(p)))
            {
                expanded.insert(path.clone());
            }
        }
        if expanded == seeds {
            break;
        }
        seeds = expanded;
    }
    let mut grouped = BTreeSet::new();
    for group in groups {
        let sources = array(get(group, "sources"))?;
        for (i, left) in sources.iter().enumerate() {
            for right in sources.iter().skip(i + 1) {
                grouped.insert(sorted_pair(text(left)?, text(right)?));
            }
        }
    }
    for pair in equivalents {
        let a = rows
            .get(&pair.0)
            .ok_or_else(|| err("Missing equivalent source"))?;
        let b = rows
            .get(&pair.1)
            .ok_or_else(|| err("Missing equivalent source"))?;
        if blocked.contains(text(get(a, "path"))?)
            || blocked.contains(text(get(b, "path"))?)
            || get(a, "path") == get(b, "path")
        {
            continue;
        }
        if nonnull(get(a, "metadata"))
            && nonnull(get(b, "metadata"))
            && get(a, "lifecycle").as_str() == Some("active")
            && get(b, "lifecycle").as_str() == Some("active")
            && crate::consolidation::AXES.iter().all(|axis| {
                retrieval::same(get(get(a, "metadata"), axis), get(get(b, "metadata"), axis))
            })
            && !grouped.contains(&pair)
        {
            return Err(err(
                "Equivalent current typed learning requires explicit provenance-preserving consolidation",
            ));
        }
    }
    let mut local = get(values, "local").clone();
    let mut landed = get(values, "landed").clone();
    let mut baseline = get(values, "base").clone();
    for path in bank::CONTEXTS.iter() {
        if blocked.contains(*path) {
            continue;
        }
        let (Some(base), Some(left), Some(right)) =
            (baseline.get(path), local.get(path), landed.get(path))
        else {
            continue;
        };
        if left != right && left != base && right != base {
            let pair = sorted_pair(
                source_for(&rows, path, "body", "local")?,
                source_for(&rows, path, "body", "landed")?,
            );
            if !decisions
                .get(&pair)
                .and_then(|decision| get(decision, "classification").as_str())
                .is_some_and(|class| ["compatible", "equivalent"].contains(&class))
            {
                return Err(err(
                    "Concurrent same-file prose needs explicit compatible meaning review",
                ));
            }
            let body = string(compatible_prose(text(base)?, text(left)?, text(right)?)?);
            for files in [&mut local, &mut landed, &mut baseline] {
                set(files, path, body.clone())?;
            }
        }
    }
    let mut structural = container_paths(&baseline)?;
    structural.extend(container_paths(&local)?);
    structural.extend(container_paths(&landed)?);
    for path in structural {
        if held.iter().any(|(held_path, component, _)| {
            held_path == &path && ["introduction", "body"].contains(&component.as_str())
        }) {
            continue;
        }
        let (Some(base), Some(left), Some(right)) =
            (baseline.get(&path), local.get(&path), landed.get(&path))
        else {
            continue;
        };
        let prefixes = [
            introduction(text(base)?)?,
            introduction(text(left)?)?,
            introduction(text(right)?)?,
        ];
        if prefixes.get(1) != prefixes.get(2)
            && prefixes.get(1) != prefixes.first()
            && prefixes.get(2) != prefixes.first()
        {
            let pair = sorted_pair(
                source_for(&rows, &path, "introduction", "local")?,
                source_for(&rows, &path, "introduction", "landed")?,
            );
            if !decisions
                .get(&pair)
                .and_then(|decision| get(decision, "classification").as_str())
                .is_some_and(|class| ["compatible", "equivalent"].contains(&class))
            {
                return Err(err(
                    "Concurrent container introductions require actual compatible meaning review",
                ));
            }
            let [base, left, right] = prefixes;
            let prefix = compatible_prose(base, left, right)?;
            for files in [&mut baseline, &mut local, &mut landed] {
                let content = replace_introduction(text(get(files, &path))?, &prefix)?;
                set(files, &path, string(content))?;
            }
        }
    }
    let refresh_land = if held.is_empty() {
        landed.clone()
    } else {
        mask(&landed, &baseline, &held)?
    };
    let publish_local = if held.is_empty() {
        local.clone()
    } else {
        mask(&local, &baseline, &held)?
    };
    let refresh = recovery::merge_files(&baseline, &local, &refresh_land, version2)?;
    let publish = recovery::merge_files(&baseline, &publish_local, &landed, version2)?;
    if version2 {
        for (before, after) in [
            (get(values, "local"), &refresh),
            (get(values, "landed"), &publish),
        ] {
            for (path, body) in fields(after)? {
                if before.get(path) != Some(body)
                    && !array(get(
                        &security::scan(&string(format!("{path}\n{}", text(body)?)), 16384)?,
                        "reasons",
                    ))?
                    .is_empty()
                {
                    return Err(err(
                        "Unsafe changed payload remains held; resolve it before exchange",
                    ));
                }
            }
        }
    }
    let operation_id = operation_id(&origin)?;
    let refresh = consolidate(
        &refresh,
        get(judgment, "consolidations"),
        &rows,
        &decisions,
        observation,
        &operation_id,
        get(judgment, "recorded_at"),
        &blocked,
    )?;
    let publish = consolidate(
        &publish,
        get(judgment, "consolidations"),
        &rows,
        &decisions,
        observation,
        &operation_id,
        get(judgment, "recorded_at"),
        &blocked,
    )?;
    let ids = rows
        .values()
        .filter_map(|row| get(get(row, "metadata"), "id").as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let audit = security::audit(
        root,
        &operation_id,
        "retrieval",
        "checked",
        get(observation, "scope_context"),
        &["external"],
        &[],
        &ids.into_iter().collect::<Vec<_>>(),
        "policy-check",
        None,
    )?;
    preserve_invalidation(values, &refresh)?;
    preserve_invalidation(values, &publish)?;
    final_sources(root, observation, binding, owned_lock)?;
    let mut proof = object([
        (
            "schema",
            string(if version2 {
                crate::sharing_frontier::PROOF
            } else {
                "agent-memory/semantic-sharing-proof-v1"
            }),
        ),
        ("judgment", judgment.clone()),
        ("refresh_hashes", tx::file_hashes(&refresh)?),
        ("publish_hashes", tx::file_hashes(&publish)?),
        ("blocked_paths", strings(blocked.iter().cloned())),
        (
            "blocked_components",
            Value::Array(
                held.iter()
                    .map(|(path, component, target)| {
                        object([
                            ("path", string(path)),
                            ("component", string(component)),
                            ("target_path", target.as_ref().map_or(Value::Null, string)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    if version2 {
        set(
            &mut proof,
            "held_causes",
            Value::Array(causes.into_values().collect()),
        )?;
        set(&mut proof, "known_resolutions", resolutions.clone())?;
    }
    if encoded(&proof)?.len() > if version2 { limits::MAX_PRIVATE } else { LIMIT } {
        return Err(err("Complete semantic proof exceeds bound; no truncation"));
    }
    if version2 {
        let mut references = object([] as [(&str, Value); 0]);
        for (key, row) in &rows {
            let mut reference = object(
                ["path", "component", "target_path", "source_sha256_by_side"]
                    .map(|name| (name, get(row, name).clone())),
            );
            let meta = if nonnull(get(row, "metadata")) {
                get(row, "metadata")
            } else if get(row, "format").as_str() == Some("standalone-record") {
                get(row, "witness_metadata")
            } else {
                &Value::Null
            };
            if nonnull(meta) {
                set(
                    &mut reference,
                    "record",
                    object([
                        ("id", get(meta, "id").clone()),
                        ("version", get(meta, "version").clone()),
                    ]),
                )?;
            }
            set(&mut references, key, reference)?;
        }
        for known in array(get(&inspection, "known_causes"))? {
            for (key, value) in fields(get(known, "references"))? {
                set(&mut references, key, value.clone())?;
            }
        }
        let verify = |owner: &[u8]| final_sources(root, observation, binding, Some(owner));
        crate::sharing_holds::record(
            root,
            text(get(observation, "project_id"))?,
            &crate::cli::pack_root()?,
            &proof,
            &references,
            crate::sharing_holds::RecordOptions {
                resolutions: Some(&resolutions),
                owned_lock,
                verify: Some(&verify),
                expected_inventory: Some(text(get(&inspection, "known_inventory_sha256"))?),
            },
        )?;
    }
    Ok(object([
        ("schema", string("agent-memory/semantic-sharing-result-v1")),
        ("operation", string("review")),
        ("proof_sha256", string(hash(&proof)?)),
        ("proof", proof),
        ("audit", audit),
        ("audit_writes_performed", Value::Bool(true)),
        ("refresh_files", refresh),
        ("publish_files", publish),
        ("conflicts", Value::Array(conflicts)),
        ("blocked_paths", strings(blocked.iter().cloned())),
        (
            "blocked_introduction_paths",
            strings(
                held.iter()
                    .filter(|(_, component, _)| component == "introduction")
                    .map(|(path, _, _)| path.clone())
                    .collect::<BTreeSet<_>>(),
            ),
        ),
        ("detecting_developer_id", string(actor)),
        ("other_developer_ids", strings(peer.iter().cloned())),
        (
            "recommend_other_developer_clarification",
            Value::Bool(!peer.is_empty()),
        ),
        ("messages_sent", Value::Bool(false)),
        ("independent_updates_may_continue", Value::Bool(true)),
        ("conflicts_pending", Value::Bool(!held.is_empty())),
        ("semantic_truth_verified", Value::Bool(false)),
        ("semantic_completeness_verified", Value::Bool(false)),
        ("judgment_is_agent_assertion", Value::Bool(true)),
        ("remote_freshness_verified", Value::Bool(true)),
        ("publication_verified", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("git_metadata_writes_performed", Value::Bool(true)),
        ("refresh_applied", Value::Bool(false)),
    ]))
}
pub fn check(repo: &Path, proof: &Value, owned_lock: Option<&[u8]>) -> Result<Value> {
    let mut names = vec![
        "schema",
        "judgment",
        "refresh_hashes",
        "publish_hashes",
        "blocked_paths",
        "blocked_components",
    ];
    let schema = get(proof, "schema").as_str();
    if schema == Some(crate::sharing_frontier::PROOF) {
        names.extend(["held_causes", "known_resolutions"]);
    }
    if !exact(proof, &names)
        || !matches!(
            schema,
            Some(
                "agent-memory/semantic-sharing-proof-v1" | "agent-memory/semantic-sharing-proof-v2"
            )
        )
    {
        return Err(err("Use the complete actual semantic proof"));
    }
    let mut value = review(repo, get(proof, "judgment"), owned_lock)?;
    if encoded(get(&value, "proof"))? != encoded(proof)? {
        return Err(err(
            "Semantic proof does not describe current reviewed sources and outputs",
        ));
    }
    set(&mut value, "operation", string("check"))?;
    set(&mut value, "source_observation_current", Value::Bool(true))?;
    Ok(value)
}
pub fn read_input(path: &Path, version2: bool) -> Result<Value> {
    let limit = if version2 { limits::MAX_PRIVATE } else { LIMIT };
    let value =
        crate::developer_identity::load_with_limit(&limits::read_bytes(path, limit)?, limit)?;
    let candidate = value.get("proof").unwrap_or(&value);
    if get(candidate, "schema").as_str().is_some_and(|schema| {
        [
            crate::sharing_frontier::PROOF,
            crate::sharing_frontier::JUDGMENT,
            crate::sharing_frontier::REQUEST,
            crate::sharing_frontier::INSPECTION,
        ]
        .contains(&schema)
    }) {
        return Ok(value);
    }
    if version2 {
        return Err(err(
            "Explicit v2 reader requires an actual v2 source-bound format",
        ));
    }
    Ok(value)
}
pub fn compact_review(repo: &Path, manifest: &Value, decisions: &Value) -> Result<Value> {
    if get(manifest, "schema").as_str() != Some(crate::sharing_frontier::INSPECTION)
        || !(exact(
            decisions,
            &["pages", "pairs", "consolidations", "rationale"],
        ) || exact(
            decisions,
            &[
                "pages",
                "pairs",
                "consolidations",
                "rationale",
                "resolutions",
            ],
        ))
    {
        return Err(err(
            "Use the selected inspection and explicit page and pair judgments",
        ));
    }
    let current = crate::sharing_frontier::inspect(repo, manifest.get("request"), None)?;
    if !retrieval::same(&current, manifest)
        || !array(get(manifest, "pending_selections"))?.is_empty()
    {
        return Err(err(
            "Selected inspection changed or still needs semantic selection",
        ));
    }
    let pages = Value::Array(
        array(get(manifest, "pages"))?
            .iter()
            .map(|page| get(page, "sha256").clone())
            .collect(),
    );
    if get(decisions, "pages") != &pages || get(decisions, "pairs").as_array().is_none() {
        return Err(err("Explicitly acknowledge each actual page in order"));
    }
    let required = array(get(manifest, "required_pairs"))?;
    let mut pairs = Vec::new();
    let mut seen = BTreeSet::new();
    for decision in array(get(decisions, "pairs"))? {
        let index = get(decision, "index")
            .as_int()
            .and_then(|i| i.to_string().parse::<usize>().ok());
        let Some(index) = index.filter(|index| {
            *index < required.len()
                && !seen.contains(index)
                && exact(decision, &["index", "classification", "reason"])
        }) else {
            return Err(err(
                "Every exact pair requires one explicit indexed judgment",
            ));
        };
        seen.insert(index);
        let pair = array(
            required
                .get(index)
                .ok_or_else(|| err("Missing indexed source pair"))?,
        )?;
        let [left, right] = pair else {
            return Err(err("Missing indexed source pair"));
        };
        pairs.push(object([
            ("left", left.clone()),
            ("right", right.clone()),
            ("classification", get(decision, "classification").clone()),
            ("reason", get(decision, "reason").clone()),
        ]));
    }
    if seen.len() != required.len() {
        return Err(err(
            "Every exact pair requires one explicit indexed judgment",
        ));
    }
    let mut judgment = object([
        ("schema", string(crate::sharing_frontier::JUDGMENT)),
        ("request", get(manifest, "request").clone()),
        ("binding", get(manifest, "binding").clone()),
        ("manifest_sha256", get(manifest, "manifest_sha256").clone()),
        ("pages", pages),
        ("pairs", Value::Array(pairs)),
        (
            "dependencies",
            get(get(manifest, "request"), "dependencies").clone(),
        ),
        ("consolidations", get(decisions, "consolidations").clone()),
        ("rationale", get(decisions, "rationale").clone()),
        ("recorded_at", string(crate::host::utc_now_isoformat()?)),
    ]);
    if let Some(resolutions) = decisions.get("resolutions") {
        set(&mut judgment, "resolutions", resolutions.clone())?;
    }
    review(repo, &judgment, None)
}

pub fn command_result(operation: &str, outcome: Result<Value>) -> lifecycle::CommandResult {
    use crate::error::Kind;
    match outcome {
        Ok(value) => lifecycle::CommandResult::json(value, 0, crate::codec::json::INDENT2_ASCII),
        Err(error) => {
            let (component,code,repair,known) = match error.kind() {
                Kind::PrivateOutput => ("output","private-output-invalid",error.to_string(),true),
                Kind::SharingAuthority { code,component,repair } => (component.as_str(),code.as_str(),repair.clone(),true),
                Kind::HoldFailure { code,repair } => ("known-dispute-state",code.as_str(),repair.clone(),true),
                _ => (operation,"semantic-review-unverified","Preserve the bank, original request and pending evidence. Run am doctor for this repository, inspect the failed component, then repeat source inspection and review; do not substitute an empty bank or a compatibility judgment.".into(),false),
            };
            let reason = if known || matches!(error.kind(), Kind::BankCapacity { .. }) {
                error.to_string()
            } else {
                "Incomplete, changed or unsafe semantic review; retain local work and inspect actual sources".into()
            };
            let mut value = object([
                ("schema", string("agent-memory/semantic-sharing-result-v1")),
                ("operation", string(operation)),
                ("publication_verified", Value::Bool(false)),
                ("canonical_writes_performed", Value::Bool(false)),
                ("remote_freshness_verified", Value::Bool(false)),
                ("failure_component", string(component)),
                ("failure_code", string(code)),
                ("error", string(reason)),
                ("repair", string(repair)),
            ]);
            if let Value::Object(pairs) = &mut value {
                if let Value::Object(details) = crate::bank_capacity::details(&error) {
                    pairs.extend(details);
                }
            }
            lifecycle::CommandResult::json(value, 1, crate::codec::json::DEFAULT)
        }
    }
}
