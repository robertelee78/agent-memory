mod support;
use agent_memory::codec::json::{self, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::lifecycle::*;
use support::{RunOptions, World};
fn diagnostic(
    world: &World,
    cli: &Path,
    operation: &str,
    repo: Option<&Path>,
    expected: i32,
) -> Value {
    let mut args = vec![cli.to_str().unwrap(), "--json", operation];
    if operation != "version" {
        if let Some(repo) = repo {
            args.extend(["--repo", repo.to_str().unwrap()]);
        }
    }
    let output = world
        .run_with(
            args,
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(expected),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    for field in ["writes_performed", "network_performed", "repairs_performed"] {
        assert_eq!(get(&value, field), &Value::Bool(false));
    }
    let security = get(&value, "security");
    assert_eq!(
        text(security, "publisher_authentication"),
        "not_established"
    );
    for field in [
        "signature_verification_performed",
        "executing_verifier_authenticated",
        "platform_acls_audited",
    ] {
        assert_eq!(get(security, field), &Value::Bool(false));
    }
    value
}
fn installed(world: &World) -> (PathBuf, Value) {
    let source = source(world);
    let value = install(world, &source, &["--presets", "none"]);
    (command(get(&value, "receipt")), value)
}

// obligation: tests/contracts/test_lifecycle_security_contract.py::LifecycleSecurityContract.test_outside_helper_without_receipt_or_with_invalid_repository_keeps_negative_qualifications
#[test]
fn outside_native_helper_keeps_negative_security_qualifications() {
    let world = World::new().unwrap();
    let before = snapshot(&world.root);
    let cli = Path::new(env!("CARGO_BIN_EXE_am"));
    // Declared Rust package-location rule: an uninstalled target/debug binary has no authenticated pack.
    for (operation, repo, expected) in [
        ("version", None, 1),
        ("status", Some(world.root.join("missing repository")), 1),
    ] {
        let value = diagnostic(&world, cli, operation, repo.as_deref(), expected);
        assert_eq!(
            text(get(&value, "security"), "recorded_source_selection"),
            "unavailable"
        );
        assert_eq!(snapshot(&world.root), before);
    }
}

// obligation: tests/contracts/test_lifecycle_security_contract.py::LifecycleSecurityContract.test_managed_and_explicit_source_have_distinct_recorded_selection_without_signature_or_write_claim
#[test]

fn retired_managed_channel_refuses_and_explicit_source_is_not_authenticated() {
    let world = World::new().unwrap();
    let remote = world.repository("unsigned managed Git", false).unwrap();
    let before = snapshot(&world.root);
    let refused = run(
        &world,
        &[
            "install",
            "--remote",
            remote.to_str().unwrap(),
            "--presets",
            "none",
        ],
        1,
    );
    assert!(text(&refused, "error").contains("Managed Git"));
    assert_eq!(snapshot(&world.root), before);
    let (cli, _) = installed(&world);
    let repo = world.repository("consumer", false).unwrap();
    let before = snapshot(&world.root);
    for operation in ["version", "status", "doctor"] {
        let value = diagnostic(&world, &cli, operation, Some(&repo), 0);
        assert_eq!(
            text(get(&value, "security"), "recorded_source_selection"),
            "operator_selected_development"
        );
        if operation == "doctor" {
            let warning = get(&value, "checks")
                .as_array()
                .unwrap()
                .iter()
                .find(|row| text(row, "component") == "distribution.trust")
                .unwrap();
            assert_eq!(text(warning, "state"), "warning");
            assert!(text(warning, "detail").contains("operator-selected development/custom"));
        }
        assert_eq!(snapshot(&world.root), before);
    }
    let human = world.run([cli.to_str().unwrap(), "--version"]).unwrap();
    let stdout = String::from_utf8(human.stdout).unwrap();
    assert!(!stdout.contains("publisher_authentication"));
    assert!(!stdout.contains("platform_acls_audited"));
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_lifecycle_security_contract.py::LifecycleSecurityContract.test_corrupt_or_spoofed_receipt_stays_unauthenticated_and_preserved_on_failures
#[test]

fn corrupt_or_spoofed_receipt_remains_unauthenticated_and_preserved() {
    for case in ["malformed", "spoofed-authentication"] {
        let world = World::new().unwrap();
        let (cli, _) = installed(&world);
        let path = state(&world).join("receipt.json");
        if case == "malformed" {
            std::fs::write(
                &path,
                b"{\"schema\":\"invalid-receipt\",\"publisher_authenticated\":true}\n",
            )
            .unwrap();
        } else {
            let mut value = read(&path);
            if let Value::Object(fields) = &mut value {
                if let Some((_, Value::Object(source))) =
                    fields.iter_mut().find(|(key, _)| key == "source")
                {
                    source.push(("publisher_authenticated".into(), Value::Bool(true)));
                    source.push(("signature_verified".into(), Value::Bool(true)));
                }
            }
            save(&path, &value);
        }
        let before = snapshot(&world.root);
        for operation in ["version", "status", "doctor"] {
            let value = diagnostic(&world, &cli, operation, Some(&world.paths["repo"]), 1);
            assert_eq!(
                text(get(&value, "security"), "recorded_source_selection"),
                "unavailable"
            );
            assert!(!get(&value, "failures").as_array().unwrap().is_empty());
            assert_eq!(snapshot(&world.root), before);
        }
    }
}

// obligation: tests/contracts/test_lifecycle_security_contract.py::LifecycleSecurityContract.test_real_busy_writer_defers_ownership_checks_without_authenticating_or_stealing_lock
#[test]

fn busy_writer_defers_ownership_checks_without_authentication_or_lock_theft() {
    let world = World::new().unwrap();
    let (cli, _) = installed(&world);
    let lock = std::fs::File::open(state(&world).join("lock")).unwrap();
    lock.lock().unwrap();
    let before = snapshot(&world.root);
    let value = diagnostic(&world, &cli, "doctor", Some(&world.paths["repo"]), 0);
    assert_eq!(
        text(get(&value, "security"), "recorded_source_selection"),
        "operator_selected_development"
    );
    let checks = get(&value, "checks").as_array().unwrap();
    assert!(
        checks
            .iter()
            .any(|row| text(row, "component") == "runtime.lock" && text(row, "state") == "warning")
    );
    assert!(
        !checks
            .iter()
            .any(|row| text(row, "component").starts_with("runtime.target."))
    );
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_lifecycle_security_contract.py::LifecycleSecurityContract.test_receipt_changed_before_shared_lock_has_one_coherent_final_selection_and_warning
#[test]

fn receipt_change_before_shared_lock_uses_one_coherent_final_selection() {
    for change in ["uninstall", "development-source", "malformed"] {
        let world = World::new().unwrap();
        let (cli, _) = installed(&world);
        let replacement = if change == "development-source" {
            let target = world.root.join("replacement explicit source");
            copy_tree(&source_parent(&cli, &world), &target);
            Some(target)
        } else {
            None
        };
        let rendezvous = world.root.join("diagnostic-rendezvous");
        let stage = "diagnostic-before-runtime-lock";
        let overrides = [
            (
                OsString::from("AGENT_MEMORY_FAULT_STAGE"),
                OsString::from(stage),
            ),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                rendezvous.clone().into_os_string(),
            ),
        ];
        let child = world
            .spawn_with(
                [
                    cli.to_str().unwrap(),
                    "--json",
                    "doctor",
                    "--repo",
                    world.paths["repo"].to_str().unwrap(),
                ],
                RunOptions {
                    check: false,
                    env: &overrides,
                    timeout: Duration::from_secs(120),
                    ..Default::default()
                },
            )
            .unwrap();
        let ready = rendezvous.join(format!("{stage}.ready"));
        let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(10)).unwrap();
        while !ready.exists() {
            assert!(!deadline.expired(), "diagnostic rendezvous was not reached");
            std::thread::sleep(Duration::from_millis(5));
        }
        if change == "malformed" {
            std::fs::write(
                state(&world).join("receipt.json"),
                b"{\"schema\":\"malformed-receipt\"}\n",
            )
            .unwrap();
        } else {
            run(&world, &["uninstall"], 0);
            if let Some(replacement) = &replacement {
                install(&world, replacement, &["--presets", "none"]);
            }
        }
        std::fs::write(rendezvous.join(format!("{stage}.release")), b"").unwrap();
        let before = snapshot(&world.root);
        let output = child.wait().unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(if change == "malformed" { 1 } else { 0 })
        );
        let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
        assert_eq!(
            text(get(&value, "security"), "recorded_source_selection"),
            if change == "development-source" {
                "operator_selected_development"
            } else {
                "unavailable"
            }
        );
        assert_eq!(
            get(get(&value, "security"), "signature_verification_performed"),
            &Value::Bool(false)
        );
        let warning = get(&value, "checks")
            .as_array()
            .unwrap()
            .iter()
            .find(|row| text(row, "component") == "distribution.trust")
            .unwrap();
        assert!(!text(warning, "detail").contains("unsigned managed Git"));
        assert!(
            text(warning, "detail").contains(if change == "development-source" {
                "operator-selected development/custom"
            } else {
                "available installation evidence"
            })
        );
        if change == "uninstall" {
            assert_eq!(text(get(&value, "installation"), "state"), "uninstalled");
        }
        assert_eq!(snapshot(&world.root), before);
    }
}
fn source_parent(_cli: &Path, world: &World) -> PathBuf {
    world.root.join("development source with spaces")
}
