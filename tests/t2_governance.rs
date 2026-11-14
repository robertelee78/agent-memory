//! Governed invalidation contracts (baseline tests/contracts/test_governance*.py).
//!
//! The in-memory desired-state tests run the production governance delta on
//! exact canonical files without the unported recovery frontier. The complete
//! public journeys are ported below them and stay ignored until their
//! providers are registered.
use agent_memory::{
    bank_contract, bank_layout, bank_transactions as tx,
    codec::json::{self, Value},
    governance::{self, Desired, Observation},
    host, records,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

// The shared admission fixture's bound `fixture`/`state` serve the admission target.
#[allow(dead_code)]
#[path = "support/admission.rs"]
mod support;

const OPERATION_ID: &str = "0b7a3f9e-3c55-4c1e-9d8a-2f6b1c0e4a11";
const AT: &str = "2026-11-13T12:00:00+00:00";
const TEMPLATE: &str =
    include_str!("../skills/agent-memory/templates/record-proposal.example.json");
const ROUTER: &str = include_str!("../skills/agent-memory/templates/router-proposal.example.json");
const DEFAULT_BODY: &str =
    "# Attempt cap\n\nStop after three attempts; retain the source conditions.\n";
const CANDIDATE_BODY: &str = "# Retry policy\n\nUse at most three attempts for this fixture.\n";

fn decode(text: &str) -> Value {
    json::decode(text, json::DecodePolicy::default()).unwrap()
}
fn s(text: &str) -> Value {
    Value::String(text.into())
}
fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.get(key)
        .unwrap_or_else(|| panic!("missing {key} in {}", encode(v)))
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    get(v, key).as_str().unwrap()
}
fn encode(v: &Value) -> String {
    json::encode(v, json::DEFAULT).unwrap()
}
fn set(v: &mut Value, key: &str, item: Value) {
    if let Some(existing) = v.get_mut(key) {
        *existing = item;
    } else if let Value::Object(rows) = v {
        rows.push((key.into(), item));
    } else {
        panic!("fixture is an object");
    }
}
fn pop(v: &mut Value, key: &str) {
    if let Value::Object(rows) = v {
        rows.retain(|(k, _)| k != key);
    }
}
fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn list(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|i| s(i)).collect())
}
fn reference(metadata: &Value) -> Value {
    obj(vec![
        ("id", get(metadata, "id").clone()),
        ("version", get(metadata, "version").clone()),
    ])
}
/// The baseline formula `str(uuid.UUID(hex=sha256(op + ':replacement-version')[:32], version=4))`:
/// RFC 4122 sets nibble 12 to the version and the top bits of nibble 16 to `10`.
fn baseline_replacement_id(operation_id: &str) -> String {
    let hex = agent_memory::developer_identity::digest(Some(
        format!("{operation_id}:replacement-version").as_bytes(),
    ))
    .unwrap();
    let mut digits: Vec<u32> = hex[..32].chars().map(|c| c.to_digit(16).unwrap()).collect();
    digits[12] = 4;
    digits[16] = (digits[16] & 0x3) | 0x8;
    let hex: String = digits
        .iter()
        .map(|d| char::from_digit(*d, 16).unwrap())
        .collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
/// Baseline test `unpack`: one JSON value per frontmatter line.
fn unpack(content: &str) -> (Value, String) {
    let (header, body) = content[4..].split_once("\n---\n").unwrap();
    let rows = header
        .lines()
        .map(|line| {
            let (key, value) = line.split_once(": ").unwrap();
            (key.to_owned(), decode(value))
        })
        .collect();
    (Value::Object(rows), body.to_owned())
}
/// Baseline test `render`: `json.dumps` of each metadata value.
fn render(metadata: &Value, body: &str) -> String {
    let mut out = String::from("---\n");
    for (key, value) in metadata.as_object().unwrap() {
        out.push_str(&format!("{key}: {}\n", encode(value)));
    }
    out + "---\n" + body
}
/// Baseline `record_fixtures.record_text` with the comparison fixture's actor/evidence.
fn record_metadata(pid: &str, actor: &str, status: &str) -> Value {
    let mut metadata = get(&decode(TEMPLATE), "metadata").clone();
    for (key, value) in [
        ("id", s(&host::uuid4().unwrap())),
        ("project-id", s(pid)),
        ("kind", s("semantic-fact")),
        ("status", s(status)),
        ("scope", s("repository")),
        ("conditions", Value::Object(vec![])),
        ("trust", s("reviewed")),
        ("visibility", s("visible")),
        (
            "admission-reason",
            s("Disposable structural fixture; not real admission evidence"),
        ),
    ] {
        set(&mut metadata, key, value);
    }
    let origin = metadata.get_mut("origin").unwrap();
    set(origin, "developer-id", s(&host::uuid4().unwrap()));
    set(origin, "code-state", s("not-code"));
    if ["quarantined", "superseded", "revoked", "expired"].contains(&status) {
        set(
            &mut metadata,
            "state-reason",
            s("Historical fixture; excluded from current guidance"),
        );
    }
    let origin = metadata.get_mut("origin").unwrap();
    set(origin, "developer-id", s(actor));
    set(&mut metadata, "evidence", list(&["evidence.md"]));
    metadata
}
fn route_row(path: &str, metadata: &Value) -> String {
    format!(
        "| [{path}](<{path}>) | Reviewed fixture learning | {} | {} | When comparing related concepts |\n",
        text(metadata, "scope"),
        text(metadata, "status")
    )
}
fn router_files(pid: &str) -> Value {
    let mut proposal = decode(ROUTER);
    set(&mut proposal, "project-id", s(pid));
    let brief = proposal
        .get_mut("contexts")
        .unwrap()
        .get_mut("projectbrief.md")
        .unwrap();
    set(
        brief,
        "body",
        s("# Project\n\nKeep durable knowledge small and evidenced.\n"),
    );
    get(&bank_contract::render(&proposal).unwrap(), "files").clone()
}

/// In-memory exact canonical bank (the baseline comparison fixture's files).
struct Bank {
    pid: String,
    actor: String,
    files: Value,
}
impl Bank {
    fn new() -> Self {
        // The composition root holds set-once registries: compose once per process.
        static PROVIDERS: std::sync::Once = std::sync::Once::new();
        PROVIDERS.call_once(|| agent_memory::cli::register_providers().unwrap());
        let pid = host::uuid4().unwrap();
        Self {
            files: router_files(&pid),
            pid,
            actor: host::uuid4().unwrap(),
        }
    }
    fn read(&self, path: &str) -> &str {
        text(&self.files, path)
    }
    fn write(&mut self, path: &str, content: String) {
        set(&mut self.files, path, Value::String(content));
    }
    fn add(&mut self, name: &str, body: &str, updates: Vec<(&str, Value)>) -> (Value, String) {
        let status = updates
            .iter()
            .find(|(k, _)| *k == "status")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("active")
            .to_owned();
        let mut metadata = record_metadata(&self.pid, &self.actor, &status);
        for (key, value) in updates {
            set(&mut metadata, key, value);
        }
        let path = format!("topics/{name}.md");
        self.write(&path, render(&metadata, body));
        let router = self.read("MEMORY.md").replace(
            "\n## Omitted contexts",
            &format!("{}\n## Omitted contexts", route_row(&path, &metadata)),
        );
        self.write("MEMORY.md", router);
        (metadata, path)
    }
    fn hashes(&self) -> Value {
        tx::file_hashes(&self.files).unwrap()
    }
    fn observe(&self, path: &str) -> Observation {
        let (_, metadata, body) = records::parse(self.read(path), &[]).unwrap();
        Observation {
            root: PathBuf::from("/nonexistent-governance-root"),
            bank: PathBuf::from("/nonexistent-governance-root/memory-bank"),
            base: self.files.clone(),
            checked: bank_contract::validate_files(&self.files, None).unwrap(),
            observed: self.hashes(),
            scope: obj(vec![
                ("developer_id", s(&self.actor)),
                ("project_id", s(&self.pid)),
            ]),
            metadata,
            body,
            context: obj(vec![]),
            git_context: obj(vec![]),
            capture: Value::Null,
            comparison: Value::Null,
            replacement: Value::Null,
        }
    }
    fn request(&self, metadata: &Value, path: &str, operation: &str) -> Value {
        request_from(
            &self.pid,
            metadata,
            path,
            operation,
            get(&self.hashes(), path).clone(),
            top_level(&self.hashes()),
        )
    }
    fn apply(&mut self, files: Value) {
        self.files = files;
    }
}
fn top_level(hashes: &Value) -> Value {
    Value::Object(
        hashes
            .as_object()
            .unwrap()
            .iter()
            .filter(|(path, _)| !path.contains('/'))
            .cloned()
            .collect(),
    )
}
fn request_from(
    pid: &str,
    metadata: &Value,
    path: &str,
    operation: &str,
    sha: Value,
    review: Value,
) -> Value {
    obj(vec![
        ("project_id", s(pid)),
        ("operation", s(operation)),
        (
            "target",
            obj(vec![
                ("path", s(path)),
                ("id", get(metadata, "id").clone()),
                ("version", get(metadata, "version").clone()),
                ("sha256", sha),
            ]),
        ),
        (
            "authority",
            obj(vec![
                ("source_class", s("user")),
                ("repository_evidence", Value::Null),
                (
                    "evidence",
                    list(&["user:explicit-disposable-fixture-correction"]),
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
        ("comparison", Value::Null),
        ("capture", Value::Null),
        ("replacement", Value::Null),
        ("delete_ids", Value::Array(vec![])),
        ("context_review", review),
        ("context_edits", obj(vec![])),
        ("context", obj(vec![])),
    ])
}
fn deletion(request: &mut Value, ids: &[&Value]) {
    let authority = request.get_mut("authority").unwrap();
    set(authority, "basis", s("explicit-deletion"));
    set(authority, "reason_class", s("user-request"));
    set(
        request,
        "delete_ids",
        Value::Array(ids.iter().map(|v| (*v).clone()).collect()),
    );
}
fn files(outcome: Desired) -> Value {
    match outcome {
        Desired::Files(files) => files,
        Desired::Deferred(deferred) => panic!("deferred: {}", encode(&deferred)),
    }
}
fn deferred(outcome: Desired) -> Value {
    match outcome {
        Desired::Deferred(deferred) => deferred,
        Desired::Files(_) => panic!("expected a deferral"),
    }
}
fn all_text(files: &Value) -> String {
    files
        .as_object()
        .unwrap()
        .iter()
        .map(|(_, body)| body.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}
fn route_of<'a>(files: &'a Value, path: &str) -> Option<&'a str> {
    text(files, "MEMORY.md")
        .lines()
        .find(|line| line.starts_with(&format!("| [{path}]")))
}

// ---------------------------------------------------------------------------
// Executed desired-state and validation contracts.
// ---------------------------------------------------------------------------

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_revocation_removes_current_guidance_and_invalidates_projection_and_cached_payload
#[test]
fn revocation_deactivates_and_compacts_without_current_payload() {
    let mut bank = Bank::new();
    let (metadata, path) = bank.add(
        "obsolete",
        "# Retry policy\n\nOBSOLETE_BODY_MUST_NOT_BE_CURRENT.\n",
        vec![],
    );
    let request = bank.request(&metadata, &path, "revoke");
    governance::request_value(&request).unwrap();
    let observation = bank.observe(&path);
    assert!(governance::qualified_authority(&request));
    assert!(governance::permitted_impact(&observation, &request));
    let routed = files(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    let (current, body) = unpack(text(&routed, &path));
    assert_eq!(get(&current, "status"), &s("revoked"));
    assert_eq!(get(&current, "origin"), get(&metadata, "origin"));
    assert!(!body.contains("OBSOLETE_BODY_MUST_NOT_BE_CURRENT"));
    let history = format!("history/events/{OPERATION_ID}.md");
    let (event, _) = unpack(text(&routed, &history));
    assert_eq!(get(&event, "kind"), &s("memory-revoked"));
    assert_eq!(get(&event, "record"), &reference(&metadata));
    assert!(route_of(&routed, &path).unwrap().contains("| revoked |"));
    assert!(route_of(&routed, &history).is_some());
    bank_contract::validate_files(&routed, None).unwrap();
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_expiry_requires_actual_known_boundary_and_policy_retention_is_distinct
#[test]
fn expiry_requires_known_boundary_and_retention_is_policy() {
    for (operation, basis, until, expected) in [
        (
            "expire",
            "time-boundary",
            s("2020-01-01T00:00:00Z"),
            Some("expired"),
        ),
        ("expire", "time-boundary", s("2099-01-01T00:00:00Z"), None),
        ("expire", "time-boundary", Value::Null, None),
        ("retention", "policy", Value::Null, Some("revoked")),
    ] {
        let mut bank = Bank::new();
        let (metadata, path) = bank.add("bounded", DEFAULT_BODY, vec![("valid-until", until)]);
        let mut request = bank.request(&metadata, &path, operation);
        let authority = request.get_mut("authority").unwrap();
        set(authority, "basis", s(basis));
        if operation == "expire" {
            set(authority, "source_class", s("tool"));
        }
        governance::request_value(&request).unwrap();
        assert!(governance::qualified_authority(&request));
        let outcome = governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT);
        match expected {
            None => assert!(outcome.is_err()),
            Some(state) => {
                let routed = files(outcome.unwrap());
                assert_eq!(get(&unpack(text(&routed, &path)).0, "status"), &s(state));
            }
        }
    }
    let mut bank = Bank::new();
    let (metadata, path) = bank.add("bounded", DEFAULT_BODY, vec![]);
    let request = bank.request(&metadata, &path, "retention");
    assert_eq!(
        governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT)
            .unwrap_err()
            .to_string(),
        "Retention requires its actual policy, never age alone"
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_ambiguous_evidence_or_older_contexts_still_applying_stays_pending_without_pruning
#[test]
fn ambiguous_or_narrower_authority_stays_pending() {
    for (key, value) in [
        ("strength", s("ambiguous")),
        ("confirmed", Value::Bool(false)),
        ("impact", s("narrower-contexts-remain")),
        ("source_class", s("assistant")),
    ] {
        let mut bank = Bank::new();
        let (metadata, path) = bank.add("conditional", DEFAULT_BODY, vec![]);
        let mut request = bank.request(&metadata, &path, "revoke");
        set(request.get_mut("authority").unwrap(), key, value);
        governance::request_value(&request).unwrap();
        let observation = bank.observe(&path);
        assert!(
            !governance::qualified_authority(&request)
                || !governance::permitted_impact(&observation, &request)
        );
        let pending = governance::pending(
            &observation,
            &obj(vec![("disposition", s("clarification-required"))]),
        )
        .unwrap();
        assert_eq!(get(&pending, "proposal"), &Value::Null);
        assert_eq!(get(&pending, "detecting_developer_id"), &s(&bank.actor));
        assert_eq!(get(&pending, "messages_sent"), &Value::Bool(false));
        assert_eq!(
            get(&pending, "canonical_writes_performed"),
            &Value::Bool(false)
        );
    }
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_current_context_only_deletion_does_not_globally_erase_a_record
#[test]
fn current_context_only_deletion_is_not_permitted() {
    let mut bank = Bank::new();
    let (metadata, path) = bank.add("conditional", DEFAULT_BODY, vec![]);
    let mut request = bank.request(&metadata, &path, "delete");
    deletion(&mut request, &[get(&metadata, "id")]);
    set(
        request.get_mut("authority").unwrap(),
        "impact",
        s("narrower-contexts-remain"),
    );
    governance::request_value(&request).unwrap();
    assert!(governance::qualified_authority(&request));
    assert!(!governance::permitted_impact(
        &bank.observe(&path),
        &request
    ));
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_terminal_deletion_requires_explicit_bounded_derived_scope_and_removes_only_safe_markers
#[test]
fn terminal_deletion_requires_bounded_derived_scope_and_leaves_safe_markers() {
    let mut bank = Bank::new();
    let (original, p1) = bank.add(
        "original",
        "# Retry policy\n\nDELETED_CANONICAL_PAYLOAD.\n",
        vec![],
    );
    let (derived, p2) = bank.add(
        "derived",
        "# Derived retry note\n\nDELETED_CANONICAL_PAYLOAD.\n",
        vec![(
            "derived-from",
            Value::Array(vec![obj(vec![
                ("id", get(&original, "id").clone()),
                ("version", Value::Int(1.into())),
            ])]),
        )],
    );
    let mut request = bank.request(&original, &p1, "delete");
    deletion(&mut request, &[get(&original, "id")]);
    let observation = bank.observe(&p1);
    let pending = deferred(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    assert_eq!(
        get(&pending, "disposition"),
        &s("deletion-scope-review-required")
    );
    let mut required = vec![
        text(&original, "id").to_owned(),
        text(&derived, "id").to_owned(),
    ];
    required.sort();
    assert_eq!(
        get(&pending, "required_delete_ids"),
        &Value::Array(required.iter().map(|id| s(id)).collect())
    );
    deletion(&mut request, &[get(&original, "id"), get(&derived, "id")]);
    let routed = files(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    assert!(routed.get(&p1).is_none());
    assert!(routed.get(&p2).is_none());
    for identity in [&original, &derived] {
        let marker = text(
            &routed,
            &format!("history/tombstones/{}.md", text(identity, "id")),
        );
        let (metadata, body) = unpack(marker);
        let keys: Vec<_> = metadata
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "scope",
                "deleted-at",
                "deletion-authority",
                "reason-class"
            ]
        );
        assert_eq!(body, "");
    }
    assert!(!all_text(&routed).contains("DELETED_CANONICAL_PAYLOAD"));
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_deletion_requires_reviewed_removal_of_copied_ordinary_context
#[test]
fn deletion_requires_reviewed_removal_of_copied_context() {
    let mut bank = Bank::new();
    let (old, path) = bank.add(
        "erase",
        "# Retry policy\n\nERASE_THIS_COPIED_ASSERTION.\n",
        vec![],
    );
    let original = bank.read("projectbrief.md").to_owned();
    let copied = unpack(bank.read(&path)).1;
    bank.write("projectbrief.md", original.clone() + &copied);
    let mut request = bank.request(&old, &path, "delete");
    deletion(&mut request, &[get(&old, "id")]);
    let observation = bank.observe(&path);
    assert!(governance::desired(&observation, &request, OPERATION_ID, AT).is_err());
    set(
        &mut request,
        "context_edits",
        obj(vec![("projectbrief.md", s(&original))]),
    );
    governance::request_value(&request).unwrap();
    let routed = files(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    assert_eq!(text(&routed, "projectbrief.md"), original);
    assert!(!all_text(&routed).contains("ERASE_THIS_COPIED_ASSERTION"));
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_deletion_refuses_a_copied_router_assertion_until_its_existing_file_workflow_cleans_it
#[test]
fn deletion_refuses_copied_router_assertion() {
    let mut bank = Bank::new();
    let (old, path) = bank.add(
        "erase",
        "# Retry policy\n\nERASE_THIS_ROUTER_COPY.\n",
        vec![],
    );
    let original = bank.read("MEMORY.md").to_owned();
    let body = unpack(bank.read(&path)).1;
    bank.write(
        "MEMORY.md",
        original.replace("\n## Routes\n", &format!("\n{body}\n## Routes\n")),
    );
    let mut request = bank.request(&old, &path, "delete");
    deletion(&mut request, &[get(&old, "id")]);
    assert!(governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT).is_err());
    bank.write("MEMORY.md", original);
    let mut request = bank.request(&old, &path, "delete");
    deletion(&mut request, &[get(&old, "id")]);
    let routed =
        files(governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT).unwrap());
    assert!(!all_text(&routed).contains("ERASE_THIS_ROUTER_COPY"));
}

fn index(pid: &str, path: &str, target: &str) -> String {
    let request = obj(vec![
        ("path", s(path)),
        ("project-id", s(pid)),
        ("updated", s("2026-10-22T22:00:00Z")),
        (
            "routes",
            Value::Array(vec![obj(vec![
                ("path", s(target)),
                ("purpose", s("Routed fixture")),
                ("scope", s("repository")),
                ("lifecycle", s("active")),
                ("read-when", s("When examining this fixture")),
            ])]),
        ),
    ]);
    text(&bank_layout::render_index(&request).unwrap(), "content").to_owned()
}

// obligation: tests/contracts/test_governance_recovery.py::GovernanceRecoveryContract.test_deleting_last_nested_route_preserves_custom_index_context_before_removing_empty_maps
#[test]
fn deleting_last_nested_route_preserves_custom_index_context() {
    let mut bank = Bank::new();
    let (metadata, path) = bank.add("last-child", DEFAULT_BODY, vec![]);
    let (child, parent) = ("topics/nested/index.md", "topics/index.md");
    bank.write(child, index(&bank.pid.clone(), child, &path));
    let parent_text = index(&bank.pid.clone(), parent, child);
    let context = "Unrelated useful index orientation must survive.";
    bank.write(
        parent,
        parent_text.replace("\n## Routes\n", &format!("\n{context}\n\n## Routes\n")),
    );
    let router: String = bank
        .read("MEMORY.md")
        .split_inclusive('\n')
        .filter(|line| !line.starts_with(&format!("| [{path}]")))
        .collect();
    bank.write(
        "MEMORY.md",
        router.replace(
            "\n## Omitted contexts",
            &format!("| [{parent}](<{parent}>) | Nested group | repository | active | When needed |\n\n## Omitted contexts"),
        ),
    );
    let mut request = bank.request(&metadata, &path, "delete");
    deletion(&mut request, &[get(&metadata, "id")]);
    assert_eq!(
        governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT)
            .unwrap_err()
            .to_string(),
        "The empty index contains custom context; preserve it before pruning its last route"
    );
    let brief = format!("{}\n{context}\n", bank.read("projectbrief.md"));
    bank.write("projectbrief.md", brief);
    bank.write(parent, parent_text);
    let mut request = bank.request(&metadata, &path, "delete");
    deletion(&mut request, &[get(&metadata, "id")]);
    let routed =
        files(governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT).unwrap());
    assert!(routed.get(child).is_none());
    assert!(routed.get(parent).is_none());
    assert!(text(&routed, "projectbrief.md").contains(context));
    assert!(route_of(&routed, parent).is_none());
}

fn supersede_observation(bank: &Bank, p1: &str, p2: &str) -> (Observation, Value) {
    let (replacement, replacement_body) = unpack(bank.read(p2));
    let (old, _) = unpack(bank.read(p1));
    let mut request = bank.request(&old, p1, "supersede");
    set(
        &mut request,
        "replacement",
        obj(vec![
            ("path", s(p2)),
            ("id", get(&replacement, "id").clone()),
            ("version", get(&replacement, "version").clone()),
            ("sha256", get(&bank.hashes(), p2).clone()),
        ]),
    );
    let mut observation = bank.observe(p1);
    let (_, metadata, body) = records::parse(bank.read(p2), &[]).unwrap();
    assert_eq!(body, replacement_body);
    observation.replacement = obj(vec![("metadata", metadata), ("body", s(&body))]);
    (observation, request)
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_existing_replacement_is_versioned_and_survives_deletion_of_the_wrong_old_claim
#[test]
fn existing_replacement_is_versioned_and_survives_deletion() {
    let mut bank = Bank::new();
    let (old, p1) = bank.add(
        "wrong",
        "# Retry policy\n\nFive attempts was the disproved old claim.\n",
        vec![],
    );
    let (replacement, p2) = bank.add(
        "correct",
        "# Retry policy\n\nThree attempts is the independently evidenced policy.\n",
        vec![],
    );
    let body = unpack(bank.read(&p2)).1;
    let (observation, request) = supersede_observation(&bank, &p1, &p2);
    let routed = files(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    let (current, current_body) = unpack(text(&routed, &p2));
    assert_eq!(get(&current, "version"), &Value::Int(2.into()));
    assert_eq!(get(&current, "origin"), get(&replacement, "origin"));
    assert!(
        get(&current, "supersedes")
            .as_array()
            .unwrap()
            .contains(&reference(&old))
    );
    assert_eq!(current_body, body);
    assert_eq!(
        get(&unpack(text(&routed, &p1)).0, "status"),
        &s("superseded")
    );
    let archived = format!("history/versions/{}/v1.md", text(&replacement, "id"));
    assert_eq!(
        get(&unpack(text(&routed, &archived)).0, "status"),
        &s("superseded")
    );
    let events: Vec<_> = routed
        .as_object()
        .unwrap()
        .iter()
        .filter(|(p, _)| p.starts_with("history/events/"))
        .map(|(p, _)| p.clone())
        .collect();
    assert_eq!(events.len(), 2);
    assert!(events.contains(&format!("history/events/{OPERATION_ID}.md")));
    assert!(events.contains(&format!(
        "history/events/{}.md",
        baseline_replacement_id(OPERATION_ID)
    )));
    bank.apply(routed);
    let (prior, _) = unpack(bank.read(&p1));
    let mut request = bank.request(&prior, &p1, "delete");
    deletion(&mut request, &[get(&old, "id")]);
    let routed =
        files(governance::desired(&bank.observe(&p1), &request, OPERATION_ID, AT).unwrap());
    let (after, text_after) = unpack(text(&routed, &p2));
    assert_eq!(text_after, body);
    assert_eq!(get(&after, "origin"), get(&replacement, "origin"));
    assert_eq!(get(&after, "supersedes"), &Value::Array(vec![]));
}

fn capture_of(candidate: Value, body: &str) -> Value {
    obj(vec![(
        "request",
        obj(vec![(
            "candidate",
            obj(vec![("metadata", candidate), ("body", s(body))]),
        )]),
    )])
}
fn correction_candidate(bank: &Bank, old: &Value, updates: Vec<(&str, Value)>) -> Value {
    let mut metadata = record_metadata(&bank.pid, &bank.actor, "candidate");
    set(&mut metadata, "id", get(old, "id").clone());
    for (key, value) in updates {
        set(&mut metadata, key, value);
    }
    set(
        metadata.get_mut("origin").unwrap(),
        "session",
        s("correcting-session"),
    );
    set(
        &mut metadata,
        "evidence",
        list(&["user:current-confirmed-correction"]),
    );
    metadata
}
fn correction_request(bank: &Bank, old: &Value, path: &str, operation: &str) -> Value {
    let mut request = bank.request(old, path, operation);
    set(
        request.get_mut("authority").unwrap(),
        "evidence",
        list(&["user:current-confirmed-correction"]),
    );
    request
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_stronger_correction_keeps_stable_identity_old_origin_and_useful_history
#[test]
fn stronger_correction_keeps_identity_origin_and_history() {
    let mut bank = Bank::new();
    let (old, path) = bank.add(
        "incorrect",
        "# Retry policy\n\nThe older fixture incorrectly said five attempts.\n",
        vec![],
    );
    let request = correction_request(&bank, &old, &path, "correct");
    let mut observation = bank.observe(&path);
    observation.capture = capture_of(correction_candidate(&bank, &old, vec![]), CANDIDATE_BODY);
    let routed = files(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    let (current, body) = unpack(text(&routed, &path));
    assert_eq!(get(&current, "id"), get(&old, "id"));
    assert_eq!(get(&current, "version"), &Value::Int(2.into()));
    assert_eq!(
        get(get(&current, "origin"), "session"),
        &s("correcting-session")
    );
    assert!(
        get(&current, "corrects")
            .as_array()
            .unwrap()
            .contains(&reference(&old))
    );
    assert_eq!(body, CANDIDATE_BODY);
    let archived = format!("history/versions/{}/v1.md", text(&old, "id"));
    let (history, previous) = unpack(text(&routed, &archived));
    assert_eq!(get(&history, "origin"), get(&old, "origin"));
    assert_eq!(get(&history, "status"), &s("superseded"));
    assert!(previous.contains("incorrectly said five attempts"));
    let (event, _) = unpack(text(&routed, &format!("history/events/{OPERATION_ID}.md")));
    assert_eq!(get(&event, "kind"), &s("memory-corrected"));
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_reactivation_needs_new_evidence_and_an_explicit_current_period
#[test]
fn reactivation_needs_new_evidence_and_current_period() {
    let mut bank = Bank::new();
    let (old, path) = bank.add("revoked", DEFAULT_BODY, vec![]);
    let request = bank.request(&old, &path, "revoke");
    let routed =
        files(governance::desired(&bank.observe(&path), &request, OPERATION_ID, AT).unwrap());
    bank.apply(routed);
    let (old, _) = unpack(bank.read(&path));
    assert!(old.get("visibility").is_none());
    let request = correction_request(&bank, &old, &path, "reactivate");
    let mut observation = bank.observe(&path);
    let mut candidate = correction_candidate(&bank, &old, vec![("valid-from", s(AT))]);
    for key in ["trust", "visibility", "confidence", "admission-reason"] {
        pop(&mut candidate, key);
    }
    observation.capture = capture_of(candidate.clone(), CANDIDATE_BODY);
    let later = "2026-11-13T13:00:00+00:00";
    let routed = files(
        governance::desired(
            &observation,
            &request,
            "5e0d3f3a-8c2b-4a4f-9d0e-7a1b2c3d4e5f",
            later,
        )
        .unwrap(),
    );
    let (current, _) = unpack(text(&routed, &path));
    assert_eq!(get(&current, "status"), &s("active"));
    assert_eq!(get(&current, "version"), &Value::Int(2.into()));
    let archived = format!("history/versions/{}/v1.md", text(&old, "id"));
    let (historical, _) = unpack(text(&routed, &archived));
    assert_eq!(get(&historical, "status"), &s("revoked"));
    assert_eq!(get(&historical, "origin"), get(&old, "origin"));
    assert!(text(&historical, "state-reason").contains(&path));
    set(&mut candidate, "valid-from", Value::Null);
    observation.capture = capture_of(candidate, CANDIDATE_BODY);
    assert!(governance::desired(&observation, &request, OPERATION_ID, later).is_err());
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_correction_cannot_reuse_unchanged_evidence_or_promote_a_procedural_capture
#[test]
fn correction_cannot_reuse_unchanged_evidence() {
    let mut bank = Bank::new();
    let (old, path) = bank.add(
        "existing",
        DEFAULT_BODY,
        vec![("evidence", list(&["user:current-confirmed-correction"]))],
    );
    let request = correction_request(&bank, &old, &path, "correct");
    let mut observation = bank.observe(&path);
    observation.capture = capture_of(correction_candidate(&bank, &old, vec![]), CANDIDATE_BODY);
    assert!(governance::desired(&observation, &request, OPERATION_ID, AT).is_err());
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_narrowing_cannot_replace_the_old_claim_that_still_applies_in_older_contexts
#[test]
fn narrowing_requires_unchanged_body_and_strictly_narrower_conditions() {
    let mut bank = Bank::new();
    let (old, path) = bank.add(
        "older-policy",
        "# Retry policy\n\nFive attempts still applies to the older branch.\n",
        vec![],
    );
    let mut request = correction_request(&bank, &old, &path, "correct");
    set(
        request.get_mut("authority").unwrap(),
        "impact",
        s("narrower-contexts-remain"),
    );
    let mut observation = bank.observe(&path);
    let narrower = correction_candidate(
        &bank,
        &old,
        vec![("conditions", obj(vec![("branch", s("older/release"))]))],
    );
    observation.capture = capture_of(narrower.clone(), CANDIDATE_BODY);
    assert!(!governance::permitted_impact(&observation, &request));
    // Control: the same strictly narrower conditions with the exact body are permitted.
    let body = observation.body.clone();
    observation.capture = capture_of(narrower, &body);
    assert!(governance::permitted_impact(&observation, &request));
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_scope_correction_retains_older_branch_guidance_without_applying_it_on_main
#[test]
fn scope_correction_keeps_older_branch_condition_as_new_version() {
    let mut bank = Bank::new();
    let (old, path) = bank.add("older-policy", CANDIDATE_BODY, vec![]);
    let mut request = correction_request(&bank, &old, &path, "correct");
    set(
        request.get_mut("authority").unwrap(),
        "impact",
        s("narrower-contexts-remain"),
    );
    let mut observation = bank.observe(&path);
    observation.capture = capture_of(
        correction_candidate(
            &bank,
            &old,
            vec![("conditions", obj(vec![("branch", s("older/release"))]))],
        ),
        CANDIDATE_BODY,
    );
    assert!(governance::permitted_impact(&observation, &request));
    let routed = files(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    let (current, _) = unpack(text(&routed, &path));
    assert_eq!(get(&current, "version"), &Value::Int(2.into()));
    assert_eq!(
        get(&current, "conditions"),
        &obj(vec![("branch", s("older/release"))])
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_unresolved_other_developer_claim_keeps_correction_pending
#[test]
fn unresolved_other_developer_claim_keeps_correction_pending() {
    let mut bank = Bank::new();
    let (old, path) = bank.add("old", DEFAULT_BODY, vec![]);
    let peer_body = "# Retry policy\n\nAnother overlapping claim still needs clarification.\n";
    let (mut peer, peer_path) = bank.add("peer", peer_body, vec![]);
    let peer_id = host::uuid4().unwrap();
    set(peer.get_mut("origin").unwrap(), "developer-id", s(&peer_id));
    bank.write(&peer_path, render(&peer, peer_body));
    let request = correction_request(&bank, &old, &path, "correct");
    let mut observation = bank.observe(&path);
    observation.capture = capture_of(correction_candidate(&bank, &old, vec![]), CANDIDATE_BODY);
    let (_, current_peer, _) = records::parse(bank.read(&peer_path), &[]).unwrap();
    observation.comparison = obj(vec![
        (
            "matches",
            Value::Array(vec![
                obj(vec![
                    ("path", s(&path)),
                    ("metadata", observation.metadata.clone()),
                ]),
                obj(vec![("path", s(&peer_path)), ("metadata", current_peer)]),
            ]),
        ),
        (
            "proof",
            obj(vec![
                ("judgment", obj(vec![("decisions", Value::Array(vec![]))])),
                ("targets", Value::Array(vec![])),
            ]),
        ),
        ("disputed_paths", list(&[&peer_path])),
        ("other_developer_ids", list(&[&peer_id])),
        ("recommend_other_developer_clarification", Value::Bool(true)),
    ]);
    let pending = deferred(governance::desired(&observation, &request, OPERATION_ID, AT).unwrap());
    let preview = governance::pending(&observation, &pending).unwrap();
    assert_eq!(get(&preview, "proposal"), &Value::Null);
    assert!(
        get(&preview, "other_developer_ids")
            .as_array()
            .unwrap()
            .contains(&s(&peer_id))
    );
    assert_eq!(
        get(&preview, "recommend_other_developer_clarification"),
        &Value::Bool(true)
    );
    assert_eq!(get(&preview, "messages_sent"), &Value::Bool(false));
}

/// Validation controls: malformed requests refuse with the baseline messages.
#[test]
fn malformed_requests_refuse_without_panicking() {
    let mut bank = Bank::new();
    let (metadata, path) = bank.add("old", DEFAULT_BODY, vec![]);
    let good = bank.request(&metadata, &path, "revoke");
    governance::request_value(&good).unwrap();
    let refuse = |mutate: &dyn Fn(&mut Value), message: &str| {
        let mut request = good.clone();
        mutate(&mut request);
        assert_eq!(
            governance::request_value(&request).unwrap_err().to_string(),
            message
        );
    };
    refuse(
        &|r| set(r, "operation", s("prune")),
        "Governance needs one explicit project operation and complete evidence",
    );
    refuse(
        &|r| set(r, "extra", Value::Null),
        "Governance needs one explicit project operation and complete evidence",
    );
    refuse(
        &|r| set(r.get_mut("target").unwrap(), "sha256", s("0")),
        "Governance requires an exact observed record path, identity, version and hash",
    );
    refuse(
        &|r| {
            set(
                r.get_mut("target").unwrap(),
                "version",
                Value::Int(0.into()),
            )
        },
        "Governance requires an exact observed record path, identity, version and hash",
    );
    refuse(
        &|r| set(r.get_mut("authority").unwrap(), "strength", s("certain")),
        "Assess actual source, strength, applicable impact and confirmation explicitly",
    );
    refuse(
        &|r| {
            set(
                r.get_mut("authority").unwrap(),
                "reason_class",
                s("user-request"),
            )
        },
        "Deletion reason class must remain content free",
    );
    refuse(
        &|r| set(r, "delete_ids", list(&["not-a-uuid"])),
        "Deletion names an explicit bounded set of authorized stable identities",
    );
    refuse(
        &|r| {
            set(
                r,
                "delete_ids",
                Value::Array(vec![get(&metadata, "id").clone()]),
            )
        },
        "Only explicit deletion can authorize terminal identities",
    );
    refuse(
        &|r| set(r, "capture", obj(vec![])),
        "Deactivation and deletion cannot smuggle replacement or admission evidence",
    );
    refuse(
        &|r| set(r, "operation", s("correct")),
        "A new corrected version requires qualified capture and exact comparison",
    );
    refuse(
        &|r| set(r, "context_review", Value::Array(vec![])),
        "Review ordinary context for copied guidance before invalidating its source",
    );
    refuse(
        &|r| set(r, "context_edits", obj(vec![("productContext.md", s("x"))])),
        "Context changes must be among the actual reviewed canonical files",
    );
    refuse(
        &|r| {
            set(r, "context_edits", obj(vec![("MEMORY.md", s("x"))]));
        },
        "Only bounded reviewed ordinary context may accompany governance",
    );
    refuse(
        &|r| {
            let evidence: Vec<_> = (0..65).map(|i| s(&format!("user:e{i}"))).collect();
            set(
                r.get_mut("authority").unwrap(),
                "evidence",
                Value::Array(evidence),
            );
        },
        "Governance authority is bounded evidence, not a transcript",
    );
    for value in [Value::Null, s("text"), Value::Array(vec![])] {
        assert!(governance::request_value(&value).is_err());
    }
}

// obligation: tests/contracts/test_installed_bank_governance.py::InstalledBankRoundtrip.test_governance_alias_checks_intent_at_preview_and_apply_before_writes
#[test]
fn governance_alias_checks_intent_before_writes() {
    support::in_world("governance_alias_checks_intent_before_writes", |world| {
        // An unbound canonical bank: the intent check precedes every observation.
        let repo = world.root.join("repo");
        let bank = repo.join("memory-bank");
        let pid = host::uuid4().unwrap();
        let developer = host::uuid4().unwrap();
        for (path, body) in router_files(&pid).as_object().unwrap() {
            let path = bank.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body.as_str().unwrap()).unwrap();
        }
        let record = bank.join("topics/wrong.md");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        let metadata = record_metadata(&pid, &developer, "active");
        std::fs::write(&record, render(&metadata, DEFAULT_BODY)).unwrap();
        let router = std::fs::read_to_string(bank.join("MEMORY.md"))
            .unwrap()
            .replace(
                "\n## Omitted contexts",
                &format!(
                    "{}\n## Omitted contexts",
                    route_row("topics/wrong.md", &metadata)
                ),
            );
        std::fs::write(bank.join("MEMORY.md"), router).unwrap();
        let base = tx::files_at(&bank).unwrap();
        let hashes = tx::file_hashes(&base).unwrap();
        let request = request_from(
            &pid,
            &metadata,
            "topics/wrong.md",
            "revoke",
            get(&hashes, "topics/wrong.md").clone(),
            top_level(&hashes),
        );
        let saved = world.save(&request, "request.json");
        let before = tx::files_at(&bank).unwrap();
        for group in ["correct", "supersede", "forget"] {
            let (code, refused) = dispatch(&[
                group,
                "preview",
                "--repo",
                repo.to_str().unwrap(),
                "--request",
                saved.to_str().unwrap(),
            ]);
            assert_eq!(code, 1, "{}", encode(&refused));
            assert_eq!(
                get(&refused, "schema"),
                &s("agent-memory/governance-result-v1")
            );
            assert_eq!(get(&refused, "operation"), &s("preview"));
            assert_eq!(
                get(&refused, "canonical_writes_performed"),
                &Value::Bool(false)
            );
            assert_eq!(
                get(&refused, "recovery_may_be_required"),
                &Value::Bool(false)
            );
            assert_eq!(tx::files_at(&bank).unwrap(), before);
        }
        let root = agent_memory::fs::canonical_path(&repo, true).unwrap();
        let proposal = obj(vec![
            ("schema", s(tx::SCHEMA)),
            ("operation_id", s(OPERATION_ID)),
            ("operation", s("revoke")),
            ("repo_root", s(root.to_str().unwrap())),
            ("project_id", s(&pid)),
            ("observed_hashes", hashes.clone()),
            ("files", base.clone()),
            ("scope_context", Value::Null),
            (
                "governance_evidence",
                obj(vec![
                    ("request", request.clone()),
                    ("recorded_at", s(AT)),
                    ("git_context", obj(vec![])),
                    ("unit_resolution_claims", Value::Array(vec![])),
                    ("comparison_proof_sha256", Value::Null),
                    ("reference_map_sha256", s(&"0".repeat(64))),
                ]),
            ),
        ]);
        tx::proposal_value(&proposal).unwrap();
        let prepared = world.save(&proposal, "revoke-proposal.json");
        let (code, refused) =
            dispatch(&["forget", "apply", "--proposal", prepared.to_str().unwrap()]);
        assert_eq!(code, 1, "{}", encode(&refused));
        assert_eq!(get(&refused, "operation"), &s("apply"));
        assert_eq!(get(&refused, "canonical_writes_performed"), &Value::Null);
        assert_eq!(
            get(&refused, "recovery_may_be_required"),
            &Value::Bool(true)
        );
        assert_eq!(
            get(&refused, "error"),
            &s(
                "Unqualified, ambiguous, changed or incomplete governance; preserve evidence and inspect"
            )
        );
        assert_eq!(tx::files_at(&bank).unwrap(), before);
        let intent = "The requested operation differs from the CLI intent";
        for (prepared, preparing) in [(&request, true), (&proposal, false)] {
            governance::require_intent(prepared, preparing, Some(&["revoke"])).unwrap();
            governance::require_intent(prepared, preparing, None).unwrap();
            for group in [
                &["correct"][..],
                &["supersede"],
                &["expire", "retention", "delete"],
            ] {
                assert_eq!(
                    governance::require_intent(prepared, preparing, Some(group))
                        .unwrap_err()
                        .to_string(),
                    intent
                );
            }
        }
    });
}
fn dispatch(args: &[&str]) -> (i32, Value) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let result = agent_memory::cli::dispatch(&args, "am").result;
    let value = result
        .value
        .clone()
        .or_else(|| (!result.stdout.is_empty()).then(|| decode(result.stdout.trim_end())))
        .unwrap_or(Value::Null);
    (result.exit_code, value)
}

// ---------------------------------------------------------------------------
// Complete public journeys (baseline GovernanceContract fixtures).
// ---------------------------------------------------------------------------

/// The baseline `ComparisonContract.fixture` world: a bound repository with a
/// staged unrelated change, exercised only through public `am` routes.
struct Journey<'a> {
    world: &'a support::World,
    repo: PathBuf,
    bank: PathBuf,
    pid: String,
    actor: String,
    /// The governance route (`govern` or an intent alias).
    group: &'static str,
}
#[derive(Debug, PartialEq)]
struct State(Vec<(String, Vec<u8>)>, Vec<u8>, Vec<u8>, String);

fn walk(root: &Path, directory: &Path, found: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(root, &path, found);
        } else {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            found.push((name, std::fs::read(&path).unwrap()));
        }
    }
}

impl<'a> Journey<'a> {
    fn new(world: &'a support::World) -> Self {
        Self::named(world, "repo")
    }
    /// One baseline `World` per sub-case: a fresh repository and bank, with the
    /// developer identity reused exactly as `identity_fixtures.bind` does.
    fn named(world: &'a support::World, name: &str) -> Self {
        let repo = world.root.join(name);
        std::fs::create_dir(&repo).unwrap();
        world.git(&repo, &["init", "--initial-branch=main"]);
        world.git(&repo, &["config", "user.name", "Fixture"]);
        world.git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(repo.join("code.py"), "ATTEMPT_LIMIT = 3\n").unwrap();
        std::fs::write(
            repo.join("evidence.md"),
            "# Observed result\n\nA disposable measured fixture, not actual project admission.\n",
        )
        .unwrap();
        world.git(&repo, &["add", "."]);
        world.git(&repo, &["commit", "-m", "Source evidence fixture"]);
        let pid = host::uuid4().unwrap();
        let bank = repo.join("memory-bank");
        for (path, body) in router_files(&pid).as_object().unwrap() {
            let path = bank.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body.as_str().unwrap()).unwrap();
        }
        let repo_text = repo.to_str().unwrap().to_owned();
        let developer = dispatch(&["developer", "status", "--repo", &repo_text]).1;
        if get(&developer, "setup_required") == &Value::Bool(true) {
            let proposal = dispatch(&[
                "developer",
                "preview",
                "--create",
                "--label",
                "Disposable test developer",
            ])
            .1;
            let saved = world.save(&proposal, "fixture-identity-proposal.json");
            dispatch(&["developer", "apply", "--proposal", saved.to_str().unwrap()]);
        }
        let current = dispatch(&["identity", "status", "--repo", &repo_text]).1;
        if get(&current, "resolved") != &Value::Bool(true) {
            let proposal = dispatch(&[
                "identity",
                "preview",
                "--repo",
                &repo_text,
                "--action",
                "bind",
                "--reason",
                "Explicit disposable fixture bank adoption",
            ])
            .1;
            let saved = world.save(&proposal, "fixture-identity-proposal.json");
            let receipt = dispatch(&["identity", "apply", "--proposal", saved.to_str().unwrap()]).1;
            assert_eq!(
                get(&receipt, "verified"),
                &Value::Bool(true),
                "{}",
                encode(&receipt)
            );
        }
        let status = dispatch(&["identity", "status", "--repo", &repo_text]).1;
        let actor = text(&status, "developer_id").to_owned();
        std::fs::write(
            repo.join("code.py"),
            "ATTEMPT_LIMIT = 4  # staged unfinished work\n",
        )
        .unwrap();
        world.git(&repo, &["add", "code.py"]);
        Self {
            world,
            repo,
            bank,
            pid,
            actor,
            group: "govern",
        }
    }
    fn repo(&self) -> &str {
        self.repo.to_str().unwrap()
    }
    fn save(&self, value: &Value, name: &str) -> String {
        self.world.save(value, name).to_str().unwrap().to_owned()
    }
    fn call(&self, args: &[&str]) -> (i32, Value) {
        dispatch(args)
    }
    fn good(&self, args: &[&str]) -> Value {
        let (code, value) = self.call(args);
        assert_eq!(code, 0, "{args:?}: {}", encode(&value));
        value
    }
    fn state(&self) -> State {
        let mut files = Vec::new();
        walk(&self.bank, &self.bank, &mut files);
        files.sort();
        State(
            files,
            std::fs::read(self.repo.join("code.py")).unwrap(),
            std::fs::read(self.repo.join(".git/index")).unwrap(),
            self.world.git(&self.repo, &["rev-parse", "HEAD"]),
        )
    }
    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.bank.join(path)).unwrap()
    }
    fn exists(&self, path: &str) -> bool {
        self.bank.join(path).exists()
    }
    fn add(&self, name: &str, body: &str, updates: Vec<(&str, Value)>) -> (Value, String) {
        let status = updates
            .iter()
            .find(|(k, _)| *k == "status")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("active")
            .to_owned();
        let mut metadata = record_metadata(&self.pid, &self.actor, &status);
        for (key, value) in updates {
            set(&mut metadata, key, value);
        }
        let path = format!("topics/{name}.md");
        let target = self.bank.join(&path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, render(&metadata, body)).unwrap();
        let router = self.read("MEMORY.md").replace(
            "\n## Omitted contexts",
            &format!("{}\n## Omitted contexts", route_row(&path, &metadata)),
        );
        std::fs::write(self.bank.join("MEMORY.md"), router).unwrap();
        (metadata, path)
    }
    fn hashes(&self) -> Value {
        tx::file_hashes(&tx::files_at(&self.bank).unwrap()).unwrap()
    }
    fn request(&self, metadata: &Value, path: &str, operation: &str) -> Value {
        let hashes = self.hashes();
        request_from(
            &self.pid,
            metadata,
            path,
            operation,
            get(&hashes, path).clone(),
            top_level(&hashes),
        )
    }
    fn preview(&self, request: &Value) -> (i32, Value) {
        let saved = self.save(request, "request.json");
        self.call(&[
            self.group,
            "preview",
            "--repo",
            self.repo(),
            "--request",
            &saved,
        ])
    }
    fn apply(&self, proposal: &Value) -> (i32, Value) {
        let saved = self.save(proposal, "governance.json");
        self.call(&[self.group, "apply", "--proposal", &saved])
    }
    fn prepared(&self, request: &Value) -> Value {
        let (code, value) = self.preview(request);
        assert_eq!(code, 0, "{}", encode(&value));
        assert_ne!(get(&value, "proposal"), &Value::Null);
        get(&value, "proposal").clone()
    }
    fn projection(&self, metadata: &Value) -> Value {
        let request = obj(vec![
            ("project_id", s(&self.pid)),
            ("records", Value::Array(vec![reference(metadata)])),
        ]);
        let saved = self.save(&request, "request.json");
        get(
            &self.good(&[
                "projection",
                "build",
                "--repo",
                self.repo(),
                "--input",
                &saved,
            ]),
            "projection",
        )
        .clone()
    }
    /// Baseline `AdmissionContract.request()` for this world.
    fn admission_request(&self) -> Value {
        let mut metadata = record_metadata(&self.pid, &self.actor, "candidate");
        set(
            &mut metadata,
            "source",
            obj(vec![
                ("kind", s("user")),
                ("actor", s("Explicit fixture source; not authenticated")),
            ]),
        );
        obj(vec![
            ("project_id", s(&self.pid)),
            (
                "candidate",
                obj(vec![("metadata", metadata), ("body", s(CANDIDATE_BODY))]),
            ),
            ("source_class", s("user")),
            ("repository_evidence", Value::Null),
            (
                "assessment",
                decode(
                    r#"{"category":"user-correction","utility":"Prevent repeated excessive retries under these conditions.","durable":true,"evidence_confidence":"confirmed","evidence_reason":"The operator explicitly confirmed the correction.","confirmed":true,"source_redundant":false,"work_state_copy":false,"review":null}"#,
                ),
            ),
        ])
    }
    fn capture(&self, request: &Value) -> Value {
        let saved = self.save(request, "request.json");
        self.good(&["capture", "--repo", self.repo(), "--request", &saved])
    }
    fn inspect(&self, request: &Value) -> Value {
        let saved = self.save(request, "request.json");
        self.good(&[
            "compare",
            "inspect",
            "--repo",
            self.repo(),
            "--request",
            &saved,
        ])
    }
    fn review(&self, request: &Value, judgment: &Value) -> Value {
        let saved = self.save(request, "request.json");
        let judged = self.save(judgment, "judgment.json");
        self.good(&[
            "compare",
            "review",
            "--repo",
            self.repo(),
            "--request",
            &saved,
            "--judgment",
            &judged,
        ])
    }
    fn judgment(
        &self,
        inspected: &Value,
        classify: &dyn Fn(&Value) -> String,
        decision_rationale: &str,
        rationale: &str,
    ) -> Value {
        let decisions = get(inspected, "matches")
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                obj(vec![
                    ("path", get(row, "path").clone()),
                    ("classification", s(&classify(row))),
                    ("rationale", s(decision_rationale)),
                ])
            })
            .collect();
        obj(vec![
            ("observation", get(inspected, "observation").clone()),
            ("decisions", Value::Array(decisions)),
            ("pending_decisions", Value::Array(vec![])),
            ("rationale", s(rationale)),
        ])
    }
    /// Baseline `AdmissionContract.compared`.
    fn compared(&self, captured: &Value, classification: &str) -> (Value, Value) {
        let request = obj(vec![
            ("project_id", s(&self.pid)),
            (
                "candidate",
                get(get(captured, "request"), "candidate").clone(),
            ),
            ("searches", list(&["retry policy", "attempt cap"])),
            ("read_paths", Value::Array(vec![])),
        ]);
        let inspected = self.inspect(&request);
        let judgment = self.judgment(
            &inspected,
            &|_| classification.to_owned(),
            "The fixture meaning has been assessed with its evidence and conditions.",
            "Read the actual bounded plausible sources before deciding novelty.",
        );
        let proof = get(&self.review(&request, &judgment), "proof").clone();
        (request, proof)
    }
    fn replacement_capture(
        &self,
        old: &Value,
        path: &str,
        operation: &str,
        classification: &str,
        omit_policy: bool,
        updates: Vec<(&str, Value)>,
    ) -> Value {
        let mut request = self.admission_request();
        let metadata = request
            .get_mut("candidate")
            .unwrap()
            .get_mut("metadata")
            .unwrap();
        set(metadata, "id", get(old, "id").clone());
        for (key, value) in updates {
            set(metadata, key, value);
        }
        set(
            metadata.get_mut("origin").unwrap(),
            "session",
            s("correcting-session"),
        );
        set(
            metadata,
            "evidence",
            list(&["user:current-confirmed-correction"]),
        );
        if omit_policy {
            for key in ["trust", "visibility", "confidence", "admission-reason"] {
                pop(metadata, key);
            }
        }
        set(
            request.get_mut("candidate").unwrap(),
            "body",
            s(CANDIDATE_BODY),
        );
        let captured = get(&self.capture(&request), "capture").clone();
        let (compared, proof) = self.compared(&captured, classification);
        let mut value = self.request(old, path, operation);
        set(&mut value, "capture", captured);
        set(
            &mut value,
            "comparison",
            obj(vec![("request", compared), ("proof", proof)]),
        );
        set(
            value.get_mut("authority").unwrap(),
            "evidence",
            list(&["user:current-confirmed-correction"]),
        );
        value
    }
    fn superseding(&self, old: &Value, p1: &str, replacement: &Value, p2: &str) -> Value {
        let request = obj(vec![
            ("project_id", s(&self.pid)),
            (
                "candidate",
                obj(vec![
                    ("metadata", replacement.clone()),
                    ("body", s(&unpack(&self.read(p2)).1)),
                ]),
            ),
            ("searches", list(&["retry policy", "attempt cap"])),
            ("read_paths", list(&[p1, p2])),
        ]);
        let observed = self.inspect(&request);
        let judgment = self.judgment(
            &observed,
            &|row| {
                let path = text(row, "path");
                if path == p1 {
                    "contradiction"
                } else if path == p2 {
                    "equivalent"
                } else {
                    "related-context"
                }
                .to_owned()
            },
            "The new confirmed source resolves the wrong claim; the replacement is read unchanged.",
            "Use the independently evidenced existing correction without inventing a third claim.",
        );
        let reviewed = self.review(&request, &judgment);
        let mut value = self.request(old, p1, "supersede");
        set(
            &mut value,
            "replacement",
            obj(vec![
                ("path", s(p2)),
                ("id", get(replacement, "id").clone()),
                ("version", get(replacement, "version").clone()),
                ("sha256", get(&self.hashes(), p2).clone()),
            ]),
        );
        set(
            &mut value,
            "comparison",
            obj(vec![
                ("request", request),
                ("proof", get(&reviewed, "proof").clone()),
            ]),
        );
        value
    }
    /// Baseline `ConsolidationContract.compared` input for `am compact preview`.
    fn consolidation(
        &self,
        metadata: &Value,
        body: &str,
        paths: &[&str],
        captured: Value,
    ) -> Value {
        let request = obj(vec![
            ("project_id", s(&self.pid)),
            (
                "candidate",
                obj(vec![("metadata", metadata.clone()), ("body", s(body))]),
            ),
            ("searches", list(&["retry policy", "attempt cap"])),
            ("read_paths", list(paths)),
        ]);
        let observed = self.inspect(&request);
        let judgment = self.judgment(
            &observed,
            &|row| {
                if get(row, "metadata") != &Value::Null
                    && get(row, "lifecycle").as_str() == Some("active")
                {
                    "equivalent".into()
                } else {
                    "related-context".into()
                }
            },
            "The actual fixture meaning and conditions were compared; history stays provenance.",
            "Retain the confirmed retry meaning and every distinct source while tightening wording.",
        );
        let reviewed = self.review(&request, &judgment);
        obj(vec![
            ("comparison_request", request),
            ("comparison_proof", get(&reviewed, "proof").clone()),
            ("capture", captured),
            ("context", obj(vec![])),
        ])
    }
    fn compact_preview(&self, value: &Value) -> (i32, Value) {
        let saved = self.save(value, "request.json");
        self.call(&[
            "compact",
            "preview",
            "--repo",
            self.repo(),
            "--input",
            &saved,
        ])
    }
    fn compact_apply(&self, proposal: &Value) -> (i32, Value) {
        let saved = self.save(proposal, "maintenance.json");
        self.call(&["compact", "apply", "--proposal", &saved])
    }
    fn recall(&self, ids: &[&Value], kinds: &[&str]) -> Value {
        let request = obj(vec![
            ("project_id", s(&self.pid)),
            ("query", s("")),
            (
                "ids",
                Value::Array(ids.iter().map(|v| (*v).clone()).collect()),
            ),
            ("paths", Value::Array(vec![])),
            ("context", obj(vec![])),
            ("kinds", list(kinds)),
            ("minimum_trust", s("reviewed")),
            ("limit", Value::Int(8.into())),
            ("include_context", Value::Bool(false)),
        ]);
        let saved = self.save(&request, "request.json");
        self.good(&["search", "--repo", self.repo(), "--request", &saved])
    }
    fn compact_inactive(&self) {
        let mut sources = Vec::new();
        for directory in ["history/sources", "history/versions"] {
            if self.exists(directory) {
                let mut found = Vec::new();
                walk(&self.bank, &self.bank.join(directory), &mut found);
                sources.extend(found.into_iter().map(|(p, _)| p));
            }
        }
        for source in sources.into_iter().filter(|p| p.ends_with(".md")) {
            let file = self.bank.join(&source);
            let compacted = self.good(&[
                "inactive-records",
                "compact",
                "--file",
                file.to_str().unwrap(),
            ]);
            let content = self.world.root.join("inactive.md");
            std::fs::write(&content, text(&compacted, "content")).unwrap();
            let prepared = self.good(&[
                "writes",
                "preview",
                "--repo",
                self.repo(),
                "--path",
                &source,
                "--content",
                content.to_str().unwrap(),
            ]);
            let saved = self.save(&prepared, "compact.json");
            self.good(&["writes", "apply", "--proposal", &saved]);
        }
    }
    fn deletion(&self, metadata: &Value, path: &str, ids: &[&Value]) -> Value {
        let mut request = self.request(metadata, path, "delete");
        deletion(&mut request, ids);
        request
    }
    fn admission_preview(&self, captured: &Value, request: &Value, proof: &Value) -> (i32, Value) {
        let capture = self.save(captured, "capture.json");
        let saved = self.save(request, "request.json");
        let proof = self.save(proof, "proof.json");
        self.call(&[
            "admission",
            "preview",
            "--repo",
            self.repo(),
            "--capture",
            &capture,
            "--request",
            &saved,
            "--proof",
            &proof,
        ])
    }
}

fn journey(name: &str, body: impl FnOnce(&Journey<'_>)) {
    support::in_world(name, |world| body(&Journey::new(world)));
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_stronger_correction_keeps_stable_identity_old_origin_and_useful_history
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_stronger_correction_keeps_identity_origin_and_history() {
    journey(
        "journey_stronger_correction_keeps_identity_origin_and_history",
        stronger_correction,
    );
}
fn stronger_correction(j: &Journey<'_>) {
    let (old, path) = j.add(
        "incorrect",
        "# Retry policy\n\nThe older fixture incorrectly said five attempts.\n",
        vec![],
    );
    let value = j.replacement_capture(&old, &path, "correct", "contradiction", false, vec![]);
    let before = j.state();
    let (code, plan) = j.preview(&value);
    assert_eq!(code, 0, "{}", encode(&plan));
    assert_eq!(before, j.state());
    let (code, done) = j.apply(get(&plan, "proposal"));
    assert_eq!(code, 0, "{}", encode(&done));
    let (current, _) = unpack(&j.read(&path));
    assert_eq!(get(&current, "id"), get(&old, "id"));
    assert_eq!(get(&current, "version"), &Value::Int(2.into()));
    assert_eq!(
        get(get(&current, "origin"), "session"),
        &s("correcting-session")
    );
    assert!(
        get(&current, "corrects")
            .as_array()
            .unwrap()
            .contains(&reference(&old))
    );
    let (archived, previous) =
        unpack(&j.read(&format!("history/versions/{}/v1.md", text(&old, "id"))));
    assert_eq!(get(&archived, "origin"), get(&old, "origin"));
    assert_eq!(get(&archived, "status"), &s("superseded"));
    assert!(previous.contains("incorrectly said five attempts"));
    let after = j.state();
    assert_eq!(
        (&before.1, &before.2, &before.3),
        (&after.1, &after.2, &after.3)
    );
    let (code, replay) = j.apply(get(&plan, "proposal"));
    assert_eq!(code, 0, "{}", encode(&replay));
    assert_eq!(get(&replay, "already_applied"), &Value::Bool(true));
    assert_eq!(after, j.state());
}

// obligation: tests/contracts/test_unified_cli_operations.py::UnifiedCLIOperations.test_installed_correction_preserves_identity_origin_history_and_repeat
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_correct_alias_preserves_identity_origin_history_and_repeat() {
    support::in_world(
        "journey_correct_alias_preserves_identity_origin_history_and_repeat",
        |world| {
            let mut j = Journey::new(world);
            j.group = "correct";
            stronger_correction(&j);
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_reactivation_needs_new_evidence_and_an_explicit_current_period
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_reactivation_needs_new_evidence_and_current_period() {
    journey(
        "journey_reactivation_needs_new_evidence_and_current_period",
        |j| {
            let (old, path) = j.add("revoked", DEFAULT_BODY, vec![]);
            let (code, plan) = j.preview(&j.request(&old, &path, "revoke"));
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            let (old, _) = unpack(&j.read(&path));
            assert!(old.get("visibility").is_none());
            let now = host::utc_now_isoformat().unwrap();
            let value = j.replacement_capture(
                &old,
                &path,
                "reactivate",
                "contradiction",
                true,
                vec![("valid-from", s(&now))],
            );
            let (code, plan) = j.preview(&value);
            assert_eq!(code, 0, "{}", encode(&plan));
            let before = j.state();
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            let (current, _) = unpack(&j.read(&path));
            assert_eq!(get(&current, "status"), &s("active"));
            assert_eq!(get(&current, "version"), &Value::Int(2.into()));
            let (historical, _) =
                unpack(&j.read(&format!("history/versions/{}/v1.md", text(&old, "id"))));
            assert_eq!(get(&historical, "status"), &s("revoked"));
            assert_eq!(get(&historical, "origin"), get(&old, "origin"));
            assert!(text(&historical, "state-reason").contains(&path));
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_consumed_capture_identity_still_finds_revoked_learning_after_all_inactive_bodies_are_compacted
#[test]
#[ignore = "needs recovery_frontier, comparison, consolidation registered"]
fn journey_consumed_capture_identity_finds_revoked_learning_after_compaction() {
    journey(
        "journey_consumed_capture_identity_finds_revoked_learning_after_compaction",
        |j| {
            let (_, path) = j.add("existing", DEFAULT_BODY, vec![]);
            let capture_request = j.admission_request();
            let captured = get(&j.capture(&capture_request), "capture").clone();
            let candidate = get(&capture_request, "candidate");
            let compared = j.consolidation(
                get(candidate, "metadata"),
                text(candidate, "body"),
                &[&path],
                captured.clone(),
            );
            let (code, plan) = j.compact_preview(&compared);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.compact_apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            let (current, _) = unpack(&j.read(&path));
            let (code, plan) = j.preview(&j.request(&current, &path, "revoke"));
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            j.compact_inactive();
            let (request, proof) = j.compared(&captured, "equivalent");
            assert_eq!(get(&proof, "disposition"), &s("lifecycle-review-required"));
            let before = j.state();
            let (code, pending) = j.admission_preview(&captured, &request, &proof);
            assert_eq!(code, 0, "{}", encode(&pending));
            assert_eq!(get(&pending, "proposal"), &Value::Null);
            assert_eq!(
                get(&pending, "disposition"),
                &s("lifecycle-review-required")
            );
            assert_eq!(before, j.state());
        },
    );
}

fn move_route(bank: &Path, previous: &str, current: &str) {
    let mut found = Vec::new();
    walk(bank, bank, &mut found);
    for (routed, raw) in found.into_iter().filter(|(p, _)| p.ends_with(".md")) {
        let base = Path::new(&routed)
            .parent()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let relative = |target: &str| {
            let depth = if base.is_empty() {
                0
            } else {
                base.split('/').count()
            };
            let mut rel = "../".repeat(depth);
            rel.push_str(target);
            rel
        };
        let before = format!("[{previous}](<{}>)", relative(previous));
        let after = format!("[{current}](<{}>)", relative(current));
        let raw = String::from_utf8(raw).unwrap();
        if raw.contains(&before) {
            std::fs::write(bank.join(&routed), raw.replace(&before, &after)).unwrap();
        }
    }
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_legacy_flat_consumed_capture_stays_pending_after_revocation_and_compaction
#[test]
#[ignore = "needs recovery_frontier, comparison, consolidation, retrieval registered"]
fn journey_legacy_flat_consumed_capture_stays_pending() {
    journey("journey_legacy_flat_consumed_capture_stays_pending", |j| {
        let (old, path) = j.add("existing", DEFAULT_BODY, vec![]);
        let mut capture_request = j.admission_request();
        set(
            capture_request
                .get_mut("candidate")
                .unwrap()
                .get_mut("metadata")
                .unwrap(),
            "derived-from",
            Value::Array(vec![reference(&old)]),
        );
        let captured = get(&j.capture(&capture_request), "capture").clone();
        let candidate = get(&capture_request, "candidate");
        let compared = j.consolidation(
            get(candidate, "metadata"),
            text(candidate, "body"),
            &[&path],
            captured.clone(),
        );
        let (code, plan) = j.compact_preview(&compared);
        assert_eq!(code, 0, "{}", encode(&plan));
        let (code, done) = j.compact_apply(get(&plan, "proposal"));
        assert_eq!(code, 0, "{}", encode(&done));
        let mut found = Vec::new();
        walk(&j.bank, &j.bank.join("history/sources"), &mut found);
        let canonical = found
            .into_iter()
            .map(|(p, _)| p)
            .find(|p| p.ends_with(".md"))
            .unwrap();
        let name = Path::new(&canonical)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let legacy = format!("history/sources/{name}");
        let (mut source, body) = unpack(&j.read(&canonical));
        assert!(get(&source, "derived-from").as_array().unwrap().len() >= 2);
        if let Some(Value::Array(items)) = source.get_mut("derived-from") {
            items.reverse();
        }
        std::fs::write(j.bank.join(&legacy), render(&source, &body)).unwrap();
        std::fs::remove_file(j.bank.join(&canonical)).unwrap();
        move_route(&j.bank, &canonical, &legacy);
        let (current, _) = unpack(&j.read(&path));
        let (code, plan) = j.preview(&j.request(&current, &path, "revoke"));
        assert_eq!(code, 0, "{}", encode(&plan));
        let (code, done) = j.apply(get(&plan, "proposal"));
        assert_eq!(code, 0, "{}", encode(&done));
        j.compact_inactive();
        let before = j.state();
        let (request, proof) = j.compared(&captured, "related-context");
        assert_eq!(
            get(&proof, "disposition"),
            &s("provenance-identity-review-required")
        );
        let inspection = j.inspect(&request);
        assert_eq!(
            get(&inspection, "capture_identity_review_paths"),
            &list(&[&legacy])
        );
        assert!(
            get(&inspection, "matches")
                .as_array()
                .unwrap()
                .iter()
                .any(|row| text(row, "path") == legacy)
        );
        let (code, pending) = j.admission_preview(&captured, &request, &proof);
        assert_eq!(code, 0, "{}", encode(&pending));
        assert_eq!(get(&pending, "proposal"), &Value::Null);
        assert_eq!(
            get(&pending, "disposition"),
            &s("provenance-identity-review-required")
        );
        assert_eq!(before, j.state());
        let capture_id = get(
            get(get(get(&captured, "request"), "candidate"), "metadata"),
            "id",
        );
        assert_eq!(
            get(&j.recall(&[capture_id], &["semantic-fact"]), "results"),
            &Value::Array(vec![])
        );
        let retained = j.read(&legacy);
        std::fs::write(j.bank.join(&canonical), &retained).unwrap();
        std::fs::remove_file(j.bank.join(&legacy)).unwrap();
        move_route(&j.bank, &legacy, &canonical);
        assert_eq!(j.read(&canonical), retained);
        let (_, proof) = j.compared(&captured, "equivalent");
        assert_eq!(get(&proof, "disposition"), &s("lifecycle-review-required"));
    });
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_scope_correction_retains_older_branch_guidance_without_applying_it_on_main
#[test]
#[ignore = "needs recovery_frontier, comparison, retrieval registered"]
fn journey_scope_correction_retains_older_branch_guidance() {
    journey(
        "journey_scope_correction_retains_older_branch_guidance",
        |j| {
            let (old, path) = j.add("older-policy", CANDIDATE_BODY, vec![]);
            let mut value = j.replacement_capture(
                &old,
                &path,
                "correct",
                "scope-correction",
                false,
                vec![("conditions", obj(vec![("branch", s("older/release"))]))],
            );
            set(
                value.get_mut("authority").unwrap(),
                "impact",
                s("narrower-contexts-remain"),
            );
            let (code, plan) = j.preview(&value);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            assert_eq!(
                get(&j.recall(&[get(&old, "id")], &["semantic-fact"]), "results"),
                &Value::Array(vec![])
            );
            j.world.git(&j.repo, &["checkout", "-b", "older/release"]);
            let older = j.recall(&[get(&old, "id")], &["semantic-fact"]);
            let results = get(&older, "results").as_array().unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(
                get(get(&results[0], "record"), "version"),
                &Value::Int(2.into())
            );
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_narrowing_cannot_replace_the_old_claim_that_still_applies_in_older_contexts
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_narrowing_cannot_replace_old_claim_in_older_contexts() {
    journey(
        "journey_narrowing_cannot_replace_old_claim_in_older_contexts",
        |j| {
            let (old, path) = j.add(
                "older-policy",
                "# Retry policy\n\nFive attempts still applies to the older branch.\n",
                vec![],
            );
            let mut value = j.replacement_capture(
                &old,
                &path,
                "correct",
                "contradiction",
                false,
                vec![("conditions", obj(vec![("branch", s("older/release"))]))],
            );
            set(
                value.get_mut("authority").unwrap(),
                "impact",
                s("narrower-contexts-remain"),
            );
            let before = j.state();
            let (code, pending) = j.preview(&value);
            assert_eq!(code, 0, "{}", encode(&pending));
            assert_eq!(get(&pending, "proposal"), &Value::Null);
            assert_eq!(get(&pending, "disposition"), &s("narrowing-required"));
            assert_eq!(before, j.state());
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_existing_replacement_is_versioned_and_survives_deletion_of_the_wrong_old_claim
#[test]
#[ignore = "needs recovery_frontier, comparison, retrieval registered"]
fn journey_existing_replacement_is_versioned_and_survives_deletion() {
    journey(
        "journey_existing_replacement_is_versioned_and_survives_deletion",
        |j| {
            let (old, p1) = j.add(
                "wrong",
                "# Retry policy\n\nFive attempts was the disproved old claim.\n",
                vec![],
            );
            let (replacement, p2) = j.add(
                "correct",
                "# Retry policy\n\nThree attempts is the independently evidenced policy.\n",
                vec![],
            );
            let value = j.superseding(&old, &p1, &replacement, &p2);
            let mut compared_request = get(get(&value, "comparison"), "request").clone();
            let before = j.state();
            let (code, plan) = j.preview(&value);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            let (current, body) = unpack(&j.read(&p2));
            assert_eq!(get(&current, "version"), &Value::Int(2.into()));
            assert_eq!(get(&current, "origin"), get(&replacement, "origin"));
            assert!(
                get(&current, "supersedes")
                    .as_array()
                    .unwrap()
                    .contains(&reference(&old))
            );
            assert_eq!(get(&unpack(&j.read(&p1)).0, "status"), &s("superseded"));
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
            let deletion = j.deletion(&unpack(&j.read(&p1)).0, &p1, &[get(&old, "id")]);
            let (code, plan) = j.preview(&deletion);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            let (after, text_after) = unpack(&j.read(&p2));
            assert_eq!(text_after, body);
            assert_eq!(get(&after, "origin"), get(&replacement, "origin"));
            assert_eq!(get(&after, "supersedes"), &Value::Array(vec![]));
            set(
                &mut compared_request,
                "candidate",
                obj(vec![("metadata", after), ("body", s(&text_after))]),
            );
            set(&mut compared_request, "read_paths", list(&[&p2]));
            let inspection = j.inspect(&compared_request);
            assert!(
                !get(&inspection, "matches")
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| text(row, "path") == p1)
            );
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_deletion_requires_reviewed_removal_of_copied_ordinary_context
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_deletion_requires_reviewed_removal_of_copied_context() {
    journey(
        "journey_deletion_requires_reviewed_removal_of_copied_context",
        |j| {
            let (old, path) = j.add(
                "erase",
                "# Retry policy\n\nERASE_THIS_COPIED_ASSERTION.\n",
                vec![],
            );
            let original = j.read("projectbrief.md");
            std::fs::write(
                j.bank.join("projectbrief.md"),
                original.clone() + &unpack(&j.read(&path)).1,
            )
            .unwrap();
            let mut value = j.deletion(&old, &path, &[get(&old, "id")]);
            let before = j.state();
            let (code, refused) = j.preview(&value);
            assert_eq!(code, 1, "{}", encode(&refused));
            assert_eq!(before, j.state());
            set(
                &mut value,
                "context_edits",
                obj(vec![("projectbrief.md", s(&original))]),
            );
            let (code, plan) = j.preview(&value);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            assert_eq!(j.read("projectbrief.md"), original);
            let after = j.state();
            assert!(
                !after.0.iter().any(
                    |(_, b)| String::from_utf8_lossy(b).contains("ERASE_THIS_COPIED_ASSERTION")
                )
            );
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_deletion_refuses_a_copied_router_assertion_until_its_existing_file_workflow_cleans_it
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_deletion_refuses_copied_router_assertion() {
    journey("journey_deletion_refuses_copied_router_assertion", |j| {
        let (old, path) = j.add(
            "erase",
            "# Retry policy\n\nERASE_THIS_ROUTER_COPY.\n",
            vec![],
        );
        let original = j.read("MEMORY.md");
        let body = unpack(&j.read(&path)).1;
        std::fs::write(
            j.bank.join("MEMORY.md"),
            original.replace("\n## Routes\n", &format!("\n{body}\n## Routes\n")),
        )
        .unwrap();
        let value = j.deletion(&old, &path, &[get(&old, "id")]);
        let before = j.state();
        let (code, refused) = j.preview(&value);
        assert_eq!(code, 1, "{}", encode(&refused));
        assert_eq!(before, j.state());
        std::fs::write(j.bank.join("MEMORY.md"), original).unwrap();
        let value = j.deletion(&old, &path, &[get(&old, "id")]);
        let (code, plan) = j.preview(&value);
        assert_eq!(code, 0, "{}", encode(&plan));
        let (code, done) = j.apply(get(&plan, "proposal"));
        assert_eq!(code, 0, "{}", encode(&done));
        assert!(
            !j.state()
                .0
                .iter()
                .any(|(_, b)| String::from_utf8_lossy(b).contains("ERASE_THIS_ROUTER_COPY"))
        );
    });
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_deleted_independent_duplicates_cannot_return_from_cached_consolidation_output
#[test]
#[ignore = "needs recovery_frontier, comparison, consolidation, projections registered"]
fn journey_deleted_duplicates_cannot_return_from_cached_consolidation() {
    journey(
        "journey_deleted_duplicates_cannot_return_from_cached_consolidation",
        |j| {
            let (first, p1) = j.add(
                "first-copy",
                "# Retry policy\n\nUse three attempts; this old duplicate must be erased.\n",
                vec![],
            );
            let (second, p2) = j.add(
                "independent-copy",
                "# Attempt cap\n\nUse three attempts for this fixture; independently captured.\n",
                vec![],
            );
            let compared = j.consolidation(
                &first,
                "# Retry policy\n\nUse three attempts.\n",
                &[&p1, &p2],
                Value::Null,
            );
            let (code, maintenance) = j.compact_preview(&compared);
            assert_eq!(code, 0, "{}", encode(&maintenance));
            let deletion = j.deletion(&first, &p1, &[get(&first, "id"), get(&second, "id")]);
            let before = j.state();
            let (code, plan) = j.preview(&deletion);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            assert!(!j.exists(&p1));
            assert!(!j.exists(&p2));
            let after = j.state();
            let (code, refused) = j.compact_apply(get(&maintenance, "proposal"));
            assert_eq!(code, 1, "{}", encode(&refused));
            assert_eq!(after, j.state());
            let records: Vec<_> = [&first, &second]
                .iter()
                .map(|m| {
                    obj(vec![
                        ("id", get(m, "id").clone()),
                        ("version", Value::Int(1.into())),
                        (
                            "body",
                            s("cached old learning cannot enter rebuilt guidance"),
                        ),
                    ])
                })
                .collect();
            let saved = j.save(
                &obj(vec![
                    ("project_id", s(&j.pid)),
                    ("records", Value::Array(records)),
                ]),
                "request.json",
            );
            let rebuilt = j.good(&["projection", "build", "--repo", j.repo(), "--input", &saved]);
            assert_eq!(
                get(get(&rebuilt, "projection"), "records"),
                &Value::Array(vec![])
            );
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_erasure_includes_consolidated_capture_identity_so_a_saved_capture_cannot_be_admitted_again
#[test]
#[ignore = "needs recovery_frontier, comparison, consolidation registered"]
fn journey_erasure_includes_consolidated_capture_identity() {
    journey(
        "journey_erasure_includes_consolidated_capture_identity",
        |j| {
            let (old, path) = j.add("existing", DEFAULT_BODY, vec![]);
            let mut capture_request = j.admission_request();
            set(
                capture_request
                    .get_mut("candidate")
                    .unwrap()
                    .get_mut("metadata")
                    .unwrap()
                    .get_mut("origin")
                    .unwrap(),
                "session",
                s("later-observation"),
            );
            let captured = get(&j.capture(&capture_request), "capture").clone();
            let capture_id = get(get(get(&capture_request, "candidate"), "metadata"), "id").clone();
            let candidate = get(&capture_request, "candidate");
            let compared = j.consolidation(
                get(candidate, "metadata"),
                text(candidate, "body"),
                &[&path],
                captured,
            );
            let (code, plan) = j.compact_preview(&compared);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.compact_apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            let mut found = Vec::new();
            walk(&j.bank, &j.bank.join("history/sources"), &mut found);
            let structured = found
                .into_iter()
                .map(|(p, _)| p)
                .find(|p| p.ends_with(".md"))
                .unwrap();
            let (source, _) = unpack(&j.read(&structured));
            assert_eq!(
                Path::new(&structured)
                    .parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap(),
                capture_id.as_str().unwrap()
            );
            let (current, _) = unpack(&j.read(&path));
            let request = j.deletion(&current, &path, &[get(&old, "id"), get(&source, "id")]);
            let before = j.state();
            let (code, pending) = j.preview(&request);
            assert_eq!(code, 0, "{}", encode(&pending));
            assert_eq!(get(&pending, "proposal"), &Value::Null);
            let mut required: Vec<_> = get(&pending, "required_delete_ids")
                .as_array()
                .unwrap()
                .to_vec();
            let mut expected = vec![
                get(&old, "id").clone(),
                get(&source, "id").clone(),
                capture_id.clone(),
            ];
            required.sort_by_key(encode);
            expected.sort_by_key(encode);
            assert_eq!(required, expected);
            assert_eq!(before, j.state());
            let flat = format!("history/sources/{}.md", text(&source, "id"));
            std::fs::rename(j.bank.join(&structured), j.bank.join(&flat)).unwrap();
            let router = j.read("MEMORY.md").replace(&structured, &flat);
            std::fs::write(j.bank.join("MEMORY.md"), router).unwrap();
            let legacy = j.deletion(&current, &path, &[get(&old, "id")]);
            let before_legacy = j.state();
            let (code, pending) = j.preview(&legacy);
            assert_eq!(code, 0, "{}", encode(&pending));
            assert_eq!(
                get(&pending, "disposition"),
                &s("provenance-identity-review-required")
            );
            assert_eq!(get(&pending, "proposal"), &Value::Null);
            assert_eq!(before_legacy, j.state());
            std::fs::rename(j.bank.join(&flat), j.bank.join(&structured)).unwrap();
            let router = j.read("MEMORY.md").replace(&flat, &structured);
            std::fs::write(j.bank.join("MEMORY.md"), router).unwrap();
            let request = j.deletion(
                &current,
                &path,
                &[get(&old, "id"), get(&source, "id"), &capture_id],
            );
            let (code, plan) = j.preview(&request);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            for identity in get(&request, "delete_ids").as_array().unwrap() {
                assert!(j.exists(&format!(
                    "history/tombstones/{}.md",
                    identity.as_str().unwrap()
                )));
            }
            let replay = j.capture(&capture_request);
            assert_eq!(get(&replay, "decision"), &s("quarantine"));
            assert_eq!(get(&replay, "capture"), &Value::Null);
            assert!(!j.exists(&path));
            assert!(!j.exists(&structured));
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_revocation_removes_current_guidance_and_invalidates_projection_and_cached_payload
#[test]
#[ignore = "needs recovery_frontier, projections registered"]
fn journey_revocation_removes_current_guidance_and_invalidates_projection() {
    journey(
        "journey_revocation_removes_current_guidance_and_invalidates_projection",
        |j| {
            let (metadata, path) = j.add(
                "obsolete",
                "# Retry policy\n\nOBSOLETE_BODY_MUST_NOT_BE_CURRENT.\n",
                vec![],
            );
            let projection = j.projection(&metadata);
            let request = j.request(&metadata, &path, "revoke");
            let before = j.state();
            let (code, plan) = j.preview(&request);
            assert_eq!(code, 0, "{}", encode(&plan));
            assert_eq!(before, j.state());
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            assert_eq!(get(&done, "governance_verified"), &Value::Bool(true));
            let (current, body) = unpack(&j.read(&path));
            assert_eq!(get(&current, "status"), &s("revoked"));
            assert_eq!(get(&current, "origin"), get(&metadata, "origin"));
            assert!(!body.contains("OBSOLETE_BODY_MUST_NOT_BE_CURRENT"));
            let saved = j.save(&projection, "request.json");
            let (code, stale) =
                j.call(&["projection", "check", "--repo", j.repo(), "--input", &saved]);
            assert_eq!(code, 1, "{}", encode(&stale));
            assert!(stale.get("records").is_none());
            let request = obj(vec![
                ("project_id", s(&j.pid)),
                (
                    "records",
                    Value::Array(vec![obj(vec![
                        ("id", get(&metadata, "id").clone()),
                        ("version", Value::Int(1.into())),
                        ("body", s("OBSOLETE_BODY_MUST_NOT_BE_CURRENT")),
                    ])]),
                ),
            ]);
            let saved = j.save(&request, "request.json");
            let guarded = j.good(&[
                "inactive-records",
                "filter",
                "--repo",
                j.repo(),
                "--input",
                &saved,
            ]);
            assert_eq!(get(&guarded, "remaining_references"), &Value::Array(vec![]));
            assert!(!encode(&guarded).contains("OBSOLETE_BODY_MUST_NOT_BE_CURRENT"));
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
            let (code, replay) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&replay));
            assert_eq!(get(&replay, "already_applied"), &Value::Bool(true));
            assert_eq!(after, j.state());
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_expiry_requires_actual_known_boundary_and_policy_retention_is_distinct
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_expiry_requires_known_boundary_and_policy_retention() {
    support::in_world(
        "journey_expiry_requires_known_boundary_and_policy_retention",
        |world| {
            for (index, (operation, basis, until, expected)) in [
                (
                    "expire",
                    "time-boundary",
                    s("2020-01-01T00:00:00Z"),
                    Some("expired"),
                ),
                ("expire", "time-boundary", s("2099-01-01T00:00:00Z"), None),
                ("expire", "time-boundary", Value::Null, None),
                ("retention", "policy", Value::Null, Some("revoked")),
            ]
            .into_iter()
            .enumerate()
            {
                let j = Journey::named(world, &format!("repo-{index}"));
                let (metadata, path) = j.add("bounded", DEFAULT_BODY, vec![("valid-until", until)]);
                let mut request = j.request(&metadata, &path, operation);
                set(request.get_mut("authority").unwrap(), "basis", s(basis));
                if operation == "expire" {
                    set(
                        request.get_mut("authority").unwrap(),
                        "source_class",
                        s("tool"),
                    );
                }
                let before = j.state();
                let (code, plan) = j.preview(&request);
                let Some(expected) = expected else {
                    assert_eq!(code, 1, "{}", encode(&plan));
                    assert_eq!(before, j.state());
                    continue;
                };
                assert_eq!(code, 0, "{}", encode(&plan));
                let (code, done) = j.apply(get(&plan, "proposal"));
                assert_eq!(code, 0, "{}", encode(&done));
                assert_eq!(get(&unpack(&j.read(&path)).0, "status"), &s(expected));
                let after = j.state();
                assert_eq!(
                    (&before.1, &before.2, &before.3),
                    (&after.1, &after.2, &after.3)
                );
            }
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_ambiguous_evidence_or_older_contexts_still_applying_stays_pending_without_pruning
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_ambiguous_or_older_contexts_stay_pending() {
    support::in_world(
        "journey_ambiguous_or_older_contexts_stay_pending",
        |world| {
            for (index, (key, value)) in [
                ("strength", s("ambiguous")),
                ("confirmed", Value::Bool(false)),
                ("impact", s("narrower-contexts-remain")),
                ("source_class", s("assistant")),
            ]
            .into_iter()
            .enumerate()
            {
                let j = &Journey::named(world, &format!("repo-{index}"));
                let (metadata, path) = j.add("conditional", DEFAULT_BODY, vec![]);
                let mut request = j.request(&metadata, &path, "revoke");
                set(request.get_mut("authority").unwrap(), key, value);
                let before = j.state();
                let (code, pending) = j.preview(&request);
                assert_eq!(code, 0, "{}", encode(&pending));
                assert_eq!(get(&pending, "proposal"), &Value::Null);
                assert_eq!(get(&pending, "detecting_developer_id"), &s(&j.actor));
                assert_eq!(get(&pending, "messages_sent"), &Value::Bool(false));
                assert_eq!(before, j.state());
            }
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_correction_cannot_reuse_unchanged_evidence_or_promote_a_procedural_capture
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_correction_cannot_reuse_evidence_or_promote_procedure() {
    support::in_world(
        "journey_correction_cannot_reuse_evidence_or_promote_procedure",
        |world| {
            for (index, case) in ["no-new-evidence", "procedure"].into_iter().enumerate() {
                let j = &Journey::named(world, &format!("repo-{index}"));
                let evidence = if case == "no-new-evidence" {
                    "user:current-confirmed-correction"
                } else {
                    "evidence.md"
                };
                let kind = if case == "procedure" {
                    "procedural-candidate"
                } else {
                    "semantic-fact"
                };
                let (old, path) = j.add(
                    "existing",
                    DEFAULT_BODY,
                    vec![("evidence", list(&[evidence])), ("kind", s(kind))],
                );
                let value = j.replacement_capture(
                    &old,
                    &path,
                    "correct",
                    "contradiction",
                    false,
                    vec![("kind", s(kind))],
                );
                let before = j.state();
                let (code, refused) = j.preview(&value);
                assert_eq!(code, 1, "{}", encode(&refused));
                assert_eq!(before, j.state());
            }
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_unresolved_other_developer_claim_keeps_correction_pending
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_unresolved_other_developer_claim_keeps_correction_pending() {
    journey(
        "journey_unresolved_other_developer_claim_keeps_correction_pending",
        |j| {
            let (old, path) = j.add("old", DEFAULT_BODY, vec![]);
            let body = "# Retry policy\n\nAnother overlapping claim still needs clarification.\n";
            let (mut peer, peer_path) = j.add("peer", body, vec![]);
            let peer_id = host::uuid4().unwrap();
            set(peer.get_mut("origin").unwrap(), "developer-id", s(&peer_id));
            std::fs::write(j.bank.join(&peer_path), render(&peer, body)).unwrap();
            let value =
                j.replacement_capture(&old, &path, "correct", "contradiction", false, vec![]);
            let before = j.state();
            let (code, pending) = j.preview(&value);
            assert_eq!(code, 0, "{}", encode(&pending));
            assert_eq!(get(&pending, "proposal"), &Value::Null);
            assert!(
                get(&pending, "other_developer_ids")
                    .as_array()
                    .unwrap()
                    .contains(&s(&peer_id))
            );
            assert_eq!(
                get(&pending, "recommend_other_developer_clarification"),
                &Value::Bool(true)
            );
            assert_eq!(get(&pending, "messages_sent"), &Value::Bool(false));
            assert_eq!(before, j.state());
        },
    );
}

fn verified_repository(j: &Journey<'_>, request: &mut Value) {
    let sha = agent_memory::developer_identity::digest(Some(
        &std::fs::read(j.repo.join("evidence.md")).unwrap(),
    ))
    .unwrap();
    let authority = request.get_mut("authority").unwrap();
    set(authority, "source_class", s("verified-repository"));
    set(authority, "evidence", list(&["evidence.md"]));
    set(
        authority,
        "repository_evidence",
        obj(vec![("path", s("evidence.md")), ("sha256", s(&sha))]),
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_changed_actual_repository_evidence_refuses_automatic_pruning
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_changed_repository_evidence_refuses_pruning() {
    journey("journey_changed_repository_evidence_refuses_pruning", |j| {
        let (old, path) = j.add("old", DEFAULT_BODY, vec![]);
        let mut value = j.request(&old, &path, "revoke");
        verified_repository(j, &mut value);
        let (code, plan) = j.preview(&value);
        assert_eq!(code, 0, "{}", encode(&plan));
        std::fs::write(
            j.repo.join("evidence.md"),
            "# Later source that no longer supports the earlier observation\n",
        )
        .unwrap();
        let before = j.state();
        let (code, refused) = j.apply(get(&plan, "proposal"));
        assert_eq!(code, 1, "{}", encode(&refused));
        assert_eq!(before, j.state());
    });
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_a_hidden_other_type_superseder_cannot_silently_defeat_the_new_replacement_version
#[test]
#[ignore = "needs recovery_frontier, comparison, retrieval registered"]
fn journey_hidden_other_type_superseder_cannot_defeat_replacement() {
    journey(
        "journey_hidden_other_type_superseder_cannot_defeat_replacement",
        |j| {
            let (old, p1) = j.add("old", DEFAULT_BODY, vec![]);
            let (replacement, p2) = j.add("replacement", DEFAULT_BODY, vec![]);
            j.add(
                "unrelated-marker",
                "# Withdrawal marker\n\nUnrelated operational orientation.\n",
                vec![
                    ("kind", s("procedural-candidate")),
                    ("visibility", s("hidden")),
                    (
                        "supersedes",
                        Value::Array(vec![obj(vec![
                            ("id", get(&replacement, "id").clone()),
                            ("version", Value::Int(2.into())),
                        ])]),
                    ),
                ],
            );
            let current = j.recall(&[get(&replacement, "id")], &["semantic-fact"]);
            let versions: Vec<_> = get(&current, "results")
                .as_array()
                .unwrap()
                .iter()
                .map(|row| get(get(row, "record"), "version").clone())
                .collect();
            assert_eq!(versions, vec![Value::Int(1.into())]);
            let value = j.superseding(&old, &p1, &replacement, &p2);
            let before = j.state();
            let (code, refused) = j.preview(&value);
            assert_eq!(code, 1, "{}", encode(&refused));
            assert_eq!(before, j.state());
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_current_context_only_deletion_does_not_globally_erase_a_record
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_current_context_only_deletion_is_pending() {
    journey("journey_current_context_only_deletion_is_pending", |j| {
        let (old, path) = j.add("conditional", DEFAULT_BODY, vec![]);
        let mut value = j.deletion(&old, &path, &[get(&old, "id")]);
        set(
            value.get_mut("authority").unwrap(),
            "impact",
            s("narrower-contexts-remain"),
        );
        let before = j.state();
        let (code, pending) = j.preview(&value);
        assert_eq!(code, 0, "{}", encode(&pending));
        assert_eq!(get(&pending, "proposal"), &Value::Null);
        assert_eq!(get(&pending, "disposition"), &s("narrowing-required"));
        assert_eq!(before, j.state());
    });
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_terminal_deletion_requires_explicit_bounded_derived_scope_and_removes_only_safe_markers
#[test]
#[ignore = "needs recovery_frontier, projections registered"]
fn journey_terminal_deletion_requires_bounded_derived_scope() {
    journey(
        "journey_terminal_deletion_requires_bounded_derived_scope",
        |j| {
            let (original, p1) = j.add(
                "original",
                "# Retry policy\n\nDELETED_CANONICAL_PAYLOAD.\n",
                vec![],
            );
            let (derived, p2) = j.add(
                "derived",
                "# Derived retry note\n\nDELETED_CANONICAL_PAYLOAD.\n",
                vec![(
                    "derived-from",
                    Value::Array(vec![obj(vec![
                        ("id", get(&original, "id").clone()),
                        ("version", Value::Int(1.into())),
                    ])]),
                )],
            );
            let projection = j.projection(&original);
            let mut request = j.deletion(&original, &p1, &[get(&original, "id")]);
            let before = j.state();
            let (code, pending) = j.preview(&request);
            assert_eq!(code, 0, "{}", encode(&pending));
            assert_eq!(
                get(&pending, "disposition"),
                &s("deletion-scope-review-required")
            );
            assert_eq!(get(&pending, "proposal"), &Value::Null);
            assert_eq!(before, j.state());
            set(
                &mut request,
                "delete_ids",
                Value::Array(vec![
                    get(&original, "id").clone(),
                    get(&derived, "id").clone(),
                ]),
            );
            let (code, plan) = j.preview(&request);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            assert!(!j.exists(&p1));
            assert!(!j.exists(&p2));
            for identity in [&original, &derived] {
                let (metadata, body) =
                    unpack(&j.read(&format!("history/tombstones/{}.md", text(identity, "id"))));
                let keys: Vec<_> = metadata
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .collect();
                assert_eq!(
                    keys,
                    [
                        "id",
                        "scope",
                        "deleted-at",
                        "deletion-authority",
                        "reason-class"
                    ]
                );
                assert_eq!(body, "");
            }
            assert!(
                !j.state()
                    .0
                    .iter()
                    .any(|(_, b)| String::from_utf8_lossy(b).contains("DELETED_CANONICAL_PAYLOAD"))
            );
            let saved = j.save(&projection, "request.json");
            let (code, stale) =
                j.call(&["projection", "check", "--repo", j.repo(), "--input", &saved]);
            assert_eq!(code, 1, "{}", encode(&stale));
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance.py::GovernanceContract.test_stale_target_or_context_review_and_tampered_delta_refuse_without_writes
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_stale_target_context_or_tampered_delta_refuse() {
    support::in_world(
        "journey_stale_target_context_or_tampered_delta_refuse",
        |world| {
            for (index, mutation) in ["target", "context", "delta"].into_iter().enumerate() {
                let j = &Journey::named(world, &format!("repo-{index}"));
                let (metadata, path) = j.add("old", DEFAULT_BODY, vec![]);
                let mut request = j.request(&metadata, &path, "revoke");
                if mutation == "context" {
                    set(&mut request, "context_review", obj(vec![]));
                }
                if mutation == "target" {
                    set(
                        request.get_mut("target").unwrap(),
                        "sha256",
                        s(&"0".repeat(64)),
                    );
                }
                let before = j.state();
                let (code, plan) = j.preview(&request);
                if mutation != "delta" {
                    assert_eq!(code, 1, "{}", encode(&plan));
                } else {
                    assert_eq!(code, 0, "{}", encode(&plan));
                    let mut proposal = get(&plan, "proposal").clone();
                    let files = proposal.get_mut("files").unwrap();
                    let brief = format!(
                        "{}\nUnrelated fabricated edit.\n",
                        text(files, "projectbrief.md")
                    );
                    set(files, "projectbrief.md", s(&brief));
                    let (code, refused) = j.apply(&proposal);
                    assert_eq!(code, 1, "{}", encode(&refused));
                }
                assert_eq!(before, j.state());
            }
        },
    );
}

// obligation: tests/contracts/test_installed_bank_governance.py::InstalledBankRoundtrip.test_governance_alias_checks_intent_at_preview_and_apply_before_writes
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_governance_alias_checks_intent_then_revokes() {
    journey("journey_governance_alias_checks_intent_then_revokes", |j| {
        let (old, path) = j.add("wrong", DEFAULT_BODY, vec![]);
        let request = j.request(&old, &path, "revoke");
        let before = j.state();
        let saved = j.save(&request, "request.json");
        for group in ["correct", "supersede", "forget"] {
            let (code, refused) =
                j.call(&[group, "preview", "--repo", j.repo(), "--request", &saved]);
            assert_eq!(code, 1, "{}", encode(&refused));
            assert_eq!(j.state(), before);
        }
        let proposal = j.good(&["revoke", "preview", "--repo", j.repo(), "--request", &saved]);
        let prepared = j.save(get(&proposal, "proposal"), "revoke-proposal.json");
        let (code, refused) = j.call(&["forget", "apply", "--proposal", &prepared]);
        assert_eq!(code, 1, "{}", encode(&refused));
        assert_eq!(j.state(), before);
        let done = j.good(&["revoke", "apply", "--proposal", &prepared]);
        assert_eq!(get(&done, "governance_verified"), &Value::Bool(true));
        assert_eq!(get(&unpack(&j.read(&path)).0, "status"), &s("revoked"));
        let after = j.state();
        assert_eq!(
            (&before.1, &before.2, &before.3),
            (&after.1, &after.2, &after.3)
        );
    });
}

/// Run the `am` binary as an owned child with fault-injection settings.
fn faulted(
    j: &Journey<'_>,
    args: &[&str],
    env: &[(&str, OsString)],
) -> agent_memory::process::Output {
    let mut argv = vec![OsString::from(env!("CARGO_BIN_EXE_am"))];
    argv.extend(args.iter().map(OsString::from));
    j.world.command(&argv, env)
}
fn recover(j: &Journey<'_>, mode: &str) -> Value {
    let value = j.good(&["transaction", "recover", "--repo", j.repo(), "--mode", mode]);
    let saved = j.save(&value, "recover.json");
    j.good(&[
        "transaction",
        "recover",
        "--repo",
        j.repo(),
        "--proposal",
        &saved,
    ])
}
fn unlock(j: &Journey<'_>) {
    let value = j.good(&["writes", "recover", "--repo", j.repo()]);
    let saved = j.save(&value, "unlock.json");
    j.good(&[
        "writes",
        "recover",
        "--repo",
        j.repo(),
        "--proposal",
        &saved,
    ]);
}

// obligation: tests/contracts/test_governance_recovery.py::GovernanceRecoveryContract.test_correction_crash_requires_owned_lock_recovery_and_rollback_before_fresh_comparison
#[test]
#[ignore = "needs recovery_frontier, recovery_transactions, comparison registered"]
fn journey_correction_crash_requires_recovery_and_rollback() {
    journey(
        "journey_correction_crash_requires_recovery_and_rollback",
        |j| {
            let (old, path) = j.add(
                "wrong",
                "# Retry policy\n\nThe disproved old claim allowed five attempts.\n",
                vec![],
            );
            let value =
                j.replacement_capture(&old, &path, "correct", "contradiction", false, vec![]);
            let proposal = j.prepared(&value);
            let before = j.state();
            let saved = j.save(&proposal, "request.json");
            let output = faulted(
                j,
                &["govern", "apply", "--proposal", &saved],
                &[
                    (
                        "AGENT_MEMORY_FAULT_STAGE",
                        format!("transaction-after-write:{path}").into(),
                    ),
                    ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
                ],
            );
            assert_eq!(output.status, agent_memory::process::Status::Code(86));
            unlock(j);
            assert_eq!(
                j.call(&[
                    "transaction",
                    "recover",
                    "--repo",
                    j.repo(),
                    "--mode",
                    "finish"
                ])
                .0,
                1
            );
            let done = recover(j, "rollback");
            assert_eq!(get(&done, "outcome"), &s("rolled-back"));
            assert_eq!(get(&done, "verified"), &Value::Bool(true));
            assert_eq!(before, j.state());
            let fresh =
                j.replacement_capture(&old, &path, "correct", "contradiction", false, vec![]);
            let proposal = j.prepared(&fresh);
            let (code, done) = j.apply(&proposal);
            assert_eq!(code, 0, "{}", encode(&done));
            assert_eq!(get(&done, "governance_verified"), &Value::Bool(true));
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance_recovery.py::GovernanceRecoveryContract.test_changed_supporting_source_at_readback_cannot_commit_pruning_and_rollback_preserves_it
#[test]
#[ignore = "needs recovery_frontier, recovery_transactions registered"]
fn journey_changed_source_at_readback_cannot_commit_pruning() {
    journey(
        "journey_changed_source_at_readback_cannot_commit_pruning",
        |j| {
            let (old, path) = j.add("obsolete", DEFAULT_BODY, vec![]);
            let mut request = j.request(&old, &path, "revoke");
            verified_repository(j, &mut request);
            let proposal = j.prepared(&request);
            let before = j.state();
            let directory = j.world.root.join("rendezvous");
            std::fs::create_dir(&directory).unwrap();
            let saved = j.save(&proposal, "request.json");
            let changed = "# Source changed during readback; retain this external edit.\n";
            let repo = j.repo.clone();
            let release = directory.join("transaction-before-receipt.release");
            let ready = directory.join("transaction-before-receipt.ready");
            let watcher = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                while !ready.exists() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                assert!(ready.exists());
                std::fs::write(repo.join("evidence.md"), changed).unwrap();
                std::fs::write(release, b"").unwrap();
            });
            let output = faulted(
                j,
                &["govern", "apply", "--proposal", &saved],
                &[
                    (
                        "AGENT_MEMORY_FAULT_STAGE",
                        "transaction-before-receipt".into(),
                    ),
                    ("AGENT_MEMORY_FAULT_MODE", "pause".into()),
                    ("AGENT_MEMORY_FAULT_DIR", directory.clone().into_os_string()),
                ],
            );
            watcher.join().unwrap();
            assert_eq!(output.status, agent_memory::process::Status::Code(1));
            let stdout = decode(std::str::from_utf8(&output.stdout).unwrap().trim_end());
            assert_ne!(stdout.get("governance_verified"), Some(&Value::Bool(true)));
            assert!(j.exists(".agent-memory-transaction.md"));
            let done = recover(j, "rollback");
            assert_eq!(get(&done, "outcome"), &s("rolled-back"));
            assert_eq!(before, j.state());
            assert_eq!(
                std::fs::read_to_string(j.repo.join("evidence.md")).unwrap(),
                changed
            );
        },
    );
}

// obligation: tests/contracts/test_governance_recovery.py::GovernanceRecoveryContract.test_nested_completed_receipt_refuses_corruption_then_cleans_up_without_requiring_new_evidence
#[test]
#[ignore = "needs recovery_frontier, recovery_transactions registered; nested sharing-boundary bank fixture"]
fn journey_nested_completed_receipt_refuses_corruption_then_cleans_up() {
    journey(
        "journey_nested_completed_receipt_refuses_corruption_then_cleans_up",
        |j| {
            // The baseline uses a nested `docs/shared memory` bank from the
            // sharing-boundary fixture; this journey exercises the same receipt
            // corruption refusals on the bound fixture bank.
            std::fs::write(
                j.repo.join("evidence.md"),
                "# Explicit current fixture observation\n",
            )
            .unwrap();
            let (old, path) = j.add("obsolete", DEFAULT_BODY, vec![]);
            let mut request = j.request(&old, &path, "revoke");
            verified_repository(j, &mut request);
            let proposal = j.prepared(&request);
            let before = j.state();
            let saved = j.save(&proposal, "request.json");
            let output = faulted(
                j,
                &["govern", "apply", "--proposal", &saved],
                &[
                    (
                        "AGENT_MEMORY_FAULT_STAGE",
                        "transaction-after-receipt".into(),
                    ),
                    ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
                ],
            );
            assert_eq!(output.status, agent_memory::process::Status::Code(86));
            unlock(j);
            let mut receipts = Vec::new();
            walk(
                &j.world.root.join("state/agent-memory"),
                &j.world.root.join("state/agent-memory"),
                &mut receipts,
            );
            let matches: Vec<_> = receipts
                .into_iter()
                .filter(|(p, _)| p.ends_with(".json"))
                .filter(|(_, raw)| {
                    json::decode_bytes(raw, json::DecodePolicy::default()).is_ok_and(|v| {
                        v.get("schema") == Some(&s("agent-memory/transaction-receipt-v1"))
                            && v.get("operation_id") == proposal.get("operation_id")
                    })
                })
                .collect();
            assert_eq!(matches.len(), 1);
            let receipt_path = j.world.root.join("state/agent-memory").join(&matches[0].0);
            let original = matches[0].1.clone();
            let receipt = json::decode_bytes(&original, json::DecodePolicy::default()).unwrap();
            let recovery = j.good(&[
                "transaction",
                "recover",
                "--repo",
                j.repo(),
                "--mode",
                "finish",
            ]);
            for field in ["project_id", "changed_memory_paths", "extra"] {
                let mut corrupted = receipt.clone();
                match field {
                    "project_id" => set(&mut corrupted, field, s(&host::uuid4().unwrap())),
                    "changed_memory_paths" => {
                        set(&mut corrupted, field, list(&["other-bank/topics/x.md"]))
                    }
                    _ => set(
                        &mut corrupted,
                        field,
                        s("Unknown receipt extension must refuse."),
                    ),
                }
                std::fs::write(&receipt_path, encode(&corrupted)).unwrap();
                let current = j.state();
                let saved = j.save(&recovery, "recover.json");
                let (code, refused) = j.call(&[
                    "transaction",
                    "recover",
                    "--repo",
                    j.repo(),
                    "--proposal",
                    &saved,
                ]);
                assert_eq!(code, 1, "{}", encode(&refused));
                assert_eq!(current, j.state());
                assert!(j.exists(".agent-memory-transaction.md"));
                std::fs::write(&receipt_path, &original).unwrap();
            }
            std::fs::write(
                j.repo.join("evidence.md"),
                "# Later source, after the verified commit point\n",
            )
            .unwrap();
            let saved = j.save(&recovery, "recover.json");
            let done = j.good(&[
                "transaction",
                "recover",
                "--repo",
                j.repo(),
                "--proposal",
                &saved,
            ]);
            assert_eq!(get(&done, "outcome"), &s("finished"));
            assert_eq!(get(&done, "writes_performed"), &Value::Bool(false));
            let edited = format!(
                "{}\nLater manual orientation must remain intact.\n",
                j.read(&path)
            );
            std::fs::write(j.bank.join(&path), edited).unwrap();
            let current = j.state();
            let (code, replay) = j.apply(&proposal);
            assert_eq!(code, 0, "{}", encode(&replay));
            assert_eq!(get(&replay, "already_applied"), &Value::Bool(true));
            assert_eq!(get(&replay, "still_current"), &Value::Bool(false));
            assert_eq!(current, j.state());
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance_recovery.py::GovernanceRecoveryContract.test_explicit_deletion_erases_corrected_versions_and_their_content_bearing_history
#[test]
#[ignore = "needs recovery_frontier, comparison registered"]
fn journey_explicit_deletion_erases_corrected_versions_and_history() {
    journey(
        "journey_explicit_deletion_erases_corrected_versions_and_history",
        |j| {
            let (old, path) = j.add(
                "wrong",
                "# Retry policy\n\nFive attempts was a disproved claim.\n",
                vec![],
            );
            let correction =
                j.replacement_capture(&old, &path, "correct", "contradiction", false, vec![]);
            let proposal = j.prepared(&correction);
            let (code, done) = j.apply(&proposal);
            assert_eq!(code, 0, "{}", encode(&done));
            let (current, _) = unpack(&j.read(&path));
            let request = j.deletion(&current, &path, &[get(&old, "id")]);
            let proposal = j.prepared(&request);
            let before = j.state();
            let (code, done) = j.apply(&proposal);
            assert_eq!(code, 0, "{}", encode(&done));
            let after = j.state();
            let names: Vec<_> = after
                .0
                .iter()
                .map(|(p, _)| p.clone())
                .filter(|p| p.ends_with(".md"))
                .collect();
            let mut expected = vec![
                "MEMORY.md".to_owned(),
                "projectbrief.md".to_owned(),
                format!("history/tombstones/{}.md", text(&old, "id")),
            ];
            expected.sort();
            assert_eq!(names, expected);
            let all: String = after
                .0
                .iter()
                .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
                .collect();
            assert!(!all.contains("Five attempts"));
            assert!(!all.contains("Use at most three attempts"));
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}

// obligation: tests/contracts/test_governance_recovery.py::GovernanceRecoveryContract.test_deleting_last_nested_route_preserves_custom_index_context_before_removing_empty_maps
#[test]
#[ignore = "needs recovery_frontier registered"]
fn journey_deleting_last_nested_route_preserves_custom_index_context() {
    journey(
        "journey_deleting_last_nested_route_preserves_custom_index_context",
        |j| {
            let (metadata, path) = j.add("last-child", DEFAULT_BODY, vec![]);
            let (child, parent) = ("topics/nested/index.md", "topics/index.md");
            std::fs::create_dir_all(j.bank.join("topics/nested")).unwrap();
            std::fs::write(j.bank.join(child), index(&j.pid, child, &path)).unwrap();
            let parent_text = index(&j.pid, parent, child);
            let context = "Unrelated useful index orientation must survive.";
            std::fs::write(
                j.bank.join(parent),
                parent_text.replace("\n## Routes\n", &format!("\n{context}\n\n## Routes\n")),
            )
            .unwrap();
            let router: String = j
                .read("MEMORY.md")
                .split_inclusive('\n')
                .filter(|line| !line.starts_with(&format!("| [{path}]")))
                .collect();
            std::fs::write(
            j.bank.join("MEMORY.md"),
            router.replace("\n## Omitted contexts", &format!("| [{parent}](<{parent}>) | Nested group | repository | active | When needed |\n\n## Omitted contexts")),
        )
        .unwrap();
            let request = j.deletion(&metadata, &path, &[get(&metadata, "id")]);
            let before = j.state();
            let (code, refused) = j.preview(&request);
            assert_eq!(code, 1, "{}", encode(&refused));
            assert_eq!(before, j.state());
            std::fs::write(
                j.bank.join("projectbrief.md"),
                format!("{}\n{context}\n", j.read("projectbrief.md")),
            )
            .unwrap();
            std::fs::write(j.bank.join(parent), parent_text).unwrap();
            let request = j.deletion(&metadata, &path, &[get(&metadata, "id")]);
            let (code, plan) = j.preview(&request);
            assert_eq!(code, 0, "{}", encode(&plan));
            let (code, done) = j.apply(get(&plan, "proposal"));
            assert_eq!(code, 0, "{}", encode(&done));
            assert!(!j.exists(child));
            assert!(!j.exists(parent));
            assert!(j.read("projectbrief.md").contains(context));
            j.good(&["bank", "validate", "--repo", j.repo()]);
            let after = j.state();
            assert_eq!(
                (&before.1, &before.2, &before.3),
                (&after.1, &after.2, &after.3)
            );
        },
    );
}
