//! Remaining native-source contracts, using real installed ownership and private World fixtures.
mod support;

use agent_memory::codec::json::{self, Value};
use agent_memory::omp_stream::Responder;
use agent_memory::{
    omp_boundary as boundary, omp_installation, omp_stream as stream, omp_transport as transport,
    turn_sources,
};
use std::ffi::OsString;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use support::{RunOptions, World};

const SESSION: &str = "native-session";
const LIFE: &str = "11111111-2222-4333-8444-555555555555";

fn decode(raw: &str) -> Value {
    json::decode(raw, Default::default()).unwrap()
}
fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap_or(&Value::Null)
}
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    get(v, k).as_str().unwrap()
}
fn string(v: &str) -> Value {
    Value::String(v.into())
}
fn set(v: &mut Value, k: &str, value: Value) {
    let Value::Object(pairs) = v else {
        panic!("fixture object required")
    };
    if let Some((_, old)) = pairs.iter_mut().find(|(key, _)| key == k) {
        *old = value;
    } else {
        pairs.push((k.into(), value));
    }
}
fn write(path: &Path, raw: impl AsRef<[u8]>, mode: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, raw).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}
fn isolated(name: &str, body: impl FnOnce(&Path)) {
    if let Ok(root) = support::require_current_world() {
        agent_memory::cli::register_providers().unwrap();
        body(&root);
        return;
    }
    let world = World::new().unwrap();
    let output = world
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                name.into(),
                "--include-ignored".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                cwd: Some(&world.root),
                check: false,
                timeout: Duration::from_secs(120),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn command(root: &Path, args: &[OsString]) -> agent_memory::process::Output {
    let env: Vec<_> = std::env::vars_os().collect();
    let spec = agent_memory::process::Spec {
        argv: args,
        env: &env,
        cwd: Some(root),
        search_path: true,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture {
            limit: 8 * 1024 * 1024,
        },
        stderr: agent_memory::process::Target::Capture {
            limit: 8 * 1024 * 1024,
        },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(90)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    let out = agent_memory::process::run_for(&spec, Duration::from_secs(90)).unwrap();
    assert_eq!(
        out.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
fn pack(root: &Path) -> PathBuf {
    let source = root.join("development-source");
    let pack = source.join("skills/agent-memory");
    std::fs::create_dir_all(pack.parent().unwrap()).unwrap();
    command(
        root,
        &[
            "cp".into(),
            "-R".into(),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("skills/agent-memory")
                .into_os_string(),
            pack.into_os_string(),
        ],
    );
    write(
        &source.join("target/release/am"),
        std::fs::read(env!("CARGO_BIN_EXE_am")).unwrap(),
        0o755,
    );
    source
}
struct Fixture {
    repo: PathBuf,
    runtime: PathBuf,
    receipt: Value,
    proof: stream::FrameProof,
}
impl Fixture {
    fn new(root: &Path) -> Self {
        let (runtime, receipt, proof) = install_only(root, "copy");
        let repo = bank_fixture(root);
        boundary::activate_marker(&repo, false, &runtime).unwrap();
        Self {
            repo,
            runtime,
            receipt,
            proof,
        }
    }
    fn site(&self) -> transport::Site<'_> {
        transport::Site::new(&self.repo, &self.runtime)
    }
    fn observe(&self, request: &Value) -> Value {
        transport::observe(
            &self.repo,
            &self.runtime,
            request,
            transport::Observation {
                receipt: Some(&self.receipt),
                ..Default::default()
            },
            &transport::InstalledScope {
                receipt: Some(&self.receipt),
                proof: &self.proof,
                runtime_root: &self.runtime,
            },
        )
        .unwrap()
    }
    fn pending(&self) -> Vec<Value> {
        let (rows, faults) = transport::pending_inventory(&self.site()).unwrap();
        assert!(faults.is_empty(), "{faults:?}");
        rows
    }
}
fn entry(id: &str, role: &str, content: Value) -> Value {
    Value::Object(vec![
        ("type".into(), string("message")),
        ("id".into(), string(id)),
        ("parentId".into(), Value::Null),
        (
            "message".into(),
            Value::Object(vec![
                ("role".into(), string(role)),
                (
                    "attribution".into(),
                    string(if role == "user" { "user" } else { "agent" }),
                ),
                ("content".into(), content),
            ]),
        ),
    ])
}
fn human(id: &str) -> Value {
    entry(id, "user", string("Only ephemeral arithmetic."))
}
fn answer(id: &str) -> Value {
    entry(
        id,
        "assistant",
        string("The substantive ephemeral answer is delivered."),
    )
}
fn request(f: &Fixture, entries: &[Value], action: &str) -> Value {
    let mut entries = entries.to_vec();
    for i in 1..entries.len() {
        let parent = get(&entries[i - 1], "id").clone();
        if get(&entries[i], "parentId") == &Value::Null {
            set(&mut entries[i], "parentId", parent);
        }
    }
    let branch = entries.iter().map(|e| get(e, "id").clone()).collect();
    let parents = entries
        .iter()
        .map(|e| {
            Value::Object(vec![
                ("id".into(), get(e, "id").clone()),
                ("parent_id".into(), get(e, "parentId").clone()),
                ("timestamp".into(), Value::Null),
            ])
        })
        .collect();
    let leaf = entries
        .last()
        .map(|e| get(e, "id").clone())
        .unwrap_or(Value::Null);
    let mut view = decode(
        r#"{"session":"native-session","cwd":"","file":null,"header":{"id":"native-session","cwd":"","version":3,"parent_session":null,"timestamp":null},"leaf_id":null,"native_parents":[],"source_kind":"session-manager","tail_status":"available","fork_status":"none"}"#,
    );
    set(&mut view, "cwd", string(f.repo.to_str().unwrap()));
    set(
        view.get_mut("header").unwrap(),
        "cwd",
        string(f.repo.to_str().unwrap()),
    );
    set(&mut view, "leaf_id", leaf);
    set(&mut view, "native_parents", Value::Array(parents));
    let mut payload = Value::Object(vec![
        ("entries".into(), Value::Array(entries.clone())),
        ("branch_ids".into(), Value::Array(branch)),
        ("event_ordinal".into(), Value::Int(entries.len().into())),
        ("terminal".into(), Value::Bool(false)),
        ("trigger".into(), string("resume")),
        ("inherited_ids".into(), Value::Array(vec![])),
        ("view".into(), view),
    ]);
    if action == "seal" {
        set(&mut payload, "request_wake", Value::Bool(false));
    }
    Value::Object(vec![
        (
            "schema".into(),
            string("agent-memory/omp-observer-request-v2"),
        ),
        ("action".into(), string(action)),
        ("session".into(), string(SESSION)),
        ("lifecycle".into(), string(LIFE)),
        ("payload".into(), payload),
    ])
}

// obligation: tests/contracts/test_omp_source_phase.py::HeldLockObservations.test_actual_descriptor_identity_and_closed_observation
#[test]
fn actual_descriptor_identity() {
    let world = World::new().unwrap();
    let path = world.root.join("held/lock");
    let held = turn_sources::private_lock(&path, false, false).unwrap();
    held.recheck().unwrap();
    std::fs::rename(&path, path.with_extension("old")).unwrap();
    write(&path, b"", 0o600);
    assert!(
        held.recheck()
            .unwrap_err()
            .to_string()
            .contains("lock changed")
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(path.with_extension("old"), &path).unwrap();
    held.finish().unwrap();
    turn_sources::private_lock(&path, false, true)
        .unwrap()
        .finish()
        .unwrap();
}
// obligation: tests/contracts/test_omp_source_phase.py::HeldLockObservations.test_parent_checks_only_existing_private_lock_mode
#[test]
fn parent_checks_only_existing_private_lock_mode() {
    for existing in [false, true] {
        let world = World::new().unwrap();
        let parent = world.root.join("parent");
        let old = world.root.join("old-parent");
        let path = parent.join("lock");
        write(&path, b"", 0o600);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let held = turn_sources::private_lock(&path, false, existing).unwrap();
        std::fs::rename(&parent, &old).unwrap();
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::hard_link(old.join("lock"), &path).unwrap();
        if existing {
            assert!(
                held.recheck()
                    .unwrap_err()
                    .to_string()
                    .contains("parent changed")
            );
        } else {
            held.recheck().unwrap();
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&parent).unwrap();
        std::fs::rename(old, parent).unwrap();
        held.finish().unwrap();
    }
}
// obligation: tests/contracts/test_omp_source_phase.py::HeldLockObservations.test_nested_barrier_exposes_actual_executor_held_observation
#[test]
fn nested_barrier_exposes_same_held_inode() {
    let world = World::new().unwrap();
    let base = world.root.join("held");
    let outer = turn_sources::source_barrier_at(&base, false, false).unwrap();
    let inner = turn_sources::source_barrier_at(&base, false, false).unwrap();
    inner.recheck().unwrap();
    drop(inner);
    outer.recheck().unwrap();
    assert!(turn_sources::private_lock(&base.join("source-barrier.lock"), false, true).is_err());
    drop(outer);
    turn_sources::private_lock(&base.join("source-barrier.lock"), false, true)
        .unwrap()
        .finish()
        .unwrap();
}
// obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_foreign_thread_source_observation_without_fallback
#[test]
fn foreign_thread_cannot_borrow_source_observation() {
    let world = World::new().unwrap();
    let base = world.root.join("held");
    let owner = turn_sources::source_barrier_at(&base, false, false).unwrap();
    std::thread::scope(|s| {
        s.spawn(|| {
            assert!(owner.borrowed(|| Ok(())).is_err());
            assert!(owner.recheck().is_err());
        })
        .join()
        .unwrap();
    });
    owner.recheck().unwrap();
    assert!(turn_sources::private_lock(&base.join("source-barrier.lock"), false, true).is_err());
}
// obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_entry_refuses_actual_source_inode_swap
#[test]
fn borrowed_entry_rejects_actual_inode_swap() {
    let world = World::new().unwrap();
    let base = world.root.join("held");
    let owner = turn_sources::source_barrier_at(&base, false, false).unwrap();
    let path = base.join("source-barrier.lock");
    std::fs::rename(&path, path.with_extension("old")).unwrap();
    write(&path, b"", 0o600);
    let mut entered = false;
    assert!(
        owner
            .borrowed(|| {
                entered = true;
                Ok(())
            })
            .is_err()
    );
    assert!(!entered);
}
// obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_exit_refuses_actual_source_inode_swap
#[test]
fn borrowed_exit_rejects_actual_inode_swap() {
    let world = World::new().unwrap();
    let base = world.root.join("held");
    let owner = turn_sources::source_barrier_at(&base, false, false).unwrap();
    let path = base.join("source-barrier.lock");
    let result = owner.borrowed(|| {
        std::fs::rename(&path, path.with_extension("old")).unwrap();
        write(&path, b"", 0o600);
        Ok(())
    });
    assert!(result.is_err());
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checked_stored_user_is_sole_genuine_root_and_mapping_is_not_required
#[test]
#[ignore = "recovery_frontier"]
fn checked_stored_user_is_sole_genuine_root() {
    isolated("checked_stored_user_is_sole_genuine_root", |root| {
        let f = Fixture::new(root);
        let result = f.observe(&request(
            &f,
            &[human("actual-human"), answer("actual-answer")],
            "seal",
        ));
        assert_eq!(get(&result, "verified"), &Value::Bool(true));
        let segments = transport::segments(&f.site(), SESSION).unwrap();
        assert_eq!(segments.len(), 1);
        let cursor = segments[0].snapshot().unwrap();
        assert_eq!(
            text(get(&cursor, "origin"), "human_root"),
            transport::uuid5_url(&format!("{}:omp:{SESSION}:actual-human", f.repo.display()))
        );
        assert_eq!(
            text(get(&cursor, "control_interval"), "human_entry"),
            "actual-human"
        );
        assert!(!segments[0].directory.join("admissions").exists());
        let manifest = transport::manifest_value(&segments[0].directory, &cursor).unwrap();
        assert_eq!(get(&manifest, "gaps"), &Value::Array(vec![]));
        let rows = f.pending();
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| get(row, "observation_id").as_str().is_some())
        );
    });
}

fn acknowledge(f: &Fixture, row: &Value) -> Value {
    let mut value = Value::Object(
        ["session", "lifecycle", "observation_id", "source_sha256"]
            .into_iter()
            .map(|k| (k.into(), get(row, k).clone()))
            .collect(),
    );
    set(&mut value, "outcome", string("no-learning"));
    set(
        &mut value,
        "assessment",
        Value::Object(vec![
            ("source_id".into(), get(row, "observation_id").clone()),
            (
                "source_review".into(),
                string(
                    "Reviewed the exact original ephemeral source and manifest; no durable project learning.",
                ),
            ),
            ("candidates".into(), Value::Array(vec![])),
        ]),
    );
    transport::complete(&f.repo, &f.runtime, &value).unwrap()
}

// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_long_segment_uses_two_shared_observations_and_ingest_is_idempotent
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_public_observer_seal_and_no_learning_long_segment
#[test]
#[ignore = "recovery_frontier"]
fn long_segment_registers_two_exact_sources_and_replays_idempotently() {
    isolated(
        "long_segment_registers_two_exact_sources_and_replays_idempotently",
        |root| {
            let f = Fixture::new(root);
            let mut entries = vec![human("human")];
            entries.extend((0..150).map(|i| answer(&format!("answer-{i}"))));
            let value = request(&f, &entries, "seal");
            assert_eq!(get(&f.observe(&value), "verified"), &Value::Bool(true));
            let before = f.pending();
            assert_eq!(before.len(), 2);
            assert_eq!(get(&f.observe(&value), "verified"), &Value::Bool(true));
            let after = f.pending();
            assert_eq!(
                before
                    .iter()
                    .map(|r| get(r, "observation_id"))
                    .collect::<Vec<_>>(),
                after
                    .iter()
                    .map(|r| get(r, "observation_id"))
                    .collect::<Vec<_>>()
            );
            for row in after {
                let source = transport::source(
                    &f.repo,
                    &f.runtime,
                    text(&row, "session"),
                    text(&row, "lifecycle"),
                    text(&row, "observation_id"),
                )
                .unwrap();
                assert_eq!(get(&source, "verified"), &Value::Bool(true));
                assert_eq!(get(&source, "coverage_complete"), &Value::Bool(true));
                assert_eq!(get(&acknowledge(&f, &row), "verified"), &Value::Bool(true));
            }
            assert_eq!(
                get(
                    &transport::list_pending(&f.repo, &f.runtime, 0).unwrap(),
                    "pending"
                ),
                &Value::Array(vec![])
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_admission_requires_sealed_registered_segment
#[test]
#[ignore = "recovery_frontier"]
fn admission_cannot_complete_an_open_segment() {
    isolated("admission_cannot_complete_an_open_segment", |root| {
        let f = Fixture::new(root);
        f.observe(&request(&f, &[human("human")], "ingest"));
        let rows = f.pending();
        let row = rows
            .iter()
            .find(|r| get(r, "observation_id").as_str().is_some())
            .unwrap();
        let mut value = Value::Object(
            ["session", "lifecycle", "observation_id", "source_sha256"]
                .into_iter()
                .map(|k| (k.into(), get(row, k).clone()))
                .collect(),
        );
        set(&mut value, "outcome", string("no-learning"));
        set(
            &mut value,
            "assessment",
            Value::Object(vec![
                ("source_id".into(), get(row, "observation_id").clone()),
                ("source_review".into(), string("Reviewed exact source.")),
                ("candidates".into(), Value::Array(vec![])),
            ]),
        );
        assert!(transport::complete(&f.repo, &f.runtime, &value).is_err());
        assert_eq!(f.pending(), rows);
    });
}

// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_gap_refuses_both_assessment_outcomes_and_preserves_marker
#[test]
#[ignore = "recovery_frontier"]
fn gapped_source_refuses_both_assessment_outcomes() {
    isolated("gapped_source_refuses_both_assessment_outcomes", |root| {
        let f = Fixture::new(root);
        let unsafe_entry = entry(
            "unsafe",
            "assistant",
            string("password=synthetic-secret-value"),
        );
        f.observe(&request(&f, &[human("human"), unsafe_entry], "seal"));
        let base = boundary::base(&f.repo).unwrap();
        let marker = std::fs::read(base.join("turn-sources-compat-v1.json")).unwrap();
        for row in f
            .pending()
            .iter()
            .filter(|r| get(r, "observation_id").as_str().is_some())
        {
            for outcome in ["no-learning", "learning"] {
                let mut value = Value::Object(
                    ["session", "lifecycle", "observation_id", "source_sha256"]
                        .into_iter()
                        .map(|k| (k.into(), get(row, k).clone()))
                        .collect(),
                );
                set(&mut value, "outcome", string(outcome));
                set(
                    &mut value,
                    "assessment",
                    Value::Object(vec![
                        ("source_id".into(), get(row, "observation_id").clone()),
                        (
                            "source_review".into(),
                            string("Reviewed original ephemeral source."),
                        ),
                        ("candidates".into(), Value::Array(vec![])),
                    ]),
                );
                let error = transport::complete(&f.repo, &f.runtime, &value).unwrap_err();
                assert!(
                    error.to_string().contains("unresolved coverage gaps"),
                    "{error}"
                );
            }
        }
        assert_eq!(
            std::fs::read(base.join("turn-sources-compat-v1.json")).unwrap(),
            marker
        );
    });
}

// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_manual_marker_delete_cannot_hide_unregistered_source
#[test]
#[ignore = "recovery_frontier"]
fn manual_marker_delete_cannot_hide_retained_source() {
    isolated("manual_marker_delete_cannot_hide_retained_source", |root| {
        let f = Fixture::new(root);
        f.observe(&request(&f, &[human("human"), answer("answer")], "seal"));
        let base = boundary::base(&f.repo).unwrap();
        std::fs::remove_file(base.join("turn-sources-compat-v1.json")).unwrap();
        assert!(boundary::required_source_capabilities(&f.repo, &f.runtime).is_err());
        assert!(!f.pending().is_empty());
        assert!(boundary::verify_marker(&f.repo, &f.runtime).is_err());
    });
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_notification_without_stored_human_has_no_origin_or_source_gap
#[test]
#[ignore = "recovery_frontier"]
fn notifications_never_create_native_human_authority() {
    isolated(
        "notifications_never_create_native_human_authority",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[], "ingest"));
            assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
            assert!(f.pending().is_empty());
            let mut forged = request(&f, &[], "ingest");
            set(
                &mut forged,
                "schema",
                string("agent-memory/omp-observer-request-v1"),
            );
            set(&mut forged, "action", string("admit"));
            let scope = transport::InstalledScope {
                receipt: Some(&f.receipt),
                proof: &f.proof,
                runtime_root: &f.runtime,
            };
            assert!(
                transport::observe(&f.repo, &f.runtime, &forged, Default::default(), &scope)
                    .is_err()
            );
            assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_on_started_runs_once_after_preflight_before_source_write
#[test]
#[ignore = "recovery_frontier"]
fn on_started_runs_once_before_native_source_write() {
    isolated("on_started_runs_once_before_native_source_write", |root| {
        let f = Fixture::new(root);
        let calls = std::cell::Cell::new(0);
        let mut started = || {
            calls.set(calls.get() + 1);
            assert!(transport::segments(&f.site(), SESSION)?.is_empty());
            Ok(())
        };
        let result = transport::observe(
            &f.repo,
            &f.runtime,
            &request(&f, &[human("human")], "seal"),
            transport::Observation {
                receipt: Some(&f.receipt),
                on_started: Some(&mut started),
                ..Default::default()
            },
            &transport::InstalledScope {
                receipt: Some(&f.receipt),
                proof: &f.proof,
                runtime_root: &f.runtime,
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(get(&result, "verified"), &Value::Bool(true));
        assert!(!f.pending().is_empty());
    });
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_legacy_notification_only_cursor_is_neither_reused_mutated_nor_sealed
#[test]
#[ignore = "recovery_frontier"]
fn legacy_notification_cursor_stays_immutable_and_unsealed() {
    isolated(
        "legacy_notification_cursor_stays_immutable_and_unsealed",
        |root| {
            let f = Fixture::new(root);
            let old = transport::new_segment(&f.site(), SESSION, LIFE, LIFE, "human", 50).unwrap();
            let original = std::fs::read(old.directory.join("cursor.json")).unwrap();
            f.observe(&request(&f, &[human("actual-native-human")], "seal"));
            assert_eq!(
                std::fs::read(old.directory.join("cursor.json")).unwrap(),
                original
            );
            assert!(!old.directory.join("segment.json").exists());
            let current = transport::segments(&f.site(), SESSION).unwrap();
            let selected =
                transport::active_segment(&current, Some(&["actual-native-human".into()]))
                    .unwrap()
                    .unwrap();
            assert_ne!(text(&selected.snapshot().unwrap(), "lifecycle"), LIFE);
            assert!(
                f.pending()
                    .iter()
                    .any(|r| text(r, "status") == "legacy-unverified-metadata")
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_neutral_hint_keeps_sealed_parent_immutable
#[test]
#[ignore = "recovery_frontier"]
fn neutral_hint_never_reopens_sealed_parent() {
    isolated("neutral_hint_never_reopens_sealed_parent", |root| {
        let f = Fixture::new(root);
        f.observe(&request(&f, &[human("human")], "seal"));
        let old = transport::segments(&f.site(), SESSION).unwrap().remove(0);
        let original = std::fs::read(old.directory.join("segment.json")).unwrap();
        f.observe(&request(&f, &[human("human"), answer("answer")], "seal"));
        assert_eq!(
            std::fs::read(old.directory.join("segment.json")).unwrap(),
            original
        );
        assert_eq!(transport::segments(&f.site(), SESSION).unwrap().len(), 2);
    });
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_missing_or_stale_receipt_is_not_a_production_fallback
#[test]
#[ignore = "recovery_frontier"]
fn missing_or_stale_receipt_never_falls_back_to_marker() {
    isolated(
        "missing_or_stale_receipt_never_falls_back_to_marker",
        |root| {
            let f = Fixture::new(root);
            let mut stale = f.receipt.clone();
            set(
                &mut stale,
                "installation_id",
                string("0".repeat(32).as_str()),
            );
            for receipt in [None, Some(&stale)] {
                let scope = transport::InstalledScope {
                    receipt,
                    proof: &f.proof,
                    runtime_root: &f.runtime,
                };
                assert!(
                    transport::observe(
                        &f.repo,
                        &f.runtime,
                        &request(&f, &[human("human")], "seal"),
                        transport::Observation {
                            receipt,
                            ..Default::default()
                        },
                        &scope
                    )
                    .is_err()
                );
                assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
            }
        },
    );
}

// obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_removed_public_observer_flag_refuses_before_any_source_write
#[test]
fn removed_public_observer_flag_refuses_before_source_write() {
    let world = World::new().unwrap();
    let output = world
        .am_with(
            ["omp-boundary", "--observer", "--stdin"],
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(output.status, agent_memory::process::Status::Code(0));
    assert!(!world.root.join("state/agent-memory/writes").exists());
}

// Installed source/transport contention regression.
#[test]
#[ignore = "recovery_frontier"]
fn contended_source_and_transport_locks_refuse_before_started() {
    isolated(
        "contended_source_and_transport_locks_refuse_before_started",
        |root| {
            let f = Fixture::new(root);
            let base = boundary::base(&f.repo).unwrap();
            for name in ["source-barrier.lock", "omp-transport.lock"] {
                let held = turn_sources::private_lock(&base.join(name), false, false).unwrap();
                let calls = std::cell::Cell::new(0);
                let mut started = || {
                    calls.set(calls.get() + 1);
                    Ok(())
                };
                let result = transport::observe(
                    &f.repo,
                    &f.runtime,
                    &request(&f, &[human("human")], "ingest"),
                    transport::Observation {
                        receipt: Some(&f.receipt),
                        on_started: Some(&mut started),
                        ..Default::default()
                    },
                    &transport::InstalledScope {
                        receipt: Some(&f.receipt),
                        proof: &f.proof,
                        runtime_root: &f.runtime,
                    },
                );
                assert!(matches!(
                    result.unwrap_err().kind(),
                    agent_memory::error::Kind::SourceTransportBusy { .. }
                ));
                assert_eq!(calls.get(), 0);
                held.finish().unwrap();
                assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
            }
        },
    );
}

// obligation: tests/contracts/test_omp_source_phase.py::LateWindowControls.test_cancel_before_clock_cannot_revive_source_nonce
#[test]
fn cancel_before_clock_cannot_revive_nonce() {
    let world = World::new().unwrap();
    assert!(world.root.is_dir());
    let instance = agent_memory::host::uuid4().unwrap();
    let nonce = agent_memory::host::uuid4().unwrap();
    let owner = stream::ClockWindows::system(&instance, Arc::new(|| true));
    let writer = stream::ResponseWriter::unstarted(&instance, vec![]);
    owner
        .control(
            "CANCEL",
            &Value::Object(vec![("nonce".into(), string(&nonce))]),
            &writer,
        )
        .unwrap();
    let now = agent_memory::clock::monotonic_ns();
    let clock = Value::Object(vec![
        ("nonce".into(), string(&nonce)),
        ("j0".into(), string(&now.to_string())),
        (
            "j_deadline".into(),
            string(&(now + 900_000_000).to_string()),
        ),
    ]);
    assert!(owner.control("CLOCK", &clock, &writer).is_err());
    assert!(owner.window(&nonce).is_none());
    owner.end();
}

fn finite(
    f: &Fixture,
) -> (
    stream::Coverage,
    stream::FiniteExecutor,
    Arc<stream::ResponseWriter>,
) {
    let instance = agent_memory::host::uuid4().unwrap();
    let binding =
        boundary::stream_startup(&f.repo, SESSION, &instance, LIFE, &f.receipt, &f.runtime)
            .unwrap()
            .unwrap();
    let coverage = stream::Coverage::open(
        Arc::new(stream::Installed::new(&f.runtime)),
        &f.repo,
        SESSION,
        &instance,
        LIFE,
        &binding,
        Some(&f.receipt),
    )
    .unwrap();
    let writer = Arc::new(stream::ResponseWriter::unstarted(&instance, vec![]));
    let clocks = stream::ClockWindows::system(&instance, Arc::new(|| true));
    let executor = stream::FiniteExecutor::new(
        writer.clone(),
        clocks,
        Some(f.receipt.clone()),
        None,
        stream::Timing::system(),
    );
    (coverage, executor, writer)
}
fn envelope(request: Value, ordinal: u64) -> Value {
    let payload = get(&request, "payload");
    let view = get(payload, "view");
    let mut natives = get(view, "native_parents")
        .as_array()
        .unwrap()
        .iter()
        .map(|p| text(p, "id").to_owned())
        .collect::<Vec<_>>();
    natives.sort();
    let identity = Value::Object(vec![
        (
            "view_id".into(),
            string(&agent_memory::host::uuid4().unwrap()),
        ),
        ("leaf_id".into(), get(view, "leaf_id").clone()),
        (
            "native_ids".into(),
            Value::Array(natives.iter().map(|s| string(s)).collect()),
        ),
        ("tool_ids".into(), Value::Array(vec![])),
    ]);
    Value::Object(vec![
        ("request".into(), request),
        ("event_ordinal".into(), Value::Int(ordinal.into())),
        ("identity".into(), identity),
    ])
}
fn queued_result(writer: &stream::ResponseWriter) -> Value {
    let frames = writer.queued();
    let raw = &frames
        .iter()
        .find(|(kind, _)| kind == "OBSERVED")
        .unwrap()
        .1;
    let start = raw.iter().position(|b| *b == b'\n').unwrap() + 1;
    let body = json::decode_bytes(&raw[start..], Default::default()).unwrap();
    get(&body, "result").clone()
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_missing_clock_commits_pending_refs_then_registers_after_response
#[test]
#[ignore = "recovery_frontier"]
fn missing_clock_commits_refs_before_postresponse_registration() {
    isolated(
        "missing_clock_commits_refs_before_postresponse_registration",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, executor, writer) = finite(&f);
            let mut value = request(&f, &[human("phase-human"), answer("phase-answer")], "seal");
            set(
                value.get_mut("payload").unwrap(),
                "request_wake",
                Value::Bool(true),
            );
            executor
                .execute(
                    &mut coverage,
                    "REQUEST",
                    1,
                    Some(&agent_memory::host::uuid4().unwrap()),
                    &turn_sources::encoded(&envelope(value, 1)).unwrap(),
                )
                .unwrap();
            let outcome = transport::read(
                &coverage.directory().join("outcomes/1.json"),
                stream::MAX_METADATA,
            )
            .unwrap();
            assert_eq!(text(&outcome, "outcome"), "verified");
            assert_eq!(get(&outcome, "source_references"), &Value::Array(vec![]));
            let result = queued_result(&writer);
            assert_eq!(get(&result, "verified"), &Value::Bool(true));
            assert_eq!(
                get(&result, "source_coverage_complete"),
                &Value::Bool(false)
            );
            assert_eq!(text(&result, "status"), "pending-undeliverable-window");
            assert_eq!(get(&result, "source_references"), &Value::Array(vec![]));
            assert_eq!(
                get(get(&result, "wake_payload"), "enrollment_refs")
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            let rows = f.pending();
            assert_eq!(rows.len(), 2);
            assert!(
                rows.iter()
                    .all(|r| get(r, "observation_id").as_str().is_some())
            );
            assert!(
                !transport::session_dir(&f.site(), SESSION)
                    .unwrap()
                    .join("root-wakes")
                    .exists()
            );
            assert_eq!(
                std::fs::read_dir(coverage.directory().join("outcomes"))
                    .unwrap()
                    .count(),
                1
            );
            coverage.close_live();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_nonarmed_and_direct_observation_keep_synchronous_registration
#[test]
#[ignore = "recovery_frontier"]
fn nonarmed_and_direct_observations_register_synchronously() {
    isolated(
        "nonarmed_and_direct_observations_register_synchronously",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, executor, writer) = finite(&f);
            let value = request(&f, &[human("phase-human"), answer("phase-answer")], "seal");
            executor
                .execute(
                    &mut coverage,
                    "REQUEST",
                    1,
                    None,
                    &turn_sources::encoded(&envelope(value.clone(), 1)).unwrap(),
                )
                .unwrap();
            assert_eq!(
                get(&queued_result(&writer), "source_references")
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(f.pending().len(), 2);
            let direct = f.observe(&value);
            assert_eq!(
                get(&direct, "source_references").as_array().unwrap().len(),
                2
            );
            assert!(
                f.pending()
                    .iter()
                    .all(|r| get(r, "observation_id").as_str().is_some())
            );
            coverage.close_live();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::MetadataBarrierBoundaries.test_exclusively_held_barrier_refuses_close_without_metadata_write
#[test]
#[ignore = "recovery_frontier"]
fn exclusive_barrier_refuses_finite_close_without_metadata_write() {
    isolated(
        "exclusive_barrier_refuses_finite_close_without_metadata_write",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, _, _) = finite(&f);
            let barrier = turn_sources::private_lock(
                &coverage.base().join("source-barrier.lock"),
                false,
                true,
            )
            .unwrap();
            let last = decode(
                r#"{"last_attempt_seq":0,"refused_count":0,"last_event_ordinal":0,"transition":null}"#,
            );
            let error = coverage.close(&last).unwrap_err();
            assert!(matches!(
                error.kind(),
                agent_memory::error::Kind::SourceTransportBusy { .. }
            ));
            assert!(!coverage.directory().join("closed.json").exists());
            barrier.finish().unwrap();
            assert!(coverage.close(&last).unwrap());
            coverage.close_live();
        },
    );
}

fn install_only(root: &Path, mode: &str) -> (PathBuf, Value, stream::FrameProof) {
    let source = pack(root);
    let agent = root.join("omp-agent");
    std::fs::create_dir(&agent).unwrap();
    let args: Vec<OsString> = vec![
        env!("CARGO_BIN_EXE_am").into(),
        "--json".into(),
        "install".into(),
        "--development-source".into(),
        source.into_os_string(),
        "--presets".into(),
        "omp".into(),
        "--omp-agent-dir".into(),
        agent.clone().into_os_string(),
        "--omp-peers-upgraded".into(),
        "--mode".into(),
        mode.into(),
    ];
    let mut check = args.clone();
    check.push("--check".into());
    let output = command(root, &check);
    let preview = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(
        get(&preview, "installation_writes_performed"),
        &Value::Bool(false)
    );
    assert!(!agent.join("extensions/agent-memory.ts").exists());
    let output = command(root, &args);
    let applied = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(get(&applied, "verified"), &Value::Bool(true));
    let receipt = turn_sources::read(
        &root.join("state/agent-memory/install/receipt.json"),
        16 * 1024 * 1024,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        turn_sources::encoded(get(&receipt, "omp_binding")).unwrap(),
        turn_sources::encoded(get(get(&preview, "omp"), "binding")).unwrap()
    );
    for target in get(&preview, "targets").as_array().unwrap() {
        assert!(Path::new(text(target, "path")).exists());
    }
    let version = command(
        root,
        &[
            Path::new(text(&receipt, "bin_dir"))
                .join("am")
                .into_os_string(),
            "--json".into(),
            "version".into(),
        ],
    );
    let version = json::decode_bytes(&version.stdout, Default::default()).unwrap();
    assert_eq!(
        get(get(&version, "executing_source"), "version"),
        get(get(&receipt, "source"), "version")
    );
    let runtime = PathBuf::from(text(get(&receipt, "source"), "pack"));
    install_activation(&receipt);
    let proof = Arc::new(
        omp_installation::verified_observer_binding(
            text(get(&receipt, "source"), "artifact_digest"),
            agent.to_str().unwrap(),
            Some(&receipt),
        )
        .unwrap(),
    );
    (runtime, receipt, proof)
}
fn pair(witness: &omp_installation::OwnedManual) -> (Value, Value) {
    let verified = witness.verify().unwrap().unwrap();
    let body = verified.text.trim_end_matches('\n');
    let count = body.split('\n').count();
    let uri = agent_memory::omp_projection::instruction_uri(witness.relative_path()).unwrap();
    let invocation = decode(&format!(
        r#"{{"type":"message","id":"manual-invocation","parentId":"human","message":{{"role":"assistant","content":[{{"type":"toolCall","id":"manual-call","name":"read","arguments":{{"i":"Reading installed guide","path":{}}}}}]}}}}"#,
        json::encode(&string(&uri), json::DEFAULT).unwrap()
    ));
    let mut result = entry(
        "manual-result",
        "toolResult",
        Value::Array(vec![Value::Object(vec![
            ("type".into(), string("text")),
            (
                "text".into(),
                string(&format!("[Skill file: {}]\n{body}", verified.header_path)),
            ),
        ])]),
    );
    set(&mut result, "parentId", string("manual-invocation"));
    let m = result.get_mut("message").unwrap();
    if let Value::Object(fields) = m {
        fields.retain(|(k, _)| k != "attribution");
    }
    set(m, "toolCallId", string("manual-call"));
    set(m, "toolName", string("read"));
    set(m, "isError", Value::Bool(false));
    set(
        m,
        "details",
        Value::Object(vec![
            ("contentType".into(), string("text/markdown")),
            ("resolvedPath".into(), string(&verified.header_path)),
            ("rowPrefixes".into(), Value::Bool(false)),
            ("totalLines".into(), Value::Int(count.into())),
            (
                "displayContent".into(),
                Value::Object(vec![
                    (
                        "lineNumbers".into(),
                        Value::Array((1..=count).map(|n| Value::Int(n.into())).collect()),
                    ),
                    ("startLine".into(), Value::Int(1u8.into())),
                    ("text".into(), string(body)),
                ]),
            ),
            (
                "meta".into(),
                Value::Object(vec![(
                    "source".into(),
                    Value::Object(vec![
                        ("type".into(), string("internal")),
                        ("value".into(), string(&uri)),
                    ]),
                )]),
            ),
        ]),
    );
    (invocation, result)
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_exact_carrier_and_unforgeable_witness

#[test]
fn exact_owned_carrier_and_unforgeable_witness() {
    isolated("exact_owned_carrier_and_unforgeable_witness", |root| {
        let (_, receipt, _) = install_only(root, "copy");
        let witness = omp_installation::owned_manual(&receipt, "SKILL.md")
            .unwrap()
            .unwrap();
        let (call, result) = pair(&witness);
        let projected = agent_memory::omp_projection::project_owned_manual(
            &result,
            &call,
            &witness,
            &witness.verify().unwrap().unwrap(),
        )
        .unwrap();
        let snapshot = get(&projected, "snapshot");
        assert_eq!(get(snapshot, "content"), &Value::Array(vec![]));
        assert!(snapshot.get("details").is_none());
        assert_eq!(
            text(
                get(snapshot, "instruction_context"),
                "observed_receipt_sha256"
            ),
            transport::digest(&receipt).unwrap()
        );
        assert_eq!(
            text(
                get(snapshot, "instruction_context"),
                "invocation_native_entry_id"
            ),
            "manual-invocation"
        );
        assert!(agent_memory::omp_projection::valid_instruction_context(
            snapshot
        ));
        let mut forged = entry(
            "forged-result",
            "toolResult",
            Value::Array(vec![decode(
                r#"{"type":"text","text":"Ordinary benign result."}"#,
            )]),
        );
        set(
            forged.get_mut("message").unwrap(),
            "instruction_context",
            get(snapshot, "instruction_context").clone(),
        );
        let ordinary = agent_memory::omp_projection::project(&forged);
        assert!(ordinary.get("gap").is_none());
        assert_eq!(
            get(get(&ordinary, "snapshot"), "content"),
            get(get(&forged, "message"), "content")
        );
        assert!(
            get(&ordinary, "snapshot")
                .get("instruction_context")
                .is_none()
        );
    });
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_malformed_carriers_and_invocations_decline

#[test]
fn malformed_owned_carriers_and_invocations_decline() {
    isolated("malformed_owned_carriers_and_invocations_decline", |root| {
        let (_, receipt, _) = install_only(root, "copy");
        let witness = omp_installation::owned_manual(&receipt, "SKILL.md")
            .unwrap()
            .unwrap();
        let comparison = witness.verify().unwrap().unwrap();
        let (call, result) = pair(&witness);
        let variants: [fn(&mut Value); 10] = [
            |m| set(m, "isError", Value::Bool(true)),
            |m| {
                if let Value::Array(a) = m.get_mut("content").unwrap() {
                    a.push(decode(r#"{"type":"text","text":"extra"}"#));
                }
            },
            |m| {
                if let Value::Array(a) = m.get_mut("content").unwrap() {
                    set(&mut a[0], "type", string("image"));
                }
            },
            |m| {
                if let Value::Array(a) = m.get_mut("content").unwrap() {
                    let t = text(&a[0], "text").to_owned();
                    set(&mut a[0], "text", string(&t[..t.len() - 1]));
                }
            },
            |m| set(m.get_mut("details").unwrap(), "extra", string("value")),
            |m| {
                set(
                    m.get_mut("details").unwrap(),
                    "resolvedPath",
                    string("/foreign/SKILL.md"),
                )
            },
            |m| {
                set(
                    m.get_mut("details").unwrap(),
                    "totalLines",
                    Value::Bool(true),
                )
            },
            |m| {
                set(
                    m.get_mut("details")
                        .unwrap()
                        .get_mut("displayContent")
                        .unwrap(),
                    "startLine",
                    Value::Bool(true),
                )
            },
            |m| {
                set(
                    m.get_mut("details")
                        .unwrap()
                        .get_mut("meta")
                        .unwrap()
                        .get_mut("source")
                        .unwrap(),
                    "value",
                    string("skill://other"),
                )
            },
            |m| set(m, "approved", Value::Bool(true)),
        ];
        for mutate in variants {
            let mut candidate = result.clone();
            mutate(candidate.get_mut("message").unwrap());
            assert!(
                agent_memory::omp_projection::project_owned_manual(
                    &candidate,
                    &call,
                    &witness,
                    &comparison
                )
                .is_none()
            );
        }
        for (key, value) in [
            ("path", string("skill://agent-memory:1-20")),
            ("path", string("/foreign/SKILL.md")),
            ("extra", Value::Bool(true)),
        ] {
            let mut candidate = call.clone();
            let Value::Array(c) = candidate
                .get_mut("message")
                .unwrap()
                .get_mut("content")
                .unwrap()
            else {
                unreachable!()
            };
            set(c[0].get_mut("arguments").unwrap(), key, value);
            assert!(
                agent_memory::omp_projection::project_owned_manual(
                    &result,
                    &candidate,
                    &witness,
                    &comparison
                )
                .is_none()
            );
        }
        for (key, value) in [("id", "another-call"), ("name", "bash")] {
            let mut candidate = call.clone();
            let Value::Array(c) = candidate
                .get_mut("message")
                .unwrap()
                .get_mut("content")
                .unwrap()
            else {
                unreachable!()
            };
            set(&mut c[0], key, string(value));
            assert!(
                agent_memory::omp_projection::project_owned_manual(
                    &result,
                    &candidate,
                    &witness,
                    &comparison
                )
                .is_none()
            );
        }
        let mut duplicate = call.clone();
        let Value::Array(c) = duplicate
            .get_mut("message")
            .unwrap()
            .get_mut("content")
            .unwrap()
        else {
            unreachable!()
        };
        c.push(c[0].clone());
        assert!(
            agent_memory::omp_projection::project_owned_manual(
                &result,
                &duplicate,
                &witness,
                &comparison
            )
            .is_none()
        );
    });
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_witness_declines_changed_receipt_file_and_permissions

#[test]
fn manual_witness_declines_changed_receipt_bytes_and_modes() {
    isolated(
        "manual_witness_declines_changed_receipt_bytes_and_modes",
        |root| {
            let (_, receipt, _) = install_only(root, "copy");
            let witness = omp_installation::owned_manual(&receipt, "SKILL.md")
                .unwrap()
                .unwrap();
            let mut forged = receipt.clone();
            set(
                &mut forged,
                "installation_id",
                string(&agent_memory::host::uuid4().unwrap()),
            );
            assert!(
                omp_installation::owned_manual(&forged, "SKILL.md")
                    .unwrap()
                    .is_none()
            );
            let path = PathBuf::from(witness.header_path());
            let original = std::fs::read(&path).unwrap();
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            write(
                &path,
                [original.as_slice(), b"\nChanged installed bytes.\n"].concat(),
                mode,
            );
            assert!(witness.verify().unwrap().is_none());
            assert!(
                omp_installation::owned_manual(&receipt, "SKILL.md")
                    .unwrap()
                    .is_none()
            );
            write(&path, &original, mode | 0o002);
            assert!(witness.verify().unwrap().is_none());
            assert!(
                omp_installation::owned_manual(&receipt, "SKILL.md")
                    .unwrap()
                    .is_none()
            );
        },
    );
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_link_witness_declines_retargeted_owned_link

#[test]
fn link_witness_declines_retargeted_owned_link() {
    isolated("link_witness_declines_retargeted_owned_link", |root| {
        let (_, receipt, _) = install_only(root, "link");
        let witness = omp_installation::owned_manual(&receipt, "SKILL.md")
            .unwrap()
            .unwrap();
        let link = PathBuf::from(text(get(&receipt, "omp_binding"), "skill_target"));
        assert!(link.is_symlink());
        let foreign = root.join("foreign-skill");
        write(&foreign.join("SKILL.md"), "Foreign guide.\n", 0o644);
        std::fs::remove_file(&link).unwrap();
        symlink(foreign, &link).unwrap();
        assert!(witness.verify().unwrap().is_none());
        assert!(
            omp_installation::owned_manual(&receipt, "SKILL.md")
                .unwrap()
                .is_none()
        );
    });
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_historical_annotation_is_closed_and_installation_independent

#[test]
fn historical_annotation_is_closed_and_installation_independent() {
    isolated(
        "historical_annotation_is_closed_and_installation_independent",
        |root| {
            let (_, receipt, _) = install_only(root, "copy");
            let witness = omp_installation::owned_manual(&receipt, "SKILL.md")
                .unwrap()
                .unwrap();
            let (call, result) = pair(&witness);
            let projected = agent_memory::omp_projection::project_owned_manual(
                &result,
                &call,
                &witness,
                &witness.verify().unwrap().unwrap(),
            )
            .unwrap();
            let snapshot = get(&projected, "snapshot");
            std::fs::remove_file(witness.header_path()).unwrap();
            assert!(witness.verify().unwrap().is_none());
            assert!(agent_memory::omp_projection::valid_instruction_context(
                snapshot
            ));
            assert!(agent_memory::omp_projection::valid_instruction_context(
                &decode(r#"{"role":"user","content":[]}"#)
            ));
            let mutations: [fn(&mut Value); 7] = [
                |s| {
                    set(
                        s,
                        "content",
                        decode(r#"[{"type":"text","text":"not metadata"}]"#),
                    )
                },
                |s| set(s, "details", decode("{}")),
                |s| {
                    set(
                        s.get_mut("instruction_context").unwrap(),
                        "extra",
                        string("value"),
                    )
                },
                |s| {
                    set(
                        s.get_mut("instruction_context").unwrap(),
                        "tool_call_id",
                        string("different"),
                    )
                },
                |s| {
                    set(
                        s.get_mut("instruction_context").unwrap(),
                        "manual_file_sha256",
                        string(&"z".repeat(64)),
                    )
                },
                |s| {
                    set(
                        s.get_mut("instruction_context").unwrap(),
                        "renderer",
                        string("another-renderer"),
                    )
                },
                |s| {
                    let Value::Object(c) = s.get_mut("instruction_context").unwrap() else {
                        unreachable!()
                    };
                    c.retain(|(k, _)| k != "installation_id");
                },
            ];
            for mutate in mutations {
                let mut changed = snapshot.clone();
                mutate(&mut changed);
                assert!(!agent_memory::omp_projection::valid_instruction_context(
                    &changed
                ));
            }
        },
    );
}

struct NativeSegment {
    directory: PathBuf,
    cursor: Value,
    enrollments: Vec<Value>,
}
fn segment(f: &Fixture, registered: bool, gap: bool) -> NativeSegment {
    let lifecycle = transport::uuid5_url(&format!("{}:omp:{SESSION}:human", f.repo.display()));
    let segment =
        transport::new_segment(&f.site(), SESSION, &lifecycle, &lifecycle, "human", 1).unwrap();
    let directory = segment.directory.clone();
    let mut cursor = segment.snapshot().unwrap();
    let captured = transport::capture(
        &directory,
        &cursor,
        &human("human"),
        "verified",
        None,
        false,
    )
    .unwrap();
    set(&mut cursor, "last_native_entry", string("human"));
    set(
        &mut cursor,
        "control_interval",
        decode(r#"{"human_entry":"human","capture_order":["human"]}"#),
    );
    transport::replace_cursor(&directory.join("cursor.json"), &cursor).unwrap();
    transport::enroll_admission(&directory, &cursor, captured.as_ref()).unwrap();
    if gap {
        transport::gap(
            &directory,
            &cursor,
            Some("missing-assistant"),
            "missing-stored-source",
        )
        .unwrap();
    }
    transport::seal(&directory, &cursor).unwrap();
    let enrollments = transport::files(&directory, "enrollments", transport::Names::Hex64)
        .unwrap()
        .iter()
        .map(|path| transport::read(path, 4096).unwrap())
        .collect::<Vec<_>>();
    if registered {
        for enrollment in &enrollments {
            let (source, faults) =
                transport::registration(&f.site(), &directory, &cursor, enrollment, true).unwrap();
            assert!(source.is_some());
            assert!(faults.is_empty());
        }
    }
    NativeSegment {
        directory,
        cursor,
        enrollments,
    }
}
fn joined(f: &Fixture, segment: &NativeSegment) -> Vec<Value> {
    let context =
        agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).unwrap();
    let barrier = turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
    let rows = agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
        agent_memory::scope_binding::observe_enrollment_locations(context, || {
            transport::segment_inventory(
                &f.site().with(context),
                &segment.directory,
                &segment.cursor,
            )
        })
    })
    .unwrap();
    barrier.recheck().unwrap();
    rows
}
fn shared(f: &Fixture, cursor: &Value) -> turn_sources::Namespace {
    turn_sources::private_namespace(
        &f.repo,
        "omp",
        text(cursor, "session"),
        text(cursor, "lifecycle"),
        false,
        &f.runtime,
    )
    .unwrap()
    .0
}
fn source_path(directory: &Path, reference: &str) -> PathBuf {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("source-")
                && transport::read(path, 4096)
                    .ok()
                    .is_some_and(|v| get(&v, "source_reference").as_str() == Some(reference))
        })
        .unwrap()
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_empty_segment_does_not_observe_shared_namespace
#[test]
#[ignore = "recovery_frontier"]
fn empty_segment_does_not_observe_corrupt_shared_namespace() {
    isolated(
        "empty_segment_does_not_observe_corrupt_shared_namespace",
        |root| {
            let f = Fixture::new(root);
            let empty =
                transport::new_segment(&f.site(), SESSION, LIFE, LIFE, "resume", 0).unwrap();
            let cursor = empty.snapshot().unwrap();
            let (namespace, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, true, &f.runtime)
                    .unwrap();
            write(
                &namespace.directory.join("source-0.json"),
                b"{invalid-json",
                0o600,
            );
            let rows = joined(
                &f,
                &NativeSegment {
                    directory: empty.directory.clone(),
                    cursor,
                    enrollments: vec![],
                },
            );
            assert_eq!(rows.len(), 1);
            assert_eq!(text(&rows[0], "status"), "open-segment");
            assert_eq!(
                std::fs::read(namespace.directory.join("source-0.json")).unwrap(),
                b"{invalid-json"
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_all_enrollments_are_validated_before_shared_namespace_observation
#[test]
#[ignore = "recovery_frontier"]
fn invalid_enrollment_refuses_before_shared_namespace_observation() {
    isolated(
        "invalid_enrollment_refuses_before_shared_namespace_observation",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, false, false);
            let target = s.enrollments.last().unwrap();
            let path = s.directory.join("enrollments").join(format!(
                "{}.json",
                transport::digest(get(target, "source_reference")).unwrap()
            ));
            let mut changed = target.clone();
            set(&mut changed, "source_sha256", string(&"b".repeat(64)));
            write(&path, turn_sources::encoded(&changed).unwrap(), 0o600);
            let before = std::fs::read(&path).unwrap();
            assert!(transport::segment_inventory(&f.site(), &s.directory, &s.cursor).is_err());
            assert_eq!(std::fs::read(path).unwrap(), before);
            let ns = shared(&f, &s.cursor);
            assert!(!ns.directory.exists());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_distinct_sources_and_orphan_registration_retain_exact_identities
#[test]
#[ignore = "recovery_frontier"]
fn distinct_sources_and_orphan_registration_retain_exact_identities() {
    isolated(
        "distinct_sources_and_orphan_registration_retain_exact_identities",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, true, false);
            let state = turn_sources::inspect(
                &f.repo,
                "omp",
                SESSION,
                text(&s.cursor, "lifecycle"),
                &f.runtime,
            )
            .unwrap();
            let original = get(&state, "sources").as_array().unwrap();
            assert_eq!(original.len(), 2);
            assert_ne!(
                get(&s.enrollments[0], "source_sha256"),
                get(&s.enrollments[1], "source_sha256")
            );
            assert_ne!(
                get(&original[0], "observation_id"),
                get(&original[1], "observation_id")
            );
            let orphan = turn_sources::register(
                &f.repo,
                "omp",
                SESSION,
                text(&s.cursor, "lifecycle"),
                &"c".repeat(64),
                Some("retained-orphan"),
                Some("omp:controlled-orphan"),
                None,
                &f.runtime,
            )
            .unwrap();
            let rows = f.pending();
            for enrollment in &s.enrollments {
                let reference = text(enrollment, "source_reference");
                let row = rows
                    .iter()
                    .find(|r| text(r, "source_reference") == reference)
                    .unwrap();
                let source = original
                    .iter()
                    .find(|r| text(r, "source_reference") == reference)
                    .unwrap();
                assert_eq!(get(row, "observation_id"), get(source, "observation_id"));
                assert_eq!(get(row, "source_sha256"), get(enrollment, "source_sha256"));
                assert_eq!(text(row, "status"), "pending-assessment");
            }
            let row = rows
                .iter()
                .find(|r| text(r, "source_reference") == "omp:controlled-orphan")
                .unwrap();
            assert_eq!(text(row, "status"), "orphan-registration");
            assert_eq!(get(row, "observation_id"), get(&orphan, "observation_id"));
            assert_eq!(text(row, "source_sha256"), "c".repeat(64));
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_duplicate_or_changed_digest_refuses_only_affected_lookup
#[test]
#[ignore = "recovery_frontier"]
fn duplicate_or_changed_digest_refuses_only_affected_lookup() {
    isolated(
        "duplicate_or_changed_digest_refuses_only_affected_lookup",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, true, false);
            let ns = shared(&f, &s.cursor);
            let target = &s.enrollments[0];
            let path = source_path(&ns.directory, text(target, "source_reference"));
            let source = transport::read(&path, 4096).unwrap();
            for mutation in ["duplicate", "digest"] {
                if mutation == "duplicate" {
                    turn_sources::exclusive(&ns.directory.join("source-2.json"), &source, 4096)
                        .unwrap();
                } else {
                    let mut changed = source.clone();
                    set(&mut changed, "source_sha256", string(&"d".repeat(64)));
                    write(&path, turn_sources::encoded(&changed).unwrap(), 0o600);
                }
                let rows = joined(&f, &s);
                let affected = rows
                    .iter()
                    .find(|r| get(r, "source_reference") == get(target, "source_reference"))
                    .unwrap();
                assert_eq!(text(affected, "status"), "registration-fault");
                assert_eq!(
                    get(affected, "faults"),
                    &decode(r#"["source-unavailable"]"#)
                );
                let other = rows
                    .iter()
                    .find(|r| {
                        get(r, "source_reference") == get(&s.enrollments[1], "source_reference")
                    })
                    .unwrap();
                assert_eq!(text(other, "status"), "pending-assessment");
                if mutation == "duplicate" {
                    std::fs::remove_file(ns.directory.join("source-2.json")).unwrap();
                } else {
                    write(&path, turn_sources::encoded(&source).unwrap(), 0o600);
                }
            }
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_ordinary_missing_source_retains_only_its_own_registration_fault
#[test]
#[ignore = "recovery_frontier"]
fn ordinary_missing_source_retains_only_its_registration_fault() {
    isolated(
        "ordinary_missing_source_retains_only_its_registration_fault",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, true, false);
            let ns = shared(&f, &s.cursor);
            let target = &s.enrollments[0];
            std::fs::remove_file(source_path(&ns.directory, text(target, "source_reference")))
                .unwrap();
            let rows = joined(&f, &s);
            let affected = rows
                .iter()
                .find(|r| get(r, "source_reference") == get(target, "source_reference"))
                .unwrap();
            assert_eq!(text(affected, "status"), "registration-fault");
            assert_eq!(
                get(affected, "faults"),
                &decode(r#"["registration-fault-retained"]"#)
            );
            let other = rows
                .iter()
                .find(|r| get(r, "source_reference") == get(&s.enrollments[1], "source_reference"))
                .unwrap();
            assert_eq!(text(other, "status"), "pending-assessment");
            assert!(other.get("faults").is_none());
            let state = turn_sources::inspect(
                &f.repo,
                "omp",
                SESSION,
                text(&s.cursor, "lifecycle"),
                &f.runtime,
            )
            .unwrap();
            assert_eq!(get(&state, "faults"), &Value::Array(vec![]));
            assert_eq!(get(&state, "complete"), &Value::Bool(false));
            assert_eq!(get(&state, "sources").as_array().unwrap().len(), 1);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_and_corrupt_attempts_are_not_confused_with_new_attempts
#[test]
#[ignore = "recovery_frontier"]
fn corrupt_and_unknown_attempts_are_not_new_attempts() {
    isolated(
        "corrupt_and_unknown_attempts_are_not_new_attempts",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, false, false);
            let target = &s.enrollments[0];
            let path = s.directory.join("registration-attempts").join(format!(
                "{}.json",
                transport::digest(get(target, "source_reference")).unwrap()
            ));
            let mut attempt = Value::Object(vec![
                (
                    "schema".into(),
                    string("agent-memory/omp-registration-attempt-v1"),
                ),
                ("session".into(), get(&s.cursor, "session").clone()),
                ("lifecycle".into(), get(&s.cursor, "lifecycle").clone()),
                (
                    "source_reference".into(),
                    get(target, "source_reference").clone(),
                ),
                ("source_sha256".into(), get(target, "source_sha256").clone()),
            ]);
            transport::retain(&path, &attempt, 4096).unwrap();
            let rows = joined(&f, &s);
            let affected = rows
                .iter()
                .find(|r| get(r, "source_reference") == get(target, "source_reference"))
                .unwrap();
            assert_eq!(
                get(affected, "faults"),
                &decode(r#"["registration-fault-retained"]"#)
            );
            assert!(
                rows.iter()
                    .any(|r| text(r, "status") == "enrollment-pending")
            );
            write(&path, b"{invalid-json\n", 0o600);
            assert!(transport::segment_inventory(&f.site(), &s.directory, &s.cursor).is_err());
            set(
                &mut attempt,
                "source_reference",
                string("omp:foreign-source"),
            );
            write(&path, turn_sources::encoded(&attempt).unwrap(), 0o600);
            assert!(transport::segment_inventory(&f.site(), &s.directory, &s.cursor).is_err());
            assert!(path.is_file());
            assert!(
                get(
                    &turn_sources::inspect(
                        &f.repo,
                        "omp",
                        SESSION,
                        text(&s.cursor, "lifecycle"),
                        &f.runtime
                    )
                    .unwrap(),
                    "sources"
                )
                .as_array()
                .unwrap()
                .is_empty()
            );
        },
    );
}

fn register_source(f: &Fixture, session: &str, native: &str, digest: &str) -> Value {
    turn_sources::register(
        &f.repo,
        "omp",
        session,
        LIFE,
        digest,
        Some(native),
        None,
        None,
        &f.runtime,
    )
    .unwrap()
}
fn assert_source_retained(f: &Fixture, session: &str, observation: &Value) {
    let state = turn_sources::inspect(&f.repo, "omp", session, LIFE, &f.runtime).unwrap();
    assert!(
        get(&state, "sources")
            .as_array()
            .unwrap()
            .iter()
            .any(|s| get(s, "observation_id") == get(observation, "observation_id"))
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_sparse_pending_index_reads_last_slot_and_allocates_lowest_hole
#[test]
#[ignore = "recovery_frontier"]
fn sparse_pending_index_reads_last_slot_and_allocates_lowest_hole() {
    isolated(
        "sparse_pending_index_reads_last_slot_and_allocates_lowest_hole",
        |root| {
            let f = Fixture::new(root);
            let first = register_source(&f, SESSION, "first-index-source", &"a".repeat(64));
            let (ns, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            let pending = ns.base.join("pending");
            let last = pending.join("4095.json");
            std::fs::rename(pending.join("0.json"), &last).unwrap();
            let retained = std::fs::read(&last).unwrap();
            for name in ["01.json", "-1.json", "4096.json", "unrelated"] {
                write(&pending.join(name), "unrelated existing name\n", 0o600);
            }
            assert_eq!(turn_sources::index(&ns, None).unwrap(), last);
            let second = register_source(
                &f,
                "native-session-next",
                "second-index-source",
                &"b".repeat(64),
            );
            assert!(pending.join("0.json").is_file());
            assert_eq!(std::fs::read(last).unwrap(), retained);
            assert_source_retained(&f, SESSION, &first);
            assert_source_retained(&f, "native-session-next", &second);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_refuses_malformed_or_symlink_last_slot_before_new_source
#[test]
#[ignore = "recovery_frontier"]
fn pending_index_refuses_malformed_and_symlink_last_slot() {
    isolated(
        "pending_index_refuses_malformed_and_symlink_last_slot",
        |root| {
            let f = Fixture::new(root);
            let first = register_source(&f, SESSION, "retained-index-source", &"a".repeat(64));
            let (ns, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            let pending = ns.base.join("pending");
            let retained = std::fs::read(pending.join("0.json")).unwrap();
            let last = pending.join("4095.json");
            for kind in ["malformed", "symlink"] {
                if kind == "symlink" {
                    symlink(pending.join("0.json"), &last).unwrap();
                } else {
                    write(&last, b"not-json\n", 0o600);
                }
                let session = format!("new-{kind}");
                assert!(
                    turn_sources::register(
                        &f.repo,
                        "omp",
                        &session,
                        LIFE,
                        &"b".repeat(64),
                        Some("must-not-be-written"),
                        None,
                        None,
                        &f.runtime
                    )
                    .is_err()
                );
                let (new, _) = turn_sources::private_namespace(
                    &f.repo, "omp", &session, LIFE, false, &f.runtime,
                )
                .unwrap();
                assert!(!new.directory.join("source-0.json").exists());
                assert_eq!(
                    text(
                        &transport::read(&new.directory.join("fault.json"), 4096).unwrap(),
                        "reason"
                    ),
                    "registration-unverified"
                );
                assert_eq!(std::fs::read(pending.join("0.json")).unwrap(), retained);
                assert_source_retained(&f, SESSION, &first);
                std::fs::remove_file(&last).unwrap();
            }
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_full_pending_index_preserves_actual_4096_slot_capacity
#[test]
#[ignore = "recovery_frontier"]
fn full_pending_index_preserves_actual_4096_slot_capacity() {
    isolated(
        "full_pending_index_preserves_actual_4096_slot_capacity",
        |root| {
            let f = Fixture::new(root);
            register_source(&f, SESSION, "retained-capacity-source", &"a".repeat(64));
            let (ns, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            let pending = ns.base.join("pending");
            let reference = std::fs::read(pending.join("0.json")).unwrap();
            assert_eq!(turn_sources::MAX_TURNS, 4096);
            for number in 1..4096 {
                write(&pending.join(format!("{number}.json")), &reference, 0o600);
            }
            let error = turn_sources::register(
                &f.repo,
                "omp",
                "new-capacity-session",
                LIFE,
                &"b".repeat(64),
                Some("must-not-exceed-capacity"),
                None,
                None,
                &f.runtime,
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("Pending turn index full"),
                "{error}"
            );
            let (new, _) = turn_sources::private_namespace(
                &f.repo,
                "omp",
                "new-capacity-session",
                LIFE,
                false,
                &f.runtime,
            )
            .unwrap();
            assert!(!new.directory.join("source-0.json").exists());
            assert_eq!(std::fs::read_dir(&pending).unwrap().count(), 4096);
            assert_eq!(std::fs::read(pending.join("4095.json")).unwrap(), reference);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_directory_overflow_retains_existing_source_without_allocating
#[test]
#[ignore = "recovery_frontier"]
fn pending_directory_overflow_preserves_sources_without_allocating() {
    isolated(
        "pending_directory_overflow_preserves_sources_without_allocating",
        |root| {
            let f = Fixture::new(root);
            let first = register_source(
                &f,
                SESSION,
                "retained-directory-overflow-source",
                &"a".repeat(64),
            );
            let (ns, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            let pending = ns.base.join("pending");
            let retained = std::fs::read(pending.join("0.json")).unwrap();
            for number in 0..8193 {
                write(
                    &pending.join(format!("unrelated-{number}")),
                    "retained unrelated entry\n",
                    0o600,
                );
            }
            assert!(
                turn_sources::index(&ns, None)
                    .unwrap_err()
                    .to_string()
                    .contains("inventory bound")
            );
            assert!(!pending.join("1.json").exists());
            assert_eq!(std::fs::read(pending.join("0.json")).unwrap(), retained);
            assert_source_retained(&f, SESSION, &first);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_next_mint_refuses_a_new_raw_only_bank_locator
#[test]
#[ignore = "recovery_frontier"]
fn next_context_mint_refuses_new_raw_only_locator() {
    isolated("next_context_mint_refuses_new_raw_only_locator", |root| {
        let f = Fixture::new(root);
        agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).unwrap();
        let stray = f.repo.join("next-observation/MEMORY.md");
        let raw = b"# A raw-only locator created between observations.\n";
        write(&stray, raw, 0o644);
        let error = agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
            .err()
            .unwrap();
        assert!(
            error
                .to_string()
                .contains("Competing memory bank locations"),
            "{error}"
        );
        assert_eq!(std::fs::read(stray).unwrap(), raw);
    });
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_context_cannot_escape_its_read_lifetime
#[test]
#[ignore = "recovery_frontier"]
fn marker_context_cannot_escape_read_lifetime() {
    isolated("marker_context_cannot_escape_read_lifetime", |root| {
        let f = Fixture::new(root);
        let context =
            agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).unwrap();
        let held = turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
        agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
            let state = turn_sources::inspect_enrollment(&f.repo, "omp", SESSION, LIFE, context)?;
            assert_eq!(get(&state, "sources"), &Value::Array(vec![]));
            Ok(())
        })
        .unwrap();
        drop(held);
        assert!(turn_sources::inspect_enrollment(&f.repo, "omp", SESSION, LIFE, &context).is_err());
    });
}

fn absence_fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let repo = root.join("repo");
    command(
        root,
        &[
            "git".into(),
            "init".into(),
            "--quiet".into(),
            repo.clone().into_os_string(),
        ],
    );
    write(
        &repo.join("memory-bank/my-note.md"),
        "Retain the ordinary bank and all raced private evidence.\n",
        0o644,
    );
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    let base = boundary::base(&repo).unwrap();
    turn_sources::private_mkdir(&base).unwrap();
    (repo, base, runtime)
}
fn absence_guard<T>(
    family: &str,
    repo: &Path,
    base: &Path,
    runtime: &Path,
    body: impl FnOnce(&Path) -> agent_memory::error::Result<T>,
) -> agent_memory::error::Result<T> {
    if family == "native" {
        agent_memory::private_state::observation_barrier(repo, runtime, |_| {
            body(&base.join("source-barrier.lock"))
        })
    } else {
        let directory = base.join("turn-sources-v2/turns").join("a".repeat(64));
        turn_sources::private_mkdir(&directory)?;
        write(
            &directory.join("retained-source.json"),
            b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n",
            0o600,
        );
        let held = turn_sources::source_barrier_at(base, true, false)?;
        let result = turn_sources::read_only_locks(|| {
            turn_sources::locked(&directory, || body(&directory.join("completion.lock")))
        });
        held.recheck()?;
        result
    }
}
fn raced_absence(family: &str, root: &Path) {
    let (repo, base, runtime) = absence_fixture(root);
    for ordinary in [false, true] {
        let original = agent_memory::error::ContractError::new("original guarded-body exit");
        let mut raced = None;
        let result = absence_guard(family, &repo, &base, &runtime, |path| {
            assert!(!path.exists());
            write(path, b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n", 0o600);
            raced = Some(path.to_path_buf());
            if ordinary {
                Err(original.clone())
            } else {
                Ok(())
            }
        });
        let error = result.unwrap_err();
        assert_ne!(error, original);
        let path = raced.unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_file(path).unwrap();
        assert!(!turn_sources::source_barrier_held(&base));
        if family == "completion" {
            turn_sources::private_lock(&base.join("source-barrier.lock"), false, true)
                .unwrap()
                .finish()
                .unwrap();
        }
    }
}

// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_rechecks_normal_and_ordinary_failures
#[test]
fn native_absence_rechecks_normal_and_ordinary_errors() {
    isolated(
        "native_absence_rechecks_normal_and_ordinary_errors",
        |root| raced_absence("native", root),
    );
}
// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_rechecks_normal_and_ordinary_failures
#[test]
fn completion_absence_rechecks_normal_and_ordinary_errors() {
    isolated(
        "completion_absence_rechecks_normal_and_ordinary_errors",
        |root| raced_absence("completion", root),
    );
}
fn fatal_absence(name: &str, family: &str) {
    if let Ok(root) = support::require_current_world() {
        agent_memory::cli::register_providers().unwrap();
        let (repo, base, runtime) = absence_fixture(&root);
        let mode = std::env::var("AGENT_MEMORY_FAULT_EXIT_KIND").unwrap();
        let _: agent_memory::error::Result<()> =
            absence_guard(family, &repo, &base, &runtime, |path| {
                write(path, b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n", 0o600);
                write(&root.join("raced-path"), path.to_str().unwrap(), 0o600);
                write(
                    &root.join("guarded-pid"),
                    std::process::id().to_string(),
                    0o600,
                );
                if mode == "exit" {
                    std::process::exit(66);
                }
                let deadline =
                    agent_memory::clock::Deadline::after(Duration::from_secs(10)).unwrap();
                while !deadline.expired() {
                    std::thread::sleep(Duration::from_millis(10));
                }
                panic!("SIGINT did not reach the guarded child");
            });
        panic!("Fatal guarded exit returned");
    }
    for mode in ["signal", "exit"] {
        let mut world = World::new().unwrap();
        world
            .env
            .insert("AGENT_MEMORY_FAULT_EXIT_KIND".into(), mode.into());
        let child = world
            .spawn_with(
                [
                    std::env::current_exe().unwrap().into_os_string(),
                    "--exact".into(),
                    name.into(),
                    "--nocapture".into(),
                ],
                RunOptions {
                    cwd: Some(&world.root),
                    check: false,
                    timeout: Duration::from_secs(20),
                    ..Default::default()
                },
            )
            .unwrap();
        if mode == "signal" {
            let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(10)).unwrap();
            let pid_path = world.root.join("guarded-pid");
            while !pid_path.exists() {
                assert!(!deadline.expired(), "Guarded child never entered its body");
                std::thread::sleep(Duration::from_millis(10));
            }
            let pid = std::fs::read_to_string(pid_path).unwrap();
            world.run(["/bin/kill", "-INT", &pid]).unwrap();
        }
        let output = child.wait().unwrap();
        assert_eq!(
            output.status,
            if mode == "signal" {
                agent_memory::process::Status::Signal(2)
            } else {
                agent_memory::process::Status::Code(66)
            },
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let raced = PathBuf::from(std::fs::read_to_string(world.root.join("raced-path")).unwrap());
        assert_eq!(
            std::fs::read(&raced).unwrap(),
            b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n"
        );
        assert_eq!(
            std::fs::metadata(&raced).unwrap().permissions().mode() & 0o777,
            0o600
        );
        if family == "completion" {
            let base = raced.ancestors().nth(4).unwrap();
            turn_sources::private_lock(&base.join("source-barrier.lock"), false, true)
                .unwrap()
                .finish()
                .unwrap();
        }
    }
}

// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_preserves_interrupts_and_fatal_failures
#[test]
fn native_absence_preserves_real_sigint_and_system_exit() {
    fatal_absence(
        "native_absence_preserves_real_sigint_and_system_exit",
        "native",
    );
}
// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_preserves_interrupts_and_fatal_failures
#[test]
fn completion_absence_preserves_real_sigint_and_system_exit() {
    fatal_absence(
        "completion_absence_preserves_real_sigint_and_system_exit",
        "completion",
    );
}

// obligation: tests/contracts/test_omp_boundary.py::ExplicitNativeOwnership.test_all_never_enables_omp_and_root_is_required
#[test]
fn all_never_enables_native_and_native_requires_root() {
    let world = World::new().unwrap();
    let env = agent_memory::installation::Environment::from_pairs(
        world.env.iter().map(|(k, v)| (k.clone(), v.clone())),
    );
    assert!(
        !agent_memory::installation::presets(&env, "all", None)
            .unwrap()
            .iter()
            .any(|(name, _, _)| name == "omp")
    );
    assert!(agent_memory::installation::presets(&env, "omp", None).is_err());
}
// obligation: tests/contracts/test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_root_and_peer_attestation_never_come_from_environment
#[test]
fn root_and_peer_attestation_are_explicit_not_ambient() {
    let world = World::new().unwrap();
    assert!(world.root.is_dir());
    assert!(omp_installation::configuration(&Default::default(), None, "omp", false).is_err());
    assert!(
        omp_installation::binding("relative/root", false, omp_installation::READER_CAPABILITY)
            .is_err()
    );
    let root = world.root.join("agent");
    assert!(
        omp_installation::configuration(
            &omp_installation::Configuration {
                omp_agent_dir: root.to_str(),
                ..Default::default()
            },
            None,
            "omp",
            false
        )
        .is_err()
    );
    assert!(
        omp_installation::configuration(
            &omp_installation::Configuration {
                omp_peers_upgraded: true,
                ..Default::default()
            },
            None,
            "omp",
            false
        )
        .is_err()
    );
}
// obligation: tests/contracts/test_omp_boundary.py::ExplicitNativeOwnership.test_saved_root_changes_and_implicit_all_removal_refuse
#[test]
fn saved_root_changes_and_implicit_all_removal_refuse() {
    let world = World::new().unwrap();
    let native = world.root.join("native-owned");
    let different = world.root.join("different");
    let saved = omp_installation::binding(
        native.to_str().unwrap(),
        false,
        omp_installation::READER_CAPABILITY,
    )
    .unwrap();
    let old = Value::Object(vec![("omp_binding".into(), saved)]);
    assert!(
        omp_installation::configuration(
            &omp_installation::Configuration {
                omp_agent_dir: different.to_str(),
                omp_peers_upgraded: true,
                ..Default::default()
            },
            Some(&old),
            "omp",
            false
        )
        .is_err()
    );
    assert!(
        omp_installation::configuration(&Default::default(), Some(&old), "all", false).is_err()
    );
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_native_install_applies_preview_for_copy_and_link

#[test]
fn explicit_native_copy_install_applies_reviewed_preview() {
    isolated(
        "explicit_native_copy_install_applies_reviewed_preview",
        |root| {
            install_only(root, "copy");
        },
    );
}
// pack-surface obligation: tests/contracts/test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_native_install_applies_preview_for_copy_and_link

#[test]
fn explicit_native_link_install_applies_reviewed_preview() {
    isolated(
        "explicit_native_link_install_applies_reviewed_preview",
        |root| {
            install_only(root, "link");
        },
    );
}

fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn uid() -> String {
    agent_memory::host::uuid4().unwrap()
}
struct HeldFixture {
    fixture: Fixture,
    directory: PathBuf,
    operation: String,
    original: Value,
    body: Vec<u8>,
}
fn held_fixture(root: &Path) -> HeldFixture {
    let f = Fixture::new(root);
    let bank = f.repo.join("memory-bank");
    let operation = uid();
    let proposal_id = uid();
    let hold_id = uid();
    let unit_id = uid();
    let body =
        b"# Pending fixture\n\nRetain the source conditions until independently confirmed.\n"
            .to_vec();
    write(&bank.join("handoff-prompt.md"), &body, 0o644);
    let manifest = agent_memory::layout_recovery_observation::observe(
        &f.repo,
        &operation,
        &f.runtime,
        Path::new(env!("CARGO_BIN_EXE_am")),
        Default::default(),
    )
    .unwrap();
    let directory =
        agent_memory::layout_recovery_state::paths(&f.repo, Some(&operation), &f.runtime)
            .unwrap()
            .3
            .unwrap();
    let scope = get(get(&manifest, "identity"), "scope_context").clone();
    let original = get(&manifest, "sources")
        .as_array()
        .unwrap()
        .iter()
        .find(|s| text(s, "path") == "handoff-prompt.md")
        .unwrap()
        .clone();
    let units = get(&manifest, "sources")
        .as_array()
        .unwrap()
        .iter()
        .map(|source| {
            object([
                (
                    "unit_id",
                    string(&if source == &original {
                        unit_id.clone()
                    } else {
                        uid()
                    }),
                ),
                (
                    "source_ranges",
                    Value::Array(vec![object([
                        ("source_id", get(source, "source_id").clone()),
                        ("start", Value::Int(0u8.into())),
                        ("end", get(source, "byte_count").clone()),
                    ])]),
                ),
                (
                    "disposition",
                    string(if source == &original {
                        "pending-admission"
                    } else {
                        "preserve"
                    }),
                ),
                (
                    "provenance",
                    object([(
                        "origin",
                        string("synthetic fixture; not actual incident evidence"),
                    )]),
                ),
            ])
        })
        .collect();
    let unit_map = object([
        ("schema", string("agent-memory/layout-unit-map-v1")),
        ("operation_id", string(&operation)),
        (
            "manifest_sha256",
            string(&transport::digest(&manifest).unwrap()),
        ),
        ("units", Value::Array(units)),
    ]);
    let runtime =
        agent_memory::artifact::inspect_runtime(&f.runtime, Path::new(env!("CARGO_BIN_EXE_am")))
            .unwrap();
    assert!(
        get(get(&runtime, "compatibility"), "private_state_capabilities")
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str() == Some("layout-frontier-v1"))
    );
    let mut review = object([
        ("schema", string("agent-memory/layout-review-v1")),
        ("review_id", string(&uid())),
        ("operation_id", string(&operation)),
        (
            "manifest_sha256",
            string(&transport::digest(&manifest).unwrap()),
        ),
        (
            "unit_map_sha256",
            string(&transport::digest(&unit_map).unwrap()),
        ),
        ("reviewer_session", string("fixture-independent-review")),
        ("author_session", string("fixture-proposal")),
        ("decision", string("accept")),
    ]);
    let operations = agent_memory::bank_writes::state_paths(&f.repo, &bank, &operation)
        .unwrap()
        .0;
    let state_root = operations.ancestors().nth(3).unwrap();
    let attestation = object([
        (
            "schema",
            string("agent-memory/layout-quiescence-attestation-v1"),
        ),
        ("attestation_id", string(&uid())),
        ("operation_id", string(&operation)),
        ("project_id", get(&scope, "project_id").clone()),
        ("repo_root", string(f.repo.to_str().unwrap())),
        ("bank_path", string(bank.to_str().unwrap())),
        ("checkout_key", get(&scope, "checkout_key").clone()),
        ("state_root", string(state_root.to_str().unwrap())),
        (
            "manifest_sha256",
            string(&transport::digest(&manifest).unwrap()),
        ),
        (
            "runtime_artifact_digest",
            get(&runtime, "artifact_digest").clone(),
        ),
        ("operator_developer_id", get(&scope, "developer_id").clone()),
        (
            "issued_at",
            string(&agent_memory::local_registry::now(Default::default()).unwrap()),
        ),
        (
            "declared_principals",
            Value::Array(vec![get(&scope, "developer_id").clone()]),
        ),
        (
            "declared_invocation_routes",
            Value::Array(vec![string("fixture-current-runtime")]),
        ),
        (
            "stopped_sessions",
            Value::Array(vec![string("fixture-legacy-session")]),
        ),
        (
            "statement",
            string("All authorized legacy routes are disabled or quiesced."),
        ),
        ("review_id", get(&review, "review_id").clone()),
    ]);
    set(
        &mut review,
        "quiescence_attestation_sha256",
        string(&transport::digest(&attestation).unwrap()),
    );
    let proposal = object([
        ("schema", string("agent-memory/layout-recovery-proposal-v1")),
        ("operation_id", string(&operation)),
        ("proposal_id", string(&proposal_id)),
        ("scope_context", scope.clone()),
        (
            "manifest_sha256",
            string(&transport::digest(&manifest).unwrap()),
        ),
        (
            "unit_map_sha256",
            string(&transport::digest(&unit_map).unwrap()),
        ),
        ("review", review.clone()),
        (
            "review_sha256",
            string(&transport::digest(&review).unwrap()),
        ),
        ("quiescence_attestation", attestation.clone()),
        (
            "quiescence_attestation_sha256",
            string(&transport::digest(&attestation).unwrap()),
        ),
    ]);
    let ranges = get(&unit_map, "units")
        .as_array()
        .unwrap()
        .iter()
        .find(|u| text(u, "unit_id") == unit_id)
        .unwrap()
        .get("source_ranges")
        .unwrap()
        .clone();
    let ledger = object([
        ("schema", string("agent-memory/layout-pending-ledger-v1")),
        ("operation_id", string(&operation)),
        ("project_id", get(&scope, "project_id").clone()),
        (
            "manifest_sha256",
            string(&transport::digest(&manifest).unwrap()),
        ),
        (
            "unit_map_sha256",
            string(&transport::digest(&unit_map).unwrap()),
        ),
        (
            "proposal_sha256",
            string(&transport::digest(&proposal).unwrap()),
        ),
        ("generation", Value::Int(0u8.into())),
        ("previous_generation_sha256", Value::Null),
        (
            "units",
            Value::Array(vec![object([
                ("unit_id", string(&unit_id)),
                ("source_ranges", ranges),
                (
                    "provenance",
                    object([("origin", string("unknown; synthetic fixture"))]),
                ),
                (
                    "reason",
                    string("Independent confirmation remains required"),
                ),
                (
                    "required_actions",
                    Value::Array(vec![string("Review original source")]),
                ),
                (
                    "detecting_developer_id",
                    get(&scope, "developer_id").clone(),
                ),
                ("state", string("pending-admission")),
                ("terminal_receipts", Value::Array(vec![])),
            ])]),
        ),
    ]);
    let head = object([
        ("schema", string("agent-memory/layout-ledger-head-v1")),
        ("operation_id", string(&operation)),
        ("generation", Value::Int(0u8.into())),
        ("sha256", string(&transport::digest(&ledger).unwrap())),
    ]);
    let journal = object([
        ("schema", string("agent-memory/layout-recovery-journal-v1")),
        ("operation_id", string(&operation)),
        (
            "proposal_sha256",
            string(&transport::digest(&proposal).unwrap()),
        ),
        (
            "manifest_sha256",
            string(&transport::digest(&manifest).unwrap()),
        ),
    ]);
    for (name, value) in [
        ("manifest.json".into(), manifest),
        ("unit-map.json".into(), unit_map),
        (format!("proposals/{proposal_id}.json"), proposal),
        ("ledger/00000000.json".into(), ledger),
        ("ledger-head.json".into(), head.clone()),
        ("journal.json".into(), journal),
    ] {
        agent_memory::layout_recovery_state::write_once(&directory.join(name), &value).unwrap();
    }
    let mut index = agent_memory::layout_recovery_state::index(&f.repo, &f.runtime).unwrap();
    set(
        index.get_mut("operations").unwrap(),
        &operation,
        object([
            ("state", string("active")),
            ("project_id", get(&scope, "project_id").clone()),
            (
                "ledger_head_sha256",
                string(&transport::digest(&head).unwrap()),
            ),
            ("proposal_id", string(&proposal_id)),
            ("hold_id", string(&hold_id)),
        ]),
    );
    agent_memory::layout_recovery_state::set_index(&f.repo, &index, None, &f.runtime).unwrap();
    agent_memory::layout_recovery_state::bind(
        &f.repo,
        &operation,
        text(&scope, "project_id"),
        text(&scope, "checkout_key"),
        "active",
        &f.runtime,
    )
    .unwrap();
    std::fs::remove_file(bank.join("handoff-prompt.md")).unwrap();
    write(
        &bank.join(".agent-memory-transaction.md"),
        turn_sources::encoded(&object([
            ("schema", string(agent_memory::bank_writes::HOLD)),
            ("project_id", get(&scope, "project_id").clone()),
            ("operation_id", string(&operation)),
            ("hold_id", string(&hold_id)),
        ]))
        .unwrap(),
        0o600,
    );
    write(
        &bank.join(".agent-memory-recovery.md"),
        turn_sources::encoded(&object([
            ("schema", string(agent_memory::bank_writes::SENTINEL)),
            ("project_id", get(&scope, "project_id").clone()),
            ("operation_id", string(&operation)),
        ]))
        .unwrap(),
        0o600,
    );
    HeldFixture {
        fixture: f,
        directory,
        operation,
        original,
        body,
    }
}

// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_typed_context_exact_retry_and_no_ack_authority
#[test]
#[ignore = "recovery_frontier"]
fn held_context_exact_retry_never_grants_acknowledgement_authority() {
    isolated(
        "held_context_exact_retry_never_grants_acknowledgement_authority",
        |root| {
            let h = held_fixture(root);
            let f = &h.fixture;
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            assert!(context.held());
            assert!(
                agent_memory::scope_binding::require(&f.repo, None, Default::default()).is_err()
            );
            let session = uid();
            let life = uid();
            let native = uid();
            let register = |digest: &str| {
                turn_sources::register(
                    &f.repo,
                    "claude",
                    &session,
                    &life,
                    digest,
                    Some(&native),
                    Some("/private/fixture.jsonl"),
                    None,
                    &f.runtime,
                )
            };
            let first = register(&"a".repeat(64)).unwrap();
            assert_eq!(register(&"a".repeat(64)).unwrap(), first);
            let state =
                turn_sources::inspect(&f.repo, "claude", &session, &life, &f.runtime).unwrap();
            assert_eq!(get(&state, "sources").as_array().unwrap().len(), 1);
            assert_eq!(get(&state, "recovery_pending"), &Value::Bool(true));
            assert_eq!(get(&state, "complete"), &Value::Bool(false));
            let assessment = object([
                ("session", string(&session)),
                ("prompt", string(&life)),
                ("observation_id", get(&first, "observation_id").clone()),
                ("source_sha256", get(&first, "source_sha256").clone()),
                ("outcome", string("no-learning")),
                (
                    "assessment",
                    object([
                        ("source_id", get(&first, "observation_id").clone()),
                        ("source_review", string("Fixture review")),
                        ("candidates", Value::Array(vec![])),
                    ]),
                ),
            ]);
            assert!(
                turn_sources::verify_assessment(
                    &f.repo,
                    &assessment,
                    text(&first, "observation_id"),
                    &f.runtime
                )
                .is_err()
            );
            assert!(register(&"b".repeat(64)).is_err());
            assert_eq!(
                std::fs::read(h.directory.join(text(&h.original, "blob"))).unwrap(),
                h.body
            );
            assert!(
                !std::fs::read_dir(&h.directory).unwrap().any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".done"))
            );
        },
    );
}

// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_unknown_missing_corrupt_hold_and_frontier_only_change_refuse
#[test]
#[ignore = "recovery_frontier"]
fn missing_or_corrupt_hold_and_frontier_only_changes_refuse() {
    isolated(
        "missing_or_corrupt_hold_and_frontier_only_changes_refuse",
        |root| {
            let h = held_fixture(root);
            let f = &h.fixture;
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            for target in [
                f.repo.join("memory-bank/.agent-memory-transaction.md"),
                f.repo.join("memory-bank/.agent-memory-recovery.md"),
                h.directory.join("journal.json"),
                h.directory.join("ledger-head.json"),
            ] {
                let retained = std::fs::read(&target).unwrap();
                for missing in [true, false] {
                    if missing {
                        std::fs::remove_file(&target).unwrap();
                    } else {
                        write(&target, b"{}\n", 0o600);
                    }
                    assert!(
                        turn_sources::register(
                            &f.repo,
                            "claude",
                            &uid(),
                            &uid(),
                            &"a".repeat(64),
                            Some(&uid()),
                            None,
                            None,
                            &f.runtime
                        )
                        .is_err()
                    );
                    write(&target, &retained, 0o600);
                }
            }
            let mut index =
                agent_memory::layout_recovery_state::index(&f.repo, &f.runtime).unwrap();
            let previous_index = transport::digest(&index).unwrap();
            let old_head = transport::read(&h.directory.join("ledger-head.json"), 65536).unwrap();
            let ledger =
                transport::read(&h.directory.join("ledger/00000000.json"), 16 * 1024 * 1024)
                    .unwrap();
            let mut next = ledger.clone();
            set(&mut next, "generation", Value::Int(1u8.into()));
            set(
                &mut next,
                "previous_generation_sha256",
                string(&transport::digest(&ledger).unwrap()),
            );
            agent_memory::layout_recovery_state::write_once(
                &h.directory.join("ledger/00000001.json"),
                &next,
            )
            .unwrap();
            let mut head = old_head.clone();
            set(&mut head, "generation", Value::Int(1u8.into()));
            set(
                &mut head,
                "sha256",
                string(&transport::digest(&next).unwrap()),
            );
            agent_memory::layout_recovery_state::replace(
                &h.directory.join("ledger-head.json"),
                &head,
                Some(&transport::digest(&old_head).unwrap()),
            )
            .unwrap();
            set(
                index
                    .get_mut("operations")
                    .unwrap()
                    .get_mut(&h.operation)
                    .unwrap(),
                "ledger_head_sha256",
                string(&transport::digest(&head).unwrap()),
            );
            agent_memory::layout_recovery_state::set_index(
                &f.repo,
                &index,
                Some(&previous_index),
                &f.runtime,
            )
            .unwrap();
            assert!(agent_memory::scope_binding::recheck_private_enrollment(&context).is_err());
            let current =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            assert!(current.held());
            assert_ne!(current.context_digest(), context.context_digest());
            assert_eq!(
                std::fs::read(h.directory.join(text(&h.original, "blob"))).unwrap(),
                h.body
            );
        },
    );
}

fn snapshot(directory: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut result = std::collections::BTreeMap::new();
    if directory.is_dir() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                result.extend(snapshot(&path));
            } else if path.is_file() {
                result.insert(path.clone(), std::fs::read(path).unwrap());
            }
        }
    }
    result
}
fn register_child(
    root: &Path,
    f: &Fixture,
    session: &str,
    life: &str,
    native: &str,
    fault: Option<(&str, &str)>,
) -> agent_memory::process::Output {
    let mut env = std::env::vars_os().collect::<Vec<_>>();
    for (key, value) in [
        (
            "AGENT_MEMORY_FAULT_REGISTER_REPO",
            f.repo.as_os_str().to_owned(),
        ),
        (
            "AGENT_MEMORY_FAULT_REGISTER_RUNTIME",
            f.runtime.as_os_str().to_owned(),
        ),
        ("AGENT_MEMORY_FAULT_REGISTER_SESSION", session.into()),
        ("AGENT_MEMORY_FAULT_REGISTER_LIFE", life.into()),
        ("AGENT_MEMORY_FAULT_REGISTER_NATIVE", native.into()),
    ] {
        env.push((key.into(), value));
    }
    if let Some((stage, mode)) = fault {
        env.push(("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()));
        env.push(("AGENT_MEMORY_FAULT_MODE".into(), mode.into()));
    }
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "held_register_child".into(),
        "--nocapture".into(),
    ];
    let spec = agent_memory::process::Spec {
        argv: &argv,
        env: &env,
        cwd: Some(root),
        search_path: false,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        stderr: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(30)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    agent_memory::process::run_for(&spec, Duration::from_secs(30)).unwrap()
}
#[test]
fn held_register_child() {
    let Some(repo) = std::env::var_os("AGENT_MEMORY_FAULT_REGISTER_REPO") else {
        return;
    };
    support::require_current_world().unwrap();
    agent_memory::cli::register_providers().unwrap();
    let runtime = PathBuf::from(std::env::var_os("AGENT_MEMORY_FAULT_REGISTER_RUNTIME").unwrap());
    let session = std::env::var("AGENT_MEMORY_FAULT_REGISTER_SESSION").unwrap();
    let life = std::env::var("AGENT_MEMORY_FAULT_REGISTER_LIFE").unwrap();
    let native = std::env::var("AGENT_MEMORY_FAULT_REGISTER_NATIVE").unwrap();
    match turn_sources::register(
        Path::new(&repo),
        "claude",
        &session,
        &life,
        &"a".repeat(64),
        Some(&native),
        Some("/private/fixture.jsonl"),
        None,
        &runtime,
    ) {
        Ok(source) => {
            write(
                &PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap())
                    .join(format!("registered-{session}.json")),
                turn_sources::encoded(&source).unwrap(),
                0o600,
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
fn held_fault_boundaries(root: &Path, mode: &str, stages: &[&str]) {
    let h = held_fixture(root);
    let f = &h.fixture;
    let bank = f.repo.join("memory-bank");
    let before = snapshot(&bank);
    let index = std::fs::read(f.repo.join(".git/index")).unwrap();
    for stage in stages {
        let session = uid();
        let life = uid();
        let native = uid();
        let failed = register_child(
            root,
            f,
            &session,
            &life,
            &native,
            Some((&format!("hold-source-{stage}"), mode)),
        );
        assert_eq!(
            failed.status,
            agent_memory::process::Status::Code(if mode == "exit" { 86 } else { 1 }),
            "{}",
            String::from_utf8_lossy(&failed.stderr)
        );
        let (ns, _) =
            turn_sources::private_namespace(&f.repo, "claude", &session, &life, false, &f.runtime)
                .unwrap();
        let intents = turn_sources::enrollment_inventory(&ns.directory, &ns.identity).unwrap();
        let original = intents.first().map(|i| get(i, "observation_id").clone());
        if let Some(original) = &original {
            let backlog =
                turn_sources::backlog(&f.repo, Some("claude"), 0, 0, 0, &f.runtime).unwrap();
            assert!(
                get(&backlog, "turns")
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| get(row, "pending")
                        .as_array()
                        .is_some_and(|p| p.iter().any(|i| get(i, "observation_id") == original)))
            );
        }
        let completed = register_child(root, f, &session, &life, &native, None);
        assert_eq!(
            completed.status,
            agent_memory::process::Status::Code(0),
            "{}",
            String::from_utf8_lossy(&completed.stderr)
        );
        let source =
            transport::read(&root.join(format!("registered-{session}.json")), 4096).unwrap();
        if let Some(original) = original {
            assert_eq!(get(&source, "observation_id"), &original);
        }
        let state = turn_sources::inspect(&f.repo, "claude", &session, &life, &f.runtime).unwrap();
        assert_eq!(get(&state, "sources").as_array().unwrap().len(), 1);
        assert_eq!(get(&state, "complete"), &Value::Bool(false));
        assert_eq!(
            text(
                &turn_sources::enrollment_inventory(&ns.directory, &ns.identity).unwrap()[0],
                "state"
            ),
            "registered"
        );
    }
    assert_eq!(snapshot(&bank), before);
    assert_eq!(std::fs::read(f.repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read(h.directory.join(text(&h.original, "blob"))).unwrap(),
        h.body
    );
}
// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_error_before_source_boundaries_reconcile_one_original_observation
#[test]
#[ignore = "recovery_frontier"]
fn error_before_source_boundaries_reconciles_original_observation() {
    isolated(
        "error_before_source_boundaries_reconciles_original_observation",
        |root| {
            held_fault_boundaries(
                root,
                "error",
                &["before-intent", "after-intent", "before-index"],
            )
        },
    );
}
// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_error_source_publication_boundaries_reconcile_one_original_observation
#[test]
#[ignore = "recovery_frontier"]
fn error_source_publication_boundaries_reconcile_original_observation() {
    isolated(
        "error_source_publication_boundaries_reconcile_original_observation",
        |root| {
            held_fault_boundaries(
                root,
                "error",
                &[
                    "before-source",
                    "after-source",
                    "after-index",
                    "before-final-recheck",
                ],
            )
        },
    );
}
// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_before_source_boundaries_reconcile_one_original_observation
#[test]
#[ignore = "recovery_frontier"]
fn hard_exit_before_source_boundaries_reconciles_original_observation() {
    isolated(
        "hard_exit_before_source_boundaries_reconciles_original_observation",
        |root| {
            held_fault_boundaries(
                root,
                "exit",
                &["before-intent", "after-intent", "before-index"],
            )
        },
    );
}
// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_source_publication_boundaries_reconcile_one_original_observation
#[test]
#[ignore = "recovery_frontier"]
fn hard_exit_source_publication_boundaries_reconcile_original_observation() {
    isolated(
        "hard_exit_source_publication_boundaries_reconcile_original_observation",
        |root| {
            held_fault_boundaries(
                root,
                "exit",
                &[
                    "before-source",
                    "after-source",
                    "after-index",
                    "before-final-recheck",
                ],
            )
        },
    );
}

// frontier obligation: tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access
#[test]
#[ignore = "recovery_frontier"]
fn every_shared_reader_refuses_unknown_compatibility_before_index_access() {
    isolated(
        "every_shared_reader_refuses_unknown_compatibility_before_index_access",
        |root| {
            let f = Fixture::new(root);
            let source = turn_sources::register(
                &f.repo,
                "codex",
                SESSION,
                LIFE,
                &"a".repeat(64),
                None,
                None,
                None,
                &f.runtime,
            )
            .unwrap();
            let (ns, _) =
                turn_sources::private_namespace(&f.repo, "codex", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            let state = turn_sources::inspect(&f.repo, "codex", SESSION, LIFE, &f.runtime).unwrap();
            let assessment = object([
                ("observation_id", get(&source, "observation_id").clone()),
                ("source_sha256", string(&"a".repeat(64))),
                ("outcome", string("no-learning")),
                (
                    "assessment",
                    object([
                        ("source_id", get(&source, "observation_id").clone()),
                        (
                            "source_review",
                            string("Actual synthetic temporary source review"),
                        ),
                        ("candidates", Value::Array(vec![])),
                    ]),
                ),
            ]);
            let receipt = turn_sources::verify_assessment(
                &f.repo,
                &assessment,
                text(&source, "observation_id"),
                &f.runtime,
            )
            .unwrap();
            let marker = ns
                .base
                .parent()
                .unwrap()
                .join("turn-sources-compat-v1.json");
            let before_source = std::fs::read(ns.directory.join("source-0.json")).unwrap();
            let before_index = snapshot(&ns.base.join("pending"));
            for invalid in [b"{partial".to_vec(),br#"{"schema":"unknown"}"#.to_vec(),br#"{"schema":"agent-memory/turn-sources-compat-v1","minimum_reader":"future-reader","harnesses":["omp"]}"#.to_vec(),br#"{"schema":"agent-memory/turn-sources-compat-v1","minimum_reader":"omp-harness-v1","harnesses":["omp","codex"]}"#.to_vec()] {
            write(&marker,&invalid,0o600);
            assert!(turn_sources::register(&f.repo,"codex",SESSION,LIFE,&"b".repeat(64),None,None,None,&f.runtime).is_err());
            assert!(turn_sources::inspect(&f.repo,"codex",SESSION,LIFE,&f.runtime).is_err());
            assert!(turn_sources::backlog(&f.repo,None,0,0,0,&f.runtime).is_err());
            assert!(turn_sources::pending_notice(&f.repo,"codex",SESSION,LIFE,&f.runtime).is_err());
            assert!(turn_sources::verify_assessment(&f.repo,&assessment,text(&source,"observation_id"),&f.runtime).is_err());
            assert!(turn_sources::reserve_wake(&f.repo,"codex",SESSION,LIFE,text(&state,"fingerprint"),&"b".repeat(64),&f.runtime).is_err());
            assert!(turn_sources::complete(&f.repo,"codex",SESSION,LIFE,text(&source,"observation_id"),&"a".repeat(64),&receipt,&f.runtime).is_err());
            assert_eq!(std::fs::read(&marker).unwrap(),invalid); assert_eq!(std::fs::read(ns.directory.join("source-0.json")).unwrap(),before_source); assert_eq!(snapshot(&ns.base.join("pending")),before_index);
        }
            write(&marker,br#"{"schema":"agent-memory/turn-sources-compat-v1","minimum_reader":"omp-harness-v1","harnesses":["omp"]}"#,0o600);
            assert_eq!(
                get(
                    &turn_sources::inspect(&f.repo, "codex", SESSION, LIFE, &f.runtime).unwrap(),
                    "sources"
                )
                .as_array()
                .unwrap()
                .len(),
                1
            );
            std::fs::remove_file(marker).unwrap();
            assert_eq!(
                get(
                    &turn_sources::inspect(&f.repo, "codex", SESSION, LIFE, &f.runtime).unwrap(),
                    "sources"
                )
                .as_array()
                .unwrap()
                .len(),
                1
            );
        },
    );
}

fn decision(coverage: &stream::Coverage, sequence: u64) -> Value {
    transport::read(
        &stream::wake_decision_directory(coverage.directory())
            .unwrap()
            .join(format!("{sequence}.json")),
        4096,
    )
    .unwrap()
}
fn wake_summary(f: &Fixture) -> Value {
    get(
        &omp_installation::diagnostics(&f.repo, Some(&f.receipt)).unwrap(),
        "wake_decisions",
    )
    .clone()
}
fn execute_seal(
    f: &Fixture,
    coverage: &mut stream::Coverage,
    executor: &stream::FiniteExecutor,
    entries: &[Value],
    armed: bool,
    sequence: u64,
) {
    let mut value = request(f, entries, "seal");
    set(
        value.get_mut("payload").unwrap(),
        "request_wake",
        Value::Bool(armed),
    );
    executor
        .execute(
            coverage,
            "REQUEST",
            sequence,
            None,
            &turn_sources::encoded(&envelope(value, sequence)).unwrap(),
        )
        .unwrap();
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_not_armed_is_distinct_from_armed_without_nonce
#[test]
#[ignore = "recovery_frontier"]
fn not_armed_differs_from_armed_without_nonce() {
    isolated("not_armed_differs_from_armed_without_nonce", |root| {
        let f = Fixture::new(root);
        let (mut coverage, executor, _) = finite(&f);
        let entries = [human("human"), answer("answer")];
        execute_seal(&f, &mut coverage, &executor, &entries, false, 1);
        execute_seal(&f, &mut coverage, &executor, &entries, true, 2);
        let first = decision(&coverage, 1);
        let second = decision(&coverage, 2);
        assert_eq!(text(&first, "status"), "not-armed");
        assert_eq!(text(&second, "status"), "pending-undeliverable-window");
        for value in [&first, &second] {
            assert_eq!(get(value, "continuation_offered"), &Value::Bool(false));
            assert_eq!(get(value, "human_root"), &Value::Null);
            assert_eq!(get(value, "root_wake_sha256"), &Value::Null);
        }
        let summary = wake_summary(&f);
        assert_eq!(
            get(get(&summary, "counts"), "not-armed"),
            &Value::Int(1u8.into())
        );
        assert_eq!(
            get(get(&summary, "counts"), "pending-undeliverable-window"),
            &Value::Int(1u8.into())
        );
        assert!(
            !transport::session_dir(&f.site(), SESSION)
                .unwrap()
                .join("root-wakes")
                .exists()
        );
        coverage.close_live();
    });
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_empty_verified_seal_reports_wake_not_evaluated
#[test]
#[ignore = "recovery_frontier"]
fn empty_verified_seal_reports_wake_not_evaluated() {
    isolated("empty_verified_seal_reports_wake_not_evaluated", |root| {
        let f = Fixture::new(root);
        let (mut coverage, executor, writer) = finite(&f);
        execute_seal(&f, &mut coverage, &executor, &[], true, 1);
        assert_eq!(
            text(
                &transport::read(
                    &coverage.directory().join("outcomes/1.json"),
                    stream::MAX_METADATA
                )
                .unwrap(),
                "outcome"
            ),
            "verified"
        );
        assert!(queued_result(&writer).get("continue").is_none());
        assert_eq!(
            text(&decision(&coverage, 1), "status"),
            "wake-not-evaluated"
        );
        assert_eq!(
            get(get(&wake_summary(&f), "counts"), "wake-not-evaluated"),
            &Value::Int(1u8.into())
        );
        assert!(
            !transport::session_dir(&f.site(), SESSION)
                .unwrap()
                .join("root-wakes")
                .exists()
        );
        coverage.close_live();
    });
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_conflicting_existing_record_is_never_rewritten_or_replayed
#[test]
#[ignore = "recovery_frontier"]
fn conflicting_optional_record_never_rewrites_or_replays_source() {
    isolated(
        "conflicting_optional_record_never_rewrites_or_replays_source",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, executor, writer) = finite(&f);
            let path = stream::wake_decision_directory(coverage.directory())
                .unwrap()
                .join("1.json");
            let conflict = br#"{"arbitrary":"do-not-disclose"}"#;
            write(&path, conflict, 0o600);
            execute_seal(
                &f,
                &mut coverage,
                &executor,
                &[human("human"), answer("answer")],
                true,
                1,
            );
            assert_eq!(get(&queued_result(&writer), "verified"), &Value::Bool(true));
            assert_eq!(std::fs::read(&path).unwrap(), conflict);
            assert_eq!(f.pending().len(), 2);
            assert_eq!(
                std::fs::read_dir(coverage.directory().join("outcomes"))
                    .unwrap()
                    .count(),
                1
            );
            let summary = wake_summary(&f);
            assert_eq!(
                get(get(&summary, "counts"), "unbound"),
                &Value::Int(1u8.into())
            );
            assert!(
                !json::encode(&summary, json::DEFAULT)
                    .unwrap()
                    .contains("do-not-disclose")
            );
            coverage.close_live();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_false_observed_send_leaves_decision_unrecorded
// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_failed_observed_send_leaves_decision_unrecorded_and_still_drains
#[test]
#[ignore = "recovery_frontier"]
fn real_closed_writer_leaves_decision_unrecorded_and_still_drains() {
    isolated(
        "real_closed_writer_leaves_decision_unrecorded_and_still_drains",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, executor, writer) = finite(&f);
            stream::Responder::finish(writer.as_ref());
            execute_seal(
                &f,
                &mut coverage,
                &executor,
                &[human("human"), answer("answer")],
                true,
                1,
            );
            assert!(
                !stream::wake_decision_directory(coverage.directory())
                    .unwrap()
                    .join("1.json")
                    .exists()
            );
            assert_eq!(
                text(
                    &transport::read(
                        &coverage.directory().join("outcomes/1.json"),
                        stream::MAX_METADATA
                    )
                    .unwrap(),
                    "outcome"
                ),
                "verified"
            );
            assert!(
                f.pending()
                    .iter()
                    .all(|r| get(r, "observation_id").as_str().is_some())
            );
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "unrecorded"),
                &Value::Int(1u8.into())
            );
            coverage.close_live();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_failure_after_response_preserves_source_root_and_drain
#[test]
#[ignore = "recovery_frontier"]
fn non_directory_optional_writer_failure_preserves_verified_source_and_drain() {
    isolated(
        "non_directory_optional_writer_failure_preserves_verified_source_and_drain",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, executor, writer) = finite(&f);
            let path = stream::wake_decision_directory(coverage.directory()).unwrap();
            write(&path, b"retained blocking evidence", 0o600);
            execute_seal(
                &f,
                &mut coverage,
                &executor,
                &[human("human"), answer("answer")],
                true,
                1,
            );
            assert_eq!(get(&queued_result(&writer), "verified"), &Value::Bool(true));
            assert_eq!(std::fs::read(&path).unwrap(), b"retained blocking evidence");
            assert!(
                f.pending()
                    .iter()
                    .all(|r| get(r, "observation_id").as_str().is_some())
            );
            assert_eq!(
                text(
                    &transport::read(
                        &coverage.directory().join("outcomes/1.json"),
                        stream::MAX_METADATA
                    )
                    .unwrap(),
                    "outcome"
                ),
                "verified"
            );
            coverage.close_live();
        },
    );
}

impl Fixture {
    fn load(root: &Path) -> Self {
        let receipt = transport::read(
            &root.join("state/agent-memory/install/receipt.json"),
            16 * 1024 * 1024,
        )
        .unwrap();
        let runtime = PathBuf::from(text(get(&receipt, "source"), "pack"));
        install_activation(&receipt);
        let proof = Arc::new(
            omp_installation::verified_observer_binding(
                text(get(&receipt, "source"), "artifact_digest"),
                text(get(&receipt, "omp_binding"), "agent_dir"),
                Some(&receipt),
            )
            .unwrap(),
        );
        Self {
            repo: root.join("repo"),
            runtime,
            receipt,
            proof,
        }
    }
}
struct OperationChild {
    child: agent_memory::process::OwnedProcess,
    deadline: agent_memory::clock::Deadline,
}
impl OperationChild {
    fn wait(self) -> agent_memory::process::Output {
        self.child.finish(self.deadline).unwrap()
    }
}
fn operation_child(
    root: &Path,
    stage: Option<&str>,
    mode: &str,
    operation: &str,
) -> OperationChild {
    let mut env = std::env::vars_os().collect::<Vec<_>>();
    env.push(("AGENT_MEMORY_FAULT_OMP_OPERATION".into(), operation.into()));
    if let Some(stage) = stage {
        env.push(("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()));
        env.push(("AGENT_MEMORY_FAULT_MODE".into(), mode.into()));
        env.push((
            "AGENT_MEMORY_FAULT_DIR".into(),
            root.join("rendezvous").into_os_string(),
        ));
    }
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "native_operation_child".into(),
        "--nocapture".into(),
    ];
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(40)).unwrap();
    let spec = agent_memory::process::Spec {
        argv: &argv,
        env: &env,
        cwd: Some(root),
        search_path: false,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        stderr: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        deadline,
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    OperationChild {
        child: agent_memory::process::spawn_owned(&spec).unwrap(),
        deadline,
    }
}
fn wait_path(path: &Path) {
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(12)).unwrap();
    while !path.exists() {
        assert!(
            !deadline.expired(),
            "Rendezvous not reached: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn release(root: &Path, stage: &str) {
    write(
        &root.join("rendezvous").join(format!("{stage}.release")),
        b"",
        0o600,
    );
}
fn ready(root: &Path, stage: &str) {
    wait_path(&root.join("rendezvous").join(format!("{stage}.ready")));
}
#[test]
fn native_operation_child() {
    let Ok(operation) = std::env::var("AGENT_MEMORY_FAULT_OMP_OPERATION") else {
        return;
    };
    let root = support::require_current_world().unwrap();
    agent_memory::cli::register_providers().unwrap();
    if operation.starts_with("registry-") {
        registry_operation(&root, &operation);
        return;
    }
    let mut f = Fixture::load(&root);
    if let Ok(alias) = std::fs::read_to_string(root.join("case-observer-repo")) {
        f.repo = PathBuf::from(alias);
    }
    let request = transport::read(&root.join("case-request.json"), transport::MAX_PAYLOAD).unwrap();
    write(
        &root.join("operation-pid"),
        std::process::id().to_string(),
        0o600,
    );
    if operation.starts_with("enrollment-") {
        enrollment_operation(&root, &f, &request, &operation);
        return;
    }
    if operation == "manual-verify" {
        let witness =
            omp_installation::owned_manual(&f.receipt, text(&request, "relative")).unwrap();
        let valid = witness
            .as_ref()
            .is_some_and(|w| w.verify().unwrap().is_some());
        write(
            &root.join("operation-result.json"),
            turn_sources::encoded(&Value::Bool(valid)).unwrap(),
            0o600,
        );
        return;
    }
    if operation == "wake-observe" {
        wake_operation(&root, &f, &request);
        return;
    }
    if matches!(operation.as_str(), "compatibility" | "source-inspect") {
        public_observation_operation(&root, &f, &operation);
        return;
    }
    if operation == "drain" {
        let result = boundary::drain_source_capabilities(&f.repo, true, true, &f.runtime);
        write(
            &root.join("operation-result.json"),
            turn_sources::encoded(&match result {
                Ok(value) => value,
                Err(error) => object([("error", string(&error.to_string()))]),
            })
            .unwrap(),
            0o600,
        );
        return;
    }
    if matches!(operation.as_str(), "finite-run" | "finite-run-open") {
        run_finite_child(&root, &f, request, operation == "finite-run-open");
        return;
    }
    if operation == "observe" {
        let result = transport::observe(
            &f.repo,
            &f.runtime,
            &request,
            transport::Observation {
                receipt: Some(&f.receipt),
                on_started: Some(&mut || {
                    write(&root.join("observer-started"), b"", 0o600);
                    Ok(())
                }),
                ..Default::default()
            },
            &transport::InstalledScope {
                receipt: Some(&f.receipt),
                proof: &f.proof,
                runtime_root: &f.runtime,
            },
        );
        write(
            &root.join("operation-result.json"),
            turn_sources::encoded(&match result {
                Ok(value) => value,
                Err(error) => object([("error", string(&error.to_string()))]),
            })
            .unwrap(),
            0o600,
        );
    } else if operation == "join" {
        let path = PathBuf::from(text(&request, "directory"));
        let cursor = get(&request, "cursor");
        let context =
            agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).unwrap();
        let barrier =
            turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
        let result = agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
            agent_memory::scope_binding::observe_enrollment_locations(context, || {
                transport::segment_inventory(&f.site().with(context), &path, cursor)
            })
        });
        barrier.recheck().unwrap();
        write(
            &root.join("operation-result.json"),
            turn_sources::encoded(&match result {
                Ok(rows) => Value::Array(rows),
                Err(error) => object([("error", string(&error.to_string()))]),
            })
            .unwrap(),
            0o600,
        );
    } else {
        let instance = uid();
        let binding =
            boundary::stream_startup(&f.repo, SESSION, &instance, LIFE, &f.receipt, &f.runtime)
                .unwrap()
                .unwrap();
        let mut coverage = stream::Coverage::open(
            Arc::new(stream::Installed::new(&f.runtime)),
            &f.repo,
            SESSION,
            &instance,
            LIFE,
            &binding,
            Some(&f.receipt),
        )
        .unwrap();
        write(
            &root.join("finite-directory"),
            coverage.directory().to_str().unwrap(),
            0o600,
        );
        let writer = if root.join("case-file-responses").exists() {
            Arc::new(stream::ResponseWriter::start(
                stream::Output::Stream(Box::new(
                    std::fs::File::create(root.join("live-responses.raw")).unwrap(),
                )),
                &instance,
            ))
        } else {
            Arc::new(stream::ResponseWriter::unstarted(&instance, vec![]))
        };
        let recipient = writer.clone();
        let clocks = stream::ClockWindows::new(
            &instance,
            Arc::new(move || recipient.live()),
            stream::Platform::Darwin,
            Arc::new(agent_memory::clock::monotonic_ns),
            None,
        );
        let nonce = "12345678-1234-4567-8901-123456789012";
        let controller = start_window_controller(&root, &clocks, &writer, nonce);
        let calibrator = if matches!(
            operation.as_str(),
            "finite-calibrated" | "finite-auto-calibrated"
        ) {
            let owner = clocks.clone();
            let response = writer.clone();
            let root = root.clone();
            let automatic = operation == "finite-auto-calibrated";
            let source_directory = transport::session_dir(&f.site(), SESSION).unwrap();
            Some(std::thread::spawn(move || {
                if automatic {
                    let deadline =
                        agent_memory::clock::Deadline::after(Duration::from_secs(12)).unwrap();
                    loop {
                        let sealed =
                            std::fs::read_dir(&source_directory)
                                .ok()
                                .is_some_and(|entries| {
                                    entries
                                        .filter_map(|e| e.ok())
                                        .any(|e| e.path().join("segment.json").is_file())
                                });
                        if sealed {
                            break;
                        }
                        assert!(!deadline.expired(), "Native seal never became durable");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                } else {
                    wait_path(&root.join("calibrate-now"));
                }
                let now = agent_memory::clock::monotonic_ns();
                owner
                    .control(
                        "CLOCK",
                        &object([
                            ("nonce", string(nonce)),
                            ("j0", string(&now.to_string())),
                            ("j_deadline", string(&(now + 950_000_000).to_string())),
                        ]),
                        response.as_ref(),
                    )
                    .unwrap();
                let ack = agent_memory::clock::monotonic_ns();
                owner
                    .control(
                        "CLOCK-ACK",
                        &object([
                            ("nonce", string(nonce)),
                            ("j0", string(&now.to_string())),
                            ("j3", string(&ack.to_string())),
                        ]),
                        response.as_ref(),
                    )
                    .unwrap();
                write(&root.join("calibrated"), b"", 0o600);
            }))
        } else {
            None
        };
        let executor = stream::FiniteExecutor::new(
            writer.clone(),
            clocks.clone(),
            Some(f.receipt.clone()),
            None,
            stream::Timing::system(),
        );
        let invalid = operation.starts_with("finite-invalid-");
        let frame_kind = if operation == "finite-invalid-refused" {
            "REFUSED-ATTEMPT"
        } else {
            "REQUEST"
        };
        let raw = if invalid {
            b"{".to_vec()
        } else {
            turn_sources::encoded(&envelope(request, 1)).unwrap()
        };
        let result = executor.execute(
            &mut coverage,
            frame_kind,
            1,
            if invalid { None } else { Some(nonce) },
            &raw,
        );
        if root.join("case-followup.json").exists() {
            let followup =
                transport::read(&root.join("case-followup.json"), transport::MAX_PAYLOAD).unwrap();
            executor
                .execute(
                    &mut coverage,
                    "REQUEST",
                    2,
                    None,
                    &turn_sources::encoded(&envelope(followup, if invalid { 1 } else { 2 }))
                        .unwrap(),
                )
                .unwrap();
            assert!(
                coverage
                    .close(&object([
                        ("last_attempt_seq", Value::Int(2u8.into())),
                        (
                            "refused_count",
                            Value::Int(u8::from(frame_kind == "REFUSED-ATTEMPT").into())
                        ),
                        (
                            "last_event_ordinal",
                            Value::Int(if invalid { 1u8 } else { 2u8 }.into())
                        ),
                        ("transition", Value::Null)
                    ]))
                    .unwrap()
            );
        }
        if root.join("case-close-first").exists() && result.is_ok() {
            assert!(coverage.close(&decode(r#"{"last_attempt_seq":1,"refused_count":0,"last_event_ordinal":1,"transition":null}"#)).unwrap());
        }
        let status = match result {
            Ok(()) => object([("ok", Value::Bool(true))]),
            Err(error) => error_value(&error),
        };
        write(
            &root.join("operation-result.json"),
            turn_sources::encoded(&status).unwrap(),
            0o600,
        );
        write(
            &root.join("responses.json"),
            turn_sources::encoded(&Value::Array(
                writer
                    .queued()
                    .iter()
                    .map(|(_, raw)| {
                        let start = raw.iter().position(|b| *b == b'\n').unwrap() + 1;
                        json::decode_bytes(&raw[start..], Default::default()).unwrap()
                    })
                    .collect(),
            ))
            .unwrap(),
            0o600,
        );
        write(
            &root.join("responses.raw"),
            writer
                .queued()
                .into_iter()
                .flat_map(|(_, bytes)| bytes)
                .collect::<Vec<_>>(),
            0o600,
        );
        write(&root.join("execute-finished"), b"", 0o600);
        if root.join("case-close-after-release").exists() {
            wait_path(&root.join("allow-close"));
            let closed = coverage.close(&decode(r#"{"last_attempt_seq":1,"refused_count":0,"last_event_ordinal":1,"transition":null}"#)).unwrap();
            write(
                &root.join("close-result.json"),
                turn_sources::encoded(&Value::Bool(closed)).unwrap(),
                0o600,
            );
        }
        coverage.close_live();
        if let Some(c) = calibrator {
            c.join().unwrap();
        }
        if let Some(controller) = controller {
            controller.join().unwrap();
        }
        clocks.end();
        if root.join("case-file-responses").exists() {
            writer.finish();
        }
    }
}
fn child_directory(root: &Path) -> PathBuf {
    PathBuf::from(std::fs::read_to_string(root.join("finite-directory")).unwrap())
}
fn armed_request(f: &Fixture) -> Value {
    let mut value = request(f, &[human("phase-human"), answer("phase-answer")], "seal");
    set(
        value.get_mut("payload").unwrap(),
        "request_wake",
        Value::Bool(true),
    );
    value
}
fn no_verified_response(root: &Path) {
    let responses = transport::read(&root.join("responses.json"), 1024 * 1024).unwrap();
    assert!(
        !responses
            .as_array()
            .unwrap()
            .iter()
            .any(|r| get(get(r, "result"), "verified") == &Value::Bool(true))
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_nonbusy_outcome_failure_never_writes_second_outcome_or_verified_response
#[test]
#[ignore = "recovery_frontier"]
fn nonbusy_outcome_exit_failure_never_publishes_verified_response() {
    isolated(
        "nonbusy_outcome_exit_failure_never_publishes_verified_response",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&armed_request(&f)).unwrap(),
                0o600,
            );
            let child = operation_child(root, Some("omp.outcome.after-commit"), "error", "finite");
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let directory = child_directory(root);
            assert_eq!(
                text(
                    &transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA)
                        .unwrap(),
                    "outcome"
                ),
                "verified"
            );
            assert_eq!(
                std::fs::read_dir(directory.join("outcomes"))
                    .unwrap()
                    .count(),
                1
            );
            no_verified_response(root);
            assert!(
                !transport::session_dir(&f.site(), SESSION)
                    .unwrap()
                    .join("root-wakes")
                    .exists()
            );
            assert!(
                f.pending()
                    .iter()
                    .all(|r| r.get("observation_id").is_none())
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_captured_source_checkpoint_failure_does_not_verify_or_schedule_drain
#[test]
#[ignore = "recovery_frontier"]
fn captured_source_checkpoint_failure_never_verifies_or_drains() {
    isolated(
        "captured_source_checkpoint_failure_never_verifies_or_drains",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&armed_request(&f)).unwrap(),
                0o600,
            );
            let child = operation_child(
                root,
                Some("omp.checkpoint.before-recheck"),
                "error",
                "finite",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let directory = child_directory(root);
            let outcome =
                transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap();
            assert_ne!(text(&outcome, "outcome"), "verified");
            no_verified_response(root);
            assert!(
                f.pending()
                    .iter()
                    .all(|r| r.get("observation_id").is_none())
            );
            assert!(
                !transport::session_dir(&f.site(), SESSION)
                    .unwrap()
                    .join("root-wakes")
                    .exists()
            );
        },
    );
}

fn checkpoint_swap(root: &Path, name: &str) {
    let f = Fixture::new(root);
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&armed_request(&f)).unwrap(),
        0o600,
    );
    let stage = "omp.checkpoint.before-recheck";
    let child = operation_child(root, Some(stage), "pause", "finite");
    ready(root, stage);
    let base = boundary::base(&f.repo).unwrap();
    let path = base.join(name);
    let old = path.with_extension("held");
    std::fs::rename(&path, &old).unwrap();
    write(&path, b"", 0o600);
    release(root, stage);
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    let directory = child_directory(root);
    if let Ok(outcome) = transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA) {
        assert_ne!(text(&outcome, "outcome"), "verified");
    }
    no_verified_response(root);
    assert!(
        !transport::session_dir(&f.site(), SESSION)
            .unwrap()
            .join("root-wakes")
            .exists()
    );
    std::fs::remove_file(path).unwrap();
    std::fs::rename(old, base.join(name)).unwrap();
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_executor_barrier_fd_swap
#[test]
#[ignore = "recovery_frontier"]
fn checkpoint_rejects_actual_executor_barrier_inode_swap() {
    isolated(
        "checkpoint_rejects_actual_executor_barrier_inode_swap",
        |root| checkpoint_swap(root, "source-barrier.lock"),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_transport_fd_swap
#[test]
#[ignore = "recovery_frontier"]
fn checkpoint_rejects_actual_transport_inode_swap() {
    isolated("checkpoint_rejects_actual_transport_inode_swap", |root| {
        checkpoint_swap(root, "omp-transport.lock")
    });
}

fn calibrated_child(root: &Path, f: &Fixture, mutate: impl FnOnce(&Path)) -> PathBuf {
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&armed_request(f)).unwrap(),
        0o600,
    );
    let stage = "omp.reservation.before-reserve";
    let child = operation_child(root, Some(stage), "pause", "finite-calibrated");
    ready(root, stage);
    let directory = child_directory(root);
    assert!(directory.join("started/1.json").is_file());
    assert!(directory.join("outcomes/1.json").is_file());
    write(&root.join("calibrate-now"), b"", 0o600);
    wait_path(&root.join("calibrated"));
    mutate(&directory);
    release(root, stage);
    let output = child.wait();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    directory
}
fn child_result(root: &Path) -> Value {
    let responses = transport::read(&root.join("responses.json"), 1024 * 1024).unwrap();
    responses
        .as_array()
        .unwrap()
        .iter()
        .find_map(|r| r.get("result"))
        .unwrap()
        .clone()
}
fn root_wakes(f: &Fixture) -> Vec<PathBuf> {
    let path = transport::session_dir(&f.site(), SESSION)
        .unwrap()
        .join("root-wakes");
    if !path.is_dir() {
        return vec![];
    }
    std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect()
}
fn assert_verified_pending_source(f: &Fixture, directory: &Path) {
    assert_eq!(
        text(
            &transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap(),
            "outcome"
        ),
        "verified"
    );
    assert_eq!(
        std::fs::read_dir(directory.join("outcomes"))
            .unwrap()
            .count(),
        1
    );
    let rows = f.pending();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| get(r, "observation_id").as_str().is_some())
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_ack_after_source_started_resolves_only_at_reservation
#[test]
#[ignore = "recovery_frontier"]
fn late_real_clock_ack_is_resolved_at_reservation() {
    isolated("late_real_clock_ack_is_resolved_at_reservation", |root| {
        let f = Fixture::new(root);
        let directory = calibrated_child(root, &f, |_| {});
        assert_eq!(get(&child_result(root), "continue"), &Value::Bool(true));
        assert_eq!(root_wakes(&f).len(), 1);
        assert_verified_pending_source(&f, &directory);
    });
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_offered_binds_actual_root_after_response_without_changing_old_evidence
#[test]
#[ignore = "recovery_frontier"]
fn offered_decision_binds_actual_root_and_preserves_old_evidence() {
    isolated(
        "offered_decision_binds_actual_root_and_preserves_old_evidence",
        |root| {
            let f = Fixture::new(root);
            let directory = calibrated_child(root, &f, |_| {});
            let path = stream::wake_decision_directory(&directory)
                .unwrap()
                .join("1.json");
            let value = transport::read(&path, 4096).unwrap();
            let roots = root_wakes(&f);
            assert_eq!(roots.len(), 1);
            let actual = transport::read(&roots[0], 4096).unwrap();
            let result = child_result(root);
            assert_eq!(get(&actual, "request_id"), get(&result, "request_id"));
            assert_eq!(get(&value, "human_root"), get(&actual, "human_root"));
            assert_eq!(
                text(&value, "root_wake_sha256"),
                transport::digest(&actual).unwrap()
            );
            for (key, name) in [
                ("open_sha256", "open.json"),
                ("attempt_sha256", "attempts/1.json"),
                ("outcome_sha256", "outcomes/1.json"),
            ] {
                assert_eq!(
                    text(&value, key),
                    transport::digest(
                        &transport::read(&directory.join(name), stream::MAX_METADATA).unwrap()
                    )
                    .unwrap()
                );
            }
            assert_eq!(value.as_object().unwrap().len(), 9);
            assert_eq!(text(&value, "status"), "offered");
            assert_eq!(get(&value, "continuation_offered"), &Value::Bool(true));
            assert!(std::fs::metadata(&path).unwrap().len() <= 4096);
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            let before = snapshot(&directory);
            let summary = wake_summary(&f);
            assert_eq!(
                get(get(&summary, "counts"), "offered"),
                &Value::Int(1u8.into())
            );
            assert_eq!(text(&summary, "delivery"), "not-observed");
            assert_eq!(get(&summary, "assessment_authority"), &Value::Bool(false));
            assert_eq!(get(&summary, "completion_inferred"), &Value::Bool(false));
            assert_eq!(snapshot(&directory), before);
            let hidden = path
                .parent()
                .unwrap()
                .with_file_name("hidden-optional-records");
            std::fs::rename(path.parent().unwrap(), &hidden).unwrap();
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "unrecorded"),
                &Value::Int(1u8.into())
            );
            std::fs::rename(hidden, path.parent().unwrap()).unwrap();
            assert_verified_pending_source(&f, &directory);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_spoofed_record_keys_types_digests_and_status_are_unbound_without_contents
#[test]
#[ignore = "recovery_frontier"]
fn spoofed_diagnostic_keys_types_digests_and_status_are_unbound() {
    isolated(
        "spoofed_diagnostic_keys_types_digests_and_status_are_unbound",
        |root| {
            let f = Fixture::new(root);
            let directory = calibrated_child(root, &f, |_| {});
            let path = stream::wake_decision_directory(&directory)
                .unwrap()
                .join("1.json");
            let actual = transport::read(&path, 4096).unwrap();
            let variants = [
                ("private_payload", string("do-not-disclose")),
                ("status", string("do-not-disclose")),
                ("continuation_offered", Value::Int(1u8.into())),
                ("sequence", Value::Bool(true)),
                ("open_sha256", string(&"0".repeat(64))),
                ("attempt_sha256", string(&"0".repeat(64))),
                ("outcome_sha256", string(&"0".repeat(64))),
                ("root_wake_sha256", string(&"0".repeat(64))),
                ("human_root", string(&uid())),
            ];
            let authority = stream::Installed::new(&f.runtime);
            let inventory = stream::inventory(&authority, &f.repo, None, false).unwrap();
            for (key, new_value) in variants {
                let mut value = actual.clone();
                set(&mut value, key, new_value);
                write(&path, turn_sources::encoded(&value).unwrap(), 0o600);
                let summary = wake_summary(&f);
                assert_eq!(
                    get(get(&summary, "counts"), "unbound"),
                    &Value::Int(1u8.into())
                );
                assert!(
                    !json::encode(&summary, json::DEFAULT)
                        .unwrap()
                        .contains("do-not-disclose")
                );
                assert_eq!(
                    stream::inventory(&authority, &f.repo, None, false).unwrap(),
                    inventory
                );
            }
            let mut value = actual.clone();
            set(&mut value, "status", string("pending"));
            set(&mut value, "continuation_offered", Value::Bool(false));
            write(&path, turn_sources::encoded(&value).unwrap(), 0o600);
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "unbound"),
                &Value::Int(1u8.into())
            );
            let mut value = actual.clone();
            let Value::Object(pairs) = &mut value else {
                unreachable!()
            };
            pairs.retain(|(k, _)| k != "schema");
            write(&path, turn_sources::encoded(&value).unwrap(), 0o600);
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "unbound"),
                &Value::Int(1u8.into())
            );
            write(
                &path,
                json::encode(&actual, json::INDENT2_UTF8).unwrap(),
                0o600,
            );
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "offered"),
                &Value::Int(1u8.into())
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_canonical_parsed_evidence_digests_survive_noncanonical_json_encoding
#[test]
#[ignore = "recovery_frontier"]
fn parsed_evidence_digests_survive_noncanonical_json_encoding() {
    isolated(
        "parsed_evidence_digests_survive_noncanonical_json_encoding",
        |root| {
            let f = Fixture::new(root);
            let directory = calibrated_child(root, &f, |_| {});
            let authority = stream::Installed::new(&f.runtime);
            let inventory = stream::inventory(&authority, &f.repo, None, false).unwrap();
            let mut paths = [
                "open.json",
                "attempts/1.json",
                "started/1.json",
                "outcomes/1.json",
            ]
            .iter()
            .map(|name| directory.join(name))
            .collect::<Vec<_>>();
            paths.extend(root_wakes(&f));
            for path in paths {
                let value = transport::read(&path, stream::MAX_METADATA).unwrap();
                write(
                    &path,
                    json::encode(&value, json::INDENT2_UTF8).unwrap(),
                    0o600,
                );
            }
            assert_eq!(
                stream::inventory(&authority, &f.repo, None, false).unwrap(),
                inventory
            );
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "offered"),
                &Value::Int(1u8.into())
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_root_write_failure_after_actual_reservation_is_unknown_not_replayed
#[test]
#[ignore = "recovery_frontier"]
fn real_root_write_failure_after_reservation_stays_unknown() {
    isolated(
        "real_root_write_failure_after_reservation_stays_unknown",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&armed_request(&f)).unwrap(),
                0o600,
            );
            let child = operation_child(
                root,
                Some("omp.reservation.after-root"),
                "error",
                "finite-auto-calibrated",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let directory = child_directory(root);
            let result = child_result(root);
            assert_eq!(get(&result, "continue"), &Value::Bool(false));
            assert_eq!(
                text(&result, "status"),
                "pending-wake-unknown-after-reservation"
            );
            assert_eq!(root_wakes(&f).len(), 1);
            assert_verified_pending_source(&f, &directory);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_real_transport_lock_exit_swap_maps_committed_reservation_to_unknown
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_wake_and_observer_unwind_faults_preserve_source_and_actual_reservation_fact
#[test]
#[ignore = "recovery_frontier"]
fn real_transport_exit_swap_preserves_committed_source_and_spent_reservation() {
    isolated(
        "real_transport_exit_swap_preserves_committed_source_and_spent_reservation",
        |root| {
            let f = Fixture::new(root);
            let path = boundary::base(&f.repo).unwrap().join("omp-transport.lock");
            let old = path.with_extension("held");
            let directory = calibrated_child(root, &f, |_| {
                std::fs::rename(&path, &old).unwrap();
                write(&path, b"", 0o600);
            });
            let result = child_result(root);
            assert_eq!(get(&result, "continue"), &Value::Bool(false));
            assert_eq!(
                text(&result, "status"),
                "pending-wake-unknown-after-reservation"
            );
            assert_eq!(root_wakes(&f).len(), 1);
            std::fs::remove_file(path).unwrap();
            std::fs::rename(
                old,
                boundary::base(&f.repo).unwrap().join("omp-transport.lock"),
            )
            .unwrap();
            assert_verified_pending_source(&f, &directory);
        },
    );
}

fn start_join(root: &Path, f: &Fixture, s: &NativeSegment, stage: &str) -> OperationChild {
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&object([
            ("directory", string(s.directory.to_str().unwrap())),
            ("cursor", s.cursor.clone()),
        ]))
        .unwrap(),
        0o600,
    );
    let _ = f;
    operation_child(root, Some(stage), "pause", "join")
}
fn sampled_attempt_mutation(root: &Path, deleted: bool) {
    let f = Fixture::new(root);
    let s = segment(&f, true, false);
    let target = &s.enrollments[1];
    let stage = "omp.join.after-shared-inventory";
    let child = start_join(root, &f, &s, stage);
    ready(root, stage);
    let path = s.directory.join("registration-attempts").join(format!(
        "{}.json",
        transport::digest(get(target, "source_reference")).unwrap()
    ));
    if deleted {
        std::fs::remove_file(&path).unwrap();
    } else {
        let mut attempt = transport::read(&path, 4096).unwrap();
        set(&mut attempt, "source_sha256", string(&"a".repeat(64)));
        write(&path, turn_sources::encoded(&attempt).unwrap(), 0o600);
    }
    release(root, stage);
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    let result = transport::read(&root.join("operation-result.json"), 1024 * 1024).unwrap();
    let rows = result.as_array().unwrap();
    let affected = rows
        .iter()
        .find(|r| get(r, "source_reference") == get(target, "source_reference"))
        .unwrap();
    assert_eq!(text(affected, "status"), "registration-fault");
    assert_eq!(
        get(affected, "faults"),
        &decode(r#"["source-unavailable"]"#)
    );
    assert!(affected.get("observation_id").is_none());
    let other = rows
        .iter()
        .find(|r| get(r, "source_reference") == get(&s.enrollments[0], "source_reference"))
        .unwrap();
    assert_eq!(text(other, "status"), "pending-assessment");
    let state = turn_sources::inspect(
        &f.repo,
        "omp",
        SESSION,
        text(&s.cursor, "lifecycle"),
        &f.runtime,
    )
    .unwrap();
    assert_eq!(get(&state, "sources").as_array().unwrap().len(), 2);
    assert_eq!(path.exists(), !deleted);
}
// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source
#[test]
#[ignore = "recovery_frontier"]
fn changed_sampled_attempt_refuses_even_matching_registered_source() {
    isolated(
        "changed_sampled_attempt_refuses_even_matching_registered_source",
        |root| sampled_attempt_mutation(root, false),
    );
}
// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source
#[test]
#[ignore = "recovery_frontier"]
fn deleted_sampled_attempt_refuses_even_matching_registered_source() {
    isolated(
        "deleted_sampled_attempt_refuses_even_matching_registered_source",
        |root| sampled_attempt_mutation(root, true),
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_observation_failure_is_local_memoized_and_fault_lists_are_independent
#[test]
#[ignore = "recovery_frontier"]
fn observation_failure_is_local_memoized_and_fault_lists_are_independent() {
    isolated(
        "observation_failure_is_local_memoized_and_fault_lists_are_independent",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, true, false);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&object([
                    ("directory", string(s.directory.to_str().unwrap())),
                    ("cursor", s.cursor.clone()),
                ]))
                .unwrap(),
                0o600,
            );
            let child = operation_child(
                root,
                Some("omp.join.before-shared-inventory"),
                "error",
                "join",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let mut result =
                transport::read(&root.join("operation-result.json"), 1024 * 1024).unwrap();
            let Value::Array(rows) = &mut result else {
                panic!("expected joined rows")
            };
            assert_eq!(rows.len(), 2);
            assert!(
                rows.iter()
                    .all(|r| text(r, "status") == "registration-fault"
                        && get(r, "faults") == &decode(r#"["source-unavailable"]"#))
            );
            let Value::Array(faults) = rows[0].get_mut("faults").unwrap() else {
                unreachable!()
            };
            faults.push(string("consumer-local-mutation"));
            assert_eq!(
                get(&rows[1], "faults"),
                &decode(r#"["source-unavailable"]"#)
            );
            assert!(
                joined(&f, &s)
                    .iter()
                    .all(|r| text(r, "status") == "pending-assessment")
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_registration_between_row_lookups_cannot_invent_retained_attempt_fault
#[test]
#[ignore = "recovery_frontier"]
fn registration_between_lookups_cannot_invent_retained_attempt_fault() {
    isolated(
        "registration_between_lookups_cannot_invent_retained_attempt_fault",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, false, false);
            let target = &s.enrollments[1];
            let stage = "omp.join.after-row-lookup";
            let child = start_join(root, &f, &s, stage);
            ready(root, stage);
            let (source, faults) =
                transport::registration(&f.site(), &s.directory, &s.cursor, target, true).unwrap();
            let source = source.unwrap();
            assert!(faults.is_empty());
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let result = transport::read(&root.join("operation-result.json"), 1024 * 1024).unwrap();
            let rows = result.as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert!(
                rows.iter()
                    .all(|r| text(r, "status") == "enrollment-pending"
                        && r.get("faults").is_none()
                        && r.get("observation_id").is_none())
            );
            let later = joined(&f, &s);
            let row = later
                .iter()
                .find(|r| get(r, "source_reference") == get(target, "source_reference"))
                .unwrap();
            assert_eq!(get(row, "observation_id"), get(&source, "observation_id"));
            assert_eq!(text(row, "status"), "pending-assessment");
            assert!(
                later
                    .iter()
                    .any(|r| text(r, "status") == "enrollment-pending")
            );
            assert!(
                s.directory
                    .join("registration-attempts")
                    .join(format!(
                        "{}.json",
                        transport::digest(get(target, "source_reference")).unwrap()
                    ))
                    .is_file()
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_namespace_change_between_both_locked_passes_refuses_every_row
#[test]
#[ignore = "recovery_frontier"]
fn namespace_change_between_locked_passes_refuses_every_row() {
    isolated(
        "namespace_change_between_locked_passes_refuses_every_row",
        |root| {
            let f = Fixture::new(root);
            let s = segment(&f, true, false);
            let ns = shared(&f, &s.cursor);
            let stage = "omp.join.between-locked-passes";
            let child = start_join(root, &f, &s, stage);
            ready(root, stage);
            let fault = object([("reason", string("physical-namespace-change"))]);
            transport::retain(&ns.directory.join("fault.json"), &fault, 4096).unwrap();
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let result = transport::read(&root.join("operation-result.json"), 1024 * 1024).unwrap();
            let rows = result.as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert!(
                rows.iter()
                    .all(|r| text(r, "status") == "registration-fault"
                        && get(r, "faults") == &decode(r#"["source-unavailable"]"#))
            );
            assert_eq!(
                transport::read(&ns.directory.join("fault.json"), 4096).unwrap(),
                fault
            );
            let state = turn_sources::inspect(
                &f.repo,
                "omp",
                SESSION,
                text(&s.cursor, "lifecycle"),
                &f.runtime,
            )
            .unwrap();
            assert!(
                get(&state, "faults")
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_str() == Some("registration-fault-retained"))
            );
        },
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_preexisting_empty_slot_is_never_creator_cleanup_authority
#[test]
fn preexisting_empty_slot_is_never_creator_cleanup_authority() {
    use std::os::unix::fs::MetadataExt;
    let world = World::new().unwrap();
    let lock = world.root.join("bank/.agent-memory.lock");
    write(&lock, b"", 0o600);
    let before = std::fs::symlink_metadata(&lock).unwrap();
    let entered = std::cell::Cell::new(false);
    let result = agent_memory::local_registry::exclusive(
        &lock,
        &uid(),
        "bank-capture",
        Default::default(),
        |_| {
            entered.set(true);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!entered.get());
    assert_eq!(std::fs::read(&lock).unwrap(), b"");
    let after = std::fs::symlink_metadata(lock).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
}

// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_local_owner_is_interruption_not_live_pid_authority
#[test]
fn unheld_local_owner_is_interruption_not_live_pid_authority() {
    let world = World::new().unwrap();
    let lock = world.root.join("bank/.agent-memory.lock");
    let mut owner = Vec::new();
    agent_memory::local_registry::exclusive(
        &lock,
        &uid(),
        "bank-capture",
        Default::default(),
        |raw| {
            owner = raw.to_vec();
            Ok(())
        },
    )
    .unwrap();
    write(&lock, &owner, 0o600);
    let error =
        agent_memory::local_registry::check_canonical_writer(&lock, &["capture"]).unwrap_err();
    assert!(matches!(
        error,
        agent_memory::local_registry::CanonicalWriterError::Contract(_)
    ));
    assert_eq!(std::fs::read(lock).unwrap(), owner);
}

// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy
#[test]
fn held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy() {
    let world = World::new().unwrap();
    let lock = world.root.join("bank/.agent-memory.lock");
    for mutation in [
        "unknown-operation",
        "foreign-host",
        "invalid-pid",
        "malformed",
        "oversize",
        "noncanonical",
        "symlink",
        "hardlink",
        "mode",
    ] {
        let _ = agent_memory::local_registry::exclusive(
            &lock,
            &uid(),
            "bank-capture",
            Default::default(),
            |owner| {
                let mut value = json::decode_bytes(owner, Default::default()).unwrap();
                match mutation {
                    "unknown-operation" => {
                        set(&mut value, "operation", string("bank-future-operation"))
                    }
                    "foreign-host" => set(&mut value, "host", string("foreign.fixture.invalid")),
                    "invalid-pid" => set(&mut value, "pid", Value::Int(0u8.into())),
                    _ => {}
                }
                let raw = match mutation {
                    "malformed" => b"{".to_vec(),
                    "oversize" => vec![b'x'; 4097],
                    "noncanonical" => turn_sources::encoded(&value).unwrap(),
                    _ => agent_memory::developer_identity::encoded(&value).unwrap(),
                };
                let alias = world.root.join("owner-alias");
                if mutation == "symlink" {
                    write(&alias, owner, 0o600);
                    std::fs::remove_file(&lock).unwrap();
                    symlink(&alias, &lock).unwrap();
                } else {
                    write(&lock, &raw, if mutation == "mode" { 0o640 } else { 0o600 });
                }
                if mutation == "hardlink" {
                    std::fs::hard_link(&lock, &alias).unwrap();
                }
                let error =
                    agent_memory::local_registry::check_canonical_writer(&lock, &["capture"])
                        .unwrap_err();
                assert!(
                    matches!(
                        error,
                        agent_memory::local_registry::CanonicalWriterError::Contract(_)
                    ),
                    "{mutation}: {error}"
                );
                assert_eq!(
                    std::fs::read(&lock).unwrap(),
                    if mutation == "symlink" { owner } else { &raw }
                );
                if alias.exists() {
                    std::fs::remove_file(alias).unwrap();
                }
                Ok(())
            },
        );
        if std::fs::symlink_metadata(&lock).is_ok() {
            std::fs::remove_file(&lock).unwrap();
        }
    }
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::AnswerFirstQualification.test_no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status
#[test]
#[ignore = "recovery_frontier"]
fn no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status() {
    isolated(
        "no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status",
        |root| {
            let source = pack(root);
            let agent = root.join("omp-agent");
            std::fs::create_dir(&agent).unwrap();
            command(
                root,
                &[
                    env!("CARGO_BIN_EXE_am").into(),
                    "install".into(),
                    "--development-source".into(),
                    source.into_os_string(),
                    "--presets".into(),
                    "omp".into(),
                    "--omp-agent-dir".into(),
                    agent.clone().into_os_string(),
                    "--omp-peers-upgraded".into(),
                    "--no-guidance".into(),
                ],
            );
            let receipt = transport::read(
                &root.join("state/agent-memory/install/receipt.json"),
                16 * 1024 * 1024,
            )
            .unwrap();
            assert_eq!(
                get(get(&receipt, "omp_binding"), "activation_target"),
                &Value::Null
            );
            let repo = bank_fixture(root);
            let runtime = PathBuf::from(text(get(&receipt, "source"), "pack"));
            boundary::activate_marker(&repo, false, &runtime).unwrap();
            for operation in ["status", "doctor"] {
                let output = command(
                    &repo,
                    &[
                        env!("CARGO_BIN_EXE_am").into(),
                        "--json".into(),
                        operation.into(),
                    ],
                );
                let result = json::decode_bytes(&output.stdout, Default::default()).unwrap();
                assert_eq!(get(&result, "ok"), &Value::Bool(true), "{result:?}");
                assert_eq!(
                    text(get(&result, "omp"), "owned_disk_integrity"),
                    "verified"
                );
                assert_eq!(
                    get(get(&result, "omp"), "answer_first"),
                    &decode(
                        r#"{"activation":"no_guidance","automation":"unqualified","native_loading":"not-observed"}"#
                    )
                );
            }
            assert!(!agent.join("AGENTS.md").exists());
        },
    );
}

fn bank_fixture(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    command(
        root,
        &[
            "git".into(),
            "init".into(),
            "--quiet".into(),
            "--initial-branch=main".into(),
            repo.clone().into_os_string(),
        ],
    );
    write(
        &repo.join("evidence.md"),
        "# Evidence\n\nEphemeral fixture.\n",
        0o644,
    );
    command(&repo, &["git".into(), "add".into(), ".".into()]);
    command(
        &repo,
        &[
            "git".into(),
            "commit".into(),
            "--quiet".into(),
            "-m".into(),
            "Fixture".into(),
        ],
    );
    let mut proposal = decode(include_str!(
        "../skills/agent-memory/templates/router-proposal.example.json"
    ));
    set(
        &mut proposal,
        "project-id",
        string("75b38e17-c36e-4fab-8ba9-5fa51f25de91"),
    );
    let rendered = agent_memory::bank_contract::render(&proposal).unwrap();
    for (name, body) in get(&rendered, "files").as_object().unwrap() {
        write(
            &repo.join("memory-bank").join(name),
            body.as_str().unwrap(),
            0o644,
        );
    }
    let p = agent_memory::developer_identity::preview(
        "create",
        None,
        &Value::Array(vec![string("Fixture observer")]),
        &Value::Array(vec![]),
    )
    .unwrap();
    agent_memory::developer_identity::apply(&p).unwrap();
    let binding = agent_memory::scope_binding::preview(
        &repo,
        "bind",
        None,
        "Explicit disposable native fixture",
        None,
    )
    .unwrap();
    agent_memory::scope_binding::apply(&binding).unwrap();
    repo
}

fn install_activation(receipt: &Value) {
    let pack = Path::new(text(get(receipt, "source"), "pack"));
    // The stream's transport resolves the running installation's pack.
    agent_memory::cli::use_development_pack(Some(pack.to_path_buf()));
    omp_installation::register_activation(Some(Arc::new(
        agent_memory::installation::InstallationActivation {
            env: agent_memory::installation::Environment::process(),
            services: agent_memory::installation::Services::registered().unwrap(),
            executable: Path::new(text(receipt, "bin_dir")).join("am"),
            harness: transport::Harness::at(pack),
        },
    )))
    .unwrap();
}

#[test]
fn contended_lock_child() {
    let Some(path) = std::env::var_os("AGENT_MEMORY_FAULT_LOCK_PATH") else {
        return;
    };
    support::require_current_world().unwrap();
    let path = PathBuf::from(path);
    let result = if std::env::var("AGENT_MEMORY_FAULT_LOCK_FAMILY").unwrap() == "private" {
        turn_sources::private_lock(&path, false, true)
            .map(|_| panic!("Contended private lock body entered"))
    } else {
        turn_sources::locked(&path, || -> agent_memory::error::Result<()> {
            panic!("Contended completion body entered")
        })
    };
    assert!(matches!(
        result.unwrap_err().kind(),
        agent_memory::error::Kind::SourceTransportBusy { .. }
    ));
}
// obligation: tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_both_contended_lock_families_raise_typed_busy_before_body
#[test]
fn private_and_completion_lock_contention_is_typed_before_body() {
    let mut world = World::new().unwrap();
    let directory = world.root.join("source-locks");
    turn_sources::private_mkdir(&directory).unwrap();
    for family in ["private", "completion"] {
        let path = if family == "private" {
            directory.join("source.lock")
        } else {
            directory.clone()
        };
        world.env.insert(
            "AGENT_MEMORY_FAULT_LOCK_PATH".into(),
            path.clone().into_os_string(),
        );
        world
            .env
            .insert("AGENT_MEMORY_FAULT_LOCK_FAMILY".into(), family.into());
        let run_child = || {
            world
                .run([
                    std::env::current_exe().unwrap().into_os_string(),
                    "--exact".into(),
                    "contended_lock_child".into(),
                    "--nocapture".into(),
                ])
                .unwrap();
        };
        if family == "private" {
            let held = turn_sources::private_lock(&path, false, false).unwrap();
            run_child();
            held.finish().unwrap();
            turn_sources::private_lock(&path, false, true)
                .unwrap()
                .finish()
                .unwrap();
        } else {
            turn_sources::locked(&path, || {
                run_child();
                Ok(())
            })
            .unwrap();
            turn_sources::locked(&path, || {
                write(&directory.join("completion-entered"), b"entered", 0o600);
                Ok(())
            })
            .unwrap();
        }
    }
}

// obligation: tests/contracts/test_omp_boundary.py::StoredBatchControls
#[test]
fn stored_batch_controls_keep_foreign_companions_and_humans_as_source() {
    let world = World::new().unwrap();
    assert!(world.root.is_dir());
    let own = decode(
        r#"{"type":"custom_message","id":"own","parentId":null,"customType":"session-stop-continuation","attribution":"agent","content":"exact reserved guidance"}"#,
    );
    assert!(
        agent_memory::omp_control::pure_batch(
            std::slice::from_ref(&own),
            Some("exact reserved guidance"),
            false,
            &[]
        )
        .is_some()
    );
    for custom_type in [
        "goal-mode-context",
        "plan-mode",
        "vibe-mode-context",
        "todo-error-reminder",
        "advisor",
        "MEMORY_RECALL_CHANGES",
        "foreign-before-agent",
        "unknown",
    ] {
        let mut foreign = own.clone();
        set(&mut foreign, "id", string("foreign"));
        set(&mut foreign, "customType", string(custom_type));
        set(&mut foreign, "content", string("Foreign contextual work"));
        assert!(
            agent_memory::omp_control::pure_batch(
                &[own.clone(), foreign],
                Some("exact reserved guidance"),
                false,
                &[]
            )
            .is_none()
        );
    }
    assert!(
        agent_memory::omp_control::pure_batch(
            &[human("human"), own.clone()],
            Some("exact reserved guidance"),
            false,
            &[]
        )
        .is_none()
    );
    let mut joined = own;
    set(
        &mut joined,
        "content",
        string("exact reserved guidance\njoined foreign stop text"),
    );
    assert!(
        agent_memory::omp_control::pure_batch(
            &[joined],
            Some("exact reserved guidance"),
            false,
            &[]
        )
        .is_none()
    );
}

// frontier obligation: tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_observer_verification_cannot_enroll_or_promote_legacy_marker
#[test]
#[ignore = "recovery_frontier"]
fn observer_verification_never_enrolls_or_promotes_legacy_marker() {
    isolated(
        "observer_verification_never_enrolls_or_promotes_legacy_marker",
        |root| {
            let (runtime, _, _) = install_only(root, "copy");
            let repo = bank_fixture(root);
            let base = boundary::base(&repo).unwrap();
            let marker = base.join("turn-sources-compat-v1.json");
            assert!(boundary::verify_marker(&repo, &runtime).is_err());
            assert!(!marker.exists());
            assert!(!base.join("omp-bank-binding-v1.json").exists());
            boundary::activate_marker(&repo, true, &runtime).unwrap();
            assert_eq!(
                boundary::verify_marker(&repo, &runtime).unwrap().root(),
                repo
            );
            assert_eq!(
                boundary::required_source_capabilities(&repo, &runtime).unwrap(),
                ["omp-harness-v1".into(), "omp-stored-source-v2".into()]
                    .into_iter()
                    .collect()
            );
            let historical = decode(
                r#"{"schema":"agent-memory/turn-sources-compat-v1","minimum_reader":"omp-harness-v1","harnesses":["omp"]}"#,
            );
            write(&marker, turn_sources::encoded(&historical).unwrap(), 0o600);
            let before = std::fs::read(&marker).unwrap();
            assert!(boundary::verify_marker(&repo, &runtime).is_err());
            assert_eq!(std::fs::read(&marker).unwrap(), before);
            assert_eq!(
                boundary::required_source_capabilities(&repo, &runtime).unwrap(),
                ["omp-harness-v1".into()].into_iter().collect()
            );
            assert!(boundary::activate_marker(&repo, true, &runtime).is_err());
            assert_eq!(std::fs::read(&marker).unwrap(), before);
            boundary::activate_marker(&repo, false, &runtime).unwrap();
            assert_eq!(
                text(&transport::read(&marker, 4096).unwrap(), "minimum_reader"),
                "omp-stored-source-v2"
            );
            turn_sources::check_source_compatibility(&base).unwrap();
            let mut future = historical;
            set(&mut future, "minimum_reader", string("future-reader"));
            write(&marker, turn_sources::encoded(&future).unwrap(), 0o600);
            assert!(turn_sources::check_source_compatibility(&base).is_err());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_local_read_reuse_requires_exact_live_context_and_preserves_inventory
#[test]
#[ignore = "recovery_frontier"]
fn transaction_local_read_reuse_requires_exact_live_context() {
    isolated(
        "transaction_local_read_reuse_requires_exact_live_context",
        |root| {
            let f = Fixture::new(root);
            let result = f.observe(&request(&f, &[human("human")], "seal"));
            let life = text(&result, "lifecycle");
            let expected =
                turn_sources::inspect(&f.repo, "omp", SESSION, life, &f.runtime).unwrap();
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
                assert!(
                    turn_sources::inspect_enrollment(&f.repo, "omp", SESSION, life, context)
                        .is_err()
                );
                Ok(())
            })
            .unwrap();
            let barrier =
                turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
            agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
                for _ in 0..3 {
                    assert_eq!(
                        turn_sources::encoded(&turn_sources::inspect_enrollment(
                            &f.repo, "omp", SESSION, life, context
                        )?)
                        .unwrap(),
                        turn_sources::encoded(&expected).unwrap()
                    );
                }
                let fresh = boundary::base(&f.repo)?;
                agent_memory::scope_binding::observe_enrollment_locations(context, || {
                    let location = agent_memory::scope_binding::observe_enrollment_location(
                        &f.repo,
                        Some(context),
                        &f.runtime,
                    )?
                    .unwrap();
                    assert_eq!(location.0, context.root());
                    assert_eq!(location.1, context.bank());
                    let site = f.site().with(context);
                    assert!(transport::session_dir(&site, SESSION)?.starts_with(&fresh));
                    agent_memory::scope_binding::recheck_private_enrollment(context)?;
                    let inner =
                        turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime)?;
                    inner.recheck()?;
                    Ok(())
                })?;
                assert!(
                    agent_memory::scope_binding::observe_enrollment_location(
                        &f.repo, None, &f.runtime
                    )?
                    .is_none()
                );
                assert!(
                    turn_sources::inspect_enrollment(
                        &root.join("foreign"),
                        "omp",
                        SESSION,
                        life,
                        context
                    )
                    .is_err()
                );
                Ok(())
            })
            .unwrap();
            barrier.recheck().unwrap();
            drop(barrier);
            assert!(
                turn_sources::inspect_enrollment(&f.repo, "omp", SESSION, life, &context).is_err()
            );
            assert_eq!(
                turn_sources::encoded(
                    &turn_sources::inspect(&f.repo, "omp", SESSION, life, &f.runtime).unwrap()
                )
                .unwrap(),
                turn_sources::encoded(&expected).unwrap()
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_changed_configured_bank_before_reused_registration_writes_neither_namespace
#[test]
#[ignore = "recovery_frontier"]
fn changed_configured_bank_before_reused_registration_writes_neither_namespace() {
    isolated(
        "changed_configured_bank_before_reused_registration_writes_neither_namespace",
        |root| {
            let f = Fixture::new(root);
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            let (original, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            let alternate = f.repo.join("alternate-bank");
            std::fs::create_dir(&alternate).unwrap();
            let alternate_base =
                agent_memory::bank_writes::state_paths(&f.repo, &alternate, &uid())
                    .unwrap()
                    .0
                    .parent()
                    .unwrap()
                    .to_path_buf();
            let before = (snapshot(&original.directory), snapshot(&alternate_base));
            let config = f.repo.join(".git/config");
            let old = std::fs::read(&config).unwrap();
            let held =
                turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
            let outcome =
                agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
                    command(
                        &f.repo,
                        &[
                            "git".into(),
                            "config".into(),
                            "--local".into(),
                            "agent-memory.bankRoot".into(),
                            "alternate-bank".into(),
                        ],
                    );
                    assert!(
                        turn_sources::register(
                            &f.repo,
                            "omp",
                            SESSION,
                            LIFE,
                            &"a".repeat(64),
                            Some("actual-native-source"),
                            None,
                            Some(context),
                            &f.runtime
                        )
                        .is_err()
                    );
                    write(&config, &old, 0o644);
                    Ok(())
                });
            write(&config, &old, 0o644);
            outcome.unwrap();
            held.recheck().unwrap();
            assert_eq!(
                (snapshot(&original.directory), snapshot(&alternate_base)),
                before
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_end_fresh_recheck_refuses_late_config_after_read_only_reuse
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_observation_refuses_late_authority_change_without_private_writes
#[test]
#[ignore = "recovery_frontier"]
fn transaction_exit_freshly_refuses_late_authority_config_without_private_writes() {
    isolated(
        "transaction_exit_freshly_refuses_late_authority_config_without_private_writes",
        |root| {
            let f = Fixture::new(root);
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            let config = f.repo.join(".git/config");
            let old = std::fs::read(&config).unwrap();
            let state = root.join("state");
            let before = snapshot(&state);
            let held =
                turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
            let result =
                agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
                    command(
                        &f.repo,
                        &[
                            "git".into(),
                            "config".into(),
                            "--local".into(),
                            "agent-memory.authorityRemote".into(),
                            "missing-native-authority".into(),
                        ],
                    );
                    turn_sources::inspect_enrollment(&f.repo, "omp", SESSION, LIFE, context)?;
                    Ok(())
                });
            assert!(result.is_err());
            write(&config, &old, 0o644);
            held.recheck().unwrap();
            drop(held);
            assert_eq!(snapshot(&state), before);
            assert!(
                turn_sources::inspect_enrollment(&f.repo, "omp", SESSION, LIFE, &context).is_err()
            );
            let (ns, _) =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap();
            assert!(!ns.directory.exists());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_pre_reservation_fault_has_no_second_outcome
#[test]
#[ignore = "recovery_frontier"]
fn postcommit_prereservation_refusal_has_no_second_outcome() {
    isolated(
        "postcommit_prereservation_refusal_has_no_second_outcome",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&armed_request(&f)).unwrap(),
                0o600,
            );
            let child = operation_child(
                root,
                Some("omp.before-reserve.before-open"),
                "error",
                "finite",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let directory = child_directory(root);
            assert_verified_pending_source(&f, &directory);
            let result = child_result(root);
            assert_eq!(get(&result, "verified"), &Value::Bool(true));
            assert_eq!(get(&result, "continue"), &Value::Bool(false));
            assert_eq!(text(&result, "status"), "pending-wake-unavailable");
            assert!(root_wakes(&f).is_empty());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_plain_prereserve_open_read_refuses_drift_after_verified_source
#[test]
#[ignore = "recovery_frontier"]
fn prereserve_open_read_refuses_drift_after_verified_source() {
    isolated(
        "prereserve_open_read_refuses_drift_after_verified_source",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&armed_request(&f)).unwrap(),
                0o600,
            );
            let stage = "omp.before-reserve.before-open";
            let child = operation_child(root, Some(stage), "pause", "finite");
            ready(root, stage);
            let directory = child_directory(root);
            let path = directory.join("open.json");
            let raw = std::fs::read(&path).unwrap();
            let mut opened = transport::read(&path, stream::MAX_METADATA).unwrap();
            set(&mut opened, "checkout_key", string("foreign-checkout"));
            write(&path, turn_sources::encoded(&opened).unwrap(), 0o600);
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            write(&path, &raw, 0o600);
            assert_verified_pending_source(&f, &directory);
            assert_eq!(
                text(&child_result(root), "status"),
                "pending-wake-unavailable"
            );
            assert!(root_wakes(&f).is_empty());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_retained_root_after_cursor_failure_stays_unknown_and_rootless
// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_wake_failure_after_reservation_preserves_verified_seal_and_single_budget
#[test]
#[ignore = "recovery_frontier"]
fn retained_root_after_cursor_failure_stays_unknown_and_rootless() {
    isolated(
        "retained_root_after_cursor_failure_stays_unknown_and_rootless",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&armed_request(&f)).unwrap(),
                0o600,
            );
            let child = operation_child(
                root,
                Some("omp.reservation.after-cursor"),
                "error",
                "finite-auto-calibrated",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let directory = child_directory(root);
            assert_verified_pending_source(&f, &directory);
            assert_eq!(root_wakes(&f).len(), 1);
            let result = child_result(root);
            assert_eq!(get(&result, "continue"), &Value::Bool(false));
            assert_eq!(
                text(&result, "status"),
                "pending-wake-unknown-after-reservation"
            );
            let decision = transport::read(
                &stream::wake_decision_directory(&directory)
                    .unwrap()
                    .join("1.json"),
                4096,
            )
            .unwrap();
            assert_eq!(
                text(&decision, "status"),
                "pending-wake-unknown-after-reservation"
            );
            assert_eq!(get(&decision, "continuation_offered"), &Value::Bool(false));
            assert_eq!(get(&decision, "human_root"), &Value::Null);
            assert_eq!(get(&decision, "root_wake_sha256"), &Value::Null);
            assert_eq!(
                get(get(&wake_summary(&f), "counts"), "offered"),
                &Value::Int(0u8.into())
            );
            let before = std::fs::read(&root_wakes(&f)[0]).unwrap();
            let wake = object([
                ("schema", string("agent-memory/omp-observer-request-v2")),
                ("action", string("wake")),
                ("session", string(SESSION)),
                ("lifecycle", get(&result, "lifecycle").clone()),
                ("payload", get(&result, "wake_payload").clone()),
            ]);
            assert_eq!(get(&f.observe(&wake), "continue"), &Value::Bool(false));
            assert_eq!(std::fs::read(&root_wakes(&f)[0]).unwrap(), before);
        },
    );
}

// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_rechecks_normal_and_ordinary_failures
#[test]
#[ignore = "recovery_frontier"]
fn public_readonly_drain_rechecks_raced_transport_absence() {
    isolated(
        "public_readonly_drain_rechecks_raced_transport_absence",
        |root| {
            let f = Fixture::new(root);
            write(&root.join("case-request.json"), b"{}", 0o600);
            let base = boundary::base(&f.repo).unwrap();
            let path = base.join("omp-transport.lock");
            assert!(!path.exists());
            let stage = "omp.drain.before-inventory";
            let child = operation_child(root, Some(stage), "pause", "drain");
            ready(root, stage);
            let evidence = b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n";
            write(&path, evidence, 0o600);
            let before = snapshot(&base);
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let result = transport::read(&root.join("operation-result.json"), 65536).unwrap();
            assert!(
                text(&result, "error")
                    .contains("Native transport appeared during read-only drain observation")
            );
            assert_eq!(snapshot(&base), before);
            turn_sources::private_lock(&base.join("source-barrier.lock"), false, true)
                .unwrap()
                .finish()
                .unwrap();
        },
    );
}

// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_preserves_interrupts_and_fatal_failures
// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_native_drain_preserves_interrupts_and_releases_source_barrier
#[test]
#[ignore = "recovery_frontier"]
fn public_native_drain_preserves_real_sigint_and_releases_barrier() {
    isolated(
        "public_native_drain_preserves_real_sigint_and_releases_barrier",
        |root| {
            let f = Fixture::new(root);
            write(&root.join("case-request.json"), b"{}", 0o600);
            let base = boundary::base(&f.repo).unwrap();
            let stage = "omp.drain.before-inventory";
            let child = operation_child(root, Some(stage), "pause", "drain");
            ready(root, stage);
            let path = base.join("omp-transport.lock");
            let evidence = b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n";
            write(&path, evidence, 0o600);
            let before = snapshot(&base);
            let pid = std::fs::read_to_string(root.join("operation-pid")).unwrap();
            command(root, &["/bin/kill".into(), "-INT".into(), pid.into()]);
            assert_eq!(
                child.wait().status,
                agent_memory::process::Status::Signal(2)
            );
            assert_eq!(snapshot(&base), before);
            turn_sources::private_lock(&base.join("source-barrier.lock"), false, true)
                .unwrap()
                .finish()
                .unwrap();
        },
    );
}

fn installed_command(f: &Fixture, args: &[OsString]) -> Value {
    let mut argv = vec![
        Path::new(text(&f.receipt, "bin_dir"))
            .join("am")
            .into_os_string(),
        "--json".into(),
    ];
    argv.extend_from_slice(args);
    json::decode_bytes(&command(&f.repo, &argv).stdout, Default::default()).unwrap()
}
fn capture_proposal(root: &Path, f: &Fixture) -> PathBuf {
    let scope = agent_memory::scope_binding::require(&f.repo, None, Default::default()).unwrap();
    let mut candidate = decode(include_str!(
        "../skills/agent-memory/templates/record-proposal.example.json"
    ));
    let metadata = candidate.get_mut("metadata").unwrap();
    set(
        metadata,
        "source",
        decode(r#"{"kind":"user","actor":"Explicit fixture source; not authenticated"}"#),
    );
    set(
        metadata.get_mut("origin").unwrap(),
        "code-state",
        string("not-code"),
    );
    set(
        metadata.get_mut("origin").unwrap(),
        "developer-id",
        get(&scope, "developer_id").clone(),
    );
    let request = object([
        ("project_id", get(metadata, "project-id").clone()),
        ("candidate", candidate.clone()),
        ("source_class", string("user")),
        ("repository_evidence", Value::Null),
        (
            "assessment",
            decode(
                r#"{"category":"user-correction","utility":"Prevent repeated excessive retries under these conditions.","durable":true,"evidence_confidence":"confirmed","evidence_reason":"The operator explicitly confirmed the correction.","confirmed":true,"source_redundant":false,"work_state_copy":false,"review":null}"#,
            ),
        ),
    ]);
    let request_path = root.join("capture-request.json");
    write(
        &request_path,
        turn_sources::encoded(&request).unwrap(),
        0o600,
    );
    let captured = installed_command(
        f,
        &[
            "capture".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            request_path.into_os_string(),
        ],
    );
    assert_eq!(text(&captured, "decision"), "eligible");
    let capture_path = root.join("capture-result.json");
    write(
        &capture_path,
        turn_sources::encoded(&captured).unwrap(),
        0o600,
    );
    let comparison = object([
        ("project_id", get(&request, "project_id").clone()),
        ("candidate", candidate),
        (
            "searches",
            Value::Array(vec![string("retry policy"), string("attempt cap")]),
        ),
        ("read_paths", Value::Array(vec![])),
    ]);
    let comparison_path = root.join("comparison-request.json");
    write(
        &comparison_path,
        turn_sources::encoded(&comparison).unwrap(),
        0o600,
    );
    let inspected = installed_command(
        f,
        &[
            "compare".into(),
            "inspect".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            comparison_path.clone().into_os_string(),
        ],
    );
    let decisions = get(&inspected, "matches")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            object([
                ("path", get(row, "path").clone()),
                ("classification", string("distinct-knowledge")),
                (
                    "rationale",
                    string(
                        "The fixture meaning has been assessed with its evidence and conditions.",
                    ),
                ),
            ])
        })
        .collect();
    let judgment = object([
        ("observation", get(&inspected, "observation").clone()),
        ("decisions", Value::Array(decisions)),
        ("pending_decisions", Value::Array(vec![])),
        (
            "rationale",
            string("Read the actual bounded plausible sources before deciding novelty."),
        ),
    ]);
    let judgment_path = root.join("comparison-judgment.json");
    write(
        &judgment_path,
        turn_sources::encoded(&judgment).unwrap(),
        0o600,
    );
    let reviewed = installed_command(
        f,
        &[
            "compare".into(),
            "review".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            comparison_path.clone().into_os_string(),
            "--judgment".into(),
            judgment_path.into_os_string(),
        ],
    );
    let proof_path = root.join("comparison-proof.json");
    write(
        &proof_path,
        turn_sources::encoded(get(&reviewed, "proof")).unwrap(),
        0o600,
    );
    let preview = installed_command(
        f,
        &[
            "admission".into(),
            "preview".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--capture".into(),
            capture_path.into_os_string(),
            "--request".into(),
            comparison_path.into_os_string(),
            "--proof".into(),
            proof_path.into_os_string(),
        ],
    );
    let proposal = root.join("capture-proposal.json");
    write(
        &proposal,
        turn_sources::encoded(get(&preview, "proposal")).unwrap(),
        0o600,
    );
    proposal
}
fn capture_child(root: &Path, f: &Fixture, proposal: &Path, stage: Option<&str>) -> OperationChild {
    let mut env = std::env::vars_os().collect::<Vec<_>>();
    if let Some(stage) = stage {
        env.push(("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()));
        env.push(("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()));
        env.push((
            "AGENT_MEMORY_FAULT_DIR".into(),
            root.join("capture-rendezvous").into_os_string(),
        ));
    }
    let argv = [
        Path::new(text(&f.receipt, "bin_dir"))
            .join("am")
            .into_os_string(),
        "--json".into(),
        "admission".into(),
        "apply".into(),
        "--proposal".into(),
        proposal.as_os_str().to_owned(),
    ];
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(70)).unwrap();
    let spec = agent_memory::process::Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&f.repo),
        search_path: false,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        stderr: agent_memory::process::Target::Capture { limit: 1024 * 1024 },
        deadline,
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    OperationChild {
        child: agent_memory::process::spawn_owned(&spec).unwrap(),
        deadline,
    }
}
fn assert_capture_succeeds(child: OperationChild) {
    let output = child.wait();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(get(&value, "admission_verified"), &Value::Bool(true));
}

// frontier obligation: tests/contracts/test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_public_capture_cannot_interpose_before_verified_outcome
#[test]
#[ignore = "recovery_frontier"]
fn public_capture_cannot_interpose_before_verified_finite_outcome() {
    isolated(
        "public_capture_cannot_interpose_before_verified_finite_outcome",
        |root| {
            let f = Fixture::new(root);
            let proposal = capture_proposal(root, &f);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request(&f, &[human("outcome-human")], "ingest")).unwrap(),
                0o600,
            );
            let stage = "omp.outcome.before-guard";
            let observer = operation_child(root, Some(stage), "pause", "finite-run");
            ready(root, stage);
            let directory = child_directory(root);
            let writer = capture_child(
                root,
                &f,
                &proposal,
                Some("transaction-locked-before-inventory"),
            );
            std::thread::sleep(Duration::from_millis(100));
            assert!(
                !root
                    .join("capture-rendezvous/transaction-locked-before-inventory.ready")
                    .exists()
            );
            assert!(!directory.join("outcomes/1.json").exists());
            release(root, stage);
            wait_path(&root.join("capture-rendezvous/transaction-locked-before-inventory.ready"));
            assert_eq!(
                text(
                    &transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA)
                        .unwrap(),
                    "outcome"
                ),
                "verified"
            );
            write(
                &root.join("capture-rendezvous/transaction-locked-before-inventory.release"),
                b"",
                0o600,
            );
            assert_capture_succeeds(writer);
            assert_eq!(
                observer.wait().status,
                agent_memory::process::Status::Code(0)
            );
            assert!(directory.join("closed.json").is_file());
            assert_eq!(
                get(
                    &transport::read(&root.join("operation-result.json"), 65536).unwrap(),
                    "exit_code"
                ),
                &Value::Int(0u8.into())
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_exhausts_real_shared_barrier_without_canonical_write
#[test]
#[ignore = "recovery_frontier"]
fn public_capture_exhausts_real_shared_barrier_without_canonical_write() {
    isolated(
        "public_capture_exhausts_real_shared_barrier_without_canonical_write",
        |root| {
            let f = Fixture::new(root);
            let proposal = capture_proposal(root, &f);
            let base = boundary::base(&f.repo).unwrap();
            let barrier =
                turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
            let bank = f.repo.join("memory-bank");
            let before = (
                snapshot(&bank),
                snapshot(&base.join("transactions")),
                snapshot(&base.join("operations")),
                std::fs::read(f.repo.join(".git/index")).unwrap(),
            );
            let started = agent_memory::clock::monotonic_ns();
            let child = capture_child(root, &f, &proposal, None);
            let output = child.wait();
            assert_ne!(output.status, agent_memory::process::Status::Code(0));
            assert!(agent_memory::clock::monotonic_ns() - started >= 29_000_000_000);
            let refusal = json::decode_bytes(&output.stdout, Default::default()).unwrap();
            assert_eq!(get(&refusal, "admission_verified"), &Value::Bool(false));
            assert_eq!(
                get(&refusal, "canonical_writes_performed"),
                &Value::Bool(false)
            );
            assert!(!bank.join(".agent-memory.lock").exists());
            assert_eq!(
                (
                    snapshot(&bank),
                    snapshot(&base.join("transactions")),
                    snapshot(&base.join("operations")),
                    std::fs::read(f.repo.join(".git/index")).unwrap()
                ),
                before
            );
            barrier.recheck().unwrap();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_refuses_unknown_marker_and_shared_upgrade
#[test]
#[ignore = "recovery_frontier"]
fn public_capture_refuses_unknown_marker_and_shared_barrier_upgrade() {
    isolated(
        "public_capture_refuses_unknown_marker_and_shared_barrier_upgrade",
        |root| {
            let f = Fixture::new(root);
            let proposal = capture_proposal(root, &f);
            let base = boundary::base(&f.repo).unwrap();
            let held =
                turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
            let error = turn_sources::canonical_writer_barrier(&f.repo, None, &f.runtime)
                .err()
                .unwrap();
            assert!(!matches!(
                error.kind(),
                agent_memory::error::Kind::SourceTransportBusy { .. }
            ));
            drop(held);
            let marker = base.join("turn-sources-compat-v1.json");
            let old = std::fs::read(&marker).unwrap();
            let invalid = br#"{"schema":"unknown"}"#;
            write(&marker, invalid, 0o600);
            let before = (
                snapshot(&f.repo.join("memory-bank")),
                snapshot(&base.join("transactions")),
                snapshot(&base.join("operations")),
                std::fs::read(f.repo.join(".git/index")).unwrap(),
            );
            let output = capture_child(root, &f, &proposal, None).wait();
            assert_ne!(output.status, agent_memory::process::Status::Code(0));
            assert_eq!(std::fs::read(&marker).unwrap(), invalid);
            assert!(!f.repo.join("memory-bank/.agent-memory.lock").exists());
            assert_eq!(
                (
                    snapshot(&f.repo.join("memory-bank")),
                    snapshot(&base.join("transactions")),
                    snapshot(&base.join("operations")),
                    std::fs::read(f.repo.join(".git/index")).unwrap()
                ),
                before
            );
            write(&marker, old, 0o600);
        },
    );
}

fn writer_and_observer_order(root: &Path, writer_first: bool) {
    let f = Fixture::new(root);
    let proposal = capture_proposal(root, &f);
    let request = request(&f, &[human("fence-human")], "ingest");
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&request).unwrap(),
        0o600,
    );
    let observer_stage = "omp.capture.before-commit";
    let writer_stage = "transaction-locked-before-inventory";
    let writer_ready = root
        .join("capture-rendezvous")
        .join(format!("{writer_stage}.ready"));
    let writer_release = root
        .join("capture-rendezvous")
        .join(format!("{writer_stage}.release"));
    if writer_first {
        let writer = capture_child(root, &f, &proposal, Some(writer_stage));
        wait_path(&writer_ready);
        let observer = operation_child(root, Some(observer_stage), "pause", "finite-run");
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !root
                .join("rendezvous")
                .join(format!("{observer_stage}.ready"))
                .exists()
        );
        write(&writer_release, b"", 0o600);
        assert_capture_succeeds(writer);
        ready(root, observer_stage);
        release(root, observer_stage);
        assert_eq!(
            observer.wait().status,
            agent_memory::process::Status::Code(0)
        );
    } else {
        let observer = operation_child(root, Some(observer_stage), "pause", "finite-run");
        ready(root, observer_stage);
        let writer = capture_child(root, &f, &proposal, Some(writer_stage));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!writer_ready.exists());
        release(root, observer_stage);
        wait_path(&writer_ready);
        write(&writer_release, b"", 0o600);
        assert_capture_succeeds(writer);
        assert_eq!(
            observer.wait().status,
            agent_memory::process::Status::Code(0)
        );
    }
    let directory = child_directory(root);
    assert!(directory.join("closed.json").is_file());
    assert_eq!(
        get(
            &transport::read(&root.join("operation-result.json"), 65536).unwrap(),
            "exit_code"
        ),
        &Value::Int(0u8.into())
    );
}
// frontier obligation: tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders
#[test]
#[ignore = "recovery_frontier"]
fn actual_capture_before_real_observer_retains_fence_order() {
    isolated(
        "actual_capture_before_real_observer_retains_fence_order",
        |root| writer_and_observer_order(root, true),
    );
}
// frontier obligation: tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders
#[test]
#[ignore = "recovery_frontier"]
fn actual_observer_before_real_capture_retains_fence_order() {
    isolated(
        "actual_observer_before_real_capture_retains_fence_order",
        |root| writer_and_observer_order(root, false),
    );
}

fn native_path(directory: &Path, child: &str, native: &str) -> PathBuf {
    directory.join(child).join(format!(
        "{}.json",
        agent_memory::developer_identity::digest(Some(native.as_bytes())).unwrap()
    ))
}
fn terminal(value: &mut Value) {
    set(
        value.get_mut("payload").unwrap(),
        "terminal",
        Value::Bool(true),
    );
}
fn tool_call(native: &str, call_id: &str) -> Value {
    entry(
        native,
        "assistant",
        Value::Array(vec![object([
            ("type", string("toolCall")),
            ("id", string(call_id)),
            ("name", string("read")),
            ("arguments", object([("path", string("synthetic.txt"))])),
        ])]),
    )
}
fn tool_result(native: &str, call_id: &str, body: &str) -> Value {
    let mut value = entry(native, "toolResult", string(body));
    let message = value.get_mut("message").unwrap();
    set(message, "toolCallId", string(call_id));
    set(message, "toolName", string("read"));
    set(message, "isError", Value::Bool(false));
    value
}

// obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_persistence_truncation_is_not_literal_complete_learning
#[test]
fn persistence_truncation_is_never_literal_complete_learning() {
    let world = World::new().unwrap();
    assert!(world.root.is_dir());
    let entry = entry(
        "native",
        "assistant",
        string(&format!(
            "Synthetic visible prefix.{}",
            agent_memory::omp_projection::PERSISTENCE_TRUNCATION_NOTICE
        )),
    );
    assert_eq!(
        agent_memory::omp_projection::project(&entry),
        decode(r#"{"gap":"oversized-source"}"#)
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_active_unavailable_tail_is_real_gap_not_a_notification_gap
#[test]
#[ignore = "recovery_frontier"]
fn active_unavailable_tail_is_real_gap_not_notification_gap() {
    isolated(
        "active_unavailable_tail_is_real_gap_not_notification_gap",
        |root| {
            let f = Fixture::new(root);
            let mut value = request(&f, &[human("human")], "ingest");
            terminal(&mut value);
            set(
                value.get_mut("payload").unwrap().get_mut("view").unwrap(),
                "tail_status",
                string("active-unavailable"),
            );
            f.observe(&value);
            let directory = transport::segments(&f.site(), SESSION)
                .unwrap()
                .remove(0)
                .directory
                .clone();
            let manifest =
                transport::read(&directory.join("segment.json"), transport::MAX_PAYLOAD).unwrap();
            let gaps = get(&manifest, "gaps").as_array().unwrap();
            assert!(
                gaps.iter()
                    .any(|g| get(g, "native_entry_id") == &Value::Null
                        && text(g, "reason") == "ambiguous-lifecycle")
            );
            assert!(!gaps.iter().any(|g| {
                get(g, "native_entry_id")
                    .as_str()
                    .is_some_and(|s| s.starts_with("event:"))
            }));
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity
#[test]
#[ignore = "recovery_frontier"]
fn retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity() {
    isolated(
        "retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[human("human")], "ingest"));
            let s = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let cursor = s.snapshot().unwrap();
            let mut original = entry(
                "answer",
                "assistant",
                string("Original complete safe stored answer."),
            );
            set(&mut original, "parentId", string("human"));
            let projected = agent_memory::omp_projection::project(&original);
            let snapshot = get(&projected, "snapshot");
            let path = native_path(&s.directory, "snapshots", "answer");
            transport::retain(&path, snapshot, transport::MAX_PAYLOAD).unwrap();
            let before = std::fs::read(&path).unwrap();
            let mut persisted = original;
            set(
                persisted.get_mut("message").unwrap(),
                "content",
                string(&format!(
                    "Persisted prefix.{}",
                    agent_memory::omp_projection::PERSISTENCE_TRUNCATION_NOTICE
                )),
            );
            let captured =
                transport::capture(&s.directory, &cursor, &persisted, "verified", None, true)
                    .unwrap()
                    .unwrap();
            assert_eq!(
                text(&captured, "snapshot_sha256"),
                transport::digest(snapshot).unwrap()
            );
            assert_eq!(std::fs::read(path).unwrap(), before);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_new_truncated_entry_keeps_gap_and_prevents_complete_source_claim
#[test]
#[ignore = "recovery_frontier"]
fn retired_new_truncated_entry_keeps_gap_and_prevents_complete_claim() {
    isolated(
        "retired_new_truncated_entry_keeps_gap_and_prevents_complete_claim",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[human("human")], "ingest"));
            let tail = entry(
                "tail",
                "assistant",
                string(&format!(
                    "Persisted prefix.{}",
                    agent_memory::omp_projection::PERSISTENCE_TRUNCATION_NOTICE
                )),
            );
            let mut value = request(&f, &[human("human"), tail], "ingest");
            terminal(&mut value);
            let view = value.get_mut("payload").unwrap().get_mut("view").unwrap();
            set(view, "source_kind", string("retired-file"));
            set(
                view,
                "file",
                string(
                    f.repo
                        .join("explicit-retired-fixture.jsonl")
                        .to_str()
                        .unwrap(),
                ),
            );
            let result = f.observe(&value);
            assert_eq!(get(&result, "verified"), &Value::Bool(true));
            assert_eq!(
                get(&result, "source_coverage_complete"),
                &Value::Bool(false)
            );
            assert!(
                f.pending()
                    .iter()
                    .any(|r| text(r, "status") == "pending-coverage-gap")
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resumed_fork_postfork_human_is_not_blanket_inherited
#[test]
#[ignore = "recovery_frontier"]
fn resumed_fork_postfork_human_is_not_blanket_inherited() {
    isolated(
        "resumed_fork_postfork_human_is_not_blanket_inherited",
        |root| {
            let f = Fixture::new(root);
            let mut value = request(
                &f,
                &[human("inherited-human"), human("postfork-human")],
                "seal",
            );
            let payload = value.get_mut("payload").unwrap();
            set(
                payload,
                "inherited_ids",
                Value::Array(vec![string("inherited-human")]),
            );
            let view = payload.get_mut("view").unwrap();
            set(view, "fork_status", string("verified"));
            let header = view.get_mut("header").unwrap();
            set(header, "parent_session", string("actual-parent"));
            set(header, "timestamp", string("2026-11-08T01:00:00.000Z"));
            let Value::Array(parents) = view.get_mut("native_parents").unwrap() else {
                unreachable!()
            };
            set(
                &mut parents[0],
                "timestamp",
                string("2026-11-08T00:59:00.000Z"),
            );
            set(
                &mut parents[1],
                "timestamp",
                string("2026-11-08T01:01:00.000Z"),
            );
            f.observe(&value);
            let roots = transport::segments(&f.site(), SESSION)
                .unwrap()
                .iter()
                .map(|s| s.snapshot().unwrap())
                .filter(|c| text(get(c, "origin"), "kind") == "genuine-user")
                .collect::<Vec<_>>();
            assert_eq!(roots.len(), 1);
            assert_eq!(
                text(get(&roots[0], "control_interval"), "human_entry"),
                "postfork-human"
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_fork_without_time_evidence_retains_nonhuman_source_with_explicit_gap
#[test]
#[ignore = "recovery_frontier"]
fn fork_without_time_evidence_retains_nonhuman_source_with_gap() {
    isolated(
        "fork_without_time_evidence_retains_nonhuman_source_with_gap",
        |root| {
            let f = Fixture::new(root);
            let mut value = request(&f, &[human("uncertain-human")], "ingest");
            terminal(&mut value);
            let view = value.get_mut("payload").unwrap().get_mut("view").unwrap();
            set(
                view.get_mut("header").unwrap(),
                "parent_session",
                string("actual-parent"),
            );
            set(view, "fork_status", string("unavailable"));
            f.observe(&value);
            let segment = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let cursor = segment.snapshot().unwrap();
            assert_eq!(text(get(&cursor, "origin"), "kind"), "foreign-continuation");
            assert!(native_path(&segment.directory, "captures", "uncertain-human").is_file());
            let manifest = transport::read(
                &segment.directory.join("segment.json"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert!(
                get(&manifest, "gaps")
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|g| text(g, "reason") == "ambiguous-lifecycle")
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_late_native_tool_result_recovers_original_sealed_gap_without_reopening
// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resolved_sealed_tool_gap_does_not_leave_stale_tool_pending_or_block_drain
#[test]
#[ignore = "recovery_frontier"]
fn exact_late_tool_result_recovers_sealed_gap_without_reopening_or_stale_pending() {
    isolated(
        "exact_late_tool_result_recovers_sealed_gap_without_reopening_or_stale_pending",
        |root| {
            let f = Fixture::new(root);
            let call = tool_call("assistant-call", "native-call");
            f.observe(&request(&f, &[human("human"), call.clone()], "seal"));
            let original = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let cursor = original.snapshot().unwrap();
            let manifest = transport::read(
                &original.directory.join("segment.json"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            let before = (
                std::fs::read(original.directory.join("segment.json")).unwrap(),
                std::fs::read(original.directory.join("cursor.json")).unwrap(),
            );
            let missing = get(&manifest, "gaps")
                .as_array()
                .unwrap()
                .iter()
                .find(|g| get(g, "native_entry_id").as_str() == Some("tool:native-call"))
                .unwrap();
            let mut value = request(
                &f,
                &[
                    human("human"),
                    call,
                    tool_result("tool-result", "native-call", "12"),
                ],
                "ingest",
            );
            terminal(&mut value);
            f.observe(&value);
            assert_eq!(
                (
                    std::fs::read(original.directory.join("segment.json")).unwrap(),
                    std::fs::read(original.directory.join("cursor.json")).unwrap()
                ),
                before
            );
            assert!(transport::coverage(&original.directory, &manifest, false).unwrap());
            let rows = f.pending();
            assert!(!rows.iter().any(|r| matches!(
                text(r, "status"),
                "pending-native-tool" | "pending-coverage-gap"
            )));
            let row = rows
                .iter()
                .find(|r| {
                    get(r, "lifecycle") == get(&cursor, "lifecycle")
                        && text(r, "source_reference").starts_with("omp:seal:")
                })
                .unwrap();
            let source = transport::source(
                &f.repo,
                &f.runtime,
                SESSION,
                text(row, "lifecycle"),
                text(row, "observation_id"),
            )
            .unwrap();
            assert_eq!(get(&source, "coverage_complete"), &Value::Bool(true));
            assert_eq!(
                turn_sources::encoded(get(&source, "source")).unwrap(),
                turn_sources::encoded(&manifest).unwrap()
            );
            let resolution = get(&source, "gap_resolutions")
                .as_array()
                .unwrap()
                .iter()
                .find(|r| get(r, "gap_id") == get(missing, "gap_id"))
                .unwrap();
            let recovered = transport::read(
                &original.directory.join(text(resolution, "recovered_ref")),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert_eq!(text(&recovered, "toolCallId"), "native-call");
            assert_eq!(text(&recovered, "content"), "12");
            assert_eq!(transport::segments(&f.site(), SESSION).unwrap().len(), 2);
            for row in rows {
                assert_eq!(get(&acknowledge(&f, &row), "verified"), &Value::Bool(true));
            }
            assert_eq!(
                get(
                    &transport::list_pending(&f.repo, &f.runtime, 0).unwrap(),
                    "pending"
                ),
                &Value::Array(vec![])
            );
            assert_eq!(
                get(
                    &boundary::drain_source_capabilities(&f.repo, true, false, &f.runtime).unwrap(),
                    "drainable"
                ),
                &Value::Bool(true)
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_foreign_tool_call_id_cannot_resolve_actual_sealed_tool_gap
#[test]
#[ignore = "recovery_frontier"]
fn foreign_tool_call_id_never_resolves_actual_sealed_gap() {
    isolated(
        "foreign_tool_call_id_never_resolves_actual_sealed_gap",
        |root| {
            let f = Fixture::new(root);
            let call = tool_call("call", "owned-call");
            f.observe(&request(&f, &[human("human"), call.clone()], "seal"));
            let original = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let before = std::fs::read(original.directory.join("segment.json")).unwrap();
            let manifest = transport::read(
                &original.directory.join("segment.json"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            let gap = get(&manifest, "gaps")
                .as_array()
                .unwrap()
                .iter()
                .find(|g| get(g, "native_entry_id").as_str() == Some("tool:owned-call"))
                .unwrap();
            let mut value = request(
                &f,
                &[
                    human("human"),
                    call,
                    tool_result(
                        "foreign-result",
                        "foreign-call",
                        "Different invocation result.",
                    ),
                ],
                "ingest",
            );
            terminal(&mut value);
            f.observe(&value);
            assert_eq!(
                std::fs::read(original.directory.join("segment.json")).unwrap(),
                before
            );
            assert!(
                !original
                    .directory
                    .join("resolutions")
                    .join(format!("{}.json", text(gap, "gap_id")))
                    .exists()
            );
            assert!(
                f.pending()
                    .iter()
                    .any(|r| text(r, "status") == "pending-coverage-gap")
            );
            assert_eq!(
                get(
                    &boundary::drain_source_capabilities(&f.repo, true, false, &f.runtime).unwrap(),
                    "drainable"
                ),
                &Value::Bool(false)
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_unsafe_exact_recovery_does_not_append_gap_to_sealed_source
#[test]
#[ignore = "recovery_frontier"]
fn unsafe_exact_recovery_never_mutates_sealed_source() {
    isolated(
        "unsafe_exact_recovery_never_mutates_sealed_source",
        |root| {
            let f = Fixture::new(root);
            let opaque = entry("missing", "assistant", decode(r#"[{"type":"image"}]"#));
            f.observe(&request(&f, &[human("human"), opaque], "seal"));
            let original = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let before = snapshot(&original.directory);
            let unsafe_entry = entry(
                "missing",
                "assistant",
                string("password=synthetic-secret-should-not-retain"),
            );
            let mut value = request(&f, &[human("human"), unsafe_entry], "ingest");
            terminal(&mut value);
            f.observe(&value);
            assert_eq!(snapshot(&original.directory), before);
            assert!(
                f.pending()
                    .iter()
                    .any(|r| text(r, "status") == "pending-coverage-gap")
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_manager_subdirectory_same_bank_accepts_and_native_root_identity_stays_canonical
#[test]
#[ignore = "recovery_frontier"]
fn manager_subdirectory_accepts_only_canonical_bank_root_identity() {
    isolated(
        "manager_subdirectory_accepts_only_canonical_bank_root_identity",
        |root| {
            let f = Fixture::new(root);
            let sub = f.repo.join("synthetic-subdirectory");
            std::fs::create_dir(&sub).unwrap();
            let mut value = request(&f, &[human("human")], "seal");
            let view = value.get_mut("payload").unwrap().get_mut("view").unwrap();
            set(view, "cwd", string(sub.to_str().unwrap()));
            set(
                view.get_mut("header").unwrap(),
                "cwd",
                string(sub.to_str().unwrap()),
            );
            let result = f.observe(&value);
            assert_eq!(
                text(&result, "lifecycle"),
                transport::uuid5_url(&format!("{}:omp:{SESSION}:human", f.repo.display()))
            );
            let segment = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            assert!(
                transport::source_bound_origin(&segment.directory, &segment.snapshot().unwrap())
                    .unwrap()
            );
            let snapshot = transport::read(
                &native_path(&segment.directory, "snapshots", "human"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert_eq!(text(&snapshot, "native_entry_id"), "human");
        },
    );
}

fn run_finite_child(root: &Path, f: &Fixture, request: Value, open_input: bool) {
    let instance = uid();
    let directory =
        stream::directory(&boundary::base(&f.repo).unwrap(), SESSION, &instance).unwrap();
    write(
        &root.join("finite-directory"),
        directory.to_str().unwrap(),
        0o600,
    );
    let input: Box<dyn std::io::Read + Send> = if open_input {
        Box::new(std::fs::File::open(root.join("source.fifo")).unwrap())
    } else if root.join("case-empty-run").exists() {
        Box::new(std::io::Cursor::new(stream::encode_frame("FINAL", 1, &decode(r#"{"last_attempt_seq":0,"refused_count":0,"last_event_ordinal":0,"transition":null}"#), None).unwrap()))
    } else if root.join("finite-input.raw").exists() {
        Box::new(std::fs::File::open(root.join("finite-input.raw")).unwrap())
    } else {
        let mut wire = stream::encode_frame("REQUEST", 1, &envelope(request, 1), None).unwrap();
        wire.extend(stream::encode_frame("FINAL",2,&decode(r#"{"last_attempt_seq":1,"refused_count":0,"last_event_ordinal":1,"transition":null}"#),None).unwrap());
        Box::new(std::io::Cursor::new(wire))
    };
    let output = std::fs::File::create(root.join("finite-response-wire")).unwrap();
    let writer = Arc::new(stream::ResponseWriter::start(
        stream::Output::Stream(Box::new(output)),
        &instance,
    ));
    let mut startup = |repo: &Path, session: &str, instance: &str, life: &str| {
        let result =
            boundary::stream_startup(repo, session, instance, life, &f.receipt, &f.runtime);
        if result.as_ref().is_err_and(|error| {
            matches!(
                error.kind(),
                agent_memory::error::Kind::SourceTransportBusy { .. }
            )
        }) {
            write(&root.join("startup-busy"), b"", 0o600);
        }
        result
    };
    let result = stream::run(
        &f.repo,
        SESSION,
        &instance,
        LIFE,
        &mut startup,
        stream::RunOptions {
            authority: Arc::new(stream::Installed::new(&f.runtime)),
            receipt: Some(f.receipt.clone()),
            input,
            writer,
            clocks: stream::ClockWindows::system,
            timing: stream::Timing::system(),
        },
    );
    let code = result.as_ref().copied().unwrap_or(1);
    write(
        &root.join("operation-result.json"),
        turn_sources::encoded(&match result {
            Ok(code) => object([("exit_code", Value::Int(code.into()))]),
            Err(error) => object([("error", string(&error.to_string()))]),
        })
        .unwrap(),
        0o600,
    );
    if code != 0 {
        std::process::exit(code);
    }
}
fn response_frames(path: &Path) -> Vec<(String, u64, Value)> {
    let bytes = std::fs::read(path).unwrap();
    let mut rest = bytes.as_slice();
    let mut result = Vec::new();
    while !rest.is_empty() {
        let end = rest.iter().position(|b| *b == b'\n').unwrap();
        assert!(end < 256);
        let fields = std::str::from_utf8(&rest[..end])
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0], "AM249");
        let length = fields[3].parse::<usize>().unwrap();
        assert!(length <= 65536);
        let value =
            json::decode_bytes(&rest[end + 1..end + 1 + length], Default::default()).unwrap();
        result.push((fields[1].to_owned(), fields[2].parse().unwrap(), value));
        rest = &rest[end + 1 + length..];
    }
    result
}

// frontier obligation: tests/contracts/test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_contended_entry_refuses_current_frame_and_stops
#[test]
#[ignore = "recovery_frontier"]
fn contended_entry_refuses_current_frame_and_stops_with_open_input() {
    use std::io::Write;
    isolated(
        "contended_entry_refuses_current_frame_and_stops_with_open_input",
        |root| {
            let f = Fixture::new(root);
            let proposal = capture_proposal(root, &f);
            let request = request(&f, &[human("prestart-human")], "ingest");
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request).unwrap(),
                0o600,
            );
            let fifo = root.join("source.fifo");
            command(root, &["mkfifo".into(), fifo.clone().into_os_string()]);
            let observer = operation_child(root, None, "", "finite-run-open");
            let mut input = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();
            let response = root.join("finite-response-wire");
            let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(12)).unwrap();
            while !std::fs::metadata(&response).is_ok_and(|m| m.len() > 0) {
                assert!(!deadline.expired());
                std::thread::sleep(Duration::from_millis(10));
            }
            let initial = response_frames(&response);
            assert_eq!(initial.len(), 1);
            assert_eq!(initial[0].0, "READY");
            let writer_stage = "transaction-locked-before-inventory";
            let writer = capture_child(root, &f, &proposal, Some(writer_stage));
            wait_path(
                &root
                    .join("capture-rendezvous")
                    .join(format!("{writer_stage}.ready")),
            );
            let owner =
                transport::read(&f.repo.join("memory-bank/.agent-memory.lock"), 4096).unwrap();
            assert_eq!(
                get(&owner, "operation_id"),
                get(
                    &transport::read(&proposal, 16 * 1024 * 1024).unwrap(),
                    "operation_id"
                )
            );
            let pid = get(&owner, "pid").as_int().unwrap().as_str().to_owned();
            command(
                root,
                &["/bin/kill".into(), "-STOP".into(), pid.clone().into()],
            );
            let envelope = envelope(request, 1);
            let mut wire = stream::encode_frame("REQUEST", 1, &envelope, None).unwrap();
            wire.extend(stream::encode_frame("REQUEST", 2, &envelope, None).unwrap());
            let start = agent_memory::clock::monotonic_ns();
            input.write_all(&wire).unwrap();
            input.flush().unwrap();
            let output = observer.wait();
            assert_eq!(output.status, agent_memory::process::Status::Code(1));
            assert!(agent_memory::clock::monotonic_ns() - start >= 29_000_000_000);
            let frames = response_frames(&response);
            let terminal = frames
                .iter()
                .filter(|(kind, _, _)| kind != "READY")
                .collect::<Vec<_>>();
            assert_eq!(terminal.len(), 1);
            assert_eq!(terminal[0].0, "REFUSED");
            assert_eq!(terminal[0].1, 1);
            assert_eq!(text(&terminal[0].2, "code"), "busy-exhausted");
            assert_eq!(text(&terminal[0].2, "outcome"), "unknown");
            assert_eq!(text(&terminal[0].2, "status"), "pending");
            let directory = child_directory(root);
            for name in ["attempts", "started", "outcomes", "closed.json"] {
                assert!(!directory.join(name).exists(), "{name}");
            }
            write(
                &root
                    .join("capture-rendezvous")
                    .join(format!("{writer_stage}.release")),
                b"",
                0o600,
            );
            command(root, &["/bin/kill".into(), "-CONT".into(), pid.into()]);
            assert_capture_succeeds(writer);
            drop(input);
            let inventory =
                stream::inventory(&stream::Installed::new(&f.runtime), &f.repo, None, false)
                    .unwrap();
            let row = get(&inventory, "streams")
                .as_array()
                .unwrap()
                .iter()
                .find(|r| get(r, "instance_id") == get(&initial[0].2, "instance_id"))
                .unwrap();
            assert_eq!(get(row, "closed"), &Value::Bool(false));
            assert!(
                get(row, "pending")
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| get(p, "code").as_str() == Some("open-stream"))
            );
        },
    );
}

#[test]
fn default_created_lock_keeps_named_inode_after_parent_swap() {
    let world = World::new().unwrap();
    let parent = world.root.join("created-parent");
    let path = parent.join("lock");
    let held = turn_sources::private_lock(&path, false, false).unwrap();
    let old = world.root.join("displaced-created-parent");
    std::fs::rename(&parent, &old).unwrap();
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::hard_link(old.join("lock"), &path).unwrap();
    held.recheck().unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&parent).unwrap();
    std::fs::rename(old, parent).unwrap();
    held.finish().unwrap();
}

fn observed(
    f: &Fixture,
    value: &Value,
    call: transport::Observation<'_, '_>,
) -> agent_memory::error::Result<Value> {
    transport::observe(
        &f.repo,
        &f.runtime,
        value,
        call,
        &transport::InstalledScope {
            receipt: Some(&f.receipt),
            proof: &f.proof,
            runtime_root: &f.runtime,
        },
    )
}
fn remove_key(value: &mut Value, key: &str) {
    let Value::Object(fields) = value else {
        panic!("expected fixture object")
    };
    fields.retain(|(name, _)| name != key);
}
fn completion_request(row: &Value) -> Value {
    object([
        ("session", get(row, "session").clone()),
        ("lifecycle", get(row, "lifecycle").clone()),
        ("observation_id", get(row, "observation_id").clone()),
        ("source_sha256", get(row, "source_sha256").clone()),
        ("outcome", string("no-learning")),
        (
            "assessment",
            object([
                ("source_id", get(row, "observation_id").clone()),
                (
                    "source_review",
                    string(
                        "Reviewed the exact original ephemeral source and manifest; no durable project learning.",
                    ),
                ),
                ("candidates", Value::Array(vec![])),
            ]),
        ),
    ])
}
fn wake_request(result: &Value) -> Value {
    object([
        ("schema", string("agent-memory/omp-observer-request-v2")),
        ("action", string("wake")),
        ("session", string(SESSION)),
        ("lifecycle", get(result, "lifecycle").clone()),
        ("payload", get(result, "wake_payload").clone()),
    ])
}
fn real_window() -> (stream::ClockWindows, Arc<stream::ResponseWriter>, String) {
    let instance = uid();
    let writer = Arc::new(stream::ResponseWriter::unstarted(&instance, vec![]));
    let recipient = writer.clone();
    // The baseline's provisional Darwin arithmetic control, with a real local clock and recipient.
    let clocks = stream::ClockWindows::new(
        &instance,
        Arc::new(move || recipient.live()),
        stream::Platform::Darwin,
        Arc::new(agent_memory::clock::monotonic_ns),
        None,
    );
    let nonce = uid();
    calibrate_window(&clocks, &writer, &nonce);
    (clocks, writer, nonce)
}
fn calibrate_window(clocks: &stream::ClockWindows, writer: &stream::ResponseWriter, nonce: &str) {
    let j0 = agent_memory::clock::monotonic_ns();
    clocks
        .control(
            "CLOCK",
            &object([
                ("nonce", string(nonce)),
                ("j0", string(&j0.to_string())),
                ("j_deadline", string(&(j0 + 950_000_000).to_string())),
            ]),
            writer,
        )
        .unwrap();
    let j3 = agent_memory::clock::monotonic_ns();
    clocks
        .control(
            "CLOCK-ACK",
            &object([
                ("nonce", string(nonce)),
                ("j0", string(&j0.to_string())),
                ("j3", string(&j3.to_string())),
            ]),
            writer,
        )
        .unwrap();
}
fn with_window(f: &Fixture, value: &Value, clocks: &stream::ClockWindows, nonce: &str) -> Value {
    observed(
        f,
        value,
        transport::Observation {
            receipt: Some(&f.receipt),
            delivery_window: clocks.window(nonce).map(stream::DeliveryWindow::Verified),
            ..Default::default()
        },
    )
    .unwrap()
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_delayed_old_checked_view_refuses_new_session_and_real_new_human_has_one_root
#[test]
#[ignore = "recovery_frontier"]
fn delayed_old_view_never_creates_new_session_authority() {
    isolated(
        "delayed_old_view_never_creates_new_session_authority",
        |root| {
            let f = Fixture::new(root);
            let old = request(&f, &[human("old-human")], "seal");
            f.observe(&old);
            let old_segment = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let before = snapshot(&old_segment.directory);
            let new_session = uid();
            let new_life = uid();
            let mut delayed = old.clone();
            set(&mut delayed, "session", string(&new_session));
            set(&mut delayed, "lifecycle", string(&new_life));
            let mut started = false;
            assert!(
                observed(
                    &f,
                    &delayed,
                    transport::Observation {
                        receipt: Some(&f.receipt),
                        on_started: Some(&mut || {
                            started = true;
                            Ok(())
                        }),
                        ..Default::default()
                    }
                )
                .is_err()
            );
            assert!(!started);
            assert!(
                transport::segments(&f.site(), &new_session)
                    .unwrap()
                    .is_empty()
            );
            let mut empty = request(&f, &[], "seal");
            set(&mut empty, "session", string(&new_session));
            set(&mut empty, "lifecycle", string(&new_life));
            set(
                empty
                    .get_mut("payload")
                    .unwrap()
                    .get_mut("view")
                    .unwrap()
                    .get_mut("header")
                    .unwrap(),
                "id",
                string(&new_session),
            );
            set(
                empty.get_mut("payload").unwrap().get_mut("view").unwrap(),
                "session",
                string(&new_session),
            );
            f.observe(&empty);
            assert!(
                transport::segments(&f.site(), &new_session)
                    .unwrap()
                    .is_empty()
            );
            let mut fresh = request(&f, &[human("new-human")], "seal");
            set(&mut fresh, "session", string(&new_session));
            set(&mut fresh, "lifecycle", string(&new_life));
            set(
                fresh
                    .get_mut("payload")
                    .unwrap()
                    .get_mut("view")
                    .unwrap()
                    .get_mut("header")
                    .unwrap(),
                "id",
                string(&new_session),
            );
            set(
                fresh.get_mut("payload").unwrap().get_mut("view").unwrap(),
                "session",
                string(&new_session),
            );
            f.observe(&fresh);
            let actual = transport::segments(&f.site(), &new_session).unwrap();
            assert_eq!(actual.len(), 1);
            assert_eq!(
                text(
                    get(&actual[0].snapshot().unwrap(), "control_interval"),
                    "human_entry"
                ),
                "new-human"
            );
            assert_eq!(snapshot(&old_segment.directory), before);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_invalid_views_refuse_before_real_transport_contention_and_bad_receipt
#[test]
#[ignore = "recovery_frontier"]
fn invalid_view_precedes_contended_transport_and_bad_receipt() {
    isolated(
        "invalid_view_precedes_contended_transport_and_bad_receipt",
        |root| {
            let f = Fixture::new(root);
            let (mut coverage, _, _) = finite(&f);
            let old_directory = coverage.directory().to_path_buf();
            coverage.close_live();
            let held = turn_sources::private_lock(
                &boundary::base(&f.repo).unwrap().join("omp-transport.lock"),
                false,
                false,
            )
            .unwrap();
            let before = snapshot(&root.join("state"));
            for shape in [true, false] {
                let mut value = request(&f, &[human("human")], "seal");
                let view = value.get_mut("payload").unwrap().get_mut("view").unwrap();
                if shape {
                    set(
                        view.get_mut("header").unwrap(),
                        "version",
                        Value::Int(2u8.into()),
                    );
                } else {
                    let foreign = root.join("foreign");
                    set(view, "cwd", string(foreign.to_str().unwrap()));
                    set(
                        view.get_mut("header").unwrap(),
                        "cwd",
                        string(foreign.to_str().unwrap()),
                    );
                }
                let mut started = false;
                let bad = object([]);
                let error = observed(
                    &f,
                    &value,
                    transport::Observation {
                        receipt: Some(&bad),
                        on_started: Some(&mut || {
                            started = true;
                            Ok(())
                        }),
                        ..Default::default()
                    },
                )
                .unwrap_err();
                assert!(!matches!(
                    error.kind(),
                    agent_memory::error::Kind::SourceTransportBusy { .. }
                ));
                assert!(!started);
                assert_eq!(snapshot(&root.join("state")), before);
            }
            held.finish().unwrap();
            assert!(!old_directory.join("retirement.json").exists());
            assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
            let fences = stream::retire_for_recovery(
                &stream::Installed::new(&f.runtime),
                &f.repo,
                SESSION,
                None,
            )
            .unwrap();
            assert_eq!(fences.len(), 1);
            assert!(old_directory.join("retirement.json").is_file());
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_completed_historical_origin_exact_replay_is_read_only_and_changed_request_refuses
#[test]
#[ignore = "recovery_frontier"]
fn completed_historical_origin_allows_only_exact_readonly_replay() {
    isolated(
        "completed_historical_origin_allows_only_exact_readonly_replay",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[human("human")], "seal"));
            let rows = f.pending();
            assert_eq!(rows.len(), 2);
            for row in &rows {
                assert_eq!(get(&acknowledge(&f, row), "verified"), &Value::Bool(true));
            }
            let s = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let mut cursor = s.snapshot().unwrap();
            remove_key(cursor.get_mut("control_interval").unwrap(), "human_entry");
            transport::replace_cursor(&s.directory.join("cursor.json"), &cursor).unwrap();
            let base = boundary::base(&f.repo).unwrap();
            let before = snapshot(&base);
            let value = completion_request(&rows[0]);
            let replay = transport::complete(&f.repo, &f.runtime, &value).unwrap();
            assert_eq!(get(&replay, "already_completed"), &Value::Bool(true));
            assert_eq!(
                text(&replay, "origin_qualification"),
                "historical-unverified-native-origin"
            );
            let mut changed = value;
            set(
                changed.get_mut("assessment").unwrap(),
                "source_review",
                string("Different new assertion."),
            );
            assert!(
                transport::complete(&f.repo, &f.runtime, &changed)
                    .unwrap_err()
                    .to_string()
                    .contains("independently stored root origin")
            );
            assert_eq!(snapshot(&base), before);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_same_role_position_decoy_never_resolves_real_event_gap
#[test]
#[ignore = "recovery_frontier"]
fn adjacent_role_decoy_cannot_resolve_exact_event_gap() {
    isolated(
        "adjacent_role_decoy_cannot_resolve_exact_event_gap",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[human("human")], "ingest"));
            let s = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let cursor = s.snapshot().unwrap();
            let event = uid();
            transport::retain(
                &s.directory.join("admissions").join(format!("{event}.json")),
                &object([
                    ("schema", string("agent-memory/omp-admission-v1")),
                    ("session", string(SESSION)),
                    ("lifecycle", get(&cursor, "lifecycle").clone()),
                    ("capture_id", string(&event)),
                    ("event_ordinal", Value::Int(2u8.into())),
                    ("role", string("assistant")),
                    ("attribution", string("agent")),
                    ("tool_call_ids", Value::Array(vec![])),
                ]),
                4096,
            )
            .unwrap();
            let gap = transport::gap(
                &s.directory,
                &cursor,
                Some(&format!("event:{event}")),
                "missing-stored-source",
            )
            .unwrap();
            transport::seal(&s.directory, &cursor).unwrap();
            let manifest = std::fs::read(s.directory.join("segment.json")).unwrap();
            f.observe(&request(
                &f,
                &[human("human"), answer("position-decoy")],
                "seal",
            ));
            assert!(
                !s.directory
                    .join("resolutions")
                    .join(format!("{}.json", text(&gap, "gap_id")))
                    .exists()
            );
            assert_eq!(
                std::fs::read(s.directory.join("segment.json")).unwrap(),
                manifest
            );
            let rows = f
                .pending()
                .into_iter()
                .filter(|r| {
                    get(r, "lifecycle") == get(&cursor, "lifecycle")
                        && r.get("observation_id").is_some()
                })
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 2);
            assert!(
                rows.iter()
                    .all(|r| text(r, "status") == "pending-coverage-gap")
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_weak_historical_event_resolution_does_not_authorize_new_acknowledgement
#[test]
#[ignore = "recovery_frontier"]
fn weak_historical_event_resolution_never_grants_new_completion() {
    isolated(
        "weak_historical_event_resolution_never_grants_new_completion",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[human("human"), answer("answer")], "ingest"));
            let s = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let mut cursor = s.snapshot().unwrap();
            let event = uid();
            set(
                cursor.get_mut("control_interval").unwrap(),
                "metadata_matches",
                object([(event.as_str(), string("answer"))]),
            );
            transport::replace_cursor(&s.directory.join("cursor.json"), &cursor).unwrap();
            let missing = transport::gap(
                &s.directory,
                &cursor,
                Some(&format!("event:{event}")),
                "missing-stored-source",
            )
            .unwrap();
            let manifest = transport::seal(&s.directory, &cursor).unwrap();
            let reference = format!(
                "snapshots/{}.json",
                agent_memory::developer_identity::digest(Some(b"answer")).unwrap()
            );
            let payload =
                transport::read(&s.directory.join(&reference), transport::MAX_PAYLOAD).unwrap();
            transport::retain(
                &s.directory
                    .join("resolutions")
                    .join(format!("{}.json", text(&missing, "gap_id"))),
                &object([
                    ("schema", string("agent-memory/omp-gap-resolution-v1")),
                    (
                        "manifest_sha256",
                        string(&transport::digest(&manifest).unwrap()),
                    ),
                    ("gap_id", get(&missing, "gap_id").clone()),
                    ("recovered_ref", string(&reference)),
                    (
                        "recovered_sha256",
                        string(&transport::digest(&payload).unwrap()),
                    ),
                    ("reason", string("verified-native-reassociation")),
                ]),
                4096,
            )
            .unwrap();
            assert!(transport::coverage(&s.directory, &manifest, true).unwrap());
            assert!(!transport::coverage(&s.directory, &manifest, false).unwrap());
            f.observe(&request(&f, &[], "ingest"));
            let rows = f
                .pending()
                .into_iter()
                .filter(|r| r.get("observation_id").is_some())
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 2);
            for row in rows {
                assert_eq!(text(&row, "status"), "pending-coverage-gap");
                assert!(
                    transport::complete(&f.repo, &f.runtime, &completion_request(&row))
                        .unwrap_err()
                        .to_string()
                        .contains("unresolved coverage gaps")
                );
            }
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_malformed_wake_state_suppresses_reservation_but_retains_and_enrolls_real_source
#[test]
#[ignore = "recovery_frontier"]
fn malformed_wake_namespace_preserves_sources_without_spending_window() {
    isolated(
        "malformed_wake_namespace_preserves_sources_without_spending_window",
        |root| {
            let f = Fixture::new(root);
            let wakes = transport::session_dir(&f.site(), SESSION)
                .unwrap()
                .join("root-wakes");
            transport::retain(
                &wakes.join("unknown.json"),
                &decode(r#"{"unexpected":"preserve"}"#),
                4096,
            )
            .unwrap();
            let result = f.observe(&request(&f, &[human("human")], "seal"));
            assert_eq!(get(&result, "verified"), &Value::Bool(true));
            assert!(!get(&result, "faults").as_array().unwrap().is_empty());
            let (rows, faults) = transport::pending_inventory(&f.site()).unwrap();
            assert!(!faults.is_empty());
            assert_eq!(
                rows.iter()
                    .filter(|r| r.get("observation_id").is_some())
                    .count(),
                2
            );
            let (owner, _writer, nonce) = real_window();
            let refused = with_window(&f, &wake_request(&result), &owner, &nonce);
            assert_eq!(get(&refused, "continue"), &Value::Bool(false));
            assert_eq!(text(&refused, "status"), "pending-wake-state-unverifiable");
            assert_eq!(
                transport::read(&wakes.join("unknown.json"), 4096).unwrap(),
                decode(r#"{"unexpected":"preserve"}"#)
            );
            owner.end();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_no_active_resume_paths_never_append_to_previously_sealed_instance_segment
#[test]
#[ignore = "recovery_frontier"]
fn resumed_carrier_message_and_unavailable_tail_never_append_to_sealed_parent() {
    scenarios(
        "resumed_carrier_message_and_unavailable_tail_never_append_to_sealed_parent",
        &["carrier", "message", "tail"],
        |root, kind| {
            let f = Fixture::new(root);
            let mut first = request(&f, &[human("old-human")], "seal");
            terminal(&mut first);
            f.observe(&first);
            let old = transport::segments(&f.site(), SESSION).unwrap().remove(0);
            let before = snapshot(&old.directory);
            let entries = match kind {
                "carrier" => vec![object([
                    ("type", string("unknown-native-carrier")),
                    ("id", string("later")),
                    ("parentId", Value::Null),
                ])],
                "message" => vec![answer("later")],
                _ => vec![],
            };
            let mut resumed = request(&f, &entries, "seal");
            terminal(&mut resumed);
            if kind == "tail" {
                set(
                    resumed.get_mut("payload").unwrap().get_mut("view").unwrap(),
                    "tail_status",
                    string("active-unavailable"),
                );
            }
            assert_eq!(get(&f.observe(&resumed), "verified"), &Value::Bool(true));
            assert_eq!(snapshot(&old.directory), before);
            let successors = transport::segments(&f.site(), SESSION)
                .unwrap()
                .into_iter()
                .filter(|s| s.directory != old.directory)
                .collect::<Vec<_>>();
            assert_eq!(successors.len(), 1);
            assert!(
                !get(
                    &transport::read(
                        &successors[0].directory.join("segment.json"),
                        transport::MAX_PAYLOAD
                    )
                    .unwrap(),
                    "gaps"
                )
                .as_array()
                .unwrap()
                .is_empty()
            );
            assert!(
                transport::pending_inventory(&f.site())
                    .unwrap()
                    .1
                    .is_empty()
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_expired_internal_delivery_window_never_reserves_root_wake
#[test]
#[ignore = "recovery_frontier"]
fn expired_real_window_does_not_spend_native_human_budget() {
    isolated(
        "expired_real_window_does_not_spend_native_human_budget",
        |root| {
            let f = Fixture::new(root);
            let result = f.observe(&request(&f, &[human("human")], "seal"));
            let wake = wake_request(&result);
            let (old, _writer, nonce) = real_window();
            std::thread::sleep(Duration::from_millis(960));
            let refused = with_window(&f, &wake, &old, &nonce);
            assert_eq!(get(&refused, "verified"), &Value::Bool(true));
            assert_eq!(get(&refused, "continue"), &Value::Bool(false));
            assert!(root_wakes(&f).is_empty());
            old.end();
            let (fresh, _writer, nonce) = real_window();
            assert_eq!(
                get(&with_window(&f, &wake, &fresh, &nonce), "continue"),
                &Value::Bool(true)
            );
            let (again, _writer, nonce) = real_window();
            assert_eq!(
                get(&with_window(&f, &wake, &again, &nonce), "continue"),
                &Value::Bool(false)
            );
            assert_eq!(root_wakes(&f).len(), 1);
            fresh.end();
            again.end();
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_held_and_gapped_segments_do_not_gain_completion_authority
#[test]
#[ignore = "recovery_frontier"]
fn held_and_gapped_segment_rows_never_authorize_completion() {
    scenarios(
        "held_and_gapped_segment_rows_never_authorize_completion",
        &["gapped", "held"],
        |root, mode| {
            let held = mode == "held";
            let h = held.then(|| held_fixture(root));
            let ordinary = (!held).then(|| Fixture::new(root));
            let f = h
                .as_ref()
                .map(|h| &h.fixture)
                .or(ordinary.as_ref())
                .unwrap();
            let s = segment(f, true, !held);
            let ns = shared(f, &s.cursor);
            let before = snapshot(&ns.directory);
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            assert_eq!(context.held(), held);
            let rows = joined(f, &s);
            assert_eq!(rows.len(), s.enrollments.len());
            for row in rows {
                assert_eq!(
                    text(&row, "status"),
                    if held {
                        "pending-assessment"
                    } else {
                        "pending-coverage-gap"
                    }
                );
                assert!(
                    transport::complete(&f.repo, &f.runtime, &completion_request(&row)).is_err()
                );
            }
            assert_eq!(snapshot(&ns.directory), before);
            assert!(
                std::fs::read_dir(&ns.directory).unwrap().all(|e| !e
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "done"))
            );
        },
    );
}

// frontier obligation: tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_corrupt_source_and_completion_faults_remain_visible
#[test]
#[ignore = "recovery_frontier"]
fn missing_corrupt_source_and_completion_records_remain_visible() {
    scenarios(
        "missing_corrupt_source_and_completion_records_remain_visible",
        &["missing-source", "corrupt-source", "corrupt-completion"],
        |root, mutation| {
            let f = Fixture::new(root);
            let s = segment(&f, true, false);
            let rows = joined(&f, &s);
            let ns = shared(&f, &s.cursor);
            let row = &rows[0];
            let source = source_path(&ns.directory, text(row, "source_reference"));
            match mutation {
                "missing-source" => std::fs::remove_file(source).unwrap(),
                "corrupt-source" => write(&source, b"{", 0o600),
                _ => {
                    acknowledge(&f, row);
                    write(
                        &ns.directory
                            .join(format!("{}.done", text(row, "observation_id"))),
                        b"{\"invalid\":\"completion\"}",
                        0o600,
                    );
                }
            }
            let before = snapshot(&ns.directory);
            let joined = joined(&f, &s);
            assert!(
                joined
                    .iter()
                    .any(|r| text(r, "status") == "registration-fault"
                        && !get(r, "faults").as_array().unwrap().is_empty())
            );
            assert_eq!(snapshot(&ns.directory), before);
        },
    );
}

fn registry_path(root: &Path) -> PathBuf {
    std::fs::read_to_string(root.join("registry-selected-path"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("registry-bank/.agent-memory.lock"))
}
fn registry_operation(root: &Path, operation: &str) {
    write(
        &root.join("registry-pid"),
        std::process::id().to_string(),
        0o600,
    );
    let path = registry_path(root);
    let result = if operation == "registry-probe" {
        match agent_memory::local_registry::check_canonical_writer(&path, &["capture"]) {
            Ok(()) => string("absent"),
            Err(agent_memory::local_registry::CanonicalWriterError::Busy) => string("busy"),
            Err(agent_memory::local_registry::CanonicalWriterError::Initializing) => {
                string("initializing")
            }
            Err(error) => object([("error", string(&error.to_string()))]),
        }
    } else {
        let operation_id =
            std::fs::read_to_string(root.join("registry-operation-id")).unwrap_or_else(|_| uid());
        let operation_name = std::fs::read_to_string(root.join("registry-operation-name"))
            .unwrap_or_else(|_| "bank-capture".into());
        match agent_memory::local_registry::exclusive(
            &path,
            &operation_id,
            &operation_name,
            Default::default(),
            |owner| {
                let marker = path.parent().unwrap().join(".agent-memory-transaction.md");
                let transaction = std::fs::read(root.join("registry-transaction-bytes")).ok();
                if let Some(raw) = &transaction {
                    write(&marker, raw, 0o600);
                }
                write(&root.join("registry-owner"), owner, 0o600);
                if operation == "registry-writer" {
                    let deadline =
                        agent_memory::clock::Deadline::after(Duration::from_secs(45)).unwrap();
                    while !root.join("registry-writer-release").exists() {
                        assert!(
                            !deadline.expired(),
                            "Owned canonical writer was not released"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                if root.join("registry-canonical-write.json").exists() {
                    let plan = transport::read(&root.join("registry-canonical-write.json"), 65536)
                        .unwrap();
                    write(
                        Path::new(text(&plan, "path")),
                        text(&plan, "content"),
                        0o644,
                    );
                }
                if transaction.is_some() {
                    std::fs::remove_file(marker).unwrap();
                }
                Ok(())
            },
        ) {
            Ok(()) => string("released"),
            Err(error) => object([("error", string(&error.to_string()))]),
        }
    };
    write(
        &root.join("operation-result.json"),
        turn_sources::encoded(&result).unwrap(),
        0o600,
    );
}
fn registry_result(root: &Path) -> Value {
    transport::read(&root.join("operation-result.json"), 65536).unwrap()
}
fn start_registry_writer(root: &Path, stage: Option<&str>) -> OperationChild {
    turn_sources::private_mkdir(registry_path(root).parent().unwrap()).unwrap();
    let child = operation_child(root, stage, "pause", "registry-writer");
    if let Some(stage) = stage {
        ready(root, stage);
    } else {
        wait_path(&root.join("registry-owner"));
    }
    child
}
fn finish_registry_writer(root: &Path, writer: OperationChild) {
    write(&root.join("registry-writer-release"), b"", 0o600);
    assert_eq!(writer.wait().status, agent_memory::process::Status::Code(0));
}
fn registry_interleave(root: &Path, boundary: &str, replacement: bool) {
    let mut writer = Some(start_registry_writer(root, None));
    let path = registry_path(root);
    let owner = std::fs::read(&path).unwrap();
    let stage = format!("registry.probe.{boundary}");
    let probe = operation_child(root, Some(&stage), "pause", "registry-probe");
    ready(root, &stage);
    if replacement {
        std::fs::rename(&path, path.with_extension("displaced")).unwrap();
        write(&path, &owner, 0o600);
    } else {
        finish_registry_writer(root, writer.take().unwrap());
    }
    release(root, &stage);
    assert_eq!(probe.wait().status, agent_memory::process::Status::Code(0));
    let result = registry_result(root);
    if replacement {
        assert!(result.get("error").is_some(), "{boundary}: {result:?}");
        assert_eq!(std::fs::read(&path).unwrap(), owner);
        assert_eq!(
            std::fs::read(path.with_extension("displaced")).unwrap(),
            owner
        );
        finish_registry_writer(root, writer.take().unwrap());
        std::fs::remove_file(path.with_extension("displaced")).unwrap();
        std::fs::remove_file(&path).unwrap();
    } else {
        assert!(
            result == string("absent")
                || (boundary == "after-final-name-stat" && result == string("busy")),
            "{boundary}: {result:?}"
        );
        assert!(!path.exists());
        agent_memory::local_registry::check_canonical_writer(&path, &["capture"]).unwrap();
    }
    for file in [
        "registry-owner",
        "registry-writer-release",
        "operation-result.json",
    ] {
        if root.join(file).exists() {
            std::fs::remove_file(root.join(file)).unwrap();
        }
    }
}

// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery
#[test]
fn clean_writer_release_at_every_real_probe_boundary_is_not_recovery() {
    isolated(
        "clean_writer_release_at_every_real_probe_boundary_is_not_recovery",
        |root| {
            for stage in [
                "after-lstat",
                "before-open",
                "after-open",
                "after-initial-fstat",
                "after-first-name-stat",
                "after-read-fstat",
                "after-read-name-stat",
                "before-flock",
                "after-busy-flock",
                "after-final-fstat",
                "after-final-name-stat",
            ] {
                registry_interleave(root, stage, false);
            }
        },
    );
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_replacement_at_checked_boundaries_remains_hard_refusal
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_rebound_inode_after_flock_observation_is_not_classified_as_busy
#[test]
fn replacement_at_every_checked_probe_boundary_is_a_hard_refusal() {
    isolated(
        "replacement_at_every_checked_probe_boundary_is_a_hard_refusal",
        |root| {
            for stage in [
                "before-open",
                "after-initial-fstat",
                "after-read-fstat",
                "before-flock",
                "after-busy-flock",
                "after-final-fstat",
            ] {
                registry_interleave(root, stage, true);
            }
        },
    );
}

// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_requires_exact_safe_slot_properties
#[test]
fn empty_writer_slot_requires_private_unlinked_ordinary_file() {
    let world = World::new().unwrap();
    let path = registry_path(&world.root);
    for mutation in ["mode", "hardlink", "symlink", "directory", "fifo"] {
        write(&path, b"", 0o600);
        let alias = path.with_extension("alias");
        match mutation {
            "mode" => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap()
            }
            "hardlink" => std::fs::hard_link(&path, &alias).unwrap(),
            "symlink" => {
                std::fs::rename(&path, &alias).unwrap();
                symlink(&alias, &path).unwrap();
            }
            "directory" => {
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            _ => {
                std::fs::remove_file(&path).unwrap();
                world.run(["mkfifo".as_ref(), path.as_os_str()]).unwrap();
            }
        }
        let error =
            agent_memory::local_registry::check_canonical_writer(&path, &["capture"]).unwrap_err();
        assert!(
            matches!(
                error,
                agent_memory::local_registry::CanonicalWriterError::Contract(_)
            ),
            "{mutation}: {error}"
        );
        if mutation == "directory" {
            std::fs::remove_dir(&path).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        if alias.exists() {
            std::fs::remove_file(alias).unwrap();
        }
    }
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_cannot_rebind_its_pinned_bank
#[test]
fn empty_writer_probe_cannot_follow_rebound_pinned_bank() {
    isolated(
        "empty_writer_probe_cannot_follow_rebound_pinned_bank",
        |root| {
            let path = registry_path(root);
            write(&path, b"", 0o600);
            let stage = "registry.probe.after-open";
            let child = operation_child(root, Some(stage), "pause", "registry-probe");
            ready(root, stage);
            let moved = root.join("displaced-registry-bank");
            std::fs::rename(path.parent().unwrap(), &moved).unwrap();
            turn_sources::private_mkdir(path.parent().unwrap()).unwrap();
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(registry_result(root).get("error").is_some());
            assert_eq!(
                std::fs::read(moved.join(".agent-memory.lock")).unwrap(),
                b""
            );
            assert!(!path.exists());
        },
    );
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_bank_replaced_by_symlink_before_open_is_typed_without_closing_foreign_fds
#[test]
fn symlink_rebound_bank_refuses_without_closing_unrelated_descriptor() {
    isolated(
        "symlink_rebound_bank_refuses_without_closing_unrelated_descriptor",
        |root| {
            use std::io::Read;
            let path = registry_path(root);
            write(&path, b"", 0o600);
            let unrelated = root.join("unrelated");
            write(&unrelated, b"unrelated descriptor remains readable", 0o600);
            let mut descriptor = std::fs::File::open(&unrelated).unwrap();
            let stage = "registry.probe.after-lstat";
            let child = operation_child(root, Some(stage), "pause", "registry-probe");
            ready(root, stage);
            let moved = root.join("displaced-registry-bank");
            std::fs::rename(path.parent().unwrap(), &moved).unwrap();
            symlink(&moved, path.parent().unwrap()).unwrap();
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(registry_result(root).get("error").is_some());
            let mut bytes = Vec::new();
            descriptor.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"unrelated descriptor remains readable");
            assert_eq!(
                std::fs::read(moved.join(".agent-memory.lock")).unwrap(),
                b""
            );
        },
    );
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_initial_lstat_io_error_is_typed_without_opening_descriptors
#[test]
fn real_marker_inspection_permission_failure_is_a_typed_refusal() {
    let world = World::new().unwrap();
    let path = registry_path(&world.root);
    write(&path, b"", 0o600);
    let parent = path.parent().unwrap();
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o000)).unwrap();
    let result = agent_memory::local_registry::check_canonical_writer(&path, &["capture"]);
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        Err(agent_memory::local_registry::CanonicalWriterError::Contract(_))
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"");
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_noncontention_first_ex_error_removes_only_creators_empty_slot
#[test]
fn forced_first_ex_syscall_errors_cleanup_only_created_empty_slot() {
    isolated(
        "forced_first_ex_syscall_errors_cleanup_only_created_empty_slot",
        |root| {
            for stage in [
                "registry.exclusive.first-ex-enolck",
                "registry.exclusive.first-ex-enotsup",
            ] {
                let child = operation_child(root, Some(stage), "error", "registry-once");
                assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
                assert!(registry_result(root).get("error").is_some());
                assert!(!registry_path(root).exists());
                assert!(!root.join("registry-owner").exists());
                agent_memory::local_registry::exclusive(
                    &registry_path(root),
                    &uid(),
                    "bank-capture",
                    Default::default(),
                    |_| Ok(()),
                )
                .unwrap();
                assert!(!registry_path(root).exists());
            }
        },
    );
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_nonempty_short_owner_write_remains_interruption_not_initialization
#[test]
fn actual_short_owner_publication_retains_only_written_prefix() {
    isolated(
        "actual_short_owner_publication_retains_only_written_prefix",
        |root| {
            let child = operation_child(
                root,
                Some("registry.exclusive.owner-write-short"),
                "error",
                "registry-once",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(registry_result(root).get("error").is_some());
            assert!(!root.join("registry-owner").exists());
            assert_eq!(std::fs::read(registry_path(root)).unwrap(), b"{");
            assert!(matches!(
                agent_memory::local_registry::check_canonical_writer(
                    &registry_path(root),
                    &["capture"]
                ),
                Err(agent_memory::local_registry::CanonicalWriterError::Contract(_))
            ));
        },
    );
}
// obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_owner_write_oserror_preserves_publication_evidence_and_closes_creator
#[test]
fn owner_write_error_keeps_empty_publication_and_releases_actual_descriptor() {
    isolated(
        "owner_write_error_keeps_empty_publication_and_releases_actual_descriptor",
        |root| {
            let child = operation_child(
                root,
                Some("registry.exclusive.owner-write-eio"),
                "error",
                "registry-once",
            );
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(registry_result(root).get("error").is_some());
            assert!(!root.join("registry-owner").exists());
            assert_eq!(std::fs::read(registry_path(root)).unwrap(), b"");
            assert!(matches!(
                agent_memory::local_registry::check_canonical_writer(
                    &registry_path(root),
                    &["capture"]
                ),
                Err(agent_memory::local_registry::CanonicalWriterError::Initializing)
            ));
            let path = registry_path(root);
            let directory =
                agent_memory::fs::Descriptor::open_directory(path.parent().unwrap()).unwrap();
            let descriptor = directory
                .open_at(path.file_name().unwrap(), libc::O_RDONLY, 0)
                .unwrap();
            agent_memory::fs::FlockGuard::try_acquire(
                descriptor,
                agent_memory::fs::LockMode::Exclusive,
            )
            .unwrap();
        },
    );
}

fn fixture_mode(root: &Path, mode: &str) -> Fixture {
    let (runtime, receipt, proof) = install_only(root, mode);
    let repo = bank_fixture(root);
    boundary::activate_marker(&repo, false, &runtime).unwrap();
    Fixture {
        repo,
        runtime,
        receipt,
        proof,
    }
}
fn first_segment(f: &Fixture) -> transport::Seg {
    transport::segments(&f.site(), SESSION).unwrap().remove(0)
}
fn replay_request(f: &Fixture, entries: &[Value], action: &str, result: &Value) -> Value {
    let mut value = request(f, entries, action);
    set(&mut value, "lifecycle", get(result, "lifecycle").clone());
    value
}
fn manual_snapshot(witness: &omp_installation::OwnedManual, call: &Value, result: &Value) -> Value {
    get(
        &agent_memory::omp_projection::project_owned_manual(
            result,
            call,
            witness,
            &witness.verify().unwrap().unwrap(),
        )
        .unwrap(),
        "snapshot",
    )
    .clone()
}
fn has_gap(directory: &Path, native: &str, reason: &str) -> bool {
    transport::files(directory, "gaps", transport::Names::Dash36)
        .unwrap()
        .iter()
        .any(|path| {
            let gap = transport::read(path, 4096).unwrap();
            text(&gap, "native_entry_id") == native && text(&gap, "reason") == reason
        })
}

// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_fresh_capture_preserves_surrounding_source_and_retries
#[test]
#[ignore = "recovery_frontier"]
fn fresh_owned_capture_preserves_surrounding_source_in_both_install_modes() {
    scenarios(
        "fresh_owned_capture_preserves_surrounding_source_in_both_install_modes",
        &["copy", "link"],
        |root, mode| {
            let f = fixture_mode(root, mode);
            let witness = omp_installation::owned_manual(&f.receipt, "SKILL.md")
                .unwrap()
                .unwrap();
            let (call, result) = pair(&witness);
            let entries = vec![human("human"), call, result, answer("answer")];
            let observed = f.observe(&request(&f, &entries, "seal"));
            let s = first_segment(&f);
            let path = native_path(&s.directory, "snapshots", "manual-result");
            let bytes = std::fs::read(&path).unwrap();
            let snapshot = transport::read(&path, transport::MAX_PAYLOAD).unwrap();
            assert_eq!(get(&snapshot, "content"), &Value::Array(vec![]));
            assert_eq!(
                text(get(&snapshot, "instruction_context"), "tool_call_id"),
                "manual-call"
            );
            assert!(agent_memory::omp_projection::valid_instruction_context(
                &snapshot
            ));
            for native in ["human", "manual-invocation", "answer"] {
                assert!(native_path(&s.directory, "captures", native).is_file());
            }
            assert!(!has_gap(
                &s.directory,
                "tool:manual-call",
                "missing-stored-source"
            ));
            f.observe(&replay_request(&f, &entries, "ingest", &observed));
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_nonread_result_never_mints_manual_witness
#[test]
#[ignore = "recovery_frontier"]
fn nonread_tool_output_remains_ordinary_learning_source() {
    isolated(
        "nonread_tool_output_remains_ordinary_learning_source",
        |root| {
            let f = Fixture::new(root);
            let call = entry(
                "bash-call",
                "assistant",
                decode(
                    r#"[{"type":"toolCall","id":"bash","name":"bash","arguments":{"command":"printf 12"}}]"#,
                ),
            );
            let mut output = tool_result("bash-result", "bash", "12");
            set(
                output.get_mut("message").unwrap(),
                "toolName",
                string("bash"),
            );
            f.observe(&request(
                &f,
                &[human("human"), call, output.clone()],
                "seal",
            ));
            let snapshot = transport::read(
                &native_path(&first_segment(&f).directory, "snapshots", "bash-result"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert_eq!(text(&snapshot, "toolName"), "bash");
            assert!(snapshot.get("instruction_context").is_none());
            assert_eq!(
                get(&snapshot, "content"),
                get(get(&output, "message"), "content")
            );
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_screened_invocation_does_not_hide_following_answer
#[test]
#[ignore = "recovery_frontier"]
fn screened_read_invocation_cannot_hide_following_answer() {
    isolated(
        "screened_read_invocation_cannot_hide_following_answer",
        |root| {
            let f = Fixture::new(root);
            let witness = omp_installation::owned_manual(&f.receipt, "SKILL.md")
                .unwrap()
                .unwrap();
            let (mut call, result) = pair(&witness);
            let Value::Array(content) =
                call.get_mut("message").unwrap().get_mut("content").unwrap()
            else {
                unreachable!()
            };
            set(
                content[0].get_mut("arguments").unwrap(),
                "i",
                string("password=secretvalue"),
            );
            f.observe(&request(
                &f,
                &[human("human"), call, result, answer("answer")],
                "seal",
            ));
            let s = first_segment(&f);
            assert!(
                transport::files(&s.directory, "gaps", transport::Names::Dash36)
                    .unwrap()
                    .iter()
                    .any(
                        |p| text(&transport::read(p, 4096).unwrap(), "native_entry_id")
                            == "manual-invocation"
                    )
            );
            let path = native_path(&s.directory, "snapshots", "manual-result");
            if path.exists() {
                assert!(
                    transport::read(&path, transport::MAX_PAYLOAD)
                        .unwrap()
                        .get("instruction_context")
                        .is_none()
                );
            }
            assert!(native_path(&s.directory, "snapshots", "answer").is_file());
            assert!(!f.pending().is_empty());
        },
    );
}
fn orphan_manual_snapshot(root: &Path, mismatched: bool) {
    let f = Fixture::new(root);
    let witness = omp_installation::owned_manual(&f.receipt, "SKILL.md")
        .unwrap()
        .unwrap();
    let (call, result) = pair(&witness);
    f.observe(&request(&f, &[human("human"), call.clone()], "ingest"));
    let s = first_segment(&f);
    let mut snapshot = manual_snapshot(&witness, &call, &result);
    if mismatched {
        set(&mut snapshot, "native_entry_id", string("another-entry"));
    }
    assert!(agent_memory::omp_projection::valid_instruction_context(
        &snapshot
    ));
    let path = native_path(&s.directory, "snapshots", "manual-result");
    transport::retain(&path, &snapshot, transport::MAX_PAYLOAD).unwrap();
    let before = std::fs::read(&path).unwrap();
    let entries = [human("human"), call, result];
    if mismatched {
        assert!(
            observed(
                &f,
                &request(&f, &entries, "ingest"),
                transport::Observation {
                    receipt: Some(&f.receipt),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(!native_path(&s.directory, "captures", "manual-result").exists());
    } else {
        std::fs::remove_file(
            Path::new(text(get(&f.receipt, "omp_binding"), "skill_target")).join("SKILL.md"),
        )
        .unwrap();
        for _ in 0..2 {
            f.observe(&request(&f, &entries, "ingest"));
        }
        let capture = transport::read(
            &native_path(&s.directory, "captures", "manual-result"),
            4096,
        )
        .unwrap();
        assert_eq!(
            text(&capture, "snapshot_sha256"),
            transport::digest(&snapshot).unwrap()
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(
        transport::files(&s.directory, "gaps", transport::Names::Dash36)
            .unwrap()
            .is_empty()
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_snapshot_publication_recovery_survives_uninstall_and_retry
#[test]
#[ignore = "recovery_frontier"]
fn orphan_owned_snapshot_recovers_after_manual_removal_without_reclassification() {
    isolated(
        "orphan_owned_snapshot_recovers_after_manual_removal_without_reclassification",
        |root| orphan_manual_snapshot(root, false),
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_mismatched_orphan_snapshot_is_integrity_failure
#[test]
#[ignore = "recovery_frontier"]
fn mismatched_orphan_owned_snapshot_refuses_without_overwrite() {
    isolated(
        "mismatched_orphan_owned_snapshot_refuses_without_overwrite",
        |root| orphan_manual_snapshot(root, true),
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_preexisting_native_gap_is_not_reclassified
#[test]
#[ignore = "recovery_frontier"]
fn preexisting_unsafe_native_gap_cannot_become_owned_instruction() {
    isolated(
        "preexisting_unsafe_native_gap_cannot_become_owned_instruction",
        |root| {
            let f = Fixture::new(root);
            let witness = omp_installation::owned_manual(&f.receipt, "SKILL.md")
                .unwrap()
                .unwrap();
            let (call, result) = pair(&witness);
            let mut unavailable = result.clone();
            set(
                unavailable.get_mut("message").unwrap(),
                "content",
                decode(r#"[{"type":"image","data":"opaque"}]"#),
            );
            f.observe(&request(
                &f,
                &[human("human"), call.clone(), unavailable],
                "ingest",
            ));
            let s = first_segment(&f);
            let before = snapshot(&s.directory.join("gaps"));
            assert!(!before.is_empty());
            f.observe(&request(&f, &[human("human"), call, result], "ingest"));
            assert_eq!(snapshot(&s.directory.join("gaps")), before);
            let path = native_path(&s.directory, "snapshots", "manual-result");
            if path.exists() {
                assert!(
                    transport::read(&path, transport::MAX_PAYLOAD)
                        .unwrap()
                        .get("instruction_context")
                        .is_none()
                );
            }
            assert!(!f.pending().is_empty());
        },
    );
}
fn sealed_manual_gap(root: &Path, same_sweep: bool) {
    let f = Fixture::new(root);
    let witness = omp_installation::owned_manual(&f.receipt, "SKILL.md")
        .unwrap()
        .unwrap();
    let (call, result) = pair(&witness);
    let entries = if same_sweep {
        vec![
            human("human"),
            call.clone(),
            human("second-human"),
            result.clone(),
        ]
    } else {
        vec![human("human"), call.clone()]
    };
    let observed = f.observe(&request(
        &f,
        &entries,
        if same_sweep { "ingest" } else { "seal" },
    ));
    let original = transport::segments(&f.site(), SESSION)
        .unwrap()
        .into_iter()
        .find(|s| {
            s.directory.join("segment.json").is_file()
                && transport::read(&s.directory.join("segment.json"), transport::MAX_PAYLOAD)
                    .unwrap()
                    .get("gaps")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|g| text(g, "native_entry_id") == "tool:manual-call")
        })
        .unwrap();
    let manifest = std::fs::read(original.directory.join("segment.json")).unwrap();
    let gaps = snapshot(&original.directory.join("gaps"));
    assert!(!gaps.is_empty());
    let later = if same_sweep {
        entries
    } else {
        vec![human("human"), call, result]
    };
    f.observe(&replay_request(&f, &later, "ingest", &observed));
    assert_eq!(
        std::fs::read(original.directory.join("segment.json")).unwrap(),
        manifest
    );
    assert_eq!(snapshot(&original.directory.join("gaps")), gaps);
    assert!(snapshot(&original.directory.join("resolutions")).is_empty());
    for s in transport::segments(&f.site(), SESSION).unwrap() {
        let path = native_path(&s.directory, "snapshots", "manual-result");
        if path.exists() {
            assert!(
                transport::read(&path, transport::MAX_PAYLOAD)
                    .unwrap()
                    .get("instruction_context")
                    .is_none()
            );
        }
    }
    assert!(!f.pending().is_empty());
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_sealed_tool_gap_does_not_authorize_late_classification
#[test]
#[ignore = "recovery_frontier"]
fn sealed_tool_gap_never_lends_late_owned_instruction_authority() {
    isolated(
        "sealed_tool_gap_never_lends_late_owned_instruction_authority",
        |root| sealed_manual_gap(root, false),
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_same_sweep_human_boundary_preserves_sealed_tool_gap
#[test]
#[ignore = "recovery_frontier"]
fn same_sweep_human_boundary_preserves_original_tool_gap() {
    isolated(
        "same_sweep_human_boundary_preserves_original_tool_gap",
        |root| sealed_manual_gap(root, true),
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_protected_private_root_cannot_silently_move_or_lose_binding
#[test]
#[ignore = "recovery_frontier"]
fn protected_private_root_cannot_lose_native_binding_silently() {
    isolated(
        "protected_private_root_cannot_lose_native_binding_silently",
        |root| {
            let f = Fixture::new(root);
            f.observe(&request(&f, &[human("human")], "seal"));
            let base = boundary::base(&f.repo).unwrap();
            assert!(
                boundary::source_capability_repositories(None)
                    .unwrap()
                    .contains(&f.repo)
            );
            std::fs::remove_file(base.join("omp-bank-binding-v1.json")).unwrap();
            assert!(boundary::required_source_capabilities(&f.repo, &f.runtime).is_err());
            assert!(boundary::source_capability_repositories(None).is_err());
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::NativeSources.test_root_wake_is_once_and_later_human_carries_unassessed_old_source
#[test]
#[ignore = "recovery_frontier"]
fn native_root_wake_is_once_and_next_human_includes_old_unassessed_sources() {
    isolated(
        "native_root_wake_is_once_and_next_human_includes_old_unassessed_sources",
        |root| {
            let f = Fixture::new(root);
            let first = f.observe(&request(&f, &[human("h1")], "seal"));
            let (owner, _writer, nonce) = real_window();
            assert_eq!(
                get(
                    &with_window(&f, &wake_request(&first), &owner, &nonce),
                    "continue"
                ),
                &Value::Bool(true)
            );
            owner.end();
            let (owner, _writer, nonce) = real_window();
            assert_eq!(
                get(
                    &with_window(&f, &wake_request(&first), &owner, &nonce),
                    "continue"
                ),
                &Value::Bool(false)
            );
            owner.end();
            let second = f.observe(&replay_request(
                &f,
                &[human("h1"), human("h2")],
                "seal",
                &first,
            ));
            let (owner, _writer, nonce) = real_window();
            let wake = with_window(&f, &wake_request(&second), &owner, &nonce);
            assert_eq!(get(&wake, "continue"), &Value::Bool(true));
            let guidance = text(&wake, "additionalContext");
            assert!(guidance.len() <= 8192);
            for id in get(get(&first, "wake_payload"), "observation_ids")
                .as_array()
                .unwrap()
            {
                assert!(guidance.contains(id.as_str().unwrap()));
            }
            assert_eq!(root_wakes(&f).len(), 2);
            owner.end();
        },
    );
}

fn scenarios(name: &str, choices: &[&str], body: impl Fn(&Path, &str)) {
    if let Ok(root) = support::require_current_world() {
        agent_memory::cli::register_providers().unwrap();
        let choice = std::env::var("AGENT_MEMORY_FAULT_CASE").unwrap();
        body(&root, &choice);
        return;
    }
    for choice in choices {
        let mut world = World::new().unwrap();
        world
            .env
            .insert("AGENT_MEMORY_FAULT_CASE".into(), (*choice).into());
        let output = world
            .run_with(
                [
                    std::env::current_exe().unwrap().into_os_string(),
                    "--exact".into(),
                    name.into(),
                    "--include-ignored".into(),
                    "--nocapture".into(),
                ],
                RunOptions {
                    cwd: Some(&world.root),
                    timeout: Duration::from_secs(120),
                    check: false,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{choice}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
fn copy_tree(root: &Path, from: &Path, to: &Path) {
    command(
        root,
        &[
            "cp".into(),
            "-R".into(),
            from.as_os_str().to_owned(),
            to.as_os_str().to_owned(),
        ],
    );
}
fn git_config(f: &Fixture, key: &str, value: &str) {
    command(
        &f.repo,
        &[
            "git".into(),
            "config".into(),
            "--local".into(),
            key.into(),
            value.into(),
        ],
    );
}
fn enrollment_operation(root: &Path, f: &Fixture, value: &Value, operation: &str) {
    let result = if operation == "enrollment-mint" {
        agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).map(
            |context| {
                object([
                    ("root", string(context.root().to_str().unwrap())),
                    ("bank", string(context.bank().to_str().unwrap())),
                    ("scope", context.scope()),
                    ("held", Value::Bool(context.held())),
                ])
            },
        )
    } else {
        let run = |context: Option<&agent_memory::scope_binding::PrivateEnrollmentContext>| {
            turn_sources::register(
                &f.repo,
                get(value, "harness").as_str().unwrap_or("omp"),
                text(value, "session"),
                get(value, "lifecycle").as_str().unwrap_or(LIFE),
                text(value, "digest"),
                get(value, "native").as_str(),
                get(value, "reference").as_str(),
                context,
                &f.runtime,
            )
        };
        if operation == "enrollment-register-reuse" {
            agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).and_then(
                |context| {
                    let barrier =
                        turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime)?;
                    let result = agent_memory::scope_binding::private_enrollment_reads(
                        &context,
                        |context| run(Some(context)),
                    );
                    barrier.recheck()?;
                    result
                },
            )
        } else {
            run(None)
        }
    };
    write(
        &root.join("operation-result.json"),
        turn_sources::encoded(&match result {
            Ok(value) => value,
            Err(error) => error_value(&error),
        })
        .unwrap(),
        0o600,
    );
}
fn register_request(session: &str, native: &str, digest: &str) -> Value {
    object([
        ("session", string(session)),
        ("native", string(native)),
        ("digest", string(digest)),
    ])
}
fn start_enrollment(root: &Path, value: &Value, stage: &str, operation: &str) -> OperationChild {
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(value).unwrap(),
        0o600,
    );
    let child = operation_child(root, Some(stage), "pause", operation);
    ready(root, stage);
    child
}
fn finish_enrollment(root: &Path, child: OperationChild, stage: &str) -> Value {
    release(root, stage);
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    registry_result(root)
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_bounded_later_sweeps_enroll_oldest_and_keep_every_new_arrival
#[test]
#[ignore = "recovery_frontier"]
fn bounded_sweeps_enroll_oldest_without_losing_new_arrivals() {
    isolated(
        "bounded_sweeps_enroll_oldest_without_losing_new_arrivals",
        |root| {
            let f = Fixture::new(root);
            let mut entries = (0..5)
                .map(|n| human(&format!("human-{n}")))
                .collect::<Vec<_>>();
            let first = f.observe(&request(&f, &entries, "seal"));
            let initial = f.pending();
            assert_eq!(initial.len(), 10);
            assert_eq!(
                initial
                    .iter()
                    .filter(|r| r.get("observation_id").is_some())
                    .count(),
                8
            );
            let waiting = initial
                .iter()
                .filter(|r| r.get("observation_id").is_none())
                .map(|r| text(r, "source_reference").to_owned())
                .collect::<Vec<_>>();
            entries.extend((5..9).map(|n| human(&format!("human-{n}"))));
            let second = f.observe(&replay_request(&f, &entries, "seal", &first));
            let arrived = f.pending();
            assert_eq!(arrived.len(), 18);
            assert_eq!(
                arrived
                    .iter()
                    .filter(|r| r.get("observation_id").is_some())
                    .count(),
                16
            );
            for old in &initial {
                let new = arrived
                    .iter()
                    .find(|r| get(r, "source_reference") == get(old, "source_reference"))
                    .unwrap();
                assert_eq!(get(new, "source_sha256"), get(old, "source_sha256"));
                if old.get("observation_id").is_some() {
                    assert_eq!(get(new, "observation_id"), get(old, "observation_id"));
                }
            }
            for reference in waiting {
                assert!(
                    arrived
                        .iter()
                        .any(|r| text(r, "source_reference") == reference
                            && r.get("observation_id").is_some())
                );
            }
            f.observe(&replay_request(&f, &entries, "seal", &second));
            let finished = f.pending();
            assert_eq!(finished.len(), arrived.len());
            assert!(finished.iter().all(|r| r.get("observation_id").is_some()));
            for old in arrived {
                let row = finished
                    .iter()
                    .find(|r| get(r, "source_reference") == get(&old, "source_reference"))
                    .unwrap();
                assert_eq!(get(row, "source_sha256"), get(&old, "source_sha256"));
            }
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_lost_success_reuses_observation_and_unknown_attempt_is_not_retried
#[test]
#[ignore = "recovery_frontier"]
fn lost_registration_response_recovers_only_actual_original_shared_source() {
    scenarios(
        "lost_registration_response_recovers_only_actual_original_shared_source",
        &["unknown", "successful"],
        |root, case| {
            let f = Fixture::new(root);
            let s = segment(&f, false, false);
            let enrollment = s
                .enrollments
                .iter()
                .find(|e| text(e, "kind") == "admission")
                .unwrap();
            let path = s.directory.join("registration-attempts").join(format!(
                "{}.json",
                transport::digest(get(enrollment, "source_reference")).unwrap()
            ));
            transport::retain(
                &path,
                &object([
                    ("schema", string("agent-memory/omp-registration-attempt-v1")),
                    ("session", get(&s.cursor, "session").clone()),
                    ("lifecycle", get(&s.cursor, "lifecycle").clone()),
                    (
                        "source_reference",
                        get(enrollment, "source_reference").clone(),
                    ),
                    ("source_sha256", get(enrollment, "source_sha256").clone()),
                ]),
                4096,
            )
            .unwrap();
            let before = std::fs::read(&path).unwrap();
            let original = if case == "successful" {
                Some(
                    turn_sources::register(
                        &f.repo,
                        "omp",
                        SESSION,
                        text(&s.cursor, "lifecycle"),
                        text(enrollment, "source_sha256"),
                        Some(text(enrollment, "native_source_id")),
                        Some(text(enrollment, "source_reference")),
                        None,
                        &f.runtime,
                    )
                    .unwrap(),
                )
            } else {
                None
            };
            f.observe(&request(&f, &[], "ingest"));
            let rows = f.pending();
            let row = rows
                .iter()
                .find(|r| get(r, "source_reference") == get(enrollment, "source_reference"))
                .unwrap();
            if let Some(original) = &original {
                assert_eq!(get(row, "observation_id"), get(original, "observation_id"));
            } else {
                assert!(row.get("observation_id").is_none());
                assert_eq!(
                    get(row, "faults"),
                    &decode(r#"["registration-fault-retained"]"#)
                );
            }
            assert_eq!(std::fs::read(path).unwrap(), before);
            assert_eq!(get(row, "source_sha256"), get(enrollment, "source_sha256"));
            let state = turn_sources::inspect(
                &f.repo,
                "omp",
                SESSION,
                text(&s.cursor, "lifecycle"),
                &f.runtime,
            )
            .unwrap();
            assert_eq!(
                get(&state, "sources")
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| get(r, "source_reference") == get(enrollment, "source_reference"))
                    .count(),
                usize::from(original.is_some())
            );
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_shared_fault_retained_across_later_observer_progress
#[test]
#[ignore = "recovery_frontier"]
fn actual_shared_source_capacity_fault_remains_retained_across_observer_progress() {
    isolated(
        "actual_shared_source_capacity_fault_remains_retained_across_observer_progress",
        |root| {
            let f = Fixture::new(root);
            let lifecycle =
                transport::uuid5_url(&format!("{}:omp:{SESSION}:human", f.repo.display()));
            for n in 0..turn_sources::MAX_SOURCES {
                turn_sources::register(
                    &f.repo,
                    "omp",
                    SESSION,
                    &lifecycle,
                    &format!("{n:064x}"),
                    Some(&format!("existing-{n}")),
                    Some(&format!("omp:existing-{n}")),
                    None,
                    &f.runtime,
                )
                .unwrap();
            }
            let value = request(&f, &[human("human")], "seal");
            let first = f.observe(&value);
            let s = first_segment(&f);
            let ns = shared(&f, &s.snapshot().unwrap());
            let fault = std::fs::read(ns.directory.join("fault.json")).unwrap();
            let attempts = snapshot(&s.directory.join("registration-attempts"));
            let before = transport::pending_inventory(&f.site()).unwrap();
            assert!(before.1.iter().any(|v| v == "registration-fault-retained"));
            f.observe(&replay_request(&f, &[human("human")], "seal", &first));
            let after = transport::pending_inventory(&f.site()).unwrap();
            assert_eq!(
                turn_sources::encoded(&Value::Array(before.0)).unwrap(),
                turn_sources::encoded(&Value::Array(after.0)).unwrap()
            );
            assert_eq!(
                std::fs::read(ns.directory.join("fault.json")).unwrap(),
                fault
            );
            assert_eq!(
                snapshot(&s.directory.join("registration-attempts")),
                attempts
            );
            assert_eq!(
                get(
                    &turn_sources::inspect(&f.repo, "omp", SESSION, &lifecycle, &f.runtime)
                        .unwrap(),
                    "sources"
                )
                .as_array()
                .unwrap()
                .len(),
                turn_sources::MAX_SOURCES
            );
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_resolves_regular_and_newline_worktree_identity
#[test]
#[ignore = "recovery_frontier"]
fn fresh_context_resolves_regular_and_newline_linked_worktree_identity() {
    isolated(
        "fresh_context_resolves_regular_and_newline_linked_worktree_identity",
        |root| {
            let f = Fixture::new(root);
            let ordinary =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            let worktree = root.join("linked worktree with space\n");
            command(
                &f.repo,
                &[
                    "git".into(),
                    "worktree".into(),
                    "add".into(),
                    "--detach".into(),
                    worktree.as_os_str().to_owned(),
                    "HEAD".into(),
                ],
            );
            copy_tree(
                root,
                &f.repo.join("memory-bank"),
                &worktree.join("memory-bank"),
            );
            let linked =
                agent_memory::scope_binding::private_enrollment_context(&worktree, &f.runtime)
                    .unwrap();
            assert_eq!(linked.root(), &worktree);
            assert_eq!(linked.project_id(), ordinary.project_id());
            assert_eq!(
                get(&linked.scope(), "checkout_key"),
                get(&ordinary.scope(), "checkout_key")
            );
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_foreign_bank_identity_without_private_writes
#[test]
#[ignore = "recovery_frontier"]
fn fresh_context_refuses_physical_foreign_bank_identity_without_private_writes() {
    isolated(
        "fresh_context_refuses_physical_foreign_bank_identity_without_private_writes",
        |root| {
            let f = Fixture::new(root);
            let bank = f.repo.join("memory-bank");
            let foreign = f.repo.join("foreign-bank");
            copy_tree(root, &bank, &foreign);
            let router = foreign.join("MEMORY.md");
            let original = std::fs::read_to_string(&router).unwrap();
            let project =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap()
                    .project_id()
                    .as_str()
                    .unwrap()
                    .to_owned();
            write(&router, original.replace(&project, &uid()), 0o644);
            std::fs::rename(&bank, root.join("retained-original-identity-bank")).unwrap();
            git_config(&f, "agent-memory.bankRoot", "foreign-bank");
            let before = snapshot(&root.join("state"));
            assert!(
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .is_err()
            );
            assert_eq!(snapshot(&root.join("state")), before);
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_bank_drift_after_actual_scope_read
#[test]
#[ignore = "recovery_frontier"]
fn fresh_context_refuses_competing_bank_added_after_actual_scope_read() {
    isolated(
        "fresh_context_refuses_competing_bank_added_after_actual_scope_read",
        |root| {
            let f = Fixture::new(root);
            let stage = "enrollment.scope.after-require";
            let child = start_enrollment(root, &object([]), stage, "enrollment-mint");
            copy_tree(root, &f.repo.join("memory-bank"), &f.repo.join("late-bank"));
            git_config(&f, "agent-memory.bankRoot", "late-bank");
            let before = snapshot(&root.join("state"));
            let result = finish_enrollment(root, child, stage);
            assert!(text(&result, "error").contains("Competing memory bank locations"));
            assert_eq!(snapshot(&root.join("state")), before);
        },
    );
}
fn raw_locator_refusal(root: &Path, held: bool, kind: &str, late: bool) {
    let h = held.then(|| held_fixture(root));
    let ordinary = (!held).then(|| Fixture::new(root));
    let f = h
        .as_ref()
        .map(|h| &h.fixture)
        .or(ordinary.as_ref())
        .unwrap();
    assert_eq!(
        agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
            .unwrap()
            .held(),
        held
    );
    let stage = "enrollment.scope.after-require";
    let child = late.then(|| start_enrollment(root, &object([]), stage, "enrollment-mint"));
    let stray = f.repo.join("raw-only/MEMORY.md");
    if kind == "symlink" {
        let target = f.repo.join("raw-target.txt");
        write(&target, "# Raw nonrouter\n", 0o644);
        std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
        symlink(target, &stray).unwrap();
        assert!(agent_memory::bank_writes::bank_path(&f.repo).is_err());
    } else {
        write(
            &stray,
            "# Raw locator fixture\n\nDeliberately not a memory router.\n",
            0o644,
        );
        if kind == "committed" {
            command(
                &f.repo,
                &[
                    "git".into(),
                    "add".into(),
                    "--".into(),
                    "raw-only/MEMORY.md".into(),
                ],
            );
            command(
                &f.repo,
                &[
                    "git".into(),
                    "commit".into(),
                    "-m".into(),
                    "Retain raw-only locator".into(),
                    "--".into(),
                    "raw-only/MEMORY.md".into(),
                ],
            );
            std::fs::remove_file(&stray).unwrap();
        }
        assert_eq!(
            agent_memory::bank_writes::bank_path(&f.repo).unwrap().1,
            f.repo.join("memory-bank")
        );
    }
    let before = (
        snapshot(&root.join("state")),
        snapshot(&f.repo.join("memory-bank")),
        std::fs::read(f.repo.join(".git/index")).unwrap(),
    );
    if let Some(child) = child {
        assert!(finish_enrollment(root, child, stage).get("error").is_some());
    } else {
        assert!(
            agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime).is_err()
        );
    }
    assert_eq!(
        (
            snapshot(&root.join("state")),
            snapshot(&f.repo.join("memory-bank")),
            std::fs::read(f.repo.join(".git/index")).unwrap()
        ),
        before
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_raw_nonrouter_refusal_with_and_without_verified_hold
#[test]
#[ignore = "recovery_frontier"]
fn raw_nonrouter_context_refusal_preserves_held_and_ordinary_banks() {
    scenarios(
        "raw_nonrouter_context_refusal_preserves_held_and_ordinary_banks",
        &["ordinary", "held"],
        |root, mode| raw_locator_refusal(root, mode == "held", "untracked", false),
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_committed_raw_locator_refusal_with_and_without_verified_hold
#[test]
#[ignore = "recovery_frontier"]
fn committed_missing_raw_locator_refusal_preserves_held_and_ordinary_banks() {
    scenarios(
        "committed_missing_raw_locator_refusal_preserves_held_and_ordinary_banks",
        &["ordinary", "held"],
        |root, mode| raw_locator_refusal(root, mode == "held", "committed", false),
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_symlink_locator_refusal_with_and_without_verified_hold
#[test]
#[ignore = "recovery_frontier"]
fn symlink_raw_locator_refusal_preserves_held_and_ordinary_banks() {
    scenarios(
        "symlink_raw_locator_refusal_preserves_held_and_ordinary_banks",
        &["ordinary", "held"],
        |root, mode| raw_locator_refusal(root, mode == "held", "symlink", false),
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_late_raw_locator_refusal_with_and_without_verified_hold
#[test]
#[ignore = "recovery_frontier"]
fn late_raw_locator_after_real_scope_read_refuses_held_and_ordinary_contexts() {
    scenarios(
        "late_raw_locator_after_real_scope_read_refuses_held_and_ordinary_contexts",
        &["ordinary", "held"],
        |root, mode| raw_locator_refusal(root, mode == "held", "untracked", true),
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_successful_but_disagreeing_bank_locators
#[test]
#[ignore = "recovery_frontier"]
fn successful_but_disagreeing_raw_and_ordinary_bank_locators_never_mint_context() {
    isolated(
        "successful_but_disagreeing_raw_and_ordinary_bank_locators_never_mint_context",
        |root| {
            let f = Fixture::new(root);
            write(&f.repo.join(".git/info/exclude"), "/memory-bank/\n", 0o644);
            let stray = f.repo.join("raw-only/MEMORY.md");
            write(
                &stray,
                "# Unrelated document\n\nNot a memory router.\n",
                0o644,
            );
            assert_eq!(
                agent_memory::bank_writes::bank_path(&f.repo).unwrap().1,
                f.repo.join("memory-bank")
            );
            assert_eq!(
                agent_memory::layout_recovery_state::paths(&f.repo, None, &f.runtime)
                    .unwrap()
                    .1,
                stray.parent().unwrap()
            );
            let before = (snapshot(&root.join("state")), snapshot(&f.repo));
            assert!(
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .is_err()
            );
            assert_eq!((snapshot(&root.join("state")), snapshot(&f.repo)), before);
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_reused_locator_keeps_post_source_mutation_scope_check_and_original_fault
#[test]
#[ignore = "recovery_frontier"]
fn reused_locator_rechecks_scope_after_actual_source_publication_and_retains_fault() {
    isolated(
        "reused_locator_rechecks_scope_after_actual_source_publication_and_retains_fault",
        |root| {
            let f = Fixture::new(root);
            let stage = "enrollment.source.after-exclusive";
            let child = start_enrollment(
                root,
                &register_request(SESSION, "actual-source", &"a".repeat(64)),
                stage,
                "enrollment-register-reuse",
            );
            git_config(
                &f,
                "agent-memory.authorityRemote",
                "missing-native-authority",
            );
            let result = finish_enrollment(root, child, stage);
            assert!(result.get("error").is_some());
            git_config(&f, "agent-memory.authorityRemote", "");
            let ns =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap()
                    .0;
            let source = transport::read(&ns.directory.join("source-0.json"), 4096).unwrap();
            assert_eq!(text(&source, "native_source_id"), "actual-source");
            assert_eq!(
                text(
                    &transport::read(&ns.directory.join("fault.json"), 4096).unwrap(),
                    "reason"
                ),
                "registration-unverified"
            );
            assert!(!ns.directory.join("source-1.json").exists());
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_concurrent_slot_creation_keeps_both_sources_and_next_hole
#[test]
#[ignore = "recovery_frontier"]
fn concurrent_real_pending_slot_creation_keeps_both_sources_and_next_hole() {
    isolated(
        "concurrent_real_pending_slot_creation_keeps_both_sources_and_next_hole",
        |root| {
            let f = Fixture::new(root);
            let first = register_source(&f, SESSION, "original-racing-source", &"a".repeat(64));
            let ns =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap()
                    .0;
            let pending = ns.base.join("pending");
            let last = pending.join("4095.json");
            std::fs::rename(pending.join("0.json"), &last).unwrap();
            let reference = transport::read(&last, 4096).unwrap();
            let stage = "enrollment.index.before-exclusive";
            let child = start_enrollment(
                root,
                &register_request("next-session", "new-racing-source", &"b".repeat(64)),
                stage,
                "enrollment-register",
            );
            assert!(turn_sources::exclusive(&pending.join("0.json"), &reference, 4096).unwrap());
            let second = finish_enrollment(root, child, stage);
            assert!(second.get("error").is_none());
            assert!(pending.join("1.json").is_file());
            assert_eq!(
                transport::read(&pending.join("0.json"), 4096).unwrap(),
                reference
            );
            assert_source_retained(&f, SESSION, &first);
            assert_source_retained(&f, "next-session", &second);
        },
    );
}

// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_retries_transient_scan_change_and_refuses_persistent_churn
#[test]
#[ignore = "recovery_frontier"]
fn pending_index_retries_real_transient_change_and_refuses_real_persistent_churn() {
    scenarios(
        "pending_index_retries_real_transient_change_and_refuses_real_persistent_churn",
        &["transient", "persistent"],
        |root, mode| {
            use std::sync::atomic::{AtomicBool, Ordering};
            let f = Fixture::new(root);
            let first = register_source(&f, SESSION, "original-scanned-source", &"a".repeat(64));
            let ns =
                turn_sources::private_namespace(&f.repo, "omp", SESSION, LIFE, false, &f.runtime)
                    .unwrap()
                    .0;
            let pending = ns.base.join("pending");
            let reference = std::fs::read(pending.join("0.json")).unwrap();
            for n in 2..4096 {
                write(&pending.join(format!("{n}.json")), &reference, 0o600);
            }
            let stage = "enrollment.index.during-scan";
            let child = start_enrollment(
                root,
                &register_request("new-session", "scanned-source", &"b".repeat(64)),
                stage,
                "enrollment-register",
            );
            write(
                &pending.join("late-unrelated"),
                b"Retain concurrent evidence\n",
                0o600,
            );
            let stop = Arc::new(AtomicBool::new(false));
            let mutator = if mode == "persistent" {
                let stop = stop.clone();
                let pending = pending.clone();
                Some(std::thread::spawn(move || {
                    let directory = std::fs::File::open(pending).unwrap();
                    let mut generation = 1u64;
                    while !stop.load(Ordering::SeqCst) {
                        directory
                            .set_times(std::fs::FileTimes::new().set_modified(
                                std::time::UNIX_EPOCH + Duration::from_secs(generation * 2),
                            ))
                            .unwrap();
                        generation += 1;
                    }
                }))
            } else {
                None
            };
            release(root, stage);
            let output = child.wait();
            stop.store(true, Ordering::SeqCst);
            if let Some(mutator) = mutator {
                mutator.join().unwrap();
            }
            assert_eq!(output.status, agent_memory::process::Status::Code(0));
            let result = registry_result(root);
            let new_ns = turn_sources::private_namespace(
                &f.repo,
                "omp",
                "new-session",
                LIFE,
                false,
                &f.runtime,
            )
            .unwrap()
            .0;
            if mode == "persistent" {
                assert!(result.get("error").is_some());
                assert!(!new_ns.directory.join("source-0.json").exists());
                assert!(!pending.join("1.json").exists());
            } else {
                assert!(result.get("error").is_none());
                assert!(pending.join("1.json").is_file());
                assert!(!new_ns.directory.join("fault.json").exists());
                assert_source_retained(&f, "new-session", &result);
            }
            assert_source_retained(&f, SESSION, &first);
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_held_read_reuse_keeps_exact_intent_pins_and_refuses_changed_hold_without_new_source
#[test]
#[ignore = "recovery_frontier"]
fn held_read_reuse_keeps_exact_intent_pins_and_refuses_new_source_after_hold_change() {
    isolated(
        "held_read_reuse_keeps_exact_intent_pins_and_refuses_new_source_after_hold_change",
        |root| {
            let h = held_fixture(root);
            let f = &h.fixture;
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            let marker = f.repo.join("memory-bank/.agent-memory-transaction.md");
            let original = std::fs::read(&marker).unwrap();
            let barrier =
                turn_sources::source_barrier(&f.repo, false, false, None, &f.runtime).unwrap();
            let mut observed = None;
            let mut namespace = None;
            let result =
                agent_memory::scope_binding::private_enrollment_reads(&context, |context| {
                    let value = turn_sources::register(
                        &f.repo,
                        "claude",
                        SESSION,
                        LIFE,
                        &"a".repeat(64),
                        Some("actual-held-native"),
                        Some("controlled-held-source"),
                        Some(context),
                        &f.runtime,
                    )?;
                    let ns = turn_sources::private_namespace(
                        &f.repo, "claude", SESSION, LIFE, false, &f.runtime,
                    )?
                    .0;
                    let intent = turn_sources::enrollment_inventory(&ns.directory, &ns.identity)?;
                    assert_eq!(intent.len(), 1);
                    assert_eq!(text(&intent[0], "context_digest"), context.context_digest());
                    assert_eq!(
                        get(&intent[0], "observation_id"),
                        get(&value, "observation_id")
                    );
                    assert_eq!(text(&intent[0], "state"), "registered");
                    let mut changed = json::decode_bytes(&original, Default::default()).unwrap();
                    set(&mut changed, "hold_id", string(&uid()));
                    write(&marker, turn_sources::encoded(&changed).unwrap(), 0o600);
                    observed = Some(value);
                    namespace = Some(ns);
                    turn_sources::register(
                        &f.repo,
                        "claude",
                        SESSION,
                        LIFE,
                        &"b".repeat(64),
                        Some("different-held-native"),
                        Some("different-controlled-source"),
                        Some(context),
                        &f.runtime,
                    )?;
                    Ok(())
                });
            write(&marker, original, 0o600);
            assert!(result.is_err());
            barrier.recheck().unwrap();
            drop(barrier);
            let ns = namespace.unwrap();
            let observed = observed.unwrap();
            let state =
                turn_sources::inspect(&f.repo, "claude", SESSION, LIFE, &f.runtime).unwrap();
            assert_eq!(get(&state, "sources").as_array().unwrap().len(), 1);
            assert_eq!(
                get(
                    &get(&state, "sources").as_array().unwrap()[0],
                    "observation_id"
                ),
                get(&observed, "observation_id")
            );
            assert_eq!(
                turn_sources::enrollment_inventory(&ns.directory, &ns.identity)
                    .unwrap()
                    .len(),
                1
            );
        },
    );
}
// frontier obligation: tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_observe_bank_drift_keeps_original_captures_refuses_final_and_next_frame_freshly_locates
#[test]
#[ignore = "recovery_frontier"]
fn observer_bank_drift_retains_both_original_captures_and_next_frame_freshly_locates() {
    isolated(
        "observer_bank_drift_retains_both_original_captures_and_next_frame_freshly_locates",
        |root| {
            let f = Fixture::new(root);
            let base = boundary::base(&f.repo).unwrap();
            let alternate = f.repo.join("alternate-bank");
            let alternate_base =
                agent_memory::bank_writes::state_paths(&f.repo, &alternate, &uid())
                    .unwrap()
                    .0
                    .parent()
                    .unwrap()
                    .to_path_buf();
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request(
                    &f,
                    &[human("first-human"), human("second-human")],
                    "seal",
                ))
                .unwrap(),
                0o600,
            );
            let stage = "omp.capture.after-commit";
            let child = operation_child(root, Some(stage), "pause", "observe");
            ready(root, stage);
            copy_tree(root, &f.repo.join("memory-bank"), &alternate);
            git_config(&f, "agent-memory.bankRoot", "alternate-bank");
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(registry_result(root).get("error").is_some());
            let captures = snapshot(&base.join("omp-source-v1"))
                .into_iter()
                .filter_map(|(path, raw)| {
                    (path.parent().and_then(Path::file_name)
                        == Some(std::ffi::OsStr::new("captures")))
                    .then(|| {
                        text(
                            &json::decode_bytes(&raw, Default::default()).unwrap(),
                            "native_entry_id",
                        )
                        .to_owned()
                    })
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                captures,
                ["first-human".to_owned(), "second-human".to_owned()]
                    .into_iter()
                    .collect()
            );
            assert!(!alternate_base.join("omp-source-v1").exists());
            assert!(!alternate_base.join("turn-sources-v2").exists());
            assert!(
                agent_memory::scope_binding::observe_enrollment_location(&f.repo, None, &f.runtime)
                    .unwrap()
                    .is_none()
            );
            std::fs::rename(
                f.repo.join("memory-bank"),
                root.join("retained-original-bank"),
            )
            .unwrap();
            assert_eq!(boundary::base(&f.repo).unwrap(), alternate_base);
            let mut started = false;
            assert!(
                observed(
                    &f,
                    &request(&f, &[human("first-human"), human("second-human")], "seal"),
                    transport::Observation {
                        receipt: Some(&f.receipt),
                        on_started: Some(&mut || {
                            started = true;
                            Ok(())
                        }),
                        ..Default::default()
                    }
                )
                .is_err()
            );
            assert!(!started);
            assert!(!alternate_base.join("omp-source-v1").exists());
            assert!(!alternate_base.join("turn-sources-v2").exists());
        },
    );
}

fn bank_base(f: &Fixture, bank: &Path) -> PathBuf {
    agent_memory::bank_writes::state_paths(&f.repo, bank, &uid())
        .unwrap()
        .0
        .parent()
        .unwrap()
        .to_path_buf()
}
fn no_started_or_retirement(base: &Path, directory: &Path) {
    assert!(!directory.join("started/1.json").exists());
    assert!(
        snapshot(base)
            .keys()
            .all(|p| p.file_name() != Some(std::ffi::OsStr::new("retirement.json")))
    );
}
fn configured_bank_phase(root: &Path, action: &str, writer: bool) {
    let f = Fixture::new(root);
    let base_a = boundary::base(&f.repo).unwrap();
    let bank_a = f.repo.join("memory-bank");
    let bank_b = f.repo.join("alternate-bank");
    let base_b = bank_base(&f, &bank_b);
    let config = f.repo.join(".git/config");
    let original_config = std::fs::read(&config).unwrap();
    let value = request(&f, &[human("phase-human"), answer("phase-answer")], action);
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&value).unwrap(),
        0o600,
    );
    write(
        &root.join("case-followup.json"),
        turn_sources::encoded(&value).unwrap(),
        0o600,
    );
    let first = "omp.checked-view.after-validation";
    let refused = "omp.observer.after-error";
    let busy = "omp.observer.after-busy-error";
    let selector = if writer {
        format!("{first},{busy},{refused}")
    } else {
        format!("{first},{refused}")
    };
    let child = operation_child(root, Some(&selector), "pause", "finite");
    ready(root, first);
    std::fs::rename(&bank_a, &bank_b).unwrap();
    git_config(&f, "agent-memory.bankRoot", "alternate-bank");
    let before_b = snapshot(&base_b);
    let canonical = if writer {
        write(
            &root.join("registry-selected-path"),
            bank_b.join(".agent-memory.lock").to_str().unwrap(),
            0o600,
        );
        Some(start_registry_writer(root, None))
    } else {
        None
    };
    release(root, first);
    if let Some(canonical) = canonical {
        ready(root, busy);
        no_started_or_retirement(&base_a, &child_directory(root));
        assert_eq!(snapshot(&base_b), before_b);
        finish_registry_writer(root, canonical);
        release(root, busy);
    }
    ready(root, refused);
    let directory = child_directory(root);
    no_started_or_retirement(&base_a, &directory);
    assert_eq!(snapshot(&base_b), before_b);
    assert!(!base_b.join("source-barrier.lock").exists());
    assert!(!base_b.join("omp-transport.lock").exists());
    std::fs::rename(&bank_b, &bank_a).unwrap();
    write(&config, original_config, 0o644);
    release(root, refused);
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    let outcome =
        transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap();
    assert_eq!(text(&outcome, "outcome"), "durable-refusal");
    assert_eq!(text(&outcome, "code"), "observer-refused");
    let second = transport::read(&directory.join("outcomes/2.json"), stream::MAX_METADATA).unwrap();
    assert_eq!(text(&second, "outcome"), "verified");
    assert!(directory.join("closed.json").is_file());
    assert_eq!(snapshot(&base_b), before_b);
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state
#[test]
#[ignore = "recovery_frontier"]
fn configured_bank_ingest_drift_refuses_without_retirement_or_new_namespace() {
    isolated(
        "configured_bank_ingest_drift_refuses_without_retirement_or_new_namespace",
        |root| configured_bank_phase(root, "ingest", false),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_seal_refuses_without_new_private_state
#[test]
#[ignore = "recovery_frontier"]
fn configured_bank_seal_drift_refuses_without_new_namespace() {
    isolated(
        "configured_bank_seal_drift_refuses_without_new_namespace",
        |root| configured_bank_phase(root, "seal", false),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_ingest_canonical_writer_is_busy_before_binding_refusal
#[test]
#[ignore = "recovery_frontier"]
fn configured_bank_ingest_writer_busy_precedes_binding_refusal() {
    isolated(
        "configured_bank_ingest_writer_busy_precedes_binding_refusal",
        |root| configured_bank_phase(root, "ingest", true),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_seal_canonical_writer_is_busy_before_binding_refusal
#[test]
#[ignore = "recovery_frontier"]
fn configured_bank_seal_writer_busy_precedes_binding_refusal() {
    isolated(
        "configured_bank_seal_writer_busy_precedes_binding_refusal",
        |root| configured_bank_phase(root, "seal", true),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_metadata_refuses_without_attempt_or_new_base
#[test]
#[ignore = "recovery_frontier"]
fn configured_bank_metadata_drift_refuses_before_attempt_publication() {
    isolated(
        "configured_bank_metadata_drift_refuses_before_attempt_publication",
        |root| {
            let f = Fixture::new(root);
            let base_a = boundary::base(&f.repo).unwrap();
            let bank_b = f.repo.join("alternate-bank");
            let base_b = bank_base(&f, &bank_b);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request(&f, &[human("human")], "seal")).unwrap(),
                0o600,
            );
            let stage = "omp.request-identity.after-validation";
            let child = operation_child(root, Some(stage), "pause", "finite-run");
            ready(root, stage);
            std::fs::rename(f.repo.join("memory-bank"), &bank_b).unwrap();
            git_config(&f, "agent-memory.bankRoot", "alternate-bank");
            let before = snapshot(&base_b);
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(1));
            let directory = child_directory(root);
            no_started_or_retirement(&base_a, &directory);
            assert!(!directory.join("attempts/1.json").exists());
            assert!(!directory.join("closed.json").exists());
            assert_eq!(snapshot(&base_b), before);
        },
    );
}
fn physical_snapshot(
    directory: &Path,
) -> std::collections::BTreeMap<PathBuf, (u64, u64, u32, Vec<u8>)> {
    use std::os::unix::fs::MetadataExt;
    snapshot(directory)
        .into_iter()
        .map(|(relative, bytes)| {
            let metadata = std::fs::symlink_metadata(directory.join(&relative)).unwrap();
            (
                relative,
                (metadata.dev(), metadata.ino(), metadata.mode(), bytes),
            )
        })
        .collect()
}
fn activated_alias_retarget(root: &Path, action: &str, exclusive: bool, direct: bool) {
    let f = Fixture::new(root);
    let base_a = boundary::base(&f.repo).unwrap();
    let other = root.join("second-activated-root");
    copy_tree(root, &f.repo, &other);
    boundary::activate_marker(&other, false, &f.runtime).unwrap();
    let base_b = boundary::base(&other).unwrap();
    let f2 = Fixture {
        repo: other.clone(),
        runtime: f.runtime.clone(),
        receipt: f.receipt.clone(),
        proof: f.proof.clone(),
    };
    let retired = if direct {
        let (mut coverage, _, _) = finite(&f2);
        let path = coverage.directory().to_path_buf();
        coverage.close_live();
        Some(path)
    } else {
        None
    };
    let alias = root.join("selected-checkout");
    symlink(&f.repo, &alias).unwrap();
    write(
        &root.join("case-observer-repo"),
        alias.to_str().unwrap(),
        0o600,
    );
    let value = request(&f, &[human("phase-human"), answer("phase-answer")], action);
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&value).unwrap(),
        0o600,
    );
    if !direct {
        write(
            &root.join("case-followup.json"),
            turn_sources::encoded(&value).unwrap(),
            0o600,
        );
    }
    let stage = "omp.checked-view.after-validation";
    let refused = "omp.observer.after-error";
    let selector = if direct {
        stage.to_owned()
    } else {
        format!("{stage},{refused}")
    };
    let mut child = Some(operation_child(
        root,
        Some(&selector),
        "pause",
        if direct { "observe" } else { "finite" },
    ));
    ready(root, stage);
    let held = exclusive.then(|| turn_sources::source_barrier_at(&base_b, true, true).unwrap());
    let canary = base_b.join("omp-bank-binding-v1.json");
    let mode = std::fs::metadata(&canary).unwrap().permissions().mode() & 0o777;
    let before = physical_snapshot(&base_b);
    assert_ne!(agent_memory::host::effective_uid(), 0);
    std::fs::set_permissions(&canary, std::fs::Permissions::from_mode(0)).unwrap();
    std::fs::remove_file(&alias).unwrap();
    symlink(&other, &alias).unwrap();
    release(root, stage);
    if !direct {
        ready(root, refused);
        no_started_or_retirement(&base_a, &child_directory(root));
    }
    let output = direct.then(|| child.take().unwrap().wait());
    std::fs::set_permissions(&canary, std::fs::Permissions::from_mode(mode)).unwrap();
    assert_eq!(physical_snapshot(&base_b), before);
    assert!(!base_b.join("omp-transport.lock").exists());
    if let Some(held) = &held {
        held.recheck().unwrap();
        assert!(
            turn_sources::private_lock(&base_b.join("source-barrier.lock"), false, true).is_err()
        );
    }
    std::fs::remove_file(&alias).unwrap();
    symlink(&f.repo, &alias).unwrap();
    if direct {
        assert_eq!(
            output.unwrap().status,
            agent_memory::process::Status::Code(0)
        );
        assert!(registry_result(root).get("error").is_some());
        assert!(!root.join("observer-started").exists());
        assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
        assert!(transport::segments(&f2.site(), SESSION).unwrap().is_empty());
        let retired = retired.unwrap();
        assert!(!retired.join("retirement.json").exists());
        assert_eq!(
            stream::retire_for_recovery(&stream::Installed::new(&f.runtime), &other, SESSION, None)
                .unwrap()
                .len(),
            1
        );
        assert!(retired.join("retirement.json").is_file());
    } else {
        release(root, refused);
        assert_eq!(
            child.take().unwrap().wait().status,
            agent_memory::process::Status::Code(0)
        );
        let directory = child_directory(root);
        assert_eq!(
            text(
                &transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap(),
                "outcome"
            ),
            "durable-refusal"
        );
        assert_eq!(
            text(
                &transport::read(&directory.join("outcomes/2.json"), stream::MAX_METADATA).unwrap(),
                "outcome"
            ),
            "verified"
        );
        assert!(directory.join("closed.json").exists());
        assert_eq!(physical_snapshot(&base_b), before);
    }
    drop(held);
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_ingest_never_acquires_foreign_exclusive_barrier
#[test]
#[ignore = "recovery_frontier"]
fn retargeted_ingest_never_acquires_foreign_exclusive_barrier() {
    isolated(
        "retargeted_ingest_never_acquires_foreign_exclusive_barrier",
        |root| activated_alias_retarget(root, "ingest", true, false),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_seal_never_acquires_foreign_exclusive_barrier
#[test]
#[ignore = "recovery_frontier"]
fn retargeted_seal_never_acquires_foreign_exclusive_barrier() {
    isolated(
        "retargeted_seal_never_acquires_foreign_exclusive_barrier",
        |root| activated_alias_retarget(root, "seal", true, false),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_ingest_refuses_before_unreadable_native_binding
#[test]
#[ignore = "recovery_frontier"]
fn retargeted_ingest_refuses_before_unreadable_foreign_binding() {
    isolated(
        "retargeted_ingest_refuses_before_unreadable_foreign_binding",
        |root| activated_alias_retarget(root, "ingest", false, false),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_seal_refuses_before_unreadable_native_binding
#[test]
#[ignore = "recovery_frontier"]
fn retargeted_seal_refuses_before_unreadable_foreign_binding() {
    isolated(
        "retargeted_seal_refuses_before_unreadable_foreign_binding",
        |root| activated_alias_retarget(root, "seal", false, false),
    );
}
// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checkout_retarget_after_view_validation_refuses_before_started_or_source
#[test]
#[ignore = "recovery_frontier"]
fn checkout_alias_retarget_after_validation_refuses_before_started_or_retirement() {
    isolated(
        "checkout_alias_retarget_after_validation_refuses_before_started_or_retirement",
        |root| activated_alias_retarget(root, "seal", false, true),
    );
}

fn prefixed_pair(
    witness: &omp_installation::OwnedManual,
    prefix: &str,
    parent: &str,
) -> (Value, Value) {
    let (mut call, mut result) = pair(witness);
    let call_id = format!("{prefix}-call");
    let invocation_id = format!("{prefix}-invocation");
    set(&mut call, "id", string(&invocation_id));
    set(&mut call, "parentId", string(parent));
    let Value::Array(parts) = call.get_mut("message").unwrap().get_mut("content").unwrap() else {
        unreachable!()
    };
    set(&mut parts[0], "id", string(&call_id));
    set(&mut result, "id", string(&format!("{prefix}-result")));
    set(&mut result, "parentId", string(&invocation_id));
    set(
        result.get_mut("message").unwrap(),
        "toolCallId",
        string(&call_id),
    );
    (call, result)
}
fn first_array_mut(value: &mut Value) -> &mut Value {
    let Value::Array(values) = value else {
        panic!("fixture array required")
    };
    values.first_mut().unwrap()
}
fn carrier_uri(call: &mut Value, result: &mut Value, uri: Value) {
    set(
        first_array_mut(call.get_mut("message").unwrap().get_mut("content").unwrap())
            .get_mut("arguments")
            .unwrap(),
        "path",
        uri.clone(),
    );
    set(
        result
            .get_mut("message")
            .unwrap()
            .get_mut("details")
            .unwrap()
            .get_mut("meta")
            .unwrap()
            .get_mut("source")
            .unwrap(),
        "value",
        uri,
    );
}

// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_fresh_owned_guides_capture_exact_metadata_in_copy_and_link_modes
#[test]
#[ignore = "recovery_frontier"]
fn fresh_owned_guides_retain_exact_closed_context_in_copy_and_link_modes() {
    scenarios(
        "fresh_owned_guides_retain_exact_closed_context_in_copy_and_link_modes",
        &["copy", "link"],
        |root, mode| {
            let f = fixture_mode(root, mode);
            let mut entries = vec![human("human")];
            let mut selected = Vec::new();
            for (n, relative) in [
                "capture/GUIDE.md",
                "admission/GUIDE.md",
                "lifecycle/install/GUIDE.md",
                "model/GUIDE.md",
            ]
            .iter()
            .enumerate()
            {
                let witness = omp_installation::owned_manual(&f.receipt, relative)
                    .unwrap()
                    .unwrap();
                assert_eq!(witness.relative_path(), *relative);
                let (call, result) = prefixed_pair(
                    &witness,
                    &format!("guide-{n}"),
                    text(entries.last().unwrap(), "id"),
                );
                selected.push((relative.to_string(), call.clone(), result.clone()));
                entries.extend([call, result]);
            }
            entries.push(answer("answer"));
            let observed = f.observe(&request(&f, &entries, "seal"));
            let s = first_segment(&f);
            let manifest =
                transport::read(&s.directory.join("segment.json"), transport::MAX_PAYLOAD).unwrap();
            assert_eq!(get(&manifest, "gaps"), &Value::Array(vec![]));
            assert!(snapshot(&s.directory.join("gaps")).is_empty());
            for (relative, call, result) in selected {
                let payload = transport::read(
                    &native_path(&s.directory, "snapshots", text(&result, "id")),
                    transport::MAX_PAYLOAD,
                )
                .unwrap();
                let expected = object([
                    ("carrier", string("message")),
                    ("native_entry_id", get(&result, "id").clone()),
                    ("native_parent_id", get(&call, "id").clone()),
                    ("role", string("toolResult")),
                    (
                        "toolCallId",
                        get(get(&result, "message"), "toolCallId").clone(),
                    ),
                    ("toolName", string("read")),
                    ("isError", Value::Bool(false)),
                    ("content", Value::Array(vec![])),
                    (
                        "instruction_context",
                        object([
                            (
                                "schema",
                                string("agent-memory/owned-instruction-context-v1"),
                            ),
                            ("kind", string("installed-am-manual")),
                            (
                                "installation_id",
                                get(&f.receipt, "installation_id").clone(),
                            ),
                            (
                                "observed_receipt_sha256",
                                string(&transport::digest(&f.receipt).unwrap()),
                            ),
                            (
                                "artifact_digest",
                                get(get(&f.receipt, "source"), "artifact_digest").clone(),
                            ),
                            (
                                "manual_file_sha256",
                                get(get(get(&f.receipt, "artifact_files"), &relative), "sha256")
                                    .clone(),
                            ),
                            ("manual_relative_path", string(&relative)),
                            ("renderer", string("omp-18.16-full-skill-v1")),
                            ("invocation_native_entry_id", get(&call, "id").clone()),
                            (
                                "tool_call_id",
                                get(get(&result, "message"), "toolCallId").clone(),
                            ),
                        ]),
                    ),
                ]);
                assert_eq!(
                    turn_sources::encoded(&payload).unwrap(),
                    turn_sources::encoded(&expected).unwrap()
                );
                assert!(agent_memory::omp_projection::valid_instruction_context(
                    &payload
                ));
            }
            for native in entries
                .iter()
                .filter(|e| get(get(e, "message"), "role").as_str() != Some("toolResult"))
            {
                let payload = transport::read(
                    &native_path(&s.directory, "snapshots", text(native, "id")),
                    transport::MAX_PAYLOAD,
                )
                .unwrap();
                assert_eq!(
                    get(&payload, "content"),
                    get(get(native, "message"), "content")
                );
                assert!(payload.get("instruction_context").is_none());
                assert!(native_path(&s.directory, "captures", text(native, "id")).is_file());
            }
            let before = snapshot(&s.directory);
            f.observe(&replay_request(&f, &entries, "ingest", &observed));
            assert_eq!(snapshot(&s.directory), before);
            assert_eq!(transport::segments(&f.site(), SESSION).unwrap().len(), 1);
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_owned_guide_alias_partial_tamper_and_foreign_reads_keep_real_gaps
#[test]
#[ignore = "recovery_frontier"]
fn owned_guide_alias_partial_tamper_and_foreign_carriers_keep_original_gaps() {
    isolated(
        "owned_guide_alias_partial_tamper_and_foreign_carriers_keep_original_gaps",
        |root| {
            let f = fixture_mode(root, "copy");
            let witness = omp_installation::owned_manual(&f.receipt, "admission/GUIDE.md")
                .unwrap()
                .unwrap();
            let aliases = [
                string("skill://agent-memory/SKILL.md"),
                string("skill://agent-memory/admission/./GUIDE.md"),
                string("skill://agent-memory/admission/../admission/GUIDE.md"),
                string("skill://agent-memory//admission/GUIDE.md"),
                string("skill://agent-memory/Admission/GUIDE.md"),
                string("skill://agent-memory/admission/guide.md"),
                string("skill://agent-memory/admission/GUIDE.md/"),
                string("skill://agent-memory/%61dmission/GUIDE.md"),
                string("skill://agent-memory/admission%2fGUIDE.md"),
                string("skill://agent-memory/admission\\GUIDE.md"),
                string("skill://agent-memory/admission/GUIDE.md:1-20"),
                string("skill://agent-memory/admission/GUIDE.md?raw=true"),
                string("skill://agent-memory/admission/GUIDE.md#section"),
                string("skill://other/admission/GUIDE.md"),
                string(witness.header_path()),
                Value::Null,
                Value::Int(7u8.into()),
            ];
            let mut entries = vec![human("human")];
            let mut rejected = Vec::new();
            for (n, uri) in aliases.into_iter().enumerate() {
                let (mut call, mut result) = prefixed_pair(
                    &witness,
                    &format!("alias-{n}"),
                    text(entries.last().unwrap(), "id"),
                );
                carrier_uri(&mut call, &mut result, uri);
                rejected.push(text(&result, "id").to_owned());
                entries.extend([call, result]);
            }
            for kind in ["partial", "tamper", "foreign", "source-mismatch"] {
                let (call, mut result) =
                    prefixed_pair(&witness, kind, text(entries.last().unwrap(), "id"));
                let message = result.get_mut("message").unwrap();
                if kind == "source-mismatch" {
                    set(
                        message
                            .get_mut("details")
                            .unwrap()
                            .get_mut("meta")
                            .unwrap()
                            .get_mut("source")
                            .unwrap(),
                        "value",
                        string("skill://agent-memory/capture/GUIDE.md"),
                    );
                } else {
                    let original = text(get(get(message, "details"), "displayContent"), "text");
                    let body = if kind == "partial" {
                        original.split_once('\n').unwrap().1.to_owned()
                    } else if kind == "tamper" {
                        format!("{original}\nAltered source tail.")
                    } else {
                        original.to_owned()
                    };
                    let path = if kind == "foreign" {
                        let path = root.join("foreign guide.md");
                        write(&path, witness.text(), 0o644);
                        path.to_str().unwrap().to_owned()
                    } else {
                        witness.header_path().to_owned()
                    };
                    set(
                        first_array_mut(message.get_mut("content").unwrap()),
                        "text",
                        string(&format!("[Skill file: {path}]\n{body}")),
                    );
                    let details = message.get_mut("details").unwrap();
                    set(details, "resolvedPath", string(&path));
                    let start = if kind == "partial" { 2usize } else { 1 };
                    let count = body.split('\n').count();
                    set(
                        details,
                        "totalLines",
                        Value::Int((count + start - 1).into()),
                    );
                    set(
                        details,
                        "displayContent",
                        object([
                            (
                                "lineNumbers",
                                Value::Array(
                                    (start..start + count)
                                        .map(|n| Value::Int(n.into()))
                                        .collect(),
                                ),
                            ),
                            ("startLine", Value::Int(start.into())),
                            ("text", string(&body)),
                        ]),
                    );
                }
                rejected.push(text(&result, "id").to_owned());
                entries.extend([call, result]);
            }
            entries.push(answer("answer"));
            f.observe(&request(&f, &entries, "seal"));
            let s = first_segment(&f);
            for native in rejected {
                assert!(!native_path(&s.directory, "snapshots", &native).exists());
                assert!(has_gap(&s.directory, &native, "unsafe-representation"));
            }
            for native in ["human", "answer"] {
                let value = entries.iter().find(|e| text(e, "id") == native).unwrap();
                assert_eq!(
                    get(
                        &transport::read(
                            &native_path(&s.directory, "snapshots", native),
                            transport::MAX_PAYLOAD
                        )
                        .unwrap(),
                        "content"
                    ),
                    get(get(value, "message"), "content")
                );
            }
            assert!(
                f.pending()
                    .iter()
                    .any(|r| text(r, "status") == "pending-coverage-gap")
            );
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_noninstruction_artifact_read_retains_ordinary_body_not_owned_metadata
#[test]
#[ignore = "recovery_frontier"]
fn noninstruction_packaged_artifact_retains_actual_ordinary_body_and_details() {
    isolated(
        "noninstruction_packaged_artifact_retains_actual_ordinary_body_and_details",
        |root| {
            let f = fixture_mode(root, "copy");
            let relative = "templates/router-markdown.example.txt";
            let witness = omp_installation::owned_manual(&f.receipt, "admission/GUIDE.md")
                .unwrap()
                .unwrap();
            assert!(get(&f.receipt, "artifact_files").get(relative).is_some());
            assert!(
                omp_installation::owned_manual(&f.receipt, relative)
                    .unwrap()
                    .is_none()
            );
            let path =
                Path::new(text(get(&f.receipt, "omp_binding"), "skill_target")).join(relative);
            let body = std::fs::read_to_string(&path).unwrap();
            let body = body.trim_end_matches('\n');
            let (mut call, mut result) = pair(&witness);
            carrier_uri(
                &mut call,
                &mut result,
                string(&format!("skill://agent-memory/{relative}")),
            );
            let message = result.get_mut("message").unwrap();
            set(
                first_array_mut(message.get_mut("content").unwrap()),
                "text",
                string(&format!("[Skill file: {}]\n{body}", path.display())),
            );
            let details = message.get_mut("details").unwrap();
            set(details, "resolvedPath", string(path.to_str().unwrap()));
            let count = body.split('\n').count();
            set(details, "totalLines", Value::Int(count.into()));
            set(
                details,
                "displayContent",
                object([
                    (
                        "lineNumbers",
                        Value::Array((1..=count).map(|n| Value::Int(n.into())).collect()),
                    ),
                    ("startLine", Value::Int(1u8.into())),
                    ("text", string(body)),
                ]),
            );
            f.observe(&request(
                &f,
                &[human("human"), call, result.clone()],
                "seal",
            ));
            let payload = transport::read(
                &native_path(&first_segment(&f).directory, "snapshots", "manual-result"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert!(payload.get("instruction_context").is_none());
            assert_eq!(
                get(&payload, "content"),
                get(get(&result, "message"), "content")
            );
            assert_eq!(
                get(&payload, "details"),
                get(get(&result, "message"), "details")
            );
        },
    );
}

// pack-surface obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_verification_races_refuse_selected_binding_payload_and_policy_changes

#[test]
fn owned_manual_verification_rejects_real_binding_payload_and_policy_races() {
    let choices = [
        "copy:directory-substitution",
        "copy:directory-mode",
        "copy:receipt",
        "copy:receipt-mode",
        "copy:manual-substitution",
        "copy:manual-symlink",
        "copy:manual-bytes",
        "copy:manual-mode",
        "link:directory-substitution",
        "link:directory-mode",
        "link:receipt",
        "link:receipt-mode",
        "link:link-substitution",
        "link:link-retarget",
    ];
    scenarios(
        "owned_manual_verification_rejects_real_binding_payload_and_policy_races",
        &choices,
        |root, choice| {
            let (mode, race) = choice.split_once(':').unwrap();
            let (_, receipt, _) = install_only(root, mode);
            let skill = PathBuf::from(text(get(&receipt, "omp_binding"), "skill_target"));
            let manual = skill.join("SKILL.md");
            let parent = skill.parent().unwrap();
            let receipt_path = root.join("state/agent-memory/install/receipt.json");
            let stage = if race.starts_with("receipt") {
                "omp.manual.before-final-receipt"
            } else {
                "omp.manual.after-final-receipt"
            };
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&object([("relative", string("SKILL.md"))])).unwrap(),
                0o600,
            );
            let child = operation_child(root, Some(stage), "pause", "manual-verify");
            ready(root, stage);
            match race {
                "directory-substitution" => {
                    std::fs::rename(parent, parent.with_extension("race-backup")).unwrap();
                    std::fs::create_dir(parent).unwrap();
                }
                "directory-mode" => {
                    let permissions = std::fs::metadata(parent).unwrap().permissions().mode();
                    std::fs::set_permissions(
                        parent,
                        std::fs::Permissions::from_mode(permissions | 0o002),
                    )
                    .unwrap();
                }
                "manual-substitution" | "manual-symlink" => {
                    let backup = manual.with_extension("race-backup");
                    let raw = std::fs::read(&manual).unwrap();
                    std::fs::rename(&manual, &backup).unwrap();
                    if race == "manual-symlink" {
                        symlink(backup, &manual).unwrap();
                    } else {
                        write(&manual, raw, 0o644);
                    }
                }
                "manual-bytes" => {
                    let mut raw = std::fs::read(&manual).unwrap();
                    raw[0] ^= 1;
                    write(&manual, raw, 0o644);
                }
                "manual-mode" => {
                    let mode = std::fs::metadata(&manual).unwrap().permissions().mode();
                    std::fs::set_permissions(
                        &manual,
                        std::fs::Permissions::from_mode(mode | 0o002),
                    )
                    .unwrap();
                }
                "receipt" => {
                    let mut value = receipt.clone();
                    set(
                        &mut value,
                        "installation_id",
                        string(&uid().replace('-', "")),
                    );
                    write(&receipt_path, turn_sources::encoded(&value).unwrap(), 0o600);
                }
                "receipt-mode" => {
                    let mode = std::fs::metadata(&receipt_path)
                        .unwrap()
                        .permissions()
                        .mode();
                    std::fs::set_permissions(
                        &receipt_path,
                        std::fs::Permissions::from_mode(mode | 0o002),
                    )
                    .unwrap();
                }
                _ => {
                    let backup = skill.with_extension("race-backup");
                    std::fs::rename(&skill, backup).unwrap();
                    let target = if race == "link-substitution" {
                        PathBuf::from(text(get(&receipt, "source"), "pack"))
                    } else {
                        root.join("foreign")
                    };
                    symlink(target, &skill).unwrap();
                }
            }
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert_eq!(registry_result(root), Value::Bool(false), "{choice}");
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_unrelated_ancestor_churn_preserves_witness_and_public_annotation
#[test]
#[ignore = "recovery_frontier"]
fn unrelated_ancestor_churn_preserves_owned_witness_and_public_source_annotation() {
    scenarios(
        "unrelated_ancestor_churn_preserves_owned_witness_and_public_source_annotation",
        &["copy", "link"],
        |root, mode| {
            let f = fixture_mode(root, mode);
            let witness = omp_installation::owned_manual(&f.receipt, "SKILL.md")
                .unwrap()
                .unwrap();
            let (call, result) = pair(&witness);
            let request = request(&f, &[human("human"), call, result], "seal");
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request).unwrap(),
                0o600,
            );
            let before = "omp.manual.before-final-receipt";
            let after = "omp.manual.after-final-receipt";
            let child =
                operation_child(root, Some(&format!("{before},{after}")), "pause", "observe");
            ready(root, before);
            let sibling = root.join("unrelated sibling");
            std::fs::create_dir(&sibling).unwrap();
            release(root, before);
            ready(root, after);
            std::fs::remove_dir(&sibling).unwrap();
            release(root, after);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(witness.verify().unwrap().is_some());
            let s = first_segment(&f);
            let payload = transport::read(
                &native_path(&s.directory, "snapshots", "manual-result"),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert_eq!(get(&payload, "content"), &Value::Array(vec![]));
            assert_eq!(
                text(get(&payload, "instruction_context"), "tool_call_id"),
                "manual-call"
            );
            assert!(agent_memory::omp_projection::valid_instruction_context(
                &payload
            ));
            assert!(!has_gap(
                &s.directory,
                "tool:manual-call",
                "missing-stored-source"
            ));
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_historical_owned_guide_never_authorizes_read_after_installation_changes
#[test]
#[ignore = "recovery_frontier"]
fn historical_owned_guide_metadata_cannot_authorize_fresh_read_after_installation_change() {
    isolated(
        "historical_owned_guide_metadata_cannot_authorize_fresh_read_after_installation_change",
        |root| {
            let f = fixture_mode(root, "copy");
            let witness = omp_installation::owned_manual(&f.receipt, "admission/GUIDE.md")
                .unwrap()
                .unwrap();
            let (call, result) = pair(&witness);
            let first = f.observe(&request(
                &f,
                &[human("human"), call.clone(), result.clone()],
                "seal",
            ));
            let s = first_segment(&f);
            let path = native_path(&s.directory, "snapshots", "manual-result");
            let historical = std::fs::read(&path).unwrap();
            let (forged_call, mut forged) = prefixed_pair(&witness, "forged", "manual-result");
            let context = get(
                &transport::read(&path, transport::MAX_PAYLOAD).unwrap(),
                "instruction_context",
            )
            .clone();
            set(
                forged.get_mut("message").unwrap(),
                "instruction_context",
                context,
            );
            write(
                Path::new(witness.header_path()),
                format!(
                    "{}\nInstallation changed after observation.\n",
                    witness.text()
                ),
                0o644,
            );
            assert!(witness.verify().unwrap().is_none());
            assert!(
                omp_installation::owned_manual(&f.receipt, "admission/GUIDE.md")
                    .unwrap()
                    .is_none()
            );
            assert!(agent_memory::omp_projection::valid_instruction_context(
                &transport::read(&path, transport::MAX_PAYLOAD).unwrap()
            ));
            for relative in [
                "admission/../admission/GUIDE.md",
                "Admission/GUIDE.md",
                "admission/GUIDE.md:1-2",
                "templates/router-markdown.example.txt",
            ] {
                let mut changed = transport::read(&path, transport::MAX_PAYLOAD).unwrap();
                set(
                    changed.get_mut("instruction_context").unwrap(),
                    "manual_relative_path",
                    string(relative),
                );
                assert!(!agent_memory::omp_projection::valid_instruction_context(
                    &changed
                ));
            }
            f.observe(&replay_request(
                &f,
                &[human("human"), call.clone(), result.clone()],
                "ingest",
                &first,
            ));
            assert_eq!(std::fs::read(&path).unwrap(), historical);
            let session = uid();
            let mut fresh = request(
                &f,
                &[human("human"), call, result, forged_call, forged],
                "seal",
            );
            set(&mut fresh, "session", string(&session));
            set(&mut fresh, "lifecycle", string(&uid()));
            let payload = fresh.get_mut("payload").unwrap();
            let view = payload.get_mut("view").unwrap();
            set(view, "session", string(&session));
            set(view.get_mut("header").unwrap(), "id", string(&session));
            f.observe(&fresh);
            let fresh = transport::segments(&f.site(), &session).unwrap().remove(0);
            for native in ["manual-result", "forged-result"] {
                assert!(!native_path(&fresh.directory, "snapshots", native).exists());
                assert!(has_gap(&fresh.directory, native, "unsafe-representation"));
            }
            assert_eq!(std::fs::read(path).unwrap(), historical);
        },
    );
}
// frontier obligation: tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_owned_guide_rechecks_receipt_selection_and_nested_symlinks
#[test]
#[ignore = "recovery_frontier"]
fn owned_guide_rechecks_selected_receipt_entry_nested_links_and_actual_rebind() {
    isolated(
        "owned_guide_rechecks_selected_receipt_entry_nested_links_and_actual_rebind",
        |root| {
            let f = fixture_mode(root, "copy");
            let relative = "lifecycle/install/GUIDE.md";
            let witness = omp_installation::owned_manual(&f.receipt, relative)
                .unwrap()
                .unwrap();
            for invalid in [
                "../lifecycle/install/GUIDE.md",
                "/lifecycle/install/GUIDE.md",
                "lifecycle//install/GUIDE.md",
                "lifecycle/install/./GUIDE.md",
                "lifecycle/install/guide.md",
                "lifecycle/install/GUIDE.md:1-2",
                "SKILL.md/extra",
                "missing/GUIDE.md",
                "",
            ] {
                assert!(
                    omp_installation::owned_manual(&f.receipt, invalid)
                        .unwrap()
                        .is_none()
                );
            }
            for kind in ["missing-entry", "other-file-hash", "installation-rebind"] {
                let mut changed = f.receipt.clone();
                if kind == "missing-entry" {
                    remove_key(changed.get_mut("artifact_files").unwrap(), relative);
                } else if kind == "other-file-hash" {
                    let other = get(get(&changed, "artifact_files"), "SKILL.md").clone();
                    set(changed.get_mut("artifact_files").unwrap(), relative, other);
                } else {
                    set(
                        &mut changed,
                        "installation_id",
                        string(&uid().replace('-', "")),
                    );
                }
                assert!(
                    omp_installation::owned_manual(&changed, relative)
                        .unwrap()
                        .is_none()
                );
            }
            let manual = PathBuf::from(witness.header_path());
            let original = std::fs::read(&manual).unwrap();
            let mode = std::fs::metadata(&manual).unwrap().permissions().mode();
            for kind in [
                "changed-bytes",
                "writable",
                "file-link",
                "directory-link",
                "hardlink",
            ] {
                let backup = manual.with_extension("backup");
                let parent_backup = manual.parent().unwrap().with_extension("backup");
                match kind {
                    "changed-bytes" => write(
                        &manual,
                        [original.as_slice(), b"\nChanged guide.\n"].concat(),
                        mode,
                    ),
                    "writable" => std::fs::set_permissions(
                        &manual,
                        std::fs::Permissions::from_mode(mode | 0o002),
                    )
                    .unwrap(),
                    "file-link" | "hardlink" => {
                        std::fs::rename(&manual, &backup).unwrap();
                        if kind == "file-link" {
                            symlink(&backup, &manual).unwrap();
                        } else {
                            std::fs::hard_link(&backup, &manual).unwrap();
                        }
                    }
                    _ => {
                        std::fs::rename(manual.parent().unwrap(), &parent_backup).unwrap();
                        symlink(&parent_backup, manual.parent().unwrap()).unwrap();
                    }
                }
                assert!(witness.verify().unwrap().is_none());
                assert!(
                    omp_installation::owned_manual(&f.receipt, relative)
                        .unwrap()
                        .is_none()
                );
                match kind {
                    "changed-bytes" => write(&manual, &original, mode),
                    "writable" => {
                        std::fs::set_permissions(&manual, std::fs::Permissions::from_mode(mode))
                            .unwrap()
                    }
                    "file-link" | "hardlink" => {
                        std::fs::remove_file(&manual).unwrap();
                        std::fs::rename(&backup, &manual).unwrap();
                    }
                    _ => {
                        std::fs::remove_file(manual.parent().unwrap()).unwrap();
                        std::fs::rename(&parent_backup, manual.parent().unwrap()).unwrap();
                    }
                }
            }
            let (call, result) = pair(&witness);
            let value = request(&f, &[human("human"), call, result], "seal");
            let receipt_path = root.join("state/agent-memory/install/receipt.json");
            let original_receipt = std::fs::read(&receipt_path).unwrap();
            let mut rebound = f.receipt.clone();
            set(
                &mut rebound,
                "installation_id",
                string(&uid().replace('-', "")),
            );
            write(
                &receipt_path,
                turn_sources::encoded(&rebound).unwrap(),
                0o600,
            );
            assert!(witness.verify().unwrap().is_none());
            assert!(
                omp_installation::owned_manual(&f.receipt, relative)
                    .unwrap()
                    .is_none()
            );
            let fresh = omp_installation::owned_manual(&rebound, relative)
                .unwrap()
                .unwrap();
            assert_ne!(
                fresh.observed_receipt_sha256(),
                witness.observed_receipt_sha256()
            );
            assert!(
                observed(
                    &f,
                    &value,
                    transport::Observation {
                        receipt: Some(&f.receipt),
                        ..Default::default()
                    }
                )
                .is_err()
            );
            assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
            let proof = Arc::new(
                omp_installation::verified_observer_binding(
                    text(get(&rebound, "source"), "artifact_digest"),
                    text(get(&rebound, "omp_binding"), "agent_dir"),
                    Some(&rebound),
                )
                .unwrap(),
            );
            let rebound = Fixture {
                repo: f.repo.clone(),
                runtime: f.runtime.clone(),
                receipt: rebound,
                proof,
            };
            rebound.observe(&value);
            let payload = transport::read(
                &native_path(
                    &first_segment(&rebound).directory,
                    "snapshots",
                    "manual-result",
                ),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert_eq!(
                get(get(&payload, "instruction_context"), "installation_id"),
                get(&rebound.receipt, "installation_id")
            );
            assert_eq!(
                text(
                    get(&payload, "instruction_context"),
                    "observed_receipt_sha256"
                ),
                transport::digest(&rebound.receipt).unwrap()
            );
            write(&receipt_path, original_receipt, 0o600);
        },
    );
}

#[test]

fn committed_full_instruction_pack_is_a_real_valid_owned_fixture() {
    isolated(
        "committed_full_instruction_pack_is_a_real_valid_owned_fixture",
        |root| {
            let (_, receipt, _) = install_only(root, "copy");
            for relative in [
                "capture/GUIDE.md",
                "admission/GUIDE.md",
                "lifecycle/install/GUIDE.md",
                "model/GUIDE.md",
            ] {
                let witness = omp_installation::owned_manual(&receipt, relative)
                    .unwrap()
                    .unwrap();
                assert_eq!(witness.relative_path(), relative);
                assert!(witness.verify().unwrap().is_some());
            }
        },
    );
}

fn controls(root: &Path, items: &[(&str, &str)], swap_transport: bool) {
    let steps = items
        .iter()
        .map(|(stage, control)| object([("stage", string(stage)), ("control", string(control))]))
        .collect();
    write(
        &root.join("case-controls.json"),
        turn_sources::encoded(&object([
            ("steps", Value::Array(steps)),
            ("swap_transport", Value::Bool(swap_transport)),
        ]))
        .unwrap(),
        0o600,
    );
}
fn start_window_controller(
    root: &Path,
    owner: &stream::ClockWindows,
    writer: &Arc<stream::ResponseWriter>,
    nonce: &str,
) -> Option<std::thread::JoinHandle<()>> {
    if !root.join("case-controls.json").exists() {
        return None;
    }
    let root = root.to_path_buf();
    let owner = owner.clone();
    let writer = writer.clone();
    let nonce = nonce.to_owned();
    Some(std::thread::spawn(move || {
        let plan = transport::read(&root.join("case-controls.json"), 65536).unwrap();
        for step in get(&plan, "steps").as_array().unwrap() {
            let stage = text(step, "stage");
            ready(&root, stage);
            match text(step, "control") {
                "cancel" | "cancel-outcome-io" => {
                    owner
                        .control(
                            "CANCEL",
                            &object([("nonce", string(&nonce))]),
                            writer.as_ref(),
                        )
                        .unwrap();
                    if text(step, "control") == "cancel-outcome-io" {
                        let path = child_directory(&root).join("outcomes");
                        if path.is_dir() {
                            std::fs::remove_dir(&path).unwrap();
                        }
                        write(&path, b"Retain actual non-directory I/O evidence\n", 0o600);
                    }
                }
                "replacement" => calibrate_window(&owner, &writer, &uid()),
                "eof" => owner.end(),
                "output-loss" => writer.finish(),
                "none" => (),
                other => panic!("unknown control fixture {other}"),
            }
            if get(&plan, "swap_transport") == &Value::Bool(true) {
                let directory = child_directory(&root);
                let base = directory.ancestors().nth(3).unwrap();
                let path = base.join("omp-transport.lock");
                std::fs::rename(&path, base.join("omp-transport.operation-held")).unwrap();
                write(&path, b"", 0o600);
            }
            write(&root.join("control-applied"), text(step, "control"), 0o600);
            release(&root, stage);
        }
    }))
}
fn wake_operation(root: &Path, f: &Fixture, value: &Value) {
    let (owner, writer, nonce) = real_window();
    if let Ok(before) = std::fs::read_to_string(root.join("wake-before-control")) {
        if before == "cancel" {
            owner.revoke();
        } else if before == "expired" {
            std::thread::sleep(Duration::from_millis(960));
        }
    }
    let controller = start_window_controller(root, &owner, &writer, &nonce);
    let result = observed(
        f,
        value,
        transport::Observation {
            receipt: Some(&f.receipt),
            delivery_window: owner.window(&nonce).map(stream::DeliveryWindow::Verified),
            ..Default::default()
        },
    );
    write(
        &root.join("operation-result.json"),
        turn_sources::encoded(&match result {
            Ok(value) => value,
            Err(error) => object([("error", string(&error.to_string()))]),
        })
        .unwrap(),
        0o600,
    );
    if let Some(controller) = controller {
        controller.join().unwrap();
    }
    owner.end();
}
fn run_wake_child(root: &Path, request: &Value, selector: Option<&str>, mode: &str) -> Value {
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(request).unwrap(),
        0o600,
    );
    let child = operation_child(root, selector, mode, "wake-observe");
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    registry_result(root)
}
// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_cancel_before_reservation_refuses_and_after_start_preserves_result
#[test]
#[ignore = "recovery_frontier"]
fn cancellation_before_reservation_refuses_but_after_reservation_keeps_one_root() {
    isolated(
        "cancellation_before_reservation_refuses_but_after_reservation_keeps_one_root",
        |root| {
            let f = Fixture::new(root);
            let initial = f.observe(&request(&f, &[human("human")], "seal"));
            let wake = wake_request(&initial);
            write(&root.join("wake-before-control"), b"cancel", 0o600);
            let refused = run_wake_child(root, &wake, None, "error");
            assert_eq!(get(&refused, "verified"), &Value::Bool(true));
            assert_eq!(get(&refused, "continue"), &Value::Bool(false));
            assert_eq!(text(&refused, "status"), "pending-undeliverable-window");
            assert!(root_wakes(&f).is_empty());
            std::fs::remove_file(root.join("wake-before-control")).unwrap();
            let stage = "omp.reservation.after-reserve";
            controls(root, &[(stage, "cancel")], false);
            let accepted = run_wake_child(root, &wake, Some(stage), "pause");
            assert_eq!(get(&accepted, "verified"), &Value::Bool(true));
            assert_eq!(get(&accepted, "continue"), &Value::Bool(true));
            assert_eq!(root_wakes(&f).len(), 1);
            std::fs::remove_file(root.join("case-controls.json")).unwrap();
            let (owner, _writer, nonce) = real_window();
            assert_eq!(
                get(&with_window(&f, &wake, &owner, &nonce), "continue"),
                &Value::Bool(false)
            );
            assert_eq!(root_wakes(&f).len(), 1);
            owner.end();
        },
    );
}
// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_replacement_and_end_after_start_preserve_result
#[test]
#[ignore = "recovery_frontier"]
fn replacing_clock_or_ending_after_reservation_preserves_exact_single_root() {
    scenarios(
        "replacing_clock_or_ending_after_reservation_preserves_exact_single_root",
        &["replacement", "eof"],
        |root, mode| {
            let f = Fixture::new(root);
            let result = f.observe(&request(&f, &[human("human")], "seal"));
            let wake = wake_request(&result);
            let stage = "omp.reservation.after-reserve";
            controls(root, &[(stage, mode)], false);
            let accepted = run_wake_child(root, &wake, Some(stage), "pause");
            assert_eq!(get(&accepted, "verified"), &Value::Bool(true));
            assert_eq!(get(&accepted, "continue"), &Value::Bool(true));
            assert_eq!(
                std::fs::read_to_string(root.join("control-applied")).unwrap(),
                mode
            );
            assert_eq!(root_wakes(&f).len(), 1);
            let (owner, _writer, nonce) = real_window();
            assert_eq!(
                get(&with_window(&f, &wake, &owner, &nonce), "continue"),
                &Value::Bool(false)
            );
            assert_eq!(root_wakes(&f).len(), 1);
            owner.end();
        },
    );
}
// frontier obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_pre_reservation_wake_exception_does_not_unverify_committed_source
#[test]
#[ignore = "recovery_frontier"]
fn real_prereservation_fault_does_not_unverify_durable_direct_seal() {
    isolated(
        "real_prereservation_fault_does_not_unverify_durable_direct_seal",
        |root| {
            let f = Fixture::new(root);
            let mut value = request(&f, &[human("human")], "seal");
            set(
                value.get_mut("payload").unwrap(),
                "request_wake",
                Value::Bool(true),
            );
            let result = run_wake_child(
                root,
                &value,
                Some("omp.reservation.before-reserve"),
                "error",
            );
            assert_eq!(get(&result, "verified"), &Value::Bool(true));
            assert_eq!(get(&result, "continue"), &Value::Bool(false));
            assert_eq!(text(&result, "status"), "pending-wake-unavailable");
            assert!(root_wakes(&f).is_empty());
            assert_eq!(
                f.pending()
                    .iter()
                    .filter(|r| r.get("observation_id").is_some())
                    .count(),
                2
            );
        },
    );
}
fn cancelled_outcome_failure(root: &Path, io_failure: bool) {
    let f = Fixture::new(root);
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&armed_request(&f)).unwrap(),
        0o600,
    );
    let pause = "omp.outcome.before-guard";
    let refuse = "omp.outcome.before-commit";
    controls(
        root,
        &[(
            pause,
            if io_failure {
                "cancel-outcome-io"
            } else {
                "cancel"
            },
        )],
        false,
    );
    let selector = if io_failure {
        pause.to_owned()
    } else {
        format!("{pause},{refuse}")
    };
    let mode = if io_failure { "pause" } else { "pause,error" };
    let child = operation_child(root, Some(&selector), mode, "finite");
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    assert!(registry_result(root).get("error").is_some());
    let directory = child_directory(root);
    assert!(directory.join("started/1.json").is_file());
    assert!(!directory.join("outcomes/1.json").exists());
    assert!(!directory.join("closed.json").exists());
    no_verified_response(root);
    assert!(root_wakes(&f).is_empty());
    let rows = f.pending();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.get("observation_id").is_none()));
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_io_failure_never_verifies_partial_seal
#[test]
#[ignore = "recovery_frontier"]
fn cancelled_uncommitted_outcome_real_io_failure_never_verifies_partial_seal() {
    isolated(
        "cancelled_uncommitted_outcome_real_io_failure_never_verifies_partial_seal",
        |root| cancelled_outcome_failure(root, true),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_refusal_never_verifies_partial_seal
#[test]
#[ignore = "recovery_frontier"]
fn cancelled_uncommitted_outcome_typed_refusal_never_verifies_partial_seal() {
    isolated(
        "cancelled_uncommitted_outcome_typed_refusal_never_verifies_partial_seal",
        |root| cancelled_outcome_failure(root, false),
    );
}
fn controlled_reserved_root_exit(root: &Path, control: &str) {
    let f = Fixture::new(root);
    let before = snapshot(&f.repo.join("memory-bank"));
    let index = std::fs::read(f.repo.join(".git/index")).unwrap();
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&armed_request(&f)).unwrap(),
        0o600,
    );
    let stage = "omp.reservation.after-reserve";
    controls(root, &[(stage, control)], true);
    let child = operation_child(root, Some(stage), "pause", "finite-auto-calibrated");
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    let base = boundary::base(&f.repo).unwrap();
    std::fs::remove_file(base.join("omp-transport.lock")).unwrap();
    std::fs::rename(
        base.join("omp-transport.operation-held"),
        base.join("omp-transport.lock"),
    )
    .unwrap();
    let directory = child_directory(root);
    assert_verified_pending_source(&f, &directory);
    assert_eq!(root_wakes(&f).len(), 1);
    let retained = transport::read(&root_wakes(&f)[0], 4096).unwrap();
    assert_eq!(text(&retained, "status"), "requested");
    if control == "output-loss" {
        no_verified_response(root);
    } else {
        let result = child_result(root);
        assert_eq!(get(&result, "verified"), &Value::Bool(true));
        assert_eq!(get(&result, "continue"), &Value::Bool(false));
        assert_eq!(text(&result, "status"), "pending-wake-unknown");
        assert!(result.get("root_request_id").is_none());
    }
    assert_eq!(snapshot(&f.repo.join("memory-bank")), before);
    assert_eq!(std::fs::read(f.repo.join(".git/index")).unwrap(), index);
    let rows = f.pending();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.get("observation_id").is_some()));
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_cancel_after_actual_reserve_then_exit_fault_retains_one_unknown_root
#[test]
#[ignore = "recovery_frontier"]
fn cancel_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root() {
    isolated(
        "cancel_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root",
        |root| controlled_reserved_root_exit(root, "cancel"),
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_replacement_after_actual_reserve_then_exit_fault_retains_one_unknown_root
#[test]
#[ignore = "recovery_frontier"]
fn replacement_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root() {
    isolated(
        "replacement_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root",
        |root| controlled_reserved_root_exit(root, "replacement"),
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_eof_after_actual_reserve_then_exit_fault_retains_one_unknown_root
#[test]
#[ignore = "recovery_frontier"]
fn eof_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root() {
    isolated(
        "eof_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root",
        |root| controlled_reserved_root_exit(root, "eof"),
    );
}

// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_output_loss_after_actual_reserve_then_exit_fault_retains_one_unknown_root
#[test]
#[ignore = "recovery_frontier"]
fn output_loss_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root() {
    isolated(
        "output_loss_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root",
        |root| controlled_reserved_root_exit(root, "output-loss"),
    );
}

fn fixture_without_native_activation(root: &Path) -> Fixture {
    let (runtime, receipt, proof) = install_only(root, "copy");
    let repo = bank_fixture(root);
    Fixture {
        repo,
        runtime,
        receipt,
        proof,
    }
}
fn public_observation_operation(root: &Path, f: &Fixture, operation: &str) {
    let result = if operation == "compatibility" {
        let target = decode(r#"{"private_state_capabilities":[]}"#);
        agent_memory::private_state::required_private_state_capabilities(
            None,
            Some(&target),
            "install",
            &f.repo,
            &f.runtime,
        )
    } else {
        let barrier = turn_sources::source_barrier(&f.repo, true, false, None, &f.runtime).unwrap();
        let result = turn_sources::read_only_locks(|| {
            turn_sources::inspect(&f.repo, "codex", SESSION, LIFE, &f.runtime)
        });
        barrier.recheck().unwrap();
        result
    };
    write(
        &root.join("operation-result.json"),
        turn_sources::encoded(&match result {
            Ok(value) => value,
            Err(error) => object([("error", string(&error.to_string()))]),
        })
        .unwrap(),
        0o600,
    );
}
fn signaled_owned_child(root: &Path, child: OperationChild) {
    let pid = std::fs::read_to_string(root.join("operation-pid")).unwrap();
    command(root, &["/bin/kill".into(), "-INT".into(), pid.into()]);
    assert_eq!(
        child.wait().status,
        agent_memory::process::Status::Signal(2)
    );
}
// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_race_remains_pending_for_normal_and_ordinary_failure
#[test]
#[ignore = "recovery_frontier"]
fn public_compatibility_absence_race_remains_pending_for_normal_and_ordinary_failure() {
    scenarios(
        "public_compatibility_absence_race_remains_pending_for_normal_and_ordinary_failure",
        &["normal", "error"],
        |root, mode| {
            let f = fixture_without_native_activation(root);
            let base = boundary::base(&f.repo).unwrap();
            turn_sources::private_mkdir(&base).unwrap();
            let lock = base.join("source-barrier.lock");
            assert!(!lock.exists());
            write(&root.join("case-request.json"), b"{}", 0o600);
            let before = "frontier.observe-bank.before-read";
            let after = "frontier.observe-bank.after-read";
            let child = operation_child(
                root,
                Some(&format!("{before},{after}")),
                if mode == "error" {
                    "pause,error"
                } else {
                    "pause,pause"
                },
                "compatibility",
            );
            ready(root, before);
            write(&lock, b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n", 0o600);
            let evidence = physical_snapshot(&base);
            release(root, before);
            if mode == "normal" {
                ready(root, after);
                release(root, after);
            }
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            let result = registry_result(root);
            assert_eq!(text(&result, "decision"), "pending");
            assert_eq!(get(&result, "unknown_state"), &Value::Bool(true));
            assert_eq!(
                get(&result, "private_evidence_preserved"),
                &Value::Bool(true)
            );
            assert_eq!(physical_snapshot(&base), evidence);
        },
    );
}
// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_preserves_interrupts_and_fatal_failures
#[test]
#[ignore = "recovery_frontier"]
fn public_compatibility_preserves_real_sigint_and_exit_with_raced_evidence() {
    scenarios(
        "public_compatibility_preserves_real_sigint_and_exit_with_raced_evidence",
        &["sigint", "exit"],
        |root, mode| {
            let f = fixture_without_native_activation(root);
            let base = boundary::base(&f.repo).unwrap();
            turn_sources::private_mkdir(&base).unwrap();
            let lock = base.join("source-barrier.lock");
            assert!(!lock.exists());
            write(&root.join("case-request.json"), b"{}", 0o600);
            let before = "frontier.observe-bank.before-read";
            let after = "frontier.observe-bank.after-read";
            let child = operation_child(
                root,
                Some(&format!("{before},{after}")),
                if mode == "exit" {
                    "pause,exit"
                } else {
                    "pause,pause"
                },
                "compatibility",
            );
            ready(root, before);
            write(&lock, b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n", 0o600);
            let evidence = physical_snapshot(&base);
            if mode == "sigint" {
                signaled_owned_child(root, child);
            } else {
                release(root, before);
                assert_eq!(child.wait().status, agent_memory::process::Status::Code(86));
            }
            assert_eq!(physical_snapshot(&base), evidence);
            turn_sources::private_lock(&lock, false, true)
                .unwrap()
                .finish()
                .unwrap();
        },
    );
}
// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_source_inspection_preserves_interrupts_with_raced_completion_lock
#[test]
#[ignore = "recovery_frontier"]
fn public_source_inspection_preserves_real_fatal_exits_and_refuses_raced_completion_lock() {
    scenarios(
        "public_source_inspection_preserves_real_fatal_exits_and_refuses_raced_completion_lock",
        &["normal", "error", "sigint", "exit"],
        |root, mode| {
            let f = Fixture::new(root);
            turn_sources::register(
                &f.repo,
                "codex",
                SESSION,
                LIFE,
                &"0".repeat(64),
                None,
                None,
                None,
                &f.runtime,
            )
            .unwrap();
            let ns =
                turn_sources::private_namespace(&f.repo, "codex", SESSION, LIFE, false, &f.runtime)
                    .unwrap()
                    .0;
            let lock = ns.directory.join("completion.lock");
            if lock.exists() {
                std::fs::remove_file(&lock).unwrap();
            }
            write(&root.join("case-request.json"), b"{}", 0o600);
            let before = "enrollment.inventory.before-read";
            let after = "enrollment.inventory.after-read";
            let modes = match mode {
                "error" => "pause,error",
                "exit" => "pause,exit",
                _ => "pause,pause",
            };
            let child = operation_child(
                root,
                Some(&format!("{before},{after}")),
                modes,
                "source-inspect",
            );
            ready(root, before);
            write(&lock, b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n", 0o600);
            let evidence = physical_snapshot(&ns.directory);
            if mode == "sigint" {
                signaled_owned_child(root, child);
            } else {
                release(root, before);
                if mode == "normal" {
                    ready(root, after);
                    release(root, after);
                }
                assert_eq!(
                    child.wait().status,
                    agent_memory::process::Status::Code(if mode == "exit" { 86 } else { 0 })
                );
                if mode != "exit" {
                    assert!(registry_result(root).get("error").is_some());
                }
            }
            assert_eq!(physical_snapshot(&ns.directory), evidence);
            turn_sources::private_lock(
                &ns.base.parent().unwrap().join("source-barrier.lock"),
                false,
                true,
            )
            .unwrap()
            .finish()
            .unwrap();
        },
    );
}
// frontier obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_unchanged_absence_allows_normal_exit_and_preserves_ordinary_error
#[test]
#[ignore = "recovery_frontier"]
fn unchanged_private_absence_allows_success_and_preserves_original_ordinary_error() {
    scenarios(
        "unchanged_private_absence_allows_success_and_preserves_original_ordinary_error",
        &["native", "completion", "transport"],
        |root, family| {
            if family == "transport" {
                let f = fixture_without_native_activation(root);
                let base = boundary::base(&f.repo).unwrap();
                let path = base.join("omp-transport.lock");
                assert!(!path.exists());
                let before = snapshot(&base);
                boundary::drain_source_capabilities(&f.repo, true, true, &f.runtime).unwrap();
                assert!(!path.exists());
                assert_eq!(snapshot(&base), before);
                write(&root.join("case-request.json"), b"{}", 0o600);
                let stage = "omp.drain.before-inventory";
                let child = operation_child(root, Some(stage), "error", "drain");
                assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
                assert_eq!(
                    text(&registry_result(root), "error"),
                    format!("Injected write failure at {stage}")
                );
                assert!(!path.exists());
                assert_eq!(snapshot(&base), before);
            } else {
                let (repo, base, runtime) = absence_fixture(root);
                let mut absent = None;
                absence_guard(family, &repo, &base, &runtime, |path| {
                    assert!(!path.exists());
                    absent = Some(path.to_path_buf());
                    Ok(())
                })
                .unwrap();
                assert!(!absent.unwrap().exists());
                let original =
                    agent_memory::error::ContractError::new("ordinary error without a race");
                let error = absence_guard(family, &repo, &base, &runtime, |path| {
                    assert!(!path.exists());
                    Err::<(), _>(original.clone())
                })
                .unwrap_err();
                assert_eq!(error, original);
            }
        },
    );
}

fn error_value(error: &agent_memory::error::ContractError) -> Value {
    object([
        ("error", string(&error.to_string())),
        (
            "busy",
            Value::Bool(matches!(
                error.kind(),
                agent_memory::error::Kind::SourceTransportBusy { .. }
            )),
        ),
        (
            "initializing",
            Value::Bool(matches!(
                error.kind(),
                agent_memory::error::Kind::SourceTransportBusy { initializing: true }
                    | agent_memory::error::Kind::CanonicalWriterInitializing
            )),
        ),
    ])
}
fn wait_path_for(path: &Path, seconds: u64) {
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(seconds)).unwrap();
    while !path.exists() {
        assert!(
            !deadline.expired(),
            "Owned operation did not reach {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn prepare_legacy_writer(root: &Path, f: &Fixture, transaction: bool) {
    let bank = f.repo.join("memory-bank");
    write(
        &root.join("registry-selected-path"),
        bank.join(".agent-memory.lock").to_str().unwrap(),
        0o600,
    );
    let file = bank.join("projectbrief.md");
    let content = format!(
        "{}\nCanonical legacy writer fixture update.\n",
        std::fs::read_to_string(&file).unwrap()
    );
    write(
        &root.join("registry-canonical-write.json"),
        turn_sources::encoded(&object([
            ("path", string(file.to_str().unwrap())),
            ("content", string(&content)),
        ]))
        .unwrap(),
        0o600,
    );
    if transaction {
        let mut files = agent_memory::bank_transactions::files_at(&bank).unwrap();
        set(&mut files, "projectbrief.md", string(&content));
        let prepared =
            agent_memory::bank_transactions::preview(&f.repo, &files, "replace").unwrap();
        let proposal = agent_memory::bank_transactions::proposal_value(&prepared).unwrap();
        write(
            &root.join("registry-transaction-bytes"),
            agent_memory::bank_transactions::marker_bytes(proposal).unwrap(),
            0o600,
        );
        write(
            &root.join("registry-operation-id"),
            text(proposal, "operation_id"),
            0o600,
        );
        write(
            &root.join("registry-operation-name"),
            b"bank-replace",
            0o600,
        );
    }
}
fn assert_retained_human(f: &Fixture, native: &str) {
    let rows = f
        .pending()
        .into_iter()
        .filter(|r| r.get("observation_id").is_some())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let source = transport::source(
        &f.repo,
        &f.runtime,
        text(&rows[0], "session"),
        text(&rows[0], "lifecycle"),
        text(&rows[0], "observation_id"),
    )
    .unwrap();
    assert_eq!(text(get(&source, "source"), "native_entry_id"), native);
    assert_eq!(
        get(&source, "source").get("content"),
        Some(&string("Only ephemeral arithmetic."))
    );
}
fn finite_writer_outcome(root: &Path, window: &str, failure: &str, transaction: bool) {
    let f = Fixture::new(root);
    prepare_legacy_writer(root, &f, transaction);
    let value = request(&f, &[human("native-a")], "ingest");
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&value).unwrap(),
        0o600,
    );
    if failure.is_empty() {
        write(&root.join("case-close-first"), b"", 0o600);
    } else {
        write(&root.join("case-close-after-release"), b"", 0o600);
    }
    let before = "omp.outcome.before-guard";
    let busy = "omp.metadata.after-busy";
    let observer = operation_child(root, Some(&format!("{before},{busy}")), "pause", "finite");
    ready(root, before);
    let writer_stage = match window {
        "pre-ex" => Some("registry.exclusive.before-first-ex"),
        "owner-fsync" => Some("registry.exclusive.after-owner-fsync"),
        _ => None,
    };
    let writer = if window == "unheld-empty" {
        write(&registry_path(root), b"", 0o600);
        None
    } else {
        Some(start_registry_writer(root, writer_stage))
    };
    let owner = std::fs::read(registry_path(root)).unwrap();
    release(root, before);
    ready(root, busy);
    let directory = child_directory(root);
    assert!(directory.join("started/1.json").is_file());
    assert!(!directory.join("outcomes/1.json").exists());
    assert_eq!(std::fs::read(registry_path(root)).unwrap(), owner);
    let error = agent_memory::local_registry::check_canonical_writer(
        &registry_path(root),
        agent_memory::bank_writes::OPERATIONS,
    )
    .unwrap_err();
    if window == "pre-ex" || window == "unheld-empty" {
        assert_eq!(owner, b"");
        assert!(matches!(
            error,
            agent_memory::local_registry::CanonicalWriterError::Initializing
        ));
    } else {
        assert!(matches!(
            error,
            agent_memory::local_registry::CanonicalWriterError::Busy
        ));
    }
    if transaction {
        let marker = transport::read(
            &f.repo.join("memory-bank/.agent-memory-transaction.md"),
            65536,
        )
        .unwrap();
        let owner = json::decode_bytes(&owner, Default::default()).unwrap();
        assert_eq!(
            text(&marker, "transaction_id"),
            text(&owner, "operation_id")
        );
    }
    assert!(turn_sources::source_barrier(&f.repo, true, true, None, &f.runtime).is_err());
    turn_sources::private_lock(&directory.join("stream.lock"), false, true)
        .unwrap()
        .finish()
        .unwrap();
    let exhausted = failure == "exhaustion";
    if exhausted && window == "pre-ex" {
        signal_registry(root, "-STOP");
    }
    let marker_path = f.repo.join("memory-bank/.agent-memory-transaction.md");
    let saved_marker = transaction.then(|| std::fs::read(&marker_path).unwrap());
    let router = f.repo.join("memory-bank/MEMORY.md");
    let original_router = std::fs::read_to_string(&router).unwrap();
    let mut writer = writer;
    if !exhausted {
        if let Some(writer) = writer.take() {
            if let Some(stage) = writer_stage {
                release(root, stage);
            }
            finish_registry_writer(root, writer);
        } else {
            std::fs::remove_file(registry_path(root)).unwrap();
        }
        if failure == "unknown-marker" {
            write(&marker_path, b"{\"schema\":\"unknown-transaction\"}", 0o600);
        } else if failure == "left-marker" {
            write(&marker_path, saved_marker.as_ref().unwrap(), 0o600);
        } else if failure == "identity" {
            let context =
                agent_memory::scope_binding::private_enrollment_context(&f.repo, &f.runtime)
                    .unwrap();
            write(
                &router,
                original_router.replace(context.project_id().as_str().unwrap(), &uid()),
                0o644,
            );
        }
    }
    release(root, busy);
    wait_path_for(&root.join("execute-finished"), 35);
    let result = registry_result(root);
    if exhausted {
        assert!(result.get("error").is_some());
        assert_eq!(get(&result, "busy"), &Value::Bool(true));
        assert!(!directory.join("outcomes/1.json").exists());
        no_verified_response(root);
        if let Some(writer) = writer {
            if let Some(stage) = writer_stage {
                release(root, stage);
                if window == "pre-ex" {
                    signal_registry(root, "-CONT");
                }
            }
            finish_registry_writer(root, writer);
        } else {
            std::fs::remove_file(registry_path(root)).unwrap();
        }
    } else if !failure.is_empty() {
        assert!(result.get("error").is_some());
        assert_eq!(get(&result, "busy"), &Value::Bool(false));
        assert!(!directory.join("outcomes/1.json").exists());
    }
    if failure == "unknown-marker" || failure == "left-marker" {
        assert!(marker_path.is_file());
        std::fs::remove_file(&marker_path).unwrap();
    }
    if failure == "identity" {
        write(&router, original_router, 0o644);
    }
    if !failure.is_empty() {
        write(&root.join("allow-close"), b"", 0o600);
    }
    assert_eq!(
        observer.wait().status,
        agent_memory::process::Status::Code(0)
    );
    assert_retained_human(&f, "native-a");
    assert_eq!(
        transport::files(
            &first_segment(&f).directory,
            "captures",
            transport::Names::Hex64
        )
        .unwrap()
        .len(),
        1
    );
    if failure.is_empty() {
        assert_eq!(
            text(
                &transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap(),
                "outcome"
            ),
            "verified"
        );
        assert!(directory.join("closed.json").is_file());
    } else {
        assert_eq!(
            transport::read(&root.join("close-result.json"), 4096).unwrap(),
            Value::Bool(false)
        );
        assert!(!directory.join("closed.json").exists());
    }
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source
#[test]
#[ignore = "recovery_frontier"]
fn writer_at_outcome_unwinds_and_closes_without_replaying_source() {
    isolated(
        "writer_at_outcome_unwinds_and_closes_without_replaying_source",
        |root| finite_writer_outcome(root, "protected", "", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_actual_transaction_at_outcome_waits_without_adopting_its_marker
#[test]
#[ignore = "recovery_frontier"]
fn transaction_at_outcome_waits_without_adopting_real_marker() {
    isolated(
        "transaction_at_outcome_waits_without_adopting_real_marker",
        |root| finite_writer_outcome(root, "protected", "", true),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_real_writer_pre_ex_empty_publication_defers_without_stealing_its_lock
#[test]
#[ignore = "recovery_frontier"]
fn pre_ex_empty_publication_defers_without_stealing_real_writer_slot() {
    isolated(
        "pre_ex_empty_publication_defers_without_stealing_real_writer_slot",
        |root| finite_writer_outcome(root, "pre-ex", "", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_real_writer_full_owner_fsync_window_has_ex_ownership_before_probe
#[test]
#[ignore = "recovery_frontier"]
fn full_owner_fsync_window_has_actual_ex_ownership_before_probe() {
    isolated(
        "full_owner_fsync_window_has_actual_ex_ownership_before_probe",
        |root| finite_writer_outcome(root, "owner-fsync", "", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_metadata_exhaustion_keeps_source_and_open_unknown
#[test]
#[ignore = "recovery_frontier"]
fn actual_writer_metadata_exhaustion_preserves_source_and_open_unknown() {
    isolated(
        "actual_writer_metadata_exhaustion_preserves_source_and_open_unknown",
        |root| finite_writer_outcome(root, "protected", "exhaustion", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_real_initializing_writer_exhaustion_cannot_verify_or_close
#[test]
#[ignore = "recovery_frontier"]
fn initializing_writer_real_deadline_cannot_verify_or_close() {
    isolated(
        "initializing_writer_real_deadline_cannot_verify_or_close",
        |root| finite_writer_outcome(root, "pre-ex", "exhaustion", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_empty_publication_exhaustion_retains_source_without_outcome
#[test]
#[ignore = "recovery_frontier"]
fn unheld_empty_publication_real_deadline_retains_source_without_outcome() {
    isolated(
        "unheld_empty_publication_real_deadline_retains_source_without_outcome",
        |root| finite_writer_outcome(root, "unheld-empty", "exhaustion", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_released_writer_with_unknown_transaction_is_hard_refusal_not_retry
#[test]
#[ignore = "recovery_frontier"]
fn released_writer_unknown_transaction_is_not_retryable() {
    isolated(
        "released_writer_unknown_transaction_is_not_retryable",
        |root| finite_writer_outcome(root, "protected", "unknown-marker", false),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_transaction_left_after_writer_release_remains_interruption
#[test]
#[ignore = "recovery_frontier"]
fn released_transaction_retained_marker_remains_interruption() {
    isolated(
        "released_transaction_retained_marker_remains_interruption",
        |root| finite_writer_outcome(root, "protected", "left-marker", true),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_writer_release_rediscovers_identity_instead_of_reusing_frame_scope
#[test]
#[ignore = "recovery_frontier"]
fn writer_release_freshly_rediscovers_changed_project_identity() {
    isolated(
        "writer_release_freshly_rediscovers_changed_project_identity",
        |root| finite_writer_outcome(root, "protected", "identity", false),
    );
}

fn busy_source_outcome(root: &Path, after_write: bool, exhausted: bool) {
    let f = Fixture::new(root);
    prepare_legacy_writer(root, &f, false);
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&armed_request(&f)).unwrap(),
        0o600,
    );
    let stage = if after_write {
        "omp.outcome.after-commit"
    } else {
        "omp.outcome.before-guard"
    };
    let busy = "omp.metadata.after-busy";
    let selector = format!("{stage},{busy}");
    let observer = operation_child(root, Some(&selector), "pause", "finite");
    ready(root, stage);
    let writer = start_registry_writer(root, None);
    let directory = child_directory(root);
    let retained = after_write.then(|| std::fs::read(directory.join("outcomes/1.json")).unwrap());
    release(root, stage);
    ready(root, busy);
    turn_sources::private_lock(
        &boundary::base(&f.repo).unwrap().join("omp-transport.lock"),
        false,
        true,
    )
    .unwrap()
    .finish()
    .unwrap();
    if !exhausted {
        finish_registry_writer(root, writer);
        release(root, busy);
        assert_eq!(
            observer.wait().status,
            agent_memory::process::Status::Code(0)
        );
    } else {
        release(root, busy);
        wait_path_for(&root.join("execute-finished"), 35);
        let result = registry_result(root);
        assert_eq!(get(&result, "busy"), &Value::Bool(true));
        assert_eq!(
            observer.wait().status,
            agent_memory::process::Status::Code(0)
        );
        finish_registry_writer(root, writer);
    }
    let outcome =
        transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap();
    assert_eq!(text(&outcome, "outcome"), "verified");
    if let Some(retained) = retained {
        assert_eq!(
            std::fs::read(directory.join("outcomes/1.json")).unwrap(),
            retained
        );
    }
    assert_eq!(
        std::fs::read_dir(directory.join("outcomes"))
            .unwrap()
            .count(),
        1
    );
    assert!(root_wakes(&f).is_empty());
    let s = first_segment(&f);
    assert_eq!(
        transport::files(&s.directory, "captures", transport::Names::Hex64)
            .unwrap()
            .len(),
        2
    );
    let rows = f.pending();
    assert_eq!(rows.len(), 2);
    if exhausted {
        no_verified_response(root);
        assert!(rows.iter().all(|r| r.get("observation_id").is_none()));
        assert!(!directory.join("closed.json").exists());
    } else {
        assert_eq!(text(&child_result(root), "status"), "pending-wake-busy");
        assert!(rows.iter().all(|r| r.get("observation_id").is_some()));
    }
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_busy_outcome_beforewrite_or_exit_retries_only_identical_source_after_unlock
#[test]
#[ignore = "recovery_frontier"]
fn busy_source_outcome_retries_exact_immutable_source_after_transport_unlock() {
    scenarios(
        "busy_source_outcome_retries_exact_immutable_source_after_transport_unlock",
        &["before", "after"],
        |root, mode| busy_source_outcome(root, mode == "after", false),
    );
}
// frontier obligation: tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_busy_outcome_exit_exhaustion_keeps_retained_record_without_response_or_drain
#[test]
#[ignore = "recovery_frontier"]
fn busy_source_outcome_exit_real_deadline_keeps_record_without_response_or_drain() {
    isolated(
        "busy_source_outcome_exit_real_deadline_keeps_record_without_response_or_drain",
        |root| busy_source_outcome(root, true, true),
    );
}
// frontier obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_real_io_failure_does_not_manufacture_invalid_request_refusal
#[test]
#[ignore = "recovery_frontier"]
fn accepted_attempt_publication_failure_is_not_an_invalid_request_refusal() {
    isolated(
        "accepted_attempt_publication_failure_is_not_an_invalid_request_refusal",
        |root| {
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request(&f, &[human("human")], "ingest")).unwrap(),
                0o600,
            );
            let child = operation_child(root, Some("omp.attempt.after-retain"), "error", "finite");
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(registry_result(root).get("error").is_some());
            let directory = child_directory(root);
            let attempt =
                transport::read(&directory.join("attempts/1.json"), stream::MAX_METADATA).unwrap();
            assert_eq!(text(&attempt, "disposition"), "accepted");
            assert!(!directory.join("outcomes/1.json").exists());
            assert!(!directory.join("started/1.json").exists());
            assert!(
                transport::read(&root.join("responses.json"), 1024 * 1024)
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
        },
    );
}
// frontier obligation: tests/contracts/test_omp_finite_stream.py::MetadataBarrierBoundaries.test_run_final_close_rejects_barrier_inode_replacement_after_metadata_write
#[test]
#[ignore = "recovery_frontier"]
fn final_close_rejects_actual_source_barrier_swap_after_retaining_closed_record() {
    isolated(
        "final_close_rejects_actual_source_barrier_swap_after_retaining_closed_record",
        |root| {
            use std::os::unix::fs::MetadataExt;
            let f = Fixture::new(root);
            write(
                &root.join("case-request.json"),
                turn_sources::encoded(&request(&f, &[human("human"), answer("answer")], "ingest"))
                    .unwrap(),
                0o600,
            );
            let stage = "omp.close.after-retain";
            let child = operation_child(root, Some(stage), "pause", "finite-run");
            ready(root, stage);
            let directory = child_directory(root);
            let base = boundary::base(&f.repo).unwrap();
            let path = base.join("source-barrier.lock");
            let old = base.join("source-barrier.final-held");
            assert!(directory.join("closed.json").is_file());
            std::fs::rename(&path, &old).unwrap();
            write(&path, b"", 0o600);
            release(root, stage);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(1));
            assert_eq!(
                text(
                    &transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA)
                        .unwrap(),
                    "outcome"
                ),
                "verified"
            );
            assert!(directory.join("closed.json").is_file());
            assert_ne!(
                std::fs::metadata(&old).unwrap().ino(),
                std::fs::metadata(&path).unwrap().ino()
            );
            std::fs::remove_file(&path).unwrap();
            std::fs::rename(old, path).unwrap();
        },
    );
}

fn signal_registry(root: &Path, signal: &str) {
    let pid = std::fs::read_to_string(root.join("registry-pid")).unwrap();
    command(root, &["/bin/kill".into(), signal.into(), pid.into()]);
}
// frontier obligation: tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_startup_typed_busy_retries_before_open_and_ready
#[test]
#[ignore = "recovery_frontier"]
fn startup_retries_actual_canonical_busy_before_publishing_open_or_ready() {
    isolated(
        "startup_retries_actual_canonical_busy_before_publishing_open_or_ready",
        |root| {
            let f = Fixture::new(root);
            prepare_legacy_writer(root, &f, false);
            let writer = start_registry_writer(root, None);
            write(&root.join("case-request.json"), b"{}", 0o600);
            write(&root.join("case-empty-run"), b"", 0o600);
            let child = operation_child(root, None, "error", "finite-run");
            wait_path(&root.join("startup-busy"));
            let directory = child_directory(root);
            assert!(!directory.join("open.json").exists());
            assert!(
                std::fs::read(root.join("finite-response-wire"))
                    .unwrap()
                    .is_empty()
            );
            finish_registry_writer(root, writer);
            assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
            assert!(directory.join("open.json").is_file());
            assert!(directory.join("closed.json").is_file());
            let frames = response_frames(&root.join("finite-response-wire"));
            assert!(frames.iter().any(|(kind, _, _)| kind == "READY"));
            assert!(frames.iter().all(|(kind, _, _)| kind != "STARTED"));
        },
    );
}
fn invalid_attempt_busy_retry(root: &Path, kind: &str, corruption: &str) {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new(root);
    prepare_legacy_writer(root, &f, false);
    let value = request(&f, &[human("native-a")], "ingest");
    write(
        &root.join("case-request.json"),
        turn_sources::encoded(&value).unwrap(),
        0o600,
    );
    if corruption.is_empty() {
        write(
            &root.join("case-followup.json"),
            turn_sources::encoded(&value).unwrap(),
            0o600,
        );
    }
    let stage = "omp.attempt.after-retain";
    let busy = "omp.metadata.after-busy";
    let child = operation_child(
        root,
        Some(&format!("{stage},{busy}")),
        "pause",
        if kind == "REQUEST" {
            "finite-invalid-request"
        } else {
            "finite-invalid-refused"
        },
    );
    ready(root, stage);
    let directory = child_directory(root);
    let path = directory.join("attempts/1.json");
    let saved = std::fs::read(&path).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let writer = start_registry_writer(root, None);
    release(root, stage);
    ready(root, busy);
    assert!(!directory.join("started/1.json").exists());
    assert!(!directory.join("outcomes/1.json").exists());
    assert!(transport::segments(&f.site(), SESSION).unwrap().is_empty());
    turn_sources::private_lock(&directory.join("stream.lock"), false, true)
        .unwrap()
        .finish()
        .unwrap();
    assert!(turn_sources::source_barrier(&f.repo, true, true, None, &f.runtime).is_err());
    finish_registry_writer(root, writer);
    if corruption == "partial" {
        write(&path, b"{", 0o600);
    } else if corruption == "different" {
        let mut changed = json::decode_bytes(&saved, Default::default()).unwrap();
        set(&mut changed, "event_ordinal", Value::Int(1u8.into()));
        write(&path, turn_sources::encoded(&changed).unwrap(), 0o600);
    }
    let expected = std::fs::read(&path).unwrap();
    release(root, busy);
    assert_eq!(child.wait().status, agent_memory::process::Status::Code(0));
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    let frames = response_frames(&root.join("responses.raw"));
    if !corruption.is_empty() {
        assert_eq!(get(&registry_result(root), "busy"), &Value::Bool(false));
        assert!(registry_result(root).get("error").is_some());
        assert!(!directory.join("outcomes/1.json").exists());
        assert!(!directory.join("closed.json").exists());
        assert!(frames.is_empty());
    } else {
        let actual = std::fs::metadata(&path).unwrap();
        assert_eq!(
            (actual.ino(), actual.mtime_nsec(), actual.mtime()),
            (metadata.ino(), metadata.mtime_nsec(), metadata.mtime())
        );
        let first =
            transport::read(&directory.join("outcomes/1.json"), stream::MAX_METADATA).unwrap();
        assert_eq!(text(&first, "outcome"), "durable-refusal");
        assert_eq!(text(&first, "code"), "invalid-request");
        assert_eq!(
            frames
                .iter()
                .filter(|(kind, seq, _)| kind == "REFUSED" && *seq == 1)
                .count(),
            1
        );
        assert_eq!(
            frames
                .iter()
                .filter(|(kind, seq, _)| kind == "STARTED" && *seq == 2)
                .count(),
            1
        );
        let closed = transport::read(&directory.join("closed.json"), 4096).unwrap();
        assert_eq!(get(&closed, "last_attempt_seq"), &Value::Int(2u8.into()));
        assert_eq!(
            get(&closed, "refused_count"),
            &Value::Int(u8::from(kind == "REFUSED-ATTEMPT").into())
        );
        assert_eq!(get(&closed, "last_event_ordinal"), &Value::Int(1u8.into()));
        assert_retained_human(&f, "native-a");
    }
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_request_durable_attempt_exit_busy_resumes_exactly_once
#[test]
#[ignore = "recovery_frontier"]
fn invalid_request_busy_exit_resumes_exactly_once() {
    isolated("invalid_request_busy_exit_resumes_exactly_once", |root| {
        invalid_attempt_busy_retry(root, "REQUEST", "")
    });
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_refused_frame_durable_attempt_exit_busy_counts_once
#[test]
#[ignore = "recovery_frontier"]
fn invalid_refused_frame_busy_exit_counts_exactly_once() {
    isolated(
        "invalid_refused_frame_busy_exit_counts_exactly_once",
        |root| invalid_attempt_busy_retry(root, "REFUSED-ATTEMPT", ""),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_attempt_exit_busy_never_adopts_partial_retry_record
#[test]
#[ignore = "recovery_frontier"]
fn invalid_attempt_busy_exit_never_adopts_partial_record() {
    isolated(
        "invalid_attempt_busy_exit_never_adopts_partial_record",
        |root| invalid_attempt_busy_retry(root, "REQUEST", "partial"),
    );
}

// frontier obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_attempt_exit_busy_never_adopts_different_retry_record
#[test]
#[ignore = "recovery_frontier"]
fn invalid_attempt_busy_exit_never_adopts_different_record() {
    isolated(
        "invalid_attempt_busy_exit_never_adopts_different_record",
        |root| invalid_attempt_busy_retry(root, "REQUEST", "different"),
    );
}
