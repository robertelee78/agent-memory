use agent_memory::codec::json::{self, Value};
use agent_memory::sharing_frontier as frontier;
use std::collections::{BTreeMap, BTreeSet};
fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}
fn o<const N: usize>(v: [(&str, Value); N]) -> Value {
    Value::Object(v.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn a(v: &[&str]) -> Value {
    Value::Array(v.iter().map(|v| s(*v)).collect())
}
fn g<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap()
}
fn row(key: &str, path: &str, body: &str, framing: Value, metadata: Value) -> Value {
    o([
        ("key", s(key)),
        ("path", s(path)),
        ("logical_key", s(key)),
        ("body", s(body)),
        ("effective_framing", framing),
        ("metadata", metadata),
        ("witness_metadata", Value::Null),
    ])
}
fn selection(
    searches: &[&str],
    paths: &[&str],
    ids: &[&str],
    complete: bool,
    uncertain: bool,
) -> Value {
    o([
        ("searches", a(searches)),
        ("paths", a(paths)),
        ("ids", a(ids)),
        ("complete", Value::Bool(complete)),
        ("uncertain", Value::Bool(uncertain)),
    ])
}
fn request(selections: Value, dependencies: Value, groups: Value) -> Value {
    o([
        ("selections", selections),
        ("dependencies", dependencies),
        ("consolidation_sources", groups),
    ])
}
fn changed(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|v| (*v).into()).collect()
}
fn empty() -> Value {
    o([])
}
// obligation: tests/contracts/test_selector_local_reuse.py::SelectorLocalReuseContract.test_exact_path_components_and_query_reuse_are_independent_of_selectors
#[test]
fn test_exact_path_components_and_query_reuse_are_independent_of_selectors() {
    let id = "12345678-1234-4234-8234-123456789abc";
    let rows = BTreeMap::from([
        ("a".into(), row("a", "topics/a.md", "", a(&[]), Value::Null)),
        ("b".into(), row("b", "topics/b.md", "", a(&[]), Value::Null)),
        (
            "c".into(),
            row(
                "c",
                "topics/shared.md",
                "introduction",
                a(&[]),
                o([("id", s(id))]),
            ),
        ),
        (
            "d".into(),
            row(
                "d",
                "topics/shared.md",
                "strasse café prose",
                a(&[]),
                Value::Null,
            ),
        ),
        (
            "e".into(),
            row(
                "e",
                "topics/text.md",
                "Straße",
                Value::Array(vec![o([("scope", s("CAFÉ framing"))])]),
                Value::Null,
            ),
        ),
        (
            "f".into(),
            row("f", "topics/other.md", "", a(&[]), Value::Null),
        ),
    ]);
    let req = request(
        o([
            (
                "a",
                selection(
                    &["  STRASSE  café ", "nothing one"],
                    &["topics/shared.md"],
                    &[],
                    true,
                    false,
                ),
            ),
            (
                "b",
                selection(&["strasse café", "nothing two"], &[], &[id], false, false),
            ),
        ]),
        o([("topics/b.md", a(&["topics/other.md"]))]),
        Value::Array(vec![a(&["d", "e"])]),
    );
    let (chosen, entries, pairs, pending) =
        frontier::selected(&rows, &changed(&["a", "b", "f"]), &req).unwrap();
    assert_eq!(g(g(&entries, "a"), "matched"), &a(&["a", "c", "d", "e"]));
    assert_eq!(g(g(&entries, "b"), "matched"), &a(&["b", "c", "d", "e"]));
    assert_eq!(
        g(&entries, "f"),
        &o([("complete", Value::Bool(false)), ("matched", a(&["f"]))])
    );
    assert_eq!(
        chosen.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["a", "b", "c", "d", "e", "f"]
    );
    assert_eq!(
        pairs,
        Value::Array(vec![
            a(&["a", "c"]),
            a(&["a", "d"]),
            a(&["a", "e"]),
            a(&["b", "c"]),
            a(&["b", "d"]),
            a(&["b", "e"]),
            a(&["b", "f"]),
            a(&["d", "e"])
        ])
    );
    assert_eq!(pending, vec!["b", "f"]);
}
// obligation: tests/contracts/test_selector_local_reuse.py::SelectorLocalReuseContract.test_changed_body_and_framing_are_observed_on_the_next_call
#[test]
fn test_changed_body_and_framing_are_observed_on_the_next_call() {
    let mut rows = BTreeMap::from([
        ("a".into(), row("a", "topics/a.md", "", a(&[]), Value::Null)),
        (
            "b".into(),
            row(
                "b",
                "topics/b.md",
                "alpha",
                Value::Array(vec![o([("scope", s("violet"))])]),
                Value::Null,
            ),
        ),
    ]);
    let req = request(
        o([(
            "a",
            selection(&["alpha violet", "another wording"], &[], &[], true, false),
        )]),
        empty(),
        a(&[]),
    );
    assert_eq!(
        g(
            g(
                &frontier::selected(&rows, &changed(&["a"]), &req).unwrap().1,
                "a"
            ),
            "matched"
        ),
        &a(&["a", "b"])
    );
    rows.insert(
        "b".into(),
        row(
            "b",
            "topics/b.md",
            "beta",
            Value::Array(vec![o([("scope", s("amber"))])]),
            Value::Null,
        ),
    );
    assert_eq!(
        g(
            g(
                &frontier::selected(&rows, &changed(&["a"]), &req).unwrap().1,
                "a"
            ),
            "matched"
        ),
        &a(&["a"])
    );
    let req = request(
        o([(
            "a",
            selection(&["beta amber", "another wording"], &[], &[], true, false),
        )]),
        empty(),
        a(&[]),
    );
    assert_eq!(
        g(
            g(
                &frontier::selected(&rows, &changed(&["a"]), &req).unwrap().1,
                "a"
            ),
            "matched"
        ),
        &a(&["a", "b"])
    );
}
// obligation: tests/contracts/test_selector_local_reuse.py::SelectorLocalReuseContract.test_more_than_256_distinct_queries_still_match_without_truncation
#[test]
fn test_more_than_256_distinct_queries_still_match_without_truncation() {
    let mut rows = BTreeMap::new();
    let mut selections = Vec::new();
    let mut changes = BTreeSet::new();
    for n in 0..130 {
        let key = format!("{n:03}");
        rows.insert(
            key.clone(),
            row(&key, &format!("topics/p{n:03}.md"), "", a(&[]), Value::Null),
        );
        selections.push((
            key.clone(),
            selection(
                &[&format!("absent{n:03}"), &format!("marker{n:03}")],
                &[],
                &[],
                true,
                false,
            ),
        ));
        changes.insert(key);
    }
    rows.insert(
        "target".into(),
        row(
            "target",
            "topics/target.md",
            "marker129",
            a(&[]),
            Value::Null,
        ),
    );
    let (chosen, entries, pairs, pending) = frontier::selected(
        &rows,
        &changes,
        &request(Value::Object(selections), empty(), a(&[])),
    )
    .unwrap();
    assert_eq!(g(g(&entries, "129"), "matched"), &a(&["129", "target"]));
    assert_eq!(g(g(&entries, "128"), "matched"), &a(&["128"]));
    assert!(chosen.contains_key("target"));
    assert_eq!(pairs, Value::Array(vec![a(&["129", "target"])]));
    assert!(pending.is_empty());
}
// obligation: tests/contracts/test_selector_local_reuse.py::SelectorLocalReuseContract.test_validation_still_precedes_matching_and_pending_is_preserved
#[test]
fn test_validation_still_precedes_matching_and_pending_is_preserved() {
    let rows = BTreeMap::from([
        ("a".into(), row("a", "topics/a.md", "", a(&[]), Value::Null)),
        ("b".into(), row("b", "topics/b.md", "", a(&[]), Value::Null)),
    ]);
    for value in [
        selection(&["Same  words", "same words"], &[], &[], true, false),
        selection(&["one", "two"], &[], &["invalid"], true, false),
        selection(&["one", "two"], &["../bad"], &[], true, false),
    ] {
        assert!(
            frontier::selected(
                &rows,
                &changed(&["a"]),
                &request(o([("a", value)]), empty(), a(&[]))
            )
            .is_err()
        );
    }
    let result = frontier::selected(
        &rows,
        &changed(&["a", "b"]),
        &request(
            o([("a", selection(&["one", "two"], &[], &[], true, true))]),
            empty(),
            a(&[]),
        ),
    )
    .unwrap();
    assert_eq!(g(g(&result.1, "a"), "uncertain"), &Value::Bool(true));
    assert_eq!(result.3, vec!["b"]);
}
#[test]
fn frontier_request_pages_and_edge_bounds_are_explicit() {
    let req = frontier::request_value(None).unwrap();
    assert_eq!(g(&req, "schema"), &s(frontier::REQUEST));
    let mut malformed = req.clone();
    *malformed.get_mut("audit_paths").unwrap() = a(&["projectbrief.md", "projectbrief.md"]);
    assert!(frontier::request_value(Some(&malformed)).is_err());
    assert!(
        frontier::page_payloads(&empty(), &BTreeMap::new(), &a(&[]), &[])
            .unwrap()
            .is_empty()
    );
    let rows = BTreeMap::from([(
        "key".into(),
        o([("key", s("key")), ("body", s("x".repeat(70000)))]),
    )]);
    assert!(frontier::page_payloads(&empty(), &rows, &a(&[]), &[]).is_err());
    let rows = BTreeMap::from([(
        "key".into(),
        o([("key", s("key")), ("body", s("Source without secrets"))]),
    )]);
    let pages = frontier::page_payloads(&empty(), &rows, &a(&[]), &[]).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(g(&pages[0], "index"), &Value::Int(0.into()));
    assert!(
        json::encode(&pages[0], json::DEFAULT)
            .unwrap()
            .contains("Source without secrets")
    );
}

use agent_memory::semantic_sharing as semantic;
fn registered() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        agent_memory::cli::register_providers().unwrap();
        // dev-tools: the test binary is not an installed runtime.
        agent_memory::cli::use_development_pack(Some(
            agent_memory::fs::canonical_path(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
                true,
            )
            .unwrap(),
        ));
    });
}
fn files() -> Value {
    registered();
    let mut proposal = json::decode(
        include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
        json::DecodePolicy::default(),
    )
    .unwrap();
    *proposal.get_mut("project-id").unwrap() = s("8e0b66aa-2be5-4fa3-b47f-1245474aa7dd");
    g(
        &agent_memory::bank_contract::render(&proposal).unwrap(),
        "files",
    )
    .clone()
}
fn triple(base: Value, local: Value, landed: Value) -> (Value, Value) {
    let values = o([("base", base), ("local", local), ("landed", landed)]);
    let checked = Value::Object(
        values
            .as_object()
            .unwrap()
            .iter()
            .map(|(side, files)| {
                (
                    side.clone(),
                    agent_memory::sharing_recovery::bounded(files, true, true).unwrap(),
                )
            })
            .collect(),
    );
    (values, checked)
}
fn replace(files: &mut Value, path: &str, from: &str, to: &str) {
    *files.get_mut(path).unwrap() = s(g(files, path).as_str().unwrap().replace(from, to));
}
#[test]
fn exact_component_identity_ignores_observation_time_but_not_inherited_meaning() {
    let base = files();
    let mut local = base.clone();
    replace(
        &mut local,
        "MEMORY.md",
        "2026-10-22T17:00:00Z",
        "2026-10-24T00:00:00Z",
    );
    assert_ne!(g(&base, "MEMORY.md"), g(&local, "MEMORY.md"));
    let (values, checked) = triple(base.clone(), local.clone(), base.clone());
    let rows = frontier::components(&values, &checked).unwrap();
    assert!(
        frontier::changed_components(&rows, &BTreeSet::new())
            .unwrap()
            .is_empty()
    );
    replace(
        &mut local,
        "MEMORY.md",
        "\n## Routes\n",
        "\nA new project-wide meaning.\n\n## Routes\n",
    );
    let (values, checked) = triple(base.clone(), local, base);
    let rows = frontier::components(&values, &checked).unwrap();
    let changed = frontier::changed_components(&rows, &BTreeSet::new()).unwrap();
    assert!(
        rows.iter()
            .filter(|(_, row)| g(row, "path").as_str() == Some("projectbrief.md"))
            .all(|(key, _)| changed.contains(key))
    );
    let request = frontier::request_value(None).unwrap();
    let result = frontier::selected(&rows, &changed, &request).unwrap();
    assert_eq!(result.3.len(), changed.len());
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_same_file_nonoverlapping_compatible_changes_combine_without_landing_order
#[test]
fn test_same_file_nonoverlapping_compatible_changes_combine_without_landing_order() {
    let base =
        "# Project\n\n## Build\nUse the isolated build.\n\n## Review\nReview scoped sources.\n";
    let local = base.replace(
        "Use the isolated build.",
        "Use the verified isolated build.",
    );
    let landed = base.replace(
        "Review scoped sources.",
        "Review scoped sources and their evidence.",
    );
    let expected = "# Project\n\n## Build\nUse the verified isolated build.\n\n## Review\nReview scoped sources and their evidence.\n";
    assert_eq!(
        semantic::compatible_prose(base, &local, &landed).unwrap(),
        expected
    );
    assert_eq!(
        semantic::compatible_prose(base, &landed, &local).unwrap(),
        expected
    );
    assert!(semantic::compatible_prose("same\n", "yellow\n", "blue\n").is_err());
    assert!(semantic::compatible_prose("a\nb\n", "a\nx\nb\n", "a\ny\nb\n").is_err());
    assert_eq!(
        semantic::compatible_prose("a\nb\n", "a\nx\nb\n", "a\nx\nb\n").unwrap(),
        "a\nx\nb\n"
    );
}
// obligation: tests/contracts/test_semantic_introductions.py::SemanticIntroductionsContract.test_compatible_router_introduction_changes_preserve_both_meanings_and_routes
#[test]
fn test_compatible_router_introduction_changes_preserve_both_meanings_and_routes() {
    let mut base = files();
    replace(
        &mut base,
        "MEMORY.md",
        "\n## Routes\n",
        "\n## Build note\nUse the isolated directory.\n\n## Review note\nRead actual sources.\n\n## Routes\n",
    );
    let original = g(&base, "MEMORY.md").as_str().unwrap();
    let local = original.replace(
        "Use the isolated directory.",
        "Use the verified isolated directory.",
    );
    let landed = original.replace("Read actual sources.", "Read actual sources and evidence.");
    let intro = semantic::compatible_prose(
        semantic::introduction(original).unwrap(),
        semantic::introduction(&local).unwrap(),
        semantic::introduction(&landed).unwrap(),
    )
    .unwrap();
    let merged = semantic::replace_introduction(original, &intro).unwrap();
    assert!(merged.contains("verified isolated directory"));
    assert!(merged.contains("sources and evidence"));
    assert!(merged.contains("| [projectbrief.md]"));
    *base.get_mut("MEMORY.md").unwrap() = s(merged);
    agent_memory::bank_contract::validate_files(&base, None).unwrap();
}
fn scoped(conditions: Value, chains: Value) -> Value {
    o([
        (
            "metadata",
            o([
                ("conditions", conditions),
                ("scope", s("repository")),
                ("valid-from", Value::Null),
                ("valid-until", Value::Null),
            ]),
        ),
        ("scope_chains", chains),
    ])
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_disjoint_conditions_stay_distinct_unknown_conditions_cannot_claim_disjointness
#[test]
fn test_disjoint_conditions_stay_distinct_unknown_conditions_cannot_claim_disjointness() {
    let left = scoped(
        o([("branch", s("older"))]),
        o([("local", a(&["repository"]))]),
    );
    let right = scoped(
        o([("branch", s("main"))]),
        o([("landed", a(&["repository"]))]),
    );
    assert!(semantic::disjoint(&left, &right).unwrap());
    let unknown = scoped(
        o([("branch", Value::Null)]),
        o([("landed", a(&["repository"]))]),
    );
    assert!(!semantic::disjoint(&left, &unknown).unwrap());
    let component_a = scoped(
        o([("component", s("src/one"))]),
        o([("local", a(&["repository"]))]),
    );
    let component_b = scoped(
        o([("component", s("src/one/nested"))]),
        o([("landed", a(&["repository"]))]),
    );
    assert!(!semantic::disjoint(&component_a, &component_b).unwrap());
}
// obligation: tests/contracts/test_semantic_introductions.py::SemanticIntroductionsContract.test_identical_prose_routed_under_multiple_conditions_cannot_claim_only_one_disjoint_variant
#[test]
fn test_identical_prose_routed_under_multiple_conditions_cannot_claim_only_one_disjoint_variant() {
    let left = o([
        ("metadata", Value::Null),
        (
            "scope_chains",
            o([
                ("local", a(&["repository", "branch:older"])),
                ("landed", a(&["repository", "branch:main"])),
            ]),
        ),
    ]);
    let right = scoped(
        o([("branch", s("main"))]),
        o([("landed", a(&["repository"]))]),
    );
    assert!(!semantic::disjoint(&left, &right).unwrap());
}
// obligation: tests/contracts/test_semantic_introductions.py::SemanticIntroductionsContract.test_router_and_custom_index_claims_are_disclosed_and_isolated_without_losing_independent_routes
#[test]
fn test_router_and_custom_index_claims_are_disclosed_and_isolated_without_losing_independent_routes()
 {
    let base = files();
    let mut local = base.clone();
    replace(
        &mut local,
        "MEMORY.md",
        "\n## Routes\n",
        "\nThe project wall should be yellow throughout this repository.\n\n## Routes\n",
    );
    let (values, checked) = triple(base.clone(), local.clone(), base.clone());
    let rows = semantic::units(&values, &checked).unwrap();
    let yellow = rows
        .values()
        .filter(|row| {
            g(row, "body")
                .as_str()
                .unwrap()
                .contains("wall should be yellow")
        })
        .collect::<Vec<_>>();
    assert_eq!(yellow.len(), 1);
    assert_eq!(g(yellow[0], "component"), &s("introduction"));
    assert!(g(yellow[0], "container").get("routes").is_some());
    let held = BTreeSet::from([("MEMORY.md".into(), "introduction".into(), None)]);
    let masked = semantic::mask(&local, &base, &held).unwrap();
    assert!(
        !g(&masked, "MEMORY.md")
            .as_str()
            .unwrap()
            .contains("wall should be yellow")
    );
    assert_eq!(g(&masked, "projectbrief.md"), g(&local, "projectbrief.md"));
    assert!(
        g(&masked, "MEMORY.md")
            .as_str()
            .unwrap()
            .contains("projectbrief.md")
    );
}
// obligation: tests/contracts/test_semantic_introductions.py::SemanticIntroductionsContract.test_disputed_route_purpose_read_condition_and_omission_cannot_leak_into_independent_publication
#[test]
fn test_disputed_route_purpose_read_condition_and_omission_cannot_leak_into_independent_publication()
 {
    let base = files();
    let mut local = base.clone();
    let (metadata, mut routes, omitted) =
        agent_memory::bank_contract::parse_router(g(&local, "MEMORY.md").as_str().unwrap())
            .unwrap();
    let first = routes
        .iter_mut()
        .find(|route| g(route, "path").as_str() == Some("projectbrief.md"))
        .unwrap();
    *first.get_mut("purpose").unwrap() = s("The project wall should be yellow.");
    *local.get_mut("MEMORY.md").unwrap() =
        s(agent_memory::bank_contract::router_text(&metadata, &routes, &omitted).unwrap());
    let held = BTreeSet::from([
        (
            "MEMORY.md".into(),
            "route-prose".into(),
            Some("projectbrief.md".into()),
        ),
        ("projectbrief.md".into(), "body".into(), None),
    ]);
    let masked = semantic::mask(&local, &base, &held).unwrap();
    assert!(
        !g(&masked, "MEMORY.md")
            .as_str()
            .unwrap()
            .contains("wall should be yellow")
    );
    assert_eq!(g(&masked, "projectbrief.md"), g(&base, "projectbrief.md"));
    agent_memory::bank_contract::validate_files(&masked, None).unwrap();
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_missing_pair_changed_sources_and_tampered_output_proof_refuse
#[test]
fn test_missing_pair_changed_sources_and_tampered_output_proof_refuse() {
    let inspect = o([
        ("binding", empty()),
        ("required_pairs", Value::Array(vec![a(&["a", "b"])])),
    ]);
    let mut judgment = o([
        ("binding", empty()),
        ("pairs", a(&[])),
        ("dependencies", empty()),
        ("consolidations", a(&[])),
        ("rationale", s("Read all actual current sources.")),
        ("recorded_at", s("2026-01-01T00:00:00Z")),
    ]);
    assert!(semantic::reviewed_value(&judgment, &inspect).is_err());
    let decision = o([
        ("left", s("a")),
        ("right", s("b")),
        ("classification", s("related")),
        ("reason", s("Read distinct meanings.")),
    ]);
    *judgment.get_mut("pairs").unwrap() = Value::Array(vec![decision.clone()]);
    assert_eq!(
        semantic::reviewed_value(&judgment, &inspect).unwrap().len(),
        1
    );
    *judgment.get_mut("pairs").unwrap() = Value::Array(vec![decision.clone(), decision]);
    assert!(semantic::reviewed_value(&judgment, &inspect).is_err());
    assert!(
        semantic::check(
            std::path::Path::new("/"),
            &o([("schema", s("agent-memory/semantic-sharing-proof-v1"))]),
            None
        )
        .is_err()
    );
}
#[test]
fn terminal_relations_require_exact_retained_identity() {
    let left = o([
        ("format", s("tombstone")),
        ("metadata", Value::Null),
        ("witness_metadata", o([("id", s("one"))])),
    ]);
    let right = o([
        ("format", s("standalone-record")),
        ("metadata", o([("id", s("one"))])),
        ("witness_metadata", Value::Null),
    ]);
    assert!(frontier::terminal_relation(&left, &right));
    let other = o([
        ("format", s("standalone-record")),
        ("metadata", o([("id", s("two"))])),
        ("witness_metadata", Value::Null),
    ]);
    assert!(!frontier::terminal_relation(&left, &other));
}
// obligation: tests/contracts/test_checked_page_export.py::CheckedPageExportContract.test_multiple_pages_preserve_every_real_source
#[test]
fn test_multiple_pages_preserve_every_real_source() {
    let rows = (0..6)
        .map(|index| {
            let key = format!("key-{index}");
            (
                key.clone(),
                o([
                    ("key", s(key)),
                    ("path", s(format!("topics/export-{index}.md"))),
                    ("body", s("A durable independent observation. ".repeat(310))),
                ]),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let pages = frontier::page_payloads(&empty(), &rows, &a(&[]), &[]).unwrap();
    assert!(pages.len() >= 2);
    let disclosed = pages
        .iter()
        .flat_map(|page| g(page, "items").as_array().unwrap())
        .filter_map(|item| item.get("source"))
        .map(|row| g(row, "key").as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(disclosed, rows.keys().map(String::as_str).collect());
    for (index, page) in pages.iter().enumerate() {
        assert_eq!(g(page, "index"), &Value::Int(index.into()));
        assert!(
            agent_memory::developer_identity::encoded(page)
                .unwrap()
                .len()
                <= 65536
        );
    }
}
// obligation: tests/contracts/test_checked_page_export.py::CheckedPageExportContract.test_changed_state_or_io_failure_never_completes
#[test]
fn test_changed_state_or_io_failure_never_completes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = agent_memory::fs::canonical_path(&std::env::temp_dir(), true)
        .unwrap()
        .join(format!(
            "am-sharing-export-{}",
            agent_memory::host::uuid4().unwrap()
        ));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (directory, _) = agent_memory::private_workspace::checked_directory(&root).unwrap();
    let raw = b"actual checked page\n";
    agent_memory::sharing_page_export::write_checked(&directory, "0.json", raw).unwrap();
    let expected = agent_memory::developer_identity::digest(Some(raw)).unwrap();
    agent_memory::sharing_page_export::readback(&directory, "0.json", raw.len(), &expected)
        .unwrap();
    std::fs::write(root.join("0.json"), b"corrupt").unwrap();
    assert!(
        agent_memory::sharing_page_export::readback(&directory, "0.json", raw.len(), &expected)
            .is_err()
    );
    std::fs::remove_file(root.join("0.json")).unwrap();
    symlink("/usr/bin/true", root.join("0.json")).unwrap();
    assert!(
        agent_memory::sharing_page_export::readback(&directory, "0.json", raw.len(), &expected)
            .is_err()
    );
    assert!(!root.join("complete.json").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[allow(unused_imports)]
mod support;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use support::{RunOptions, World};
fn git(w: &World, repo: &Path, args: &[&str]) -> String {
    String::from_utf8(
        w.git_with(
            args,
            RunOptions {
                cwd: Some(repo),
                ..Default::default()
            },
        )
        .unwrap()
        .stdout,
    )
    .unwrap()
    .trim_end_matches('\n')
    .to_owned()
}
struct SharingFixture {
    w: World,
    repo: PathBuf,
    bank: PathBuf,
    peer: PathBuf,
}
impl SharingFixture {
    fn new() -> Self {
        registered();
        let mut w = World::new().unwrap();
        let runtime = support::installed_runtime::install(&w.root, |argv| {
            w.run_with(
                argv,
                RunOptions {
                    check: false,
                    timeout: std::time::Duration::from_secs(120),
                    ..Default::default()
                },
            )
            .unwrap()
        });
        w.env.insert(
            "AGENT_MEMORY_TEST_OWNED_COMMAND".into(),
            runtime.command.clone().into_os_string(),
        );
        let repo = w.repository("repo", false).unwrap();
        let remote = w.repository("remote.git", true).unwrap();
        git(
            &w,
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(
            &w,
            &repo,
            &["config", "--local", "agent-memory.authorityBranch", "main"],
        );
        let bank = repo.join("memory-bank");
        std::fs::create_dir(&bank).unwrap();
        for (path, body) in files().as_object().unwrap() {
            std::fs::write(bank.join(path), body.as_str().unwrap()).unwrap();
        }
        std::fs::write(repo.join("code.rs"), "const VALUE: u8 = 1;\n").unwrap();
        support::identity_fixtures::bind_with_command(&w, &repo, &runtime.command).unwrap();
        std::fs::write(
            bank.join("activeContext.md"),
            "# Current focus\n\nIndependent local orientation.\n",
        )
        .unwrap();
        let router = std::fs::read_to_string(bank.join("MEMORY.md")).unwrap().lines().filter(|line|!line.starts_with("- `activeContext.md`:")).map(|line|format!("{line}\n")).collect::<String>().replace("\n## Omitted contexts\n","| [activeContext.md](<activeContext.md>) | Session focus | repository | active | At a boundary |\n\n## Omitted contexts\n");
        std::fs::write(bank.join("MEMORY.md"), router).unwrap();
        git(&w, &repo, &["add", "."]);
        git(&w, &repo, &["commit", "-m", "Isolated shared bank"]);
        git(&w, &repo, &["push", "origin", "HEAD:main"]);
        std::fs::write(repo.join("code.rs"), "const VALUE: u8 = 2;\n").unwrap();
        git(&w, &repo, &["add", "code.rs"]);
        let peer = w.clone(&remote, "peer").unwrap();
        Self {
            w,
            repo,
            bank,
            peer,
        }
    }
    fn save(&self, value: &Value, name: &str) -> PathBuf {
        let path = self.w.inside(name).unwrap();
        std::fs::write(&path, json::encode(value, json::DEFAULT).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .unwrap();
        path
    }
    fn call(&self, operation: &str, extra: &[OsString]) -> (i32, Value) {
        let args = [
            self.w.env[std::ffi::OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
            "--json".into(),
            "semantic".into(),
            operation.into(),
            "--repo".into(),
            self.repo.as_os_str().to_owned(),
        ]
        .into_iter()
        .chain(extra.iter().cloned());
        let output = self
            .w
            .run_with(
                args,
                RunOptions {
                    check: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let agent_memory::process::Status::Code(code) = output.status else {
            panic!("native semantic command terminated: {:?}", output.status);
        };
        let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
        (code, value)
    }
    fn good(&self, operation: &str, extra: &[OsString]) -> Value {
        let (code, value) = self.call(operation, extra);
        assert_eq!(code, 0, "{value:?}");
        value
    }
    fn publish(&self) {
        git(&self.w, &self.peer, &["add", "memory-bank"]);
        git(
            &self.w,
            &self.peer,
            &["commit", "-m", "Independent source-bound learning"],
        );
        git(&self.w, &self.peer, &["push", "origin", "HEAD:main"]);
    }
    fn add(&self, bank: &Path, name: &str, body: &str) -> (Value, String) {
        let project = support::record_fixtures::project_id(
            &std::fs::read_to_string(bank.join("MEMORY.md")).unwrap(),
        )
        .unwrap();
        let content =
            support::record_fixtures::record_text(body, project.as_str().unwrap()).unwrap();
        let (_, meta, _) = agent_memory::records::parse(&content, &[]).unwrap();
        let path = format!("topics/{name}.md");
        std::fs::create_dir_all(bank.join("topics")).unwrap();
        std::fs::write(bank.join(&path), content).unwrap();
        let router = std::fs::read_to_string(bank.join("MEMORY.md")).unwrap();
        std::fs::write(bank.join("MEMORY.md"),router.replace("\n## Omitted contexts\n",&format!("| [{path}](<{path}>) | Scoped reviewed lesson | repository | active | When conditions match |\n\n## Omitted contexts\n"))).unwrap();
        (meta, path)
    }
    fn judgment(&self, view: &Value, classify: impl Fn(&Value, &Value) -> &'static str) -> Value {
        let rows = g(view, "sources")
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (g(row, "key").as_str().unwrap(), row))
            .collect::<BTreeMap<_, _>>();
        let pairs = g(view, "required_pairs")
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                let [left, right] = pair.as_array().unwrap() else {
                    panic!("required pair");
                };
                let left = left.as_str().unwrap();
                let right = right.as_str().unwrap();
                o([
                    ("left", s(left)),
                    ("right", s(right)),
                    ("classification", s(classify(rows[left], rows[right]))),
                    (
                        "reason",
                        s("Actual complete source meaning reviewed for this isolated case."),
                    ),
                ])
            })
            .collect();
        o([
            ("binding", g(view, "binding").clone()),
            ("pairs", Value::Array(pairs)),
            ("dependencies", empty()),
            ("consolidations", a(&[])),
            (
                "rationale",
                s("Preserve every actual source condition and origin."),
            ),
            (
                "recorded_at",
                s(agent_memory::host::utc_now_isoformat().unwrap()),
            ),
        ])
    }
    fn review(&self, value: &Value) -> Value {
        self.good(
            "review",
            &[
                "--input".into(),
                self.save(value, "judgment.json").into_os_string(),
            ],
        )
    }
    fn state(&self) -> (Vec<(PathBuf, Vec<u8>)>, String, String) {
        fn visit(root: &Path, rows: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in std::fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, rows);
                } else if path.file_name().unwrap() != ".agent-memory.lock" {
                    rows.push((path.clone(), std::fs::read(path).unwrap()));
                }
            }
        }
        let mut rows = Vec::new();
        visit(&self.bank, &mut rows);
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        (
            rows,
            git(&self.w, &self.repo, &["diff", "--cached", "--binary"]),
            git(&self.w, &self.repo, &["rev-parse", "HEAD"]),
        )
    }
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_textually_clean_cross_file_yellow_blue_conflict_shows_both_origins_and_keeps_independent_work
#[test]
fn test_textually_clean_cross_file_yellow_blue_conflict_shows_both_origins_and_keeps_independent_work()
 {
    let f = SharingFixture::new();
    let (_, yellow) = f.add(
        &f.bank,
        "wall-local",
        "# Wall\n\nThe same wall should be yellow.\n",
    );
    let (blue_meta, blue) = f.add(
        &f.peer.join("memory-bank"),
        "wall-peer",
        "# Wall\n\nThe same wall should be blue.\n",
    );
    std::fs::write(
        f.peer.join("memory-bank/projectbrief.md"),
        "# Project\n\nIndependent landed orientation.\n",
    )
    .unwrap();
    f.publish();
    let before = f.state();
    let view = f.good("inspect", &[]);
    let judgment = f.judgment(&view, |left, right| {
        if [
            g(left, "path").as_str().unwrap(),
            g(right, "path").as_str().unwrap(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
            == BTreeSet::from([yellow.as_str(), blue.as_str()])
        {
            "contradiction"
        } else {
            "related"
        }
    });
    let result = f.review(&judgment);
    assert_eq!(
        g(&result, "blocked_paths"),
        &a(&BTreeSet::from([yellow.as_str(), blue.as_str()])
            .into_iter()
            .collect::<Vec<_>>())
    );
    assert_eq!(g(&result, "conflicts").as_array().unwrap().len(), 1);
    assert!(
        g(&result, "other_developer_ids")
            .as_array()
            .unwrap()
            .contains(g(g(&blue_meta, "origin"), "developer-id"))
    );
    assert_eq!(g(&result, "messages_sent"), &Value::Bool(false));
    assert!(g(&result, "refresh_files").get(&yellow).is_some());
    assert!(g(&result, "refresh_files").get(&blue).is_none());
    assert!(g(&result, "publish_files").get(&blue).is_some());
    assert!(g(&result, "publish_files").get(&yellow).is_none());
    assert!(
        g(g(&result, "refresh_files"), "projectbrief.md")
            .as_str()
            .unwrap()
            .contains("Independent landed")
    );
    assert_eq!(before, f.state());
    let proof = f.save(g(&result, "proof"), "proof.json");
    let checked = f.good("check", &["--input".into(), proof.into_os_string()]);
    assert_eq!(
        g(&checked, "source_observation_current"),
        &Value::Bool(true)
    );
    assert_eq!(before, f.state());
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_only_explicit_dependent_updates_pause_with_conflict
#[test]
fn test_only_explicit_dependent_updates_pause_with_conflict() {
    let f = SharingFixture::new();
    let (_, left) = f.add(&f.bank, "wall", "# Wall\n\nPaint yellow.\n");
    let (_, right) = f.add(
        &f.peer.join("memory-bank"),
        "wall-peer",
        "# Wall\n\nPaint blue.\n",
    );
    f.publish();
    std::fs::write(
        f.bank.join("activeContext.md"),
        "# Focus\n\nDependent paint purchase.\n",
    )
    .unwrap();
    std::fs::write(
        f.bank.join("projectbrief.md"),
        "# Project\n\nIndependent architecture.\n",
    )
    .unwrap();
    let view = f.good("inspect", &[]);
    let mut judgment = f.judgment(&view, |a, b| {
        if [
            g(a, "path").as_str().unwrap(),
            g(b, "path").as_str().unwrap(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
            == BTreeSet::from([left.as_str(), right.as_str()])
        {
            "contradiction"
        } else {
            "related"
        }
    });
    *judgment.get_mut("dependencies").unwrap() = o([("activeContext.md", a(&[&left]))]);
    let before = f.state();
    let result = f.review(&judgment);
    assert!(
        g(&result, "blocked_paths")
            .as_array()
            .unwrap()
            .contains(&s("activeContext.md"))
    );
    assert!(
        !g(&result, "blocked_paths")
            .as_array()
            .unwrap()
            .contains(&s("projectbrief.md"))
    );
    assert!(
        g(g(&result, "publish_files"), "projectbrief.md")
            .as_str()
            .unwrap()
            .contains("Independent architecture")
    );
    assert!(
        !g(g(&result, "publish_files"), "activeContext.md")
            .as_str()
            .unwrap()
            .contains("Dependent paint")
    );
    assert_eq!(before, f.state());
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_differently_worded_duplicate_combines_stable_identity_all_evidence_and_origins
#[test]
fn test_differently_worded_duplicate_combines_stable_identity_all_evidence_and_origins() {
    let f = SharingFixture::new();
    let (first, left) = f.add(
        &f.bank,
        "retry-local",
        "# Retry cap\n\nRetry at most three times.\n",
    );
    let (second, right) = f.add(
        &f.peer.join("memory-bank"),
        "retry-peer",
        "# Attempt limit\n\nUse no more than three retries.\n",
    );
    f.publish();
    let view = f.good("inspect", &[]);
    let selected = g(&view, "sources")
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| [left.as_str(), right.as_str()].contains(&g(row, "path").as_str().unwrap()))
        .collect::<Vec<_>>();
    let primary = selected
        .iter()
        .find(|row| g(row, "path").as_str() == Some(&left))
        .unwrap();
    let mut judgment = f.judgment(&view, |a, b| {
        if [
            g(a, "path").as_str().unwrap(),
            g(b, "path").as_str().unwrap(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
            == BTreeSet::from([left.as_str(), right.as_str()])
        {
            "equivalent"
        } else {
            "related"
        }
    });
    *judgment.get_mut("consolidations").unwrap() = Value::Array(vec![o([
        (
            "sources",
            Value::Array(selected.iter().map(|row| g(row, "key").clone()).collect()),
        ),
        ("primary", g(primary, "key").clone()),
        ("body", s("# Retry cap\n\nRetry at most three times.\n")),
        (
            "reason",
            s("Both actual claims establish the same cap under identical conditions."),
        ),
    ])]);
    let before = f.state();
    let result = f.review(&judgment);
    for name in ["refresh_files", "publish_files"] {
        let files = g(&result, name);
        let (_, current, _) =
            agent_memory::records::parse(g(files, &left).as_str().unwrap(), &[]).unwrap();
        let (_, historical, _) =
            agent_memory::records::parse(g(files, &right).as_str().unwrap(), &[]).unwrap();
        assert_eq!(g(&current, "id"), g(&first, "id"));
        assert_eq!(g(&current, "version"), &Value::Int(2.into()));
        assert_eq!(g(&historical, "status"), &s("superseded"));
        assert_eq!(g(&historical, "origin"), g(&second, "origin"));
        assert!(
            files
                .get(&format!(
                    "history/versions/{}/v1.md",
                    g(&first, "id").as_str().unwrap()
                ))
                .is_some()
        );
    }
    assert_eq!(before, f.state());
}
// obligation: tests/contracts/test_checked_page_export.py::CheckedPageExportContract.test_empty_inspection_has_only_a_complete_marker
#[test]
fn test_empty_inspection_has_only_a_complete_marker() {
    let f = SharingFixture::new();
    let before = f.state();
    let output = f.w.inside("empty.json").unwrap();
    let pages = f.w.inside("empty-pages").unwrap();
    let receipt = f.good(
        "inspect-v2",
        &[
            "--output".into(),
            output.clone().into_os_string(),
            "--pages-dir".into(),
            pages.clone().into_os_string(),
        ],
    );
    assert_eq!(g(&receipt, "page_count"), &Value::Int(0.into()));
    let view = json::decode_bytes(
        &std::fs::read(output).unwrap(),
        json::DecodePolicy::default(),
    )
    .unwrap();
    assert_eq!(g(&view, "pages"), &a(&[]));
    assert_eq!(std::fs::read_dir(&pages).unwrap().count(), 1);
    assert!(pages.join("complete.json").is_file());
    assert_eq!(before, f.state());
}
// obligation: tests/contracts/test_checked_page_export.py::CheckedPageExportContract.test_bundle_matches_manifest_and_independent_public_pages
#[test]
fn test_bundle_matches_manifest_and_independent_public_pages() {
    use std::os::unix::fs::PermissionsExt;
    let f = SharingFixture::new();
    std::fs::write(
        f.bank.join("activeContext.md"),
        "# Focus\n\nIndependent checked export evidence.\n",
    )
    .unwrap();
    let before = f.state();
    let output = f.w.inside("inspection.json").unwrap();
    let pages = f.w.inside("pages").unwrap();
    let receipt = f.good(
        "inspect-v2",
        &[
            "--output".into(),
            output.clone().into_os_string(),
            "--pages-dir".into(),
            pages.clone().into_os_string(),
        ],
    );
    let raw = std::fs::read(&output).unwrap();
    let view = json::decode_bytes(&raw, json::DecodePolicy::default()).unwrap();
    let marker_raw = std::fs::read(pages.join("complete.json")).unwrap();
    let marker = json::decode_bytes(&marker_raw, json::DecodePolicy::default()).unwrap();
    assert_eq!(
        g(&receipt, "sha256").as_str(),
        agent_memory::developer_identity::digest(Some(&raw)).as_deref()
    );
    assert_eq!(
        g(&receipt, "page_export_sha256").as_str(),
        agent_memory::developer_identity::digest(Some(&marker_raw)).as_deref()
    );
    assert_eq!(g(&marker, "manifest_sha256"), g(&view, "manifest_sha256"));
    assert_eq!(
        std::fs::metadata(&pages).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&output).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    let mut bodies = String::new();
    for descriptor in g(&view, "pages").as_array().unwrap() {
        let index = g(descriptor, "index").as_int().unwrap().to_string();
        let raw = std::fs::read(pages.join(format!("{index}.json"))).unwrap();
        let public = f.good(
            "page-v2",
            &[
                "--input".into(),
                output.as_os_str().to_owned(),
                "--page".into(),
                index.into(),
            ],
        );
        assert_eq!(
            raw,
            agent_memory::developer_identity::encoded(&public).unwrap()
        );
        assert_eq!(
            g(descriptor, "sha256").as_str(),
            agent_memory::developer_identity::digest(Some(&raw)).as_deref()
        );
        for item in g(&public, "items").as_array().unwrap() {
            if let Some(row) = item.get("source") {
                bodies.push_str(g(row, "body").as_str().unwrap());
            }
        }
    }
    assert!(bodies.contains("Independent checked export evidence."));
    assert_eq!(before, f.state());
}
// obligation: tests/contracts/test_checked_page_export.py::CheckedPageExportContract.test_collisions_and_unsafe_paths_refuse_without_clobbering
#[test]
fn test_collisions_and_unsafe_paths_refuse_without_clobbering() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    registered();
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    let occupied = w.inside("occupied").unwrap();
    std::fs::create_dir(&occupied).unwrap();
    std::fs::set_permissions(&occupied, std::fs::Permissions::from_mode(0o700)).unwrap();
    let existing = w.inside("existing").unwrap();
    std::fs::write(&existing, b"keep").unwrap();
    let unsafe_dir = w.inside("unsafe").unwrap();
    std::fs::create_dir(&unsafe_dir).unwrap();
    std::fs::set_permissions(&unsafe_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = w.inside("link").unwrap();
    symlink(&occupied, &link).unwrap();
    let output = w.inside("new.json").unwrap();
    for (manifest, pages) in [
        (&existing, w.inside("new-pages").unwrap()),
        (&output, occupied.clone()),
        (&output, repo.join("pages")),
        (&output, unsafe_dir.join("pages")),
        (&output, link.join("pages")),
        (&output, PathBuf::from("relative-pages")),
        (&output, output.clone()),
    ] {
        let refusal =
            agent_memory::sharing_page_export::preflight(&repo, manifest, &pages).unwrap_err();
        assert_eq!(refusal.kind(), &agent_memory::error::Kind::PrivateOutput);
    }
    assert_eq!(std::fs::read(existing).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(occupied).unwrap().count(), 0);
}

// obligation: tests/contracts/test_selector_local_reuse.py::SelectorLocalReuseContract.test_uncached_and_bounded_reuse_preserve_framed_matches_and_edges
#[test]
fn test_uncached_and_bounded_reuse_preserve_framed_matches_and_edges() {
    let mut rows = BTreeMap::new();
    // Real per-source sizes, but a rendering inventory larger than the 24 MiB cache.
    for index in 0..1700 {
        let key = format!("filler-{index:04}");
        rows.insert(
            key.clone(),
            row(
                &key,
                &format!("topics/{key}.md"),
                &"Neutral observation. ".repeat(800),
                a(&[]),
                Value::Null,
            ),
        );
    }
    for key in ["a", "c"] {
        rows.insert(
            key.into(),
            row(key, &format!("topics/{key}.md"), "", a(&[]), Value::Null),
        );
    }
    rows.insert(
        "z-target".into(),
        row(
            "z-target",
            "topics/target.md",
            "Straße",
            Value::Array(vec![o([("scope", s("CAFÉ framing"))])]),
            Value::Null,
        ),
    );
    let request = request(
        o([
            (
                "a",
                selection(
                    &["strasse café", "unrelated phrase"],
                    &[],
                    &[],
                    false,
                    false,
                ),
            ),
            (
                "c",
                selection(&["strasse café", "other phrase"], &[], &[], true, false),
            ),
        ]),
        empty(),
        a(&[]),
    );
    let (chosen, entries, pairs, pending) =
        frontier::selected(&rows, &changed(&["a", "c"]), &request).unwrap();
    assert_eq!(
        chosen.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["a", "c", "z-target"]
    );
    assert_eq!(g(g(&entries, "a"), "matched"), &a(&["a", "z-target"]));
    assert_eq!(g(g(&entries, "c"), "matched"), &a(&["c", "z-target"]));
    assert_eq!(
        pairs,
        Value::Array(vec![a(&["a", "z-target"]), a(&["c", "z-target"])])
    );
    assert_eq!(pending, vec!["a"]);
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_new_disputed_context_keeps_omission_explanation_and_independent_plan_valid
#[test]
fn test_new_disputed_context_keeps_omission_explanation_and_independent_plan_valid() {
    let base = files();
    let mut local = base.clone();
    let Value::Object(rows) = &mut local else {
        panic!("files");
    };
    rows.push((
        "productContext.md".into(),
        s("# Product intent\n\nThe wall should be yellow.\n"),
    ));
    let (meta, mut routes, mut omitted) =
        agent_memory::bank_contract::parse_router(g(&local, "MEMORY.md").as_str().unwrap())
            .unwrap();
    let Value::Object(reasons) = &mut omitted else {
        panic!("omissions");
    };
    reasons.retain(|(name, _)| name != "productContext.md");
    routes.push(o([
        ("path", s("productContext.md")),
        ("purpose", s("Product intent")),
        ("scope", s("repository")),
        ("lifecycle", s("active")),
        ("read-when", s("Before painting")),
    ]));
    *local.get_mut("MEMORY.md").unwrap() =
        s(agent_memory::bank_contract::router_text(&meta, &routes, &omitted).unwrap());
    let masked = semantic::mask(
        &local,
        &base,
        &BTreeSet::from([("productContext.md".into(), "body".into(), None)]),
    )
    .unwrap();
    assert!(masked.get("productContext.md").is_none());
    assert!(
        g(&masked, "MEMORY.md")
            .as_str()
            .unwrap()
            .contains("- `productContext.md`: No additional product context has been supplied")
    );
    assert_eq!(g(&masked, "projectbrief.md"), g(&local, "projectbrief.md"));
    agent_memory::bank_contract::validate_files(&masked, None).unwrap();
}
#[test]
fn bounded_input_schema_dispatch_and_refusal_output_preserve_private_text() {
    let w = World::new().unwrap();
    let path = w.inside("input.json").unwrap();
    std::fs::write(
        &path,
        b"{\"schema\":\"agent-memory/semantic-sharing-proof-v2\"}",
    )
    .unwrap();
    assert!(semantic::read_input(&path, true).is_ok());
    assert!(semantic::read_input(&path, false).is_ok());
    std::fs::write(
        &path,
        b"{\"schema\":\"agent-memory/semantic-sharing-proof-v1\"}",
    )
    .unwrap();
    assert!(semantic::read_input(&path, true).is_err());
    assert!(semantic::read_input(&path, false).is_ok());
    std::fs::write(&path, vec![b' '; semantic::LIMIT + 1]).unwrap();
    assert!(semantic::read_input(&path, false).is_err());
    let result = semantic::command_result(
        "review",
        Err(agent_memory::error::ContractError::new(
            "PRIVATE_SOURCE_MARKER",
        )),
    );
    assert_eq!(result.exit_code, 1);
    assert!(!result.stdout.contains("PRIVATE_SOURCE_MARKER"));
    assert!(result.stdout.contains("semantic-review-unverified"));
}

fn render_record(metadata: &Value, body: &str) -> String {
    let mut content =
        agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap();
    content.push_str(body);
    content
}
fn add_file(files: &mut Value, path: &str, content: String, metadata: &Value, history: bool) {
    let Value::Object(rows) = files else {
        panic!("files");
    };
    rows.push((path.into(), s(content)));
    let (meta, mut routes, omitted) =
        agent_memory::bank_contract::parse_router(g(files, "MEMORY.md").as_str().unwrap()).unwrap();
    routes.push(
        agent_memory::consolidation::route(path, "Retained source fixture", metadata, history)
            .unwrap(),
    );
    *files.get_mut("MEMORY.md").unwrap() =
        s(agent_memory::bank_contract::router_text(&meta, &routes, &omitted).unwrap());
}
// obligation: tests/contracts/test_semantic_introductions.py::SemanticIntroductionsContract.test_disputed_invalidation_caption_keeps_erasure_and_revocation_guards
#[test]
fn test_disputed_invalidation_caption_keeps_erasure_and_revocation_guards() {
    let mut base = files();
    let id = "12345678-1234-4234-8234-123456789abc";
    let content = support::record_fixtures::record_text_with_ids(
        "# Obsolete note\n\nExplicitly invalidated observation.\n",
        "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd",
        support::record_fixtures::RecordOptions {
            status: "revoked",
            ..Default::default()
        },
        id,
        "357b8b24-d50d-464e-bda6-a0bd9c8518ba",
    )
    .unwrap();
    let (_, metadata, _) = agent_memory::records::parse(&content, &[]).unwrap();
    add_file(
        &mut base,
        "topics/obsolete.md",
        content.clone(),
        &metadata,
        false,
    );
    let mut local = base.clone();
    replace(
        &mut local,
        "MEMORY.md",
        "Retained source fixture",
        "The project wall should be yellow",
    );
    let held = BTreeSet::from([
        (
            "MEMORY.md".into(),
            "route-prose".into(),
            Some("topics/obsolete.md".into()),
        ),
        ("topics/obsolete.md".into(), "body".into(), None),
    ]);
    let masked = semantic::mask(&local, &base, &held).unwrap();
    assert_eq!(g(&masked, "topics/obsolete.md"), &s(&content));
    assert!(
        !g(&masked, "MEMORY.md")
            .as_str()
            .unwrap()
            .contains("wall should be yellow")
    );
    let (values, _) = triple(base.clone(), local, base.clone());
    semantic::preserve_invalidation(&values, &masked).unwrap();
    let mut active = metadata.clone();
    *active.get_mut("status").unwrap() = s("active");
    let mut resurrected = base;
    *resurrected.get_mut("topics/obsolete.md").unwrap() = s(render_record(
        &active,
        "# Obsolete note\n\nExplicitly invalidated observation.\n",
    ));
    replace(&mut resurrected, "MEMORY.md", "| revoked |", "| active |");
    assert!(semantic::preserve_invalidation(&values, &resurrected).is_err());
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_hidden_other_kind_superseder_still_prevents_duplicate_reactivation
#[test]
fn test_hidden_other_kind_superseder_still_prevents_duplicate_reactivation() {
    let f = SharingFixture::new();
    let (first, left) = f.add(
        &f.bank,
        "old-cap",
        "# Retry cap\n\nUse at most three retries.\n",
    );
    let (_, right) = f.add(
        &f.peer.join("memory-bank"),
        "same-cap",
        "# Same cap\n\nThree retries are the limit.\n",
    );
    let (mut invalidator, path) = f.add(
        &f.peer.join("memory-bank"),
        "withdrawal",
        "# Withdrawal\n\nThis exact old cap was invalidated.\n",
    );
    *invalidator.get_mut("kind").unwrap() = s("procedural-candidate");
    *invalidator.get_mut("visibility").unwrap() = s("hidden");
    *invalidator.get_mut("supersedes").unwrap() = Value::Array(vec![o([
        ("id", g(&first, "id").clone()),
        ("version", g(&first, "version").clone()),
    ])]);
    std::fs::write(
        f.peer.join("memory-bank").join(path),
        render_record(
            &invalidator,
            "# Withdrawal\n\nThis exact old cap was invalidated.\n",
        ),
    )
    .unwrap();
    f.publish();
    let view = f.good("inspect", &[]);
    let selected = g(&view, "sources")
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| [left.as_str(), right.as_str()].contains(&g(row, "path").as_str().unwrap()))
        .collect::<Vec<_>>();
    let primary = selected
        .iter()
        .find(|row| g(row, "path").as_str() == Some(&left))
        .unwrap();
    let mut judgment = f.judgment(&view, |a, b| {
        if [
            g(a, "path").as_str().unwrap(),
            g(b, "path").as_str().unwrap(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
            == BTreeSet::from([left.as_str(), right.as_str()])
        {
            "equivalent"
        } else {
            "related"
        }
    });
    *judgment.get_mut("consolidations").unwrap() = Value::Array(vec![o([
        (
            "sources",
            Value::Array(selected.iter().map(|row| g(row, "key").clone()).collect()),
        ),
        ("primary", g(primary, "key").clone()),
        ("body", s("# Retry cap\n\nUse at most three retries.\n")),
        (
            "reason",
            s("Read both exact claims without reviving an invalidated source."),
        ),
    ])]);
    let before = f.state();
    let (code, _) = f.call(
        "review",
        &[
            "--input".into(),
            f.save(&judgment, "judgment.json").into_os_string(),
        ],
    );
    assert_eq!(code, 1);
    assert_eq!(before, f.state());
}
// obligation: tests/contracts/test_semantic_sharing.py::SemanticSharingContract.test_explicit_clarifying_decision_corrects_then_consolidates_with_both_origins_and_history
#[test]
fn test_explicit_clarifying_decision_corrects_then_consolidates_with_both_origins_and_history() {
    let f = SharingFixture::new();
    let (old, left) = f.add(
        &f.bank,
        "wall-local",
        "# Wall\n\nThe wall should be yellow.\n",
    );
    let (landed, right) = f.add(
        &f.peer.join("memory-bank"),
        "wall-landed",
        "# Wall\n\nThe wall should be blue.\n",
    );
    f.publish();
    let native = |args: &[OsString]| {
        let output =
            f.w.run_with(
                [
                    f.w.env[std::ffi::OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
                    "--json".into(),
                ]
                .into_iter()
                .chain(args.iter().cloned()),
                RunOptions {
                    check: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{value:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        value
    };
    let scope = native(&[
        "identity".into(),
        "status".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
    ]);
    let mut metadata = old.clone();
    *metadata.get_mut("status").unwrap() = s("candidate");
    *metadata.get_mut("evidence").unwrap() = a(&["user:explicit-wall-blue-clarification"]);
    *metadata.get_mut("source").unwrap() = o([
        ("kind", s("user")),
        ("actor", s("Explicit fixture source; not authenticated")),
    ]);
    *metadata
        .get_mut("origin")
        .unwrap()
        .get_mut("developer-id")
        .unwrap() = g(&scope, "developer_id").clone();
    let candidate = o([
        ("metadata", metadata),
        ("body", s("# Wall\n\nThe wall should be blue.\n")),
    ]);
    let request = o([
        ("project_id", g(&scope, "project_id").clone()),
        ("candidate", candidate),
        ("source_class", s("user")),
        ("repository_evidence", Value::Null),
        (
            "assessment",
            o([
                ("category", s("user-correction")),
                (
                    "utility",
                    s("Prevent repeated confusion about this explicitly clarified wall"),
                ),
                ("durable", Value::Bool(true)),
                ("evidence_confidence", s("confirmed")),
                (
                    "evidence_reason",
                    s("The current detecting developer explicitly clarified the correction"),
                ),
                ("confirmed", Value::Bool(true)),
                ("source_redundant", Value::Bool(false)),
                ("work_state_copy", Value::Bool(false)),
                ("review", Value::Null),
            ]),
        ),
    ]);
    let path = f.save(&request, "clarification-capture.json");
    let captured = native(&[
        "capture".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--request".into(),
        path.into_os_string(),
    ]);
    let capture = g(&captured, "capture");
    let compared = o([
        ("project_id", g(&scope, "project_id").clone()),
        ("candidate", g(g(capture, "request"), "candidate").clone()),
        (
            "searches",
            a(&["wall color", "explicit wall clarification"]),
        ),
        ("read_paths", a(&[])),
    ]);
    let path = f.save(&compared, "clarification-comparison.json");
    let view = native(&[
        "compare".into(),
        "inspect".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--request".into(),
        path.as_os_str().to_owned(),
    ]);
    let judgment = o([
        ("observation",g(&view,"observation").clone()),
        ("decisions",Value::Array(g(&view,"matches").as_array().unwrap().iter().map(|row|o([("path",g(row,"path").clone()),("classification",s("contradiction")),("rationale",s("The explicit current developer clarification contradicts the prior yellow claim"))])).collect())),
        ("pending_decisions",a(&[])),("rationale",s("Compared full prior claims with the actual detecting-developer decision")),
    ]);
    let judged = f.save(&judgment, "clarification-judgment.json");
    let proof = native(&[
        "compare".into(),
        "review".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--request".into(),
        path.into_os_string(),
        "--judgment".into(),
        judged.into_os_string(),
    ]);
    let files = agent_memory::bank_transactions::files_at(&f.bank).unwrap();
    let hashes = agent_memory::bank_transactions::file_hashes(&files).unwrap();
    let context_review = Value::Object(
        hashes
            .as_object()
            .unwrap()
            .iter()
            .filter(|(p, _)| !p.contains('/'))
            .cloned()
            .collect(),
    );
    let request = o([
        ("project_id", g(&scope, "project_id").clone()),
        ("operation", s("correct")),
        (
            "target",
            o([
                ("path", s(&left)),
                ("id", g(&old, "id").clone()),
                ("version", g(&old, "version").clone()),
                ("sha256", g(&hashes, &left).clone()),
            ]),
        ),
        (
            "authority",
            o([
                ("source_class", s("user")),
                ("repository_evidence", Value::Null),
                ("evidence", a(&["user:explicit-wall-blue-clarification"])),
                (
                    "reason",
                    s(
                        "The current detecting developer explicitly clarified that this wall is blue.",
                    ),
                ),
                ("confirmed", Value::Bool(true)),
                ("strength", s("established")),
                ("basis", s("stronger-evidence")),
                ("impact", s("all-target-conditions")),
                ("review", Value::Null),
                ("reason_class", Value::Null),
            ]),
        ),
        ("capture", capture.clone()),
        (
            "comparison",
            o([("request", compared), ("proof", g(&proof, "proof").clone())]),
        ),
        ("replacement", Value::Null),
        ("delete_ids", a(&[])),
        ("context_review", context_review),
        ("context_edits", empty()),
        ("context", empty()),
    ]);
    let path = f.save(&request, "clarification-governance.json");
    let preview = native(&[
        "govern".into(),
        "preview".into(),
        "--repo".into(),
        f.repo.as_os_str().to_owned(),
        "--request".into(),
        path.into_os_string(),
    ]);
    let proposal = f.save(g(&preview, "proposal"), "clarification-proposal.json");
    let done = native(&[
        "govern".into(),
        "apply".into(),
        "--proposal".into(),
        proposal.into_os_string(),
    ]);
    assert_eq!(g(&done, "verified"), &Value::Bool(true));
    let (_, corrected, _) =
        agent_memory::records::parse(&std::fs::read_to_string(f.bank.join(&left)).unwrap(), &[])
            .unwrap();
    assert_eq!(g(&corrected, "version"), &Value::Int(2.into()));
    assert!(
        g(&corrected, "evidence")
            .as_array()
            .unwrap()
            .contains(&s("user:explicit-wall-blue-clarification"))
    );
    let history = format!("history/versions/{}/v1.md", g(&old, "id").as_str().unwrap());
    let view = f.good("inspect", &[]);
    let selected = g(&view, "sources")
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| [left.as_str(), right.as_str()].contains(&g(row, "path").as_str().unwrap()))
        .collect::<Vec<_>>();
    let primary = selected
        .iter()
        .find(|row| g(row, "path").as_str() == Some(&left))
        .unwrap();
    let mut judgment = f.judgment(&view, |a, b| {
        if [
            g(a, "path").as_str().unwrap(),
            g(b, "path").as_str().unwrap(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
            == BTreeSet::from([left.as_str(), right.as_str()])
        {
            "equivalent"
        } else {
            "related"
        }
    });
    *judgment.get_mut("consolidations").unwrap() = Value::Array(vec![o([
        (
            "sources",
            Value::Array(selected.iter().map(|row| g(row, "key").clone()).collect()),
        ),
        ("primary", g(primary, "key").clone()),
        ("body", s("# Wall\n\nThe wall should be blue.\n")),
        (
            "reason",
            s("The actual retained developer clarification resolves the old yellow source."),
        ),
    ])]);
    let before = f.state();
    let result = f.review(&judgment);
    let output = g(&result, "refresh_files");
    let (_, current, _) =
        agent_memory::records::parse(g(output, &left).as_str().unwrap(), &[]).unwrap();
    let (_, previous, _) =
        agent_memory::records::parse(g(output, &right).as_str().unwrap(), &[]).unwrap();
    assert_eq!(g(&current, "id"), g(&old, "id"));
    assert_eq!(g(&current, "version"), &Value::Int(3.into()));
    assert!(
        g(&current, "evidence")
            .as_array()
            .unwrap()
            .contains(&s("user:explicit-wall-blue-clarification"))
    );
    assert_eq!(g(&previous, "origin"), g(&landed, "origin"));
    assert_eq!(g(&previous, "status"), &s("superseded"));
    let (_, archived, _) =
        agent_memory::records::parse(g(output, &history).as_str().unwrap(), &[]).unwrap();
    assert_eq!(g(&archived, "origin"), g(&old, "origin"));
    assert!(output.get(&history).is_some());
    assert!(output.get(&history.replace("v1.md", "v2.md")).is_some());
    assert_eq!(before, f.state());
    f.save(output, "clarification-files.json");
    f.w.run_with(
        [
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "clarified_apply_worker".into(),
            "--nocapture".into(),
        ],
        RunOptions {
            env: &[("AGENT_MEMORY_FAULT_CLARIFIED_APPLY".into(), "1".into())],
            ..Default::default()
        },
    )
    .unwrap();
    let after = f.state();
    assert_eq!((&after.1, &after.2), (&before.1, &before.2));
}
#[test]
fn clarified_apply_worker() {
    if std::env::var_os("AGENT_MEMORY_FAULT_CLARIFIED_APPLY").is_none() {
        return;
    }
    let root = support::require_current_world().unwrap();
    agent_memory::cli::register_providers().unwrap();
    support::installed_runtime::register_component_runtime();
    let files = json::decode_bytes(
        &std::fs::read(root.join("clarification-files.json")).unwrap(),
        Default::default(),
    )
    .unwrap();
    let proposal =
        agent_memory::bank_transactions::preview(&root.join("repo"), &files, "refresh").unwrap();
    let receipt = agent_memory::bank_transactions::apply(&proposal).unwrap();
    assert_eq!(g(&receipt, "verified"), &Value::Bool(true));
}
