use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, Value};
use agent_memory::error::{ContractError, Result};
use agent_memory::{bank_writes, host, private_state as private, process};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
fn array(values: &[&str]) -> Value {
    Value::Array(values.iter().map(|value| s(*value)).collect())
}
fn g<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn refusal<T: std::fmt::Debug>(value: Result<T>, expected: &str) {
    assert_eq!(value.unwrap_err().message(), expected);
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("am-private-{}", host::uuid4().unwrap()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }
    fn directory(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        path
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
type Snapshot = BTreeMap<PathBuf, (u64, u64, u32, Option<Vec<u8>>, Option<PathBuf>)>;
fn snapshot(root: &Path) -> Snapshot {
    fn visit(root: &Path, result: &mut Snapshot) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let info = std::fs::symlink_metadata(&path).unwrap();
            let bytes = info.is_file().then(|| std::fs::read(&path).unwrap());
            let link = info
                .file_type()
                .is_symlink()
                .then(|| std::fs::read_link(&path).unwrap());
            result.insert(
                path.clone(),
                (info.dev(), info.ino(), info.mode(), bytes, link),
            );
            if info.is_dir() {
                visit(&path, result);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, &mut result);
    result
}

static OUTER_LAUNCH: std::sync::Mutex<()> = std::sync::Mutex::new(());
// Each filesystem lifecycle case executes with a private process environment,
// never set_var, current-directory mutation, network or the user's config.
fn isolated(name: &str, body: impl FnOnce(&Path)) {
    if std::env::var("AM_PRIVATE_CASE").ok().as_deref() == Some(name) {
        let root = PathBuf::from(std::env::var_os("AM_PRIVATE_ROOT").unwrap());
        body(&root);
        return;
    }
    let _launch = OUTER_LAUNCH
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = Root::new();
    for directory in ["home", "config", "state", "data", "runtime", "repo"] {
        root.directory(directory);
    }
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        name.into(),
        "--include-ignored".into(),
        "--nocapture".into(),
    ];
    let env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("HOME".into(), root.0.join("home").into_os_string()),
        (
            "XDG_CONFIG_HOME".into(),
            root.0.join("config").into_os_string(),
        ),
        (
            "XDG_STATE_HOME".into(),
            root.0.join("state").into_os_string(),
        ),
        ("XDG_DATA_HOME".into(), root.0.join("data").into_os_string()),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("AM_PRIVATE_CASE".into(), name.into()),
        ("AM_PRIVATE_ROOT".into(), root.0.as_os_str().to_owned()),
    ];
    let spec = process::Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&root.0),
        search_path: false,
        stdin: process::Input::Null,
        stdout: process::Target::Capture { limit: 1024 * 1024 },
        stderr: process::Target::Capture { limit: 1024 * 1024 },
        deadline: Deadline::from_absolute_ns(u64::MAX),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    let output = process::run_for(&spec, Duration::from_secs(30)).unwrap();
    assert_eq!(
        output.status,
        process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn target() -> Value {
    object([("private_state_capabilities", array(&[]))])
}
fn setup_bank(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let repo = root.join("repo");
    let bank = repo.join("memory-bank");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&bank)
        .unwrap();
    write(
        &bank.join("my-note.md"),
        b"Preserve ordinary routerless notes.\n",
    );
    let (operations, _) =
        bank_writes::state_paths(&repo, &bank, "00000000-0000-4000-8000-000000000000").unwrap();
    (repo, bank, operations.parent().unwrap().to_path_buf())
}

#[test]
fn declarations_are_exact_unique_and_finite() {
    assert!(private::compatibility(None).unwrap().is_empty());
    assert!(
        private::compatibility(Some(&object([])))
            .unwrap()
            .is_empty()
    );
    let declared = object([(
        "compatibility",
        object([("private_state_capabilities", array(private::TOKENS))]),
    )]);
    assert_eq!(
        private::compatibility(Some(&declared)).unwrap(),
        private::TOKENS
            .iter()
            .map(|value| value.to_string())
            .collect::<BTreeSet<_>>()
    );
    for tokens in [
        array(&["layout-frontier-v1", "layout-frontier-v1"]),
        array(&["future-private-v1"]),
        array(&[""]),
        Value::Null,
        s("layout-frontier-v1"),
        Value::Array(vec![Value::Bool(true)]),
    ] {
        refusal(
            private::compatibility(Some(&object([("private_state_capabilities", tokens)]))),
            "Target runtime has an unknown private-state compatibility declaration",
        );
    }
    for value in [Value::Null, Value::Bool(false), array(&[]), s("runtime")] {
        refusal(
            private::compatibility(Some(&value)),
            "Target runtime has an unknown private-state compatibility declaration",
        );
    }
}

#[test]
fn missing_checkout_is_not_a_link_or_inaccessible_path() {
    let root = Root::new();
    assert!(private::missing_path(&root.0.join("deleted/checkout")).unwrap());
    let existing = root.directory("checkout");
    assert!(!private::missing_path(&existing).unwrap());
    let link = root.0.join("linked");
    std::os::unix::fs::symlink(root.0.join("missing-target"), &link).unwrap();
    assert!(!private::missing_path(&link).unwrap());
    assert!(private::missing_path(&link.join("child")).is_err());
    let file = root.0.join("ordinary-file");
    write(&file, b"unchanged\n");
    assert!(!private::missing_path(&file).unwrap());
    assert!(private::missing_path(&file.join("checkout")).is_err());
    assert_eq!(std::fs::read(file).unwrap(), b"unchanged\n");
}

#[test]
fn public_summary_excludes_every_private_locator_and_pin() {
    let proof = object([
        ("decision", s("pending")),
        ("required_capabilities", array(&["layout-frontier-v1"])),
        ("target_capabilities", array(&[])),
        ("missing_capabilities", array(&["layout-frontier-v1"])),
        (
            "requirements",
            object([("/private/repo/bank", array(&["layout-frontier-v1"]))]),
        ),
        (
            "unknown_state",
            Value::Array(vec![object([("reason", s("PRIVATE-REASON"))])]),
        ),
        ("evidence_pins", object([("digest", s("PRIVATE-DIGEST"))])),
    ]);
    let summary = private::public_summary(&proof).unwrap();
    let expected = "{\"decision\": \"pending\", \"required_capabilities\": [\"layout-frontier-v1\"], \"target_capabilities\": [], \"missing_capabilities\": [\"layout-frontier-v1\"], \"observed_banks\": 1, \"unknown_private_states\": 1}";
    assert_eq!(json::encode(&summary, json::DEFAULT).unwrap(), expected);
    refusal(
        private::public_summary(&object([])),
        "Private-state summary is malformed",
    );
}

#[test]
fn native_absence_observation_is_readonly() {
    let root = Root::new();
    let base = root.directory("private-base");
    let before = snapshot(&root.0);
    // The real native registry reader runs; any outcome leaves the world unchanged.
    let _ = private::native_absence(&root.0, &base, &root.0.join("runtime"));
    assert_eq!(snapshot(&root.0), before);
}

#[test]
fn present_native_barrier_is_not_mistaken_for_absence_or_rewritten() {
    let root = Root::new();
    let base = root.directory("private-base");
    let lock = base.join("source-barrier.lock");
    write(&lock, b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n");
    let before = snapshot(&root.0);
    refusal(
        private::native_absence(&root.0, &base, &root.0.join("runtime")),
        "Native source barrier appeared during observation",
    );
    assert_eq!(snapshot(&root.0), before);
}

#[test]
fn lifecycle_paths_and_preserved_reader_are_required_before_hook_access() {
    let uninstall = object([("operation", s("uninstall"))]);
    assert_eq!(
        private::prepare_lifecycle(&uninstall, false, Path::new("/runtime")).unwrap(),
        None
    );
    for supplied in [
        s("/private/renewal.json"),
        Value::Array(vec![Value::Int(1.into())]),
        Value::Array(vec![s("/renewal"); 1025]),
    ] {
        let plan = object([
            ("operation", s("uninstall")),
            ("layout_quiescence", supplied),
        ]);
        refusal(
            private::prepare_lifecycle(&plan, true, Path::new("/runtime")),
            "Lifecycle quiescence inputs must be explicit protected-file paths",
        );
    }
    let plan = object([
        ("operation", s("uninstall")),
        ("layout_quiescence", array(&["/private/renewal.json"])),
    ]);
    refusal(
        private::prepare_lifecycle(&plan, false, Path::new("/runtime")),
        "Uninstall renewal needs an actually preserved capable reader",
    );
}

// obligation: tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_layout_first_native_activation_requires_reviewed_new_routes_without_omp_preseed
#[test]
fn lifecycle_selects_exact_changed_invocation_routes_without_publishing() {
    let kinds = [
        "wrapper",
        "hook-link",
        "hook-config",
        "copy",
        "link",
        "activation",
        "source",
    ];
    let mut actions: Vec<_> = kinds
        .iter()
        .map(|kind| {
            object([
                ("kind", s(*kind)),
                ("changed", Value::Bool(true)),
                ("after", object([])),
                ("path", s(format!("/routes/{kind}"))),
            ])
        })
        .collect();
    actions.push(object([
        ("kind", s("wrapper")),
        ("changed", Value::Bool(false)),
        ("after", object([])),
        ("path", s("/unchanged")),
    ]));
    actions.push(object([
        ("kind", s("copy")),
        ("changed", Value::Bool(true)),
        ("after", Value::Null),
        ("path", s("/removed")),
    ]));
    let source = object([("artifact_digest", s("a".repeat(64)))]);
    let plan = object([
        ("operation", s("install")),
        ("source", source.clone()),
        ("actions", Value::Array(actions)),
    ]);
    let checked = private::prepare_lifecycle(&plan, true, Path::new("/runtime"))
        .unwrap()
        .unwrap();
    assert_eq!(
        g(&checked, "layout_required_routes"),
        &array(&[
            "/routes/wrapper",
            "/routes/hook-link",
            "/routes/hook-config",
            "/routes/copy",
            "/routes/link",
            "/routes/activation"
        ])
    );
    assert_eq!(g(&checked, "layout_quiescence_overrides"), &object([]));
    let applied = private::prepare_lifecycle(&plan, false, Path::new("/runtime"))
        .unwrap()
        .unwrap();
    assert!(applied.get("layout_quiescence_overrides").is_none());
    assert_eq!(g(&plan, "source"), &source);
    let preserved = object([
        ("artifact_digest", s("b".repeat(64))),
        (
            "reader_invocation_routes",
            array(&["/reader/a", "/reader/a", "/reader/b"]),
        ),
    ]);
    let uninstall = object([
        ("operation", s("uninstall")),
        ("preserved_private_state_reader", preserved),
        ("source", source),
    ]);
    let result = private::prepare_lifecycle(&uninstall, true, Path::new("/runtime"))
        .unwrap()
        .unwrap();
    assert_eq!(
        g(&result, "layout_required_routes"),
        &array(&["/reader/a", "/reader/a", "/reader/b"])
    );
    assert_eq!(g(&result, "artifact_digest"), &s("b".repeat(64)));
}

#[test]
fn renewal_envelope_has_exact_attestation_and_independent_review_keys() {
    for renewal in [
        Value::Null,
        object([]),
        object([("attestation", object([]))]),
        object([
            ("attestation", object([])),
            ("review", object([])),
            ("extra", s("not-authority")),
        ]),
    ] {
        let plan = object([
            ("operation", s("update")),
            (
                "source",
                object([
                    ("artifact_digest", s("a".repeat(64))),
                    ("reader_invocation_routes", array(&["/reader"])),
                ]),
            ),
            (
                "native_layout_quiescence_overrides",
                Value::Array(vec![renewal]),
            ),
        ]);
        refusal(
            private::prepare_lifecycle(&plan, true, Path::new("/runtime")),
            "Lifecycle renewal uses the exact attestation/independent-review envelope",
        );
        let applied = private::prepare_lifecycle(&plan, false, Path::new("/runtime"))
            .unwrap()
            .unwrap();
        assert!(applied.get("layout_quiescence_overrides").is_none());
    }
}

#[test]
fn valid_envelope_requires_the_real_quiescence_validator() {
    let renewal = object([
        (
            "attestation",
            object([
                ("operation_id", s("00000000-0000-4000-8000-000000000000")),
                ("repo_root", s("/private/repo")),
            ]),
        ),
        ("review", object([])),
    ]);
    let plan = object([
        ("operation", s("update")),
        (
            "source",
            object([
                ("artifact_digest", s("a".repeat(64))),
                ("reader_invocation_routes", array(&["/reader"])),
            ]),
        ),
        (
            "native_layout_quiescence_overrides",
            Value::Array(vec![renewal]),
        ),
    ]);
    refusal(
        private::prepare_lifecycle(&plan, true, Path::new("/runtime")),
        "recovery_frontier provider is not registered",
    );
}

#[test]
fn inventory_observation_is_readonly() {
    isolated("inventory_observation_is_readonly", |root| {
        let before = snapshot(root);
        // The real native registry and source-state readers run directly; any
        // outcome must leave the isolated world byte-identical.
        let _ = private::repository_inventory(None, &root.join("repo"), &root.join("runtime"));
        assert_eq!(snapshot(root), before);
    });
}

// These use the actual composition root only. No fake native inventory, frontier,
// admission, source barrier or runtime-capability provider is installed here.
// obligation: tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_routerless_notes_and_ordinary_receipts_remain_readonly_without_native_obligations
#[test]
#[ignore = "needs recovery_frontier registered (bank observation decides compatibility)"]
fn routerless_notes_and_receipts_remain_readonly() {
    isolated("routerless_notes_and_receipts_remain_readonly", |root| {
        agent_memory::cli::register_providers().unwrap();
        let (repo, bank, base) = setup_bank(root);
        for prior_receipt in [false, true] {
            if prior_receipt {
                agent_memory::fs::private_mkdir(&base.join("operations")).unwrap();
                write(
                    &base.join("operations/ordinary-owned-receipt.json"),
                    b"{\"retained\":\"ordinary\"}\n",
                );
            }
            let before = snapshot(root);
            let first = private::required_private_state_capabilities(
                None,
                Some(&target()),
                "install",
                &repo,
                &root.join("runtime"),
            )
            .unwrap();
            let second = private::required_private_state_capabilities(
                None,
                Some(&target()),
                "install",
                &repo,
                &root.join("runtime"),
            )
            .unwrap();
            assert_eq!(first, second);
            assert_eq!(g(&first, "decision"), &s("compatible"));
            assert_eq!(g(&first, "required_capabilities"), &array(&[]));
            assert_eq!(snapshot(root), before);
            assert!(!base.join("source-barrier.lock").exists());
            assert!(!bank.join("MEMORY.md").exists());
        }
    });
}

// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_rechecks_normal_and_ordinary_failures
#[test]
fn native_absence_rechecks_normal_and_ordinary_failures() {
    isolated(
        "native_absence_rechecks_normal_and_ordinary_failures",
        |root| {
            agent_memory::cli::register_providers().unwrap();
            let (repo, _, base) = setup_bank(root);
            for fail in [false, true] {
                let mut saved = None;
                let error =
                    private::observation_barrier(&repo, &root.join("runtime"), |_| -> Result<()> {
                        agent_memory::fs::private_mkdir(&base).unwrap();
                        write(
                            &base.join("source-barrier.lock"),
                            b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n",
                        );
                        saved = Some(snapshot(root));
                        if fail {
                            Err(ContractError::new("original body refusal"))
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_err();
                assert_eq!(
                    error.message(),
                    "Native source barrier appeared during observation"
                );
                assert_eq!(snapshot(root), saved.unwrap());
                std::fs::remove_file(base.join("source-barrier.lock")).unwrap();
            }
        },
    );
}

// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_preserves_interrupts_and_fatal_failures
#[test]
fn native_absence_preserves_original_unwind_and_raced_evidence() {
    isolated(
        "native_absence_preserves_original_unwind_and_raced_evidence",
        |root| {
            agent_memory::cli::register_providers().unwrap();
            let (repo, _, base) = setup_bank(root);
            let mut saved = None;
            let original = std::sync::Arc::new("original fatal unwind");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _: Result<()> =
                    private::observation_barrier(&repo, &root.join("runtime"), |_| {
                        agent_memory::fs::private_mkdir(&base).unwrap();
                        write(
                            &base.join("source-barrier.lock"),
                            b"RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n",
                        );
                        saved = Some(snapshot(root));
                        std::panic::panic_any(original.clone());
                    });
            }));
            let caught = result
                .unwrap_err()
                .downcast::<std::sync::Arc<&str>>()
                .unwrap();
            assert!(std::sync::Arc::ptr_eq(&caught, &original));
            assert_eq!(snapshot(root), saved.unwrap());
        },
    );
}

// obligation: tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_unchanged_absence_allows_normal_exit_and_preserves_ordinary_error
#[test]
fn unchanged_native_absence_preserves_ordinary_refusal() {
    isolated(
        "unchanged_native_absence_preserves_ordinary_refusal",
        |root| {
            agent_memory::cli::register_providers().unwrap();
            let (repo, _, base) = setup_bank(root);
            let before = snapshot(root);
            private::observation_barrier(&repo, &root.join("runtime"), |pin| {
                assert!(pin.is_some());
                Ok(())
            })
            .unwrap();
            refusal(
                private::observation_barrier(&repo, &root.join("runtime"), |_| -> Result<()> {
                    Err(ContractError::new("original ordinary refusal"))
                }),
                "original ordinary refusal",
            );
            assert!(!base.join("source-barrier.lock").exists());
            assert_eq!(snapshot(root), before);
        },
    );
}

// obligation: tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_saved_bank_binding_keeps_missing_repository_and_bank_required
#[test]
fn saved_missing_repository_and_bank_remain_required_and_block_activation() {
    isolated(
        "saved_missing_repository_and_bank_remain_required_and_block_activation",
        |root| {
            agent_memory::cli::register_providers().unwrap();
            let before = snapshot(root);
            for repo in [root.join("missing-checkout"), root.join("repo")] {
                for key in ["banks", "bank_bindings"] {
                    let installation = object([(
                        key,
                        Value::Array(vec![object([("repository", s(repo.to_str().unwrap()))])]),
                    )]);
                    let inventory = private::repository_inventory(
                        Some(&installation),
                        &root.join("repo"),
                        &root.join("runtime"),
                    )
                    .unwrap();
                    assert!(inventory.roots.contains(&repo));
                    assert!(
                        g(&inventory.pins, "required_bank_repositories")
                            .as_array()
                            .unwrap()
                            .contains(&s(repo.to_str().unwrap()))
                    );
                    let proof = private::required_private_state_capabilities(
                        Some(&installation),
                        Some(&target()),
                        "activate",
                        &root.join("repo"),
                        &root.join("runtime"),
                    )
                    .unwrap();
                    assert_eq!(g(&proof, "decision"), &s("pending"));
                    let mut activated = false;
                    assert!(
                        private::activation_guard::<_, ContractError>(
                            Some(&installation),
                            Some(&target()),
                            "activate",
                            &root.join("repo"),
                            &root.join("runtime"),
                            None,
                            |_| {
                                activated = true;
                                Ok(())
                            }
                        )
                        .is_err()
                    );
                    assert!(!activated);
                }
            }
            assert_eq!(snapshot(root), before);
        },
    );
}

#[test]
fn nested_activation_restores_context_after_unwind_and_exact_rechecks() {
    isolated(
        "nested_activation_restores_context_after_unwind_and_exact_rechecks",
        |root| {
            agent_memory::cli::register_providers().unwrap();
            let repo = root.join("repo");
            let runtime = root.join("runtime");
            let before = snapshot(root);
            let mut transitions = 0;
            private::activation_guard::<_, ContractError>(
                None,
                Some(&target()),
                "install",
                &repo,
                &runtime,
                None,
                |_| {
                    let mut transition = || {
                        transitions += 1;
                        Ok(None)
                    };
                    private::activation_guard::<_, ContractError>(
                        None,
                        Some(&target()),
                        "install",
                        &repo,
                        &runtime,
                        Some(&mut transition),
                        |_| Ok(()),
                    )
                },
            )
            .unwrap();
            assert_eq!(transitions, 1);
            let original = std::sync::Arc::new("activation interrupted");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _: Result<()> = private::activation_guard(
                    None,
                    Some(&target()),
                    "install",
                    &repo,
                    &runtime,
                    None,
                    |_| {
                        std::panic::panic_any(original.clone());
                    },
                );
            }));
            let caught = result
                .unwrap_err()
                .downcast::<std::sync::Arc<&str>>()
                .unwrap();
            assert!(std::sync::Arc::ptr_eq(&caught, &original));
            private::activation_guard::<_, ContractError>(
                None,
                Some(&target()),
                "install",
                &repo,
                &runtime,
                None,
                |_| Ok(()),
            )
            .unwrap();
            assert_eq!(snapshot(root), before);
        },
    );
}

// obligation: tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_lockless_orphan_native_or_malformed_sources_refuse_readonly
#[test]
fn lockless_native_names_and_malformed_shared_sources_refuse_readonly() {
    isolated(
        "lockless_native_names_and_malformed_shared_sources_refuse_readonly",
        |root| {
            agent_memory::cli::register_providers().unwrap();
            let (repo, _, base) = setup_bank(root);
            agent_memory::fs::private_mkdir(&base).unwrap();
            for name in [
                "turn-sources-compat-v1.json",
                "omp-source-v1",
                "omp-stream-v1",
                "omp-stream-session-locks",
                "turn-sources-v2",
            ] {
                let entry = base.join(name);
                if name.ends_with(".json") {
                    write(&entry, b"{incomplete");
                } else {
                    agent_memory::fs::private_mkdir(&entry).unwrap();
                    if name == "turn-sources-v2" {
                        let directory = entry.join("turns").join("a".repeat(64));
                        agent_memory::fs::private_mkdir(&directory).unwrap();
                        write(&directory.join("source-0.json"), b"{incomplete");
                    }
                }
                let before = snapshot(root);
                let mut entered = false;
                assert!(
                    private::observation_barrier(&repo, &root.join("runtime"), |_| {
                        entered = true;
                        Ok(())
                    })
                    .is_err()
                );
                assert!(!entered);
                assert_eq!(snapshot(root), before);
                assert!(!base.join("source-barrier.lock").exists());
                if entry.is_dir() {
                    std::fs::remove_dir_all(entry).unwrap();
                } else {
                    std::fs::remove_file(entry).unwrap();
                }
            }
        },
    );
}
