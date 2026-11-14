mod support;
use agent_memory::codec::json::Value;
use agent_memory::process::Status;
use std::fs::{File, FileTimes};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::World;
use support::lifecycle::*;

fn purge(world: &World, memory: &Path, options: &[&str], expected: i32) -> Value {
    let mut args = vec![
        "purge-data",
        "--repo",
        world.paths["repo"].to_str().unwrap(),
        "--path",
        memory.to_str().unwrap(),
    ];
    args.extend_from_slice(options);
    run(world, &args, expected)
}
fn token(world: &World, memory: &Path) -> String {
    text(
        get(&purge(world, memory, &["--check"], 0), "preview"),
        "confirmation",
    )
    .to_owned()
}
fn interrupt(world: &World, memory: &Path, token: &str, stage: &str) {
    let args = [
        "purge-data",
        "--repo",
        world.paths["repo"].to_str().unwrap(),
        "--path",
        memory.to_str().unwrap(),
        "--confirm",
        token,
    ];
    assert_eq!(fault(world, &args, stage, "exit").status, Status::Code(86));
}
fn stage(world: &World, token: &str) -> PathBuf {
    let record = read(&state(world).join("purges").join(format!("{token}.json")));
    PathBuf::from(text(&record, "stage"))
}
fn touch_metadata(path: &Path) {
    let metadata = std::fs::metadata(path).unwrap();
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            FileTimes::new()
                .set_accessed(metadata.accessed().unwrap())
                .set_modified(metadata.modified().unwrap() + Duration::from_secs(1)),
        )
        .unwrap();
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_changed_content_and_recreated_identical_bank_require_new_confirmation
#[test]
#[ignore = "needs recovery_frontier registered"]
fn content_change_or_recreated_bank_requires_fresh_confirmation() {
    for change in ["content", "new-inode"] {
        let world = World::new().unwrap();
        let memory = bank(&world, &world.paths["repo"]);
        let token = token(&world, &memory);
        if change == "content" {
            let path = memory.join("projectbrief.md");
            let mut bytes = std::fs::read(&path).unwrap();
            bytes.extend_from_slice(b"\nNew evidence.\n");
            std::fs::write(path, bytes).unwrap();
        } else {
            std::fs::rename(&memory, world.root.join("retained old bank")).unwrap();
            bank(&world, &world.paths["repo"]);
        }
        let before = snapshot(&memory);
        let denied = purge(&world, &memory, &["--confirm", &token], 1);
        assert_eq!(get(&denied, "verified"), &Value::Bool(false));
        assert_eq!(snapshot(&memory), before);
    }
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_same_inode_and_content_after_file_metadata_change_requires_fresh_consent
#[test]
#[ignore = "needs recovery_frontier registered"]
fn file_metadata_change_requires_fresh_consent() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    let path = memory.join("projectbrief.md");
    let bytes = std::fs::read(&path).unwrap();
    let original = std::fs::metadata(&path).unwrap();
    let token = token(&world, &memory);
    touch_metadata(&path);
    let changed = std::fs::metadata(&path).unwrap();
    assert_eq!(changed.ino(), original.ino());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_ne!(
        (changed.ctime(), changed.ctime_nsec()),
        (original.ctime(), original.ctime_nsec())
    );
    let before = snapshot(&memory);
    assert_eq!(
        get(
            &purge(&world, &memory, &["--confirm", &token], 1),
            "verified"
        ),
        &Value::Bool(false)
    );
    assert_eq!(snapshot(&memory), before);
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_ordinary_hardlink_aliases_finish_and_recover_the_unlink_bookkeeping_gap
#[test]
#[ignore = "needs recovery_frontier registered"]
fn hardlink_aliases_finish_and_recover_the_unlink_gap() {
    for point in [None, Some("purge-after-alias-unlink")] {
        let world = World::new().unwrap();
        let memory = bank(&world, &world.paths["repo"]);
        std::fs::hard_link(memory.join("projectbrief.md"), memory.join("alias.md")).unwrap();
        let token = token(&world, &memory);
        if let Some(point) = point {
            interrupt(&world, &memory, &token, point);
        }
        assert_eq!(
            get(
                &purge(&world, &memory, &["--confirm", &token], 0),
                "verified"
            ),
            &Value::Bool(true)
        );
        assert!(!memory.exists());
        assert_eq!(
            get(
                &purge(&world, &memory, &["--confirm", &token], 0),
                "changed"
            ),
            &Value::Bool(false)
        );
    }
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_metadata_changed_after_alias_unlink_gap_is_not_adopted_as_self_mutation
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_metadata_after_alias_unlink_is_not_adopted() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    std::fs::hard_link(memory.join("projectbrief.md"), memory.join("alias.md")).unwrap();
    let token = token(&world, &memory);
    interrupt(&world, &memory, &token, "purge-after-alias-unlink");
    let stage = stage(&world, &token);
    touch_metadata(&stage.join("projectbrief.md"));
    let before = snapshot(&stage);
    assert_eq!(
        get(
            &purge(&world, &memory, &["--confirm", &token], 1),
            "verified"
        ),
        &Value::Bool(false)
    );
    assert_eq!(snapshot(&stage), before);
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_retry_preserves_a_recreated_canonical_bank_and_purges_only_the_confirmed_quarantine
#[test]
#[ignore = "needs recovery_frontier registered"]
fn retry_preserves_recreated_canonical_bank() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    let token = token(&world, &memory);
    interrupt(&world, &memory, &token, "purge-after-detach");
    bank(&world, &world.paths["repo"]);
    std::fs::write(memory.join("projectbrief.md"), b"New bank must survive.\n").unwrap();
    let before = snapshot(&memory);
    let completed = purge(&world, &memory, &["--confirm", &token], 0);
    assert_eq!(get(&completed, "verified"), &Value::Bool(true));
    assert_eq!(
        get(&completed, "replacement_bank_present"),
        &Value::Bool(true)
    );
    assert_eq!(snapshot(&memory), before);
    assert!(!Path::new(text(get(&completed, "receipt"), "stage")).exists());
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_unapproved_quarantine_data_is_preserved_on_retry
#[test]
#[ignore = "needs recovery_frontier registered"]
fn unapproved_quarantine_additions_are_preserved() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    let token = token(&world, &memory);
    interrupt(&world, &memory, &token, "purge-after-detach");
    let stage = stage(&world, &token);
    std::fs::write(
        stage.join("unapproved.txt"),
        b"Do not delete this later addition.\n",
    )
    .unwrap();
    let before = snapshot(&stage);
    assert_eq!(
        get(
            &purge(&world, &memory, &["--confirm", &token], 1),
            "verified"
        ),
        &Value::Bool(false)
    );
    assert_eq!(snapshot(&stage), before);
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_foreign_bank_lock_record_is_preserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn foreign_bank_lock_is_preserved() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    let token = token(&world, &memory);
    std::fs::write(
        memory.join(".agent-memory.lock"),
        br#"{"operation":"bank-checkpoint","host":"other-writer","pid":1}"#,
    )
    .unwrap();
    let before = snapshot(&memory);
    purge(&world, &memory, &["--confirm", &token], 1);
    assert_eq!(snapshot(&memory), before);
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_pending_memory_transaction_marker_is_preserved
#[test]
#[ignore = "needs recovery_frontier registered"]
fn pending_bank_transaction_is_preserved() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    std::fs::write(
        memory.join(".agent-memory-transaction.md"),
        b"Incomplete transaction: preserve and recover.\n",
    )
    .unwrap();
    let before = snapshot(&world.root);
    purge(&world, &memory, &["--check"], 1);
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_live_bank_writer_blocks_purge_until_the_process_releases_its_lock
#[test]
#[ignore = "needs recovery_frontier registered"]
fn live_writer_blocks_purge_until_flock_is_released() {
    let world = World::new().unwrap();
    let memory = bank(&world, &world.paths["repo"]);
    let token = token(&world, &memory);
    let lock = memory.join(".agent-memory.lock");
    let file = File::options()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&lock)
        .unwrap();
    let record = format!(
        "{{\"operation\":\"bank-checkpoint\",\"pid\":{},\"host\":\"{}\"}}",
        std::process::id(),
        agent_memory::host::hostname().unwrap()
    );
    std::fs::write(&lock, record).unwrap();
    file.lock().unwrap();
    let before = snapshot(&memory);
    purge(&world, &memory, &["--confirm", &token], 1);
    assert_eq!(snapshot(&memory), before);
    std::fs::remove_file(lock).unwrap();
    drop(file);
    assert_eq!(
        get(
            &purge(&world, &memory, &["--confirm", &token], 0),
            "verified"
        ),
        &Value::Bool(true)
    );
}
