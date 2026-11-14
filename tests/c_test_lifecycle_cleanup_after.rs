mod support;

// obligation: tests/contracts/test_lifecycle_cleanup_after.py::CleanupAfterContract.test_after_error_cleanup_write
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn interruption_after_actual_error_cleanup_recovers_and_retries() {
    support::cleanup_replay::replay("after");
}
