mod support;
use agent_memory::codec::json::Value;
use support::World;
use support::distribution::Channel;
use support::lifecycle::{get, snapshot};

// obligation: tests/contracts/test_update_parallel_inventory.py::ParallelUpdateInventory.test_bounded_pool_keeps_all_roots_and_propagates_failure
#[test]
fn sequential_native_inventory_keeps_every_root_and_propagates_real_failure() {
    let world = World::new().unwrap();
    let channel = Channel::new(&world);
    let roots = (0..8)
        .map(|index| {
            world
                .repository(format!("inventory-{index}"), false)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let args = roots
        .iter()
        .map(|root| root.to_str().unwrap())
        .collect::<Vec<_>>();
    let before = snapshot(&world.root);
    let result = channel.run(&world, "inventory", &args, 0);
    let observed = get(&result, "observed_repositories").as_array().unwrap();
    for root in &roots {
        assert!(observed.contains(&Value::String(root.to_string_lossy().into_owned())));
    }
    assert_eq!(get(&result, "banks"), &Value::Array(vec![]));
    assert_eq!(snapshot(&world.root), before);
    let root = &roots[3];
    world
        .git_with(
            ["config", "--local", "agent-memory.bankroot", "../outside"],
            support::RunOptions {
                cwd: Some(root),
                ..Default::default()
            },
        )
        .unwrap();
    let before = snapshot(&world.root);
    let denied = channel.run(&world, "inventory", &args, 1);
    assert!(!support::lifecycle::text(&denied, "error").is_empty());
    assert_eq!(snapshot(&world.root), before);
}
