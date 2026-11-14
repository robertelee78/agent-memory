//! Project/inspection entry points, exercised through the real binary.
mod support;

use agent_memory::codec::json::{self, Value};
use agent_memory::process::{Output, Status};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use support::{RunOptions, World};

const PROJECT: &str = "45aa2f8e-6218-4c08-9265-797a7c62e30e";
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

fn s(text: impl Into<String>) -> Value {
    Value::String(text.into())
}
fn obj(values: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap()
}
fn set(value: &mut Value, key: &str, replacement: Value) {
    let Value::Object(fields) = value else {
        panic!("fixture object")
    };
    let (_, field) = fields.iter_mut().find(|(name, _)| name == key).unwrap();
    *field = replacement;
}
fn run(w: &World, args: &[&str]) -> Output {
    w.am_with(
        args,
        RunOptions {
            check: false,
            ..Default::default()
        },
    )
    .unwrap()
}
fn receipt(output: Output, code: i32) -> Value {
    assert_eq!(output.status, Status::Code(code), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(output.stdout.ends_with(b"\n"));
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}
fn save(w: &World, name: &str, value: &Value) -> PathBuf {
    let path = w.inside(name).unwrap();
    fs::write(&path, json::encode(value, json::DEFAULT).unwrap()).unwrap();
    path
}
fn project(w: &World, repo: &Path, command: &str, rest: &[&str], code: i32) -> Value {
    let mut args = vec!["project", command, "--repo", repo.to_str().unwrap()];
    args.extend_from_slice(rest);
    receipt(run(w, &args), code)
}
fn preview(w: &World, repo: &Path) -> Value {
    project(
        w,
        repo,
        "preview",
        &[
            "--action",
            "create",
            "--reason",
            "Explicit disposable fixture",
        ],
        0,
    )
}
fn apply(w: &World, proposal: &Value, code: i32) -> Value {
    let path = save(w, "project-proposal.json", proposal);
    receipt(
        run(
            w,
            &["project", "apply", "--proposal", path.to_str().unwrap()],
        ),
        code,
    )
}
fn bank(repo: &Path, project_id: &str, complete: bool) -> PathBuf {
    let proposal = obj([
        ("project-id", s(project_id)),
        ("router-updated", s("2026-10-22T20:00:00Z")),
        (
            "contexts",
            obj(BODIES
                .iter()
                .filter(|(name, _)| complete || *name == "activeContext.md")
                .map(|(name, body)| {
                    (
                        *name,
                        obj([
                            ("body", s(*body)),
                            (
                                "purpose",
                                s(format!("Useful {}", name.trim_end_matches(".md"))),
                            ),
                            ("scope", s("repository")),
                            ("lifecycle", s("active")),
                            ("read-when", s("When continuing retry work")),
                        ]),
                    )
                })),
        ),
        (
            "omitted",
            obj(BODIES
                .iter()
                .filter(|(name, _)| !complete && *name != "activeContext.md")
                .map(|(name, _)| (*name, s("No additional verified context needed yet")))),
        ),
    ]);
    let rendered = agent_memory::bank_contract::render(&proposal).unwrap();
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    for (name, body) in get(&rendered, "files").as_object().unwrap() {
        fs::write(bank.join(name), body.as_str().unwrap()).unwrap();
    }
    bank
}
fn fixture(w: &World, tracked: bool, complete: bool) -> (PathBuf, PathBuf) {
    let repo = if tracked {
        w.repository("current checkout", false).unwrap()
    } else {
        let path = w.inside("plain-project").unwrap();
        fs::create_dir(&path).unwrap();
        path
    };
    fs::write(
        repo.join("README.md"),
        "Bounded retry helper; reduce transient work.\n",
    )
    .unwrap();
    fs::write(repo.join("retry.rs"), "pub fn limit() -> usize { 3 }\n").unwrap();
    fs::write(repo.join("evidence.txt"), "3\n").unwrap();
    let bank = bank(&repo, PROJECT, complete);
    if tracked {
        git(w, &repo, &["add", "."]);
        git(
            w,
            &repo,
            &[
                "commit",
                "-m",
                "Canonical current fixture with source evidence",
            ],
        );
    }
    (repo, bank)
}
fn git(w: &World, repo: &Path, args: &[&str]) -> Output {
    w.git_with(
        args,
        RunOptions {
            cwd: Some(repo),
            ..Default::default()
        },
    )
    .unwrap()
}
fn snapshot(path: &Path) -> BTreeMap<OsString, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect()
}
fn inspect(w: &World, repo: &Path, code: i32) -> Value {
    receipt(run(w, &["inspect", "--repo", repo.to_str().unwrap()]), code)
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_normalization_strips_secrets_and_unifies_only_documented_equivalence
#[test]
fn project_normalize_preserves_bytes_and_sanitizes_refusals() {
    let w = World::new().unwrap();
    let same = [
        "https://alice:secret@GITHUB.COM/Owner/Repo.git/?token=secret#secret",
        "git@github.com:Owner/Repo.git",
        "ssh://git@GITHUB.com:22/OWNER/REPO.git/",
        "https://github.com:443/owner/repo",
        "http://github.com:80/owner/repo.git",
        "repo://github.com/owner/repo",
        "https://github.com/%6fwner/repo.git",
    ];
    let expected = concat!(
        "{\n  \"schema\": \"agent-memory/project-result-v1\",\n",
        "  \"operation\": \"normalize\",\n",
        "  \"security_boundary\": \"Repository and OS permissions; project IDs identify a bank, not authentication or an ACL.\",\n",
        "  \"changed_memory_paths\": [],\n",
        "  \"canonical_remote\": \"repo://github.com/owner/repo\",\n",
        "  \"writes_performed\": false\n}\n",
    );
    for remote in same {
        let output = run(&w, &["project", "normalize", remote]);
        assert_eq!(output.stdout, expected.as_bytes());
        receipt(output, 0);
    }
    let distinct = [
        "https://other.example/owner/repo.git",
        "https://github.com/another/repo.git",
        "https://github.com/owner/fork.git",
        "ssh://git@github.com:2222/owner/repo.git",
        "https://Example.com/Owner/Repo.git",
        "https://example.com/owner/repo.git",
    ];
    let mut identities = BTreeSet::from(["repo://github.com/owner/repo".to_owned()]);
    for remote in distinct {
        let value = receipt(run(&w, &["project", "normalize", remote]), 0);
        assert!(identities.insert(text(&value, "canonical_remote").to_owned()));
    }
    for remote in [
        "https://secret@host/owner/%2Frepo",
        "ext::secret",
        "https://secret@host/a/../b",
        "ssh://secret@host:wrong/repo",
    ] {
        let output = run(&w, &["project", "normalize", remote]);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
        let value = receipt(output, 1);
        assert_eq!(get(&value, "verified"), &Value::Bool(false));
    }
    let local = receipt(
        run(
            &w,
            &[
                "project",
                "normalize",
                "./local",
                "--base",
                w.root.to_str().unwrap(),
            ],
        ),
        0,
    );
    assert!(text(&local, "canonical_remote").contains("local"));
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_bootstrap_is_read_only_until_explicit_local_registration_and_bank_initialization
#[test]
fn project_status_preview_apply_are_explicit_and_read_only_until_apply() {
    let w = World::new().unwrap();
    let repo = w.repository("project checkout", false).unwrap();
    let registry = w.root.join("config/agent-memory/projects.json");
    let status = project(&w, &repo, "status", &[], 0);
    assert_eq!(get(&status, "project_id"), &Value::Null);
    assert_eq!(get(&status, "resolved"), &Value::Bool(false));
    assert!(!registry.parent().unwrap().exists());
    let proposal = preview(&w, &repo);
    assert!(!registry.parent().unwrap().exists());
    let identity = text(&proposal, "project_id");
    let applied = apply(&w, &proposal, 0);
    assert_eq!(get(&applied, "verified"), &Value::Bool(true));
    assert_eq!(get(&applied, "changed_memory_paths"), &Value::Array(vec![]));
    assert!(!repo.join("memory-bank").exists());
    assert!(
        json::encode(get(&applied, "binding_status"), json::DEFAULT)
            .unwrap()
            .contains("pending")
    );
    bank(&repo, identity, false);
    assert_eq!(
        get(&project(&w, &repo, "status", &[], 0), "resolved"),
        &Value::Bool(true)
    );
    assert!(!registry.with_file_name("developers.json").exists());
    let before = fs::read(&registry).unwrap();
    assert_eq!(
        get(&apply(&w, &proposal, 0), "already_applied"),
        &Value::Bool(true)
    );
    assert_eq!(fs::read(&registry).unwrap(), before);
    project(
        &w,
        &repo,
        "preview",
        &[
            "--action",
            "create",
            "--reason",
            "Must not replace existing identity",
        ],
        1,
    );
    project(&w, &w.root.join("missing repository"), "status", &[], 1);
    apply(&w, &obj([] as [(&str, Value); 0]), 1);
    let output = run(&w, &["project", "apply", "--proposal", "absent.json"]);
    assert!(text(&receipt(output, 1), "error").contains("absent.json"));
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_changed_registry_bank_remote_or_authority_refuses_stale_proposals
#[test]
fn project_apply_refuses_changed_repository_evidence() {
    let w = World::new().unwrap();
    let repo = w.repository("changing checkout", false).unwrap();
    git(
        &w,
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/project.git",
        ],
    );
    let proposal = preview(&w, &repo);
    git(
        &w,
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/other/project.git",
        ],
    );
    apply(&w, &proposal, 1);
    assert!(!w.root.join("config/agent-memory/projects.json").exists());
    let output = project(&w, &repo, "status", &["--authority", "missing"], 0);
    assert_eq!(get(&output, "resolved"), &Value::Bool(false));
    project(
        &w,
        &repo,
        "preview",
        &[
            "--authority",
            "missing",
            "--action",
            "create",
            "--reason",
            "Explicit refusal",
        ],
        1,
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_project_registry_stale_lock_recovery_is_bounded_and_read_back
#[test]
fn project_recover_proves_dead_owner_and_preserves_registry() {
    let w = World::new().unwrap();
    let repo = w.repository("recover checkout", false).unwrap();
    apply(&w, &preview(&w, &repo), 0);
    let registry = w.root.join("config/agent-memory/projects.json");
    let lock = registry.with_file_name(".projects.lock");
    let mut owner = obj([
        ("operation_id", s(agent_memory::host::uuid4().unwrap())),
        ("operation", s("project-create")),
        ("host", s(agent_memory::host::hostname().unwrap())),
        ("pid", Value::Int(std::process::id().into())),
        ("start", s("2026-10-22T18:00:00Z")),
    ]);
    fs::write(&lock, json::encode(&owner, json::DEFAULT).unwrap()).unwrap();
    let refused = receipt(run(&w, &["project", "recover"]), 1);
    assert!(text(&refused, "error").contains("still alive"));
    let dead = w.run(["/usr/bin/true"]).unwrap().pid;
    set(&mut owner, "pid", Value::Int(dead.into()));
    fs::write(&lock, json::encode(&owner, json::DEFAULT).unwrap()).unwrap();
    let prepared = receipt(run(&w, &["project", "recover"]), 0);
    assert!(lock.exists());
    let path = save(&w, "recovery.json", &prepared);
    let before = fs::read(&registry).unwrap();
    let recovered = receipt(
        run(
            &w,
            &["project", "recover", "--proposal", path.to_str().unwrap()],
        ),
        0,
    );
    assert_eq!(get(&recovered, "verified"), &Value::Bool(true));
    assert!(!lock.exists());
    assert_eq!(fs::read(&registry).unwrap(), before);
    receipt(
        run(&w, &["project", "recover", "--proposal", "missing.json"]),
        1,
    );
}

#[test]
fn routes_enforce_argument_surface_and_render_public_usage() {
    let w = World::new().unwrap();
    for (args, message) in [
        (
            vec!["project"],
            "the following arguments are required: command",
        ),
        (
            vec!["project", "normalize"],
            "the following arguments are required: remote",
        ),
        (
            vec!["project", "status", "--bad"],
            "unrecognized arguments: --bad",
        ),
        (
            vec!["project", "preview"],
            "the following arguments are required: --action, --reason",
        ),
        (
            vec!["project", "preview", "--action", "bad", "--reason", "r"],
            "invalid choice: 'bad'",
        ),
        (
            vec!["project", "apply"],
            "the following arguments are required: --proposal",
        ),
        (
            vec!["project", "recover", "--proposal"],
            "argument --proposal: expected one argument",
        ),
        (
            vec!["inspect", "--repo"],
            "argument --repo: expected one argument",
        ),
        (
            vec!["inspect", "--path"],
            "argument --path: expected one argument",
        ),
        (
            vec!["inspect", "--unknown"],
            "unrecognized arguments: --unknown",
        ),
    ] {
        let output = run(&w, &args);
        assert_eq!(output.status, Status::Code(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with("usage: am "), "{stderr}");
        assert!(stderr.contains(message), "{args:?}: {stderr}");
    }
    for args in [
        vec!["project", "--help"],
        vec!["project", "normalize", "--help"],
        vec!["project", "status", "--help"],
        vec!["project", "preview", "--help"],
        vec!["project", "apply", "--help"],
        vec!["project", "recover", "--help"],
        vec!["inspect", "--help"],
    ] {
        let output = run(&w, &args);
        assert_eq!(output.status, Status::Code(0));
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("show this help message and exit")
        );
    }
    let output = run(&w, &["inspect", "--help"]);
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("usage: am inspect [-h] [--repo REPO] [--path PATH]\n")
    );
    assert!(!w.root.join("config/agent-memory").exists());
}

#[test]
fn inspect_preserves_typed_refusal_bytes() {
    let w = World::new().unwrap();
    let expected = b"{\"schema\": \"agent-memory/current-bank-view-v1\", \"valid\": false, \"writes_performed\": false, \"error\": \"An existing canonical bank directory is required; initialization uses the multi-file protocol\"}\n";
    for args in [
        vec!["inspect"],
        vec!["inspect", "--path", "activeContext.md"],
    ] {
        let output = run(&w, &args);
        assert_eq!(output.stdout, expected);
        receipt(output, 1);
    }
}

// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_minimal_untracked_bank_survives_poisoned_then_removed_application_state
#[test]
#[ignore = "needs recovery_frontier registered"]
fn inspect_ignores_poisoned_then_removed_application_state() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, false, false);
    let poison = w.root.join("state/agent-memory");
    fs::create_dir(&poison).unwrap();
    fs::write(
        poison.join("current.json"),
        "{\"body\":\"STALE_CACHE_CANARY\"}",
    )
    .unwrap();
    fs::write(
        w.root.join("cache/index.sqlite"),
        "Not a usable database; must not be opened",
    )
    .unwrap();
    fs::write(w.root.join("data/events.jsonl"), "STALE_CACHE_CANARY\n").unwrap();
    let before = inspect(&w, &repo, 0);
    assert_eq!(
        get(&before, "reads_memory_generated_state"),
        &Value::Bool(false)
    );
    assert_eq!(text(get(&before, "git"), "status"), "not-discovered");
    assert_eq!(get(&before, "contexts").as_object().unwrap().len(), 6);
    for (name, _) in BODIES {
        let context = get(get(&before, "contexts"), name);
        assert_eq!(
            get(context, "present"),
            &Value::Bool(*name == "activeContext.md")
        );
        if *name != "activeContext.md" {
            assert!(!text(context, "omission_reason").is_empty());
        }
    }
    let current = snapshot(&bank);
    for name in ["state", "cache", "data", "config"] {
        fs::remove_dir_all(w.root.join(name)).unwrap();
    }
    let after = inspect(&w, &repo, 0);
    assert_eq!(after, before);
    assert_eq!(snapshot(&bank), current);
    assert_eq!(
        fs::read_to_string(bank.join("activeContext.md")).unwrap(),
        BODIES[2].1
    );
    assert!(
        !json::encode(&after, json::DEFAULT)
            .unwrap()
            .contains("STALE_CACHE_CANARY")
    );
    assert_eq!(
        get(get(&after, "sharing"), "publication_verified"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(get(&after, "sharing"), "freshness_verified"),
        &Value::Bool(false)
    );
}

// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_all_six_tracked_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_are_shared
#[test]
#[ignore = "needs recovery_frontier registered"]
fn inspect_retains_git_evidence_without_claiming_current_bytes_are_shared() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, true, true);
    let head = String::from_utf8(git(&w, &repo, &["rev-parse", "HEAD"]).stdout).unwrap();
    fs::write(repo.join("feature.rs"), "unfinished = true\n").unwrap();
    git(&w, &repo, &["add", "feature.rs"]);
    fs::write(repo.join("scratch.txt"), "Preserve working copy\n").unwrap();
    let target = bank.join("activeContext.md");
    fs::write(
        &target,
        BODIES[2]
            .1
            .replace("add an integration fixture", "inspect the second clone"),
    )
    .unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let current = snapshot(&bank);
    for name in ["state", "cache", "data", "config"] {
        fs::remove_dir_all(w.root.join(name)).unwrap();
    }
    let value = inspect(&w, &repo, 0);
    assert!(
        get(&value, "contexts")
            .as_object()
            .unwrap()
            .iter()
            .all(|(_, context)| get(context, "present") == &Value::Bool(true))
    );
    let context = get(&value, "git");
    assert_eq!(text(context, "context_head"), head.trim_end());
    assert_eq!(
        get(context, "indexed_paths"),
        &Value::Array(
            current
                .keys()
                .map(|name| s(name.to_str().unwrap()))
                .collect()
        )
    );
    assert_eq!(get(context, "not_indexed_paths"), &Value::Array(vec![]));
    assert_eq!(text(context, "current_bytes_committed"), "not-assessed");
    for flag in ["publication_verified", "freshness_verified"] {
        assert_eq!(get(get(&value, "sharing"), flag), &Value::Bool(false));
    }
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        String::from_utf8(git(&w, &repo, &["rev-parse", "HEAD"]).stdout).unwrap(),
        head
    );
    assert_eq!(
        fs::read_to_string(repo.join("scratch.txt")).unwrap(),
        "Preserve working copy\n"
    );
    assert_eq!(snapshot(&bank), current);
    assert!(
        fs::read_to_string(bank.join("progress.md"))
            .unwrap()
            .contains("[actual probe output](../evidence.txt)")
    );
    let clone = w.clone(&repo, "fresh-clone").unwrap();
    let fresh = inspect(&w, &clone, 0);
    assert_eq!(text(&fresh, "project_id"), PROJECT);
    assert_eq!(
        fs::read_to_string(clone.join("memory-bank/activeContext.md")).unwrap(),
        BODIES[2].1
    );
    assert!(fs::read_to_string(target).unwrap().contains("second clone"));
}

// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_missing_git_still_reads_the_copied_portable_markdown_bank
#[test]
#[ignore = "needs recovery_frontier registered"]
fn inspect_without_git_keeps_markdown_authoritative() {
    let mut w = World::new().unwrap();
    let (repo, _) = fixture(&w, false, false);
    let empty = w.inside("empty-bin").unwrap();
    fs::create_dir(&empty).unwrap();
    w.env.insert("PATH".into(), empty.into_os_string());
    let value = inspect(&w, &repo, 0);
    assert_eq!(text(get(&value, "git"), "status"), "unavailable");
    assert_eq!(text(&value, "canonical_source"), "Markdown");
    let names: BTreeSet<_> = get(&value, "source_hashes")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(names, BTreeSet::from(["MEMORY.md", "activeContext.md"]));
    assert_eq!(get(&value, "writes_performed"), &Value::Bool(false));
}

// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_git_root_spelling_preserves_trailing_spaces
#[test]
#[ignore = "needs recovery_frontier registered"]
fn inspect_preserves_git_root_trailing_spaces() {
    let w = World::new().unwrap();
    let (repo, _) = fixture(&w, true, false);
    let renamed = w.inside("repo with trailing space ").unwrap();
    fs::rename(repo, &renamed).unwrap();
    let value = inspect(&w, &renamed, 0);
    assert_eq!(text(get(&value, "git"), "status"), "checkout");
    assert_eq!(
        get(get(&value, "git"), "not_indexed_paths"),
        &Value::Array(vec![])
    );
}

// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_busy_or_interrupted_bank_is_not_repaired_by_state_loss
#[test]
#[ignore = "needs recovery_frontier registered"]
fn inspect_preserves_busy_or_interrupted_bank_after_state_loss() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, false, false);
    for name in ["state", "cache", "data"] {
        fs::remove_dir_all(w.root.join(name)).unwrap();
    }
    for name in [".agent-memory.lock", ".agent-memory-transaction.md"] {
        let marker = bank.join(name);
        fs::write(&marker, "Unknown recovery evidence; preserve it\n").unwrap();
        let before = snapshot(&bank);
        inspect(&w, &repo, 1);
        assert_eq!(snapshot(&bank), before);
        fs::remove_file(marker).unwrap();
    }
}

// obligation: tests/contracts/test_current_bank.py::CurrentBankContract.test_git_inspection_ignores_ambient_repo_redirection_and_does_not_execute_fsmonitor_or_write_trace
#[test]
#[ignore = "needs recovery_frontier registered"]
fn inspect_suppresses_ambient_git_redirection_and_fsmonitor_trace_writes() {
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
    git(
        &w,
        &repo,
        &["config", "core.fsmonitor", script.to_str().unwrap()],
    );
    let head = String::from_utf8(git(&w, &repo, &["rev-parse", "HEAD"]).stdout).unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let trace = w.inside("git-trace.json").unwrap();
    w.env
        .insert("GIT_DIR".into(), foreign.join(".git").into_os_string());
    w.env
        .insert("GIT_TRACE2_EVENT".into(), trace.clone().into_os_string());
    let value = inspect(&w, &repo, 0);
    assert_eq!(text(get(&value, "git"), "context_head"), head.trim_end());
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert!(!canary.exists());
    assert!(!trace.exists());
}

#[test]
#[ignore = "needs recovery_frontier and comparison_view registered"]
fn inspect_path_is_explicit_historical_orientation_and_refuses_unknown_contexts() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, false, true);
    support::identity_fixtures::bind(&w, &repo).unwrap();
    let before = snapshot(&bank);
    let value = receipt(
        run(
            &w,
            &[
                "inspect",
                "--repo",
                repo.to_str().unwrap(),
                "--path",
                "projectbrief.md",
            ],
        ),
        0,
    );
    assert_eq!(text(&value, "schema"), "agent-memory/context-inspection-v1");
    assert_eq!(text(&value, "inspection_class"), "historical-orientation");
    assert_eq!(text(&value, "body"), BODIES[0].1);
    for flag in [
        "assignment",
        "usable_as_current_guidance",
        "source_assertions_verified",
        "identity_authenticated",
        "publication_verified",
        "writes_performed",
    ] {
        assert_eq!(get(&value, flag), &Value::Bool(false));
    }
    assert_eq!(get(&value, "untrusted_data"), &Value::Bool(true));
    assert_eq!(get(&value, "trust"), &Value::Null);
    let output = receipt(
        run(
            &w,
            &[
                "inspect",
                "--repo",
                repo.to_str().unwrap(),
                "--path",
                "MEMORY.md",
            ],
        ),
        1,
    );
    assert_eq!(
        text(&output, "error"),
        "Explicit historical inspection selects an existing ordinary context file only"
    );
    assert_eq!(snapshot(&bank), before);
}

const DELETED: &str = "efc6f266-71fc-42a7-a482-086dcb268f80";

fn tombstone_metadata() -> Value {
    obj([
        ("id", s(DELETED)),
        ("scope", s(PROJECT)),
        ("deleted-at", s("2026-10-22T18:00:00.123456789012345Z")),
        (
            "deletion-authority",
            s("a15ad970-05f9-42bd-a8c8-26ca06b8c751"),
        ),
        ("reason-class", s("user-request")),
    ])
}

fn render_tombstone(w: &World, metadata: &Value, code: i32) -> Value {
    let input = save(w, "tombstone-input.json", metadata);
    receipt(
        run(
            w,
            &["tombstones", "render", "--input", input.to_str().unwrap()],
        ),
        code,
    )
}

// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_exact_five_field_format_roundtrips_without_personal_prose_or_generated_state
#[test]
fn tombstones_roundtrip_exact_safe_fields_without_generated_state() {
    let w = World::new().unwrap();
    let metadata = tombstone_metadata();
    let rendered = render_tombstone(&w, &metadata, 0);
    assert_eq!(get(&rendered, "metadata"), &metadata);
    assert_eq!(
        get(&rendered, "identity_authenticated"),
        &Value::Bool(false)
    );
    assert_eq!(get(&rendered, "writes_performed"), &Value::Bool(false));
    let content = concat!(
        "---\n",
        "id: \"efc6f266-71fc-42a7-a482-086dcb268f80\"\n",
        "scope: \"45aa2f8e-6218-4c08-9265-797a7c62e30e\"\n",
        "deleted-at: \"2026-10-22T18:00:00.123456789012345Z\"\n",
        "deletion-authority: \"a15ad970-05f9-42bd-a8c8-26ca06b8c751\"\n",
        "reason-class: \"user-request\"\n",
        "---\n",
    );
    assert_eq!(text(&rendered, "content"), content);
    let file = w.inside("tombstone.md").unwrap();
    fs::write(&file, content).unwrap();
    let checked = receipt(
        run(
            &w,
            &["tombstones", "validate", "--file", file.to_str().unwrap()],
        ),
        0,
    );
    assert_eq!(get(&checked, "metadata"), &metadata);
    assert_eq!(
        text(&checked, "path"),
        format!("history/tombstones/{DELETED}.md")
    );
    assert_eq!(get(&checked, "valid"), &Value::Bool(true));
    assert!(!w.root.join("config/agent-memory").exists());
    assert!(!w.root.join("state/agent-memory").exists());
}

// obligation: tests/contracts/test_tombstones.py::TombstonesContract.test_sensitive_extra_fields_bodies_names_and_fingerprints_refuse_without_echo_or_mutation
#[test]
fn tombstones_refuse_sensitive_inputs_without_echo_or_mutation() {
    let w = World::new().unwrap();
    let metadata = tombstone_metadata();
    for key in [
        "body",
        "source",
        "evidence",
        "hash",
        "content-sha256",
        "email",
        "deleted-path",
        "title",
        "DELETED_SECRET_CANARY",
    ] {
        let mut bad = metadata.clone();
        let Value::Object(fields) = &mut bad else {
            panic!("fixture object")
        };
        fields.push((key.into(), s("DELETED_SECRET_CANARY")));
        let value = render_tombstone(&w, &bad, 1);
        assert_eq!(
            text(&value, "error"),
            agent_memory::tombstones::INVALID_INPUT
        );
        assert!(
            !json::encode(&value, json::DEFAULT)
                .unwrap()
                .contains("DELETED_SECRET_CANARY")
        );
    }
    for key in [
        "id",
        "scope",
        "deletion-authority",
        "reason-class",
        "deleted-at",
    ] {
        let mut bad = metadata.clone();
        set(&mut bad, key, s("DELETED_SECRET_CANARY"));
        let value = render_tombstone(&w, &bad, 1);
        assert!(
            !json::encode(&value, json::DEFAULT)
                .unwrap()
                .contains("DELETED_SECRET_CANARY")
        );
    }
    let rendered = render_tombstone(&w, &metadata, 0);
    let file = w.inside("bad-tombstone.md").unwrap();
    let before = format!("{}DELETED_SECRET_CANARY\n", text(&rendered, "content"));
    fs::write(&file, &before).unwrap();
    let output = run(
        &w,
        &["tombstones", "validate", "--file", file.to_str().unwrap()],
    );
    let expected = b"{\"schema\": \"agent-memory/tombstone-result-v1\", \"operation\": \"validate\", \"valid\": false, \"writes_performed\": false, \"error\": \"Invalid tombstone, source identity, projection or canonical bank; preserve input for explicit inspection\"}\n";
    assert_eq!(output.stdout, expected);
    receipt(output, 1);
    assert_eq!(fs::read_to_string(file).unwrap(), before);
}

#[test]
fn tombstones_parser_help_argument_errors_and_input_refusals() {
    let w = World::new().unwrap();
    for (args, message) in [
        (
            vec!["tombstones"],
            "the following arguments are required: command",
        ),
        (
            vec!["tombstones", "render"],
            "the following arguments are required: --input",
        ),
        (
            vec!["tombstones", "validate"],
            "the following arguments are required: --file",
        ),
        (
            vec!["tombstones", "filter"],
            "the following arguments are required: --input",
        ),
        (
            vec!["tombstones", "filter", "--input", "x", "--repo"],
            "argument --repo: expected one argument",
        ),
        (
            vec!["tombstones", "render", "--input", "x", "--unknown"],
            "unrecognized arguments: --unknown",
        ),
    ] {
        let output = run(&w, &args);
        assert_eq!(output.status, Status::Code(2), "{output:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with("usage: am tombstones"));
        assert!(stderr.contains(message), "{stderr}");
    }
    for command in ["render", "validate", "filter"] {
        let help = run(&w, &["tombstones", command, "--help"]);
        assert_eq!(help.status, Status::Code(0));
        assert!(help.stderr.is_empty());
        assert!(
            String::from_utf8(help.stdout)
                .unwrap()
                .contains("show this help message and exit")
        );
        let flag = if command == "validate" {
            "--file"
        } else {
            "--input"
        };
        let refused = receipt(
            run(&w, &["tombstones", command, flag, "DELETED_SECRET_CANARY"]),
            1,
        );
        assert_eq!(
            text(&refused, "error"),
            agent_memory::tombstones::INVALID_INPUT
        );
    }
    let source = save(
        &w,
        "filter.json",
        &obj([
            ("project_id", s(PROJECT)),
            ("records", Value::Array(vec![])),
        ]),
    );
    let refused = receipt(
        run(
            &w,
            &["tombstones", "filter", "--input", source.to_str().unwrap()],
        ),
        1,
    );
    assert_eq!(text(&refused, "operation"), "filter");
    assert_eq!(
        text(&refused, "error"),
        agent_memory::tombstones::INVALID_INPUT
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
fn tombstones_filter_suppresses_deleted_content_and_refuses_duplicate_ids() {
    let w = World::new().unwrap();
    let (repo, bank) = fixture(&w, false, false);
    let rendered = render_tombstone(&w, &tombstone_metadata(), 0);
    let marker = bank.join(text(&rendered, "path"));
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, text(&rendered, "content")).unwrap();
    let router = bank.join("MEMORY.md");
    let row = format!(
        "| [{0}](<{0}>) | Deletion guard | repository | deleted | Before rebuilding projections |\n",
        text(&rendered, "path")
    );
    fs::write(
        &router,
        fs::read_to_string(&router).unwrap().replace(
            "\n## Omitted contexts\n",
            &format!("{row}\n## Omitted contexts\n"),
        ),
    )
    .unwrap();
    let deleted = obj([("id", s(DELETED)), ("body", s("DELETED_SECRET_CANARY"))]);
    let survivor = obj([
        ("id", s("7dce6bb4-48d6-4ad2-a4b8-57ea7d14e15a")),
        ("body", s("Useful surviving lesson")),
    ]);
    let mut projection = obj([
        ("project_id", s(PROJECT)),
        (
            "records",
            Value::Array(vec![deleted.clone(), survivor.clone()]),
        ),
    ]);
    let source = save(&w, "projection.json", &projection);
    let output = run(
        &w,
        &[
            "tombstones",
            "filter",
            "--repo",
            repo.to_str().unwrap(),
            "--input",
            source.to_str().unwrap(),
        ],
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("DELETED_SECRET_CANARY"));
    let guarded = receipt(output, 0);
    assert_eq!(get(&guarded, "records"), &Value::Array(vec![survivor]));
    assert_eq!(get(&guarded, "suppressed"), &Value::Int(1.into()));
    set(
        &mut projection,
        "records",
        Value::Array(vec![deleted.clone(), deleted]),
    );
    save(&w, "projection.json", &projection);
    let refused = receipt(
        run(
            &w,
            &[
                "tombstones",
                "filter",
                "--repo",
                repo.to_str().unwrap(),
                "--input",
                source.to_str().unwrap(),
            ],
        ),
        1,
    );
    assert_eq!(
        text(&refused, "error"),
        agent_memory::tombstones::INVALID_INPUT
    );
}
