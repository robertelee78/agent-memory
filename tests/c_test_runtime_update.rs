mod support;
use agent_memory::codec::json::Value;
use std::path::Path;
use support::World;
use support::distribution::Channel;
use support::lifecycle::*;

const BASE: &str = env!("CARGO_PKG_VERSION");
const NEXT: &str = "0.0.16";
fn channel(world: &World) -> Channel {
    let channel = Channel::new(world);
    channel.publish(BASE, &"a".repeat(40));
    channel
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_changed_owned_copy_update_retains_prior_payload_and_receipt_backups
#[test]
fn changed_owned_copy_retains_payload_and_receipt_backups() {
    let world = World::new().unwrap();
    let channel = channel(&world);
    let installed = channel.install(&world, "codex", "copy");
    let receipt = get(&installed, "receipt");
    let previous = get(receipt, "targets")
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| ["copy", "source"].contains(&text(target, "kind")))
        .map(|target| {
            (
                text(target, "path").to_owned(),
                snapshot(Path::new(text(target, "path"))),
            )
        })
        .collect::<Vec<_>>();
    channel.publish(NEXT, &"b".repeat(40));
    let updated = channel.run(&world, "update", &[], 0);
    let backups = get(get(&updated, "receipt"), "backups").as_array().unwrap();
    for (path, before) in previous {
        assert!(
            backups
                .iter()
                .any(|backup| text(backup, "original_path") == path
                    && snapshot(Path::new(text(backup, "path"))) == before)
        );
    }
    assert!(backups.iter().any(|backup| text(backup, "original_path")
        == state(&world).join("receipt.json").to_str().unwrap()));
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_discovered_and_explicit_banks_remain_unchanged_and_unsupported_schema_refuses
#[test]
#[ignore = "needs recovery_frontier registered"]
fn discovered_and_explicit_banks_are_preserved_and_unknown_schema_refuses() {
    for registered in [false, true] {
        let world = World::new().unwrap();
        let channel = channel(&world);
        let installed = channel.install(&world, "codex", "copy");
        let repo = world.repository("another project", false).unwrap();
        let memory = bank(&world, &repo);
        if registered {
            support::identity_fixtures::bind(&world, &repo).unwrap();
        }
        channel.publish(NEXT, &"b".repeat(40));
        let argument = format!("repo={}", repo.display());
        let options = if registered {
            vec!["--check"]
        } else {
            vec!["--check", argument.as_str()]
        };
        let before = snapshot(&memory);
        let planned = channel.run(&world, "update", &options, 0);
        assert!(
            get(get(&planned, "compatibility"), "banks")
                .as_array()
                .unwrap()
                .iter()
                .any(|item| text(item, "bank") == memory.to_str().unwrap())
        );
        assert_eq!(snapshot(&memory), before);
        let router = memory.join("MEMORY.md");
        let raw = std::fs::read_to_string(&router)
            .unwrap()
            .replace("agent-memory/v1", "agent-memory/v999");
        std::fs::write(router, raw).unwrap();
        let before = snapshot(&world.root);
        let options = if registered {
            vec![]
        } else {
            vec![argument.as_str()]
        };
        channel.run(&world, "update", &options, 1);
        let mut after = snapshot(&world.root);
        let mut before = before;
        for map in [&mut after, &mut before] {
            map.retain(|path, _| !path.ends_with("owner.json") && !path.ends_with("lock"));
        }
        assert_eq!(after, before);
        assert_eq!(
            text(get(get(&installed, "receipt"), "source"), "version"),
            BASE
        );
    }
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_competing_update_observes_the_live_lifecycle_lock
#[test]
fn competing_update_observes_live_lifecycle_lock() {
    let world = World::new().unwrap();
    let channel = channel(&world);
    channel.install(&world, "codex", "copy");
    channel.publish(NEXT, &"b".repeat(40));
    let path = state(&world).join("lock");
    let lock = std::fs::File::options()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    lock.lock().unwrap();
    let before = snapshot(&world.root);
    let denied = channel.run(&world, "update", &[], 1);
    assert!(
        text(&denied, "error").contains("holds the lock"),
        "{denied:?}"
    );
    assert_eq!(snapshot(&world.root), before);
    drop(lock);
    assert_eq!(
        get(&channel.run(&world, "update", &[], 0), "verified"),
        &Value::Bool(true)
    );
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_unchanged_revision_refreshes_allowed_outside_text_without_a_code_switch
#[test]
fn unchanged_revision_preserves_outside_text_without_switching_code() {
    let world = World::new().unwrap();
    let channel = channel(&world);
    let installed = channel.install(&world, "codex", "copy");
    let runtime = std::path::PathBuf::from(text(get(get(&installed, "receipt"), "source"), "path"));
    let before_code = snapshot(&runtime);
    let target = world.paths["codex"].join("AGENTS.md");
    let raw = format!(
        "New outside user text.\n{}",
        std::fs::read_to_string(&target).unwrap()
    );
    std::fs::write(&target, &raw).unwrap();
    let updated = channel.run(&world, "update", &[], 0);
    assert_eq!(get(&updated, "verified"), &Value::Bool(true));
    assert_eq!(get(&updated, "update_available"), &Value::Bool(false));
    assert_eq!(get(&updated, "restart_clients"), &Value::Array(vec![]));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), raw);
    assert_eq!(snapshot(&runtime), before_code);
    assert_eq!(
        get(&channel.run(&world, "recover", &[], 0), "verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(&channel.run(&world, "update", &[], 0), "changed"),
        &Value::Bool(false)
    );
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_unchanged_channel_does_not_derive_a_new_artifact_from_a_different_umask
#[test]
fn unchanged_signed_channel_does_not_rederive_artifact_under_private_umask() {
    let world = World::new().unwrap();
    let channel = channel(&world);
    let installed = channel.install(&world, "codex", "copy");
    let receipt = get(&installed, "receipt");
    let targets = get(receipt, "targets")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                text(row, "path").to_owned(),
                snapshot(Path::new(text(row, "path"))),
            )
        })
        .collect::<Vec<_>>();
    let receipt_bytes = std::fs::read(state(&world).join("receipt.json")).unwrap();
    let updated = channel.run(&world, "update", &[], 0);
    assert_eq!(get(&updated, "verified"), &Value::Bool(true));
    assert_eq!(get(&updated, "changed"), &Value::Bool(false));
    assert_eq!(get(&updated, "loaded_content_changes"), &Value::Bool(false));
    for (path, before) in targets {
        assert_eq!(snapshot(Path::new(&path)), before);
    }
    assert_eq!(
        std::fs::read(state(&world).join("receipt.json")).unwrap(),
        receipt_bytes
    );
}
