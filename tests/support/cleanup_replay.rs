use super::lifecycle::*;
use super::{RunOptions, World};
use agent_memory::codec::json::{self, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
fn fixture(world: &World) -> (PathBuf, Vec<String>, PathBuf) {
    let source = source(world);
    let memory = world.paths["repo"].join("memory-bank");
    std::fs::create_dir(&memory).unwrap();
    std::fs::write(memory.join("note.md"), b"Preserve durable bank data.\n").unwrap();
    let args = vec![
        "install".into(),
        "--development-source".into(),
        source.to_string_lossy().into_owned(),
        "--presets".into(),
        "codex".into(),
        "--mode".into(),
        "copy".into(),
    ];
    (source, args, memory)
}
fn invoke(
    world: &World,
    args: &[String],
    trace: &Path,
    error: Option<&str>,
    stop: Option<&str>,
    expected: i32,
) -> Option<Value> {
    let mut overrides = vec![(
        OsString::from("AGENT_MEMORY_FAULT_TRACE"),
        trace.as_os_str().to_owned(),
    )];
    if let Some(error) = error {
        overrides.push(("AGENT_MEMORY_FAULT_ERROR_STAGE".into(), error.into()));
    }
    if let Some(stop) = stop {
        overrides.push(("AGENT_MEMORY_FAULT_STAGE".into(), stop.into()));
        overrides.push(("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()));
    }
    let output = world
        .am_with(
            std::iter::once("--json").chain(args.iter().map(String::as_str)),
            RunOptions {
                env: &overrides,
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(expected),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    (!output.stdout.is_empty())
        .then(|| json::decode_bytes(&output.stdout, Default::default()).unwrap())
}
fn rows(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| json::decode(line, Default::default()).unwrap())
        .collect()
}
pub fn replay(phase: &str) {
    let world = World::new().unwrap();
    let (_, args, _) = fixture(&world);
    let trace = world.root.join("success.jsonl");
    let clean = invoke(&world, &args, &trace, None, None, 0).unwrap();
    let observations = rows(&trace);
    let publication = observations
        .iter()
        .find(|row| text(row, "kind") == "record-publication")
        .unwrap();
    let step = get(publication, "step").as_int().unwrap().as_u64().unwrap();
    let anchor = observations
        .iter()
        .find(|row| {
            get(row, "step").as_int().unwrap().as_u64() == Some(step - 1)
                && text(row, "phase") == "after"
        })
        .unwrap();
    assert_eq!(text(anchor, "kind"), "file-stage");
    let anchor = text(anchor, "point").to_owned();
    let failed_world = World::new().unwrap();
    let (_, failed_args, _) = fixture(&failed_world);
    let trace = failed_world.root.join("error.jsonl");
    let failed = invoke(&failed_world, &failed_args, &trace, Some(&anchor), None, 1).unwrap();
    assert_eq!(get(&failed, "verified"), &Value::Bool(false));
    let selected = rows(&trace)
        .into_iter()
        .filter(|row| text(row, "kind") == "stage-cleanup" && text(row, "phase") == phase)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    for event in selected {
        let world = World::new().unwrap();
        let (_, args, memory) = fixture(&world);
        let before = snapshot(&memory);
        let trace = world.root.join("actual.jsonl");
        invoke(
            &world,
            &args,
            &trace,
            Some(&anchor),
            Some(text(&event, "point")),
            86,
        );
        assert_eq!(rows(&trace).last(), Some(&event));
        assert_eq!(
            get(&run(&world, &["recover"], 0), "verified"),
            &Value::Bool(true)
        );
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let retry = run(&world, &refs, 0);
        assert_eq!(get(&retry, "verified"), &Value::Bool(true));
        let receipt = get(&retry, "receipt");
        let expected = get(&clean, "receipt");
        assert_eq!(
            get(get(receipt, "source"), "artifact_digest"),
            get(get(expected, "source"), "artifact_digest")
        );
        assert_eq!(get(receipt, "mode"), get(expected, "mode"));
        assert_eq!(get(receipt, "presets"), get(expected, "presets"));
        assert_eq!(
            get(receipt, "targets")
                .as_array()
                .unwrap()
                .iter()
                .map(|row| text(row, "kind"))
                .collect::<Vec<_>>(),
            get(expected, "targets")
                .as_array()
                .unwrap()
                .iter()
                .map(|row| text(row, "kind"))
                .collect::<Vec<_>>()
        );
        let stable = std::fs::read(state(&world).join("receipt.json")).unwrap();
        assert_eq!(get(&run(&world, &refs, 0), "verified"), &Value::Bool(true));
        assert_eq!(
            std::fs::read(state(&world).join("receipt.json")).unwrap(),
            stable
        );
        assert_eq!(snapshot(&memory), before);
    }
}
