use agent_memory::{boundary_assessment, lifecycle, remote_identity, validation_memo as memo, identity_git};
use agent_memory::codec::json::{self, DecodePolicy, DuplicateKeys, Value};
use agent_memory::error::ContractError;
use agent_memory::process::{Output, Status};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, symlink};
use std::sync::atomic::{AtomicUsize, Ordering};

fn value(s: &str) -> Value { json::decode(s, DecodePolicy { duplicate_keys: DuplicateKeys::Reject, ..DecodePolicy::default() }).unwrap() }
fn s(s: &str) -> Value { Value::String(s.into()) }
fn field<'a>(v: &'a Value, key: &str) -> &'a Value { v.get(key).unwrap() }
fn set(v: &mut Value, key: &str, new: Value) {
    if let Some(old) = v.get_mut(key) { *old = new; }
    else if let Value::Object(pairs) = v { pairs.push((key.into(), new)); }
    else { panic!("not an object"); }
}
fn remove(v: &mut Value, key: &str) { if let Value::Object(pairs) = v { pairs.retain(|(k, _)| k != key); } }
fn selection() -> (Value, Value) {
    (value(r#"{"status":"active","trust":"reviewed","visibility":"visible","confidence":0.2,"valid-from":"2026-01-01T00:00:00Z","valid-until":"2027-01-01T00:00:00Z"}"#),
    value(r#"{"authorized-scope":true,"applicable":true,"now":"2026-10-22T17:00:00Z","minimum-trust":"reviewed","active-superseders":[]}"#))
}
const RECORD: &str = "c7063a92-f252-4d83-a728-3355018aca81";
const OLD: &str = "737a69f9-e8d9-49d0-88c2-dda9942267a4";
const NEW: &str = "441b9eaa-72a2-45fb-bbf2-57297d7228f4";
fn event(status: &str) -> (Value, Value) {
    let mut record = value(r#"{"id":"c7063a92-f252-4d83-a728-3355018aca81","version":1,"event-id":"737a69f9-e8d9-49d0-88c2-dda9942267a4","event-ids":["737a69f9-e8d9-49d0-88c2-dda9942267a4"],"evidence":["git:old-commit"],"status":"active","valid-until":"2026-10-22T17:00:00Z"}"#);
    set(&mut record, "status", s(status));
    let event = value(r#"{"id":"441b9eaa-72a2-45fb-bbf2-57297d7228f4","evidence":["git:new-commit"],"authority":"developer:operator","reason":"Stronger evidence","reviewed":true,"now":"2026-10-22T17:00:00Z","replacement":{"id":"c7063a92-f252-4d83-a728-3355018aca81","version":2,"status":"active","replaces":{"id":"c7063a92-f252-4d83-a728-3355018aca81","version":1},"applicability-preserved":true}}"#);
    (record, event)
}
fn is_current(record: &Value, context: &Value) -> bool { field(&lifecycle::current(record, context).unwrap(), "current").as_bool().unwrap() }
fn legal(record: &Value, operation: &str, event: &Value) -> bool { field(&lifecycle::transition(record, operation, event).unwrap(), "legal").as_bool().unwrap() }

// obligation: tests/contracts/test_boundary_assessment.py::BoundaryAssessmentContract.test_duplicate_needs_explicit_read_evidence_and_rationale
#[test]
fn duplicate_needs_explicit_read_evidence_and_rationale() {
    let assessment = value(r#"{"source_id":"turn","source_review":"Read the original turn and compared current memory","candidates":[{"claim":"Production retries are three","disposition":"already-recorded","reason":"The existing record covers the same production condition","memory_evidence":"memory-bank/topics/retries.md#production"}]}"#);
    assert_eq!(boundary_assessment::validate(&assessment, "turn").unwrap(), &assessment);
    for (key, new) in [("memory_evidence", None), ("memory_evidence", Some(s(""))),
        ("disposition", Some(s("eligible"))), ("disposition", Some(s("ambiguous")))] {
        let mut changed = assessment.clone();
        let Value::Array(candidates) = changed.get_mut("candidates").unwrap() else { panic!() };
        if let Some(new) = new { set(&mut candidates[0], key, new); } else { remove(&mut candidates[0], key); }
        assert_eq!(boundary_assessment::validate(&changed, "turn").unwrap_err().to_string(), "Eligible, ambiguous or unresolved learning remains pending");
    }
    assert!(boundary_assessment::validate(&assessment, "another-turn").is_err());
    let mut empty = assessment.clone(); set(&mut empty, "candidates", value("[]"));
    assert!(boundary_assessment::validate(&empty, "turn").is_ok());
    set(&mut empty, "source_review", s(&"é".repeat(1025)));
    assert_eq!(boundary_assessment::validate(&empty, "turn").unwrap_err().to_string(), "Use a bounded source-bound no-learning assessment");
    let mut bounded = assessment.clone(); set(&mut bounded, "source_review", s(&"a".repeat(1900)));
    assert_eq!(boundary_assessment::validate(&bounded, "turn").unwrap_err().to_string(), "No-learning assessment exceeds its bound");
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_generated_guide_definition_and_router_share_one_model
#[test]
fn generated_guide_definition_and_router_share_one_model() {
    assert_eq!(lifecycle::guide().as_bytes(), include_bytes!("../skills/agent-memory/model/GUIDE.md"));
    let model = lifecycle::definition();
    assert_eq!(field(&model, "states"), &value(r#"["candidate","quarantined","active","superseded","revoked","expired","deleted"]"#));
    let guide = lifecycle::guide();
    for state in field(&model, "states").as_array().unwrap() { assert!(guide.contains(&format!("`{}`", state.as_str().unwrap()))); }
    for rule in field(&model, "current-predicate").as_array().unwrap() { assert!(guide.contains(&format!("`{}`", field(rule, "gate").as_str().unwrap()))); }
    let router = value(include_str!("../skills/agent-memory/schemas/bank-router.json"));
    assert_eq!(field(&router, "x-lifecycle-model"), &s("lifecycle.json"));
    assert!(router.get("x-lifecycle-states").is_none());
    assert_eq!(lifecycle::command("guide", None).stdout.as_bytes(), include_bytes!("../skills/agent-memory/model/GUIDE.md"));
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_current_requires_every_gate_before_ranking
#[test]
fn current_requires_every_gate_before_ranking() {
    let (record, context) = selection(); assert!(is_current(&record, &context));
    for (area, key, new, reason) in [
        (false, "authorized-scope", Value::Bool(false), "authorized-scope"),
        (false, "applicable", Value::Bool(false), "applicable"),
        (true, "status", s("revoked"), "active"),
        (true, "trust", s("untrusted"), "sufficient-trust"),
        (true, "visibility", s("hidden"), "visible"),
        (false, "active-superseders", value(r#"["new-record/version-2"]"#), "no-active-superseder"),
        (true, "valid-until", s("2026-10-22T17:00:00Z"), "valid-time"),
    ] {
        let (mut r, mut c) = (record.clone(), context.clone());
        set(if area { &mut r } else { &mut c }, key, new);
        let decision = lifecycle::current(&r, &c).unwrap();
        assert_eq!(field(&decision, "current"), &Value::Bool(false));
        assert!(field(&decision, "reasons").as_array().unwrap().contains(&s(reason)));
    }
    let mut r = record; set(&mut r, "confidence", Value::Float(1.0)); set(&mut r, "trust", s("untrusted"));
    assert!(!is_current(&r, &context)); set(&mut r, "confidence", Value::Float(0.0)); set(&mut r, "trust", s("verified"));
    assert!(is_current(&r, &context));
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_time_is_start_inclusive_end_exclusive_with_offsets
#[test]
fn time_is_start_inclusive_end_exclusive_with_offsets() {
    let (mut record, mut context) = selection();
    set(&mut record, "valid-from", s("2026-10-22T10:00:00-07:00")); set(&mut record, "valid-until", s("2026-10-22T17:00:01Z"));
    assert!(is_current(&record, &context)); set(&mut context, "now", s("2026-10-22T17:00:01Z")); assert!(!is_current(&record, &context));
    set(&mut record, "valid-from", Value::Null); set(&mut record, "valid-until", Value::Null); assert!(is_current(&record, &context));
    assert_eq!(lifecycle::time_value(&s("1970-01-01T00:00:00Z")).unwrap(), (0, "".into()));
    assert_eq!(lifecycle::time_value(&s("1969-12-31T23:59:59.12000Z")).unwrap(), (-1, "12".into()));
    assert_eq!(lifecycle::time_value(&s("2000-02-29T00:00:00+23:59")).unwrap(), lifecycle::time_value(&s("2000-02-28T00:01:00Z")).unwrap());
    for bad in ["0000-01-01T00:00:00Z", "1900-02-29T00:00:00Z", "2026-04-31T00:00:00Z", "2026-01-01T24:00:00Z", "2026-01-01T23:59:60Z", "2026-13-01T00:00:00Z"] {
        assert_eq!(lifecycle::time_value(&s(bad)).unwrap_err().to_string(), "Invalid RFC3339 time");
    }
    for bad in ["2026-01-01t00:00:00Z", "2026-01-01T00:00:00z", "2026-01-01T00:00:00+24:00", "2026-01-01T00:00:00+00:60", "2026-01-01T00:00:00.١Z", "2026-01-01T00:00:00.Z"] {
        assert_eq!(lifecycle::time_value(&s(bad)).unwrap_err().to_string(), "Time must be timezone-qualified RFC3339 with valid offset components");
    }
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_unknown_or_malformed_axes_do_not_become_current
#[test]
fn unknown_or_malformed_axes_do_not_become_current() {
    for (area, key, new) in [
        (true, "status", s("invented")), (true, "trust", s("certain")), (true, "visibility", s("personal")),
        (true, "confidence", Value::Bool(true)), (true, "confidence", Value::Float(1.1)),
        (true, "confidence", value(&format!("1{}", "0".repeat(400)))),
        (false, "authorized-scope", s("yes")), (false, "applicable", Value::Null), (false, "minimum-trust", Value::Null),
        (false, "now", s("2026-10-22T17:00:00+00:99")), (false, "now", s("2026-10-22T17:00:00.١Z")),
        (false, "active-superseders", Value::Null), (true, "valid-until", s("2025-01-01T00:00:00Z")),
    ] {
        let (mut r, mut c) = selection(); set(if area { &mut r } else { &mut c }, key, new);
        assert!(lifecycle::current(&r, &c).is_err());
    }
    assert_eq!(lifecycle::references(&value(r#"["x","x"]"#)).unwrap_err().to_string(), "Duplicate references");
    assert!(lifecycle::references(&value(r#"["\u001c"]"#)).is_err());
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_fractional_boundaries_preserve_every_accepted_digit
#[test]
fn fractional_boundaries_preserve_every_accepted_digit() {
    let (mut r, mut c) = selection(); set(&mut r, "valid-from", s("2026-10-22T17:00:00.0000001Z")); set(&mut r, "valid-until", Value::Null);
    set(&mut c, "now", s("2026-10-22T17:00:00.0000000Z")); assert!(!is_current(&r, &c));
    set(&mut r, "valid-from", Value::Null); set(&mut r, "valid-until", s("2026-10-22T17:00:00.0000009Z"));
    set(&mut c, "now", s("2026-10-22T17:00:00.0000001Z")); assert!(is_current(&r, &c));
    set(&mut c, "now", s("2026-10-22T17:00:00.000000900000Z")); assert!(!is_current(&r, &c));
    let fraction = format!("{}1", "0".repeat(50));
    set(&mut r, "valid-from", s(&format!("2026-10-22T10:00:00.{fraction}-07:00"))); set(&mut r, "valid-until", Value::Null);
    set(&mut c, "now", s("2026-10-22T17:00:00Z")); assert!(!is_current(&r, &c));
    set(&mut c, "now", s(&format!("2026-10-22T17:00:00.{fraction}00Z"))); assert!(is_current(&r, &c));
    let (mut r, mut e) = event("active"); set(&mut r, "valid-until", s("2026-10-22T17:00:00.0000009Z"));
    set(&mut e, "now", s("2026-10-22T17:00:00.0000001Z")); assert!(!legal(&r, "expire", &e));
    set(&mut e, "now", s("2026-10-22T10:00:00.000000900-07:00")); assert!(legal(&r, "expire", &e));
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_transition_table_drives_all_legal_states_and_deletion_is_terminal
#[test]
fn transition_table_drives_all_legal_states_and_deletion_is_terminal() {
    for rule in field(&lifecycle::definition(), "transitions").as_array().unwrap() {
        let operation = field(rule, "operation").as_str().unwrap();
        for status in field(rule, "from").as_array().unwrap() {
            let (r, e) = event(status.as_str().unwrap()); let decision = lifecycle::transition(&r, operation, &e).unwrap();
            assert_eq!(field(&decision, "legal"), &Value::Bool(true)); assert_eq!(field(&decision, "to"), field(rule, "to"));
        }
        let (r, e) = event("deleted"); let decision = lifecycle::transition(&r, operation, &e).unwrap();
        assert_eq!(field(&decision, "legal"), &Value::Bool(false)); assert_eq!(field(&decision, "reasons"), &value(r#"["terminal-deletion"]"#));
    }
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_correction_and_supersession_require_exact_active_replacement
#[test]
fn correction_and_supersession_require_exact_active_replacement() {
    let (r, e) = event("active"); let decision = lifecycle::transition(&r, "correct", &e).unwrap();
    assert_eq!(field(&decision, "creates_new_version"), &Value::Bool(true)); assert_eq!(field(&decision, "prior_version_status"), &s("superseded"));
    assert_eq!(field(&decision, "new_version_status"), &s("active"));
    for (key, new) in [("status", s("candidate")), ("version", value("1")),
        ("replaces", value(&format!(r#"{{"id":"{RECORD}","version":9}}"#))), ("applicability-preserved", Value::Bool(false))] {
        let mut bad = e.clone(); set(bad.get_mut("replacement").unwrap(), key, new); assert!(!legal(&r, "correct", &bad));
    }
    let mut e = e; set(e.get_mut("replacement").unwrap(), "id", s(NEW)); assert!(!legal(&r, "correct", &e)); assert!(legal(&r, "supersede", &e));
    // Python dict equality treats 1.0 and True as equal to version 1 inside replaces.
    for n in [value("1.0"), Value::Bool(true)] {
        let (r, mut e) = event("active"); set(e.get_mut("replacement").unwrap().get_mut("replaces").unwrap(), "version", n);
        assert!(legal(&r, "correct", &e));
    }
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_revocation_expiry_and_reactivation_remain_distinct
#[test]
fn revocation_expiry_and_reactivation_remain_distinct() {
    let (r, mut e) = event("active"); remove(&mut e, "replacement"); set(&mut e, "evidence", value("[]"));
    assert_eq!(field(&lifecycle::transition(&r, "revoke", &e).unwrap(), "to"), &s("revoked"));
    set(&mut e, "now", s("2026-10-22T16:59:59Z")); assert!(!legal(&r, "expire", &e));
    set(&mut e, "now", s("2026-10-22T17:00:00Z")); assert!(legal(&r, "expire", &e));
    for status in ["superseded", "revoked", "expired"] {
        let (r, mut e) = event(status); let decision = lifecycle::transition(&r, "reactivate", &e).unwrap();
        assert_eq!(field(&decision, "legal"), &Value::Bool(true)); assert_eq!(field(&decision, "prior_version_status"), &s(status));
        assert_eq!(field(&decision, "new_version_status"), &s("active"));
        set(&mut e, "evidence", field(&r, "evidence").clone()); assert!(!legal(&r, "reactivate", &e));
        let (_, mut e) = event(status); set(&mut e, "id", s(OLD)); assert!(!legal(&r, "reactivate", &e));
    }
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_each_event_guard_refuses_missing_authority_review_or_new_event
#[test]
fn each_event_guard_refuses_missing_authority_review_or_new_event() {
    for (op, key) in [("correct", "authority"), ("revoke", "reason"), ("admit", "reviewed"), ("delete", "id")] {
        let (r, mut e) = event(if op == "admit" { "candidate" } else { "active" }); remove(&mut e, key); assert!(!legal(&r, op, &e));
    }
    let (mut r, e) = event("active"); set(&mut r, "event-ids", value(&format!(r#"["{OLD}","{NEW}"]"#)));
    assert!(field(&lifecycle::transition(&r, "correct", &e).unwrap(), "reasons").as_array().unwrap().contains(&s("new-event")));
    assert!(lifecycle::identity(&s(RECORD)));
    for bad in [RECORD.to_uppercase(), RECORD.replace('-', ""), format!("{{{RECORD}}}"), format!("urn:uuid:{RECORD}"), RECORD.replacen("4d83", "1d83", 1), RECORD.replacen("a728", "c728", 1)] {
        assert!(!lifecycle::identity(&s(&bad)));
    }
    set(&mut r, "version", Value::Bool(true)); assert!(lifecycle::transition(&r, "correct", &e).is_err());
}

// obligation: tests/contracts/test_lifecycle_model.py::LifecycleContract.test_copied_portable_evaluator_does_not_modify_inputs_or_create_state
// Pure evaluator/input immutability subset; installed/copy-pack CLI part is deferred.
#[test]
fn evaluator_is_read_only_and_emits_exact_mode_receipts() {
    let (record, context) = selection(); let before = (record.clone(), context.clone());
    let request = Value::Object(vec![("record".into(), record.clone()), ("context".into(), context.clone())]);
    let result = lifecycle::command("current", Some(&request)); assert_eq!(result.exit_code, 0);
    assert_eq!(field(result.value.as_ref().unwrap(), "writes_performed"), &Value::Bool(false)); assert_eq!((record, context), before);
    let missing = lifecycle::command("current", None); assert_eq!(missing.exit_code, 1);
    assert_eq!(missing.stdout, "{\"schema\": \"agent-memory/lifecycle-result-v1\", \"operation\": \"current\", \"error\": \"--input is required\", \"writes_performed\": false}\n");
    assert!(missing.stderr.is_empty());
}

#[test]
fn validation_memo_digests_are_baseline_bytes_not_json_hashes() {
    // Independently derived from SHA256 of UTF-8 'abc' and the baseline's
    // eight-byte big-endian framing: len(a),a,len(x),x,len(b),b,len(é),é.
    let key = memo::text_key(&s("abc"), &value("16384")).unwrap();
    assert_eq!(memo::hex(&key.digest), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!((key.type_name, key.representation.as_str()), ("int", "16384"));
    let files = value(r#"{"b":"é","a":"x"}"#); let (digest, captured) = memo::map_input(&files).unwrap();
    assert_eq!(captured, files); assert_eq!(memo::hex(&digest), "8b7c41f65ac8e76ada5f2aa31fa985330ad4a576497f7df0b954df1ba5a0a265");
    assert_eq!(memo::map_input(&value(r#"{"a":"x","b":"é"}"#)).unwrap().0, digest);
    assert_eq!(memo::hex(&memo::map_input(&value("{}")).unwrap().0), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert!(memo::text_key(&s("abc"), &Value::Bool(true)).is_none()); assert!(memo::text_key(&s("abc"), &value("16384.0")).is_none());
    assert!(memo::text_key(&s(&"é".repeat(32769)), &value("65536")).is_none());
    assert!(memo::map_input(&Value::Object(vec![("x".into(), s(&"é".repeat(8193)))])).is_none());
}

// obligation: tests/contracts/test_validation_memo.py::ValidationMemoContract.test_changed_child_bytes_paths_and_returned_values_do_not_reuse_old_truth
// Memo-level isolation/key subset; bank integration is deferred.
#[test]
fn memo_keys_and_returned_values_do_not_reuse_old_truth() {
    memo::operation(|_| {
        let files = value(r#"{"a":"old"}"#); let key = memo::map_input(&files).unwrap().0;
        let mut first = memo::checked("bank-format", &key, || Ok(value(r#"{"entries":["retained"]}"#)), None).unwrap();
        set(&mut first, "entries", value("[]"));
        assert_eq!(memo::checked("bank-format", &key, || panic!("cache miss"), None).unwrap(), value(r#"{"entries":["retained"]}"#));
        let changed = memo::map_input(&value(r#"{"a":"new"}"#)).unwrap().0;
        let renamed = memo::map_input(&value(r#"{"b":"old"}"#)).unwrap().0;
        assert_ne!(key, changed); assert_ne!(key, renamed);
        for key in [changed, renamed] {
            assert!(memo::checked("bank-format", &key, || Err(ContractError::new("failure")), None).is_err());
            assert_eq!(memo::checked("bank-format", &key, || Ok(s("fresh")), None).unwrap(), s("fresh"));
        }
    }); assert!(memo::current().is_none());
}

// obligation: tests/contracts/test_validation_memo.py::ValidationMemoContract.test_identical_file_authority_text_rechecks_changed_filesystem_topology
// Fresh-guard mechanism subset; actual bank/symlink topology integration is deferred.
#[test]
fn memo_fresh_guard_runs_before_accepting_hits_and_failures_are_not_cached() {
    memo::operation(|_| {
        memo::checked("format", b"key", || Ok(s("original")), None).unwrap();
        let mut guard = || Err(ContractError::new("changed topology"));
        assert_eq!(memo::checked("format", b"key", || panic!("must not produce"), Some(&mut guard)).unwrap_err().to_string(), "changed topology");
        assert_eq!(memo::checked("format", b"key", || panic!("must stay cached"), None).unwrap(), s("original"));
    });
}

// obligation: tests/contracts/test_validation_memo.py::ValidationMemoContract.test_exceptions_nesting_and_budget_fallback_do_not_leak_validation_state
#[test]
fn memo_exceptions_nesting_and_suspension_restore_exact_context() {
    let unwind = std::panic::catch_unwind(|| memo::operation(|outer| {
        memo::operation(|inner| assert!(std::rc::Rc::ptr_eq(&outer, &inner)));
        memo::suspended(|| { assert!(memo::current().is_none()); memo::operation(|inner| assert!(!std::rc::Rc::ptr_eq(&outer, &inner))); });
        assert!(std::rc::Rc::ptr_eq(&outer, &memo::current().unwrap()));
        outer.borrow_mut().limits.bytes = 0;
        assert_eq!(memo::checked("format", b"x", || Ok(s("uncached")), None).unwrap(), s("uncached"));
        assert!(outer.borrow().is_empty()); panic!("fixture unwind");
    }));
    assert!(unwind.is_err()); assert!(memo::current().is_none());
    memo::operation(|fresh| assert!(fresh.borrow().is_empty()));
    let unwind = std::panic::catch_unwind(|| memo::operation(|_| memo::suspended(|| panic!("suspension unwind"))));
    assert!(unwind.is_err()); assert!(memo::current().is_none());
}

// obligation: tests/contracts/test_validation_memo.py::ValidationMemoContract.test_small_retained_budgets_evict_and_oversized_results_fall_back_without_truncation
#[test]
fn memo_small_budgets_evict_lru_without_truncating_large_results() {
    let mut active = memo::Memo::new(); active.limits = memo::Limits { entries: 2, maps: 1, bytes: 65536 };
    for key in [b"one", b"two", b"tri"] { active.checked("bank-format", key, || Ok(s("map")), None).unwrap(); assert_eq!(active.maps, 1); }
    for key in [b"one", b"two", b"tri"] { active.checked("scan", key, || Ok(s("text")), None).unwrap(); assert!(active.len() <= 2); }
    active.limits.bytes = 500;
    for key in [b"one", b"two"] { active.checked("scan", key, || Ok(s("small")), None).unwrap(); assert_eq!(active.len(), 1); assert!(active.bytes <= 500); }
    let before = (active.len(), active.bytes); let large = "complete source ".repeat(100);
    assert_eq!(active.checked("scan", b"large", || Ok(s(&large)), None).unwrap(), s(&large)); assert_eq!((active.len(), active.bytes), before);
    let mut lru = memo::Memo::new(); lru.limits.entries = 2;
    lru.checked("scan", b"a", || Ok(s("a")), None).unwrap(); lru.checked("scan", b"b", || Ok(s("b")), None).unwrap();
    lru.checked("scan", b"a", || panic!(), None).unwrap(); lru.checked("scan", b"c", || Ok(s("c")), None).unwrap();
    assert_eq!(lru.checked("scan", b"b", || Ok(s("new b")), None).unwrap(), s("new b"));
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = PathBuf::from(format!("/tmp/agent-memory-t1-a1-{}-{}-{}", std::process::id(), agent_memory::clock::monotonic_ns(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::DirBuilder::new().mode(0o700).create(&path).unwrap(); Self(std::fs::canonicalize(path).unwrap())
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_normalization_strips_secrets_and_unifies_only_documented_equivalence
#[test]
fn normalization_strips_secrets_and_unifies_only_documented_equivalence() {
    let expected = "repo://github.com/owner/repo";
    for remote in ["https://alice:secret@GITHUB.COM/Owner/Repo.git/?token=secret#secret", "git@github.com:Owner/Repo.git",
        "ssh://git@GITHUB.com:22/OWNER/REPO.git/", "https://github.com:443/owner/repo", "http://github.com:80/owner/repo.git",
        expected, "https://github.com/%6fwner/repo.git"] { assert_eq!(remote_identity::normalize(&s(remote), None).unwrap(), expected); }
    let distinct: Vec<_> = ["https://other.example/owner/repo.git", "https://github.com/another/repo.git", "https://github.com/owner/fork.git",
        "ssh://git@github.com:2222/owner/repo.git", "https://Example.com/Owner/Repo.git", "https://example.com/owner/repo.git", expected]
        .map(|remote| remote_identity::normalize(&s(remote), None).unwrap()).into();
    let unique: std::collections::HashSet<_> = distinct.iter().collect(); assert_eq!(unique.len(), distinct.len());
    for remote in ["https://secret@host/owner/%2Frepo", "ext::secret", "https://secret@host/a/../b", "ssh://secret@host:wrong/repo"] {
        let error = remote_identity::normalize(&s(remote), None).unwrap_err().to_string(); assert!(!error.contains("secret"));
    }
    for (input, output) in [("ssh://user@[2001:DB8::1]:22/Repo.git", "repo://[2001:db8::1]/Repo"),
        ("repo://host:00000/repo", "repo://host:0/repo"), ("https://host/r%20epo", "repo://host/r%20epo"),
        ("https://host/r%7fepo", "repo://host/r%7Fepo"), ("https://éxample.com/Repo.GIT", "repo://éxample.com/Repo.GIT")] {
        assert_eq!(remote_identity::normalize(&s(input), None).unwrap(), output);
    }
    for input in ["https://host/r%0aepo", "https://host/r%", "https://host/r%5cepo", "https://host/re po", "http://[127.0.0.1]/repo",
        "http://[bad]/repo", "http://[::1]x/repo", "http://host:65536/repo", "http://host:١/repo"] { assert!(remote_identity::normalize(&s(input), None).is_err(), "{input}"); }
}

#[test]
fn remote_local_paths_resolve_existing_prefix_and_preserve_missing_remainder() {
    let temp = Temp::new(); let real = temp.0.join("real"); std::fs::create_dir(&real).unwrap();
    symlink(&real, temp.0.join("alias")).unwrap();
    let expected = format!("file://{}/real/missing/repo", temp.0.display());
    assert_eq!(remote_identity::normalize(&s("alias/missing/./part/../repo.git"), Some(&temp.0)).unwrap(), expected);
    let url = format!("file://localhost{}/alias/missing/repo.git?ignored#ignored", temp.0.display());
    assert_eq!(remote_identity::normalize(&s(&url), None).unwrap(), expected);
    assert!(remote_identity::normalize(&s("file://remote/repo"), None).is_err());
    assert!(remote_identity::normalize(&s("file:///"), None).is_err());
    symlink("cycle", temp.0.join("cycle")).unwrap();
    assert!(remote_identity::normalize(&s("cycle/repo"), Some(&temp.0)).is_err());
}

#[test]
fn netloc_nfkc_table_matches_baseline_vectors() {
    let vectors = value(include_str!("vectors/nfkc-netloc-delimiters-v1.json"));
    let codepoints: Vec<u32> = field(&vectors, "codepoints").as_array().unwrap().iter()
        .map(|v| field(v, "cp").as_int().unwrap().as_u64().unwrap() as u32).collect();
    assert_eq!(remote_identity::NFKC_NETLOC_DELIMITERS.as_slice(), codepoints.as_slice());
    for cp in codepoints {
        let c = char::from_u32(cp).unwrap();
        let error = remote_identity::normalize(&s(&format!("https://user@a{c}b/repo")), None).unwrap_err();
        assert_eq!(error.to_string(), "Unsupported or ambiguous repository remote; choose an explicit canonical binding");
    }
    assert_eq!(remote_identity::normalize(&s("https://user:password@K.example/repo"), None).unwrap(), "repo://k.example/repo");
}

#[test]
fn git_invocation_retains_only_exact_hardened_environment() {
    let input = [("PATH", "/usr/bin:/bin"), ("HOME", "/private/home"), ("GIT_DIR", "/hostile"), ("GIT_TRACE", "1"),
        ("GIT_CONFIG_COUNT", "4"), ("git_lowercase", "retained"), ("LANG", "C.UTF-8")].map(|(k, v)| (k.into(), v.into()));
    let invocation = identity_git::invocation_with_env(Path::new("relative repo"), &["status".into()], false, input);
    assert_eq!(invocation.argv, ["git", "--no-pager", "--no-replace-objects", "--literal-pathspecs", "-c", "core.fsmonitor=false",
        "-c", "core.untrackedCache=false", "-C", "relative repo", "status"].map(OsString::from));
    let expected = [("PATH", "/usr/bin:/bin"), ("HOME", "/private/home"), ("git_lowercase", "retained"), ("LANG", "C.UTF-8"),
        ("GIT_NO_LAZY_FETCH", "1"), ("GIT_ALLOW_PROTOCOL", ""), ("GIT_OPTIONAL_LOCKS", "0"), ("GIT_TRACE", "0"),
        ("GIT_TRACE2", "0"), ("GIT_TRACE2_EVENT", "0"), ("GIT_TRACE2_PERF", "0")].map(|(k, v)| (OsString::from(k), OsString::from(v)));
    assert_eq!(invocation.env, expected);
    let glob = identity_git::invocation_with_env(Path::new("."), &[], true, []);
    assert_eq!(glob.argv[3], "--glob-pathspecs");
}
fn output(code: Status, stdout: &[u8]) -> Output {
    Output { pid: 0, status: code, stdout: stdout.into(), stderr: Vec::new(), elapsed_ns: 0, launch_elapsed_ns: 0, poll_wakeups: 0 }
}

#[test]
fn git_output_preserves_os_bytes_and_classifies_only_ordinary_absence_as_optional() {
    assert_eq!(identity_git::_git_output(Ok(output(Status::Code(0), b" space\xff\n\n")), false).unwrap().unwrap().as_bytes(), b" space\xff\n");
    for status in [Status::Code(1), Status::Signal(15)] {
        assert!(identity_git::_git_output(Ok(output(status, b"not accepted")), true).unwrap().is_none());
        assert_eq!(identity_git::_git_output(Ok(output(status, b"")), false).unwrap_err().to_string(), "Cannot inspect repository evidence with Git; no identity change performed");
    }
    for (reason, kind, optional_absence) in [(agent_memory::process::Refusal::Spawn, std::io::ErrorKind::NotFound, true),
        (agent_memory::process::Refusal::Deadline, std::io::ErrorKind::TimedOut, true),
        (agent_memory::process::Refusal::Spawn, std::io::ErrorKind::PermissionDenied, false),
        (agent_memory::process::Refusal::Infrastructure, std::io::ErrorKind::Other, false)] {
        let source = if kind == std::io::ErrorKind::NotFound { std::io::Error::from_raw_os_error(libc::ENOENT) }
            else { std::io::Error::from(kind) };
        let error = agent_memory::process::ProcessError { reason, source: Some(source), evidence: None, cleanup_error: None, incomplete: false };
        let result = identity_git::_git_output(Err(error), true);
        if optional_absence { assert!(result.unwrap().is_none()); } else { assert!(matches!(result, Err(identity_git::Error::Infrastructure(_)))); }
    }
    let error = agent_memory::process::ProcessError { reason: agent_memory::process::Refusal::Deadline, source: None, evidence: None,
        cleanup_error: Some(std::io::Error::other("unsafe cleanup")), incomplete: false };
    assert!(matches!(identity_git::_git_output(Err(error), true), Err(identity_git::Error::Infrastructure(_))));
}

#[test]
fn root_head_parser_preserves_paths_and_refuses_incomplete_success() {
    for length in [40, 64] {
        let root = b"/tmp/embedded\nroot\n\n"; let hash = "a".repeat(length);
        let mut bytes = root.to_vec(); bytes.push(b'\n'); bytes.extend_from_slice(hash.as_bytes()); bytes.push(b'\n');
        let parsed = identity_git::root_head_output(&output(Status::Code(0), &bytes)).unwrap();
        assert_eq!(parsed.0.unwrap().as_bytes(), root); assert_eq!(parsed.1.unwrap(), hash);
    }
    for bytes in [b"".as_slice(), b"/tmp/root\n", b"/tmp/root\nHEAD\n", b"/tmp/root\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", b"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"] {
        assert_eq!(identity_git::root_head_output(&output(Status::Code(0), bytes)).unwrap_err().to_string(), "Repository root/HEAD observation is incomplete; do not infer absence");
    }
    assert_eq!(identity_git::root_head_output(&output(Status::Code(1), b"/tmp/root\n\n")).unwrap().0.unwrap().as_bytes(), b"/tmp/root\n");
    assert!(identity_git::root_head_output(&output(Status::Signal(9), b"/tmp/root\n")).unwrap().0.is_none());
}

fn fixture_git(repo: &Path, home: &Path, args: &[&str]) -> Output {
    let env = std::env::vars_os().filter(|(k, _)| k != "HOME" && k != "XDG_CONFIG_HOME")
        .chain([(OsString::from("HOME"), home.as_os_str().to_owned()), (OsString::from("XDG_CONFIG_HOME"), home.as_os_str().to_owned())]);
    let invocation = identity_git::invocation_with_env(repo, &args.iter().map(|arg| OsString::from(*arg)).collect::<Vec<_>>(), false, env);
    let result = identity_git::run_invocation(&invocation, agent_memory::clock::Deadline::from_seconds(10.0).unwrap()).unwrap();
    assert_eq!(result.status, Status::Code(0), "{}", String::from_utf8_lossy(&result.stderr)); result
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_root_spellings_and_linked_worktrees_preserve_public_identity
// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unborn_root_preserves_local_settings_and_indexed_bank_identity
// Root/HEAD observation subsets; downstream scope/settings/bank identity integration is deferred.
#[test]
fn real_git_private_repositories_preserve_unborn_committed_and_linked_root_bytes() {
    let temp = Temp::new(); let home = temp.0.join("home"); std::fs::create_dir(&home).unwrap();
    for name in ["normal root", "embedded\nnewline root", "trailing LF root\n\n"] {
        let repo = temp.0.join(name); std::fs::create_dir(&repo).unwrap();
        fixture_git(&repo, &home, &["init", "--initial-branch=main"]);
        let observed = identity_git::root_head(&repo).unwrap(); assert_eq!(observed.0, Some(repo.as_os_str().to_owned())); assert_eq!(observed.1, None);
        fixture_git(&repo, &home, &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null", "commit", "--allow-empty", "-m", "fixture"]);
        let observed = identity_git::root_head(&repo).unwrap(); assert_eq!(observed.0, Some(repo.as_os_str().to_owned())); assert_eq!(observed.1.as_ref().unwrap().len(), 40);
        let linked = temp.0.join(format!("{name} linked\n"));
        fixture_git(&repo, &home, &["-c", "core.hooksPath=/dev/null", "worktree", "add", "--detach", linked.to_str().unwrap(), "HEAD"]);
        let linked_observed = identity_git::root_head(&linked).unwrap();
        assert_eq!(linked_observed.0, Some(linked.into_os_string())); assert_eq!(linked_observed.1, observed.1);
        assert_eq!(identity_git::git(&repo, &["rev-parse".into(), "--verify".into(), "HEAD".into()], false, false).unwrap().unwrap().as_bytes(), observed.1.unwrap().as_bytes());
    }
    assert_eq!(identity_git::root_head(&home).unwrap(), (None, None));
    let repo = temp.0.join("sha256 repository\n"); std::fs::create_dir(&repo).unwrap();
    fixture_git(&repo, &home, &["init", "--initial-branch=main", "--object-format=sha256"]);
    fixture_git(&repo, &home, &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null", "commit", "--allow-empty", "-m", "fixture"]);
    assert_eq!(identity_git::root_head(&repo).unwrap().1.unwrap().len(), 64);
}

#[test]
fn security_source_classes_have_one_baseline_definition() {
    assert_eq!(agent_memory::model_reference::CLASSES,
        &["assistant", "external", "tool", "user", "verified-repository"]);
}

fn query(args: &[&str]) -> (Vec<OsString>, bool) {
    (args.iter().map(|arg| OsString::from(*arg)).collect(), false)
}
fn git_environment(home: &Path) -> Vec<(OsString, OsString)> {
    std::env::vars_os().filter(|(key, _)| key != "HOME" && key != "XDG_CONFIG_HOME")
        .chain([(OsString::from("HOME"), home.as_os_str().to_owned()),
            (OsString::from("XDG_CONFIG_HOME"), home.as_os_str().to_owned())]).collect()
}

// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_command_failures_are_consumed_in_original_order_at_every_position
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_optional_command_absence_does_not_hide_later_required_error
#[test]
fn real_git_group_consumes_failures_in_original_order_and_optional_cannot_hide_later_error() {
    let temp = Temp::new(); fixture_git(&temp.0, &temp.0, &["init", "--initial-branch=main"]);
    let environment = git_environment(&temp.0);
    for position in 0..3 {
        let mut specs = vec![query(&["rev-parse", "--show-toplevel"]); 3];
        specs[position] = query(&["rev-parse", "--verify", "does-not-exist"]);
        let result = identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
            for index in 0..position {
                assert_eq!(group.value(index, false)?.unwrap(), temp.0.as_os_str());
            }
            group.value(position, false)?;
            Ok(())
        });
        assert_eq!(result.unwrap_err().to_string(), "Cannot inspect repository evidence with Git; no identity change performed");
    }
    let specs = vec![query(&["rev-parse", "--verify", "missing-first"]), query(&["rev-parse", "--verify", "missing-second"])];
    let result = identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
        assert!(group.value(0, true)?.is_none()); group.value(1, false)?; Ok(())
    });
    assert_eq!(result.unwrap_err().to_string(), "Cannot inspect repository evidence with Git; no identity change performed");
    let complete = identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
        assert!(group.value(0, true)?.is_none()); assert!(group.value(1, true)?.is_none()); Ok(())
    });
    complete.unwrap();
}

#[test]
fn real_git_group_requires_complete_once_only_ordered_observation() {
    let temp = Temp::new(); fixture_git(&temp.0, &temp.0, &["init", "--initial-branch=main"]);
    let specs = vec![query(&["rev-parse", "--show-toplevel"]); 2];
    let environment = git_environment(&temp.0);
    let result = identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
        assert_eq!(group.value(1, false).unwrap_err().to_string(), "Git query outputs must be consumed once in original order");
        group.value(0, false)?;
        assert_eq!(group.value(0, false).unwrap_err().to_string(), "Git query outputs must be consumed once in original order");
        Ok(())
    });
    assert_eq!(result.unwrap_err().to_string(), "Incomplete ordered Git query observation");
    assert_eq!(identity_git::_git_group(&temp.0, &[], |_| Ok(())).unwrap_err().to_string(),
        "A Git query group requires one to three fixed commands");
    let result = identity_git::_git_group(&temp.0, &specs, |group| {
        group.value(0, false)?; Err::<(), _>(identity_git::Error::Contract(ContractError::new("owned malformed earlier config")))
    });
    assert_eq!(result.unwrap_err().to_string(), "owned malformed earlier config");
}

// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_success_drains_both_pipes_under_pressure_and_preserves_exact_bytes
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_sequential_runner_keeps_real_partial_timeout_diagnostics_and_cleanup
// Success/nonzero bytes subset uses real Git; deterministic partial-timeout bytes require the T0 child fixture.
#[test]
fn real_git_groups_drain_large_exact_outputs_and_single_runner_keeps_nonzero_bytes() {
    let temp = Temp::new(); fixture_git(&temp.0, &temp.0, &["init", "--initial-branch=main"]);
    // A real Git shell alias emits finite data to both streams under pipe pressure.
    // It finishes normally; no descendant is left for timeout cleanup to signal.
    let alias = "alias.pressure=!i=0; while [ \"$i\" -lt 96 ]; do printf '%08192d' 0; printf '%08192d' 0 >&2; i=$((i+1)); done; printf '\\n\\n'";
    let specs = vec![query(&["-c", alias, "pressure"]); 3];
    let environment = git_environment(&temp.0);
    identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
        for index in 0..3 {
            let observed = group.value(index, false)?.unwrap();
            assert_eq!(observed.as_bytes().len(), 96 * 8192 + 1);
            assert!(observed.as_bytes()[..96 * 8192].iter().all(|b| *b == b'0'));
            assert_eq!(observed.as_bytes().last(), Some(&b'\n'));
        }
        Ok(())
    }).unwrap();
    let invocation = identity_git::invocation_with_env(&temp.0,
        &query(&["-c", "alias.failure=!printf 'raw\\n\\n'; printf diagnostic >&2; exit 4", "failure"]).0,
        false, environment);
    let command = identity_git::run_invocation_for(&invocation, std::time::Duration::from_secs(10)).unwrap();
    assert_eq!(command.status, Status::Code(4)); assert_eq!(command.stdout, b"raw\n\n"); assert_eq!(command.stderr, b"diagnostic");
}

// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_four_invoking_workers_each_finish_their_own_real_children
#[test]
fn four_workers_own_independent_real_git_groups() {
    let temp = Temp::new(); fixture_git(&temp.0, &temp.0, &["init", "--initial-branch=main"]);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4).map(|_| {
            let path = &temp.0;
            scope.spawn(move || {
                let specs = vec![query(&["rev-parse", "--show-toplevel"]); 3];
                identity_git::_git_group(path, &specs, |group| {
                    for index in 0..3 { assert_eq!(group.value(index, false)?.unwrap(), path.as_os_str()); } Ok(())
                }).unwrap();
            })
        }).collect();
        for worker in workers { worker.join().unwrap(); }
    });
}

#[test]
fn real_git_missing_executable_permissions_and_expired_intervals_keep_error_classes() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new();
    let environment = vec![(OsString::from("PATH"), temp.0.as_os_str().to_owned())];
    let specs = vec![query(&["--version"]); 2];
    identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
        assert!(group.value(0, true)?.is_none());
        assert_eq!(group.value(1, false).unwrap_err().to_string(), "Cannot inspect local repository identity evidence");
        Ok(())
    }).unwrap();
    let executable = temp.0.join("git"); std::fs::write(&executable, b"not executable").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o600)).unwrap();
    let result = identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(10), |group| {
        group.value(0, true)?; group.value(1, true)?; Ok(())
    });
    assert!(result.is_err());
    let environment = git_environment(&temp.0);
    identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::ZERO, |group| {
        assert!(group.value(0, true)?.is_none());
        assert_eq!(group.value(1, false).unwrap_err().to_string(), "Cannot inspect local repository identity evidence");
        Ok(())
    }).unwrap();
}

#[test]
fn enotdir_is_not_optional_missing_git() {
    let error = agent_memory::process::ProcessError {
        reason: agent_memory::process::Refusal::Spawn, evidence: None,
        source: Some(std::io::Error::from_raw_os_error(libc::ENOTDIR)), cleanup_error: None, incomplete: false,
    };
    assert!(matches!(identity_git::_git_output(Err(error), true), Err(identity_git::Error::Infrastructure(_))));
}

struct Gate { ready: PathBuf, release: PathBuf, done: PathBuf }
impl Gate {
    fn new(root: &Path, name: &str) -> Self {
        Self { ready: root.join(format!("{name}.ready")), release: root.join(format!("{name}.release")), done: root.join(format!("{name}.done")) }
    }
    fn alias(&self) -> String {
        // Test-owned finite shell child: even assertion failure cannot leave it
        // blocked forever. No test or implementation signals an unowned PID.
        format!("alias.gate=!printf ready > '{}'; i=0; while [ ! -f '{}' ] && [ \"$i\" -lt 500 ]; do sleep 0.01; i=$((i+1)); done; printf done > '{}'",
            self.ready.display(), self.release.display(), self.done.display())
    }
    fn wait_ready(&self) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !self.ready.exists() {
            assert!(std::time::Instant::now() < end, "real Git gate did not start");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, b"release");
        if self.ready.exists() {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(6);
            while !self.done.exists() && std::time::Instant::now() < end { std::thread::sleep(std::time::Duration::from_millis(2)); }
        }
    }
}

// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_owner_parsing_time_is_charged_at_value_and_normal_exit
// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_late_observed_exit_is_not_accepted_after_owner_parser_deadline
#[test]
fn real_git_owner_parsing_time_cannot_turn_pending_timeout_into_success() {
    let temp = Temp::new(); fixture_git(&temp.0, &temp.0, &["init", "--initial-branch=main"]);
    let environment = git_environment(&temp.0);
    for consume in [true, false] {
        let gate = Gate::new(&temp.0, if consume { "value" } else { "exit" });
        let alias = gate.alias();
        let specs = vec![query(&["rev-parse", "--show-toplevel"]), query(&["-c", &alias, "gate"])];
        let observed = identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(1), |group| {
            assert_eq!(group.value(0, false)?.unwrap(), temp.0.as_os_str()); gate.wait_ready();
            std::thread::sleep(std::time::Duration::from_millis(1100));
            if consume { assert!(group.value(1, true)?.is_none()); }
            Ok(())
        });
        if consume { observed.unwrap(); }
        else { assert_eq!(observed.unwrap_err().to_string(), "Incomplete ordered Git query observation"); }
        drop(gate); // release and observe the owned test alias's finite descendant
    }
}

// obligation: tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_already_observed_completion_survives_late_consumption
#[test]
fn real_git_already_observed_completion_survives_late_consumption() {
    let temp = Temp::new(); fixture_git(&temp.0, &temp.0, &["init", "--initial-branch=main"]);
    let environment = git_environment(&temp.0);
    let gate = Gate::new(&temp.0, "earlier"); let alias = gate.alias();
    let quick_done = temp.0.join("quick.done");
    let quick_alias = format!("alias.quick=!printf complete; printf done > '{}'", quick_done.display());
    let specs = vec![query(&["-c", &alias, "gate"]), query(&["-c", &quick_alias, "quick"])];
    std::thread::scope(|scope| {
        scope.spawn(|| {
            gate.wait_ready();
            let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !quick_done.exists() {
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            // Keep value(0) pumping while the quick sibling exits and reaches EOF.
            std::thread::sleep(std::time::Duration::from_millis(100));
            std::fs::write(&gate.release, b"release").unwrap();
        });
        identity_git::git_group_with(&temp.0, &specs, &environment, std::time::Duration::from_secs(2), |group| {
            assert_eq!(group.value(0, false)?.unwrap().as_bytes(), b"");
            std::thread::sleep(std::time::Duration::from_millis(2100));
            assert_eq!(group.value(1, false)?.unwrap().as_bytes(), b"complete");
            Ok(())
        }).unwrap();
    });
}

#[test]
fn model_renderer_reports_missing_axis_section_without_panicking() {
    assert_eq!(agent_memory::model_reference::render("# incomplete").unwrap_err().to_string(),
        "not enough values to unpack (expected 2, got 1)");
}
