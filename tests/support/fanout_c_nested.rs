//! Additional baseline closure and nested interruption journeys on public am commands.
use super::*;
use std::os::unix::fs::MetadataExt;
trait ArrayMut {
    fn as_array_mut(&mut self) -> Option<&mut Vec<Value>>;
}
impl ArrayMut for Value {
    fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(rows) => Some(rows),
            _ => None,
        }
    }
}
fn bytes(path: &Path) -> BTreeMap<String, Vec<u8>> {
    snapshot(path, false)
}
fn modes(bank: &Path) -> BTreeMap<String, u32> {
    snapshot(bank, true)
        .keys()
        .map(|p| {
            (
                p.clone(),
                fs::metadata(bank.join(p)).unwrap().mode() & 0o7777,
            )
        })
        .collect()
}
fn read(path: &Path) -> Value {
    json::decode_bytes(&fs::read(path).unwrap(), Default::default()).unwrap()
}
fn originals(f: &Fixture) -> BTreeMap<String, Vec<u8>> {
    array(g(&f.manifest, "sources"))
        .iter()
        .map(|r| {
            (
                t(r, "path").to_owned(),
                fs::read(f.directory.join(t(r, "blob"))).unwrap(),
            )
        })
        .collect()
}
fn feature(f: &Fixture) -> (Vec<u8>, Vec<u8>) {
    fs::write(f.repo.join("feature.txt"), b"Feature baseline\n").unwrap();
    f.world.git(["add", "feature.txt"]).unwrap();
    f.world
        .git(["commit", "-m", "Synthetic feature baseline"])
        .unwrap();
    fs::write(f.repo.join("feature.txt"), b"Staged feature\n").unwrap();
    f.world.git(["add", "feature.txt"]).unwrap();
    fs::write(f.repo.join("feature.txt"), b"Unstaged feature\n").unwrap();
    (
        fs::read(f.repo.join(".git/index")).unwrap(),
        f.world.git(["rev-parse", "HEAD"]).unwrap().stdout,
    )
}
fn unchanged(f: &Fixture, saved: &(Vec<u8>, Vec<u8>)) {
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), saved.0);
    assert_eq!(f.world.git(["rev-parse", "HEAD"]).unwrap().stdout, saved.1);
    assert_eq!(
        fs::read(f.repo.join("feature.txt")).unwrap(),
        b"Unstaged feature\n"
    );
}
fn refused_preview(f: &Fixture, nested: &str, mode: &str) {
    let out = f.world.inside(format!("refused-{}.json", uuid())).unwrap();
    f.layout(
        "recover-nested",
        vec![
            "--operation".into(),
            f.operation.clone().into(),
            "--nested".into(),
            nested.into(),
            "--mode".into(),
            mode.into(),
            "--output".into(),
            out.clone().into_os_string(),
        ],
        1,
    );
    assert!(!out.exists());
}
fn fault_layout(f: &Fixture, operation: &str, proposal: &Value, stage: &str, mode: &str) {
    let (code, value) = f.call(
        vec![
            "bank".into(),
            "recover-layout".into(),
            operation.into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--proposal".into(),
            f.save(proposal),
        ],
        Some((stage, mode)),
    );
    assert_eq!(code, if mode == "exit" { 86 } else { 1 }, "{value:?}");
    if mode == "exit" {
        f.unlock();
    }
}
fn nested_rollback(stage: &str, mode: &str, path_role: &str, early: bool) {
    let f = Fixture::new();
    let work = feature(&f);
    let source = originals(&f);
    let hold = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
    let sentinel = fs::read(f.bank.join(".agent-memory-recovery.md")).unwrap();
    let before = f.snapshot();
    let old_modes = modes(&f.bank);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    let pending = g(&status, "pending_unit_ids").clone();
    let (proposal, nested, target) = f.admission(false);
    let event = format!("history/events/{nested}.md");
    let path = match path_role {
        "target" => target.as_str(),
        "router" => "MEMORY.md",
        _ => event.as_str(),
    };
    let stage = if stage.ends_with("after-rename") || stage.ends_with("after-progress") {
        format!("{stage}:{path}")
    } else {
        stage.to_owned()
    };
    if early {
        f.fault_admission(&proposal, &stage, mode);
    } else {
        f.fault_admission(&proposal, "layout-recovery-nested-before-receipt", "error");
        refused_preview(&f, &nested, "finish");
        let preview = f.nested_preview(&nested, "rollback");
        fault_layout(&f, "recover-nested", &preview, &stage, mode);
        assert!(
            f.directory
                .join("nested")
                .join(&nested)
                .join("rollback-plan.json")
                .is_file()
        );
    }
    if stage.ends_with("before-nested-marker") {
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
            hold
        );
        assert_eq!(f.snapshot(), before);
        f.good(vec![
            "admission".into(),
            "apply".into(),
            "--proposal".into(),
            f.save(&proposal),
        ]);
    } else {
        refused_preview(&f, &nested, "finish");
        let preview = f.nested_preview(&nested, "rollback");
        f.recover(&preview, 0);
        assert_eq!(f.snapshot(), before);
        assert_eq!(modes(&f.bank), old_modes);
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
            hold
        );
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-recovery.md")).unwrap(),
            sentinel
        );
    }
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(g(&status, "pending_unit_ids"), &pending);
    assert_eq!(
        g(&status, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    assert_eq!(originals(&f), source);
    unchanged(&f, &work);
}
fn nested_receipt(stage: &str, mode: &str, partial: bool) {
    let f = Fixture::new();
    let work = feature(&f);
    let source = originals(&f);
    let hold = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
    let old_modes = modes(&f.bank);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    let pending = g(&status, "pending_unit_ids").clone();
    let (proposal, nested, target) = f.admission(!partial);
    f.fault_admission(&proposal, stage, mode);
    let audit = f.world.paths["state"].clone();
    fn find(root: &Path, name: &str) -> Option<PathBuf> {
        for e in fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if let Some(p) = find(&p, name) {
                    return Some(p);
                }
            } else if p.file_name().unwrap().to_str().unwrap().starts_with(name) {
                return Some(p);
            }
        }
        None
    }
    let audit = find(&audit, &format!("{nested}.capture.started.")).unwrap();
    let audit_bytes = fs::read(&audit).unwrap();
    let capture = g(g(g(&proposal, "proposal"), "admission_evidence"), "capture");
    assert_eq!(
        g(&read(&audit), "source_classes"),
        &Value::Array(vec![g(g(capture, "request"), "source_class").clone()])
    );
    let committed = f.snapshot();
    assert_eq!(
        committed[&target],
        t(g(g(&proposal, "proposal"), "files"), &target).as_bytes()
    );
    if !stage.ends_with("after-hold-restore") {
        refused_preview(&f, &nested, "rollback");
        let preview = f.nested_preview(&nested, "finish");
        f.recover(&preview, 0);
    }
    assert_eq!(f.snapshot(), committed);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    if partial {
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
            hold
        );
        assert_eq!(g(&status, "pending_unit_ids"), &pending);
    } else {
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
    }
    assert_eq!(
        g(&status, "semantic_recovery_complete"),
        &Value::Bool(!partial)
    );
    let history = f.good(vec![
        "admission".into(),
        "apply".into(),
        "--proposal".into(),
        f.save(&proposal),
    ]);
    assert_eq!(g(&history, "already_applied"), &Value::Bool(true));
    assert_eq!(fs::read(audit).unwrap(), audit_bytes);
    assert_eq!(f.snapshot(), committed);
    for (p, mode) in old_modes {
        assert_eq!(fs::metadata(f.bank.join(p)).unwrap().mode() & 0o7777, mode);
    }
    assert_eq!(originals(&f), source);
    unchanged(&f, &work);
}

const SCRATCH: &str =
    "# Disposable scratch\n\nSynthetic temporary work-state; explicitly not durable guidance.\n";
fn ledger(f: &Fixture) -> (Value, Value) {
    let head = read(&f.directory.join("ledger-head.json"));
    let generation = g(&head, "generation").as_int().unwrap().as_u64().unwrap();
    let ledger = read(
        &f.directory
            .join("ledger")
            .join(format!("{generation:08}.json")),
    );
    (head, ledger)
}
fn discard_request(f: &Fixture) -> (String, Value) {
    let (head, ledger) = ledger(f);
    let unit = array(g(&ledger, "units"))
        .iter()
        .find(|u| t(u, "state") == "pending-admission")
        .unwrap();
    let id = t(unit, "unit_id").to_owned();
    let authority = obj([
        ("schema", s("agent-memory/layout-nonguidance-discard-v1")),
        ("review_id", s(uuid())),
        ("operation_id", s(&f.operation)),
        ("unit_id", s(&id)),
        ("manifest_sha256", g(&ledger, "manifest_sha256").clone()),
        ("unit_map_sha256", g(&ledger, "unit_map_sha256").clone()),
        ("ledger_head_sha256", g(&head, "sha256").clone()),
        ("source_ranges", g(unit, "source_ranges").clone()),
        ("author_harness", s("controlled-source-author")),
        ("author_session", s("scratch-author")),
        ("reviewer_developer_id", s(&f.actor)),
        ("reviewer_harness", s("controlled-independent-reviewer")),
        ("reviewer_session", s("scratch-reviewer")),
        ("review_time", g(&f.manifest, "inspected_at").clone()),
        ("non_guidance", Value::Bool(true)),
        (
            "reason",
            s(
                "Explicit controlled authority for disposable scratch work-state; not guidance or an age-based discard.",
            ),
        ),
        ("decision", s("discard")),
    ]);
    (id, obj([("discard_authority", authority)]))
}
fn discard(f: &Fixture) -> Value {
    let (id, request) = discard_request(f);
    f.private_layout(
        "resolve",
        vec![
            "--operation".into(),
            f.operation.clone().into(),
            "--unit".into(),
            id.into(),
            "--input".into(),
            f.save(&request),
        ],
    )
}
fn resolution_fault(stage: &str, mode: &str) {
    let f = Fixture::with_body(false, SCRATCH);
    let work = feature(&f);
    let source = originals(&f);
    let canonical = f.snapshot();
    let proposal = discard(&f);
    let resolution = t(g(&proposal, "proposal"), "resolution_id");
    fault_layout(&f, "resolve", &proposal, stage, mode);
    let path = f
        .directory
        .join("resolutions")
        .join(format!("{resolution}.json"));
    let receipt = fs::read(&path).unwrap();
    f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
    let history = f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
    assert_eq!(g(&history, "already_applied"), &Value::Bool(true));
    assert_eq!(g(&history, "writes_performed"), &Value::Bool(false));
    assert_eq!(fs::read(path).unwrap(), receipt);
    assert_eq!(f.snapshot(), canonical);
    assert_eq!(originals(&f), source);
    unchanged(&f, &work);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(g(&status, "semantic_recovery_complete"), &Value::Bool(true));
    assert!(array(g(&status, "pending_unit_ids")).is_empty());
    assert!(!f.bank.join(".agent-memory-transaction.md").exists());
    assert!(!f.bank.join(".agent-memory-recovery.md").exists());
}
fn revision_request(f: &Fixture, parent_id: &str) -> Value {
    let (head, ledger) = ledger(f);
    let map_hash = t(&ledger, "unit_map_sha256");
    let mut map = read(&f.directory.join("unit-map.json"));
    if sha(&map) != map_hash {
        map = read(
            &f.directory
                .join("unit-maps")
                .join(format!("{map_hash}.json")),
        );
    }
    let parent = array(g(&map, "units"))
        .iter()
        .find(|u| t(u, "unit_id") == parent_id)
        .unwrap()
        .clone();
    let span = &array(g(&parent, "source_ranges"))[0];
    let start = g(span, "start").as_int().unwrap().as_u64().unwrap() as usize;
    let end = g(span, "end").as_int().unwrap().as_u64().unwrap() as usize;
    let middle = (start + end) / 2;
    let children: Vec<_> = [(start, middle), (middle, end)]
        .into_iter()
        .map(|(a, b)| {
            let mut span = span.clone();
            set(&mut span, "start", n(a));
            set(&mut span, "end", n(b));
            obj([
                ("unit_id", s(uuid())),
                ("source_ranges", Value::Array(vec![span])),
                (
                    "semantic_rationale",
                    s("Controlled reviewed partition retains all original source conditions."),
                ),
                ("provenance", g(&parent, "provenance").clone()),
                ("reason", g(&parent, "reason").clone()),
                ("required_actions", g(&parent, "required_actions").clone()),
            ])
        })
        .collect();
    let replacements = Value::Array(vec![obj([
        ("parent_unit_id", s(parent_id)),
        ("source_ranges", g(&parent, "source_ranges").clone()),
        ("new_units", Value::Array(children.clone())),
        (
            "semantic_rationale",
            s("Controlled independent refinement of the original complete synthetic source."),
        ),
    ])]);
    let mut units = array(g(&map, "units"))
        .iter()
        .filter(|u| t(u, "unit_id") != parent_id)
        .cloned()
        .collect::<Vec<_>>();
    for child in children {
        let mut unit = parent.clone();
        for key in [
            "unit_id",
            "source_ranges",
            "provenance",
            "reason",
            "required_actions",
        ] {
            set(&mut unit, key, g(&child, key).clone());
        }
        units.push(unit);
    }
    set(&mut map, "units", Value::Array(units));
    let author = obj([
        ("developer_id", s(&f.actor)),
        ("harness", s("controlled-revision-author")),
        ("session", s("revision-author")),
        ("evidence", s("Controlled exact source review.")),
    ]);
    let mut reviewer = author.clone();
    set(
        &mut reviewer,
        "harness",
        s("controlled-independent-reviewer"),
    );
    set(&mut reviewer, "session", s("revision-reviewer"));
    let review = obj([
        ("schema", s("agent-memory/layout-unit-revision-review-v1")),
        ("review_id", s(uuid())),
        ("operation_id", s(&f.operation)),
        ("author", author),
        ("reviewer", reviewer),
        ("parent_unit_map_sha256", s(map_hash)),
        ("parent_ledger_head_sha256", s(sha(&head))),
        ("manifest_sha256", s(sha(&f.manifest))),
        ("replacements_sha256", s(sha(&replacements))),
        ("intended_unit_map_sha256", s(sha(&map))),
        ("reviewed_at", g(&f.manifest, "inspected_at").clone()),
        ("decision", s("approve")),
        (
            "rationale",
            s("Controlled full-source partition and native reservation review."),
        ),
        (
            "evidence",
            s("Every original source byte is retained in exactly one child."),
        ),
    ]);
    obj([
        ("schema", s("agent-memory/layout-unit-revision-request-v1")),
        ("operation_id", s(&f.operation)),
        ("replacements", replacements),
        ("review", review),
    ])
}
fn revision_fault(stage: Option<(&str, &str)>) {
    let f = Fixture::with_body(false, SCRATCH);
    let work = feature(&f);
    let source = originals(&f);
    let bank = bytes(&f.bank);
    let old_modes = modes(&f.bank);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    let parent = array(g(&status, "pending_unit_ids"))[0].as_str().unwrap();
    let request = revision_request(&f, parent);
    let head_before = fs::read(f.directory.join("ledger-head.json")).unwrap();
    let head = read(&f.directory.join("ledger-head.json"));
    let generation = g(&head, "generation").as_int().unwrap().as_u64().unwrap() + 1;
    if stage.is_none() {
        for delta in [-1i128, 1] {
            let mut invalid = request.clone();
            let row = invalid
                .get_mut("replacements")
                .unwrap()
                .as_array_mut()
                .unwrap()[0]
                .get_mut("new_units")
                .unwrap()
                .as_array_mut()
                .unwrap()[1]
                .get_mut("source_ranges")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .first_mut()
                .unwrap();
            let start = g(row, "start").as_int().unwrap().as_i128().unwrap();
            set(row, "start", Value::Int((start + delta).into()));
            let hash = sha(g(&invalid, "replacements"));
            set(
                invalid.get_mut("review").unwrap(),
                "replacements_sha256",
                s(hash),
            );
            let output = f.world.inside(format!("invalid-{delta}.json")).unwrap();
            f.layout(
                "revise-units",
                vec![
                    "--operation".into(),
                    f.operation.clone().into(),
                    "--input".into(),
                    f.save(&invalid),
                    "--output".into(),
                    output.clone().into_os_string(),
                ],
                1,
            );
            assert!(!output.exists());
            assert_eq!(bytes(&f.bank), bank);
            assert_eq!(
                fs::read(f.directory.join("ledger-head.json")).unwrap(),
                head_before
            );
        }
    }
    let proposal = f.private_layout(
        "revise-units",
        vec![
            "--operation".into(),
            f.operation.clone().into(),
            "--input".into(),
            f.save(&request),
        ],
    );
    if let Some((stage, mode)) = stage {
        let index = f.directory.parent().unwrap().join("index.json");
        let index_before = fs::read(&index).unwrap();
        fault_layout(&f, "revise-units", &proposal, stage, mode);
        if stage.ends_with("before-head") {
            assert_eq!(
                fs::read(f.directory.join("ledger-head.json")).unwrap(),
                head_before
            );
            assert!(
                !f.directory
                    .join("ledger")
                    .join(format!("{generation:08}.json"))
                    .exists()
            );
            assert!(
                !f.directory
                    .join("ledger-transitions")
                    .join(format!("{generation:08}.json"))
                    .exists()
            );
        } else {
            assert_eq!(
                g(&read(&f.directory.join("ledger-head.json")), "generation")
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                generation
            );
            assert_eq!(fs::read(index).unwrap(), index_before);
        }
    }
    let applied = f.layout(
        "revise-units",
        vec!["--proposal".into(), f.save(&proposal)],
        0,
    );
    assert_eq!(g(&applied, "verified"), &Value::Bool(true));
    let retained = bytes(&f.directory);
    let history = f.layout(
        "revise-units",
        vec!["--proposal".into(), f.save(&proposal)],
        0,
    );
    assert_eq!(g(&history, "already_applied"), &Value::Bool(true));
    assert_eq!(g(&history, "receipt_is_historical"), &Value::Bool(true));
    assert_eq!(bytes(&f.directory), retained);
    let (_, ledger) = ledger(&f);
    let old = array(g(&ledger, "units"))
        .iter()
        .find(|u| t(u, "unit_id") == parent)
        .unwrap();
    assert_eq!(t(old, "state"), "superseded-by-revision");
    let children: std::collections::BTreeSet<_> =
        array(g(&array(g(&request, "replacements"))[0], "new_units"))
            .iter()
            .map(|u| t(u, "unit_id"))
            .collect();
    let actual: std::collections::BTreeSet<_> = array(g(old, "child_unit_ids"))
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(actual, children);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(
        array(g(&status, "pending_unit_ids"))
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        children
    );
    assert_eq!(
        g(&status, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    assert_eq!(bytes(&f.bank), bank);
    assert_eq!(modes(&f.bank), old_modes);
    assert_eq!(originals(&f), source);
    unchanged(&f, &work);
}

fn compare(
    f: &Fixture,
    operation: &str,
    request: &Value,
    proof: Option<&Value>,
    expected: i32,
) -> (Value, Option<Value>) {
    let output = f
        .world
        .inside(format!("comparison-{}.json", uuid()))
        .unwrap();
    let mut args = vec![
        "compare".into(),
        operation.into(),
        "--repo".into(),
        f.repo.clone().into_os_string(),
        "--request".into(),
        f.save(request),
        "--private-output".into(),
        output.clone().into_os_string(),
    ];
    if let Some(proof) = proof {
        args.extend(["--proof".into(), f.save(proof)]);
    }
    let (code, envelope) = f.call(args, None);
    assert_eq!(code, expected, "{envelope:?}");
    let private = if output.exists() {
        Some(read(&output))
    } else {
        None
    };
    (envelope, private)
}
fn comparison_evidence(f: &Fixture, full: bool) -> (Value, Value, Value) {
    let (proposal, _, _) = f.admission(full);
    let evidence = g(g(&proposal, "proposal"), "admission_evidence");
    (
        proposal.clone(),
        g(evidence, "comparison_request").clone(),
        g(evidence, "comparison_proof").clone(),
    )
}
fn frontier_proof(kind: &str) {
    let f = Fixture::new();
    let source = originals(&f);
    let (proposal, request, proof) = comparison_evidence(&f, true);
    compare(&f, "check", &request, Some(&proof), 0);
    let bank = bytes(&f.bank);
    let private = bytes(&f.directory);
    match kind {
        "lexical" => {
            let mut lexical = request.clone();
            set(
                &mut lexical,
                "searches",
                Value::Array(vec![
                    s("unrelated lexical sentinel"),
                    s("distinct unrelated search"),
                ]),
            );
            let (_, view) = compare(&f, "inspect", &lexical, None, 0);
            let view = view.unwrap();
            let units: std::collections::BTreeSet<_> = array(g(&view, "pending_matches"))
                .iter()
                .map(|r| t(r, "unit_id"))
                .collect();
            assert_eq!(units.len(), 1);
            for omit in [true, false] {
                let mut altered = proof.clone();
                if omit {
                    set(
                        altered.get_mut("judgment").unwrap(),
                        "pending_decisions",
                        Value::Array(vec![]),
                    );
                } else {
                    set(
                        altered
                            .get_mut("judgment")
                            .unwrap()
                            .get_mut("observation")
                            .unwrap()
                            .get_mut("recovery_frontier")
                            .unwrap(),
                        "ledger_heads",
                        Value::Array(vec![]),
                    );
                }
                let (result, _) = compare(&f, "check", &request, Some(&altered), 1);
                assert_eq!(g(&result, "comparison_read_complete"), &Value::Bool(false));
            }
        }
        "missing-review" => {
            let mut bad = proof.clone();
            set(
                bad.get_mut("judgment")
                    .unwrap()
                    .get_mut("observation")
                    .unwrap()
                    .get_mut("recovery_frontier")
                    .unwrap(),
                "review_digest",
                Value::Null,
            );
            compare(&f, "check", &request, Some(&bad), 1);
        }
        "foreign-generation" => {
            let mut foreign = proof.clone();
            let row = &mut foreign
                .get_mut("judgment")
                .unwrap()
                .get_mut("pending_decisions")
                .unwrap()
                .as_array_mut()
                .unwrap()[0];
            let id = uuid();
            set(row, "unit_id", s(&id));
            set(row.get_mut("coverage_claim").unwrap(), "unit_id", s(id));
            compare(&f, "check", &request, Some(&foreign), 1);
            assert_eq!(bytes(&f.bank), bank);
            assert_eq!(bytes(&f.directory), private);
            let parent = array(g(
                &f.layout(
                    "status",
                    vec!["--operation".into(), f.operation.clone().into()],
                    0,
                ),
                "pending_unit_ids",
            ))[0]
                .as_str()
                .unwrap()
                .to_owned();
            let revised = f.private_layout(
                "revise-units",
                vec![
                    "--operation".into(),
                    f.operation.clone().into(),
                    "--input".into(),
                    f.save(&revision_request(&f, &parent)),
                ],
            );
            f.layout(
                "revise-units",
                vec!["--proposal".into(), f.save(&revised)],
                0,
            );
            compare(&f, "check", &request, Some(&proof), 1);
            let (code, _) = f.call(
                vec![
                    "admission".into(),
                    "apply".into(),
                    "--proposal".into(),
                    f.save(&proposal),
                ],
                None,
            );
            assert_eq!(code, 1);
            assert_eq!(bytes(&f.bank), bank);
            let (fresh, _, _) = f.admission(false);
            f.good(vec![
                "admission".into(),
                "apply".into(),
                "--proposal".into(),
                f.save(&fresh),
            ]);
            let status = f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0,
            );
            assert_eq!(array(g(&status, "pending_unit_ids")).len(), 2);
            assert_eq!(
                g(&status, "semantic_recovery_complete"),
                &Value::Bool(false)
            );
        }
        "index" => {
            let index = f.directory.parent().unwrap().join("index.json");
            let original = fs::read(&index).unwrap();
            fs::write(&index, b"{\"corrupt\": true}\n").unwrap();
            compare(&f, "inspect", &request, None, 1);
            assert_eq!(bytes(&f.bank), bank);
            fs::write(index, original).unwrap();
            let (_, view) = compare(&f, "inspect", &request, None, 0);
            assert_eq!(array(g(&view.unwrap(), "pending_matches")).len(), 1);
        }
        _ => panic!("Unknown frontier mutation"),
    }
    if !["foreign-generation", "index"].contains(&kind) {
        assert_eq!(bytes(&f.bank), bank);
        assert_eq!(bytes(&f.directory), private);
    }
    assert_eq!(originals(&f), source);
}
fn backup_dependency(kind: &str) {
    let f = if kind == "resolution" {
        Fixture::with_body(false, SCRATCH)
    } else {
        Fixture::new()
    };
    let source = originals(&f);
    let receipt = fs::read(f.directory.join("receipt.json")).unwrap();
    let child = f.private_layout(
        "rollback",
        vec!["--operation".into(), f.operation.clone().into()],
    );
    let child_id = t(g(&child, "proposal"), "operation_id").to_owned();
    assert_ne!(child_id, f.operation);
    let child_dir = f.directory.parent().unwrap().join(&child_id);
    if kind == "control" {
        let pending = g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0,
            ),
            "pending_unit_ids",
        )
        .clone();
        f.layout("rollback", vec!["--proposal".into(), f.save(&child)], 0);
        assert_eq!(f.snapshot(), source);
        assert!(f.bank.join(".agent-memory-transaction.md").exists());
        assert!(f.bank.join(".agent-memory-recovery.md").exists());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(g(&status, "pending_unit_ids"), &pending);
    } else {
        let canonical = f.snapshot();
        if kind == "admission" {
            let (p, nested, target) = f.admission(true);
            f.good(vec![
                "admission".into(),
                "apply".into(),
                "--proposal".into(),
                f.save(&p),
            ]);
            assert_eq!(
                fs::read(f.bank.join(&target)).unwrap(),
                t(g(g(&p, "proposal"), "files"), &target).as_bytes()
            );
            let (_, ledger) = ledger(&f);
            let unit = array(g(&ledger, "units"))
                .iter()
                .find(|u| t(u, "state") == "admitted")
                .unwrap();
            let link = &array(g(unit, "terminal_receipts"))[0];
            let receipt = read(
                &f.directory
                    .join("resolutions")
                    .join(format!("{}.json", t(link, "resolution_id"))),
            );
            assert_eq!(t(&receipt, "canonical_operation_id"), nested);
        } else {
            let p = discard(&f);
            f.layout("resolve", vec!["--proposal".into(), f.save(&p)], 0);
            assert_eq!(f.snapshot(), canonical);
        }
        let before = bytes(&f.bank);
        let private = bytes(&f.directory);
        let child_bytes = bytes(&child_dir);
        f.layout("rollback", vec!["--proposal".into(), f.save(&child)], 1);
        let out = f.world.inside("refused-fresh-restore.json").unwrap();
        f.layout(
            "rollback",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--output".into(),
                out.clone().into_os_string(),
            ],
            1,
        );
        assert!(!out.exists());
        assert_eq!(bytes(&f.bank), before);
        assert_eq!(bytes(&f.directory), private);
        assert_eq!(bytes(&child_dir), child_bytes);
    }
    assert_eq!(originals(&f), source);
    assert_eq!(fs::read(f.directory.join("receipt.json")).unwrap(), receipt);
}
fn semantic_discard(historical: bool) {
    let f = Fixture::with_body(false, SCRATCH);
    let source = originals(&f);
    let before = bytes(&f.bank);
    let private = bytes(&f.directory);
    let (id, mut request) = discard_request(&f);
    if !historical {
        let authority = request.get_mut("discard_authority").unwrap();
        let harness = g(authority, "author_harness").clone();
        let session = g(authority, "author_session").clone();
        set(authority, "reviewer_harness", harness);
        set(authority, "reviewer_session", session);
        let out = f.world.inside("refused-discard.json").unwrap();
        f.layout(
            "resolve",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--unit".into(),
                id.clone().into(),
                "--input".into(),
                f.save(&request),
                "--output".into(),
                out.clone().into_os_string(),
            ],
            1,
        );
        assert!(!out.exists());
        assert_eq!(bytes(&f.bank), before);
        assert_eq!(bytes(&f.directory), private);
        assert!(
            array(g(
                &f.layout(
                    "status",
                    vec!["--operation".into(), f.operation.clone().into()],
                    0
                ),
                "pending_unit_ids"
            ))
            .contains(&s(&id))
        );
    }
    let proposal = discard(&f);
    let first = f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
    assert_eq!(g(&first, "semantic_recovery_complete"), &Value::Bool(true));
    if historical {
        let p = f.bank.join("projectbrief.md");
        let mut later = fs::read(&p).unwrap();
        later.extend_from_slice(b"\nLater independently retained context.\n");
        fs::write(&p, &later).unwrap();
        let head = fs::read(f.directory.join("ledger-head.json")).unwrap();
        let history = f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
        assert_eq!(g(&history, "already_applied"), &Value::Bool(true));
        assert_eq!(g(&history, "receipt_is_historical"), &Value::Bool(true));
        assert_eq!(g(&history, "writes_performed"), &Value::Bool(false));
        assert_eq!(fs::read(p).unwrap(), later);
        assert_eq!(
            fs::read(f.directory.join("ledger-head.json")).unwrap(),
            head
        );
    }
    assert_eq!(originals(&f), source);
}

include!("fanout_c_policies.rs");
fn bind_review(request: &mut Value) {
    for (key, field) in [
        ("unit_map", "unit_map_sha256"),
        ("intended_files", "intended_files_sha256"),
        ("metadata_plan", "metadata_plan_sha256"),
    ] {
        let hash = sha(g(request, key));
        set(request.get_mut("review").unwrap(), field, s(hash));
    }
    let decisions = array(g(g(request, "unit_map"), "units"))
        .iter()
        .map(|u| {
            obj([
                ("unit_id", g(u, "unit_id").clone()),
                ("disposition", g(u, "disposition").clone()),
                ("rationale", g(u, "reason").clone()),
            ])
        })
        .collect();
    set(
        request.get_mut("review").unwrap(),
        "unit_decisions",
        Value::Array(decisions),
    );
}
fn partition_request(request: &mut Value, ranges: &[(usize, usize)]) {
    let units = array(g(g(request, "unit_map"), "units"));
    let pending = units
        .iter()
        .find(|u| t(u, "disposition") == "pending-admission")
        .unwrap()
        .clone();
    let mut rows = units
        .iter()
        .filter(|u| t(u, "disposition") != "pending-admission")
        .cloned()
        .collect::<Vec<_>>();
    for (start, end) in ranges {
        let mut unit = pending.clone();
        set(&mut unit, "unit_id", s(uuid()));
        let span = unit
            .get_mut("source_ranges")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .first_mut()
            .unwrap();
        set(span, "start", n(*start));
        set(span, "end", n(*end));
        rows.push(unit);
    }
    set(
        request.get_mut("unit_map").unwrap(),
        "units",
        Value::Array(rows),
    );
    bind_review(request);
}
fn reported_fixture() -> Fixture {
    let mut body = String::new();
    for (i, policy) in POLICIES.iter().enumerate() {
        let size = if i == 7 { 3558 } else { 3560 };
        let mut part = format!("# Synthetic policy {}\n\n{policy}\n\n", i + 1);
        let sentence = format!("{policy}\n");
        while part.len() + sentence.len() <= size {
            part.push_str(&sentence);
        }
        part.push_str(&" ".repeat(size - part.len()));
        body.push_str(&part);
    }
    assert_eq!(body.len(), 28478);
    Fixture::configured(
        true,
        &body,
        |f| {
            fs::write(
                f.repo.join("synthetic-policy.md"),
                format!(
                    "# Independent synthetic source\n\n{}\n",
                    POLICIES.join("\n\n")
                ),
            )
            .unwrap();
            let sentence = "Under the synthetic review condition, preserve existing guidance and check the original source before acting.\n";
            let mut large = "# Active context\n\n".to_owned();
            while large.len() + sentence.len() <= 79054 {
                large.push_str(sentence);
            }
            large.push_str(&" ".repeat(79054 - large.len()));
            fs::write(f.bank.join("activeContext.md"), large).unwrap();
        },
        |req| {
            partition_request(
                req,
                &(0..8)
                    .map(|i| (i * 3560, if i == 7 { 28478 } else { (i + 1) * 3560 }))
                    .collect::<Vec<_>>(),
            )
        },
    )
}
fn eight_admission(partial: bool) {
    let f = reported_fixture();
    let work = feature(&f);
    let source = originals(&f);
    assert_eq!(source["handoff-prompt.md"].len(), 28478);
    let initial_head = read(&f.directory.join("ledger-head.json"));
    let initial = read(&f.directory.join("ledger/00000000.json"));
    let pending: std::collections::BTreeSet<_> = array(g(&initial, "units"))
        .iter()
        .filter(|u| t(u, "state") == "pending-admission")
        .map(|u| t(u, "unit_id").to_owned())
        .collect();
    assert_eq!(pending.len(), 8);
    let unit = array(g(&initial, "units"))
        .iter()
        .find(|u| {
            array(g(u, "source_ranges"))
                .iter()
                .any(|span| g(span, "start").as_int().unwrap().as_u64().unwrap() == 0)
                && t(u, "state") == "pending-admission"
        })
        .unwrap();
    let selected = if partial {
        Some(t(unit, "unit_id"))
    } else {
        None
    };
    let body = if partial {
        POLICY.to_owned()
    } else {
        format!("# Synthetic policies\n\n{}\n", POLICIES.join("\n\n"))
    };
    let before = f.snapshot();
    let (proposal, nested, target) = f.admission_selected(true, selected, &body);
    let decisions = g(
        g(
            g(g(&proposal, "proposal"), "admission_evidence"),
            "comparison_proof",
        ),
        "judgment",
    );
    let credited: std::collections::BTreeSet<_> = array(g(decisions, "pending_decisions"))
        .iter()
        .filter(|u| t(g(u, "coverage_claim"), "coverage") == "full")
        .map(|u| t(u, "unit_id").to_owned())
        .collect();
    assert_eq!(credited.len(), if partial { 1 } else { 8 });
    let result = f.good(vec![
        "admission".into(),
        "apply".into(),
        "--proposal".into(),
        f.save(&proposal),
    ]);
    assert_eq!(t(&result, "operation_id"), nested);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(
        g(&status, "semantic_recovery_complete"),
        &Value::Bool(!partial)
    );
    let remaining: std::collections::BTreeSet<_> = array(g(&status, "pending_unit_ids"))
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(remaining, pending.difference(&credited).cloned().collect());
    let (head, ledger) = ledger(&f);
    assert_ne!(head, initial_head);
    let mut receipts = std::collections::BTreeSet::new();
    for unit in array(g(&ledger, "units")) {
        let old = array(g(&initial, "units"))
            .iter()
            .find(|u| t(u, "unit_id") == t(unit, "unit_id"))
            .unwrap();
        assert_eq!(g(unit, "source_ranges"), g(old, "source_ranges"));
        assert_eq!(g(unit, "provenance"), g(old, "provenance"));
        if credited.contains(t(unit, "unit_id")) {
            assert_eq!(t(unit, "state"), "admitted");
            let links = array(g(unit, "terminal_receipts"));
            assert_eq!(links.len(), 1);
            let link = &links[0];
            receipts.insert(t(link, "resolution_id").to_owned());
            let receipt = read(
                &f.directory
                    .join("resolutions")
                    .join(format!("{}.json", t(link, "resolution_id"))),
            );
            assert_eq!(sha(&receipt), t(link, "sha256"));
            assert_eq!(t(&receipt, "canonical_operation_id"), nested);
            assert_eq!(g(&receipt, "unit_id"), g(unit, "unit_id"));
        } else {
            assert_eq!(g(unit, "state"), g(old, "state"));
        }
    }
    assert_eq!(receipts.len(), credited.len());
    assert_eq!(originals(&f), source);
    for (path, body) in before {
        if path != "MEMORY.md" {
            assert_eq!(fs::read(f.bank.join(path)).unwrap(), body);
        }
    }
    let raw = fs::read_to_string(f.bank.join(&target)).unwrap();
    for policy in if partial {
        &POLICIES[..1]
    } else {
        &POLICIES[..]
    } {
        assert!(raw.contains(policy));
    }
    if partial {
        let mut changed = proposal.clone();
        let files = changed
            .get_mut("proposal")
            .unwrap()
            .get_mut("files")
            .unwrap();
        let body = t(files, &target).to_owned() + "\nDifferent requested content.\n";
        set(files, &target, s(body));
        let before = bytes(&f.bank);
        let (code, _) = f.call(
            vec![
                "admission".into(),
                "apply".into(),
                "--proposal".into(),
                f.save(&changed),
            ],
            None,
        );
        assert_eq!(code, 1);
        assert_eq!(bytes(&f.bank), before);
    }
    unchanged(&f, &work);
}
fn recall(f: &Fixture, expected: i32) -> Value {
    let request = obj([
        ("project_id", s(&f.project)),
        ("query", s("")),
        ("ids", Value::Array(vec![])),
        ("paths", Value::Array(vec![])),
        ("context", obj([])),
        ("kinds", Value::Array(vec![])),
        ("minimum_trust", s("reviewed")),
        ("limit", n(8)),
        ("include_context", Value::Bool(true)),
    ]);
    let (code, value) = f.call(
        vec![
            "search".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            f.save(&request),
        ],
        None,
    );
    assert_eq!(code, expected, "{value:?}");
    value
}
fn reported_frontier(kind: &str) {
    let f = reported_fixture();
    let source = originals(&f);
    let pending = g(
        &f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        ),
        "pending_unit_ids",
    )
    .clone();
    assert_eq!(array(&pending).len(), 8);
    match kind {
        "recall" => {
            assert_eq!(fs::metadata(&f.directory).unwrap().mode() & 0o7777, 0o700);
            for row in array(g(&f.manifest, "sources")) {
                assert_eq!(
                    fs::metadata(f.directory.join(t(row, "blob")))
                        .unwrap()
                        .mode()
                        & 0o7777,
                    0o600
                );
            }
            let value = recall(&f, 0);
            assert_eq!(t(&value, "semantic_coverage"), "incomplete-recovery");
            assert_eq!(g(&value, "pending_recovery_units"), &n(8));
            assert!(!array(g(&value, "results")).is_empty());
            let (_, request, _) = comparison_evidence(&f, false);
            let (code, envelope) = f.call(
                vec![
                    "compare".into(),
                    "inspect".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                    "--request".into(),
                    f.save(&request),
                ],
                None,
            );
            assert_eq!(code, 1);
            assert!(envelope.get("pending_matches").is_none());
            let (envelope, view) = compare(&f, "inspect", &request, None, 0);
            assert!(envelope.get("pending_matches").is_none());
            let view = view.unwrap();
            assert_eq!(array(g(&view, "pending_matches")).len(), 8);
            let public = String::from_utf8(encoded(&envelope).unwrap()).unwrap();
            for row in array(g(&view, "pending_matches")) {
                assert!(!public.contains(t(row, "body")));
            }
        }
        "sentinel" => {
            fs::remove_file(f.bank.join(".agent-memory-recovery.md")).unwrap();
            let value = recall(&f, 1);
            assert!(value.get("results").is_none());
            assert!(f.bank.join(".agent-memory-transaction.md").exists());
        }
        "rollback" => {
            let child = f.private_layout(
                "rollback",
                vec!["--operation".into(), f.operation.clone().into()],
            );
            let result = f.layout("rollback", vec!["--proposal".into(), f.save(&child)], 0);
            assert_eq!(
                g(&result, "semantic_recovery_complete"),
                &Value::Bool(false)
            );
            assert!(f.bank.join(".agent-memory-transaction.md").exists());
            assert!(f.bank.join(".agent-memory-recovery.md").exists());
            let status = f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0,
            );
            assert_eq!(g(&status, "pending_unit_ids"), &pending);
            let (code, _) = f.call(
                vec![
                    "bank".into(),
                    "validate".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                ],
                None,
            );
            assert_eq!(code, 1);
        }
        _ => panic!("Unknown reported fixture"),
    }
    assert_eq!(originals(&f), source);
}

fn copy_tree(source: &Path, target: &Path) {
    let meta = fs::symlink_metadata(source).unwrap();
    if meta.is_dir() {
        fs::create_dir(target).unwrap();
        fs::set_permissions(target, meta.permissions()).unwrap();
        for e in fs::read_dir(source).unwrap() {
            let p = e.unwrap().path();
            copy_tree(&p, &target.join(p.file_name().unwrap()));
        }
    } else if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(source).unwrap(), target).unwrap();
    } else {
        fs::copy(source, target).unwrap();
    }
}
fn discovery_loss(kind: &str) {
    let mut f = Fixture::new();
    let bank = bytes(&f.bank);
    let private = bytes(&f.directory);
    let config = PathBuf::from(
        f.world
            .env
            .get(std::ffi::OsStr::new("AGENT_MEMORY_CONFIG_HOME"))
            .unwrap(),
    );
    match kind {
        "index" => {
            fs::remove_file(f.directory.parent().unwrap().join("index.json")).unwrap();
        }
        "all" => {
            for name in [".agent-memory-transaction.md", ".agent-memory-recovery.md"] {
                fs::remove_file(f.bank.join(name)).unwrap();
            }
            fs::remove_file(config.join("layout-recovery-bindings.json")).unwrap();
            fs::remove_dir_all(f.directory.parent().unwrap()).unwrap();
            let value = recall(&f, 0);
            assert_ne!(
                value.get("semantic_coverage"),
                Some(&s("incomplete-recovery"))
            );
            assert!(
                value
                    .get("pending_recovery_units")
                    .is_none_or(|v| v == &n(0))
            );
            return;
        }
        "copy" | "move" => {
            let next = f.world.inside(format!("{kind}-checkout")).unwrap();
            let old = fs::metadata(f.repo.join(".git")).unwrap();
            if kind == "copy" {
                copy_tree(&f.repo, &next);
            } else {
                fs::rename(&f.repo, &next).unwrap();
            }
            let target = f.world.inside(format!("{kind}-config")).unwrap();
            copy_tree(&config, &target);
            fs::remove_file(target.join("layout-recovery-bindings.json")).unwrap();
            let path = target.join("projects.json");
            let mut data = read(&path);
            let old_key = format!("{}:{}", old.dev(), old.ino());
            let info = fs::metadata(next.join(".git")).unwrap();
            let new_key = format!("{}:{}", info.dev(), info.ino());
            let mut entry = g(g(&data, "checkouts"), &old_key).clone();
            entry
                .get_mut("locations")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(s(next.to_str().unwrap()));
            set(data.get_mut("checkouts").unwrap(), &new_key, entry);
            super::private(&path, &data);
            f.world
                .env
                .insert("AGENT_MEMORY_CONFIG_HOME".into(), target.into_os_string());
            f.repo = next.clone();
            f.bank = next.join("memory-bank");
            f.world.paths.insert("repo", next);
        }
        "state" | "xdg-state" => {
            let root = f.world.inside(format!("other-{kind}")).unwrap();
            if kind == "state" {
                f.world
                    .env
                    .insert("AGENT_MEMORY_STATE_HOME".into(), root.into_os_string());
            } else {
                f.world
                    .env
                    .remove(std::ffi::OsStr::new("AGENT_MEMORY_STATE_HOME"));
                f.world
                    .env
                    .insert("XDG_STATE_HOME".into(), root.into_os_string());
            }
        }
        "config" | "xdg-config" => {
            let root = f.world.inside(format!("other-{kind}")).unwrap();
            let target = if kind == "config" {
                root.clone()
            } else {
                fs::create_dir(&root).unwrap();
                root.join("agent-memory")
            };
            copy_tree(&config, &target);
            if kind == "config" {
                f.world.env.insert(
                    "AGENT_MEMORY_CONFIG_HOME".into(),
                    target.clone().into_os_string(),
                );
            } else {
                f.world
                    .env
                    .remove(std::ffi::OsStr::new("AGENT_MEMORY_CONFIG_HOME"));
                f.world
                    .env
                    .insert("XDG_CONFIG_HOME".into(), root.into_os_string());
            }
            let positive = recall(&f, 0);
            assert_eq!(t(&positive, "semantic_coverage"), "incomplete-recovery");
            fs::remove_file(target.join("layout-recovery-bindings.json")).unwrap();
        }
        _ => panic!("Unknown discovery fixture"),
    }
    let value = recall(&f, 1);
    assert!(value.get("results").is_none());
    assert_eq!(bytes(&f.bank), bank);
    if kind == "index" {
        assert_eq!(bytes(&f.directory), private);
    }
}
fn pending_disclosure(approval: bool) {
    let body = if approval {
        "# Approved policy\n\nVersion 1 allowed the synthetic action. Version 2 may prohibit it; the current authority and contradiction are unknown.\n"
    } else {
        "# Confidential fixture\n\nUnique private source evidence 874209 remains pending.\n"
    };
    let f = Fixture::with_body(false, body);
    let (_, request, _) = comparison_evidence(&f, false);
    let (envelope, view) = compare(&f, "inspect", &request, None, 0);
    let view = view.unwrap();
    let rows = array(g(&view, "pending_matches"));
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(t(row, "body"), body);
    let public = String::from_utf8(encoded(&envelope).unwrap()).unwrap();
    let audit = fs::read_to_string(t(g(&view, "audit"), "path")).unwrap();
    let original = array(g(&f.manifest, "sources"))
        .iter()
        .find(|r| t(r, "path") == "handoff-prompt.md")
        .unwrap();
    for secret in [
        body.trim(),
        t(original, "sha256"),
        t(&array(g(row, "source_ranges"))[0], "source_id"),
        "source_ranges",
    ] {
        assert!(!public.contains(secret));
        assert!(!audit.contains(secret));
    }
    if approval {
        assert_eq!(g(row, "source_assertions_verified"), &Value::Bool(false));
        assert_eq!(
            t(row, "selection_class"),
            "comparison-only-not-current-guidance"
        );
        assert!(!f.bank.join("handoff-prompt.md").exists());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(
            g(&status, "semantic_recovery_complete"),
            &Value::Bool(false)
        );
        assert_eq!(
            array(g(&status, "pending_unit_ids")),
            &[g(row, "unit_id").clone()]
        );
    }
}

fn nested_review_drift() {
    let f = Fixture::new();
    let work = feature(&f);
    let (proposal, nested, target) = f.admission(true);
    f.fault_admission(&proposal, "layout-recovery-nested-after-receipt", "error");
    let preview = f.nested_preview(&nested, "finish");
    let token = g(
        g(
            g(
                g(g(&proposal, "proposal"), "admission_evidence"),
                "comparison_proof",
            ),
            "judgment",
        ),
        "observation",
    )
    .get("recovery_frontier")
    .unwrap();
    let review = f
        .directory
        .join("coverage-reviews")
        .join(format!("{}.json", t(token, "review_digest")));
    let original = fs::read(&review).unwrap();
    assert_eq!(bytes_sha(&original), t(token, "review_digest"));
    let mut altered = read(&review);
    let rationale = t(&altered, "rationale").to_owned() + " Changed after recovery preview.";
    set(&mut altered, "rationale", s(rationale));
    let aside = f.world.inside("retained-coverage-review.json").unwrap();
    for missing in [false, true] {
        if missing {
            fs::rename(&review, &aside).unwrap();
        } else {
            private(&review, &altered);
        }
        let before = bytes(&f.bank);
        let retained = bytes(&f.directory);
        f.recover(&preview, 1);
        assert_eq!(bytes(&f.bank), before);
        assert_eq!(bytes(&f.directory), retained);
        unchanged(&f, &work);
        if missing {
            fs::rename(&aside, &review).unwrap();
        } else {
            fs::write(&review, &original).unwrap();
        }
    }
    f.recover(&preview, 0);
    assert_eq!(
        fs::read(f.bank.join(&target)).unwrap(),
        t(g(g(&proposal, "proposal"), "files"), &target).as_bytes()
    );
    assert_eq!(
        g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0
            ),
            "semantic_recovery_complete"
        ),
        &Value::Bool(true)
    );
    private(&review, &altered);
    let bank = bytes(&f.bank);
    let retained = bytes(&f.directory);
    let retry = f.nested_preview(&nested, "finish");
    f.recover(&retry, 0);
    assert_eq!(bytes(&f.bank), bank);
    assert_eq!(bytes(&f.directory), retained);
    unchanged(&f, &work);
}
fn receipt_window() {
    for stage in [
        "layout-recovery-nested-after-receipt",
        "layout-recovery-admission-receipt-before-resolution",
    ] {
        let f = Fixture::new();
        let work = feature(&f);
        let (proposal, nested, target) = f.admission(true);
        f.fault_admission(&proposal, stage, "error");
        let marker = read(&f.bank.join(".agent-memory-transaction.md"));
        assert_eq!(t(&marker, "operation_id"), f.operation);
        assert_eq!(t(&marker, "nested_id"), nested);
        assert!(f.bank.join(".agent-memory-recovery.md").is_file());
        let canonical = f.snapshot();
        assert_eq!(
            canonical[&target],
            t(g(g(&proposal, "proposal"), "files"), &target).as_bytes()
        );
        let resolutions = f.directory.join("resolutions");
        let existing = if resolutions.exists() {
            bytes(&resolutions)
        } else {
            BTreeMap::new()
        };
        assert_eq!(
            existing.len(),
            if stage.ends_with("before-resolution") {
                1
            } else {
                0
            }
        );
        assert_eq!(
            g(&read(&f.directory.join("ledger-head.json")), "generation"),
            &n(0)
        );
        refused_preview(&f, &nested, "rollback");
        let preview = f.nested_preview(&nested, "finish");
        let journal = f
            .directory
            .join("nested")
            .join(&nested)
            .join("journal.json");
        let original = fs::read(&journal).unwrap();
        let mut value = read(&journal);
        let row = &mut value
            .get_mut("resolution_plan")
            .unwrap()
            .get_mut("ledgers")
            .unwrap()
            .as_array_mut()
            .unwrap()[0]
            .get_mut("resolutions")
            .unwrap()
            .as_array_mut()
            .unwrap()[0];
        set(row, "review_sha256", s("0".repeat(64)));
        private(&journal, &value);
        let before = bytes(&f.bank);
        let retained = bytes(&f.directory);
        f.recover(&preview, 1);
        assert_eq!(bytes(&f.bank), before);
        assert_eq!(bytes(&f.directory), retained);
        fs::write(&journal, original).unwrap();
        f.recover(&preview, 0);
        assert_eq!(f.snapshot(), canonical);
        let (_, ledger) = ledger(&f);
        let unit = array(g(&ledger, "units"))
            .iter()
            .find(|u| t(u, "state") == "admitted")
            .unwrap();
        let links = array(g(unit, "terminal_receipts"));
        assert_eq!(links.len(), 1);
        let receipt = read(&resolutions.join(format!("{}.json", t(&links[0], "resolution_id"))));
        assert_eq!(t(&receipt, "canonical_operation_id"), nested);
        assert_eq!(sha(&receipt), t(&links[0], "sha256"));
        for (name, body) in existing {
            assert_eq!(fs::read(resolutions.join(name)).unwrap(), body);
        }
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
        assert!(!f.bank.join(".agent-memory-recovery.md").exists());
        let before = bytes(&f.bank);
        let head = fs::read(f.directory.join("ledger-head.json")).unwrap();
        let retry = f.nested_preview(&nested, "finish");
        f.recover(&retry, 0);
        assert_eq!(bytes(&f.bank), before);
        assert_eq!(
            fs::read(f.directory.join("ledger-head.json")).unwrap(),
            head
        );
        unchanged(&f, &work);
    }
}
fn child_resolution_exclusion() {
    for child_first in [true, false] {
        let f = Fixture::with_body(false, SCRATCH);
        let work = feature(&f);
        let child = f.private_layout(
            "rollback",
            vec!["--operation".into(), f.operation.clone().into()],
        );
        let child_id = t(g(&child, "proposal"), "operation_id").to_owned();
        let resolution = discard(&f);
        let child_dir = f.directory.parent().unwrap().join(&child_id);
        if child_first {
            fault_layout(
                &f,
                "rollback",
                &child,
                "layout-recovery-before-marker-swap",
                "error",
            );
            assert_eq!(
                t(
                    &read(&f.bank.join(".agent-memory-transaction.md")),
                    "operation_id"
                ),
                f.operation
            );
            assert!(child_dir.join("journal.json").exists());
            let before = bytes(&f.bank);
            let retained = bytes(f.directory.parent().unwrap());
            let p = g(&resolution, "proposal");
            let out = f.world.inside("blocked-resolution.json").unwrap();
            f.layout(
                "resolve",
                vec![
                    "--operation".into(),
                    f.operation.clone().into(),
                    "--unit".into(),
                    g(p, "unit_id").as_str().unwrap().into(),
                    "--input".into(),
                    f.save(g(p, "terminal")),
                    "--output".into(),
                    out.clone().into_os_string(),
                ],
                1,
            );
            assert!(!out.exists());
            f.layout("resolve", vec!["--proposal".into(), f.save(&resolution)], 1);
            assert_eq!(bytes(&f.bank), before);
            assert_eq!(bytes(f.directory.parent().unwrap()), retained);
            let preview = f.private_layout(
                "recover",
                vec![
                    "--operation".into(),
                    child_id.into(),
                    "--mode".into(),
                    "finish".into(),
                ],
            );
            f.layout("recover", vec!["--proposal".into(), f.save(&preview)], 0);
        } else {
            fault_layout(
                &f,
                "resolve",
                &resolution,
                "layout-recovery-resolution-after-receipt",
                "error",
            );
            let id = t(g(&resolution, "proposal"), "resolution_id");
            assert!(
                f.directory
                    .join("resolutions")
                    .join(format!("{id}.json"))
                    .exists()
            );
            assert!(
                f.directory
                    .join("resolution-journals")
                    .join(format!("{id}.json"))
                    .exists()
            );
            assert!(
                !f.directory
                    .join("resolution-completions")
                    .join(format!("{id}.json"))
                    .exists()
            );
            assert!(!child_dir.join("journal.json").exists());
            let before = bytes(&f.bank);
            let retained = bytes(f.directory.parent().unwrap());
            f.layout("rollback", vec!["--proposal".into(), f.save(&child)], 1);
            let out = f.world.inside("blocked-child.json").unwrap();
            f.layout(
                "rollback",
                vec![
                    "--operation".into(),
                    f.operation.clone().into(),
                    "--output".into(),
                    out.clone().into_os_string(),
                ],
                1,
            );
            assert!(!out.exists());
            assert_eq!(bytes(&f.bank), before);
            assert_eq!(bytes(f.directory.parent().unwrap()), retained);
            assert!(!child_dir.join("journal.json").exists());
            f.layout("resolve", vec!["--proposal".into(), f.save(&resolution)], 0);
            f.layout("rollback", vec!["--proposal".into(), f.save(&child)], 1);
        }
        unchanged(&f, &work);
    }
}

include!("fanout_c_nested_cases.rs");

fn candidate_request(f: &Fixture) -> Value {
    let raw = record_fixtures::record_text_with_ids(
        POLICY,
        &f.project,
        record_fixtures::RecordOptions {
            status: "candidate",
            ..Default::default()
        },
        &uuid(),
        &f.actor,
    )
    .unwrap();
    let allowed = g(&agent_memory::records::MODEL, "properties")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>();
    let (metadata, _) = agent_memory::codec::frontmatter::read(&raw, &allowed).unwrap();
    obj([
        ("project_id", s(&f.project)),
        (
            "candidate",
            obj([("metadata", Value::Object(metadata)), ("body", s(POLICY))]),
        ),
        (
            "searches",
            Value::Array(vec![
                s("offline delivery queue"),
                s("retain items after failure"),
            ]),
        ),
        ("read_paths", Value::Array(vec![])),
    ])
}
fn capture_request(f: &Fixture) -> Value {
    let compared = candidate_request(f);
    let mut candidate = g(&compared, "candidate").clone();
    set(
        candidate.get_mut("metadata").unwrap(),
        "evidence",
        Value::Array(vec![s("synthetic-policy.md")]),
    );
    obj([
        ("project_id", s(&f.project)),
        ("candidate", candidate),
        ("source_class", s("verified-repository")),
        (
            "repository_evidence",
            obj([
                ("path", s("synthetic-policy.md")),
                (
                    "sha256",
                    s(bytes_sha(
                        &fs::read(f.repo.join("synthetic-policy.md")).unwrap(),
                    )),
                ),
            ]),
        ),
        (
            "assessment",
            obj([
                ("category", s("durable-discovery")),
                (
                    "utility",
                    s("Prevent repeated excessive retries under these conditions."),
                ),
                ("durable", Value::Bool(true)),
                ("evidence_confidence", s("confirmed")),
                (
                    "evidence_reason",
                    s("The independently readable synthetic repository policy confirms this."),
                ),
                ("confirmed", Value::Bool(true)),
                ("source_redundant", Value::Bool(false)),
                ("work_state_copy", Value::Bool(false)),
                (
                    "review",
                    s("Controlled review of actual independently readable repository evidence."),
                ),
            ]),
        ),
    ])
}
fn capture(f: &Fixture, request: &Value, expected: i32) -> Value {
    let (code, value) = f.call(
        vec![
            "capture".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            f.save(request),
        ],
        None,
    );
    assert_eq!(code, expected, "{value:?}");
    value
}
fn backup_capture() {
    let f = Fixture::new();
    let source = array(g(&f.manifest, "sources"))
        .iter()
        .find(|r| t(r, "path") == "handoff-prompt.md")
        .unwrap();
    let backup = f.directory.join(t(source, "blob"));
    let original = fs::read(&backup).unwrap();
    assert_eq!(
        original,
        fs::read(f.repo.join("synthetic-policy.md")).unwrap()
    );
    let request = capture_request(&f);
    assert_eq!(t(&capture(&f, &request, 0), "decision"), "eligible");
    let alias = f.repo.join("backup-evidence.md");
    std::os::unix::fs::symlink(&backup, &alias).unwrap();
    let bank = bytes(&f.bank);
    let private = bytes(&f.directory);
    let index = fs::read(f.repo.join(".git/index")).ok();
    for path in [backup.to_str().unwrap(), "backup-evidence.md"] {
        let mut bad = request.clone();
        set(bad.get_mut("repository_evidence").unwrap(), "path", s(path));
        set(
            bad.get_mut("candidate")
                .unwrap()
                .get_mut("metadata")
                .unwrap(),
            "evidence",
            Value::Array(vec![s(path)]),
        );
        capture(&f, &bad, 1);
        assert_eq!(bytes(&f.bank), bank);
        assert_eq!(bytes(&f.directory), private);
        assert_eq!(fs::read(f.repo.join(".git/index")).ok(), index);
        assert_eq!(fs::read(&backup).unwrap(), original);
        assert!(
            fs::symlink_metadata(&alias)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
    assert_eq!(
        g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0
            ),
            "semantic_recovery_complete"
        ),
        &Value::Bool(false)
    );
}
fn empty_frontier() {
    let f = Fixture::with_body(false, SCRATCH);
    let discard = discard(&f);
    f.layout("resolve", vec!["--proposal".into(), f.save(&discard)], 0);
    let before = bytes(&f.bank);
    let request = candidate_request(&f);
    let (_, view) = compare(&f, "inspect", &request, None, 0);
    let view = view.unwrap();
    assert_eq!(t(&view, "schema"), "agent-memory/comparison-result-v2");
    let observation = g(&view, "observation");
    assert_eq!(
        t(observation, "schema"),
        "agent-memory/comparison-observation-v2"
    );
    assert_eq!(
        g(g(observation, "recovery_frontier"), "coverage_complete"),
        &Value::Bool(true)
    );
    assert!(array(g(g(observation, "recovery_frontier"), "unit_ids")).is_empty());
    assert!(array(g(&view, "pending_matches")).is_empty());
    let judgment = obj([
        ("observation", observation.clone()),
        (
            "decisions",
            Value::Array(
                array(g(&view, "matches"))
                    .iter()
                    .map(|r| {
                        obj([
                            ("path", g(r, "path").clone()),
                            ("classification", s("distinct-knowledge")),
                            ("rationale", s("Complete independent source comparison.")),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("pending_decisions", Value::Array(vec![])),
        ("rationale", s("Complete source-bound comparison.")),
    ]);
    let review = |judgment: &Value, expected: i32| {
        let out = f
            .world
            .inside(format!("empty-review-{}.json", uuid()))
            .unwrap();
        let (code, value) = f.call(
            vec![
                "compare".into(),
                "review".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
                "--request".into(),
                f.save(&request),
                "--judgment".into(),
                f.save(judgment),
                "--private-output".into(),
                out.clone().into_os_string(),
            ],
            None,
        );
        assert_eq!(code, expected, "{value:?}");
        if out.exists() { read(&out) } else { value }
    };
    let reviewed = review(&judgment, 0);
    let mut proof = g(&reviewed, "proof").clone();
    assert_eq!(t(&proof, "schema"), "agent-memory/comparison-proof-v2");
    for field in ["pending_decisions", "recovery_frontier"] {
        let mut bad = judgment.clone();
        let target = if field == "recovery_frontier" {
            bad.get_mut("observation").unwrap()
        } else {
            &mut bad
        };
        if let Value::Object(rows) = target {
            rows.retain(|(k, _)| k != field);
        }
        review(&bad, 1);
    }
    set(&mut proof, "schema", s("agent-memory/comparison-proof-v1"));
    compare(&f, "check", &request, Some(&proof), 1);
    assert_eq!(bytes(&f.bank), before);
}
fn oversized_ordinary() {
    let f = reported_fixture();
    let work = feature(&f);
    let original = originals(&f);
    let request = capture_request(&f);
    let search = obj([
        ("project_id", s(&f.project)),
        ("query", s("")),
        ("ids", Value::Array(vec![])),
        ("paths", Value::Array(vec![])),
        ("context", obj([])),
        ("kinds", Value::Array(vec![])),
        ("minimum_trust", s("reviewed")),
        ("limit", n(8)),
        ("include_context", Value::Bool(true)),
    ]);
    // Restore the captured oversized source, not a synthetic reader failure.
    for name in [".agent-memory-transaction.md", ".agent-memory-recovery.md"] {
        fs::remove_file(f.bank.join(name)).unwrap();
    }
    fs::remove_dir_all(f.directory.parent().unwrap()).unwrap();
    let config = PathBuf::from(
        f.world
            .env
            .get(std::ffi::OsStr::new("AGENT_MEMORY_CONFIG_HOME"))
            .unwrap(),
    );
    fs::remove_file(config.join("layout-recovery-bindings.json")).unwrap();
    let calls = vec![
        vec![
            "writes".into(),
            "inspect".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
        ],
        vec![
            "search".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            f.save(&search),
        ],
        vec![
            "capture".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            f.save(&request),
        ],
    ];
    for args in &calls {
        let (code, value) = f.call(args.clone(), None);
        assert_eq!(code, 0, "{value:?}");
        unchanged(&f, &work);
    }
    for (path, body) in &original {
        fs::create_dir_all(f.bank.join(path).parent().unwrap()).unwrap();
        fs::write(f.bank.join(path), body).unwrap();
    }
    let before = bytes(&f.bank);
    for args in calls {
        let (code, _) = f.call(args, None);
        assert_ne!(code, 0);
        assert_eq!(bytes(&f.bank), before);
        unchanged(&f, &work);
    }
}

fn unpack(raw: &str) -> (Value, String) {
    let allowed = g(&agent_memory::records::MODEL, "properties")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>();
    let (meta, body) = agent_memory::codec::frontmatter::read(raw, &allowed).unwrap();
    (Value::Object(meta), body.to_owned())
}
fn add_record(f: &mut Fixture, name: &str, body: &str) -> (Value, String) {
    let raw = record_fixtures::record_text_with_ids(
        body,
        &f.project,
        Default::default(),
        &uuid(),
        &f.actor,
    )
    .unwrap();
    let (mut meta, body) = unpack(&raw);
    for (key, value) in [
        ("trust", "reviewed"),
        ("visibility", "visible"),
        ("admission-reason", "Controlled prior-guidance fixture"),
    ] {
        set(&mut meta, key, s(value));
    }
    let path = format!("topics/{name}.md");
    let raw = agent_memory::codec::frontmatter::write(meta.as_object().unwrap()).unwrap() + &body;
    fs::create_dir_all(f.bank.join(&path).parent().unwrap()).unwrap();
    fs::write(f.bank.join(&path), &raw).unwrap();
    set(&mut f.native, &path, s(raw));
    let (metadata, mut routes, omitted) =
        agent_memory::bank_contract::parse_router(t(&f.native, "MEMORY.md")).unwrap();
    routes.push(obj([
        ("path", s(&path)),
        ("purpose", s("Synthetic historical source")),
        ("scope", s("repository")),
        ("lifecycle", s("active")),
        ("read-when", s("When comparing fixture meaning")),
    ]));
    let router = agent_memory::bank_contract::router_text(&metadata, &routes, &omitted).unwrap();
    set(&mut f.native, "MEMORY.md", s(&router));
    fs::write(f.bank.join("MEMORY.md"), router).unwrap();
    (meta, path)
}
fn boundary_fixture(count: usize) -> Fixture {
    let f = Fixture::configured(
        false,
        SCRATCH,
        |f| {
            for number in 0..count {
                add_record(
                    f,
                    &format!("policy-{number:02}"),
                    &format!(
                        "# Attempt cap\n\nFor synthetic environment {number:02} retain attempt cap evidence before retrying.\n"
                    ),
                );
            }
        },
        |_| {},
    );
    let p = discard(&f);
    f.layout("resolve", vec!["--proposal".into(), f.save(&p)], 0);
    f
}
fn boundary(kind: &str) {
    let f = boundary_fixture(if ["disclosure", "proof"].contains(&kind) {
        40
    } else {
        0
    });
    let work = feature(&f);
    let bank = bytes(&f.bank);
    if kind == "capture" {
        let mut request = capture_request(&f);
        let evidence = request
            .get_mut("candidate")
            .unwrap()
            .get_mut("metadata")
            .unwrap()
            .get_mut("evidence")
            .unwrap()
            .as_array_mut()
            .unwrap();
        evidence.push(s("Synthetic boundary context: é \" "));
        let remaining = 16384 - encoded(&request).unwrap().len();
        let evidence = request
            .get_mut("candidate")
            .unwrap()
            .get_mut("metadata")
            .unwrap()
            .get_mut("evidence")
            .unwrap()
            .as_array_mut()
            .unwrap();
        let text = evidence.last().unwrap().as_str().unwrap().to_owned() + &"x".repeat(remaining);
        *evidence.last_mut().unwrap() = s(text);
        assert_eq!(encoded(&request).unwrap().len(), 16384);
        assert_eq!(t(&capture(&f, &request, 0), "decision"), "eligible");
        assert_eq!(bytes(&f.bank), bank);
        let evidence = request
            .get_mut("candidate")
            .unwrap()
            .get_mut("metadata")
            .unwrap()
            .get_mut("evidence")
            .unwrap()
            .as_array_mut()
            .unwrap();
        let text = evidence.last().unwrap().as_str().unwrap().to_owned() + "x";
        *evidence.last_mut().unwrap() = s(text);
        assert_eq!(encoded(&request).unwrap().len(), 16385);
        let value = capture(&f, &request, 1);
        assert_eq!(g(&value, "canonical_writes_performed"), &Value::Bool(false));
    } else if kind == "request" {
        let request = candidate_request(&f);
        let path = PathBuf::from(f.save(&request));
        let mut raw = fs::read(&path).unwrap();
        raw.resize(65536, b' ');
        fs::write(&path, &raw).unwrap();
        let invoke = || {
            f.call(
                vec![
                    "compare".into(),
                    "inspect".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                    "--request".into(),
                    path.clone().into_os_string(),
                ],
                None,
            )
        };
        let (code, value) = invoke();
        assert_eq!(code, 0, "{value:?}");
        assert_eq!(g(&value, "comparison_read_complete"), &Value::Bool(true));
        assert_eq!(t(g(&value, "observation"), "request_sha256"), sha(&request));
        assert_eq!(bytes(&f.bank), bank);
        raw.push(b' ');
        fs::write(&path, raw).unwrap();
        let (code, value) = invoke();
        assert_eq!(code, 1);
        assert_eq!(g(&value, "comparison_read_complete"), &Value::Bool(false));
    } else {
        let request = candidate_request(&f);
        let (_, view) = compare(&f, "inspect", &request, None, 0);
        let view = view.unwrap();
        let rows = array(g(&view, "matches"));
        assert_eq!(rows.len(), 40);
        if kind == "disclosure" {
            let delta = 65536
                - rows
                    .iter()
                    .map(|r| encoded(r).unwrap().len())
                    .sum::<usize>();
            let fragment = "é \"";
            let extra = encoded(&s(fragment)).unwrap().len() - encoded(&s("")).unwrap().len();
            assert!(delta > extra);
            let path = f.bank.join("topics/policy-00.md");
            let original = fs::read_to_string(&path).unwrap();
            let exact = original + fragment + &"x".repeat(delta - extra);
            fs::write(&path, &exact).unwrap();
            let (_, view) = compare(&f, "inspect", &request, None, 0);
            assert_eq!(
                array(g(&view.unwrap(), "matches"))
                    .iter()
                    .map(|r| encoded(r).unwrap().len())
                    .sum::<usize>(),
                65536
            );
            fs::write(&path, exact + "x").unwrap();
            let before = bytes(&f.bank);
            let (result, _) = compare(&f, "inspect", &request, None, 1);
            assert_eq!(g(&result, "comparison_read_complete"), &Value::Bool(false));
            assert_eq!(bytes(&f.bank), before);
            unchanged(&f, &work);
            return;
        }
        let judgment = obj([
            ("observation", g(&view, "observation").clone()),
            (
                "decisions",
                Value::Array(
                    rows.iter()
                        .map(|r| {
                            obj([
                                ("path", g(r, "path").clone()),
                                ("classification", s("distinct-knowledge")),
                                (
                                    "rationale",
                                    s("Controlled context review: é \" ".to_owned()
                                        + &"x".repeat(1000)),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("pending_decisions", Value::Array(vec![])),
            ("rationale", s("Complete independent source comparison.")),
        ]);
        let mut proof = obj([
            ("schema", s("agent-memory/comparison-proof-v2")),
            ("judgment", judgment),
            ("disposition", s("create-distinct")),
            ("targets", Value::Array(vec![])),
        ]);
        let mut remaining = 65536 - encoded(&proof).unwrap().len();
        for row in proof
            .get_mut("judgment")
            .unwrap()
            .get_mut("decisions")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            let current = t(row, "rationale");
            let count = remaining.min(2048 - current.len());
            let text = current.to_owned() + &"x".repeat(count);
            remaining -= count;
            set(row, "rationale", s(text));
        }
        assert_eq!(remaining, 0);
        assert_eq!(encoded(&proof).unwrap().len(), 65536);
        let out = f.world.inside("reviewed-boundary.json").unwrap();
        f.good(vec![
            "compare".into(),
            "review".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
            "--request".into(),
            f.save(&request),
            "--judgment".into(),
            f.save(g(&proof, "judgment")),
            "--private-output".into(),
            out.clone().into_os_string(),
        ]);
        let reviewed = read(&out);
        assert_eq!(g(&reviewed, "proof"), &proof);
        compare(&f, "check", &request, Some(&proof), 0);
        let row = proof
            .get_mut("judgment")
            .unwrap()
            .get_mut("decisions")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        let text = t(row, "rationale").to_owned() + "x";
        assert!(text.len() <= 2048);
        set(row, "rationale", s(text));
        assert_eq!(encoded(&proof).unwrap().len(), 65537);
        let (result, _) = compare(&f, "check", &request, Some(&proof), 1);
        assert_eq!(g(&result, "comparison_read_complete"), &Value::Bool(false));
    }
    assert_eq!(bytes(&f.bank), bank);
    unchanged(&f, &work);
}

fn private_call(f: &Fixture, mut args: Vec<OsString>, expected: i32) -> Value {
    let output = f
        .world
        .inside(format!("private-call-{}.json", uuid()))
        .unwrap();
    args.extend(["--private-output".into(), output.clone().into_os_string()]);
    let (code, value) = f.call(args, None);
    assert_eq!(code, expected, "{value:?}");
    if output.exists() {
        read(&output)
    } else {
        value
    }
}
fn coverage(f: &Fixture, proof: &Value, files: &Value) -> Value {
    let (head, ledger) = ledger(f);
    obj([
        ("schema", s("agent-memory/layout-coverage-review-v1")),
        ("review_id", s(uuid())),
        ("operation_id", s(&f.operation)),
        ("reviewer_developer_id", s(&f.actor)),
        ("reviewer_harness", s("consumer-independent-reviewer")),
        ("reviewer_session", s(uuid())),
        ("review_time", g(&f.manifest, "inspected_at").clone()),
        ("manifest_sha256", g(&ledger, "manifest_sha256").clone()),
        ("unit_map_sha256", g(&ledger, "unit_map_sha256").clone()),
        ("ledger_head_sha256", g(&head, "sha256").clone()),
        ("comparison_proof_input_sha256", s(sha(proof))),
        ("intended_output_sha256", s(sha(files))),
        (
            "claims",
            Value::Array(
                array(g(g(proof, "judgment"), "pending_decisions"))
                    .iter()
                    .map(|r| g(r, "coverage_claim").clone())
                    .collect(),
            ),
        ),
        (
            "rationale",
            s("Independent review of complete intended output and original source."),
        ),
        ("decision", s("approve")),
    ])
}
fn reviewed(f: &Fixture, request: &Value, proof: &Value, files: Option<&Value>) -> Value {
    let mut args = vec![
        "compare".into(),
        "review".into(),
        "--repo".into(),
        f.repo.clone().into_os_string(),
        "--request".into(),
        f.save(request),
        "--judgment".into(),
        f.save(g(proof, "judgment")),
    ];
    if let Some(files) = files {
        args.extend([
            "--coverage-review".into(),
            f.save(&coverage(f, proof, files)),
        ]);
    }
    let value = private_call(f, args, 0);
    assert_eq!(t(&value, "disposition"), t(proof, "disposition"));
    g(&value, "proof").clone()
}
fn consumer_flow(route: &str, pending: bool, plan_negative: bool) {
    let first_path = "topics/consumer-first.md";
    let second_path = "topics/consumer-second.md";
    let f = Fixture::configured(
        false,
        if route == "update" { POLICY } else { POLICY },
        |f| {
            add_record(
                f,
                "consumer-first",
                if route == "governance" && !plan_negative {
                    "# Synthetic policy 1\n\nDiscard queued items after failure.\n"
                } else {
                    POLICY
                },
            );
            add_record(f, "consumer-second", POLICY);
        },
        |_| {},
    );
    if !pending {
        let p = discard(&f);
        f.layout("resolve", vec!["--proposal".into(), f.save(&p)], 0);
    }
    let work = feature(&f);
    let old = unpack(&fs::read_to_string(f.bank.join(first_path)).unwrap()).0;
    let replacement = unpack(&fs::read_to_string(f.bank.join(second_path)).unwrap()).0;
    let mut request = candidate_request(&f);
    set(
        &mut request,
        "candidate",
        obj([
            (
                "metadata",
                if route == "governance" && !plan_negative {
                    replacement.clone()
                } else {
                    old.clone()
                },
            ),
            (
                "body",
                s(if route == "governance" {
                    POLICY.to_owned()
                } else {
                    POLICY.to_owned() + "\nRetain the reviewed source conditions.\n"
                }),
            ),
        ]),
    );
    set(
        &mut request,
        "searches",
        Value::Array(vec![
            s("unmatched-consumer-one"),
            s("unmatched-consumer-two"),
        ]),
    );
    set(
        &mut request,
        "read_paths",
        Value::Array(
            if route == "consolidate" || (route == "governance" && !plan_negative) {
                vec![s(first_path), s(second_path)]
            } else {
                vec![s(first_path)]
            },
        ),
    );
    let (_, view) = compare(&f, "inspect", &request, None, 0);
    let view = view.unwrap();
    assert_eq!(array(g(&view, "pending_matches")).is_empty(), !pending);
    let decisions = array(g(&view, "matches"))
        .iter()
        .map(|r| {
            obj([
                ("path", g(r, "path").clone()),
                (
                    "classification",
                    s(
                        if route == "governance" && !plan_negative && t(r, "path") == first_path {
                            "contradiction"
                        } else {
                            "equivalent"
                        },
                    ),
                ),
                (
                    "rationale",
                    s("The inspected source meaning and conditions were reviewed independently."),
                ),
            ])
        })
        .collect();
    let pending_decisions = array(g(&view, "pending_matches"))
        .iter()
        .map(|r| {
            obj([
                ("unit_id", g(r, "unit_id").clone()),
                (
                    "classification",
                    s(if route == "update" && plan_negative {
                        "equivalent"
                    } else {
                        "distinct-knowledge"
                    }),
                ),
                ("rationale", s("The unconfirmed original remains pending.")),
                (
                    "coverage_claim",
                    obj([
                        ("unit_id", g(r, "unit_id").clone()),
                        ("coverage", s("none")),
                        ("covered_source_ranges", Value::Array(vec![])),
                        ("target_record_refs", Value::Array(vec![])),
                    ]),
                ),
            ])
        })
        .collect();
    let judgment = obj([
        ("observation", g(&view, "observation").clone()),
        ("decisions", Value::Array(decisions)),
        ("pending_decisions", Value::Array(pending_decisions)),
        ("rationale", s("Complete independent comparison.")),
    ]);
    let disposition = if route == "governance" && !plan_negative {
        "clarification-required"
    } else if route == "consolidate" {
        "consolidate-existing"
    } else {
        "update-existing"
    };
    let targets = array(g(&view, "matches"))
        .iter()
        .filter(|r| !(route == "governance" && !plan_negative && t(r, "path") == first_path))
        .map(|r| {
            obj([
                ("path", g(r, "path").clone()),
                ("sha256", g(r, "sha256").clone()),
                ("id", g(g(r, "metadata"), "id").clone()),
                ("version", g(g(r, "metadata"), "version").clone()),
                ("required_preserved_metadata", g(r, "metadata").clone()),
                ("scope_chain", g(r, "scope_chain").clone()),
            ])
        })
        .collect();
    let mut proof = obj([
        ("schema", s("agent-memory/comparison-proof-v2")),
        ("judgment", judgment.clone()),
        ("disposition", s(disposition)),
        ("targets", Value::Array(targets)),
    ]);
    let contexts = Value::Object(
        fs::read_dir(&f.bank)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|x| x == "md")
                    && !p
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(".agent-memory")
            })
            .map(|p| {
                (
                    p.file_name().unwrap().to_str().unwrap().to_owned(),
                    s(bytes_sha(&fs::read(p).unwrap())),
                )
            })
            .collect(),
    );
    let plan_input = if route == "governance" {
        obj([
            ("project_id", s(&f.project)),
            (
                "operation",
                s(if plan_negative { "revoke" } else { "supersede" }),
            ),
            (
                "target",
                obj([
                    ("path", s(first_path)),
                    ("id", g(&old, "id").clone()),
                    ("version", g(&old, "version").clone()),
                    (
                        "sha256",
                        s(bytes_sha(&fs::read(f.bank.join(first_path)).unwrap())),
                    ),
                ]),
            ),
            (
                "authority",
                obj([
                    ("source_class", s("user")),
                    ("repository_evidence", Value::Null),
                    (
                        "evidence",
                        Value::Array(vec![s("user:explicit-disposable-fixture-correction")]),
                    ),
                    (
                        "reason",
                        s(
                            "The operator confirmed this claim is incorrect under every retained condition.",
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
            (
                "comparison",
                obj([("request", request.clone()), ("judgment", judgment.clone())]),
            ),
            ("capture", Value::Null),
            (
                "replacement",
                if plan_negative {
                    Value::Null
                } else {
                    obj([
                        ("path", s(second_path)),
                        ("id", g(&replacement, "id").clone()),
                        ("version", g(&replacement, "version").clone()),
                        (
                            "sha256",
                            s(bytes_sha(&fs::read(f.bank.join(second_path)).unwrap())),
                        ),
                    ])
                },
            ),
            ("delete_ids", Value::Array(vec![])),
            ("context_review", contexts),
            ("context_edits", obj([])),
            ("context", obj([])),
        ])
    } else {
        obj([
            ("comparison_request", request.clone()),
            ("comparison_judgment", judgment),
            ("capture", Value::Null),
            ("context", obj([])),
        ])
    };
    let verb = if route == "governance" {
        "govern"
    } else {
        "compact"
    };
    let flag = if route == "governance" {
        "--request"
    } else {
        "--input"
    };
    let before = bytes(&f.bank);
    let plan = if pending {
        let plan = private_call(
            &f,
            vec![
                verb.into(),
                "plan".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
                flag.into(),
                f.save(&plan_input),
            ],
            0,
        );
        assert_eq!(g(&plan, "proposal"), &Value::Null);
        assert_eq!(g(&plan, "verified"), &Value::Bool(false));
        assert_eq!(bytes(&f.bank), before);
        Some(plan)
    } else {
        None
    };
    if let Some(plan) = &plan {
        if !plan_negative || route == "update" {
            let path = if route == "governance" {
                second_path
            } else {
                first_path
            };
            let files = g(g(plan, "output_plan"), "files");
            let (metadata, body) = unpack(t(files, path));
            assert!(body.contains(POLICIES[0]));
            for decision in proof
                .get_mut("judgment")
                .unwrap()
                .get_mut("pending_decisions")
                .unwrap()
                .as_array_mut()
                .unwrap()
            {
                let row = array(g(&view, "pending_matches"))
                    .iter()
                    .find(|r| g(r, "unit_id") == g(decision, "unit_id"))
                    .unwrap();
                set(
                    decision,
                    "coverage_claim",
                    obj([
                        ("unit_id", g(row, "unit_id").clone()),
                        ("coverage", s("full")),
                        ("covered_source_ranges", g(row, "source_ranges").clone()),
                        (
                            "target_record_refs",
                            Value::Array(vec![obj([
                                ("id", g(&metadata, "id").clone()),
                                ("version", g(&metadata, "version").clone()),
                                ("path", s(path)),
                                ("resulting_sha256", s(bytes_sha(t(files, path).as_bytes()))),
                            ])]),
                        ),
                    ]),
                );
            }
        }
    }
    let proof = reviewed(
        &f,
        &request,
        &proof,
        plan.as_ref().map(|p| g(g(p, "output_plan"), "files")),
    );
    let input = if route == "governance" {
        let mut value = plan_input.clone();
        set(
            &mut value,
            "comparison",
            obj([("request", request.clone()), ("proof", proof)]),
        );
        value
    } else {
        obj([
            ("comparison_request", request),
            ("comparison_proof", proof),
            ("capture", Value::Null),
            ("context", obj([])),
        ])
    };
    let mut args = vec![
        verb.into(),
        "preview".into(),
        "--repo".into(),
        f.repo.clone().into_os_string(),
        flag.into(),
        f.save(&input),
    ];
    if let Some(plan) = &plan {
        let output = g(plan, "output_plan");
        for field in if plan_negative {
            vec!["repo_root", "observed_hashes", "files"]
        } else {
            vec!["operation_id"]
        } {
            let mut changed = output.clone();
            match field {
                "repo_root" => set(
                    &mut changed,
                    field,
                    s(f.world.inside("foreign-project").unwrap().to_str().unwrap()),
                ),
                "observed_hashes" => set(
                    changed.get_mut(field).unwrap(),
                    "MEMORY.md",
                    s("0".repeat(64)),
                ),
                "files" => {
                    let body = t(g(&changed, "files"), "projectbrief.md").to_owned()
                        + "\nUnreviewed plan edit.\n";
                    set(
                        changed.get_mut("files").unwrap(),
                        "projectbrief.md",
                        s(body),
                    );
                }
                _ => set(&mut changed, "operation_id", s(uuid())),
            }
            let mut bad = args.clone();
            bad.extend(["--plan".into(), f.save(&changed)]);
            private_call(&f, bad, 1);
            assert_eq!(bytes(&f.bank), before);
        }
        if plan_negative {
            let (code, _) = f.call(
                vec![
                    verb.into(),
                    "apply".into(),
                    "--proposal".into(),
                    f.save(output),
                ],
                None,
            );
            assert_eq!(code, 1);
            let (code, value) = f.call(
                vec![
                    verb.into(),
                    "plan".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                    flag.into(),
                    f.save(&plan_input),
                ],
                None,
            );
            assert_eq!(code, 1);
            assert!(
                !String::from_utf8(encoded(&value).unwrap())
                    .unwrap()
                    .contains(POLICY)
            );
        }
        args.extend(["--plan".into(), f.save(plan)]);
    }
    let proposal = private_call(&f, args.clone(), 0);
    if let Some(plan) = &plan {
        assert_eq!(
            g(g(&proposal, "proposal"), "files"),
            g(g(plan, "output_plan"), "files")
        );
        let out = f.world.inside("consumer-without-receipt.json").unwrap();
        f.layout(
            "resolve",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--unit".into(),
                t(&array(g(&view, "pending_matches"))[0], "unit_id").into(),
                "--input".into(),
                f.save(&obj([(
                    "transaction_proposal",
                    g(&proposal, "proposal").clone(),
                )])),
                "--output".into(),
                out.into_os_string(),
            ],
            1,
        );
    }
    let completed = f.good(vec![
        verb.into(),
        "apply".into(),
        "--proposal".into(),
        f.save(&proposal),
    ]);
    assert_eq!(g(&completed, "verified"), &Value::Bool(true));
    let transaction = g(&proposal, "proposal");
    let receipt = read(Path::new(t(&completed, "receipt_path")));
    assert_eq!(t(&receipt, "operation_id"), t(transaction, "operation_id"));
    assert_eq!(t(&receipt, "request_sha256"), sha(transaction));
    assert_eq!(t(&receipt, "outcome"), "finished");
    assert_eq!(g(&receipt, "verified"), &Value::Bool(true));
    for (path, body) in g(transaction, "files").as_object().unwrap() {
        assert_eq!(
            fs::read(f.bank.join(path)).unwrap(),
            body.as_str().unwrap().as_bytes()
        );
    }
    if pending {
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(
            g(&status, "semantic_recovery_complete"),
            &Value::Bool(!plan_negative || route == "update")
        );
        if plan_negative {
            private_call(&f, args, 1);
        }
    }
    unchanged(&f, &work);
}

fn discard_id(f: &Fixture, id: &str) -> Value {
    let (_, mut request) = discard_request(f);
    let (head, ledger) = ledger(f);
    let unit = array(g(&ledger, "units"))
        .iter()
        .find(|u| t(u, "unit_id") == id)
        .unwrap();
    let authority = request.get_mut("discard_authority").unwrap();
    set(authority, "unit_id", s(id));
    set(authority, "source_ranges", g(unit, "source_ranges").clone());
    set(authority, "ledger_head_sha256", g(&head, "sha256").clone());
    f.private_layout(
        "resolve",
        vec![
            "--operation".into(),
            f.operation.clone().into(),
            "--unit".into(),
            id.into(),
            "--input".into(),
            f.save(&request),
        ],
    )
}
fn paired_fixture() -> Fixture {
    let body = SCRATCH.to_owned() + POLICY;
    let end = body.len();
    Fixture::configured(
        false,
        &body,
        |_| {},
        |req| partition_request(req, &[(0, SCRATCH.len()), (SCRATCH.len(), end)]),
    )
}
fn nested_scratch_exclusion() {
    let f = paired_fixture();
    let (_, ledger) = ledger(&f);
    let units = array(g(&ledger, "units"))
        .iter()
        .filter(|u| t(u, "state") == "pending-admission")
        .collect::<Vec<_>>();
    assert_eq!(units.len(), 2);
    let first = t(units[0], "unit_id");
    let second = t(units[1], "unit_id");
    let discard = discard_id(&f, first);
    let (proposal, nested, _) = f.admission(false);
    f.fault_admission(&proposal, "layout-recovery-nested-after-receipt", "error");
    assert_eq!(
        t(
            &read(&f.bank.join(".agent-memory-transaction.md")),
            "schema"
        ),
        "agent-memory/layout-frontier-transaction-v1"
    );
    let bank = bytes(&f.bank);
    let retained = bytes(f.directory.parent().unwrap());
    f.layout("resolve", vec!["--proposal".into(), f.save(&discard)], 1);
    assert_eq!(bytes(&f.bank), bank);
    assert_eq!(bytes(f.directory.parent().unwrap()), retained);
    let preview = f.nested_preview(&nested, "finish");
    f.recover(&preview, 0);
    let refreshed = discard_id(&f, first);
    f.layout("resolve", vec!["--proposal".into(), f.save(&refreshed)], 0);
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(
        g(&status, "pending_unit_ids"),
        &Value::Array(vec![s(second)])
    );
}
fn unfinished_resolution() {
    for mode in ["error", "exit"] {
        let f = paired_fixture();
        let original = originals(&f);
        let work = feature(&f);
        let (_, ledger) = ledger(&f);
        let units = array(g(&ledger, "units"))
            .iter()
            .filter(|u| t(u, "state") == "pending-admission")
            .collect::<Vec<_>>();
        let first_id = t(units[0], "unit_id").to_owned();
        let second_id = t(units[1], "unit_id").to_owned();
        let first = discard_id(&f, &first_id);
        let second = discard_id(&f, &second_id);
        let (admission, _, _) = f.admission_selected(true, Some(&second_id), POLICY);
        let revision_input = revision_request(&f, &second_id);
        let revision = f.private_layout(
            "revise-units",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--input".into(),
                f.save(&revision_input),
            ],
        );
        fault_layout(
            &f,
            "resolve",
            &first,
            "layout-recovery-resolution-after-receipt",
            mode,
        );
        let before = bytes(&f.bank);
        let retained = bytes(&f.directory);
        let index = fs::read(f.directory.parent().unwrap().join("index.json")).unwrap();
        let blocked = || {
            for proposal in [&first, &second] {
                let p = g(proposal, "proposal");
                let out = f
                    .world
                    .inside(format!("blocked-resolve-{}.json", uuid()))
                    .unwrap();
                f.layout(
                    "resolve",
                    vec![
                        "--operation".into(),
                        f.operation.clone().into(),
                        "--unit".into(),
                        t(p, "unit_id").into(),
                        "--input".into(),
                        f.save(g(p, "terminal")),
                        "--output".into(),
                        out.clone().into_os_string(),
                    ],
                    1,
                );
                assert!(!out.exists());
            }
            f.layout("resolve", vec!["--proposal".into(), f.save(&second)], 1);
            let (code, _) = f.call(
                vec![
                    "admission".into(),
                    "apply".into(),
                    "--proposal".into(),
                    f.save(&admission),
                ],
                None,
            );
            assert_eq!(code, 1);
            f.layout(
                "revise-units",
                vec!["--proposal".into(), f.save(&revision)],
                1,
            );
            let out = f
                .world
                .inside(format!("blocked-revision-{}.json", uuid()))
                .unwrap();
            f.layout(
                "revise-units",
                vec![
                    "--operation".into(),
                    f.operation.clone().into(),
                    "--input".into(),
                    f.save(&revision_input),
                    "--output".into(),
                    out.clone().into_os_string(),
                ],
                1,
            );
            assert!(!out.exists());
        };
        blocked();
        assert_eq!(bytes(&f.directory), retained);
        assert_eq!(bytes(&f.bank), before);
        assert_eq!(
            fs::read(f.directory.parent().unwrap().join("index.json")).unwrap(),
            index
        );
        let resolution = t(g(&first, "proposal"), "resolution_id");
        let completion = f
            .directory
            .join("resolution-completions")
            .join(format!("{resolution}.json"));
        fs::create_dir_all(completion.parent().unwrap()).unwrap();
        private(&completion, &obj([("verified", Value::Bool(true))]));
        let corrupt = bytes(&f.directory);
        f.layout("resolve", vec!["--proposal".into(), f.save(&first)], 1);
        blocked();
        assert_eq!(bytes(&f.directory), corrupt);
        assert_eq!(bytes(&f.bank), before);
        fs::remove_file(completion).unwrap();
        f.layout("resolve", vec!["--proposal".into(), f.save(&first)], 0);
        assert_eq!(
            g(&read(&f.directory.join("ledger-head.json")), "generation"),
            &n(1)
        );
        assert_eq!(
            fs::read(
                f.directory
                    .join("resolutions")
                    .join(format!("{resolution}.json"))
            )
            .unwrap(),
            retained[&format!("resolutions/{resolution}.json")]
        );
        let fresh_revision = f.private_layout(
            "revise-units",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--input".into(),
                f.save(&revision_request(&f, &second_id)),
            ],
        );
        f.layout(
            "revise-units",
            vec!["--proposal".into(), f.save(&fresh_revision)],
            0,
        );
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        let unit = array(g(&status, "pending_unit_ids"))[0].as_str().unwrap();
        let (p, _, _) = f.admission_selected(true, Some(unit), POLICY);
        f.good(vec![
            "admission".into(),
            "apply".into(),
            "--proposal".into(),
            f.save(&p),
        ]);
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(array(g(&status, "pending_unit_ids")).len(), 1);
        let p = discard_id(
            &f,
            array(g(&status, "pending_unit_ids"))[0].as_str().unwrap(),
        );
        f.layout("resolve", vec!["--proposal".into(), f.save(&p)], 0);
        assert_eq!(
            g(&read(&f.directory.join("ledger-head.json")), "generation"),
            &n(4)
        );
        assert_eq!(
            g(
                &f.layout(
                    "status",
                    vec!["--operation".into(), f.operation.clone().into()],
                    0
                ),
                "semantic_recovery_complete"
            ),
            &Value::Bool(true)
        );
        f.private_layout(
            "retention",
            vec![
                "inspect".into(),
                "--operation".into(),
                f.operation.clone().into(),
            ],
        );
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
        assert!(!f.bank.join(".agent-memory-recovery.md").exists());
        assert_eq!(originals(&f), original);
        unchanged(&f, &work);
    }
}
fn standalone_retry() {
    for (stage, mode) in [
        ("resolution-after-receipt", "exit"),
        ("ledger-generation-before-head", "error"),
        ("ledger-generation-after-head", "exit"),
        ("sentinel-retirement", "error"),
        ("retirement-after-marker-removal", "exit"),
        ("retirement-after-sentinel-removal", "error"),
    ] {
        let f = Fixture::with_body(false, SCRATCH);
        let canonical = f.snapshot();
        let original = originals(&f);
        let proposal = discard(&f);
        let p = g(&proposal, "proposal");
        fault_layout(
            &f,
            "resolve",
            &proposal,
            &format!("layout-recovery-{stage}"),
            mode,
        );
        let path = f
            .directory
            .join("resolutions")
            .join(format!("{}.json", t(p, "resolution_id")));
        let receipt = fs::read(&path).unwrap();
        let mut changed = p.clone();
        set(&mut changed, "terminal_state", s("admitted"));
        f.layout("resolve", vec!["--proposal".into(), f.save(&changed)], 1);
        f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
        let (head, ledger) = ledger(&f);
        assert_eq!(g(&head, "generation"), &n(1));
        let unit = array(g(&ledger, "units"))
            .iter()
            .find(|u| g(u, "unit_id") == g(p, "unit_id"))
            .unwrap();
        assert_eq!(t(unit, "state"), "reviewed-non-guidance-discard");
        assert_eq!(array(g(unit, "terminal_receipts")).len(), 1);
        assert_eq!(fs::read(path).unwrap(), receipt);
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
        assert!(!f.bank.join(".agent-memory-recovery.md").exists());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(g(&status, "semantic_recovery_complete"), &Value::Bool(true));
        assert_eq!(g(&status, "pending_units"), &n(0));
        let history = f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
        assert_eq!(g(&history, "already_applied"), &Value::Bool(true));
        assert_eq!(g(&history, "writes_performed"), &Value::Bool(false));
        assert_eq!(f.snapshot(), canonical);
        assert_eq!(originals(&f), original);
    }
}

fn topic_groups(f: &mut Fixture, paths: &[String], size: usize) {
    let (metadata, _, omitted) =
        agent_memory::bank_contract::parse_router(t(&f.native, "MEMORY.md")).unwrap();
    let mut routes = vec![];
    for (number, group) in paths.chunks(size).enumerate() {
        let path = format!("topics/frontier-index-{number}.md");
        let children = group
            .iter()
            .map(|path| {
                obj([
                    ("path", s(path)),
                    ("purpose", s("Synthetic historical source")),
                    ("scope", s("task:x")),
                    ("lifecycle", s("expired")),
                    ("read-when", s("When comparing fixture meaning")),
                ])
            })
            .collect::<Vec<_>>();
        let header = obj([
            ("schema", s("agent-memory/topic-index-v1")),
            ("project-id", s(&f.project)),
            ("updated", g(&metadata, "router-updated").clone()),
        ]);
        let raw = agent_memory::bank_layout::index_text(&path, &header, &children).unwrap();
        fs::write(f.bank.join(&path), &raw).unwrap();
        set(&mut f.native, &path, s(raw));
        routes.push(obj([
            ("path", s(path)),
            ("purpose", s("Synthetic group")),
            ("scope", s("task:x")),
            ("lifecycle", s("active")),
            ("read-when", s("When comparing fixture meaning")),
        ]));
    }
    let router = agent_memory::bank_contract::router_text(&metadata, &routes, &omitted).unwrap();
    fs::write(f.bank.join("MEMORY.md"), &router).unwrap();
    set(&mut f.native, "MEMORY.md", s(router));
}
fn selective_frontier() {
    let f = Fixture::configured(
        false,
        POLICY,
        |f| {
            let mut paths = vec![];
            for i in 0..65 {
                paths.push(
                    add_record(
                        f,
                        &format!("frontier-{i:02}"),
                        &format!(
                            "# Capacity fixture\n\ncommonfrontier {}\n",
                            if i == 0 { "selectivefrontier" } else { "other" }
                        ),
                    )
                    .1,
                );
            }
            topic_groups(f, &paths, 33);
        },
        |_| {},
    );
    let before = bytes(&f.bank);
    let mut request = candidate_request(&f);
    set(
        &mut request,
        "searches",
        Value::Array(vec![s("selectivefrontier"), s("unmatchedsentinel")]),
    );
    let (_, view) = compare(&f, "inspect", &request, None, 0);
    let view = view.unwrap();
    assert_eq!(
        array(g(&view, "matches"))
            .iter()
            .map(|r| t(r, "path"))
            .collect::<Vec<_>>(),
        vec!["topics/frontier-00.md"]
    );
    assert_eq!(array(g(&view, "pending_matches")).len(), 1);
    set(
        &mut request,
        "searches",
        Value::Array(vec![s("commonfrontier"), s("unmatchedsentinel")]),
    );
    let (result, _) = compare(&f, "inspect", &request, None, 1);
    assert_eq!(g(&result, "comparison_read_complete"), &Value::Bool(false));
    assert_eq!(bytes(&f.bank), before);
    assert_eq!(
        array(g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0
            ),
            "pending_unit_ids"
        ))
        .len(),
        1
    );
}
fn mixed_rows() {
    let f = Fixture::configured(
        false,
        "pppppppp",
        |f| {
            let mut paths = vec![];
            for i in 0..51 {
                let name = if i == 50 {
                    "z-extra".into()
                } else {
                    format!("p{i:02}")
                };
                let (_, path) = add_record(f, &name, if i == 50 { "other" } else { "cap" });
                let (mut metadata, body) = unpack(&fs::read_to_string(f.bank.join(&path)).unwrap());
                for (key, value) in [
                    ("kind", "result"),
                    ("status", "expired"),
                    ("scope", "task:x"),
                    ("state-reason", "f"),
                ] {
                    set(&mut metadata, key, s(value));
                }
                set(
                    &mut metadata,
                    "source",
                    obj([("actor", s("f")), ("kind", s("f"))]),
                );
                set(&mut metadata, "evidence", Value::Array(vec![s("e")]));
                for key in ["harness", "session"] {
                    set(metadata.get_mut("origin").unwrap(), key, s("f"));
                }
                set(
                    metadata.get_mut("origin").unwrap(),
                    "code-state",
                    s("unknown"),
                );
                if let Value::Object(rows) = &mut metadata {
                    rows.retain(|(key, _)| {
                        !["admission-reason", "trust", "visibility"].contains(&key.as_str())
                    });
                }
                let raw = agent_memory::codec::frontmatter::write(metadata.as_object().unwrap())
                    .unwrap()
                    + &body;
                fs::write(f.bank.join(&path), &raw).unwrap();
                set(&mut f.native, &path, s(raw));
                paths.push(path);
            }
            topic_groups(f, &paths, 26);
            let mut router = t(&f.native, "MEMORY.md").to_owned();
            let (_, _, omitted) = agent_memory::bank_contract::parse_router(&router).unwrap();
            for (path, _) in omitted.as_object().unwrap() {
                fs::write(f.bank.join(path), "cap").unwrap();
                set(&mut f.native, path, s("cap"));
            }
            router = agent_memory::bank_contract::add_context_routes(
                &router,
                &omitted
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(p, _)| p.clone())
                    .collect::<Vec<_>>(),
                &s("2026-11-14T00:00:00Z"),
            )
            .unwrap();
            fs::write(f.bank.join("MEMORY.md"), &router).unwrap();
            set(&mut f.native, "MEMORY.md", s(router));
            fs::write(
                f.repo.join("e"),
                b"Synthetic structural fixture, not actual semantic confirmation.\n",
            )
            .unwrap();
        },
        |req| partition_request(req, &(0..8).map(|i| (i, i + 1)).collect::<Vec<_>>()),
    );
    let before = bytes(&f.bank);
    let originals = originals(&f);
    let head = fs::read(f.directory.join("ledger-head.json")).unwrap();
    let initial = fs::read(f.directory.join("ledger/00000000.json")).unwrap();
    let mut request = candidate_request(&f);
    set(
        &mut request,
        "searches",
        Value::Array(vec![s("cap"), s("absent")]),
    );
    let (_, view) = compare(&f, "inspect", &request, None, 0);
    let view = view.unwrap();
    let rows = array(g(&view, "matches"));
    let pending = array(g(&view, "pending_matches"));
    assert_eq!(rows.len() + pending.len(), 64);
    assert_eq!(pending.len(), 8);
    let disclosure = rows
        .iter()
        .chain(pending)
        .map(|r| encoded(r).unwrap().len())
        .sum::<usize>();
    assert!(
        disclosure
            + rows
                .iter()
                .map(|r| encoded(r).unwrap().len())
                .max()
                .unwrap()
            < 65536
    );
    let judgment = obj([
        ("observation", g(&view, "observation").clone()),
        (
            "decisions",
            Value::Array(
                rows.iter()
                    .map(|r| {
                        obj([
                            ("path", g(r, "path").clone()),
                            ("classification", s("uncertain")),
                            (
                                "rationale",
                                s("Structural review only, not semantic confirmation."),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "pending_decisions",
            Value::Array(
                pending
                    .iter()
                    .map(|r| {
                        obj([
                            ("unit_id", g(r, "unit_id").clone()),
                            ("classification", s("uncertain")),
                            (
                                "rationale",
                                s("Unconfirmed original source remains pending."),
                            ),
                            (
                                "coverage_claim",
                                obj([
                                    ("unit_id", g(r, "unit_id").clone()),
                                    ("covered_source_ranges", Value::Array(vec![])),
                                    ("target_record_refs", Value::Array(vec![])),
                                    ("coverage", s("none")),
                                ]),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "rationale",
            s("Complete bounded structural comparison; no admission authority."),
        ),
    ]);
    let proof = obj([
        ("schema", s("agent-memory/comparison-proof-v2")),
        ("judgment", judgment),
        ("disposition", s("review-required")),
        ("targets", Value::Array(vec![])),
    ]);
    let proof = reviewed(&f, &request, &proof, Some(&f.native));
    compare(&f, "check", &request, Some(&proof), 0);
    set(
        &mut request,
        "searches",
        Value::Array(vec![s("cap"), s("other")]),
    );
    let (result, _) = compare(&f, "inspect", &request, None, 1);
    assert_eq!(g(&result, "comparison_read_complete"), &Value::Bool(false));
    assert_eq!(bytes(&f.bank), before);
    assert_eq!(self::originals(&f), originals);
    assert_eq!(
        fs::read(f.directory.join("ledger-head.json")).unwrap(),
        head
    );
    assert_eq!(
        fs::read(f.directory.join("ledger/00000000.json")).unwrap(),
        initial
    );
}

fn reservation(transition: bool) {
    if !transition {
        eight_admission(false);
    }
    let mut f = Fixture::with_body(false, SCRATCH);
    // Finish the initial disposable fixture so each new inspection is over the same
    // canonical bank, with no prior pending source.
    let p = discard(&f);
    f.layout("resolve", vec!["--proposal".into(), f.save(&p)], 0);
    let raw = if transition {
        POLICY.repeat(16)
    } else {
        "abcdefghijklmnop".into()
    };
    fs::write(f.bank.join("handoff-prompt.md"), &raw).unwrap();
    let before = bytes(&f.bank);
    let mut accepted = vec![];
    let mut rejected = None;
    let mut last = None;
    for count in if transition { 8..17 } else { 16..17 } {
        let observed = f.private_layout("inspect", vec![]);
        f.operation = t(&observed, "operation_id").to_owned();
        f.manifest = g(&observed, "manifest").clone();
        let mut request = f.layout_request();
        let ranges = (0..count)
            .map(|number| {
                if transition {
                    (
                        number * POLICY.len(),
                        if number + 1 == count {
                            raw.len()
                        } else {
                            (number + 1) * POLICY.len()
                        },
                    )
                } else {
                    (number, number + 1)
                }
            })
            .collect::<Vec<_>>();
        partition_request(&mut request, &ranges);
        let out = f.world.inside(format!("reservation-{count}.json")).unwrap();
        let (code, value) = f.call(
            vec![
                "bank".into(),
                "recover-layout".into(),
                "prepare".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
                "--operation".into(),
                f.operation.clone().into(),
                "--input".into(),
                f.save(&request),
                "--output".into(),
                out.clone().into_os_string(),
            ],
            None,
        );
        assert_eq!(bytes(&f.bank), before);
        if code != 0 {
            assert!(!out.exists());
            assert_eq!(
                g(&value, "canonical_mutation_may_have_occurred"),
                &Value::Bool(false)
            );
            rejected = Some(count);
            break;
        }
        let proposal = read(&out);
        assert_eq!(
            array(g(g(&proposal, "proposal"), "pending_units")).len(),
            count
        );
        assert!(
            g(g(g(&proposal, "proposal"), "reservation"), "proof_bytes")
                .as_int()
                .unwrap()
                .as_u64()
                .unwrap()
                <= 65536
        );
        accepted.push(count);
        last = Some(proposal);
    }
    assert!(rejected.is_some());
    if transition {
        assert!(!accepted.is_empty());
        assert_eq!(rejected, accepted.last().map(|n| n + 1));
        let proposal = last.unwrap();
        let result = f.layout("apply", vec!["--proposal".into(), f.save(&proposal)], 0);
        assert_eq!(
            array(g(&result, "pending_unit_ids")).len(),
            *accepted.last().unwrap()
        );
        f.directory = PathBuf::from(
            f.world
                .env
                .get(std::ffi::OsStr::new("AGENT_MEMORY_STATE_HOME"))
                .unwrap(),
        )
        .join("writes")
        .join(bytes_sha(f.bank.to_str().unwrap().as_bytes()))
        .join("layout-recovery")
        .join(&f.operation);
        let (admission, _, _) = f.admission_selected(true, None, POLICY);
        let proof = g(
            g(g(&admission, "proposal"), "admission_evidence"),
            "comparison_proof",
        );
        assert_eq!(
            array(g(g(proof, "judgment"), "pending_decisions")).len(),
            *accepted.last().unwrap()
        );
        assert!(encoded(proof).unwrap().len() <= 65536);
        assert!(
            array(g(g(proof, "judgment"), "pending_decisions"))
                .iter()
                .all(|d| t(g(d, "coverage_claim"), "coverage") == "full")
        );
        f.good(vec![
            "admission".into(),
            "apply".into(),
            "--proposal".into(),
            f.save(&admission),
        ]);
        assert_eq!(
            g(
                &f.layout(
                    "status",
                    vec!["--operation".into(), f.operation.clone().into()],
                    0
                ),
                "semantic_recovery_complete"
            ),
            &Value::Bool(true)
        );
    } else {
        assert!(accepted.is_empty());
    }
}
fn version_pair() {
    let f = Fixture::configured(
        false,
        "# Approved policy\n\nThe source does not resolve which version applies.\n",
        |f| {
            let (mut old, old_path) = add_record(f, "historical-version", POLICY);
            set(&mut old, "status", s("superseded"));
            set(
                &mut old,
                "state-reason",
                s("The independently reviewed correction retains this historical version."),
            );
            let raw =
                agent_memory::codec::frontmatter::write(old.as_object().unwrap()).unwrap() + POLICY;
            fs::write(f.bank.join(&old_path), &raw).unwrap();
            set(&mut f.native, &old_path, s(raw));
            let (mut current, current_path) = add_record(
                f,
                "current-version",
                "# Revised policy\n\nThe corrected fixture allows four attempts.\n",
            );
            set(&mut current, "id", g(&old, "id").clone());
            set(&mut current, "version", n(2));
            set(
                &mut current,
                "corrects",
                Value::Array(vec![obj([
                    ("id", g(&old, "id").clone()),
                    ("version", g(&old, "version").clone()),
                ])]),
            );
            let raw = agent_memory::codec::frontmatter::write(current.as_object().unwrap())
                .unwrap()
                + "# Revised policy\n\nThe corrected fixture allows four attempts.\n";
            fs::write(f.bank.join(&current_path), &raw).unwrap();
            set(&mut f.native, &current_path, s(raw));
            let router = t(&f.native, "MEMORY.md").replace(
                "repository | active | When comparing fixture meaning",
                "repository | superseded | When comparing fixture meaning",
            );
            fs::write(f.bank.join("MEMORY.md"), &router).unwrap();
            set(&mut f.native, "MEMORY.md", s(router));
        },
        |req| {
            // Bind exact typed identities, including the retained historical version.
            let files = g(req, "intended_files").clone();
            for unit in req
                .get_mut("unit_map")
                .unwrap()
                .get_mut("units")
                .unwrap()
                .as_array_mut()
                .unwrap()
            {
                let target = array(g(unit, "targets"))
                    .first()
                    .map(|r| t(r, "path").to_owned());
                if let Some(path) = target {
                    if path.starts_with("topics/") {
                        let (meta, _) = unpack(t(&files, &path));
                        set(
                            unit,
                            "existing_record",
                            obj([
                                ("id", g(&meta, "id").clone()),
                                ("version", g(&meta, "version").clone()),
                            ]),
                        );
                    }
                }
            }
            bind_review(req);
        },
    );
    assert_eq!(
        fs::read_to_string(f.bank.join("topics/historical-version.md")).unwrap(),
        t(&f.native, "topics/historical-version.md")
    );
    assert_eq!(
        fs::read_to_string(f.bank.join("topics/current-version.md")).unwrap(),
        t(&f.native, "topics/current-version.md")
    );
    let mut request = candidate_request(&f);
    set(
        &mut request,
        "searches",
        Value::Array(vec![
            s("unrelated-boundary-one"),
            s("unrelated-boundary-two"),
        ]),
    );
    set(
        &mut request,
        "read_paths",
        Value::Array(vec![
            s("topics/historical-version.md"),
            s("topics/current-version.md"),
        ]),
    );
    let (_, view) = compare(&f, "inspect", &request, None, 0);
    let view = view.unwrap();
    let versions: std::collections::BTreeSet<_> = array(g(&view, "matches"))
        .iter()
        .map(|r| {
            (
                g(g(r, "metadata"), "version")
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                t(r, "lifecycle"),
            )
        })
        .collect();
    assert_eq!(
        versions,
        [(1, "superseded"), (2, "active")].into_iter().collect()
    );
    assert_eq!(array(g(&view, "pending_matches")).len(), 1);
    assert_eq!(
        g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0
            ),
            "semantic_recovery_complete"
        ),
        &Value::Bool(false)
    );
}

fn partial_union() {
    let f = Fixture::new();
    let pending = g(
        &f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        ),
        "pending_unit_ids",
    )
    .clone();
    let id = array(&pending)[0].as_str().unwrap();
    let (_, initial) = ledger(&f);
    let span = array(g(
        array(g(&initial, "units"))
            .iter()
            .find(|u| t(u, "unit_id") == id)
            .unwrap(),
        "source_ranges",
    ))[0]
        .clone();
    let start = g(&span, "start").as_int().unwrap().as_u64().unwrap() as usize;
    let end = g(&span, "end").as_int().unwrap().as_u64().unwrap() as usize;
    let middle = (start + end) / 2;
    let mut transactions = vec![];
    let mut receipts = vec![];
    let mut claims = vec![];
    for (a, b) in [(start, middle), (middle, end)] {
        let (proposal, _, target) = f.admission(false);
        let tx = g(&proposal, "proposal");
        let evidence = g(tx, "admission_evidence");
        let request = g(evidence, "comparison_request");
        let mut proof = g(evidence, "comparison_proof").clone();
        set(
            proof
                .get_mut("judgment")
                .unwrap()
                .get_mut("observation")
                .unwrap()
                .get_mut("recovery_frontier")
                .unwrap(),
            "review_digest",
            Value::Null,
        );
        let mut piece = span.clone();
        set(&mut piece, "start", n(a));
        set(&mut piece, "end", n(b));
        let metadata = g(g(request, "candidate"), "metadata");
        let claim = obj([
            ("unit_id", s(id)),
            ("coverage", s("partial")),
            ("covered_source_ranges", Value::Array(vec![piece])),
            (
                "target_record_refs",
                Value::Array(vec![obj([
                    ("id", g(metadata, "id").clone()),
                    ("version", n(1)),
                    ("path", s(&target)),
                    (
                        "resulting_sha256",
                        s(bytes_sha(t(g(tx, "files"), &target).as_bytes())),
                    ),
                ])]),
            ),
        ]);
        set(
            &mut proof
                .get_mut("judgment")
                .unwrap()
                .get_mut("pending_decisions")
                .unwrap()
                .as_array_mut()
                .unwrap()[0],
            "coverage_claim",
            claim.clone(),
        );
        let reviewed = reviewed(&f, request, &proof, Some(g(tx, "files")));
        let actual = private_call(
            &f,
            vec![
                "admission".into(),
                "preview".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
                "--capture".into(),
                f.save(g(evidence, "capture")),
                "--request".into(),
                f.save(request),
                "--proof".into(),
                f.save(&reviewed),
                "--operation-id".into(),
                t(tx, "operation_id").into(),
                "--recorded-at".into(),
                t(evidence, "recorded_at").into(),
            ],
            0,
        );
        let result = f.good(vec![
            "admission".into(),
            "apply".into(),
            "--proposal".into(),
            f.save(&actual),
        ]);
        let receipt = read(Path::new(t(&result, "receipt_path")));
        let tx = g(&actual, "proposal").clone();
        receipts.push(obj([
            ("operation_id", g(&tx, "operation_id").clone()),
            ("receipt_sha256", s(sha(&receipt))),
            (
                "proof_sha256",
                s(sha(g(g(&tx, "admission_evidence"), "comparison_proof"))),
            ),
        ]));
        claims.push(claim);
        transactions.push(tx.clone());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(g(&status, "pending_unit_ids"), &pending);
        assert_eq!(
            g(&status, "semantic_recovery_complete"),
            &Value::Bool(false)
        );
        let out = f
            .world
            .inside(format!("partial-single-{}.json", uuid()))
            .unwrap();
        f.layout(
            "resolve",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--unit".into(),
                id.into(),
                "--input".into(),
                f.save(&obj([("transaction_proposal", tx)])),
                "--output".into(),
                out.clone().into_os_string(),
            ],
            1,
        );
        assert!(!out.exists());
    }
    let (head, ledger) = ledger(&f);
    let union = obj([
        ("schema", s("agent-memory/layout-coverage-review-v1")),
        ("review_id", s(uuid())),
        ("operation_id", s(&f.operation)),
        ("reviewer_developer_id", s(&f.actor)),
        ("reviewer_harness", s("union-reviewer")),
        ("reviewer_session", s(uuid())),
        ("review_time", g(&f.manifest, "inspected_at").clone()),
        ("manifest_sha256", g(&ledger, "manifest_sha256").clone()),
        ("unit_map_sha256", g(&ledger, "unit_map_sha256").clone()),
        ("ledger_head_sha256", g(&head, "sha256").clone()),
        (
            "comparison_proof_input_sha256",
            s(sha(&Value::Array(receipts))),
        ),
        (
            "intended_output_sha256",
            s(sha(g(transactions.last().unwrap(), "files"))),
        ),
        ("claims", Value::Array(claims)),
        (
            "rationale",
            s("Independent review of both complete nonoverlapping committed fragments."),
        ),
        ("decision", s("approve")),
    ]);
    let terminal = obj([
        ("transaction_proposals", Value::Array(transactions)),
        ("coverage_review", union),
    ]);
    let resolution = f.private_layout(
        "resolve",
        vec![
            "--operation".into(),
            f.operation.clone().into(),
            "--unit".into(),
            id.into(),
            "--input".into(),
            f.save(&terminal),
        ],
    );
    let result = f.layout("resolve", vec!["--proposal".into(), f.save(&resolution)], 0);
    assert_eq!(g(&result, "semantic_recovery_complete"), &Value::Bool(true));
    assert!(
        array(g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0
            ),
            "pending_unit_ids"
        ))
        .is_empty()
    );
}
fn route_frontiers() {
    let f = Fixture::configured(
        false,
        POLICY,
        |f| {
            add_record(f, "route-first", POLICY);
            add_record(f, "route-second", POLICY);
        },
        |_| {},
    );
    let before = bytes(&f.bank);
    for disposition in [
        "update-existing",
        "consolidate-existing",
        "governance-required",
    ] {
        let mut request = candidate_request(&f);
        let (metadata, body) =
            unpack(&fs::read_to_string(f.bank.join("topics/route-first.md")).unwrap());
        let mut candidate = metadata;
        if disposition == "governance-required" {
            set(
                &mut candidate,
                "conditions",
                obj([("branch", s("older/release"))]),
            );
        }
        set(
            &mut request,
            "candidate",
            obj([("metadata", candidate), ("body", s(body))]),
        );
        set(
            &mut request,
            "searches",
            Value::Array(vec![s("unmatched-lexical-one"), s("unmatched-lexical-two")]),
        );
        set(
            &mut request,
            "read_paths",
            Value::Array(if disposition == "consolidate-existing" {
                vec![s("topics/route-first.md"), s("topics/route-second.md")]
            } else {
                vec![s("topics/route-first.md")]
            }),
        );
        let (_, view) = compare(&f, "inspect", &request, None, 0);
        let view = view.unwrap();
        assert_eq!(array(g(&view, "pending_matches")).len(), 1);
        let judgment=obj([("observation",g(&view,"observation").clone()),("decisions",Value::Array(array(g(&view,"matches")).iter().map(|r|obj([("path",g(r,"path").clone()),("classification",s(if disposition=="governance-required"{"scope-correction"}else{"equivalent"})),("rationale",s("Independent review retains the observed conditions."))])).collect())),("pending_decisions",Value::Array(array(g(&view,"pending_matches")).iter().map(|r|obj([("unit_id",g(r,"unit_id").clone()),("classification",s("distinct-knowledge")),("rationale",s("Unconfirmed source remains pending without coverage credit.")),("coverage_claim",obj([("unit_id",g(r,"unit_id").clone()),("coverage",s("none")),("covered_source_ranges",Value::Array(vec![])),("target_record_refs",Value::Array(vec![]))]))])).collect())),("rationale",s("Complete source-bound review."))]);
        let targets = array(g(&view, "matches"))
            .iter()
            .map(|r| {
                obj([
                    ("path", g(r, "path").clone()),
                    ("sha256", g(r, "sha256").clone()),
                    ("id", g(g(r, "metadata"), "id").clone()),
                    ("version", g(g(r, "metadata"), "version").clone()),
                    ("required_preserved_metadata", g(r, "metadata").clone()),
                    ("scope_chain", g(r, "scope_chain").clone()),
                ])
            })
            .collect();
        let proof = obj([
            ("schema", s("agent-memory/comparison-proof-v2")),
            ("judgment", judgment),
            ("disposition", s(disposition)),
            ("targets", Value::Array(targets)),
        ]);
        let proof = reviewed(&f, &request, &proof, Some(&f.native));
        assert_eq!(
            array(g(g(&proof, "judgment"), "pending_decisions"))
                .iter()
                .map(|r| t(g(r, "coverage_claim"), "coverage"))
                .collect::<Vec<_>>(),
            vec!["none"]
        );
    }
    assert_eq!(bytes(&f.bank), before);
    assert_eq!(
        array(g(
            &f.layout(
                "status",
                vec!["--operation".into(), f.operation.clone().into()],
                0
            ),
            "pending_unit_ids"
        ))
        .len(),
        1
    );
}

fn catalog_retirement() {
    let f = Fixture::with_body(false, SCRATCH);
    let mut request = obj([
        ("project_id", s(&f.project)),
        ("query", s("")),
        ("ids", Value::Array(vec![])),
        ("paths", Value::Array(vec![s("projectbrief.md")])),
        ("context", obj([])),
        ("kinds", Value::Array(vec![])),
        ("minimum_trust", s("reviewed")),
        ("limit", n(8)),
        ("include_context", Value::Bool(true)),
    ]);
    let trace = |request: &mut Value| {
        let value = private_call(
            &f,
            vec![
                "discover".into(),
                "start".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
                "--intent".into(),
                "recall".into(),
                "--task".into(),
                "Review the project orientation.".into(),
            ],
            0,
        );
        let mut selected = g(&value, "discovery").clone();
        set(
            &mut selected,
            "sources",
            Value::Array(vec![obj([
                ("path", s("projectbrief.md")),
                (
                    "sha256",
                    s(bytes_sha(
                        &fs::read(f.bank.join("projectbrief.md")).unwrap(),
                    )),
                ),
            ])]),
        );
        set(&mut selected, "complete", Value::Bool(true));
        set(
            &mut selected,
            "rationale",
            s("Read the selected project orientation source."),
        );
        set(request, "discovery", selected);
    };
    let search = |request: &Value, disabled: bool| {
        let env = if disabled {
            vec![("AGENT_MEMORY_NO_RECALL_CATALOG".into(), "1".into())]
        } else {
            vec![]
        };
        let output = f
            .world
            .run_with(
                [
                    f.command.clone().into_os_string(),
                    "search".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                    "--request".into(),
                    f.save(request),
                ],
                RunOptions {
                    env: &env,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(output.status, agent_memory::process::Status::Code(0));
        json::decode_bytes(&output.stdout, Default::default()).unwrap()
    };
    trace(&mut request);
    let cold = search(&request, true);
    search(&request, false);
    let warm = search(&request, false);
    assert_eq!(g(&warm, "optional_indexes_used"), &Value::Bool(true));
    fn pointer(root: &Path) -> Option<PathBuf> {
        for e in fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if let Some(p) = pointer(&p) {
                    return Some(p);
                }
            } else if p.file_name().unwrap() == "current.json"
                && p.parent().unwrap().file_name().unwrap() == "recall-catalog-v1"
            {
                return Some(p);
            }
        }
        None
    }
    let pointer = pointer(&f.world.paths["state"]).unwrap();
    let pending = g(
        &f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        ),
        "pending_unit_ids",
    )
    .clone();
    for result in [&cold, &warm] {
        assert_eq!(t(result, "semantic_coverage"), "incomplete-recovery");
        assert_eq!(g(result, "pending_recovery_unit_ids"), &pending);
        assert_eq!(g(result, "pending_recovery_units"), &n(1));
        assert!(!array(g(result, "recovery_required_actions")).is_empty());
    }
    assert_eq!(g(&cold, "results"), g(&warm, "results"));
    let before = f.snapshot();
    let proposal = discard(&f);
    let result = f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
    assert_eq!(t(&result, "operation"), "resolve");
    assert_eq!(t(&result, "state"), "resolved");
    assert_eq!(g(&result, "semantic_recovery_complete"), &Value::Bool(true));
    assert_eq!(
        g(&result, "private_artifact_writes_performed"),
        &Value::Bool(true)
    );
    let replay = f.layout("resolve", vec!["--proposal".into(), f.save(&proposal)], 0);
    assert_eq!(t(&replay, "state"), "historical");
    assert_eq!(g(&replay, "receipt_is_historical"), &Value::Bool(true));
    for field in [
        "writes_performed",
        "private_artifact_writes_performed",
        "temporary_lock_writes_performed",
        "semantic_recovery_complete",
    ] {
        assert_eq!(g(&replay, field), &Value::Bool(false));
    }
    assert!(!pointer.exists());
    assert_eq!(f.snapshot(), before);
    trace(&mut request);
    let enabled = search(&request, false);
    let disabled = search(&request, true);
    assert_eq!(g(&enabled, "results"), g(&cold, "results"));
    assert_eq!(g(&enabled, "results"), g(&disabled, "results"));
    for result in [&enabled, &disabled] {
        assert_eq!(
            t(result, "semantic_coverage"),
            "complete-canonical-observation"
        );
        assert!(array(g(result, "pending_recovery_unit_ids")).is_empty());
        assert_eq!(g(result, "pending_recovery_units"), &n(0));
        assert!(array(g(result, "recovery_required_actions")).is_empty());
    }
}
