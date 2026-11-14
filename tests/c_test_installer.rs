use sha2::{Digest, Sha256};
mod support;
use agent_memory::codec::json::Value;
use support::World;
use support::lifecycle::*;

const BEGIN: &str = "<!-- agent-memory:activation:begin -->";
const END: &str = "<!-- agent-memory:activation:end -->";

// obligation: tests/contracts/test_installer.py::InstallerContract.test_managed_source_local_channel_and_external_receipt
#[test]
fn managed_git_channel_refuses_without_writes() {
    let world = World::new().unwrap();
    let remote = world.repository("local channel", false).unwrap();
    let before = snapshot(&world.root);
    let value = run(
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
    assert!(text(&value, "error").contains("Managed Git"), "{value:?}");
    assert_eq!(snapshot(&world.root), before);
    assert!(!state(&world).join("receipt.json").exists());
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_codex_stop_install_preserves_foreign_settings_and_uninstalls
#[test]

fn codex_stop_preserves_foreign_settings_and_uninstalls() {
    let world = World::new().unwrap();
    let source = source(&world);
    let settings = world.paths["codex"].join("hooks.json");
    let original =
        br#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"foreign-stop"}]}]}}"#;
    std::fs::write(&settings, original).unwrap();
    install(&world, &source, &["--presets", "codex"]);
    let value = read(&settings);
    let rows = get(get(&value, "hooks"), "Stop").as_array().unwrap();
    let foreign = agent_memory::codec::json::decode_bytes(original, Default::default()).unwrap();
    assert_eq!(
        rows.first(),
        get(get(&foreign, "hooks"), "Stop")
            .as_array()
            .unwrap()
            .first()
    );
    let hooks = get(&rows[1], "hooks").as_array().unwrap();
    assert!(text(&hooks[0], "command").contains("hook codex-stop"));
    assert_eq!(get(&hooks[0], "async"), &Value::Null);
    run(&world, &["uninstall"], 0);
    assert_eq!(read(&settings), foreign);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_claude_stop_ownership_preserves_foreign_hooks_and_tampering
#[test]

fn claude_stop_preserves_foreign_and_modified_owned_hooks() {
    let world = World::new().unwrap();
    let source = source(&world);
    let settings = world.paths["claude"].join("settings.json");
    std::fs::write(
        &settings,
        br#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"foreign-stop"}]}]}}"#,
    )
    .unwrap();
    install(&world, &source, &["--presets", "claude"]);
    let raw = std::fs::read_to_string(&settings).unwrap();
    assert!(raw.contains("foreign-stop"));
    assert!(raw.contains("\"asyncRewake\": true"));
    assert!(raw.contains("hook claude-stop"));
    let modified = raw.replace("hook claude-stop", "changed-by-user");
    std::fs::write(&settings, &modified).unwrap();
    let removed = run(&world, &["uninstall"], 0);
    assert!(
        get(&removed, "preserved")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| text(row, "path") == settings.to_str().unwrap())
    );
    assert_eq!(std::fs::read_to_string(settings).unwrap(), modified);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_opencode_versioned_hook_is_owned_and_removed
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn opencode_hook_is_owned_and_removed_without_touching_other_plugins() {
    for (name, version) in [("opencode", "1.19.3"), ("opencode2", "opencode v2.1.3")] {
        let mut world = World::new().unwrap();
        let source = source(&world);
        let bin = world.root.join("fake-bin");
        std::fs::create_dir(&bin).unwrap();
        let tool = bin.join(name);
        std::fs::write(&tool, format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n")).unwrap();
        mode(&tool, 0o755);
        let path = format!(
            "{}:{}",
            bin.display(),
            world.env[std::ffi::OsStr::new("PATH")].to_string_lossy()
        );
        world.env.insert("PATH".into(), path.into());
        let plugin = world.paths["opencode"].join("plugins/agent-memory.js");
        std::fs::create_dir_all(plugin.parent().unwrap()).unwrap();
        let other = plugin.with_file_name("other.js");
        std::fs::write(&other, b"foreign plugin\n").unwrap();
        install(&world, &source, &["--presets", "opencode"]);
        assert!(plugin.is_symlink());
        assert!(
            std::fs::read_link(&plugin)
                .unwrap()
                .ends_with("hooks/opencode.js")
        );
        run(&world, &["uninstall"], 0);
        assert!(std::fs::symlink_metadata(&plugin).is_err());
        assert_eq!(std::fs::read(other).unwrap(), b"foreign plugin\n");
    }
}

// obligation: tests/contracts/test_installer.py::MacOSCommandContract.test_uninstall_preserves_modified_am_and_runtime_while_removing_unchanged_compatibility_wrapper
#[test]

fn uninstall_preserves_modified_command_and_its_runtime() {
    let world = World::new().unwrap();
    let source = source(&world);
    let installed = install(&world, &source, &["--presets", "none"]);
    let receipt = get(&installed, "receipt");
    let cli = command(receipt);
    std::fs::remove_file(&cli).unwrap();
    std::fs::write(&cli, b"My modified am wrapper.\n").unwrap();
    let removed = run(&world, &["uninstall"], 0);
    assert_eq!(std::fs::read(&cli).unwrap(), b"My modified am wrapper.\n");
    assert!(!cli.with_file_name("agent-memory").exists());
    assert!(source.exists());
    assert!(
        get(&removed, "preserved")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| text(row, "path") == cli.to_str().unwrap())
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_omp_missing_or_malformed_candidate_activation_refuses_without_fallback
#[test]

fn omp_missing_or_malformed_candidate_activation_has_no_fallback() {
    let long = format!("{BEGIN}\n{}\n{END}", "x".repeat(4096));
    let duplicate = format!("{BEGIN}\n{BEGIN}\n{END}{END}");
    let reversed = format!("{END}\n{BEGIN}");
    let outside = format!("Foreign prefix\n{BEGIN}\n{END}");
    for mode_name in ["copy", "link"] {
        for content in [
            None,
            Some(long.as_str()),
            Some(&duplicate),
            Some(&reversed),
            Some(&outside),
        ] {
            let world = World::new().unwrap();
            let source = source(&world);
            let template =
                source.join("skills/agent-memory/templates/activation-omp-markdown.example.txt");
            if let Some(content) = content {
                std::fs::write(&template, content).unwrap();
            } else {
                std::fs::remove_file(&template).unwrap();
            }
            let root = world.root.join("omp root");
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("AGENTS.md"), b"Foreign instructions.\n").unwrap();
            let before = (snapshot(&root), snapshot(&source));
            let refused = run(
                &world,
                &[
                    "install",
                    "--development-source",
                    source.to_str().unwrap(),
                    "--presets",
                    "omp",
                    "--omp-agent-dir",
                    root.to_str().unwrap(),
                    "--omp-peers-upgraded",
                    "--mode",
                    mode_name,
                ],
                1,
            );
            assert_eq!(get(&refused, "verified"), &Value::Bool(false));
            assert_eq!((snapshot(&root), snapshot(&source)), before);
            assert!(!state(&world).join("receipt.json").exists());
            assert!(!world.paths["home"].join(".local/bin/am").exists());
        }
    }
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_omp_modified_or_foreign_activation_is_preserved
#[test]

fn omp_modified_or_foreign_activation_is_preserved() {
    for mode_name in ["copy", "link"] {
        for owned in [false, true] {
            let world = World::new().unwrap();
            let source = source(&world);
            let root = world.root.join("omp preserved root");
            std::fs::create_dir(&root).unwrap();
            let options = [
                "--presets",
                "omp",
                "--omp-agent-dir",
                root.to_str().unwrap(),
                "--omp-peers-upgraded",
                "--mode",
                mode_name,
            ];
            let config = root.join("AGENTS.md");
            if owned {
                install(&world, &source, &options);
                let raw = std::fs::read_to_string(&config).unwrap();
                std::fs::write(
                    &config,
                    raw.replacen(BEGIN, &format!("{BEGIN}\nUser modification"), 1),
                )
                .unwrap();
            } else {
                std::fs::write(&config, format!("{BEGIN}\nForeign owned block.\n{END}\n")).unwrap();
            }
            let before = snapshot(&root);
            let mut args = vec!["install", "--development-source", source.to_str().unwrap()];
            args.extend(options);
            let refused = run(&world, &args, 1);
            assert_eq!(get(&refused, "verified"), &Value::Bool(false));
            assert_eq!(snapshot(&root), before);
            if owned {
                let bytes = std::fs::read(&config).unwrap();
                let removed = run(&world, &["uninstall", "--omp-writers-quiesced"], 0);
                assert!(
                    get(&removed, "preserved")
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| text(row, "path") == config.to_str().unwrap())
                );
                assert_eq!(std::fs::read(&config).unwrap(), bytes);
            }
        }
    }
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_omp_historical_shared_activation_uninstall_preserves_unrelated_text
#[test]

fn historical_omp_activation_uninstall_preserves_surrounding_text() {
    for mode_name in ["copy", "link"] {
        let world = World::new().unwrap();
        let source = source(&world);
        let pack = source.join("skills/agent-memory");
        std::fs::copy(
            pack.join("templates/activation-markdown.example.txt"),
            pack.join("templates/activation-omp-markdown.example.txt"),
        )
        .unwrap();
        let root = world.root.join("historical omp");
        std::fs::create_dir(&root).unwrap();
        let config = root.join("AGENTS.md");
        std::fs::write(&config, b"Original user rules.\n").unwrap();
        install(
            &world,
            &source,
            &[
                "--presets",
                "omp",
                "--omp-agent-dir",
                root.to_str().unwrap(),
                "--omp-peers-upgraded",
                "--mode",
                mode_name,
            ],
        );
        let raw = format!(
            "{}Later user rules.\n",
            std::fs::read_to_string(&config).unwrap()
        );
        std::fs::write(&config, &raw).unwrap();
        let (prefix, rest) = raw.split_once(BEGIN).unwrap();
        let (_, suffix) = rest.split_once(END).unwrap();
        run(&world, &["uninstall", "--omp-writers-quiesced"], 0);
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            format!("{prefix}{suffix}")
        );
    }
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_omp_binding_installs_candidate_activation_in_copy_and_link_modes
#[test]

fn omp_candidate_activation_is_used_in_both_modes() {
    for mode_name in ["copy", "link"] {
        let world = World::new().unwrap();
        let source = source(&world);
        let pack = source.join("skills/agent-memory");
        let root = world.root.join("explicit omp root");
        std::fs::create_dir(&root).unwrap();
        let config = root.join("AGENTS.md");
        std::fs::write(&config, b"Unrelated omp instructions.\n").unwrap();
        let common =
            std::fs::read_to_string(pack.join("templates/activation-markdown.example.txt"))
                .unwrap();
        let candidate = format!("{BEGIN}\nCandidate-owned omp invocation fixture.\n{END}");
        std::fs::write(
            pack.join("templates/activation-omp-markdown.example.txt"),
            &candidate,
        )
        .unwrap();
        let codex = world.paths["codex"].join("AGENTS.md");
        std::fs::write(&codex, b"Unrelated codex instructions.\n").unwrap();
        let options = [
            "--presets",
            "codex,omp",
            "--omp-agent-dir",
            root.to_str().unwrap(),
            "--omp-peers-upgraded",
            "--mode",
            mode_name,
        ];
        let mut args = vec![
            "install",
            "--development-source",
            source.to_str().unwrap(),
            "--check",
        ];
        args.extend(options);
        let before = snapshot(&world.root);
        let preview = run(&world, &args, 0);
        assert_eq!(
            get(&preview, "installation_writes_performed"),
            &Value::Bool(false)
        );
        assert_eq!(snapshot(&world.root), before);
        let installed = install(&world, &source, &options);
        let receipt = get(&installed, "receipt");
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            format!("Unrelated omp instructions.\n{candidate}\n")
        );
        assert_eq!(
            std::fs::read_to_string(&codex).unwrap(),
            format!("Unrelated codex instructions.\n{common}\n")
        );
        for (path, block) in [(&config, &candidate), (&codex, &common)] {
            let target = get(receipt, "targets")
                .as_array()
                .unwrap()
                .iter()
                .find(|row| text(row, "path") == path.to_str().unwrap())
                .unwrap();
            assert_eq!(text(target, "block_hash"), sha(block.as_bytes()));
            assert_eq!(
                text(get(target, "identity"), "hash"),
                sha(&std::fs::read(path).unwrap())
            );
        }
        let skill = std::path::PathBuf::from(text(get(receipt, "omp_binding"), "skill_target"));
        assert_eq!(skill.is_symlink(), mode_name == "link");
        assert_eq!(
            std::fs::read_to_string(skill.join("templates/activation-omp-markdown.example.txt"))
                .unwrap(),
            candidate
        );
        assert_eq!(
            get(&install(&world, &source, &[]), "changed"),
            &Value::Bool(false)
        );
        let updated = format!(
            "Later unrelated prefix.\n{}Later suffix.\n",
            std::fs::read_to_string(&config).unwrap()
        );
        std::fs::write(&config, &updated).unwrap();
        install(&world, &source, &[]);
        let (prefix, rest) = updated.split_once(BEGIN).unwrap();
        let (_, suffix) = rest.split_once(END).unwrap();
        run(&world, &["uninstall", "--omp-writers-quiesced"], 0);
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            format!("{prefix}{suffix}")
        );
        assert_eq!(
            std::fs::read(codex).unwrap(),
            b"Unrelated codex instructions.\n"
        );
    }
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_omp_historical_owned_shared_block_updates_and_rolls_back_by_recorded_hash
#[test]

fn historical_omp_activation_updates_and_rolls_back_by_hash() {
    for mode_name in ["copy", "link"] {
        let world = World::new().unwrap();
        let source = source(&world);
        let pack = source.join("skills/agent-memory");
        let root = world.root.join("historical omp root");
        std::fs::create_dir(&root).unwrap();
        let config = root.join("AGENTS.md");
        std::fs::write(&config, b"Keep unrelated instructions.\n").unwrap();
        let template = pack.join("templates/activation-omp-markdown.example.txt");
        let current = std::fs::read_to_string(&template).unwrap();
        std::fs::copy(
            pack.join("templates/activation-markdown.example.txt"),
            &template,
        )
        .unwrap();
        install(
            &world,
            &source,
            &[
                "--presets",
                "omp",
                "--omp-agent-dir",
                root.to_str().unwrap(),
                "--omp-peers-upgraded",
                "--mode",
                mode_name,
            ],
        );
        let receipt_path = state(&world).join("receipt.json");
        let previous_receipt = std::fs::read(&receipt_path).unwrap();
        let previous_config = std::fs::read(&config).unwrap();
        std::fs::write(&template, &current).unwrap();
        let failed = fault(
            &world,
            &["install", "--development-source", source.to_str().unwrap()],
            "install-before-receipt",
            "error",
        );
        assert_eq!(failed.status, agent_memory::process::Status::Code(1));
        assert_eq!(std::fs::read(&receipt_path).unwrap(), previous_receipt);
        assert_eq!(std::fs::read(&config).unwrap(), previous_config);
        let updated = install(&world, &source, &[]);
        assert!(
            get(&updated, "restart_clients")
                .as_array()
                .unwrap()
                .contains(&Value::String("omp".into()))
        );
        let target = get(get(&updated, "receipt"), "targets")
            .as_array()
            .unwrap()
            .iter()
            .find(|row| text(row, "path") == config.to_str().unwrap())
            .unwrap();
        assert_eq!(text(target, "block_hash"), sha(current.as_bytes()));
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            format!("Keep unrelated instructions.\n{current}\n")
        );
        run(&world, &["uninstall", "--omp-writers-quiesced"], 0);
        assert_eq!(
            std::fs::read(config).unwrap(),
            b"Keep unrelated instructions.\n"
        );
    }
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
