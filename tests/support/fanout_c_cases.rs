// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_hold_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_after_hold_error() {
    isolated("fanout_c::layout_apply_faults_after_hold_error", |w| {
        apply_fault(w, "layout-recovery-after-hold", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_hold_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_after_hold_exit() {
    isolated("fanout_c::layout_apply_faults_after_hold_exit", |w| {
        apply_fault(w, "layout-recovery-after-hold", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_sentinel_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_after_sentinel_error() {
    isolated("fanout_c::layout_apply_faults_after_sentinel_error", |w| {
        apply_fault(w, "layout-recovery-after-sentinel", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_sentinel_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_after_sentinel_exit() {
    isolated("fanout_c::layout_apply_faults_after_sentinel_exit", |w| {
        apply_fault(w, "layout-recovery-after-sentinel", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_swap_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_after_swap_error() {
    isolated("fanout_c::layout_apply_faults_after_swap_error", |w| {
        apply_fault(w, "layout-recovery-after-marker-swap", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_swap_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_after_swap_exit() {
    isolated("fanout_c::layout_apply_faults_after_swap_exit", |w| {
        apply_fault(w, "layout-recovery-after-marker-swap", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_journal_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_journal_error() {
    isolated("fanout_c::layout_apply_faults_before_journal_error", |w| {
        apply_fault(w, "layout-recovery-before-journal", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_journal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_journal_exit() {
    isolated("fanout_c::layout_apply_faults_before_journal_exit", |w| {
        apply_fault(w, "layout-recovery-before-journal", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_marker_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_marker_error() {
    isolated("fanout_c::layout_apply_faults_before_marker_error", |w| {
        apply_fault(w, "layout-recovery-before-marker", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_marker_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_marker_exit() {
    isolated("fanout_c::layout_apply_faults_before_marker_exit", |w| {
        apply_fault(w, "layout-recovery-before-marker", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_receipt_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_receipt_exit() {
    isolated("fanout_c::layout_apply_faults_before_receipt_exit", |w| {
        apply_fault(w, "layout-recovery-before-receipt", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_router_error() {
    isolated("fanout_c::layout_apply_faults_before_router_error", |w| {
        apply_fault(w, "layout-recovery-before-router", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_router_exit() {
    isolated("fanout_c::layout_apply_faults_before_router_exit", |w| {
        apply_fault(w, "layout-recovery-before-router", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_sentinel_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_sentinel_error() {
    isolated("fanout_c::layout_apply_faults_before_sentinel_error", |w| {
        apply_fault(w, "layout-recovery-before-sentinel", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_sentinel_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_sentinel_exit() {
    isolated("fanout_c::layout_apply_faults_before_sentinel_exit", |w| {
        apply_fault(w, "layout-recovery-before-sentinel", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_swap_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_swap_error() {
    isolated("fanout_c::layout_apply_faults_before_swap_error", |w| {
        apply_fault(w, "layout-recovery-before-marker-swap", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_swap_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_before_swap_exit() {
    isolated("fanout_c::layout_apply_faults_before_swap_exit", |w| {
        apply_fault(w, "layout-recovery-before-marker-swap", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_hold_swap_after_error() {
    isolated("fanout_c::layout_apply_faults_hold_swap_after_error", |w| {
        apply_fault(w, "layout-recovery-after-marker-swap", "error", "finish", false, true);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_hold_swap_after_exit() {
    isolated("fanout_c::layout_apply_faults_hold_swap_after_exit", |w| {
        apply_fault(w, "layout-recovery-after-marker-swap", "exit", "finish", false, true);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_hold_swap_before_error() {
    isolated("fanout_c::layout_apply_faults_hold_swap_before_error", |w| {
        apply_fault(w, "layout-recovery-before-marker-swap", "error", "finish", false, true);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_hold_swap_before_exit() {
    isolated("fanout_c::layout_apply_faults_hold_swap_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-marker-swap", "exit", "finish", false, true);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_index_error() {
    isolated("fanout_c::layout_apply_faults_index_error", |w| {
        apply_fault(w, "layout-recovery-final-index", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_index_exit() {
    isolated("fanout_c::layout_apply_faults_index_exit", |w| {
        apply_fault(w, "layout-recovery-final-index", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_intent_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_index_intent_error() {
    isolated("fanout_c::layout_apply_faults_index_intent_error", |w| {
        apply_fault(w, "layout-recovery-index-intent", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_intent_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_index_intent_exit() {
    isolated("fanout_c::layout_apply_faults_index_intent_exit", |w| {
        apply_fault(w, "layout-recovery-index-intent", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_journal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_journal_exit() {
    isolated("fanout_c::layout_apply_faults_journal_exit", |w| {
        apply_fault(w, "layout-recovery-after-journal", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_marker_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_marker_exit() {
    isolated("fanout_c::layout_apply_faults_marker_exit", |w| {
        apply_fault(w, "layout-recovery-after-marker", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_marker_removal_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_marker_removal_error() {
    isolated("fanout_c::layout_apply_faults_marker_removal_error", |w| {
        apply_fault(w, "layout-recovery-marker-removal", "error", "finish", true, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_marker_removal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_marker_removal_exit() {
    isolated("fanout_c::layout_apply_faults_marker_removal_exit", |w| {
        apply_fault(w, "layout-recovery-marker-removal", "exit", "finish", true, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_originals_completed_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_originals_completed_error() {
    isolated("fanout_c::layout_apply_faults_originals_completed_error", |w| {
        inspect_fault(w, "layout-recovery-originals-completed", "error", false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_originals_completed_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_originals_completed_exit() {
    isolated("fanout_c::layout_apply_faults_originals_completed_exit", |w| {
        inspect_fault(w, "layout-recovery-originals-completed", "exit", false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_partial_staging_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_partial_staging_exit() {
    isolated("fanout_c::layout_apply_faults_partial_staging_exit", |w| {
        inspect_fault(w, "layout-recovery-partial-original-staging", "exit", true);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_active_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_progress_active_error() {
    isolated("fanout_c::layout_apply_faults_progress_active_error", |w| {
        apply_fault(w, "layout-recovery-after-progress:activeContext.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_progress_active_exit() {
    isolated("fanout_c::layout_apply_faults_progress_active_exit", |w| {
        apply_fault(w, "layout-recovery-after-progress:activeContext.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_handoff_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_progress_handoff_error() {
    isolated("fanout_c::layout_apply_faults_progress_handoff_error", |w| {
        apply_fault(w, "layout-recovery-after-progress:handoff-prompt.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_handoff_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_progress_handoff_exit() {
    isolated("fanout_c::layout_apply_faults_progress_handoff_exit", |w| {
        apply_fault(w, "layout-recovery-after-progress:handoff-prompt.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_progress_router_error() {
    isolated("fanout_c::layout_apply_faults_progress_router_error", |w| {
        apply_fault(w, "layout-recovery-after-progress:MEMORY.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_progress_router_exit() {
    isolated("fanout_c::layout_apply_faults_progress_router_exit", |w| {
        apply_fault(w, "layout-recovery-after-progress:MEMORY.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_readback_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_readback_error() {
    isolated("fanout_c::layout_apply_faults_readback_error", |w| {
        apply_fault(w, "layout-recovery-before-final-readback", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_readback_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_readback_exit() {
    isolated("fanout_c::layout_apply_faults_readback_exit", |w| {
        apply_fault(w, "layout-recovery-before-final-readback", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_receipt_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_receipt_exit() {
    isolated("fanout_c::layout_apply_faults_receipt_exit", |w| {
        apply_fault(w, "layout-recovery-after-receipt", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rename_active_exit() {
    isolated("fanout_c::layout_apply_faults_rename_active_exit", |w| {
        apply_fault(w, "layout-recovery-after-rename:activeContext.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_handoff_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rename_handoff_error() {
    isolated("fanout_c::layout_apply_faults_rename_handoff_error", |w| {
        apply_fault(w, "layout-recovery-after-rename:handoff-prompt.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_handoff_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rename_handoff_exit() {
    isolated("fanout_c::layout_apply_faults_rename_handoff_exit", |w| {
        apply_fault(w, "layout-recovery-after-rename:handoff-prompt.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rename_router_error() {
    isolated("fanout_c::layout_apply_faults_rename_router_error", |w| {
        apply_fault(w, "layout-recovery-after-rename:MEMORY.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rename_router_exit() {
    isolated("fanout_c::layout_apply_faults_rename_router_exit", |w| {
        apply_fault(w, "layout-recovery-after-rename:MEMORY.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_active_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_replace_active_error() {
    isolated("fanout_c::layout_apply_faults_replace_active_error", |w| {
        apply_fault(w, "layout-recovery-before-replace:activeContext.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_replace_active_exit() {
    isolated("fanout_c::layout_apply_faults_replace_active_exit", |w| {
        apply_fault(w, "layout-recovery-before-replace:activeContext.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_handoff_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_replace_handoff_error() {
    isolated("fanout_c::layout_apply_faults_replace_handoff_error", |w| {
        apply_fault(w, "layout-recovery-before-replace:handoff-prompt.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_handoff_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_replace_handoff_exit() {
    isolated("fanout_c::layout_apply_faults_replace_handoff_exit", |w| {
        apply_fault(w, "layout-recovery-before-replace:handoff-prompt.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_replace_router_error() {
    isolated("fanout_c::layout_apply_faults_replace_router_error", |w| {
        apply_fault(w, "layout-recovery-before-replace:MEMORY.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_replace_router_exit() {
    isolated("fanout_c::layout_apply_faults_replace_router_exit", |w| {
        apply_fault(w, "layout-recovery-before-replace:MEMORY.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_hold_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_hold_after_error() {
    isolated("fanout_c::layout_apply_faults_rollback_hold_after_error", |w| {
        apply_fault(w, "layout-recovery-after-hold", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_hold_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_hold_after_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_hold_after_exit", |w| {
        apply_fault(w, "layout-recovery-after-hold", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_index_intent_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_index_intent_error() {
    isolated("fanout_c::layout_apply_faults_rollback_index_intent_error", |w| {
        apply_fault(w, "layout-recovery-index-intent", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_index_intent_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_index_intent_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_index_intent_exit", |w| {
        apply_fault(w, "layout-recovery-index-intent", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_journal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_journal_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_journal_exit", |w| {
        apply_fault(w, "layout-recovery-after-journal", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_marker_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_marker_before_error() {
    isolated("fanout_c::layout_apply_faults_rollback_marker_before_error", |w| {
        apply_fault(w, "layout-recovery-before-marker", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_marker_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_marker_before_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_marker_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-marker", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_active_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_progress_active_error() {
    isolated("fanout_c::layout_apply_faults_rollback_progress_active_error", |w| {
        apply_fault(w, "layout-recovery-after-progress:activeContext.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_progress_active_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_progress_active_exit", |w| {
        apply_fault(w, "layout-recovery-after-progress:activeContext.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_handoff_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_progress_handoff_error() {
    isolated("fanout_c::layout_apply_faults_rollback_progress_handoff_error", |w| {
        apply_fault(w, "layout-recovery-after-progress:handoff-prompt.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_handoff_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_progress_handoff_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_progress_handoff_exit", |w| {
        apply_fault(w, "layout-recovery-after-progress:handoff-prompt.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_progress_router_error() {
    isolated("fanout_c::layout_apply_faults_rollback_progress_router_error", |w| {
        apply_fault(w, "layout-recovery-after-progress:MEMORY.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_progress_router_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_progress_router_exit", |w| {
        apply_fault(w, "layout-recovery-after-progress:MEMORY.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_readback_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_readback_error() {
    isolated("fanout_c::layout_apply_faults_rollback_readback_error", |w| {
        apply_fault(w, "layout-recovery-before-final-readback", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_readback_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_readback_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_readback_exit", |w| {
        apply_fault(w, "layout-recovery-before-final-readback", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_receipt_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_receipt_before_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_receipt_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-receipt", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_rename_active_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_rename_active_exit", |w| {
        apply_fault(w, "layout-recovery-after-rename:activeContext.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_handoff_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_rename_handoff_error() {
    isolated("fanout_c::layout_apply_faults_rollback_rename_handoff_error", |w| {
        apply_fault(w, "layout-recovery-after-rename:handoff-prompt.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_handoff_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_rename_handoff_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_rename_handoff_exit", |w| {
        apply_fault(w, "layout-recovery-after-rename:handoff-prompt.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_rename_router_error() {
    isolated("fanout_c::layout_apply_faults_rollback_rename_router_error", |w| {
        apply_fault(w, "layout-recovery-after-rename:MEMORY.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_rename_router_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_rename_router_exit", |w| {
        apply_fault(w, "layout-recovery-after-rename:MEMORY.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_active_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_replace_active_error() {
    isolated("fanout_c::layout_apply_faults_rollback_replace_active_error", |w| {
        apply_fault(w, "layout-recovery-before-replace:activeContext.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_replace_active_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_replace_active_exit", |w| {
        apply_fault(w, "layout-recovery-before-replace:activeContext.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_handoff_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_replace_handoff_error() {
    isolated("fanout_c::layout_apply_faults_rollback_replace_handoff_error", |w| {
        apply_fault(w, "layout-recovery-before-replace:handoff-prompt.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_handoff_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_replace_handoff_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_replace_handoff_exit", |w| {
        apply_fault(w, "layout-recovery-before-replace:handoff-prompt.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_replace_router_error() {
    isolated("fanout_c::layout_apply_faults_rollback_replace_router_error", |w| {
        apply_fault(w, "layout-recovery-before-replace:MEMORY.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_replace_router_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_replace_router_exit", |w| {
        apply_fault(w, "layout-recovery-before-replace:MEMORY.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_router_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_router_before_error() {
    isolated("fanout_c::layout_apply_faults_rollback_router_before_error", |w| {
        apply_fault(w, "layout-recovery-before-router", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_router_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_router_before_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_router_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-router", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_sentinel_after_error() {
    isolated("fanout_c::layout_apply_faults_rollback_sentinel_after_error", |w| {
        apply_fault(w, "layout-recovery-after-sentinel", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_sentinel_after_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_sentinel_after_exit", |w| {
        apply_fault(w, "layout-recovery-after-sentinel", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_sentinel_before_error() {
    isolated("fanout_c::layout_apply_faults_rollback_sentinel_before_error", |w| {
        apply_fault(w, "layout-recovery-before-sentinel", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_sentinel_before_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_sentinel_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-sentinel", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_swap_after_error() {
    isolated("fanout_c::layout_apply_faults_rollback_swap_after_error", |w| {
        apply_fault(w, "layout-recovery-after-marker-swap", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_swap_after_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_swap_after_exit", |w| {
        apply_fault(w, "layout-recovery-after-marker-swap", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_swap_before_error() {
    isolated("fanout_c::layout_apply_faults_rollback_swap_before_error", |w| {
        apply_fault(w, "layout-recovery-before-marker-swap", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_swap_before_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_swap_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-marker-swap", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_active_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_temp_active_error() {
    isolated("fanout_c::layout_apply_faults_rollback_temp_active_error", |w| {
        apply_fault(w, "layout-recovery-temporary-write:activeContext.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_temp_active_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_temp_active_exit", |w| {
        apply_fault(w, "layout-recovery-temporary-write:activeContext.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_temp_router_error() {
    isolated("fanout_c::layout_apply_faults_rollback_temp_router_error", |w| {
        apply_fault(w, "layout-recovery-temporary-write:MEMORY.md", "error", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_rollback_temp_router_exit() {
    isolated("fanout_c::layout_apply_faults_rollback_temp_router_exit", |w| {
        apply_fault(w, "layout-recovery-temporary-write:MEMORY.md", "exit", "rollback", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_router_exit() {
    isolated("fanout_c::layout_apply_faults_router_exit", |w| {
        apply_fault(w, "layout-recovery-after-router", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_sentinel_retirement_after_error() {
    isolated("fanout_c::layout_apply_faults_sentinel_retirement_after_error", |w| {
        apply_fault(w, "layout-recovery-sentinel-retirement", "error", "finish", true, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_sentinel_retirement_after_exit() {
    isolated("fanout_c::layout_apply_faults_sentinel_retirement_after_exit", |w| {
        apply_fault(w, "layout-recovery-sentinel-retirement", "exit", "finish", true, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_sentinel_retirement_before_error() {
    isolated("fanout_c::layout_apply_faults_sentinel_retirement_before_error", |w| {
        apply_fault(w, "layout-recovery-before-sentinel-retirement", "error", "finish", true, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_sentinel_retirement_before_exit() {
    isolated("fanout_c::layout_apply_faults_sentinel_retirement_before_exit", |w| {
        apply_fault(w, "layout-recovery-before-sentinel-retirement", "exit", "finish", true, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_active_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_temp_active_error() {
    isolated("fanout_c::layout_apply_faults_temp_active_error", |w| {
        apply_fault(w, "layout-recovery-temporary-write:activeContext.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_active_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_temp_active_exit() {
    isolated("fanout_c::layout_apply_faults_temp_active_exit", |w| {
        apply_fault(w, "layout-recovery-temporary-write:activeContext.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_temp_router_error() {
    isolated("fanout_c::layout_apply_faults_temp_router_error", |w| {
        apply_fault(w, "layout-recovery-temporary-write:MEMORY.md", "error", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_apply_faults_temp_router_exit() {
    isolated("fanout_c::layout_apply_faults_temp_router_exit", |w| {
        apply_fault(w, "layout-recovery-temporary-write:MEMORY.md", "exit", "finish", false, false);
    });
}

// deferred obligation: tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_after_receipt_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_restore_faults_after_receipt_error() {
    isolated("fanout_c::layout_restore_faults_after_receipt_error", |w| {
        restore_fault(w, "layout-recovery-after-receipt", "error");
    });
}

// deferred obligation: tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_probe_journal_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_restore_faults_probe_journal_error() {
    isolated("fanout_c::layout_restore_faults_probe_journal_error", |w| {
        restore_fault(w, "layout-recovery-probe-journal", "error");
    });
}

// deferred obligation: tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_probe_journal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_restore_faults_probe_journal_exit() {
    isolated("fanout_c::layout_restore_faults_probe_journal_exit", |w| {
        restore_fault(w, "layout-recovery-probe-journal", "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_0_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_0_error() {
    isolated("fanout_c::layout_retention_faults_delete_0_error", |w| {
        retention_partition(w, 0, "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_0_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_0_exit() {
    isolated("fanout_c::layout_retention_faults_delete_0_exit", |w| {
        retention_partition(w, 0, "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_1_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_1_error() {
    isolated("fanout_c::layout_retention_faults_delete_1_error", |w| {
        retention_partition(w, 1, "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_1_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_1_exit() {
    isolated("fanout_c::layout_retention_faults_delete_1_exit", |w| {
        retention_partition(w, 1, "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_2_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_2_error() {
    isolated("fanout_c::layout_retention_faults_delete_2_error", |w| {
        retention_partition(w, 2, "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_2_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_2_exit() {
    isolated("fanout_c::layout_retention_faults_delete_2_exit", |w| {
        retention_partition(w, 2, "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_3_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_3_error() {
    isolated("fanout_c::layout_retention_faults_delete_3_error", |w| {
        retention_partition(w, 3, "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_3_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_delete_3_exit() {
    isolated("fanout_c::layout_retention_faults_delete_3_exit", |w| {
        retention_partition(w, 3, "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_intent_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_intent_error() {
    isolated("fanout_c::layout_retention_faults_intent_error", |w| {
        retention_fault(w, "layout-recovery-retention-intent", "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_intent_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_intent_exit() {
    isolated("fanout_c::layout_retention_faults_intent_exit", |w| {
        retention_fault(w, "layout-recovery-retention-intent", "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_progress_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_progress_error() {
    isolated("fanout_c::layout_retention_faults_progress_error", |w| {
        retention_fault(w, "layout-recovery-retention-after-progress", "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_progress_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_progress_exit() {
    isolated("fanout_c::layout_retention_faults_progress_exit", |w| {
        retention_fault(w, "layout-recovery-retention-after-progress", "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_tombstone_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_tombstone_error() {
    isolated("fanout_c::layout_retention_faults_tombstone_error", |w| {
        retention_fault(w, "layout-recovery-retention-final-tombstone", "error");
    });
}

// deferred obligation: tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_tombstone_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_retention_faults_tombstone_exit() {
    isolated("fanout_c::layout_retention_faults_tombstone_exit", |w| {
        retention_fault(w, "layout-recovery-retention-final-tombstone", "exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_attempt_journal_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_attempt_journal_error() {
    isolated("fanout_c::layout_metadata_probe_faults_attempt_journal_error", |w| {
        probe_journey(w,"layout-recovery-probe-after-attempt-journal","error");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_attempt_journal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_attempt_journal_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_attempt_journal_exit", |w| {
        probe_journey(w,"layout-recovery-probe-after-attempt-journal","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_0_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_0_error() {
    isolated("fanout_c::layout_metadata_probe_faults_created_0_error", |w| {
        probe_partition(w,"created",0,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_0_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_0_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_created_0_exit", |w| {
        probe_partition(w,"created",0,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_1_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_1_error() {
    isolated("fanout_c::layout_metadata_probe_faults_created_1_error", |w| {
        probe_partition(w,"created",1,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_1_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_1_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_created_1_exit", |w| {
        probe_partition(w,"created",1,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_2_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_2_error() {
    isolated("fanout_c::layout_metadata_probe_faults_created_2_error", |w| {
        probe_partition(w,"created",2,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_2_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_2_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_created_2_exit", |w| {
        probe_partition(w,"created",2,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_3_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_3_error() {
    isolated("fanout_c::layout_metadata_probe_faults_created_3_error", |w| {
        probe_partition(w,"created",3,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_3_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_created_3_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_created_3_exit", |w| {
        probe_partition(w,"created",3,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_directories_removed_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_directories_removed_error() {
    isolated("fanout_c::layout_metadata_probe_faults_directories_removed_error", |w| {
        probe_journey(w,"layout-recovery-probe-directories-removed","error");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_directories_removed_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_directories_removed_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_directories_removed_exit", |w| {
        probe_journey(w,"layout-recovery-probe-directories-removed","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_journal_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_journal_error() {
    isolated("fanout_c::layout_metadata_probe_faults_journal_error", |w| {
        probe_journey(w,"layout-recovery-probe-journal","error");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_journal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_journal_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_journal_exit", |w| {
        probe_journey(w,"layout-recovery-probe-journal","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_created_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_late_created_error() {
    isolated("fanout_c::layout_metadata_probe_faults_late_created_error", |w| {
        probe_partition(w,"created",0,"error",true);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_created_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_late_created_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_late_created_exit", |w| {
        probe_partition(w,"created",0,"exit",true);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_removed_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_late_removed_error() {
    isolated("fanout_c::layout_metadata_probe_faults_late_removed_error", |w| {
        probe_partition(w,"removed",0,"error",true);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_removed_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_late_removed_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_late_removed_exit", |w| {
        probe_partition(w,"removed",0,"exit",true);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_0_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_0_error() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_0_error", |w| {
        probe_partition(w,"removed",0,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_0_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_0_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_0_exit", |w| {
        probe_partition(w,"removed",0,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_1_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_1_error() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_1_error", |w| {
        probe_partition(w,"removed",1,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_1_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_1_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_1_exit", |w| {
        probe_partition(w,"removed",1,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_2_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_2_error() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_2_error", |w| {
        probe_partition(w,"removed",2,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_2_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_2_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_2_exit", |w| {
        probe_partition(w,"removed",2,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_3_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_3_error() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_3_error", |w| {
        probe_partition(w,"removed",3,"error",false);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_3_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_removed_3_exit() {
    isolated("fanout_c::layout_metadata_probe_faults_removed_3_exit", |w| {
        probe_partition(w,"removed",3,"exit",false);
    });
}

// deferred obligation: tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_interrupted_external_file_refuses_finish_and_rollback
#[test]
#[ignore = "recovery_frontier"]
fn layout_damaged_evidence_interrupted_external_file_refuses_finish_and_rollback() {
    isolated("fanout_c::layout_damaged_evidence_interrupted_external_file_refuses_finish_and_rollback", |w| {
        external_change(w,false,false);
    });
}

// deferred obligation: tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_interrupted_metadata_edit_refuses_finish_and_rollback
#[test]
#[ignore = "recovery_frontier"]
fn layout_damaged_evidence_interrupted_metadata_edit_refuses_finish_and_rollback() {
    isolated("fanout_c::layout_damaged_evidence_interrupted_metadata_edit_refuses_finish_and_rollback", |w| {
        external_change(w,true,false);
    });
}

// deferred obligation: tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_original_evidence_refuses_changed_or_missing
#[test]
#[ignore = "recovery_frontier"]
fn layout_damaged_evidence_original_evidence_refuses_changed_or_missing() {
    isolated("fanout_c::layout_damaged_evidence_original_evidence_refuses_changed_or_missing", |w| {
        damaged_evidence(w,"original");
    });
}

// deferred obligation: tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_review_refuses_changed_or_missing
#[test]
#[ignore = "recovery_frontier"]
fn layout_damaged_evidence_review_refuses_changed_or_missing() {
    isolated("fanout_c::layout_damaged_evidence_review_refuses_changed_or_missing", |w| {
        damaged_evidence(w,"review");
    });
}

// deferred obligation: tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_partial_state_and_validate_refuse_without_adoption
#[test]
#[ignore = "recovery_frontier"]
fn layout_restore_faults_partial_state_and_validate_refuse_without_adoption() {
    isolated("fanout_c::layout_restore_faults_partial_state_and_validate_refuse_without_adoption", |w| {
        external_change(w,false,true);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_unsafe_apply.py::RecoveryUnsafeApplyContract.test_changed_secret_or_oversized_pending_original_refuses_apply_before_mutation
#[test]
#[ignore = "recovery_frontier"]
fn recovery_unsafe_apply_contract_changed_secret_or_oversized_pending_original_refuses_apply_before_mutation() {
    isolated("fanout_c::recovery_unsafe_apply_contract_changed_secret_or_oversized_pending_original_refuses_apply_before_mutation", |w| {
        unsafe_original(w);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_before_pin_directory_error_cleans_owned_probe
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_before_pin_directory_error_cleans_owned_probe() {
    isolated("fanout_c::layout_metadata_probe_faults_before_pin_directory_error_cleans_owned_probe", |w| {
        probe_ambiguity(w,"directory","","error");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_before_pin_file_error_cleans_owned_probe
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_before_pin_file_error_cleans_owned_probe() {
    isolated("fanout_c::layout_metadata_probe_faults_before_pin_file_error_cleans_owned_probe", |w| {
        probe_ambiguity(w,"file","","error");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_inode_flags_after_pin_remain_pending
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_changed_inode_flags_after_pin_remain_pending() {
    isolated("fanout_c::layout_metadata_probe_faults_changed_inode_flags_after_pin_remain_pending", |w| {
        probe_ambiguity(w,"file","flags","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_nonce_after_pin_remains_pending
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_changed_nonce_after_pin_remains_pending() {
    isolated("fanout_c::layout_metadata_probe_faults_changed_nonce_after_pin_remains_pending", |w| {
        probe_ambiguity(w,"file","nonce","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_pin_flags_with_unchanged_inode_remain_pending
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_changed_pin_flags_with_unchanged_inode_remain_pending() {
    isolated("fanout_c::layout_metadata_probe_faults_changed_pin_flags_with_unchanged_inode_remain_pending", |w| {
        probe_ambiguity(w,"file","pin-flags","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_foreign_replacement_after_pin_remains_pending
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_foreign_replacement_after_pin_remains_pending() {
    isolated("fanout_c::layout_metadata_probe_faults_foreign_replacement_after_pin_remains_pending", |w| {
        probe_ambiguity(w,"file","inode","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_fresh_inspection_refuses_unowned_probe_without_mutating_sources
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_fresh_inspection_refuses_unowned_probe_without_mutating_sources() {
    isolated("fanout_c::layout_metadata_probe_faults_fresh_inspection_refuses_unowned_probe_without_mutating_sources", |w| {
        unowned_probe(w);
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_unpinned_directory_remains_pending_until_operator_removes_it
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_unpinned_directory_remains_pending_until_operator_removes_it() {
    isolated("fanout_c::layout_metadata_probe_faults_unpinned_directory_remains_pending_until_operator_removes_it", |w| {
        probe_ambiguity(w,"directory","","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_unpinned_file_remains_pending_until_operator_removes_it
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_faults_unpinned_file_remains_pending_until_operator_removes_it() {
    isolated("fanout_c::layout_metadata_probe_faults_unpinned_file_remains_pending_until_operator_removes_it", |w| {
        probe_ambiguity(w,"file","","exit");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_changed_attempt_plan_refuses_without_deletion
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_retention_changed_attempt_plan_refuses_without_deletion() {
    isolated("fanout_c::layout_metadata_probe_retention_changed_attempt_plan_refuses_without_deletion", |w| {
        metadata_retention(w,"plan");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_foreign_probe_at_completed_path_refuses_without_deletion
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_retention_foreign_probe_at_completed_path_refuses_without_deletion() {
    isolated("fanout_c::layout_metadata_probe_retention_foreign_probe_at_completed_path_refuses_without_deletion", |w| {
        metadata_retention(w,"foreign");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_missing_attempt_completion_refuses_without_deletion
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_retention_missing_attempt_completion_refuses_without_deletion() {
    isolated("fanout_c::layout_metadata_probe_retention_missing_attempt_completion_refuses_without_deletion", |w| {
        metadata_retention(w,"missing");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_reviewed_cleanup_retains_completed_attempts_and_historical_receipt
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_retention_reviewed_cleanup_retains_completed_attempts_and_historical_receipt() {
    isolated("fanout_c::layout_metadata_probe_retention_reviewed_cleanup_retains_completed_attempts_and_historical_receipt", |w| {
        metadata_retention(w,"");
    });
}

// deferred obligation: tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_unknown_pin_refuses_without_deletion
#[test]
#[ignore = "recovery_frontier"]
fn layout_metadata_probe_retention_unknown_pin_refuses_without_deletion() {
    isolated("fanout_c::layout_metadata_probe_retention_unknown_pin_refuses_without_deletion", |w| {
        metadata_retention(w,"pin");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_apply_rejects_changed_reviewed_quiescence_attestation
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_apply_rejects_changed_reviewed_quiescence_attestation() {
    isolated("fanout_c::recovery_identity_closure_contract_apply_rejects_changed_reviewed_quiescence_attestation", |w| {
        quiescence_change(w);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_authority_fetch_push_disagreement_refuses_recovery_inspection
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_authority_fetch_push_disagreement_refuses_recovery_inspection() {
    isolated("fanout_c::recovery_identity_closure_contract_authority_fetch_push_disagreement_refuses_recovery_inspection", |w| {
        inspect_identity_refusal(w,"authority");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_darwin_acl_refuses_without_modification
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_darwin_acl_refuses_without_modification() {
    isolated("fanout_c::recovery_identity_closure_contract_darwin_acl_refuses_without_modification", |w| {
        inspect_identity_refusal(w,"acl");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_duplicate_uuid_text_does_not_establish_raw_router_identity
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_duplicate_uuid_text_does_not_establish_raw_router_identity() {
    isolated("fanout_c::recovery_identity_closure_contract_duplicate_uuid_text_does_not_establish_raw_router_identity", |w| {
        inspect_identity_refusal(w,"uuid");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_head_or_index_change_after_preview_refuses_before_bank_mutation
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_head_or_index_change_after_preview_refuses_before_bank_mutation() {
    isolated("fanout_c::recovery_identity_closure_contract_head_or_index_change_after_preview_refuses_before_bank_mutation", |w| {
        apply_changed_binding(w,"head-index");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_live_bank_lock_refuses_inspection_without_stealing_lock
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_live_bank_lock_refuses_inspection_without_stealing_lock() {
    isolated("fanout_c::recovery_identity_closure_contract_live_bank_lock_refuses_inspection_without_stealing_lock", |w| {
        inspect_identity_refusal(w,"lock");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_registry_change_after_preview_refuses_apply
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_registry_change_after_preview_refuses_apply() {
    isolated("fanout_c::recovery_identity_closure_contract_registry_change_after_preview_refuses_apply", |w| {
        apply_changed_binding(w,"registry");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_settings_or_bank_parent_change_after_preview_refuses_apply
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_settings_or_bank_parent_change_after_preview_refuses_apply() {
    isolated("fanout_c::recovery_identity_closure_contract_settings_or_bank_parent_change_after_preview_refuses_apply", |w| {
        apply_changed_binding(w,"settings-parent");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_unsupported_extended_attributes_refuse_without_modification
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_unsupported_extended_attributes_refuse_without_modification() {
    isolated("fanout_c::recovery_identity_closure_contract_unsupported_extended_attributes_refuse_without_modification", |w| {
        inspect_identity_refusal(w,"xattr");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_private_backup_does_not_replace_available_canonical_guidance
#[test]
#[ignore = "recovery_frontier"]
fn recovery_backup_dependency_contract_private_backup_does_not_replace_available_canonical_guidance() {
    isolated("fanout_c::recovery_backup_dependency_contract_private_backup_does_not_replace_available_canonical_guidance", |w| {
        root_semantics(w,"backup");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_apply_and_child_rollback_preserve_exact_per_path_xattrs
#[test]
#[ignore = "recovery_frontier"]
fn darwin_provenance_recovery_contract_apply_and_child_rollback_preserve_exact_per_path_xattrs() {
    isolated("fanout_c::darwin_provenance_recovery_contract_apply_and_child_rollback_preserve_exact_per_path_xattrs", |w| {
        provenance(w,"preserve");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_changed_metadata_refuses_before_replacement
#[test]
#[ignore = "recovery_frontier"]
fn darwin_provenance_recovery_contract_changed_metadata_refuses_before_replacement() {
    isolated("fanout_c::darwin_provenance_recovery_contract_changed_metadata_refuses_before_replacement", |w| {
        provenance(w,"changed");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_nondefault_inode_flags_refuse_inspection_without_mutation
#[test]
#[ignore = "recovery_frontier"]
fn darwin_provenance_recovery_contract_nondefault_inode_flags_refuse_inspection_without_mutation() {
    isolated("fanout_c::darwin_provenance_recovery_contract_nondefault_inode_flags_refuse_inspection_without_mutation", |w| {
        provenance(w,"flags");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_reviewed_wrong_provenance_refuses_without_replacement
#[test]
#[ignore = "recovery_frontier"]
fn darwin_provenance_recovery_contract_reviewed_wrong_provenance_refuses_without_replacement() {
    isolated("fanout_c::darwin_provenance_recovery_contract_reviewed_wrong_provenance_refuses_without_replacement", |w| {
        provenance(w,"wrong");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_unknown_xattr_refuses_inspection_without_mutation
#[test]
#[ignore = "recovery_frontier"]
fn darwin_provenance_recovery_contract_unknown_xattr_refuses_inspection_without_mutation() {
    isolated("fanout_c::darwin_provenance_recovery_contract_unknown_xattr_refuses_inspection_without_mutation", |w| {
        provenance(w,"unknown");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_unsafe_or_oversized_indivisible_source_cannot_become_a_pending_frontier
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_unsafe_or_oversized_indivisible_source_cannot_become_a_pending_frontier() {
    isolated("fanout_c::recovery_frontier_closure_contract_unsafe_or_oversized_indivisible_source_cannot_become_a_pending_frontier", |w| {
        unsafe_pending(w,false);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_root_semantics.py::RecoveryRootSemanticsContract.test_historical_provenance_cannot_be_rewritten_as_layout_repair
#[test]
#[ignore = "recovery_frontier"]
fn recovery_root_semantics_contract_historical_provenance_cannot_be_rewritten_as_layout_repair() {
    isolated("fanout_c::recovery_root_semantics_contract_historical_provenance_cannot_be_rewritten_as_layout_repair", |w| {
        root_semantics(w,"provenance");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_root_semantics.py::RecoveryRootSemanticsContract.test_historical_source_and_tombstone_survive_repair_with_exact_originals
#[test]
#[ignore = "recovery_frontier"]
fn recovery_root_semantics_contract_historical_source_and_tombstone_survive_repair_with_exact_originals() {
    isolated("fanout_c::recovery_root_semantics_contract_historical_source_and_tombstone_survive_repair_with_exact_originals", |w| {
        root_semantics(w,"preserve");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_root_semantics.py::RecoveryRootSemanticsContract.test_terminal_marker_cannot_be_discarded_because_backup_exists
#[test]
#[ignore = "recovery_frontier"]
fn recovery_root_semantics_contract_terminal_marker_cannot_be_discarded_because_backup_exists() {
    isolated("fanout_c::recovery_root_semantics_contract_terminal_marker_cannot_be_discarded_because_backup_exists", |w| {
        root_semantics(w,"discard");
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_semantic_dependencies.py::RecoverySemanticDependenciesContract.test_transcript_source_cannot_be_copied_or_hidden_as_pending
#[test]
#[ignore = "recovery_frontier"]
fn recovery_semantic_dependencies_contract_transcript_source_cannot_be_copied_or_hidden_as_pending() {
    isolated("fanout_c::recovery_semantic_dependencies_contract_transcript_source_cannot_be_copied_or_hidden_as_pending", |w| {
        unsafe_pending(w,true);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_setgid_collaborative_bank_mode_and_group_survive_apply
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_setgid_collaborative_bank_mode_and_group_survive_apply() {
    isolated("fanout_c::recovery_identity_closure_contract_setgid_collaborative_bank_mode_and_group_survive_apply", |w| {
        setgid_bank(w);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_changed_review_pin_refuses_interrupted_finish_without_replacement
#[test]
#[ignore = "recovery_frontier"]
fn nested_recovery_closure_changed_review_pin_refuses_interrupted_finish_without_replacement() {
    isolated("fanout_c::nested_recovery_closure_changed_review_pin_refuses_interrupted_finish_without_replacement", |w| {
        review_pin(w);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_child_transaction_marker_binds_child_operation_id
#[test]
#[ignore = "recovery_frontier"]
fn nested_recovery_closure_child_transaction_marker_binds_child_operation_id() {
    isolated("fanout_c::nested_recovery_closure_child_transaction_marker_binds_child_operation_id", |w| {
        child_marker(w);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_conflicting_historical_checkout_decisions_refuse_inspection
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_conflicting_historical_checkout_decisions_refuse_inspection() {
    isolated("fanout_c::recovery_identity_closure_contract_conflicting_historical_checkout_decisions_refuse_inspection", |w| {
        historical_checkout(w);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_raw_router_bootstrap_uses_checked_committed_identity_not_normal_router_limits
#[test]
#[ignore = "recovery_frontier"]
fn recovery_reconciliation_contract_raw_router_bootstrap_uses_checked_committed_identity_not_normal_router_limits() {
    isolated("fanout_c::recovery_reconciliation_contract_raw_router_bootstrap_uses_checked_committed_identity_not_normal_router_limits", |w| {
        raw_router(w);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_retained_rollback_repair_handoff_keeps_both_independent_obligations
#[test]
#[ignore = "recovery_frontier"]
fn recovery_reconciliation_contract_retained_rollback_repair_handoff_keeps_both_independent_obligations() {
    isolated("fanout_c::recovery_reconciliation_contract_retained_rollback_repair_handoff_keeps_both_independent_obligations", |w| {
        retained_handoff(w,false);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_unfinished_rolled_back_resolution_excludes_outer_repair_handoff
#[test]
#[ignore = "recovery_frontier"]
fn recovery_reconciliation_contract_unfinished_rolled_back_resolution_excludes_outer_repair_handoff() {
    isolated("fanout_c::recovery_reconciliation_contract_unfinished_rolled_back_resolution_excludes_outer_repair_handoff", |w| {
        retained_handoff(w,true);
    });
}

// deferred obligation: tests/contracts/test_layout_recovery_released_identity.py::ReleasedIdentityHoldContract.test_incapable_current_runtime_cannot_apply_reviewed_layout
#[test]
#[ignore = "recovery_frontier"]
fn released_identity_hold_contract_incapable_current_runtime_cannot_apply_reviewed_layout() {
    isolated("fanout_c::released_identity_hold_contract_incapable_current_runtime_cannot_apply_reviewed_layout", |w| {
        incapable_runtime(w);
    });
}

