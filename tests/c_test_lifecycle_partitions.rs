#[path = "support/fault_partitions.rs"]
mod fault_partitions;
mod support;
use agent_memory::codec::json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use support::lifecycle::{get, text};
use support::{RunOptions, World};
static RECEIPTS: LazyLock<Vec<Value>> = LazyLock::new(collect_receipts);
/// The sibling test executable built by this `cargo test` run. Earlier builds
/// with other hashes stay in `deps`, so the most recently written one is current.
fn binary(target: &str) -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let directory = executable.parent().unwrap();
    let prefix = format!("{target}-");
    std::fs::read_dir(directory)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_str()?;
            let hash = name.strip_prefix(&prefix)?;
            (hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| (entry.metadata().unwrap().modified().unwrap(), entry.path()))
        })
        .max()
        .unwrap_or_else(|| {
            panic!("Build the {target} test executable before running partition coverage")
        })
        .1
}
fn receipts() -> &'static [Value] {
    &RECEIPTS
}
fn collect_receipts() -> Vec<Value> {
    let world = World::new().unwrap();
    let mut receipts = Vec::new();
    for group in fault_partitions::groups() {
        let target = if group.operation == "install" {
            "t2_installation"
        } else {
            "t2_lifecycle"
        };
        let function = if let Some(mode) = group.mode {
            format!(
                "every_{}_write_{}_phase_interrupts_recovers_and_retries_in_{mode}_mode",
                group.operation, group.phase
            )
        } else {
            format!(
                "every_update_write_{}_phase_interrupts_recovers_and_retries",
                group.phase
            )
        };
        let binary = binary(target);
        let output = world
            .run_with(
                [
                    binary.to_str().unwrap(),
                    &function,
                    "--exact",
                    "--nocapture",
                    "--test-threads=1",
                ],
                RunOptions {
                    timeout: Duration::from_secs(300),
                    ..Default::default()
                },
            )
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        for suite in group.suites {
            let proof = fault_partitions::extract(&stderr, &suite)
                .unwrap_or_else(|error| panic!("{suite}: {error}\n{stderr}"));
            receipts.push(Value::Object(vec![
                ("name".into(), Value::String(suite)),
                ("status".into(), Value::String("passed".into())),
                ("fault_partition".into(), proof),
            ]));
        }
    }
    receipts
}
fn change(value: &mut Value, key: &str, replacement: Value) {
    let Value::Object(fields) = value else {
        panic!("object");
    };
    fields.iter_mut().find(|(name, _)| name == key).unwrap().1 = replacement;
}
fn proof(suites: &mut [Value], name: &str) -> Value {
    get(
        suites.iter().find(|row| text(row, "name") == name).unwrap(),
        "fault_partition",
    )
    .clone()
}
fn replace(suites: &mut [Value], name: &str, proof: Value) {
    change(
        suites
            .iter_mut()
            .find(|row| text(row, "name") == name)
            .unwrap(),
        "fault_partition",
        proof,
    );
}

// obligation: tests/contracts/test_lifecycle_partitions.py::LifecyclePartitionContract.test_complete_actual_interruption_inventory_and_artifact_agree
#[test]
fn complete_actual_replays_have_disjoint_inventory_and_identical_artifacts() {
    let summaries = fault_partitions::coverage(receipts()).unwrap();
    assert_eq!(summaries.len(), 10);
    for row in summaries {
        let expected = if text(&row, "group").starts_with("uninstall") {
            4
        } else {
            8
        };
        assert_eq!(get(&row, "partitions"), &Value::Int(expected.into()));
        assert_eq!(
            get(&row, "observed_points"),
            get(&row, "actual_interruptions_and_retries")
        );
        assert_eq!(text(&row, "artifact_digest").len(), 64);
    }
}

// obligation: tests/contracts/test_lifecycle_partitions.py::LifecyclePartitionContract.test_missing_duplicate_or_foreign_interruption_cannot_complete_release
#[test]
fn missing_duplicate_or_foreign_actual_interruption_cannot_complete_release() {
    let name = "uninstall-write-copy-before";
    for failure in [
        "missing-partition",
        "omitted-write",
        "duplicate-write",
        "different-artifact",
        "different-observed-inventory",
    ] {
        let mut suites = receipts().to_vec();
        if failure == "missing-partition" {
            suites.retain(|row| text(row, "name") != name);
        } else {
            let mut proof = proof(&mut suites, name);
            match failure {
                "omitted-write" => {
                    let mut values = get(&proof, "executed").as_array().unwrap().to_vec();
                    values.pop();
                    change(&mut proof, "executed", Value::Array(values));
                }
                "duplicate-write" => {
                    let mut values = get(&proof, "executed").as_array().unwrap().to_vec();
                    assert!(values.len() >= 2);
                    values[1] = values[0].clone();
                    change(&mut proof, "executed", Value::Array(values));
                }
                "different-artifact" => {
                    change(&mut proof, "artifact_digest", Value::String("b".repeat(64)))
                }
                _ => {
                    let mut rows = get(&proof, "observed").as_array().unwrap().to_vec();
                    change(
                        &mut rows[0],
                        "point",
                        Value::String("unobserved-write".into()),
                    );
                    change(&mut proof, "observed", Value::Array(rows));
                }
            }
            replace(&mut suites, name, proof);
        }
        assert!(fault_partitions::coverage(&suites).is_err(), "{failure}");
    }
}

// obligation: tests/contracts/test_lifecycle_partitions.py::LifecyclePartitionContract.test_finer_partitions_reject_wrong_count_and_missing_execution
#[test]
fn finer_actual_partitions_reject_wrong_count_and_missing_execution() {
    for name in ["update-write-before", "install-write-copy-after"] {
        for failure in ["wrong-count", "omitted-write", "missing-partition"] {
            let mut suites = receipts().to_vec();
            if failure == "missing-partition" {
                suites.retain(|row| text(row, "name") != name);
            } else {
                let mut proof = proof(&mut suites, name);
                if failure == "wrong-count" {
                    change(&mut proof, "count", Value::Int(4.into()));
                } else {
                    let mut values = get(&proof, "executed").as_array().unwrap().to_vec();
                    values.pop();
                    change(&mut proof, "executed", Value::Array(values));
                }
                replace(&mut suites, name, proof);
            }
            assert!(
                fault_partitions::coverage(&suites).is_err(),
                "{name}: {failure}"
            );
        }
    }
}
