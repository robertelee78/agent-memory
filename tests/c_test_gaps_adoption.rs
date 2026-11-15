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
    // Adoption keeps every captured owned target (baseline
    // `installation.py:838-839` refuses to strand any of them).
    let presets = text(&old, "presets").to_owned();
    let old_source = PathBuf::from(text(get(&old, "source"), "path"));
    let retained = snapshot(&old_source);
    let history = fs::read(state(&w).join("journal.json")).unwrap();
    let source = source(&w);
    if alias {
        let alias = w.root.join("state-alias");
        symlink(&w.paths["state"], &alias).unwrap();
        w.env
            .insert("XDG_STATE_HOME".into(), alias.clone().into_os_string());
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
            presets.as_str().into(),
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
        install(&w, &source, &["--presets", &presets])
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
        get(&install(&w, &source, &["--presets", &presets]), "changed"),
        &Value::Bool(false)
    );
    assert_eq!(fs::read(state(&w).join("receipt.json")).unwrap(), stable);
    let removed = native(&w, &["uninstall"]);
    assert_eq!(get(&removed, "verified"), &Value::Bool(true));
    assert!(!binary.exists());
    assert!(!binary.parent().unwrap().join("agent-memory").exists());
    assert!(
        !old_source.exists(),
        "explicit uninstall removes owned legacy source"
    );
}

#[test]
// tests/contracts/test_installer.py::MacOSCommandContract.test_actual_shipped_old_updater_then_install_migrates_without_stranding_ownership
fn captured_old_install_migrates_without_stranding_ownership() {
    adoption("all-copy", false, false);
}
#[test]
// tests/contracts/test_installer.py::MacOSCommandContract.test_shipped_uninstaller_can_remove_new_wrappers_with_aliased_xdg_and_target_paths
fn captured_ownership_removes_native_commands_under_aliased_xdg() {
    adoption("all-link", true, false);
}
#[test]
// tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_actual_released_v002_updater_accepts_changed_runtime_under_umask002
fn captured_legacy_ownership_adopts_under_umask002() {
    adoption("all-copy", false, true);
}
#[test]
// tests/contracts/test_public_cli.py::PublicCLIContract.test_actual_legacy_wrapper_gains_flags_and_human_output_without_wrapper_replacement
fn captured_legacy_commands_gain_native_flags_with_explicit_wrapper_cutover() {
    adoption("all-link", false, false);
}
#[test]
// tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_trusted_source_helper_updates_when_the_installed_command_does_not_yet_dispatch_update
fn trusted_native_binary_adopts_without_executing_legacy_dispatch() {
    adoption("claude-opencode-copy", false, false);
}

fn interrupt_captured_activation(w: &World, old: &Value) -> (PathBuf, Vec<u8>) {
    let target = get(old, "targets")
        .as_array()
        .unwrap()
        .iter()
        .find(|target| text(target, "kind") == "activation")
        .unwrap();
    let path = PathBuf::from(text(target, "path"));
    let mut journal = read(&state(w).join("journal.json"));
    let transaction = text(&journal, "transaction");
    let stage = path
        .parent()
        .unwrap()
        .join(format!(".agent-memory-{transaction}-0"));
    fs::rename(&path, &stage).unwrap();
    let original = fs::read(&stage).unwrap();
    let mut interrupted = original.clone();
    interrupted.extend_from_slice(b"\nInterrupted Python update.\n");
    fs::write(&path, interrupted).unwrap();
    mode(&path, 0o644);
    let action = support::cutover::object([
        ("path", Value::String(path.to_str().unwrap().into())),
        ("kind", Value::String("activation".into())),
        ("stage", Value::String(stage.to_str().unwrap().into())),
        ("backup", Value::Null),
        ("before", get(target, "identity").clone()),
        ("after", installation::fingerprint(&path).unwrap()),
    ]);
    put(&mut journal, "status", Value::String("activating".into()));
    put(&mut journal, "receipt_identity", Value::Null);
    put(&mut journal, "created_directories", Value::Array(vec![]));
    put(&mut journal, "actions", Value::Array(vec![action]));
    put(&mut journal, "activated", Value::Int(1.into()));
    fs::write(
        state(w).join("journal.json"),
        installation::encoded(&journal).unwrap(),
    )
    .unwrap();
    (path, original)
}

#[test]
// Design requirement (no baseline test): Python-era adoption, in-flight journal v1 recorded-step rollback.
fn captured_python_journal_rolls_back_under_verified_install_candidate() {
    let w = World::new().unwrap();
    let old = captured(&w, "all-copy");
    let old_source = PathBuf::from(text(get(&old, "source"), "path"));
    let retained = snapshot(&old_source);
    let (path, _) = interrupt_captured_activation(&w, &old);
    let source = source(&w);
    let adopted = install(&w, &source, &["--presets", "all"]);
    assert_eq!(get(&adopted, "verified"), &Value::Bool(true));
    assert!(
        !fs::read_to_string(path)
            .unwrap()
            .contains("Interrupted Python update.")
    );
    assert_eq!(snapshot(&old_source), retained);
    assert!(fs::read_dir(state(&w)).unwrap().any(|entry| {
        let entry = entry.unwrap();
        entry.file_name().to_string_lossy().starts_with("journal-")
            && text(&read(&entry.path()), "status") == "rolled-back"
    }));
}

#[test]
// Design requirement (no baseline test): Python-era adoption, standalone journal recovery verifies the retained reader.
fn standalone_python_recovery_reads_a_verified_pack_and_refuses_drift() {
    let w = World::new().unwrap();
    let mut old = captured(&w, "all-copy");
    let root = PathBuf::from(text(get(&old, "source"), "path"));
    let pack = PathBuf::from(text(get(&old, "source"), "pack"));
    // Lifecycle-only captures omit the legacy pack. Supply a real portable
    // inventory with the captured compatibility, without any Python bytes.
    let reader = source(&w);
    copy_tree(&reader.join(installation::RELEASE_PACK), &pack);
    let skill = pack.join("SKILL.md");
    fs::write(
        &skill,
        fs::read_to_string(&skill)
            .unwrap()
            .replacen(env!("CARGO_PKG_VERSION"), "0.0.14", 1),
    )
    .unwrap();
    fs::write(
        pack.join("schemas/runtime-compatibility.json"),
        installation::encoded(get(get(&old, "source"), "compatibility")).unwrap(),
    )
    .unwrap();
    let package = native(
        &w,
        &["artifact", "inspect", "--pack", pack.to_str().unwrap()],
    );
    assert_eq!(text(&package, "version"), "0.0.14");
    put(
        old.get_mut("source").unwrap(),
        "artifact_digest",
        get(&package, "artifact_digest").clone(),
    );
    let Value::Array(targets) = old.get_mut("targets").unwrap() else {
        panic!("captured targets")
    };
    for target in targets {
        if text(target, "path") == root.to_str().unwrap() {
            put(
                target,
                "identity",
                installation::fingerprint(&root).unwrap(),
            );
        }
    }
    let receipt_path = state(&w).join("receipt.json");
    fs::write(&receipt_path, installation::encoded(&old).unwrap()).unwrap();
    let mut journal = read(&state(&w).join("journal.json"));
    put(&mut journal, "receipt", old.clone());
    put(
        &mut journal,
        "receipt_identity",
        installation::fingerprint(&receipt_path).unwrap(),
    );
    fs::write(
        state(&w).join("journal.json"),
        installation::encoded(&journal).unwrap(),
    )
    .unwrap();
    let retained = snapshot(&root);
    let (path, original) = interrupt_captured_activation(&w, &old);
    let recovered = native(&w, &["recover"]);
    assert_eq!(text(&recovered, "status"), "rolled-back");
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(snapshot(&root), retained);
    fs::write(
        pack.join("templates/foreign.txt"),
        "Unowned changed reader.\n",
    )
    .unwrap();
    let drifted = snapshot(&root);
    let refused = run(&w, &["recover"], 1);
    assert_eq!(get(&refused, "verified"), &Value::Bool(false));
    assert_eq!(snapshot(&root), drifted);
}

#[test]
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
                "all",
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
    // The consumer is registered through a genuinely installed native command,
    // whose ownership is then removed by the real uninstaller before the
    // captured v0.0.14 installation state is laid down.
    let runtime = support::installed_runtime::install(&w.root, |argv| w.run(argv).unwrap());
    w.env.insert(
        "AGENT_MEMORY_TEST_OWNED_COMMAND".into(),
        runtime.command.clone().into_os_string(),
    );
    let repo = w.repository("registered-consumer", false).unwrap();
    let bank = bank(&w, &repo);
    support::identity_fixtures::bind_with_command(&w, &repo, &runtime.command).unwrap();
    assert_eq!(
        get(&native(&w, &["uninstall"]), "verified"),
        &Value::Bool(true)
    );
    w.env
        .remove(std::ffi::OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND"));
    let old = captured(&w, "all-copy");
    let source = PathBuf::from(text(get(&old, "source"), "path"));
    let old_source = snapshot(&source);
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
        &["all", "copy"],
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
    let version = channel.run(&w, "diagnostic", &["version"], 0);
    assert_eq!(get(&version, "ok"), &Value::Bool(true));
}
#[test]
// tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_actual_v002_index_refresh_and_deleted_worktree_upgrade_through_signed_one_liner
fn captured_old_source_and_deleted_worktree_adopt_to_signed_release() {
    signed_adoption(true, false);
}
#[test]
// tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_actual_v002_process_then_same_version_signed_adoption_under_022_and_002
fn captured_legacy_source_adopts_signed_release_with_collaborative_modes() {
    signed_adoption(false, false);
    signed_adoption(false, true);
}

#[test]
// tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_released_v014_sources_preview_then_upgrade_without_inventing_native_authority
fn captured_pending_sources_preview_and_adopt_without_inventing_native_authority() {
    let mut w = World::new().unwrap();
    // The captured fixture deliberately contains lifecycle records only
    // (derive.sh:9-19), not the pending source directory. Produce its unchanged
    // turn-source-v2 format through a real installed Codex prompt hook first.
    let runtime = support::installed_runtime::install(&w.root, |argv| w.run(argv).unwrap());
    w.env.insert(
        "AGENT_MEMORY_TEST_OWNED_COMMAND".into(),
        runtime.command.clone().into_os_string(),
    );
    let repo = w.repository("pending-consumer", false).unwrap();
    let memory = bank(&w, &repo);
    support::identity_fixtures::bind_with_command(&w, &repo, &runtime.command).unwrap();
    let event = support::cutover::object([
        ("hook_event_name", Value::String("UserPromptSubmit".into())),
        (
            "session_id",
            Value::String(agent_memory::host::uuid4().unwrap()),
        ),
        (
            "turn_id",
            Value::String(agent_memory::host::uuid4().unwrap()),
        ),
        ("cwd", Value::String(repo.to_str().unwrap().into())),
        (
            "prompt",
            Value::String("Public synthetic pre-barrier source.".into()),
        ),
    ]);
    let event = support::cutover::save(&w, &event, "pending-source-event.json");
    let hook = w
        .run([
            std::ffi::OsStr::new("/bin/sh"),
            std::ffi::OsStr::new("-c"),
            std::ffi::OsStr::new("exec \"$1\" hook codex-prompt < \"$2\""),
            std::ffi::OsStr::new("pending-source"),
            runtime.command.as_os_str(),
            event.as_os_str(),
        ])
        .unwrap();
    let hook = agent_memory::codec::json::decode_bytes(&hook.stdout, Default::default()).unwrap();
    assert!(hook.get("hookSpecificOutput").is_some(), "{hook:?}");
    native(&w, &["uninstall"]);
    w.env
        .remove(std::ffi::OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND"));
    let writes = w.paths["state"].join("agent-memory/writes");
    let sources = fs::read_dir(&writes)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("turn-sources-v2"))
        .find(|path| path.is_dir())
        .unwrap();
    let barrier = sources.parent().unwrap().join("source-barrier.lock");
    // The released hook predates this coordinator. Its source records remain
    // valid without a barrier; adoption may establish one, not rewrite sources.
    fs::remove_file(&barrier).unwrap();
    captured(&w, "codex-copy-pending-source");
    let source = source(&w);
    let before = snapshot(&sources);
    let before_bank = snapshot(&memory);
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
    assert_eq!(snapshot(&sources), before);
    assert_eq!(snapshot(&memory), before_bank);
    assert!(barrier.is_file());
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
    assert_eq!(snapshot(&sources), before);
    assert_eq!(snapshot(&memory), before_bank);
}

#[test]
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
    let installed = install(&w, &native_source, &["--presets", "all"]);
    assert_eq!(get(&installed, "verified"), &Value::Bool(true));
    assert_eq!(snapshot(&source), before);
    let command = command(get(&installed, "receipt"));
    let observed = w
        .run([command.as_os_str(), "--json".as_ref(), "doctor".as_ref()])
        .unwrap();
    let observed =
        agent_memory::codec::json::decode_bytes(&observed.stdout, Default::default()).unwrap();
    assert_eq!(get(&observed, "ok"), &Value::Bool(true), "{observed:?}");
}
