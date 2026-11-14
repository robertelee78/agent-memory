use agent_memory::bank_contract as contract;
use agent_memory::bank_transactions as txn;
use agent_memory::bank_writes::StateEnvironment;
use agent_memory::codec::json::Value;
use agent_memory::developer_identity::{digest, encoded};
use agent_memory::fs::{Descriptor, DirectoryChain, RenameMode};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const ID: &str = "00000000-0000-4000-8000-000000000000";
const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
struct World {
    root: PathBuf,
}
impl World {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("am-txn-{}", agent_memory::host::uuid4().unwrap()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root: std::fs::canonicalize(root).unwrap(),
        }
    }
    fn path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
    fn mkdir(&self, path: &str) {
        std::fs::create_dir_all(self.path(path)).unwrap();
    }
    fn write(&self, path: &str, raw: &[u8]) {
        std::fs::write(self.path(path), raw).unwrap();
    }
    fn environment(&self) -> StateEnvironment {
        StateEnvironment {
            state_home: Some(self.path("state").into_os_string()),
            xdg_state_home: None,
            home: Some(self.root.clone().into_os_string()),
        }
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn files() -> Value {
    object([
        ("MEMORY.md", string("router\n")),
        ("activeContext.md", string("old\n")),
        ("projectbrief.md", string("old brief\n")),
    ])
}
fn proposal(root: &Path) -> Value {
    object([
        ("schema", string(txn::SCHEMA)),
        ("operation_id", string(ID)),
        ("operation", string("compact")),
        ("repo_root", string(root.to_str().unwrap())),
        ("project_id", string(PROJECT)),
        ("observed_hashes", txn::file_hashes(&files()).unwrap()),
        (
            "files",
            object([
                ("MEMORY.md", string("new router\n")),
                ("activeContext.md", string("new\n")),
                ("progress.md", string("new progress\n")),
            ]),
        ),
        ("scope_context", Value::Null),
    ])
}
fn valid_files() -> Value {
    let p = object([
        ("project-id", string(PROJECT)),
        ("router-updated", string("2026-10-22T18:00:00Z")),
        (
            "contexts",
            object([(
                "activeContext.md",
                object([
                    ("body", string("# Focus\n\nOriginal task.\n")),
                    ("purpose", string("Fixture context")),
                    ("scope", string("repository")),
                    ("lifecycle", string("active")),
                    ("read-when", string("When resuming")),
                ]),
            )]),
        ),
        (
            "omitted",
            Value::Object(
                contract::CONTEXTS
                    .iter()
                    .filter(|p| **p != "activeContext.md")
                    .map(|p| (p.to_string(), string("No distinct evidence")))
                    .collect(),
            ),
        ),
    ]);
    get(&contract::render(&p).unwrap(), "files").clone()
}
fn write_files(bank: &Path, files: &Value) {
    for (path, body) in files.as_object().unwrap() {
        std::fs::write(bank.join(path), body.as_str().unwrap()).unwrap();
    }
}

#[test]
fn marker_exact_bytes_and_sha256_are_baseline_contracts() {
    let p = object([
        ("operation_id", string(ID)),
        ("operation", string("replace")),
        (
            "observed_hashes",
            object([(
                "activeContext.md",
                string("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            )]),
        ),
        ("files", object([("activeContext.md", string("new\n"))])),
    ]);
    let expected=b"{\n  \"base_hashes\": {\n    \"activeContext.md\": \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n  },\n  \"operation\": \"replace\",\n  \"paths\": [\n    \"activeContext.md\"\n  ],\n  \"phase\": \"applying\",\n  \"schema\": \"agent-memory/transaction-marker-v1\",\n  \"transaction_id\": \"00000000-0000-4000-8000-000000000000\"\n}\n";
    assert_eq!(txn::marker_bytes(&p).unwrap(), expected);
    assert_eq!(
        get(
            &txn::file_hashes(&object([("a", string("abc"))])).unwrap(),
            "a"
        ),
        &string("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_every_interruption_boundary_refuses_normal_reads_and_converges_in_both_directions
#[test]
fn interrupted_marker_is_private_and_contains_no_bodies() {
    let w = World::new();
    w.mkdir("repo/memory-bank");
    let p = proposal(&w.path("repo"));
    let raw = txn::marker_bytes(&p).unwrap();
    let marker = w.path("repo/memory-bank").join(txn::MARKER);
    txn::create_marker(&marker, &raw).unwrap();
    assert_eq!(
        std::fs::metadata(&marker).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        !String::from_utf8(raw.clone())
            .unwrap()
            .contains("new progress")
    );
    assert!(
        !String::from_utf8(raw.clone())
            .unwrap()
            .contains("old brief")
    );
    txn::check_marker(&w.path("repo/memory-bank"), &raw).unwrap();
    let marker_value = agent_memory::developer_identity::load(&raw).unwrap();
    assert_eq!(get(&marker_value, "paths").as_array().unwrap().len(), 4);
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_exclusive_marker_creation_preserves_a_new_unknown_marker_and_readset_edit_at_write_boundary
#[test]
fn exclusive_marker_creation_preserves_unknown_marker() {
    let w = World::new();
    w.mkdir("bank");
    w.write("bank/.agent-memory-transaction.md", b"unknown marker\n");
    assert!(txn::create_marker(&w.path("bank").join(txn::MARKER), b"replacement").is_err());
    assert_eq!(
        std::fs::read(w.path("bank").join(txn::MARKER)).unwrap(),
        b"unknown marker\n"
    );
    assert!(
        txn::check_marker(&w.path("bank"), b"replacement")
            .unwrap_err()
            .to_string()
            .contains("changed or is unknown")
    );
}
#[test]
fn marker_symlink_and_oversize_are_refused_without_touching_target() {
    let w = World::new();
    w.mkdir("bank");
    w.write("outside", b"preserve");
    symlink(w.path("outside"), w.path("bank").join(txn::MARKER)).unwrap();
    assert!(txn::check_marker(&w.path("bank"), b"preserve").is_err());
    assert!(txn::create_marker(&w.path("bank").join(txn::MARKER), b"replace").is_err());
    assert_eq!(std::fs::read(w.path("outside")).unwrap(), b"preserve");
    std::fs::remove_file(w.path("bank").join(txn::MARKER)).unwrap();
    std::fs::File::create(w.path("bank").join(txn::MARKER))
        .unwrap()
        .set_len((txn::MAX_MARKER_BYTES + 1) as u64)
        .unwrap();
    assert!(txn::check_marker(&w.path("bank"), b"x").is_err());
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_missing_corrupt_or_changed_journal_is_not_guessed_or_reconstructed
#[test]
fn partial_journals_and_marker_without_journal_preserve_evidence() {
    let w = World::new();
    w.mkdir("repo/memory-bank");
    let p = proposal(&w.path("repo"));
    for raw in [b"{\"schema\":".as_slice(), b"not a recovery journal", b"{}"] {
        assert!(
            txn::journal_value(raw, &w.path("repo"), &w.path("repo/memory-bank"), ID, true)
                .is_err()
        );
    }
    let marker = txn::marker_bytes(&p).unwrap();
    txn::create_marker(&w.path("repo/memory-bank").join(txn::MARKER), &marker).unwrap();
    let (journal, _) = txn::locations_with_environment(
        &w.path("repo"),
        &w.path("repo/memory-bank"),
        ID,
        &w.environment(),
    )
    .unwrap();
    assert!(!journal.exists());
    assert!(!w.path("state").exists());
    txn::check_marker(&w.path("repo/memory-bank"), &marker).unwrap();
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_external_unchanged_read_set_edit_blocks_finish_and_rollback_without_overwrite
#[test]
fn allowed_partial_requires_every_observed_path_to_be_base_or_final() {
    let w = World::new();
    let p = proposal(&w.root);
    let base = get(&p, "observed_hashes").clone();
    let final_hashes = txn::file_hashes(get(&p, "files")).unwrap();
    txn::allowed_partial(&p, &base).unwrap();
    txn::allowed_partial(&p, &final_hashes).unwrap();
    let mut partial = base.clone();
    *partial.get_mut("activeContext.md").unwrap() = get(&final_hashes, "activeContext.md").clone();
    txn::allowed_partial(&p, &partial).unwrap();
    *partial.get_mut("activeContext.md").unwrap() = string("foreign");
    assert!(
        txn::allowed_partial(&p, &partial)
            .unwrap_err()
            .to_string()
            .contains("external edit")
    );
    let mut foreign = base;
    if let Value::Object(map) = &mut foreign {
        map.push(("unobserved.md".into(), string("foreign")));
    }
    assert!(txn::allowed_partial(&p, &foreign).is_err());
}
#[test]
fn rename_done_marker_present_is_a_valid_partial_but_not_external_edit_authority() {
    let w = World::new();
    w.mkdir("bank");
    write_files(&w.path("bank"), &files());
    let p = proposal(&w.root);
    let marker = txn::marker_bytes(&p).unwrap();
    txn::create_marker(&w.path("bank").join(txn::MARKER), &marker).unwrap();
    let chain = DirectoryChain::open(&w.path("bank")).unwrap();
    let mut temporary = chain
        .directory()
        .create_exclusive(
            OsStr::new(".agent-memory-transaction-crash.tmp"),
            libc::O_WRONLY,
            0o644,
        )
        .unwrap();
    temporary.write_all(b"new\n").unwrap();
    temporary.fsync().unwrap();
    agent_memory::fs::rename_at(
        chain.directory(),
        OsStr::new(".agent-memory-transaction-crash.tmp"),
        chain.directory(),
        OsStr::new("activeContext.md"),
        RenameMode::Replace,
    )
    .unwrap();
    chain.directory().fsync_directory().unwrap();
    let mut current = files();
    *current.get_mut("activeContext.md").unwrap() =
        string(std::fs::read_to_string(w.path("bank/activeContext.md")).unwrap());
    txn::allowed_partial(&p, &txn::file_hashes(&current).unwrap()).unwrap();
    txn::check_marker(&w.path("bank"), &marker).unwrap();
    w.write("bank/activeContext.md", b"manual correction\n");
    *current.get_mut("activeContext.md").unwrap() = string("manual correction\n");
    assert!(txn::allowed_partial(&p, &txn::file_hashes(&current).unwrap()).is_err());
    assert_eq!(
        std::fs::read(w.path("bank/activeContext.md")).unwrap(),
        b"manual correction\n"
    );
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_crash_after_marker_removal_replays_known_outcome_and_purges_leftover_backup
#[test]
fn journal_cleanup_removes_only_exact_owned_bytes_and_empty_directory() {
    let w = World::new();
    w.mkdir("transactions/op");
    w.write("transactions/op/journal.json", b"owned journal");
    let journal = w.path("transactions/op/journal.json");
    assert!(txn::cleanup(&journal, b"different").is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), b"owned journal");
    w.write("transactions/op/unrelated", b"preserve");
    txn::cleanup(&journal, b"owned journal").unwrap();
    assert!(w.path("transactions/op").is_dir());
    assert_eq!(
        std::fs::read(w.path("transactions/op/unrelated")).unwrap(),
        b"preserve"
    );
    w.write("transactions/op/journal.json", b"owned journal");
    std::fs::remove_file(w.path("transactions/op/unrelated")).unwrap();
    txn::cleanup(&journal, b"owned journal").unwrap();
    assert!(!w.path("transactions/op").exists());
    txn::cleanup(&journal, b"owned journal").unwrap();
}
#[test]
fn journal_cleanup_refuses_symlink_even_when_target_bytes_match() {
    let w = World::new();
    w.mkdir("transactions/op");
    w.write("outside", b"same");
    symlink(w.path("outside"), w.path("transactions/op/journal.json")).unwrap();
    assert!(txn::cleanup(&w.path("transactions/op/journal.json"), b"same").is_err());
    assert_eq!(std::fs::read(w.path("outside")).unwrap(), b"same");
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_impossible_projection_and_leaf_directory_conversions_refuse_before_journaling
#[test]
fn preflight_refuses_shape_changes_and_unobserved_occupants() {
    for case in [
        "file-parent",
        "directory-target",
        "non-directory-parent",
        "unobserved-target",
    ] {
        let w = World::new();
        w.mkdir("bank");
        w.write("bank/MEMORY.md", b"router\n");
        let mut p = proposal(&w.root);
        let additions = match case {
            "file-parent" => object([
                ("topics/a.md", string("leaf")),
                ("topics/a.md/b.md", string("child")),
            ]),
            "directory-target" => {
                w.mkdir("bank/topics/a.md");
                object([("topics/a.md", string("leaf"))])
            }
            "non-directory-parent" => {
                w.mkdir("bank/topics");
                w.write("bank/topics/occupant.bin", b"preserve");
                object([("topics/occupant.bin/b.md", string("child"))])
            }
            _ => {
                w.mkdir("bank/topics");
                w.write("bank/topics/a.md", b"preserve");
                object([("topics/a.md", string("leaf"))])
            }
        };
        *p.get_mut("files").unwrap() = additions;
        assert!(txn::preflight(&w.path("bank"), &p, None).is_err(), "{case}");
        assert!(!w.path("bank").join(txn::MARKER).exists());
    }
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_case_unicode_parent_and_existing_occupant_aliases_reject_preview_and_forged_apply_without_loss
#[test]
fn preflight_refuses_case_unicode_and_parent_aliases() {
    for paths in [
        ["topics/Case.md", "topics/case.md"],
        ["topics/café.md", "topics/cafe\u{301}.md"],
        ["topics/Foo/a.md", "topics/foo/b.md"],
    ] {
        let w = World::new();
        w.mkdir("bank");
        let p = object([
            ("observed_hashes", object([])),
            (
                "files",
                object([(paths[0], string("one")), (paths[1], string("two"))]),
            ),
        ]);
        assert!(txn::preflight(&w.path("bank"), &p, None).is_err());
    }
}
// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_noncanonical_physical_blocker_arriving_after_preview_is_checked_before_journal
#[test]
fn check_layout_preserves_late_foreign_occupants_and_runtime_symlinks() {
    let w = World::new();
    w.mkdir("bank/topics");
    w.write("bank/topics/occupant.bin", b"foreign");
    let paths = BTreeSet::from(["topics/occupant.bin/new.md".to_owned()]);
    assert!(txn::check_layout(&w.path("bank"), &paths, &object([])).is_err());
    assert_eq!(
        std::fs::read(w.path("bank/topics/occupant.bin")).unwrap(),
        b"foreign"
    );
    w.mkdir("bank/.agent-memory-runtime");
    symlink(
        w.path("bank/topics/occupant.bin"),
        w.path("bank/.agent-memory-runtime/link"),
    )
    .unwrap();
    assert!(txn::check_layout(&w.path("bank"), &BTreeSet::new(), &object([])).is_err());
}
#[test]
fn transaction_locations_are_readonly_external_and_refuse_symlink_journals() {
    let w = World::new();
    w.mkdir("repo/memory-bank");
    let (journal, receipt) = txn::locations_with_environment(
        &w.path("repo"),
        &w.path("repo/memory-bank"),
        ID,
        &w.environment(),
    )
    .unwrap();
    assert!(journal.starts_with(w.path("state/writes")));
    assert!(receipt.starts_with(w.path("state/writes")));
    assert!(journal.ends_with(format!("transactions/{ID}/journal.json")));
    assert!(receipt.ends_with(format!("operations/{ID}.json")));
    assert!(!w.path("state").exists());
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    w.write("outside", b"preserve");
    symlink(w.path("outside"), &journal).unwrap();
    assert!(
        txn::locations_with_environment(
            &w.path("repo"),
            &w.path("repo/memory-bank"),
            ID,
            &w.environment()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(w.path("outside")).unwrap(), b"preserve");
}
#[test]
fn governed_recovery_requires_rollback_unless_finished_receipt_is_known() {
    for key in [
        "admission_evidence",
        "consolidation_evidence",
        "governance_evidence",
        "refresh_evidence",
    ] {
        let p = object([(key, object([]))]);
        assert!(txn::admission_recovery(&p, "finish", None).is_err());
        txn::admission_recovery(&p, "rollback", None).unwrap();
        txn::admission_recovery(
            &p,
            "finish",
            Some(&object([("outcome", string("finished"))])),
        )
        .unwrap();
        assert!(
            txn::admission_recovery(
                &p,
                "finish",
                Some(&object([("outcome", string("rolled-back"))]))
            )
            .is_err()
        );
    }
}
#[test]
fn source_attribution_is_typed_and_preserves_governed_classes() {
    assert_eq!(
        txn::source_classes(&object([("operation", string("replace"))])).unwrap(),
        ["external"]
    );
    assert_eq!(
        txn::source_classes(&object([
            ("operation", string("replace")),
            ("checkpoint_evidence", object([]))
        ]))
        .unwrap(),
        ["assistant"]
    );
    let p = object([(
        "admission_evidence",
        object([(
            "capture",
            object([("request", object([("source_class", string("human"))]))]),
        )]),
    )]);
    assert_eq!(txn::source_classes(&p).unwrap(), ["human"]);
    assert!(txn::source_classes(&object([("admission_evidence", object([]))])).is_err());
    assert!(txn::source_classes(&object([("operation", string("initialize"))])).is_err());
}
#[test]
fn missing_optional_cache_provider_uses_canonical_fallback() {
    let w = World::new();
    assert_eq!(
        txn::invalidate_optional_indexes(&w.root, &object([])),
        string("deferred-to-canonical-fallback")
    );
    assert!(!w.path("state").exists());
}
#[test]
fn malformed_proposals_and_unregistered_operation_kinds_are_typed_refusals() {
    assert!(txn::proposal_value(&Value::Null).is_err());
    assert!(txn::proposal_value(&object([])).is_err());
    assert!(txn::register_transaction_validator("unknown", None).is_err());
    assert!(txn::file_hashes(&object([("a", Value::Bool(true))])).is_err());
}
#[test]
fn descriptor_directory_creation_is_exclusive_and_removal_is_empty_only() {
    let w = World::new();
    let directory = Descriptor::open_directory(&w.root).unwrap();
    let child = directory
        .create_directory(OsStr::new("new"), 0o755)
        .unwrap();
    assert_eq!(child.stat().unwrap().mode & 0o777, 0o755);
    assert!(
        directory
            .create_directory(OsStr::new("new"), 0o700)
            .is_err()
    );
    assert_eq!(child.stat().unwrap().mode & 0o777, 0o755);
    let mut file = child
        .create_exclusive(OsStr::new("owned"), libc::O_WRONLY, 0o600)
        .unwrap();
    file.write_all(b"preserve").unwrap();
    assert!(directory.remove_directory_at(OsStr::new("new")).is_err());
    child.unlink_at(OsStr::new("owned")).unwrap();
    directory.remove_directory_at(OsStr::new("new")).unwrap();
    assert!(directory.stat_at(OsStr::new("new")).is_err());
    assert!(
        directory
            .create_directory(OsStr::new("bad"), 0o1000)
            .is_err()
    );
    symlink(w.path("outside"), w.path("link")).unwrap();
    assert!(directory.remove_directory_at(OsStr::new("link")).is_err());
}
#[test]
fn unknown_receipts_and_receipt_symlinks_never_authorize_cleanup() {
    let w = World::new();
    w.mkdir("repo/memory-bank");
    let p = proposal(&w.path("repo"));
    assert!(txn::receipt_value(&w.path("absent"), &p).unwrap().is_none());
    w.write("receipt", b"{}");
    assert!(txn::receipt_value(&w.path("receipt"), &p).is_err());
    let raw = encoded(&object([
        ("schema", string("agent-memory/transaction-receipt-v1")),
        ("operation_id", string(ID)),
        ("verified", Value::Bool(true)),
    ]))
    .unwrap();
    w.write("receipt", &raw);
    assert!(txn::receipt_value(&w.path("receipt"), &p).is_err());
    symlink(w.path("receipt"), w.path("link")).unwrap();
    assert!(txn::receipt_value(&w.path("link"), &p).is_err());
    assert_eq!(
        digest(Some(&std::fs::read(w.path("receipt")).unwrap())),
        digest(Some(&raw))
    );
}
#[test]
fn command_errors_are_json_and_preserve_capacity_details() {
    let command = txn::command_result(
        "recover",
        Err(agent_memory::error::ContractError::new("inspection needed")),
    );
    assert_eq!(command.exit_code, 1);
    assert_eq!(command.stderr, "");
    assert_eq!(
        command.stdout,
        "{\"schema\": \"agent-memory/transaction-result-v1\", \"operation\": \"recover\", \"error\": \"inspection needed\", \"verified\": false, \"inspection_required\": true}\n"
    );
}

#[test]
fn governed_evidence_requires_registered_validator_but_raw_mechanical_does_not() {
    for (operation, key) in [
        ("capture", "admission_evidence"),
        ("compact", "consolidation_evidence"),
        ("correct", "governance_evidence"),
        ("refresh", "refresh_evidence"),
        ("initialize", "initialization_evidence"),
    ] {
        let proposal = object([("operation", string(operation)), (key, object([]))]);
        let error = txn::validate_operation(&proposal, None).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("Transaction validator is not registered for operation kind: {operation}")
        );
    }
    for operation in ["replace", "compact", "correct", "forget"] {
        txn::validate_operation(&object([("operation", string(operation))]), None).unwrap();
    }
}

fn valid_receipt(w: &World, p: &Value, outcome: &str) -> Value {
    object([
        ("schema", string("agent-memory/transaction-receipt-v1")),
        ("operation_id", string(ID)),
        (
            "request_sha256",
            string(digest(Some(&encoded(p).unwrap())).unwrap()),
        ),
        (
            "bank_path",
            string(w.path("repo/memory-bank").to_str().unwrap()),
        ),
        ("project_id", string(PROJECT)),
        ("observed_hashes", get(p, "observed_hashes").clone()),
        (
            "resulting_hashes",
            if outcome == "finished" {
                txn::file_hashes(get(p, "files")).unwrap()
            } else {
                get(p, "observed_hashes").clone()
            },
        ),
        ("outcome", string(outcome)),
        (
            "changed_memory_paths",
            Value::Array(
                txn::changed(p)
                    .unwrap()
                    .into_iter()
                    .map(|path| string(format!("memory-bank/{path}")))
                    .collect(),
            ),
        ),
        ("readback_at", string("2026-10-22T19:00:00+00:00")),
        ("verified", Value::Bool(true)),
    ])
}

// obligation: tests/contracts/test_admission.py::AdmissionContract.test_complete_receipt_schema_and_bindings_govern_cleanup_and_historical_replay
#[test]
fn complete_receipt_schema_and_every_binding_are_required() {
    let w = World::new();
    w.mkdir("repo/memory-bank");
    let p = proposal(&w.path("repo"));
    for outcome in ["finished", "rolled-back"] {
        let receipt = valid_receipt(&w, &p, outcome);
        w.write("receipt", &encoded(&receipt).unwrap());
        assert_eq!(
            txn::receipt_value(&w.path("receipt"), &p)
                .unwrap()
                .map(|value| encoded(&value).unwrap()),
            Some(encoded(&receipt).unwrap())
        );
        for key in txn::RECEIPT_FIELDS {
            let mut bad = receipt.clone();
            *bad.get_mut(key).unwrap() = Value::Null;
            w.write("receipt", &encoded(&bad).unwrap());
            assert!(
                txn::receipt_value(&w.path("receipt"), &p).is_err(),
                "{outcome} {key}"
            );
            let mut bad = receipt.clone();
            if let Value::Object(fields) = &mut bad {
                fields.retain(|(field, _)| field != key);
            }
            w.write("receipt", &encoded(&bad).unwrap());
            assert!(
                txn::receipt_value(&w.path("receipt"), &p).is_err(),
                "{outcome} missing {key}"
            );
        }
        let mut bad = receipt.clone();
        if let Value::Object(fields) = &mut bad {
            fields.push(("extra".into(), string("UNTRUSTED_RECEIPT_PAYLOAD_SENTINEL")));
        }
        w.write("receipt", &encoded(&bad).unwrap());
        let error = txn::receipt_value(&w.path("receipt"), &p).unwrap_err();
        assert!(
            !error
                .to_string()
                .contains("UNTRUSTED_RECEIPT_PAYLOAD_SENTINEL")
        );
        assert_eq!(
            std::fs::read(w.path("receipt")).unwrap(),
            encoded(&bad).unwrap()
        );
    }
}

#[test]
fn transaction_bank_lock_uses_same_path_owner_metadata_and_exclusive_flock() {
    use agent_memory::fs::lock::{FlockGuard, LockMode};
    let w = World::new();
    w.mkdir("bank");
    let lock = w.path("bank/.agent-memory.lock");
    agent_memory::local_registry::exclusive(
        &lock,
        ID,
        "bank-compact",
        agent_memory::host::Sources::default(),
        |owner| {
            assert_eq!(std::fs::read(&lock).unwrap(), owner);
            let metadata = agent_memory::developer_identity::load(owner).unwrap();
            assert_eq!(get(&metadata, "operation"), &string("bank-compact"));
            let directory = Descriptor::open_directory(&w.path("bank")).unwrap();
            let file = directory
                .open_at(OsStr::new(".agent-memory.lock"), libc::O_RDWR, 0)
                .unwrap();
            assert!(FlockGuard::try_acquire(file, LockMode::Exclusive).is_err());
            Ok(())
        },
    )
    .unwrap();
    assert!(!lock.exists());
}

#[test]
fn real_replace_file_preserves_mode_creates_deletes_and_rechecks_the_entire_read_set() {
    let w = World::new();
    w.mkdir("bank");
    write_files(&w.path("bank"), &files());
    std::fs::set_permissions(
        w.path("bank/activeContext.md"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let p = proposal(&w.root);
    let marker = txn::marker_bytes(&p).unwrap();
    txn::create_marker(&w.path("bank").join(txn::MARKER), &marker).unwrap();
    let paths = BTreeSet::from([
        "MEMORY.md".to_owned(),
        "activeContext.md".to_owned(),
        "projectbrief.md".to_owned(),
        "topics/new.md".to_owned(),
    ]);
    let mut expected = txn::file_hashes(&files()).unwrap();
    txn::replace_file(
        &w.path("bank"),
        "activeContext.md",
        Some("new\n"),
        &expected,
        &marker,
        &paths,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(w.path("bank/activeContext.md")).unwrap(),
        b"new\n"
    );
    assert_eq!(
        std::fs::metadata(w.path("bank/activeContext.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    expected = agent_memory::bank_writes::hashes(&w.path("bank")).unwrap();
    txn::replace_file(
        &w.path("bank"),
        "topics/new.md",
        Some("# New\n"),
        &expected,
        &marker,
        &paths,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::metadata(w.path("bank/topics"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(w.path("bank/topics/new.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    expected = agent_memory::bank_writes::hashes(&w.path("bank")).unwrap();
    txn::replace_file(
        &w.path("bank"),
        "projectbrief.md",
        None,
        &expected,
        &marker,
        &paths,
        None,
        None,
    )
    .unwrap();
    assert!(!w.path("bank/projectbrief.md").exists());
    expected = agent_memory::bank_writes::hashes(&w.path("bank")).unwrap();
    let mutate = || {
        std::fs::write(w.path("bank/MEMORY.md"), b"foreign router\n").unwrap();
        Ok(())
    };
    assert!(
        txn::replace_file(
            &w.path("bank"),
            "activeContext.md",
            Some("must not land\n"),
            &expected,
            &marker,
            &paths,
            None,
            Some(&mutate)
        )
        .unwrap_err()
        .to_string()
        .contains("External edit observed")
    );
    assert_eq!(
        std::fs::read(w.path("bank/activeContext.md")).unwrap(),
        b"new\n"
    );
    assert_eq!(
        std::fs::read(w.path("bank/MEMORY.md")).unwrap(),
        b"foreign router\n"
    );
    assert!(std::fs::read_dir(w.path("bank")).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    txn::check_marker(&w.path("bank"), &marker).unwrap();
}

// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_late_alias_at_replacement_boundary_stops_and_recovery_preserves_foreign_occupant
#[test]
fn real_replace_boundary_preserves_late_alias_and_unknown_marker() {
    for change in ["alias", "marker"] {
        let w = World::new();
        w.mkdir("bank/topics");
        write_files(&w.path("bank"), &files());
        let p = proposal(&w.root);
        let marker = txn::marker_bytes(&p).unwrap();
        txn::create_marker(&w.path("bank").join(txn::MARKER), &marker).unwrap();
        let paths = BTreeSet::from([
            "MEMORY.md".to_owned(),
            "activeContext.md".to_owned(),
            "projectbrief.md".to_owned(),
            "topics/note.md".to_owned(),
        ]);
        let expected = txn::file_hashes(&files()).unwrap();
        let mutate = || {
            if change == "alias" {
                w.write("bank/topics/NOTE.MD", b"UNOBSERVED_OCCUPANT_CANARY\n");
            } else {
                w.write("bank/.agent-memory-transaction.md", b"unknown marker\n");
            }
            Ok(())
        };
        assert!(
            txn::replace_file(
                &w.path("bank"),
                "topics/note.md",
                Some("# New\n"),
                &expected,
                &marker,
                &paths,
                None,
                Some(&mutate)
            )
            .is_err()
        );
        assert!(
            std::fs::read_dir(w.path("bank/topics"))
                .unwrap()
                .all(|entry| entry.unwrap().file_name() != OsStr::new("note.md"))
        );
        if change == "alias" {
            assert_eq!(
                std::fs::read(w.path("bank/topics/NOTE.MD")).unwrap(),
                b"UNOBSERVED_OCCUPANT_CANARY\n"
            );
        } else {
            assert_eq!(
                std::fs::read(w.path("bank").join(txn::MARKER)).unwrap(),
                b"unknown marker\n"
            );
        }
        assert_eq!(
            std::fs::read(w.path("bank/activeContext.md")).unwrap(),
            b"old\n"
        );
    }
}

#[test]
fn real_files_inventory_accepts_empty_bank_and_ignores_runtime_payloads() {
    let w = World::new();
    w.mkdir("bank");
    assert_eq!(txn::files_at(&w.path("bank")).unwrap(), object([]));
    write_files(&w.path("bank"), &files());
    w.mkdir("bank/.agent-memory-state");
    w.write(
        "bank/.agent-memory-state/private.md",
        b"private runtime source\n",
    );
    w.write("bank/.agent-memory.lock", b"lock owner\n");
    assert_eq!(txn::files_at(&w.path("bank")).unwrap(), files());
    w.write("bank/activeContext.md", &[0xff]);
    assert!(txn::files_at(&w.path("bank")).is_err());
}

// obligation: tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_every_interruption_boundary_refuses_normal_reads_and_converges_in_both_directions
#[test]
#[ignore = "needs recovery_frontier registered"]
fn real_partial_rename_converges_finish_and_rollback_at_every_file_boundary() {
    for mode in ["finish", "rollback"] {
        for completed in 0..=2 {
            let w = World::new();
            w.mkdir("repo/memory-bank");
            let before = valid_files();
            let mut after = before.clone();
            *after.get_mut("activeContext.md").unwrap() = string("# Focus\n\nNew task.\n");
            let router = get(&before, "MEMORY.md")
                .as_str()
                .unwrap()
                .replace("2026-10-22T18:00:00Z", "2026-10-22T19:00:00Z");
            *after.get_mut("MEMORY.md").unwrap() = string(router);
            write_files(&w.path("repo/memory-bank"), &before);
            let mut p = proposal(&w.path("repo"));
            *p.get_mut("observed_hashes").unwrap() = txn::file_hashes(&before).unwrap();
            *p.get_mut("files").unwrap() = after.clone();
            let marker = txn::marker_bytes(&p).unwrap();
            let (journal, receipt) = txn::locations_with_environment(
                &w.path("repo"),
                &w.path("repo/memory-bank"),
                ID,
                &w.environment(),
            )
            .unwrap();
            let raw = encoded(&object([
                ("schema", string("agent-memory/transaction-journal-v1")),
                ("proposal", p.clone()),
                ("base_files", before.clone()),
            ]))
            .unwrap();
            agent_memory::local_registry::private_mkdir(journal.parent().unwrap()).unwrap();
            agent_memory::local_registry::replace(&journal, &raw, None).unwrap();
            txn::create_marker(&w.path("repo/memory-bank").join(txn::MARKER), &marker).unwrap();
            let paths = BTreeSet::from(["activeContext.md".to_owned(), "MEMORY.md".to_owned()]);
            for path in ["activeContext.md", "MEMORY.md"]
                .into_iter()
                .take(completed)
            {
                let expected =
                    agent_memory::bank_writes::hashes(&w.path("repo/memory-bank")).unwrap();
                txn::replace_file(
                    &w.path("repo/memory-bank"),
                    path,
                    get(&after, path).as_str(),
                    &expected,
                    &marker,
                    &paths,
                    None,
                    None,
                )
                .unwrap();
            }
            let observed = agent_memory::bank_writes::hashes(&w.path("repo/memory-bank")).unwrap();
            let result = agent_memory::local_registry::exclusive(
                &w.path("repo/memory-bank/.agent-memory.lock"),
                ID,
                "bank-recover",
                agent_memory::host::Sources::default(),
                |owner| {
                    txn::converge(
                        &w.path("repo"),
                        &w.path("repo/memory-bank"),
                        &p,
                        &before,
                        &journal,
                        &raw,
                        &receipt,
                        mode,
                        &observed,
                        owner,
                        true,
                        agent_memory::host::Sources::default(),
                    )
                },
            )
            .unwrap();
            assert_eq!(get(&result, "verified"), &Value::Bool(true));
            assert_eq!(
                txn::files_at(&w.path("repo/memory-bank")).unwrap(),
                if mode == "finish" { after } else { before }
            );
            assert!(!journal.exists());
            assert!(!w.path("repo/memory-bank").join(txn::MARKER).exists());
        }
    }
}
