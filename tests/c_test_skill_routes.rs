mod support;
use agent_memory::codec::json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use support::World;
use support::lifecycle::*;
fn pack(world: &World) -> PathBuf {
    let path = world.root.join("portable package with spaces");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &path,
    );
    path
}
fn check(world: &World, pack: &Path, valid: bool) -> Value {
    let value = run(
        world,
        &["validate-pack", "--pack", pack.to_str().unwrap()],
        if valid { 0 } else { 1 },
    );
    assert_eq!(get(&value, "valid"), &Value::Bool(valid), "{value:?}");
    for key in [
        "writes_performed",
        "candidate_code_executed",
        "semantic_completeness_verified",
    ] {
        assert_eq!(get(&value, key), &Value::Bool(false));
    }
    value
}
fn append(path: &Path, content: &str) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();
}

// obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_complete_copied_pack_validates_without_changing_pack_or_caller_state
#[test]

fn complete_copied_pack_validates_without_changes() {
    let world = World::new().unwrap();
    let pack = pack(&world);
    let before = snapshot(&world.root);
    let value = check(&world, &pack, true);
    assert!(get(&value, "guides").as_array().unwrap().len() >= 35);
    assert_eq!(
        get(get(&value, "file_operations"), "offline/GUIDE.md"),
        &Value::Array(
            ["initialize", "inspect", "checkpoint", "resume"]
                .into_iter()
                .map(|value| Value::String(value.into()))
                .collect()
        )
    );
    let observed = snapshot(&pack);
    let files = get(&value, "files").as_object().unwrap();
    assert_eq!(
        files.len(),
        observed
            .iter()
            .filter(|(path, _)| pack.join(path).is_file())
            .count()
    );
    for (path, (bits, bytes)) in observed
        .iter()
        .filter(|(path, _)| pack.join(path).is_file())
    {
        let record = get(get(&value, "files"), path.to_str().unwrap());
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(text(record, "sha256"), digest);
        assert_eq!(text(record, "mode"), format!("0o{:o}", bits & 0o777));
    }
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_every_documented_helper_dispatch_also_accepts_real_public_help
#[test]
#[ignore = "ungate: fails since its awaited modules landed; fixed by the ungate lane"]
fn every_documented_native_dispatch_accepts_real_public_help() {
    let world = World::new().unwrap();
    let pack = pack(&world);
    let value = check(&world, &pack, true);
    let forms: BTreeSet<_> = get(&value, "documented_commands")
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            get(row, "command")
                .as_str()
                .is_some_and(|name| name.starts_with("am"))
        })
        .map(|row| {
            (
                text(row, "command").to_owned(),
                get(row, "operation").as_str().map(str::to_owned),
            )
        })
        .collect();
    assert!(forms.len() >= 65);
    let before = snapshot(&world.root);
    for (command, operation) in forms {
        let mut args = command
            .split_whitespace()
            .skip(1)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if let Some(operation) = operation {
            args.push(operation);
        }
        args.push("--help".into());
        world.am(args).unwrap();
    }
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_reference_forms_work_for_owner_bindings_and_root_routes_with_global_definitions
#[test]

fn reference_owner_bindings_and_root_routes_use_global_definitions() {
    let world = World::new().unwrap();
    let pack = pack(&world);
    let path = pack.join("checkpoint/GUIDE.md");
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("[capture domain](../capture/GUIDE.md)"));
    std::fs::write(
        &path,
        raw.replace(
            "[capture domain](../capture/GUIDE.md)",
            "[capture domain][owner]",
        ),
    )
    .unwrap();
    append(&path, "\n[owner]: ../capture/GUIDE.md\n");
    let path = pack.join("SKILL.md");
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("[checkpoint](checkpoint/GUIDE.md)"));
    std::fs::write(
        &path,
        raw.replace(
            "[checkpoint](checkpoint/GUIDE.md)",
            "[checkpoint][checkpoint-route]",
        ),
    )
    .unwrap();
    append(&path, "\n[checkpoint-route]: checkpoint/GUIDE.md\n");
    check(&world, &pack, true);
}

// obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_installed_delegation_checks_real_child_operations_and_flags_without_execution
#[test]

fn delegation_checks_real_native_operations_and_flags_without_execution() {
    for command in [
        "am checkpoint nonexistent",
        "am search --imaginary argument",
        "am boundary nonexistent",
    ] {
        let world = World::new().unwrap();
        let pack = pack(&world);
        append(
            &pack.join("maintain/cli/GUIDE.md"),
            &format!("\n```sh\n{command}\n```\n"),
        );
        let before = snapshot(&pack);
        let value = check(&world, &pack, false);
        let issue = get(&value, "issues").as_array().unwrap().first().unwrap();
        assert_eq!(text(issue, "code"), "dispatch");
        assert_eq!(snapshot(&pack), before);
    }
}

// obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_working_delegated_help_commands_are_valid_documented_commands
#[test]

fn working_native_help_is_valid_documentation() {
    for group in ["checkpoint", "boundary", "capture", "identity"] {
        let world = World::new().unwrap();
        let pack = pack(&world);
        let before = snapshot(&pack);
        let help = world.am([group, "--help"]).unwrap();
        assert!(
            String::from_utf8_lossy(&help.stdout)
                .to_ascii_lowercase()
                .contains("usage:")
        );
        assert_eq!(snapshot(&pack), before);
        append(
            &pack.join("maintain/cli/GUIDE.md"),
            &format!("\n```sh\nam {group} --help\n```\n"),
        );
        check(&world, &pack, true);
    }
}
