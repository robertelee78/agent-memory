mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use std::path::PathBuf;
use support::cutover::*;

#[test]
fn cutover_dispatch_worker() {
    dispatch_worker();
}
fn request(r: &Recall, path: &str) -> Value {
    let mut request = r.request(&[path.to_owned()], object(Vec::<(String, Value)>::new()));
    put(
        &mut request,
        "query",
        text("How often may a temporary failure be attempted again?"),
    );
    put(
        &mut request,
        "kinds",
        Value::Array(vec![text("semantic-fact")]),
    );
    put(&mut request, "include_context", Value::Bool(false));
    put(
        &mut request,
        "discovery",
        discovery_trace(r, &[path.to_owned()], "recall"),
    );
    request
}
fn ready() -> (Recall, String, Value, PathBuf) {
    let r = Recall::new();
    let path = r.add(
        "attempt-policy",
        "# Attempt policy\n\nRepeat transient requests at most three times.\n",
        &object(Vec::<(String, Value)>::new()),
    );
    let request = request(&r, &path);
    let (output, first) = r.retrieve(&request);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(get(&first, "results").as_array().unwrap().len(), 1);
    assert!(!yes(&first, "optional_indexes_used"));
    assert!(yes(get(&first, "metrics"), "catalog_updated"));
    let state = r.world.paths["state"].clone();
    let pointer = physical(&state)
        .keys()
        .find(|p| p.ends_with("recall-catalog-v1/current.json"))
        .map(|p| state.join(p))
        .unwrap();
    (r, path, request, pointer)
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_ordinary_recall_preserves_optional_navigation_catalog_without_republishing_it
fn ordinary_recall_preserves_navigation_catalog_without_republication() {
    let (r, path, mut request, pointer) = ready();
    let bank = physical(&r.bank);
    let code = r.code();
    let directory = pointer.parent().unwrap();
    let before = physical(directory);
    let mtimes: Vec<_> = before
        .keys()
        .map(|p| {
            (
                p.clone(),
                fs::metadata(directory.join(p)).unwrap().modified().unwrap(),
            )
        })
        .collect();
    let Value::Object(fields) = &mut request else {
        panic!("request")
    };
    fields.retain(|(k, _)| k != "discovery");
    let (output, result) = r.retrieve(&request);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(
        string(&get(&result, "results").as_array().unwrap()[0], "path"),
        path
    );
    assert!(!yes(&result, "optional_indexes_used"));
    assert!(!yes(get(&result, "metrics"), "catalog_updated"));
    assert!(!yes(&result, "derived_cache_writes_performed"));
    assert_eq!(physical(directory), before);
    for (p, time) in mtimes {
        assert_eq!(
            fs::metadata(directory.join(p)).unwrap().modified().unwrap(),
            time
        );
    }
    assert_eq!(physical(&r.bank), bank);
    assert_eq!(r.code(), code);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_common_installed_cli_reuses_the_same_validated_catalog
fn common_cli_reuses_the_same_validated_catalog() {
    let (r, path, request, _) = ready();
    let before = physical(&r.bank);
    let code = r.code();
    // The dispatch worker uses the source checkout's pack as the installed
    // runtime pack, without fabricating an installation ownership receipt.
    for _ in 0..2 {
        let input = save(&r.world, &request, "catalog-request.json");
        let (exit, stdout, stderr) = dispatch(
            &r.world,
            &[
                "--json".into(),
                "search".into(),
                "--repo".into(),
                r.repo.as_os_str().to_owned(),
                "--request".into(),
                input.into_os_string(),
            ],
            None,
        );
        assert_eq!(exit, 0, "{stdout}\n{stderr}");
        let result = json::decode(&stdout, Default::default()).unwrap();
        assert!(yes(&result, "optional_indexes_used"));
        assert_eq!(
            string(&get(&result, "results").as_array().unwrap()[0], "path"),
            path
        );
    }
    assert_eq!(physical(&r.bank), before);
    assert_eq!(r.code(), code);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_warm_policy_matches_full_reader_for_hidden_unsafe_and_conditional_corrections
fn warm_catalog_policy_matches_full_reader_for_all_replacement_gates() {
    for (name, updates, old_remains) in [
        ("hidden", object([("visibility", text("hidden"))]), false),
        (
            "other-kind",
            object([("kind", text("project-decision"))]),
            false,
        ),
        ("unsafe", object(Vec::<(String, Value)>::new()), false),
        (
            "expired",
            object([("valid-until", text("2020-01-01T00:00:00Z"))]),
            true,
        ),
        ("untrusted", object([("trust", text("untrusted"))]), true),
        (
            "unknown",
            object([("conditions", object([("prerequisite", Value::Null)]))]),
            true,
        ),
        (
            "other-branch",
            object([(
                "conditions",
                object([("branch", text("feature/different"))]),
            )]),
            true,
        ),
    ] {
        let mut r = Recall::new();
        let old = r.add(
            &format!("old-{name}"),
            "# Retry lesson\n\nA verified retry probe needs its recorded conditions.\n",
            &object(Vec::<(String, Value)>::new()),
        );
        let (_, metadata, _) =
            agent_memory::records::parse(&fs::read_to_string(r.bank.join(&old)).unwrap(), &[])
                .unwrap();
        let mut updates = updates;
        put(
            &mut updates,
            "supersedes",
            Value::Array(vec![object([
                ("id", get(&metadata, "id").clone()),
                ("version", Value::Int(1.into())),
            ])]),
        );
        let body = if name == "unsafe" {
            "# Correction\n\npassword=WITHHELD_CORRECTION_BODY\n"
        } else {
            "# Correction\n\nAn independently qualified replacement.\n"
        };
        r.add(&format!("new-{name}"), body, &updates);
        let request = request(&r, &old);
        r.world
            .env
            .insert("AGENT_MEMORY_NO_RECALL_CATALOG".into(), "1".into());
        let (output, cold) = r.retrieve(&request);
        assert_eq!(output.status, Status::Code(0));
        r.world
            .env
            .remove(std::ffi::OsStr::new("AGENT_MEMORY_NO_RECALL_CATALOG"));
        assert_eq!(r.retrieve(&request).0.status, Status::Code(0));
        let (output, warm) = r.retrieve(&request);
        assert_eq!(output.status, Status::Code(0));
        assert!(yes(&warm, "optional_indexes_used"));
        assert_eq!(get(&warm, "results"), get(&cold, "results"));
        assert_eq!(
            !get(&warm, "results").as_array().unwrap().is_empty(),
            old_remains
        );
        assert!(
            !json::encode(&warm, json::DEFAULT)
                .unwrap()
                .contains("WITHHELD_CORRECTION_BODY")
        );
    }
}
