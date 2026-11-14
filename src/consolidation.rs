//! Source-bound, meaning-preserving maintenance; no daemon or procedure promotion.
use crate::bank_contract as bank;
use crate::bank_transactions as tx;
use crate::bank_writes as writes;
use crate::codec::{
    frontmatter,
    json::{self, Value},
};
use crate::developer_identity::{encoded, sha256_hex as digest, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::layout_resolution::{field, has_keys, items, set, text, with};
use crate::lifecycle::{self, get, object, string};
use crate::{
    admission, bank_layout, comparison, host, layout_resolution, records, retrieval, security,
    sharing_limits,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const INPUT_LIMIT: usize = 262144;
const INPUT: &[&str] = &[
    "comparison_request",
    "comparison_proof",
    "capture",
    "context",
];
const PLAN_INPUT: &[&str] = &[
    "comparison_request",
    "comparison_judgment",
    "capture",
    "context",
];
const EVIDENCE: &[&str] = &[
    "comparison_request",
    "comparison_proof",
    "capture",
    "context",
    "recorded_at",
    "git_context",
    "unit_resolution_claims",
    "comparison_proof_sha256",
    "reference_map_sha256",
];
pub(crate) const AXES: &[&str] = &[
    "kind",
    "scope",
    "conditions",
    "applicability",
    "valid-from",
    "valid-until",
    "trust",
    "visibility",
];
const MAX_CHANGED_FILE: usize = 16384;
const MAX_PROVENANCE: usize = 64;
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn same(a: &Value, b: &Value) -> Result<bool> {
    Ok(encoded(a)? == encoded(b)?)
}
fn render(metadata: &Value) -> Result<String> {
    frontmatter::write(
        metadata
            .as_object()
            .ok_or_else(|| err("Invalid record metadata"))?,
    )
}
fn remove(value: &mut Value, key: &str) {
    if let Value::Object(rows) = value {
        rows.retain(|(k, _)| k != key);
    }
}
pub fn result(operation: &str, fields: Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/consolidation-result-v1")),
        ("operation", string(operation)),
        ("procedure_promotion_performed", Value::Bool(false)),
        ("semantic_truth_verified", Value::Bool(false)),
        ("main_freshness_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
    ]);
    bank::merge(&mut value, &fields)?;
    Ok(value)
}
pub fn reference(metadata: &Value) -> Result<Value> {
    Ok(object([
        ("id", field(metadata, "id")?.clone()),
        ("version", field(metadata, "version")?.clone()),
    ]))
}
pub fn child_id(operation_id: &str, label: &str) -> Result<String> {
    let hash = digest(format!("agent-memory/consolidation/v1\n{operation_id}\n{label}").as_bytes());
    let mut output = String::with_capacity(36);
    for (index, c) in hash.chars().take(32).enumerate() {
        if matches!(index, 8 | 12 | 16 | 20) {
            output.push('-');
        }
        output.push(if index == 12 {
            '4'
        } else if index == 16 {
            char::from_digit(
                (c.to_digit(16).ok_or_else(|| err("Invalid digest"))? & 3) | 8,
                16,
            )
            .ok_or_else(|| err("Invalid UUID variant"))?
        } else {
            c
        });
    }
    Ok(output)
}
pub fn union(values: impl IntoIterator<Item = Value>) -> Result<Value> {
    let mut positions = BTreeMap::new();
    let mut output = Vec::new();
    for value in values {
        let key = encoded(&value)?;
        if let Some(index) = positions.get(&key) {
            *output
                .get_mut(*index)
                .ok_or_else(|| err("Invalid union position"))? = value;
        } else {
            positions.insert(key, output.len());
            output.push(value);
        }
    }
    Ok(Value::Array(output))
}
pub fn fingerprint(metadata: &Value) -> Result<Vec<u8>> {
    let mut values = Value::Object(
        [
            "kind",
            "scope",
            "conditions",
            "applicability",
            "source",
            "origin",
        ]
        .into_iter()
        .map(|k| Ok((k.into(), field(metadata, k)?.clone())))
        .collect::<Result<_>>()?,
    );
    let mut evidence = items(field(metadata, "evidence")?)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| err("Invalid evidence"))
        })
        .collect::<Result<Vec<_>>>()?;
    evidence.sort();
    set(
        &mut values,
        "evidence",
        Value::Array(evidence.into_iter().map(string).collect()),
    );
    encoded(&values)
}
pub fn input_value(value: &Value, evidence: bool) -> Result<Value> {
    if !has_keys(value, if evidence { EVIDENCE } else { INPUT }) {
        return Err(err(
            "Maintenance requires exact current comparison, optional qualified capture and actual context",
        ));
    }
    comparison::request_value(field(value, "comparison_request")?)?;
    let recall = object([
        (
            "project_id",
            field(field(value, "comparison_request")?, "project_id")?.clone(),
        ),
        ("query", string("")),
        ("ids", Value::Array(vec![])),
        ("paths", Value::Array(vec![])),
        ("context", field(value, "context")?.clone()),
        (
            "kinds",
            field(
                field(field(&records::MODEL, "properties")?, "kind")?,
                "enum",
            )?
            .clone(),
        ),
        ("minimum_trust", string("reviewed")),
        ("limit", Value::Int(64.into())),
        ("include_context", Value::Bool(false)),
    ]);
    retrieval::request_value(&recall)?;
    if evidence {
        if field(value, "git_context")?.as_object().is_none() {
            return Err(err("Maintenance must pin actual Git context"));
        }
        if lifecycle::time_value(field(value, "recorded_at")?)?
            > lifecycle::time_value(&string(host::utc_now_isoformat()?))?
        {
            return Err(err("Maintenance event cannot be future dated"));
        }
    }
    Ok(recall)
}
fn source_context(
    repo: &Path,
    value: &Value,
    owner: Option<&[u8]>,
) -> Result<(Value, Option<Value>)> {
    let recall = input_value(value, value.get("recorded_at").is_some())?;
    let captured = if field(value, "capture")? == &Value::Null {
        None
    } else {
        Some(admission::capture_value(
            repo,
            field(value, "capture")?,
            owner,
        )?)
    };
    if let Some(c) = &captured {
        if !same(
            field(field(c, "request")?, "candidate")?,
            field(field(value, "comparison_request")?, "candidate")?,
        )? {
            return Err(err("Consolidate the exact qualified captured learning"));
        }
    }
    Ok((recall, captured))
}
pub struct Inspection {
    pub compared: Value,
    pub plan: Option<Value>,
    pub captured: Option<Value>,
}
pub fn inspect(repo: &Path, value: &Value, owner: Option<&[u8]>) -> Result<Inspection> {
    let (recall, captured) = source_context(repo, value, owner)?;
    let compared = comparison::check(
        repo,
        field(value, "comparison_request")?,
        field(value, "comparison_proof")?,
        owner,
    )?;
    qualify(repo, value, recall, captured, compared, owner)
}
fn qualify(
    repo: &Path,
    value: &Value,
    mut recall: Value,
    captured: Option<Value>,
    compared: Value,
    owner: Option<&[u8]>,
) -> Result<Inspection> {
    let (root, _) = writes::bank_path(repo)?;
    if let Some(context) = value.get("git_context") {
        if !same(&retrieval::git_context(&root)?, context)? {
            return Err(err("The observed checkout changed before maintenance"));
        }
    }
    if !matches!(
        get(&compared, "disposition").as_str(),
        Some("update-existing" | "consolidate-existing")
    ) {
        return Ok(Inspection {
            compared,
            plan: None,
            captured,
        });
    }
    let targets = items(field(field(&compared, "proof")?, "targets")?)?;
    if !(1..=8).contains(&targets.len()) || targets.iter().any(|v| get(v, "id") == &Value::Null) {
        return Err(err(
            "Automatic maintenance requires one bounded group of typed existing learning",
        ));
    }
    set(
        &mut recall,
        "paths",
        Value::Array(targets.iter().map(|v| get(v, "path").clone()).collect()),
    );
    let qualified = retrieval::retrieve(repo, &recall, owner, &[])?;
    let rows = items(field(&qualified, "results")?)?;
    let paths: BTreeSet<&str> = rows
        .iter()
        .map(|v| text(v, "path"))
        .collect::<Result<_>>()?;
    let expected: BTreeSet<&str> = targets
        .iter()
        .map(|v| text(v, "path"))
        .collect::<Result<_>>()?;
    if paths != expected
        || get(&qualified, "bank_observation_sha256")
            != get(get(&compared, "observation"), "bank_observation_sha256")
    {
        return Err(err(
            "Every maintenance source must remain current and qualified; do not revive missing or inactive sources",
        ));
    }
    for target in targets {
        let row = rows
            .iter()
            .find(|v| get(v, "path") == get(target, "path"))
            .ok_or_else(|| err("Missing qualified source"))?;
        if get(row, "source_sha256") != get(target, "sha256") {
            return Err(err(
                "A source version changed between comparison and qualification",
            ));
        }
    }
    let candidate = field(field(value, "comparison_request")?, "candidate")?;
    let primary = if captured.is_none() {
        let selected: Vec<&Value> = targets
            .iter()
            .filter(|v| get(v, "id") == get(get(candidate, "metadata"), "id"))
            .collect();
        let primary = selected.first().ok_or_else(|| {
            err("A wording-only proposal must retain the exact selected existing metadata")
        })?;
        if selected.len() != 1
            || !same(
                field(primary, "required_preserved_metadata")?,
                field(candidate, "metadata")?,
            )?
        {
            return Err(err(
                "A wording-only proposal must retain the exact selected existing metadata",
            ));
        }
        *primary
    } else {
        targets
            .first()
            .ok_or_else(|| err("Missing maintenance source"))?
    };
    let metadata = field(primary, "required_preserved_metadata")?;
    for target in targets {
        for key in AXES {
            if !same(
                field(field(target, "required_preserved_metadata")?, key)?,
                field(metadata, key)?,
            )? {
                return Err(err(
                    "Distinct validity, trust, visibility, type or applicability must remain separate",
                ));
            }
        }
    }
    if captured.is_some() {
        for key in ["valid-from", "valid-until"] {
            if !same(
                field(field(candidate, "metadata")?, key)?,
                field(metadata, key)?,
            )? {
                return Err(err(
                    "A captured observation with distinct valid time must remain separate",
                ));
            }
        }
    }
    let plan = object([
        ("primary", primary.clone()),
        ("targets", Value::Array(targets.to_vec())),
        ("qualified", qualified),
    ]);
    Ok(Inspection {
        compared,
        plan: Some(plan),
        captured,
    })
}
pub fn route(path: &str, purpose: &str, metadata: &Value, history: bool) -> Result<Value> {
    Ok(object([
        ("path", string(path)),
        ("purpose", string(purpose)),
        (
            "scope",
            if history {
                string("repository")
            } else {
                field(metadata, "scope")?.clone()
            },
        ),
        (
            "lifecycle",
            if history {
                string("active")
            } else {
                field(metadata, "status")?.clone()
            },
        ),
        (
            "read-when",
            string("When inspecting this learning and its provenance"),
        ),
    ]))
}
fn updated(rows: Vec<Value>, changes: &BTreeMap<String, Value>) -> Vec<Value> {
    rows.into_iter()
        .map(|mut row| {
            if let Some(lifecycle) = get(&row, "path").as_str().and_then(|p| changes.get(p)) {
                set(&mut row, "lifecycle", lifecycle.clone());
            }
            row
        })
        .collect()
}
pub fn routed(
    base: &Value,
    checked: &Value,
    changes: &BTreeMap<String, Value>,
    additions: Vec<Value>,
    at: &Value,
) -> Result<Value> {
    let mut files = base.clone();
    let (_, routes, _) = bank::parse_router(text(base, "MEMORY.md")?)?;
    let allowed: Vec<&str> = field(&bank::SPEC, "properties")?
        .as_object()
        .ok_or_else(|| err("Invalid router schema"))?
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    let (root_metadata, root_body) = frontmatter::read(text(base, "MEMORY.md")?, &allowed)?;
    let mut root_metadata = Value::Object(root_metadata);
    for entry in items(field(checked, "entries")?)? {
        if get(entry, "format").as_str() != Some("topic-index") {
            continue;
        }
        let path = text(entry, "path")?;
        let allowed: Vec<&str> = field(&bank_layout::SPEC, "properties")?
            .as_object()
            .ok_or_else(|| err("Invalid index schema"))?
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        let (header, body) = frontmatter::read(text(base, path)?, &allowed)?;
        let mut header = Value::Object(header);
        let rows = bank_layout::parse_index(path, &header, body)?;
        if rows.iter().any(|r| {
            get(r, "path")
                .as_str()
                .is_some_and(|p| changes.contains_key(p))
        }) {
            set(&mut header, "updated", at.clone());
            let (prefix, _) = body
                .split_once("\n## Routes\n")
                .ok_or_else(|| err("Index requires routes section"))?;
            set(
                &mut files,
                path,
                string(format!(
                    "{}{prefix}\n## Routes\n\n{}",
                    render(&header)?,
                    bank::route_table(&updated(rows, changes), path)?
                )),
            );
        }
    }
    set(&mut root_metadata, "router-updated", at.clone());
    let (prefix, tail) = root_body
        .split_once("\n## Routes\n")
        .ok_or_else(|| err("Router requires routes section"))?;
    let (_, omissions) = tail
        .split_once("\n## Omitted contexts\n")
        .ok_or_else(|| err("Router requires omitted contexts section"))?;
    let mut rows = updated(routes, changes);
    rows.extend(additions);
    set(
        &mut files,
        "MEMORY.md",
        string(format!(
            "{}{prefix}\n## Routes\n\n{}\n## Omitted contexts\n{omissions}",
            render(&root_metadata)?,
            bank::route_table(&rows, "MEMORY.md")?
        )),
    );
    Ok(files)
}
fn version_key(metadata: &Value) -> Result<(String, String)> {
    Ok((
        text(metadata, "id")?.to_owned(),
        field(metadata, "version")?
            .as_int()
            .ok_or_else(|| err("Invalid record version"))?
            .as_str()
            .to_owned(),
    ))
}
fn next_version(version: &str) -> Result<Value> {
    if version.is_empty() || !version.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err("Invalid record version"));
    }
    let mut bytes = version.as_bytes().to_vec();
    let mut carry = true;
    for digit in bytes.iter_mut().rev() {
        if !carry {
            break;
        }
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            carry = false;
        }
    }
    if carry {
        bytes.insert(0, b'1');
    }
    let value = String::from_utf8(bytes).map_err(|_| err("Invalid record version"))?;
    Ok(Value::Int(json::Integer::new(&value)?))
}
pub fn build(
    base: &Value,
    evidence: &Value,
    operation_id: &str,
    plan: &Value,
    captured: Option<&Value>,
) -> Result<Option<Value>> {
    let checked = bank::validate_files(base, None)?;
    let primary = field(plan, "primary")?;
    let targets = items(field(plan, "targets")?)?;
    let original = field(primary, "required_preserved_metadata")?;
    let candidate = field(field(evidence, "comparison_request")?, "candidate")?;
    let mut inventory = BTreeMap::new();
    let mut deleted = BTreeSet::new();
    for entry in items(field(&checked, "entries")?)? {
        match text(entry, "format")? {
            "tombstone" => {
                deleted.insert(text(entry, "id")?.to_owned());
            }
            "standalone-record" => {
                let (_, m, _) = records::parse(text(base, text(entry, "path")?)?, &[])?;
                inventory.insert(version_key(&m)?, m);
            }
            _ => {}
        }
    }
    let mut ordered = vec![primary];
    ordered.extend(
        targets
            .iter()
            .filter(|v| get(v, "path") != get(primary, "path")),
    );
    let mut existing: Vec<Value> = ordered
        .iter()
        .map(|t| field(t, "required_preserved_metadata").cloned())
        .collect::<Result<_>>()?;
    let mut pending = existing.iter().map(reference).collect::<Result<Vec<_>>>()?;
    let mut visited = BTreeSet::new();
    while let Some(reference) = pending.pop() {
        let key = version_key(&reference)?;
        if !visited.insert(key.clone()) {
            continue;
        }
        if deleted.contains(&key.0) {
            return Err(err(
                "Deleted provenance cannot be restored by consolidation",
            ));
        }
        if visited.len() > MAX_PROVENANCE {
            return Err(err(
                "Provenance exceeds bounded maintenance; inspect smaller independent groups",
            ));
        }
        if let Some(metadata) = inventory.get(&key) {
            if matches!(
                get(metadata, "status").as_str(),
                Some("revoked" | "expired")
            ) {
                return Err(err(
                    "Inactive provenance requires explicit requalification, not automatic consolidation",
                ));
            }
            existing.push(metadata.clone());
            pending.extend(items(field(metadata, "derived-from")?)?.iter().cloned());
        }
    }
    let mut captured_metadata = captured
        .map(|c| {
            field(field(c, "request")?, "candidate")
                .and_then(|v| field(v, "metadata"))
                .cloned()
        })
        .transpose()?;
    if let (Some(metadata), Some(c)) = (&mut captured_metadata, captured) {
        let observed = field(field(c, "request")?, "repository_evidence")?;
        if observed != &Value::Null {
            let mut refs = items(field(metadata, "evidence")?)?.to_vec();
            refs.push(string(format!(
                "repo:{}#sha256={}",
                text(observed, "path")?,
                text(observed, "sha256")?
            )));
            set(metadata, "evidence", union(refs)?);
        }
    }
    let new_capture = if let Some(metadata) = &captured_metadata {
        let fingerprint = fingerprint(metadata)?;
        let mut found = false;
        for source in &existing {
            if self::fingerprint(source)? == fingerprint {
                found = true;
                break;
            }
        }
        !found
    } else {
        false
    };
    let (_, _, old_body) = records::parse(text(base, text(primary, "path")?)?, &[])?;
    if targets.len() == 1 && old_body == text(candidate, "body")? && !new_capture {
        return Ok(None);
    }
    let mut new = original.clone();
    let max = inventory
        .keys()
        .filter(|(id, _)| Some(id.as_str()) == get(original, "id").as_str())
        .map(|(_, v)| v.as_str())
        .max_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
        .ok_or_else(|| err("Missing original version"))?;
    set(&mut new, "version", next_version(max)?);
    set(&mut new, "updated", field(evidence, "recorded_at")?.clone());
    remove(&mut new, "state-reason");
    remove(&mut new, "superseded-by");
    set(
        &mut new,
        "admission-reason",
        string(format!(
            "Meaning-preserving consolidation: {}",
            text(
                field(field(evidence, "comparison_proof")?, "judgment")?,
                "rationale"
            )?
        )),
    );
    let mut sources: Vec<Value> = ordered
        .iter()
        .map(|t| field(t, "required_preserved_metadata").cloned())
        .collect::<Result<_>>()?;
    let provenance_index = if new_capture {
        let mut provenance = captured_metadata
            .clone()
            .ok_or_else(|| err("Missing captured source"))?;
        set(
            &mut provenance,
            "id",
            string(child_id(operation_id, "captured-source")?),
        );
        set(&mut provenance, "version", Value::Int(1.into()));
        let mut refs = items(field(&provenance, "derived-from")?)?.to_vec();
        refs.push(reference(
            captured_metadata
                .as_ref()
                .ok_or_else(|| err("Missing captured source"))?,
        )?);
        set(&mut provenance, "derived-from", union(refs)?);
        let index = sources.len();
        sources.push(provenance);
        Some(index)
    } else {
        None
    };
    let mut refs = Vec::new();
    for source in &sources {
        refs.extend(items(field(source, "evidence")?)?.iter().cloned());
    }
    for source in &sources {
        refs.push(string(format!(
            "record:{}/v{}",
            text(source, "id")?,
            field(source, "version")?
                .as_int()
                .ok_or_else(|| err("Invalid source version"))?
        )));
    }
    set(&mut new, "evidence", union(refs)?);
    let mut derived = items(field(&new, "derived-from")?)?.to_vec();
    derived.extend(sources.iter().map(reference).collect::<Result<Vec<_>>>()?);
    for source in &sources {
        derived.extend(items(field(source, "derived-from")?)?.iter().cloned());
    }
    set(&mut new, "derived-from", union(derived)?);
    let mut supersedes = items(field(&new, "supersedes")?)?.to_vec();
    supersedes.extend(sources.iter().map(reference).collect::<Result<Vec<_>>>()?);
    for source in &sources {
        supersedes.extend(items(field(source, "supersedes")?)?.iter().cloned());
        supersedes.extend(items(field(source, "corrects")?)?.iter().cloned());
    }
    set(&mut new, "supersedes", union(supersedes)?);
    records::validate(&new, text(candidate, "body")?, &[])?;
    let authority = field(
        field(field(plan, "qualified")?, "scope_context")?,
        "developer_id",
    )?;
    let mut files = base.clone();
    let mut additions = Vec::new();
    let mut changes = BTreeMap::new();
    for (index, source) in sources.iter_mut().enumerate() {
        let event_id = child_id(operation_id, &format!("supersede-{index}"))?;
        let event = object([
            ("id", string(&event_id)),
            ("authority", authority.clone()),
            (
                "reason",
                string("Equivalent source retained by consolidated current learning"),
            ),
            ("evidence", field(source, "evidence")?.clone()),
            (
                "replacement",
                with(
                    reference(&new)?,
                    [
                        ("status", string("active")),
                        ("replaces", reference(source)?),
                        ("applicability-preserved", Value::Bool(true)),
                    ],
                ),
            ),
        ]);
        if Some(index) == provenance_index {
            let admission_event = object([
                ("id", string(child_id(operation_id, "capture-admission")?)),
                ("evidence", field(source, "evidence")?.clone()),
                ("authority", authority.clone()),
                ("reviewed", Value::Bool(true)),
            ]);
            if get(
                &lifecycle::transition(source, &string("admit"), &admission_event)?,
                "legal",
            ) != &Value::Bool(true)
            {
                return Err(err("Captured provenance cannot bypass admission"));
            }
            *source = with(
                source.clone(),
                [
                    ("status", string("active")),
                    ("trust", string("reviewed")),
                    ("visibility", string("visible")),
                    (
                        "admission-reason",
                        string("Qualified duplicate captured for provenance only"),
                    ),
                ],
            );
        }
        if get(
            &lifecycle::transition(source, &string("supersede"), &event)?,
            "legal",
        ) != &Value::Bool(true)
        {
            return Err(err("Source does not satisfy the common supersession model"));
        }
        let mut inactive = with(
            source.clone(),
            [
                ("status", string("superseded")),
                ("updated", field(evidence, "recorded_at")?.clone()),
                ("state-reason", field(&event, "reason")?.clone()),
                ("superseded-by", Value::Array(vec![reference(&new)?])),
            ],
        );
        if index == 0 {
            let path = format!(
                "history/versions/{}/v{}.md",
                text(original, "id")?,
                field(original, "version")?
                    .as_int()
                    .ok_or_else(|| err("Invalid original version"))?
            );
            if base.get(&path).is_some() {
                return Err(err("Do not replace existing version history"));
            }
            set(
                &mut inactive,
                "state-reason",
                string(format!(
                    "{}. Original bank-relative link base: {}",
                    text(&event, "reason")?,
                    text(primary, "path")?
                )),
            );
            set(&mut files, &path, string(render(&inactive)? + &old_body));
            additions.push(route(
                &path,
                "Previous exact learning version and origin",
                &inactive,
                false,
            )?);
        } else if Some(index) == provenance_index {
            let path = records::captured_source_path(
                field(
                    captured_metadata
                        .as_ref()
                        .ok_or_else(|| err("Missing captured source"))?,
                    "id",
                )?,
                field(source, "id")?,
            )?;
            if base.get(&path).is_some() {
                return Err(err("Do not replace existing captured-source provenance"));
            }
            let body = text(
                field(
                    field(captured.ok_or_else(|| err("Missing capture"))?, "request")?,
                    "candidate",
                )?,
                "body",
            )?;
            set(&mut files, &path, string(render(&inactive)? + body));
            additions.push(route(
                &path,
                "Captured source provenance",
                &inactive,
                false,
            )?);
        } else {
            let target = ordered
                .get(index)
                .ok_or_else(|| err("Missing source target"))?;
            let path = text(target, "path")?;
            let (_, _, body) = records::parse(text(base, path)?, &[])?;
            set(&mut files, path, string(render(&inactive)? + &body));
            changes.insert(path.to_owned(), string("superseded"));
        }
        let history = format!("history/events/{event_id}.md");
        let event_record = object([
            ("schema", string("agent-memory/history-event-v1")),
            ("id", string(event_id)),
            ("project-id", field(&new, "project-id")?.clone()),
            ("kind", string("memory-superseded")),
            ("record", reference(source)?),
            ("actor", authority.clone()),
            ("recorded-at", field(evidence, "recorded_at")?.clone()),
            ("evidence", field(source, "evidence")?.clone()),
        ]);
        if base.get(&history).is_some() {
            return Err(err("Do not overwrite a prior maintenance event"));
        }
        records::event(&event_record)?;
        set(&mut files, &history, string(render(&event_record)?));
        additions.push(route(
            &history,
            "Consolidation source event",
            &event_record,
            true,
        )?);
    }
    set(
        &mut files,
        text(primary, "path")?,
        string(render(&new)? + text(candidate, "body")?),
    );
    let event_path = format!("history/events/{operation_id}.md");
    let event = object([
        ("schema", string("agent-memory/history-event-v1")),
        ("id", string(operation_id)),
        ("project-id", field(&new, "project-id")?.clone()),
        ("kind", string("memory-admitted")),
        ("record", reference(&new)?),
        ("actor", authority.clone()),
        ("recorded-at", field(evidence, "recorded_at")?.clone()),
        ("evidence", field(&new, "evidence")?.clone()),
    ]);
    if base.get(&event_path).is_some() {
        return Err(err("Maintenance operation ID was already used"));
    }
    records::event(&event)?;
    set(&mut files, &event_path, string(render(&event)?));
    additions.push(route(
        &event_path,
        "Consolidated version event",
        &event,
        true,
    )?);
    let mut routed = routed(
        base,
        &checked,
        &changes,
        additions,
        field(evidence, "recorded_at")?,
    )?;
    for (path, body) in files.as_object().ok_or_else(|| err("Invalid bank files"))? {
        if base.get(path) != Some(body) {
            set(&mut routed, path, body.clone());
        }
    }
    bank::validate_files(&routed, None)?;
    let changed = Value::Object(
        routed
            .as_object()
            .ok_or_else(|| err("Invalid bank files"))?
            .iter()
            .filter(|(p, b)| base.get(p) != Some(b))
            .cloned()
            .collect(),
    );
    if changed.as_object().is_some_and(|rows| {
        rows.iter()
            .any(|(_, b)| b.as_str().is_none_or(|s| s.len() > MAX_CHANGED_FILE))
    }) {
        return Err(err(
            "Complete provenance exceeds a bounded record/router; inspect and reorganize without dropping evidence",
        ));
    }
    security::ensure_safe(&changed)?;
    Ok(Some(routed))
}
fn normalized_input(value: &Value) -> Result<Value> {
    input_value(value, false)?;
    Ok(with(
        value.clone(),
        [
            (
                "comparison_proof",
                get(value, "comparison_proof")
                    .get("proof")
                    .unwrap_or(get(value, "comparison_proof"))
                    .clone(),
            ),
            (
                "capture",
                get(value, "capture")
                    .get("capture")
                    .unwrap_or(get(value, "capture"))
                    .clone(),
            ),
        ],
    ))
}
fn evidence(repo: &Path, value: Value, recorded_at: Value) -> Result<Value> {
    let (root, _) = writes::bank_path(repo)?;
    let mut evidence = with(
        value,
        [
            ("recorded_at", recorded_at),
            ("git_context", retrieval::git_context(&root)?),
        ],
    );
    let fields = layout_resolution::evidence_fields(evidence.get("comparison_proof"))?;
    bank::merge(&mut evidence, &fields)?;
    Ok(evidence)
}
fn proposal(
    repo: &Path,
    evidence: &Value,
    operation_id: &str,
    checked: Inspection,
) -> Result<(Option<Value>, Value)> {
    let Some(plan) = checked.plan else {
        let disposition = text(&checked.compared, "disposition")?;
        return Ok((
            None,
            object([
                (
                    "disposition",
                    string(if disposition == "create-distinct" {
                        "proposal-only"
                    } else {
                        disposition
                    }),
                ),
                (
                    "detecting_developer_id",
                    field(&checked.compared, "detecting_developer_id")?.clone(),
                ),
                (
                    "other_developer_ids",
                    field(&checked.compared, "other_developer_ids")?.clone(),
                ),
                ("messages_sent", Value::Bool(false)),
                (
                    "recommend_other_developer_clarification",
                    field(&checked.compared, "recommend_other_developer_clarification")?.clone(),
                ),
            ]),
        ));
    };
    let (root, bank_path) = writes::bank_path(repo)?;
    let base = tx::files_at(&bank_path)?;
    let Some(desired) = build(
        &base,
        evidence,
        operation_id,
        &plan,
        checked.captured.as_ref(),
    )?
    else {
        return Ok((None, object([("disposition", string("no-change"))])));
    };
    let p = tx::preview(&root, &desired, "compact")?;
    let p = with(
        field(&p, "proposal")?.clone(),
        [
            ("operation_id", string(operation_id)),
            ("consolidation_evidence", evidence.clone()),
        ],
    );
    sharing_limits::private_bytes(&p)?;
    if !same(&tx::file_hashes(&base)?, field(&p, "observed_hashes")?)?
        || get(
            get(&checked.compared, "observation"),
            "bank_observation_sha256",
        )
        .as_str()
            != Some(
                digest(&encoded(&object([
                    ("project_id", field(&p, "project_id")?.clone()),
                    ("sources", field(&p, "observed_hashes")?.clone()),
                ]))?)
                .as_str(),
            )
    {
        return Err(err(
            "Maintenance sources changed while building read-only output",
        ));
    }
    writes::recovery_frontier_provider()?.require_scope(
        &root,
        field(&p, "project_id")?,
        Some(field(&p, "scope_context")?),
    )?;
    Ok((
        Some(p),
        object([
            ("disposition", string("consolidate-existing")),
            ("per_record_operator_approval_required", Value::Bool(false)),
            ("run_at_turn_boundary", Value::Bool(true)),
        ]),
    ))
}
pub fn plan(repo: &Path, value: &Value) -> Result<Value> {
    if !has_keys(value, PLAN_INPUT) {
        return Err(err(
            "Maintenance planning requires exact comparison request, judgment, capture and context",
        ));
    }
    let request = field(value, "comparison_request")?;
    let compared = comparison::judge(
        &comparison::inspect(repo, request, None)?,
        request,
        field(value, "comparison_judgment")?,
    )?;
    for decision in items(field(
        field(field(&compared, "proof")?, "judgment")?,
        "pending_decisions",
    )?)? {
        if get(get(decision, "coverage_claim"), "coverage").as_str() != Some("none") {
            return Err(err(
                "Maintenance planning cannot claim pending coverage before output review",
            ));
        }
    }
    let mut planning = value.clone();
    remove(&mut planning, "comparison_judgment");
    set(
        &mut planning,
        "comparison_proof",
        field(&compared, "proof")?.clone(),
    );
    let value = normalized_input(&planning)?;
    let operation_id = host::uuid4()?;
    let evidence = evidence(repo, value.clone(), string(host::utc_now_isoformat()?))?;
    let (recall, captured) = source_context(repo, &evidence, None)?;
    let checked = qualify(repo, &evidence, recall, captured, compared, None)?;
    let (p, details) = proposal(repo, &evidence, &operation_id, checked)?;
    let output = if let Some(p) = p {
        layout_resolution::make_output_plan(&p, &value)?
    } else {
        Value::Null
    };
    result(
        "plan",
        with(
            details,
            [
                ("output_plan", output),
                ("proposal", Value::Null),
                ("canonical_writes_performed", Value::Bool(false)),
                ("coverage_review_required", Value::Bool(true)),
                ("verified", Value::Bool(false)),
            ],
        ),
    )
}
pub fn preview(
    repo: &Path,
    value: &Value,
    output_plan: Option<&Value>,
    operation_id: Option<&str>,
    recorded_at: Option<&str>,
) -> Result<Value> {
    let value = normalized_input(value)?;
    let (id, at, output_plan) = if let Some(plan) = output_plan {
        if operation_id.is_some() || recorded_at.is_some() {
            return Err(err(
                "Output plan cannot be combined with explicit operation context",
            ));
        }
        let plan = plan.get("output_plan").unwrap_or(plan);
        let (id, at) = layout_resolution::output_plan_context(plan, &value)?;
        (id, at, Some(plan))
    } else {
        let id = operation_id
            .map(str::to_owned)
            .map(Ok)
            .unwrap_or_else(host::uuid4)?;
        if !uuid_valid(&string(&id)) {
            return Err(err(
                "Maintenance planning requires a canonical operation UUIDv4",
            ));
        }
        (
            string(id),
            string(
                recorded_at
                    .map(str::to_owned)
                    .map(Ok)
                    .unwrap_or_else(host::utc_now_isoformat)?,
            ),
            None,
        )
    };
    let evidence = evidence(repo, value.clone(), at)?;
    let (p, details) = proposal(
        repo,
        &evidence,
        id.as_str().ok_or_else(|| err("Invalid operation ID"))?,
        inspect(repo, &evidence, None)?,
    )?;
    let Some(p) = p else {
        if output_plan.is_some() {
            return Err(err(
                "Output plan no longer produces a qualified maintenance change",
            ));
        }
        return result(
            "preview",
            with(
                details,
                [
                    ("proposal", Value::Null),
                    ("canonical_writes_performed", Value::Bool(false)),
                ],
            ),
        );
    };
    if let Some(plan) = output_plan {
        layout_resolution::check_output_plan(&p, &value, plan)?;
    }
    validate_transaction(&p, None)?;
    result(
        "preview",
        with(
            details,
            [
                ("proposal", p),
                ("canonical_writes_performed", Value::Bool(false)),
            ],
        ),
    )
}
pub fn validate_transaction(p: &Value, owner: Option<&[u8]>) -> Result<()> {
    let evidence = field(p, "consolidation_evidence")?;
    input_value(evidence, true)?;
    if get(p, "operation").as_str() != Some("compact")
        || get(p, "project_id") != get(get(evidence, "comparison_request"), "project_id")
    {
        return Err(err(
            "Maintenance evidence belongs only to its own project operation",
        ));
    }
    let repo = Path::new(text(p, "repo_root")?);
    let checked = inspect(repo, evidence, owner)?;
    let plan = checked
        .plan
        .ok_or_else(|| err("Maintenance now requires clarification or a separate admission"))?;
    let (root, bank_path) = writes::bank_path(repo)?;
    let base = tx::files_at(&bank_path)?;
    let desired = build(
        &base,
        evidence,
        text(p, "operation_id")?,
        &plan,
        checked.captured.as_ref(),
    )?
    .unwrap_or(Value::Null);
    if !same(&tx::file_hashes(&base)?, field(p, "observed_hashes")?)?
        || !same(&desired, field(p, "files")?)?
    {
        return Err(err(
            "Maintenance proposal changed its exact canonical delta or source provenance",
        ));
    }
    writes::recovery_frontier_provider()?.require_scope_owned(
        &root,
        field(p, "project_id")?,
        Some(field(p, "scope_context")?),
        owner,
        None,
    )?;
    layout_resolution::claims_for(p, owner, None, &crate::cli::runtime_root()?)?;
    Ok(())
}
pub fn check_sources(p: &Value, owner: Option<&[u8]>, marker: Option<&[u8]>) -> Result<()> {
    let evidence = field(p, "consolidation_evidence")?;
    let repo = Path::new(text(p, "repo_root")?);
    let frontier = writes::recovery_frontier_provider()?;
    frontier.require_scope_owned(
        repo,
        field(p, "project_id")?,
        Some(field(p, "scope_context")?),
        owner,
        marker,
    )?;
    frontier.check(
        field(
            field(
                field(field(evidence, "comparison_proof")?, "judgment")?,
                "observation",
            )?,
            "recovery_frontier",
        )?,
        repo,
        owner,
        marker,
    )?;
    if !same(
        &retrieval::git_context(repo)?,
        field(evidence, "git_context")?,
    )? {
        return Err(err("Checkout context changed during maintenance"));
    }
    if field(evidence, "capture")? != &Value::Null {
        let fields = Value::Object(
            [
                "capture",
                "comparison_request",
                "comparison_proof",
                "recorded_at",
                "unit_resolution_claims",
                "comparison_proof_sha256",
                "reference_map_sha256",
            ]
            .into_iter()
            .map(|k| Ok((k.into(), field(evidence, k)?.clone())))
            .collect::<Result<_>>()?,
        );
        admission::check_sources(
            &with(p.clone(), [("admission_evidence", fields)]),
            owner,
            marker,
        )?;
    }
    let (context, _) = retrieval::observed_context(repo, field(evidence, "context")?)?;
    for (path, body) in field(p, "files")?
        .as_object()
        .ok_or_else(|| err("Invalid proposal files"))?
    {
        let body = body
            .as_str()
            .ok_or_else(|| err("Invalid proposal content"))?;
        if get(get(p, "observed_hashes"), path).as_str() == Some(digest(body.as_bytes()).as_str()) {
            continue;
        }
        let Ok((kind, metadata, _)) = records::parse(body, &[]) else {
            continue;
        };
        if kind != "standalone-record" || get(&metadata, "status").as_str() != Some("active") {
            continue;
        }
        let applicable = records::applicability(
            &metadata,
            &object([
                ("bank_project_id", field(p, "project_id")?.clone()),
                ("context", context.clone()),
            ]),
        )?;
        let current = lifecycle::current(
            &metadata,
            &object([
                ("authorized-scope", Value::Bool(true)),
                ("applicable", field(&applicable, "applicable")?.clone()),
                ("now", string(host::utc_now_isoformat()?)),
                ("minimum-trust", string("reviewed")),
                ("active-superseders", Value::Array(vec![])),
            ]),
        )?;
        if get(&current, "current") != &Value::Bool(true) {
            return Err(err(
                "Consolidated learning lost current qualification during the transaction",
            ));
        }
    }
    Ok(())
}
pub fn apply(prepared: &Value) -> Result<Value> {
    let p = tx::proposal_value(prepared)?;
    if p.get("consolidation_evidence").is_none() {
        return Err(err("Use a complete governed maintenance proposal"));
    }
    let receipt = tx::apply(p)?;
    let mut fields = receipt.clone();
    remove(&mut fields, "schema");
    remove(&mut fields, "operation");
    set(
        &mut fields,
        "consolidation_verified",
        Value::Bool(
            get(&receipt, "verified") == &Value::Bool(true)
                && get(&receipt, "outcome").as_str() == Some("finished"),
        ),
    );
    result("apply", fields)
}
pub struct Consolidation;
impl tx::TransactionValidator for Consolidation {
    fn validate_transaction(&self, p: &Value, owner: Option<&[u8]>) -> Result<()> {
        validate_transaction(p, owner)
    }
    fn check_sources(&self, p: &Value, owner: &[u8], marker: &[u8]) -> Result<()> {
        check_sources(p, Some(owner), Some(marker))
    }
}
pub fn command_result(operation: &str, outcome: Result<Value>) -> lifecycle::CommandResult {
    match outcome {
        Ok(value) => lifecycle::CommandResult::json(value, 0, json::INDENT2_UTF8),
        Err(error) => {
            let message = if matches!(error.kind(), Kind::BankCapacity { .. }) {
                error.to_string()
            } else {
                "Incomplete, unsafe, changed or unqualified maintenance; preserve evidence and inspect before retry".into()
            };
            let mut fields = object([
                ("verified", Value::Bool(false)),
                (
                    "canonical_writes_performed",
                    if matches!(operation, "plan" | "preview") {
                        Value::Bool(false)
                    } else {
                        Value::Null
                    },
                ),
                (
                    "recovery_may_be_required",
                    Value::Bool(operation == "apply"),
                ),
                ("error", string(message)),
            ]);
            let result = bank::merge(&mut fields, &crate::bank_capacity::details(&error))
                .and_then(|_| result(operation, fields));
            match result {
                Ok(value) => lifecycle::CommandResult::json(value, 1, json::DEFAULT),
                Err(error) => lifecycle::CommandResult {
                    value: None,
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("{error}\n"),
                },
            }
        }
    }
}
