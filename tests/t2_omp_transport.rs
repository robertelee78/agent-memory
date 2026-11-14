//! omp_projection, omp_control and omp_transport: stored-entry projection,
//! reserved control classification, retained native evidence, pre-guard
//! observer refusals and the installed `am omp-boundary` transport route.
//! Environment-dependent cases re-execute this binary inside a private 0700
//! World (HOME/XDG roots inside it); no global environment is mutated.
#[allow(unused_imports)]
mod support;

use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::error::Result;
use agent_memory::omp_transport::{self as transport, Observation, ObserverScope, Site};
use agent_memory::scope_binding::PrivateEnrollmentContext;
use agent_memory::turn_sources::SourceBarrier;
use agent_memory::{omp_boundary, omp_control, omp_projection as projection, security};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::{RunOptions, World};

fn decode(text: &str) -> Value {
    json::decode(text, DecodePolicy::default()).unwrap()
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}
fn set(value: &mut Value, key: &str, item: Value) {
    if let Value::Object(pairs) = value {
        match pairs.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = item,
            None => pairs.push((key.to_owned(), item)),
        }
    }
}
fn at<'a>(value: &'a mut Value, path: &[&str]) -> &'a mut Value {
    let mut current = value;
    for key in path {
        current = current.get_mut(key).unwrap();
    }
    current
}
fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}
fn encoded(value: &Value) -> String {
    json::encode(value, json::SORTED_COMPACT_UTF8).unwrap()
}
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Run `body` in a re-executed copy of this test inside a private World; the
/// re-executed worker recognises itself by its verified world environment.
fn isolated(name: &str, body: impl FnOnce(&Path)) {
    if let Ok(root) = support::require_current_world() {
        agent_memory::cli::register_providers().unwrap();
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        body(&root);
        return;
    }
    let world = World::new().unwrap();
    let output = world
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                name.into(),
                "--include-ignored".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                cwd: Some(&world.root),
                check: false,
                timeout: Duration::from_secs(120),
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    world.close().unwrap();
}

/// A Git repository with a plain bank directory, inside the current world.
fn repository(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    std::fs::create_dir_all(repo.join("memory-bank")).unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "--quiet", "--initial-branch=main"])
        .arg(&repo)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::canonicalize(repo).unwrap()
}

fn entry(id: &str, role: &str, content: Value, attribution: &str) -> Value {
    decode(&format!(
        r#"{{"type":"message","id":"{id}","parentId":null,"message":{{"role":"{role}","attribution":"{attribution}","content":{}}}}}"#,
        encoded(&content)
    ))
}

/// Baseline `observer_payload`: a controlled native-format stored view.
fn observer_payload(repo: &Path, session: &str, entries: &[Value], terminal: bool) -> Value {
    let mut entries = entries.to_vec();
    for index in 1..entries.len() {
        if get(&entries[index], "parentId") == &Value::Null {
            let parent = get(&entries[index - 1], "id").clone();
            set(&mut entries[index], "parentId", parent);
        }
    }
    let branch: Vec<Value> = entries.iter().map(|e| get(e, "id").clone()).collect();
    let parents: Vec<Value> = entries
        .iter()
        .map(|e| {
            Value::Object(vec![
                ("id".into(), get(e, "id").clone()),
                ("parent_id".into(), get(e, "parentId").clone()),
                ("timestamp".into(), Value::Null),
            ])
        })
        .collect();
    let repo = repo.to_str().unwrap();
    let leaf = entries
        .last()
        .map(|e| get(e, "id").clone())
        .unwrap_or(Value::Null);
    let view = decode(&format!(
        r#"{{"session":"{session}","cwd":"{repo}","file":null,"header":{{"id":"{session}","cwd":"{repo}","version":3,"parent_session":null,"timestamp":null}},"leaf_id":null,"native_parents":[],"source_kind":"session-manager","tail_status":"available","fork_status":"none"}}"#
    ));
    let mut view = view;
    set(&mut view, "leaf_id", leaf);
    set(&mut view, "native_parents", Value::Array(parents));
    Value::Object(vec![
        ("entries".into(), Value::Array(entries.clone())),
        ("branch_ids".into(), Value::Array(branch)),
        ("event_ordinal".into(), Value::Int(entries.len().into())),
        ("terminal".into(), Value::Bool(terminal)),
        ("trigger".into(), string("resume")),
        ("inherited_ids".into(), Value::Array(vec![])),
        ("view".into(), view),
    ])
}
fn request(repo: &Path, session: &str, lifecycle: &str, entries: &[Value], action: &str) -> Value {
    let mut payload = observer_payload(repo, session, entries, false);
    if action == "seal" {
        set(&mut payload, "request_wake", Value::Bool(false));
    }
    Value::Object(vec![
        (
            "schema".into(),
            string("agent-memory/omp-observer-request-v2"),
        ),
        ("action".into(), string(action)),
        ("session".into(), string(session)),
        ("lifecycle".into(), string(lifecycle)),
        ("payload".into(), payload),
    ])
}

/// An observer scope that must never be reached by a pre-guard refusal.
struct Unreachable(Cell<bool>);
impl ObserverScope for Unreachable {
    fn observe(
        &self,
        _: &Path,
        _: Option<&SourceBarrier>,
        _: &mut dyn FnMut(&PrivateEnrollmentContext) -> Result<()>,
    ) -> Result<()> {
        self.0.set(true);
        Err(agent_memory::error::ContractError::new(
            "observer scope reached",
        ))
    }
}

/// Observe `value` and require a refusal before the scope, start or any source.
fn refused_before_start(repo: &Path, runtime: &Path, value: &Value) -> String {
    let scope = Unreachable(Cell::new(false));
    let started = Cell::new(false);
    let mut on_started = || {
        started.set(true);
        Ok(())
    };
    let call = Observation {
        on_started: Some(&mut on_started),
        ..Observation::default()
    };
    let error = transport::observe(repo, runtime, value, call, &scope).unwrap_err();
    assert!(
        !scope.0.get(),
        "observer scope reached for {}",
        encoded(value)
    );
    assert!(!started.get());
    error.message().to_owned()
}

// ----- security pattern search -----

#[test]
fn security_search_matches_the_captured_cpython_per_pattern_vectors() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/security-regex-v1.json");
    let vectors = agent_memory::sharing_limits::read_json(&path, 4 * 1024 * 1024).unwrap();
    let cases = get(&vectors, "cases").as_array().unwrap();
    let mut seen = (false, false);
    for case in cases {
        let text = get(case, "text").as_str().unwrap();
        let expected = get(case, "per_pattern")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| !get(row, "matches").as_array().unwrap().is_empty());
        assert_eq!(
            security::matches_any(text).unwrap(),
            expected,
            "{}",
            get(case, "id").as_str().unwrap()
        );
        if expected {
            seen.0 = true;
        } else {
            seen.1 = true;
        }
    }
    // At least the captured plain-safe negative and one positive case.
    assert_eq!(seen, (true, true));
    assert!(!security::matches_any("Useful independent source.").unwrap());
}

// ----- omp_projection -----

fn stored(message: &str) -> Value {
    decode(&format!(
        r#"{{"type":"message","id":"native-1","parentId":null,"message":{message}}}"#
    ))
}

// obligation: tests/contracts/test_omp_projection.py::StoredProjection.test_retains_tool_call_and_result_attribution
#[test]
fn retains_tool_call_and_result_attribution() {
    let source = stored(
        r#"{"role":"toolResult","toolCallId":"call-1","toolName":"read","content":[{"type":"text","text":"Example result"}],"details":{"count":1}}"#,
    );
    let result = projection::project(&source);
    let snapshot = get(&result, "snapshot");
    assert_eq!(get(snapshot, "toolCallId"), &string("call-1"));
    assert_eq!(get(snapshot, "details"), &decode(r#"{"count":1}"#));
    assert!(result.get("gap").is_none());
    assert_eq!(
        get(&result, "exclusions"),
        &decode(r#"["provider-metadata","reasoning-and-signatures"]"#)
    );
}

// obligation: tests/contracts/test_omp_projection.py::StoredProjection.test_reasoning_and_provider_fields_not_copied
#[test]
fn reasoning_and_provider_fields_not_copied() {
    let source = stored(
        r#"{"role":"assistant","providerPayload":{"private":"not copied"},"content":[{"type":"thinking","thinking":"not copied"},{"type":"text","text":"Visible stored result","textSignature":"not copied"}]}"#,
    );
    let result = projection::project(&source);
    assert_eq!(
        get(get(&result, "snapshot"), "content"),
        &decode(r#"[{"type":"text","text":"Visible stored result"}]"#)
    );
    assert!(!encoded(&result).contains("not copied"));
}

// obligation: tests/contracts/test_omp_projection.py::StoredProjection.test_opaque_or_secret_source_never_retained
#[test]
fn opaque_or_secret_source_never_retained() {
    for block in [
        r#"{"type":"image","data":"private"}"#,
        r#"{"type":"text","text":"password=secretvalue"}"#,
    ] {
        let result = projection::project(&stored(&format!(
            r#"{{"role":"user","content":[{block}]}}"#
        )));
        assert!(result.get("gap").is_some());
        assert!(result.get("snapshot").is_none());
        assert!(!encoded(&result).contains("secretvalue"));
    }
    let image = projection::project(&stored(
        r#"{"role":"user","content":[{"type":"image","data":"private"}]}"#,
    ));
    assert_eq!(get(&image, "gap"), &string("missing-payload"));
    let secret = projection::project(&stored(
        r#"{"role":"user","content":[{"type":"text","text":"password=secretvalue"}]}"#,
    ));
    assert_eq!(get(&secret, "gap"), &string("unsafe-representation"));
}

// obligation: tests/contracts/test_omp_projection.py::StoredProjection.test_tool_payload_metadata_is_not_copied
#[test]
fn tool_payload_metadata_is_not_copied() {
    let source = stored(
        r#"{"role":"toolResult","content":"result","details":{"providerPayload":{"secret":"data"}}}"#,
    );
    assert!(projection::project(&source).get("gap").is_some());
}

#[test]
fn projection_gaps_carriers_and_execution_records_are_exact() {
    let gap = |entry: &str| {
        get(&projection::project(&decode(entry)), "gap")
            .as_str()
            .map(str::to_owned)
    };
    assert_eq!(
        gap(r#"{"type":"label","id":"x"}"#).as_deref(),
        Some("missing-stored-source")
    );
    assert_eq!(gap(r#"[]"#).as_deref(), Some("missing-stored-source"));
    assert_eq!(
        gap(r#"{"type":"message","id":"x","message":{"role":"robot"}}"#).as_deref(),
        Some("missing-stored-source")
    );
    // An unhashable role raised TypeError in the baseline: payload gap.
    assert_eq!(
        gap(r#"{"type":"message","id":"x","message":{"role":["user"]}}"#).as_deref(),
        Some("missing-payload")
    );
    assert_eq!(
        gap(r#"{"type":"message","id":1,"message":{"role":"user","content":"x"}}"#).as_deref(),
        Some("missing-payload")
    );
    assert_eq!(
        gap(r#"{"type":"message","id":"x","message":{"role":"toolResult","isError":1,"content":"x"}}"#)
            .as_deref(),
        Some("missing-payload")
    );
    let truncated = format!(
        r#"{{"type":"message","id":"x","message":{{"role":"user","content":{}}}}}"#,
        encoded(&string(&format!(
            "body{}",
            projection::PERSISTENCE_TRUNCATION_NOTICE
        )))
    );
    assert_eq!(gap(&truncated).as_deref(), Some("oversized-source"));
    assert_eq!(
        gap(r#"{"type":"message","id":"x","message":{"role":"user","content":[{"type":"text","text":"a"},{"type":"reference"}]}}"#)
            .as_deref(),
        Some("missing-payload")
    );
    let bash = projection::project(&decode(
        r#"{"type":"message","id":"b","parentId":"a","message":{"role":"bashExecution","command":"ls","output":"x","cancelled":false,"extra":1}}"#,
    ));
    assert_eq!(
        encoded(get(&bash, "snapshot")),
        r#"{"cancelled":false,"carrier":"message","command":"ls","exitCode":null,"native_entry_id":"b","native_parent_id":"a","output":"x","role":"bashExecution"}"#
    );
    let truncated_output = projection::project(&decode(
        r#"{"type":"message","id":"b","message":{"role":"bashExecution","command":"ls","output":"x","truncated":true}}"#,
    ));
    assert_eq!(get(&truncated_output, "gap"), &string("missing-payload"));
    let mention = projection::project(&decode(
        r#"{"type":"message","id":"m","message":{"role":"fileMention","files":[{"path":"a.md","content":"A"}]}}"#,
    ));
    assert_eq!(
        encoded(get(get(&mention, "snapshot"), "files")),
        r#"[{"content":"A","path":"a.md"}]"#
    );
    let custom = projection::project(&decode(
        r#"{"type":"custom_message","id":"c","parentId":null,"customType":"t","attribution":"agent","content":"guidance"}"#,
    ));
    assert_eq!(
        encoded(get(&custom, "snapshot")),
        r#"{"attribution":"agent","carrier":"custom_message","content":"guidance","customType":"t","native_entry_id":"c","native_parent_id":null}"#
    );
}

#[test]
fn instruction_uris_timestamps_and_closed_instruction_context() {
    assert_eq!(
        projection::instruction_uri("SKILL.md").as_deref(),
        Some("skill://agent-memory")
    );
    assert_eq!(
        projection::instruction_uri("bank/GUIDE.md").as_deref(),
        Some("skill://agent-memory/bank/GUIDE.md")
    );
    for alias in [
        "GUIDE.md",
        "Bank/GUIDE.md",
        "bank//GUIDE.md",
        "1bank/GUIDE.md",
        "bank/guide.md",
    ] {
        assert_eq!(projection::instruction_uri(alias), None, "{alias}");
    }
    assert_eq!(
        projection::instruction_relative(&string("skill://agent-memory/a-1/b/GUIDE.md")).as_deref(),
        Some("a-1/b/GUIDE.md")
    );
    assert_eq!(
        projection::instruction_relative(&string("skill://agent-memory/")),
        None
    );
    for valid in [
        "2024-02-29T23:59:59Z",
        "2026-11-08T01:00:00.000Z",
        "2026-11-08T01:00:00.123456Z",
    ] {
        assert!(
            projection::stored_timestamp(&string(valid))
                .unwrap()
                .is_some(),
            "{valid}"
        );
    }
    for invalid in [
        "2023-02-29T00:00:00Z",
        "2026-11-08T24:00:00Z",
        "2026-11-08T01:00:60Z",
        "2026-11-08T01:00:00.1234567Z",
        "2026-11-08T01:00:00+00:00",
        "0000-01-01T00:00:00Z",
    ] {
        assert!(
            projection::stored_timestamp(&string(invalid)).is_err(),
            "{invalid}"
        );
    }
    assert!(
        projection::stored_timestamp(&Value::Null)
            .unwrap()
            .is_none()
    );
    let early = projection::stored_timestamp(&string("2026-11-08T00:59:00Z")).unwrap();
    let late = projection::stored_timestamp(&string("2026-11-08T01:00:00.5Z")).unwrap();
    assert!(early < late);
    let hash = "a".repeat(64);
    let snapshot = decode(&format!(
        r#"{{"carrier":"message","native_entry_id":"n","native_parent_id":null,"role":"toolResult","toolCallId":"c","toolName":"read","isError":false,"content":[],"instruction_context":{{"schema":"agent-memory/owned-instruction-context-v1","kind":"installed-am-manual","installation_id":"i","observed_receipt_sha256":"{hash}","artifact_digest":"{hash}","manual_file_sha256":"{hash}","manual_relative_path":"SKILL.md","renderer":"omp-18.16-full-skill-v1","invocation_native_entry_id":"inv","tool_call_id":"c"}}}}"#
    ));
    assert!(projection::valid_instruction_context(&snapshot));
    assert!(projection::valid_instruction_context(&decode(
        r#"{"carrier":"message"}"#
    )));
    assert!(!projection::valid_instruction_context(&Value::Null));
    let mutations: [(&[&str], Value); 6] = [
        (&["isError"], Value::Int(0u8.into())),
        (&["content"], decode(r#"[{"type":"text","text":"body"}]"#)),
        (&["instruction_context", "tool_call_id"], string("other")),
        (
            &["instruction_context", "manual_relative_path"],
            string("bank/../SKILL.md"),
        ),
        (
            &["instruction_context", "artifact_digest"],
            string(&"A".repeat(64)),
        ),
        (
            &["instruction_context", "installation_id"],
            string("password=secretvalue"),
        ),
    ];
    for (path, item) in mutations {
        let mut changed = snapshot.clone();
        *at(&mut changed, path) = item;
        assert!(!projection::valid_instruction_context(&changed), "{path:?}");
    }
    let mut extra = snapshot.clone();
    set(&mut extra, "details", Value::Null);
    assert!(!projection::valid_instruction_context(&extra));
}

// ----- omp_control -----

// obligation: tests/contracts/test_omp_boundary.py::StoredBatchControls.test_foreign_companions_and_human_input_never_become_own_maintenance
#[test]
fn foreign_companions_and_human_input_never_become_own_maintenance() {
    let guidance = Some("exact reserved guidance");
    let own = decode(
        r#"{"type":"custom_message","id":"own","parentId":null,"customType":"session-stop-continuation","attribution":"agent","content":"exact reserved guidance"}"#,
    );
    assert!(omp_control::pure_batch(std::slice::from_ref(&own), guidance, false, &[]).is_some());
    for custom_type in [
        "goal-mode-context",
        "plan-mode",
        "vibe-mode-context",
        "todo-error-reminder",
        "advisor",
        "MEMORY_RECALL_CHANGES",
        "foreign-before-agent",
        "unknown",
    ] {
        let mut foreign = own.clone();
        set(&mut foreign, "id", string("foreign"));
        set(&mut foreign, "customType", string(custom_type));
        set(&mut foreign, "content", string("Foreign contextual work"));
        assert!(omp_control::pure_batch(&[own.clone(), foreign], guidance, false, &[]).is_none());
    }
    let human = decode(
        r#"{"type":"message","id":"human","parentId":null,"message":{"role":"user","attribution":"user","content":"Actual new human request"}}"#,
    );
    assert!(omp_control::pure_batch(&[human, own.clone()], guidance, false, &[]).is_none());
    let mut joined = own;
    set(
        &mut joined,
        "content",
        string("exact reserved guidance\\njoined foreign stop text"),
    );
    assert!(omp_control::pure_batch(&[joined], guidance, false, &[]).is_none());
}

#[test]
fn reserved_boundary_invocations_and_results_stay_pure_only_when_exact() {
    assert_eq!(
        omp_control::shell_split(r#"am omp-boundary --repo "/a b" 'c d' e\ f "x\y" "q\"""#)
            .unwrap(),
        [
            "am",
            "omp-boundary",
            "--repo",
            "/a b",
            "c d",
            "e f",
            "x\\y",
            "q\""
        ]
    );
    assert_eq!(
        omp_control::shell_split(r#"a '' "" b"#).unwrap(),
        ["a", "", "", "b"]
    );
    assert!(omp_control::shell_split("am 'unclosed").is_none());
    assert!(omp_control::shell_split("am trailing\\").is_none());
    let call = |command: &str| {
        decode(&format!(
            r#"{{"type":"message","id":"call","parentId":null,"message":{{"role":"assistant","content":[{{"type":"text","text":"  "}},{{"type":"toolCall","id":"tool-1","name":"bash","arguments":{{"command":{}}}}}]}}}}"#,
            encoded(&string(command))
        ))
    };
    let result = decode(
        r#"{"type":"message","id":"result","parentId":"call","message":{"role":"toolResult","toolCallId":"tool-1","toolName":"bash","content":"done"}}"#,
    );
    let guidance = Some("reserved");
    let (projected, tools) = omp_control::pure_batch(
        &[call("am omp-boundary --repo /r --list-pending")],
        guidance,
        true,
        &[],
    )
    .unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(tools, ["tool-1"]);
    let (_, tools) = omp_control::pure_batch(
        &[call("agent-memory omp-boundary --list-pending"), result],
        guidance,
        true,
        &[],
    )
    .unwrap();
    assert!(tools.is_empty());
    for command in [
        "am omp-boundary --list-pending; rm -rf x",
        "am capture",
        "omp-boundary",
        "am omp-boundary $(id)",
        "am 'unclosed",
    ] {
        assert!(
            omp_control::pure_batch(&[call(command)], guidance, true, &[]).is_none(),
            "{command}"
        );
    }
    // Without continuation exactly one own hidden message is required.
    assert!(omp_control::pure_batch(&[call("am omp-boundary")], guidance, false, &[]).is_none());
    assert!(omp_control::pure_batch(&[], None, true, &[]).is_none());
}

// ----- omp_transport records -----

#[test]
fn native_uuids_cursors_and_admission_metadata_are_exact() {
    // RFC 4122 / CPython documentation vector: uuid5(NAMESPACE_DNS, 'python.org').
    assert_eq!(
        transport::uuid5("6ba7b810-9dad-11d1-80b4-00c04fd430c8", "python.org").unwrap(),
        "886313e1-3b8a-5372-9b90-0c9aee199e5d"
    );
    assert_eq!(
        transport::uuid5_url("x"),
        transport::uuid5("6ba7b811-9dad-11d1-80b4-00c04fd430c8", "x").unwrap()
    );
    let lifecycle = "0f0e0d0c-0b0a-4908-8706-050403020100";
    assert!(transport::canonical_uuid(&string(lifecycle)).is_ok());
    for invalid in [
        "0F0E0D0C-0B0A-4908-8706-050403020100",
        "{0f0e0d0c-0b0a-4908-8706-050403020100}",
        "0f0e0d0c0b0a490887060504030201000000",
    ] {
        assert!(
            transport::canonical_uuid(&string(invalid)).is_err(),
            "{invalid}"
        );
    }
    let cursor = decode(&format!(
        r#"{{"schema":"agent-memory/omp-cursor-v1","session":"s","lifecycle":"{lifecycle}","origin":{{"kind":"genuine-user","human_root":"{lifecycle}","native_trigger":"human"}},"last_native_entry":null,"event_ordinal":3,"control_interval":{{"capture_order":["a","b"],"human_entry":"a"}},"tool_lifecycles":{{"call":"{lifecycle}"}}}}"#
    ));
    assert!(transport::cursor_value(&cursor, &string("s"), lifecycle).is_ok());
    assert!(transport::cursor_value(&cursor, &string("t"), lifecycle).is_err());
    let invalid: [(&[&str], Value); 7] = [
        (&["origin", "native_trigger"], string("foreign-hidden")),
        (&["origin", "kind"], string("other")),
        (&["event_ordinal"], Value::Int((-1i8).into())),
        (&["event_ordinal"], Value::Bool(true)),
        (
            &["control_interval", "capture_order"],
            decode(r#"["a","a"]"#),
        ),
        (&["control_interval", "unknown"], Value::Null),
        (&["tool_lifecycles", "call"], string("not-a-uuid")),
    ];
    for (path, item) in invalid {
        let mut changed = cursor.clone();
        if path.len() == 2
            && changed
                .get(path[0])
                .is_some_and(|v| v.get(path[1]).is_none())
        {
            set(changed.get_mut(path[0]).unwrap(), path[1], item);
        } else {
            *at(&mut changed, path) = item;
        }
        assert!(
            transport::cursor_value(&changed, &string("s"), lifecycle).is_err(),
            "{path:?}"
        );
    }
    let admission = decode(&format!(
        r#"{{"schema":"agent-memory/omp-admission-v1","session":"s","lifecycle":"{lifecycle}","capture_id":"{lifecycle}","event_ordinal":0,"role":"user","attribution":"user","tool_call_ids":["a","b"]}}"#
    ));
    assert!(transport::admission_metadata(&admission, &string("s")).is_ok());
    let mut duplicate = admission.clone();
    set(&mut duplicate, "tool_call_ids", decode(r#"["a","a"]"#));
    assert!(transport::admission_metadata(&duplicate, &string("s")).is_err());
    assert!(transport::admission_metadata(&admission, &string("other")).is_err());
}

// ----- pre-guard observer refusals (no source, start or scope) -----

const SESSION: &str = "native-session";
const LIFECYCLE: &str = "0f0e0d0c-0b0a-4908-8706-050403020100";

fn human(id: &str, text: &str) -> Value {
    entry(id, "user", string(text), "user")
}

// obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_changed_header_branch_and_parent_graph_refuse_before_transaction
#[test]
fn changed_header_branch_and_parent_graph_refuse_before_transaction() {
    isolated(
        "changed_header_branch_and_parent_graph_refuse_before_transaction",
        |root| {
            let repo = repository(root, "repo");
            let runtime = root.join("runtime");
            let original = request(
                &repo,
                SESSION,
                LIFECYCLE,
                &[human("human", "Actual source.")],
                "ingest",
            );
            let mutations: [fn(&mut Value); 3] = [
                |v| *at(v, &["payload", "view", "header", "id"]) = string("another-session"),
                |v| *at(v, &["payload", "branch_ids"]) = Value::Array(vec![]),
                |v| {
                    let parents = at(v, &["payload", "view", "native_parents"]);
                    if let Value::Array(items) = parents {
                        set(&mut items[0], "parent_id", string("missing"));
                    }
                },
            ];
            for mutate in mutations {
                let mut value = original.clone();
                mutate(&mut value);
                refused_before_start(&repo, &runtime, &value);
            }
            let site = Site::new(&repo, &runtime);
            assert!(transport::segments(&site, SESSION).unwrap().is_empty());
        },
    );
}

// obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_view_closed_identity_negatives_refuse_before_source_writes
#[test]
fn retired_view_closed_identity_negatives_refuse_before_source_writes() {
    isolated(
        "retired_view_closed_identity_negatives_refuse_before_source_writes",
        |root| {
            let repo = repository(root, "repo");
            let runtime = root.join("runtime");
            let mut original = request(
                &repo,
                SESSION,
                LIFECYCLE,
                &[human("human", "Actual request.")],
                "ingest",
            );
            let file = repo.join("owned-native.jsonl");
            *at(&mut original, &["payload", "view", "source_kind"]) = string("retired-file");
            *at(&mut original, &["payload", "view", "file"]) = string(file.to_str().unwrap());
            let parent = repo.parent().unwrap().to_str().unwrap().to_owned();
            fn parents(v: &mut Value) -> &mut Vec<Value> {
                match at(v, &["view", "native_parents"]) {
                    Value::Array(items) => items,
                    _ => unreachable!(),
                }
            }
            fn first_entry(v: &mut Value) -> &mut Value {
                match at(v, &["entries"]) {
                    Value::Array(items) => &mut items[0],
                    _ => unreachable!(),
                }
            }
            let mutations: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
                (
                    "duplicate",
                    Box::new(move |v| {
                        let copy = parents(v)[0].clone();
                        parents(v).push(copy);
                    }),
                ),
                (
                    "cycle",
                    Box::new(move |v| {
                        set(&mut parents(v)[0], "parent_id", string("human"));
                        set(first_entry(v), "parentId", string("human"));
                    }),
                ),
                (
                    "leaf",
                    Box::new(|v| *at(v, &["view", "leaf_id"]) = string("missing")),
                ),
                (
                    "cwd",
                    Box::new(move |v| *at(v, &["view", "cwd"]) = string(&parent)),
                ),
                (
                    "version",
                    Box::new(|v| *at(v, &["view", "header", "version"]) = Value::Int(99u8.into())),
                ),
                (
                    "header",
                    Box::new(|v| *at(v, &["view", "header", "id"]) = string("wrong-session")),
                ),
                (
                    "file",
                    Box::new(|v| *at(v, &["view", "file"]) = Value::Null),
                ),
                (
                    "tail",
                    Box::new(|v| *at(v, &["view", "tail_status"]) = string("active-unavailable")),
                ),
                (
                    "message",
                    Box::new(move |v| set(first_entry(v), "message", Value::Null)),
                ),
                (
                    "reserved-native",
                    Box::new(move |v| {
                        set(&mut parents(v)[0], "id", string("tool:collision"));
                        set(first_entry(v), "id", string("tool:collision"));
                        *at(v, &["view", "leaf_id"]) = string("tool:collision");
                        *at(v, &["branch_ids"]) = decode(r#"["tool:collision"]"#);
                    }),
                ),
            ];
            for (name, mutation) in mutations {
                let mut value = original.clone();
                mutation(at(&mut value, &["payload"]));
                let refusal = refused_before_start(&repo, &runtime, &value);
                assert!(!refusal.is_empty(), "{name}");
            }
            let site = Site::new(&repo, &runtime);
            assert!(transport::segments(&site, SESSION).unwrap().is_empty());
        },
    );
}

// obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_nested_foreign_repository_cwd_cannot_borrow_parent_bank_scope
#[test]
fn nested_foreign_repository_cwd_cannot_borrow_parent_bank_scope() {
    isolated(
        "nested_foreign_repository_cwd_cannot_borrow_parent_bank_scope",
        |root| {
            let repo = repository(root, "repo");
            let nested = repository(&repo, "foreign-repository");
            let runtime = root.join("runtime");
            let mut value = request(
                &repo,
                SESSION,
                LIFECYCLE,
                &[human("human", "A different repository task.")],
                "ingest",
            );
            let nested_text = string(nested.to_str().unwrap());
            *at(&mut value, &["payload", "view", "cwd"]) = nested_text.clone();
            *at(&mut value, &["payload", "view", "header", "cwd"]) = nested_text;
            let refusal = refused_before_start(&repo, &runtime, &value);
            assert!(refusal.contains("another bank scope"), "{refusal}");
            let site = Site::new(&repo, &runtime);
            assert!(transport::segments(&site, SESSION).unwrap().is_empty());
        },
    );
}

// obligation: tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_unavailable_or_nongit_manager_cwd_refuses_before_source_start
#[test]
fn unavailable_or_nongit_manager_cwd_refuses_before_source_start() {
    isolated(
        "unavailable_or_nongit_manager_cwd_refuses_before_source_start",
        |root| {
            let repo = repository(root, "repo");
            let runtime = root.join("runtime");
            let nongit = root.join("not-a-repository");
            std::fs::create_dir(&nongit).unwrap();
            let invalid = repo.join("unavailable-nested-git");
            std::fs::create_dir(&invalid).unwrap();
            std::fs::write(invalid.join(".git"), "invalid Git locator\n").unwrap();
            for cwd in [nongit, invalid, root.join("missing-directory")] {
                let mut value = request(
                    &repo,
                    SESSION,
                    LIFECYCLE,
                    &[human("human", "Unqualified location.")],
                    "ingest",
                );
                *at(&mut value, &["payload", "view", "cwd"]) = string(cwd.to_str().unwrap());
                refused_before_start(&repo, &runtime, &value);
            }
            let site = Site::new(&repo, &runtime);
            assert!(transport::segments(&site, SESSION).unwrap().is_empty());
        },
    );
}

#[test]
fn unsupported_requests_refuse_before_start() {
    isolated("unsupported_requests_refuse_before_start", |root| {
        let repo = repository(root, "repo");
        let runtime = root.join("runtime");
        let base = request(
            &repo,
            SESSION,
            LIFECYCLE,
            &[human("human", "Source.")],
            "seal",
        );
        let mutations: [fn(&mut Value); 6] = [
            |v| *at(v, &["schema"]) = string("agent-memory/omp-observer-request-v1"),
            |v| *at(v, &["action"]) = string("admit"),
            |v| *at(v, &["lifecycle"]) = string("not-a-uuid"),
            |v| *at(v, &["payload", "request_wake"]) = Value::Int(1u8.into()),
            |v| *at(v, &["payload", "trigger"]) = string("unknown"),
            |v| *at(v, &["payload", "inherited_ids"]) = decode(r#"["absent"]"#),
        ];
        for mutate in mutations {
            let mut value = base.clone();
            mutate(&mut value);
            refused_before_start(&repo, &runtime, &value);
        }
        // An ingest request cannot carry the seal-only wake selection.
        let mut ingest = base.clone();
        *at(&mut ingest, &["action"]) = string("ingest");
        refused_before_start(&repo, &runtime, &ingest);
    });
}

// ----- retained segment evidence -----

fn write_private(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn retained_capture_gap_seal_and_resolution_bind_exact_native_identity() {
    isolated(
        "retained_capture_gap_seal_and_resolution_bind_exact_native_identity",
        |root| {
            let repo = repository(root, "repo");
            let runtime = root.join("runtime");
            let site = Site::new(&repo, &runtime);
            let base = omp_boundary::base(&repo).unwrap();
            agent_memory::fs::private_mkdir_with_sync(&base, true).unwrap();
            let binding = format!(
                r#"{{"repository":"{}","schema":"agent-memory/omp-bank-binding-v1"}}"#,
                repo.display()
            );
            write_private(&base.join("omp-bank-binding-v1.json"), binding.as_bytes());
            let lifecycle =
                transport::uuid5_url(&format!("{}:omp:{SESSION}:human", repo.display()));
            let segment =
                transport::new_segment(&site, SESSION, &lifecycle, &lifecycle, "human", 1).unwrap();
            let directory = segment.directory.clone();
            assert_eq!(
                directory,
                base.join("omp-source-v1")
                    .join(sha256(SESSION.as_bytes()))
                    .join(&lifecycle)
            );
            // Without its stored human entry the root has no source authority.
            let cursor = segment.snapshot().unwrap();
            assert!(!transport::source_bound_origin(&directory, &cursor).unwrap());
            let user = human("human", "An ephemeral arithmetic question");
            let captured = transport::capture(&directory, &cursor, &user, "verified", None, false)
                .unwrap()
                .unwrap();
            assert_eq!(
                get(&captured, "capture_id").as_str().unwrap(),
                transport::uuid5(&lifecycle, "human").unwrap()
            );
            assert_eq!(
                get(&captured, "snapshot_ref").as_str().unwrap(),
                format!("snapshots/{}.json", sha256(b"human"))
            );
            let snapshot = transport::read(
                &directory.join(get(&captured, "snapshot_ref").as_str().unwrap()),
                transport::MAX_PAYLOAD,
            )
            .unwrap();
            assert_eq!(
                get(&captured, "snapshot_sha256").as_str().unwrap(),
                sha256(encoded(&snapshot).as_bytes())
            );
            // Recapture is idempotent; a changed retained snapshot is refused.
            assert_eq!(
                transport::capture(&directory, &cursor, &user, "verified", None, false).unwrap(),
                Some(captured.clone())
            );
            let changed = human("human", "A different body under the same native ID");
            assert!(
                transport::capture(&directory, &cursor, &changed, "verified", None, false).is_err()
            );
            let mut bound = cursor.clone();
            set(
                &mut bound,
                "control_interval",
                decode(r#"{"capture_order":["human"],"human_entry":"human"}"#),
            );
            transport::replace_cursor(&directory.join("cursor.json"), &bound).unwrap();
            assert!(transport::source_bound_origin(&directory, &bound).unwrap());
            // An unsafe entry becomes a content-free gap, never a snapshot.
            let unsafe_entry = entry(
                "unsafe",
                "assistant",
                string("password=synthetic-secret-value"),
                "agent",
            );
            assert!(
                transport::capture(&directory, &bound, &unsafe_entry, "verified", None, false)
                    .unwrap()
                    .is_none()
            );
            let late = entry(
                "late",
                "assistant",
                decode(r#"[{"type":"image","data":"x"}]"#),
                "agent",
            );
            assert!(
                transport::capture(&directory, &bound, &late, "verified", None, false)
                    .unwrap()
                    .is_none()
            );
            let segments = transport::segments(&site, SESSION).unwrap();
            assert_eq!(segments.len(), 1);
            let (rows, faults) = transport::pending_inventory(&site).unwrap();
            assert!(faults.is_empty(), "{faults:?}");
            assert_eq!(
                rows.iter()
                    .map(|r| get(r, "status").as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["open-segment"]
            );
            assert!(
                transport::native_disposition_ids(&site, SESSION)
                    .unwrap()
                    .into_iter()
                    .eq(["human", "late", "unsafe"].map(str::to_owned))
            );
            let manifest = transport::seal(&directory, &bound).unwrap();
            assert_eq!(
                get(&manifest, "seal_id").as_str().unwrap(),
                transport::uuid5(&lifecycle, "seal").unwrap()
            );
            assert_eq!(
                get(&manifest, "admission_ref"),
                get(&captured, "snapshot_ref")
            );
            assert_eq!(get(&manifest, "previous_digest"), &Value::Null);
            let gaps = get(&manifest, "gaps").as_array().unwrap();
            assert_eq!(gaps.len(), 2);
            assert!(!encoded(&manifest).contains("synthetic-secret"));
            assert_eq!(
                encoded(&transport::manifest_value(&directory, &bound).unwrap()),
                encoded(&manifest)
            );
            assert!(!transport::coverage(&directory, &manifest, false).unwrap());
            // Sealing is idempotent and enrolls the admission and the segment.
            assert_eq!(
                encoded(&transport::seal(&directory, &bound).unwrap()),
                encoded(&manifest)
            );
            let enrollments =
                transport::files(&directory, "enrollments", transport::Names::Hex64).unwrap();
            assert_eq!(enrollments.len(), 2);
            assert!(directory.join("admission.json").exists());
            // A sealed gap is resolved only by the exact safe native source.
            let late_safe = entry(
                "late",
                "assistant",
                string("Recovered safe stored body"),
                "agent",
            );
            transport::capture(&directory, &bound, &late_safe, "verified", None, false)
                .unwrap()
                .unwrap();
            let resolutions =
                transport::files(&directory, "resolutions", transport::Names::Dash36).unwrap();
            assert_eq!(resolutions.len(), 1);
            assert!(!transport::coverage(&directory, &manifest, false).unwrap());
            // The unsafe source can never be recovered by a payload.
            assert!(
                transport::capture(&directory, &bound, &unsafe_entry, "verified", None, false)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                transport::files(&directory, "resolutions", transport::Names::Dash36)
                    .unwrap()
                    .len(),
                1
            );
        },
    );
}

#[test]
fn unbound_or_foreign_families_never_gain_source_authority() {
    isolated(
        "unbound_or_foreign_families_never_gain_source_authority",
        |root| {
            let repo = repository(root, "repo");
            let runtime = root.join("runtime");
            let site = Site::new(&repo, &runtime);
            let base = omp_boundary::base(&repo).unwrap();
            agent_memory::fs::private_mkdir_with_sync(&base, true).unwrap();
            let binding = format!(
                r#"{{"repository":"{}","schema":"agent-memory/omp-bank-binding-v1"}}"#,
                repo.display()
            );
            write_private(&base.join("omp-bank-binding-v1.json"), binding.as_bytes());
            // A random genuine root is never bound to a stored native user.
            let random = "11111111-2222-4333-8444-555555555555";
            let segment =
                transport::new_segment(&site, SESSION, random, random, "human", 1).unwrap();
            let cursor = segment.snapshot().unwrap();
            let mut claimed = cursor.clone();
            set(
                &mut claimed,
                "control_interval",
                decode(r#"{"human_entry":"human"}"#),
            );
            assert!(!transport::source_bound_origin(&segment.directory, &claimed).unwrap());
            assert!(transport::seal(&segment.directory, &claimed).is_err());
            // Extending it with a successor is refused.
            assert!(
                transport::new_segment(
                    &site,
                    SESSION,
                    "22222222-2222-4333-8444-555555555555",
                    random,
                    "foreign-hidden",
                    2
                )
                .is_err()
            );
            // A foreign continuation root is source-bound by its own origin.
            let foreign = "33333333-2222-4333-8444-555555555555";
            let continuation =
                transport::new_segment(&site, SESSION, foreign, foreign, "resume", 3).unwrap();
            assert!(
                transport::source_bound_origin(
                    &continuation.directory,
                    &continuation.snapshot().unwrap()
                )
                .unwrap()
            );
            // Its unbound sibling family reports legacy metadata, never a source.
            let (rows, _) = transport::pending_inventory(&site).unwrap();
            let statuses: Vec<&str> = rows
                .iter()
                .map(|r| get(r, "status").as_str().unwrap())
                .collect();
            assert!(statuses.contains(&"open-segment"), "{statuses:?}");
            // Unknown record names in a segment are an interrupted record, a fault.
            write_private(&continuation.directory.join("unexpected.json"), b"{}");
            let (rows, faults) = transport::pending_inventory(&site).unwrap();
            assert!(
                faults.contains(&"native-enrollment-unverifiable".to_owned()),
                "{faults:?}"
            );
            assert!(
                rows.iter().any(|r| get(r, "source_reference").as_str()
                    == Some(&format!("omp:fault:{foreign}")))
            );
        },
    );
}

// ----- installed route -----

const COMPATIBILITY: &str = r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"installed_commands":["am","agent-memory"],"installation_receipt_schemas":["agent-memory/install-receipt-v1","agent-memory/install-receipt-v2","agent-memory/install-receipt-v3"],"git_required_for":["git-sharing"],"required_agent_clients":[],"optional_adapters":[]}"#;

fn write_mode(path: &Path, body: impl AsRef<[u8]>, mode: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A minimal valid portable pack plus this build's real `am`.
fn development_source(world: &World) -> PathBuf {
    let root = world.inside("development-source").unwrap();
    let pack = root.join("skills/agent-memory");
    write_mode(
        &pack.join("SKILL.md"),
        "---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n",
        0o644,
    );
    write_mode(
        &pack.join("bank/GUIDE.md"),
        "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n",
        0o644,
    );
    let mut operation =
        String::from("---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n");
    for section in agent_memory::pack_validation::SECTIONS {
        operation.push_str(&format!("## {section}\n\n"));
        if section == "Required context" {
            operation.push_str(
                "A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n",
            );
        } else {
            operation.push_str("Explicit operation context.\n\n");
        }
    }
    write_mode(&pack.join("inspect/GUIDE.md"), operation, 0o644);
    write_mode(
        &pack.join("schemas/runtime-compatibility.json"),
        COMPATIBILITY,
        0o644,
    );
    write_mode(
        &pack.join("schemas/bank-router.json"),
        r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#,
        0o644,
    );
    write_mode(
        &pack.join("templates/activation-markdown.example.txt"),
        agent_memory::installation::SHIM,
        0o644,
    );
    write_mode(
        &pack.join("templates/activation-omp-markdown.example.txt"),
        include_str!("../skills/agent-memory/templates/activation-omp-markdown.example.txt"),
        0o644,
    );
    write_mode(
        &pack.join("hooks/opencode.js"),
        "export default {}\n",
        0o644,
    );
    write_mode(
        &pack.join("hooks/omp.ts"),
        "export default function () {}\n",
        0o644,
    );
    for directory in ["", "bank", "inspect", "schemas", "templates", "hooks"] {
        std::fs::set_permissions(pack.join(directory), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    write_mode(
        &root.join("target/release/am"),
        std::fs::read(env!("CARGO_BIN_EXE_am")).unwrap(),
        0o755,
    );
    root
}

fn run(world: &World, argv: &[OsString], cwd: &Path) -> agent_memory::process::Output {
    world
        .run_with(
            argv,
            RunOptions {
                cwd: Some(cwd),
                check: false,
                timeout: Duration::from_secs(120),
                ..RunOptions::default()
            },
        )
        .unwrap()
}

#[test]
fn installed_stream_passes_observer_binding_and_reaches_real_transport() {
    let world = World::new().unwrap();
    let source = development_source(&world);
    let agent = world.inside("omp-agent").unwrap();
    std::fs::create_dir(&agent).unwrap();
    let install: Vec<OsString> = [
        env!("CARGO_BIN_EXE_am"),
        "install",
        "--development-source",
        source.to_str().unwrap(),
        "--presets",
        "omp",
        "--omp-agent-dir",
        agent.to_str().unwrap(),
        "--omp-peers-upgraded",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let installed = run(&world, &install, &world.root);
    assert_eq!(
        installed.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&installed.stdout),
        String::from_utf8_lossy(&installed.stderr)
    );
    let receipt = agent_memory::sharing_limits::read_json(
        &world.root.join("state/agent-memory/install/receipt.json"),
        16 * 1024 * 1024,
    )
    .unwrap();
    let am = Path::new(get(&receipt, "bin_dir").as_str().unwrap()).join("am");
    let artifact = get(get(&receipt, "source"), "artifact_digest")
        .as_str()
        .unwrap()
        .to_owned();
    let agent_dir = get(get(&receipt, "omp_binding"), "agent_dir")
        .as_str()
        .unwrap()
        .to_owned();
    let outside = world.inside("outside-any-repository").unwrap();
    std::fs::create_dir(&outside).unwrap();
    let stream = |repo: &Path| -> Vec<OsString> {
        [
            am.to_str().unwrap(),
            "omp-boundary",
            "--stream",
            "--repo",
            repo.to_str().unwrap(),
            "--session",
            SESSION,
            "--instance",
            "44444444-2222-4333-8444-555555555555",
            "--lifecycle",
            LIFECYCLE,
            "--artifact",
            &artifact,
            "--omp-agent-dir",
            &agent_dir,
        ]
        .iter()
        .map(OsString::from)
        .collect()
    };
    // The installed observer binding verifies, the runtime pack resolves and
    // the finite worker starts: outside a bank it reports inactive, exit 0.
    let inactive = run(&world, &stream(&outside), &outside);
    let stdout = String::from_utf8(inactive.stdout.clone()).unwrap();
    assert_eq!(
        inactive.status,
        agent_memory::process::Status::Code(0),
        "{stdout}\n{}",
        String::from_utf8_lossy(&inactive.stderr)
    );
    assert!(stdout.starts_with("AM249 INACTIVE 0 "), "{stdout}");
    assert!(stdout.contains("\"inactive-no-bank\""), "{stdout}");
    // A wrong artifact is refused by the observer binding before any work.
    let mut forged = stream(&outside);
    let position = forged.iter().position(|a| a == "--artifact").unwrap() + 1;
    forged[position] = OsString::from("0".repeat(64));
    let refused = run(&world, &forged, &outside);
    assert_eq!(refused.status, agent_memory::process::Status::Code(1));
    assert!(refused.stdout.is_empty());
    // In a repository with a bank the installed route reaches the real
    // transport inventory: verified, empty, complete, no canonical writes.
    let repo = world.repository("repo", false).unwrap();
    std::fs::create_dir(repo.join("memory-bank")).unwrap();
    let pending: Vec<OsString> = [
        am.to_str().unwrap(),
        "omp-boundary",
        "--repo",
        repo.to_str().unwrap(),
        "--list-pending",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let listed = run(&world, &pending, &repo);
    let stdout = String::from_utf8(listed.stdout.clone()).unwrap();
    assert_eq!(
        listed.status,
        agent_memory::process::Status::Code(0),
        "{stdout}\n{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let value = decode(&stdout);
    assert_eq!(get(&value, "operation"), &string("pending"));
    assert_eq!(get(&value, "pending"), &Value::Array(vec![]));
    assert_eq!(get(&value, "inventory_complete"), &Value::Bool(true));
    assert_eq!(get(&value, "verified"), &Value::Bool(true));
    assert_eq!(
        get(&value, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    world.close().unwrap();
}
