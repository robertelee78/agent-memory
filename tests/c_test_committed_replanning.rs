mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use support::World;
use support::cutover::*;

fn setup() -> World {
    let w = World::new().unwrap();
    let state = w.paths["state"].join("agent-memory/install");
    fs::create_dir_all(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let captured =
        include_str!("fixtures/v014-state/all-copy/state/agent-memory/install/receipt.json")
            .replace("@WORLD@", w.root.to_str().unwrap());
    let mut receipt = json::decode(&captured, Default::default()).unwrap();
    // Native receipts can retain an inert adopted managed-channel provenance;
    // update must still refuse before staging or consulting mutable target data.
    put(
        &mut receipt,
        "schema",
        text("agent-memory/install-receipt-v3"),
    );
    put(&mut receipt, "targets", Value::Array(vec![]));
    put(
        receipt.get_mut("source").unwrap(),
        "target",
        text(agent_memory::installation::TARGET),
    );
    let path = state.join("receipt.json");
    fs::write(&path, json::encode(&receipt, json::DEFAULT).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    w
}
fn refused_update(w: &World) {
    let before = physical(&w.root);
    for args in [
        vec!["--json", "update", "--check"],
        vec!["--json", "update"],
    ] {
        let output = w
            .am_with(
                args,
                support::RunOptions {
                    check: false,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(output.status, Status::Code(1));
        let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
        assert!(!yes(&value, "verified"));
        let encoded = String::from_utf8_lossy(&output.stdout);
        assert!(
            encoded.contains("re-bootstrap") || encoded.contains("bootstrap"),
            "{encoded}"
        );
        assert_eq!(physical(&w.root), before);
    }
    let output = w
        .am_with(
            [
                "install",
                "--remote",
                "/never-contact-this-channel",
                "--ref",
                "main",
                "--presets",
                "codex",
                "--check",
            ],
            support::RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(1));
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostic.contains("bootstrap") || diagnostic.contains("Managed"),
        "{diagnostic}"
    );
    assert_eq!(physical(&w.root), before);
}

#[test]
// obligation: tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_outside_edit_after_lost_commit_response_requires_explicit_replan
fn retired_managed_channel_never_replans_over_external_activation_edits() {
    let w = setup();
    let target = w.paths["codex"].join("AGENTS.md");
    fs::write(&target, "Keep this user text outside AM ownership.\n").unwrap();
    refused_update(&w);
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "Keep this user text outside AM ownership.\n"
    );
}

#[test]
// obligation: tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_uncommitted_or_conflicting_journals_never_authorize_readonly_plans
fn uncommitted_or_conflicting_journals_never_authorize_retired_channel_updates() {
    let w = setup();
    let journal = w.paths["state"].join("agent-memory/install/journal.json");
    for status in ["staging", "rollback-conflict", "activating", "complete"] {
        let value = object([
            ("schema", text("agent-memory/install-journal-v1")),
            ("status", text(status)),
            ("transaction", text("22222222-2222-4222-8222-222222222222")),
            ("actions", Value::Array(vec![])),
            ("receipt", object(Vec::<(String, Value)>::new())),
            ("receipt_identity", object([("hash", text("0".repeat(64)))])),
        ]);
        fs::write(&journal, json::encode(&value, json::DEFAULT).unwrap()).unwrap();
        fs::set_permissions(&journal, fs::Permissions::from_mode(0o600)).unwrap();
        refused_update(&w);
    }
}

#[test]
// obligation: tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_redirected_target_parent_refuses_at_preflight_without_writes
fn redirected_target_parent_never_authorizes_retired_channel_activation() {
    let w = setup();
    let retained = w.root.join("retained-codex");
    fs::rename(&w.paths["codex"], &retained).unwrap();
    symlink(&retained, &w.paths["codex"]).unwrap();
    refused_update(&w);
    assert!(
        fs::symlink_metadata(&w.paths["codex"])
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
// obligation: tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_committed_managed_index_refresh_verifies_without_changing_payload_or_receipt
fn retired_managed_index_refresh_preserves_payload_and_receipt_under_refusal() {
    let w = setup();
    let source = w.paths["data"].join("agent-memory/source");
    fs::create_dir_all(source.join(".git")).unwrap();
    fs::write(
        source.join(".git/index"),
        b"captured inert index bytes after benign refresh",
    )
    .unwrap();
    fs::write(source.join("SKILL.md"), b"retained managed payload\n").unwrap();
    let before = physical(&source);
    refused_update(&w);
    refused_update(&w);
    assert_eq!(physical(&source), before);
}
