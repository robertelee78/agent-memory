#![cfg(feature = "dev-tools")]
mod support;
use agent_memory::codec::json::{self, Value};
use std::path::{Path, PathBuf};
use support::lifecycle::{copy_tree, get, save, text};
use support::{RunOptions, World};

fn call(world: &World, args: &[&str], expected: i32) -> Value {
    let output = world
        .run_with(
            std::iter::once(env!("CARGO_BIN_EXE_am-dev")).chain(args.iter().copied()),
            RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(expected),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    json::decode_bytes(&output.stdout, Default::default()).unwrap()
}
fn snapshot(world: &World, pack: &Path) -> Value {
    call(
        world,
        &[
            "guide-evidence",
            "snapshot",
            "--pack",
            pack.to_str().unwrap(),
        ],
        0,
    )
}
fn check(
    world: &World,
    old: &Path,
    new: &Path,
    before: &Value,
    mapping: Option<Value>,
    expected: i32,
) -> Value {
    let input = world.root.join("evidence.json");
    save(&input, before);
    let mut args = vec![
        "guide-evidence",
        "check",
        "--before",
        input.to_str().unwrap(),
        "--before-pack",
        old.to_str().unwrap(),
        "--pack",
        new.to_str().unwrap(),
    ];
    let path = world.root.join("map.json");
    if let Some(mapping) = mapping {
        save(&path, &mapping);
        args.extend(["--map", path.to_str().unwrap()]);
    }
    call(world, &args, expected)
}
fn fixture(world: &World) -> (PathBuf, PathBuf, Value) {
    let old = world.root.join("old-pack");
    std::fs::create_dir_all(old.join("rules")).unwrap();
    std::fs::write(
        old.join("SKILL.md"),
        b"# Example skill\n\nOpen [the rules](rules/GUIDE.md).\n",
    )
    .unwrap();
    std::fs::write(old.join("rules/GUIDE.md"), b"# Rules\n\nA session MUST preserve the original evidence.\n\nA session MUST NOT guess another developer's answer.\n\n```text\nThis example MAY be ignored by the normative prose collector.\n```\n").unwrap();
    let new = world.root.join("new-pack");
    copy_tree(&old, &new);
    let before = snapshot(world, &old);
    (old, new, before)
}
fn rule<'a>(snapshot: &'a Value, needle: &str) -> (&'a str, &'a Value) {
    let (key, rule) = get(snapshot, "rules")
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, rule)| text(rule, "text").contains(needle))
        .unwrap();
    (key, rule)
}
fn mapping(old: &str, new: &str) -> Value {
    Value::Object(vec![(old.into(), Value::Object(vec![("new_rules".into(), Value::Array(vec![Value::String(new.into())])), ("meaning_preserved".into(), Value::Bool(true)), ("reason".into(), Value::String("An explicit independent review assertion cannot override force or polarity checks.".into()))]))])
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_real_runtime_guides_are_reachable_and_an_unchanged_tree_preserves_every_rule
#[test]
fn real_runtime_guides_are_reachable_and_unchanged_rules_preserve_force() {
    let world = World::new().unwrap();
    let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
    let before = snapshot(&world, &pack);
    assert!(get(&before, "rules").as_object().unwrap().len() > 100);
    let value = check(&world, &pack, &pack, &before, None, 0);
    assert_eq!(
        get(&value, "rule_map").as_array().unwrap().len(),
        get(&before, "rules").as_object().unwrap().len()
    );
    assert_eq!(get(&value, "passed"), &Value::Bool(true));
    assert_eq!(
        get(&value, "semantic_equivalence_verified"),
        &Value::Bool(false)
    );
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_moved_identical_rules_and_explicit_wording_map_keep_equal_force
#[test]
fn moved_identical_rules_and_reviewed_wording_keep_equal_force() {
    let world = World::new().unwrap();
    let (old, new, before) = fixture(&world);
    std::fs::rename(new.join("rules"), new.join("renamed")).unwrap();
    std::fs::write(
        new.join("SKILL.md"),
        b"# Example skill\n\nOpen [the rules](renamed/GUIDE.md).\n",
    )
    .unwrap();
    let value = check(&world, &old, &new, &before, None, 0);
    assert!(
        get(&value, "rule_map")
            .as_array()
            .unwrap()
            .iter()
            .all(|row| text(row, "method") == "identical-text")
    );
    let path = new.join("renamed/GUIDE.md");
    let raw = std::fs::read_to_string(&path).unwrap().replace(
        "preserve the original evidence",
        "retain every original source and its evidence",
    );
    std::fs::write(path, raw).unwrap();
    let after = snapshot(&world, &new);
    let mapping = mapping(
        rule(&before, "preserve the original").0,
        rule(&after, "retain every original").0,
    );
    let value = check(&world, &old, &new, &before, Some(mapping), 0);
    assert_eq!(get(&value, "rule_map").as_array().unwrap().len(), 2);
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_removed_weakened_inverted_and_unreachable_instructions_fail_even_with_a_mapping
#[test]
fn removed_weakened_inverted_unreachable_or_substituted_rules_refuse() {
    for mutation in [
        "remove",
        "weaken",
        "invert",
        "unreachable",
        "lie-about-before",
    ] {
        let world = World::new().unwrap();
        let (old, new, mut before) = fixture(&world);
        let path = new.join("rules/GUIDE.md");
        let original = std::fs::read_to_string(&path).unwrap();
        let changed = match mutation {
            "remove" => original.replace("A session MUST preserve the original evidence.", ""),
            "weaken" => original.replace("MUST preserve", "SHOULD preserve"),
            "invert" => original.replace("MUST preserve", "MUST NOT preserve"),
            "unreachable" => {
                std::fs::write(
                    new.join("SKILL.md"),
                    b"# Example skill\n\nNo rule route remains.\n",
                )
                .unwrap();
                original
            }
            _ => {
                if let Value::Object(fields) = &mut before {
                    let (_, rules) = fields.iter_mut().find(|(key, _)| key == "rules").unwrap();
                    if let Value::Object(rules) = rules {
                        rules.remove(0);
                    }
                }
                original
            }
        };
        std::fs::write(path, changed).unwrap();
        let mapping = if ["weaken", "invert"].contains(&mutation) {
            let after = snapshot(&world, &new);
            Some(mapping(
                rule(&before, "preserve").0,
                rule(&after, "preserve").0,
            ))
        } else {
            None
        };
        let result = check(&world, &old, &new, &before, mapping, 1);
        assert_eq!(get(&result, "passed"), &Value::Bool(false));
    }
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_shorter_literal_fences_cannot_hide_a_real_rule_between_longer_examples
#[test]
fn shorter_literal_fences_do_not_hide_rules_between_long_examples() {
    for ch in ['`', '~'] {
        let world = World::new().unwrap();
        let old = world.root.join("old-pack");
        let new = world.root.join("new-pack");
        std::fs::create_dir(&old).unwrap();
        let literal = ch.to_string().repeat(3);
        let outer = ch.to_string().repeat(4);
        let raw = format!(
            "# Fence documentation\n\nFirst example:\n\n{outer}markdown\n{literal}\n{outer}\n\nA session MUST preserve the original source evidence.\n\nSecond example:\n\n{outer}markdown\n{literal}\n{outer}\n\nA session MUST retain unrelated working files.\n"
        );
        std::fs::write(old.join("SKILL.md"), &raw).unwrap();
        copy_tree(&old, &new);
        std::fs::write(
            new.join("SKILL.md"),
            raw.replace(
                "A session MUST preserve the original source evidence.\n",
                "",
            ),
        )
        .unwrap();
        let before = snapshot(&world, &old);
        assert_eq!(get(&before, "rules").as_object().unwrap().len(), 2);
        assert_eq!(
            get(&check(&world, &old, &new, &before, None, 1), "passed"),
            &Value::Bool(false)
        );
    }
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_existing_runtime_wrapped_must_not_cannot_invert_to_a_positive_obligation
#[test]
fn existing_wrapped_prohibition_cannot_be_mapped_to_positive_force() {
    let world = World::new().unwrap();
    let old = world.root.join("old-pack");
    let new = world.root.join("new-pack");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &old,
    );
    copy_tree(&old, &new);
    let path = new.join("consolidation/GUIDE.md");
    let needle = "The helper MUST\nNOT change unrelated source files, the Git index, HEAD or feature ancestry.";
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains(needle));
    std::fs::write(path, raw.replace(needle, &needle.replace("\nNOT ", "\n"))).unwrap();
    let before = snapshot(&world, &old);
    let after = snapshot(&world, &new);
    assert!(
        get(rule(&before, needle).1, "force")
            .as_array()
            .unwrap()
            .contains(&Value::Array(vec![
                Value::String("negative".into()),
                Value::Int(3.into())
            ]))
    );
    let mapping = mapping(
        rule(&before, needle).0,
        rule(&after, "The helper MUST\nchange unrelated source files").0,
    );
    assert_eq!(
        get(
            &check(&world, &old, &new, &before, Some(mapping), 1),
            "passed"
        ),
        &Value::Bool(false)
    );
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_examples_are_actual_public_exit_results_bound_to_the_complete_pack
#[test]
fn guide_examples_are_actual_native_exit_results_bound_to_complete_pack() {
    let world = World::new().unwrap();
    let value = call(
        &world,
        &["capture-guide-examples", "--am", env!("CARGO_BIN_EXE_am")],
        0,
    );
    let examples = get(&value, "examples").as_array().unwrap();
    assert_eq!(
        examples
            .iter()
            .map(|row| get(row, "exit").clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Int(0.into()),
            Value::Int(1.into()),
            Value::Int(2.into())
        ]
    );
    assert_eq!(
        get(&examples[0], "observed_file_paths"),
        &Value::Array(vec![
            Value::String("MEMORY.md".into()),
            Value::String("projectbrief.md".into())
        ])
    );
    assert_eq!(
        text(get(&examples[1], "observed_fields"), "error"),
        "Render input needs project-id, router-updated, contexts and omitted"
    );
    assert!(text(&examples[2], "observed_stderr").contains("unknown-operation"));
    let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
    for (path, digest) in get(&value, "artifact_files").as_object().unwrap() {
        use sha2::{Digest, Sha256};
        let actual: String = Sha256::digest(std::fs::read(pack.join(path)).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest.as_str(), Some(actual.as_str()));
    }
    assert_eq!(
        get(&value, "live_credentials_required"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(&value, "live_configuration_changed"),
        &Value::Bool(false)
    );
}

// obligation: tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_actual_incident_runner_exercises_both_user_visible_public_regressions
#[test]
#[ignore = "needs recovery_frontier registered and the retained incident native tests enabled"]
fn native_incident_runner_executes_both_retained_public_regressions() {
    let world = World::new().unwrap();
    let catalog = support::lifecycle::read(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/incidents.json"),
    );
    let incidents = get(&catalog, "incidents").as_array().unwrap();
    assert_eq!(incidents.len(), 2);
    for incident in incidents {
        assert_eq!(text(get(incident, "delivery"), "kind"), "merge");
        assert_eq!(
            get(get(incident, "acceptance"), "candidate"),
            get(incident, "corrected_candidate")
        );
        assert_eq!(
            get(get(incident, "integration"), "candidate"),
            get(incident, "corrected_candidate")
        );
        assert_eq!(
            get(get(incident, "delivery"), "commit"),
            get(get(incident, "integration"), "commit")
        );
        let value = call(&world, &["run-incident", text(incident, "id")], 0);
        assert_eq!(get(&value, "passed"), &Value::Bool(true));
        assert_eq!(get(&value, "work_key"), get(incident, "work_key"));
        assert_eq!(
            get(&value, "tests"),
            &Value::Int(get(incident, "tests").as_array().unwrap().len().into())
        );
        assert_eq!(
            get(&value, "live_credentials_required"),
            &Value::Bool(false)
        );
    }
}
