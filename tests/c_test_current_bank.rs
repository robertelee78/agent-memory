mod support;
use agent_memory::codec::json::Value;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use support::cutover::*;
use support::{RunOptions, World};

const BODIES: &[(&str, &str)] = &[
    (
        "projectbrief.md",
        "# Project brief\n\nBounded retry helper only; [goals](../README.md).\n",
    ),
    (
        "productContext.md",
        "# Product context\n\nReduce repeated transient work; [problem](../README.md).\n",
    ),
    (
        "activeContext.md",
        "# Active context\n\nFocus: inspect retry limit.\nEvidence: [limit](../retry.rs).\nOpen: integration behavior remains unverified.\nNext: add an integration fixture.\n",
    ),
    (
        "systemPatterns.md",
        "# System patterns\n\nThe retry limit is explicit; [implementation](../retry.rs).\n",
    ),
    (
        "techContext.md",
        "# Technical context\n\nRust standard library only; [source](../retry.rs).\n",
    ),
    (
        "progress.md",
        "# Progress\n\nThe isolated limit probe returned 3; [actual probe output](../evidence.txt).\nIntegration behavior remains unverified.\n",
    ),
];
fn fixture(w: &World, tracked: bool, complete: bool) -> (PathBuf, PathBuf) {
    let repo = if tracked {
        w.repository("repo", false).unwrap()
    } else {
        let p = w.inside("plain-project").unwrap();
        fs::create_dir(&p).unwrap();
        p
    };
    fs::write(
        repo.join("README.md"),
        "Bounded retry helper; reduce transient work.\n",
    )
    .unwrap();
    fs::write(repo.join("retry.rs"), "pub fn limit() -> u32 { 3 }\n").unwrap();
    fs::write(repo.join("evidence.txt"), "3\n").unwrap();
    let files = render(
        if complete { BODIES } else { &BODIES[2..3] },
        "2026-10-22T20:00:00Z",
    );
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    write_files(&bank, &files);
    if tracked {
        w.git_with(
            ["add", "."],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
        w.git_with(
            [
                "commit",
                "-m",
                "Canonical current fixture with source evidence",
            ],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    }
    (repo, bank)
}
fn inspect(w: &World, repo: &std::path::Path) -> Value {
    good(
        w,
        "am inspect",
        &["--repo".into(), repo.as_os_str().to_owned()],
    )
}

#[test]
#[ignore = "needs am inspect route and recovery_frontier registered"]
// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_minimal_untracked_bank_survives_poisoned_then_removed_application_state
fn untracked_bank_survives_poisoned_then_removed_application_state() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, false, false);
    let poison = w.paths["state"].join("agent-memory");
    fs::create_dir(&poison).unwrap();
    fs::write(
        poison.join("current.json"),
        "{\"body\":\"STALE_CACHE_CANARY\"}",
    )
    .unwrap();
    fs::write(
        w.paths["cache"].join("index.sqlite"),
        "Not a usable database; must not be opened",
    )
    .unwrap();
    fs::write(w.paths["data"].join("events.jsonl"), "STALE_CACHE_CANARY\n").unwrap();
    let before = inspect(&w, &repo);
    assert!(!yes(&before, "reads_memory_generated_state"));
    assert_eq!(string(get(&before, "git"), "status"), "not-discovered");
    let contexts = get(&before, "contexts").as_object().unwrap();
    assert_eq!(contexts.len(), 6);
    for (path, context) in contexts {
        assert_eq!(yes(context, "present"), path == "activeContext.md");
        if path != "activeContext.md" {
            assert!(!string(context, "omission_reason").is_empty());
        }
    }
    let current = physical(&bank);
    for name in ["state", "cache", "data", "config"] {
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
    let after = inspect(&w, &repo);
    assert_eq!(after, before);
    assert_eq!(physical(&bank), current);
    assert_eq!(
        fs::read_to_string(bank.join("activeContext.md")).unwrap(),
        BODIES[2].1
    );
    assert!(!format!("{after:?}").contains("STALE_CACHE_CANARY"));
    assert!(!yes(get(&after, "sharing"), "publication_verified"));
    assert!(!yes(get(&after, "sharing"), "freshness_verified"));
}

#[test]
#[ignore = "needs am inspect route and recovery_frontier registered"]
// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_all_six_tracked_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_are_shared
fn all_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_shared() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, true, true);
    let head = w
        .git_with(
            ["rev-parse", "HEAD"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap()
        .stdout;
    fs::write(repo.join("feature.rs"), "const UNFINISHED: bool = true;\n").unwrap();
    w.git_with(
        ["add", "feature.rs"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    fs::write(repo.join("scratch.txt"), "Preserve working copy\n").unwrap();
    let target = bank.join("activeContext.md");
    fs::write(
        &target,
        fs::read_to_string(&target)
            .unwrap()
            .replace("add an integration fixture", "inspect the second clone"),
    )
    .unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let current = physical(&bank);
    for name in ["state", "cache", "data", "config"] {
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
    let result = inspect(&w, &repo);
    assert!(
        get(&result, "contexts")
            .as_object()
            .unwrap()
            .iter()
            .all(|(_, v)| yes(v, "present"))
    );
    let git = get(&result, "git");
    assert_eq!(
        string(git, "context_head"),
        std::str::from_utf8(&head).unwrap().trim_end_matches('\n')
    );
    let indexed: Vec<_> = get(git, "indexed_paths")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        indexed,
        current.keys().map(String::as_str).collect::<Vec<_>>()
    );
    assert!(get(git, "not_indexed_paths").as_array().unwrap().is_empty());
    assert_eq!(string(git, "current_bytes_committed"), "not-assessed");
    assert!(!yes(get(&result, "sharing"), "publication_verified"));
    assert!(!yes(get(&result, "sharing"), "freshness_verified"));
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        w.git_with(
            ["rev-parse", "HEAD"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            }
        )
        .unwrap()
        .stdout,
        head
    );
    assert_eq!(
        fs::read_to_string(repo.join("scratch.txt")).unwrap(),
        "Preserve working copy\n"
    );
    assert_eq!(physical(&bank), current);
    assert!(
        fs::read_to_string(bank.join("progress.md"))
            .unwrap()
            .contains("[actual probe output](../evidence.txt)")
    );
    let clone = w.clone(&repo, "fresh-clone").unwrap();
    assert_eq!(string(&inspect(&w, &clone), "project_id"), PROJECT);
    assert_eq!(
        fs::read_to_string(clone.join("memory-bank/activeContext.md")).unwrap(),
        BODIES[2].1
    );
    assert!(fs::read_to_string(target).unwrap().contains("second clone"));
}

#[test]
#[ignore = "needs am inspect route and recovery_frontier registered"]
// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_git_root_spelling_preserves_trailing_spaces
fn git_root_spelling_preserves_trailing_spaces() {
    let w = World::new().unwrap();
    let (repo, _) = fixture(&w, true, false);
    let renamed = w.inside("repo with trailing space ").unwrap();
    fs::rename(repo, &renamed).unwrap();
    let result = inspect(&w, &renamed);
    assert_eq!(string(get(&result, "git"), "status"), "checkout");
    assert!(
        get(get(&result, "git"), "not_indexed_paths")
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "needs am inspect route and recovery_frontier registered"]
// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_busy_or_interrupted_bank_is_not_repaired_by_state_loss
fn busy_or_interrupted_bank_is_not_repaired_by_state_loss() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, false, false);
    for name in ["state", "cache", "data"] {
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
    for name in [".agent-memory.lock", MARKER] {
        let marker = bank.join(name);
        fs::write(&marker, "Unknown recovery evidence; preserve it\n").unwrap();
        let before = physical(&bank);
        refused(
            &w,
            "am inspect",
            &["--repo".into(), repo.as_os_str().to_owned()],
        );
        assert_eq!(physical(&bank), before);
        fs::remove_file(marker).unwrap();
    }
}

#[test]
#[ignore = "needs am inspect route and recovery_frontier registered"]
// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_git_inspection_ignores_ambient_repo_redirection_and_does_not_execute_fsmonitor_or_write_trace
fn git_inspection_ignores_ambient_redirection_fsmonitor_and_trace() {
    let mut w = World::new().unwrap();
    let (repo, _) = fixture(&w, true, false);
    let foreign = w.repository("foreign", false).unwrap();
    let canary = w.inside("fsmonitor-ran").unwrap();
    let script = w.inside("fsmonitor-canary").unwrap();
    fs::write(
        &script,
        format!("#!/bin/sh\nprintf ran > '{}'\n", canary.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    w.git_with(
        [
            OsString::from("config"),
            "core.fsmonitor".into(),
            script.into_os_string(),
        ],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    let head = w
        .git_with(
            ["rev-parse", "HEAD"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap()
        .stdout;
    let index = fs::read(repo.join(".git/index")).unwrap();
    let trace = w.inside("git-trace.json").unwrap();
    w.env
        .insert("GIT_DIR".into(), foreign.join(".git").into_os_string());
    w.env
        .insert("GIT_TRACE2_EVENT".into(), trace.as_os_str().to_owned());
    let result = inspect(&w, &repo);
    assert_eq!(
        string(get(&result, "git"), "context_head"),
        std::str::from_utf8(&head).unwrap().trim_end_matches('\n')
    );
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert!(!canary.exists());
    assert!(!trace.exists());
}
