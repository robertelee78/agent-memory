mod support;
use std::fs;
use std::path::PathBuf;
use support::cutover::*;
use support::{RunOptions, World};

fn fixture(w: &World) -> (PathBuf, PathBuf) {
    let repo = w.repository("repo", false).unwrap();
    fs::write(
        repo.join("README.md"),
        "# Fixture\n\nApproved intent: investigate retry behavior within this component.\n",
    )
    .unwrap();
    let request = object([
        ("project-id", text(PROJECT)),
        ("router-updated", text("2026-10-22T18:00:00Z")),
        (
            "contexts",
            object([
                (
                    "projectbrief.md",
                    object([
                        (
                            "body",
                            text(
                                "# Project brief\n\nGoal: investigate retry behavior. This is approved intent, not proof of implementation; [source](../README.md).\n",
                            ),
                        ),
                        ("purpose", text("Goals and evidence")),
                        ("scope", text("repository")),
                        ("lifecycle", text("active")),
                        ("read-when", text("When resuming the project")),
                    ]),
                ),
                (
                    "activeContext.md",
                    object([
                        (
                            "body",
                            text(
                                "# Active context\n\nCurrent focus: retry component only.\n\nOpen question: applicability outside this component is unknown.\n\nNext: inspect the current test result; [intent](../README.md).\n",
                            ),
                        ),
                        ("purpose", text("Current retry focus")),
                        ("scope", text("component:retry")),
                        ("lifecycle", text("active")),
                        ("read-when", text("When editing the retry component")),
                    ]),
                ),
                (
                    "progress.md",
                    object([
                        (
                            "body",
                            text(
                                "# Historical progress\n\nExpired guidance for branch legacy; not current project guidance. RETIRED_GUIDANCE_CANARY\n",
                            ),
                        ),
                        ("purpose", text("Retired branch history")),
                        ("scope", text("branch:legacy")),
                        ("lifecycle", text("expired")),
                        (
                            "read-when",
                            text("Only when investigating that earlier branch"),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "omitted",
            object([
                (
                    "productContext.md",
                    text("No distinct useful evidence supplied"),
                ),
                (
                    "systemPatterns.md",
                    text("No distinct useful evidence supplied"),
                ),
                (
                    "techContext.md",
                    text("No distinct useful evidence supplied"),
                ),
            ]),
        ),
    ]);
    let rendered = agent_memory::bank_contract::render(&request).unwrap();
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    write_files(&bank, get(&rendered, "files"));
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Useful canonical bank"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    (repo, bank)
}
fn delete_generated(w: &World) {
    for name in ["config", "state", "data", "cache", "backups"] {
        fs::write(
            w.paths[name].join("disposable-memory-index.json"),
            "{\"stale\": \"not canonical truth\"}",
        )
        .unwrap();
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
}
fn bounded(path: impl AsRef<std::path::Path>, limit: usize) -> String {
    let raw = fs::read(path).unwrap();
    assert!(
        raw.len() <= limit,
        "narrow the read rather than truncate conditions"
    );
    String::from_utf8(raw).unwrap()
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_usability.py::BankUsabilityContract.test_fresh_clone_resumes_from_bounded_current_markdown_without_any_generated_state
fn fresh_clone_resumes_from_bounded_canonical_markdown_without_state() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w);
    let source = physical(&bank);
    let clone = w.clone(&repo, "fresh clone").unwrap();
    let cloned_bank = clone.join("memory-bank");
    delete_generated(&w);
    let receipt = good(
        &w,
        "bank_contract.py",
        &[
            "validate".into(),
            "--repo".into(),
            clone.as_os_str().to_owned(),
        ],
    );
    assert_eq!(string(&receipt, "project_id"), PROJECT);
    assert!(!yes(&receipt, "writes_performed"));
    let router = bounded(cloned_bank.join("MEMORY.md"), 16384);
    for retained in ["component:retry", "branch:legacy", "expired"] {
        assert!(router.contains(retained));
    }
    let routes = get(&receipt, "routes").as_array().unwrap();
    let current: Vec<_> = routes
        .iter()
        .filter(|r| ["projectbrief.md", "activeContext.md"].contains(&string(r, "path")))
        .collect();
    assert_eq!(current.len(), 2);
    assert!(current.iter().all(|r| string(r, "lifecycle") == "active"));
    assert_eq!(
        string(
            routes
                .iter()
                .find(|r| string(r, "path") == "activeContext.md")
                .unwrap(),
            "scope"
        ),
        "component:retry"
    );
    let historical = routes
        .iter()
        .find(|r| string(r, "path") == "progress.md")
        .unwrap();
    assert_eq!(string(historical, "lifecycle"), "expired");
    assert_eq!(string(historical, "scope"), "branch:legacy");
    let assembled = format!(
        "{}\n{}",
        bounded(cloned_bank.join("projectbrief.md"), 4096),
        bounded(cloned_bank.join("activeContext.md"), 4096)
    );
    for retained in [
        "approved intent, not proof of implementation",
        "outside this component is unknown",
        "Next: inspect the current test result",
    ] {
        assert!(assembled.contains(retained));
    }
    assert!(!assembled.contains("RETIRED_GUIDANCE_CANARY"));
    assert!(clone.join("README.md").is_file());
    assert_eq!(physical(&cloned_bank), source);
    for name in ["config", "state", "data", "cache", "backups"] {
        assert!(!w.paths[name].exists());
    }
    let copy = w.inside("file-only copy").unwrap();
    fs::create_dir(&copy).unwrap();
    fs::create_dir(copy.join("memory-bank")).unwrap();
    for (path, raw) in &source {
        fs::write(copy.join("memory-bank").join(path), raw).unwrap();
    }
    assert!(!copy.join(".git").exists());
    assert!(
        bounded(copy.join("memory-bank/activeContext.md"), 4096)
            .contains("Current focus: retry component only")
    );
    let checked = good(
        &w,
        "bank_contract.py",
        &["validate".into(), "--repo".into(), copy.into_os_string()],
    );
    assert_eq!(string(&checked, "project_id"), PROJECT);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_usability.py::BankUsabilityContract.test_deleting_generated_state_does_not_make_an_interrupted_bank_usable_or_repair_it
fn state_loss_never_repairs_or_hides_an_interrupted_bank() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w);
    delete_generated(&w);
    let canonical = physical(&bank);
    for name in [MARKER, ".agent-memory.lock"] {
        let marker = bank.join(name);
        fs::write(
            &marker,
            "Unknown interrupted operation; preserve for explicit inspection.\n",
        )
        .unwrap();
        let receipt = refused(
            &w,
            "bank_contract.py",
            &[
                "validate".into(),
                "--repo".into(),
                repo.as_os_str().to_owned(),
            ],
        );
        assert!(!yes(&receipt, "valid"));
        assert!(marker.exists());
        let retained: std::collections::BTreeMap<_, _> = physical(&bank)
            .into_iter()
            .filter(|(p, _)| canonical.contains_key(p))
            .collect();
        assert_eq!(retained, canonical);
        fs::remove_file(marker).unwrap();
    }
    let receipt = good(
        &w,
        "bank_contract.py",
        &["validate".into(), "--repo".into(), repo.into_os_string()],
    );
    assert_eq!(string(&receipt, "project_id"), PROJECT);
    for name in ["config", "state", "data", "cache", "backups"] {
        assert!(!w.paths[name].exists());
    }
}
