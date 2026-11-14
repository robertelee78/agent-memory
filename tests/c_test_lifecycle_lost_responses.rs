mod support;
use agent_memory::codec::json::Value;
use support::World;
use support::distribution::Channel;
use support::lifecycle::*;

// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_recovery_response_is_lost_after_real_rollback_is_durable
#[test]
#[ignore = "needs recovery_frontier registered"]
fn recovery_response_loss_leaves_real_rollback_durable_and_repeatable() {
    let mut world = World::new().unwrap();
    let channel = Channel::new(&world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    let installed = channel.install(&world, "codex", "copy");
    let original = get(&installed, "receipt");
    let memory = world.paths["repo"].join("memory-bank");
    std::fs::create_dir(&memory).unwrap();
    std::fs::write(
        memory.join("lesson.md"),
        b"Preserve memory across lost responses.\n",
    )
    .unwrap();
    let before = snapshot(&memory);
    channel.fault(&mut world, "uninstall", "uninstall-after-activate-0");
    channel.fault(&mut world, "recover", "recover-response-lost");
    let committed = std::fs::read(state(&world).join("receipt.json")).unwrap();
    let journal = read(&state(&world).join("journal.json"));
    assert!(["complete", "rolled-back"].contains(&text(&journal, "status")));
    assert_eq!(snapshot(&memory), before);
    let repeated = channel.run(&world, "recover", &[], 0);
    assert_eq!(get(&repeated, "verified"), &Value::Bool(true));
    assert_eq!(text(&repeated, "status"), "rolled-back");
    assert_eq!(
        std::fs::read(state(&world).join("receipt.json")).unwrap(),
        committed
    );
    assert_eq!(read(&state(&world).join("receipt.json")), *original);
    assert_eq!(snapshot(&memory), before);
}
