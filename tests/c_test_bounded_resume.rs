mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use support::cutover::*;

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bounded_resume.py::BoundedResumeContract.test_router_then_exact_task_context_omits_unrelated_records_without_optional_stores
fn exact_task_resume_omits_unrelated_records_without_optional_stores() {
    let r = Recall::new();
    let path = r.add(
        "api",
        "# Retry limit\n\nKeep three attempts for issue-42 in the API.\n",
        &object([
            ("scope", text("component:src/api")),
            ("conditions", object([("task", text("issue-42"))])),
        ]),
    );
    r.add(
        "worker",
        "# Worker\n\nUNRELATED_WORKER_CONTEXT_MUST_NOT_LOAD.\n",
        &object([("scope", text("component:src/worker"))]),
    );
    r.add(
        "episodic-detail",
        "# Old probe\n\nUNREQUESTED_PROBE_MUST_NOT_LOAD.\n",
        &object([("kind", text("episodic-attempt"))]),
    );
    let before = r.code();
    let router = fs::read(r.bank.join("MEMORY.md")).unwrap();
    assert!(router.len() <= 16384);
    assert!(String::from_utf8_lossy(&router).contains("Before qualified recall"));
    for name in ["cache", "data"] {
        fs::remove_dir_all(&r.world.paths[name]).unwrap();
    }
    let request = r.request(
        &["activeContext.md".into(), path.clone()],
        object([("component", text("src/api")), ("task", text("issue-42"))]),
    );
    let (output, value) = r.retrieve(&request);
    assert_eq!(output.status, Status::Code(0), "{value:?}");
    let paths: std::collections::BTreeSet<_> = get(&value, "results")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| string(row, "path"))
        .collect();
    assert_eq!(paths, ["activeContext.md", path.as_str()].into());
    let encoded = json::encode(&value, json::DEFAULT).unwrap();
    assert!(!encoded.contains("UNRELATED_WORKER_CONTEXT_MUST_NOT_LOAD"));
    assert!(!encoded.contains("UNREQUESTED_PROBE_MUST_NOT_LOAD"));
    assert_eq!(r.code(), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bounded_resume.py::BoundedResumeContract.test_exact_resume_preserves_branch_unknown_validity_trust_and_kind_gates
fn exact_resume_preserves_all_policy_gates() {
    let r = Recall::new();
    let mut paths = Vec::new();
    for (name, updates) in [
        ("current", object(Vec::<(String, Value)>::new())),
        (
            "other-branch",
            object([("conditions", object([("branch", text("feature/not-here"))]))]),
        ),
        (
            "unknown-task",
            object([("conditions", object([("task", text("not-observed"))]))]),
        ),
        (
            "unknown-applicability",
            object([("applicability", text("unknown"))]),
        ),
        (
            "expired",
            object([("valid-until", text("2020-01-01T00:00:00Z"))]),
        ),
        ("untrusted", object([("trust", text("untrusted"))])),
        ("other-kind", object([("kind", text("episodic-attempt"))])),
    ] {
        paths.push(r.add(
            name,
            "# Scoped lesson\n\nResume only under its actual conditions.\n",
            &updates,
        ));
    }
    let (output, value) = r.retrieve(&r.request(&paths, object(Vec::<(String, Value)>::new())));
    assert_eq!(output.status, Status::Code(0), "{value:?}");
    let selected: Vec<_> = get(&value, "results")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| string(row, "path"))
        .collect();
    assert_eq!(selected, vec![paths[0].as_str()]);
    assert!(value.get("main_freshness_verified").is_none());
    assert!(!yes(&value, "publication_verified"));
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bounded_resume.py::BoundedResumeContract.test_ambiguous_task_keeps_only_router_and_prose_never_gains_typed_trust
fn ambiguous_task_does_not_load_details_or_certify_prose_trust() {
    let r = Recall::new();
    r.add(
        "task-specific",
        "# Retry lesson\n\nA verified retry probe needs its recorded conditions.\n",
        &object([("conditions", object([("task", text("other-session-task"))]))]),
    );
    assert!(fs::read(r.bank.join("MEMORY.md")).unwrap().len() <= 16384);
    let mut request = r.request(&[], object(Vec::<(String, Value)>::new()));
    put(&mut request, "kinds", Value::Array(vec![]));
    put(&mut request, "include_context", Value::Bool(false));
    let (output, empty) = r.retrieve(&request);
    assert_eq!(output.status, Status::Code(0));
    assert!(get(&empty, "results").as_array().unwrap().is_empty());
    let (output, result) = r.retrieve(&r.request(
        &["activeContext.md".into()],
        object(Vec::<(String, Value)>::new()),
    ));
    assert_eq!(output.status, Status::Code(0));
    let rows = get(&result, "results").as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(string(&rows[0], "path"), "activeContext.md");
    assert_eq!(get(&rows[0], "trust"), &Value::Null);
    assert_eq!(string(&rows[0], "selection_class"), "orientation-only");
}
