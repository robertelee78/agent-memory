mod support;
use agent_memory::codec::json::Value;
use std::fs;
use support::World;
use support::cutover::*;

#[test]
fn cutover_dispatch_worker() {
    dispatch_worker();
}

#[test]
// obligation: tests/contracts/test_claude_boundary.py::ClaudeBoundaryContract.test_uninitialized_and_nonrepository_are_quiet
fn uninitialized_and_nonrepository_claude_stop_are_quiet() {
    let w = World::new().unwrap();
    let repo = w.repository("repo", false).unwrap();
    let outside = w.inside("no-repo").unwrap();
    fs::create_dir(&outside).unwrap();
    for directory in [repo, outside] {
        let event = object([
            ("hook_event_name", text("Stop")),
            ("stop_hook_active", Value::Bool(false)),
            ("cwd", text(directory.to_str().unwrap())),
        ]);
        let (code, stdout, stderr) =
            dispatch(&w, &["hook".into(), "claude-stop".into()], Some(&event));
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
    }
}
