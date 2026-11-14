use agent_memory::codec::json::Value;
use agent_memory::error::{Kind, Result};
use sha2::{Digest, Sha256};
fn object(pairs: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}
use agent_memory::turn_sources as sources;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

fn sha256_hex(raw: &[u8]) -> String {
    Sha256::digest(raw)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const PROJECT: &str = "b3dcae21-f5f3-40ac-bcfe-e845a3d2868b";
const OBSERVATION: &str = "75b38e17-c36e-4fab-8ba9-5fa51f25de91";
const SECOND: &str = "bdcb692b-2322-4f21-b9dd-a4a4eca21145";
struct World(PathBuf);
impl World {
    fn new() -> Result<Self> {
        let path =
            std::env::temp_dir().join(format!("am-turn-sources-{}", agent_memory::host::uuid4()?));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        Ok(Self(agent_memory::fs::canonical_path(&path, false)?))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn identity() -> Value {
    object([
        ("project_id", string(PROJECT)),
        ("harness", string("codex")),
        ("session", string("native-session")),
        ("lifecycle", string("native-lifecycle")),
    ])
}
fn set(value: &mut Value, key: &str, replacement: Value) {
    if let Some(v) = value.get_mut(key) {
        *v = replacement;
    } else if let Value::Object(v) = value {
        v.push((key.into(), replacement));
    }
}
fn observation(id: &str, digest: &str) -> Value {
    let mut value = identity();
    for (key, item) in [
        ("schema", string("agent-memory/turn-source-v2")),
        ("observation_id", string(id)),
        ("source_sha256", string(digest)),
        ("native_source_id", Value::Null),
        ("source_reference", Value::Null),
        ("control_origin_hint", Value::Bool(false)),
    ] {
        set(&mut value, key, item);
    }
    value
}
fn receipt(id: &str, digest: &str) -> Value {
    object([
        ("observation_id", string(id)),
        ("source_sha256", string(digest)),
        ("outcome", string("no-learning")),
        (
            "assessment",
            object([
                ("source_id", string(id)),
                (
                    "source_review",
                    string("Reviewed the actual bounded fixture source; no durable project claim."),
                ),
                ("candidates", Value::Array(vec![])),
            ]),
        ),
        ("assessment_is_agent_assertion", Value::Bool(true)),
        ("source_publication_link_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("witnessed_commit", Value::Null),
        ("input_sha256", string("b".repeat(64))),
        ("canonical_writes_performed", Value::Bool(false)),
    ])
}
fn completion(id: &str, digest: &str) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/source-completed-v2")),
        ("observation_id", string(id)),
        ("source_sha256", string(digest)),
        ("receipt", receipt(id, digest)),
    ]);
    let checksum = sha256_hex(&sources::encoded(&value)?);
    set(&mut value, "record_sha256", string(checksum));
    Ok(value)
}
fn array_len(value: &Value, key: &str) -> usize {
    get(value, key).as_array().map_or(0, |v| v.len())
}
fn has_fault(value: &Value, fault: &str) -> bool {
    get(value, "faults")
        .as_array()
        .is_some_and(|v| v.contains(&string(fault)))
}
fn namespace(world: &World, identity: Value) -> Result<sources::Namespace> {
    let root = agent_memory::host::text(world.path().as_os_str(), "fixture root")?;
    let key = sha256_hex(&sources::encoded(&Value::Array(vec![
        string(root),
        get(&identity, "harness").clone(),
        get(&identity, "session").clone(),
        get(&identity, "lifecycle").clone(),
    ]))?);
    Ok(sources::Namespace {
        root: world.path().to_path_buf(),
        bank: world.path().join("memory-bank"),
        base: world.path().join("source-state"),
        directory: world.path().join("source-state/turns").join(key),
        identity,
    })
}
fn source_file(directory: &Path, slot: usize, value: &Value) -> Result<()> {
    assert!(sources::exclusive(
        &directory.join(format!("source-{slot}.json")),
        value,
        sources::MAX_SOURCE
    )?);
    Ok(())
}

#[test]
fn exact_sorted_compact_utf8_bytes_and_private_durable_create() -> Result<()> {
    let w = World::new()?;
    let value = object([("z", string("é")), ("a", Value::Bool(false))]);
    assert_eq!(
        sources::encoded(&value)?,
        b"{\"a\":false,\"z\":\"\xc3\xa9\"}"
    );
    let path = w.path().join("immutable.json");
    assert!(sources::exclusive(&path, &value, sources::MAX_SOURCE)?);
    assert!(!sources::exclusive(
        &path,
        &Value::Null,
        sources::MAX_SOURCE
    )?);
    assert_eq!(
        std::fs::read(&path).map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?,
        sources::encoded(&value)?
    );
    assert_eq!(
        std::fs::metadata(&path)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(sources::exclusive(&w.path().join("over-bound.json"), &value, 1).is_err());
    assert!(!w.path().join("over-bound.json").exists());
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_registration_ignores_completion_lock_and_partial_slot_is_pending
#[test]
fn append_is_independent_of_completion_lock_and_corrupt_slots_remain_pending() -> Result<()> {
    let w = World::new()?;
    let digest = "a".repeat(64);
    source_file(w.path(), 0, &observation(OBSERVATION, &digest))?;
    sources::locked(w.path(), || {
        source_file(w.path(), 1, &observation(SECOND, &digest))?;
        assert!(matches!(
            sources::locked(w.path(), || Ok(()))
                .err()
                .map(|e| e.kind().clone()),
            Some(Kind::SourceTransportBusy { .. })
        ));
        Ok(())
    })?;
    let bad = w.path().join("source-2.json");
    std::fs::write(&bad, b"{partial")
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    let state = sources::inventory(w.path(), &identity())?;
    assert_eq!(array_len(&state, "sources"), 2);
    assert!(has_fault(&state, "source-reservation-unverifiable"));
    assert_eq!(get(&state, "complete"), &Value::Bool(false));
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_corrupt_receipt_and_cross_observation_assessment_cannot_complete
#[test]
fn corrupt_completion_checksum_receipt_and_wrong_source_never_acknowledge() -> Result<()> {
    let w = World::new()?;
    let digest = "a".repeat(64);
    source_file(w.path(), 0, &observation(OBSERVATION, &digest))?;
    let path = w.path().join(format!("{OBSERVATION}.done"));
    let original = completion(OBSERVATION, &digest)?;
    assert!(sources::exclusive(
        &path,
        &original,
        sources::MAX_COMPLETION
    )?);
    assert_eq!(
        get(&sources::inventory(w.path(), &identity())?, "complete"),
        &Value::Bool(true)
    );
    for bad in [Value::Object(vec![]), receipt(SECOND, &digest), {
        let mut bad = receipt(OBSERVATION, &digest);
        set(&mut bad, "publication_verified", Value::Bool(true));
        bad
    }] {
        assert!(sources::validate_receipt(&bad, OBSERVATION, &digest).is_err());
        let mut record = original.clone();
        set(&mut record, "receipt", bad);
        std::fs::write(&path, sources::encoded(&record)?)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        let state = sources::inventory(w.path(), &identity())?;
        assert!(has_fault(&state, "completion-unverifiable"));
        assert_eq!(array_len(&state, "pending"), 1);
        assert_eq!(get(&state, "complete"), &Value::Bool(false));
    }
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_native_opaque_ids_and_private_reference
#[test]
fn opaque_native_identity_private_reference_and_source_schema_are_exact() -> Result<()> {
    let mut source = observation(OBSERVATION, &"a".repeat(64));
    set(
        &mut source,
        "native_source_id",
        string("opaque-native-event"),
    );
    set(
        &mut source,
        "source_reference",
        string("/private/transcript.jsonl"),
    );
    sources::validate_source(&source, &identity())?;
    let w = World::new()?;
    source_file(w.path(), 0, &source)?;
    let state = sources::inventory(w.path(), &identity())?;
    let actual = get(&state, "sources")
        .as_array()
        .and_then(|v| v.first())
        .ok_or_else(|| agent_memory::error::ContractError::new("fixture source missing"))?;
    assert_eq!(
        get(actual, "source_reference"),
        &string("/private/transcript.jsonl")
    );
    assert_eq!(
        get(actual, "native_source_id"),
        &string("opaque-native-event")
    );
    for (key, bad) in [
        ("observation_id", string("not-a-uuid")),
        ("source_sha256", string("A".repeat(64))),
        ("control_origin_hint", string("true")),
        ("native_source_id", string("line\nbreak")),
        ("source_reference", string("x".repeat(2049))),
    ] {
        let mut value = source.clone();
        set(&mut value, key, bad);
        assert!(sources::validate_source(&value, &identity()).is_err());
    }
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_index_hole_does_not_hide_later_bad_reference
#[test]
fn pending_index_hole_never_hides_later_malformed_reservation() -> Result<()> {
    let w = World::new()?;
    let ns = namespace(&w, identity())?;
    sources::private_mkdir(&ns.base.join("pending"))?;
    assert!(sources::exclusive(
        &ns.base.join("pending/1.json"),
        &object([] as [(&str, Value); 0]),
        sources::MAX_SOURCE
    )?);
    assert!(sources::index(&ns, None).is_err());
    assert!(!ns.base.join("pending/0.json").exists());
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_fifo_read_refuses_without_waiting_and_bad_json_index_blocks_input
#[test]
fn ordinary_reads_refuse_nonregular_symlink_overbound_and_bad_index() -> Result<()> {
    let w = World::new()?;
    assert!(sources::read(w.path(), sources::MAX_SOURCE).is_err());
    let file = w.path().join("source.json");
    assert!(sources::exclusive(
        &file,
        &Value::Null,
        sources::MAX_SOURCE
    )?);
    let link = w.path().join("link.json");
    std::os::unix::fs::symlink(&file, &link)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert!(sources::read(&link, sources::MAX_SOURCE).is_err());
    assert!(sources::read(&file, 1).is_err());
    for bad in [object([] as [(&str, Value); 0]), string("garbage")] {
        let sub = World::new()?;
        let ns = namespace(&sub, identity())?;
        sources::private_mkdir(&ns.base.join("pending"))?;
        assert!(sources::exclusive(
            &ns.base.join("pending/0.json"),
            &bad,
            sources::MAX_SOURCE
        )?);
        assert!(sources::index(&ns, None).is_err());
        assert!(!ns.base.join("pending/1.json").exists());
    }
    // Real FIFO creation is delegated to the checked process foundation.
    let fifo = w.path().join("fifo");
    let argv = vec![
        "mkfifo".into(),
        "-m".into(),
        "600".into(),
        fifo.as_os_str().to_owned(),
    ];
    let result = agent_memory::process::run(&agent_memory::process::Spec {
        argv: &argv,
        search_path: true,
        env: &[("PATH".into(), "/usr/bin:/bin".into())],
        cwd: Some(w.path()),
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 4096 },
        stderr: agent_memory::process::Target::Capture { limit: 4096 },
        deadline: agent_memory::clock::Deadline::from_seconds(3.0)?,
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    })
    .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert_eq!(result.status, agent_memory::process::Status::Code(0));
    assert!(sources::read(&fifo, sources::MAX_SOURCE).is_err());
    Ok(())
}

// obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_shared_source_barrier_reentrancy_and_lifecycle_contention_preserve_sources
#[test]
fn shared_barrier_reentrancy_no_upgrade_and_descriptor_contention() -> Result<()> {
    let w = World::new()?;
    let a = sources::source_barrier_at(w.path(), false, false)?;
    let b = sources::source_barrier_at(w.path(), false, false)?;
    assert!(sources::source_barrier_held(w.path()));
    assert!(sources::source_barrier_at(w.path(), true, false).is_err());
    assert!(matches!(
        sources::private_lock(&w.path().join("source-barrier.lock"), false, true)
            .err()
            .map(|e| e.kind().clone()),
        Some(Kind::SourceTransportBusy { .. })
    ));
    a.borrowed(|| b.recheck())?;
    drop(a);
    b.recheck()?;
    drop(b);
    assert!(!sources::source_barrier_held(w.path()));
    let mut writer = sources::source_barrier_at(w.path(), true, true)?;
    let nested = sources::source_barrier_at(w.path(), false, true)?;
    writer.finish()?;
    drop(nested);
    drop(writer);
    Ok(())
}

// obligation: tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_both_contended_lock_families_raise_typed_busy_before_body
#[test]
fn both_real_lock_families_refuse_typed_busy_and_release_cleanly() -> Result<()> {
    let w = World::new()?;
    let path = w.path().join("source.lock");
    let held = sources::private_lock(&path, false, false)?;
    assert!(matches!(
        sources::private_lock(&path, false, true)
            .err()
            .map(|e| e.kind().clone()),
        Some(Kind::SourceTransportBusy { .. })
    ));
    drop(held);
    sources::private_lock(&path, false, true)?.finish()?;
    sources::locked(w.path(), || {
        assert!(matches!(
            sources::locked(w.path(), || -> Result<()> {
                panic!("contended body executed")
            })
            .err()
            .map(|e| e.kind().clone()),
            Some(Kind::SourceTransportBusy { .. })
        ));
        Ok(())
    })?;
    sources::locked(w.path(), || Ok(()))?;
    Ok(())
}

#[test]
fn borrowed_barrier_refuses_foreign_thread_and_changed_named_inode() -> Result<()> {
    let w = World::new()?;
    let held = sources::source_barrier_at(w.path(), true, false)?;
    let path = w.path().join("source-barrier.lock");
    std::fs::rename(&path, w.path().join("original-lock"))
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert!(sources::exclusive(
        &path,
        &Value::Null,
        sources::MAX_SOURCE
    )?);
    assert!(held.recheck().is_err());
    assert!(
        held.borrowed(|| -> Result<()> { panic!("changed barrier entered body") })
            .is_err()
    );
    drop(held);
    let held = sources::source_barrier_at(w.path(), true, true)?;
    let joined = std::thread::spawn(move || {
        held.borrowed(|| -> Result<()> { panic!("foreign barrier entered body") })
            .is_err()
    })
    .join();
    assert!(matches!(joined, Ok(true)));
    Ok(())
}

// obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access
#[test]
fn unknown_compatibility_is_preserved_on_entry_and_exit_refusal() -> Result<()> {
    let w = World::new()?;
    let marker = w.path().join("turn-sources-compat-v1.json");
    let valid = object([
        ("schema", string("agent-memory/turn-sources-compat-v1")),
        ("minimum_reader", string("omp-harness-v1")),
        ("harnesses", Value::Array(vec![string("omp")])),
    ]);
    for capability in ["omp-harness-v1", "omp-stored-source-v2"] {
        let mut value = valid.clone();
        set(&mut value, "minimum_reader", string(capability));
        std::fs::write(&marker, sources::encoded(&value)?)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        sources::check_source_compatibility(w.path())?;
    }
    let mut future = valid.clone();
    set(&mut future, "minimum_reader", string("future-reader"));
    let bytes = sources::encoded(&future)?;
    std::fs::write(&marker, &bytes)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert!(sources::source_barrier_at(w.path(), false, false).is_err());
    assert!(!w.path().join("source-barrier.lock").exists());
    std::fs::write(&marker, sources::encoded(&valid)?)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    let mut held = sources::source_barrier_at(w.path(), false, false)?;
    std::fs::write(&marker, &bytes)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert!(held.finish().is_err());
    assert_eq!(
        std::fs::read(&marker)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?,
        bytes
    );
    Ok(())
}

#[test]
fn read_only_lock_observation_never_creates_missing_state() -> Result<()> {
    let w = World::new()?;
    let path = w.path().join("absent.lock");
    sources::read_only_locks(|| {
        assert!(sources::private_lock(&path, false, false).is_err());
        Ok(())
    })?;
    assert!(!path.exists());
    let base = w.path().join("private");
    let directory = base.join("turn-sources-v2/turns/turn");
    sources::private_mkdir(&directory)?;
    let held = sources::source_barrier_at(&base, true, false)?;
    sources::read_only_locks(|| sources::locked(&directory, || Ok(())))?;
    assert!(!directory.join("completion.lock").exists());
    held.recheck()?;
    Ok(())
}

#[test]
fn held_intent_and_unidentified_original_bytes_remain_visible_without_source() -> Result<()> {
    let w = World::new()?;
    let identity = identity();
    let mut intent = identity.clone();
    for (key, item) in [
        ("schema", string(sources::INTENT_SCHEMA)),
        ("enrollment_id", string(SECOND)),
        ("observation_id", string(OBSERVATION)),
        ("native_event_id", string("actual-native-event")),
        ("source_sha256", string("a".repeat(64))),
        ("source_reference", Value::Null),
        ("context_digest", string("b".repeat(64))),
        ("state", string("intent")),
    ] {
        set(&mut intent, key, item);
    }
    let key = sha256_hex(&sources::encoded(&Value::Array(vec![
        identity.clone(),
        string("actual-native-event"),
    ]))?);
    let target = w.path().join("enrollments");
    sources::private_mkdir(&target)?;
    let path = target.join(format!("{key}.json"));
    assert!(sources::exclusive(&path, &intent, sources::MAX_INTENT)?);
    let state = sources::inventory(w.path(), &identity)?;
    assert!(has_fault(&state, "enrollment-unverified"));
    assert!(has_fault(&state, "enrollment-source-missing"));
    assert_eq!(sources::pending_rows(&state, w.path(), &identity)?.len(), 1);
    let updated = sources::enrollment_state(&path, &intent, "registered", None)?;
    assert_eq!(get(&updated, "observation_id"), &string(OBSERVATION));
    assert!(sources::enrollment_state(&path, &intent, "unverified", None).is_err());
    let retained = w.path().join("unidentified-inputs");
    sources::private_mkdir(&retained)?;
    let mut value = identity.clone();
    for (key, item) in [
        ("schema", string(sources::UNIDENTIFIED_SCHEMA)),
        ("retention_id", string(SECOND)),
        ("source_reference", string("/private/native-source")),
        ("source_base64", string("YWN0dWFs")),
        (
            "source_sha256",
            string("e5c6fde86910ded72db5cc7afc32f850440d4ef7caa5dbb69f5bdc0d3e39cb3b"),
        ),
        ("context_digest", string("b".repeat(64))),
        ("uncertainty", string("native-event-identity-unavailable")),
    ] {
        set(&mut value, key, item);
    }
    // The fixed digest is independently checked against the literal source.
    assert_eq!(get(&value, "source_sha256"), &string(sha256_hex(b"actual")));
    assert!(sources::exclusive(
        &retained.join(format!("{SECOND}.json")),
        &value,
        sources::MAX_RETAINED_INPUT
    )?);
    assert_eq!(
        sources::unidentified_inventory(w.path(), &identity)?.len(),
        1
    );
    let state = sources::inventory(w.path(), &identity)?;
    assert!(has_fault(&state, "unidentified-input-retained"));
    assert_eq!(array_len(&state, "sources"), 0);
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_index_cleanup_restores_racing_append_and_unsafe_index_blocks
#[test]
fn cleanup_removes_only_complete_index_and_late_append_is_reindexed() -> Result<()> {
    let w = World::new()?;
    let ns = namespace(&w, identity())?;
    sources::private_mkdir(&ns.directory)?;
    let digest = "a".repeat(64);
    source_file(&ns.directory, 0, &observation(OBSERVATION, &digest))?;
    let slot = sources::index(&ns, None)?;
    assert!(slot.exists());
    sources::cleanup(&ns)?;
    assert!(slot.exists());
    assert!(sources::exclusive(
        &ns.directory.join(format!("{OBSERVATION}.done")),
        &completion(OBSERVATION, &digest)?,
        sources::MAX_COMPLETION
    )?);
    sources::cleanup(&ns)?;
    assert!(!slot.exists());
    source_file(&ns.directory, 1, &observation(SECOND, &digest))?;
    let restored = sources::index(&ns, None)?;
    assert!(restored.exists());
    let state = sources::inventory(&ns.directory, &ns.identity)?;
    assert_eq!(array_len(&state, "pending"), 1);
    sources::cleanup(&ns)?;
    assert!(restored.exists());
    Ok(())
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_caps_fault_and_new_turn_backlog
#[test]
fn retained_capacity_fault_cannot_be_cleared_by_completing_original_sources() -> Result<()> {
    let w = World::new()?;
    let digest = "a".repeat(64);
    source_file(w.path(), 0, &observation(OBSERVATION, &digest))?;
    sources::fault(w.path(), "source-capacity");
    assert!(sources::exclusive(
        &w.path().join(format!("{OBSERVATION}.done")),
        &completion(OBSERVATION, &digest)?,
        sources::MAX_COMPLETION
    )?);
    let state = sources::inventory(w.path(), &identity())?;
    assert_eq!(array_len(&state, "pending"), 0);
    assert!(has_fault(&state, "registration-fault-retained"));
    assert_eq!(get(&state, "complete"), &Value::Bool(false));
    assert!(!sources::exclusive(
        &w.path().join("source-0.json"),
        &Value::Null,
        sources::MAX_SOURCE
    )?);
    Ok(())
}

fn in_repository(name: &str, body: impl FnOnce(&Path, &Path) -> Result<()>) -> Result<()> {
    use agent_memory::{clock::Deadline, process};
    use std::ffi::OsString;
    if std::env::var("AM_TURN_SOURCE_CASE").ok().as_deref() == Some(name) {
        agent_memory::cli::register_providers()
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        let root = std::env::var_os("AM_TURN_SOURCE_ROOT")
            .map(PathBuf::from)
            .ok_or_else(|| {
                agent_memory::error::ContractError::new("missing isolated fixture root")
            })?;
        return body(
            &root.join("repository"),
            Path::new(env!("CARGO_MANIFEST_DIR")),
        );
    }
    let w = World::new()?;
    let home = w.path().join("home");
    let repo = w.path().join("repository");
    sources::private_mkdir(&home)?;
    sources::private_mkdir(&repo)?;
    let env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "/usr/bin:/bin:/usr/local/bin".into()),
        ("HOME".into(), home.into_os_string()),
        (
            "AGENT_MEMORY_CONFIG_HOME".into(),
            w.path().join("config").into_os_string(),
        ),
        (
            "AGENT_MEMORY_STATE_HOME".into(),
            w.path().join("state/agent-memory").into_os_string(),
        ),
        (
            "XDG_CONFIG_HOME".into(),
            w.path().join("xdg-config").into_os_string(),
        ),
        (
            "XDG_STATE_HOME".into(),
            w.path().join("state").into_os_string(),
        ),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("LC_ALL".into(), "C".into()),
        ("AM_TURN_SOURCE_CASE".into(), name.into()),
        (
            "AM_TURN_SOURCE_ROOT".into(),
            w.path().as_os_str().to_owned(),
        ),
    ];
    let run = |argv: &[OsString]| -> Result<()> {
        let output = process::run(&process::Spec {
            argv,
            env: &env,
            search_path: true,
            cwd: Some(&repo),
            stdin: process::Input::Null,
            stdout: process::Target::Capture { limit: 1024 * 1024 },
            stderr: process::Target::Capture { limit: 1024 * 1024 },
            deadline: Deadline::from_seconds(120.0)?,
            new_session: false,
            file_size_limit: None,
            umask: Some(0o077),
            pass_fds: &[],
        })
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        assert_eq!(
            output.status,
            process::Status::Code(0),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    };
    run(&["git".into(), "init".into(), "--initial-branch=main".into()])?;
    run(&[
        "git".into(),
        "config".into(),
        "user.name".into(),
        "Disposable fixture".into(),
    ])?;
    run(&[
        "git".into(),
        "config".into(),
        "user.email".into(),
        "fixture@example.invalid".into(),
    ])?;
    let bank = repo.join("memory-bank");
    sources::private_mkdir(&bank)?;
    let rendered = agent_memory::bank_contract::render(&object([
        ("project-id", string(PROJECT)),
        ("router-updated", string("2026-10-22T18:00:00Z")),
        (
            "contexts",
            object([(
                "activeContext.md",
                object([
                    ("body", string("# Focus\n\nDisposable source contract.\n")),
                    ("purpose", string("Current focus")),
                    ("scope", string("repository")),
                    ("lifecycle", string("active")),
                    ("read-when", string("When resuming")),
                ]),
            )]),
        ),
        (
            "omitted",
            object(
                [
                    "projectbrief.md",
                    "productContext.md",
                    "systemPatterns.md",
                    "techContext.md",
                    "progress.md",
                ]
                .map(|key| (key, string("No distinct fixture source"))),
            ),
        ),
    ]))?;
    for (name, content) in get(&rendered, "files").as_object().into_iter().flatten() {
        let content = content
            .as_str()
            .ok_or_else(|| agent_memory::error::ContractError::new("fixture bank body"))?;
        std::fs::write(bank.join(name), content)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    }
    run(&[
        std::env::current_exe()
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?
            .into_os_string(),
        "--exact".into(),
        name.into(),
        "--include-ignored".into(),
        "--nocapture".into(),
    ])?;
    Ok(())
}
fn bind_repository(repo: &Path) -> Result<()> {
    let developer = agent_memory::developer_identity::preview(
        "create",
        None,
        &Value::Array(vec![string("Disposable source fixture")]),
        &Value::Array(vec![]),
    )?;
    agent_memory::developer_identity::apply(&developer)?;
    let binding = agent_memory::scope_binding::preview(
        repo,
        "bind",
        None,
        "Explicit disposable source fixture",
        None,
    )?;
    agent_memory::scope_binding::apply(&binding)?;
    Ok(())
}
fn registered(
    repo: &Path,
    runtime: &Path,
    session: &str,
    lifecycle: &str,
    body: &[u8],
) -> Result<Value> {
    sources::register(
        repo,
        "codex",
        session,
        lifecycle,
        &sha256_hex(body),
        None,
        None,
        None,
        runtime,
    )
}
fn acknowledge(
    repo: &Path,
    runtime: &Path,
    session: &str,
    lifecycle: &str,
    source: &Value,
) -> Result<Value> {
    let id = sources::bounded(get(source, "observation_id"), 512)?;
    let digest = sources::bounded(get(source, "source_sha256"), 512)?;
    let value = object([
        ("observation_id", string(id)),
        ("source_sha256", string(digest)),
        ("outcome", string("no-learning")),
        (
            "assessment",
            object([
                ("source_id", string(id)),
                (
                    "source_review",
                    string("Reviewed the actual temporary fixture input; no durable learning."),
                ),
                ("candidates", Value::Array(vec![])),
            ]),
        ),
    ]);
    let verified = sources::verify_assessment(repo, &value, id, runtime)?;
    sources::complete(
        repo, "codex", session, lifecycle, id, digest, &verified, runtime,
    )
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_distinct_and_identical_submissions_and_idempotent_ack
#[test]
#[ignore = "needs security, recovery_frontier, installation and their real dependency providers registered"]
fn distinct_identical_callbacks_idempotent_ack_and_late_steering_backlog() -> Result<()> {
    in_repository(
        "distinct_identical_callbacks_idempotent_ack_and_late_steering_backlog",
        |repo, runtime| {
            bind_repository(repo)?;
            let a = registered(repo, runtime, "session", "lifecycle", b"actual request")?;
            let b = registered(repo, runtime, "session", "lifecycle", b"steering")?;
            let c = registered(repo, runtime, "session", "lifecycle", b"actual request")?;
            assert_ne!(get(&a, "observation_id"), get(&b, "observation_id"));
            assert_ne!(get(&a, "observation_id"), get(&c, "observation_id"));
            acknowledge(repo, runtime, "session", "lifecycle", &a)?;
            assert_eq!(
                array_len(
                    &sources::inspect(repo, "codex", "session", "lifecycle", runtime)?,
                    "pending"
                ),
                2
            );
            assert_eq!(
                get(
                    &acknowledge(repo, runtime, "session", "lifecycle", &a)?,
                    "already_completed"
                ),
                &Value::Bool(true)
            );
            acknowledge(repo, runtime, "session", "lifecycle", &b)?;
            acknowledge(repo, runtime, "session", "lifecycle", &c)?;
            assert_eq!(
                get(
                    &sources::inspect(repo, "codex", "session", "lifecycle", runtime)?,
                    "complete"
                ),
                &Value::Bool(true)
            );
            assert_eq!(
                array_len(&sources::backlog(repo, None, 0, 0, 0, runtime)?, "turns"),
                0
            );
            let late = registered(repo, runtime, "session", "lifecycle", b"late steering")?;
            let backlog = sources::backlog(repo, None, 0, 0, 0, runtime)?;
            let turn = get(&backlog, "turns")
                .as_array()
                .and_then(|v| v.first())
                .ok_or_else(|| agent_memory::error::ContractError::new("late source missing"))?;
            let pending = get(turn, "pending")
                .as_array()
                .and_then(|v| v.first())
                .ok_or_else(|| {
                    agent_memory::error::ContractError::new("late observation missing")
                })?;
            assert_eq!(get(pending, "observation_id"), get(&late, "observation_id"));
            Ok(())
        },
    )
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_encoded_completion_bound_and_wake_cap
#[test]
#[ignore = "needs security, recovery_frontier, installation and their real dependency providers registered"]
fn wake_two_reservations_exact_fingerprint_and_assessment_bound() -> Result<()> {
    in_repository(
        "wake_two_reservations_exact_fingerprint_and_assessment_bound",
        |repo, runtime| {
            bind_repository(repo)?;
            let first = registered(repo, runtime, "session", "lifecycle", b"actual request")?;
            for (position, expected) in [(0, true), (1, true), (2, false)] {
                if position > 0 {
                    registered(
                        repo,
                        runtime,
                        "session",
                        "lifecycle",
                        format!("steering {position}").as_bytes(),
                    )?;
                }
                let state = sources::inspect(repo, "codex", "session", "lifecycle", runtime)?;
                let fingerprint = sources::bounded(get(&state, "fingerprint"), 512)?;
                assert_eq!(
                    sources::reserve_wake(
                        repo,
                        "codex",
                        "session",
                        "lifecycle",
                        fingerprint,
                        &"a".repeat(64),
                        runtime
                    )?,
                    expected
                );
                assert!(!sources::reserve_wake(
                    repo,
                    "codex",
                    "session",
                    "lifecycle",
                    fingerprint,
                    &"a".repeat(64),
                    runtime
                )?);
            }
            let value = object([
                ("observation_id", get(&first, "observation_id").clone()),
                ("source_sha256", get(&first, "source_sha256").clone()),
                ("outcome", string("no-learning")),
                (
                    "assessment",
                    object([
                        ("source_id", get(&first, "observation_id").clone()),
                        ("source_review", string("x".repeat(65000))),
                        ("candidates", Value::Array(vec![])),
                    ]),
                ),
            ]);
            assert!(
                sources::verify_assessment(
                    repo,
                    &value,
                    sources::bounded(get(&first, "observation_id"), 512)?,
                    runtime
                )
                .is_err()
            );
            Ok(())
        },
    )
}

// obligation: tests/contracts/test_turn_sources.py::Sources.test_caps_fault_and_new_turn_backlog
#[test]
#[ignore = "needs security, recovery_frontier, installation and their real dependency providers registered"]
fn full_source_capacity_fault_survives_ack_and_other_turn_enrolls() -> Result<()> {
    in_repository(
        "full_source_capacity_fault_survives_ack_and_other_turn_enrolls",
        |repo, runtime| {
            bind_repository(repo)?;
            let mut observations = Vec::new();
            for _ in 0..sources::MAX_SOURCES {
                observations.push(registered(
                    repo,
                    runtime,
                    "session",
                    "lifecycle",
                    b"actual request",
                )?);
            }
            assert!(registered(repo, runtime, "session", "lifecycle", b"not accepted").is_err());
            for observed in &observations {
                acknowledge(repo, runtime, "session", "lifecycle", observed)?;
            }
            let state = sources::inspect(repo, "codex", "session", "lifecycle", runtime)?;
            assert_eq!(get(&state, "complete"), &Value::Bool(false));
            assert!(has_fault(&state, "registration-fault-retained"));
            registered(
                repo,
                runtime,
                "other-session",
                "other-lifecycle",
                b"actual other request",
            )?;
            let backlog = sources::backlog(repo, None, 0, 0, 0, runtime)?;
            assert_eq!(array_len(&backlog, "turns"), 2);
            Ok(())
        },
    )
}

#[test]
#[ignore = "needs security, recovery_frontier, installation and their real dependency providers registered"]
fn published_completion_without_real_gate_is_typed_pending_not_success() -> Result<()> {
    in_repository(
        "published_completion_without_real_gate_is_typed_pending_not_success",
        |repo, runtime| {
            bind_repository(repo)?;
            let source = registered(
                repo,
                runtime,
                "session",
                "lifecycle",
                b"actual publication candidate",
            )?;
            let value = object([
                ("observation_id", get(&source, "observation_id").clone()),
                ("source_sha256", get(&source, "source_sha256").clone()),
                ("outcome", string("published")),
                (
                    "assessment",
                    string("Actual source still requires publication authority."),
                ),
                ("publication_request", Value::Null),
            ]);
            let outcome = sources::verify_assessment(
                repo,
                &value,
                sources::bounded(get(&source, "observation_id"), 512)?,
                runtime,
            );
            // The real publication request check refuses the absent request.
            assert!(outcome.is_err());
            let state = sources::inspect(repo, "codex", "session", "lifecycle", runtime)?;
            assert_eq!(array_len(&state, "pending"), 1);
            assert_eq!(array_len(&state, "completed"), 0);
            Ok(())
        },
    )
}

#[test]
fn missing_completion_namespace_refuses_without_creating_state() -> Result<()> {
    let w = World::new()?;
    for directory in [w.path().join("missing"), w.path().join("absent/nested")] {
        let before = sources::inventory(&directory, &identity())?;
        let called = std::cell::Cell::new(false);
        let outcome = sources::locked(&directory, || {
            called.set(true);
            Ok(())
        });
        assert!(outcome.is_err());
        assert!(!called.get());
        assert!(!directory.exists());
        assert!(!directory.join("completion.lock").exists());
        assert_eq!(sources::inventory(&directory, &identity())?, before);
    }
    assert!(!w.path().join("absent").exists());
    let actual = w.path().join("actual");
    sources::private_mkdir(&actual)?;
    let alias = w.path().join("alias");
    std::os::unix::fs::symlink(&actual, &alias)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert!(sources::locked(&alias, || Ok(())).is_err());
    assert!(!actual.join("completion.lock").exists());
    sources::locked(&actual, || Ok(()))?;
    assert!(actual.join("completion.lock").exists());
    Ok(())
}

#[test]
fn disappeared_source_and_parent_are_absent_without_fault_records() -> Result<()> {
    let w = World::new()?;
    let directory = w.path().join("state");
    let path = directory.join("source.json");
    assert_eq!(sources::read_bytes(&path, sources::MAX_SOURCE)?, None);
    sources::private_mkdir(&directory)?;
    let value = object([("source", string("actual retained input"))]);
    assert!(sources::exclusive(&path, &value, sources::MAX_SOURCE)?);
    assert_eq!(sources::read(&path, sources::MAX_SOURCE)?, Some(value));
    std::fs::remove_file(&path)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert_eq!(sources::read_bytes(&path, sources::MAX_SOURCE)?, None);
    assert_eq!(sources::read(&path, sources::MAX_SOURCE)?, None);
    assert!(!directory.join("fault.json").exists());
    std::fs::remove_dir(&directory)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    assert_eq!(sources::read_bytes(&path, sources::MAX_SOURCE)?, None);
    assert!(!directory.exists());
    Ok(())
}

#[test]
fn retained_input_surplus_base64_padding_refuses_without_rewriting_bytes() -> Result<()> {
    let w = World::new()?;
    let target = w.path().join("unidentified-inputs");
    sources::private_mkdir(&target)?;
    let path = target.join(format!("{SECOND}.json"));
    let mut value = identity();
    for (key, item) in [
        ("schema", string(sources::UNIDENTIFIED_SCHEMA)),
        ("retention_id", string(SECOND)),
        ("source_reference", Value::Null),
        ("source_base64", string("YWN0dWFs")),
        (
            "source_sha256",
            string("e5c6fde86910ded72db5cc7afc32f850440d4ef7caa5dbb69f5bdc0d3e39cb3b"),
        ),
        ("context_digest", string("b".repeat(64))),
        ("uncertainty", string("native-event-identity-unavailable")),
    ] {
        set(&mut value, key, item);
    }
    assert!(sources::exclusive(
        &path,
        &value,
        sources::MAX_RETAINED_INPUT
    )?);
    assert_eq!(
        sources::unidentified_inventory(w.path(), &identity())?.len(),
        1
    );
    for base64 in ["YWN0dWFs=", "YWN0dWFs====", "YWN0dWFs========"] {
        set(&mut value, "source_base64", string(base64));
        let raw = sources::encoded(&value)?;
        std::fs::write(&path, &raw)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
        assert!(sources::unidentified_inventory(w.path(), &identity()).is_err());
        let state = sources::inventory(w.path(), &identity())?;
        assert!(has_fault(&state, "unidentified-input-unverifiable"));
        assert_eq!(get(&state, "complete"), &Value::Bool(false));
        assert_eq!(
            std::fs::read(&path)
                .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?,
            raw
        );
    }
    Ok(())
}

#[test]
fn source_json_reader_preserves_baseline_nonfinite_number_policy() -> Result<()> {
    let w = World::new()?;
    let path = w.path().join("numbers.json");
    let raw = br#"{"overflow":1e400,"negative":-Infinity,"positive":Infinity,"nan":NaN}"#;
    std::fs::write(&path, raw)
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?;
    let value = sources::read(&path, sources::MAX_SOURCE)?
        .ok_or_else(|| agent_memory::error::ContractError::new("number fixture missing"))?;
    assert!(matches!(get(&value, "nan"), Value::Float(value) if value.is_nan()));
    assert_eq!(
        sources::encoded(&value)?,
        br#"{"nan":NaN,"negative":-Infinity,"overflow":Infinity,"positive":Infinity}"#
    );
    assert_eq!(
        std::fs::read(&path).map_err(|e| agent_memory::error::ContractError::new(e.to_string()))?,
        raw
    );
    Ok(())
}
