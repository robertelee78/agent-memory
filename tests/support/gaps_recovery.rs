use super::*;
use agent_memory::process::{Input, Spec, Status, Target};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use support::distribution::Channel;
fn values(v: &Value, key: &str) -> Vec<Value> {
    g(v, key).as_array().unwrap().to_vec()
}
fn library(f: &Fixture, action: &str, stage: Option<&str>) -> (support::Child, PathBuf) {
    let request = obj([
        ("repo", s(f.repo.to_str().unwrap())),
        ("runtime", s(f.runtime.to_str().unwrap())),
        ("action", s(action)),
    ]);
    let path = f.save(&request);
    let directory = f.world.root.join("gaps-observe-pause");
    let mut env = vec![("AGENT_MEMORY_FAULT_GAPS_RECOVERY".into(), path)];
    if let Some(stage) = stage {
        env.extend([
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                directory.clone().into_os_string(),
            ),
        ]);
    }
    let child = f
        .world
        .spawn_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "gaps_recovery::library_worker".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                env: &env,
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    if let Some(stage) = stage {
        let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(15)).unwrap();
        while !directory.join(format!("{stage}.ready")).exists() {
            assert!(!deadline.expired(), "missing {stage}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    (child, directory)
}
fn collect(child: support::Child) -> Value {
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(0), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    json::decode(
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("GAPS-RECOVERY:"))
            .unwrap(),
        Default::default(),
    )
    .unwrap()
}
#[test]
fn library_worker() {
    let Some(path) = std::env::var_os("AGENT_MEMORY_FAULT_GAPS_RECOVERY") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    let request = json::decode_bytes(&fs::read(path).unwrap(), Default::default()).unwrap();
    let repo = Path::new(t(&request, "repo"));
    let runtime = Path::new(t(&request, "runtime"));
    let result: agent_memory::error::Result<Value> = match t(&request, "action") {
        "held-settings" => (|| {
            let settings = agent_memory::repository_config::settings(repo)?;
            let recovery = agent_memory::repository_config::recovery_settings(repo)?;
            let context = agent_memory::scope_binding::private_enrollment_context(repo, runtime)?;
            assert!(context.held());
            assert_eq!(context.root(), repo);
            assert_eq!(settings.bank_root, recovery.bank_root);
            Ok(obj([
                ("bank_root", s(settings.bank_root)),
                ("held", Value::Bool(context.held())),
            ]))
        })(),
        "private" => (|| {
            let target = agent_memory::recovery_quiescence::measured_runtime(runtime)?;
            let installation = obj([(
                "observed_repositories",
                Value::Array(vec![s(repo.to_str().unwrap())]),
            )]);
            agent_memory::private_state::require_compatible(
                Some(&installation),
                Some(&target),
                "update",
                None,
                repo,
                runtime,
            )
        })(),
        _ => panic!("unknown recovery fixture operation"),
    };
    let value = match result {
        Ok(value) => obj([("ok", Value::Bool(true)), ("value", value)]),
        Err(error) => obj([("ok", Value::Bool(false)), ("error", s(error.to_string()))]),
    };
    println!(
        "GAPS-RECOVERY:{}",
        json::encode(&value, json::DEFAULT).unwrap()
    );
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_verified_hold_remains_readonly_through_grouped_settings_and_enrollment
fn real_verified_hold_remains_readonly_through_grouped_settings_and_enrollment() {
    let f = Fixture::new();
    let before = (
        snapshot(&f.bank, false),
        snapshot(&f.directory, false),
        snapshot(&f.world.paths["config"], false),
        snapshot(&f.world.paths["state"], false),
    );
    let result = collect(library(&f, "held-settings", None).0);
    assert_eq!(g(&result, "ok"), &Value::Bool(true));
    assert_eq!(g(g(&result, "value"), "held"), &Value::Bool(true));
    assert_eq!(t(g(&result, "value"), "bank_root"), "memory-bank");
    assert_eq!(
        (
            snapshot(&f.bank, false),
            snapshot(&f.directory, false),
            snapshot(&f.world.paths["config"], false),
            snapshot(&f.world.paths["state"], false)
        ),
        before
    );
    for source in values(&f.manifest, "sources") {
        let original = fs::read(f.directory.join(t(&source, "blob"))).unwrap();
        assert_eq!(bytes_sha(&original), t(&source, "sha256"));
    }
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_private_lifecycle_observation_refuses_config_bank_hold_and_attestation_races
fn private_lifecycle_observation_refuses_config_bank_hold_and_attestation_races() {
    for kind in ["config", "bank", "hold", "attestation"] {
        let f = Fixture::new();
        let baseline = (
            snapshot(&f.bank, false),
            snapshot(&f.directory, false),
            snapshot(&f.world.paths["config"], false),
            snapshot(&f.world.paths["state"], false),
        );
        assert_eq!(
            g(&collect(library(&f, "private", None).0), "ok"),
            &Value::Bool(true)
        );
        let stage = "private_state.recovery_quiescence._selected_attestation";
        let (child, pause) = library(&f, "private", Some(stage));
        let displaced = f.bank.with_file_name("displaced-bank");
        let path = match kind {
            "config" => f.repo.join(".git/config"),
            "hold" => f.bank.join(".agent-memory-transaction.md"),
            "attestation" => fs::read_dir(f.directory.join("proposals"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
            _ => f.bank.clone(),
        };
        let saved = if kind == "bank" {
            None
        } else {
            Some(fs::read(&path).unwrap())
        };
        match kind {
            "bank" => fs::rename(&f.bank, &displaced).unwrap(),
            "config" => {
                let mut changed = saved.clone().unwrap();
                changed.extend_from_slice(b"\n[agent-memory]\n bankRoot = foreign-bank\n");
                fs::write(&path, changed).unwrap();
            }
            "hold" => fs::write(
                &path,
                b"Changed hold during actual read-only observation.\n",
            )
            .unwrap(),
            _ => {
                let mut value =
                    json::decode_bytes(saved.as_ref().unwrap(), Default::default()).unwrap();
                let att = value.get_mut("quiescence_attestation").unwrap();
                let mut routes = values(att, "declared_invocation_routes");
                routes.push(s("/changed-route"));
                set(att, "declared_invocation_routes", Value::Array(routes));
                private(&path, &value);
            }
        }
        fs::write(pause.join(format!("{stage}.release")), b"release").unwrap();
        assert_eq!(g(&collect(child), "ok"), &Value::Bool(false), "{kind}");
        if kind == "bank" {
            fs::rename(displaced, &f.bank).unwrap();
        } else {
            fs::write(&path, saved.unwrap()).unwrap();
        }
        assert_eq!(
            g(&collect(library(&f, "private", None).0), "ok"),
            &Value::Bool(true)
        );
        assert_eq!(
            (
                snapshot(&f.bank, false),
                snapshot(&f.directory, false),
                snapshot(&f.world.paths["config"], false),
                snapshot(&f.world.paths["state"], false)
            ),
            baseline
        );
    }
}
fn signed_hold() -> (Fixture, Channel) {
    let saved = Rc::new(RefCell::new(None));
    let routes = Rc::new(RefCell::new(Vec::new()));
    let save_channel = saved.clone();
    let save_routes = routes.clone();
    let read_routes = routes.clone();
    let f = Fixture::configured(
        false,
        POLICY,
        move |f| {
            let channel = Channel::new(&f.world);
            channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
            let installed = channel.install(&f.world, "none", "copy");
            let receipt = g(&installed, "receipt");
            f.runtime = PathBuf::from(t(g(receipt, "source"), "pack"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            f.command = Path::new(t(receipt, "bin_dir")).join("am");
            *save_routes.borrow_mut() = values(receipt, "targets")
                .iter()
                .map(|row| s(t(row, "path")))
                .collect();
            *save_channel.borrow_mut() = Some(channel);
        },
        move |request| {
            let att = request.get_mut("quiescence_attestation").unwrap();
            let mut routes = values(att, "declared_invocation_routes");
            routes.extend(read_routes.borrow().iter().cloned());
            routes.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            routes.dedup();
            set(att, "declared_invocation_routes", Value::Array(routes));
            let checksum = sha(att);
            set(
                request.get_mut("review").unwrap(),
                "quiescence_attestation_sha256",
                s(checksum),
            );
        },
    );
    let channel = saved.borrow_mut().take().unwrap();
    (f, channel)
}
fn renewal(f: &Fixture, digest: &str) -> PathBuf {
    let mut att = g(&f.layout_request(), "quiescence_attestation").clone();
    set(&mut att, "runtime_artifact_digest", s(digest));
    set(&mut att, "attestation_id", s(uuid()));
    let review_id = uuid();
    set(&mut att, "review_id", s(&review_id));
    let receipt =
        support::lifecycle::read(&support::lifecycle::state(&f.world).join("receipt.json"));
    let mut routes = values(&att, "declared_invocation_routes");
    routes.extend(
        values(&receipt, "targets")
            .iter()
            .map(|row| s(t(row, "path"))),
    );
    routes.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    routes.dedup();
    set(&mut att, "declared_invocation_routes", Value::Array(routes));
    let envelope = obj([
        ("attestation", att.clone()),
        (
            "review",
            obj([
                ("schema", s("agent-memory/layout-quiescence-review-v1")),
                ("review_id", s(review_id)),
                ("operation_id", s(&f.operation)),
                ("attestation_sha256", s(sha(&att))),
                ("reviewer_developer_id", s(&f.actor)),
                ("reviewer_harness", s("fixture-independent-reviewer")),
                ("reviewer_session", s(format!("rollback-{}", uuid()))),
                ("review_time", g(&att, "issued_at").clone()),
                ("decision", s("approve")),
                (
                    "rationale",
                    s(
                        "Review this exact capable artifact and every receipt-owned invocation route.",
                    ),
                ),
            ]),
        ),
    ]);
    PathBuf::from(f.save(&envelope))
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_layout_recovery_runtime_rollback.py::HeldRuntimeRollbackContract.test_interrupted_capable_update_rolls_back_exact_runtime_and_keeps_hold
fn interrupted_capable_update_rolls_back_exact_runtime_and_keeps_hold() {
    let (f, channel) = signed_hold();
    let receipt_path = support::lifecycle::state(&f.world).join("receipt.json");
    let before_receipt = fs::read(&receipt_path).unwrap();
    let receipt = json::decode_bytes(&before_receipt, Default::default()).unwrap();
    let targets = values(&receipt, "targets")
        .iter()
        .map(|row| {
            let path = PathBuf::from(t(row, "path"));
            let snapshot = support::lifecycle::snapshot(&path);
            (path, snapshot)
        })
        .collect::<Vec<_>>();
    let manifest = channel.publish("99.0.0", &"2".repeat(40));
    let path = renewal(
        &f,
        &manifest
            .targets
            .get(agent_memory::installation::TARGET)
            .unwrap()
            .artifact_digest,
    );
    let arg = format!("layout={}", path.display());
    let bank = snapshot(&f.bank, false);
    let mut world_environment = f.world.env.clone();
    world_environment.insert(
        "AGENT_MEMORY_FAULT_STAGE".into(),
        "update-after-activate-0".into(),
    );
    world_environment.insert("AGENT_MEMORY_FAULT_MODE".into(), "exit".into());
    let mut world = f.world;
    let old = std::mem::replace(&mut world.env, world_environment);
    channel.run(&world, "update", &[&arg], 86);
    world.env = old;
    let private_before = snapshot(&f.directory, false);
    assert_eq!(
        g(&channel.run(&world, "recover", &[], 0), "verified"),
        &Value::Bool(true)
    );
    assert_eq!(fs::read(receipt_path).unwrap(), before_receipt);
    for (path, before) in targets {
        assert_eq!(support::lifecycle::snapshot(&path), before);
    }
    assert_eq!(snapshot(&f.bank, false), bank);
    assert_eq!(snapshot(&f.directory, false), private_before);
    let output = world
        .am_with(
            [
                "--json",
                "bank",
                "recover-layout",
                "status",
                "--repo",
                f.repo.to_str().unwrap(),
                "--operation",
                &f.operation,
            ],
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(0));
    let status = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(
        g(&status, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    assert!(!values(&status, "pending_unit_ids").is_empty());
}

fn observe(f: &Fixture, receipt: &Value) {
    let session = uuid();
    let lifecycle = uuid();
    let instance = uuid();
    let id = uuid();
    let entry = obj([
        ("type", s("message")),
        ("id", s(&id)),
        ("parentId", Value::Null),
        (
            "message",
            obj([
                ("role", s("user")),
                ("attribution", s("user")),
                (
                    "content",
                    s("Disposable synthetic carrier observation, no native client or provider."),
                ),
            ]),
        ),
    ]);
    let payload = obj([
        ("entries", Value::Array(vec![entry])),
        ("branch_ids", Value::Array(vec![s(&id)])),
        ("event_ordinal", n(1)),
        ("terminal", Value::Bool(false)),
        ("trigger", s("resume")),
        ("inherited_ids", Value::Array(vec![])),
        ("request_wake", Value::Bool(false)),
        (
            "view",
            obj([
                ("session", s(&session)),
                ("cwd", s(f.repo.to_str().unwrap())),
                ("file", Value::Null),
                (
                    "header",
                    obj([
                        ("id", s(&session)),
                        ("cwd", s(f.repo.to_str().unwrap())),
                        ("version", n(3)),
                        ("parent_session", Value::Null),
                        ("timestamp", Value::Null),
                    ]),
                ),
                ("leaf_id", s(&id)),
                (
                    "native_parents",
                    Value::Array(vec![obj([
                        ("id", s(&id)),
                        ("parent_id", Value::Null),
                        ("timestamp", Value::Null),
                    ])]),
                ),
                ("source_kind", s("session-manager")),
                ("tail_status", s("available")),
                ("fork_status", s("none")),
            ]),
        ),
    ]);
    let request = obj([
        ("schema", s("agent-memory/omp-observer-request-v2")),
        ("action", s("seal")),
        ("session", s(&session)),
        ("lifecycle", s(&lifecycle)),
        ("payload", payload),
    ]);
    let identity = obj([
        ("view_id", s(uuid())),
        ("leaf_id", s(&id)),
        ("native_ids", Value::Array(vec![s(&id)])),
        ("tool_ids", Value::Array(vec![])),
    ]);
    let mut wire = agent_memory::omp_stream::encode_frame(
        "REQUEST",
        1,
        &obj([
            ("request", request),
            ("event_ordinal", n(1)),
            ("identity", identity),
        ]),
        None,
    )
    .unwrap();
    wire.extend(
        agent_memory::omp_stream::encode_frame(
            "FINAL",
            2,
            &obj([
                ("last_attempt_seq", n(1)),
                ("refused_count", n(0)),
                ("last_event_ordinal", n(1)),
                ("transition", Value::Null),
            ]),
            None,
        )
        .unwrap(),
    );
    let argv = vec![
        f.command.as_os_str().to_owned(),
        "omp-boundary".into(),
        "--stream".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--session".into(),
        session.into(),
        "--instance".into(),
        instance.into(),
        "--lifecycle".into(),
        lifecycle.into(),
        "--artifact".into(),
        t(g(receipt, "source"), "artifact_digest").into(),
        "--omp-agent-dir".into(),
        t(g(receipt, "omp_binding"), "agent_dir").into(),
    ];
    let env = f
        .world
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    let result = agent_memory::process::run(&Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&f.repo),
        search_path: false,
        stdin: Input::Bytes(&wire),
        stdout: Target::Capture { limit: 1024 * 1024 },
        stderr: Target::Capture { limit: 1024 * 1024 },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(60)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .unwrap();
    assert_eq!(result.status, Status::Code(0), "{result:?}");
    let mut raw = result.stdout.as_slice();
    let mut verified = false;
    while !raw.is_empty() {
        let end = raw.iter().position(|b| *b == b'\n').unwrap();
        let header = std::str::from_utf8(&raw[..end])
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>();
        let length = header[3].parse::<usize>().unwrap();
        let value =
            json::decode_bytes(&raw[end + 1..end + 1 + length], Default::default()).unwrap();
        if header[1] == "OBSERVED" {
            verified = g(g(&value, "result"), "verified") == &Value::Bool(true);
        }
        raw = &raw[end + 1 + length..];
    }
    assert!(verified);
}
fn pending(f: &Fixture) -> Vec<Value> {
    values(
        &f.good(vec![
            "omp-boundary".into(),
            "--repo".into(),
            f.repo.as_os_str().to_owned(),
            "--list-pending".into(),
            "--offset".into(),
            "0".into(),
        ]),
        "pending",
    )
}
fn acknowledge(f: &Fixture, row: &Value, expected: i32) {
    let source = f.good(vec![
        "omp-boundary".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--source".into(),
        g(row, "observation_id").as_str().unwrap().into(),
        "--session".into(),
        g(row, "session").as_str().unwrap().into(),
        "--lifecycle".into(),
        g(row, "lifecycle").as_str().unwrap().into(),
    ]);
    assert_eq!(g(&source, "coverage_complete"), &Value::Bool(true));
    assert!(values(&source, "faults").is_empty());
    let mut request = Value::Object(
        ["session", "lifecycle", "observation_id", "source_sha256"]
            .map(|key| (key.into(), g(row, key).clone()))
            .to_vec(),
    );
    set(&mut request, "outcome", s("no-learning"));
    set(
        &mut request,
        "assessment",
        obj([
            ("source_id", g(row, "observation_id").clone()),
            (
                "source_review",
                s(
                    "Reviewed the exact synthetic carrier or its complete sealed manifest; no durable project learning.",
                ),
            ),
            ("candidates", Value::Array(vec![])),
        ]),
    );
    let raw = encoded(&request).unwrap();
    let argv = [
        f.command.as_os_str().to_owned(),
        "omp-boundary".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--stdin".into(),
    ];
    let env = f
        .world
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    let output = agent_memory::process::run(&Spec {
        argv: &argv,
        env: &env,
        cwd: Some(&f.repo),
        search_path: false,
        stdin: Input::Bytes(&raw),
        stdout: Target::Capture { limit: 65536 },
        stderr: Target::Capture { limit: 65536 },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(20)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .unwrap();
    assert_eq!(output.status, Status::Code(expected), "{output:?}");
    if expected == 0 {
        let result = json::decode_bytes(&output.stdout, Default::default()).unwrap();
        assert_eq!(g(&result, "verified"), &Value::Bool(true));
    }
}
fn mixed(completed: bool) -> Fixture {
    let routes = Rc::new(RefCell::new(Vec::new()));
    let saved = routes.clone();
    let reviewed = routes.clone();
    Fixture::configured(
        false,
        POLICY,
        move |f| {
            let agent = f.world.root.join("explicit-omp");
            fs::create_dir(&agent).unwrap();
            let installed = f.good(vec![
                "install".into(),
                "--development-source".into(),
                f.runtime.as_os_str().to_owned(),
                "--presets".into(),
                "omp".into(),
                "--omp-agent-dir".into(),
                agent.into_os_string(),
                "--omp-peers-upgraded".into(),
            ]);
            let receipt = g(&installed, "receipt");
            f.runtime = PathBuf::from(t(g(receipt, "source"), "pack"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            f.command = Path::new(t(receipt, "bin_dir")).join("am");
            *saved.borrow_mut() = values(receipt, "targets")
                .iter()
                .map(|row| s(t(row, "path")))
                .collect();
            observe(f, receipt);
            let rows = pending(f);
            assert_eq!(rows.len(), 2);
            if completed {
                for row in &rows {
                    acknowledge(f, row, 0);
                }
                assert!(pending(f).is_empty());
            }
        },
        move |request| {
            let att = request.get_mut("quiescence_attestation").unwrap();
            let mut routes = values(att, "declared_invocation_routes");
            routes.extend(reviewed.borrow().iter().cloned());
            routes.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            routes.dedup();
            set(att, "declared_invocation_routes", Value::Array(routes));
            let checksum = sha(att);
            set(
                request.get_mut("review").unwrap(),
                "quiescence_attestation_sha256",
                s(checksum),
            );
        },
    )
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_omp_first_mixed_pending_rejects_actual_published_old_managed_candidate
fn omp_first_mixed_pending_rejects_retired_managed_candidate() {
    let f = mixed(false);
    let before = (
        snapshot(&f.bank, false),
        snapshot(&f.directory, false),
        snapshot(&f.world.paths["config"], false),
        snapshot(&f.world.paths["state"], false),
    );
    let pending_before = pending(&f);
    for check in [false, true] {
        let mut args = vec![
            "install".into(),
            "--remote".into(),
            "file:///inert-legacy-managed-candidate".into(),
            "--ref".into(),
            "main".into(),
        ];
        if check {
            args.push("--check".into());
        }
        let (code, denied) = f.call(args, None);
        assert_eq!(code, 1);
        assert_eq!(
            t(&denied, "error"),
            agent_memory::installation::MANAGED_GUIDANCE
        );
        assert_eq!(
            (
                snapshot(&f.bank, false),
                snapshot(&f.directory, false),
                snapshot(&f.world.paths["config"], false),
                snapshot(&f.world.paths["state"], false)
            ),
            before
        );
        assert_eq!(pending(&f), pending_before);
    }
    let row = pending_before.first().unwrap();
    acknowledge(&f, row, 1);
    assert_eq!(pending(&f), pending_before);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_completed_omp_drains_before_layout_then_old_reader_after_both
fn completed_omp_drains_only_after_independent_layout_hold_retires() {
    let f = mixed(true);
    assert!(pending(&f).is_empty());
    let base = f.directory.parent().unwrap().parent().unwrap();
    let marker = base.join("turn-sources-compat-v1.json");
    assert!(marker.exists());
    let retained = snapshot(&base.join("omp-source-v1"), false);
    let originals = snapshot(&f.directory.join("originals"), false);
    let before = (
        snapshot(&f.bank, false),
        snapshot(&f.directory, false),
        snapshot(&f.world.paths["config"], false),
    );
    let (code, denied) = f.call(
        vec!["uninstall".into(), "--omp-writers-quiesced".into()],
        None,
    );
    assert_eq!(code, 1);
    assert!(t(&denied, "error").contains("Private-state runtime compatibility"));
    assert!(marker.exists());
    assert_eq!(
        (
            snapshot(&f.bank, false),
            snapshot(&f.directory, false),
            snapshot(&f.world.paths["config"], false)
        ),
        before
    );
    let recovered = f.good(vec!["recover".into()]);
    assert_eq!(g(&recovered, "verified"), &Value::Bool(true));
    assert!(marker.exists());
    assert!(pending(&f).is_empty());
    let (proposal, _, _) = f.admission(true);
    let admitted = f.good(vec![
        "admission".into(),
        "apply".into(),
        "--proposal".into(),
        f.save(&proposal),
    ]);
    assert_eq!(g(&admitted, "admission_verified"), &Value::Bool(true));
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(g(&status, "semantic_recovery_complete"), &Value::Bool(true));
    assert_eq!(snapshot(&f.directory.join("originals"), false), originals);
    let removed = f.good(vec!["uninstall".into(), "--omp-writers-quiesced".into()]);
    assert_eq!(g(&removed, "verified"), &Value::Bool(true));
    assert!(!marker.exists());
    assert!(
        fs::read_dir(base.join("omp-drains"))
            .unwrap()
            .next()
            .is_some()
    );
    assert_eq!(snapshot(&base.join("omp-source-v1"), false), retained);
    let before = snapshot(&f.bank, false);
    let output = f
        .world
        .am_with(
            [
                "--json",
                "bank",
                "validate",
                "--repo",
                f.repo.to_str().unwrap(),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(snapshot(&f.bank, false), before);
}
