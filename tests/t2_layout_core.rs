//! layout_resolution, layout_units, layout_restore, layout_recovery_context,
//! layout_recovery_temps and recovery_quiescence on real private files.
//!
//! The public journeys of the baseline contracts drive `am bank
//! recover-layout`, which needs layout_recovery and recovery_frontier; the
//! probe-ownership halves are in `tests/t2_layout_core_probes.rs`, the rest
//! is deferred until those modules are ported.
use agent_memory::codec::json::Value;
use agent_memory::developer_identity::{digest, encoded};
use agent_memory::error::Kind;
use agent_memory::{
    layout_recovery_temps as temps, layout_resolution as resolution, layout_restore as restore,
    layout_units as units, recovery_quiescence as quiescence,
};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

const OPERATION: &str = "11111111-1111-4111-8111-111111111111";
const PROJECT: &str = "22222222-2222-4222-8222-222222222222";

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn n(value: i64) -> Value {
    Value::Int(value.into())
}
fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn sha(value: &Value) -> String {
    digest(Some(&encoded(value).unwrap())).unwrap()
}
fn bytes_sha(raw: &[u8]) -> String {
    digest(Some(raw)).unwrap()
}
fn refusal<T: std::fmt::Debug>(result: agent_memory::error::Result<T>, expected: &str) {
    assert_eq!(result.unwrap_err().to_string(), expected);
}

/// A private 0700 directory under canonical /tmp, removed on drop.
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let tmp = fs::canonicalize("/tmp").unwrap();
        let path = tmp.join(format!(
            "am-layoutcore-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn join(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
    fn bank(&self) -> PathBuf {
        let bank = self.join("bank");
        fs::create_dir(&bank).unwrap();
        fs::set_permissions(&bank, fs::Permissions::from_mode(0o700)).unwrap();
        bank
    }
    fn private(&self, name: &str) -> PathBuf {
        let directory = self.join(name);
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        directory
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, body: &[u8], mode: u32) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn xattrs(path: &Path) -> Value {
    agent_memory::layout_recovery_observation::metadata_supported(path).unwrap()
}
fn facts(path: &Path) -> Value {
    let info = fs::symlink_metadata(path).unwrap();
    object([
        ("uid", n(i64::from(info.uid()))),
        ("gid", n(i64::from(info.gid()))),
        ("mode", n(i64::from(info.mode() & 0o7777))),
        ("xattrs", xattrs(path)),
    ])
}
fn names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

// ---- layout_recovery_temps ----

#[test]
fn probe_plan_derives_every_shadow_name_before_mutation() {
    let root = Root::new();
    let bank = root.bank();
    write(&bank.join("a.md"), b"A\n", 0o644);
    let directory_facts = facts(&bank);
    let file_facts = facts(&bank.join("a.md"));
    let directories = object([("topics/nested", directory_facts.clone())]);
    let requirement = object([
        ("path", s("topics/nested/a.md")),
        ("facts", file_facts.clone()),
    ]);
    let plan =
        temps::probe_plan(OPERATION, std::slice::from_ref(&requirement), &directories).unwrap();
    assert_eq!(
        plan,
        temps::probe_plan(OPERATION, std::slice::from_ref(&requirement), &directories).unwrap()
    );
    let tasks = get(&plan, "tasks").as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(get(&tasks[0], "type"), &s("directory"));
    assert_eq!(get(&tasks[1], "type"), &s("file"));
    // Baseline: one shadow route for the planned directory; the file has a
    // direct route plus one through the shadow of its planned parent.
    let directory_routes = get(&tasks[0], "routes").as_array().unwrap();
    assert_eq!(directory_routes.len(), 1);
    let file_routes = get(&tasks[1], "routes").as_array().unwrap();
    assert_eq!(file_routes.len(), 2);
    let direct = file_routes[0].as_array().unwrap();
    assert_eq!(direct.len(), 1);
    let shadowed = file_routes[1].as_array().unwrap();
    assert_eq!(shadowed.len(), 2);
    let prefix = format!(".agent-memory-probe-{OPERATION}-");
    let shadow = get(&shadowed[0], "path").as_str().unwrap();
    // The shadow parent name binds {"directory", "parent"} at the original parent.
    let label = object([("directory", s("topics/nested")), ("parent", s("topics"))]);
    assert_eq!(shadow, format!("topics/{prefix}{}", &sha(&label)[..20]));
    let label = object([
        ("target", s("topics/nested/a.md")),
        ("facts", file_facts.clone()),
        ("parent", s("topics/nested")),
    ]);
    assert_eq!(
        get(&direct[0], "path").as_str().unwrap(),
        format!("topics/nested/{prefix}{}", &sha(&label)[..20])
    );
    assert_eq!(
        get(&shadowed[1], "path")
            .as_str()
            .unwrap()
            .split('/')
            .count(),
        3
    );
    // Validation refuses before any name is used.
    refusal(
        temps::probe_plan("not-a-uuid", &[], &Value::Object(Vec::new())),
        "Invalid metadata probe operation identity",
    );
    let mut wide = file_facts.clone();
    if let Value::Object(pairs) = &mut wide {
        pairs.retain(|(key, _)| key != "mode");
        pairs.push(("mode".into(), n(0o17777)));
    }
    refusal(
        temps::probe_plan(
            OPERATION,
            &[object([("path", s("a.md")), ("facts", wide)])],
            &Value::Object(Vec::new()),
        ),
        "Invalid metadata probe facts",
    );
    refusal(
        temps::probe_plan(
            OPERATION,
            &[object([("path", s("a.md"))])],
            &Value::Object(Vec::new()),
        ),
        "Invalid metadata probe requirement",
    );
    refusal(
        temps::validate_probe_plan(&plan, &[], &directories),
        "Metadata probe plan differs from authorized requirements",
    );
}

#[test]
fn probe_proves_reviewed_metadata_then_removes_every_owned_probe() {
    let root = Root::new();
    let bank = root.bank();
    let directory = root.private("operation");
    write(&bank.join("a.md"), b"Original\n", 0o640);
    let before = names(&bank);
    let requirement = object([("path", s("a.md")), ("facts", facts(&bank.join("a.md")))]);
    let plan = temps::probe_plan(OPERATION, &[requirement], &Value::Object(Vec::new())).unwrap();
    temps::probe(&bank, &plan, &directory, None, None, None, None).unwrap();
    assert_eq!(names(&bank), before);
    assert_eq!(fs::read(bank.join("a.md")).unwrap(), b"Original\n");
    assert!(
        temps::probe_entries(&bank, &plan, &directory)
            .unwrap()
            .is_empty()
    );
    let attempts = directory.join("metadata-probe-attempts");
    let attempt_ids = names(&attempts);
    assert_eq!(attempt_ids.len(), 1);
    let attempt = attempts.join(&attempt_ids[0]);
    assert_eq!(names(&attempt), ["completion.json", "pins", "plan.json"]);
    assert_eq!(names(&attempt.join("pins")).len(), 1);
    let journal: Value =
        agent_memory::developer_identity::load(&fs::read(attempt.join("plan.json")).unwrap())
            .unwrap();
    let completion: Value =
        agent_memory::developer_identity::load(&fs::read(attempt.join("completion.json")).unwrap())
            .unwrap();
    assert_eq!(
        completion,
        object([
            ("attempt_id", s(attempt_ids[0].as_str())),
            ("operation_id", s(OPERATION)),
            ("plan_sha256", s(sha(&journal))),
            (
                "schema",
                s("agent-memory/layout-metadata-probe-completion-v1")
            ),
        ])
    );
    assert_eq!(get(&journal, "plan_sha256"), &s(sha(&plan)));
    for path in [attempts.clone(), attempt.clone(), attempt.join("pins")] {
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o7777, 0o700);
    }
    // A completed attempt keeps its evidence; a new probe still refuses no work.
    temps::probe_complete(&bank, &plan, &directory).unwrap();
}

#[test]
fn unreproducible_metadata_is_pending_and_probes_are_still_removed() {
    let root = Root::new();
    let bank = root.bank();
    let directory = root.private("operation");
    write(&bank.join("a.md"), b"Original\n", 0o600);
    let mut wanted = facts(&bank.join("a.md"));
    if let Value::Object(pairs) = &mut wanted {
        let uid = pairs.iter_mut().find(|(key, _)| key == "uid").unwrap();
        uid.1 = n(i64::from(fs::metadata(&bank).unwrap().uid()) + 1);
    }
    let plan = temps::probe_plan(
        OPERATION,
        &[object([("path", s("a.md")), ("facts", wanted)])],
        &Value::Object(Vec::new()),
    )
    .unwrap();
    refusal(
        temps::probe(&bank, &plan, &directory, None, None, None, None),
        "pending-metadata-preservation",
    );
    assert_eq!(names(&bank), ["a.md"]);
    assert!(
        temps::probe_entries(&bank, &plan, &directory)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unknown_attempt_evidence_and_unowned_probes_are_preserved() {
    let root = Root::new();
    let bank = root.bank();
    let directory = root.private("operation");
    write(&bank.join("a.md"), b"Original\n", 0o600);
    let plan = temps::probe_plan(
        OPERATION,
        &[object([
            ("path", s("a.md")),
            ("facts", facts(&bank.join("a.md"))),
        ])],
        &Value::Object(Vec::new()),
    )
    .unwrap();
    temps::probe(&bank, &plan, &directory, None, None, None, None).unwrap();
    let attempts = directory.join("metadata-probe-attempts");
    let attempt = attempts.join(&names(&attempts)[0]);
    write(&attempt.join("foreign.json"), b"{}\n", 0o600);
    refusal(
        temps::probe_entries(&bank, &plan, &directory),
        "Unknown metadata probe attempt evidence",
    );
    assert!(attempt.join("foreign.json").exists());
    fs::remove_file(attempt.join("foreign.json")).unwrap();
    // A pin record naming a node that is not in the journal is never trusted.
    let pins = attempt.join("pins");
    let stray = format!(
        ".agent-memory-probe-{}.json",
        agent_memory::host::uuid4().unwrap()
    );
    write(&pins.join(&stray), b"{}\n", 0o600);
    refusal(
        temps::probe_entries(&bank, &plan, &directory),
        "Unknown metadata probe pin evidence",
    );
    assert!(pins.join(&stray).exists());
}

#[test]
fn only_exact_journal_temporaries_are_recognized_and_cleaned() {
    let root = Root::new();
    let bank = root.bank();
    write(&bank.join("a.md"), b"Final\n", 0o600);
    let temporary = temps::source_name(OPERATION, "a.md");
    let expected_name = format!(
        ".agent-memory-layout-{OPERATION}-{}.tmp",
        &bytes_sha(b"a.md")[..16]
    );
    assert_eq!(temporary, expected_name);
    assert_eq!(
        temps::source_name(OPERATION, "topics/a.md"),
        format!(
            "topics/.agent-memory-layout-{OPERATION}-{}.tmp",
            &bytes_sha(b"topics/a.md")[..16]
        )
    );
    write(&bank.join(&temporary), b"Original\n", 0o600);
    let row = |body: &[u8]| {
        let mut row = facts(&bank.join("a.md"));
        if let Value::Object(pairs) = &mut row {
            pairs.push(("path".into(), s("a.md")));
            pairs.push(("sha256".into(), s(bytes_sha(body))));
        }
        row
    };
    let actions = [object([
        ("path", s("a.md")),
        ("before", s(bytes_sha(b"Original\n"))),
        ("after", s(bytes_sha(b"Final\n"))),
    ])];
    let base = [row(b"Original\n")];
    let finals = [("a.md".to_owned(), row(b"Final\n"))].into_iter().collect();
    let pins = temps::entries(&bank, OPERATION, &actions, &base, &finals, &[]).unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(get(&pins[0], "path"), &s(temporary.as_str()));
    assert_eq!(get(&pins[0], "sha256"), &s(bytes_sha(b"Original\n")));
    // A partial or foreign temporary is preserved for explicit inspection.
    write(&bank.join(&temporary), b"Origin", 0o600);
    refusal(
        temps::entries(&bank, OPERATION, &actions, &base, &finals, &[]),
        "Unknown, partial or changed transaction temporary; preserve it for explicit inspection",
    );
    refusal(
        temps::clean(&bank, &pins, None, None),
        "Known recovery temporary changed after preview",
    );
    assert!(bank.join(&temporary).exists());
    write(&bank.join(&temporary), b"Original\n", 0o600);
    let pins = temps::entries(&bank, OPERATION, &actions, &base, &finals, &[]).unwrap();
    temps::clean(&bank, &pins, None, None).unwrap();
    assert_eq!(names(&bank), ["a.md"]);
    // Marker temporaries bind the owner, inherited group and 0600.
    let marker = encoded(&object([
        ("schema", s("agent-memory/layout-recovery-transaction-v1")),
        ("project_id", s(PROJECT)),
        ("operation_id", s(OPERATION)),
        ("transaction_kind", s("rollback")),
    ]))
    .unwrap();
    let marker_name = format!(".agent-memory-layout-marker-{OPERATION}.tmp");
    write(&bank.join(&marker_name), &marker, 0o600);
    let pins = temps::entries(
        &bank,
        OPERATION,
        &[],
        &[],
        &Default::default(),
        std::slice::from_ref(&marker),
    )
    .unwrap();
    assert_eq!(get(&pins[0], "path"), &s(marker_name.as_str()));
    fs::set_permissions(bank.join(&marker_name), fs::Permissions::from_mode(0o644)).unwrap();
    refusal(
        temps::entries(&bank, OPERATION, &[], &[], &Default::default(), &[marker]),
        "Unknown, partial or changed transaction temporary; preserve it for explicit inspection",
    );
}

// ---- layout_restore ----

fn view_row(path: &str, sha: &str) -> Value {
    object([
        ("path", s(path)),
        ("type", s("file")),
        ("runtime", Value::Bool(false)),
        ("uid", n(501)),
        ("gid", n(20)),
        ("mode", n(0o100600)),
        ("sha256", s(sha)),
        ("xattrs", Value::Object(Vec::new())),
    ])
}

fn restore_proposal(base: Vec<Value>, desired: Vec<Value>, actions: Vec<Value>) -> Value {
    object([
        ("base_view", Value::Array(base)),
        ("desired_view", Value::Array(desired)),
        ("actions", Value::Array(actions)),
        ("directories_to_remove", Value::Object(Vec::new())),
    ])
}

#[test]
fn child_restore_boundary_accepts_only_an_ordered_action_prefix() {
    let a0 = "a".repeat(64);
    let a1 = "b".repeat(64);
    let r0 = "c".repeat(64);
    let r1 = "d".repeat(64);
    let base = vec![view_row("a.md", &a0), view_row("MEMORY.md", &r0)];
    let desired = vec![view_row("a.md", &a1), view_row("MEMORY.md", &r1)];
    let actions = vec![
        object([("path", s("a.md"))]),
        object([("path", s("MEMORY.md"))]),
    ];
    let p = restore_proposal(base.clone(), desired.clone(), actions);
    assert_eq!(restore::boundary(&p, &base, None).unwrap(), 0);
    let first = vec![view_row("a.md", &a1), view_row("MEMORY.md", &r0)];
    assert_eq!(restore::boundary(&p, &first, None).unwrap(), 1);
    assert_eq!(restore::boundary(&p, &desired, None).unwrap(), 2);
    // Router before its target is not a legal prefix; an external edit never is.
    let out_of_order = vec![view_row("a.md", &a0), view_row("MEMORY.md", &r1)];
    let impossible = "Child restore contains an impossible partial ordering or external edit";
    refusal(restore::boundary(&p, &out_of_order, None), impossible);
    let edited = vec![
        view_row("a.md", &a0),
        view_row("MEMORY.md", &r0),
        view_row("extra.md", &a0),
    ];
    refusal(restore::boundary(&p, &edited, None), impossible);
    // A retained reverse plan replaces the proposal views.
    let reverse = object([
        ("initial_view", Value::Array(first.clone())),
        ("desired_view", Value::Array(base.clone())),
        ("actions", Value::Array(vec![object([("path", s("a.md"))])])),
    ]);
    assert_eq!(restore::boundary(&p, &first, Some(&reverse)).unwrap(), 0);
    assert_eq!(restore::boundary(&p, &base, Some(&reverse)).unwrap(), 1);
}

#[test]
fn child_restore_probe_plan_covers_both_views_of_every_action() {
    let base = vec![view_row("a.md", &"a".repeat(64))];
    let mut desired_row = view_row("a.md", &"b".repeat(64));
    if let Value::Object(pairs) = &mut desired_row {
        pairs.iter_mut().find(|(key, _)| key == "mode").unwrap().1 = n(0o100640);
    }
    let mut p = restore_proposal(
        base,
        vec![desired_row, view_row("unchanged.md", &"c".repeat(64))],
        vec![object([("path", s("a.md"))])],
    );
    if let Value::Object(pairs) = &mut p {
        pairs.push(("operation_id".into(), s(OPERATION)));
    }
    let plan = restore::metadata_probes(&p).unwrap();
    let tasks = get(&plan, "tasks").as_array().unwrap();
    let modes: Vec<&Value> = tasks
        .iter()
        .map(|task| get(get(task, "facts"), "mode"))
        .collect();
    assert_eq!(tasks.len(), 2);
    assert!(modes.contains(&&n(0o600)));
    assert!(modes.contains(&&n(0o640)));
}

#[test]
fn layout_recovery_seam_refuses_without_a_registered_provider() {
    let missing = |result: agent_memory::error::Result<_>| {
        let error: agent_memory::error::ContractError = result.map(|_: Value| ()).unwrap_err();
        assert!(matches!(error.kind(), Kind::MissingProvider), "{error}");
    };
    let root = Root::new();
    missing(restore::value(&root.0, &Value::Object(Vec::new()), &root.0));
    missing(units::apply(&root.0, &Value::Object(Vec::new()), &root.0));
    missing(
        units::revised(
            &Value::Object(Vec::new()),
            &Value::Object(Vec::new()),
            &Value::Array(vec![Value::Null]),
        )
        .map(|_| Value::Null),
    );
}

// ---- layout_resolution ----

fn consolidation(proof_review: Value) -> (Value, Value) {
    let proof = object([(
        "judgment",
        object([
            (
                "observation",
                object([(
                    "recovery_frontier",
                    object([
                        ("review_digest", proof_review),
                        ("reference_map_sha256", s("e".repeat(64))),
                    ]),
                )]),
            ),
            (
                "pending_decisions",
                Value::Array(vec![object([
                    ("unit_id", s(OPERATION)),
                    ("coverage_claim", object([("coverage", s("full"))])),
                ])]),
            ),
        ]),
    )]);
    let value = object([("comparison_proof", proof.clone()), ("intent", s("merge"))]);
    (proof, value)
}

fn governed(recorded_at: &str) -> Value {
    object([
        ("operation", s("compact")),
        ("repo_root", s("/repo")),
        ("project_id", s(PROJECT)),
        ("operation_id", s(OPERATION)),
        ("observed_hashes", object([("a.md", s("a".repeat(64)))])),
        ("files", object([("a.md", s("A\n"))])),
        (
            "consolidation_evidence",
            object([("recorded_at", s(recorded_at))]),
        ),
    ])
}

#[test]
fn output_plans_bind_semantic_input_but_not_later_coverage_review() {
    let (_, value) = consolidation(Value::Null);
    let p = governed("2026-11-01T10:00:00+00:00");
    let plan = resolution::make_output_plan(&p, &value).unwrap();
    assert_eq!(get(&plan, "schema"), &s(resolution::OUTPUT_PLAN));
    assert_eq!(get(&plan, "recorded_at"), &s("2026-11-01T10:00:00+00:00"));
    // The later review digest and coverage claims are outside the planning digest.
    let (_, reviewed) = consolidation(s("f".repeat(64)));
    let (operation, recorded) = resolution::output_plan_context(&plan, &reviewed).unwrap();
    assert_eq!(
        (operation, recorded),
        (s(OPERATION), s("2026-11-01T10:00:00+00:00"))
    );
    resolution::check_output_plan(&p, &reviewed, &plan).unwrap();
    // Different meaning, rebuilt bytes or future time refuse.
    let changed = object([
        ("comparison_proof", get(&value, "comparison_proof").clone()),
        ("intent", s("split")),
    ]);
    refusal(
        resolution::output_plan_context(&plan, &changed),
        "Output plan differs from the exact governed request and comparison input",
    );
    let mut rebuilt = p.clone();
    if let Value::Object(pairs) = &mut rebuilt {
        pairs.iter_mut().find(|(key, _)| key == "files").unwrap().1 = object([("a.md", s("B\n"))]);
    }
    refusal(
        resolution::check_output_plan(&rebuilt, &value, &plan),
        "Current governed output or source differs from the independently reviewed plan",
    );
    let future =
        resolution::make_output_plan(&governed("2999-01-01T00:00:00+00:00"), &value).unwrap();
    refusal(
        resolution::output_plan_context(&future, &value),
        "Output plan cannot supply a future event time",
    );
}

#[test]
fn evidence_fields_and_resolution_records_have_baseline_shapes() {
    let empty = resolution::evidence_fields(None).unwrap();
    assert_eq!(
        empty,
        object([
            ("unit_resolution_claims", Value::Array(Vec::new())),
            ("comparison_proof_sha256", Value::Null),
            ("reference_map_sha256", s(bytes_sha(b"{}\n"))),
        ])
    );
    let (proof, _) = consolidation(Value::Null);
    let fields = resolution::evidence_fields(Some(&proof)).unwrap();
    assert_eq!(get(&fields, "comparison_proof_sha256"), &s(sha(&proof)));
    assert_eq!(get(&fields, "reference_map_sha256"), &s("e".repeat(64)));
    assert_eq!(
        get(&fields, "unit_resolution_claims"),
        &Value::Array(vec![object([("coverage", s("full"))])])
    );
    let p = object([
        ("resolution_id", s(OPERATION)),
        ("operation_id", s(OPERATION)),
        ("unit_id", s(PROJECT)),
        ("ledger_head", object([("generation", n(3))])),
        ("terminal_state", s("admitted")),
        ("canonical_operation_id", s(PROJECT)),
        ("canonical_receipt_sha256", s("a".repeat(64))),
        ("claims", Value::Array(Vec::new())),
        ("independent_review_sha256", s("b".repeat(64))),
    ]);
    let receipt = resolution::resolution_receipt(&p).unwrap();
    assert_eq!(get(&receipt, "generation"), &n(3));
    assert_eq!(get(&receipt, "request_sha256"), &s(sha(&p)));
    assert_eq!(get(&receipt, "verified"), &Value::Bool(true));
    let journal = object([("schema", s("x"))]);
    let completion = resolution::resolution_completion(OPERATION, &receipt, &journal).unwrap();
    assert_eq!(
        completion,
        object([
            ("schema", s("agent-memory/layout-resolution-completion-v1")),
            ("resolution_id", s(OPERATION)),
            ("receipt_sha256", s(sha(&receipt))),
            ("journal_sha256", s(sha(&journal))),
            ("verified", Value::Bool(true)),
        ])
    );
}

#[test]
fn atomic_marker_hands_off_exactly_and_leaves_no_temporary() {
    let root = Root::new();
    let bank = root.bank();
    let old = b"old marker\n".to_vec();
    write(&bank.join(".agent-memory-transaction.md"), &old, 0o600);
    resolution::atomic_marker(&bank, Some(&old), b"new marker\n", None).unwrap();
    assert_eq!(
        fs::read(bank.join(".agent-memory-transaction.md")).unwrap(),
        b"new marker\n"
    );
    assert_eq!(names(&bank), [".agent-memory-transaction.md"]);
    // A changed expected marker refuses before any temporary is created.
    refusal(
        resolution::atomic_marker(&bank, Some(&old), b"other\n", None),
        "Bank ownership changed; preserve work and retry with actual ownership",
    );
    assert_eq!(names(&bank), [".agent-memory-transaction.md"]);
}

// ---- recovery_quiescence ----

fn attested(digest_value: &str) -> (Value, Value, Value) {
    let attestation = object([
        ("runtime_artifact_digest", s(digest_value)),
        ("review_id", s(PROJECT)),
    ]);
    let review = object([
        ("review_id", s(PROJECT)),
        ("quiescence_attestation_sha256", s(sha(&attestation))),
    ]);
    let proposal = object([
        ("quiescence_attestation", attestation.clone()),
        ("quiescence_attestation_sha256", s(sha(&attestation))),
        ("review", review.clone()),
        ("review_sha256", s(sha(&review))),
    ]);
    (attestation, review, proposal)
}

#[test]
fn selected_attestation_prefers_retained_renewal_heads() {
    let root = Root::new();
    let directory = root.private("operation");
    let runtime = "r".repeat(64);
    let (attestation, _, proposal) = attested(&runtime);
    assert_eq!(
        quiescence::selected_at(&directory, &proposal, &runtime).unwrap(),
        Some(attestation.clone())
    );
    assert_eq!(
        quiescence::selected_at(&directory, &proposal, &"x".repeat(64)).unwrap(),
        None
    );
    let mut tampered = proposal.clone();
    if let Value::Object(pairs) = &mut tampered {
        pairs
            .iter_mut()
            .find(|(key, _)| key == "review_sha256")
            .unwrap()
            .1 = s("0".repeat(64));
    }
    assert_eq!(
        quiescence::selected_at(&directory, &tampered, &runtime).unwrap(),
        None
    );
    // A retained head is authoritative and must be exact.
    let renewed = object([
        ("runtime_artifact_digest", s(runtime.as_str())),
        ("attestation_id", s(OPERATION)),
    ]);
    let review = object([
        ("attestation_sha256", s(sha(&renewed))),
        ("decision", s("approve")),
    ]);
    agent_memory::layout_recovery_state::write_once(
        &directory
            .join("attestations")
            .join(format!("{OPERATION}.json")),
        &renewed,
    )
    .unwrap();
    agent_memory::layout_recovery_state::write_once(
        &directory
            .join("quiescence-reviews")
            .join(format!("{PROJECT}.json")),
        &review,
    )
    .unwrap();
    let head = |review_sha: String| {
        object([(
            runtime.as_str(),
            object([
                ("attestation_id", s(OPERATION)),
                ("sha256", s(sha(&renewed))),
                ("review_id", s(PROJECT)),
                ("review_sha256", s(review_sha)),
            ]),
        )])
    };
    let heads = directory.join("quiescence-attestations.json");
    agent_memory::layout_recovery_state::replace(&heads, &head(sha(&review)), None).unwrap();
    let selected = quiescence::selected_at(&directory, &proposal, &runtime)
        .unwrap()
        .unwrap();
    assert_eq!(encoded(&selected).unwrap(), encoded(&renewed).unwrap());
    let current = bytes_sha(&fs::read(&heads).unwrap());
    agent_memory::layout_recovery_state::replace(&heads, &head("0".repeat(64)), Some(&current))
        .unwrap();
    refusal(
        quiescence::selected_at(&directory, &proposal, &runtime),
        "Renewed runtime quiescence attestation is missing, corrupt or mismatched",
    );
}

#[test]
fn attestation_requires_frontier_capable_measured_runtime() {
    let manifest = object([
        ("operation_id", s(OPERATION)),
        ("project_id", s(PROJECT)),
        ("repo_root", s("/repo")),
        ("bank_path", s("/repo/memory-bank")),
        (
            "identity",
            object([(
                "scope_context",
                object([("checkout_key", s("1:2")), ("developer_id", s(PROJECT))]),
            )]),
        ),
    ]);
    let runtime = object([
        ("artifact_digest", s("r".repeat(64))),
        (
            "compatibility",
            object([("private_state_capabilities", Value::Array(Vec::new()))]),
        ),
    ]);
    refusal(
        quiescence::attestation_value(&Value::Object(Vec::new()), &manifest, &runtime),
        "pending-runtime-compatibility",
    );
    assert!(!quiescence::frontier_capable(&runtime));
    let capable = object([(
        "compatibility",
        object([(
            "private_state_capabilities",
            Value::Array(vec![s("layout-frontier-v1")]),
        )]),
    )]);
    assert!(quiescence::frontier_capable(&capable));
}
