use agent_memory::bank_observation;
use agent_memory::codec::json::{self, Value};
use agent_memory::error::Kind;
use agent_memory::recall_catalog as catalog;
use agent_memory::{artifact, bank_contract, bank_transactions, host, migration_contract};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
mod support;

const ROOT: &str = "/repo";
const BANK: &str = "/repo/memory-bank";
const NAME: &str = "g-0123456789abcdef0123456789abcdef.catalog";
// Independent vector: OpenSSL HMAC-SHA256 with key 000102...1f over the
// sorted compact unsigned header + LF + {"hashes":{}}. No implementation
// output is used as the authentication oracle.
const RECORD: &str = concat!(
    "{\"binding\":{\"bank_root\":\"/repo/memory-bank\",\"project_id\":\"project\",\"repo_root\":\"/repo\",\"repository_settings_sha256\":\"settings\"},",
    "\"engine\":{\"artifact_digest\":\"rust-artifact\"},\"mac\":\"4ce6a25bf39c56913f2c33e1f6cf9894f5d53cb89f896565ae7b7e3f229f686d\",",
    "\"payload_bytes\":13,\"payload_sha256\":\"2771d13c637c267132afff9db67537bef95708534b79ae8d954254c4e64e4e0f\",\"schema\":\"agent-memory/recall-catalog-v1\"}\n{\"hashes\":{}}"
);
fn secret() -> Vec<u8> {
    (0u8..32).collect()
}
fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn obj(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
struct World(PathBuf);
impl World {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("am-catalog-{}", host::uuid4().unwrap()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn write(&self, name: &str, raw: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, raw).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }
    fn owned(&self) {
        self.write("key", secret());
        self.write(NAME, RECORD);
        self.write(
            "current.json",
            format!("{{\"file\":\"{NAME}\",\"schema\":\"{}\"}}", catalog::SCHEMA),
        );
    }
    fn runtime(&self) -> catalog::Runtime {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            artifact::register_migration_contract(Some(Arc::new(migration_contract::Bundled)))
                .unwrap()
        });
        self.write("pack/SKILL.md", "---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {\"version\":\"0.0.14\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n");
        self.write(
            "pack/bank/GUIDE.md",
            "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n",
        );
        let mut operation = String::from(
            "---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n",
        );
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
        self.write("pack/inspect/GUIDE.md", operation);
        self.write("pack/schemas/runtime-compatibility.json", r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"python_minimum":"3.11","git_required_for":["git-sharing"],"required_python_packages":[],"required_agent_clients":[],"optional_adapters":[]}"#);
        self.write(
            "pack/schemas/bank-router.json",
            r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#,
        );
        let binary = self.write("am", b"opaque native runtime fixture");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        catalog::Runtime {
            pack: self.0.join("pack"),
            binary,
        }
    }
    fn observed(&self) -> bank_observation::Observation {
        let proposal = obj([
            ("project-id", s("8e0b66aa-2be5-4fa3-b47f-1245474aa7dd")),
            ("router-updated", s("2026-10-22T17:00:00Z")),
            (
                "contexts",
                obj([(
                    "projectbrief.md",
                    obj([
                        ("body", s("# Project\n\nCANONICAL_BODY_NOT_IN_CATALOG\n")),
                        ("purpose", s("Orient the agent")),
                        ("scope", s("repository")),
                        ("lifecycle", s("active")),
                        ("read-when", s("When resuming work")),
                    ]),
                )]),
            ),
            (
                "omitted",
                obj(bank_contract::CONTEXTS
                    .iter()
                    .filter(|p| **p != "projectbrief.md")
                    .map(|p| (*p, s("No relevant evidence supplied yet")))),
            ),
        ]);
        let rendered = bank_contract::render(&proposal).unwrap();
        for (path, raw) in rendered.get("files").unwrap().as_object().unwrap() {
            self.write(&format!("repo/memory-bank/{path}"), raw.as_str().unwrap());
        }
        bank_observation::observe(&self.0.join("repo"), None, None).unwrap()
    }
    fn environment(&self) -> agent_memory::bank_writes::StateEnvironment {
        agent_memory::bank_writes::StateEnvironment {
            state_home: Some(self.0.join("state").into_os_string()),
            xdg_state_home: None,
            home: None,
        }
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn authentication_matches_independent_hmac_and_binds_every_header_field() {
    let (header, payload) = catalog::authenticate(
        RECORD.as_bytes(),
        Path::new(ROOT),
        Path::new(BANK),
        "project",
        &secret(),
        false,
    )
    .unwrap();
    assert_eq!(payload, b"{\"hashes\":{}}");
    assert_eq!(
        header.get("engine"),
        Some(&obj([("artifact_digest", s("rust-artifact"))]))
    );
    for changed in [
        RECORD.replace("rust-artifact", "changed-artifact"),
        RECORD.replace("settings", "changed-settings"),
        RECORD.replace("project", "foreign"),
        RECORD.replace("hashes", "forged"),
        RECORD.replace("4ce6a25b", "0ce6a25b"),
    ] {
        assert!(
            catalog::authenticate(
                changed.as_bytes(),
                Path::new(ROOT),
                Path::new(BANK),
                "project",
                &secret(),
                false
            )
            .is_err()
        );
    }
    assert!(
        catalog::authenticate(
            RECORD.as_bytes(),
            Path::new(ROOT),
            Path::new(BANK),
            "project",
            &[0; 32],
            false
        )
        .is_err()
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_caller_modified_metadata_cannot_forge_a_successful_empty_selection
#[test]
fn changed_metadata_and_recomputed_digest_still_need_the_private_mac() {
    use sha2::{Digest, Sha256};
    let (first, _) = RECORD.split_once('\n').unwrap();
    let mut header = json::decode(first, Default::default()).unwrap();
    let payload = b"{\"hashes\":{},\"checked\":{\"entries\":[]}}";
    *header.get_mut("payload_bytes").unwrap() = Value::Int(payload.len().into());
    let digest: String = Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    *header.get_mut("payload_sha256").unwrap() = s(digest);
    let mut changed = catalog::compact(&header).unwrap();
    changed.push(b'\n');
    changed.extend_from_slice(payload);
    let error = catalog::authenticate(
        &changed,
        Path::new(ROOT),
        Path::new(BANK),
        "project",
        &secret(),
        false,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "catalog authentication");
    // A byte-identical authenticated payload is the only accepted vector.
    assert!(
        catalog::authenticate(
            RECORD.as_bytes(),
            Path::new(ROOT),
            Path::new(BANK),
            "project",
            &secret(),
            false
        )
        .is_ok()
    );
}

#[test]
fn private_records_refuse_links_aliases_modes_fifo_and_bounds_without_repair() {
    let world = World::new();
    let path = world.write("private", b"abc");
    assert_eq!(catalog::read_private(&path, 3).unwrap().unwrap(), b"abc");
    assert!(catalog::read_private(&path, 2).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(catalog::read_private(&path, 3).is_err());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let alias = world.0.join("alias");
    std::fs::hard_link(&path, &alias).unwrap();
    assert!(catalog::read_private(&path, 3).is_err());
    std::fs::remove_file(&alias).unwrap();
    symlink(&path, &alias).unwrap();
    assert!(catalog::read_private(&alias, 3).is_err());
    assert!(
        catalog::read_private(&world.0.join("missing"), 3)
            .unwrap()
            .is_none()
    );
    use agent_memory::process::{Input, Spec, Status, Target};
    let argv = [
        std::ffi::OsString::from("/usr/bin/mkfifo"),
        world.0.join("fifo").into_os_string(),
    ];
    let output = agent_memory::process::run(&Spec {
        argv: &argv,
        search_path: false,
        env: &[],
        cwd: Some(&world.0),
        stdin: Input::Null,
        stdout: Target::Capture { limit: 4096 },
        stderr: Target::Capture { limit: 4096 },
        deadline: agent_memory::clock::Deadline::from_seconds(5.0).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    })
    .unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert!(catalog::read_private(&world.0.join("fifo"), 3).is_err());
}

#[test]
fn corrupt_key_recovery_preserves_small_and_oversized_evidence() {
    let world = World::new();
    world.write("key", b"corrupt original key");
    assert!(catalog::key(&world.0, false).is_err());
    let fresh = catalog::key(&world.0, true).unwrap();
    assert_eq!(fresh.len(), 32);
    assert_eq!(catalog::key(&world.0, true).unwrap(), fresh);
    let recovered = catalog::namespace(&world.0)
        .unwrap()
        .into_iter()
        .find(|p| p.to_str().unwrap().starts_with("key-recovery-"))
        .unwrap();
    assert_eq!(
        std::fs::read(world.0.join(recovered)).unwrap(),
        b"corrupt original key"
    );
    let large = vec![b'x'; catalog::MAX_KEY + 1];
    world.write("key", &large);
    assert_eq!(catalog::key(&world.0, true).unwrap().len(), 32);
    assert!(
        catalog::namespace(&world.0)
            .unwrap()
            .into_iter()
            .any(|name| std::fs::read(world.0.join(name)).is_ok_and(|raw| raw == large))
    );
    let unsafe_world = World::new();
    let path = unsafe_world.write("key", b"unsafe");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(catalog::key(&unsafe_world.0, true).is_err());
    assert_eq!(catalog::namespace(&unsafe_world.0).unwrap().len(), 1);
}

#[test]
fn pointer_accepts_only_bounded_exact_owned_generation_names() {
    let world = World::new();
    world.owned();
    assert_eq!(
        catalog::pointer(&world.0).unwrap().0.get("file"),
        Some(&s(NAME))
    );
    for name in [
        "../foreign.catalog",
        "g-0123456789abcdef0123456789abcdeF.catalog",
        "g-123.catalog",
        "/tmp/foreign",
    ] {
        world.write(
            "current.json",
            format!("{{\"schema\":\"{}\",\"file\":\"{name}\"}}", catalog::SCHEMA),
        );
        assert!(catalog::pointer(&world.0).is_err());
    }
    world.write("current.json", vec![b' '; catalog::MAX_POINTER + 1]);
    assert!(catalog::pointer(&world.0).is_err());
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_actual_forget_removes_owned_generations_before_any_later_recall
#[test]
fn changed_hashes_delete_only_authenticated_owned_generations() {
    let world = World::new();
    world.owned();
    let corrupt = "g-11111111111111111111111111111111.catalog";
    world.write(corrupt, b"retained corrupt recovery evidence");
    world.write("unknown", b"retained unknown evidence");
    let state = catalog::invalidate_after_change_at(
        Path::new(ROOT),
        Path::new(BANK),
        &world.0,
        &obj([("new.md", s("changed"))]),
    )
    .unwrap();
    assert_eq!(state, "invalidated");
    assert!(!world.0.join("current.json").exists());
    assert!(!world.0.join(NAME).exists());
    assert_eq!(
        std::fs::read(world.0.join(corrupt)).unwrap(),
        b"retained corrupt recovery evidence"
    );
    assert_eq!(
        std::fs::read(world.0.join("unknown")).unwrap(),
        b"retained unknown evidence"
    );
}

#[test]
fn unchanged_hashes_and_foreign_bindings_never_delete_evidence() {
    let world = World::new();
    world.owned();
    assert_eq!(
        catalog::invalidate_after_change_at(
            Path::new(ROOT),
            Path::new(BANK),
            &world.0,
            &obj([] as [(&str, Value); 0])
        )
        .unwrap(),
        "current"
    );
    assert!(world.0.join(NAME).exists());
    catalog::collect_owned(
        &world.0,
        Path::new("/foreign"),
        Path::new(BANK),
        "project",
        &secret(),
        None,
    )
    .unwrap();
    assert!(world.0.join(NAME).exists());
    let absent = world.0.join("absent");
    assert_eq!(
        catalog::invalidate_after_change_at(
            Path::new(ROOT),
            Path::new(BANK),
            &absent,
            &Value::Null
        )
        .unwrap(),
        "not-present"
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_corrupt_oversized_cleanup_header_defers_without_decoding_it_or_failing_actual_forget
#[test]
fn oversized_cleanup_header_is_bounded_and_transaction_hook_defers() {
    let world = World::new();
    world.owned();
    let raw = format!(
        "{{\"padding\":\"{}\"}}\n{{}}",
        "x".repeat(catalog::MAX_HEADER + 1)
    );
    world.write(NAME, &raw);
    let error = catalog::invalidate_after_change_at(
        Path::new(ROOT),
        Path::new(BANK),
        &world.0,
        &Value::Null,
    )
    .unwrap_err();
    assert_eq!(error.kind(), &Kind::RecallCatalogMiss);
    assert_eq!(error.to_string(), "catalog header bound");
    assert_eq!(std::fs::read(world.0.join(NAME)).unwrap(), raw.as_bytes());
    use bank_transactions::IndexInvalidation;
    assert_eq!(
        catalog::IndexInvalidation
            .invalidate_after_change(&world.0, &Value::Null)
            .unwrap(),
        s("deferred-to-canonical-fallback")
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_full_namespace_preserves_unknown_evidence_and_stops_adding_cache_files
#[test]
fn full_unknown_namespace_refuses_new_lock_without_adding_or_repairing_files() {
    let world = World::new();
    for n in 0..catalog::MAX_NAMESPACE {
        world.write(&format!("unknown-{n}"), b"preserve unknown evidence");
    }
    assert!(catalog::publication_lock(&world.0).is_err());
    assert_eq!(
        catalog::namespace(&world.0).unwrap().len(),
        catalog::MAX_NAMESPACE
    );
    for name in catalog::namespace(&world.0).unwrap() {
        assert_eq!(
            std::fs::read(world.0.join(name)).unwrap(),
            b"preserve unknown evidence"
        );
    }
    world.write("one-over", b"retain");
    assert!(catalog::namespace(&world.0).is_err());
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_hint_inventory_charges_excluded_runtime_entries_against_its_physical_budget
#[test]
fn excluded_runtime_entries_spend_the_same_finite_physical_budget() {
    let world = World::new();
    world.write("MEMORY.md", b"not parsed by the hint walker");
    for n in 0..4 {
        world.write(&format!(".agent-memory-derived/{n}"), b"ignored");
    }
    assert!(catalog::physical_inventory_with_limit(&world.0, 2).is_err());
    let (physical, stamps) = catalog::physical_inventory_with_limit(&world.0, 3).unwrap();
    assert_eq!(physical.len(), 1);
    assert_eq!(physical.get("MEMORY.md"), Some(&"file".to_owned()));
    assert_eq!(stamps.as_object().unwrap().len(), 1);
    let stamp = stamps.get("MEMORY.md").unwrap().as_array().unwrap();
    assert_eq!(stamp.len(), 7);
    assert_eq!(stamp.get(2).unwrap().as_int().unwrap().as_u64(), Some(29));
    symlink(
        world.0.join("MEMORY.md"),
        world.0.join(".agent-memory-derived/link"),
    )
    .unwrap();
    assert!(catalog::physical_inventory(&world.0).is_err());
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_busy_optional_publisher_leaves_useful_recall_available_and_rebuilds_after_unlock
#[test]
fn busy_publication_flock_is_nonblocking_and_releases_without_pid_markers() {
    let world = World::new();
    let held = catalog::publication_lock(&world.0).unwrap();
    assert!(catalog::publication_lock(&world.0).is_err());
    assert_eq!(
        catalog::namespace(&world.0).unwrap(),
        vec![std::ffi::OsString::from("publication.lock")]
    );
    drop(held);
    assert!(catalog::publication_lock(&world.0).is_ok());
    assert_eq!(
        std::fs::read(world.0.join("publication.lock")).unwrap(),
        b""
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_changed_executing_package_rebuilds_instead_of_reusing_an_old_engine_witness
// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_actual_loaded_policy_code_namespace_and_dependency_changes_invalidate_the_witness
// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_warm_recall_refuses_changed_identity_tokens_defaults_and_callable_attributes
#[test]
fn rust_generation_is_exact_runtime_artifact_and_changed_binary_invalidates_it() {
    let world = World::new();
    let runtime = world.runtime();
    let engine = catalog::generation(&runtime).unwrap();
    let inspected = artifact::inspect_runtime(&runtime.pack, &runtime.binary).unwrap();
    assert_eq!(
        engine.value(),
        &obj([(
            "artifact_digest",
            inspected.get("artifact_digest").unwrap().clone()
        )])
    );
    catalog::coherent(&engine).unwrap();
    // A Python-era generation cannot compare equal even if its digest happens
    // to equal the runtime digest: its identity shape is intentionally distinct.
    let python = obj([
        ("sha256", inspected.get("artifact_digest").unwrap().clone()),
        ("modules", Value::Array(vec![])),
    ]);
    assert_ne!(
        catalog::compact(engine.value()).unwrap(),
        catalog::compact(&python).unwrap()
    );
    std::fs::write(&runtime.binary, b"changed native runtime fixture").unwrap();
    assert!(catalog::coherent(&engine).is_err());
    assert_ne!(
        catalog::generation(&runtime).unwrap().value(),
        engine.value()
    );
    std::fs::write(&runtime.binary, b"opaque native runtime fixture").unwrap();
    catalog::coherent(&engine).unwrap();
    world.write("pack/new-policy.json", b"{}");
    assert!(catalog::coherent(&engine).is_err());
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_unavailable_bounded_boot_manifest_disables_only_the_optional_accelerator
#[test]
fn unsafe_or_oversized_runtime_inventory_disables_only_optional_generation() {
    let world = World::new();
    let runtime = world.runtime();
    catalog::generation(&runtime).unwrap();
    let extra = world.write("pack/extra.json", b"{}");
    std::fs::remove_file(&extra).unwrap();
    symlink(world.0.join("outside"), &extra).unwrap();
    assert!(catalog::generation(&runtime).is_err());
    std::fs::remove_file(&extra).unwrap();
    std::fs::File::create(&extra)
        .unwrap()
        .set_len(2 * 1024 * 1024 + 1)
        .unwrap();
    assert!(catalog::generation(&runtime).is_err());
    assert_eq!(
        std::fs::metadata(&extra).unwrap().len(),
        2 * 1024 * 1024 + 1
    );
    assert_eq!(
        std::fs::read(&runtime.binary).unwrap(),
        b"opaque native runtime fixture"
    );
    let active_memo = agent_memory::validation_memo::operation(|_| catalog::generation(&runtime));
    assert!(active_memo.is_err());
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_oversized_canonical_source_refuses_with_or_without_catalog
#[test]
fn canonical_capacity_is_enforced_independently_of_optional_cache_metadata() {
    let world = World::new();
    world.write("MEMORY.md", b"router is not parsed by source scan");
    world.write("large.md", vec![b'x'; catalog::MAX_SOURCE + 1]);
    let error = bank_observation::scan(&world.0, false, true).unwrap_err();
    assert!(matches!(error.kind(), Kind::BankCapacity { .. }));
    assert_eq!(
        agent_memory::bank_capacity::details(&error).get("failure_code"),
        Some(&s("bank-source-byte-capacity"))
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_warm_semantic_selection_matches_cold_sources_without_storing_learning_bodies
#[test]
#[ignore = "needs retrieval and discovery plus isolated real frontier callbacks; no recall query provider is substituted"]
fn warm_catalog_preserves_canonical_body_without_storing_it() {
    let world = World::new();
    let runtime = world.runtime();
    let mut observed = world.observed();
    let engine = catalog::generation(&runtime).unwrap();
    let environment = world.environment();
    assert!(catalog::publish_with_environment(&mut observed, &engine, &environment).unwrap());
    let project = observed
        .checked
        .get("project_id")
        .unwrap()
        .as_str()
        .unwrap();
    let mut loaded = catalog::load_catalog_with_environment(
        &observed.root,
        project,
        &runtime,
        None,
        &environment,
    )
    .unwrap();
    assert_eq!(
        loaded
            .source("projectbrief.md", catalog::MAX_SOURCE)
            .unwrap(),
        observed.sources.get("projectbrief.md").unwrap().as_slice()
    );
    loaded.recheck(None).unwrap();
    let (_, _, directory) =
        catalog::locations_with_environment(&observed.root, &runtime.pack, &environment).unwrap();
    let name = catalog::pointer(&directory)
        .unwrap()
        .0
        .get("file")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        !String::from_utf8_lossy(&std::fs::read(directory.join(name)).unwrap())
            .contains("CANONICAL_BODY_NOT_IN_CATALOG")
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_missing_cache_automatically_rebuilds_and_returns_the_same_useful_memory
#[test]
#[ignore = "needs retrieval and discovery plus isolated real frontier callbacks; no recall query provider is substituted"]
fn missing_cache_can_be_republished_from_unchanged_canonical_observation() {
    let world = World::new();
    let runtime = world.runtime();
    let mut observed = world.observed();
    let engine = catalog::generation(&runtime).unwrap();
    let environment = world.environment();
    let project = observed
        .checked
        .get("project_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        catalog::load_catalog_with_environment(
            &observed.root,
            &project,
            &runtime,
            None,
            &environment
        )
        .is_err()
    );
    catalog::publish_with_environment(&mut observed, &engine, &environment).unwrap();
    let loaded = catalog::load_catalog_with_environment(
        &observed.root,
        &project,
        &runtime,
        None,
        &environment,
    )
    .unwrap();
    assert_eq!(loaded.hashes, observed.hashes);
    loaded.recheck(None).unwrap();
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_late_mmap_change_with_restored_mtime_cannot_pass_the_final_whole_content_check
#[test]
fn final_recheck_hashes_actual_bytes_and_removes_stale_owned_pointer() {
    support::with_frontier(
        "final_recheck_hashes_actual_bytes_and_removes_stale_owned_pointer",
        || {
            let world = World::new();
            let runtime = world.runtime();
            let mut observed = world.observed();
            let engine = catalog::generation(&runtime).unwrap();
            let environment = world.environment();
            catalog::publish_with_environment(&mut observed, &engine, &environment).unwrap();
            let project = observed
                .checked
                .get("project_id")
                .unwrap()
                .as_str()
                .unwrap();
            let loaded = catalog::load_catalog_with_environment(
                &observed.root,
                project,
                &runtime,
                None,
                &environment,
            )
            .unwrap();
            let target = observed.bank.join("projectbrief.md");
            let original = std::fs::read(&target).unwrap();
            let changed = String::from_utf8(original)
                .unwrap()
                .replacen("Project", "Changed", 1);
            std::fs::write(&target, changed).unwrap();
            assert!(loaded.recheck(None).is_err());
            let (_, _, directory) =
                catalog::locations_with_environment(&observed.root, &runtime.pack, &environment)
                    .unwrap();
            assert!(!directory.join("current.json").exists());
        },
    );
}

// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_loaded_immutable_reader_survives_concurrent_generation_replacement
#[test]
fn immutable_loaded_reader_survives_replacement_and_owned_generation_collection() {
    support::with_frontier(
        "immutable_loaded_reader_survives_replacement_and_owned_generation_collection",
        || {
            let world = World::new();
            let runtime = world.runtime();
            let mut observed = world.observed();
            let environment = world.environment();
            let engine = catalog::generation(&runtime).unwrap();
            catalog::publish_with_environment(&mut observed, &engine, &environment).unwrap();
            let project = observed
                .checked
                .get("project_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            let loaded = catalog::load_catalog_with_environment(
                &observed.root,
                &project,
                &runtime,
                None,
                &environment,
            )
            .unwrap();
            let (_, _, directory) =
                catalog::locations_with_environment(&observed.root, &runtime.pack, &environment)
                    .unwrap();
            let old = catalog::pointer(&directory)
                .unwrap()
                .0
                .get("file")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            std::fs::remove_file(directory.join("current.json")).unwrap();
            catalog::publish_with_environment(&mut observed, &engine, &environment).unwrap();
            assert!(!directory.join(old).exists());
            loaded.recheck(None).unwrap();
        },
    );
}
