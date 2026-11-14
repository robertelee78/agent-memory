mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use support::cutover::*;
use support::{RunOptions, World};

fn route(path: &str, scope: &str) -> Value {
    object([
        ("path", text(path)),
        ("purpose", text("Useful fixture context")),
        ("scope", text(scope)),
        ("lifecycle", text("active")),
        ("read-when", text("When examining the fixture component")),
    ])
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_bank_layout.py::LayoutContract.test_copied_artifact_preserves_index_and_code_and_refuses_busy_or_interrupted_banks
fn relocated_binary_preserves_nested_bank_and_unknown_interruption_evidence() {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    let mut files = render(
        &[(
            "projectbrief.md",
            "# Project brief\n\nGoals and boundaries: [project source](../README.md).\n",
        )],
        "2026-10-22T17:00:00Z",
    );
    let raw = support::record_fixtures::record_text_with(
        "# Conditional result\n\nRetain the original bounded probe.\n",
        PROJECT,
        support::record_fixtures::RecordOptions {
            scope: "component:src/retry",
            kind: "result",
            ..Default::default()
        },
    )
    .unwrap();
    let (_, mut metadata, body) = agent_memory::records::parse(&raw, &[]).unwrap();
    put(
        &mut metadata,
        "conditions",
        object([("branch", text("feature/retry"))]),
    );
    put(
        &mut files,
        "episodes/retry notes.md",
        text(
            agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap() + &body,
        ),
    );
    for (path, child) in [
        ("topics/retries/index.md", "episodes/retry notes.md"),
        ("topics/index.md", "topics/retries/index.md"),
    ] {
        let index = agent_memory::bank_layout::render_index(&object([
            ("path", text(path)),
            ("project-id", text(PROJECT)),
            ("updated", text("2026-10-22T22:00:00Z")),
            (
                "routes",
                Value::Array(vec![route(child, "component:src/retry")]),
            ),
        ]))
        .unwrap();
        put(&mut files, path, get(&index, "content").clone());
    }
    let row = "| [topics/index.md](<topics/index.md>) | Useful fixture context | branch:feature/retry | active | When examining the fixture component |\n";
    let router = string(&files, "MEMORY.md").replace(
        "\n## Omitted contexts",
        &format!("{row}\n## Omitted contexts"),
    );
    put(&mut files, "MEMORY.md", text(router));
    let bank = repo.join("memory-bank");
    fs::create_dir(&bank).unwrap();
    write_files(&bank, &files);
    let pack = w.inside("copied layout pack").unwrap();
    fs::create_dir(&pack).unwrap();
    let binary = pack.join("am");
    fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
    let invoke = || {
        w.run_with(
            [
                binary.as_os_str().to_owned(),
                "--json".into(),
                "bank-layout".into(),
                "inspect".into(),
                "--repo".into(),
                repo.as_os_str().to_owned(),
            ],
            RunOptions {
                check: false,
                ..RunOptions::default()
            },
        )
        .unwrap()
    };
    let output = invoke();
    assert_eq!(output.status, Status::Code(0));
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert!(yes(&value, "valid"));
    for name in [".agent-memory.lock", MARKER] {
        let marker = bank.join(name);
        fs::write(&marker, "Preserve unknown interruption evidence\n").unwrap();
        assert_eq!(invoke().status, Status::Code(1));
        assert_eq!(
            fs::read_to_string(&marker).unwrap(),
            "Preserve unknown interruption evidence\n"
        );
        fs::remove_file(marker).unwrap();
    }
    same_map(&canonical(&bank), &files);
}
