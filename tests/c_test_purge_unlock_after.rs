mod support;

// obligation: tests/contracts/test_purge_unlock_after.py::PurgeUnlockAfterContract.test_after_refused_purge_unlock
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interruption_after_refused_purge_unlock_preserves_readable_memory() {
    support::purge_replay::unlock("after");
}
