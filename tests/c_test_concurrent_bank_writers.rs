mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::{Output, Status};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::cutover::*;
use support::{RunOptions, World};

fn fixture() -> (World, PathBuf, PathBuf) {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    write_files(
        &bank,
        &render(
            &[
                (
                    "projectbrief.md",
                    "# projectbrief.md\n\nUseful fixture context.",
                ),
                (
                    "activeContext.md",
                    "# activeContext.md\n\nUseful fixture context.",
                ),
            ],
            "2026-10-22T18:00:00Z",
        ),
    );
    support::identity_fixtures::bind(&w, &repo).unwrap();
    fs::write(repo.join("code.rs"), "const VALUE: u32 = 1;\n").unwrap();
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Canonical defensive race fixture"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    fs::write(
        repo.join("code.rs"),
        "const VALUE: u32 = 2; // staged unfinished feature\n",
    )
    .unwrap();
    w.git_with(
        ["add", "code.rs"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    (w, repo, bank)
}
fn git_state(w: &World, repo: &Path) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let index = w
        .git_with(
            ["rev-parse", "--path-format=absolute", "--git-path", "index"],
            RunOptions {
                cwd: Some(repo),
                ..RunOptions::default()
            },
        )
        .unwrap()
        .stdout;
    (
        fs::read(repo.join("code.rs")).unwrap(),
        fs::read(std::str::from_utf8(&index).unwrap().trim_end_matches('\n')).unwrap(),
        w.git_with(
            ["rev-parse", "HEAD"],
            RunOptions {
                cwd: Some(repo),
                ..RunOptions::default()
            },
        )
        .unwrap()
        .stdout,
        w.git_with(
            ["symbolic-ref", "HEAD"],
            RunOptions {
                cwd: Some(repo),
                ..RunOptions::default()
            },
        )
        .unwrap()
        .stdout,
    )
}
fn preview(w: &World, repo: &Path, body: &str) -> Value {
    let content = w.inside("race-content.md").unwrap();
    fs::write(&content, body).unwrap();
    good(
        w,
        "am writes",
        &[
            "preview".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--path".into(),
            "activeContext.md".into(),
            "--content".into(),
            content.into_os_string(),
        ],
    )
}
fn apply(w: &World, p: &Value) -> (Output, Value) {
    command(
        w,
        "am writes",
        &[
            "apply".into(),
            "--proposal".into(),
            save(w, p, "race-apply.json").into_os_string(),
        ],
    )
}
fn pause(w: &World, p: &Value, stage: &str, name: &str) -> (support::Child, PathBuf) {
    let directory = w.inside(name).unwrap();
    let env = [
        ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
        ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
        (
            "AGENT_MEMORY_FAULT_DIR".into(),
            directory.as_os_str().to_owned(),
        ),
    ];
    let child = w
        .spawn_with(
            [
                OsString::from(env!("CARGO_BIN_EXE_am")),
                "--json".into(),
                "writes".into(),
                "apply".into(),
                "--proposal".into(),
                save(w, p, &format!("{name}.json")).into_os_string(),
            ],
            RunOptions {
                check: false,
                env: &env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    wait(&directory.join(format!("{stage}.ready")));
    (child, directory.join(format!("{stage}.release")))
}
fn wait(path: &Path) {
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(8)).unwrap();
    while !path.exists() {
        assert!(
            !deadline.expired(),
            "rendezvous did not arrive: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn collect(child: &support::Child, code: i32) -> Value {
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(code));
    assert!(output.stderr.is_empty());
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_four_contenders_refuse_live_owner_and_stale_retries_then_preserve_every_rebased_fact
fn four_real_contenders_refuse_owner_and_stale_retries_preserving_all_rebased_facts() {
    let (w, repo, bank) = fixture();
    let before = git_state(&w, &repo);
    let target = bank.join("activeContext.md");
    let original = fs::read_to_string(&target).unwrap();
    let facts: Vec<_> = (0..5).map(|n| format!("Race fact {n}")).collect();
    let proposals: Vec<_> = facts
        .iter()
        .map(|fact| preview(&w, &repo, &format!("{original}\n{fact}\n")))
        .collect();
    let (winner, release) = pause(
        &w,
        &proposals[0],
        "locked-before-inventory",
        "winner-rendezvous",
    );
    let lock = bank.join(".agent-memory.lock");
    let owner_bytes = fs::read(&lock).unwrap();
    let owner = json::decode_bytes(&owner_bytes, Default::default()).unwrap();
    assert_eq!(
        string(&owner, "host"),
        agent_memory::host::hostname().unwrap()
    );
    let mut contenders = Vec::new();
    for (n, p) in proposals.iter().enumerate().skip(1) {
        let cmd = w
            .command(
                [
                    OsString::from(env!("CARGO_BIN_EXE_am")),
                    "--json".into(),
                    "writes".into(),
                    "apply".into(),
                    "--proposal".into(),
                    save(&w, p, &format!("contender-{n}.json")).into_os_string(),
                ],
                RunOptions {
                    check: false,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        // Each invoking owner supervises its own real am child. No global
        // three-child Group or World slot limit is widened for this fixture.
        contenders.push(std::thread::spawn(move || cmd.run().unwrap()));
    }
    for contender in contenders {
        let output = contender.join().unwrap();
        assert_eq!(output.status, Status::Code(1));
        let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
        assert!(!yes(&value, "verified"));
        assert_eq!(fs::read(&lock).unwrap(), owner_bytes);
        assert_eq!(fs::read_to_string(&target).unwrap(), original);
    }
    fs::write(release, b"").unwrap();
    assert!(yes(&collect(&winner, 0), "verified"));
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        string(get(&proposals[0], "proposal"), "content")
    );
    assert!(!lock.exists());
    for n in 1..5 {
        let before = fs::read(&target).unwrap();
        assert_eq!(apply(&w, &proposals[n]).0.status, Status::Code(1));
        assert_eq!(fs::read(&target).unwrap(), before);
        let fresh = preview(
            &w,
            &repo,
            &format!("{}\n{}\n", fs::read_to_string(&target).unwrap(), facts[n]),
        );
        let (output, written) = apply(&w, &fresh);
        assert_eq!(output.status, Status::Code(0));
        assert!(yes(&written, "verified"));
    }
    let text = fs::read_to_string(target).unwrap();
    assert!(text.starts_with(&original));
    for fact in facts {
        assert_eq!(text.matches(&fact).count(), 1);
    }
    assert_eq!(
        physical(&w.paths["state"])
            .keys()
            .filter(|p| p.contains("/operations/"))
            .count(),
        5
    );
    assert_eq!(git_state(&w, &repo), before);
}

#[test]
fn atomic_observer_worker() {
    let Some(root) = std::env::var_os("AGENT_MEMORY_FAULT_ATOMIC_OBSERVER") else {
        return;
    };
    let root = PathBuf::from(root);
    let target = root.join("repo/memory-bank/activeContext.md");
    let old = fs::read(root.join("observer-old")).unwrap();
    let new = fs::read(root.join("observer-new")).unwrap();
    let mut reads = 0u64;
    let mut seen_old = false;
    let mut seen_new = false;
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(20)).unwrap();
    loop {
        let raw = fs::read(&target).unwrap();
        assert!(raw.len() <= 65536);
        if raw == old {
            seen_old = true;
        } else if raw == new {
            seen_new = true;
        } else {
            panic!("observer saw partial replacement");
        }
        reads += 1;
        if reads == 1 {
            fs::write(root.join("observer-ready"), b"").unwrap();
        }
        if root.join("observer-stop").exists() {
            break;
        }
        assert!(!deadline.expired());
        std::thread::sleep(Duration::from_micros(500));
    }
    assert!(reads > 2);
    assert!(seen_old && seen_new);
    fs::write(root.join("observer-verified"), b"complete-old-and-new").unwrap();
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_concurrent_observer_sees_complete_old_or_new_bytes_through_atomic_replacement
fn real_concurrent_observer_sees_only_complete_old_or_new_bytes() {
    let (w, repo, bank) = fixture();
    let before = git_state(&w, &repo);
    let target = bank.join("activeContext.md");
    let old = fs::read(&target).unwrap();
    let sentence = "Retain a complete observed fact.\n";
    let remaining = 16384 - old.len() - 1;
    let new = format!(
        "{}\n{}{}",
        std::str::from_utf8(&old).unwrap(),
        sentence.repeat(remaining / sentence.len()),
        " ".repeat(remaining % sentence.len())
    );
    assert_eq!(new.len(), 16384);
    let p = preview(&w, &repo, &new);
    let (writer, release) = pause(&w, &p, "before-recheck", "atomic-writer");
    fs::write(w.root.join("observer-old"), &old).unwrap();
    fs::write(w.root.join("observer-new"), &new).unwrap();
    let env = [(
        "AGENT_MEMORY_FAULT_ATOMIC_OBSERVER".into(),
        w.root.as_os_str().to_owned(),
    )];
    let reader = w
        .spawn_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "atomic_observer_worker".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                check: false,
                env: &env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    wait(&w.root.join("observer-ready"));
    fs::write(release, b"").unwrap();
    assert!(yes(&collect(&writer, 0), "verified"));
    std::thread::sleep(Duration::from_millis(5));
    fs::write(w.root.join("observer-stop"), b"").unwrap();
    assert_eq!(reader.wait().unwrap().status, Status::Code(0));
    assert_eq!(
        fs::read(w.root.join("observer-verified")).unwrap(),
        b"complete-old-and-new"
    );
    assert_eq!(fs::read_to_string(target).unwrap(), new);
    assert_eq!(git_state(&w, &repo), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_actual_external_editor_invalidates_the_observed_target_without_losing_its_edit
fn real_external_editor_invalidates_target_without_losing_its_edit() {
    let (w, repo, bank) = fixture();
    let before = git_state(&w, &repo);
    let p = preview(&w, &repo, "# Focus\n\nNext: verify the new lesson.\n");
    let (writer, release) = pause(&w, &p, "before-recheck", "external-editor");
    let external =
        "# Focus\n\nActual external editor changed the target while its writer was paused.\n";
    let editor = w
        .spawn([
            OsString::from("/bin/sh"),
            "-c".into(),
            "printf '%s' \"$1\" > \"$2\"".into(),
            "editor".into(),
            external.into(),
            bank.join("activeContext.md").into_os_string(),
        ])
        .unwrap();
    assert_eq!(editor.wait().unwrap().status, Status::Code(0));
    fs::write(release, b"").unwrap();
    assert!(!yes(&collect(&writer, 1), "verified"));
    assert_eq!(
        fs::read_to_string(bank.join("activeContext.md")).unwrap(),
        external
    );
    assert!(!bank.join(".agent-memory.lock").exists());
    assert!(
        !physical(&w.paths["state"])
            .keys()
            .any(|p| p.contains("/operations/"))
    );
    assert_eq!(git_state(&w, &repo), before);
}

#[test]
#[ignore = "needs recovery_frontier and am transaction route registered"]
// obligation: tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_killed_transaction_owner_cannot_be_stolen_and_public_recovery_preserves_the_complete_bank
fn killed_transaction_owner_cannot_be_stolen_and_explicit_recovery_preserves_bank() {
    let b = Bank::new();
    let competing = preview(
        &b.world,
        &b.repo,
        "# Focus\n\nNext: verify the new lesson.\n",
    );
    let (owner, _) = b.pause(&b.prepared, "transaction-after-write:activeContext.md");
    let lock = b.bank.join(".agent-memory.lock");
    let lock_before = fs::read(&lock).unwrap();
    let owner_info = json::decode_bytes(&lock_before, Default::default()).unwrap();
    let pid = get(&owner_info, "pid").as_int().unwrap().as_u64().unwrap();
    assert_eq!(apply(&b.world, &competing).0.status, Status::Code(1));
    assert_eq!(fs::read(&lock).unwrap(), lock_before);
    refused(
        &b.world,
        "am writes",
        &[
            "recover".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
        ],
    );
    assert_eq!(fs::read(&lock).unwrap(), lock_before);
    b.world
        .run(["/bin/kill".to_owned(), "-KILL".to_owned(), pid.to_string()])
        .unwrap();
    assert_eq!(owner.wait().unwrap().status, Status::Signal(libc::SIGKILL));
    assert!(b.bank.join(MARKER).exists());
    b.unlock();
    refused(
        &b.world,
        "am writes",
        &[
            "inspect".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
        ],
    );
    let (output, finished) = b.recover(&b.recovery("finish"), &[]);
    assert_eq!(output.status, Status::Code(0));
    assert!(yes(&finished, "verified"));
    same_map(&canonical(&b.bank), &b.after);
    assert!(!b.bank.join(MARKER).exists());
    assert!(!lock.exists());
    assert_eq!(apply(&b.world, &competing).0.status, Status::Code(1));
    same_map(&canonical(&b.bank), &b.after);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_distinct_worktrees_hold_independent_locks_and_sharing_manifests_contain_only_canonical_paths
fn distinct_worktrees_hold_independent_locks_and_share_only_canonical_paths() {
    let (w, repo, bank) = fixture();
    let other = w.inside("independent worktree").unwrap();
    w.git_with(
        [
            OsString::from("worktree"),
            "add".into(),
            "-b".into(),
            "other-session".into(),
            other.as_os_str().to_owned(),
        ],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    let before = git_state(&w, &repo);
    let a = preview(&w, &repo, "# Focus\n\nFirst worktree fact.\n");
    let b = preview(&w, &other, "# Focus\n\nIndependent second worktree fact.\n");
    let (first, release_a) = pause(&w, &a, "locked-before-inventory", "worktree-a");
    let (second, release_b) = pause(&w, &b, "locked-before-inventory", "worktree-b");
    let first_lock = bank.join(".agent-memory.lock");
    let second_lock = other.join("memory-bank/.agent-memory.lock");
    let first_owner =
        json::decode_bytes(&fs::read(&first_lock).unwrap(), Default::default()).unwrap();
    let second_owner =
        json::decode_bytes(&fs::read(&second_lock).unwrap(), Default::default()).unwrap();
    assert_ne!(get(&first_owner, "pid"), get(&second_owner, "pid"));
    for root in [&repo, &other] {
        refused(
            &w,
            "am sharing",
            &["--repo".into(), root.as_os_str().to_owned()],
        );
    }
    fs::write(release_a, b"").unwrap();
    fs::write(release_b, b"").unwrap();
    let receipts = [collect(&first, 0), collect(&second, 0)];
    assert_ne!(
        get(&receipts[0], "receipt_path"),
        get(&receipts[1], "receipt_path")
    );
    assert_eq!(
        get(&receipts[0], "project_id"),
        get(&receipts[1], "project_id")
    );
    assert!(!first_lock.exists());
    assert!(!second_lock.exists());
    for root in [&repo, &other] {
        let boundary = good(
            &w,
            "am sharing",
            &["--repo".into(), root.as_os_str().to_owned()],
        );
        let paths: std::collections::BTreeSet<_> = get(&boundary, "eligible_memory_paths")
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            [
                "memory-bank/MEMORY.md",
                "memory-bank/activeContext.md",
                "memory-bank/projectbrief.md"
            ]
            .into()
        );
        assert_eq!(
            get(&boundary, "excluded_tracked_artifact_count"),
            &Value::Int(0.into())
        );
        assert!(!yes(&boundary, "publication_verified"));
        assert!(!yes(&boundary, "remote_freshness_verified"));
        let tracked = w
            .git_with(
                ["ls-files"],
                RunOptions {
                    cwd: Some(root),
                    ..RunOptions::default()
                },
            )
            .unwrap();
        assert!(!String::from_utf8_lossy(&tracked.stdout).contains(".agent-memory"));
    }
    assert_eq!(git_state(&w, &repo), before);
}
