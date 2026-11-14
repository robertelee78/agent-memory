//! Migration catalog v2: static inspection of declared codec names and
//! conversion chains, and the production `native` validator.
use agent_memory::bank_contract as bank;
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::error::Result;
use agent_memory::migration_codecs::{self, Compiled};
use agent_memory::migration_contract::{self as contract, Codecs, Converter, Validator};
use agent_memory::validation_memo::hex;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const LEGACY: &str = "agent-memory/test-only-legacy";
const MIDDLE: &str = "agent-memory/test-only-intermediate";
const NATIVE: &str = "agent-memory/v1";

static EXECUTED: AtomicBool = AtomicBool::new(false);
static NEXT: AtomicU64 = AtomicU64::new(0);

fn executed(_files: &Value) -> Result<Value> {
    EXECUTED.store(true, Ordering::SeqCst);
    Ok(Value::Null)
}

/// Test-only registry mirroring the baseline fixture module: validators
/// `native`, `legacy`, `middle`; converters `first`, `second` (unless withheld).
struct Fixture {
    withheld: Option<&'static str>,
}

impl Codecs for Fixture {
    fn validator(&self, name: &str) -> Option<Validator> {
        matches!(name, "native" | "legacy" | "middle").then_some(executed as Validator)
    }
    fn converter(&self, name: &str) -> Option<Converter> {
        (matches!(name, "first" | "second") && self.withheld != Some(name))
            .then_some(executed as Converter)
    }
}

struct Pack(PathBuf);

impl Pack {
    fn new() -> Self {
        let name = format!(
            "am-t1-migration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let root = std::env::temp_dir().join(name);
        std::fs::create_dir_all(root.join("schemas")).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn write(&self, raw: &[u8]) -> Value {
        std::fs::write(self.0.join(contract::CATALOG), raw).unwrap();
        files_for(raw)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Pack {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn s(text: &str) -> Value {
    Value::String(text.to_owned())
}

fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn sha256(raw: &[u8]) -> String {
    hex(&Sha256::digest(raw))
}

fn files_for(raw: &[u8]) -> Value {
    obj([(
        contract::CATALOG,
        obj([("sha256", s(&sha256(raw))), ("mode", s("0o644"))]),
    )])
}

fn code(name: &str) -> Value {
    obj([("codec", s(name))])
}

fn codec(schema: &str, validator: &str) -> Value {
    obj([("schema", s(schema)), ("validator", code(validator))])
}

fn step(id: &str, from: &str, to: &str, converter: &str) -> Value {
    obj([
        ("id", s(id)),
        ("from", s(from)),
        ("to", s(to)),
        ("converter", code(converter)),
    ])
}

fn chain_codecs() -> Vec<Value> {
    vec![
        codec(NATIVE, "native"),
        codec(LEGACY, "legacy"),
        codec(MIDDLE, "middle"),
    ]
}

fn chain_steps() -> Vec<Value> {
    vec![
        step("test-only-first", LEGACY, MIDDLE, "first"),
        step("test-only-second", MIDDLE, NATIVE, "second"),
    ]
}

fn catalog(codecs: Vec<Value>, steps: Vec<Value>) -> Value {
    obj([
        ("schema", s(contract::SCHEMA)),
        ("codecs", Value::Array(codecs)),
        ("steps", Value::Array(steps)),
    ])
}

fn compatibility(sources: &[&str]) -> Value {
    obj([
        ("readable_bank_schemas", Value::Array(vec![s(NATIVE)])),
        (
            "migrator_source_schemas",
            Value::Array(sources.iter().map(|v| s(v)).collect()),
        ),
        ("migration_catalog", s(contract::CATALOG)),
    ])
}

fn encoded(value: &Value) -> Vec<u8> {
    json::encode(value, json::DEFAULT).unwrap().into_bytes()
}

fn inspect(value: &Value, sources: &[&str], codecs: &dyn Codecs) -> Result<Value> {
    let pack = Pack::new();
    let files = pack.write(&encoded(value));
    contract::inspect(pack.path(), &compatibility(sources), &files, codecs)
}

fn refusal(value: &Value, sources: &[&str]) -> String {
    inspect(value, sources, &Fixture { withheld: None })
        .unwrap_err()
        .to_string()
}

// The baseline's `decorator` damage (a decorated Python function) has no Rust
// counterpart: codecs are compiled functions, so nothing parses candidate
// source. Its public `am artifact inspect` exit status waits for the CLI.
// obligation: tests/contracts/test_migration_catalog.py::MigrationCatalogContract.test_bundled_chain_is_statically_inspected_without_importing_candidate_code
#[test]
fn bundled_chain_is_inspected_without_running_a_codec() {
    let sources = [LEGACY, MIDDLE];
    let complete = catalog(chain_codecs(), chain_steps());
    let raw = encoded(&complete);
    let inspected = inspect(&complete, &sources, &Fixture { withheld: None }).unwrap();
    assert_eq!(inspected.get("schema"), Some(&s(contract::SCHEMA)));
    assert_eq!(inspected.get("sha256"), Some(&s(&sha256(&raw))));
    assert_eq!(
        inspected.get("codecs"),
        Some(&obj([
            (NATIVE, code("native")),
            (LEGACY, code("legacy")),
            (MIDDLE, code("middle")),
        ]))
    );
    assert_eq!(inspected.get("steps"), Some(&Value::Array(chain_steps())));

    let mut missing_step = chain_steps();
    missing_step.pop();
    assert_eq!(
        refusal(&catalog(chain_codecs(), missing_step), &sources),
        "Advertised migration source schemas differ from complete bundled chains"
    );

    let mut cycle = chain_steps();
    cycle.push(step("test-only-cycle", MIDDLE, LEGACY, "first"));
    assert_eq!(
        refusal(&catalog(chain_codecs(), cycle), &sources),
        "Migration catalog contains a cycle"
    );

    let mut ambiguous = chain_steps();
    ambiguous.push(step("test-only-shortcut", LEGACY, NATIVE, "first"));
    assert_eq!(
        refusal(&catalog(chain_codecs(), ambiguous), &sources),
        "Migration catalog contains an ambiguous conversion path"
    );

    let missing_code = inspect(
        &complete,
        &sources,
        &Fixture {
            withheld: Some("second"),
        },
    );
    assert_eq!(
        missing_code.unwrap_err().to_string(),
        "Migration code must name a codec compiled into this runtime"
    );

    assert_eq!(
        refusal(&complete, &[]),
        "Advertised migration source schemas differ from complete bundled chains"
    );
    assert!(!EXECUTED.load(Ordering::SeqCst));
}

// obligation: tests/contracts/test_migration_catalog.py::MigrationCatalogContract.test_complete_unreachable_diamond_graph_is_bounded_without_enumerating_every_path
#[test]
fn complete_unreachable_diamond_graph_is_bounded() {
    let mut codecs = chain_codecs();
    let mut steps = chain_steps();
    let mut previous = vec!["test-dead-root".to_owned()];
    let mut dead = previous.clone();
    for depth in 0..15 {
        let following = vec![format!("test-dead-{depth}a"), format!("test-dead-{depth}b")];
        dead.extend(following.iter().cloned());
        for source in &previous {
            for target in &following {
                let id = format!("edge-{}", steps.len());
                let from = format!("agent-memory/{source}");
                let to = format!("agent-memory/{target}");
                steps.push(step(&id, &from, &to, "first"));
            }
        }
        previous = following;
    }
    dead.sort();
    for name in &dead {
        codecs.push(codec(&format!("agent-memory/{name}"), "middle"));
    }
    let started = Instant::now();
    let inspected = inspect(
        &catalog(codecs, steps),
        &[LEGACY, MIDDLE],
        &Fixture { withheld: None },
    );
    assert!(inspected.is_ok(), "{inspected:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(!EXECUTED.load(Ordering::SeqCst));
}

#[test]
fn committed_catalog_names_only_the_compiled_native_validator() {
    let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
    let raw = std::fs::read(pack.join(contract::CATALOG)).unwrap();
    let declared = std::fs::read(pack.join("schemas/runtime-compatibility.json")).unwrap();
    let compatibility = json::decode_bytes(&declared, DecodePolicy::default()).unwrap();
    let inspected = contract::inspect(&pack, &compatibility, &files_for(&raw), &Compiled).unwrap();
    assert_eq!(
        inspected,
        obj([
            ("schema", s("agent-memory/migration-catalog-v2")),
            ("sha256", s(&sha256(&raw))),
            ("codecs", obj([(NATIVE, code("native"))])),
            ("steps", Value::Array(vec![])),
        ])
    );
    assert!(Compiled.validator("native").is_some());
    assert!(Compiled.validator("legacy").is_none());
    assert!(Compiled.converter("native").is_none());
}

#[test]
fn absent_catalog_is_allowed_only_without_advertised_migrators() {
    let pack = Pack::new();
    let none = obj([
        ("readable_bank_schemas", Value::Array(vec![s(NATIVE)])),
        ("migrator_source_schemas", Value::Array(vec![])),
    ]);
    let files = obj([]);
    assert_eq!(
        contract::inspect(pack.path(), &none, &files, &Compiled).unwrap(),
        Value::Null
    );
    let explicit_null = obj([
        ("migrator_source_schemas", Value::Array(vec![])),
        ("migration_catalog", Value::Null),
    ]);
    assert_eq!(
        contract::inspect(pack.path(), &explicit_null, &files, &Compiled).unwrap(),
        Value::Null
    );
    for advertised in [Value::Array(vec![s(LEGACY)]), Value::Null, s(LEGACY)] {
        let spec = obj([("migrator_source_schemas", advertised)]);
        assert_eq!(
            contract::inspect(pack.path(), &spec, &files, &Compiled)
                .unwrap_err()
                .to_string(),
            "Advertised migrators require a complete bundled catalog"
        );
    }
}

#[test]
fn catalog_must_be_the_bundled_unchanged_bounded_declaration() {
    let complete = encoded(&catalog(vec![codec(NATIVE, "native")], vec![]));
    let spec = compatibility(&[]);

    let pack = Pack::new();
    let files = pack.write(&complete);
    let mut elsewhere = spec.clone();
    *elsewhere.get_mut("migration_catalog").unwrap() = s("schemas/other.json");
    for (spec, files) in [(&elsewhere, &files), (&spec, &obj([]))] {
        assert_eq!(
            contract::inspect(pack.path(), spec, files, &Compiled)
                .unwrap_err()
                .to_string(),
            "Migration catalog must be a complete bundled declaration"
        );
    }

    let changed = "Migration catalog is oversized or changed";
    let mut edited = complete.clone();
    edited.push(b'\n');
    std::fs::write(pack.path().join(contract::CATALOG), &edited).unwrap();
    let after_edit = contract::inspect(pack.path(), &spec, &files, &Compiled);
    assert_eq!(after_edit.unwrap_err().to_string(), changed);

    let mut oversized = complete.clone();
    oversized.resize(65537, b' ');
    let files = pack.write(&oversized);
    let too_large = contract::inspect(pack.path(), &spec, &files, &Compiled);
    assert_eq!(too_large.unwrap_err().to_string(), changed);
    assert_eq!(
        std::fs::read(pack.path().join(contract::CATALOG)).unwrap(),
        oversized
    );

    let mut at_bound = complete.clone();
    at_bound.resize(65536, b' ');
    let files = pack.write(&at_bound);
    assert!(contract::inspect(pack.path(), &spec, &files, &Compiled).is_ok());

    std::fs::remove_file(pack.path().join(contract::CATALOG)).unwrap();
    let missing = contract::inspect(pack.path(), &spec, &files, &Compiled);
    assert_eq!(missing.unwrap_err().to_string(), changed);
}

#[test]
fn nonfinite_catalog_constants_use_the_baseline_refusal() {
    let pack = Pack::new();
    for token in ["NaN", "Infinity", "-Infinity"] {
        let raw = format!(
            "{{\"schema\":\"agent-memory/migration-catalog-v2\",\"codecs\":[],\"steps\":[],\"x\":{token}}}"
        );
        let files = pack.write(raw.as_bytes());
        assert_eq!(
            contract::inspect(pack.path(), &compatibility(&[]), &files, &Compiled)
                .unwrap_err()
                .to_string(),
            "Nonfinite migration catalog",
            "{token}"
        );
    }
}

#[test]
fn cycles_through_readable_schemas_are_refused() {
    let mut steps = chain_steps();
    steps.push(step("readable-to-legacy", NATIVE, LEGACY, "first"));
    assert_eq!(
        refusal(&catalog(chain_codecs(), steps), &[LEGACY, MIDDLE]),
        "Migration catalog contains a cycle"
    );
}

#[test]
fn malformed_catalogs_are_refused_without_panic() {
    let shape = "Unsupported migration catalog shape";
    let native = || vec![codec(NATIVE, "native")];

    // Python-era catalog: module path and function, never executed here.
    let python_era = obj([
        ("schema", s("agent-memory/migration-catalog-v1")),
        (
            "codecs",
            Value::Array(vec![obj([
                ("schema", s(NATIVE)),
                (
                    "validator",
                    obj([
                        ("module", s("scripts/agent_memory/migration_codecs.py")),
                        ("function", s("native")),
                    ]),
                ),
            ])]),
        ),
        ("steps", Value::Array(vec![])),
    ]);
    assert_eq!(refusal(&python_era, &[]), shape);

    let mut extra = catalog(native(), vec![]);
    if let Value::Object(pairs) = &mut extra {
        pairs.push(("note".to_owned(), s("unexpected")));
    }
    assert_eq!(refusal(&extra, &[]), shape);
    assert_eq!(refusal(&catalog(vec![], vec![]), &[]), shape);
    assert_eq!(refusal(&Value::Array(vec![]), &[]), shape);
    assert_eq!(
        refusal(
            &obj([
                ("schema", s(contract::SCHEMA)),
                ("codecs", s("native")),
                ("steps", Value::Array(vec![])),
            ]),
            &[]
        ),
        shape
    );
    let many: Vec<Value> = (0..65)
        .map(|n| codec(&format!("agent-memory/test-{n}"), "native"))
        .collect();
    assert_eq!(refusal(&catalog(many, vec![]), &[]), shape);
    let crowded: Vec<Value> = (0..65)
        .map(|n| step(&format!("edge-{n}"), LEGACY, MIDDLE, "first"))
        .collect();
    assert_eq!(refusal(&catalog(chain_codecs(), crowded), &[]), shape);

    let repeated = "Invalid or repeated migration codec";
    let mut twice = native();
    twice.push(codec(NATIVE, "native"));
    assert_eq!(refusal(&catalog(twice, vec![]), &[]), repeated);
    for schema in [
        "agent-memory/",
        "other/v1",
        "agent-memory/has space",
        "agent-memory/v1\n",
    ] {
        let named = vec![codec(schema, "native")];
        assert_eq!(
            refusal(&catalog(named, vec![]), &[]),
            repeated,
            "{schema:?}"
        );
    }
    let long = format!("agent-memory/{}", "a".repeat(129));
    assert_eq!(
        refusal(&catalog(vec![codec(&long, "native")], vec![]), &[]),
        repeated
    );
    let unkeyed = vec![obj([("schema", s(NATIVE))])];
    assert_eq!(refusal(&catalog(unkeyed, vec![]), &[]), repeated);
    let mistyped = vec![obj([
        ("schema", Value::Null),
        ("validator", code("native")),
    ])];
    assert_eq!(refusal(&catalog(mistyped, vec![]), &[]), repeated);

    let declaration = "Invalid migration code declaration";
    for validator in [
        s("native"),
        Value::Null,
        obj([("codec", s("native")), ("sha256", s("0"))]),
        obj([("module", s("x")), ("function", s("native"))]),
    ] {
        let declared = vec![obj([("schema", s(NATIVE)), ("validator", validator)])];
        assert_eq!(refusal(&catalog(declared, vec![]), &[]), declaration);
    }
    let uncompiled = "Migration code must name a codec compiled into this runtime";
    let long_name = "a".repeat(65);
    for name in [
        "Native",
        "9native",
        "",
        "first",
        "unknown",
        long_name.as_str(),
    ] {
        let declared = vec![codec(NATIVE, name)];
        assert_eq!(
            refusal(&catalog(declared, vec![]), &[]),
            uncompiled,
            "{name:?}"
        );
    }
    let numeric = vec![obj([
        ("schema", s(NATIVE)),
        ("validator", obj([("codec", Value::Bool(true))])),
    ])];
    assert_eq!(refusal(&catalog(numeric, vec![]), &[]), uncompiled);
    // A validator name is not a converter and the reverse.
    let swapped = vec![step("swap", LEGACY, MIDDLE, "legacy")];
    assert_eq!(refusal(&catalog(chain_codecs(), swapped), &[]), uncompiled);

    let invalid_step = "Invalid, incomplete or repeated migration step";
    let long_id = "a".repeat(129);
    for steps in [
        vec![step("same", LEGACY, LEGACY, "first")],
        vec![step(
            "unknown-from",
            "agent-memory/unknown",
            NATIVE,
            "first",
        )],
        vec![step("unknown-to", LEGACY, "agent-memory/unknown", "first")],
        vec![step("Upper", LEGACY, MIDDLE, "first")],
        vec![step("", LEGACY, MIDDLE, "first")],
        vec![step(&long_id, LEGACY, MIDDLE, "first")],
        vec![
            step("twice", LEGACY, MIDDLE, "first"),
            step("twice", MIDDLE, NATIVE, "second"),
        ],
        vec![obj([
            ("id", s("partial")),
            ("from", s(LEGACY)),
            ("to", s(MIDDLE)),
        ])],
        vec![obj([
            ("id", s("list-endpoint")),
            ("from", Value::Array(vec![s(LEGACY)])),
            ("to", s(MIDDLE)),
            ("converter", code("first")),
        ])],
        vec![s("step")],
    ] {
        assert_eq!(
            refusal(&catalog(chain_codecs(), steps), &[LEGACY, MIDDLE]),
            invalid_step
        );
    }

    let pack = Pack::new();
    let duplicate =
        br#"{"schema":"agent-memory/migration-catalog-v2","codecs":[],"codecs":[],"steps":[]}"#;
    let files = pack.write(duplicate);
    assert!(contract::inspect(pack.path(), &compatibility(&[]), &files, &Compiled).is_err());
    let truncated = br#"{"schema":"agent-memory/migration-catalog-v2","codecs":["#;
    let files = pack.write(truncated);
    assert!(contract::inspect(pack.path(), &compatibility(&[]), &files, &Compiled).is_err());
    assert!(!EXECUTED.load(Ordering::SeqCst));
}

#[test]
fn readable_schemas_need_bundled_validators() {
    let only_legacy = catalog(vec![codec(LEGACY, "legacy")], vec![]);
    let message = "Readable schemas require complete bundled validators";
    assert_eq!(refusal(&only_legacy, &[]), message);

    let pack = Pack::new();
    let files = pack.write(&encoded(&catalog(vec![codec(NATIVE, "native")], vec![])));
    for readable in [
        Value::Null,
        s(NATIVE),
        Value::Array(vec![Value::Bool(true)]),
    ] {
        let spec = obj([
            ("readable_bank_schemas", readable),
            ("migrator_source_schemas", Value::Array(vec![])),
            ("migration_catalog", s(contract::CATALOG)),
        ]);
        assert_eq!(
            contract::inspect(pack.path(), &spec, &files, &Compiled)
                .unwrap_err()
                .to_string(),
            message
        );
    }
}

#[test]
fn route_returns_the_single_chain_in_order() {
    let inspected = inspect(
        &catalog(chain_codecs(), chain_steps()),
        &[LEGACY, MIDDLE],
        &Fixture { withheld: None },
    )
    .unwrap();
    assert_eq!(
        contract::route(&inspected, LEGACY, NATIVE).unwrap(),
        Some(chain_steps())
    );
    let last = chain_steps().split_off(1);
    assert_eq!(
        contract::route(&inspected, MIDDLE, NATIVE).unwrap(),
        Some(last)
    );
    assert_eq!(
        contract::route(&inspected, NATIVE, NATIVE).unwrap(),
        Some(vec![])
    );
    assert_eq!(contract::route(&inspected, NATIVE, LEGACY).unwrap(), None);
    assert_eq!(
        contract::route(&inspected, "agent-memory/unknown", NATIVE).unwrap(),
        None
    );
    assert_eq!(
        contract::route(&inspected, "agent-memory/unknown", "agent-memory/unknown").unwrap(),
        Some(vec![])
    );
    assert!(contract::route(&Value::Null, LEGACY, NATIVE).is_err());
    let unshaped = obj([("steps", Value::Array(vec![obj([("from", s(LEGACY))])]))]);
    assert!(contract::route(&unshaped, LEGACY, NATIVE).is_err());
}

const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";

fn native_bank() -> Value {
    let context = obj([
        ("body", s("# Project\n\nUseful project context.\n")),
        ("purpose", s("Orient the agent")),
        ("scope", s("repository")),
        ("lifecycle", s("active")),
        ("read-when", s("When resuming work")),
    ]);
    let omitted: Vec<(String, Value)> = bank::CONTEXTS
        .iter()
        .filter(|path| **path != "projectbrief.md")
        .map(|path| ((*path).to_owned(), s("No relevant evidence supplied yet")))
        .collect();
    let proposal = obj([
        ("project-id", s(PROJECT)),
        ("router-updated", s("2026-10-22T17:00:00Z")),
        ("contexts", obj([("projectbrief.md", context)])),
        ("omitted", Value::Object(omitted)),
    ]);
    bank::render(&proposal)
        .unwrap()
        .get("files")
        .unwrap()
        .clone()
}

#[test]
fn native_validator_binds_project_and_every_canonical_byte() {
    let files = native_bank();
    let mut canonical = json::encode(&files, json::SORTED_INDENT2_UTF8_FINITE).unwrap();
    canonical.push('\n');
    assert_eq!(
        migration_codecs::native(&files).unwrap(),
        obj([
            ("project_id", s(PROJECT)),
            ("preservation_sha256", s(&sha256(canonical.as_bytes()))),
        ])
    );

    let mut edited = files.clone();
    let brief = edited.get_mut("projectbrief.md").unwrap();
    *brief = s(&format!("{}\nOne more line.\n", brief.as_str().unwrap()));
    let changed = migration_codecs::native(&edited).unwrap();
    assert_eq!(changed.get("project_id"), Some(&s(PROJECT)));
    assert_ne!(
        changed.get("preservation_sha256"),
        migration_codecs::native(&files)
            .unwrap()
            .get("preservation_sha256")
    );

    for malformed in [
        Value::Null,
        obj([]),
        obj([("MEMORY.md", Value::Bool(true))]),
    ] {
        assert!(migration_codecs::native(&malformed).is_err());
    }
}
