mod support;

// obligation: tests/contracts/test_lifecycle_cleanup_before.py::CleanupBeforeContract.test_before_error_cleanup_write
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interruption_before_actual_error_cleanup_recovers_and_retries() {
    support::cleanup_replay::replay("before");
}
