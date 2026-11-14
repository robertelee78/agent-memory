mod support;

// obligation: tests/contracts/test_purge_write_before.py::PurgeWriteContract.test_before_every_purge_write
#[test]
#[ignore = "needs recovery_frontier registered"]
fn before_every_actual_purge_write_recovers_with_the_same_confirmation() {
    support::purge_replay::replay("before");
}
