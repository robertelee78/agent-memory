//! Inspect the bundled migration catalog and the codec names it declares
//! without running a codec.
//!
//! `migration-catalog-v2` names validators and converters compiled into this
//! runtime. Their code identity is the runtime binary, which the artifact
//! digest already binds, so an entry carries a name and no file digest.
//! `migration-catalog-v1` named Python modules by path and sha256; it is
//! Python-era data and is refused here as an unsupported shape.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::{ContractError, Result};
use crate::lifecycle::{object, string};
use crate::pack_validation;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const CATALOG: &str = "schemas/migrations.json";
pub const SCHEMA: &str = "agent-memory/migration-catalog-v2";
const CATALOG_MAX_BYTES: usize = 65536;
const MAX_CODECS: usize = 64;
const MAX_STEPS: usize = 64;

/// Complete bank file map in; `{project_id, preservation_sha256}` out.
pub type Validator = fn(&Value) -> Result<Value>;
/// Complete bank file map in; complete converted file map out.
pub type Converter = fn(&Value) -> Result<Value>;

/// The codecs compiled into a runtime, addressed by catalog name.
pub trait Codecs {
    fn validator(&self, name: &str) -> Option<Validator>;
    fn converter(&self, name: &str) -> Option<Converter>;
}

#[derive(Clone, Copy)]
enum Role {
    Validator,
    Converter,
}

pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::new(message))
    }
}

/// Object whose key set is exactly `keys` (the decoder already refused repeats).
fn exact_keys(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == keys.len() && keys.iter().all(|key| value.get(key).is_some())
    })
}

/// `[a-z][a-z0-9_]{0,63}`
fn codec_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && name.len() <= 64
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `agent-memory/[A-Za-z0-9_.-]{1,128}`
fn schema_name(name: &str) -> bool {
    name.strip_prefix("agent-memory/").is_some_and(|rest| {
        (1..=128).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    })
}

/// `[a-z0-9][a-z0-9_.-]{0,127}`
fn step_id(name: &str) -> bool {
    let plain = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let mut bytes = name.bytes();
    bytes.next().is_some_and(plain)
        && name.len() <= 128
        && bytes.all(|b| plain(b) || matches!(b, b'_' | b'.' | b'-'))
}

/// One code declaration: `{"codec": NAME}` naming a codec of the given role
/// that is compiled into this runtime.
fn entry(value: &Value, codecs: &dyn Codecs, role: Role) -> Result<Value> {
    require(
        exact_keys(value, &["codec"]),
        "Invalid migration code declaration",
    )?;
    let compiled = value
        .get("codec")
        .and_then(Value::as_str)
        .filter(|name| codec_name(name))
        .is_some_and(|name| match role {
            Role::Validator => codecs.validator(name).is_some(),
            Role::Converter => codecs.converter(name).is_some(),
        });
    require(
        compiled,
        "Migration code must name a codec compiled into this runtime",
    )?;
    Ok(value.clone())
}

struct Edge<'a> {
    from: &'a str,
    to: &'a str,
    step: &'a Value,
}

fn edges(steps: &[Value]) -> Result<Vec<Edge<'_>>> {
    steps
        .iter()
        .map(|step| {
            let from = step.get("from").and_then(Value::as_str);
            let to = step.get("to").and_then(Value::as_str);
            match (from, to) {
                (Some(from), Some(to)) => Ok(Edge { from, to, step }),
                _ => Err(ContractError::new("Unsupported migration catalog shape")),
            }
        })
        .collect()
}

type Chain<'a> = Option<Vec<&'a Value>>;

fn visit<'a>(
    steps: &[Edge<'a>],
    schema: &'a str,
    target: &str,
    active: &mut Vec<&'a str>,
    memo: &mut BTreeMap<&'a str, Chain<'a>>,
) -> Result<Chain<'a>> {
    require(
        !active.contains(&schema),
        "Migration catalog contains a cycle",
    )?;
    if schema == target {
        return Ok(Some(Vec::new()));
    }
    if let Some(known) = memo.get(schema) {
        return Ok(known.clone());
    }
    let mut found: Chain<'a> = None;
    active.push(schema);
    for edge in steps.iter().filter(|edge| edge.from == schema) {
        if let Some(suffix) = visit(steps, edge.to, target, active, memo)? {
            require(
                found.is_none(),
                "Migration catalog contains an ambiguous conversion path",
            )?;
            let mut chain = vec![edge.step];
            chain.extend(suffix);
            found = Some(chain);
        }
    }
    active.pop();
    memo.insert(schema, found.clone());
    Ok(found)
}

fn route_edges<'a>(steps: &[Edge<'a>], source: &'a str, target: &str) -> Result<Chain<'a>> {
    visit(steps, source, target, &mut Vec::new(), &mut BTreeMap::new())
}

/// The single conversion chain from `source` to `target` in an inspected
/// catalog: `None` when no chain exists, an empty chain when they are equal,
/// a refusal when the catalog is cyclic or offers two chains.
pub fn route(catalog: &Value, source: &str, target: &str) -> Result<Option<Vec<Value>>> {
    let steps = catalog
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| ContractError::new("Unsupported migration catalog shape"))?;
    let steps = edges(steps)?;
    // Edges borrow the catalog; the source name must live as long, so look it
    // up among the catalog's own names and fall back to a direct comparison.
    let known = steps
        .iter()
        .flat_map(|edge| [edge.from, edge.to])
        .find(|name| *name == source);
    let chain = match known {
        Some(source) => route_edges(&steps, source, target)?,
        None if source == target => Some(Vec::new()),
        None => None,
    };
    Ok(chain.map(|steps| steps.into_iter().cloned().collect()))
}

/// Detect cycles even in a part of the graph no readable schema reaches.
fn acyclic<'a>(
    steps: &[Edge<'a>],
    schema: &'a str,
    active: &mut Vec<&'a str>,
    complete: &mut BTreeSet<&'a str>,
) -> Result<()> {
    require(
        !active.contains(&schema),
        "Migration catalog contains a cycle",
    )?;
    if complete.contains(schema) {
        return Ok(());
    }
    active.push(schema);
    for edge in steps.iter().filter(|edge| edge.from == schema) {
        acyclic(steps, edge.to, active, complete)?;
    }
    active.pop();
    complete.insert(schema);
    Ok(())
}

fn read_catalog(pack: &Path, files: &Value) -> Result<(Value, String)> {
    let changed = || ContractError::new("Migration catalog is oversized or changed");
    let raw = crate::fs::read_source(pack, Path::new(CATALOG), CATALOG_MAX_BYTES)
        .map_err(|_| changed())?
        .bytes;
    let measured = pack_validation::sha256(&raw);
    let recorded = files
        .get(CATALOG)
        .and_then(|entry| entry.get("sha256"))
        .and_then(Value::as_str);
    require(
        recorded == Some(measured.as_str()),
        "Migration catalog is oversized or changed",
    )?;
    let value = json::decode_bytes(
        &raw,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            max_bytes: CATALOG_MAX_BYTES,
            ..DecodePolicy::default()
        },
    )?;
    Ok((value, measured))
}

fn declared_codecs(listed: &[Value], codecs: &dyn Codecs) -> Result<Vec<(String, Value)>> {
    let mut declared: Vec<(String, Value)> = Vec::new();
    for codec in listed {
        let schema = codec
            .get("schema")
            .and_then(Value::as_str)
            .filter(|schema| schema_name(schema))
            .filter(|schema| declared.iter().all(|(known, _)| known != schema));
        let (true, Some(schema)) = (exact_keys(codec, &["schema", "validator"]), schema) else {
            return Err(ContractError::new("Invalid or repeated migration codec"));
        };
        let validator = codec.get("validator").unwrap_or(&Value::Null);
        declared.push((
            schema.to_owned(),
            entry(validator, codecs, Role::Validator)?,
        ));
    }
    Ok(declared)
}

/// A step endpoint: a string naming a declared codec schema.
fn endpoint<'a>(step: &'a Value, key: &str, declared: &[(String, Value)]) -> Option<&'a str> {
    step.get(key)
        .and_then(Value::as_str)
        .filter(|name| declared.iter().any(|(schema, _)| schema == name))
}

fn declared_steps(
    listed: &[Value],
    declared: &[(String, Value)],
    codecs: &dyn Codecs,
) -> Result<Vec<Value>> {
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    let mut steps = Vec::new();
    for step in listed {
        let id = step
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| step_id(id) && !ids.contains(id));
        let from = endpoint(step, "from", declared);
        let to = endpoint(step, "to", declared);
        let shaped = exact_keys(step, &["id", "from", "to", "converter"]);
        let (true, Some(id), Some(from), Some(to)) = (shaped, id, from, to) else {
            return Err(ContractError::new(
                "Invalid, incomplete or repeated migration step",
            ));
        };
        require(from != to, "Invalid, incomplete or repeated migration step")?;
        ids.insert(id);
        let converter = step.get("converter").unwrap_or(&Value::Null);
        entry(converter, codecs, Role::Converter)?;
        steps.push(step.clone());
    }
    Ok(steps)
}

/// Members of a compatibility list; `None` unless it is a list of strings.
fn names<'a>(compatibility: &'a Value, key: &str) -> Option<Vec<&'a str>> {
    compatibility
        .get(key)
        .and_then(Value::as_array)?
        .iter()
        .map(Value::as_str)
        .collect()
}

/// Validate the catalog a pack bundles against its compatibility declaration
/// and the codecs compiled into this runtime. Returns `null` when the pack
/// declares no catalog, otherwise `{schema, sha256, codecs, steps}`.
pub fn inspect(
    pack: &Path,
    compatibility: &Value,
    files: &Value,
    codecs: &dyn Codecs,
) -> Result<Value> {
    let advertised = names(compatibility, "migrator_source_schemas");
    let name = compatibility
        .get("migration_catalog")
        .unwrap_or(&Value::Null);
    if matches!(name, Value::Null) {
        require(
            advertised.is_some_and(|schemas| schemas.is_empty()),
            "Advertised migrators require a complete bundled catalog",
        )?;
        return Ok(Value::Null);
    }
    require(
        name.as_str() == Some(CATALOG) && files.get(CATALOG).is_some(),
        "Migration catalog must be a complete bundled declaration",
    )?;
    let (value, sha256) = read_catalog(pack, files)?;
    let listed_codecs = value.get("codecs").and_then(Value::as_array);
    let listed_steps = value.get("steps").and_then(Value::as_array);
    let schema = value.get("schema").and_then(Value::as_str);
    let (true, Some(SCHEMA), Some(listed_codecs), Some(listed_steps)) = (
        exact_keys(&value, &["schema", "codecs", "steps"]),
        schema,
        listed_codecs,
        listed_steps,
    ) else {
        return Err(ContractError::new("Unsupported migration catalog shape"));
    };
    require(
        (1..=MAX_CODECS).contains(&listed_codecs.len()) && listed_steps.len() <= MAX_STEPS,
        "Unsupported migration catalog shape",
    )?;
    let declared = declared_codecs(listed_codecs, codecs)?;
    let steps = declared_steps(listed_steps, &declared, codecs)?;

    let readable = names(compatibility, "readable_bank_schemas");
    let has_validator = |schema: &str| declared.iter().any(|(known, _)| known == schema);
    let Some(readable) = readable.filter(|schemas| schemas.iter().all(|s| has_validator(s))) else {
        return Err(ContractError::new(
            "Readable schemas require complete bundled validators",
        ));
    };

    let graph = edges(&steps)?;
    let mut complete = BTreeSet::new();
    for (schema, _) in &declared {
        acyclic(&graph, schema, &mut Vec::new(), &mut complete)?;
    }
    let mut supported = BTreeSet::new();
    for (schema, _) in &declared {
        if readable.contains(&schema.as_str()) {
            continue;
        }
        for target in &readable {
            if route_edges(&graph, schema, target)?.is_some() {
                supported.insert(schema.as_str());
                break;
            }
        }
    }
    let advertised: Option<BTreeSet<&str>> =
        advertised.map(|schemas| schemas.into_iter().collect());
    require(
        advertised.as_ref() == Some(&supported),
        "Advertised migration source schemas differ from complete bundled chains",
    )?;
    // Check every requested pair, including currently readable sources.
    for (source, _) in &declared {
        for target in &readable {
            route_edges(&graph, source, target)?;
        }
    }
    Ok(object([
        ("schema", string(SCHEMA)),
        ("sha256", string(sha256)),
        ("codecs", Value::Object(declared.clone())),
        ("steps", Value::Array(steps.clone())),
    ]))
}

/// The production implementation of artifact inspection's migration seam.
pub struct Bundled;

impl crate::artifact::MigrationContract for Bundled {
    fn inspect(&self, root: &Path, compatibility: &Value, files: &Value) -> Result<Value> {
        inspect(
            root,
            compatibility,
            files,
            &crate::migration_codecs::Compiled,
        )
    }
}
