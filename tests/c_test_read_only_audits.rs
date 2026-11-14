mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use support::World;
use support::cutover::*;

#[test]
fn cutover_dispatch_worker() {
    dispatch_worker();
}
fn snapshot(root: &std::path::Path) -> BTreeMap<String, (Vec<u8>, u32)> {
    physical(root)
        .into_iter()
        .map(|(p, raw)| {
            let mode = fs::symlink_metadata(root.join(&p))
                .unwrap()
                .permissions()
                .mode();
            (p, (raw, mode))
        })
        .collect()
}

#[test]
// obligation: tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_direct_hook_and_boundary_helpers_do_not_create_source_caches
fn direct_native_hooks_and_boundary_help_preserve_runtime_bytes_and_modes() {
    let w = World::new().unwrap();
    let source = development_source(&w, "direct-pack");
    let repo = w.repository("repo", false).unwrap();
    let before = snapshot(&source);
    let event = object([
        ("hook_event_name", text("UserPromptSubmit")),
        ("session_id", text("11111111-1111-4111-8111-111111111111")),
        ("turn_id", text("22222222-2222-4222-8222-222222222222")),
        ("cwd", text(repo.to_str().unwrap())),
        ("prompt", text("fixture input")),
    ]);
    let (code, stdout, stderr) =
        dispatch(&w, &["hook".into(), "codex-prompt".into()], Some(&event));
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(
        agent_memory::codec::json::decode(&stdout, Default::default()).unwrap(),
        object(Vec::<(String, Value)>::new())
    );
    for route in ["codex-boundary", "claude-boundary", "private"] {
        let output = w
            .run([
                source.join("target/release/am").into_os_string(),
                route.into(),
                "--help".into(),
            ])
            .unwrap();
        assert_eq!(output.status, Status::Code(0));
    }
    assert_eq!(snapshot(&source), before);
    assert!(
        !before
            .keys()
            .any(|p| p.contains("__pycache__") || p.ends_with(".pyc"))
    );
}

#[test]

// obligation: tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_repeated_installed_and_documented_source_audits_preserve_every_byte_and_mode
fn repeated_installed_and_source_audits_preserve_all_runtime_bytes_and_modes() {
    for mode in ["copy", "link"] {
        let w = World::new().unwrap();
        let source = development_source(&w, "audit-source");
        let installed = good(
            &w,
            "installer.py",
            &[
                "install".into(),
                "--development-source".into(),
                source.as_os_str().to_owned(),
                "--presets".into(),
                "none".into(),
                "--mode".into(),
                mode.into(),
            ],
        );
        let receipt = get(&installed, "receipt");
        let cli = std::path::Path::new(string(receipt, "bin_dir")).join("am");
        let before = snapshot(&source);
        let owned = snapshot(cli.parent().unwrap());
        for _ in 0..2 {
            for route in ["version", "status", "doctor"] {
                let output = w
                    .run_with(
                        [cli.as_os_str().to_owned(), "--json".into(), route.into()],
                        support::RunOptions {
                            check: false,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                assert_eq!(
                    output.status,
                    Status::Code(0),
                    "{}",
                    String::from_utf8_lossy(&output.stdout)
                );
            }
        }
        assert_eq!(snapshot(&source), before);
        assert_eq!(snapshot(cli.parent().unwrap()), owned);
    }
}

#[test]
// obligation: tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_outside_receipt_aware_audit_refuses_and_preserves_inert_cache_drift
fn outside_receipt_aware_audit_refuses_and_preserves_opaque_cache_drift() {
    let w = World::new().unwrap();
    let source = development_source(&w, "audit-source");
    let installed = good(
        &w,
        "installer.py",
        &[
            "install".into(),
            "--development-source".into(),
            source.as_os_str().to_owned(),
            "--presets".into(),
            "codex".into(),
            "--mode".into(),
            "copy".into(),
        ],
    );
    let receipt = get(&installed, "receipt");
    let copy = get(receipt, "targets")
        .as_array()
        .unwrap()
        .iter()
        .find(|target| string(target, "kind") == "copy")
        .unwrap();
    let cache = std::path::Path::new(string(copy, "path"))
        .join("scripts/agent_memory/__pycache__/unexpected.cpython-311.pyc");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, b"INERT_UNEXPECTED_CACHE_BYTES_DO_NOT_EXECUTE\n").unwrap();
    let before = snapshot(&w.root);
    let cli = std::path::Path::new(string(receipt, "bin_dir")).join("am");
    let output = w
        .run_with(
            [cli.into_os_string(), "--json".into(), "doctor".into()],
            support::RunOptions {
                check: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, Status::Code(1));
    let diagnostic = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert_eq!(get(&diagnostic, "writes_performed"), &Value::Bool(false));
    assert!(
        get(&diagnostic, "failures")
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| string(failure, "component") == "runtime.target.copy")
    );
    assert_eq!(snapshot(&w.root), before);
    assert_eq!(
        fs::read(cache).unwrap(),
        b"INERT_UNEXPECTED_CACHE_BYTES_DO_NOT_EXECUTE\n"
    );
}

#[test]
// obligation: tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_bytecode_prefix_keeps_isolation_dispatch_and_shell_restrictions
fn retired_python_launchers_and_shell_injection_are_refused_without_execution() {
    for command in [
        "python3 -B \"$SKILL_DIR/scripts/checkpoint.py\" inspect",
        "python3 -B -I \"$SKILL_DIR/scripts/checkpoint.py\" nonexistent",
        "python3 -B -I \"$SKILL_DIR/scripts/checkpoint.py\" inspect; touch /tmp/never-execute",
        "am checkpoint inspect; touch /tmp/never-execute",
    ] {
        let w = World::new().unwrap();
        let source = development_source(&w, "candidate");
        let guide = source.join("skills/agent-memory/checkpoint/GUIDE.md");
        fs::write(
            &guide,
            fs::read_to_string(&guide).unwrap() + &format!("\n```sh\n{command}\n```\n"),
        )
        .unwrap();
        let before = snapshot(&w.root);
        let output = w
            .am_with(
                [
                    std::ffi::OsString::from("--json"),
                    "validate-pack".into(),
                    "--pack".into(),
                    source.join("skills/agent-memory").into_os_string(),
                ],
                support::RunOptions {
                    check: false,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_ne!(output.status, Status::Code(0), "{command}");
        let value =
            agent_memory::codec::json::decode_bytes(&output.stdout, Default::default()).unwrap();
        assert!(!yes(&value, "valid"));
        assert_eq!(snapshot(&w.root), before);
    }
}
