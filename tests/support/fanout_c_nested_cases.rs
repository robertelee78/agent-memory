// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_before_receipt_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_before_receipt_exit() {
    nested_rollback("layout-recovery-nested-before-receipt","exit","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_before_resolution_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_before_resolution_error() {
    nested_receipt("layout-recovery-admission-receipt-before-resolution","error",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_before_resolution_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_before_resolution_exit() {
    nested_receipt("layout-recovery-admission-receipt-before-resolution","exit",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_hold_restore_after_error() {
    nested_receipt("layout-recovery-after-hold-restore","error",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_hold_restore_after_exit() {
    nested_receipt("layout-recovery-after-hold-restore","exit",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_hold_restore_before_error() {
    nested_receipt("layout-recovery-before-hold-restore","error",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_hold_restore_before_exit() {
    nested_receipt("layout-recovery-before-hold-restore","exit",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_after_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_marker_after_error() {
    nested_rollback("layout-recovery-after-nested-marker","error","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_after_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_marker_after_exit() {
    nested_rollback("layout-recovery-after-nested-marker","exit","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_before_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_marker_before_error() {
    nested_rollback("layout-recovery-before-nested-marker","error","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_before_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_marker_before_exit() {
    nested_rollback("layout-recovery-before-nested-marker","exit","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_plan_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_plan_error() {
    nested_rollback("layout-recovery-nested-rollback-after-plan","error","event",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_plan_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_plan_exit() {
    nested_rollback("layout-recovery-nested-rollback-after-plan","exit","event",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_progress_error() {
    nested_rollback("layout-recovery-nested-rollback-after-progress","error","event",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_progress_exit() {
    nested_rollback("layout-recovery-nested-rollback-after-progress","exit","event",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_progress_router_error() {
    nested_rollback("layout-recovery-nested-rollback-after-progress","error","router",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_progress_router_exit() {
    nested_rollback("layout-recovery-nested-rollback-after-progress","exit","router",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_target_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_progress_target_error() {
    nested_rollback("layout-recovery-nested-rollback-after-progress","error","target",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_target_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_progress_target_exit() {
    nested_rollback("layout-recovery-nested-rollback-after-progress","exit","target",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_receipt_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_receipt_error() {
    nested_receipt("layout-recovery-nested-after-receipt","error",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_receipt_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_receipt_exit() {
    nested_receipt("layout-recovery-nested-after-receipt","exit",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rename_error() {
    nested_rollback("layout-recovery-nested-after-rename","error","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rename_exit() {
    nested_rollback("layout-recovery-nested-after-rename","exit","event",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_router_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rename_router_error() {
    nested_rollback("layout-recovery-nested-after-rename","error","router",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_router_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rename_router_exit() {
    nested_rollback("layout-recovery-nested-after-rename","exit","router",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_target_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rename_target_error() {
    nested_rollback("layout-recovery-nested-after-rename","error","target",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_target_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rename_target_exit() {
    nested_rollback("layout-recovery-nested-after-rename","exit","target",true);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_retirement_binding_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_retirement_binding_error() {
    nested_receipt("layout-recovery-retirement-after-binding","error",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_retirement_binding_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_retirement_binding_exit() {
    nested_receipt("layout-recovery-retirement-after-binding","exit",false);
}

// deferred obligation: tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rollback_after_rename_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_nested_faults_rollback_after_rename_error() {
    nested_rollback("layout-recovery-nested-rollback-after-rename","error","event",false);
}

// deferred obligation: tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_after_head_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_resolution_faults_after_head_error() {
    resolution_fault("layout-recovery-ledger-generation-after-head","error");
}

// deferred obligation: tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_before_head_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_resolution_faults_before_head_exit() {
    resolution_fault("layout-recovery-ledger-generation-before-head","exit");
}

// deferred obligation: tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_marker_removal_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_resolution_faults_marker_removal_error() {
    resolution_fault("layout-recovery-retirement-after-marker-removal","error");
}

// deferred obligation: tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_receipt_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_resolution_faults_receipt_error() {
    resolution_fault("layout-recovery-resolution-after-receipt","error");
}

// deferred obligation: tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_sentinel_removal_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_resolution_faults_sentinel_removal_exit() {
    resolution_fault("layout-recovery-retirement-after-sentinel-removal","exit");
}

// deferred obligation: tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_after_head_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_revision_faults_after_head_error() {
    revision_fault(Some(("layout-recovery-ledger-generation-after-head","error")));
}

// deferred obligation: tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_after_head_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_revision_faults_after_head_exit() {
    revision_fault(Some(("layout-recovery-ledger-generation-after-head","exit")));
}

// deferred obligation: tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_before_head_error
#[test]
#[ignore = "recovery_frontier"]
fn layout_revision_faults_before_head_error() {
    revision_fault(Some(("layout-recovery-ledger-generation-before-head","error")));
}

// deferred obligation: tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_before_head_exit
#[test]
#[ignore = "recovery_frontier"]
fn layout_revision_faults_before_head_exit() {
    revision_fault(Some(("layout-recovery-ledger-generation-before-head","exit")));
}

// deferred obligation: tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_revision_rejects_overlap_and_loss_then_exact_retry_is_historical
#[test]
#[ignore = "recovery_frontier"]
fn recovery_reconciliation_contract_revision_rejects_overlap_and_loss_then_exact_retry_is_historical() {
    revision_fault(None);
}

// deferred obligation: tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_completed_repair_without_later_admission_can_restore
#[test]
#[ignore = "recovery_frontier"]
fn recovery_backup_dependency_contract_completed_repair_without_later_admission_can_restore() {
    backup_dependency("control");
}

// deferred obligation: tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_later_admission_refuses_completed_repair_restore
#[test]
#[ignore = "recovery_frontier"]
fn recovery_backup_dependency_contract_later_admission_refuses_completed_repair_restore() {
    backup_dependency("admission");
}

// deferred obligation: tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_later_ledger_resolution_blocks_restore_without_canonical_change
#[test]
#[ignore = "recovery_frontier"]
fn recovery_backup_dependency_contract_later_ledger_resolution_blocks_restore_without_canonical_change() {
    backup_dependency("resolution");
}

// deferred obligation: tests/contracts/test_layout_recovery_comparison.py::RecoveryComparisonContract.test_lexical_miss_omission_and_substituted_frontier_never_prove_novelty
#[test]
#[ignore = "recovery_frontier"]
fn recovery_comparison_contract_lexical_miss_omission_and_substituted_frontier_never_prove_novelty() {
    frontier_proof("lexical");
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_corrupt_private_index_blocks_public_comparison_without_canonical_write
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_substitution_contract_corrupt_private_index_blocks_public_comparison_without_canonical_write() {
    frontier_proof("index");
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_foreign_claim_and_old_generation_refuse_then_fresh_review_succeeds
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_substitution_contract_foreign_claim_and_old_generation_refuse_then_fresh_review_succeeds() {
    frontier_proof("foreign-generation");
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_missing_coverage_review_refuses_full_claim_without_changing_source
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_substitution_contract_missing_coverage_review_refuses_full_claim_without_changing_source() {
    frontier_proof("missing-review");
}

// deferred obligation: tests/contracts/test_layout_recovery_semantic_dependencies.py::RecoverySemanticDependenciesContract.test_discard_requires_distinct_reviewer_and_preserves_pending_source
#[test]
#[ignore = "recovery_frontier"]
fn recovery_semantic_dependencies_contract_discard_requires_distinct_reviewer_and_preserves_pending_source() {
    semantic_discard(false);
}

// deferred obligation: tests/contracts/test_layout_recovery_semantic_dependencies.py::RecoverySemanticDependenciesContract.test_historical_resolution_retry_does_not_revert_later_context
#[test]
#[ignore = "recovery_frontier"]
fn recovery_semantic_dependencies_contract_historical_resolution_retry_does_not_revert_later_context() {
    semantic_discard(true);
}

// deferred obligation: tests/contracts/test_layout_recovery_eight_admission.py::EightUnitAdmissionContract.test_reported_handoff_resolves_every_unit_against_actual_admission
#[test]
#[ignore = "recovery_frontier"]
fn eight_unit_admission_contract_reported_handoff_resolves_every_unit_against_actual_admission() {
    eight_admission(false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_removed_sentinel_is_pending_not_successful_empty_coverage
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_contract_removed_sentinel_is_pending_not_successful_empty_coverage() {
    reported_frontier("sentinel");
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_reported_size_pending_repair_retains_exact_private_originals_and_qualified_recall
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_contract_reported_size_pending_repair_retains_exact_private_originals_and_qualified_recall() {
    reported_frontier("recall");
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_rolled_back_pending_units_keep_private_ledger_and_legacy_hold
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_contract_rolled_back_pending_units_keep_private_ledger_and_legacy_hold() {
    reported_frontier("rollback");
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_approval_heading_and_unresolved_version_conflict_remain_private_pending_source
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_approval_heading_and_unresolved_version_conflict_remain_private_pending_source() {
    pending_disclosure(true);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_private_comparison_does_not_put_original_hash_or_ranges_in_envelope_or_audit
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_private_comparison_does_not_put_original_hash_or_ranges_in_envelope_or_audit() {
    pending_disclosure(false);
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_changed_state_or_config_homes_cannot_hide_public_apply_hold
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_changed_state_or_config_homes_cannot_hide_public_apply_hold() {
    for kind in ["state","xdg-state","config","xdg-config"] { discovery_loss(kind); }
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_copied_checkout_cannot_treat_public_hold_as_complete
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_copied_checkout_cannot_treat_public_hold_as_complete() {
    discovery_loss("copy");
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_deleting_every_discovery_record_loses_the_pending_obligation
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_deleting_every_discovery_record_loses_the_pending_obligation() {
    discovery_loss("all");
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_moved_checkout_cannot_treat_public_hold_as_complete
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_moved_checkout_cannot_treat_public_hold_as_complete() {
    discovery_loss("move");
}

// deferred obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_orphaned_public_hold_cannot_be_read_as_empty_frontier
#[test]
#[ignore = "recovery_frontier"]
fn recovery_identity_closure_contract_orphaned_public_hold_cannot_be_read_as_empty_frontier() {
    discovery_loss("index");
}

// deferred obligation: tests/contracts/test_layout_recovery_partial_admission.py::PartialRecoveryAdmissionContract.test_one_of_eight_is_admitted_without_consuming_the_other_sources
#[test]
#[ignore = "recovery_frontier"]
fn partial_recovery_admission_contract_one_of_eight_is_admitted_without_consuming_the_other_sources() {
    eight_admission(true);
}

// deferred obligation: tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_changed_coverage_review_refuses_nested_finish_without_replacement
#[test]
#[ignore = "recovery_frontier"]
fn nested_recovery_closure_changed_coverage_review_refuses_nested_finish_without_replacement() {
    nested_review_drift();
}

// deferred obligation: tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_receipted_nested_finish_reconciles_resolution_receipt_window
#[test]
#[ignore = "recovery_frontier"]
fn nested_recovery_closure_receipted_nested_finish_reconciles_resolution_receipt_window() {
    receipt_window();
}

// deferred obligation: tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_unfinished_resolution_excludes_nested_child_finish
#[test]
#[ignore = "recovery_frontier"]
fn nested_recovery_closure_unfinished_resolution_excludes_nested_child_finish() {
    child_resolution_exclusion();
}

// deferred obligation: tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_capture_request_exact_limit_and_one_byte_over_preserve_sources
#[test]
#[ignore = "recovery_frontier"]
fn layout_comparison_byte_boundaries_capture_request_exact_limit_and_one_byte_over_preserve_sources() {
    boundary("capture");
}

// deferred obligation: tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_comparison_request_exact_limit_and_one_byte_over_preserve_sources
#[test]
#[ignore = "recovery_frontier"]
fn layout_comparison_byte_boundaries_comparison_request_exact_limit_and_one_byte_over_preserve_sources() {
    boundary("request");
}

// deferred obligation: tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_complete_proof_exact_encoded_limit_and_one_byte_over
#[test]
#[ignore = "recovery_frontier"]
fn layout_comparison_byte_boundaries_complete_proof_exact_encoded_limit_and_one_byte_over() {
    boundary("proof");
}

// deferred obligation: tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_disclosure_exact_encoded_limit_and_one_byte_over_preserve_sources
#[test]
#[ignore = "recovery_frontier"]
fn layout_comparison_byte_boundaries_disclosure_exact_encoded_limit_and_one_byte_over_preserve_sources() {
    boundary("disclosure");
}

// deferred obligation: tests/contracts/test_layout_recovery_backup_capture.py::RecoveryBackupCaptureContract.test_private_backup_cannot_substitute_independent_repository_capture
#[test]
#[ignore = "recovery_frontier"]
fn recovery_backup_capture_contract_private_backup_cannot_substitute_independent_repository_capture() {
    backup_capture();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_checked_empty_frontier_is_explicit_and_v1_or_omitted_fields_cannot_write
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_contract_checked_empty_frontier_is_explicit_and_v1_or_omitted_fields_cannot_write() {
    empty_frontier();
}

// deferred obligation: tests/contracts/test_layout_recovery_partial_admission.py::PartialRecoveryAdmissionContract.test_oversized_bank_refuses_ordinary_read_and_capture_without_mutation
#[test]
#[ignore = "recovery_frontier"]
fn partial_recovery_admission_contract_oversized_bank_refuses_ordinary_read_and_capture_without_mutation() {
    oversized_ordinary();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_consolidation_reviews_and_commits_without_pending_frontier
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_consolidation_reviews_and_commits_without_pending_frontier() {
    consumer_flow("consolidate",false,false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_governance_reviews_and_commits_without_pending_frontier
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_governance_reviews_and_commits_without_pending_frontier() {
    consumer_flow("governance",false,false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_consolidation_accepts_independently_reviewed_exact_intended_output
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_consolidation_accepts_independently_reviewed_exact_intended_output() {
    consumer_flow("consolidate",true,false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_consolidation_applies_the_independently_reviewed_plan
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_consolidation_applies_the_independently_reviewed_plan() {
    consumer_flow("consolidate",true,true);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_governance_accepts_independently_reviewed_exact_intended_output
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_governance_accepts_independently_reviewed_exact_intended_output() {
    consumer_flow("governance",true,false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_governance_applies_the_independently_reviewed_plan
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_governance_applies_the_independently_reviewed_plan() {
    consumer_flow("governance",true,true);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_update_accepts_independently_reviewed_exact_intended_output
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_update_accepts_independently_reviewed_exact_intended_output() {
    consumer_flow("update",true,false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_update_applies_only_the_independently_reviewed_plan
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_update_applies_only_the_independently_reviewed_plan() {
    consumer_flow("update",true,true);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_update_reviews_and_commits_without_pending_frontier
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_update_reviews_and_commits_without_pending_frontier() {
    consumer_flow("update",false,false);
}

// deferred obligation: tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_nested_marker_blocks_independent_scratch_resolution_until_finish
#[test]
#[ignore = "recovery_frontier"]
fn nested_recovery_closure_nested_marker_blocks_independent_scratch_resolution_until_finish() {
    nested_scratch_exclusion();
}

// deferred obligation: tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_standalone_receipt_retry_finishes_exact_linkage_and_retirement
#[test]
#[ignore = "recovery_frontier"]
fn recovery_reconciliation_contract_standalone_receipt_retry_finishes_exact_linkage_and_retirement() {
    standalone_retry();
}

// deferred obligation: tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_unfinished_resolution_excludes_new_resolutions_nested_admission_and_revision
#[test]
#[ignore = "recovery_frontier"]
fn recovery_reconciliation_contract_unfinished_resolution_excludes_new_resolutions_nested_admission_and_revision() {
    unfinished_resolution();
}

// deferred obligation: tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_complete_mixed_64_rows_and_65th_refusal_below_byte_limit
#[test]
#[ignore = "recovery_frontier"]
fn layout_comparison_byte_boundaries_complete_mixed_64_rows_and_65th_refusal_below_byte_limit() {
    mixed_rows();
}

// deferred obligation: tests/contracts/test_layout_recovery_catalog.py::RecoveryCatalogContract.test_ledger_only_retirement_refreshes_enabled_and_disabled_recall
#[test]
#[ignore = "recovery_frontier"]
fn recovery_catalog_contract_ledger_only_retirement_refreshes_enabled_and_disabled_recall() {
    catalog_retirement();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_more_than_64_optional_rows_allow_selective_but_not_broad_comparison
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_more_than_64_optional_rows_allow_selective_but_not_broad_comparison() {
    selective_frontier();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_partial_claims_need_two_receipts_and_independent_complete_union
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_partial_claims_need_two_receipts_and_independent_complete_union() {
    partial_union();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_frontier_is_included_in_update_consolidation_and_governance_routes
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_frontier_is_included_in_update_consolidation_and_governance_routes() {
    route_frontiers();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_repair_preserves_both_canonical_versions_for_comparison
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_pending_repair_preserves_both_canonical_versions_for_comparison() {
    version_pair();
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_reservation_allows_full_credit_proof_and_rejects_unreviewable_frontier
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_reservation_allows_full_credit_proof_and_rejects_unreviewable_frontier() {
    reservation(false);
}

// deferred obligation: tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_reservation_transition_accepts_last_complete_frontier_and_refuses_next
#[test]
#[ignore = "recovery_frontier"]
fn recovery_frontier_closure_contract_reservation_transition_accepts_last_complete_frontier_and_refuses_next() {
    reservation(true);
}

