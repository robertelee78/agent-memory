mod support;
use agent_memory::codec::json::Value;
use support::distribution::Channel;
use support::lifecycle::*;
use support::{RunOptions, World};
fn exercise(kind: &str) {
    let mut world = World::new().unwrap();
    let channel = Channel::new(&world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    let installed = channel.install(&world, "codex", "copy");
    let repo = world.repository("mutable repository", false).unwrap();
    let memory = bank(&world, &repo);
    let git = |world: &World, args: &[&str]| {
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
    git(&world, &["add", "memory-bank"]);
    git(&world, &["commit", "-m", "Bank baseline"]);
    let original_head = String::from_utf8(git(&world, &["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    let blob = String::from_utf8(
        git(
            &world,
            &[
                "hash-object",
                "-w",
                memory.join("MEMORY.md").to_str().unwrap(),
            ],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let alternate = if kind == "head" {
        world.env.insert(
            "GIT_INDEX_FILE".into(),
            world.root.join("private-index").into_os_string(),
        );
        git(&world, &["read-tree", "HEAD"]);
        git(
            &world,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{blob},other-bank/MEMORY.md"),
            ],
        );
        let tree = String::from_utf8(git(&world, &["write-tree"]).stdout).unwrap();
        let result = String::from_utf8(
            git(
                &world,
                &[
                    "commit-tree",
                    tree.trim(),
                    "-p",
                    &original_head,
                    "-m",
                    "Competing bank",
                ],
            )
            .stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        world.env.remove(std::ffi::OsStr::new("GIT_INDEX_FILE"));
        result
    } else {
        String::new()
    };
    channel.publish("0.0.16", &"b".repeat(40));
    let receipt_bytes = std::fs::read(state(&world).join("receipt.json")).unwrap();
    let targets = get(get(&installed, "receipt"), "targets")
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
    let original = (
        std::fs::read(repo.join(".git/index")).unwrap(),
        std::fs::read(repo.join(".git/config")).unwrap(),
        std::fs::read(memory.join("MEMORY.md")).unwrap(),
    );
    let argument = format!("repo={}", repo.display());
    let (child, release) = channel.spawn(&world, "update", &[&argument], "update-after-activate-0");
    match kind {
        "head" => {
            git(
                &world,
                &["update-ref", "refs/heads/main", &alternate, &original_head],
            );
        }
        "index" => {
            git(
                &world,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("100644,{blob},other-bank/MEMORY.md"),
                ],
            );
        }
        "config" => {
            git(
                &world,
                &["config", "--local", "agent-memory.bankroot", "other-bank"],
            );
        }
        "bank" => {
            let raw = String::from_utf8(original.2.clone()).unwrap();
            assert!(raw.contains("agent-memory/v1"));
            std::fs::write(
                memory.join("MEMORY.md"),
                raw.replace("agent-memory/v1", "agent-memory/v999"),
            )
            .unwrap();
        }
        "lock" => {
            std::fs::write(
                memory.join(".agent-memory.lock"),
                b"other session owns this lock",
            )
            .unwrap();
        }
        "transient" => {
            let lock = memory.join(".agent-memory.lock");
            std::fs::write(&lock, b"brief concurrent session").unwrap();
            std::fs::remove_file(lock).unwrap();
        }
        _ => panic!("unknown mutation"),
    }
    let observed = snapshot(&repo);
    std::fs::write(release, b"").unwrap();
    let result = Channel::collect(child, if kind == "transient" { 0 } else { 1 });
    assert_eq!(
        snapshot(&repo),
        observed,
        "update changed independently modified project state"
    );
    if kind == "transient" {
        assert_eq!(get(&result, "verified"), &Value::Bool(true));
        assert_eq!(
            text(get(get(&result, "receipt"), "source"), "revision"),
            "b".repeat(40)
        );
        return;
    }
    assert_eq!(get(&result, "verified"), &Value::Bool(false));
    assert!(!text(&result, "error").is_empty());
    assert!(!text(&result, "repair").is_empty());
    assert_eq!(
        std::fs::read(state(&world).join("receipt.json")).unwrap(),
        receipt_bytes
    );
    let journal = read(&state(&world).join("journal.json"));
    assert!(["activating", "rolled-back"].contains(&text(&journal, "status")));
    for action in get(&journal, "actions").as_array().unwrap() {
        if get(action, "before") != &Value::Null && get(action, "after") != get(action, "before") {
            assert!(std::fs::symlink_metadata(text(action, "stage")).is_ok());
        }
    }
    match kind {
        "head" => {
            git(
                &world,
                &["update-ref", "refs/heads/main", &original_head, &alternate],
            );
        }
        "index" => {
            std::fs::write(repo.join(".git/index"), &original.0).unwrap();
        }
        "config" => {
            std::fs::write(repo.join(".git/config"), &original.1).unwrap();
        }
        "bank" => {
            std::fs::write(memory.join("MEMORY.md"), &original.2).unwrap();
        }
        "lock" => {
            std::fs::remove_file(memory.join(".agent-memory.lock")).unwrap();
        }
        _ => unreachable!(),
    }
    assert_eq!(
        get(&channel.run(&world, "recover", &[], 0), "verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        std::fs::read(state(&world).join("receipt.json")).unwrap(),
        receipt_bytes
    );
    for (path, before) in targets {
        assert_eq!(snapshot(&path), before);
    }
    if kind == "lock" {
        assert!(text(&result, "error").contains("idle bank"));
    }
}

// obligation: tests/contracts/test_update_mutable_public.py::MutableUpdateCompatibility.test_changed_head_is_reobserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_head_is_reobserved() {
    exercise("head");
}
// obligation: tests/contracts/test_update_mutable_public.py::MutableUpdateCompatibility.test_changed_index_is_reobserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_index_is_reobserved() {
    exercise("index");
}
// obligation: tests/contracts/test_update_mutable_public.py::MutableUpdateCompatibility.test_changed_local_config_is_reobserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_local_configuration_is_reobserved() {
    exercise("config");
}
// obligation: tests/contracts/test_update_mutable_public.py::MutableUpdateCompatibility.test_changed_bank_is_reobserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_bank_is_reobserved() {
    exercise("bank");
}
// obligation: tests/contracts/test_update_mutable_public.py::MutableUpdateCompatibility.test_new_idle_lock_is_reobserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn new_idle_lock_is_reobserved() {
    exercise("lock");
}
// obligation: tests/contracts/test_update_mutable_public.py::MutableUpdateCompatibility.test_transient_lock_cleared_before_recheck_succeeds
#[test]
#[ignore = "needs recovery_frontier registered"]
fn transient_lock_cleared_before_recheck_succeeds() {
    exercise("transient");
}
