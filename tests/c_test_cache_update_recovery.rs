mod support;
use agent_memory::codec::json::Value;
use std::path::{Path, PathBuf};
use support::World;
use support::distribution::Channel;
use support::lifecycle::*;
fn setup(world: &World, mode_name: &str) -> (Channel, Value, PathBuf) {
    let channel = Channel::new(world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    let installed = channel.install(world, "codex", mode_name);
    let runtime = PathBuf::from(text(get(get(&installed, "receipt"), "source"), "path"));
    (channel, installed, runtime)
}
fn opaque(runtime: &Path, body: &[u8]) -> PathBuf {
    let directory = runtime.join("skills/agent-memory/scripts/agent_memory/__pycache__");
    std::fs::create_dir_all(&directory).unwrap();
    mode(&directory, 0o700);
    let path = directory.join("frontmatter.cpython-314.pyc");
    std::fs::write(&path, body).unwrap();
    mode(&path, 0o600);
    path
}
fn unchanged_except_owner(world: &World) -> std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)> {
    let mut value = snapshot(&world.root);
    value.retain(|path, _| !path.ends_with("owner.json") && !path.ends_with("lock"));
    value
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_older_signed_runtime_upgrades_preserving_all_opaque_bytes
#[test]
fn rust_signed_runtime_refuses_opaque_bytecode_without_qualification_or_execution() {
    let world = World::new().unwrap();
    let (channel, _, runtime) = setup(&world, "link");
    let cache = opaque(&runtime, b"\x00\xffarbitrary opaque bytes\n");
    channel.publish("0.0.16", &"b".repeat(40));
    let before = unchanged_except_owner(&world);
    let result = channel.run(&world, "update", &[], 1);
    assert_eq!(get(&result, "verified"), &Value::Bool(false));
    assert_eq!(unchanged_except_owner(&world), before);
    assert_eq!(
        std::fs::read(cache).unwrap(),
        b"\x00\xffarbitrary opaque bytes\n"
    );
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_copy_client_drift_does_not_receive_source_qualification
#[test]
fn copied_client_opaque_drift_never_receives_source_qualification() {
    let world = World::new().unwrap();
    let (channel, installed, runtime) = setup(&world, "copy");
    opaque(&runtime, b"opaque source evidence\n");
    let target = get(get(&installed, "receipt"), "targets")
        .as_array()
        .unwrap()
        .iter()
        .find(|row| text(row, "kind") == "copy")
        .unwrap();
    let target = PathBuf::from(text(target, "path"));
    let directory = target.join("scripts/agent_memory/__pycache__");
    std::fs::create_dir_all(&directory).unwrap();
    let cache = directory.join("frontmatter.cpython-314.pyc");
    std::fs::write(&cache, b"opaque copied target").unwrap();
    let before = unchanged_except_owner(&world);
    let result = channel.run(&world, "update", &[], 1);
    assert_eq!(get(&result, "verified"), &Value::Bool(false));
    assert_eq!(unchanged_except_owner(&world), before);
    assert_eq!(std::fs::read(cache).unwrap(), b"opaque copied target");
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_reachable_writable_cache_directory_remains_refused
#[test]
fn reachable_writable_opaque_cache_directory_remains_refused() {
    let world = World::new().unwrap();
    let (channel, _, runtime) = setup(&world, "link");
    let cache = opaque(&runtime, b"opaque untrusted evidence\n");
    mode(cache.parent().unwrap(), 0o775);
    for parent in cache
        .parent()
        .unwrap()
        .ancestors()
        .skip(1)
        .take_while(|path| path.starts_with(&world.root))
    {
        mode(parent, 0o755);
    }
    let before = unchanged_except_owner(&world);
    channel.run(&world, "install", &["codex", "link"], 1);
    assert_eq!(unchanged_except_owner(&world), before);
    assert_eq!(
        std::fs::read(cache).unwrap(),
        b"opaque untrusted evidence\n"
    );
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_interrupted_activation_preserves_dirty_tree_as_pending_then_trusted_retry
#[test]
fn opaque_bytecode_is_refused_before_activation_and_retained_on_trusted_retry() {
    let mut world = World::new().unwrap();
    let (channel, _, runtime) = setup(&world, "link");
    let cache = opaque(&runtime, b"opaque untrusted evidence\n");
    channel.publish("0.0.16", &"b".repeat(40));
    let before = unchanged_except_owner(&world);
    world.env.insert(
        "AGENT_MEMORY_FAULT_STAGE".into(),
        "update-after-activate-0".into(),
    );
    world
        .env
        .insert("AGENT_MEMORY_FAULT_MODE".into(), "exit".into());
    channel.run(&world, "update", &[], 1);
    world
        .env
        .remove(std::ffi::OsStr::new("AGENT_MEMORY_FAULT_STAGE"));
    world
        .env
        .remove(std::ffi::OsStr::new("AGENT_MEMORY_FAULT_MODE"));
    assert_eq!(unchanged_except_owner(&world), before);
    assert_eq!(
        std::fs::read(&cache).unwrap(),
        b"opaque untrusted evidence\n"
    );
    channel.run(&world, "install", &["codex", "link"], 1);
    assert_eq!(unchanged_except_owner(&world), before);
    assert_eq!(
        std::fs::read(cache).unwrap(),
        b"opaque untrusted evidence\n"
    );
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_concurrent_cache_writer_is_preserved_then_source_only_retry_converges
#[test]
fn changed_opaque_cache_bytes_are_preserved_and_never_qualified_by_retry() {
    let world = World::new().unwrap();
    let (channel, _, runtime) = setup(&world, "link");
    let cache = opaque(&runtime, b"opaque untrusted evidence\n");
    channel.run(&world, "update", &["--check"], 1);
    let changed = b"opaque concurrent-writer evidence after qualification\n";
    std::fs::write(&cache, changed).unwrap();
    let before = unchanged_except_owner(&world);
    channel.run(&world, "update", &[], 1);
    assert_eq!(unchanged_except_owner(&world), before);
    channel.run(&world, "install", &["codex", "link"], 1);
    assert_eq!(unchanged_except_owner(&world), before);
    assert_eq!(std::fs::read(cache).unwrap(), changed);
}
