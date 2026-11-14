# fanout-c report

## Files changed

- `tests/t2_layout_recovery.rs` — append-only module registration and optional tombstone version in test request fixture.
- `tests/t1_recovery_transactions.rs` — append-only module registration; extend real World fixture with configurable source bytes/setup/review and selected-unit admission; retain typed existing-record identities.
- `tests/support/fanout_c.rs` — recovery apply/restore/retention/probe, identity, unsafe-source, historical-root and handoff journeys.
- `tests/support/fanout_c_cases.rs` — 193 independently named deferred cases.
- `tests/support/fanout_c_nested.rs` — real CLI nested/ledger/comparison/admission/consumer/catalog closure journeys.
- `tests/support/fanout_c_nested_cases.rs` — 91 independently named deferred cases.
- `tests/support/fanout_c_policies.rs` — eight verbatim synthetic policy fixture texts captured from baseline source (not product definitions).

## Tests added per baseline file

All 356 assigned unported/partial selectors are accounted for: 284 new written frontier-dependent tests; 11 already-written ignored public tests reused; 61 not-applicable proposals below. The new cases use `#[ignore = "recovery_frontier"]`, and `// deferred obligation:` rather than ledger annotations, as required while the provider is absent. Existing lower-level partial probe tests are not counted as full ports.

| Baseline file | New deferred tests | Existing deferred tests | N/A proposals |
|---|---:|---:|---:|
| test_layout_apply_faults.py | 102 | 0 | 0 |
| test_layout_checkpoint_faults.py | 0 | 4 | 0 |
| test_layout_comparison_byte_boundaries.py | 5 | 0 | 0 |
| test_layout_damaged_evidence.py | 4 | 0 | 0 |
| test_layout_legacy_child.py | 0 | 0 | 2 |
| test_layout_legacy_hold_receipt.py | 0 | 0 | 3 |
| test_layout_legacy_markers.py | 0 | 0 | 34 |
| test_layout_legacy_nested.py | 0 | 0 | 9 |
| test_layout_legacy_stale.py | 0 | 0 | 2 |
| test_layout_legacy_swap.py | 0 | 0 | 4 |
| test_layout_metadata_probe_faults.py | 35 | 0 | 1 |
| test_layout_metadata_probe_retention.py | 5 | 0 | 0 |
| test_layout_nested_faults.py | 30 | 0 | 0 |
| test_layout_nested_reconciliation.py | 0 | 6 | 0 |
| test_layout_recovery_backup_capture.py | 1 | 0 | 0 |
| test_layout_recovery_backup_dependency.py | 4 | 0 | 0 |
| test_layout_recovery_catalog.py | 1 | 0 | 0 |
| test_layout_recovery_comparison.py | 1 | 0 | 0 |
| test_layout_recovery_darwin_provenance.py | 5 | 0 | 0 |
| test_layout_recovery_eight_admission.py | 1 | 0 | 0 |
| test_layout_recovery_frontier.py | 4 | 0 | 0 |
| test_layout_recovery_frontier_closure.py | 18 | 0 | 0 |
| test_layout_recovery_frontier_substitution.py | 3 | 0 | 0 |
| test_layout_recovery_identity_closure.py | 16 | 1 | 0 |
| test_layout_recovery_legacy.py | 0 | 0 | 1 |
| test_layout_recovery_nested_closure.py | 6 | 0 | 2 |
| test_layout_recovery_partial_admission.py | 2 | 0 | 0 |
| test_layout_recovery_reconciliation.py | 6 | 0 | 0 |
| test_layout_recovery_released_identity.py | 1 | 0 | 2 |
| test_layout_recovery_root_semantics.py | 3 | 0 | 0 |
| test_layout_recovery_runtime_rollback.py | 0 | 0 | 1 |
| test_layout_recovery_semantic_dependencies.py | 3 | 0 | 0 |
| test_layout_recovery_unsafe_apply.py | 1 | 0 | 0 |
| test_layout_resolution_faults.py | 5 | 0 | 0 |
| test_layout_restore_faults.py | 4 | 0 | 0 |
| test_layout_retention_faults.py | 14 | 0 | 0 |
| test_layout_revision_faults.py | 4 | 0 | 0 |

## Annotations count

0 new ledger annotations. Ran `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/support/fanout_c_cases.rs tests/support/fanout_c_nested_cases.rs` in this worktree: `marked ported=0 partial=0`. Frontier-blocked obligations intentionally remain unannotated. No N/A ledger changes were made.

## Ignored-on-frontier ids

### New tests (284)

- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_hold_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_hold_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_sentinel_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_sentinel_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_swap_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_after_swap_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_journal_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_journal_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_marker_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_marker_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_receipt_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_sentinel_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_sentinel_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_swap_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_before_swap_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_after_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_after_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_before_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_hold_swap_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_intent_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_index_intent_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_journal_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_marker_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_marker_removal_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_marker_removal_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_originals_completed_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_originals_completed_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_partial_staging_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_active_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_handoff_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_handoff_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_progress_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_readback_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_readback_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_receipt_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_handoff_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_handoff_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rename_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_active_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_handoff_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_handoff_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_replace_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_hold_after_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_hold_after_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_index_intent_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_index_intent_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_journal_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_marker_before_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_marker_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_active_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_handoff_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_handoff_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_progress_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_readback_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_readback_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_receipt_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_handoff_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_handoff_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_rename_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_active_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_handoff_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_handoff_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_replace_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_router_before_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_router_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_after_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_after_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_before_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_sentinel_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_after_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_after_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_before_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_swap_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_active_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_rollback_temp_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_router_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_after_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_after_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_before_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_sentinel_retirement_before_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_active_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_active_exit`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_router_error`
- `tests/contracts/test_layout_apply_faults.py::LayoutApplyFaults.test_temp_router_exit`
- `tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_after_receipt_error`
- `tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_probe_journal_error`
- `tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_probe_journal_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_0_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_0_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_1_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_1_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_2_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_2_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_3_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_delete_3_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_intent_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_intent_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_progress_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_progress_exit`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_tombstone_error`
- `tests/contracts/test_layout_retention_faults.py::LayoutRetentionFaults.test_tombstone_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_attempt_journal_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_attempt_journal_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_0_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_0_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_1_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_1_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_2_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_2_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_3_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_created_3_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_directories_removed_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_directories_removed_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_journal_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_journal_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_created_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_created_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_removed_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_late_removed_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_0_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_0_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_1_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_1_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_2_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_2_exit`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_3_error`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_removed_3_exit`
- `tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_interrupted_external_file_refuses_finish_and_rollback`
- `tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_interrupted_metadata_edit_refuses_finish_and_rollback`
- `tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_original_evidence_refuses_changed_or_missing`
- `tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_review_refuses_changed_or_missing`
- `tests/contracts/test_layout_restore_faults.py::LayoutRestoreFaults.test_partial_state_and_validate_refuse_without_adoption`
- `tests/contracts/test_layout_recovery_unsafe_apply.py::RecoveryUnsafeApplyContract.test_changed_secret_or_oversized_pending_original_refuses_apply_before_mutation`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_before_pin_directory_error_cleans_owned_probe`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_before_pin_file_error_cleans_owned_probe`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_inode_flags_after_pin_remain_pending`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_nonce_after_pin_remains_pending`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_changed_pin_flags_with_unchanged_inode_remain_pending`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_foreign_replacement_after_pin_remains_pending`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_fresh_inspection_refuses_unowned_probe_without_mutating_sources`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_unpinned_directory_remains_pending_until_operator_removes_it`
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_unpinned_file_remains_pending_until_operator_removes_it`
- `tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_changed_attempt_plan_refuses_without_deletion`
- `tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_foreign_probe_at_completed_path_refuses_without_deletion`
- `tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_missing_attempt_completion_refuses_without_deletion`
- `tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_reviewed_cleanup_retains_completed_attempts_and_historical_receipt`
- `tests/contracts/test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.test_unknown_pin_refuses_without_deletion`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_before_receipt_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_before_resolution_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_before_resolution_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_after_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_after_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_before_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_hold_restore_before_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_after_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_after_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_before_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_marker_before_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_plan_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_plan_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_router_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_router_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_target_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_progress_target_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_receipt_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_receipt_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_router_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_router_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_target_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rename_target_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_retirement_binding_error`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_retirement_binding_exit`
- `tests/contracts/test_layout_nested_faults.py::LayoutNestedFaults.test_rollback_after_rename_error`
- `tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_after_head_error`
- `tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_before_head_exit`
- `tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_marker_removal_error`
- `tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_receipt_error`
- `tests/contracts/test_layout_resolution_faults.py::LayoutResolutionFaults.test_sentinel_removal_exit`
- `tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_after_head_error`
- `tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_after_head_exit`
- `tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_before_head_error`
- `tests/contracts/test_layout_revision_faults.py::LayoutRevisionFaults.test_before_head_exit`
- `tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_revision_rejects_overlap_and_loss_then_exact_retry_is_historical`
- `tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_completed_repair_without_later_admission_can_restore`
- `tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_later_admission_refuses_completed_repair_restore`
- `tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_later_ledger_resolution_blocks_restore_without_canonical_change`
- `tests/contracts/test_layout_recovery_comparison.py::RecoveryComparisonContract.test_lexical_miss_omission_and_substituted_frontier_never_prove_novelty`
- `tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_corrupt_private_index_blocks_public_comparison_without_canonical_write`
- `tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_foreign_claim_and_old_generation_refuse_then_fresh_review_succeeds`
- `tests/contracts/test_layout_recovery_frontier_substitution.py::RecoveryFrontierSubstitutionContract.test_missing_coverage_review_refuses_full_claim_without_changing_source`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_apply_rejects_changed_reviewed_quiescence_attestation`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_authority_fetch_push_disagreement_refuses_recovery_inspection`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_darwin_acl_refuses_without_modification`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_duplicate_uuid_text_does_not_establish_raw_router_identity`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_head_or_index_change_after_preview_refuses_before_bank_mutation`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_live_bank_lock_refuses_inspection_without_stealing_lock`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_registry_change_after_preview_refuses_apply`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_settings_or_bank_parent_change_after_preview_refuses_apply`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_unsupported_extended_attributes_refuse_without_modification`
- `tests/contracts/test_layout_recovery_semantic_dependencies.py::RecoverySemanticDependenciesContract.test_discard_requires_distinct_reviewer_and_preserves_pending_source`
- `tests/contracts/test_layout_recovery_semantic_dependencies.py::RecoverySemanticDependenciesContract.test_historical_resolution_retry_does_not_revert_later_context`
- `tests/contracts/test_layout_recovery_eight_admission.py::EightUnitAdmissionContract.test_reported_handoff_resolves_every_unit_against_actual_admission`
- `tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_removed_sentinel_is_pending_not_successful_empty_coverage`
- `tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_reported_size_pending_repair_retains_exact_private_originals_and_qualified_recall`
- `tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_rolled_back_pending_units_keep_private_ledger_and_legacy_hold`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_approval_heading_and_unresolved_version_conflict_remain_private_pending_source`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_private_comparison_does_not_put_original_hash_or_ranges_in_envelope_or_audit`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_changed_state_or_config_homes_cannot_hide_public_apply_hold`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_copied_checkout_cannot_treat_public_hold_as_complete`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_deleting_every_discovery_record_loses_the_pending_obligation`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_moved_checkout_cannot_treat_public_hold_as_complete`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_orphaned_public_hold_cannot_be_read_as_empty_frontier`
- `tests/contracts/test_layout_recovery_partial_admission.py::PartialRecoveryAdmissionContract.test_one_of_eight_is_admitted_without_consuming_the_other_sources`
- `tests/contracts/test_layout_recovery_backup_dependency.py::RecoveryBackupDependencyContract.test_private_backup_does_not_replace_available_canonical_guidance`
- `tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_apply_and_child_rollback_preserve_exact_per_path_xattrs`
- `tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_changed_metadata_refuses_before_replacement`
- `tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_nondefault_inode_flags_refuse_inspection_without_mutation`
- `tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_reviewed_wrong_provenance_refuses_without_replacement`
- `tests/contracts/test_layout_recovery_darwin_provenance.py::DarwinProvenanceRecoveryContract.test_unknown_xattr_refuses_inspection_without_mutation`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_unsafe_or_oversized_indivisible_source_cannot_become_a_pending_frontier`
- `tests/contracts/test_layout_recovery_root_semantics.py::RecoveryRootSemanticsContract.test_historical_provenance_cannot_be_rewritten_as_layout_repair`
- `tests/contracts/test_layout_recovery_root_semantics.py::RecoveryRootSemanticsContract.test_historical_source_and_tombstone_survive_repair_with_exact_originals`
- `tests/contracts/test_layout_recovery_root_semantics.py::RecoveryRootSemanticsContract.test_terminal_marker_cannot_be_discarded_because_backup_exists`
- `tests/contracts/test_layout_recovery_semantic_dependencies.py::RecoverySemanticDependenciesContract.test_transcript_source_cannot_be_copied_or_hidden_as_pending`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_setgid_collaborative_bank_mode_and_group_survive_apply`
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_changed_coverage_review_refuses_nested_finish_without_replacement`
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_changed_review_pin_refuses_interrupted_finish_without_replacement`
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_child_transaction_marker_binds_child_operation_id`
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_receipted_nested_finish_reconciles_resolution_receipt_window`
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_unfinished_resolution_excludes_nested_child_finish`
- `tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_capture_request_exact_limit_and_one_byte_over_preserve_sources`
- `tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_comparison_request_exact_limit_and_one_byte_over_preserve_sources`
- `tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_complete_proof_exact_encoded_limit_and_one_byte_over`
- `tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_disclosure_exact_encoded_limit_and_one_byte_over_preserve_sources`
- `tests/contracts/test_layout_recovery_backup_capture.py::RecoveryBackupCaptureContract.test_private_backup_cannot_substitute_independent_repository_capture`
- `tests/contracts/test_layout_recovery_frontier.py::RecoveryFrontierContract.test_checked_empty_frontier_is_explicit_and_v1_or_omitted_fields_cannot_write`
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_conflicting_historical_checkout_decisions_refuse_inspection`
- `tests/contracts/test_layout_recovery_partial_admission.py::PartialRecoveryAdmissionContract.test_oversized_bank_refuses_ordinary_read_and_capture_without_mutation`
- `tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_raw_router_bootstrap_uses_checked_committed_identity_not_normal_router_limits`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_consolidation_reviews_and_commits_without_pending_frontier`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_governance_reviews_and_commits_without_pending_frontier`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_consolidation_accepts_independently_reviewed_exact_intended_output`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_consolidation_applies_the_independently_reviewed_plan`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_governance_accepts_independently_reviewed_exact_intended_output`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_governance_applies_the_independently_reviewed_plan`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_update_accepts_independently_reviewed_exact_intended_output`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_update_applies_only_the_independently_reviewed_plan`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_update_reviews_and_commits_without_pending_frontier`
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_nested_marker_blocks_independent_scratch_resolution_until_finish`
- `tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_retained_rollback_repair_handoff_keeps_both_independent_obligations`
- `tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_standalone_receipt_retry_finishes_exact_linkage_and_retirement`
- `tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_unfinished_resolution_excludes_new_resolutions_nested_admission_and_revision`
- `tests/contracts/test_layout_recovery_reconciliation.py::RecoveryReconciliationContract.test_unfinished_rolled_back_resolution_excludes_outer_repair_handoff`
- `tests/contracts/test_layout_recovery_released_identity.py::ReleasedIdentityHoldContract.test_incapable_current_runtime_cannot_apply_reviewed_layout`
- `tests/contracts/test_layout_comparison_byte_boundaries.py::LayoutComparisonByteBoundaries.test_complete_mixed_64_rows_and_65th_refusal_below_byte_limit`
- `tests/contracts/test_layout_recovery_catalog.py::RecoveryCatalogContract.test_ledger_only_retirement_refreshes_enabled_and_disabled_recall`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_more_than_64_optional_rows_allow_selective_but_not_broad_comparison`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_partial_claims_need_two_receipts_and_independent_complete_union`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_frontier_is_included_in_update_consolidation_and_governance_routes`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_pending_repair_preserves_both_canonical_versions_for_comparison`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_reservation_allows_full_credit_proof_and_rejects_unreviewable_frontier`
- `tests/contracts/test_layout_recovery_frontier_closure.py::RecoveryFrontierClosureContract.test_reservation_transition_accepts_last_complete_frontier_and_refuses_next`

### Existing written ignored tests reused (11)

- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_publication_refuses_unicode_and_case_aliased_authority_sources` — `tests/t2_publication.rs`
- `tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_durable_canonical_receipt_reconciles_only_its_exact_journaled_head_gap` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_interrupted_pre_receipt_rollback_resumes_ordered_plan_not_forward_prefix` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_nested_retirement_resumes_active_binding_after_terminal_index` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_exact_nested_finish_resumes_sentinel_retirement_without_canonical_replay` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_exact_nested_finish_resumes_after_marker_removal_without_canonical_replay` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_exact_nested_finish_resumes_after_sentinel_removal_without_canonical_replay` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_before_receipt_error` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_before_receipt_exit` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_after_receipt_error` — `tests/t1_recovery_transactions.rs`
- `tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_after_receipt_exit` — `tests/t1_recovery_transactions.rs`

## Not-applicable proposals (61; Main decides and marks)

- `tests/contracts/test_layout_legacy_child.py::LayoutLegacyChild.test_handoff` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_child.py::LayoutLegacyChild.test_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_hold_receipt.py::LayoutLegacyHoldReceipt.test_after_hold_restore` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_hold_receipt.py::LayoutLegacyHoldReceipt.test_existing_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_hold_receipt.py::LayoutLegacyHoldReceipt.test_existing_receipt_with_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_apply_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_apply_decoy_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_apply_plain_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_apply_index` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_apply_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_apply_rename0` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_apply_rename1` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_apply_rename2` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_finish_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_finish_rename1` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_finish_rename2` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_rollback_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_rollback_rename0` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_terminal_index` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_terminal_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_terminal_removal` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_terminal_rename0` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_boundary_terminal_rename1` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_finish_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_finish_decoy_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_finish_plain` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_finish_plain_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_hold_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_hold_decoy_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_hold_plain` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_hold_plain_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_rollback_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_rollback_decoy_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_rollback_plain` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_rollback_plain_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_terminal_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_terminal_decoy_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_terminal_plain` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_markers.py::LayoutLegacyMarkers.test_terminal_plain_receipted` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_boundary_credited_unlinked` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_boundary_hold_restore` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_boundary_rename_event` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_boundary_rename_router` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_boundary_rename_target` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_with_nested_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_with_nested_receipt_with_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_without_nested_receipt` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_nested.py::LayoutLegacyNested.test_without_nested_receipt_with_decoy` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_stale.py::LayoutLegacyStaleLock.test_old_stale_lock_before_handoff` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_stale.py::LayoutLegacyStaleLock.test_old_stale_lock_recovery_preserves_interrupted_apply` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_swap.py::LayoutLegacySwap.test_old_consumers_after_hold_swap` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_swap.py::LayoutLegacySwap.test_old_consumers_after_swap` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_swap.py::LayoutLegacySwap.test_old_consumers_before_hold_swap` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_legacy_swap.py::LayoutLegacySwap.test_old_consumers_before_swap` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_recovery_legacy.py::LegacyLayoutConsumers.test_published_old_consumers_and_doctor_preserve_pending_hold` — Requires executing the published v0.0.14 Python runtime as an old reader/writer; no Rust replacement can establish the specified historical-runtime behavior. Rust hold preservation remains covered by current-runtime recovery tests.
- `tests/contracts/test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.test_legacy_stale_probe_without_marker` — Specifically executes the published v0.0.14 Python reader against an unpinned probe. The historical runtime cannot run in the Rust-only test tree.
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_interrupted_activation_refuses_incapable_only_rollback_target` — Exercises managed --remote/--ref installation/update channels, explicitly dropped by the cutover design; those channels now refuse with re-bootstrap guidance before activation. No equivalent managed-channel transaction remains.
- `tests/contracts/test_layout_recovery_nested_closure.py::NestedRecoveryClosure.test_legacy_reader_does_not_recognize_runtime_only_sentinel` — Requires executing the published Python v0.0.14 runtime; historical interpreter execution is deliberately absent from the Rust-only tree.
- `tests/contracts/test_layout_recovery_released_identity.py::ReleasedIdentityHoldContract.test_released_identity_status_preserves_public_hold` — Requires executing the published Python v0.0.14 runtime; historical interpreter execution is deliberately absent from the Rust-only tree.
- `tests/contracts/test_layout_recovery_released_identity.py::ReleasedIdentityHoldContract.test_released_managed_update_refuses_real_public_hold_without_activation` — Exercises managed --remote/--ref installation/update channels, explicitly dropped by the cutover design; those channels now refuse with re-bootstrap guidance before activation. No equivalent managed-channel transaction remains.
- `tests/contracts/test_layout_recovery_runtime_rollback.py::HeldRuntimeRollbackContract.test_interrupted_capable_update_rolls_back_exact_runtime_and_keeps_hold` — Exercises managed --remote/--ref installation/update channels, explicitly dropped by the cutover design; those channels now refuse with re-bootstrap guidance before activation. No equivalent managed-channel transaction remains.

## Product changes

None. No product modules, public APIs, traits, crates, Python files, or runtime behavior changed. Test-fixture extensions only.

## Verification

Platform: `aarch64-apple-darwin`; Darwin 27.3.0 arm64; rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1.

- `cargo fmt` — passed.
- `cargo check --offline --all-targets --features dev-tools` — passed, zero warnings. Earlier compile errors in new fixtures were repaired before this successful check.
- `cargo test --offline --features dev-tools --test t2_layout_recovery` — 12 passed, 208 ignored.
- `cargo test --offline --features dev-tools --test t1_recovery_transactions` — 4 passed, 101 ignored.
- Required ledger-mark command — passed, zero new annotations.
- No full suite, Python execution, Git writes, new crates, or tracker writes.

## Gaps found

- `recovery_frontier` remains absent. All 284 new journeys compile but are intentionally ignored; their post-frontier runtime behavior has not been executed or claimed green. Enable and run these journeys once the real provider lands; no stand-in provider was installed.
- The 11 inherited public tests are likewise frontier-blocked; no duplicate body was created for them.
- Executing the published v0.0.14 Python reader/writer remains external release qualification, not a Rust in-tree test. N/A proposals explicitly identify that historical-runtime dependency.
- Three legacy managed-channel lifecycle selectors are proposed N/A because the design removes the channel; Main must adjudicate these proposals.
- No Linux/musl run was performed in this lane.
