use agent_memory::{
    bank_capacity, bank_contract, bank_observation as obs, bank_state,
    codec::{
        frontmatter,
        json::{self, Value},
    },
    error::Kind,
    fs, records, tombstones,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
const ID: &str = "a9abac6f-c33e-44c9-b40f-7b2ebafda860";
const AUTHORITY: &str = "8232597a-ad5e-4f68-96b4-15c3fd71f335";
fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn obj<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
struct World(PathBuf);
impl World {
    fn new() -> Self {
        let path = PathBuf::from("/tmp").join(format!(
            "am-observation-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn metadata() -> Value {
    obj([
        ("id", s(ID)),
        ("scope", s(PROJECT)),
        ("deleted-at", s("2026-10-22T18:00:00.123456789012345Z")),
        ("deletion-authority", s(AUTHORITY)),
        ("reason-class", s("user-request")),
    ])
}
fn marker() -> String {
    format!(
        "---\nid: \"{ID}\"\nscope: \"{PROJECT}\"\ndeleted-at: \"2026-10-22T18:00:00.123456789012345Z\"\ndeletion-authority: \"{AUTHORITY}\"\nreason-class: \"user-request\"\n---\n"
    )
}
fn capacity(
    error: &agent_memory::error::ContractError,
    code: &str,
    limit: usize,
    observed: usize,
    measurement: &str,
) {
    let details = bank_capacity::details(error);
    assert_eq!(details.get("failure_code"), Some(&s(code)));
    assert_eq!(
        details.get("capacity_limit"),
        Some(&Value::Int(limit.into()))
    );
    assert_eq!(
        details.get("capacity_observed"),
        Some(&Value::Int(observed.into()))
    );
    assert_eq!(details.get("capacity_measurement"), Some(&s(measurement)));
    assert_eq!(details.get("failure_phase"), Some(&s("bank-eligibility")));
    assert!(!error.to_string().contains("PRIVATE_BODY_CANARY"));
}
fn open_fixture_descriptors(root: &Path) -> usize {
    std::fs::read_dir("/dev/fd")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter(|path| path.starts_with(root))
        .count()
}

// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_exact_five_field_format_roundtrips_without_personal_prose_or_generated_state
#[test]
fn exact_five_field_format_roundtrips_with_committed_contract_bytes() {
    let world = World::new();
    let rendered = tombstones::render(&metadata()).unwrap();
    assert_eq!(rendered.get("content"), Some(&s(marker())));
    assert_eq!(
        rendered.get("path"),
        Some(&s(format!("history/tombstones/{ID}.md")))
    );
    assert_eq!(rendered.get("metadata"), Some(&metadata()));
    assert_eq!(rendered.get("writes_performed"), Some(&Value::Bool(false)));
    assert_eq!(
        rendered.get("identity_authenticated"),
        Some(&Value::Bool(false))
    );
    world.write("marker.md", marker());
    let checked =
        tombstones::validate(&std::fs::read_to_string(world.0.join("marker.md")).unwrap()).unwrap();
    assert_eq!(checked.get("metadata"), Some(&metadata()));
    assert_eq!(checked.get("valid"), Some(&Value::Bool(true)));
    assert_eq!(
        checked.get("validation_scope"),
        Some(&s("tombstone-format"))
    );
    assert_eq!(std::fs::read_dir(&world.0).unwrap().count(), 1);
}
// obligation: tests/contracts/test_records.py::RecordContract.test_copied_artifact_and_five_field_tombstone_exception_require_no_harness_or_state
#[test]
fn real_tombstone_parser_dispatches_without_policy_or_harness() {
    let (kind, actual, body) = records::parse(&marker(), &[]).unwrap();
    assert_eq!(kind, "tombstone");
    assert_eq!(actual, metadata());
    assert_eq!(body, "");
    assert_eq!(
        records::validate(&metadata(), "", &[]).unwrap(),
        ("tombstone".into(), metadata())
    );
    for required in ["confidence", "trust", "provenance"] {
        assert!(records::parse(&marker(), &[required]).is_err());
    }
    assert!(records::parse(&(marker() + "deleted payload"), &[]).is_err());
}
// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_sensitive_extra_fields_bodies_names_and_fingerprints_refuse_without_echo_or_mutation
#[test]
fn sensitive_extra_fields_and_bodies_refuse_content_free_without_mutation() {
    let world = World::new();
    for key in [
        "body",
        "source",
        "evidence",
        "hash",
        "content-sha256",
        "email",
        "deleted-path",
        "title",
        "DELETED_SECRET_CANARY",
    ] {
        let mut bad = metadata();
        if let Value::Object(fields) = &mut bad {
            fields.push((key.into(), s("DELETED_SECRET_CANARY")));
        }
        let command = tombstones::command_result("render", tombstones::render(&bad));
        assert_eq!(command.exit_code, 1);
        assert!(!command.stdout.contains("DELETED_SECRET_CANARY"));
        assert_eq!(
            command.value.unwrap().get("error"),
            Some(&s(tombstones::INVALID_INPUT))
        );
    }
    for key in [
        "id",
        "scope",
        "deletion-authority",
        "reason-class",
        "deleted-at",
    ] {
        for bad_value in [
            s("DELETED_SECRET_CANARY"),
            Value::Null,
            Value::Bool(false),
            Value::Array(vec![]),
            obj([("secret", s("DELETED_SECRET_CANARY"))]),
        ] {
            let mut bad = metadata();
            *bad.get_mut(key).unwrap() = bad_value;
            let command = tombstones::command_result("render", tombstones::render(&bad));
            assert_eq!(command.exit_code, 1);
            assert!(!command.stdout.contains("DELETED_SECRET_CANARY"));
        }
    }
    let bad_body = marker() + "DELETED_SECRET_CANARY\n";
    world.write("bad.md", &bad_body);
    let command = tombstones::command_result("validate", tombstones::validate(&bad_body));
    assert_eq!(command.exit_code, 1);
    assert!(!command.stdout.contains("DELETED_SECRET_CANARY"));
    assert_eq!(
        std::fs::read_to_string(world.0.join("bad.md")).unwrap(),
        bad_body
    );
}
#[test]
fn tombstone_reason_enum_timestamp_and_uuid_boundaries() {
    for reason in [
        "user-request",
        "incorrect",
        "obsolete",
        "sensitive",
        "retention",
    ] {
        let mut value = metadata();
        *value.get_mut("reason-class").unwrap() = s(reason);
        assert!(tombstones::render(&value).is_ok());
    }
    for identity in [
        ID.to_uppercase(),
        "00000000-0000-1000-8000-000000000000".into(),
        "00000000-0000-4000-0000-000000000000".into(),
    ] {
        let mut value = metadata();
        *value.get_mut("id").unwrap() = s(identity);
        assert!(tombstones::value(&value).is_err());
    }
    for time in [
        "2026-10-22",
        "2026-10-22T24:00:00Z",
        "2026-10-22T18:00:00+00:99",
    ] {
        let mut value = metadata();
        *value.get_mut("deleted-at").unwrap() = s(time);
        assert!(tombstones::value(&value).is_err());
    }
    let mut reversed = metadata().as_object().unwrap().to_vec();
    reversed.reverse();
    assert_eq!(
        tombstones::value(&Value::Object(reversed)).unwrap(),
        metadata()
    );
    assert_eq!(
        tombstones::parse(&(marker() + " \t\n\u{1c}")).unwrap(),
        metadata()
    );
}
// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_every_physical_history_occupant_is_checked_before_returning_stale_content
#[test]
fn every_physical_history_occupant_and_spelling_alias_is_checked() {
    for variant in [
        "wrong-extension",
        "uppercase-extension",
        "hidden-runtime",
        "subdirectory",
        "nested-marker",
        "history-file",
        "tombstones-file",
        "history-alias",
        "tombstones-alias",
        "symlink",
    ] {
        let world = World::new();
        let path = match variant {
            "wrong-extension" => format!("history/tombstones/{ID}.txt"),
            "uppercase-extension" => format!("history/tombstones/{ID}.MD"),
            "hidden-runtime" => "history/tombstones/.agent-memory-hidden.txt".into(),
            "nested-marker" => format!("history/tombstones/other/{ID}.md"),
            "history-file" => "history".into(),
            "tombstones-file" => "history/tombstones".into(),
            "history-alias" => format!("History/tombstones/{ID}.md"),
            "tombstones-alias" => format!("history/Tombstones/{ID}.md"),
            _ => "history/tombstones/other".into(),
        };
        if variant == "subdirectory" {
            std::fs::create_dir_all(world.0.join(&path)).unwrap();
        } else if variant == "symlink" {
            std::fs::create_dir_all(world.0.join("history/tombstones")).unwrap();
            world.write("outside", marker());
            symlink(world.0.join("outside"), world.0.join(&path)).unwrap();
        } else {
            world.write(&path, marker());
        }
        assert!(tombstones::canonical_paths(&world.0).is_err(), "{variant}");
        assert!(world.0.join(path).symlink_metadata().is_ok());
    }
}
#[test]
fn canonical_history_paths_allow_only_canonical_ordinary_markers() {
    let world = World::new();
    assert!(tombstones::canonical_paths(&world.0).unwrap().is_empty());
    let path = tombstones::relative_path(ID);
    world.write(&path, marker());
    assert_eq!(
        tombstones::canonical_paths(&world.0).unwrap(),
        [path.clone()].into_iter().collect()
    );
    let scanned = obs::scan(&world.0, true, false).unwrap();
    obs::physical_contract(&scanned.physical).unwrap();
    assert!(scanned.sources.contains_key(&path));
}

// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_complete_utf8_source_boundary_and_file_only_validator
#[test]
fn complete_utf8_file_boundary_and_known_hashes_are_preserved() {
    let world = World::new();
    world.write("MEMORY.md", b"abc");
    world.write("nested/empty.md", b"");
    let scanned = obs::scan(&world.0, true, true).unwrap();
    assert_eq!(
        scanned.hashes.get("MEMORY.md"),
        Some(&s(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ))
    );
    assert_eq!(
        scanned.hashes.get("nested/empty.md"),
        Some(&s(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ))
    );
    assert_eq!((scanned.payload_bytes, scanned.file_reads), (3, 2));
    let prefix = "# Fixture context\n\nPRIVATE_BODY_CANARY café 🧠\n";
    let body = format!("{prefix}{}\n", "x".repeat(16384 - prefix.len() - 1));
    world.write("context.md", &body);
    assert_eq!(
        obs::read_source(&world.0, "context.md", 16384)
            .unwrap()
            .len(),
        16384
    );
    world.write("context.md", body + "x");
    let error = obs::scan(&world.0, false, true).unwrap_err();
    capacity(&error, "bank-source-byte-capacity", 16384, 16385, "exact");
    assert_eq!(
        std::fs::metadata(world.0.join("context.md")).unwrap().len(),
        16385
    );
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_stable_reported_size_descriptor_is_exact_and_preserved
#[test]
fn stable_reported_oversized_source_is_exact_and_preserved() {
    let world = World::new();
    let original = [
        b"PRIVATE_BODY_CANARY".as_slice(),
        &vec![b'x'; 79054 - b"PRIVATE_BODY_CANARY".len()],
    ]
    .concat();
    world.write("projectbrief.md", &original);
    let error = obs::read_source(&world.0, "projectbrief.md", 16384).unwrap_err();
    capacity(&error, "bank-source-byte-capacity", 16384, 79054, "exact");
    assert_eq!(
        std::fs::read(world.0.join("projectbrief.md")).unwrap(),
        original
    );
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_complete_map_count_is_exact_and_partial_inventory_is_at_least
#[test]
fn partial_inventory_count_is_at_least_and_reads_the_excess_source() {
    let world = World::new();
    world.write("one.md", "one");
    world.write("two.md", "two");
    world.write("zzz.md", "three");
    let error = obs::scan_with_limits(
        &world.0,
        false,
        false,
        obs::ScanLimits {
            max_files: 1,
            max_bytes: obs::MAX_BYTES,
        },
    )
    .unwrap_err();
    capacity(&error, "bank-source-count-capacity", 1, 2, "at-least");
    assert_eq!(std::fs::read_dir(&world.0).unwrap().count(), 3);
}
#[test]
fn aggregate_excess_is_lower_bound_and_source_oversize_has_priority() {
    let world = World::new();
    world.write("one.md", "1234");
    world.write("two.md", "5678");
    let limits = obs::ScanLimits {
        max_files: obs::MAX_FILES,
        max_bytes: 5,
    };
    let error = obs::scan_with_limits(&world.0, false, false, limits).unwrap_err();
    capacity(&error, "bank-aggregate-byte-capacity", 5, 6, "at-least");
    world.write("two.md", vec![b'x'; 16385]);
    let error = obs::scan_with_limits(&world.0, false, false, limits).unwrap_err();
    capacity(&error, "bank-source-byte-capacity", 16384, 16385, "exact");
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_public_snapshot_refuses_real_aggregate_excess
#[test]
fn real_aggregate_budget_is_enforced_without_returning_partial_sources() {
    let world = World::new();
    let bytes = vec![b'x'; 16384];
    for index in 0..4097 {
        world.write(&format!("bulk/{index:04}.md"), &bytes);
    }
    let error = obs::scan(&world.0, false, false).unwrap_err();
    capacity(
        &error,
        "bank-aggregate-byte-capacity",
        64 * 1024 * 1024,
        64 * 1024 * 1024 + 1,
        "at-least",
    );
    assert_eq!(
        std::fs::read_dir(world.0.join("bulk")).unwrap().count(),
        4097
    );
    assert_eq!(std::fs::read(world.0.join("bulk/4096.md")).unwrap(), bytes);
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_excess_count_source_redirect_stays_an_unsafe_refusal
#[test]
fn unsafe_excess_count_source_wins_over_count_capacity() {
    let world = World::new();
    world.write("one.md", "one");
    world.write("outside", "PRIVATE_BODY_CANARY");
    symlink(world.0.join("outside"), world.0.join("two.md")).unwrap();
    let error = obs::scan_with_limits(
        &world.0,
        false,
        false,
        obs::ScanLimits {
            max_files: 1,
            max_bytes: obs::MAX_BYTES,
        },
    )
    .unwrap_err();
    assert!(!matches!(error.kind(), Kind::BankCapacity { .. }));
    assert_eq!(
        std::fs::read_to_string(world.0.join("outside")).unwrap(),
        "PRIVATE_BODY_CANARY"
    );
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_symlink_and_nonregular_markdown_are_noncapacity_refusals
#[test]
fn symlink_and_nonregular_markdown_refuse_without_following() {
    let world = World::new();
    world.write("outside", "safe");
    symlink(world.0.join("outside"), world.0.join("source.md")).unwrap();
    let error = obs::scan(&world.0, false, false).unwrap_err();
    assert_eq!(error.message(), "Bank paths must not be symlinks");
    assert!(!matches!(error.kind(), Kind::BankCapacity { .. }));
    std::fs::remove_file(world.0.join("source.md")).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(world.0.join("source.md")).unwrap();
    let error = obs::scan(&world.0, false, false).unwrap_err();
    assert_eq!(
        error.message(),
        "Canonical Markdown source must be an ordinary file"
    );
    assert!(!matches!(error.kind(), Kind::BankCapacity { .. }));
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_parent_replaced_by_symlink_is_not_followed
#[test]
fn parent_replaced_by_symlink_is_never_followed() {
    let world = World::new();
    world.write("nested/source.md", "safe");
    world.write("outside/source.md", "outside");
    std::fs::rename(world.0.join("nested"), world.0.join("old")).unwrap();
    symlink(world.0.join("outside"), world.0.join("nested")).unwrap();
    assert!(obs::read_source(&world.0, "nested/source.md", 16384).is_err());
    assert_eq!(
        std::fs::read_to_string(world.0.join("old/source.md")).unwrap(),
        "safe"
    );
    assert_eq!(
        std::fs::read_to_string(world.0.join("outside/source.md")).unwrap(),
        "outside"
    );
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_oversized_source_or_parent_replaced_before_binding_recheck
#[test]
fn pinned_oversized_source_or_parent_replacement_remains_unsafe_not_capacity() {
    for change_parent in [false, true] {
        let world = World::new();
        world.write("nested/source.md", vec![b'x'; 79054]);
        let pinned = fs::PinnedRead::open(&world.0, Path::new("nested/source.md"), 16384).unwrap();
        if change_parent {
            std::fs::rename(world.0.join("nested"), world.0.join("old")).unwrap();
        } else {
            std::fs::rename(
                world.0.join("nested/source.md"),
                world.0.join("nested/old.txt"),
            )
            .unwrap();
        }
        world.write("nested/source.md", "replacement");
        assert!(matches!(
            pinned.read(),
            Err(fs::ReadError::SourceChanged | fs::ReadError::ParentChanged)
        ));
    }
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_reused_parent_substitution_refuses_and_closes_descriptors
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_reused_parent_replaced_after_leaf_open_refuses_and_closes
#[test]
fn pinned_parent_and_leaf_substitution_refuse_and_release_descriptors() {
    for change_parent in [false, true] {
        let world = World::new();
        world.write("nested/source.md", "original");
        let pinned = fs::PinnedRead::open(&world.0, Path::new("nested/source.md"), 16384).unwrap();
        if change_parent {
            std::fs::rename(world.0.join("nested"), world.0.join("old")).unwrap();
        } else {
            std::fs::rename(
                world.0.join("nested/source.md"),
                world.0.join("nested/old.md"),
            )
            .unwrap();
        }
        world.write("nested/source.md", "replacement");
        assert!(pinned.read().is_err());
        assert_eq!(open_fixture_descriptors(&world.0), 0);
    }
}
#[test]
fn growth_after_open_is_source_changed_not_capacity() {
    let world = World::new();
    world.write("source.md", "x");
    let pinned = fs::PinnedRead::open(&world.0, Path::new("source.md"), 16384).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(world.0.join("source.md"))
        .unwrap()
        .write_all(&vec![b'x'; 16384])
        .unwrap();
    assert!(matches!(pinned.read(), Err(fs::ReadError::SourceChanged)));
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_scan_closes_descriptors_on_success_and_count_capacity
#[test]
fn scanner_releases_descriptors_on_success_and_capacity() {
    let world = World::new();
    world.write("a.md", "a");
    world.write("b.md", "b");
    obs::scan(&world.0, true, false).unwrap();
    assert_eq!(open_fixture_descriptors(&world.0), 0);
    assert!(
        obs::scan_with_limits(
            &world.0,
            true,
            false,
            obs::ScanLimits {
                max_files: 1,
                max_bytes: obs::MAX_BYTES
            }
        )
        .is_err()
    );
    assert_eq!(open_fixture_descriptors(&world.0), 0);
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_stream_construction_failure_preserves_an_unrelated_descriptor
#[test]
fn read_failure_never_closes_unrelated_descriptor() {
    let world = World::new();
    world.write("unrelated", "Independent descriptor must remain usable");
    let mut file = std::fs::File::open(world.0.join("unrelated")).unwrap();
    assert!(obs::read_source(&world.0, "missing.md", 16384).is_err());
    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();
    assert_eq!(text, "Independent descriptor must remain usable");
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_unknown_markers_are_refused_without_opening
#[test]
fn unknown_markers_are_preserved_and_content_free() {
    for name in [".agent-memory.lock", ".agent-memory-transaction.md"] {
        let world = World::new();
        let body = format!("PRIVATE_BODY_CANARY{}", "x".repeat(5000));
        world.write(name, &body);
        let error = obs::markers(&world.0, None, None).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_BODY_CANARY"));
        assert!(!matches!(error.kind(), Kind::BankCapacity { .. }));
        assert_eq!(std::fs::read_to_string(world.0.join(name)).unwrap(), body);
    }
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_exact_owned_large_transaction_marker_and_mismatch
#[test]
fn exact_owned_large_markers_and_mismatch() {
    let world = World::new();
    let expected = vec![b'x'; 20000];
    world.write(".agent-memory-transaction.md", &expected);
    obs::markers(&world.0, None, Some(&expected)).unwrap();
    assert!(obs::markers(&world.0, None, Some(&expected[..expected.len() - 1])).is_err());
    assert!(obs::markers(&world.0, None, Some(b"wrong")).is_err());
    assert_eq!(
        std::fs::read(world.0.join(".agent-memory-transaction.md")).unwrap(),
        expected
    );
    std::fs::remove_file(world.0.join(".agent-memory-transaction.md")).unwrap();
    assert_eq!(
        obs::markers(&world.0, None, Some(&expected))
            .unwrap_err()
            .message(),
        "Owned bank lock disappeared; preserve work and retry with actual ownership"
    );
    world.write(".agent-memory.lock", b"owner");
    obs::markers(&world.0, Some(b"owner"), None).unwrap();
}
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_owned_marker_replacement_during_read_is_refused
#[test]
fn owned_marker_replacement_is_detected_by_pinned_read() {
    for name in [".agent-memory.lock", ".agent-memory-transaction.md"] {
        let world = World::new();
        world.write(name, vec![b'x'; 5000]);
        let pinned = fs::PinnedRead::open(&world.0, Path::new(name), 5000).unwrap();
        std::fs::rename(world.0.join(name), world.0.join("old-marker")).unwrap();
        world.write(name, "foreign marker");
        assert!(matches!(pinned.read(), Err(fs::ReadError::SourceChanged)));
        assert_eq!(
            std::fs::metadata(world.0.join("old-marker")).unwrap().len(),
            5000
        );
        assert_eq!(
            std::fs::read_to_string(world.0.join(name)).unwrap(),
            "foreign marker"
        );
    }
}
#[test]
fn runtime_topology_is_included_but_runtime_markdown_is_not_payload() {
    let world = World::new();
    world.write(".agent-memory-work/nested/ignored.md", "not canonical");
    world.write("source.md", "abc");
    let (physical, hashes, sources) = obs::scan_sources(&world.0).unwrap();
    assert_eq!(physical.get(".agent-memory-work"), Some(&s("directory")));
    assert_eq!(
        physical.get(".agent-memory-work/nested/ignored.md"),
        Some(&s("file"))
    );
    assert_eq!(physical.get("source.md"), Some(&s("file")));
    assert_eq!(hashes.as_object().unwrap().len(), 1);
    assert_eq!(sources.len(), 1);
    assert!(sources.contains_key("source.md"));
    assert_eq!(obs::scan_hashes(&world.0).unwrap(), hashes);
    world.write(".agent-memory-work/other.txt", "topology change");
    assert_ne!(obs::scan_sources(&world.0).unwrap().0, physical);
}
#[test]
fn scanner_checks_runtime_symlinks_and_finite_physical_inventory() {
    let world = World::new();
    world.write(".agent-memory-work/one", "one");
    world.write(".agent-memory-work/two", "two");
    let error = obs::scan_with_limits(
        &world.0,
        false,
        false,
        obs::ScanLimits {
            max_files: 1,
            max_bytes: obs::MAX_BYTES,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.message(),
        "Complete physical bank inventory exceeds its finite budget"
    );
    symlink("/", world.0.join(".agent-memory-work/link")).unwrap();
    assert!(obs::scan(&world.0, false, false).is_err());
}
#[test]
fn scan_requires_router_only_when_requested_and_refuses_invalid_limits() {
    let world = World::new();
    assert_eq!(
        obs::scan(&world.0, true, true).unwrap_err().message(),
        "A complete canonical router is required"
    );
    assert!(obs::scan(&world.0, true, false).unwrap().sources.is_empty());
    assert!(
        obs::scan_with_limits(
            &world.0,
            false,
            false,
            obs::ScanLimits {
                max_files: obs::MAX_FILES + 1,
                max_bytes: obs::MAX_BYTES
            }
        )
        .is_err()
    );
    assert!(obs::read_source(&world.0, "../escape", 16384).is_err());
    assert!(obs::read_source(&world.0, "/absolute", 16384).is_err());
}
#[test]
fn physical_history_contract_refuses_portable_aliases_and_bad_kinds() {
    for map in [
        BTreeMap::from([("History".into(), "directory".into())]),
        BTreeMap::from([("history".into(), "file".into())]),
        BTreeMap::from([("history/tombstones".into(), "file".into())]),
        BTreeMap::from([("history/tombstones/hidden.txt".into(), "file".into())]),
        BTreeMap::from([(format!("history/tombstones/{ID}.md"), "directory".into())]),
    ] {
        assert!(obs::physical_contract(&map).is_err());
    }
}
#[test]
fn absent_domain_provider_is_explicit_refusal_not_a_silent_fallback() {
    let world = World::new();
    std::fs::create_dir(world.0.join("memory-bank")).unwrap();
    let error = obs::observe(&world.0, None, None).unwrap_err();
    assert_eq!(
        error.message(),
        "recovery_frontier provider is not registered"
    );
    let result = bank_state::command_inspect(&world.0, None);
    assert_eq!(result.exit_code, 1);
    assert_eq!(
        result.value.unwrap().get("writes_performed"),
        Some(&Value::Bool(false))
    );
}
// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_missing_git_still_reads_the_copied_portable_markdown_bank
#[test]
fn missing_git_is_optional_and_does_not_claim_publication() {
    let world = World::new();
    let value = bank_state::git_context_with_environment(
        &world.0,
        &Value::Object(vec![]),
        &[(OsString::from("PATH"), world.0.as_os_str().to_owned())],
    )
    .unwrap();
    assert_eq!(value.get("status"), Some(&s("unavailable")));
    assert_eq!(
        value.get("reason"),
        Some(&s(
            "Git is not installed; current Markdown remains readable"
        ))
    );
}
#[test]
fn plain_project_git_inspection_is_read_only_and_not_discovered() {
    let world = World::new();
    let value = bank_state::git_context_with_environment(
        &world.0,
        &Value::Object(vec![]),
        &[
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), world.0.as_os_str().to_owned()),
        ],
    )
    .unwrap();
    assert_eq!(value.get("status"), Some(&s("not-discovered")));
    assert_eq!(std::fs::read_dir(&world.0).unwrap().count(), 0);
}
#[test]
fn structured_command_errors_retain_capacity_and_tombstone_errors_never_echo() {
    let world = World::new();
    world.write("source.md", vec![b'x'; 16385]);
    let error = obs::read_source(&world.0, "source.md", 16384).unwrap_err();
    let result = tombstones::command_result("filter", Err(error.clone()));
    assert_eq!(result.exit_code, 1);
    assert_eq!(
        result.value.unwrap().get("failure_code"),
        Some(&s("bank-source-byte-capacity"))
    );
    let result = bank_state::command_result(Err(error));
    assert_eq!(result.exit_code, 1);
    assert_eq!(
        result.value.unwrap().get("capacity_observed"),
        Some(&Value::Int(16385.into()))
    );
    let result = tombstones::command_result(
        "render",
        Err(agent_memory::error::ContractError::new(
            "DELETED_SECRET_CANARY",
        )),
    );
    assert!(!result.stdout.contains("DELETED_SECRET_CANARY"));
    assert!(json::decode(result.stdout.trim_end(), json::DecodePolicy::default()).is_ok());
    assert!(
        frontmatter::read(
            &marker(),
            &[
                "id",
                "scope",
                "deleted-at",
                "deletion-authority",
                "reason-class"
            ]
        )
        .is_ok()
    );
}

// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_tombstones_and_duplicate_record_versions_cannot_hide_or_recreate_current_content
#[test]
fn real_tombstone_parser_allows_safe_bank_and_refuses_resurrection() {
    let mut proposal = json::decode(
        include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
        json::DecodePolicy::default(),
    )
    .unwrap();
    *proposal.get_mut("project-id").unwrap() = s(PROJECT);
    let mut files = bank_contract::render(&proposal)
        .unwrap()
        .get("files")
        .unwrap()
        .clone();
    let path = tombstones::relative_path(ID);
    let router = files.get("MEMORY.md").unwrap().as_str().unwrap().replace("\n## Omitted contexts\n", &format!("| [{path}](<{path}>) | Deletion guard | repository | deleted | Before rebuilding projections |\n\n## Omitted contexts\n"));
    *files.get_mut("MEMORY.md").unwrap() = s(router);
    if let Value::Object(entries) = &mut files {
        entries.push((path.clone(), s(marker())));
    }
    let mut parsed = bank_contract::ParsedRecords::new();
    assert!(bank_contract::validate_files(&files, Some(&mut parsed)).is_ok());
    assert_eq!(parsed.get(&path).unwrap().0, "tombstone");
    for case in ["resurrection", "duplicate"] {
        let mut changed = files.clone();
        let mut metadata = json::decode(
            include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
            json::DecodePolicy::default(),
        )
        .unwrap()
        .get("metadata")
        .unwrap()
        .clone();
        for (key, value) in [
            (
                "id",
                s(if case == "resurrection" {
                    ID
                } else {
                    AUTHORITY
                }),
            ),
            ("project-id", s(PROJECT)),
            ("scope", s("repository")),
            ("status", s("active")),
            ("conditions", Value::Object(vec![])),
        ] {
            *metadata.get_mut(key).unwrap() = value;
        }
        *metadata
            .get_mut("origin")
            .unwrap()
            .get_mut("code-state")
            .unwrap() = s("not-code");
        if let Value::Object(fields) = &mut metadata {
            fields.extend([
                ("trust".into(), s("reviewed")),
                ("visibility".into(), s("visible")),
                (
                    "admission-reason".into(),
                    s("Disposable structural fixture; not real admission evidence"),
                ),
            ]);
        }
        let content = frontmatter::write(metadata.as_object().unwrap()).unwrap()
            + "# SECRET_DELETION_CANARY\n";
        let paths = if case == "duplicate" {
            vec!["topics/one.md", "topics/two.md"]
        } else {
            vec!["topics/one.md"]
        };
        for path in paths {
            let router = changed.get("MEMORY.md").unwrap().as_str().unwrap().replace("\n## Omitted contexts\n",&format!("| [{path}](<{path}>) | Fixture lesson | repository | active | When examining fixture |\n\n## Omitted contexts\n"));
            *changed.get_mut("MEMORY.md").unwrap() = s(router);
            if let Value::Object(entries) = &mut changed {
                entries.push((path.into(), s(&content)));
            }
        }
        let error = bank_contract::validate_files(&changed, None).unwrap_err();
        assert!(!error.to_string().contains("SECRET_DELETION_CANARY"));
    }
    let world = World::new();
    for (path, body) in files.as_object().unwrap() {
        world.write(path, body.as_str().unwrap());
    }
    world.write("history/tombstones/hidden.txt", "SECRET_DELETION_CANARY");
    let physical = obs::physical_inventory(&world.0).unwrap();
    assert!(obs::physical_contract(&physical).is_err());
}

#[test]
fn count_capacity_precedes_aggregate_after_a_safe_excess_source_read() {
    let world = World::new();
    world.write("a.md", "a");
    world.write("b.md", "b");
    let error = obs::scan_with_limits(
        &world.0,
        false,
        false,
        obs::ScanLimits {
            max_files: 1,
            max_bytes: 1,
        },
    )
    .unwrap_err();
    capacity(&error, "bank-source-count-capacity", 1, 2, "at-least");
}
