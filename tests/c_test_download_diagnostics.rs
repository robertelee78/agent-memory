mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::console::{Channel as Stream, Transcript};
use support::lifecycle::{get, mode, text};
use support::{RunOptions, World};

// obligation: tests/contracts/test_download_diagnostics.py::PreviewGuidance.test_preview_is_not_installation_and_does_not_request_restart
#[test]
fn preview_never_claims_installation_or_restart() {
    let value = json::decode(r#"{"schema":"agent-memory/install-plan-v1","restart_clients":["codex"],"targets":[],"unrelated_option":"OPTION_SECRET"}"#, Default::default()).unwrap();
    let mut output = Transcript::default();
    agent_memory::console::render(&value, "install", "am", 0, &mut output);
    let text = output.text(Stream::Stdout);
    assert!(text.contains("does not install am"));
    assert!(text.contains("without --check or --dry-run"));
    for excluded in ["Restart", "PATH", "OPTION_SECRET"] {
        assert!(!text.contains(excluded), "{text}");
    }
}

#[test]
fn download_failure_child() {
    if std::env::var_os("AGENT_MEMORY_FAULT_DOWNLOAD_CHILD").is_none() {
        return;
    }
    let environment = std::env::vars_os().collect::<Vec<_>>();
    let error = agent_memory::release_distribution::download_with_environment(
        "https://example.com/release-manifest.json?token=URL_SECRET",
        1024,
        Default::default(),
        &environment,
        std::time::Duration::from_secs(10),
    )
    .unwrap_err();
    let message = error.to_string();
    let value = Value::Object(vec![
        ("details".into(), error.details),
        ("message".into(), Value::String(message)),
        ("repair".into(), Value::String(error.repair)),
    ]);
    println!(
        "DOWNLOAD-RESULT:{}",
        json::encode(&value, json::COMPACT_ASCII).unwrap()
    );
}
fn failed_retention() -> Value {
    let mut world = World::new().unwrap();
    let directory = world.root.join("download tools");
    std::fs::create_dir(&directory).unwrap();
    let curl = directory.join("curl");
    std::fs::write(
        &curl,
        b"#!/bin/sh\nprintf '000'\nprintf 'receive error' >&2\nexit 56\n",
    )
    .unwrap();
    mode(&curl, 0o700);
    world.env.insert(
        "PATH".into(),
        format!("{}:/usr/bin:/bin", directory.display()).into(),
    );
    world
        .env
        .insert("AGENT_MEMORY_FAULT_DOWNLOAD_CHILD".into(), "1".into());
    let executable = std::env::current_exe().unwrap();
    let output = world
        .run_with(
            [
                "/bin/sh",
                "-c",
                "trap '' XFSZ; ulimit -f 0; exec \"$@\"",
                "download-resource-limit",
                executable.to_str().unwrap(),
                "download_failure_child",
                "--exact",
                "--nocapture",
            ],
            RunOptions {
                timeout: std::time::Duration::from_secs(30),
                ..Default::default()
            },
        )
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let row = stdout
        .lines()
        .find_map(|line| line.strip_prefix("DOWNLOAD-RESULT:"))
        .unwrap_or_else(|| panic!("{stdout}"));
    json::decode(row, Default::default()).unwrap()
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_retention_failure_preserves_primary_error
#[test]
fn real_diagnostic_write_failure_preserves_primary_download_error() {
    let error = failed_retention();
    assert_eq!(get(get(&error, "details"), "diagnostic_path"), &Value::Null);
    assert!(text(&error, "message").contains("receive failure"));
    assert!(text(&error, "repair").contains("Could not retain"));
}

// obligation: tests/contracts/test_download_diagnostics.py::ReviewRegressions.test_report_policy_failure_preserves_primary_error
#[test]
fn failed_report_retention_never_replaces_the_primary_error() {
    let error = failed_retention();
    assert!(text(&error, "message").contains("receive failure"));
    assert_eq!(get(get(&error, "details"), "diagnostic_path"), &Value::Null);
    assert_eq!(
        get(get(&error, "details"), "activation_accepted"),
        &Value::Bool(false)
    );
    assert!(
        !json::encode(&error, json::DEFAULT)
            .unwrap()
            .contains("URL_SECRET")
    );
}

// obligation: tests/contracts/test_download_diagnostics.py::BootstrapPreview.test_signed_preview_uses_packaged_guidance_and_preserves_profiles
#[test]
fn signed_preview_uses_packaged_guidance_without_profile_changes() {
    let world = World::new().unwrap();
    let channel = support::distribution::Channel::new(&world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    let before = world
        .paths
        .values()
        .map(|path| (path.clone(), support::lifecycle::snapshot(path)))
        .collect::<Vec<_>>();
    let value = channel.run(&world, "install", &["none", "copy", "--check"], 0);
    let mut output = Transcript::default();
    agent_memory::console::render(&value, "install", "am", 0, &mut output);
    let text = output.text(Stream::Stdout);
    assert!(text.contains("does not install am"));
    assert!(text.contains("without --check or --dry-run"));
    for excluded in ["Restart", "PATH", "ENV_SECRET"] {
        assert!(!text.contains(excluded));
    }
    assert!(
        !support::lifecycle::state(&world)
            .join("receipt.json")
            .exists()
    );
    for (path, snapshot) in before {
        assert_eq!(support::lifecycle::snapshot(&path), snapshot);
    }
}
