mod support;
use agent_memory::codec::json::{self, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use support::World;
use support::distribution::Channel;
use support::lifecycle::*;
fn defaults(world: &World) -> Vec<PathBuf> {
    vec![
        world.paths["home"].join(".agents/skills"),
        world.paths["codex"].join("skills"),
        world.paths["claude"].join("skills"),
        world.paths["config"].join("opencode/skills"),
    ]
}
fn optional(path: &Path) -> Option<std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)>> {
    path.exists().then(|| snapshot(path))
}
fn layout(world: &World, receipt: &Value) {
    let code = PathBuf::from(text(get(receipt, "source"), "path"));
    let pack = PathBuf::from(text(get(receipt, "source"), "pack"));
    assert_eq!(code, world.paths["data"].join("agent-memory/runtime"));
    assert_eq!(pack, code.join("skills/agent-memory"));
    let state = state(world);
    assert!(!state.starts_with(&code));
    assert!(!code.starts_with(&state));
    assert_eq!(get(receipt, "data_roots"), &Value::Array(vec![]));
    assert_eq!(
        text(receipt, "runtime_state"),
        state.parent().unwrap().to_str().unwrap()
    );
    assert_eq!(
        text(receipt, "bin_dir"),
        world.paths["home"].join(".local/bin").to_str().unwrap()
    );
    for target in get(receipt, "targets").as_array().unwrap() {
        let path = PathBuf::from(text(target, "path"));
        assert!(!path.starts_with(&world.paths["repo"]));
        assert!(!path.starts_with(&world.paths["cache"]));
        match text(target, "kind") {
            "copy" | "link" => {
                if text(receipt, "mode") == "link" {
                    assert_eq!(std::fs::canonicalize(&path).unwrap(), pack);
                } else {
                    assert!(!path.is_symlink());
                }
                let original = agent_memory::release_distribution::portable_files(&pack).unwrap();
                let copied = agent_memory::release_distribution::portable_files(&path).unwrap();
                assert_eq!(copied, original);
            }
            "activation" => {
                let template = if get(receipt, "omp_binding")
                    .get("activation_target")
                    .and_then(Value::as_str)
                    == path.to_str()
                {
                    "activation-omp-markdown.example.txt"
                } else {
                    "activation-markdown.example.txt"
                };
                let block = std::fs::read(pack.join("templates").join(template)).unwrap();
                let hash: String = Sha256::digest(&block)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                assert_eq!(text(target, "block_hash"), hash);
                let raw = std::fs::read_to_string(&path).unwrap();
                let start = raw.find("<!-- agent-memory:activation:begin -->").unwrap();
                let end = raw.find("<!-- agent-memory:activation:end -->").unwrap()
                    + "<!-- agent-memory:activation:end -->".len();
                assert_eq!(raw.get(start..end).unwrap().as_bytes(), block);
            }
            "command-link" => assert_eq!(std::fs::read_link(path).unwrap(), code.join("bin/am")),
            _ => {}
        }
    }
    for backup in get(receipt, "backups").as_array().unwrap() {
        let path = Path::new(text(backup, "path"));
        assert!(!path.starts_with(&code));
        assert!(path.exists());
    }
}
fn native(world: &World, cli: &Path, args: &[&str]) -> Value {
    let output = world
        .run(
            std::iter::once(cli.to_str().unwrap())
                .chain(["--json"])
                .chain(args.iter().copied()),
        )
        .unwrap();
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}

// obligation: tests/contracts/test_installed_layout.py::InstalledLayoutContract.test_no_client_mode_still_exposes_installed_lifecycle_without_a_bank_or_identity
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn no_client_mode_exposes_lifecycle_without_bank_or_identity() {
    for mode in ["link", "copy"] {
        let world = World::new().unwrap();
        let channel = Channel::new(&world);
        channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
        let before = defaults(&world)
            .iter()
            .map(|path| optional(path))
            .collect::<Vec<_>>();
        let installed = channel.install(&world, "none", mode);
        let receipt = get(&installed, "receipt");
        layout(&world, receipt);
        assert_eq!(get(receipt, "enabled_adapters"), &Value::Array(vec![]));
        assert!(
            !get(receipt, "targets")
                .as_array()
                .unwrap()
                .iter()
                .any(|row| ["link", "copy", "activation"].contains(&text(row, "kind")))
        );
        let cli = command(receipt);
        assert_eq!(
            text(get(&native(&world, &cli, &["status"]), "bank"), "state"),
            "not_located"
        );
        assert_eq!(
            get(&native(&world, &cli, &["doctor"]), "ok"),
            &Value::Bool(true)
        );
        assert_eq!(
            get(&channel.run(&world, "update", &[], 0), "changed"),
            &Value::Bool(false)
        );
        assert_eq!(
            defaults(&world)
                .iter()
                .map(|path| optional(path))
                .collect::<Vec<_>>(),
            before
        );
        assert!(!world.paths["repo"].join("memory-bank").exists());
        assert!(!world.paths["config"].join("agent-memory").exists());
        assert_eq!(
            get(&channel.run(&world, "uninstall", &[], 0), "verified"),
            &Value::Bool(true)
        );
        assert!(state(&world).join("receipt.json").exists());
    }
}

// obligation: tests/contracts/test_installed_layout.py::InstalledLayoutContract.test_custom_only_discovery_does_not_create_default_client_links_or_guidance
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn custom_only_targets_never_create_default_client_links_or_guidance() {
    for mode in ["link", "copy"] {
        let world = World::new().unwrap();
        let channel = Channel::new(&world);
        channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
        let before = defaults(&world)
            .iter()
            .map(|path| optional(path))
            .collect::<Vec<_>>();
        let one = world.root.join("first custom skills");
        let two = world.root.join("second custom skills");
        let installed = channel.run(
            &world,
            "install",
            &[
                "none",
                mode,
                &format!("target={}", one.display()),
                &format!("target={}", two.display()),
            ],
            0,
        );
        let receipt = get(&installed, "receipt");
        layout(&world, receipt);
        let targets = get(receipt, "targets").as_array().unwrap();
        let actual: BTreeSet<_> = targets
            .iter()
            .filter(|row| ["link", "copy"].contains(&text(row, "kind")))
            .map(|row| text(row, "path").to_owned())
            .collect();
        assert_eq!(
            actual,
            [one.join("agent-memory"), two.join("agent-memory")]
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        );
        assert!(!targets.iter().any(|row| text(row, "kind") == "activation"));
        assert_eq!(
            defaults(&world)
                .iter()
                .map(|path| optional(path))
                .collect::<Vec<_>>(),
            before
        );
        let version = native(&world, &command(receipt), &["version"]);
        assert_eq!(
            get(get(&version, "installation"), "enabled_adapters"),
            get(receipt, "enabled_adapters")
        );
        assert_eq!(
            get(receipt, "enabled_adapters").as_array().unwrap().len(),
            2
        );
    }
}

// obligation: tests/contracts/test_installed_layout.py::InstalledLayoutContract.test_all_clients_share_one_artifact_and_installed_lifecycle_preserves_distinct_data
#[test]
#[ignore = "needs recovery_frontier registered"]
fn all_clients_share_one_artifact_and_lifecycle_preserves_distinct_data() {
    for mode in ["link", "copy"] {
        let world = World::new().unwrap();
        let channel = Channel::new(&world);
        channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
        let fixture = support::canaries::Canary::new(&world);
        let cache = world.paths["cache"].join("agent-memory/cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("fixture-only.json"),
            b"{\"fixture_only\":true}\n",
        )
        .unwrap();
        let indices = world.paths["cache"].join("agent-memory/indices");
        std::fs::create_dir_all(&indices).unwrap();
        std::fs::write(indices.join("references.json"), b"{\"derived\":true}\n").unwrap();
        let settings = [
            world.paths["codex"].join("AGENTS.md"),
            world.paths["claude"].join("CLAUDE.md"),
            world.paths["config"].join("opencode/AGENTS.md"),
        ];
        for path in &settings {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"Keep existing client instructions.\n").unwrap();
        }
        let custom = world.root.join("custom harness skills with spaces");
        let mut roots = defaults(&world);
        roots.push(custom.clone());
        let mut neighbors = Vec::new();
        for root in &roots {
            let neighbor = root.join("neighbor-skill");
            std::fs::create_dir_all(&neighbor).unwrap();
            std::fs::write(neighbor.join("SKILL.md"), b"# Unrelated skill\n").unwrap();
            neighbors.push(neighbor);
        }
        let mut protected = vec![
            fixture.bank.clone(),
            cache.clone(),
            indices.clone(),
            world.paths["config"].join("agent-memory"),
            fixture.repo.join(".git"),
            fixture.repo.join("app.rs"),
        ];
        protected.extend(neighbors);
        let before = protected
            .iter()
            .map(|path| (path.clone(), snapshot(path)))
            .collect::<Vec<_>>();
        let installed = channel.run(
            &world,
            "install",
            &["all", mode, &format!("target={}", custom.display())],
            0,
        );
        let receipt = get(&installed, "receipt");
        layout(&world, receipt);
        let targets = get(receipt, "targets").as_array().unwrap();
        let actual: BTreeSet<_> = targets
            .iter()
            .filter(|row| ["copy", "link"].contains(&text(row, "kind")))
            .map(|row| PathBuf::from(text(row, "path")))
            .collect();
        assert_eq!(
            actual,
            roots.iter().map(|path| path.join("agent-memory")).collect()
        );
        assert_eq!(
            get(receipt, "enabled_adapters").as_array().unwrap().len(),
            5
        );
        let cli = command(receipt);
        for name in [
            "install",
            "uninstall",
            "update",
            "version",
            "doctor",
            "status",
        ] {
            world
                .run([cli.to_str().unwrap(), "--json", name, "--help"])
                .unwrap();
        }
        for args in [
            vec!["install", "--check"],
            vec!["uninstall", "--check"],
            vec!["version"],
            vec!["status", "--repo", fixture.repo.to_str().unwrap()],
        ] {
            let before = snapshot(&world.root);
            native(&world, &cli, &args);
            assert_eq!(snapshot(&world.root), before);
        }
        channel.publish("0.0.16", &"b".repeat(40));
        let updated = channel.run(&world, "update", &[], 0);
        layout(&world, get(&updated, "receipt"));
        let backups = get(get(&updated, "receipt"), "backups")
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    PathBuf::from(text(row, "path")),
                    snapshot(Path::new(text(row, "path"))),
                )
            })
            .collect::<Vec<_>>();
        assert!(!backups.is_empty());
        channel.run(&world, "uninstall", &[], 0);
        assert!(!cli.exists());
        assert!(!Path::new(text(get(get(&updated, "receipt"), "source"), "path")).exists());
        assert_eq!(
            get(&read(&state(&world).join("receipt.json")), "uninstalled"),
            &Value::Bool(true)
        );
        for (path, expected) in &before {
            assert_eq!(&snapshot(path), expected);
        }
        for (path, expected) in backups {
            assert_eq!(snapshot(&path), expected);
        }
        for path in &settings {
            assert_eq!(
                std::fs::read(path).unwrap(),
                b"Keep existing client instructions.\n"
            );
        }
        std::fs::remove_dir_all(cache).unwrap();
        std::fs::remove_dir_all(indices).unwrap();
        assert_eq!(
            text(
                &run(
                    &world,
                    &["inspect", "--repo", fixture.repo.to_str().unwrap()],
                    0
                ),
                "project_id"
            ),
            fixture.project
        );
        let restored = channel.run(
            &world,
            "install",
            &["all", mode, &format!("target={}", custom.display())],
            0,
        );
        layout(&world, get(&restored, "receipt"));
        assert_eq!(snapshot(&fixture.bank), before[0].1);
    }
}
