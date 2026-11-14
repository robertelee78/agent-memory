mod support;
use agent_memory::codec::json::Value;
use agent_memory::process::Status;
use std::fs;
use std::path::{Path, PathBuf};
use support::cutover::*;
use support::{RunOptions, World};
const BODY: &str = "# Active context\n\n## Current focus\nVerify the component-specific retry lesson; [task](../task.md).\n\n## Recent changes\nThe branch's isolated test passed; [evidence](../test-result.md).\n\n## Open questions\nApplicability outside this component remains unverified.\n\n## Next step\nInspect the first clone's result.\n";
fn fixture() -> (World, PathBuf, PathBuf) {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    let mut files = render(&[("activeContext.md", BODY)], "2026-10-22T18:00:00Z");
    let router = string(&files, "MEMORY.md")
        .replace("| repository | active |", "| component:retry | active |");
    put(&mut files, "MEMORY.md", text(router));
    write_files(&bank, &files);
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Fixture checkpoint base"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    (w, repo, bank)
}
fn preview(w: &World, repo: &Path, body: &str) -> (agent_memory::process::Output, Value) {
    let path = w.inside("reviewed-context.md").unwrap();
    fs::write(&path, body).unwrap();
    command(
        w,
        "am writes",
        &[
            "preview".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--path".into(),
            "activeContext.md".into(),
            "--content".into(),
            path.into_os_string(),
        ],
    )
}
fn apply(w: &World, p: &Value) -> (agent_memory::process::Output, Value) {
    command(
        w,
        "am writes",
        &[
            "apply".into(),
            "--proposal".into(),
            save(w, p, "write-proposal.json").into_os_string(),
        ],
    )
}

#[test]
// obligation: tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_file_only_checkpoint_resumes_without_generated_state_and_preserves_other_work
fn file_only_checkpoint_resumes_without_generated_state_preserving_other_work() {
    let (w, repo, bank) = fixture();
    fs::write(repo.join("feature.rs"), "const UNFINISHED: bool = true;\n").unwrap();
    w.git_with(
        ["add", "feature.rs"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    fs::write(repo.join("scratch.txt"), "Unrelated unstaged work\n").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
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
    let router = fs::read(bank.join("MEMORY.md")).unwrap();
    for name in ["config", "state", "cache"] {
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
    let target = bank.join("activeContext.md");
    assert!(!bank.join(".agent-memory.lock").exists());
    assert!(!bank.join(MARKER).exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), BODY);
    let updated = BODY.replace(
        "Inspect the first clone's result.",
        "Inspect the second clone's refresh result before broadening applicability.",
    );
    fs::write(&target, &updated).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), updated);
    let diff = w
        .git_with(
            ["diff", "--name-only"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(diff.stdout, b"memory-bank/activeContext.md\n");
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
        "Unrelated unstaged work\n"
    );
    assert_eq!(fs::read(bank.join("MEMORY.md")).unwrap(), router);
    let resumed = w
        .run([
            std::ffi::OsString::from("/bin/cat"),
            target.as_os_str().to_owned(),
        ])
        .unwrap();
    let resumed = std::str::from_utf8(&resumed.stdout).unwrap();
    for retained in [
        "## Current focus",
        "[task](../task.md)",
        "[evidence](../test-result.md)",
        "remains unverified",
        "second clone's refresh result",
    ] {
        assert!(resumed.contains(retained));
    }
    assert!(!w.paths["state"].exists());
    let base = w
        .git_with(
            ["show", "HEAD:memory-bank/activeContext.md"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap();
    fs::write(&target, base.stdout).unwrap();
    assert_eq!(fs::read_to_string(target).unwrap(), BODY);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read_to_string(repo.join("scratch.txt")).unwrap(),
        "Unrelated unstaged work\n"
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_stale_mediated_proposal_refuses_then_fresh_write_preserves_manual_evidence
fn stale_mediated_proposal_refuses_and_fresh_write_preserves_manual_evidence() {
    let (w, repo, bank) = fixture();
    support::identity_fixtures::bind(&w, &repo).unwrap();
    let (output, old) = preview(
        &w,
        &repo,
        &BODY.replace("first clone", "old in-session snapshot"),
    );
    assert_eq!(output.status, Status::Code(0));
    let target = bank.join("activeContext.md");
    let current = BODY.replace(
        "Inspect the first clone's result.",
        "Inspect the second clone's result; [manual evidence](../manual-result.md).",
    );
    fs::write(&target, &current).unwrap();
    let (output, error) = apply(&w, &old);
    assert_eq!(output.status, Status::Code(1));
    assert!(!yes(&error, "verified"));
    assert_eq!(fs::read_to_string(&target).unwrap(), current);
    let reviewed = current.replace(
        "Verify the component-specific retry lesson",
        "Continue checking the component-specific retry lesson",
    );
    let (output, fresh) = preview(&w, &repo, &reviewed);
    assert_eq!(output.status, Status::Code(0));
    assert_ne!(
        get(get(&old, "proposal"), "observed_hashes"),
        get(get(&fresh, "proposal"), "observed_hashes")
    );
    let (output, applied) = apply(&w, &fresh);
    assert_eq!(output.status, Status::Code(0));
    assert!(yes(&applied, "verified"));
    let body = fs::read_to_string(target).unwrap();
    assert_eq!(body, reviewed);
    for retained in [
        "[manual evidence](../manual-result.md)",
        "[evidence](../test-result.md)",
        "remains unverified",
    ] {
        assert!(body.contains(retained));
    }
    assert!(!body.contains("old in-session snapshot"));
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_visible_lock_and_interrupted_bank_stop_mediated_resume_without_repair
fn visible_lock_and_interruption_stop_mediated_resume_without_repair() {
    let (w, repo, bank) = fixture();
    let original = fs::read(bank.join("activeContext.md")).unwrap();
    for name in [".agent-memory.lock", MARKER] {
        let marker = bank.join(name);
        fs::write(
            &marker,
            "Unrecognized owner/state requires inspection; do not remove by age.\n",
        )
        .unwrap();
        refused(
            &w,
            "am writes",
            &[
                "inspect".into(),
                "--repo".into(),
                repo.as_os_str().to_owned(),
            ],
        );
        assert_eq!(
            preview(
                &w,
                &repo,
                &format!("{BODY}\nDo not apply during interruption.\n")
            )
            .0
            .status,
            Status::Code(1)
        );
        assert_eq!(fs::read(bank.join("activeContext.md")).unwrap(), original);
        assert!(marker.exists());
        fs::remove_file(marker).unwrap();
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_invalid_external_edit_is_reported_instead_of_template_repair
fn invalid_external_edit_is_reported_instead_of_template_repair() {
    let (w, repo, bank) = fixture();
    let (output, old) = preview(&w, &repo, BODY);
    assert_eq!(output.status, Status::Code(0));
    let target = bank.join("activeContext.md");
    fs::write(&target, "   \n").unwrap();
    let (output, error) = apply(&w, &old);
    assert_eq!(output.status, Status::Code(1));
    assert!(!yes(&error, "verified"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "   \n");
    assert_eq!(preview(&w, &repo, BODY).0.status, Status::Code(1));
    assert_eq!(fs::read_to_string(target).unwrap(), "   \n");
}
