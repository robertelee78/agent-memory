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
    let test = format!("release_builder::gaps_tests::{name}");
    // Earlier builds (other hashes, other feature sets) stay beside this one:
    // use the most recent library test executable that actually has the test.
    let mut candidates = std::fs::read_dir(directory)
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
        .filter(|entry| {
            std::process::Command::new(entry.path())
                .args(["--list", "--exact", &test])
                .output()
                .is_ok_and(|listed| {
                    String::from_utf8_lossy(&listed.stdout).contains(&format!("{test}: test"))
                })
        })
        .map(|entry| (entry.metadata().unwrap().modified().unwrap(), entry.path()))
        .collect::<Vec<_>>();
    candidates.sort();
    let candidates: Vec<_> = candidates.into_iter().rev().map(|(_, path)| path).collect();
    assert!(
        !candidates.is_empty(),
        "compile the library test executable with cargo test --offline --features dev-tools --lib --no-run"
    );
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
// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_one_command_first_publish_exact_resume_and_harmless_descendant
fn first_publish_resume_and_harmless_descendant() {
    run("one_command_first_publish_exact_resume_and_harmless_descendant");
}
#[test]
// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation
fn wrong_tags_and_racing_main_never_claim_activation() {
    run("wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation");
}
