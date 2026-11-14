mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use support::cutover::*;

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_changed_source_refused_at_recheck
fn changed_canonical_source_refuses_at_observation_recheck() {
    let r = Recall::new();
    let request = r.request(
        &["projectbrief.md".into()],
        object(Vec::<(String, Value)>::new()),
    );
    let (child, release) = r.pause(&request, "recall-after-source-read");
    fs::write(r.bank.join("projectbrief.md"), "changed after observation").unwrap();
    fs::write(release, b"").unwrap();
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(1));
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert!(value.get("results").is_none());
    assert!(value.get("failure_code").is_none());
    assert_eq!(
        fs::read_to_string(r.bank.join("projectbrief.md")).unwrap(),
        "changed after observation"
    );
}
