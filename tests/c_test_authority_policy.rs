mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::ffi::OsString;
use std::fs;
use support::cutover::*;
use support::{RunOptions, World};

fn request() -> Value {
    object([
        ("fact", text("implementation")),
        ("fact_key", text("retry-limit")),
        ("project_id", text(PROJECT)),
        (
            "detecting_developer_id",
            text("22222222-2222-4222-8222-222222222222"),
        ),
        (
            "memory",
            object([
                ("id", text("33333333-3333-4333-8333-333333333333")),
                ("version", Value::Int(2.into())),
                ("project_id", text(PROJECT)),
                ("value", Value::Int(5.into())),
                ("scope", text("component:retry.rs")),
                ("conditions", object([("branch", text("main"))])),
                ("developer_id", text("44444444-4444-4444-8444-444444444444")),
                (
                    "evidence",
                    Value::Array(vec![text("historical:retry-observation")]),
                ),
            ]),
        ),
        ("sources", Value::Array(vec![])),
    ])
}
fn source(kind: &str, value: Value, reference: &str) -> Value {
    object([
        ("kind", text(kind)),
        ("ref", text(reference)),
        ("project_id", text(PROJECT)),
        ("fact_key", text("retry-limit")),
        ("current", Value::Bool(true)),
        ("applicable", Value::Bool(true)),
        ("qualified", Value::Bool(true)),
        ("value", value),
    ])
}
fn assess(w: &World, request: &Value) -> (agent_memory::process::Output, Value) {
    let path = save(w, request, "reviewed-assertions.json");
    let before = fs::read(&path).unwrap();
    let result = command(
        w,
        "authority_policy.py",
        &[
            "assess".into(),
            "--input".into(),
            path.as_os_str().to_owned(),
        ],
    );
    assert_eq!(fs::read(path).unwrap(), before);
    result
}

#[test]
// obligation: tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_conflicting_code_intent_and_direct_project_decision_keep_their_own_authorities
fn conflicting_code_intent_and_project_decisions_keep_distinct_authorities() {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    fs::write(repo.join("retry.rs"), "const ATTEMPTS: u32 = 3;\n").unwrap();
    fs::write(
        repo.join("approved-adr.md"),
        "Approved desired retry limit: 5\n",
    )
    .unwrap();
    fs::write(
        repo.join("project-decision.md"),
        "Operator decision for this project: yellow walls.\n",
    )
    .unwrap();
    w.git_with(
        ["add", "."],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    w.git_with(
        ["commit", "-m", "Fixture evidence"],
        RunOptions {
            cwd: Some(&repo),
            ..RunOptions::default()
        },
    )
    .unwrap();
    let revision = w
        .git_with(
            ["rev-parse", "HEAD"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            },
        )
        .unwrap()
        .stdout;
    let revision = std::str::from_utf8(&revision)
        .unwrap()
        .trim_end_matches('\n');
    let before = physical(&repo);
    let actual: u32 = fs::read_to_string(repo.join("retry.rs"))
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches(';')
        .parse()
        .unwrap();
    let mut req = request();
    let code_ref = format!("{revision}:retry.rs");
    let intent_ref = format!("{revision}:approved-adr.md");
    put(
        &mut req,
        "sources",
        Value::Array(vec![
            source("code", Value::Int(actual.into()), &code_ref),
            source("approved-planning", Value::Int(5.into()), &intent_ref),
            source("memory", Value::Int(5.into()), "memory:landed-later"),
        ]),
    );
    let (output, result) = assess(&w, &req);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(
        get(get(&result, "correction_candidate"), "proposed_value"),
        &Value::Int(3.into())
    );
    assert_eq!(
        get(&result, "source_refs"),
        &Value::Array(vec![text(&code_ref)])
    );
    assert!(
        get(&result, "observed_refs")
            .as_array()
            .unwrap()
            .contains(&text(&intent_ref))
    );
    put(&mut req, "fact", text("intent"));
    let (output, result) = assess(&w, &req);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(string(&result, "action"), "use-current-authority");
    assert_eq!(
        get(&result, "source_refs"),
        &Value::Array(vec![text(&intent_ref)])
    );
    put(&mut req, "fact", text("requirement"));
    put(&mut req, "fact_key", text("project-wall-color"));
    put(req.get_mut("memory").unwrap(), "value", text("blue"));
    let mut decision = source(
        "user-statement",
        text("yellow"),
        &format!("{revision}:project-decision.md"),
    );
    put(&mut decision, "fact_key", text("project-wall-color"));
    let mut memory = source("memory", text("blue"), "memory:newest-note");
    put(&mut memory, "fact_key", text("project-wall-color"));
    put(&mut req, "sources", Value::Array(vec![decision, memory]));
    let (output, result) = assess(&w, &req);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(
        get(get(&result, "correction_candidate"), "proposed_value"),
        &text("yellow")
    );
    assert_eq!(physical(&repo), before);
    assert_eq!(
        w.git_with(
            ["rev-parse", "HEAD"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            }
        )
        .unwrap()
        .stdout,
        format!("{revision}\n").as_bytes()
    );
    assert!(
        w.git_with(
            ["status", "--porcelain"],
            RunOptions {
                cwd: Some(&repo),
                ..RunOptions::default()
            }
        )
        .unwrap()
        .stdout
        .is_empty()
    );
}

#[test]
// obligation: tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_unqualified_inapplicable_or_stale_sources_cannot_promote_memory_to_proof
fn unqualified_inapplicable_or_stale_sources_do_not_promote_memory_to_proof() {
    let w = World::new().unwrap();
    let mut req = request();
    for flag in ["current", "applicable", "qualified"] {
        let mut code = source("code", Value::Int(3.into()), "code:current-reference");
        put(&mut code, flag, Value::Bool(false));
        put(
            &mut req,
            "sources",
            Value::Array(vec![
                code,
                source("memory", Value::Int(5.into()), "memory:current-reference"),
            ]),
        );
        let (output, result) = assess(&w, &req);
        assert_eq!(output.status, Status::Code(0));
        assert_eq!(string(&result, "action"), "inspect-current-authority");
        assert_eq!(get(&result, "correction_candidate"), &Value::Null);
    }
    put(&mut req, "fact", text("procedure"));
    let mut procedure = source(
        "reviewed-procedure",
        text("claimed-approved"),
        "reviewed-procedure:current-reference",
    );
    put(&mut procedure, "qualified", Value::Bool(false));
    put(
        &mut req,
        "sources",
        Value::Array(vec![
            procedure,
            source(
                "memory",
                text("claimed-approved"),
                "memory:current-reference",
            ),
        ]),
    );
    let (output, result) = assess(&w, &req);
    assert_eq!(output.status, Status::Code(0));
    assert_eq!(string(&result, "action"), "inspect-current-authority");
}

#[test]
// obligation: tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_invalid_identity_fact_references_and_scope_refuse_and_copied_artifact_is_read_only
fn invalid_identity_fact_references_and_scope_refuse_and_portable_binary_is_readonly() {
    let w = World::new().unwrap();
    let mut original = request();
    put(
        &mut original,
        "sources",
        Value::Array(vec![source(
            "code",
            Value::Int(3.into()),
            "code:current-reference",
        )]),
    );
    let mut variants = Vec::new();
    for (key, value) in [
        ("project_id", text("55555555-5555-4555-8555-555555555555")),
        ("fact_key", text("different-question")),
        ("current", text("yes")),
    ] {
        let mut bad = original.clone();
        let Value::Array(sources) = bad.get_mut("sources").unwrap() else {
            panic!("sources")
        };
        put(&mut sources[0], key, value);
        variants.push(bad);
    }
    for (key, value) in [
        ("version", Value::Bool(true)),
        ("scope", text("personal:me")),
        ("project_id", text("55555555-5555-4555-8555-555555555555")),
    ] {
        let mut bad = original.clone();
        put(bad.get_mut("memory").unwrap(), key, value);
        variants.push(bad);
    }
    let mut bad = original.clone();
    let Value::Array(sources) = bad.get_mut("sources").unwrap() else {
        panic!("sources")
    };
    sources.push(sources[0].clone());
    variants.push(bad);
    for bad in variants {
        let (output, result) = assess(&w, &bad);
        assert_eq!(output.status, Status::Code(1), "{bad:?}");
        assert!(!yes(&result, "writes_performed"));
    }
    let pack = w.inside("portable authority pack").unwrap();
    fs::create_dir(&pack).unwrap();
    let binary = pack.join("am");
    fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
    let output = w
        .run([
            binary.into_os_string(),
            OsString::from("--json"),
            "authority-policy".into(),
            "assess".into(),
            "--input".into(),
            save(&w, &original, "portable.json").into_os_string(),
        ])
        .unwrap();
    let result = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(string(&result, "action"), "propose-correct");
    assert!(!w.paths["config"].join("agent-memory").exists());
    assert!(!w.paths["state"].join("agent-memory").exists());
}
