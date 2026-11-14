mod support;
use agent_memory::codec::json::{self, Value};
use support::lifecycle::*;
use support::{RunOptions, World};
fn initialize(world: &World) -> (std::path::PathBuf, std::path::PathBuf, String) {
    let repo = world.repository("offline project", false).unwrap();
    std::fs::write(
        repo.join("README.md"),
        b"# Retry service\n\nProject scope: retry only.\n",
    )
    .unwrap();
    let project = agent_memory::host::uuid4().unwrap();
    let mut proposal = json::decode(r##"{"router-updated":"2026-10-22T20:00:00Z","contexts":{"activeContext.md":{"body":"# Active context\n\nFocus: retry only; [scope](../README.md).\n\nOpen: other components unverified.\n\nNext: inspect the first fixture.\n","purpose":"Resume verified retry work","scope":"component:retry","lifecycle":"active","read-when":"When resuming retry work"}},"omitted":{"projectbrief.md":"No additional verified evidence supplied","productContext.md":"No additional verified evidence supplied","systemPatterns.md":"No additional verified evidence supplied","techContext.md":"No additional verified evidence supplied","progress.md":"No additional verified evidence supplied"}}"##, Default::default()).unwrap();
    if let Value::Object(fields) = &mut proposal {
        fields.push(("project-id".into(), Value::String(project.clone())));
    }
    let input = world.root.join("reviewed-router.json");
    save(&input, &proposal);
    let rendered = run(
        world,
        &["bank", "render", "--input", input.to_str().unwrap()],
        0,
    );
    let memory = repo.join("memory-bank");
    std::fs::create_dir(&memory).unwrap();
    for (name, body) in get(&rendered, "files").as_object().unwrap() {
        std::fs::write(memory.join(name), body.as_str().unwrap()).unwrap();
    }
    assert_eq!(std::fs::read_dir(&memory).unwrap().count(), 2);
    assert_eq!(
        text(
            &run(
                world,
                &["bank", "validate", "--repo", repo.to_str().unwrap()],
                0
            ),
            "project_id"
        ),
        project
    );
    world
        .git_with(
            ["add", "README.md", "memory-bank"],
            RunOptions {
                cwd: Some(&repo),
                ..Default::default()
            },
        )
        .unwrap();
    world
        .git_with(
            ["commit", "-m", "Useful manual bank"],
            RunOptions {
                cwd: Some(&repo),
                ..Default::default()
            },
        )
        .unwrap();
    (repo, memory, project)
}
fn absent(world: &World) {
    for name in [
        "config",
        "state",
        "cache",
        "codex",
        "claude",
        "opencode",
        "credentials",
    ] {
        std::fs::remove_dir_all(&world.paths[name]).unwrap();
    }
}

// obligation: tests/contracts/test_offline_skill.py::OfflineSkillContract.test_offline_manual_checkpoint_and_fresh_file_resume_preserve_project_and_unrelated_work
#[test]
#[ignore = "needs recovery_frontier registered"]
fn manual_checkpoint_and_fresh_clone_resume_need_no_generated_state() {
    let world = World::new().unwrap();
    let (repo, memory, project) = initialize(&world);
    absent(&world);
    run(
        &world,
        &["writes", "inspect", "--repo", repo.to_str().unwrap()],
        0,
    );
    let router = std::fs::read(memory.join("MEMORY.md")).unwrap();
    std::fs::write(repo.join("feature.rs"), b"const UNFINISHED: bool = true;\n").unwrap();
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
    git(&["add", "feature.rs"]);
    std::fs::write(repo.join("scratch.txt"), b"Preserve unrelated work\n").unwrap();
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    let head = git(&["rev-parse", "HEAD"]).stdout;
    assert!(!memory.join(".agent-memory.lock").exists());
    assert!(!memory.join(".agent-memory-transaction.md").exists());
    let target = memory.join("activeContext.md");
    let raw = std::fs::read_to_string(&target).unwrap();
    std::fs::write(
        &target,
        raw.replace("inspect the first fixture", "inspect the second fixture"),
    )
    .unwrap();
    assert_eq!(std::fs::read(memory.join("MEMORY.md")).unwrap(), router);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(git(&["rev-parse", "HEAD"]).stdout, head);
    assert_eq!(
        git(&["diff", "--name-only"]).stdout,
        b"memory-bank/activeContext.md\n"
    );
    assert_eq!(
        std::fs::read(repo.join("scratch.txt")).unwrap(),
        b"Preserve unrelated work\n"
    );
    assert!(
        !String::from_utf8_lossy(&git(&["show", "HEAD:memory-bank/activeContext.md"]).stdout)
            .contains("second fixture")
    );
    git(&["add", "memory-bank/activeContext.md"]);
    git(&[
        "commit",
        "--only",
        "memory-bank/activeContext.md",
        "-m",
        "Manual checkpoint",
    ]);
    let clone = world.clone(&repo, "fresh session").unwrap();
    let resumed = world
        .run([
            "/bin/cat",
            clone.join("memory-bank/activeContext.md").to_str().unwrap(),
        ])
        .unwrap();
    let resumed = String::from_utf8(resumed.stdout).unwrap();
    for needle in [
        "second fixture",
        "other components unverified",
        "[scope](../README.md)",
    ] {
        assert!(resumed.contains(needle));
    }
    assert_eq!(
        text(
            &run(
                &world,
                &["bank", "validate", "--repo", clone.to_str().unwrap()],
                0
            ),
            "project_id"
        ),
        project
    );
    assert_eq!(
        std::fs::read(clone.join("memory-bank/MEMORY.md")).unwrap(),
        router
    );
    for name in [
        "config",
        "state",
        "cache",
        "codex",
        "claude",
        "opencode",
        "credentials",
    ] {
        assert!(!world.paths[name].exists(), "{name}");
    }
}

// obligation: tests/contracts/test_offline_skill.py::OfflineSkillContract.test_generated_state_absence_never_bypasses_busy_or_interrupted_bank
#[test]
#[ignore = "needs recovery_frontier registered"]
fn absent_generated_state_never_bypasses_busy_or_interrupted_bank() {
    let world = World::new().unwrap();
    let (repo, memory, _) = initialize(&world);
    absent(&world);
    let before = snapshot(&memory);
    for name in [".agent-memory.lock", ".agent-memory-transaction.md"] {
        let marker = memory.join(name);
        std::fs::write(&marker, b"Preserve marker; explicit inspection required\n").unwrap();
        for (group, operation) in [("bank", "validate"), ("writes", "inspect")] {
            run(
                &world,
                &[group, operation, "--repo", repo.to_str().unwrap()],
                1,
            );
            assert!(marker.exists());
        }
        let mut current = snapshot(&memory);
        current.remove(std::path::Path::new(name));
        assert_eq!(current, before);
        std::fs::remove_file(marker).unwrap();
    }
}
