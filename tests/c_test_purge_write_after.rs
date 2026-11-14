mod support;

// obligation: tests/contracts/test_purge_write_after.py::PurgeWriteContract.test_after_every_purge_write
#[test]
#[ignore = "needs recovery_frontier registered"]
fn after_every_actual_purge_write_recovers_with_the_same_confirmation() {
    support::purge_replay::replay("after");
}
