mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use support::World;
use support::cutover::*;

fn fixture(w: &World) -> PathBuf {
    let bank = w.paths["repo"].join("memory-bank");
    fs::create_dir(&bank).unwrap();
    write_files(
        &bank,
        &render(
            &[(
                "projectbrief.md",
                "# projectbrief.md\n\nUseful project context; see [source](../README.md).\n",
            )],
            "2026-10-22T17:00:00Z",
        ),
    );
    bank
}
fn validate(w: &World) -> Value {
    good(
        w,
        "bank_contract.py",
        &[
            "validate".into(),
            "--repo".into(),
            w.paths["repo"].as_os_str().to_owned(),
        ],
    )
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_router.py::RouterContract.test_minimal_bank_resumes_from_only_canonical_files
fn minimal_bank_resumes_from_canonical_files_only() {
    let w = World::new().unwrap();
    let bank = fixture(&w);
    for name in ["state", "cache", "backups"] {
        fs::write(w.paths[name].join("disposable"), "not authoritative").unwrap();
        fs::remove_dir_all(&w.paths[name]).unwrap();
    }
    let before = physical(&bank);
    let receipt = validate(&w);
    assert_eq!(string(&receipt, "project_id"), PROJECT);
    let routes = get(&receipt, "routes").as_array().unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(string(&routes[0], "path"), "projectbrief.md");
    let omitted: std::collections::BTreeSet<_> = get(&receipt, "omitted")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        omitted,
        agent_memory::bank_contract::CONTEXTS
            .iter()
            .copied()
            .filter(|p| *p != "projectbrief.md")
            .collect()
    );
    assert!(
        fs::read_to_string(bank.join("projectbrief.md"))
            .unwrap()
            .contains("[source](../README.md)")
    );
    assert_eq!(physical(&bank), before);
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_router.py::RouterContract.test_paths_cannot_route_runtime_state_outside_bank_or_personal_overlays
fn router_never_routes_runtime_outside_or_personal_paths() {
    let w = World::new().unwrap();
    let bank = fixture(&w);
    let router = bank.join("MEMORY.md");
    let original = fs::read_to_string(&router).unwrap();
    for path in [
        "../state.md",
        "/tmp/state.md",
        ".agent-memory-transaction.md",
        "personal/profile.md",
        "topics/../../state.md",
        "topics//state.md",
        "topics/secret%2fstate.md",
        "topics/.runtime.md",
    ] {
        fs::write(
            &router,
            original.replace(
                "[projectbrief.md](<projectbrief.md>)",
                &format!("[{path}](<{path}>)"),
            ),
        )
        .unwrap();
        refused(
            &w,
            "bank_contract.py",
            &[
                "validate".into(),
                "--repo".into(),
                w.paths["repo"].as_os_str().to_owned(),
            ],
        );
    }
    fs::write(router, original).unwrap();
    fs::remove_file(bank.join("projectbrief.md")).unwrap();
    let outside = w.inside("outside.md").unwrap();
    fs::write(&outside, "outside bank").unwrap();
    symlink(&outside, bank.join("projectbrief.md")).unwrap();
    refused(
        &w,
        "bank_contract.py",
        &[
            "validate".into(),
            "--repo".into(),
            w.paths["repo"].as_os_str().to_owned(),
        ],
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "outside bank");
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_router.py::RouterContract.test_optional_topic_is_readable_and_runtime_state_is_not_routed
fn optional_topic_is_readable_but_runtime_is_not_routed() {
    let w = World::new().unwrap();
    let bank = fixture(&w);
    fs::create_dir(bank.join("topics")).unwrap();
    let body = support::record_fixtures::record_text_with(
        "# Design\n\nA conditional lesson; [ADR](../../docs/adr.md).\n",
        PROJECT,
        support::record_fixtures::RecordOptions {
            scope: "branch:feature",
            ..Default::default()
        },
    )
    .unwrap();
    fs::write(bank.join("topics/design notes.md"), body).unwrap();
    let router = bank.join("MEMORY.md");
    let row = "| [topics/design notes.md](<topics/design notes.md>) | Design context | branch:feature | active | When editing the feature |\n";
    fs::write(
        &router,
        fs::read_to_string(&router).unwrap().replace(
            "\n## Omitted contexts",
            &format!("\n{row}\n## Omitted contexts"),
        ),
    )
    .unwrap();
    let receipt = validate(&w);
    assert_eq!(
        string(
            get(&receipt, "routes").as_array().unwrap().last().unwrap(),
            "scope"
        ),
        "branch:feature"
    );
    fs::write(bank.join(".agent-memory.lock"), "writer").unwrap();
    refused(
        &w,
        "bank_contract.py",
        &[
            "validate".into(),
            "--repo".into(),
            w.paths["repo"].as_os_str().to_owned(),
        ],
    );
    fs::remove_file(bank.join(".agent-memory.lock")).unwrap();
    fs::write(bank.join(MARKER), "interrupted").unwrap();
    refused(
        &w,
        "bank_contract.py",
        &[
            "validate".into(),
            "--repo".into(),
            w.paths["repo"].as_os_str().to_owned(),
        ],
    );
    assert_eq!(
        fs::read_to_string(bank.join(MARKER)).unwrap(),
        "interrupted"
    );
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_router.py::RouterContract.test_portable_helper_works_after_copy_outside_checkout
fn relocated_native_binary_reads_the_same_router() {
    let w = World::new().unwrap();
    fixture(&w);
    let directory = w.inside("portable pack").unwrap();
    fs::create_dir(&directory).unwrap();
    let binary = directory.join("am");
    fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
    let output = w
        .run([
            binary.into_os_string(),
            "--json".into(),
            "bank".into(),
            "validate".into(),
            "--repo".into(),
            w.paths["repo"].as_os_str().to_owned(),
        ])
        .unwrap();
    assert_eq!(output.status, Status::Code(0));
    let receipt = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert!(yes(&receipt, "valid"));
}
