mod support;
use agent_memory::bank_capacity;
use agent_memory::codec::json::Value;
use agent_memory::process::Status;
use std::fs;
use std::os::unix::fs::symlink;
use support::cutover::*;

fn oversized() -> String {
    format!("# BODY_SECRET_CANARY\n\n{}\n", "é".repeat(8190))
}
fn capacity(output: &agent_memory::process::Output, v: &Value, path: &str, observed: usize) {
    assert_eq!(output.status, Status::Code(1), "{v:?}");
    for (key, expected) in [
        ("failure_code", "bank-source-byte-capacity"),
        ("failure_phase", "bank-eligibility"),
        ("failure_component", "canonical-bank"),
        ("failure_path", path),
        ("capacity_measurement", "exact"),
        ("capacity_unit", "bytes"),
    ] {
        assert_eq!(string(v, key), expected);
    }
    assert_eq!(
        get(v, "capacity_limit").as_int().and_then(|n| n.as_u64()),
        Some(16384)
    );
    assert_eq!(
        get(v, "capacity_observed")
            .as_int()
            .and_then(|n| n.as_u64()),
        Some(observed as u64)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("BODY_SECRET_CANARY"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("BODY_SECRET_CANARY"));
}
fn large() -> Bank {
    let mut b = Bank::new();
    let additions: Vec<_> = (0..48)
        .map(|n| {
            (
                format!("topics/inventory-evidence-{n:03}.md"),
                "# Fixture\n\nA bounded record.\n".to_owned(),
            )
        })
        .collect();
    b.before = optional(&b.before, &additions);
    // Reuse the exact structural records rather than generating new identities.
    let same: Vec<_> = additions
        .iter()
        .map(|(p, _)| (p.clone(), string(&b.before, p).to_owned()))
        .collect();
    b.after = optional(&b.after, &same);
    write_files(&b.bank, &b.before);
    b.prepared = b.preview(&b.after);
    b.feature();
    b
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_render_and_single_preview_reject_complete_utf8_source
fn render_and_single_preview_reject_complete_utf8_source() {
    let b = Bank::new();
    b.feature();
    let before = physical(&b.repo);
    let body = oversized();
    assert!(body.chars().count() < 16384 && body.len() > 16384);
    let content = b.world.inside("large-content.md").unwrap();
    fs::write(&content, &body).unwrap();
    let (output, value) = command(
        &b.world,
        "am writes",
        &[
            "preview".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
            "--path".into(),
            "activeContext.md".into(),
            "--content".into(),
            content.into_os_string(),
        ],
    );
    capacity(&output, &value, "activeContext.md", body.len());
    let mut template = agent_memory::codec::json::decode(
        include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
        Default::default(),
    )
    .unwrap();
    put(&mut template, "project-id", text(PROJECT));
    put(
        template
            .get_mut("contexts")
            .unwrap()
            .get_mut("projectbrief.md")
            .unwrap(),
        "body",
        text(&body),
    );
    let (output, value) = command(
        &b.world,
        "am bank",
        &[
            "render".into(),
            "--input".into(),
            save(&b.world, &template, "large-render.json").into_os_string(),
        ],
    );
    capacity(&output, &value, "projectbrief.md", body.len());
    assert_eq!(physical(&b.repo), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_single_apply_revalidates_a_schema_valid_changed_request
fn single_apply_revalidates_a_schema_valid_changed_request() {
    let b = Bank::new();
    b.feature();
    let content = b.world.inside("content.md").unwrap();
    fs::write(&content, "# Focus\n\nNew focus.\n").unwrap();
    let mut p = good(
        &b.world,
        "am writes",
        &[
            "preview".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
            "--path".into(),
            "activeContext.md".into(),
            "--content".into(),
            content.into_os_string(),
        ],
    );
    put(p.get_mut("proposal").unwrap(), "content", text(oversized()));
    let before = physical(&b.repo);
    let (output, value) = command(
        &b.world,
        "am writes",
        &[
            "apply".into(),
            "--proposal".into(),
            save(&b.world, &p, "changed.json").into_os_string(),
        ],
    );
    capacity(&output, &value, "activeContext.md", oversized().len());
    assert_eq!(physical(&b.repo), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_multifile_preview_and_apply_reject_complete_oversized_map
fn multifile_preview_and_apply_reject_complete_oversized_map() {
    let b = Bank::new();
    b.feature();
    let before = physical(&b.repo);
    let mut desired = b.after.clone();
    put(&mut desired, "activeContext.md", text(oversized()));
    let (output, value) = command(
        &b.world,
        "am transaction",
        &[
            "preview".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
            "--files".into(),
            save(&b.world, &desired, "oversized-map.json").into_os_string(),
            "--operation".into(),
            "compact".into(),
        ],
    );
    capacity(&output, &value, "activeContext.md", oversized().len());
    assert_eq!(physical(&b.repo), before);
    let mut p = b.prepared.clone();
    put(p.get_mut("proposal").unwrap(), "files", desired);
    let (output, value) = b.apply(&p);
    capacity(&output, &value, "activeContext.md", oversized().len());
    assert_eq!(physical(&b.repo), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_multifile_capacity_includes_record_metadata
fn multifile_capacity_includes_record_metadata() {
    let b = Bank::new();
    b.feature();
    let mut desired = optional(
        &b.after,
        &[(
            "topics/metadata.md".into(),
            "# Short body\n\nUseful evidence.\n".into(),
        )],
    );
    let original = string(&desired, "topics/metadata.md");
    let body = original.replace(
        "Disposable structural fixture; not real admission evidence",
        &format!("BODY_SECRET_CANARY{}", "é".repeat(8200)),
    );
    assert!(body.len() > 16384);
    put(&mut desired, "topics/metadata.md", text(&body));
    let before = physical(&b.repo);
    let (output, value) = command(
        &b.world,
        "am transaction",
        &[
            "preview".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
            "--files".into(),
            save(&b.world, &desired, "metadata-map.json").into_os_string(),
            "--operation".into(),
            "compact".into(),
        ],
    );
    capacity(&output, &value, "topics/metadata.md", body.len());
    assert_eq!(physical(&b.repo), before);
}

#[test]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_aggregate_replacement_counts_the_complete_result_not_both_versions
fn aggregate_replacement_counts_only_the_complete_result() {
    // Use the real fixed Rust bound, not a second configurable capacity policy.
    let max = agent_memory::sharing_limits::MAX_BANK;
    let max_file = agent_memory::sharing_limits::MAX_FILE;
    let mut files = object(
        (0..max / max_file).map(|n| (format!("topics/{n:05}.md"), text("x".repeat(max_file)))),
    );
    bank_capacity::check_files(&files).unwrap();
    put(&mut files, "topics/00000.md", text("y".repeat(max_file)));
    bank_capacity::check_files(&files).unwrap();
    put(&mut files, "topics/overflow.md", text("é"));
    let error = bank_capacity::check_files(&files).unwrap_err();
    let detail = bank_capacity::details(&error);
    assert_eq!(
        string(&detail, "failure_code"),
        "bank-aggregate-byte-capacity"
    );
    assert_eq!(
        get(&detail, "capacity_limit")
            .as_int()
            .and_then(|n| n.as_u64()),
        Some(max as u64)
    );
    assert_eq!(
        get(&detail, "capacity_observed")
            .as_int()
            .and_then(|n| n.as_u64()),
        Some((max + 2) as u64)
    );
    assert_eq!(string(&detail, "capacity_measurement"), "exact");
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_transaction_preflight_refuses_symlink_inside_runtime_directory
fn transaction_preflight_refuses_symlink_inside_runtime_directory() {
    let b = Bank::new();
    b.feature();
    let runtime = b.bank.join(".agent-memory-private");
    fs::create_dir(&runtime).unwrap();
    let outside = b.world.inside("outside").unwrap();
    fs::write(&outside, b"preserve outside").unwrap();
    symlink(&outside, runtime.join("link")).unwrap();
    let before = physical(&b.repo);
    assert!(
        agent_memory::bank_transactions::preflight(&b.bank, get(&b.prepared, "proposal"), None)
            .is_err()
    );
    assert_eq!(physical(&b.repo), before);
    assert!(
        fs::symlink_metadata(runtime.join("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(outside).unwrap(), b"preserve outside");
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_transaction_preflight_refuses_incomplete_inventory_and_failed_target_stat
fn preflight_refuses_inaccessible_inventory_and_unclassifiable_target() {
    let b = Bank::new();
    b.feature();
    let hidden = b.bank.join("topics");
    fs::create_dir(&hidden).unwrap();
    fs::write(hidden.join("unobserved.bin"), b"preserve foreign occupant").unwrap();
    let before = physical(&b.repo);
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0)).unwrap();
    let refused =
        agent_memory::bank_transactions::preflight(&b.bank, get(&b.prepared, "proposal"), None)
            .is_err();
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(refused);
    assert_eq!(physical(&b.repo), before);
    let target = b.bank.join("activeContext.md");
    let retained = b.world.inside("retained-context").unwrap();
    fs::rename(&target, &retained).unwrap();
    fifo(&target);
    assert!(
        agent_memory::bank_transactions::preflight(&b.bank, get(&b.prepared, "proposal"), None)
            .is_err()
    );
    fs::remove_file(target).unwrap();
    fs::rename(retained, b.bank.join("activeContext.md")).unwrap();
    assert_eq!(physical(&b.repo), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_inventory_ordinary_apply_and_retry
fn large_inventory_ordinary_apply_and_exact_retry() {
    let b = large();
    let index = fs::read(b.repo.join(".git/index")).unwrap();
    let head = fs::read(b.repo.join(".git/HEAD")).unwrap();
    let (output, done) = b.apply(&b.prepared);
    assert_eq!(output.status, Status::Code(0), "{done:?}");
    assert!(yes(&done, "verified"));
    same_map(&canonical(&b.bank), &b.after);
    let (output, replay) = b.apply(&b.prepared);
    assert_eq!(output.status, Status::Code(0));
    assert!(yes(&replay, "already_applied"));
    assert!(!yes(&replay, "writes_performed"));
    same_map(&canonical(&b.bank), &b.after);
    assert_eq!(fs::read(b.repo.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(b.repo.join(".git/HEAD")).unwrap(), head);
    assert_eq!(
        fs::read(b.repo.join("feature.txt")).unwrap(),
        b"Unfinished feature must remain intact.\n"
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_inventory_transaction_and_exact_retry
fn large_inventory_transaction_finish_and_exact_retry() {
    let b = large();
    b.crash(&b.prepared, "transaction-after-marker");
    let marker = fs::read(b.bank.join(MARKER)).unwrap();
    assert!(marker.len() > 4096);
    let value = agent_memory::codec::json::decode_bytes(&marker, Default::default()).unwrap();
    assert_eq!(
        get(&value, "base_hashes").as_object().unwrap().len(),
        b.before.as_object().unwrap().len()
    );
    b.unlock();
    let (output, done) = b.recover(&b.recovery("finish"), &[]);
    assert_eq!(output.status, Status::Code(0), "{done:?}");
    assert!(yes(&done, "verified"));
    same_map(&canonical(&b.bank), &b.after);
    let (output, replay) = b.apply(&b.prepared);
    assert_eq!(output.status, Status::Code(0));
    assert!(!yes(&replay, "writes_performed"));
    same_map(&canonical(&b.bank), &b.after);
    assert!(!b.bank.join(MARKER).exists());
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_marker_rollback_and_unknown_marker_refusal
fn large_marker_rollback_and_unknown_marker_refusal() {
    let b = large();
    b.crash(&b.prepared, "transaction-after-write:activeContext.md");
    let marker = fs::read(b.bank.join(MARKER)).unwrap();
    assert!(marker.len() > 4096);
    let value = agent_memory::codec::json::decode_bytes(&marker, Default::default()).unwrap();
    assert_eq!(
        get(&value, "base_hashes").as_object().unwrap().len(),
        b.before.as_object().unwrap().len()
    );
    b.unlock();
    let saved = physical(&b.repo);
    let error = refused(
        &b.world,
        "am bank",
        &[
            "validate".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
        ],
    );
    assert!(error.get("failure_code").is_none());
    assert_eq!(physical(&b.repo), saved);
    let (output, done) = b.recover(&b.recovery("rollback"), &[]);
    assert_eq!(output.status, Status::Code(0));
    assert!(yes(&done, "verified"));
    same_map(&canonical(&b.bank), &b.before);
    let (output, retry) = b.apply(&b.prepared);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(string(&retry, "outcome"), "rolled-back");
    assert!(!yes(&retry, "writes_performed"));
    assert!(!b.bank.join(MARKER).exists());
    fs::write(b.bank.join(MARKER), marker).unwrap();
    let saved = physical(&b.repo);
    let error = refused(
        &b.world,
        "am bank",
        &[
            "validate".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
        ],
    );
    assert!(error.get("failure_code").is_none());
    assert_eq!(physical(&b.repo), saved);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_owned_marker_keeps_frontier_but_unknown_or_changed_bytes_refuse
fn large_owned_marker_keeps_frontier_but_changed_witness_refuses() {
    let b = large();
    // The same canonical frontier is captured by the actual scope context; no
    // substitute RecoveryFrontier provider is registered by this fixture.
    let before = good(
        &b.world,
        "am identity",
        &[
            "status".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
        ],
    );
    b.crash(&b.prepared, "transaction-after-marker");
    b.unlock();
    let marker = fs::read(b.bank.join(MARKER)).unwrap();
    agent_memory::bank_observation::markers(&b.bank, None, Some(&marker)).unwrap();
    for witness in [None, Some(&marker[..marker.len() - 1])] {
        assert!(agent_memory::bank_observation::markers(&b.bank, None, witness).is_err());
    }
    let mut changed = marker.clone();
    *changed.last_mut().unwrap() = b' ';
    assert!(agent_memory::bank_observation::markers(&b.bank, None, Some(&changed)).is_err());
    let sentinel = b.bank.join(".agent-memory-recovery.md");
    fs::write(&sentinel, &marker).unwrap();
    assert!(agent_memory::bank_observation::markers(&b.bank, None, Some(&marker)).is_err());
    fs::remove_file(sentinel).unwrap();
    assert!(yes(&before, "resolved"));
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_unknown_marker_is_refused_without_opening_its_body
fn unknown_fifo_marker_is_refused_without_opening_its_body() {
    let b = Bank::new();
    b.feature();
    let before = physical(&b.repo);
    let marker = b.bank.join(MARKER);
    fifo(&marker);
    let (output, error) = command(
        &b.world,
        "am bank",
        &[
            "validate".into(),
            "--repo".into(),
            b.repo.as_os_str().to_owned(),
        ],
    );
    assert_eq!(output.status, Status::Code(1));
    assert!(error.get("failure_code").is_none());
    use std::os::unix::fs::FileTypeExt;
    assert!(fs::symlink_metadata(&marker).unwrap().file_type().is_fifo());
    fs::remove_file(marker).unwrap();
    assert_eq!(physical(&b.repo), before);
}
