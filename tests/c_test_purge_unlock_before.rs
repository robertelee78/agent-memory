mod support;
use agent_memory::codec::json::Value;
use support::World;
use support::lifecycle::*;

// obligation: tests/contracts/test_purge_unlock_before.py::PurgeUnlockBeforeContract.test_before_refused_purge_unlock
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interruption_before_refused_purge_unlock_releases_only_proved_ownership() {
    support::purge_replay::unlock("before");
}

// obligation: tests/contracts/test_purge_unlock_before.py::PurgeUnlockBeforeContract.test_live_foreign_or_malformed_lock_is_not_released
#[test]
#[ignore = "needs recovery_frontier registered"]
fn live_foreign_or_malformed_lock_is_never_released() {
    for case in ["live", "foreign-host", "malformed-owner"] {
        let world = World::new().unwrap();
        let memory = bank(&world, &world.paths["repo"]);
        let preview = run(
            &world,
            &[
                "purge-data",
                "--repo",
                world.paths["repo"].to_str().unwrap(),
                "--path",
                memory.to_str().unwrap(),
                "--check",
            ],
            0,
        );
        assert_ne!(
            text(get(&preview, "preview"), "confirmation"),
            "0".repeat(64)
        );
        let host = agent_memory::host::hostname().unwrap();
        let mut owner = Value::Object(vec![
            (
                "operation_id".into(),
                Value::String(agent_memory::host::uuid4().unwrap()),
            ),
            ("operation".into(), Value::String("bank-purge".into())),
            (
                "host".into(),
                Value::String(if case == "foreign-host" {
                    format!("{host}-different-host")
                } else {
                    host
                }),
            ),
            ("pid".into(), Value::Int(std::process::id().into())),
            (
                "start".into(),
                Value::String("2026-11-14T00:00:00+00:00".into()),
            ),
        ]);
        if case == "malformed-owner" {
            if let Value::Object(fields) = &mut owner {
                fields.retain(|(key, _)| key != "operation_id");
            }
        }
        save(&memory.join(".agent-memory.lock"), &owner);
        let before = snapshot(&memory);
        let refused = run(
            &world,
            &[
                "purge-data",
                "--repo",
                world.paths["repo"].to_str().unwrap(),
                "--path",
                memory.to_str().unwrap(),
                "--confirm",
                &"0".repeat(64),
            ],
            1,
        );
        assert_eq!(get(&refused, "verified"), &Value::Bool(false));
        assert_eq!(snapshot(&memory), before);
    }
}
