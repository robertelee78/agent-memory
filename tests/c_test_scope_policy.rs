mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use support::cutover::*;
use support::{RunOptions, World};

#[test]
// obligation: tests/contracts/test_scope_policy.py::ScopePolicyContract.test_router_and_copied_portable_helper_share_the_same_scope_contract
fn router_and_relocated_binary_use_the_same_scope_contract() {
    let w = World::new().unwrap();
    let pack = w.inside("portable scope pack").unwrap();
    fs::create_dir(&pack).unwrap();
    let binary = pack.join("am");
    fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
    let request = object([
        ("bank_project_id", text(PROJECT)),
        ("project_id", text(PROJECT)),
        ("scope", text("branch:feature")),
        ("context", object([("branch", text("feature"))])),
    ]);
    let path = save(&w, &request, "scope-request.json");
    let output = w
        .run([
            binary.as_os_str().to_owned(),
            "--json".into(),
            "scope-policy".into(),
            "decide".into(),
            "--input".into(),
            path.into_os_string(),
        ])
        .unwrap();
    let result = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert!(yes(&result, "applicable"));
    let mut proposal = json::decode(
        include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
        Default::default(),
    )
    .unwrap();
    put(&mut proposal, "project-id", text(PROJECT));
    for scope in [
        "personal:me",
        "component:src/./api",
        "component:src//api",
        "component:C:drive",
    ] {
        put(
            proposal
                .get_mut("contexts")
                .unwrap()
                .get_mut("projectbrief.md")
                .unwrap(),
            "scope",
            text(scope),
        );
        let input = save(&w, &proposal, "router.json");
        let output = w
            .run_with(
                [
                    binary.as_os_str().to_owned(),
                    "--json".into(),
                    "bank".into(),
                    "render".into(),
                    "--input".into(),
                    input.into_os_string(),
                ],
                RunOptions {
                    check: false,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        assert_eq!(output.status, Status::Code(1), "{scope}");
    }
    let output = w
        .run([
            binary.into_os_string(),
            "--json".into(),
            "scope-policy".into(),
            "model".into(),
        ])
        .unwrap();
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    let model = get(&value, "model");
    assert_eq!(string(model, "default-scope"), "repository");
    assert_eq!(get(model, "personal-namespaces"), &Value::Bool(false));
    assert_eq!(
        string(model, "developer-role"),
        "provenance-and-clarification-only"
    );
}
