//! Keep known disputes attached to current source variants.
use crate::client_hooks::membership_equal as equal;
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

/// Implemented by `sharing_holds` when that module is wired into the runtime.
pub trait HoldCauses: Send + Sync {
    fn cause_id(&self, cause: &Value) -> Result<String>;
}
/// Implemented by `semantic_sharing` when that module is wired into the runtime.
pub trait SemanticSharing: Send + Sync {
    fn safe_text(&self, value: &Value) -> Result<()>;
    fn disjoint(&self, left: &Value, right: &Value) -> Result<bool>;
    fn qualified_sources(
        &self,
        value: &Value,
        root: &Path,
        project_id: &str,
    ) -> Result<BTreeSet<String>>;
    /// Baseline semantic_sharing.check(repo, proof, owned_lock=).
    fn check(&self, repo: &Path, proof: &Value, owned_lock: Option<&[u8]>) -> Result<Value>;
    /// Baseline semantic_sharing.final_sources(root, observation, binding, owned_lock=).
    fn final_sources(
        &self,
        root: &Path,
        observation: &Value,
        binding: &Value,
        owned_lock: Option<&[u8]>,
    ) -> Result<()>;
    /// Baseline semantic_sharing.read_input(path, version2=).
    fn read_input(&self, path: &Path, version2: bool) -> Result<Value>;
    fn inspect(&self, repo: &Path, base: Option<&str>) -> Result<Value>;
    fn snapshots(
        &self,
        repo: &Path,
        base: Option<&str>,
        quarantine_records: bool,
    ) -> Result<(std::path::PathBuf, Value, Value, Value, Value)>;
}
/// Implemented by `security` when that module is wired into the runtime.
pub trait SecurityScan: Send + Sync {
    fn has_reasons(&self, body: &Value) -> Result<bool>;
}
static HOLDS: OnceLock<Box<dyn HoldCauses>> = OnceLock::new();
static SEMANTIC: OnceLock<Box<dyn SemanticSharing>> = OnceLock::new();
static SECURITY: OnceLock<Box<dyn SecurityScan>> = OnceLock::new();

pub fn register_hold_causes(provider: Box<dyn HoldCauses>) -> Result<()> {
    HOLDS
        .set(provider)
        .map_err(|_| ContractError::new("Hold causes already registered"))
}
pub fn register_semantic_sharing(provider: Box<dyn SemanticSharing>) -> Result<()> {
    SEMANTIC
        .set(provider)
        .map_err(|_| ContractError::new("Semantic sharing already registered"))
}
/// The registered semantic_sharing provider; absence is a typed MissingProvider.
pub(crate) fn semantic_sharing_provider() -> Result<&'static dyn SemanticSharing> {
    SEMANTIC.get().map(Box::as_ref).ok_or_else(|| {
        ContractError::missing_provider("semantic_sharing provider is not registered")
    })
}
pub fn register_security_scan(provider: Box<dyn SecurityScan>) -> Result<()> {
    SECURITY
        .set(provider)
        .map_err(|_| ContractError::new("Security scan already registered"))
}
fn semantic() -> Result<&'static dyn SemanticSharing> {
    SEMANTIC
        .get()
        .map(Box::as_ref)
        .ok_or_else(|| ContractError::new("Semantic sharing is not registered"))
}
fn security() -> Result<&'static dyn SecurityScan> {
    SECURITY
        .get()
        .map(Box::as_ref)
        .ok_or_else(|| ContractError::new("Security scan is not registered"))
}
fn malformed() -> ContractError {
    ContractError::new("Malformed known dispute source")
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value.get(name).ok_or_else(malformed)
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    field(value, name)?.as_str().ok_or_else(malformed)
}
fn array<'a>(value: &'a Value, name: &str) -> Result<&'a [Value]> {
    field(value, name)?.as_array().ok_or_else(malformed)
}
fn object(value: &Value) -> Result<&[(String, Value)]> {
    value.as_object().ok_or_else(malformed)
}
fn version(value: &Value) -> Result<String> {
    Ok(field(value, "version")?
        .as_int()
        .ok_or_else(malformed)?
        .as_str()
        .to_owned())
}
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !matches!(v, Value::Null))
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Int(value)) => value.as_str() != "0",
        Some(Value::Float(value)) => *value != 0.0,
        _ => true,
    }
}
fn strings(values: &BTreeSet<String>) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}
fn logical_key(value: &Value) -> Result<(String, String, Option<String>)> {
    let target = match field(value, "target_path")? {
        Value::Null => None,
        Value::String(target) => Some(target.clone()),
        _ => return Err(malformed()),
    };
    Ok((
        text(value, "path")?.to_owned(),
        text(value, "component")?.to_owned(),
        target,
    ))
}

pub type Rows = BTreeMap<String, Value>;
#[derive(Default, Debug)]
pub struct SourceIndex {
    logical: BTreeMap<(String, String, Option<String>), BTreeSet<String>>,
    identities: BTreeMap<String, BTreeSet<String>>,
    corrections: BTreeMap<(String, String), BTreeSet<String>>,
}

pub fn current_rows(rows: &Rows) -> Result<BTreeMap<&str, &Value>> {
    let mut current = BTreeMap::new();
    for (key, row) in rows {
        if array(row, "sides")?
            .iter()
            .any(|side| matches!(side.as_str(), Some("local" | "landed")))
        {
            current.insert(key.as_str(), row);
        }
    }
    Ok(current)
}
pub fn source_index(rows: &Rows) -> Result<SourceIndex> {
    let mut index = SourceIndex::default();
    for (key, row) in current_rows(rows)? {
        index
            .logical
            .entry(logical_key(row)?)
            .or_default()
            .insert(key.to_owned());
        let meta = row
            .get("metadata")
            .filter(|v| truthy(Some(v)))
            .or_else(|| row.get("witness_metadata").filter(|v| truthy(Some(v))));
        if let Some(meta) = meta {
            if let Some(id) = meta.get("id") {
                index
                    .identities
                    .entry(id.as_str().ok_or_else(malformed)?.to_owned())
                    .or_default()
                    .insert(key.to_owned());
            }
            for name in ["corrects", "supersedes"] {
                if let Some(refs) = meta.get(name) {
                    for reference in refs.as_array().ok_or_else(malformed)? {
                        index
                            .corrections
                            .entry((text(reference, "id")?.to_owned(), version(reference)?))
                            .or_default()
                            .insert(key.to_owned());
                    }
                }
            }
        }
    }
    Ok(index)
}
pub fn successors(reference: &Value, index: &SourceIndex) -> Result<Vec<String>> {
    let mut found = BTreeSet::new();
    if let Some(record) = present(reference.get("record")) {
        let id = text(record, "id")?;
        if let Some(keys) = index.identities.get(id) {
            found.extend(keys.iter().cloned());
        }
        if let Some(keys) = index.corrections.get(&(id.to_owned(), version(record)?)) {
            found.extend(keys.iter().cloned());
        }
    } else if let Some(keys) = index.logical.get(&logical_key(reference)?) {
        found.extend(keys.iter().cloned());
    }
    Ok(found.into_iter().collect())
}
pub fn describe(record: &Value, rows: &Rows, index: Option<&SourceIndex>) -> Result<Value> {
    let provider = HOLDS
        .get()
        .ok_or_else(|| ContractError::new("Hold causes are not registered"))?;
    let owned;
    let index = match index {
        Some(index) => index,
        None => {
            owned = source_index(rows)?;
            &owned
        }
    };
    let mut endpoints = Vec::new();
    let mut all = BTreeSet::new();
    let mut missing = BTreeSet::new();
    for (key, reference) in object(field(record, "references")?)? {
        let sources = successors(reference, index)?;
        if sources.is_empty() {
            missing.insert(key.clone());
        }
        all.extend(sources.iter().cloned());
        endpoints.push((
            key.clone(),
            Value::Array(sources.into_iter().map(Value::String).collect()),
        ));
    }
    let cause = field(record, "cause")?;
    Ok(Value::Object(vec![
        ("id".into(), Value::String(provider.cause_id(cause)?)),
        ("cause".into(), cause.clone()),
        ("references".into(), field(record, "references")?.clone()),
        ("endpoint_sources".into(), Value::Object(endpoints)),
        ("sources".into(), strings(&all)),
        ("missing_sources".into(), strings(&missing)),
        (
            "resolution".into(),
            record.get("resolution").cloned().unwrap_or(Value::Null),
        ),
        ("resolved".into(), Value::Bool(false)),
    ]))
}

pub fn validate_resolution(
    value: &Value,
    known: &Value,
    rows: &Rows,
    qualified: &BTreeSet<String>,
    stored: bool,
) -> Result<Value> {
    let semantic = semantic()?;
    let security = security()?;
    let invalid = || {
        ContractError::new("A known dispute needs an explicit bounded stronger-evidence resolution")
    };
    let mut required = vec!["basis", "evidence", "reason"];
    if stored {
        required.extend(["endpoint_sources", "judgment_sha256", "proof_sha256"]);
    }
    let pairs = value.as_object().ok_or_else(invalid)?;
    if pairs.len() != required.len() || required.iter().any(|name| value.get(name).is_none()) {
        return Err(invalid());
    }
    let basis = value
        .get("basis")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if !matches!(
        basis,
        "operator-clarification"
            | "qualified-correction"
            | "terminal-deletion"
            | "disjoint-conditions"
    ) {
        return Err(invalid());
    }
    let evidence = value
        .get("evidence")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if evidence.is_empty() || evidence.len() > 64 {
        return Err(invalid());
    }
    let mut evidence_keys = BTreeSet::new();
    for entry in evidence {
        if !evidence_keys.insert(entry.as_str().ok_or_else(invalid)?) {
            return Err(invalid());
        }
    }
    semantic.safe_text(field(value, "reason")?)?;
    if stored
        && !equal(
            field(value, "endpoint_sources")?,
            field(known, "endpoint_sources")?,
        )
    {
        return Err(ContractError::new(
            "Resolved claims changed; renew the source-bound clarification",
        ));
    }
    let actual = current_rows(rows)?;
    let evidence_rows: Vec<&Value> = evidence.iter().map(|key| {
        actual.get(key.as_str().ok_or_else(invalid)?).copied().ok_or_else(|| {
            ContractError::new("Resolution evidence must be present at its exact current semantic source version")
        })
    }).collect::<Result<_>>()?;
    for row in &evidence_rows {
        if truthy(row.get("content_withheld")) || truthy(row.get("terminal_body_withheld")) {
            return Err(ContractError::new(
                "Withheld assertions cannot silently resolve a known dispute",
            ));
        }
        if security.has_reasons(field(row, "body")?)? {
            return Err(ContractError::new(
                "Unsafe resolution evidence needs source clarification",
            ));
        }
    }
    let references: Vec<&Value> = object(field(known, "references")?)?
        .iter()
        .filter_map(|(_, reference)| present(reference.get("record")))
        .collect();
    match basis {
        "terminal-deletion" => {
            let mut found = false;
            for row in &evidence_rows {
                if text(row, "format")? == "tombstone" {
                    let witness = row.get("witness_metadata").and_then(|m| m.get("id"));
                    if witness.is_some() && references.iter().any(|r| r.get("id") == witness) {
                        found = true;
                    }
                }
            }
            if !found {
                return Err(ContractError::new(
                    "Terminal resolution needs the actual retained deletion witness",
                ));
            }
        }
        "disjoint-conditions" => {
            let groups = object(field(known, "endpoint_sources")?)?;
            let mut disjoint = !truthy(known.get("missing_sources")) && groups.len() == 2;
            if disjoint {
                let mut groups = groups.iter();
                let left = groups
                    .next()
                    .ok_or_else(malformed)?
                    .1
                    .as_array()
                    .ok_or_else(malformed)?;
                let right = groups
                    .next()
                    .ok_or_else(malformed)?
                    .1
                    .as_array()
                    .ok_or_else(malformed)?;
                for a in left {
                    for b in right {
                        let left = rows
                            .get(a.as_str().ok_or_else(malformed)?)
                            .ok_or_else(malformed)?;
                        let right = rows
                            .get(b.as_str().ok_or_else(malformed)?)
                            .ok_or_else(malformed)?;
                        if !semantic.disjoint(left, right)? {
                            disjoint = false;
                        }
                    }
                }
            }
            if !disjoint {
                return Err(ContractError::new(
                    "Scope resolution needs actual known non-overlap for every current claim variant",
                ));
            }
            let sources = array(known, "sources")?;
            if evidence.iter().any(|key| !sources.contains(key)) {
                return Err(ContractError::new(
                    "Scope resolution must name the actual changed conditional claims",
                ));
            }
        }
        _ => {
            for row in &evidence_rows {
                if !qualified.contains(text(row, "key")?) || present(row.get("metadata")).is_none()
                {
                    return Err(ContractError::new(
                        "Resolution evidence must be safe qualified current learning, not a newer unqualified label",
                    ));
                }
            }
            if basis == "operator-clarification" {
                if !evidence_rows.iter().any(|row| {
                    row.get("metadata")
                        .and_then(|m| m.get("kind"))
                        .and_then(Value::as_str)
                        == Some("user-feedback")
                }) {
                    return Err(ContractError::new(
                        "Operator clarification needs the inspected attributed feedback record",
                    ));
                }
            } else {
                let mut found = false;
                for row in &evidence_rows {
                    let meta = field(row, "metadata")?;
                    for name in ["corrects", "supersedes"] {
                        if let Some(refs) = meta.get(name) {
                            for item in refs.as_array().ok_or_else(malformed)? {
                                if references.iter().any(|reference| equal(reference, item)) {
                                    found = true;
                                }
                            }
                        }
                    }
                }
                if !found {
                    return Err(ContractError::new(
                        "Correction resolution must name an actual stronger exact-version correction",
                    ));
                }
            }
        }
    }
    Ok(Value::Object(vec![
        ("basis".into(), field(value, "basis")?.clone()),
        ("evidence".into(), field(value, "evidence")?.clone()),
        ("reason".into(), field(value, "reason")?.clone()),
        (
            "endpoint_sources".into(),
            field(known, "endpoint_sources")?.clone(),
        ),
    ]))
}
pub fn qualified_current(
    rows: &Rows,
    values: &Value,
    root: &Path,
    project_id: &str,
) -> Result<BTreeSet<String>> {
    let semantic = semantic()?;
    let mut paths = BTreeMap::new();
    for side in ["local", "landed"] {
        paths.insert(
            side,
            semantic.qualified_sources(field(values, side)?, root, project_id)?,
        );
    }
    let mut result = BTreeSet::new();
    for (key, row) in rows {
        let sides = array(row, "sides")?;
        let path = text(row, "path")?;
        if paths.iter().any(|(side, paths)| {
            sides.iter().any(|v| v.as_str() == Some(*side)) && paths.contains(path)
        }) {
            result.insert(key.clone());
        }
    }
    Ok(result)
}
pub fn cached(known: &Value, rows: &Rows, qualified: &BTreeSet<String>) -> Result<bool> {
    let Some(resolution) = present(known.get("resolution")) else {
        return Ok(false);
    };
    // Missing providers are integration failures, not an invalid cached resolution.
    semantic()?;
    security()?;
    Ok(validate_resolution(resolution, known, rows, qualified, true).is_ok())
}
