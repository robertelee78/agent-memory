#![cfg(feature = "dev-tools")]
mod support;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
fn run(name: &str) {
    let world = support::World::new().unwrap();
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let candidates = std::fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("agent_memory-")
                && entry.metadata().unwrap().is_file()
                && entry.metadata().unwrap().permissions().mode() & 0o111 != 0
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert_eq!(
        candidates.len(),
        1,
        "compile exactly one current library test executable with cargo test --offline --features dev-tools --lib --no-run"
    );
    let test = format!("release_builder::gaps_tests::{name}");
    let output = world
        .run_with(
            [
                candidates[0].as_os_str(),
                std::ffi::OsStr::new("--exact"),
                std::ffi::OsStr::new(&test),
                std::ffi::OsStr::new("--nocapture"),
                std::ffi::OsStr::new("--test-threads=1"),
            ],
            support::RunOptions {
                timeout: Duration::from_secs(240),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(output.status, agent_memory::process::Status::Code(0));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "publisher obligation was not executed"
    );
}
#[test]
#[ignore = "needs Gate C Python removal (native adapter entrypoint)"]
// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_one_command_first_publish_exact_resume_and_harmless_descendant
fn first_publish_resume_and_harmless_descendant() {
    run("one_command_first_publish_exact_resume_and_harmless_descendant");
}
#[test]
#[ignore = "needs Gate C Python removal (native adapter entrypoint)"]
// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation
fn wrong_tags_and_racing_main_never_claim_activation() {
    run("wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation");
}
