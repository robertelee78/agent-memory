use agent_memory::{
    bank_writes::{self, StateEnvironment},
    codec::json::{self, Value},
    developer_identity::encoded,
    error::{ContractError, Kind},
    host::{self, Sources},
    local_registry as storage,
    sharing_holds::{self as holds, Locations, ReadOptions, RecordOptions},
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
const OPERATION: &str = "a9abac6f-c33e-44c9-b40f-7b2ebafda860";
fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn obj<const N: usize>(values: [(&str, Value); N]) -> Value {
    Value::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn set(value: &mut Value, key: &str, new: Value) {
    let Value::Object(pairs) = value else {
        panic!("test object")
    };
    if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *old = new;
    } else {
        pairs.push((key.into(), new));
    }
}
fn reseal(mut value: Value) -> Value {
    if let Value::Object(pairs) = &mut value {
        pairs.retain(|(k, _)| k != "checksum");
    }
    holds::seal(&value).unwrap()
}
fn sha(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}
fn code(error: &ContractError) -> Option<&str> {
    if let Kind::HoldFailure { code, .. } = error.kind() {
        Some(code)
    } else {
        None
    }
}
fn capacity(error: ContractError, expected: &str, amount: usize, limit: usize) {
    assert_eq!(code(&error), Some(expected));
    assert!(error.message().contains(&amount.to_string()));
    assert!(error.message().contains(&limit.to_string()));
    let Kind::HoldFailure { repair, .. } = error.kind() else {
        panic!("capacity refusal")
    };
    assert!(repair.contains("cannot retain"));
    assert!(repair.contains("recovery and identical retries do not increase capacity"));
}
struct World(PathBuf);
impl World {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("am-holds-{}", host::uuid4().unwrap()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        std::fs::create_dir(root.join("runtime")).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn write(&self, path: &Path, bytes: impl AsRef<[u8]>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn paths(&self) -> Locations {
        let root = self.0.join("repo");
        let bank = root.join("memory-bank");
        std::fs::create_dir_all(&bank).unwrap();
        let base = self.0.join("state/semantic-holds");
        let paths = Locations {
            root,
            bank,
            directory: base.join(PROJECT),
            anchor: base.join(format!("{PROJECT}.present.json")),
        };
        holds::prepare_directory(&paths.directory).unwrap();
        paths
    }
    fn environment(&self) -> StateEnvironment {
        StateEnvironment {
            state_home: Some(self.0.join("private-state").into_os_string()),
            xdg_state_home: None,
            home: Some(self.0.clone().into_os_string()),
        }
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn cause(index: usize) -> Value {
    obj([
        ("sources", Value::Array(vec![s(format!("{index:064x}"))])),
        ("classification", s("contradiction")),
        (
            "reason",
            s("The yellow and blue claims remain in disagreement."),
        ),
        ("origin", s("a".repeat(64))),
    ])
}
fn reference() -> Value {
    obj([
        ("path", s("MEMORY.md")),
        ("component", Value::Null),
        ("target_path", s("MEMORY.md")),
        ("source_sha256_by_side", obj([])),
    ])
}
fn resolution(judgment: &str) -> Value {
    obj([
        ("basis", s("operator-clarification")),
        ("evidence", Value::Array(vec![s("b".repeat(64))])),
        (
            "reason",
            s("The detecting developer confirmed a blue finish."),
        ),
        ("endpoint_sources", obj([])),
        ("judgment_sha256", s(judgment)),
        ("proof_sha256", s("c".repeat(64))),
    ])
}
fn row(index: usize) -> Value {
    obj([
        ("binding", obj([])),
        ("cause", cause(index)),
        (
            "references",
            Value::Object(vec![(format!("{index:064x}"), reference())]),
        ),
        ("dependencies", obj([])),
        ("judgment_sha256", s("d".repeat(64))),
        ("proof_sha256", s("e".repeat(64))),
        ("resolution", Value::Null),
    ])
}
fn store(root: &Path, count: usize) -> Value {
    holds::seal(&obj([
        ("schema", s(holds::SCHEMA)),
        ("repo_root", s(root.to_str().unwrap())),
        ("project_id", s(PROJECT)),
        (
            "records",
            Value::Object(
                (0..count)
                    .map(|i| (holds::cause_id(&cause(i)).unwrap(), row(i)))
                    .collect(),
            ),
        ),
    ]))
    .unwrap()
}
fn first(value: &mut Value) -> &mut Value {
    let Value::Object(records) = value.get_mut("records").unwrap() else {
        panic!("records")
    };
    &mut records.first_mut().unwrap().1
}
fn sized(value: Value, size: usize) -> Value {
    let mut value = value;
    let amount = size - encoded(&value).unwrap().len();
    set(first(&mut value), "binding", s("x".repeat(amount)));
    // Replacing {} with a JSON string adds exactly its unescaped payload length.
    let value = reseal(value);
    assert_eq!(encoded(&value).unwrap().len(), size);
    value
}
fn seed(paths: &Locations, value: &Value) {
    holds::commit_store(paths, PROJECT, value, None).unwrap();
}
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, u32)> {
    fn visit(root: &Path, rows: &mut Vec<(PathBuf, Vec<u8>, u32)>) {
        for entry in std::fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(&entry.path(), rows);
            } else {
                rows.push((
                    entry.path(),
                    std::fs::read(entry.path()).unwrap(),
                    entry.metadata().unwrap().permissions().mode(),
                ));
            }
        }
    }
    let mut result = Vec::new();
    visit(root, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[test]
fn checksum_and_cause_bytes_match_python_contract() {
    assert_eq!(encoded(&holds::seal(&obj([])).unwrap()).unwrap(), b"{\n  \"checksum\": \"ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356\"\n}\n");
    let cause = obj([
        ("sources", Value::Array(vec![s("b"), s("a")])),
        ("classification", s("contradiction")),
    ]);
    let expected = sha(b"{\n  \"classification\": \"contradiction\",\n  \"sources\": [\n    \"a\",\n    \"b\"\n  ]\n}\n");
    assert_eq!(holds::cause_id(&cause).unwrap(), expected);
    let mut altered = cause.clone();
    set(
        &mut altered,
        "reason",
        s("different wording is not cause identity"),
    );
    assert_eq!(holds::cause_id(&altered).unwrap(), expected);
    assert!(holds::cause_id(&Value::Null).is_err());
}

// obligation: tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_record_boundary_excess_and_retry
#[test]
fn record_capacity_boundary_and_identical_retry() {
    let w = World::new();
    let paths = w.paths();
    let valid = store(&paths.root, 4096);
    holds::registry_value(&valid, &paths.root, PROJECT, true).unwrap();
    seed(&paths, &valid);
    let before = snapshot(&w.0);
    let excess = store(&paths.root, 4097);
    for _ in 0..2 {
        capacity(
            holds::registry_value(&excess, &paths.root, PROJECT, true).unwrap_err(),
            "known-holds-record-capacity",
            4097,
            4096,
        );
        assert_eq!(snapshot(&w.0), before);
    }
    assert_eq!(
        code(&holds::registry_value(&excess, &paths.root, PROJECT, false).unwrap_err()),
        None
    );
}

// obligation: tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_history_boundary_excess_and_retry
#[test]
fn history_capacity_boundary_and_identical_retry() {
    let w = World::new();
    let paths = w.paths();
    let mut value = store(&paths.root, 1);
    set(first(&mut value), "resolution", resolution(&"1".repeat(64)));
    set(
        first(&mut value),
        "resolution_history",
        Value::Array(vec![resolution(&"2".repeat(64)); 4096]),
    );
    let valid = reseal(value);
    holds::registry_value(&valid, &paths.root, PROJECT, true).unwrap();
    seed(&paths, &valid);
    let mut excess = valid.clone();
    set(
        first(&mut excess),
        "resolution_history",
        Value::Array(vec![resolution(&"2".repeat(64)); 4097]),
    );
    let excess = reseal(excess);
    let before = snapshot(&w.0);
    for _ in 0..2 {
        capacity(
            holds::registry_value(&excess, &paths.root, PROJECT, true).unwrap_err(),
            "known-holds-history-capacity",
            4097,
            4096,
        );
        assert_eq!(snapshot(&w.0), before);
    }
}

// obligation: tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_sealed_byte_boundary_excess_and_retry
#[test]
fn sealed_byte_capacity_boundary_and_identical_retry() {
    let w = World::new();
    let paths = w.paths();
    let valid = sized(store(&paths.root, 1), holds::MAX_STORE);
    holds::registry_value(&valid, &paths.root, PROJECT, true).unwrap();
    seed(&paths, &valid);
    let excess = sized(store(&paths.root, 1), holds::MAX_STORE + 1);
    let before = snapshot(&w.0);
    for _ in 0..2 {
        capacity(
            holds::registry_value(&excess, &paths.root, PROJECT, true).unwrap_err(),
            "known-holds-byte-capacity",
            holds::MAX_STORE + 1,
            holds::MAX_STORE,
        );
        assert_eq!(snapshot(&w.0), before);
    }
}

// obligation: tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_structure_precedes_capacity_and_combined_dimension_precedence
#[test]
fn structure_precedes_capacity_and_combined_dimension_precedence() {
    let w = World::new();
    let paths = w.paths();
    let mut value = store(&paths.root, 4097);
    let mut original = resolution(&"1".repeat(64));
    set(&mut original, "reason", s("x".repeat(4096)));
    set(first(&mut value), "resolution", original.clone());
    set(
        first(&mut value),
        "resolution_history",
        Value::Array(vec![original; 4097]),
    );
    value = reseal(value);
    assert!(encoded(&value).unwrap().len() > holds::MAX_STORE);
    let before = snapshot(&w.0);
    capacity(
        holds::registry_value(&value, &paths.root, PROJECT, true).unwrap_err(),
        "known-holds-record-capacity",
        4097,
        4096,
    );
    if let Value::Object(records) = value.get_mut("records").unwrap() {
        records.truncate(1);
    }
    value = reseal(value);
    assert!(encoded(&value).unwrap().len() > holds::MAX_STORE);
    capacity(
        holds::registry_value(&value, &paths.root, PROJECT, true).unwrap_err(),
        "known-holds-history-capacity",
        4097,
        4096,
    );
    for malformed in ["checksum", "last-history-entry"] {
        let mut invalid = value.clone();
        if malformed == "checksum" {
            set(&mut invalid, "checksum", s("0".repeat(64)));
        } else {
            let Value::Array(history) = first(&mut invalid).get_mut("resolution_history").unwrap()
            else {
                panic!("history")
            };
            set(history.last_mut().unwrap(), "unexpected", Value::Bool(true));
            invalid = reseal(invalid);
        }
        assert_eq!(
            code(&holds::registry_value(&invalid, &paths.root, PROJECT, true).unwrap_err()),
            None
        );
    }
    assert_eq!(
        code(&holds::registry_value(&value, &paths.root, PROJECT, false).unwrap_err()),
        None
    );
    assert_eq!(snapshot(&w.0), before);
}

// obligation: tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_persisted_corruption_and_interruption_remain_noncapacity
#[test]
fn persisted_corruption_and_interruption_remain_noncapacity() {
    let w = World::new();
    let paths = w.paths();
    let valid = store(&paths.root, 1);
    seed(&paths, &valid);
    for mode in ["checksum", "oversized-persisted", "interrupted"] {
        let mut value = if mode == "oversized-persisted" {
            store(&paths.root, 4097)
        } else {
            valid.clone()
        };
        if mode == "checksum" {
            set(&mut value, "checksum", s("0".repeat(64)));
        }
        for name in ["registry.json", "recovery.json"] {
            w.write(&paths.directory.join(name), encoded(&value).unwrap());
        }
        if mode == "interrupted" {
            w.write(
                &paths.directory.join("journal.json"),
                b"{\"retained-unknown-journal\":true}",
            );
        }
        let before = snapshot(&w.0);
        let error = holds::read_store_at(&paths, PROJECT, ReadOptions::default()).unwrap_err();
        assert!(!code(&error).is_some_and(|s| s.ends_with("-capacity")));
        if mode == "interrupted" {
            assert_eq!(code(&error), Some("known-holds-interrupted"));
        }
        assert_eq!(snapshot(&w.0), before);
    }
}

// obligation: tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_renewed_clarification_proof_replays_without_losing_prior_resolution
#[test]
fn renewed_resolution_filter_restores_exact_predecessors() {
    let root = Path::new("/private/repo");
    let mut value = store(root, 1);
    let first_resolution = resolution(&"1".repeat(64));
    let second = resolution(&"2".repeat(64));
    let third = resolution(&"3".repeat(64));
    set(first(&mut value), "resolution", third.clone());
    set(
        first(&mut value),
        "resolution_history",
        Value::Array(vec![first_resolution.clone(), second.clone()]),
    );
    let value = reseal(value);
    holds::registry_value(&value, root, PROJECT, false).unwrap();
    let filtered = holds::filtered_records(&value, Some(&"3".repeat(64))).unwrap();
    let record = filtered.first().unwrap();
    assert_eq!(record.get("resolution"), Some(&second));
    assert_eq!(
        record.get("resolution_history"),
        Some(&Value::Array(vec![first_resolution.clone()]))
    );
    assert_eq!(
        holds::filtered_records(&value, None)
            .unwrap()
            .first()
            .unwrap()
            .get("resolution"),
        Some(&third)
    );
    assert!(
        holds::filtered_records(&value, Some(&"d".repeat(64)))
            .unwrap()
            .is_empty()
    );
    let mut shorter = value.clone();
    set(
        first(&mut shorter),
        "resolution_history",
        Value::Array(vec![first_resolution.clone()]),
    );
    let filtered = holds::filtered_records(&shorter, Some(&"3".repeat(64))).unwrap();
    assert_eq!(
        filtered.first().unwrap().get("resolution"),
        Some(&first_resolution)
    );
    assert!(
        filtered
            .first()
            .unwrap()
            .get("resolution_history")
            .is_none()
    );
}

// obligation: tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_known_dispute_survives_rewording_changed_framing_and_registry_recovery
#[test]
fn registry_recovery_and_legacy_import_preserve_original_evidence() {
    let w = World::new();
    let paths = w.paths();
    let value = store(&paths.root, 1);
    seed(&paths, &value);
    let expected = std::fs::read(paths.directory.join("registry.json")).unwrap();
    std::fs::remove_file(paths.directory.join("registry.json")).unwrap();
    assert_eq!(
        code(&holds::read_store_at(&paths, PROJECT, ReadOptions::default()).unwrap_err()),
        Some("known-holds-registry-missing")
    );
    storage::exclusive(
        &paths.directory.join("lock"),
        OPERATION,
        "semantic-holds",
        Sources::default(),
        |owner| holds::recover_store(&paths, PROJECT, owner),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(paths.directory.join("registry.json")).unwrap(),
        expected
    );
    let record = row(0);
    let mut legacy = record.clone();
    if let Value::Object(pairs) = &mut legacy {
        pairs.retain(|(key, _)| key != "resolution");
    }
    set(
        &mut legacy,
        "schema",
        s("agent-memory/known-semantic-hold-v1"),
    );
    set(&mut legacy, "repo_root", s(paths.root.to_str().unwrap()));
    set(&mut legacy, "project_id", s(PROJECT));
    let legacy = reseal(legacy);
    let raw = encoded(&legacy).unwrap();
    let legacy_path = paths
        .directory
        .parent()
        .unwrap()
        .join(format!("{}.json", holds::cause_id(&cause(0)).unwrap()));
    w.write(&legacy_path, &raw);
    std::fs::remove_file(&paths.anchor).unwrap();
    std::fs::remove_dir_all(&paths.directory).unwrap();
    assert_eq!(
        code(&holds::read_store_at(&paths, PROJECT, ReadOptions::default()).unwrap_err()),
        Some("known-holds-legacy-state")
    );
    holds::prepare_directory(&paths.directory).unwrap();
    storage::exclusive(
        &paths.directory.join("lock"),
        OPERATION,
        "semantic-holds",
        Sources::default(),
        |owner| holds::recover_store(&paths, PROJECT, owner),
    )
    .unwrap();
    assert_eq!(std::fs::read(&legacy_path).unwrap(), raw);
    assert_eq!(
        encoded(
            &holds::read_store_at(&paths, PROJECT, ReadOptions::default())
                .unwrap()
                .0
        )
        .unwrap(),
        encoded(&value).unwrap()
    );
}

#[test]
fn journal_replays_only_observed_or_desired_state_and_preserves_later_evidence() {
    for current in ["observed", "desired", "later", "missing"] {
        let w = World::new();
        let paths = w.paths();
        let old = store(&paths.root, 1);
        seed(&paths, &old);
        let desired = store(&paths.root, 2);
        let desired_raw = encoded(&desired).unwrap();
        let old_raw = encoded(&old).unwrap();
        let journal = holds::seal(&obj([
            ("schema", s("agent-memory/known-holds-journal-v1")),
            ("repo_root", s(paths.root.to_str().unwrap())),
            ("project_id", s(PROJECT)),
            (
                "observed_sha256",
                if current == "missing" {
                    Value::Null
                } else {
                    s(sha(&old_raw))
                },
            ),
            ("desired", desired.clone()),
            ("desired_sha256", s(sha(&desired_raw))),
        ]))
        .unwrap();
        w.write(
            &paths.directory.join("journal.json"),
            encoded(&journal).unwrap(),
        );
        if current == "desired" {
            w.write(&paths.directory.join("registry.json"), &desired_raw);
        }
        if current == "later" {
            w.write(
                &paths.directory.join("registry.json"),
                encoded(&store(&paths.root, 3)).unwrap(),
            );
        }
        if current == "missing" {
            std::fs::remove_file(paths.directory.join("registry.json")).unwrap();
        }
        let before = snapshot(&w.0);
        let result = storage::exclusive(
            &paths.directory.join("lock"),
            OPERATION,
            "semantic-holds",
            Sources::default(),
            |owner| holds::recover_store(&paths, PROJECT, owner),
        );
        if current == "later" {
            assert_eq!(
                result.unwrap_err().message(),
                "Later hold evidence is preserved; do not replay over it"
            );
            assert_eq!(snapshot(&w.0), before);
        } else {
            result.unwrap();
            assert!(!paths.directory.join("journal.json").exists());
            for name in ["registry.json", "recovery.json"] {
                assert_eq!(
                    std::fs::read(paths.directory.join(name)).unwrap(),
                    desired_raw
                );
            }
        }
    }
}

#[test]
fn private_modes_lock_ownership_and_recovery_evidence_are_mandatory() {
    let w = World::new();
    let paths = w.paths();
    assert!(
        holds::read_store_at(&paths, PROJECT, ReadOptions::default())
            .unwrap()
            .1
            .is_none()
    );
    seed(&paths, &store(&paths.root, 1));
    assert_eq!(
        std::fs::metadata(&paths.directory)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for path in [
        paths.directory.join("registry.json"),
        paths.directory.join("recovery.json"),
        paths.anchor.clone(),
    ] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    storage::exclusive(
        &paths.directory.join("lock"),
        OPERATION,
        "semantic-holds",
        Sources::default(),
        |owner| {
            let raw =
                holds::trusted_read(&paths.directory.join("lock"), holds::MAX_MARKER)?.unwrap();
            assert_eq!(raw, owner);
            let marker = json::decode_bytes(&raw, json::DecodePolicy::default())?;
            assert_eq!(marker.get("operation"), Some(&s("semantic-holds")));
            assert_eq!(marker.get("operation_id"), Some(&s(OPERATION)));
            assert_eq!(
                code(&holds::read_store_at(&paths, PROJECT, ReadOptions::default()).unwrap_err()),
                Some("known-holds-busy")
            );
            holds::read_store_at(
                &paths,
                PROJECT,
                ReadOptions {
                    owner: Some(owner),
                    ..ReadOptions::default()
                },
            )?;
            assert!(
                storage::exclusive(
                    &paths.directory.join("lock"),
                    OPERATION,
                    "semantic-holds",
                    Sources::default(),
                    |_| Ok(())
                )
                .is_err()
            );
            Ok(())
        },
    )
    .unwrap();
    assert!(!paths.directory.join("lock").exists());
    w.write(&paths.directory.join("recovery.json"), b"changed");
    assert_eq!(
        code(&holds::read_store_at(&paths, PROJECT, ReadOptions::default()).unwrap_err()),
        Some("known-holds-recovery-incomplete")
    );
}

#[test]
fn symlinks_permissions_and_bounded_reads_refuse_without_mutation() {
    let w = World::new();
    let paths = w.paths();
    let target = w.0.join("target");
    w.write(&target, b"private");
    let link = paths.directory.join("registry.json");
    symlink(&target, &link).unwrap();
    assert!(holds::trusted_read(&link, holds::MAX_STORE).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"private");
    std::fs::remove_file(&link).unwrap();
    w.write(&link, b"12345");
    assert_eq!(
        holds::trusted_read(&link, 4).unwrap_err().message(),
        "Complete private input exceeds its explicit encoded budget; no truncation"
    );
    std::fs::set_permissions(&link, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(holds::trusted_read(&link, 8).is_err());
}

#[test]
fn structural_refusals_do_not_infer_resolution_or_repair_evidence() {
    let root = Path::new("/private/repo");
    for corruption in [
        "cause-fields",
        "duplicate-sources",
        "source-digest",
        "reason",
        "origin",
        "references",
        "reference-path",
        "record-reference",
        "history-empty",
        "judgment",
        "record-extra",
    ] {
        let mut value = store(root, 1);
        let record = first(&mut value);
        match corruption {
            "cause-fields" => set(record.get_mut("cause").unwrap(), "extra", Value::Null),
            "duplicate-sources" => set(
                record.get_mut("cause").unwrap(),
                "sources",
                Value::Array(vec![s("0".repeat(64)); 2]),
            ),
            "source-digest" => set(
                record.get_mut("cause").unwrap(),
                "sources",
                Value::Array(vec![s("invalid")]),
            ),
            "reason" => set(record.get_mut("cause").unwrap(), "reason", s(" ")),
            "origin" => set(record.get_mut("cause").unwrap(), "origin", s("bad")),
            "references" => set(record, "references", obj([])),
            "reference-path" | "record-reference" => {
                let Value::Object(references) = record.get_mut("references").unwrap() else {
                    panic!("references")
                };
                let reference = &mut references.first_mut().unwrap().1;
                if corruption == "reference-path" {
                    set(reference, "path", s("../outside.md"));
                } else {
                    set(
                        reference,
                        "record",
                        obj([("id", s("bad")), ("version", s("bad"))]),
                    );
                }
            }
            "history-empty" => set(record, "resolution_history", Value::Array(vec![])),
            "judgment" => set(record, "judgment_sha256", s("bad")),
            _ => set(record, "unexpected", Value::Null),
        }
        let invalid = reseal(value);
        assert!(
            holds::registry_value(&invalid, root, PROJECT, true).is_err(),
            "{corruption}"
        );
    }
}

#[test]
fn record_owned_bank_lock_retains_first_detection_and_resolution_history() {
    let w = World::new();
    let paths = w.paths();
    let env = w.environment();
    let binding = obj([
        ("bank_root", s("memory-bank")),
        ("base", s("a".repeat(40))),
        ("authority_head", s("b".repeat(40))),
        ("scope_context", obj([])),
    ]);
    let mut judgment = obj([
        ("binding", binding),
        ("dependencies", obj([])),
        ("rationale", s("initial")),
    ]);
    let mut proof = obj([
        ("judgment", judgment.clone()),
        ("held_causes", Value::Array(vec![cause(0)])),
    ]);
    let references = Value::Object(vec![("0".repeat(64), reference())]);
    let runtime = w.0.join("runtime");
    let actual = holds::locations_with_environment(&paths.root, PROJECT, &runtime, &env).unwrap();
    let bank_lock = paths.bank.join(".agent-memory.lock");
    storage::exclusive(
        &bank_lock,
        OPERATION,
        "semantic-review-holds",
        Sources::default(),
        |owner| {
            holds::record_with_environment(
                &paths.root,
                PROJECT,
                &runtime,
                &proof,
                &references,
                RecordOptions {
                    owned_lock: Some(owner),
                    ..RecordOptions::default()
                },
                &env,
            )?;
            let original = std::fs::read(actual.directory.join("registry.json")).unwrap();
            holds::record_with_environment(
                &paths.root,
                PROJECT,
                &runtime,
                &proof,
                &references,
                RecordOptions {
                    owned_lock: Some(owner),
                    ..RecordOptions::default()
                },
                &env,
            )?;
            assert_eq!(
                std::fs::read(actual.directory.join("registry.json")).unwrap(),
                original
            );
            let key = holds::cause_id(&cause(0))?;
            let mut prior = None;
            for i in 0..3 {
                set(&mut judgment, "rationale", s(format!("clarification {i}")));
                set(&mut proof, "judgment", judgment.clone());
                let resolutions = Value::Object(vec![(
                    key.clone(),
                    obj([
                        ("basis", s("operator-clarification")),
                        ("evidence", Value::Array(vec![s("f".repeat(64))])),
                        ("reason", s("Blue confirmed")),
                        ("endpoint_sources", obj([])),
                    ]),
                )]);
                let before = holds::read_store_at(&actual, PROJECT, ReadOptions::default())?.0;
                let expected = sha(&encoded(&Value::Array(holds::filtered_records(
                    &before, None,
                )?))?);
                holds::record_with_environment(
                    &paths.root,
                    PROJECT,
                    &runtime,
                    &proof,
                    &references,
                    RecordOptions {
                        resolutions: Some(&resolutions),
                        owned_lock: Some(owner),
                        expected_inventory: Some(&expected),
                        ..RecordOptions::default()
                    },
                    &env,
                )?;
                let current = holds::read_store_at(&actual, PROJECT, ReadOptions::default())?.0;
                let record = current.get("records").unwrap().get(&key).unwrap();
                assert_eq!(
                    record.get("judgment_sha256"),
                    json::decode_bytes(&original, json::DecodePolicy::default())?
                        .get("records")
                        .unwrap()
                        .get(&key)
                        .unwrap()
                        .get("judgment_sha256")
                );
                if let Some(prior) = prior {
                    assert_eq!(
                        record
                            .get("resolution_history")
                            .unwrap()
                            .as_array()
                            .unwrap()
                            .last(),
                        Some(&prior)
                    );
                }
                let judgment_sha = sha(&encoded(&judgment)?);
                let filtered = holds::filtered_records(&current, Some(&judgment_sha))?;
                assert_eq!(
                    encoded(&Value::Array(filtered))?,
                    encoded(&Value::Array(holds::filtered_records(&before, None)?))?
                );
                prior = record.get("resolution").cloned();
                let retained = snapshot(&w.0);
                holds::record_with_environment(
                    &paths.root,
                    PROJECT,
                    &runtime,
                    &proof,
                    &references,
                    RecordOptions {
                        resolutions: Some(&resolutions),
                        owned_lock: Some(owner),
                        expected_inventory: Some(&expected),
                        ..RecordOptions::default()
                    },
                    &env,
                )?;
                assert_eq!(snapshot(&w.0), retained);
            }
            let error = holds::record_with_environment(
                &paths.root,
                PROJECT,
                &runtime,
                &proof,
                &references,
                RecordOptions {
                    owned_lock: Some(owner),
                    expected_inventory: Some(&"0".repeat(64)),
                    ..RecordOptions::default()
                },
                &env,
            )
            .unwrap_err();
            assert_eq!(
                error.message(),
                "Known dispute inventory changed; repeat the actual current review"
            );
            Ok(())
        },
    )
    .unwrap();
    assert!(
        bank_writes::state_root_with(Some(&paths.root), &env)
            .unwrap()
            .starts_with(&w.0)
    );
    assert!(holds::bank_owner(&paths.root, OPERATION, Some(b"foreign"), |_| Ok(())).is_err());
}

#[test]
fn explicit_runtime_boundary_and_missing_registrations_refuse_without_writes() {
    let w = World::new();
    let paths = w.paths();
    let environment = StateEnvironment {
        state_home: Some(w.0.join("runtime/private-state").into_os_string()),
        xdg_state_home: None,
        home: Some(w.0.clone().into_os_string()),
    };
    let before = snapshot(&w.0);
    assert_eq!(
        holds::locations_with_environment(&paths.root, PROJECT, &w.0.join("runtime"), &environment)
            .unwrap_err()
            .message(),
        "Known hold state must stay outside the executing runtime/source"
    );
    assert_eq!(
        holds::locations_with_environment(
            &paths.root,
            "not-a-uuid",
            &w.0.join("runtime"),
            &w.environment()
        )
        .unwrap_err()
        .message(),
        "Known holds require the actual canonical project UUID"
    );
    assert!(
        holds::recover_with_environment(
            &paths.root,
            PROJECT,
            &w.0.join("runtime"),
            &w.environment()
        )
        .is_err()
    );
    assert!(holds::bank_owner(&paths.root, OPERATION, None, |_| Ok(())).is_err());
    assert_eq!(snapshot(&w.0), before);
}

#[test]
fn legacy_namespace_inventory_is_bounded_and_existing_keys_are_not_reimported() {
    let w = World::new();
    let paths = w.paths();
    let value = store(&paths.root, 1);
    let key = holds::cause_id(&cause(0)).unwrap();
    let legacy_path = paths
        .directory
        .parent()
        .unwrap()
        .join(format!("{key}.json"));
    // Already imported evidence is retained, not reread or overwritten.
    w.write(&legacy_path, b"unknown old bytes");
    let desired = holds::import_legacy(
        &paths.root,
        PROJECT,
        &paths.directory,
        value.get("records").unwrap(),
    )
    .unwrap();
    assert_eq!(desired, value);
    for i in 0..holds::MAX_RECORDS + 64 {
        w.write(
            &paths
                .directory
                .parent()
                .unwrap()
                .join(format!("retained-{i}")),
            b"",
        );
    }
    let before = snapshot(&w.0);
    assert_eq!(
        code(&holds::legacy_paths(&paths.directory).unwrap_err()),
        Some("known-holds-legacy-capacity")
    );
    assert_eq!(snapshot(&w.0), before);
}

#[test]
fn unknown_journals_changed_anchors_and_incomplete_backups_are_preserved() {
    for mode in [
        "unknown-journal",
        "changed-content",
        "changed-anchor",
        "backup-only",
        "anchor-only",
    ] {
        let w = World::new();
        let paths = w.paths();
        let value = store(&paths.root, 1);
        seed(&paths, &value);
        match mode {
            "unknown-journal" => w.write(&paths.directory.join("journal.json"), b"{}"),
            "changed-content" => {
                let journal = holds::seal(&obj([
                    ("schema", s("agent-memory/known-holds-journal-v1")),
                    ("repo_root", s(paths.root.to_str().unwrap())),
                    ("project_id", s(PROJECT)),
                    ("observed_sha256", Value::Null),
                    ("desired", value),
                    ("desired_sha256", s("0".repeat(64))),
                ]))
                .unwrap();
                w.write(
                    &paths.directory.join("journal.json"),
                    encoded(&journal).unwrap(),
                );
            }
            "changed-anchor" => w.write(&paths.anchor, b"{}"),
            "backup-only" => {
                std::fs::remove_file(paths.directory.join("registry.json")).unwrap();
                std::fs::remove_file(&paths.anchor).unwrap();
            }
            _ => {
                std::fs::remove_file(paths.directory.join("registry.json")).unwrap();
                std::fs::remove_file(paths.directory.join("recovery.json")).unwrap();
            }
        }
        let before = snapshot(&w.0);
        let error = storage::exclusive(
            &paths.directory.join("lock"),
            OPERATION,
            "semantic-holds",
            Sources::default(),
            |owner| holds::recover_store(&paths, PROJECT, owner),
        )
        .unwrap_err();
        assert_eq!(code(&error), None, "{mode}");
        assert_eq!(snapshot(&w.0), before, "{mode}");
    }
}
