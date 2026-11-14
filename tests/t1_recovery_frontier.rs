use agent_memory::codec::json::Value;
use agent_memory::developer_identity::{digest, encoded};
use agent_memory::error::Kind;
use agent_memory::{bank_contract, layout_recovery_state as state, recovery_frontier as frontier};
use std::ffi::OsString;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
mod support;
use support::{RunOptions, World};

const PROJECT: &str = "75b38e17-c36e-4fab-8ba9-5fa51f25de91";
const OPERATION: &str = "b3dcae21-f5f3-40ac-bcfe-e845a3d2868b";
const PROPOSAL: &str = "bdcb692b-2322-4f21-b9dd-a4a4eca21145";
const HOLD_ID: &str = "3e7c5e91-bac1-4256-b423-d788b388a701";
const SOURCE: &str = "99809987-92b5-48ab-b9fd-b984cd3ef5b5";
const UNIT: &str = "3e7c5e91-bac1-4256-b423-d788b388a702";
const DEVELOPER: &str = "3e7c5e91-bac1-4256-b423-d788b388a703";
const BODY: &[u8] = b"# Retained original\n\nThe current policy is unknown.\n";
fn string(value: impl Into<String>) -> Value { Value::String(value.into()) }
fn object<const N: usize>(rows: [(&str, Value); N]) -> Value {
    Value::Object(rows.into_iter().map(|(name, value)| (name.to_owned(), value)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value { value.get(key).unwrap() }
fn set(value: &mut Value, key: &str, replacement: Value) {
    if let Some(old) = value.get_mut(key) { *old = replacement; }
    else if let Value::Object(rows) = value { rows.push((key.to_owned(), replacement)); }
    else { panic!("fixture object"); }
}
fn hash(value: &Value) -> Value { string(digest(Some(&encoded(value).unwrap())).unwrap()) }
fn canonical_same(a: &Value, b: &Value) { assert_eq!(encoded(a).unwrap(), encoded(b).unwrap()); }
fn private_file(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn in_world(name: &str, body: impl FnOnce(&Path)) {
    if std::env::var("AM_FRONTIER_CASE").as_deref() == Ok(name) {
        let root = PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap());
        body(&root);
        return;
    }
    let world = World::new().unwrap();
    let env = [(OsString::from("AM_FRONTIER_CASE"), OsString::from(name))];
    world.run_with([
        std::env::current_exe().unwrap().into_os_string(), "--exact".into(), name.into(),
        "--include-ignored".into(), "--nocapture".into(),
    ], RunOptions { env: &env, ..Default::default() }).unwrap();
}
fn range(start: usize, end: usize) -> Value {
    object([("source_id", string(SOURCE)), ("start", Value::Int(start.into())), ("end", Value::Int(end.into()))])
}
fn unit() -> Value {
    object([("unit_id", string(UNIT)), ("source_ranges", Value::Array(vec![range(0, BODY.len())])),
        ("provenance", object([("source_class", string("external"))])), ("reason", string("Unresolved original conditions")),
        ("required_actions", Value::Array(vec![string("Independent review")])), ("detecting_developer_id", string(DEVELOPER)),
        ("state", string("pending-admission")), ("terminal_receipts", Value::Array(vec![]))])
}
struct Evidence {
    root: PathBuf, bank: PathBuf, runtime: PathBuf, directory: PathBuf,
    index: Value, row: Value, head: Value, ledger: Value, marker: Vec<u8>,
}
impl Evidence {
    fn new(root: &Path) -> Self {
        let repo = root.join("repo");
        let bank = repo.join("memory-bank");
        let runtime = root.join("data/runtime");
        for path in [&bank, &runtime] { std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path).unwrap(); }
        let (_, _, base, directory) = state::_paths_for_bank(&repo, &bank, Some(OPERATION), &runtime).unwrap();
        let directory = directory.unwrap();
        let review = object([("independent_source", string("Retained immutable review fixture; not a runtime capability"))]);
        let manifest = object([("observation_complete", Value::Bool(true)), ("project_id", string(PROJECT)),
            ("identity", object([("scope_context", object([("checkout_key", string("fixture-checkout"))]))])),
            ("sources", Value::Array(vec![object([("source_id", string(SOURCE)), ("path", string("handoff-prompt.md")),
                ("byte_count", Value::Int(BODY.len().into())), ("sha256", string(digest(Some(BODY)).unwrap())),
                ("blob", string("caller-locator-must-not-be-followed"))])]))]);
        let map = object([("units", Value::Array(vec![unit()]))]);
        let proposal = object([("proposal_id", string(PROPOSAL)), ("review", review.clone()), ("review_sha256", hash(&review))]);
        let ledger = object([("schema", string("agent-memory/layout-pending-ledger-v1")), ("operation_id", string(OPERATION)),
            ("project_id", string(PROJECT)), ("manifest_sha256", hash(&manifest)), ("unit_map_sha256", hash(&map)),
            ("proposal_sha256", hash(&proposal)), ("generation", Value::Int(0.into())),
            ("previous_generation_sha256", Value::Null), ("units", Value::Array(vec![unit()]))]);
        let head = object([("schema", string("agent-memory/layout-ledger-head-v1")), ("operation_id", string(OPERATION)),
            ("generation", Value::Int(0.into())), ("sha256", hash(&ledger))]);
        let row = object([("state", string("active")), ("project_id", string(PROJECT)), ("ledger_head_sha256", hash(&head)),
            ("proposal_id", string(PROPOSAL)), ("hold_id", string(HOLD_ID))]);
        let index = object([("schema", string(state::INDEX)), ("repo_root", string(repo.to_str().unwrap())),
            ("bank_path", string(bank.to_str().unwrap())), ("operations", object([(OPERATION, row.clone())]))]);
        for (path, value) in [
            (directory.join("manifest.json"), manifest), (directory.join("unit-map.json"), map),
            (directory.join("proposals").join(format!("{PROPOSAL}.json")), proposal),
            (directory.join("ledger/00000000.json"), ledger.clone()), (directory.join("ledger-head.json"), head.clone()),
            (directory.join("journal.json"), object([("operation_id", string(OPERATION)),
                ("proposal_sha256", get(&ledger, "proposal_sha256").clone()), ("manifest_sha256", get(&ledger, "manifest_sha256").clone())])),
            (base.join("index.json"), index.clone()),
        ] { state::write_once(&path, &value).unwrap(); }
        state::put_bytes(&directory.join("originals").join(format!("{SOURCE}.bin")), BODY).unwrap();
        let (_, bindings, _) = agent_memory::local_registry::paths("layout-recovery-bindings").unwrap();
        let state_root = PathBuf::from(std::env::var_os("AGENT_MEMORY_STATE_HOME").unwrap());
        let binding = object([("repo_root", string(repo.to_str().unwrap())), ("bank_path", string(bank.to_str().unwrap())),
            ("checkout_key", string("fixture-checkout")), ("project_id", string(PROJECT)),
            ("state_root", string(state_root.to_str().unwrap())), ("index_path", string(base.join("index.json").to_str().unwrap())),
            ("lifecycle_state", string("active"))]);
        state::write_once(&bindings, &object([("schema", string(state::BINDINGS)), ("bindings", object([(OPERATION, binding)]))])).unwrap();
        let marker = encoded(&object([("schema", string(frontier::HOLD)), ("project_id", string(PROJECT)),
            ("operation_id", string(OPERATION)), ("hold_id", string(HOLD_ID))])).unwrap();
        private_file(&bank.join(".agent-memory-transaction.md"), &marker);
        private_file(&bank.join(".agent-memory-recovery.md"), &encoded(&object([("schema", string(frontier::SENTINEL)),
            ("project_id", string(PROJECT)), ("operation_id", string(OPERATION))])).unwrap());
        Self { root: repo, bank, runtime, directory, index, row, head, ledger, marker }
    }
    fn observe(&self) -> agent_memory::error::Result<Value> {
        frontier::observe_bank(&self.root, &self.bank, None, None, false, &self.runtime)
    }
    fn original(&self) -> PathBuf { self.directory.join("originals").join(format!("{SOURCE}.bin")) }
    fn rewrite_ledger(&mut self, current: Value) {
        self.ledger = current;
        set(&mut self.head, "sha256", hash(&self.ledger));
        set(&mut self.row, "ledger_head_sha256", hash(&self.head));
        set(self.index.get_mut("operations").unwrap(), OPERATION, self.row.clone());
        private_file(&self.directory.join("ledger-head.json"), &encoded(&self.head).unwrap());
        private_file(&self.directory.join("ledger").join(format!("{:08}.json", get(&self.ledger, "generation").as_int().unwrap().as_i64().unwrap())), &encoded(&self.ledger).unwrap());
        let index_path = self.directory.parent().unwrap().join("index.json");
        private_file(&index_path, &encoded(&self.index).unwrap());
    }
}

#[test]
fn exact_markers_bound_unknown_disclosure_and_preserve_every_unsafe_occupant() {
    let world = World::new().unwrap();
    let bank = world.root.join("bank");
    assert_eq!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).unwrap(), None);
    let marker = encoded(&object([("schema", string(frontier::HOLD)), ("project_id", string(PROJECT)),
        ("operation_id", string(OPERATION)), ("hold_id", string(HOLD_ID))])).unwrap();
    let path = bank.join(".agent-memory-transaction.md");
    private_file(&path, &marker);
    assert_eq!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).unwrap().unwrap(), marker);
    assert_eq!(get(&frontier::marker_value(&marker).unwrap(), "operation_id"), &string(OPERATION));
    assert!(frontier::marker_value(&[marker.as_slice(), b" "].concat()).is_err());
    let large = vec![b'x'; 4097];
    private_file(&path, &large);
    assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).is_err());
    assert_eq!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", Some(&large)).unwrap().unwrap(), large);
    assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", Some(b"different")).is_err());
    assert!(frontier::marker_bytes(&bank, ".agent-memory-recovery.md", Some(&large)).is_err());
    let alias = bank.join("alias");
    std::fs::hard_link(&path, &alias).unwrap();
    assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", Some(&large)).is_err());
    std::fs::remove_file(&path).unwrap();
    symlink(&alias, &path).unwrap();
    assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", Some(&large)).is_err());
    assert_eq!(std::fs::read(alias).unwrap(), large);
}

#[test]
fn all_typed_marker_schemas_require_exact_fields_uuid_and_canonical_bytes() {
    for schema in [frontier::HOLD, frontier::NESTED, frontier::TRANSACTION] {
        let mut value = object([("schema", string(schema)), ("project_id", string(PROJECT)), ("operation_id", string(OPERATION))]);
        if schema == frontier::TRANSACTION { set(&mut value, "transaction_kind", string("apply")); }
        else { set(&mut value, "hold_id", string(HOLD_ID)); }
        if schema == frontier::NESTED { set(&mut value, "nested_id", string(PROPOSAL)); }
        canonical_same(&frontier::marker_value(&encoded(&value).unwrap()).unwrap(), &value);
        set(&mut value, "project_id", string("not-an-identity"));
        assert!(frontier::marker_value(&encoded(&value).unwrap()).is_err());
    }
    for bytes in [b"[]\n".as_slice(), b"{}\n", b"{\"schema\":\"unknown\"}\n", b"{\"schema\":1,\"schema\":1}\n"] {
        assert!(frontier::marker_value(bytes).is_err());
    }
}

// obligation: tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_reported_size_pending_repair_retains_exact_private_originals_and_qualified_recall
#[test]
fn live_frontier_binds_original_ledger_index_and_comparison_only_public_coverage() {
    in_world("live_frontier_binds_original_ledger_index_and_comparison_only_public_coverage", |root| {
        let fixture = Evidence::new(root);
        let before = std::fs::read(fixture.original()).unwrap();
        let token = fixture.observe().unwrap();
        assert_eq!(get(&token, "coverage_complete"), &Value::Bool(false));
        assert_eq!(get(&token, "unit_ids"), &Value::Array(vec![string(UNIT)]));
        assert_eq!(get(&token, "operation_ids"), &Value::Array(vec![string(OPERATION)]));
        assert_eq!(get(&token, "hold_sha256"), &string(digest(Some(&fixture.marker)).unwrap()));
        assert_eq!(get(&token, "index_sha256"), &hash(&fixture.index));
        assert_eq!(get(&token, "review_digest"), &Value::Null);
        let public = frontier::public_coverage(&token).unwrap();
        assert_eq!(get(&public, "semantic_coverage"), &string("incomplete-recovery"));
        assert_eq!(get(&public, "pending_recovery_units"), &Value::Int(1.into()));
        assert!(!String::from_utf8(encoded(&public).unwrap()).unwrap().contains("source_ranges"));
        let (directory, head, current) = frontier::ledger_at_bank(&fixture.root, OPERATION, &fixture.row, &fixture.bank, &fixture.runtime).unwrap();
        canonical_same(&head, &fixture.head);
        canonical_same(&current, &fixture.ledger);
        frontier::evidence(&directory, &current, &fixture.row).unwrap();
        assert_eq!(std::fs::read(fixture.original()).unwrap(), before);
        assert_eq!(std::fs::read(fixture.bank.join(".agent-memory-transaction.md")).unwrap(), fixture.marker);
    });
}

// obligation: tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_removed_sentinel_is_pending_not_successful_empty_coverage
#[test]
fn missing_or_foreign_sentinel_and_missing_hold_never_become_empty_coverage() {
    in_world("missing_or_foreign_sentinel_and_missing_hold_never_become_empty_coverage", |root| {
        let fixture = Evidence::new(root);
        let sentinel = fixture.bank.join(".agent-memory-recovery.md");
        let retained = std::fs::read(&sentinel).unwrap();
        std::fs::remove_file(&sentinel).unwrap();
        assert!(fixture.observe().unwrap_err().to_string().contains("sentinel/index pair"));
        assert_eq!(std::fs::read(fixture.original()).unwrap(), BODY);
        private_file(&sentinel, b"foreign\n");
        assert!(fixture.observe().is_err());
        private_file(&sentinel, &retained);
        std::fs::remove_file(fixture.bank.join(".agent-memory-transaction.md")).unwrap();
        assert!(fixture.observe().unwrap_err().to_string().contains("hold is absent"));
        assert_eq!(std::fs::read(fixture.original()).unwrap(), BODY);
    });
}

// obligation: tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_corrupt_private_index_blocks_public_comparison_without_canonical_write
#[test]
fn corrupt_index_and_bound_index_removal_refuse_with_original_evidence_intact() {
    in_world("corrupt_index_and_bound_index_removal_refuse_with_original_evidence_intact", |root| {
        let fixture = Evidence::new(root);
        let path = fixture.directory.parent().unwrap().join("index.json");
        private_file(&path, b"{\"operations\":{},\"operations\":{}}\n");
        assert!(fixture.observe().is_err());
        std::fs::remove_file(path).unwrap();
        assert!(fixture.observe().unwrap_err().to_string().contains("Bound recovery index is missing"));
        assert_eq!(std::fs::read(fixture.original()).unwrap(), BODY);
        assert_eq!(std::fs::read(fixture.bank.join(".agent-memory-transaction.md")).unwrap(), fixture.marker);
    });
}

#[test]
fn immutable_predecessor_chain_and_complete_terminal_receipts_are_mandatory() {
    in_world("immutable_predecessor_chain_and_complete_terminal_receipts_are_mandatory", |root| {
        let mut fixture = Evidence::new(root);
        let original_ledger = fixture.ledger.clone();
        let mut next = original_ledger.clone();
        set(&mut next, "generation", Value::Int(1.into()));
        set(&mut next, "previous_generation_sha256", hash(&original_ledger));
        set(&mut fixture.head, "generation", Value::Int(1.into()));
        fixture.rewrite_ledger(next);
        fixture.observe().unwrap();
        let old_path = fixture.directory.join("ledger/00000000.json");
        private_file(&old_path, b"{}\n");
        assert!(fixture.observe().unwrap_err().to_string().contains("immutable predecessor chain"));
        private_file(&old_path, &encoded(&original_ledger).unwrap());
        let mut invalid = fixture.ledger.clone();
        let mut terminal_unit = unit();
        set(&mut terminal_unit, "state", string("admitted"));
        set(&mut invalid, "units", Value::Array(vec![terminal_unit]));
        fixture.rewrite_ledger(invalid);
        assert!(fixture.observe().unwrap_err().to_string().contains("no actual durable resolution evidence"));
        let receipt = object([("operation_id", string(OPERATION)), ("unit_id", string(UNIT)), ("terminal_state", string("pending-admission"))]);
        state::write_once(&fixture.directory.join("resolutions").join(format!("{PROPOSAL}.json")), &receipt).unwrap();
        let mut terminal_unit = unit();
        set(&mut terminal_unit, "state", string("admitted"));
        set(&mut terminal_unit, "terminal_receipts", Value::Array(vec![object([("resolution_id", string(PROPOSAL)), ("sha256", hash(&receipt))])]));
        let mut invalid = fixture.ledger.clone();
        set(&mut invalid, "units", Value::Array(vec![terminal_unit]));
        fixture.rewrite_ledger(invalid);
        assert!(fixture.observe().unwrap_err().to_string().contains("cannot falsely declare"));
        assert_eq!(std::fs::read(fixture.original()).unwrap(), BODY);
    });
}

#[test]
fn original_locator_is_derived_and_every_source_byte_count_hash_and_review_are_bound() {
    in_world("original_locator_is_derived_and_every_source_byte_count_hash_and_review_are_bound", |root| {
        let fixture = Evidence::new(root);
        frontier::evidence(&fixture.directory, &fixture.ledger, &fixture.row).unwrap();
        let original = fixture.original();
        private_file(&original, b"changed original\n");
        assert!(frontier::evidence(&fixture.directory, &fixture.ledger, &fixture.row).unwrap_err().to_string().contains("original is missing, damaged or changed"));
        private_file(&original, BODY);
        let proposal_path = fixture.directory.join("proposals").join(format!("{PROPOSAL}.json"));
        let before = std::fs::read(&proposal_path).unwrap();
        private_file(&proposal_path, b"{}\n");
        assert!(frontier::evidence(&fixture.directory, &fixture.ledger, &fixture.row).is_err());
        assert_eq!(std::fs::read(original).unwrap(), BODY);
        private_file(&proposal_path, &before);
        fixture.observe().unwrap();
    });
}

// obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_partial_claims_need_two_receipts_and_independent_complete_union
#[test]
fn coverage_claims_bind_whole_framing_nonoverlapping_fragments_and_exact_record_references() {
    let row = unit();
    let reference = object([("id", string(PROPOSAL)), ("version", Value::Int(1.into())),
        ("path", string("topics/result.md")), ("resulting_sha256", string("a".repeat(64)))]);
    let midpoint = BODY.len() / 2;
    let full = object([("unit_id", string(UNIT)), ("coverage", string("full")),
        ("covered_source_ranges", Value::Array(vec![range(midpoint, BODY.len()), range(0, midpoint)])),
        ("target_record_refs", Value::Array(vec![reference.clone()]))]);
    frontier::claim_value(&full, &row).unwrap();
    for ranges in [vec![range(1, BODY.len())], vec![range(0, midpoint + 1), range(midpoint, BODY.len())], vec![range(0, BODY.len() + 1)]] {
        let mut bad = full.clone();
        set(&mut bad, "covered_source_ranges", Value::Array(ranges));
        assert!(frontier::claim_value(&bad, &row).is_err());
    }
    let mut partial = full.clone();
    set(&mut partial, "coverage", string("partial"));
    set(&mut partial, "covered_source_ranges", Value::Array(vec![range(0, midpoint)]));
    frontier::claim_value(&partial, &row).unwrap();
    set(&mut partial, "target_record_refs", Value::Array(vec![reference.clone(), reference]));
    assert!(frontier::claim_value(&partial, &row).is_err());
    let none = object([("unit_id", string(UNIT)), ("coverage", string("none")),
        ("covered_source_ranges", Value::Array(vec![])), ("target_record_refs", Value::Array(vec![]))]);
    frontier::claim_value(&none, &row).unwrap();
    for value in [object([("source_id", string(SOURCE)), ("start", Value::Bool(false)), ("end", Value::Int(1.into()))]),
        range(0, 1_048_577), range(1, 1)] { assert!(frontier::range_value(&value).is_err()); }
}

#[test]
fn proof_digest_excludes_only_review_attestation_and_successor_is_capacity_only() {
    let token = object([("unit_ids", Value::Array(vec![string(UNIT)])), ("coverage_complete", Value::Bool(false)),
        ("ledger_heads", Value::Array(vec![object([("operation_id", string(OPERATION)), ("generation", Value::Int(8.into())), ("sha256", string("a".repeat(64)))])])),
        ("review_digest", string("b".repeat(64))), ("hold_sha256", string("c".repeat(64))), ("sentinel_sha256", string("d".repeat(64)))]);
    let proof = object([("judgment", object([("observation", object([("recovery_frontier", token.clone())]))]))]);
    let input = frontier::proof_input(&proof).unwrap();
    let mut changed = proof.clone();
    set(changed.get_mut("judgment").unwrap().get_mut("observation").unwrap().get_mut("recovery_frontier").unwrap(), "review_digest", string("e".repeat(64)));
    assert_eq!(frontier::proof_input(&changed).unwrap(), input);
    set(changed.get_mut("judgment").unwrap().get_mut("observation").unwrap().get_mut("recovery_frontier").unwrap(), "hold_sha256", Value::Null);
    assert_ne!(frontier::proof_input(&changed).unwrap(), input);
    let successor = frontier::successor_token(&token, &Value::Array(vec![]), OPERATION, true).unwrap();
    assert_eq!(get(&successor, "coverage_complete"), &Value::Bool(true));
    assert_eq!(get(&successor, "hold_sha256"), &Value::Null);
    assert_eq!(get(&successor, "sentinel_sha256"), &Value::Null);
    assert_eq!(get(&successor, "review_digest"), &Value::Null);
    assert_eq!(get(&get(&successor, "ledger_heads").as_array().unwrap()[0], "generation"), &Value::Int(9.into()));
    assert_eq!(get(&token, "review_digest"), &string("b".repeat(64)));
}

#[test]
fn resolution_without_actual_provider_is_typed_pending_not_a_successful_ledger_transition() {
    let world = World::new().unwrap();
    let error = frontier::advance_ledger(&world.root.join("repo"), OPERATION, &Value::Array(vec![]),
        &Value::Null, b"owned", None, None, &world.root.join("data")).unwrap_err();
    assert_eq!(error.kind(), &Kind::MissingProvider);
    assert!(error.to_string().contains("layout_resolution"));
    assert!(!world.root.join("repo/index.json").exists());
}

#[test]
fn checked_empty_lifecycle_frontier_has_explicit_null_identity_and_no_semantic_absence_claim() {
    in_world("checked_empty_lifecycle_frontier_has_explicit_null_identity_and_no_semantic_absence_claim", |root| {
        let repo = root.join("repo");
        let bank = repo.join("memory-bank");
        let runtime = root.join("data/runtime");
        for path in [&bank, &runtime] { std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path).unwrap(); }
        let token = frontier::observe_bank(&repo, &bank, None, None, false, &runtime).unwrap();
        assert_eq!(get(&token, "project_id"), &Value::Null);
        assert_eq!(get(&token, "unit_ids"), &Value::Array(vec![]));
        assert_eq!(get(&token, "index_sha256"), &Value::Null);
        assert_eq!(get(&token, "frontier_digest"), &string(digest(Some(b"[]\n")).unwrap()));
        assert_eq!(get(&token, "reference_map_sha256"), &string(digest(Some(b"{}\n")).unwrap()));
        assert!(!bank.join("MEMORY.md").exists());
        assert!(frontier::observe_bank(&repo, &bank, None, None, true, &runtime).is_err());
        let ordinary = vec![b'x'; 4097];
        private_file(&bank.join(".agent-memory-transaction.md"), &ordinary);
        assert!(frontier::observe_bank(&repo, &bank, None, None, false, &runtime).is_err());
    });
}

fn canonical_files() -> Value {
    let omitted = Value::Object(["projectbrief.md", "productContext.md", "systemPatterns.md", "techContext.md", "progress.md"]
        .into_iter().map(|path| (path.to_owned(), string("No distinct fixture source"))).collect());
    let rendered = bank_contract::render(&object([("project-id", string(PROJECT)),
        ("router-updated", string("2026-10-22T18:00:00Z")),
        ("contexts", object([("activeContext.md", object([("body", string("# Current focus\n\nBounded recovery contract.\n")),
            ("purpose", string("Current focus")), ("scope", string("repository")), ("lifecycle", string("active")),
            ("read-when", string("When resuming"))]))])), ("omitted", omitted)])).unwrap();
    get(&rendered, "files").clone()
}
fn capacity_token() -> Value {
    object([("bank_path", string("/private/fixture/memory-bank")), ("project_id", string(PROJECT)),
        ("operation_ids", Value::Array(vec![string(OPERATION)])),
        ("ledger_heads", Value::Array(vec![object([("operation_id", string(OPERATION)), ("generation", Value::Int(0.into())), ("sha256", string("f".repeat(64)))])])),
        ("frontier_digest", string("f".repeat(64))), ("coverage_complete", Value::Bool(false)),
        ("unit_ids", Value::Array(vec![string(UNIT)])), ("reference_map_sha256", string("f".repeat(64))),
        ("review_digest", Value::Null), ("index_sha256", string("f".repeat(64))),
        ("hold_sha256", string("f".repeat(64))), ("sentinel_sha256", string("f".repeat(64)))])
}

#[test]
fn empty_reservation_is_bounded_and_oversized_pending_text_refuses_before_disclosure() {
    let files = canonical_files();
    let reservation = frontier::reservation(&files, &Value::Array(vec![]), &capacity_token()).unwrap();
    assert_eq!(get(&reservation, "rows"), &Value::Int(0.into()));
    assert_eq!(get(&reservation, "disclosure_bytes"), &Value::Int(0.into()));
    assert!(get(&reservation, "proof_bytes").as_int().unwrap().as_u64().unwrap() < 65536);
    let oversized = Value::Array(vec![object([("body", string("x".repeat(16385)))])]);
    assert!(frontier::reservation(&files, &oversized, &capacity_token()).unwrap_err().to_string().contains("indivisible"));
}

// obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_reservation_transition_accepts_last_complete_frontier_and_refuses_next
#[test]
#[ignore = "needs security registered for actual repository_config::SecurityScan; no comparison or layout provider is substituted"]
fn reservation_accepts_last_complete_extremal_proof_then_refuses_adjacent_partition() {
    agent_memory::cli::register_providers().unwrap();
    let files = canonical_files();
    let token = capacity_token();
    let mut pending = Vec::new();
    let mut last = None;
    for count in 1..=64 {
        pending.push(object([("unit_id", string(agent_memory::host::uuid4().unwrap())),
            ("operation_id", string(OPERATION)), ("body", string("# Original\n\nUnresolved source conditions.\n")),
            ("source_ranges", Value::Array(vec![range(0, 20)])), ("provenance", Value::Object(vec![])),
            ("reason", string("Retained independent review needed")),
            ("selection_class", string("comparison-only-not-current-guidance")), ("source_assertions_verified", Value::Bool(false))]));
        match frontier::reservation(&files, &Value::Array(pending.clone()), &token) {
            Ok(value) => {
                assert_eq!(get(&value, "rows"), &Value::Int(count.into()));
                assert!(get(&value, "proof_bytes").as_int().unwrap().as_u64().unwrap() <= 65536);
                last = Some(count);
            }
            Err(error) => {
                assert!(error.to_string().contains("cannot guarantee a complete bounded comparison"));
                assert_eq!(last, Some(count - 1));
                return;
            }
        }
    }
    panic!("Extremal proof bound must reject an adjacent partition");
}

#[test]
fn source_marker_guard_restores_nested_and_unwound_observation_without_minting_authority() {
    let world = World::new().unwrap();
    let bank = world.root.join("bank");
    let marker = bank.join(".agent-memory-transaction.md");
    private_file(&marker, b"unknown marker remains source evidence\n");
    agent_memory::local_registry::exclusive(&bank.join(".agent-memory.lock"), OPERATION, "replace",
        agent_memory::host::Sources::default(), |_| {
            assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).is_ok());
            let outer = frontier::observe_source_markers();
            let inner = frontier::observe_source_markers();
            assert!(matches!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).unwrap_err().kind(),
                Kind::CanonicalWriterBusy));
            drop(inner);
            assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).is_err());
            drop(outer);
            assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).is_ok());
            let unwind = std::panic::catch_unwind(|| {
                let _guard = frontier::observe_source_markers();
                panic!("source observation unwind");
            });
            assert!(unwind.is_err());
            assert!(frontier::marker_bytes(&bank, ".agent-memory-transaction.md", None).is_ok());
            Ok(())
        }).unwrap();
    assert_eq!(std::fs::read(marker).unwrap(), b"unknown marker remains source evidence\n");
    assert!(!bank.join(".agent-memory.lock").exists());
}

#[test]
fn duplicate_units_and_unretained_revision_children_cannot_discard_source_obligations() {
    in_world("duplicate_units_and_unretained_revision_children_cannot_discard_source_obligations", |root| {
        let mut fixture = Evidence::new(root);
        let original = fixture.ledger.clone();
        let mut invalid = original.clone();
        set(&mut invalid, "units", Value::Array(vec![unit(), unit()]));
        fixture.rewrite_ledger(invalid);
        assert!(fixture.observe().unwrap_err().to_string().contains("missing, duplicated"));
        let mut revised = unit();
        set(&mut revised, "state", string("superseded-by-revision"));
        set(&mut revised, "child_unit_ids", Value::Array(vec![string(PROPOSAL)]));
        let mut invalid = original.clone();
        set(&mut invalid, "units", Value::Array(vec![revised]));
        fixture.rewrite_ledger(invalid);
        assert!(fixture.observe().unwrap_err().to_string().contains("omitted a retained child"));
        fixture.rewrite_ledger(original);
        fixture.observe().unwrap();
        assert_eq!(std::fs::read(fixture.original()).unwrap(), BODY);
    });
}

// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_owned_marker_keeps_frontier_but_unknown_or_changed_bytes_refuse
#[test]
fn large_exact_ordinary_transaction_witness_preserves_frontier_without_recovery_authority() {
    in_world("large_exact_ordinary_transaction_witness_preserves_frontier_without_recovery_authority", |root| {
        let repo = root.join("repo");
        let bank = repo.join("memory-bank");
        let runtime = root.join("data/runtime");
        for path in [&bank, &runtime] { std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path).unwrap(); }
        let before = frontier::observe_bank(&repo, &bank, None, None, false, &runtime).unwrap();
        let marker = encoded(&object([("schema", string("agent-memory/transaction-v1")),
            ("operation_id", string(OPERATION)), ("observed_hashes", object([("retained-long-path", string("a".repeat(8192)))]))])).unwrap();
        private_file(&bank.join(".agent-memory-transaction.md"), &marker);
        canonical_same(&frontier::observe_bank(&repo, &bank, None, Some(&marker), false, &runtime).unwrap(), &before);
        assert!(frontier::observe_bank(&repo, &bank, None, None, false, &runtime).is_err());
        assert!(frontier::observe_bank(&repo, &bank, None, Some(&marker[..marker.len() - 1]), false, &runtime).is_err());
        let mut different = marker.clone();
        *different.last_mut().unwrap() = b' ';
        assert!(frontier::observe_bank(&repo, &bank, None, Some(&different), false, &runtime).is_err());
        private_file(&bank.join(".agent-memory-recovery.md"), &marker);
        assert!(frontier::observe_bank(&repo, &bank, None, Some(&marker), false, &runtime).is_err());
        assert_eq!(std::fs::read(bank.join(".agent-memory-transaction.md")).unwrap(), marker);
    });
}

#[test]
fn no_receipt_runtime_location_refuses_with_bootstrap_guidance_and_preserves_sources() {
    in_world("no_receipt_runtime_location_refuses_with_bootstrap_guidance_and_preserves_sources", |root| {
        agent_memory::cli::register_providers().unwrap();
        let source = root.join("repo/retained-original.md");
        private_file(&source, BODY);
        let receipt = root.join("state/agent-memory/install/receipt.json");
        assert!(!receipt.exists());
        let refusal = agent_memory::cli::pack_root().unwrap_err();
        assert!(refusal.to_string().contains("bootstrap"), "{refusal}");
        assert!(!refusal.to_string().contains("pass --pack"), "{refusal}");
        assert_eq!(std::fs::read(source).unwrap(), BODY);
        assert!(!receipt.exists());
        assert!(!root.join("repo/.agent-memory-transaction.md").exists());
    });
}
