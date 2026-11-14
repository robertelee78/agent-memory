//! Explicit migration, recovery and restoration with production and
//! test-only codecs in disposable Worlds.
//!
//! Test-only format codecs cannot be compiled into the production `am`, so
//! every migration command runs in a child copy of this test binary
//! (`migration_worker`) whose runtime is an isolated fixture runtime root
//! (minimal portable pack + `bin/am`) and whose codecs are either the
//! production `migration_codecs::Compiled` or the test-only fixture codecs
//! below. Each child gets the World's environment, registers the production
//! providers, and exits with the command's own exit code (86 for an injected
//! `exit` fault).
#[allow(unused_imports)]
mod support;

use agent_memory::bank_contract;
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::developer_identity::{digest, encoded};
use agent_memory::error::{ContractError, Result as DomainResult};
use agent_memory::migration::{LocationAudit, Runtime};
use agent_memory::migration_codecs::{self, Compiled};
use agent_memory::migration_contract::{Codecs, Converter, Validator};
use agent_memory::process::Status;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use support::identity_fixtures::bind;
use support::{RunOptions, World};

const LEGACY: &str = "agent-memory/test-only-legacy";
const MIDDLE: &str = "agent-memory/test-only-intermediate";
const NATIVE: &str = "agent-memory/v1";
const MARKER: &str = ".agent-memory-transaction.md";
const STAND_IN: &[u8] = b"#!/bin/sh\nprintf 'stand-in am %s\\n' \"$*\"\n";
const TIMEOUT: Duration = Duration::from_secs(120);

// ----- test-only codecs (baseline tests/migration_fixtures.py CODE) -----

/// TEST ONLY: not a historical or proposed production bank format.
struct FixtureCodecs;

impl Codecs for FixtureCodecs {
    fn validator(&self, name: &str) -> Option<Validator> {
        match name {
            "native" => Some(migration_codecs::native),
            "legacy" => Some(legacy),
            "middle" => Some(middle),
            _ => None,
        }
    }
    fn converter(&self, name: &str) -> Option<Converter> {
        match name {
            "first" => Some(first),
            "second" => Some(second),
            _ => None,
        }
    }
}

fn replaced(files: &Value, path: &str, from: &str, to: &str) -> DomainResult<Value> {
    let Value::Object(pairs) = files else {
        return Err(ContractError::new("Fixture codec needs a file map"));
    };
    let mut pairs = pairs.clone();
    let body = pairs
        .iter_mut()
        .find(|(name, _)| name == path)
        .ok_or_else(|| ContractError::new("Fixture codec needs its file"))?;
    let text = body
        .1
        .as_str()
        .ok_or_else(|| ContractError::new("Fixture codec needs text"))?;
    body.1 = Value::String(text.replacen(from, to, 1));
    Ok(Value::Object(pairs))
}

fn normalized(files: &Value, schema: &str) -> DomainResult<Value> {
    let mut value = replaced(files, "MEMORY.md", schema, NATIVE)?;
    if schema == LEGACY {
        value = replaced(
            &value,
            "projectbrief.md",
            "# Fixture encoded project\n",
            "# Project\n",
        )?;
    } else if schema == MIDDLE {
        value = replaced(
            &value,
            "projectbrief.md",
            "# Fixture intermediate project\n",
            "# Project\n",
        )?;
    }
    let checked = bank_contract::validate_files(&value, None)?;
    let preservation = digest(Some(&encoded(&value)?)).unwrap_or_default();
    Ok(Value::Object(vec![
        (
            "project_id".into(),
            checked.get("project_id").cloned().unwrap_or(Value::Null),
        ),
        ("preservation_sha256".into(), Value::String(preservation)),
    ]))
}

fn legacy(files: &Value) -> DomainResult<Value> {
    normalized(files, LEGACY)
}

fn middle(files: &Value) -> DomainResult<Value> {
    normalized(files, MIDDLE)
}

fn first(files: &Value) -> DomainResult<Value> {
    let value = replaced(files, "MEMORY.md", LEGACY, MIDDLE)?;
    replaced(
        &value,
        "projectbrief.md",
        "# Fixture encoded project\n",
        "# Fixture intermediate project\n",
    )
}

fn second(files: &Value) -> DomainResult<Value> {
    let value = replaced(files, "MEMORY.md", MIDDLE, NATIVE)?;
    replaced(
        &value,
        "projectbrief.md",
        "# Fixture intermediate project\n",
        "# Project\n",
    )
}

// ----- child worker -----

#[test]
fn migration_worker() {
    let Ok(args) = std::env::var("AM_MIGRATION_ARGS") else {
        return;
    };
    let args = json::decode(&args, DecodePolicy::default()).unwrap();
    let args: Vec<String> = args
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| arg.as_str().unwrap().to_owned())
        .collect();
    let root = PathBuf::from(std::env::var_os("AM_MIGRATION_RUNTIME").unwrap());
    let out = PathBuf::from(std::env::var_os("AM_MIGRATION_OUT").unwrap());
    agent_memory::cli::register_providers().unwrap();
    // The runtime this child executes as: an explicit root, not the test binary.
    agent_memory::security::register_migration_audit(Some(Arc::new(LocationAudit::at(&root))));
    let result = if args.first().map(String::as_str) == Some("transaction-recover") {
        let repo = PathBuf::from(&args[1]);
        let outcome = agent_memory::bank_transactions::recover(&repo, None, None);
        agent_memory::bank_transactions::command_result("recover", outcome)
    } else {
        let compiled = std::env::var("AM_MIGRATION_CODECS").as_deref() == Ok("compiled");
        let codecs: Arc<dyn Codecs + Send + Sync> = if compiled {
            Arc::new(Compiled)
        } else {
            Arc::new(FixtureCodecs)
        };
        agent_memory::cli::helpers::migration::dispatch_with(&args, "am migrate", || {
            let surface = agent_memory::cli::surface::Declared::current()?;
            Ok(Runtime::new(
                &root,
                &root.join("bin/am"),
                codecs,
                Some(Arc::new(surface)),
            ))
        })
    };
    std::fs::write(&out, result.stdout.as_bytes()).unwrap();
    std::process::exit(result.exit_code);
}

struct Fixture {
    world: World,
    /// The trusted fixture runtime root (`skills/agent-memory`, `bin/am`).
    source: PathBuf,
    codecs: &'static str,
    calls: usize,
}

fn decode(raw: &[u8]) -> Value {
    json::decode_bytes(raw, DecodePolicy::default()).unwrap()
}

fn text(value: &Value) -> String {
    json::encode(value, json::DEFAULT).unwrap()
}

impl Fixture {
    /// Baseline `installed(world, codecs=...)`: a trusted fixture runtime with
    /// the test-only chain, or the production catalog and compiled codecs.
    fn new(codecs: bool) -> Self {
        let world = World::new().unwrap();
        let source = world.inside("trusted-fixture-source").unwrap();
        runtime(&source, codecs);
        Self {
            world,
            source,
            codecs: if codecs { "fixture" } else { "compiled" },
            calls: 0,
        }
    }

    /// Point the next worker child at `args` and `runtime`; returns its argv.
    fn worker(&mut self, args: &[&str], runtime: Option<&Path>) -> Vec<OsString> {
        self.calls += 1;
        let out = self.out();
        let encoded = text(&Value::Array(
            args.iter().map(|a| Value::String((*a).into())).collect(),
        ));
        let runtime = runtime.unwrap_or(&self.source).to_path_buf();
        for (key, value) in [
            ("AM_MIGRATION_ARGS", OsString::from(encoded)),
            ("AM_MIGRATION_RUNTIME", runtime.into_os_string()),
            ("AM_MIGRATION_OUT", out.into_os_string()),
            ("AM_MIGRATION_CODECS", OsString::from(self.codecs)),
        ] {
            self.world.env.insert(key.into(), value);
        }
        vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "migration_worker".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]
    }

    /// The output file of the latest worker child.
    fn out(&self) -> PathBuf {
        self.world
            .inside(format!("migration-out-{}.json", self.calls))
            .unwrap()
    }

    fn output(&self) -> Option<Value> {
        let raw = std::fs::read(self.out()).ok()?;
        (!raw.is_empty()).then(|| decode(&raw))
    }

    /// Baseline `call(world, command, *args, expected=…, env=…)`; an injected
    /// fault exits the child.
    fn call_with(
        &mut self,
        args: &[&str],
        expected: i32,
        fault: Option<&str>,
        runtime: Option<&Path>,
    ) -> Value {
        let env: Vec<(OsString, OsString)> = fault
            .map(|stage| {
                vec![
                    ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
                    ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
                ]
            })
            .unwrap_or_default();
        let argv = self.worker(args, runtime);
        let output = self
            .world
            .run_with(
                argv,
                RunOptions {
                    env: &env,
                    check: false,
                    timeout: TIMEOUT,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        let value = self.output();
        assert_eq!(
            output.status,
            Status::Code(expected),
            "{:?} {} {}",
            value.as_ref().map(text),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        value.unwrap_or(Value::Null)
    }

    fn call(&mut self, args: &[&str], expected: i32) -> Value {
        self.call_with(args, expected, None, None)
    }

    fn preview(&mut self, repo: &Path) -> Value {
        self.call(&["preview", "--repo", s(repo), "--to", NATIVE], 0)
    }

    /// Baseline `save(world, value, name)`: a private 0600 JSON file.
    fn save(&self, value: &Value, name: &str) -> String {
        let path = self.world.inside(name).unwrap();
        std::fs::write(&path, text(value)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path.to_str().unwrap().to_owned()
    }

    fn apply(&mut self, p: &Value, expected: i32) -> Value {
        let path = self.save(p, "migration-proposal.json");
        self.call(&["apply", "--proposal", &path], expected)
    }

    /// Baseline `unlock(world, command, repo)`.
    fn unlock(&mut self, repo: &Path) -> Value {
        let value = self.call(&["recover-lock", "--repo", s(repo)], 0);
        if field(&value, "proposal") != &Value::Null {
            let path = self.save(&value, "unlock.json");
            return self.call(&["recover-lock", "--repo", s(repo), "--proposal", &path], 0);
        }
        value
    }

    /// Baseline `recover(world, command, repo, operation, mode)`.
    fn recover(&mut self, repo: &Path, operation: &str, mode: &str) -> Value {
        let value = self.call(
            &[
                "recover",
                "--repo",
                s(repo),
                "--operation",
                operation,
                "--mode",
                mode,
            ],
            0,
        );
        let path = self.save(&value, "recovery.json");
        self.call(
            &[
                "recover",
                "--repo",
                s(repo),
                "--mode",
                mode,
                "--proposal",
                &path,
            ],
            0,
        )
    }

    /// Baseline `memory(world, legacy=...)`: a bound native bank, optionally
    /// re-encoded in the test-only legacy format, with `projectbrief.md` 0640.
    fn memory(&self, legacy: bool) -> (PathBuf, PathBuf) {
        let repo = self.world.repository("repo", false).unwrap();
        let bank = bank(&self.world, &repo);
        bind(&self.world, &repo).unwrap();
        if legacy {
            let path = bank.join("MEMORY.md");
            let body = read_text(&path).replacen(NATIVE, LEGACY, 1);
            std::fs::write(&path, body).unwrap();
            let path = bank.join("projectbrief.md");
            let body = read_text(&path).replacen("# Project\n", "# Fixture encoded project\n", 1);
            std::fs::write(&path, body).unwrap();
        }
        chmod(&bank.join("projectbrief.md"), 0o640);
        (repo, bank)
    }
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    field(value, key).as_str().unwrap()
}

fn truth(value: &Value, key: &str) -> bool {
    field(value, key) == &Value::Bool(true)
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn write(path: &Path, body: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut result = vec![root.to_path_buf()];
    if std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()) {
        let mut entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for entry in entries {
            result.extend(walk(&entry));
        }
    }
    result
}

/// Baseline `snapshot(root)`: every node below `root` with digest and mode.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for path in walk(root).into_iter().skip(1) {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let key = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mode = meta.permissions().mode() & 0o777;
        let value = if meta.file_type().is_symlink() {
            format!("link {}", std::fs::read_link(&path).unwrap().display())
        } else if meta.is_dir() {
            format!("dir {mode:o}")
        } else {
            let raw = std::fs::read(&path).unwrap();
            format!("{} {mode:o}", digest(Some(&raw)).unwrap())
        };
        result.insert(key, value);
    }
    result
}

const COMPATIBILITY_HEAD: &str = r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":"#;
const COMPATIBILITY_TAIL: &str = r#","migration_catalog":"schemas/migrations.json","python_minimum":"3.11","installed_commands":["am","agent-memory"],"installation_receipt_schemas":["agent-memory/install-receipt-v1","agent-memory/install-receipt-v2","agent-memory/install-receipt-v3"],"git_required_for":["git-sharing"],"required_python_packages":[],"required_agent_clients":[],"optional_adapters":[]}"#;
const PRODUCTION_CATALOG: &str = include_str!("../skills/agent-memory/schemas/migrations.json");

/// The test-only two-step chain added to the production catalog
/// (baseline `migration_fixtures.catalog`).
fn fixture_catalog() -> String {
    let mut value = decode(PRODUCTION_CATALOG.as_bytes());
    let Value::Object(fields) = &mut value else {
        panic!("catalog object");
    };
    let codec = |schema: &str, name: &str| {
        decode(format!(r#"{{"schema":"{schema}","validator":{{"codec":"{name}"}}}}"#).as_bytes())
    };
    let step = |id: &str, from: &str, to: &str, name: &str| {
        decode(
            format!(
                r#"{{"id":"{id}","from":"{from}","to":"{to}","converter":{{"codec":"{name}"}}}}"#
            )
            .as_bytes(),
        )
    };
    for (key, entry) in fields.iter_mut() {
        if key == "codecs" {
            let Value::Array(codecs) = entry else {
                panic!("codecs array");
            };
            codecs.push(codec(LEGACY, "legacy"));
            codecs.push(codec(MIDDLE, "middle"));
        } else if key == "steps" {
            *entry = Value::Array(vec![
                step("test-only-first", LEGACY, MIDDLE, "first"),
                step("test-only-second", MIDDLE, NATIVE, "second"),
            ]);
        }
    }
    text(&value)
}

/// A minimal valid portable pack plus a stand-in `bin/am` under `root`.
fn runtime(root: &Path, codecs: bool) {
    let pack = root.join("skills/agent-memory");
    write(
        &pack.join("SKILL.md"),
        "---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n",
    );
    write(
        &pack.join("bank/GUIDE.md"),
        "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n",
    );
    let mut operation =
        String::from("---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n");
    for section in agent_memory::pack_validation::SECTIONS {
        operation.push_str(&format!("## {section}\n\n"));
        if section == "Required context" {
            operation.push_str(
                "A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n",
            );
        } else {
            operation.push_str("Explicit operation context.\n\n");
        }
    }
    write(&pack.join("inspect/GUIDE.md"), operation);
    let sources = if codecs {
        format!(r#"["{LEGACY}","{MIDDLE}"]"#)
    } else {
        "[]".to_owned()
    };
    write(
        &pack.join("schemas/runtime-compatibility.json"),
        format!("{COMPATIBILITY_HEAD}{sources}{COMPATIBILITY_TAIL}"),
    );
    let catalog = if codecs {
        fixture_catalog()
    } else {
        PRODUCTION_CATALOG.to_owned()
    };
    write(&pack.join("schemas/migrations.json"), catalog);
    write(
        &pack.join("schemas/bank-router.json"),
        r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#,
    );
    write(
        &pack.join("templates/activation-markdown.example.txt"),
        agent_memory::installation::SHIM,
    );
    write(
        &pack.join("templates/activation-omp-markdown.example.txt"),
        include_str!("../skills/agent-memory/templates/activation-omp-markdown.example.txt"),
    );
    write(&pack.join("hooks/opencode.js"), "export default {}\n");
    write(
        &pack.join("hooks/omp.ts"),
        "export default function () {}\n",
    );
    write(&root.join("bin/am"), STAND_IN);
    for entry in walk(root) {
        if entry.is_dir() {
            chmod(&entry, 0o755);
        } else {
            chmod(&entry, 0o644);
        }
    }
    chmod(&root.join("bin/am"), 0o755);
}

/// Baseline `runtime_update_fixtures.bank(world, repo)`: render and validate
/// the fixture bank through the public `am bank` commands.
fn bank(world: &World, repo: &Path) -> PathBuf {
    let proposal = r##"{"project-id":"8e0b66aa-2be5-4fa3-b47f-1245474aa7dd","router-updated":"2026-10-22T17:00:00Z","contexts":{"projectbrief.md":{"body":"# Project\n\nPreserve durable shared learning.\n","purpose":"Orient the agent","scope":"repository","lifecycle":"active","read-when":"When resuming work"}},"omitted":{"productContext.md":"No applicable evidence yet","activeContext.md":"No applicable evidence yet","systemPatterns.md":"No applicable evidence yet","techContext.md":"No applicable evidence yet","progress.md":"No applicable evidence yet"}}"##;
    let path = world.inside("bank-input.json").unwrap();
    std::fs::write(&path, proposal).unwrap();
    let rendered = world.am(["bank", "render", "--input", s(&path)]).unwrap();
    let rendered = decode(&rendered.stdout);
    let directory = repo.join("memory-bank");
    std::fs::create_dir(&directory).unwrap();
    for (name, body) in field(&rendered, "files").as_object().unwrap() {
        std::fs::write(directory.join(name), body.as_str().unwrap()).unwrap();
    }
    directory
}

fn backups(root: &Path) -> Vec<PathBuf> {
    walk(root)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "backup.json"))
        .collect()
}

fn operation(value: &Value) -> String {
    string(field(value, "proposal"), "operation_id").to_owned()
}

// ----- contracts -----

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_production_native_noop_and_unsupported_refusal_do_not_create_backup_or_change_bank
#[test]
#[ignore = "needs recovery_frontier registered"]
fn production_native_noop_and_unsupported_refusal_do_not_create_backup_or_change_bank() {
    let mut f = Fixture::new(false);
    let (repo, bank) = f.memory(false);
    let before = snapshot(&bank);
    let value = f.preview(&repo);
    assert_eq!(string(&value, "state"), "already-current");
    assert!(!truth(&value, "backup_created"));
    assert!(!truth(&value, "canonical_writes_performed"));
    assert_eq!(snapshot(&bank), before);
    let catalog = decode(
        &std::fs::read(f.source.join("skills/agent-memory/schemas/migrations.json")).unwrap(),
    );
    assert_eq!(field(&catalog, "steps"), &Value::Array(vec![]));
    let compatibility = decode(
        &std::fs::read(
            f.source
                .join("skills/agent-memory/schemas/runtime-compatibility.json"),
        )
        .unwrap(),
    );
    assert_eq!(
        field(&compatibility, "migrator_source_schemas"),
        &Value::Array(vec![])
    );
    let path = bank.join("MEMORY.md");
    std::fs::write(&path, read_text(&path).replace(NATIVE, LEGACY)).unwrap();
    let before = snapshot(&bank);
    let denied = f.call(&["preview", "--repo", s(&repo), "--to", NATIVE], 1);
    assert!(!truth(&denied, "verified"));
    assert_eq!(snapshot(&bank), before);
    assert!(backups(&f.world.paths["state"]).is_empty());
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_complete_multistep_conversion_has_private_complete_backup_and_historical_replay_preserves_new_edits
#[test]
#[ignore = "needs recovery_frontier registered"]
fn complete_multistep_conversion_has_private_complete_backup_and_historical_replay_preserves_new_edits()
 {
    let mut f = Fixture::new(true);
    let (repo, bank) = f.memory(true);
    let before = snapshot(&bank);
    let package = snapshot(&f.source);
    let p = f.preview(&repo);
    assert_eq!(snapshot(&bank), before);
    assert_eq!(
        text(field(field(&p, "proposal"), "chain")),
        r#"["test-only-first", "test-only-second"]"#
    );
    assert!(!truth(&p, "backup_created"));
    let done = f.apply(&p, 0);
    assert!(truth(&done, "verified"));
    let receipt = field(&done, "receipt");
    assert!(truth(receipt, "outcome_directly_readable"));
    assert!(truth(receipt, "outcome_format_revalidated"));
    assert_eq!(mode_of(&bank.join("projectbrief.md")), 0o640);
    assert!(read_text(&bank.join("MEMORY.md")).contains(NATIVE));
    assert!(read_text(&bank.join("projectbrief.md")).contains("# Project"));
    let backup = PathBuf::from(string(&done, "backup_path"));
    assert_eq!(mode_of(&backup), 0o600);
    let saved = decode(&std::fs::read(&backup).unwrap());
    assert_eq!(
        string(field(&saved, "files"), "MEMORY.md")
            .matches(LEGACY)
            .count(),
        1
    );
    assert_eq!(snapshot(&f.source), package);
    let receipt_path = PathBuf::from(string(&done, "receipt_path"));
    let original_receipt = std::fs::read(&receipt_path).unwrap();
    let replay = f.apply(&p, 0);
    assert!(truth(&replay, "receipt_is_historical"));
    assert!(!truth(&replay, "canonical_writes_performed"));
    let path = bank.join("projectbrief.md");
    std::fs::write(&path, read_text(&path) + "\nNew later learning.\n").unwrap();
    let newer = snapshot(&bank);
    let replay = f.apply(&p, 0);
    assert!(truth(&replay, "receipt_is_historical"));
    assert!(!truth(&replay, "still_current"));
    assert_eq!(snapshot(&bank), newer);
    assert_eq!(std::fs::read(&receipt_path).unwrap(), original_receipt);
}

// The baseline runs the retained `scripts/migrate_bank.py` with the global
// wrapper removed. Here the retained package is the complete runtime root
// (`skills/agent-memory` + `bin/am`); the restoration runs as that retained
// runtime after the original runtime's binary is removed. The fixture codecs
// are test-only, so the retained stand-in binary itself is not executed.
// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_completed_restore_is_new_transaction_and_retained_helper_works_without_global_install
#[test]
#[ignore = "needs recovery_frontier registered"]
fn completed_restore_is_new_transaction_and_retained_runtime_works_without_global_install() {
    let mut f = Fixture::new(true);
    let (repo, bank) = f.memory(true);
    let before = snapshot(&bank);
    let p = f.preview(&repo);
    let done = f.apply(&p, 0);
    let receipt = PathBuf::from(string(&done, "receipt_path"));
    let original_receipt = std::fs::read(&receipt).unwrap();
    let original = string(field(&done, "receipt"), "operation_id").to_owned();
    let p = f.call(
        &["restore", "--repo", s(&repo), "--operation", &original],
        0,
    );
    assert_ne!(operation(&p), original);
    let retained = PathBuf::from(string(&done, "retained_helper"));
    assert!(retained.ends_with("trusted-package/bin/am"));
    let retained_package = retained.parent().unwrap().parent().unwrap().to_path_buf();
    let retained_before = snapshot(&retained_package);
    // Remove the original runtime binary; only the retained package remains.
    std::fs::remove_file(f.source.join("bin/am")).unwrap();
    let pfile = f.save(&p, "restore.json");
    let restored = f.call_with(
        &["restore", "--repo", s(&repo), "--proposal", &pfile],
        0,
        None,
        Some(&retained_package),
    );
    assert!(truth(&restored, "verified"));
    assert_eq!(snapshot(&bank), before);
    assert_eq!(std::fs::read(&receipt).unwrap(), original_receipt);
    assert_eq!(snapshot(&retained_package), retained_before);
    assert_ne!(string(&restored, "receipt_path"), s(&receipt));
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_stale_proposal_mode_edit_and_new_learning_after_completion_refuse_without_overwrite
#[test]
#[ignore = "needs recovery_frontier registered"]
fn stale_proposal_mode_edit_and_new_learning_after_completion_refuse_without_overwrite() {
    let mut f = Fixture::new(true);
    let (repo, bank) = f.memory(true);
    let p = f.preview(&repo);
    let path = bank.join("projectbrief.md");
    chmod(&path, 0o600);
    let before = snapshot(&bank);
    let denied = f.apply(&p, 1);
    assert!(!truth(&denied, "verified"));
    assert_eq!(snapshot(&bank), before);
    let p = f.preview(&repo);
    let done = f.apply(&p, 0);
    std::fs::write(&path, read_text(&path) + "\nPreserve newer correction.\n").unwrap();
    let before = snapshot(&bank);
    let original = string(field(&done, "receipt"), "operation_id").to_owned();
    f.call(
        &["restore", "--repo", s(&repo), "--operation", &original],
        1,
    );
    assert_eq!(snapshot(&bank), before);
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_interrupted_conversion_recovers_finish_or_original_rollback_and_lost_response_is_safe
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interrupted_conversion_recovers_finish_or_original_rollback_and_lost_response_is_safe() {
    let stages = [
        "migration-before-backup",
        "migration-after-backup",
        "migration-after-marker",
        "migration-after-write:projectbrief.md",
        "migration-before-receipt",
        "migration-after-receipt",
        "migration-after-marker-removal",
    ];
    for mode in ["finish", "rollback"] {
        for stage in stages {
            if mode == "rollback"
                && matches!(
                    stage,
                    "migration-after-receipt" | "migration-after-marker-removal"
                )
            {
                continue; // A finished conversion needs a NEW restoration transaction.
            }
            let mut f = Fixture::new(true);
            let (repo, bank) = f.memory(true);
            let before = snapshot(&bank);
            let p = f.preview(&repo);
            let id = operation(&p);
            let path = f.save(&p, "migration-proposal.json");
            f.call_with(&["apply", "--proposal", &path], 86, Some(stage), None);
            f.unlock(&repo);
            if bank.join(MARKER).exists() {
                let normal = f
                    .world
                    .am_with(
                        ["--json", "bank", "validate", "--repo", s(&repo)],
                        RunOptions {
                            check: false,
                            ..RunOptions::default()
                        },
                    )
                    .unwrap();
                assert_ne!(normal.status, Status::Code(0), "{mode} {stage}");
            }
            let done = f.recover(&repo, &id, mode);
            assert!(truth(&done, "verified"), "{mode} {stage}");
            assert!(!bank.join(MARKER).exists(), "{mode} {stage}");
            if mode == "rollback" {
                assert_eq!(snapshot(&bank), before, "{mode} {stage}");
            } else {
                assert!(
                    read_text(&bank.join("MEMORY.md")).contains(NATIVE),
                    "{mode} {stage}"
                );
            }
        }
    }
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_receipt_backup_pin_corruption_refuses_historical_verification_and_completed_restore
#[test]
#[ignore = "needs recovery_frontier registered"]
fn receipt_backup_pin_corruption_refuses_historical_verification_and_completed_restore() {
    for bad in [
        "null",
        r#"{"pretend": "digest"}"#,
        r#""not-a-digest""#,
        &format!("\"{}\"", "0".repeat(64)),
    ] {
        let mut f = Fixture::new(true);
        let (repo, bank) = f.memory(true);
        let p = f.preview(&repo);
        let done = f.apply(&p, 0);
        let receipt = PathBuf::from(string(&done, "receipt_path"));
        let mut value = decode(&std::fs::read(&receipt).unwrap());
        if let Value::Object(fields) = &mut value {
            for (key, entry) in fields.iter_mut() {
                if key == "backup_sha256" {
                    *entry = decode(bad.as_bytes());
                }
            }
        }
        std::fs::write(&receipt, text(&value)).unwrap();
        let before = snapshot(&bank);
        f.apply(&p, 1);
        let original = string(field(&done, "receipt"), "operation_id").to_owned();
        f.call(
            &["restore", "--repo", s(&repo), "--operation", &original],
            1,
        );
        assert_eq!(snapshot(&bank), before, "{bad}");
    }
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_interrupted_unknown_new_edits_and_corrupt_backup_never_get_overwritten
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interrupted_unknown_new_edits_and_corrupt_backup_never_get_overwritten() {
    for change in ["bytes", "mode", "extra", "backup"] {
        let mut f = Fixture::new(true);
        let (repo, bank) = f.memory(true);
        let p = f.preview(&repo);
        let path = f.save(&p, "migration-proposal.json");
        f.call_with(
            &["apply", "--proposal", &path],
            86,
            Some("migration-after-write:projectbrief.md"),
            None,
        );
        f.unlock(&repo);
        match change {
            "bytes" => {
                let path = bank.join("projectbrief.md");
                std::fs::write(
                    &path,
                    read_text(&path) + "\nPreserve a consequential correction.\n",
                )
                .unwrap();
            }
            "mode" => chmod(&bank.join("projectbrief.md"), 0o600),
            "extra" => std::fs::write(
                bank.join("new-learning.md"),
                "# Preserve newly captured learning\n",
            )
            .unwrap(),
            _ => {
                let backup = PathBuf::from(string(&p, "backup_path"));
                let mut value = decode(&std::fs::read(&backup).unwrap());
                if let Value::Object(fields) = &mut value {
                    for (key, entry) in fields.iter_mut() {
                        if key == "files"
                            && let Value::Object(files) = entry
                        {
                            for (name, body) in files.iter_mut() {
                                if name == "projectbrief.md" {
                                    let corrupt = body.as_str().unwrap().to_owned()
                                        + "\nCorrupt backup participant.\n";
                                    *body = Value::String(corrupt);
                                }
                            }
                        }
                    }
                }
                std::fs::write(&backup, text(&value)).unwrap();
            }
        }
        let before = snapshot(&bank);
        let id = operation(&p);
        f.call(
            &[
                "recover",
                "--repo",
                s(&repo),
                "--operation",
                &id,
                "--mode",
                "rollback",
            ],
            1,
        );
        assert_eq!(snapshot(&bank), before, "{change}");
    }
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_restoration_interruption_has_its_own_recovery_and_preserves_original_finished_receipt
#[test]
#[ignore = "needs recovery_frontier registered"]
fn restoration_interruption_has_its_own_recovery_and_preserves_original_finished_receipt() {
    for mode in ["finish", "rollback"] {
        let mut f = Fixture::new(true);
        let (repo, bank) = f.memory(true);
        let original = snapshot(&bank);
        let p = f.preview(&repo);
        let done = f.apply(&p, 0);
        let converted = snapshot(&bank);
        let receipt = PathBuf::from(string(&done, "receipt_path"));
        let receipt_bytes = std::fs::read(&receipt).unwrap();
        let id = string(field(&done, "receipt"), "operation_id").to_owned();
        let p = f.call(&["restore", "--repo", s(&repo), "--operation", &id], 0);
        let path = f.save(&p, "restore.json");
        f.call_with(
            &["restore", "--repo", s(&repo), "--proposal", &path],
            86,
            Some("migration-after-write:projectbrief.md"),
            None,
        );
        f.unlock(&repo);
        let restored = f.recover(&repo, &operation(&p), mode);
        assert!(truth(&restored, "verified"), "{mode}");
        let expected = if mode == "finish" {
            &original
        } else {
            &converted
        };
        assert_eq!(&snapshot(&bank), expected, "{mode}");
        assert_eq!(std::fs::read(&receipt).unwrap(), receipt_bytes, "{mode}");
    }
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_changed_state_root_inside_runtime_refuses_before_any_runtime_or_bank_write
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_state_root_inside_runtime_refuses_before_any_runtime_or_bank_write() {
    for operation_name in ["preview", "apply", "historical", "recover"] {
        let mut f = Fixture::new(true);
        let (repo, bank) = f.memory(true);
        let p = f.preview(&repo);
        if operation_name == "historical" {
            f.apply(&p, 0);
        }
        let before_bank = snapshot(&bank);
        let before_code = snapshot(&f.source);
        let unsafe_state = f.source.join("unsafe-state-inside-runtime");
        f.world.env.insert(
            "AGENT_MEMORY_STATE_HOME".into(),
            unsafe_state.into_os_string(),
        );
        match operation_name {
            "preview" => {
                f.call(&["preview", "--repo", s(&repo), "--to", NATIVE], 1);
            }
            "recover" => {
                let id = operation(&p);
                f.call(
                    &[
                        "recover",
                        "--repo",
                        s(&repo),
                        "--operation",
                        &id,
                        "--mode",
                        "finish",
                    ],
                    1,
                );
            }
            _ => {
                f.apply(&p, 1);
            }
        }
        assert_eq!(snapshot(&f.source), before_code, "{operation_name}");
        assert_eq!(snapshot(&bank), before_bank, "{operation_name}");
    }
}

// The baseline's generic recovery is the public `am transaction recover`;
// `bank_transactions` has no CLI route yet, so the same domain recovery runs
// in the worker child.
// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_migration_preserves_uncommitted_feature_code_index_head_and_rejects_generic_recovery
#[test]
#[ignore = "needs recovery_frontier registered"]
fn migration_preserves_uncommitted_feature_code_index_head_and_rejects_generic_recovery() {
    let mut f = Fixture::new(true);
    let (repo, bank) = f.memory(true);
    let code = repo.join("feature.py");
    std::fs::write(&code, "initial = True\n").unwrap();
    let git = |f: &Fixture, args: &[&str]| {
        f.world
            .git_with(
                args,
                RunOptions {
                    cwd: Some(&repo),
                    ..RunOptions::default()
                },
            )
            .unwrap()
    };
    git(&f, &["add", "feature.py"]);
    git(&f, &["commit", "-m", "Disposable feature base"]);
    std::fs::write(&code, "staged_change = True\n").unwrap();
    git(&f, &["add", "feature.py"]);
    std::fs::write(&code, "staged_change = True\nworking_change = True\n").unwrap();
    let before_code = std::fs::read(&code).unwrap();
    let before_index = std::fs::read(repo.join(".git/index")).unwrap();
    let before_head = git(&f, &["rev-parse", "HEAD"]).stdout;
    let p = f.preview(&repo);
    let path = f.save(&p, "migration-proposal.json");
    f.call_with(
        &["apply", "--proposal", &path],
        86,
        Some("migration-after-marker"),
        None,
    );
    f.unlock(&repo);
    let paused = snapshot(&bank);
    f.call(&["transaction-recover", s(&repo)], 1);
    assert_eq!(snapshot(&bank), paused);
    let done = f.recover(&repo, &operation(&p), "finish");
    assert!(truth(&done, "verified"));
    assert_eq!(std::fs::read(&code).unwrap(), before_code);
    assert_eq!(
        std::fs::read(repo.join(".git/index")).unwrap(),
        before_index
    );
    assert_eq!(git(&f, &["rev-parse", "HEAD"]).stdout, before_head);
}

// obligation: tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_live_lock_and_immediate_pre_replace_mode_change_are_detected_without_overwrite
#[test]
#[ignore = "needs recovery_frontier registered"]
fn live_lock_and_immediate_pre_replace_mode_change_are_detected_without_overwrite() {
    let mut f = Fixture::new(true);
    let (repo, bank) = f.memory(true);
    let p = f.preview(&repo);
    let rendezvous = f.world.inside("migration-rendezvous").unwrap();
    let stage = "transaction-before-recheck:projectbrief.md";
    for (key, value) in [
        ("AGENT_MEMORY_FAULT_STAGE", OsString::from(stage)),
        ("AGENT_MEMORY_FAULT_MODE", OsString::from("pause")),
        (
            "AGENT_MEMORY_FAULT_DIR",
            rendezvous.clone().into_os_string(),
        ),
    ] {
        f.world.env.insert(key.into(), value);
    }
    let path = f.save(&p, "migration-proposal.json");
    let argv = f.worker(&["apply", "--proposal", &path], None);
    let out = f.out();
    let child = f
        .world
        .spawn_with(
            argv,
            RunOptions {
                check: false,
                timeout: TIMEOUT,
                ..RunOptions::default()
            },
        )
        .unwrap();
    let ready = rendezvous.join(format!("{}.ready", stage.replace('/', "%2F")));
    let release = rendezvous.join(format!("{}.release", stage.replace('/', "%2F")));
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !ready.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        ready.exists(),
        "Writer did not reach the owned pre-replacement rendezvous"
    );
    f.call(&["preview", "--repo", s(&repo), "--to", NATIVE], 1);
    let brief = bank.join("projectbrief.md");
    let original_bytes = std::fs::read(&brief).unwrap();
    chmod(&brief, 0o600);
    std::fs::write(&release, b"").unwrap();
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(1));
    let value = decode(&std::fs::read(out).unwrap());
    assert_eq!(field(&value, "canonical_writes_performed"), &Value::Null);
    assert!(truth(&value, "canonical_state_requires_inspection"));
    assert_eq!(std::fs::read(&brief).unwrap(), original_bytes);
    assert_eq!(mode_of(&brief), 0o600);
    assert!(bank.join(MARKER).exists());
}

/// `am migrate` is a declared route of the binary; outside a runtime layout
/// it refuses with the migration inspection envelope, never a traceback.
#[test]
fn installed_migrate_route_refuses_outside_a_runtime_layout() {
    let world = World::new().unwrap();
    let output = world
        .am_with(
            ["--json", "migrate", "preview", "--to", NATIVE],
            RunOptions {
                check: false,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(1));
    let value = decode(&output.stdout);
    assert_eq!(string(&value, "schema"), "agent-memory/migration-result-v1");
    assert_eq!(string(&value, "operation"), "preview");
    assert_eq!(string(&value, "state"), "inspection-required");
    assert_eq!(
        field(&value, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    assert!(!truth(&value, "canonical_state_requires_inspection"));
    assert_eq!(
        string(&value, "error"),
        "This executable is not inside a runtime with a portable pack; run the installed or retained am"
    );
}
