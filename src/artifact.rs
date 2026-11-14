//! Describe complete portable artifact bytes without executing candidate code.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string, text};
use crate::pack_validation::{self, CommandSurface, Invalid};
use std::path::Path;

pub const COMPATIBILITY: &str = "schemas/runtime-compatibility.json";
/// Inventory entry naming the installed runtime binary inside the artifact digest.
pub const BINARY_ENTRY: &str = "bin/am";
/// Explicit reader bound for the runtime binary (the 2 MiB pack asset bound does not apply).
pub const BINARY_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The only mode an installed runtime binary may carry.
const BINARY_MODE: u32 = 0o755;
const DECLARATION_MAX_BYTES: usize = 65536;
const LIMITS: [&str; 3] = [
    "Digest binds relative file names, bytes and file modes; it is not a signature.",
    "Schema/tool capabilities are explicit declarations, separately exercised by public tests.",
    "Installation ownership, channel and client activation require lifecycle readback.",
];

pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::new(message))
    }
}

pub fn declaration(root: &Path, relative: &Value, files: &Value) -> Result<Value> {
    let relative = relative
        .as_str()
        .filter(|v| {
            files.get(v).is_some() && !v.starts_with('/') && !v.split('/').any(|p| p == "..")
        })
        .ok_or_else(|| {
            ContractError::new("Declared artifact asset is missing or escapes the inventory")
        })?;
    let raw = crate::fs::read_source(root, Path::new(relative), DECLARATION_MAX_BYTES)
        .map_err(|_| {
            ContractError::new("Declaration is oversized or changed after package validation")
        })?
        .bytes;
    require(
        text(get(files, relative), "sha256") == Some(pack_validation::sha256(&raw).as_str()),
        "Declaration is oversized or changed after package validation",
    )?;
    json::decode_bytes(
        &raw,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            max_bytes: DECLARATION_MAX_BYTES,
            ..DecodePolicy::default()
        },
    )
}

pub fn strings(value: &Value, allow_empty: bool) -> bool {
    value.as_array().is_some_and(|v| {
        (allow_empty || !v.is_empty())
            && v.iter().all(|s| {
                s.as_str()
                    .is_some_and(|s| !s.is_empty() && s.chars().count() <= 256)
            })
            // `i` comes from `enumerate`, so `..i` is always within `v`.
            && v.iter().enumerate().all(|(i, s)| !v[..i].contains(s))
    })
}

/// Members of a list already accepted by `strings`.
fn members(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

pub fn command_names(compatibility: &Value) -> Result<Vec<String>> {
    let fallback = Value::Array(vec![string("agent-memory")]);
    let names = compatibility.get("installed_commands").unwrap_or(&fallback);
    let accepted = strings(names, false);
    let listed = members(names);
    require(
        accepted
            && listed.iter().all(|s| matches!(*s, "am" | "agent-memory"))
            && listed.contains(&"agent-memory"),
        "Unsupported installed command declaration",
    )?;
    Ok(listed.into_iter().map(String::from).collect())
}

/// Migration codecs' contract inspection, passed explicitly to the `*_with`
/// forms; candidate code is never imported.
pub trait MigrationContract: Send + Sync {
    fn inspect(&self, root: &Path, compatibility: &Value, files: &Value) -> Result<Value>;
}

fn check_compatibility(spec: &Value) -> Result<()> {
    require(
        spec.as_object().is_some()
            && text(spec, "schema") == Some("agent-memory/runtime-compatibility-v1"),
        "Unsupported runtime compatibility declaration",
    )?;
    require(
        strings(get(spec, "readable_bank_schemas"), false)
            && strings(get(spec, "migrator_source_schemas"), true)
            && strings(get(spec, "git_required_for"), false)
            && strings(get(spec, "optional_adapters"), true)
            && get(spec, "required_agent_clients") == &Value::Array(vec![]),
        "Compatibility must declare bounded schema/tool lists and a client-neutral core",
    )?;
    command_names(spec)?;
    let empty = Value::Array(vec![]);
    let capabilities = spec.get("private_state_capabilities").unwrap_or(&empty);
    require(
        strings(capabilities, true)
            && members(capabilities).iter().all(|v| {
                matches!(
                    *v,
                    "layout-frontier-v1" | "omp-harness-v1" | "omp-stored-source-v2"
                )
            }),
        "Private-state capabilities must be explicit unique supported tokens",
    )?;
    let default_receipts = Value::Array(vec![string("agent-memory/install-receipt-v1")]);
    let receipts = spec
        .get("installation_receipt_schemas")
        .unwrap_or(&default_receipts);
    require(
        strings(receipts, false)
            && members(receipts).iter().all(|v| {
                matches!(
                    *v,
                    "agent-memory/install-receipt-v1"
                        | "agent-memory/install-receipt-v2"
                        | "agent-memory/install-receipt-v3"
                )
            }),
        "Installation receipt readers must be explicit unique supported schemas",
    )
}

/// Baseline `router['properties']['schema']['const']`: a missing key is a refusal.
fn router_schema(router: &Value) -> Result<&Value> {
    router
        .get("properties")
        .and_then(|v| v.get("schema"))
        .and_then(|v| v.get("const"))
        .ok_or_else(|| ContractError::new("Bank router declaration lacks its schema constant"))
}

/// An optional adapter's entrypoint is one native command (`am <name>`)
/// declared by the signed command surface; the pack holds no executable.
fn adapter_entry(
    root: &Path,
    relative: &str,
    files: &Value,
    surface: Option<&dyn CommandSurface>,
) -> Result<Value> {
    require(
        relative.starts_with("adapters/"),
        "Optional adapters must be explicitly separated",
    )?;
    let adapter = declaration(root, &string(relative), files)?;
    require(
        adapter.as_object().is_some()
            && text(&adapter, "schema") == Some("agent-memory/optional-adapter-v2")
            && text(&adapter, "name").is_some_and(|v| !v.is_empty())
            && matches!(text(&adapter, "kind"), Some("service" | "client-discovery"))
            && strings(get(&adapter, "commands"), true)
            && get(&adapter, "requires_authentication").as_bool().is_some()
            && text(&adapter, "unavailable_behavior").is_some_and(|v| !v.is_empty()),
        "Invalid optional adapter declaration",
    )?;
    let surface = surface.ok_or_else(|| {
        ContractError::new(
            "Signed command surface provider is not registered; adapter entrypoint validation refuses",
        )
    })?;
    let name = text(&adapter, "entrypoint")
        .and_then(|v| v.strip_prefix("am "))
        .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace));
    let declared = match name {
        Some(name) => surface
            .command(name)
            .map_err(|error| ContractError::new(error.detail))?
            .is_some(),
        None => false,
    };
    require(
        declared,
        "Optional adapter entrypoint must be an am command declared by the command surface",
    )?;
    // Python `{'path': relative, **adapter}`: `path` keeps the first position.
    let mut entry = vec![("path".to_string(), string(relative))];
    for (key, value) in adapter.as_object().into_iter().flatten() {
        if let Some((_, existing)) = entry.iter_mut().find(|(k, _)| k == key) {
            *existing = value.clone();
        } else {
            entry.push((key.clone(), value.clone()));
        }
    }
    Ok(Value::Object(entry))
}

/// sha256 of `json.dumps(files, sort_keys=True, separators=(',', ':'))`.
fn files_digest(files: &Value) -> Result<String> {
    let encoded = json::encode(files, json::SORTED_COMPACT_ASCII)?;
    Ok(pack_validation::sha256(encoded.as_bytes()))
}

fn file_count(files: &Value) -> Result<usize> {
    files
        .as_object()
        .map(<[(String, Value)]>::len)
        .ok_or_else(|| ContractError::new("Artifact file inventory must be an object"))
}

fn inspect_validation(
    root: &Path,
    validation: &Value,
    surface: Option<&dyn CommandSurface>,
    migration: Option<&dyn MigrationContract>,
) -> Result<Value> {
    let files = get(validation, "files");
    let spec = declaration(root, &string(COMPATIBILITY), files)?;
    check_compatibility(&spec)?;
    let migration = migration.ok_or_else(|| {
        ContractError::new(
            "Migration contract provider is not registered; artifact inspection refuses",
        )
    })?;
    let migrations = migration.inspect(root, &spec, files)?;
    let router = declaration(root, &string("schemas/bank-router.json"), files)?;
    let schema = router_schema(&router)?;
    require(
        get(&spec, "readable_bank_schemas")
            .as_array()
            .is_some_and(|schemas| schemas.contains(schema)),
        "Native bank schema is absent from its readability declaration",
    )?;
    let mut adapters = Vec::new();
    for relative in members(get(&spec, "optional_adapters")) {
        adapters.push(adapter_entry(root, relative, files, surface)?);
    }
    let digest = files_digest(files)?;
    let count = file_count(files)?;
    Ok(object([
        ("schema", string("agent-memory/artifact-inspection-v1")),
        ("operation", string("inspect")),
        ("valid", Value::Bool(true)),
        ("version", get(validation, "version").clone()),
        ("artifact_digest", string(digest)),
        ("files", files.clone()),
        ("file_count", Value::Int(count.into())),
        ("compatibility", spec),
        ("optional_adapters", Value::Array(adapters)),
        ("migration_catalog", migrations),
        ("guides", get(validation, "guides").clone()),
        (
            "documented_commands",
            get(validation, "documented_commands").clone(),
        ),
        (
            "file_operations",
            get(validation, "file_operations").clone(),
        ),
        ("writes_performed", Value::Bool(false)),
        ("candidate_code_executed", Value::Bool(false)),
        ("source_origin_authenticated", Value::Bool(false)),
        ("installation_state_checked", Value::Bool(false)),
        ("runtime_behavior_certified", Value::Bool(false)),
        (
            "limits",
            Value::Array(LIMITS.into_iter().map(string).collect()),
        ),
    ]))
}

fn pack_error(error: ContractError) -> Invalid {
    Invalid::new("malformed-pack", ".", error.to_string())
}

fn runtime_error(detail: &str) -> Invalid {
    Invalid::new("runtime-binary", BINARY_ENTRY, detail)
}

pub fn inspect(pack: &Path) -> pack_validation::Result<Value> {
    pack_validation::with_validated(pack, |root, v, surface| {
        inspect_validation(
            root,
            v,
            Some(surface),
            Some(&crate::migration_contract::Bundled),
        )
        .map_err(pack_error)
    })
}

pub fn inspect_with(
    pack: &Path,
    surface: Option<&dyn CommandSurface>,
    migration: Option<&dyn MigrationContract>,
) -> pack_validation::Result<Value> {
    pack_validation::with_validated_surface(pack, surface, |root, v| {
        inspect_validation(root, v, surface, migration).map_err(pack_error)
    })
}

/// The one artifact digest: the pack manifest exactly as `inspect` produces it
/// plus the runtime binary as the `bin/am` entry, encoded and hashed like the
/// pack-only digest. Returns the manifest including the binary and its digest.
pub fn runtime_digest(
    files: &Value,
    binary_sha256: &str,
    binary_mode: u32,
) -> pack_validation::Result<(Value, String)> {
    let entries = files
        .as_object()
        .ok_or_else(|| runtime_error("Artifact file inventory must be an object"))?;
    if entries
        .iter()
        .any(|(path, _)| path == BINARY_ENTRY || path.starts_with("bin/"))
    {
        return Err(runtime_error(
            "Portable pack must not contain runtime binary entries",
        ));
    }
    if binary_mode != BINARY_MODE {
        return Err(runtime_error("Runtime binary needs mode 0o755"));
    }
    let hex = binary_sha256.len() == 64
        && binary_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !hex {
        return Err(runtime_error(
            "Runtime binary digest must be 64 lowercase hexadecimal characters",
        ));
    }
    let mut combined = entries.to_vec();
    let position = combined
        .iter()
        .position(|(path, _)| path.as_str() > BINARY_ENTRY)
        .unwrap_or(combined.len());
    combined.insert(
        position,
        (
            BINARY_ENTRY.to_string(),
            pack_validation::file_entry(binary_sha256, binary_mode),
        ),
    );
    let combined = Value::Object(combined);
    let digest = files_digest(&combined).map_err(pack_error)?;
    Ok((combined, digest))
}

fn with_binary(inspection: Value, binary: &Path) -> pack_validation::Result<Value> {
    let limit = usize::try_from(BINARY_MAX_BYTES)
        .map_err(|_| runtime_error("Runtime binary bound exceeds this platform"))?;
    let (raw, mode) = pack_validation::binary_asset(binary, BINARY_ENTRY, limit)?;
    let sha256 = pack_validation::sha256(&raw);
    let (files, digest) = runtime_digest(get(&inspection, "files"), &sha256, mode)?;
    let count = file_count(&files).map_err(pack_error)?;
    let Value::Object(fields) = inspection else {
        return Err(pack_error(ContractError::new(
            "Artifact inspection must be an object",
        )));
    };
    let fields = fields
        .into_iter()
        .map(|(key, value)| match key.as_str() {
            "artifact_digest" => (key, string(digest.clone())),
            "files" => (key, files.clone()),
            "file_count" => (key, Value::Int(count.into())),
            _ => (key, value),
        })
        .collect();
    Ok(Value::Object(fields))
}

/// `inspect` with the installed binary bound into `files`, `file_count` and
/// `artifact_digest`; the binary is read inside the pack's fresh-inventory bracket.
pub fn inspect_runtime(pack: &Path, binary: &Path) -> pack_validation::Result<Value> {
    pack_validation::with_validated(pack, |root, v, surface| {
        let inspection = inspect_validation(
            root,
            v,
            Some(surface),
            Some(&crate::migration_contract::Bundled),
        )
        .map_err(pack_error)?;
        with_binary(inspection, binary)
    })
}

pub fn inspect_runtime_with(
    pack: &Path,
    binary: &Path,
    surface: Option<&dyn CommandSurface>,
    migration: Option<&dyn MigrationContract>,
) -> pack_validation::Result<Value> {
    pack_validation::with_validated_surface(pack, surface, |root, v| {
        let inspection = inspect_validation(root, v, surface, migration).map_err(pack_error)?;
        with_binary(inspection, binary)
    })
}

pub fn command(pack: &Path) -> crate::lifecycle::CommandResult {
    let value = inspect(pack).unwrap_or_else(|e| {
        object([
            ("schema", string("agent-memory/artifact-inspection-v1")),
            ("operation", string("inspect")),
            ("valid", Value::Bool(false)),
            ("writes_performed", Value::Bool(false)),
            ("candidate_code_executed", Value::Bool(false)),
            ("error", string(e.to_string())),
            ("installation_state_checked", Value::Bool(false)),
        ])
    });
    let exit_code = if get(&value, "valid") == &Value::Bool(true) {
        0
    } else {
        1
    };
    crate::lifecycle::CommandResult::json(value, exit_code, json::INDENT2_ASCII)
}
