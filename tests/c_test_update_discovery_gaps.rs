mod support;
use agent_memory::codec::json::Value;
use support::distribution::Channel;
use support::lifecycle::*;
use support::{RunOptions, World};
fn setup(world: &World, mode: &str) -> (Channel, Value) {
    let channel = Channel::new(world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    let receipt = channel.install(world, "codex", mode);
    (channel, receipt)
}

// obligation: tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_deleted_registered_worktree_does_not_block_update_repeat_or_recovery
#[test]
#[ignore = "needs recovery_frontier registered"]
fn deleted_registered_worktree_is_a_gap_not_a_fabricated_explicit_root() {
    for mode in ["link", "copy"] {
        let world = World::new().unwrap();
        let (channel, _) = setup(&world, mode);
        let repo = world.repository("registered project", false).unwrap();
        let memory = bank(&world, &repo);
        support::identity_fixtures::bind(&world, &repo).unwrap();
        let git = |args: &[&str]| {
            world
                .git_with(
                    args,
                    RunOptions {
                        cwd: Some(&repo),
                        ..Default::default()
                    },
                )
                .unwrap()
        };
        git(&["add", "."]);
        git(&["commit", "-m", "Bound project"]);
        let missing = world.root.join("deleted session");
        git(&[
            "worktree",
            "add",
            "-b",
            "deleted-session",
            missing.to_str().unwrap(),
        ]);
        std::fs::remove_dir_all(&missing).unwrap();
        let project = snapshot(&repo);
        channel.publish("0.0.16", &"b".repeat(40));
        let before = snapshot(&world.root);
        let preview = channel.run(&world, "update", &["--check"], 0);
        assert_eq!(snapshot(&world.root), before);
        let compatibility = get(&preview, "compatibility");
        assert!(
            get(compatibility, "discovery_gaps")
                .as_array()
                .unwrap()
                .iter()
                .any(|row| text(row, "location") == missing.to_str().unwrap())
        );
        let observed = get(compatibility, "observed_repositories")
            .as_array()
            .unwrap();
        assert!(!observed.contains(&Value::String(missing.to_string_lossy().into_owned())));
        assert!(observed.contains(&Value::String(repo.to_string_lossy().into_owned())));
        let updated = channel.run(&world, "update", &[], 0);
        assert_eq!(get(&updated, "verified"), &Value::Bool(true));
        assert_eq!(
            text(get(get(&updated, "receipt"), "source"), "revision"),
            "b".repeat(40)
        );
        assert_eq!(snapshot(&repo), project);
        assert!(memory.is_dir());
        assert_eq!(
            get(&channel.run(&world, "update", &[], 0), "changed"),
            &Value::Bool(false)
        );
        assert_eq!(
            get(&channel.run(&world, "recover", &[], 0), "verified"),
            &Value::Bool(true)
        );
        assert_eq!(snapshot(&repo), project);
    }
}

// obligation: tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_missing_explicit_repository_remains_an_error_and_preserves_targets
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn missing_explicit_repository_is_an_error_and_preserves_targets() {
    let world = World::new().unwrap();
    let (channel, installed) = setup(&world, "copy");
    let receipt = get(&installed, "receipt");
    let targets = get(receipt, "targets")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                std::path::PathBuf::from(text(row, "path")),
                snapshot(std::path::Path::new(text(row, "path"))),
            )
        })
        .collect::<Vec<_>>();
    let raw = std::fs::read(state(&world).join("receipt.json")).unwrap();
    let missing = format!(
        "repo={}",
        world.root.join("explicit missing repo").display()
    );
    for check in [false, true] {
        let options = if check {
            vec!["--check", missing.as_str()]
        } else {
            vec![missing.as_str()]
        };
        let result = channel.run(&world, "update", &options, 1);
        assert!(text(&result, "error").contains("Explicit repository is unavailable"));
        for (path, before) in &targets {
            assert_eq!(&snapshot(path), before);
        }
        assert_eq!(
            std::fs::read(state(&world).join("receipt.json")).unwrap(),
            raw
        );
    }
}

// obligation: tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_available_repository_without_a_bank_stays_in_compatibility_rechecks
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn available_bankless_repository_remains_in_compatibility_rechecks() {
    let world = World::new().unwrap();
    let (channel, _) = setup(&world, "copy");
    let repo = world.repository("existing empty project", false).unwrap();
    let argument = format!("repo={}", repo.display());
    let before = snapshot(&world.root);
    let result = channel.run(&world, "update", &["--check", &argument], 0);
    let compatibility = get(&result, "compatibility");
    assert!(
        get(compatibility, "observed_repositories")
            .as_array()
            .unwrap()
            .contains(&Value::String(repo.to_string_lossy().into_owned()))
    );
    assert!(
        !get(compatibility, "banks")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| text(row, "repository") == repo.to_str().unwrap())
    );
    assert_eq!(snapshot(&world.root), before);
}
