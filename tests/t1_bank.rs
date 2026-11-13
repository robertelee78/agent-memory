use agent_memory::bank_contract as bank;
use agent_memory::bank_layout as layout;
use agent_memory::codec::{
    frontmatter,
    json::{self, Value},
};
use agent_memory::error::Result;

const PROJECT: &str = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
const UPDATED: &str = "2026-10-22T17:00:00Z";
fn s(text: impl Into<String>) -> Value {
    Value::String(text.into())
}
fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn proposal(expanded: bool) -> Value {
    let supplied: Vec<_> = bank::CONTEXTS
        .iter()
        .copied()
        .filter(|p| expanded || *p == "projectbrief.md")
        .collect();
    obj([("project-id",s(PROJECT)),("router-updated",s(UPDATED)),("contexts",Value::Object(supplied.iter().map(|p| ((*p).to_owned(), obj([("body",s(format!("# {p}\n\nUseful project context; see [source](../README.md).\n"))),("purpose",s("Orient the agent")),("scope",s("repository")),("lifecycle",s("active")),("read-when",s("When resuming work"))]))).collect())),("omitted",Value::Object(bank::CONTEXTS.iter().filter(|p| !supplied.contains(p)).map(|p| ((*p).to_owned(),s("No relevant evidence supplied yet"))).collect()))])
}
fn files(expanded: bool) -> Value {
    field(&bank::render(&proposal(expanded)).unwrap(), "files").clone()
}
fn router(files: &Value) -> &str {
    field(files, "MEMORY.md").as_str().unwrap()
}
fn set(value: &mut Value, key: &str, new: Value) {
    *value.get_mut(key).unwrap() = new;
}
fn route(path: &str) -> Value {
    obj([
        ("path", s(path)),
        ("purpose", s("Useful fixture context")),
        ("scope", s("repository")),
        ("lifecycle", s("active")),
        ("read-when", s("When examining the fixture component")),
    ])
}
fn index_meta() -> Value {
    obj([
        ("schema", s("agent-memory/topic-index-v1")),
        ("project-id", s(PROJECT)),
        ("updated", s("2026-10-22T22:00:00Z")),
    ])
}

// obligation: tests/contracts/test_router.py::RouterContract.test_render_refuses_empty_files_and_missing_omission_reasons
#[test]
fn render_refuses_empty_files_and_missing_omission_reasons() {
    for corruption in ["empty", "missing-omission", "wrong-project", "wrong-time"] {
        let mut p = proposal(false);
        match corruption {
            "empty" => set(
                p.get_mut("contexts")
                    .unwrap()
                    .get_mut("projectbrief.md")
                    .unwrap(),
                "body",
                s("  "),
            ),
            "missing-omission" => set(&mut p, "omitted", Value::Object(vec![])),
            "wrong-project" => set(&mut p, "project-id", s("project-name")),
            _ => set(&mut p, "router-updated", s("2026-10-22")),
        }
        let command = bank::command_render(Ok(p));
        assert_eq!(command.exit_code, 1, "{corruption}");
        assert_eq!(
            field(command.value.as_ref().unwrap(), "writes_performed"),
            &Value::Bool(false)
        );
    }
}
// obligation: tests/contracts/test_router.py::RouterContract.test_byte_zero_schema_and_restricted_metadata_errors
#[test]
fn byte_zero_schema_and_restricted_metadata_errors() {
    let f = files(false);
    let original = router(&f);
    let bad = [
        format!("\u{feff}{original}"),
        format!("prefix\n{original}"),
        original.replace("\"agent-memory/v1\"", "\"agent-memory/v999\""),
        original.replacen("schema: ", "unknown: ", 1),
        original.replacen("---\n", "---\nschema: alias\n", 1),
        original.replace("\"agent-memory/v1\"", "&alias something"),
        original.replace("\"agent-memory/v1\"", "!tag something"),
        original.replace("\"agent-memory/v1\"", "{\"x\": 1, \"x\": 2}"),
        original.replace("\"agent-memory/v1\"", "\"unterminated"),
        original.replace(
            "schema: \"agent-memory/v1\"",
            "schema:\n  nested: unsupported",
        ),
    ];
    for text in bad {
        assert!(bank::parse_router(&text).is_err());
    }
}
// obligation: tests/contracts/test_router.py::RouterContract.test_timezone_offsets_are_validated_without_normalizing_invalid_components
#[test]
fn timezone_offsets_are_validated_without_normalizing_invalid_components() {
    let original = files(false);
    for offset in [
        "+00:99", "-00:60", "+24:00", "-24:00", "+99:00", "+01:99", "Z", "+00:00", "-00:00",
        "+05:30", "-07:00", "+23:59", "-23:59",
    ] {
        let valid = !["+00:99", "-00:60", "+24:00", "-24:00", "+99:00", "+01:99"].contains(&offset);
        let timestamp = format!("2026-10-22T17:00:00{offset}");
        let mut p = proposal(false);
        set(&mut p, "router-updated", s(&timestamp));
        assert_eq!(bank::render(&p).is_ok(), valid, "{offset}");
        assert_eq!(
            bank::parse_router(&router(&original).replace(UPDATED, &timestamp)).is_ok(),
            valid,
            "{offset}"
        );
    }
}
// obligation: tests/contracts/test_router.py::RouterContract.test_missing_empty_and_omitted_existing_targets_fail
#[test]
fn missing_empty_and_omitted_existing_targets_fail() {
    for case in ["missing", "empty", "omitted"] {
        let mut f = files(false);
        match case {
            "missing" => {
                if let Value::Object(map) = &mut f {
                    map.retain(|(p, _)| p != "projectbrief.md");
                }
            }
            "empty" => set(&mut f, "projectbrief.md", s(" \n")),
            _ => {
                if let Value::Object(map) = &mut f {
                    map.push(("progress.md".into(), s("Unrouted content")));
                }
            }
        }
        assert!(bank::validate_files(&f, None).is_err(), "{case}");
    }
}
// obligation: tests/contracts/test_router.py::RouterContract.test_router_bounds_duplicate_routes_and_orphan_files
#[test]
fn router_bounds_duplicate_routes_and_orphan_files() {
    let mut f = files(false);
    let original = router(&f).to_owned();
    let row = original
        .lines()
        .find(|l| l.contains("[projectbrief.md]"))
        .unwrap();
    for text in [
        original.replace(row, &format!("{row}\n{row}")),
        format!("{original}{}", "x".repeat(16384)),
        original.replace("(<projectbrief.md>)", "(<projectbrief.md)"),
        original.replace("| repository |", "| component:../other |"),
        original.replace(
            row,
            &(0..65)
                .map(|n| row.replace("projectbrief.md", &format!("topics/{n}.md")))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    ] {
        assert!(bank::parse_router(&text).is_err());
    }
    if let Value::Object(map) = &mut f {
        map.push(("topics/orphan.md".into(), s("Not reachable")));
    }
    assert!(bank::validate_files(&f, None).is_err());
}
// obligation: tests/contracts/test_router.py::RouterContract.test_optional_authority_anchor_uses_the_common_reader_and_canonical_identifiers
#[test]
fn optional_authority_anchor_uses_the_common_reader_and_canonical_identifiers() {
    for remote in [
        s("repo://github.com/owner/project"),
        Value::Null,
        s("https://secret@github.com/owner/project"),
        s("repo://github.com/Owner/Project"),
        s("unknown"),
    ] {
        let expected = remote.as_str() == Some("repo://github.com/owner/project");
        let mut p = proposal(false);
        if let Value::Object(map) = &mut p {
            map.push(("canonical-remote".into(), remote));
        }
        assert_eq!(bank::render(&p).is_ok(), expected);
    }
}
#[test]
fn rendering_matches_committed_contract_bytes() {
    let result = bank::render(&proposal(false)).unwrap();
    let f = field(&result, "files");
    let expected = concat!(
        "---\nschema: \"agent-memory/v1\"\nproject-id: \"8e0b66aa-2be5-4fa3-b47f-1245474aa7dd\"\nrouter-updated: \"2026-10-22T17:00:00Z\"\n---\n",
        "# Project memory\n\nRouter time describes this map, not every file's revision.\n\n## Routes\n\n",
        "| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n",
        "| [projectbrief.md](<projectbrief.md>) | Orient the agent | repository | active | When resuming work |\n",
        "\n## Omitted contexts\n\n",
        "- `productContext.md`: No relevant evidence supplied yet\n",
        "- `activeContext.md`: No relevant evidence supplied yet\n",
        "- `systemPatterns.md`: No relevant evidence supplied yet\n",
        "- `techContext.md`: No relevant evidence supplied yet\n",
        "- `progress.md`: No relevant evidence supplied yet\n"
    );
    assert_eq!(router(f), expected);
    assert_eq!(
        field(&bank::validate_files(f, None).unwrap(), "project_id"),
        &s(PROJECT)
    );
    assert!(
        json::decode(
            &bank::command_render(Ok(proposal(false))).stdout,
            json::DecodePolicy::default()
        )
        .is_ok()
    );
}
// obligation: tests/contracts/test_router.py::RouterContract.test_expanded_bank_has_six_useful_contexts_and_independent_router_time
#[test]
fn expanded_bank_retains_independent_router_time_and_context_additions() {
    let mut f = files(true);
    let before = router(&f).to_owned();
    set(
        &mut f,
        "activeContext.md",
        s("# Current focus\n\nNext: verify a durable lesson.\n"),
    );
    let checked = bank::validate_files(&f, None).unwrap();
    assert_eq!(field(&checked, "routes").as_array().unwrap().len(), 6);
    assert_eq!(field(&checked, "omitted"), &Value::Object(vec![]));
    assert_eq!(router(&f), before);
    let minimal = files(false);
    for original in [
        router(&minimal).to_owned(),
        router(&minimal).replace("\n\n## Omitted", "\n## Omitted"),
    ] {
        let with_prose = original.replace(
            "# Project memory",
            "# Project memory\n\nUnrelated navigation prose survives.",
        );
        let updated = bank::add_context_routes(
            &with_prose,
            &["activeContext.md".into()],
            &s("2026-10-23T17:00:00Z"),
        )
        .unwrap();
        assert!(updated.contains("Unrelated navigation prose survives."));
        let (meta, routes, omitted) = bank::parse_router(&updated).unwrap();
        assert_eq!(field(&meta, "router-updated"), &s("2026-10-23T17:00:00Z"));
        assert_eq!(routes.len(), 2);
        assert!(omitted.get("activeContext.md").is_none());
    }
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_index_byte_row_and_unicode_framing_limits_are_enforced_before_a_valid_result
#[test]
fn index_byte_row_and_unicode_framing_limits_are_enforced_before_a_valid_result() {
    for separator in ['\u{85}', '\u{2028}', '\u{2029}', '\t'] {
        for key in ["purpose", "read-when", "scope", "path"] {
            let mut r = route("topics/example.md");
            let old = field(&r, key).as_str().unwrap().to_owned();
            set(&mut r, key, s(format!("{old}{separator}extra")));
            assert!(layout::index_text("topics/index.md", &index_meta(), &[r]).is_err());
        }
    }
    let routes: Vec<_> = (0..65).map(|n| route(&format!("topics/{n}.md"))).collect();
    assert!(layout::index_text("topics/index.md", &index_meta(), &routes).is_err());
    let text = layout::index_text(
        "topics/index.md",
        &index_meta(),
        &[route("topics/example.md")],
    )
    .unwrap();
    let oversized = text.replace(
        "project-id: ",
        &format!("project-id: {}", " ".repeat(17000)),
    );
    assert!(layout::validate_index("topics/index.md", oversized.as_bytes()).is_err());
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_source_index_locations_reject_controls_and_preserve_ordinary_unicode_and_spaces
#[test]
fn source_index_locations_reject_controls_and_preserve_ordinary_unicode_and_spaces() {
    let routes = [route("episodes/retry.md")];
    let good = layout::index_text("topics/café index.md", &index_meta(), &routes).unwrap();
    for c in [
        '\0', '\t', '\n', '\r', '\u{1c}', '\u{7f}', '\u{85}', '\u{2028}', '\u{2029}',
    ] {
        let path = format!("topics/in{c}dex.md");
        assert!(layout::index_text(&path, &index_meta(), &routes).is_err());
        assert!(layout::validate_index(&path, good.as_bytes()).is_err());
    }
    assert!(layout::validate_index("topics/café index.md", good.as_bytes()).is_ok());
    let (_, body) = frontmatter::read(&good, &["schema", "project-id", "updated"]).unwrap();
    assert_eq!(
        layout::parse_index("topics/café index.md", &index_meta(), body).unwrap(),
        routes
    );
}
#[test]
fn missing_provider_refuses_without_claiming_bank_validation() -> Result<()> {
    assert!(bank::validate(std::path::Path::new("/"), None, None).is_err());
    assert!(bank::recover_layout(&[]).is_err());
    Ok(())
}

fn put(files: &mut Value, path: &str, content: String) {
    if let Some(old) = files.get_mut(path) {
        *old = s(content);
    } else if let Value::Object(map) = files {
        map.push((path.to_owned(), s(content)));
    }
}
fn remove(files: &mut Value, path: &str) {
    if let Value::Object(map) = files {
        map.retain(|(p, _)| p != path);
    }
}
fn root_route(files: &mut Value, route: &Value) {
    let row = bank::route_table(std::slice::from_ref(route), "MEMORY.md")
        .unwrap()
        .split_inclusive('\n')
        .skip(2)
        .collect::<String>();
    let text = router(files).replace(
        "\n## Omitted contexts",
        &format!("{row}\n## Omitted contexts"),
    );
    put(files, "MEMORY.md", text);
}
fn record(scope: &str, kind: &str, status: &str) -> (Value, String) {
    let proposal = json::decode(
        include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
        json::DecodePolicy::default(),
    )
    .unwrap();
    let mut metadata = field(&proposal, "metadata").clone();
    for (k, v) in [
        ("project-id", s(PROJECT)),
        ("scope", s(scope)),
        ("kind", s(kind)),
        ("status", s(status)),
        ("conditions", Value::Object(vec![])),
    ] {
        set(&mut metadata, k, v);
    }
    set(
        metadata.get_mut("origin").unwrap(),
        "code-state",
        s("not-code"),
    );
    if let Value::Object(map) = &mut metadata {
        map.extend([
            ("trust".into(), s("reviewed")),
            ("visibility".into(), s("visible")),
            (
                "admission-reason".into(),
                s("Disposable structural fixture; not real admission evidence"),
            ),
        ]);
        if ["quarantined", "superseded", "revoked", "expired"].contains(&status) {
            map.push((
                "state-reason".into(),
                s("Historical fixture; excluded from current guidance"),
            ));
        }
    }
    (
        metadata,
        "# Bounded lesson\n\n[Current authority](../../docs/adr/retry.md).\n".into(),
    )
}
fn record_text(metadata: &Value, body: &str) -> String {
    frontmatter::write(metadata.as_object().unwrap()).unwrap() + body
}
fn nested() -> (Value, String) {
    let mut f = files(false);
    let path = "episodes/retry notes.md";
    let (mut meta, body) = record("component:src/retry", "result", "active");
    set(
        &mut meta,
        "conditions",
        obj([("branch", s("feature/retry"))]),
    );
    put(&mut f, path, record_text(&meta, &body));
    let mut leaf = route(path);
    set(&mut leaf, "scope", s("component:src/retry"));
    let inner = layout::index_text("topics/retries/index.md", &index_meta(), &[leaf]).unwrap();
    put(&mut f, "topics/retries/index.md", inner);
    let mut mid = route("topics/retries/index.md");
    set(&mut mid, "scope", s("component:src/retry"));
    let outer = layout::index_text("topics/index.md", &index_meta(), &[mid]).unwrap();
    put(&mut f, "topics/index.md", outer);
    let mut root = route("topics/index.md");
    set(&mut root, "scope", s("branch:feature/retry"));
    root_route(&mut f, &root);
    (f, path.into())
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_all_optional_directories_preserve_record_identity_provenance_conditions_and_nonbinding_procedures
#[test]
fn all_optional_directories_preserve_record_identity_provenance_conditions_and_nonbinding_procedures()
 {
    let mut f = files(false);
    let paths = [
        ("topics/decision.md", "project-decision", "active"),
        ("episodes/retry.md", "result", "active"),
        ("procedures/retry.md", "procedural-candidate", "active"),
        ("history/revoked.md", "semantic-fact", "revoked"),
    ];
    for (n, (path, kind, status)) in paths.iter().enumerate() {
        let (mut meta, body) = record("component:src/retry", kind, status);
        set(
            &mut meta,
            "id",
            s(format!("a9abac6f-c33e-44c9-b40f-7b2ebafda86{n}")),
        );
        set(
            &mut meta,
            "conditions",
            obj([
                ("branch", s("feature/retry")),
                ("environment", s("fixture")),
                ("prerequisite", Value::Null),
            ]),
        );
        put(&mut f, path, record_text(&meta, &body));
        let mut r = route(path);
        set(&mut r, "scope", s("component:src/retry"));
        set(&mut r, "lifecycle", s(*status));
        root_route(&mut f, &r);
    }
    let before = f.clone();
    let mut parsed = bank::ParsedRecords::new();
    let checked = bank::validate_files(&f, Some(&mut parsed)).unwrap();
    let entries = field(&checked, "entries").as_array().unwrap();
    for (path, kind, _) in paths {
        let entry = entries
            .iter()
            .find(|e| field(e, "path").as_str() == Some(path))
            .unwrap();
        assert_eq!(field(entry, "kind"), &s(kind));
        assert_eq!(field(entry, "binding_procedure"), &Value::Bool(false));
        assert_eq!(
            field(entry, "conditions"),
            field(&parsed[path].1, "conditions")
        );
        assert_eq!(
            record_text(&parsed[path].1, &parsed[path].2),
            field(&f, path).as_str().unwrap()
        );
    }
    assert_eq!(f, before);
    assert!(bank::validate_files(&f, Some(&mut parsed)).is_err());
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_two_nested_indices_resolve_correct_relative_links_and_retain_inherited_conditions_after_clone
#[test]
fn two_nested_indices_resolve_correct_relative_links_and_retain_inherited_conditions() {
    let (f, path) = nested();
    assert!(
        field(&f, "topics/retries/index.md")
            .as_str()
            .unwrap()
            .contains("(<../../episodes/retry notes.md>)")
    );
    let checked = bank::validate_files(&f, None).unwrap();
    let leaf = field(&checked, "entries")
        .as_array()
        .unwrap()
        .iter()
        .find(|e| field(e, "path").as_str() == Some(&path))
        .unwrap();
    assert_eq!(
        field(leaf, "scope_chain"),
        &Value::Array(vec![
            s("branch:feature/retry"),
            s("component:src/retry"),
            s("component:src/retry")
        ])
    );
    assert_eq!(
        field(leaf, "conditions"),
        &obj([("branch", s("feature/retry"))])
    );
    assert_eq!(
        field(&bank::validate_files(&f.clone(), None).unwrap(), "entries"),
        field(&checked, "entries")
    );
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_missing_orphan_cyclic_duplicate_and_overdeep_routes_refuse_without_repair
#[test]
fn missing_orphan_cyclic_duplicate_and_overdeep_routes_refuse_without_repair() {
    for case in ["missing", "orphan", "cycle", "duplicate", "deep"] {
        let (mut f, path) = nested();
        match case {
            "missing" => remove(&mut f, &path),
            "orphan" => {
                let (meta, body) = record("repository", "semantic-fact", "active");
                put(&mut f, "topics/orphan.md", record_text(&meta, &body));
            }
            "cycle" => put(
                &mut f,
                "topics/retries/index.md",
                layout::index_text(
                    "topics/retries/index.md",
                    &index_meta(),
                    &[route("topics/index.md")],
                )
                .unwrap(),
            ),
            "duplicate" => {
                let mut r = route(&path);
                set(&mut r, "scope", s("component:src/retry"));
                root_route(&mut f, &r);
            }
            _ => {
                let mut r = route(&path);
                set(&mut r, "scope", s("component:src/retry"));
                put(
                    &mut f,
                    "topics/third.md",
                    layout::index_text("topics/third.md", &index_meta(), &[r]).unwrap(),
                );
                put(
                    &mut f,
                    "topics/retries/index.md",
                    layout::index_text(
                        "topics/retries/index.md",
                        &index_meta(),
                        &[route("topics/third.md")],
                    )
                    .unwrap(),
                );
            }
        }
        let before = f.clone();
        assert!(bank::validate_files(&f, None).is_err(), "{case}");
        assert_eq!(f, before);
    }
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_metadata_project_status_unadmitted_unknown_schema_and_inherited_scope_mismatches_refuse
#[test]
fn metadata_project_status_unadmitted_unknown_schema_and_inherited_scope_mismatches_refuse() {
    for case in [
        "project",
        "status",
        "candidate",
        "schema",
        "plain",
        "inheritance",
        "procedure",
    ] {
        let (mut f, mut path) = nested();
        let (mut meta, body) = record("component:src/retry", "result", "active");
        set(
            &mut meta,
            "conditions",
            obj([("branch", s("feature/retry"))]),
        );
        match case {
            "project" => set(
                &mut meta,
                "project-id",
                s("75b38e17-c36e-4fab-8ba9-5fa51f25de91"),
            ),
            "status" => set(&mut meta, "status", s("revoked")),
            "candidate" => {
                set(&mut meta, "status", s("candidate"));
                let mut r = route(&path);
                set(&mut r, "scope", s("component:src/retry"));
                set(&mut r, "lifecycle", s("candidate"));
                put(
                    &mut f,
                    "topics/retries/index.md",
                    layout::index_text("topics/retries/index.md", &index_meta(), &[r]).unwrap(),
                );
            }
            "schema" => set(&mut meta, "schema", s("agent-memory/record-v999")),
            "inheritance" => set(&mut meta, "conditions", Value::Object(vec![])),
            "procedure" => {
                remove(&mut f, &path);
                path = "procedures/retry.md".into();
                let mut r = route(&path);
                set(&mut r, "scope", s("component:src/retry"));
                put(
                    &mut f,
                    "topics/retries/index.md",
                    layout::index_text("topics/retries/index.md", &index_meta(), &[r]).unwrap(),
                );
            }
            _ => (),
        }
        put(
            &mut f,
            &path,
            if case == "plain" {
                "SECRET_LAYOUT_CANARY".into()
            } else {
                record_text(&meta, &body)
            },
        );
        let error = bank::validate_files(&f, None).unwrap_err();
        assert!(!error.to_string().contains("SECRET_LAYOUT_CANARY"));
        let command = layout::command_result(Err(error));
        assert_eq!(command.exit_code, 1);
        assert!(!command.stdout.contains("SECRET_LAYOUT_CANARY"));
    }
}
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_optional_history_event_is_not_current_guidance_and_duplicate_or_deletion_labels_refuse
#[test]
fn optional_history_event_is_not_current_guidance_and_duplicate_or_deletion_labels_refuse() {
    for case in [
        "safe",
        "duplicate",
        "deleted",
        "body",
        "current-only-in-history",
    ] {
        let mut f = files(false);
        let (meta, body) = record("repository", "semantic-fact", "active");
        let current = record_text(&meta, &body);
        put(&mut f, "topics/current.md", current.clone());
        root_route(&mut f, &route("topics/current.md"));
        let event = obj([
            ("schema", s("agent-memory/history-event-v1")),
            ("id", s("b3dcae21-f5f3-40ac-bcfe-e845a3d2868b")),
            ("project-id", s(PROJECT)),
            ("kind", s("memory-admitted")),
            (
                "record",
                obj([
                    ("id", field(&meta, "id").clone()),
                    ("version", Value::Int(1.into())),
                ]),
            ),
            ("actor", s("fixture-observer")),
            ("recorded-at", s("2026-10-22T22:00:00Z")),
            (
                "evidence",
                Value::Array(vec![s("fixture://admission-review")]),
            ),
        ]);
        let event = record_text(
            &event,
            if case == "body" {
                "SECRET_HISTORY_CANARY"
            } else {
                ""
            },
        );
        put(&mut f, "history/event.md", event.clone());
        let mut r = route("history/event.md");
        if case == "deleted" {
            set(&mut r, "lifecycle", s("deleted"));
        }
        root_route(&mut f, &r);
        if case == "duplicate" {
            put(&mut f, "history/copy.md", event);
            root_route(&mut f, &route("history/copy.md"));
        }
        if case == "current-only-in-history" {
            remove(&mut f, "topics/current.md");
            put(&mut f, "history/active.md", current.clone());
            let text = router(&f).replace("topics/current.md", "history/active.md");
            put(&mut f, "MEMORY.md", text);
        }
        let result = bank::validate_files(&f, None);
        assert_eq!(result.is_ok(), case == "safe", "{case}");
        if case == "safe" {
            let checked = result.unwrap();
            let entry = field(&checked, "entries")
                .as_array()
                .unwrap()
                .iter()
                .find(|e| field(e, "path").as_str() == Some("history/event.md"))
                .unwrap();
            assert_eq!(field(entry, "current_guidance"), &Value::Bool(false));
            remove(&mut f, "history/event.md");
            let text = router(&f)
                .split_inclusive('\n')
                .filter(|line| !line.contains("[history/event.md]"))
                .collect::<String>();
            put(&mut f, "MEMORY.md", text);
            assert!(bank::validate_files(&f, None).is_ok());
            assert_eq!(field(&f, "topics/current.md"), &s(current));
        } else {
            assert!(
                !result
                    .unwrap_err()
                    .to_string()
                    .contains("SECRET_HISTORY_CANARY")
            );
        }
    }
}
#[test]
fn memo_keeps_companion_records_and_rechecks_router() {
    agent_memory::validation_memo::operation(|_| {
        let (f, _) = nested();
        let mut a = bank::ParsedRecords::new();
        let mut b = bank::ParsedRecords::new();
        let first = bank::validate_files(&f, Some(&mut a)).unwrap();
        let second = bank::validate_files(&f, Some(&mut b)).unwrap();
        assert_eq!(first, second);
        assert_eq!(a, b);
        let mut changed = f;
        let text = router(&changed).replace(PROJECT, "not-a-project");
        put(&mut changed, "MEMORY.md", text);
        assert!(bank::validate_files(&changed, None).is_err());
    });
}
#[test]
fn portable_paths_detect_case_unicode_components_and_file_topology() {
    for paths in [
        vec!["topics/Parent/a.md", "topics/parent/b.md"],
        vec!["topics/café/a.md", "topics/cafe\u{301}/b.md"],
        vec!["topics/Straße.md", "topics/STRASSE.md"],
    ] {
        assert!(bank::portable_aliases(paths).is_err());
    }
    assert!(bank::file_topology(["topics/parent.md", "topics/parent.md/child.md"]).is_err());
    assert!(bank::file_topology(["topics/parent/one.md", "topics/parent/two.md"]).is_ok());
    assert_eq!(bank::portable_key("Straße"), "strasse");
}
struct World {
    root: std::path::PathBuf,
}
impl World {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "agent-memory-test-corebank-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root: std::fs::canonicalize(root).unwrap(),
        }
    }
    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.root.join(relative)
    }
    fn mkdir(&self, relative: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.path(relative);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn write(&self, relative: &str, bytes: &[u8]) {
        std::fs::write(self.path(relative), bytes).unwrap();
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn physical_inventory_checks_runtime_descendants_without_following_links() {
    use std::os::unix::fs::symlink;
    let world = World::new();
    world.mkdir("memory-bank/topics");
    world.mkdir("memory-bank/.agent-memory-runtime");
    world.write("memory-bank/MEMORY.md", b"router");
    world.write("memory-bank/.agent-memory-runtime/state", b"runtime");
    world.write("memory-bank/topics/lesson.md", b"lesson");
    let bank_path = world.path("memory-bank");
    let paths = bank::physical_paths(&bank_path).unwrap();
    assert_eq!(paths, ["MEMORY.md", "topics", "topics/lesson.md"]);
    symlink(
        world.path("memory-bank/topics/lesson.md"),
        world.path("memory-bank/.agent-memory-runtime/unsafe"),
    )
    .unwrap();
    assert!(bank::physical_paths(&bank_path).is_err());
    assert_eq!(
        std::fs::read(world.path("memory-bank/topics/lesson.md")).unwrap(),
        b"lesson"
    );
}
#[test]
fn bank_root_spelling_alias_is_refused_without_repair() {
    let world = World::new();
    world.mkdir("Memory-Bank");
    assert!(bank::bank_root(&world.path("memory-bank")).is_err());
    assert!(world.path("Memory-Bank").is_dir());
}
#[test]
fn descriptor_listing_refuses_over_bound_without_partial_inventory() {
    let world = World::new();
    world.write("a", b"a");
    world.write("b", b"b");
    let directory = agent_memory::fs::Descriptor::open_directory(&world.root).unwrap();
    assert_eq!(directory.entries_bounded(2).unwrap().unwrap().len(), 2);
    assert!(directory.entries_bounded(1).unwrap().is_none());
    assert!(directory.entries_bounded(0).unwrap().is_none());
}
#[test]
fn state_paths_reject_symlinks_repo_local_and_nondirectories() {
    use agent_memory::bank_writes::{StateEnvironment, state_paths_with, state_root_with};
    use std::os::unix::fs::symlink;
    let world = World::new();
    world.mkdir("repo/memory-bank");
    world.mkdir("state");
    let repo = world.path("repo");
    let bank_path = repo.join("memory-bank");
    let mut environment = StateEnvironment {
        state_home: Some(world.path("state").into_os_string()),
        xdg_state_home: None,
        home: None,
    };
    let (directory, receipt) = state_paths_with(&repo, &bank_path, PROJECT, &environment).unwrap();
    assert_eq!(
        receipt.file_name().unwrap(),
        format!("{PROJECT}.json").as_str()
    );
    assert!(!directory.exists());
    assert_eq!(
        directory
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .len(),
        64
    );
    environment.state_home = Some(repo.join("runtime-state").into_os_string());
    assert!(state_root_with(Some(&repo), &environment).is_err());
    world.write("not-a-directory", b"preserve");
    environment.state_home = Some(world.path("not-a-directory").into_os_string());
    assert!(state_root_with(Some(&repo), &environment).is_err());
    symlink(world.path("state"), world.path("linked")).unwrap();
    environment.state_home = Some(world.path("linked").join("child").into_os_string());
    assert!(state_root_with(Some(&repo), &environment).is_err());
    environment.state_home = Some("relative".into());
    assert!(state_root_with(Some(&repo), &environment).is_err());
    assert_eq!(
        std::fs::read(world.path("not-a-directory")).unwrap(),
        b"preserve"
    );
}
#[test]
fn receipts_are_private_observed_and_never_overwrite_unexpected_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let world = World::new();
    let receipt = world.path("state/operations/receipt.json");
    let payload = b"{\n  \"verified\": true\n}\n";
    agent_memory::bank_writes::record(&receipt, payload).unwrap();
    assert_eq!(std::fs::read(&receipt).unwrap(), payload);
    assert_eq!(
        std::fs::metadata(&receipt).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(agent_memory::bank_writes::record(&receipt, b"unexpected replacement").is_err());
    assert_eq!(std::fs::read(receipt).unwrap(), payload);
}
#[test]
fn write_proposals_refuse_missing_scope_provider_and_invalid_paths() {
    let world = World::new();
    let proposal = obj([
        ("schema", s("agent-memory/write-proposal-v1")),
        ("operation_id", s(PROJECT)),
        ("operation", s("replace")),
        ("repo_root", s(world.root.to_str().unwrap())),
        ("project_id", s(PROJECT)),
        (
            "observed_hashes",
            obj([
                ("MEMORY.md", s("a".repeat(64))),
                ("activeContext.md", s("b".repeat(64))),
            ]),
        ),
        ("path", s("activeContext.md")),
        ("content", s("# Updated\n")),
        ("scope_context", Value::Null),
    ]);
    assert!(
        agent_memory::bank_writes::proposal_value(&proposal)
            .unwrap_err()
            .to_string()
            .contains("scope_binding")
    );
    for path in [
        "../outside.md",
        "/tmp/outside.md",
        "MEMORY.md",
        "topics/.runtime.md",
    ] {
        let mut bad = proposal.clone();
        set(&mut bad, "path", s(path));
        assert!(agent_memory::bank_writes::proposal_value(&bad).is_err());
    }
    let response = agent_memory::bank_writes::command_result(
        "apply",
        Err(agent_memory::error::ContractError::new("refused")),
    );
    assert_eq!(response.exit_code, 1);
    assert_eq!(
        field(response.value.as_ref().unwrap(), "inspection_required"),
        &Value::Bool(true)
    );
}
#[test]
fn replacement_capacity_counts_utf8_complete_result_without_writing() {
    let f = files(false);
    let observation = bank::Observation {
        checked: Value::Null,
        hashes: Value::Null,
        sources: f
            .as_object()
            .unwrap()
            .iter()
            .map(|(p, b)| (p.clone(), b.as_str().unwrap().as_bytes().to_vec()))
            .collect(),
        recovery_frontier: Value::Null,
    };
    assert!(
        agent_memory::bank_writes::validate_replacement(
            &observation,
            "projectbrief.md",
            &"é".repeat(8192)
        )
        .is_ok()
    );
    let error = agent_memory::bank_writes::validate_replacement(
        &observation,
        "projectbrief.md",
        &format!("{}x", "é".repeat(8192)),
    )
    .unwrap_err();
    assert!(matches!(
        error.kind(),
        agent_memory::error::Kind::BankCapacity {
            observed: 16385,
            ..
        }
    ));
    assert_eq!(
        field(&f, "projectbrief.md").as_str().unwrap().as_bytes(),
        observation.sources["projectbrief.md"]
    );
}

// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_tombstones_and_duplicate_record_versions_cannot_hide_or_recreate_current_content
#[test]
fn duplicate_record_versions_and_multiple_active_versions_refuse() {
    for version in [1, 2] {
        let mut f = files(false);
        let (mut meta, body) = record("repository", "semantic-fact", "active");
        put(&mut f, "topics/one.md", record_text(&meta, &body));
        root_route(&mut f, &route("topics/one.md"));
        set(&mut meta, "version", Value::Int(version.into()));
        put(&mut f, "topics/two.md", record_text(&meta, &body));
        root_route(&mut f, &route("topics/two.md"));
        assert_eq!(
            bank::validate_files(&f, None).unwrap_err().to_string(),
            "Duplicate record version or ambiguous active versions"
        );
    }
}

#[test]
fn malformed_router_text_has_exact_refusals_and_never_panics() {
    let valid = files(false);
    let original = router(&valid);
    for (text, expected) in [
        ("", "Frontmatter must start at byte zero with --- and LF"),
        (
            "---\nschema: \"agent-memory/v1\"\n",
            "Missing frontmatter terminator",
        ),
    ] {
        assert_eq!(bank::parse_router(text).unwrap_err().to_string(), expected);
        assert_eq!(
            bank::add_context_routes(text, &["activeContext.md".into()], &s(UPDATED))
                .unwrap_err()
                .to_string(),
            expected
        );
    }
    let reversed = original
        .replace("\n## Routes\n", "\n## Temporary\n")
        .replace("\n## Omitted contexts\n", "\n## Routes\n")
        .replace("\n## Temporary\n", "\n## Omitted contexts\n");
    assert_eq!(
        bank::parse_router(&reversed).unwrap_err().to_string(),
        "Router needs exactly one Routes and Omitted contexts section"
    );
    for end in original.char_indices().map(|(end, _)| end) {
        let text = original.get(..end).unwrap();
        let _ = bank::parse_router(text);
        let _ = bank::add_context_routes(text, &["activeContext.md".into()], &s(UPDATED));
    }
    assert_eq!(
        bank::parse_table("", "MEMORY.md").unwrap_err().to_string(),
        "Invalid route table header"
    );
    assert_eq!(
        bank::route_value(&Value::Null).unwrap_err().to_string(),
        "Routes require path, purpose, scope, lifecycle and read-when"
    );
    assert_eq!(
        layout::index_text("topics/index.md", &Value::Null, &[])
            .unwrap_err()
            .to_string(),
        "Metadata requires the known fields and no extras"
    );
}

#[test]
fn missing_and_mistyped_proposal_fields_have_exact_refusals() {
    let world = World::new();
    let valid = obj([
        ("schema", s("agent-memory/write-proposal-v1")),
        ("operation_id", s(PROJECT)),
        ("operation", s("replace")),
        ("repo_root", s(world.root.to_str().unwrap())),
        ("project_id", s(PROJECT)),
        (
            "observed_hashes",
            obj([
                ("MEMORY.md", s("a".repeat(64))),
                ("activeContext.md", s("b".repeat(64))),
            ]),
        ),
        ("path", s("activeContext.md")),
        ("content", s("# Updated\n")),
        ("scope_context", Value::Null),
    ]);
    for (key, value) in [
        ("operation", Value::Bool(false)),
        ("repo_root", Value::Array(vec![])),
        ("project_id", Value::Null),
    ] {
        let mut bad = valid.clone();
        set(&mut bad, key, value);
        assert_eq!(
            agent_memory::bank_writes::proposal_value(&bad)
                .unwrap_err()
                .to_string(),
            "Invalid observed-write proposal"
        );
        remove(&mut bad, key);
        assert_eq!(
            agent_memory::bank_writes::apply(&bad)
                .unwrap_err()
                .to_string(),
            "Invalid observed-write proposal"
        );
    }
}

#[test]
fn truncated_receipt_has_exact_decode_refusal_without_mutation() {
    let world = World::new();
    world.write("receipt.json", b"{");
    let raw = std::fs::read(world.path("receipt.json")).unwrap();
    let error = agent_memory::bank_writes::load_receipt(&raw).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Expecting property name enclosed in double quotes at byte 1"
    );
    let response = agent_memory::bank_writes::command_result("apply", Err(error));
    assert_eq!(response.exit_code, 1);
    assert_eq!(
        field(response.value.as_ref().unwrap(), "error"),
        &s("Expecting property name enclosed in double quotes at byte 1")
    );
    assert_eq!(std::fs::read(world.path("receipt.json")).unwrap(), b"{");
}

#[test]
fn malformed_deep_command_values_refuse_encoding_instead_of_panicking() {
    let mut value = Value::Null;
    for _ in 0..257 {
        value = Value::Array(vec![value]);
    }
    let write = agent_memory::bank_writes::command_result("apply", Ok(value.clone()));
    assert_eq!(write.exit_code, 1);
    assert_eq!(
        field(write.value.as_ref().unwrap(), "error"),
        &s("JSON depth limit exceeded")
    );
    let index = layout::command_result(Ok(value));
    assert_eq!(index.exit_code, 1);
    assert_eq!(
        field(index.value.as_ref().unwrap(), "error"),
        &s("Invalid optional record layout, index or canonical bank; inspect original files")
    );
}
