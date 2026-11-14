use super::lifecycle::*;
use super::{RunOptions, World};
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
fn fixture(world: &World) -> (PathBuf, Value, Vec<String>) {
    let memory = bank(world, &world.paths["repo"]);
    std::fs::hard_link(memory.join("projectbrief.md"), memory.join("alias.md")).unwrap();
    let generated = memory.join(".agent-memory-cache");
    std::fs::create_dir(&generated).unwrap();
    std::fs::write(generated.join("references.json"), b"{\"derived\":true}\n").unwrap();
    let preview = get(
        &run(
            world,
            &[
                "purge-data",
                "--repo",
                world.paths["repo"].to_str().unwrap(),
                "--path",
                memory.to_str().unwrap(),
                "--check",
            ],
            0,
        ),
        "preview",
    )
    .clone();
    let args = vec![
        "purge-data".into(),
        "--repo".into(),
        world.paths["repo"].to_string_lossy().into_owned(),
        "--path".into(),
        memory.to_string_lossy().into_owned(),
        "--confirm".into(),
        text(&preview, "confirmation").into(),
    ];
    (memory, preview, args)
}
fn invoke(
    world: &World,
    args: &[String],
    expected: i32,
    trace: &Path,
    point: Option<&str>,
) -> Option<Value> {
    let mut overrides = vec![(
        OsString::from("AGENT_MEMORY_FAULT_TRACE"),
        trace.as_os_str().to_owned(),
    )];
    if let Some(point) = point {
        overrides.push(("AGENT_MEMORY_FAULT_STAGE".into(), point.into()));
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
        Status::Code(expected),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    (!output.stdout.is_empty())
        .then(|| json::decode_bytes(&output.stdout, Default::default()).unwrap())
}
fn rows(trace: &Path) -> Vec<Value> {
    std::fs::read_to_string(trace)
        .unwrap()
        .lines()
        .map(|line| json::decode(line, Default::default()).unwrap())
        .collect()
}
pub fn replay(phase: &str) {
    let world = World::new().unwrap();
    let (memory, _, args) = fixture(&world);
    let trace = world.root.join("clean-trace.jsonl");
    let clean = invoke(&world, &args, 0, &trace, None).unwrap();
    assert_eq!(get(&clean, "verified"), &Value::Bool(true));
    assert!(!memory.exists());
    let rows = rows(&trace);
    let kinds: BTreeSet<_> = rows.iter().map(|row| text(row, "kind")).collect();
    for kind in [
        "purge-entry-remove",
        "purge-lock-remove",
        "purge-root-remove",
        "purge-bank-lock",
        "record-publication",
        "atomic-node-switch",
    ] {
        assert!(kinds.contains(kind), "{kind}: {kinds:?}");
    }
    let selected = rows
        .iter()
        .filter(|row| text(row, "phase") == phase)
        .collect::<Vec<_>>();
    assert!(!selected.is_empty());
    let mut executed = Vec::new();
    for event in selected {
        let world = World::new().unwrap();
        let (memory, preview, args) = fixture(&world);
        let retained = [
            world.paths["repo"].join(".git"),
            world.paths["config"].clone(),
            world.paths["data"].clone(),
            world.paths["home"].clone(),
            world.paths["backups"].clone(),
        ]
        .into_iter()
        .map(|path| {
            let before = snapshot(&path);
            (path, before)
        })
        .collect::<Vec<_>>();
        let trace = world.root.join("actual-trace.jsonl");
        invoke(&world, &args, 86, &trace, Some(text(event, "point")));
        assert_eq!(rows_from(&trace).last(), Some(event));
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let completed = run(&world, &refs, 0);
        assert_eq!(get(&completed, "verified"), &Value::Bool(true));
        let receipt = get(&completed, "receipt");
        assert_eq!(get(receipt, "preview"), &preview);
        assert_eq!(
            get(receipt, "status"),
            get(get(&clean, "receipt"), "status")
        );
        assert_eq!(
            get(receipt, "deletion_readback_verified"),
            &Value::Bool(true)
        );
        assert!(!memory.exists());
        assert!(!Path::new(text(receipt, "stage")).exists());
        assert_eq!(
            get(&completed, "replacement_bank_present"),
            &Value::Bool(false)
        );
        assert_eq!(get(&run(&world, &refs, 0), "changed"), &Value::Bool(false));
        for (path, before) in retained {
            assert_eq!(snapshot(&path), before);
        }
        executed.push(text(event, "point").to_owned());
    }
    assert_eq!(
        executed.len(),
        rows.iter()
            .filter(|row| text(row, "phase") == phase)
            .count()
    );
}
fn rows_from(path: &Path) -> Vec<Value> {
    rows(path)
}
pub fn unlock(phase: &str) {
    let world = World::new().unwrap();
    let (memory, preview, mut args) = fixture(&world);
    let invalid = "0".repeat(64);
    assert_ne!(text(&preview, "confirmation"), invalid);
    *args.last_mut().unwrap() = invalid;
    let before = snapshot(&memory);
    let trace = world.root.join("refusal.jsonl");
    let refused = invoke(&world, &args, 1, &trace, None).unwrap();
    assert_eq!(get(&refused, "verified"), &Value::Bool(false));
    assert_eq!(snapshot(&memory), before);
    let events = rows(&trace)
        .into_iter()
        .filter(|row| text(row, "kind") == "purge-bank-unlock" && text(row, "phase") == phase)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    for event in events {
        let world = World::new().unwrap();
        let (memory, _, mut args) = fixture(&world);
        *args.last_mut().unwrap() = "0".repeat(64);
        let before = snapshot(&memory);
        let trace = world.root.join("actual.jsonl");
        invoke(&world, &args, 86, &trace, Some(text(&event, "point")));
        assert_eq!(rows(&trace).last(), Some(&event));
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(get(&run(&world, &refs, 1), "verified"), &Value::Bool(false));
        assert_eq!(snapshot(&memory), before);
        assert_eq!(
            get(
                &run(
                    &world,
                    &[
                        "bank",
                        "validate",
                        "--repo",
                        world.paths["repo"].to_str().unwrap()
                    ],
                    0
                ),
                "valid"
            ),
            &Value::Bool(true)
        );
    }
}
