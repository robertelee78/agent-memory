mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::Status;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use support::cutover::*;

#[test]
// obligation: tests/contracts/test_recall_observation.py::RecallObservationContract.test_portable_alias_guard_preserves_component_unicode_and_root_anchor_collisions
fn portable_aliases_preserve_unicode_components_and_root_anchor_distinctions() {
    for values in [
        vec!["topics/Name/a.md", "topics/name/b.md"],
        vec!["topics/café.md", "topics/cafe\u{301}.md"],
        vec!["/Topic/a", "/topic/b"],
        vec!["//Topic/a", "//topic/b"],
        vec!["topics/Straße/a", "topics/STRASSE/b"],
    ] {
        assert!(agent_memory::bank_contract::portable_aliases(values).is_err());
    }
    for values in [
        vec![
            "topics/shared/a.md",
            "topics/shared/b.md",
            "topics/shared/a.md",
        ],
        vec!["/topics/a", "/topics/b"],
        vec!["//topics/a", "//topics/b"],
        vec!["a//b", "a/./b"],
        vec![".", "/", "//"],
    ] {
        agent_memory::bank_contract::portable_aliases(values).unwrap();
    }
}
fn fixture() -> (Recall, String, Value) {
    let r = Recall::new();
    let path = r.add(
        "observed",
        "# Authored count\n\nThe authored count is three.\n",
        &object(Vec::<(String, Value)>::new()),
    );
    let mut request = r.request(&[], object(Vec::<(String, Value)>::new()));
    put(&mut request, "query", text("authored count"));
    put(&mut request, "include_context", Value::Bool(false));
    (r, path, request)
}
fn finish_refused(child: support::Child, release: std::path::PathBuf) {
    fs::write(release, b"").unwrap();
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(1));
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert!(!yes(&value, "verified"));
    assert!(value.get("results").is_none());
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_recall_observation.py::RecallObservationContract.test_omitted_context_occupied_by_directory_or_fifo_is_not_absence
fn omitted_context_directory_or_fifo_is_not_absence() {
    for directory in [true, false] {
        let (r, _, request) = fixture();
        let code = r.code();
        let target = r.bank.join("productContext.md");
        if directory {
            fs::create_dir(&target).unwrap();
        } else {
            fifo(&target);
        }
        let (output, value) = r.retrieve(&request);
        assert_eq!(output.status, Status::Code(1));
        assert!(value.get("results").is_none());
        assert!(target.exists());
        assert_eq!(r.code(), code);
        if directory {
            fs::remove_dir(target).unwrap();
        } else {
            fs::remove_file(target).unwrap();
        }
        assert_eq!(r.retrieve(&request).0.status, Status::Code(0));
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_recall_observation.py::RecallObservationContract.test_changed_source_after_read_or_at_final_recheck_is_unavailable
fn changed_source_after_read_or_final_recheck_is_unavailable() {
    for stage in ["recall-after-source-read", "recall-final-source-recheck"] {
        let (r, path, request) = fixture();
        let code = r.code();
        let (child, release) = r.pause(&request, stage);
        let target = r.bank.join(path);
        fs::write(
            &target,
            fs::read_to_string(&target)
                .unwrap()
                .replace("count is three.", "count is four."),
        )
        .unwrap();
        finish_refused(child, release);
        assert!(
            fs::read_to_string(target)
                .unwrap()
                .contains("count is four.")
        );
        assert_eq!(r.code(), code);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_recall_observation.py::RecallObservationContract.test_fifo_marker_arriving_at_final_recheck_refuses_without_blocking_or_removal
fn fifo_marker_arriving_at_final_recheck_is_preserved_without_blocking() {
    for name in [".agent-memory.lock", MARKER] {
        let (r, _, request) = fixture();
        let code = r.code();
        let (child, release) = r.pause(&request, "recall-final-source-recheck");
        let target = r.bank.join(name);
        fifo(&target);
        finish_refused(child, release);
        assert!(fs::symlink_metadata(&target).unwrap().file_type().is_fifo());
        assert_eq!(r.code(), code);
        fs::remove_file(target).unwrap();
    }
}
