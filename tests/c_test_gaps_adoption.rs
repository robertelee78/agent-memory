#![cfg(feature = "dev-tools")]
mod support;
use agent_memory::codec::json::Value;
use agent_memory::installation;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use support::cutover::put;
use support::distribution::Channel;
use support::lifecycle::*;
use support::{RunOptions, World};

/// Derived v0.0.14 state, never an executable legacy runtime. The cutover
/// deliberately replaces Python wrappers; it does not retain interpreter dispatch.
fn captured(w: &World, scenario: &str) -> Value {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v014-state")
        .join(scenario);
    let relocate = |raw: &str| raw.replace("@WORLD@", w.root.to_str().unwrap());
    for line in fs::read_to_string(base.join("modes.txt")).unwrap().lines() {
        let (bits, relative) = line.split_once(' ').unwrap();
        let path = w.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            relocate(&fs::read_to_string(base.join(relative)).unwrap()),
        )
        .unwrap();
        mode(&path, u32::from_str_radix(bits, 8).unwrap());
    }
    mode(&state(w), 0o700);
    for line in fs::read_to_string(base.join("trees.txt")).unwrap().lines() {
        let (kind, relative) = line.split_once(' ').unwrap();
        let path = w.root.join(relative);
        fs::create_dir_all(&path).unwrap();
        mode(&path, 0o700);
        fs::write(path.join("opaque"), format!("opaque {kind} {relative}\n")).unwrap();
        mode(&path.join("opaque"), 0o644);
    }
    for line in fs::read_to_string(base.join("links.txt")).unwrap().lines() {
        let (relative, target) = line.split_once(" -> ").unwrap();
        let path = w.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(relocate(target), path).unwrap();
    }
    let receipt_path = state(w).join("receipt.json");
    let mut receipt = read(&receipt_path);
    let identities = read(&base.join("identities.json"));
    let Value::Array(targets) = receipt.get_mut("targets").unwrap() else {
        panic!("captured targets")
    };
    for target in targets {
        let path = PathBuf::from(text(target, "path"));
        let actual = installation::fingerprint(&path).unwrap();
        let relative = path.strip_prefix(&w.root).unwrap().to_str().unwrap();
        if let Some(expected) = identities.get(relative) {
            assert!(installation::same(&actual, expected));
        } else {
            put(target, "identity", actual);
        }
    }
    fs::write(&receipt_path, installation::encoded(&receipt).unwrap()).unwrap();
    mode(&receipt_path, 0o600);
    let journal_path = state(w).join("journal.json");
    let mut journal = read(&journal_path);
    put(&mut journal, "receipt", receipt.clone());
    put(
        &mut journal,
        "receipt_identity",
        installation::fingerprint(&receipt_path).unwrap(),
    );
    fs::write(journal_path, installation::encoded(&journal).unwrap()).unwrap();
    receipt
}
fn native(w: &World, args: &[&str]) -> Value {
    run(w, args, 0)
}
fn adoption(scenario: &str, alias: bool, collaborative: bool) {
    let mut w = World::new().unwrap();
    let old = captured(&w, scenario);
    let old_source = PathBuf::from(text(get(&old, "source"), "path"));
    let retained = snapshot(&old_source);
    let history = fs::read(state(&w).join("journal.json")).unwrap();
    let source = source(&w);
    if alias {
        let alias = w.root.join("state-alias");
        symlink(&w.paths["state"], &alias).unwrap();
        w.env
            .insert("XDG_STATE_HOME".into(), alias.clone().into_os_string());
        w.env.insert(
            "AGENT_MEMORY_STATE_HOME".into(),
            alias.join("agent-memory").into_os_string(),
        );
    }
    let output = if collaborative {
        let env = w
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        let args = [
            env!("CARGO_BIN_EXE_am").into(),
            "--json".into(),
            "install".into(),
            "--development-source".into(),
            source.clone().into_os_string(),
            "--presets".into(),
            "none".into(),
        ];
        let output = agent_memory::process::run(&agent_memory::process::Spec {
            argv: &args,
            env: &env,
            cwd: Some(&w.paths["repo"]),
            search_path: false,
            stdin: agent_memory::process::Input::Null,
            stdout: agent_memory::process::Target::Capture {
                limit: support::world::CAPTURE_LIMIT,
            },
            stderr: agent_memory::process::Target::Capture {
                limit: support::world::CAPTURE_LIMIT,
            },
            deadline: agent_memory::clock::Deadline::from_seconds(60.0).unwrap(),
            new_session: false,
            file_size_limit: None,
            umask: Some(0o002),
            pass_fds: &[],
        })
        .unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{output:?}"
        );
        agent_memory::codec::json::decode_bytes(&output.stdout, Default::default()).unwrap()
    } else {
        install(&w, &source, &["--presets", "none"])
    };
    assert_eq!(get(&output, "verified"), &Value::Bool(true));
    let receipt = get(&output, "receipt");
    assert_eq!(text(receipt, "schema"), "agent-memory/install-receipt-v3");
    assert_eq!(
        get(receipt, "installation_id"),
        get(&old, "installation_id")
    );
    assert_eq!(
        snapshot(&old_source),
        retained,
        "Python-era source is inert retained history"
    );
    assert!(
        fs::read_dir(state(&w))
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("journal-")
                && fs::read(e.path()).ok().as_deref() == Some(history.as_slice()))
    );
    let binary = command(receipt);
    for name in ["am", "agent-memory"] {
        let target = binary.parent().unwrap().join(name);
        for flag in ["--version", "-V"] {
            let result = w
                .run([target.as_os_str(), std::ffi::OsStr::new(flag)])
                .unwrap();
            assert!(String::from_utf8_lossy(&result.stdout).contains(env!("CARGO_PKG_VERSION")));
        }
    }
    let stable = fs::read(state(&w).join("receipt.json")).unwrap();
    assert_eq!(
        get(&install(&w, &source, &["--presets", "none"]), "changed"),
        &Value::Bool(false)
    );
    assert_eq!(fs::read(state(&w).join("receipt.json")).unwrap(), stable);
    let removed = native(&w, &["uninstall"]);
    assert_eq!(get(&removed, "verified"), &Value::Bool(true));
    assert!(!binary.exists());
    assert!(!binary.parent().unwrap().join("agent-memory").exists());
    assert_eq!(snapshot(&old_source), retained);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_installer.py::MacOSCommandContract.test_actual_shipped_old_updater_then_install_migrates_without_stranding_ownership
fn captured_old_install_migrates_without_stranding_ownership() {
    adoption("all-copy", false, false);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_installer.py::MacOSCommandContract.test_shipped_uninstaller_can_remove_new_wrappers_with_aliased_xdg_and_target_paths
fn captured_ownership_removes_native_commands_under_aliased_xdg() {
    adoption("all-link", true, false);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_actual_released_v002_updater_accepts_changed_runtime_under_umask002
fn captured_legacy_ownership_adopts_under_umask002() {
    adoption("all-copy", false, true);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_public_cli.py::PublicCLIContract.test_actual_legacy_wrapper_gains_flags_and_human_output_without_wrapper_replacement
fn captured_legacy_commands_gain_native_flags_with_explicit_wrapper_cutover() {
    adoption("all-link", false, false);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_trusted_source_helper_updates_when_the_installed_command_does_not_yet_dispatch_update
fn trusted_native_binary_adopts_without_executing_legacy_dispatch() {
    adoption("claude-opencode-copy", false, false);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_installer.py::MacOSCommandContract.test_current_and_shipped_recovery_read_each_others_wrapper_journals
fn captured_and_native_journals_recover_at_the_receipt_boundary() {
    for committed in [false, true] {
        let w = World::new().unwrap();
        let old = captured(&w, "all-copy");
        let source = source(&w);
        let old_wrapper = fs::read(command(&old)).unwrap();
        let stage = if committed {
            "install-after-receipt"
        } else {
            "install-before-receipt"
        };
        let output = fault(
            &w,
            &[
                "install",
                "--development-source",
                source.to_str().unwrap(),
                "--presets",
                "none",
            ],
            stage,
            "exit",
        );
        assert_eq!(output.status, agent_memory::process::Status::Code(86));
        let recovered = native(&w, &["recover"]);
        assert_eq!(get(&recovered, "verified"), &Value::Bool(true));
        if committed {
            assert_ne!(fs::read(command(&old)).unwrap(), old_wrapper);
            assert_eq!(
                text(&read(&state(&w).join("receipt.json")), "schema"),
                "agent-memory/install-receipt-v3"
            );
        } else {
            assert_eq!(fs::read(command(&old)).unwrap(), old_wrapper);
        }
        let stable = fs::read(state(&w).join("receipt.json")).unwrap();
        native(&w, &["recover"]);
        assert_eq!(fs::read(state(&w).join("receipt.json")).unwrap(), stable);
    }
}

fn signed_adoption(deleted_worktree: bool, collaborative: bool) {
    let mut w = World::new().unwrap();
    let old = captured(&w, "all-copy");
    let source = PathBuf::from(text(get(&old, "source"), "path"));
    let old_source = snapshot(&source);
    let repo = w.repository("registered-consumer", false).unwrap();
    let bank = bank(&w, &repo);
    support::identity_fixtures::bind(&w, &repo).unwrap();
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..Default::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Consumer bank"],
        RunOptions {
            cwd: Some(&repo),
            ..Default::default()
        },
    )
    .unwrap();
    if deleted_worktree {
        let missing = w.root.join("removed-session");
        w.git_with(
            [
                std::ffi::OsStr::new("worktree"),
                std::ffi::OsStr::new("add"),
                std::ffi::OsStr::new("--detach"),
                missing.as_os_str(),
            ],
            RunOptions {
                cwd: Some(&repo),
                ..Default::default()
            },
        )
        .unwrap();
        fs::remove_dir_all(missing).unwrap();
    }
    let before_bank = snapshot(&bank);
    let channel = Channel::new(&w);
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let adopted = channel.run_with_umask(
        &w,
        "install",
        &["none", "copy"],
        0,
        if collaborative { "002" } else { "022" },
    );
    let receipt = get(&adopted, "receipt");
    assert_eq!(text(get(receipt, "source"), "kind"), "release");
    assert_eq!(snapshot(&bank), before_bank);
    assert_eq!(snapshot(&source), old_source);
    assert_eq!(
        get(&channel.run(&w, "update", &[], 0), "changed"),
        &Value::Bool(false)
    );
    assert_eq!(snapshot(&bank), before_bank);
    // No source execution or interpreter is part of same-version signed adoption.
    w.env.insert("PATH".into(), "/usr/bin:/bin".into());
    let version = native(&w, &["version"]);
    assert_ne!(get(&version, "verified"), &Value::Bool(false));
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_actual_v002_index_refresh_and_deleted_worktree_upgrade_through_signed_one_liner
fn captured_old_source_and_deleted_worktree_adopt_to_signed_release() {
    signed_adoption(true, false);
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_actual_v002_process_then_same_version_signed_adoption_under_022_and_002
fn captured_legacy_source_adopts_signed_release_with_collaborative_modes() {
    signed_adoption(false, false);
    signed_adoption(false, true);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_released_v014_sources_preview_then_upgrade_without_inventing_native_authority
fn captured_pending_sources_preview_and_adopt_without_inventing_native_authority() {
    let w = World::new().unwrap();
    captured(&w, "codex-copy-pending-source");
    let source = source(&w);
    let before = snapshot(&w.paths["state"].join("agent-memory/writes"));
    let all_before = snapshot(&w.root);
    let checked = native(
        &w,
        &[
            "install",
            "--check",
            "--development-source",
            source.to_str().unwrap(),
            "--presets",
            "codex",
        ],
    );
    assert_eq!(text(&checked, "schema"), "agent-memory/install-plan-v1");
    assert_eq!(snapshot(&w.root), all_before);
    let adopted = install(&w, &source, &["--presets", "codex"]);
    assert_eq!(get(&adopted, "verified"), &Value::Bool(true));
    assert_eq!(
        snapshot(&w.paths["state"].join("agent-memory/writes")),
        before
    );
    let state = support::cutover::physical(&w.paths["state"]);
    assert!(
        !state
            .keys()
            .any(|name| name.ends_with("omp-bank-binding-v1.json")
                || name.ends_with("turn-sources-compat-v1.json"))
    );
    assert_eq!(
        get(&native(&w, &["recover"]), "verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        snapshot(&w.paths["state"].join("agent-memory/writes")),
        before
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_doctor_uses_the_same_legacy_ownership_check
fn doctor_checks_captured_ownership_and_preserves_foreign_source() {
    let w = World::new().unwrap();
    let old = captured(&w, "all-copy");
    let source = PathBuf::from(text(get(&old, "source"), "path"));
    let before = snapshot(&w.root);
    let observed = run(&w, &["doctor"], 1);
    assert_eq!(get(&observed, "ok"), &Value::Bool(false));
    assert_eq!(snapshot(&w.root), before);
    // Native diagnostics do not accept an opaque Python runtime as an executable
    // native package, and never repair or remove its foreign bytes on observation.
    fs::write(source.join("kept-note"), "Retain user source history.\n").unwrap();
    let before = snapshot(&source);
    run(&w, &["doctor"], 1);
    assert_eq!(snapshot(&source), before);
    fs::remove_file(source.join("kept-note")).unwrap();
    let before = snapshot(&source);
    let native_source = support::lifecycle::source(&w);
    let installed = install(&w, &native_source, &["--presets", "none"]);
    assert_eq!(get(&installed, "verified"), &Value::Bool(true));
    assert_eq!(snapshot(&source), before);
    assert_eq!(get(&native(&w, &["doctor"]), "ok"), &Value::Bool(true));
}
