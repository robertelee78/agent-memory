#![cfg(feature = "dev-tools")]
mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::distribution::Channel;
use support::lifecycle::*;
use support::{RunOptions, World};

fn invoke(w: &World, binary: &Path, args: &[&str], expected: i32) -> Value {
    let output = w
        .run_with(
            std::iter::once(binary.as_os_str()).chain(
                std::iter::once(std::ffi::OsStr::new("--json"))
                    .chain(args.iter().map(std::ffi::OsStr::new)),
            ),
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(expected), "{output:?}");
    if output.stdout.is_empty() {
        Value::Null
    } else {
        json::decode_bytes(&output.stdout, Default::default()).unwrap()
    }
}
fn copy_exact(from: &Path, to: &Path) {
    let metadata = fs::symlink_metadata(from).unwrap();
    if metadata.is_symlink() {
        symlink(fs::read_link(from).unwrap(), to).unwrap();
    } else if metadata.is_dir() {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            copy_exact(&entry.path(), &to.join(entry.file_name()));
        }
        fs::set_permissions(to, metadata.permissions()).unwrap();
    } else {
        fs::copy(from, to).unwrap();
        fs::set_permissions(to, metadata.permissions()).unwrap();
    }
}
fn remove_owned(path: &Path) {
    if let Ok(m) = fs::symlink_metadata(path) {
        if m.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
}
fn traced(w: &World, args: &[&str], trace: &Path, stop: Option<&str>, expected: i32) -> Value {
    let mut env = vec![(
        "AGENT_MEMORY_FAULT_TRACE".into(),
        trace.as_os_str().to_owned(),
    )];
    if let Some(stage) = stop {
        env.extend([
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
        ]);
    }
    let output = w
        .am_with(
            std::iter::once("--json").chain(args.iter().copied()),
            RunOptions {
                env: &env,
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(expected), "{output:?}");
    if output.stdout.is_empty() {
        Value::Null
    } else {
        json::decode_bytes(&output.stdout, Default::default()).unwrap()
    }
}
fn trace_rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| json::decode(line, Default::default()).unwrap())
        .collect()
}
fn semantics(receipt: &Value) -> Value {
    let mut value = receipt.clone();
    let Value::Object(rows) = &mut value else {
        panic!("receipt")
    };
    rows.retain(|(key, _)| {
        [
            "schema",
            "mode",
            "presets",
            "custom_targets",
            "enabled_adapters",
            "bin_dir",
            "data_roots",
            "memory_data_location",
            "runtime_state",
            "uninstalled",
            "installation_id",
            "source",
        ]
        .contains(&key.as_str())
    });
    value
}
fn recover_replay(committed: bool, phase: &str) {
    let w = World::new().unwrap();
    let source = source(&w);
    fs::write(
        w.paths["codex"].join("AGENTS.md"),
        "Keep original developer instructions.\n",
    )
    .unwrap();
    let repo = w.repository("consumer", false).unwrap();
    let memory = bank(&w, &repo);
    let bank_before = snapshot(&memory);
    let installed = install(&w, &source, &["--mode", "copy", "--presets", "codex"]);
    let receipt_before = get(&installed, "receipt").clone();
    let fault_stage = if committed {
        "uninstall-after-receipt"
    } else {
        "uninstall-after-activate-0"
    };
    let output = fault(&w, &["uninstall"], fault_stage, "exit");
    assert_eq!(output.status, Status::Code(86), "{output:?}");
    let saved = w.root.join("saved-recovery-input");
    fs::create_dir(&saved).unwrap();
    let parts = [
        "data", "state", "home", "codex", "claude", "config", "opencode", "backups",
    ];
    for name in parts {
        copy_exact(&w.paths[name], &saved.join(name));
    }
    let clean_trace = w.root.join("clean.jsonl");
    let clean = traced(&w, &["recover"], &clean_trace, None, 0);
    assert_eq!(get(&clean, "verified"), &Value::Bool(true));
    let expected = read(&state(&w).join("receipt.json"));
    let target_paths = get(&expected, "targets")
        .as_array()
        .unwrap()
        .iter()
        .map(|target| PathBuf::from(text(target, "path")))
        .collect::<Vec<_>>();
    let targets = target_paths
        .iter()
        .map(|path| (path.clone(), path.exists().then(|| snapshot(path))))
        .collect::<Vec<_>>();
    let rows = trace_rows(&clean_trace);
    assert!(!rows.is_empty());
    assert_eq!(rows.len() % 2, 0);
    for (step, pair) in rows.chunks_exact(2).enumerate() {
        assert_eq!(get(&pair[0], "step"), &Value::Int(step.into()));
        assert_eq!(get(&pair[1], "step"), &Value::Int(step.into()));
        assert_eq!(text(&pair[0], "phase"), "before");
        assert_eq!(text(&pair[1], "phase"), "after");
        assert_eq!(get(&pair[0], "kind"), get(&pair[1], "kind"));
    }
    let events = rows
        .iter()
        .filter(|row| text(row, "phase") == phase)
        .collect::<Vec<_>>();
    assert!(!events.is_empty());
    assert!(
        rows.iter()
            .any(|row| text(row, "kind") == "record-publication")
    );
    let mut executed = Vec::new();
    for event in &events {
        for name in parts {
            remove_owned(&w.paths[name]);
            copy_exact(&saved.join(name), &w.paths[name]);
        }
        remove_owned(&w.paths["tmp"]);
        fs::create_dir(&w.paths["tmp"]).unwrap();
        mode(&w.paths["tmp"], 0o700);
        let trace = w.root.join("actual.jsonl");
        remove_owned(&trace);
        traced(&w, &["recover"], &trace, Some(text(event, "point")), 86);
        assert_eq!(trace_rows(&trace).last(), Some(*event));
        assert_eq!(
            get(&run(&w, &["recover"], 0), "verified"),
            &Value::Bool(true)
        );
        assert_eq!(
            get(&run(&w, &["recover"], 0), "verified"),
            &Value::Bool(true)
        );
        let current = read(&state(&w).join("receipt.json"));
        assert_eq!(semantics(&current), semantics(&expected));
        assert_eq!(
            get(&current, "installation_id"),
            get(&receipt_before, "installation_id")
        );
        for (path, expected) in &targets {
            assert_eq!(path.exists().then(|| snapshot(path)), *expected);
        }
        assert_eq!(snapshot(&memory), bank_before);
        let stable = fs::read(state(&w).join("receipt.json")).unwrap();
        assert_eq!(
            get(&run(&w, &["recover"], 0), "verified"),
            &Value::Bool(true)
        );
        assert_eq!(fs::read(state(&w).join("receipt.json")).unwrap(), stable);
        executed.push(text(event, "point").to_owned());
    }
    assert_eq!(
        executed,
        events
            .iter()
            .map(|event| text(event, "point").to_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_recover_commit_write_copy_after.py::LifecycleWriteContract.test_after_every_recover_commit_copy_write
fn after_every_recover_commit_copy_write() {
    recover_replay(true, "after");
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_recover_commit_write_copy_before.py::LifecycleWriteContract.test_before_every_recover_commit_copy_write
fn before_every_recover_commit_copy_write() {
    recover_replay(true, "before");
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_recover_rollback_write_copy_after.py::LifecycleWriteContract.test_after_every_recover_rollback_copy_write
fn after_every_recover_rollback_copy_write() {
    recover_replay(false, "after");
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_recover_rollback_write_copy_before.py::LifecycleWriteContract.test_before_every_recover_rollback_copy_write
fn before_every_recover_rollback_copy_write() {
    recover_replay(false, "before");
}

fn retired_channel(symlinked: bool) {
    let w = World::new().unwrap();
    let repo = w.repository("managed-source", false).unwrap();
    fs::write(repo.join("keep.txt"), "Retained local source and index.\n").unwrap();
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..Default::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Inert managed history"],
        RunOptions {
            cwd: Some(&repo),
            ..Default::default()
        },
    )
    .unwrap();
    let alias = w.root.join("source-alias");
    if symlinked {
        symlink(repo.parent().unwrap(), &alias).unwrap();
    }
    let selected = if symlinked {
        alias.join(repo.file_name().unwrap())
    } else {
        repo.clone()
    };
    let before = snapshot(&w.root);
    for check in [false, true] {
        let mut args = vec![
            "install",
            "--remote",
            selected.to_str().unwrap(),
            "--ref",
            "main",
            "--presets",
            "none",
        ];
        if check {
            args.push("--check");
        }
        let denied = run(&w, &args, 1);
        assert_eq!(get(&denied, "verified"), &Value::Bool(false));
        assert_eq!(
            text(&denied, "error"),
            agent_memory::installation::MANAGED_GUIDANCE
        );
        assert_eq!(snapshot(&w.root), before);
    }
}
#[test]
// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_fast_forward_outside_the_pack_needs_no_client_restart
fn retired_managed_fast_forward_does_not_activate_or_restart() {
    retired_channel(false);
}
#[test]
// obligation: tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_interrupted_switch_restores_exact_drifted_source_and_retries
fn retired_managed_switch_preserves_exact_source_on_every_refusal() {
    retired_channel(false);
}
#[test]
// obligation: tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_managed_update_accepts_repository_under_symlinked_ancestor
fn retired_managed_channel_refuses_even_under_symlinked_ancestor() {
    retired_channel(true);
}

#[test]
// obligation: tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_parent_substitution_before_activation_preserves_targets_and_recovers
fn parent_substitution_before_activation_preserves_targets_and_recovers() {
    let w = World::new().unwrap();
    let source = source(&w);
    let public = w.root.join("public-bin");
    fs::create_dir(&public).unwrap();
    mode(&public, 0o755);
    let directory = w.root.join("pause");
    let stage = "install-before-activate-1";
    let env = [
        ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
        ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
        (
            "AGENT_MEMORY_FAULT_DIR".into(),
            directory.clone().into_os_string(),
        ),
    ];
    let child = w
        .spawn_with(
            [
                OsString::from(env!("CARGO_BIN_EXE_am")),
                "--json".into(),
                "install".into(),
                "--development-source".into(),
                source.into_os_string(),
                "--presets".into(),
                "none".into(),
                "--bin-dir".into(),
                public.clone().into_os_string(),
            ],
            RunOptions {
                env: &env,
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(15)).unwrap();
    while !directory.join(format!("{stage}.ready")).exists() {
        assert!(!deadline.expired(), "activation rendezvous missing");
        std::thread::sleep(Duration::from_millis(5));
    }
    let saved = w.root.join("original-bin");
    fs::rename(&public, &saved).unwrap();
    let replacement = w.root.join("replacement-bin");
    fs::create_dir(&replacement).unwrap();
    symlink(&replacement, &public).unwrap();
    fs::write(directory.join(format!("{stage}.release")), b"release").unwrap();
    assert_eq!(child.wait().unwrap().status, Status::Code(1));
    assert!(!replacement.join("am").exists());
    assert!(!replacement.join("agent-memory").exists());
    fs::remove_file(&public).unwrap();
    fs::rename(saved, public).unwrap();
    assert_eq!(
        get(&run(&w, &["recover"], 0), "verified"),
        &Value::Bool(true)
    );
}

#[test]
// obligation: tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_private_temp_barrier_native_alias_and_group_write_payloads_work
fn private_temp_barrier_native_alias_and_group_write_payloads_work() {
    for copy in [false, true] {
        let mut w = World::new().unwrap();
        let source = source(&w);
        mode(&source, 0o700);
        for path in [source.join("skills/agent-memory/SKILL.md")] {
            let bits = fs::metadata(&path).unwrap().permissions().mode();
            mode(&path, (bits & 0o777) | 0o020);
        }
        let alias = w.root.join("temp-alias");
        symlink(&w.paths["tmp"], &alias).unwrap();
        w.env.insert("TMPDIR".into(), alias.into_os_string());
        mode(&w.root, 0o755);
        let public = w.root.join("public-skills");
        fs::create_dir(&public).unwrap();
        mode(&public, 0o755);
        let source_before = snapshot(&source);
        let output = w
            .run_with(
                [
                    OsString::from("/bin/sh"),
                    "-c".into(),
                    "umask 002; exec \"$@\"".into(),
                    "native-install".into(),
                    env!("CARGO_BIN_EXE_am").into(),
                    OsString::from("--json"),
                    "install".into(),
                    "--development-source".into(),
                    source.as_os_str().to_owned(),
                    "--mode".into(),
                    if copy { "copy" } else { "link" }.into(),
                    "--presets".into(),
                    "codex".into(),
                    "--target-dir".into(),
                    public.as_os_str().to_owned(),
                ],
                RunOptions {
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(output.status, Status::Code(0));
        // Explicit development sources remain user-owned; the native cutover
        // does not chmod them into a managed checkout.
        assert_eq!(snapshot(&source), source_before);
        assert_eq!(
            fs::metadata(state(&w)).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_ne!(
            fs::metadata(source.join("skills/agent-memory/SKILL.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o020,
            0
        );
        if copy {
            assert_eq!(
                fs::metadata(public.join("agent-memory"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        assert_eq!(
            get(&run(&w, &["recover"], 0), "verified"),
            &Value::Bool(true)
        );
        assert_eq!(
            get(&run(&w, &["uninstall"], 0), "verified"),
            &Value::Bool(true)
        );
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_fresh_and_existing_install_check_and_apply_refuse_unsupported_bank_without_activation
fn unsupported_bank_refuses_fresh_and_existing_install_without_activation() {
    for existing in [false, true] {
        for check in [false, true] {
            let w = World::new().unwrap();
            let source = source(&w);
            if existing {
                install(&w, &source, &["--presets", "none"]);
            }
            let repo = w.repository("consumer", false).unwrap();
            let memory = bank(&w, &repo);
            let router = memory.join("MEMORY.md");
            fs::write(
                &router,
                fs::read_to_string(&router)
                    .unwrap()
                    .replace("agent-memory/v1", "agent-memory/v999"),
            )
            .unwrap();
            let before_bank = snapshot(&memory);
            let before_source = snapshot(&source);
            let before_bin = w.paths["home"]
                .join(".local/bin")
                .exists()
                .then(|| snapshot(&w.paths["home"].join(".local/bin")));
            let mut args = vec![
                "install",
                "--development-source",
                source.to_str().unwrap(),
                "--presets",
                "none",
                "--repo",
                repo.to_str().unwrap(),
            ];
            if check {
                args.push("--check");
            }
            let denied = run(&w, &args, 1);
            assert_eq!(get(&denied, "verified"), &Value::Bool(false));
            assert_eq!(snapshot(&memory), before_bank);
            assert_eq!(snapshot(&source), before_source);
            assert_eq!(
                w.paths["home"]
                    .join(".local/bin")
                    .exists()
                    .then(|| snapshot(&w.paths["home"].join(".local/bin"))),
                before_bin
            );
        }
    }
}

#[test]
// obligation: tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_missing_signature_verifier_refuses_before_lifecycle_state_is_created
fn missing_signature_verifier_refuses_before_lifecycle_state_is_created() {
    let mut w = World::new().unwrap();
    let channel = Channel::new(&w);
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let tools = w.root.join("old-openssh");
    fs::create_dir(&tools).unwrap();
    fs::write(tools.join("ssh-keygen"), "#!/bin/sh\nexit 1\n").unwrap();
    mode(&tools.join("ssh-keygen"), 0o755);
    let old = w.env[std::ffi::OsStr::new("PATH")].clone();
    w.env.insert(
        "PATH".into(),
        std::env::join_paths(std::iter::once(tools).chain(std::env::split_paths(&old))).unwrap(),
    );
    let before = (snapshot(&w.paths["state"]), snapshot(&w.paths["data"]));
    assert_eq!(
        get(
            &channel.run(&w, "install", &["none", "copy"], 1),
            "verified"
        ),
        &Value::Bool(false)
    );
    assert_eq!(
        (snapshot(&w.paths["state"]), snapshot(&w.paths["data"])),
        before
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_one_line_install_update_failures_recovery_and_preservation
fn signed_install_update_failures_recovery_and_preservation() {
    let mut w = World::new().unwrap();
    let channel = Channel::new(&w);
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let instructions = w.paths["codex"].join("AGENTS.md");
    fs::write(&instructions, "Keep my instructions.\n").unwrap();
    let installed = channel.install(&w, "codex", "copy");
    let receipt = get(&installed, "receipt");
    let source = PathBuf::from(text(get(receipt, "source"), "path"));
    let before = snapshot(&source);
    let receipt_before = fs::read(state(&w).join("receipt.json")).unwrap();
    let manifest = channel.publish("99.0.0", &"2".repeat(40));
    let assets = channel.root.join("assets/v99.0.0");
    for name in [
        agent_memory::release_distribution::MANIFEST,
        agent_memory::release_distribution::SIGNATURE,
        agent_memory::release_distribution::ARCHIVE,
    ] {
        let path = assets.join(name);
        let raw = fs::read(&path).unwrap();
        let mut corrupt = raw.clone();
        let middle = corrupt.len() / 2;
        corrupt[middle] ^= 1;
        fs::write(&path, corrupt).unwrap();
        if name == agent_memory::release_distribution::MANIFEST {
            fs::write(
                channel.root.join("latest").join(name),
                fs::read(&path).unwrap(),
            )
            .unwrap();
        }
        let rejected = channel.run(&w, "update", &[], 1);
        assert_eq!(get(&rejected, "verified"), &Value::Bool(false));
        assert_eq!(snapshot(&source), before);
        assert_eq!(
            fs::read(state(&w).join("receipt.json")).unwrap(),
            receipt_before
        );
        fs::write(&path, &raw).unwrap();
        if name == agent_memory::release_distribution::MANIFEST {
            fs::write(channel.root.join("latest").join(name), raw).unwrap();
        }
    }
    let signature = assets.join(agent_memory::release_distribution::SIGNATURE);
    let raw = fs::read(&signature).unwrap();
    fs::remove_file(&signature).unwrap();
    channel.run(&w, "update", &[], 1);
    fs::write(signature, raw).unwrap();
    assert_eq!(snapshot(&source), before);
    channel.fault(&mut w, "update", "update-after-activate-0");
    assert_eq!(
        get(&channel.run(&w, "recover", &[], 0), "verified"),
        &Value::Bool(true)
    );
    assert_eq!(snapshot(&source), before);
    let updated = channel.run(&w, "update", &[], 0);
    assert_eq!(get(&updated, "verified"), &Value::Bool(true));
    assert_eq!(
        text(get(get(&updated, "receipt"), "source"), "version"),
        manifest.version
    );
    let memory = w.paths["repo"].join("memory-bank");
    fs::create_dir(&memory).unwrap();
    fs::write(memory.join("keep.md"), "Consumer memory stays.\n").unwrap();
    let bank_before = snapshot(&memory);
    channel.run(&w, "uninstall", &[], 0);
    assert_eq!(snapshot(&memory), bank_before);
    assert_eq!(
        fs::read_to_string(instructions).unwrap(),
        "Keep my instructions.\n"
    );
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    channel.install(&w, "none", "link");
    assert_eq!(snapshot(&memory), bank_before);
}

fn piped_install(w: &World, channel: &Channel, source: &Path, check: bool) -> String {
    let base = format!(
        "{}/",
        support::distribution::file_url(
            &channel
                .root
                .join("assets")
                .join(format!("v{}", env!("CARGO_PKG_VERSION")))
        )
    );
    let signers = fs::read_to_string(channel.root.join("allowed-signers")).unwrap();
    let script = include_str!("../install.sh")
        .lines()
        .map(|line| {
            if line.starts_with("AM_RELEASE_BASE=") {
                format!("AM_RELEASE_BASE='{base}'")
            } else if line.starts_with("AM_PROTOCOLS=") {
                "AM_PROTOCOLS='=file'".into()
            } else if line.starts_with("AM_ALLOWED_SIGNERS=") {
                format!("AM_ALLOWED_SIGNERS='{}'", signers.trim())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut args = vec![
        OsString::from("/bin/sh"),
        "-s".into(),
        "--".into(),
        "--development-source".into(),
        source.as_os_str().to_owned(),
        "--presets".into(),
        "codex".into(),
    ];
    if check {
        args.push("--check".into());
    }
    let env = w
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    let output = agent_memory::process::run(&agent_memory::process::Spec {
        argv: &args,
        env: &env,
        cwd: Some(&w.paths["repo"]),
        search_path: false,
        stdin: agent_memory::process::Input::Bytes(script.as_bytes()),
        stdout: agent_memory::process::Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        stderr: agent_memory::process::Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(60)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .unwrap();
    assert_eq!(output.status, Status::Code(0), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_installer.py::OneLineBootstrapContract.test_pipe_preview_repeat_update_and_uninstall_preserve_memory_and_user_instructions
fn pipe_preview_repeat_update_and_uninstall_preserve_memory_and_instructions() {
    let w = World::new().unwrap();
    let channel = Channel::new(&w);
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let source = source(&w);
    let memory = w.paths["repo"].join("memory-bank");
    fs::create_dir(&memory).unwrap();
    fs::write(memory.join("keep.md"), "Retain canonical memory.\n").unwrap();
    let before_bank = snapshot(&memory);
    let instructions = w.paths["codex"].join("AGENTS.md");
    fs::write(&instructions, "Retain personal instructions.\n").unwrap();
    let before = snapshot(&w.root);
    assert!(piped_install(&w, &channel, &source, true).contains("preview"));
    assert_eq!(snapshot(&w.root), before);
    piped_install(&w, &channel, &source, false);
    let raw = fs::read(state(&w).join("receipt.json")).unwrap();
    piped_install(&w, &channel, &source, false);
    assert_eq!(fs::read(state(&w).join("receipt.json")).unwrap(), raw);
    let receipt = read(&state(&w).join("receipt.json"));
    let binary = command(&receipt);
    let denied = invoke(&w, &binary, &["update", "--check"], 1);
    assert_eq!(get(&denied, "verified"), &Value::Bool(false));
    assert_eq!(snapshot(&memory), before_bank);
    assert_eq!(
        get(&invoke(&w, &binary, &["uninstall"], 0), "verified"),
        &Value::Bool(true)
    );
    assert!(!binary.exists());
    assert_eq!(snapshot(&memory), before_bank);
    assert_eq!(
        fs::read_to_string(instructions).unwrap(),
        "Retain personal instructions.\n"
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_portable_lifecycle.py::PortableLifecycleContract.test_link_custom_only_updates_without_optional_clients_identity_or_a_bank
fn link_custom_only_preserves_neighbors_without_identity_or_a_bank() {
    let w = World::new().unwrap();
    let channel = Channel::new(&w);
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let custom = [w.root.join("custom one"), w.root.join("custom two")];
    for target in &custom {
        fs::create_dir_all(target.join("neighbor")).unwrap();
        fs::write(target.join("neighbor/SKILL.md"), "# Keep neighbor\n").unwrap();
    }
    let neighbors = custom
        .iter()
        .map(|p| snapshot(&p.join("neighbor")))
        .collect::<Vec<_>>();
    let defaults = ["codex", "claude", "opencode"].map(|name| snapshot(&w.paths[name]));
    let targets = custom
        .iter()
        .map(|path| format!("target={}", path.display()))
        .collect::<Vec<_>>();
    let installed = channel.run(
        &w,
        "install",
        &["none", "link", &targets[0], &targets[1]],
        0,
    );
    let receipt = get(&installed, "receipt");
    let cli = command(receipt);
    assert!(
        !get(receipt, "targets")
            .as_array()
            .unwrap()
            .iter()
            .any(|t| text(t, "kind") == "activation")
    );
    assert_eq!(
        text(get(&invoke(&w, &cli, &["status"], 0), "bank"), "state"),
        "not_located"
    );
    channel.publish("99.0.0", &"2".repeat(40));
    let before = snapshot(&w.root);
    let checked = channel.run(&w, "update", &["--check"], 0);
    assert_eq!(get(&checked, "update_available"), &Value::Bool(true));
    assert_eq!(snapshot(&w.root), before);
    let updated = channel.run(&w, "update", &[], 0);
    assert_eq!(get(&updated, "verified"), &Value::Bool(true));
    let repeated = channel.run(&w, "update", &[], 0);
    assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
    assert_eq!(get(&repeated, "restart_clients"), &Value::Array(vec![]));
    channel.run(&w, "uninstall", &[], 0);
    assert_eq!(
        ["codex", "claude", "opencode"].map(|name| snapshot(&w.paths[name])),
        defaults
    );
    assert_eq!(
        custom
            .iter()
            .map(|p| snapshot(&p.join("neighbor")))
            .collect::<Vec<_>>(),
        neighbors
    );
    assert!(!w.paths["repo"].join("memory-bank").exists());
    assert!(
        !w.paths["config"]
            .join("agent-memory/developer.json")
            .exists()
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_portable_lifecycle.py::PortableLifecycleContract.test_copy_all_clients_keep_qualified_memory_through_update_optional_state_loss_uninstall_and_reinstall
fn copy_all_clients_keep_qualified_memory_through_full_lifecycle() {
    let r = support::cutover::Recall::new();
    let w = &r.world;
    let path = r.add(
        "layout",
        "# Shared learning\n\nShared learning remains qualified after runtime removal.\n",
        &support::cutover::object([(
            "conditions",
            support::cutover::object([("component", support::cutover::text("app.rs"))]),
        )]),
    );
    let request = r.request(
        &[path.clone()],
        support::cutover::object([("component", support::cutover::text("app.rs"))]),
    );
    let request_path = support::cutover::save(w, &request, "portable-recall.json");
    let bank_before = snapshot(&r.bank);
    let git_before = r.code();
    let instructions = [
        w.paths["codex"].join("AGENTS.md"),
        w.paths["claude"].join("CLAUDE.md"),
        w.paths["config"].join("opencode/AGENTS.md"),
    ];
    for path in &instructions {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "Retain original matrix user instructions.\n").unwrap();
    }
    let channel = Channel::new(w);
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let custom = w.root.join("portable custom skills");
    let target = format!("target={}", custom.display());
    let installed = channel.run(w, "install", &["all", "copy", &target], 0);
    let receipt = get(&installed, "receipt");
    let cli = command(receipt);
    let stable = fs::read(state(w).join("receipt.json")).unwrap();
    let repeated = channel.run(w, "install", &["all", "copy", &target], 0);
    assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
    assert_eq!(fs::read(state(w).join("receipt.json")).unwrap(), stable);
    let args = [
        "get",
        "--repo",
        r.repo.to_str().unwrap(),
        "--request",
        request_path.to_str().unwrap(),
    ];
    let recalled = invoke(w, &cli, &args, 0);
    assert_eq!(get(&recalled, "results").as_array().unwrap().len(), 1);
    assert_eq!(
        get(
            &channel.run(w, "doctor", &[r.repo.to_str().unwrap()], 0),
            "ok"
        ),
        &Value::Bool(true)
    );
    let manifest = channel.publish("99.0.0", &"2".repeat(40));
    let before = snapshot(&w.root);
    let preview = channel.run(w, "update", &["--check"], 0);
    assert_eq!(get(&preview, "check_complete"), &Value::Bool(true));
    assert_eq!(get(&preview, "update_available"), &Value::Bool(true));
    assert_eq!(get(&preview, "verified"), &Value::Bool(false));
    assert_eq!(snapshot(&w.root), before);
    let updated = channel.run(w, "update", &[], 0);
    assert_eq!(get(&updated, "verified"), &Value::Bool(true));
    assert_eq!(
        text(get(get(&updated, "receipt"), "source"), "version"),
        manifest.version
    );
    assert!(
        !get(&updated, "restart_clients")
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        get(&invoke(w, &cli, &args, 0), "results"),
        get(&recalled, "results")
    );
    // Only derived, non-authoritative stores disappear; registry, source
    // completion, recovery and provenance receipts are never substituted.
    let caches = [
        w.paths["cache"].join("agent-memory-navigation"),
        w.paths["cache"].join("agent-memory-projection"),
    ];
    for path in &caches {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("derived.json"), "{\"derived\":true}\n").unwrap();
        fs::remove_dir_all(path).unwrap();
    }
    assert_eq!(
        get(&invoke(w, &cli, &args, 0), "results"),
        get(&recalled, "results")
    );
    assert_eq!(snapshot(&r.bank), bank_before);
    assert_eq!(r.code(), git_before);
    channel.run(w, "uninstall", &[], 0);
    assert!(!cli.exists());
    assert!(fs::metadata(r.bank.join("MEMORY.md")).unwrap().len() <= 16384);
    assert!(
        fs::read_to_string(r.bank.join("MEMORY.md"))
            .unwrap()
            .contains(&path)
    );
    assert!(
        fs::read_to_string(r.bank.join(&path))
            .unwrap()
            .contains("Shared learning remains")
    );
    for path in &instructions {
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "Retain original matrix user instructions.\n"
        );
    }
    channel.publish(env!("CARGO_PKG_VERSION"), &"1".repeat(40));
    let restored = channel.run(w, "install", &["all", "copy", &target], 0);
    let cli = command(get(&restored, "receipt"));
    assert_eq!(
        get(&invoke(w, &cli, &args, 0), "results"),
        get(&recalled, "results")
    );
    let before = snapshot(&w.root);
    let preview = invoke(
        w,
        &cli,
        &[
            "purge-data",
            "--repo",
            r.repo.to_str().unwrap(),
            "--path",
            r.bank.to_str().unwrap(),
            "--check",
        ],
        0,
    );
    assert!(
        get(get(&preview, "preview"), "confirmation")
            .as_str()
            .is_some()
    );
    assert_eq!(snapshot(&w.root), before);
    channel.run(w, "uninstall", &[], 0);
    assert_eq!(snapshot(&r.bank), bank_before);
    assert_eq!(r.code(), git_before);
}
