#![cfg(not(feature = "dev-tools"))]

use agent_memory::codec::json::Value;
use agent_memory::error::ContractError;
use agent_memory::{bank_writes, runtime_faults};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn production_boundaries_ignore_the_entire_fault_environment() {
    if std::env::var_os("AM_PRODUCTION_FAULT_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "production_boundaries_ignore_the_entire_fault_environment",
                "--nocapture",
            ])
            .env("AM_PRODUCTION_FAULT_CHILD", "1")
            .env("AGENT_MEMORY_FAULT_STAGE", "production-probe")
            .env("AGENT_MEMORY_FAULT_MODE", "exit")
            .env("AGENT_MEMORY_FAULT_DIR", "/missing-fault-world")
            .env("AGENT_MEMORY_FAULT_TRACE", "/missing-fault-trace")
            .env("AGENT_MEMORY_FAULT_ERROR_STAGE", "install-write-0-before")
            .env("AGENT_MEMORY_TEST_ROOT", "/missing-fault-world")
            .env("AGENT_MEMORY_TEST_TOKEN", "invalid-token")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        return;
    }
    bank_writes::fault("production-probe").unwrap();
    let mut writes = 0;
    let result: Result<_, ContractError> =
        runtime_faults::operation("install", &[PathBuf::from("/outside-world")], || {
            runtime_faults::temporary_probe(|| {
                runtime_faults::mutate("real-write", || {
                    writes += 1;
                    Ok(7)
                })
            })
        });
    assert_eq!(result.unwrap(), 7);
    assert_eq!(writes, 1);
    runtime_faults::response(
        "install",
        &Value::Object(vec![("verified".into(), Value::Bool(true))]),
    )
    .unwrap();
}
