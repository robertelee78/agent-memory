# fanout-b report

**Incomplete assignment; stopped on the operator’s wrap-up instruction.** This report does not claim all 275 assigned obligations are closed.

## Files changed

- `tests/fanout_b_omp.rs`
- `tests/obligations/python-baseline-v1.json`
- `src/fs/lock.rs`
- `src/omp_transport.rs`
- `src/omp_transport/inventory.rs`
- `src/omp_transport/observe.rs`
- `src/omp_transport/providers.rs`
- `src/omp_stream/coverage.rs`
- `src/omp_stream/executor.rs`
- `src/turn_sources.rs`
- `src/omp_boundary.rs`
- `src/cli/mod.rs`

Integration base: `5764442c86ae929c4da221b1c8c7f2e52f517699`. Baseline read-only commit: `78eaf70`. No Git writes, Python execution, subagents, new crates, awa/tracker writes, or baseline edits. No refresh was needed.

## Coverage and annotations

- 275 initially unported/partial assigned IDs.
- 27 unique live annotated IDs: 19 classified fully ported, 8 explicitly partial. There are 28 live annotation comments because copy/link tests share one obligation.
- Required `ledger-mark.sh /tmp/fanout-b-ported.txt tests/fanout_b_omp.rs` ran from this worktree: 17 newly ported, 6 newly partial.
- Two preexisting partial IDs were promoted with `am-dev obligations mark --status ported --ids-file /tmp/fanout-b-promoted.txt`: native absence ordinary exits and both contended lock families. Two other preexisting partial rows remain partial. Thus 25 ledger statuses changed and 27 live IDs are represented.
- 82 unique IDs have written frontier-ignored tests (84 comments). They remain unannotated in the ledger, using human-only `// frontier obligation:` comments.
- 11 not-applicable proposals are unmarked; 7 methods are routed to OmpSmoke; 148 remaining IDs are unwritten and listed below.
- The test target contains 110 tests: final observed result 32 passed / 78 ignored. Helper subprocess entry tests contribute to the passing count. Ignored tests compiled, but were not behaviorally executed.

### Tests added per baseline file

All mappings below are in `tests/fanout_b_omp.rs`. Multiple baseline IDs may map to the same real Rust test.

| Baseline file | Assigned IDs | Live unique IDs | Frontier unique IDs | Other remaining |
|---|---:|---:|---:|---:|
| `test_omp_source_phase.py` | 53 | 7 | 12 | 34 |
| `test_omp_boundary.py` | 50 | 10 | 5 | 35 |
| `test_omp_finite_stream.py` | 43 | 3 | 1 | 39 |
| `test_omp_stored_authority_v2.py` | 33 | 2 | 16 | 15 |
| `test_omp_enrollment_progress.py` | 27 | 0 | 11 | 16 |
| `test_omp_segment_inventory_batch.py` | 16 | 0 | 10 | 6 |
| `test_held_source_enrollment.py` | 13 | 0 | 7 | 6 |
| `test_private_observer_interrupts.py` | 11 | 4 | 3 | 4 |
| `test_omp_wake_decisions.py` | 19 | 0 | 11 | 8 |
| `test_omp_stored_source_lifecycle.py` | 5 | 1 | 1 | 3 |
| `test_omp_writer_fence.py` | 3 | 0 | 3 | 0 |
| `test_omp_finite_outcome_fence.py` | 2 | 0 | 2 | 0 |

#### test_omp_source_phase.py

- `test_omp_source_phase.py::HeldLockObservations.test_actual_descriptor_identity_and_closed_observation` → `actual_descriptor_identity` — ported live
- `test_omp_source_phase.py::HeldLockObservations.test_parent_checks_only_existing_private_lock_mode` → `parent_checks_only_existing_private_lock_mode` — ported live
- `test_omp_source_phase.py::HeldLockObservations.test_nested_barrier_exposes_actual_executor_held_observation` → `nested_barrier_exposes_same_held_inode` — ported live
- `test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_foreign_thread_source_observation_without_fallback` → `foreign_thread_cannot_borrow_source_observation` — partial live
- `test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_entry_refuses_actual_source_inode_swap` → `borrowed_entry_rejects_actual_inode_swap` — partial live
- `test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_exit_refuses_actual_source_inode_swap` → `borrowed_exit_rejects_actual_inode_swap` — partial live
- `test_omp_source_phase.py::LateWindowControls.test_cancel_before_clock_cannot_revive_source_nonce` → `cancel_before_clock_cannot_revive_nonce` — ported live
- `test_omp_source_phase.py::ArmedSourcePhase.test_missing_clock_commits_pending_refs_then_registers_after_response` → `missing_clock_commits_refs_before_postresponse_registration` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_nonarmed_and_direct_observation_keep_synchronous_registration` → `nonarmed_and_direct_observations_register_synchronously` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_nonbusy_outcome_failure_never_writes_second_outcome_or_verified_response` → `nonbusy_outcome_exit_failure_never_publishes_verified_response` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_captured_source_checkpoint_failure_does_not_verify_or_schedule_drain` → `captured_source_checkpoint_failure_never_verifies_or_drains` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_executor_barrier_fd_swap` → `checkpoint_rejects_actual_executor_barrier_inode_swap` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_transport_fd_swap` → `checkpoint_rejects_actual_transport_inode_swap` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_ack_after_source_started_resolves_only_at_reservation` → `late_real_clock_ack_is_resolved_at_reservation` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_root_write_failure_after_actual_reservation_is_unknown_not_replayed` → `real_root_write_failure_after_reservation_stays_unknown` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_real_transport_lock_exit_swap_maps_committed_reservation_to_unknown` → `real_transport_exit_swap_preserves_committed_source_and_spent_reservation` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_wake_and_observer_unwind_faults_preserve_source_and_actual_reservation_fact` → `real_transport_exit_swap_preserves_committed_source_and_spent_reservation` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_pre_reservation_fault_has_no_second_outcome` → `postcommit_prereservation_refusal_has_no_second_outcome` — frontier (ignored)
- `test_omp_source_phase.py::ArmedSourcePhase.test_plain_prereserve_open_read_refuses_drift_after_verified_source` → `prereserve_open_read_refuses_drift_after_verified_source` — frontier (ignored)

#### test_omp_boundary.py

- `test_omp_boundary.py::NativeSources.test_long_segment_uses_two_shared_observations_and_ingest_is_idempotent` → `long_segment_registers_two_exact_sources_and_replays_idempotently` — frontier (ignored)
- `test_omp_boundary.py::NativeSources.test_admission_requires_sealed_registered_segment` → `admission_cannot_complete_an_open_segment` — frontier (ignored)
- `test_omp_boundary.py::NativeSources.test_gap_refuses_both_assessment_outcomes_and_preserves_marker` → `gapped_source_refuses_both_assessment_outcomes` — frontier (ignored)
- `test_omp_boundary.py::NativeSources.test_manual_marker_delete_cannot_hide_unregistered_source` → `manual_marker_delete_cannot_hide_retained_source` — frontier (ignored)
- `test_omp_boundary.py::OwnedInstructionBoundary.test_exact_carrier_and_unforgeable_witness` → `exact_owned_carrier_and_unforgeable_witness` — ported live
- `test_omp_boundary.py::OwnedInstructionBoundary.test_malformed_carriers_and_invocations_decline` → `malformed_owned_carriers_and_invocations_decline` — ported live
- `test_omp_boundary.py::OwnedInstructionBoundary.test_witness_declines_changed_receipt_file_and_permissions` → `manual_witness_declines_changed_receipt_bytes_and_modes` — ported live
- `test_omp_boundary.py::OwnedInstructionBoundary.test_link_witness_declines_retargeted_owned_link` → `link_witness_declines_retargeted_owned_link` — ported live
- `test_omp_boundary.py::OwnedInstructionBoundary.test_historical_annotation_is_closed_and_installation_independent` → `historical_annotation_is_closed_and_installation_independent` — ported live
- `test_omp_boundary.py::ExplicitNativeOwnership.test_all_never_enables_omp_and_root_is_required` → `all_never_enables_native_and_native_requires_root` — ported live
- `test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_root_and_peer_attestation_never_come_from_environment` → `root_and_peer_attestation_are_explicit_not_ambient` — ported live
- `test_omp_boundary.py::ExplicitNativeOwnership.test_saved_root_changes_and_implicit_all_removal_refuse` → `saved_root_changes_and_implicit_all_removal_refuse` — ported live
- `test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_native_install_applies_preview_for_copy_and_link` → `explicit_native_copy_install_applies_reviewed_preview` — ported live
- `test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_native_install_applies_preview_for_copy_and_link` → `explicit_native_link_install_applies_reviewed_preview` — ported live
- `test_omp_boundary.py::StoredBatchControls` → `stored_batch_controls_keep_foreign_companions_and_humans_as_source` — ported live
- `test_omp_boundary.py::NativeSources.test_late_native_tool_result_recovers_original_sealed_gap_without_reopening` → `exact_late_tool_result_recovers_sealed_gap_without_reopening_or_stale_pending` — frontier (ignored)

#### test_omp_finite_stream.py

- `test_omp_finite_stream.py::MetadataBarrierBoundaries.test_exclusively_held_barrier_refuses_close_without_metadata_write` → `exclusive_barrier_refuses_finite_close_without_metadata_write` — frontier (ignored)
- `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_preexisting_empty_slot_is_never_creator_cleanup_authority` → `preexisting_empty_slot_is_never_creator_cleanup_authority` — ported live
- `test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_local_owner_is_interruption_not_live_pid_authority` → `unheld_local_owner_is_interruption_not_live_pid_authority` — partial live
- `test_omp_finite_stream.py::CanonicalWriterContention.test_held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy` → `held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy` — partial live

#### test_omp_stored_authority_v2.py

- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checked_stored_user_is_sole_genuine_root_and_mapping_is_not_required` → `checked_stored_user_is_sole_genuine_root` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_notification_without_stored_human_has_no_origin_or_source_gap` → `notifications_never_create_native_human_authority` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_on_started_runs_once_after_preflight_before_source_write` → `on_started_runs_once_before_native_source_write` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_legacy_notification_only_cursor_is_neither_reused_mutated_nor_sealed` → `legacy_notification_cursor_stays_immutable_and_unsealed` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_neutral_hint_keeps_sealed_parent_immutable` → `neutral_hint_never_reopens_sealed_parent` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_missing_or_stale_receipt_is_not_a_production_fallback` → `missing_or_stale_receipt_never_falls_back_to_marker` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_removed_public_observer_flag_refuses_before_any_source_write` → `removed_public_observer_flag_refuses_before_source_write` — ported live
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_wake_failure_after_reservation_preserves_verified_seal_and_single_budget` → `retained_root_after_cursor_failure_stays_unknown_and_rootless` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_persistence_truncation_is_not_literal_complete_learning` → `persistence_truncation_is_never_literal_complete_learning` — partial live
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_active_unavailable_tail_is_real_gap_not_a_notification_gap` → `active_unavailable_tail_is_real_gap_not_notification_gap` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity` → `retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_new_truncated_entry_keeps_gap_and_prevents_complete_source_claim` → `retired_new_truncated_entry_keeps_gap_and_prevents_complete_claim` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resumed_fork_postfork_human_is_not_blanket_inherited` → `resumed_fork_postfork_human_is_not_blanket_inherited` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_fork_without_time_evidence_retains_nonhuman_source_with_explicit_gap` → `fork_without_time_evidence_retains_nonhuman_source_with_gap` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resolved_sealed_tool_gap_does_not_leave_stale_tool_pending_or_block_drain` → `exact_late_tool_result_recovers_sealed_gap_without_reopening_or_stale_pending` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_foreign_tool_call_id_cannot_resolve_actual_sealed_tool_gap` → `foreign_tool_call_id_never_resolves_actual_sealed_gap` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_unsafe_exact_recovery_does_not_append_gap_to_sealed_source` → `unsafe_exact_recovery_never_mutates_sealed_source` — frontier (ignored)
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_manager_subdirectory_same_bank_accepts_and_native_root_identity_stays_canonical` → `manager_subdirectory_accepts_only_canonical_bank_root_identity` — frontier (ignored)

#### test_omp_enrollment_progress.py

- `test_omp_enrollment_progress.py::EnrollmentProgress.test_public_observer_seal_and_no_learning_long_segment` → `long_segment_registers_two_exact_sources_and_replays_idempotently` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_sparse_pending_index_reads_last_slot_and_allocates_lowest_hole` → `sparse_pending_index_reads_last_slot_and_allocates_lowest_hole` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_refuses_malformed_or_symlink_last_slot_before_new_source` → `pending_index_refuses_malformed_and_symlink_last_slot` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_full_pending_index_preserves_actual_4096_slot_capacity` → `full_pending_index_preserves_actual_4096_slot_capacity` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_directory_overflow_retains_existing_source_without_allocating` → `pending_directory_overflow_preserves_sources_without_allocating` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_next_mint_refuses_a_new_raw_only_bank_locator` → `next_context_mint_refuses_new_raw_only_locator` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_context_cannot_escape_its_read_lifetime` → `marker_context_cannot_escape_read_lifetime` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_local_read_reuse_requires_exact_live_context_and_preserves_inventory` → `transaction_local_read_reuse_requires_exact_live_context` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_changed_configured_bank_before_reused_registration_writes_neither_namespace` → `changed_configured_bank_before_reused_registration_writes_neither_namespace` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_end_fresh_recheck_refuses_late_config_after_read_only_reuse` → `transaction_exit_freshly_refuses_late_authority_config_without_private_writes` — frontier (ignored)
- `test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_observation_refuses_late_authority_change_without_private_writes` → `transaction_exit_freshly_refuses_late_authority_config_without_private_writes` — frontier (ignored)

#### test_omp_segment_inventory_batch.py

- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_empty_segment_does_not_observe_shared_namespace` → `empty_segment_does_not_observe_corrupt_shared_namespace` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_all_enrollments_are_validated_before_shared_namespace_observation` → `invalid_enrollment_refuses_before_shared_namespace_observation` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_distinct_sources_and_orphan_registration_retain_exact_identities` → `distinct_sources_and_orphan_registration_retain_exact_identities` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_duplicate_or_changed_digest_refuses_only_affected_lookup` → `duplicate_or_changed_digest_refuses_only_affected_lookup` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_ordinary_missing_source_retains_only_its_own_registration_fault` → `ordinary_missing_source_retains_only_its_registration_fault` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_and_corrupt_attempts_are_not_confused_with_new_attempts` → `corrupt_and_unknown_attempts_are_not_new_attempts` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source` → `changed_sampled_attempt_refuses_even_matching_registered_source` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source` → `deleted_sampled_attempt_refuses_even_matching_registered_source` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_observation_failure_is_local_memoized_and_fault_lists_are_independent` → `observation_failure_is_local_memoized_and_fault_lists_are_independent` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_registration_between_row_lookups_cannot_invent_retained_attempt_fault` → `registration_between_lookups_cannot_invent_retained_attempt_fault` — frontier (ignored)
- `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_namespace_change_between_both_locked_passes_refuses_every_row` → `namespace_change_between_locked_passes_refuses_every_row` — frontier (ignored)

#### test_held_source_enrollment.py

- `test_held_source_enrollment.py::HeldSourceEnrollment.test_typed_context_exact_retry_and_no_ack_authority` → `held_context_exact_retry_never_grants_acknowledgement_authority` — frontier (ignored)
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_unknown_missing_corrupt_hold_and_frontier_only_change_refuse` → `missing_or_corrupt_hold_and_frontier_only_changes_refuse` — frontier (ignored)
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_error_before_source_boundaries_reconcile_one_original_observation` → `error_before_source_boundaries_reconciles_original_observation` — frontier (ignored)
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_error_source_publication_boundaries_reconcile_one_original_observation` → `error_source_publication_boundaries_reconcile_original_observation` — frontier (ignored)
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_before_source_boundaries_reconcile_one_original_observation` → `hard_exit_before_source_boundaries_reconciles_original_observation` — frontier (ignored)
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_source_publication_boundaries_reconcile_one_original_observation` → `hard_exit_source_publication_boundaries_reconcile_original_observation` — frontier (ignored)
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access` → `every_shared_reader_refuses_unknown_compatibility_before_index_access` — frontier (ignored)

#### test_private_observer_interrupts.py

- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_rechecks_normal_and_ordinary_failures` → `native_absence_rechecks_normal_and_ordinary_errors` — ported live
- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_rechecks_normal_and_ordinary_failures` → `completion_absence_rechecks_normal_and_ordinary_errors` — ported live
- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_preserves_interrupts_and_fatal_failures` → `native_absence_preserves_real_sigint_and_system_exit` — partial live
- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_preserves_interrupts_and_fatal_failures` → `completion_absence_preserves_real_sigint_and_system_exit` — partial live
- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_rechecks_normal_and_ordinary_failures` → `public_readonly_drain_rechecks_raced_transport_absence` — frontier (ignored)
- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_preserves_interrupts_and_fatal_failures` → `public_native_drain_preserves_real_sigint_and_releases_barrier` — frontier (ignored)
- `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_native_drain_preserves_interrupts_and_releases_source_barrier` → `public_native_drain_preserves_real_sigint_and_releases_barrier` — frontier (ignored)

#### test_omp_wake_decisions.py

- `test_omp_wake_decisions.py::WakeDecisions.test_not_armed_is_distinct_from_armed_without_nonce` → `not_armed_differs_from_armed_without_nonce` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_empty_verified_seal_reports_wake_not_evaluated` → `empty_verified_seal_reports_wake_not_evaluated` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_conflicting_existing_record_is_never_rewritten_or_replayed` → `conflicting_optional_record_never_rewrites_or_replays_source` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_false_observed_send_leaves_decision_unrecorded` → `real_closed_writer_leaves_decision_unrecorded_and_still_drains` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_failed_observed_send_leaves_decision_unrecorded_and_still_drains` → `real_closed_writer_leaves_decision_unrecorded_and_still_drains` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_failure_after_response_preserves_source_root_and_drain` → `non_directory_optional_writer_failure_preserves_verified_source_and_drain` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_offered_binds_actual_root_after_response_without_changing_old_evidence` → `offered_decision_binds_actual_root_and_preserves_old_evidence` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_spoofed_record_keys_types_digests_and_status_are_unbound_without_contents` → `spoofed_diagnostic_keys_types_digests_and_status_are_unbound` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_canonical_parsed_evidence_digests_survive_noncanonical_json_encoding` → `parsed_evidence_digests_survive_noncanonical_json_encoding` — frontier (ignored)
- `test_omp_wake_decisions.py::AnswerFirstQualification.test_no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status` → `no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status` — frontier (ignored)
- `test_omp_wake_decisions.py::WakeDecisions.test_retained_root_after_cursor_failure_stays_unknown_and_rootless` → `retained_root_after_cursor_failure_stays_unknown_and_rootless` — frontier (ignored)

#### test_omp_stored_source_lifecycle.py

- `test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_both_contended_lock_families_raise_typed_busy_before_body` → `private_and_completion_lock_contention_is_typed_before_body` — ported live
- `test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_observer_verification_cannot_enroll_or_promote_legacy_marker` → `observer_verification_never_enrolls_or_promotes_legacy_marker` — frontier (ignored)

#### test_omp_writer_fence.py

- `test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_exhausts_real_shared_barrier_without_canonical_write` → `public_capture_exhausts_real_shared_barrier_without_canonical_write` — frontier (ignored)
- `test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_refuses_unknown_marker_and_shared_upgrade` → `public_capture_refuses_unknown_marker_and_shared_barrier_upgrade` — frontier (ignored)
- `test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders` → `actual_capture_before_real_observer_retains_fence_order` — frontier (ignored)
- `test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders` → `actual_observer_before_real_capture_retains_fence_order` — frontier (ignored)

#### test_omp_finite_outcome_fence.py

- `test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_public_capture_cannot_interpose_before_verified_outcome` → `public_capture_cannot_interpose_before_verified_finite_outcome` — frontier (ignored)
- `test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_contended_entry_refuses_current_frame_and_stops` → `contended_entry_refuses_current_frame_and_stops_with_open_input` — frontier (ignored)

### Partial live details

- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_foreign_thread_source_observation_without_fallback`: Actual source barrier is rejected across a foreign thread and still excludes an independent exclusive FD. The complete installed transport/on_started/private-snapshot path remains unwritten.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_entry_refuses_actual_source_inode_swap`: Real inode swaps exercise SourceBarrier::borrowed entry/exit refusal. The complete finite executor attempt/STARTED/outcome/no-retirement assertions remain unwritten.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_exit_refuses_actual_source_inode_swap`: Real inode swaps exercise SourceBarrier::borrowed entry/exit refusal. The complete finite executor attempt/STARTED/outcome/no-retirement assertions remain unwritten.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_preserves_interrupts_and_fatal_failures`: Real SIGINT and exit code 66 preserve raced evidence and release OS locks. Python GeneratorExit/asyncio cancellation/MemoryError have no directly corresponding catchable Rust exception; those subcases are not claimed ported.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_preserves_interrupts_and_fatal_failures`: Real SIGINT and exit code 66 preserve raced evidence and release OS locks. Python GeneratorExit/asyncio cancellation/MemoryError have no directly corresponding catchable Rust exception; those subcases are not claimed ported.
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_local_owner_is_interruption_not_live_pid_authority`: Real canonical owner bytes and kernel locks exercise public local_registry classification; full private_enrollment_context/stream authority assertions remain blocked on the frontier and unwritten.
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy`: Real canonical owner bytes and kernel locks exercise public local_registry classification; full private_enrollment_context/stream authority assertions remain blocked on the frontier and unwritten.
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_persistence_truncation_is_not_literal_complete_learning`: Real projection rejects the persistence-truncation notice as oversized-source. Complete retired-instance transport and learning-completion path remains unwritten.

### Ignored-on-frontier IDs

Every frontier entry in the per-file mapping above has `#[ignore = "recovery_frontier"]`; exact unique IDs:

- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_error_before_source_boundaries_reconcile_one_original_observation`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_error_source_publication_boundaries_reconcile_one_original_observation`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_before_source_boundaries_reconcile_one_original_observation`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_source_publication_boundaries_reconcile_one_original_observation`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_typed_context_exact_retry_and_no_ack_authority`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_unknown_missing_corrupt_hold_and_frontier_only_change_refuse`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_admission_requires_sealed_registered_segment`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_gap_refuses_both_assessment_outcomes_and_preserves_marker`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_late_native_tool_result_recovers_original_sealed_gap_without_reopening`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_long_segment_uses_two_shared_observations_and_ingest_is_idempotent`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_manual_marker_delete_cannot_hide_unregistered_source`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_changed_configured_bank_before_reused_registration_writes_neither_namespace`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_full_pending_index_preserves_actual_4096_slot_capacity`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_context_cannot_escape_its_read_lifetime`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_observation_refuses_late_authority_change_without_private_writes`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_next_mint_refuses_a_new_raw_only_bank_locator`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_directory_overflow_retains_existing_source_without_allocating`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_refuses_malformed_or_symlink_last_slot_before_new_source`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_public_observer_seal_and_no_learning_long_segment`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_sparse_pending_index_reads_last_slot_and_allocates_lowest_hole`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_end_fresh_recheck_refuses_late_config_after_read_only_reuse`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_local_read_reuse_requires_exact_live_context_and_preserves_inventory`
- `tests/contracts/test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_contended_entry_refuses_current_frame_and_stops`
- `tests/contracts/test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_public_capture_cannot_interpose_before_verified_outcome`
- `tests/contracts/test_omp_finite_stream.py::MetadataBarrierBoundaries.test_exclusively_held_barrier_refuses_close_without_metadata_write`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_all_enrollments_are_validated_before_shared_namespace_observation`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_distinct_sources_and_orphan_registration_retain_exact_identities`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_duplicate_or_changed_digest_refuses_only_affected_lookup`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_empty_segment_does_not_observe_shared_namespace`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_and_corrupt_attempts_are_not_confused_with_new_attempts`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_namespace_change_between_both_locked_passes_refuses_every_row`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_observation_failure_is_local_memoized_and_fault_lists_are_independent`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_ordinary_missing_source_retains_only_its_own_registration_fault`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_registration_between_row_lookups_cannot_invent_retained_attempt_fault`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_ack_after_source_started_resolves_only_at_reservation`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_captured_source_checkpoint_failure_does_not_verify_or_schedule_drain`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_missing_clock_commits_pending_refs_then_registers_after_response`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_nonarmed_and_direct_observation_keep_synchronous_registration`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_nonbusy_outcome_failure_never_writes_second_outcome_or_verified_response`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_plain_prereserve_open_read_refuses_drift_after_verified_source`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_pre_reservation_fault_has_no_second_outcome`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_real_transport_lock_exit_swap_maps_committed_reservation_to_unknown`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_root_write_failure_after_actual_reservation_is_unknown_not_replayed`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_executor_barrier_fd_swap`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_transport_fd_swap`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_wake_and_observer_unwind_faults_preserve_source_and_actual_reservation_fact`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_active_unavailable_tail_is_real_gap_not_a_notification_gap`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checked_stored_user_is_sole_genuine_root_and_mapping_is_not_required`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_foreign_tool_call_id_cannot_resolve_actual_sealed_tool_gap`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_fork_without_time_evidence_retains_nonhuman_source_with_explicit_gap`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_legacy_notification_only_cursor_is_neither_reused_mutated_nor_sealed`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_manager_subdirectory_same_bank_accepts_and_native_root_identity_stays_canonical`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_missing_or_stale_receipt_is_not_a_production_fallback`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_neutral_hint_keeps_sealed_parent_immutable`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_notification_without_stored_human_has_no_origin_or_source_gap`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_on_started_runs_once_after_preflight_before_source_write`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resolved_sealed_tool_gap_does_not_leave_stale_tool_pending_or_block_drain`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resumed_fork_postfork_human_is_not_blanket_inherited`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_new_truncated_entry_keeps_gap_and_prevents_complete_source_claim`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_unsafe_exact_recovery_does_not_append_gap_to_sealed_source`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_wake_failure_after_reservation_preserves_verified_seal_and_single_budget`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_observer_verification_cannot_enroll_or_promote_legacy_marker`
- `tests/contracts/test_omp_wake_decisions.py::AnswerFirstQualification.test_no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_canonical_parsed_evidence_digests_survive_noncanonical_json_encoding`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_conflicting_existing_record_is_never_rewritten_or_replayed`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_empty_verified_seal_reports_wake_not_evaluated`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_failed_observed_send_leaves_decision_unrecorded_and_still_drains`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_false_observed_send_leaves_decision_unrecorded`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_not_armed_is_distinct_from_armed_without_nonce`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_offered_binds_actual_root_after_response_without_changing_old_evidence`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_failure_after_response_preserves_source_root_and_drain`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_retained_root_after_cursor_failure_stays_unknown_and_rootless`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_spoofed_record_keys_types_digests_and_status_are_unbound_without_contents`
- `tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders`
- `tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_exhausts_real_shared_barrier_without_canonical_write`
- `tests/contracts/test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_refuses_unknown_marker_and_shared_upgrade`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_native_drain_preserves_interrupts_and_releases_source_barrier`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_preserves_interrupts_and_fatal_failures`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_rechecks_normal_and_ordinary_failures`

### Not-applicable proposals (Main must decide/mark)

- `tests/contracts/test_omp_finite_stream.py::FrozenImports.test_unpreloaded_late_packed_module_is_refused_without_blocking_stdlib`: Python import-loader/interpreter execution is removed. Compiled Rust has no late packed-module import path; this interpreter mechanism has no remaining runtime observation.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_armed_started_precommit_memory_error_retains_original_unknown_response`: This method solely forces a catchable Python MemoryError/RecursionError. Rust exposes no equivalent catchable interpreter exception; allocation exhaustion/stack exhaustion are process-fatal rather than an injected ordinary exception. Existing real I/O, SIGINT and exit-code tests are not claimed to cover this exception mechanism.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_live_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_released_source_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_stream_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_transport_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_ordinary_started_memory_error_retains_original_unknown_response`: This method solely forces a catchable Python MemoryError/RecursionError. Rust exposes no equivalent catchable interpreter exception; allocation exhaustion/stack exhaustion are process-fatal rather than an injected ordinary exception. Existing real I/O, SIGINT and exit-code tests are not claimed to cover this exception mechanism.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_ordinary_started_recursion_error_retains_original_unknown_response`: This method solely forces a catchable Python MemoryError/RecursionError. Rust exposes no equivalent catchable interpreter exception; allocation exhaustion/stack exhaustion are process-fatal rather than an injected ordinary exception. Existing real I/O, SIGINT and exit-code tests are not claimed to cover this exception mechanism.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_resource_exhaustion_is_not_ordinary_wake_success`: This method solely forces a catchable Python MemoryError/RecursionError. Rust exposes no equivalent catchable interpreter exception; allocation exhaustion/stack exhaustion are process-fatal rather than an injected ordinary exception. Existing real I/O, SIGINT and exit-code tests are not claimed to cover this exception mechanism.
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_memory_error_keeps_fatal_policy`: This method solely forces a catchable Python MemoryError/RecursionError. Rust exposes no equivalent catchable interpreter exception; allocation exhaustion/stack exhaustion are process-fatal rather than an injected ordinary exception. Existing real I/O, SIGINT and exit-code tests are not claimed to cover this exception mechanism.
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_recursion_error_keeps_fatal_policy`: This method solely forces a catchable Python MemoryError/RecursionError. Rust exposes no equivalent catchable interpreter exception; allocation exhaustion/stack exhaustion are process-fatal rather than an injected ordinary exception. Existing real I/O, SIGINT and exit-code tests are not claimed to cover this exception mechanism.

### Routed elsewhere

Per Main, these seven are **routed to OmpSmoke lane**; not ported and not marked here:

- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_both_exact_current_human_turns_and_persistent_session_are_required` — routed to OmpSmoke lane
- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_historical_observers_cannot_qualify_new_current_session` — routed to OmpSmoke lane
- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_late_native_frames_cannot_reuse_initial_answer_or_settled_proof` — routed to OmpSmoke lane
- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_source_verifier_binds_completed_human_ids_text_roots_and_digests` — routed to OmpSmoke lane
- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_source_verifier_waits_for_fault_free_shared_registration` — routed to OmpSmoke lane
- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_stream_readback_requires_terminal_receipts_and_current_leaf_not_child_quiescence` — routed to OmpSmoke lane
- `tests/contracts/test_omp_boundary.py::NativeSmokeQualification.test_unsupported_binary_refuses_before_native_launch_or_human_turns` — routed to OmpSmoke lane

## Product changes

### PrivateLock baseline parity

`src/fs/lock.rs` records the existing-only acquisition flag. Recheck pins the original parent chain only for `existing_only=true`; default-created locks re-resolve the named path and still require the actual held inode. This matches baseline `private_lock(..., existing_only=False)` rather than refusing all parent replacements. No caller flag was changed: existing values already match baseline call sites. Read-only lock mode continues to force true. Real tests cover both preexisting and default-created hard-linked held inodes across a parent swap, plus inode substitution and actual cross-thread refusal.

LSP references were attempted for PrivateLock constructors; no language server was configured. Text fallback enumerated the baseline and Rust callers.

Callers whose **false-flag behavior changes** (parent replacement retaining the same named held inode is no longer independently rejected):

- `turn_sources::private_lock` with false outside read-only mode, including `source_barrier_at` / ordinary `source_barrier` (baseline turn_sources.py:94,168).
- Held register and retained-unidentified enrollment locks (turn_sources.py:423,623; Rust turn_sources.rs enrollment.lock callers).
- `omp_boundary::drain_source_capabilities` mutation path, transport lock (omp_boundary.py:516).
- `omp_stream::session_creation_lock` (omp_stream.py:242).
- `omp_stream::with_lock` callers with false, including Coverage stream metadata/control and close (omp_stream.py:810,910,928,1026), ordinary inventory and recovery metadata stream locks (1841 when not read-only;1904;1951).
- `Coverage::open` live.lock shared acquisition (omp_stream.py:907).
- Stream recovery inventory’s exclusive live.lock probe (omp_stream.py:1898).
- Transport inventory guard’s ordinary transport lock (omp_transport.py:1005,1027,1088,1287).
- `omp_transport::observe` ordinary transport lock (omp_transport.py:1325).

Unchanged true-flag callers: required source capabilities transport lock (omp_boundary.py:445), read-only transport guard (485), completion.lock read-only acquisition (turn_sources.py:535), explicit existing-only source barriers, and `with_lock` read-only inventory mode (omp_stream.py:1841). The default remains baseline false; true still checks parent identity.

### Concrete Harness root

`Harness` now requires a `pack_root`, with `Harness::at(&Path)` and `Harness::installed()`. The latter resolves the installed root exactly through the existing CLI resolver, then delegates. A zero-state `InstalledHarness` lazily constructs the concrete Harness for installation services, preserving first-install/CLI behavior (an eager root lookup would fail before installation). CLI wiring uses the lazy wrapper. No new trait or mock provider was added. Tests register the existing concrete InstallationActivation and Transport with the fixture’s real installed pack, rather than resolving the test runner as an installed am.

### Dev-tools fault points

All retained points call the existing token-bound `bank_writes::fault` mechanism and each call is guarded by `#[cfg(feature = "dev-tools")]`; unarmed behavior is unchanged. Nine initially-added but unused points were removed before final checks. These are the retained points, final source locations and served tests/obligations:

| File:line | Point | Served baseline test / reason |
|---|---|---|
| src/omp_transport.rs:1325 | omp.capture.before-commit | test_omp_writer_fence.py observer-first/writer-first real capture ordering (both fence tests mapped above) |
| src/omp_stream/coverage.rs:876 | omp.outcome.before-guard | test_omp_finite_outcome_fence.py finite source barrier excludes canonical writer before outcome |
| src/omp_stream/coverage.rs:887 | omp.outcome.after-commit | test_omp_source_phase.py nonbusy outcome exit failure must not publish verified response |
| src/omp_stream/executor.rs:150 | omp.before-reserve.before-open | test_omp_source_phase.py postcommit pre-reservation refusal and recheck of changed open metadata |
| src/omp_transport/observe.rs:245 | omp.reservation.before-reserve | test_omp_source_phase.py late real clock ACK; test_omp_wake_decisions.py offered evidence binding, diagnostics and noncanonical digests |
| src/omp_transport/observe.rs:252 | omp.reservation.after-root | test_omp_source_phase.py lost root acknowledgment after a real reservation; retained root and no successful continuation |
| src/omp_transport/observe.rs:258 | omp.reservation.after-cursor | test_omp_wake_decisions.py unknown rootless outcome after cursor failure |
| src/omp_transport/observe.rs:1851 | omp.checkpoint.before-recheck | test_omp_source_phase.py captured-source checkpoint fault and actual source/transport lock inode swaps |
| src/omp_transport/inventory.rs:396 | omp.join.before-shared-inventory | test_omp_segment_inventory_batch.py local memoized observation fault and independent fault lists |
| src/omp_transport/inventory.rs:399 | omp.join.after-shared-inventory | test_omp_segment_inventory_batch.py sampled registration attempt changed/deleted after inventory |
| src/omp_transport/inventory.rs:419 | omp.join.after-row-lookup | test_omp_segment_inventory_batch.py registration between row lookups |
| src/turn_sources.rs:1710 | omp.join.between-locked-passes | test_omp_segment_inventory_batch.py physical namespace mutation between two locked passes; only fault call added to this shared file |
| src/omp_boundary.rs:1098 | omp.drain.before-inventory | test_private_observer_interrupts.py raced transport absence and real SIGINT through public native drain |

No unsafe code, crates, new registration traits, production test seams, or release fallbacks were introduced.

## Verification

Platform actually run: `aarch64-apple-darwin`; Darwin 27.3.0 arm64 (`xnu-13432.60.57~99/RELEASE_ARM64_T6050`); rustc 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM23.1.1). No Linux execution/qualification claimed.

Final commands after all product/test edits:

```sh
cargo fmt
cargo check --offline --all-targets --features dev-tools
cargo test --offline --features dev-tools --test fanout_b_omp
cargo test --offline --features dev-tools --test fanout_b_omp
cargo test --offline --features dev-tools --test fanout_b_omp
```

Check completed with zero warnings. Three consecutive target runs: **32 passed, 78 ignored, 0 failed**, respectively6.23s/6.38s/6.24s (artifact14452). No full suite was run. Earlier fixture/check failures were fixed; these results are the final observed state.

## Gaps found

- **Assignment acceptance is not met.** The 148 IDs below are unwritten, not silently treated as ported or not applicable. The eight partial live IDs also retain the gaps above. The operator requested immediate wrap-up after the soft request limit.
- `recovery_frontier` remains unported in this worktree. All82 frontier-marked IDs compile only; they require behavioral execution after integration of the real frontier. Do not count ignores as passing behavioral coverage.
- Some wake fixture tests cover retained verified source and unarmed/no-window diagnostics rather than every offered-root variant; conflict/optional-writer baseline subcases need comparison before promotion.
- The stored-authority long-source truncation projection test is explicitly partial; lifecycle integration remains listed.
- Baseline runtime allocation/recursion/fatal exception subcases are not reclassified as ordinary success, mocked, or claimed equivalent to SIGINT.
- No native harness session, signed release update, Linux musl behavior, real endpoint, or latency qualification is established by these fixtures.

### Unwritten obligation IDs

- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_late_hold_registry_and_ledger_changes_retain_written_source_then_retry`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_public_layout_hold_enrolls_real_codex_and_claude_prompt_hooks`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_real_codex_claude_hooks_keep_task_active_but_boundaries_pending`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_rolled_back_raw_scope_defers_late_real_held_checkpoint_without_source_loss`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_shared_source_barrier_reentrancy_and_lifecycle_contention_preserve_sources`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_verified_hold_rejects_location_and_hold_changes_during_observation`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_held_native_preview_consumes_reviewed_envelope_without_publishing`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_managed_native_updates_read_back_only_owned_public_commands`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_native_update_ignores_missing_historical_worktree_but_preserves_required_enrollment`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_public_uninstall_keeps_pending_sources_and_disabled_owned_binding`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_public_uninstall_requires_quiescence_and_actually_drains_completed_sources`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_signed_native_updates_preserve_extension_integrity`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_capacity_retains_arrival_then_reuses_reference_after_capacity_frees`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_completed_snapshots_require_bound_drain_proof_after_marker_removal`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_crashed_pure_snapshot_never_reopens_sealed_parent`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_exact_own_hidden_batch_does_not_open_a_source_successor`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_full_backlog_and_long_unicode_repo_still_offer_one_bounded_wake`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_protected_private_root_cannot_silently_move_or_lose_binding`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_root_wake_is_once_and_later_human_carries_unassessed_old_source`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_fresh_capture_preserves_surrounding_source_and_retries`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_fresh_owned_guides_capture_exact_metadata_in_copy_and_link_modes`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_historical_owned_guide_never_authorizes_read_after_installation_changes`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_mismatched_orphan_snapshot_is_integrity_failure`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_noninstruction_artifact_read_retains_ordinary_body_not_owned_metadata`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_nonread_result_never_mints_manual_witness`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_owned_guide_alias_partial_tamper_and_foreign_reads_keep_real_gaps`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_owned_guide_rechecks_receipt_selection_and_nested_symlinks`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_preexisting_native_gap_is_not_reclassified`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_same_sweep_human_boundary_preserves_sealed_tool_gap`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_screened_invocation_does_not_hide_following_answer`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_sealed_tool_gap_does_not_authorize_late_classification`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_snapshot_publication_recovery_survives_uninstall_and_retry`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_unrelated_ancestor_churn_preserves_witness_and_public_annotation`
- `tests/contracts/test_omp_boundary.py::OwnedInstructionBoundary.test_verification_races_refuse_selected_binding_payload_and_policy_changes`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_bounded_later_sweeps_enroll_oldest_and_keep_every_new_arrival`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_committed_raw_locator_refusal_with_and_without_verified_hold`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_late_raw_locator_refusal_with_and_without_verified_hold`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_raw_nonrouter_refusal_with_and_without_verified_hold`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_symlink_locator_refusal_with_and_without_verified_hold`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_bank_drift_after_actual_scope_read`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_foreign_bank_identity_without_private_writes`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_successful_but_disagreeing_bank_locators`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_resolves_regular_and_newline_worktree_identity`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_held_read_reuse_keeps_exact_intent_pins_and_refuses_changed_hold_without_new_source`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_lost_success_reuses_observation_and_unknown_attempt_is_not_retried`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_observe_bank_drift_keeps_original_captures_refuses_final_and_next_frame_freshly_locates`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_concurrent_slot_creation_keeps_both_sources_and_next_hole`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_retries_transient_scan_change_and_refuses_persistent_churn`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_reused_locator_keeps_post_source_mutation_scope_check_and_original_fault`
- `tests/contracts/test_omp_enrollment_progress.py::EnrollmentProgress.test_shared_fault_retained_across_later_observer_progress`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_actual_transaction_at_outcome_waits_without_adopting_its_marker`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_metadata_exhaustion_keeps_source_and_open_unknown`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_arbitrary_marker_read_failure_without_writer_is_not_retryable`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_empty_held_and_unheld_slots_never_mint_scope_or_claim_known_writer`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_failed_marker_read_with_unproven_empty_slot_keeps_original_refusal`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_attempt_exit_busy_never_adopts_different_retry_record`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_attempt_exit_busy_never_adopts_partial_retry_record`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_refused_frame_durable_attempt_exit_busy_counts_once`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_request_durable_attempt_exit_busy_resumes_exactly_once`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_late_transaction_after_marker_read_is_exact_writer_deferral`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_late_transaction_before_frontier_entry_is_exact_writer_deferral`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_late_transaction_during_failed_marker_read_is_exact_writer_deferral`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_real_initializing_writer_exhaustion_cannot_verify_or_close`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_real_writer_full_owner_fsync_window_has_ex_ownership_before_probe`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_real_writer_pre_ex_empty_publication_defers_without_stealing_its_lock`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_rebound_inode_after_flock_observation_is_not_classified_as_busy`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_released_writer_with_unknown_transaction_is_hard_refusal_not_retry`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_transaction_left_after_writer_release_remains_interruption`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_empty_publication_exhaustion_retains_source_without_outcome`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_writer_arriving_after_preflight_is_classified_at_actual_identity_guard`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_writer_release_rediscovers_identity_instead_of_reusing_frame_scope`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_bank_replaced_by_symlink_before_open_is_typed_without_closing_foreign_fds`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_failed_first_ex_preserves_replacement_or_changed_empty_slot`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_initial_lstat_io_error_is_typed_without_opening_descriptors`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_interrupt_during_marker_read_is_not_reclassified_as_writer_retry`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_noncontention_first_ex_error_removes_only_creators_empty_slot`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_nonempty_short_owner_write_remains_interruption_not_initialization`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_owner_write_oserror_preserves_publication_evidence_and_closes_creator`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_real_owner_publication_between_empty_samples_is_only_initialization`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_real_recovery_wins_first_ex_without_stranding_creators_empty_slot`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_replacement_at_checked_boundaries_remains_hard_refusal`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_cannot_rebind_its_pinned_bank`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_requires_exact_safe_slot_properties`
- `tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_real_io_failure_does_not_manufacture_invalid_request_refusal`
- `tests/contracts/test_omp_finite_stream.py::CoverageFixture.test_startup_typed_busy_retries_before_open_and_ready`
- `tests/contracts/test_omp_finite_stream.py::MetadataBarrierBoundaries.test_run_final_close_rejects_barrier_inode_replacement_after_metadata_write`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_held_and_gapped_segments_do_not_gain_completion_authority`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_historical_completed_gate_observation_is_not_reused_for_lookup`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_corrupt_source_and_completion_faults_remain_visible`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_no_context_preserves_fresh_per_reference_observations`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_real_completion_between_result_and_wake_requires_new_inventory`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_type_and_key_errors_still_reach_whole_segment_fault_handling`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_ingest_never_acquires_foreign_exclusive_barrier`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_ingest_refuses_before_unreadable_native_binding`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_seal_never_acquires_foreign_exclusive_barrier`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_seal_refuses_before_unreadable_native_binding`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_busy_outcome_beforewrite_or_exit_retries_only_identical_source_after_unlock`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_busy_outcome_exit_exhaustion_keeps_retained_record_without_response_or_drain`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_cancel_after_actual_reserve_then_exit_fault_retains_one_unknown_root`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_io_failure_never_verifies_partial_seal`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_refusal_never_verifies_partial_seal`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_clock_ack_behind_accepted_16mib_frame_reaches_late_reservation`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_closed_pipe_writer_false_drains_before_next_fifo_and_final_in_full_run`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_completion_guard_discovers_fresh_locations_and_restores_live_locator`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_metadata_refuses_without_attempt_or_new_base`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_seal_refuses_without_new_private_state`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_ingest_canonical_writer_is_busy_before_binding_refusal`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_seal_canonical_writer_is_busy_before_binding_refusal`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_eof_after_actual_reserve_then_exit_fault_retains_one_unknown_root`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_oldest8_after_response_and_cli_collision_leave_canonical_state_unchanged`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_output_loss_after_actual_reserve_then_exit_fault_retains_one_unknown_root`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postresponse_busy_is_one_batch_and_leaves_discoverable_enrollments`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postresponse_control_flow_is_not_masked_or_closed`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_replacement_after_actual_reserve_then_exit_fault_retains_one_unknown_root`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_response_loss_still_drains_once_before_next_fifo_and_final`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_reused_header_nonce_decline_and_exit_fault_do_not_claim_own_reservation`
- `tests/contracts/test_omp_source_phase.py::LateWindowControls.test_nonce_cancel_replacement_eof_output_loss_and_one_shot_lookup`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_cancel_before_reservation_refuses_and_after_start_preserves_result`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checkout_retarget_after_view_validation_refuses_before_started_or_source`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_completed_historical_origin_exact_replay_is_read_only_and_changed_request_refuses`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_delayed_old_checked_view_refuses_new_session_and_real_new_human_has_one_root`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_expired_internal_delivery_window_never_reserves_root_wake`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_invalid_views_refuse_before_real_transport_contention_and_bad_receipt`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_legacy_random_root_wake_follows_exact_native_user_without_spending_new_human_budget`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_malformed_wake_state_suppresses_reservation_but_retains_and_enrolls_real_source`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_no_active_resume_paths_never_append_to_previously_sealed_instance_segment`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_pre_reservation_wake_exception_does_not_unverify_committed_source`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_random_legacy_root_with_real_reconciled_human_still_cannot_acknowledge`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_reparented_linux_window_cannot_reserve`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_replacement_and_end_after_start_preserve_result`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_same_role_position_decoy_never_resolves_real_event_gap`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_weak_historical_event_resolution_does_not_authorize_new_acknowledgement`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_prior_committed_activation_recovers_after_a_later_install_rolls_back`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_real_binding_proof_rejects_forgery_and_mid_instance_drift_before_source`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_receipt_commit_separates_bank_activation_from_rollback`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_bounded_summary_orphans_and_unreadable_namespace_do_not_change_doctor_health`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_existing_root_retains_legacy_bare_pending_without_inferred_cause`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_malformed_real_result_shapes_are_unknown_without_inventing_a_cause`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_nonseal_and_failed_source_attempts_do_not_acquire_diagnostic_rows`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_offered_writer_checks_actual_request_without_cursor_or_root_scan`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_substituted_oversize_response_never_records_an_offered_continuation`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_preserves_interrupts_and_fatal_failures`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_race_remains_pending_for_normal_and_ordinary_failure`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_source_inspection_preserves_interrupts_with_raced_completion_lock`
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_unchanged_absence_allows_normal_exit_and_preserves_ordinary_error`

## Resumed implementation — batch 1

Main requested continuation. This continuation supersedes the earlier stopped snapshot; final totals will be recomputed.

Twenty previously unwritten obligations now have tests: eight stored-authority methods, two segment-inventory methods, and ten writer-observation obligations (nine real runnable tests). There are 128 unwritten obligations remaining. Registry probes use actual independent children and kernel flocks; owner-publication faults write a real short prefix or return an approved EIO; first-acquisition faults feed approved ENOLCK/ENOTSUP into the existing error branch with the actual created descriptor.

`cargo fmt` and warning-free `cargo check --offline --all-targets --features dev-tools` succeeded. Own target: **41 passed, 88 ignored, 0 failed** (artifact14474). Newly gated tests remain unannotated on the frontier. New live annotations will be ledger-marked before final handoff.

Additional files: `src/local_registry.rs`, `src/omp_stream/wake.rs`. Further approved fault points are dev-tools-only; final report will list their final lines and served obligations. The exact added obligation comments and test names are in `tests/fanout_b_omp.rs`.

## Resumed implementation — batch 2

Forty-six of the 148 originally unwritten IDs now have tests; **102 remain unwritten**. The added batch covers ten owned/native boundary methods and all sixteen remaining enrollment-progress methods. Independent fixture variants now re-execute in separate support World instances instead of sharing installation state.

Verification: formatting and warning-free all-targets dev-tools check succeeded; `fanout_b_omp`: **41 passed, 114 ignored**; touched existing `t1_bank`: **41 passed, 4 ignored** (artifact14504).

Additional changed files are `src/scope_binding.rs`, `src/bank_writes.rs`, and `tests/t1_bank.rs`. Main approved the shared fault-selector extension: comma-separated stage lists are parsed only under dev-tools; empty/duplicate entries refuse with the existing validation error; the token check and per-stage ready/release files remain unchanged, and single-stage selection preserves its existing path. A real child test verifies two rendezvous in order, and another rejects malformed lists. Further call-only scope/index/source-publication hooks support real filesystem mutations without mock providers.

# Final handoff — authoritative replacement for earlier snapshots

This section supersedes every earlier count, verification claim, mapping, and fault-location table in this append-only report. **Acceptance remains incomplete: 52 assigned IDs have no Rust test, and eight live IDs remain partial.** The operator requested wrap-up. No missing obligation is claimed ported.

## Files changed (final)

- `tests/fanout_b_omp.rs`
- `tests/obligations/python-baseline-v1.json`
- `src/fs/lock.rs`
- `src/omp_transport.rs`
- `src/omp_transport/inventory.rs`
- `src/omp_transport/observe.rs`
- `src/omp_transport/providers.rs`
- `src/omp_stream/coverage.rs`
- `src/omp_stream/executor.rs`
- `src/turn_sources.rs`
- `src/omp_boundary.rs`
- `src/omp_installation.rs`
- `src/cli/mod.rs`
- `src/local_registry.rs`
- `src/scope_binding.rs`
- `src/bank_writes.rs`
- `tests/t1_bank.rs`

Integration base remains `5764442c86ae929c4da221b1c8c7f2e52f517699`; baseline `78eaf70`. Per Main, no byte refresh was performed after the pack changes in `f6ac3c1e`. `src/omp_stream/wake.rs` had only an unused added fault call; that call was removed, leaving no intended change to that file.

## Final annotation and coverage counts

- 275 assigned initially unported/partial IDs.
- 204 unique IDs have written test mappings (207 annotation comments): 31 live, 166 ignored on `recovery_frontier`, and seven ignored for the unavailable f6ac3c1e pack surface.
- Live IDs: 23 fully ported, eight explicitly partial. Only the 31 live IDs use machine `// obligation:` comments.
- Frontier and pack-surface cases use human-only comments and are left unannotated in the ledger.
- 12 not-applicable proposals remain unmarked; seven methods are routed to OmpSmoke, unmarked; 52 IDs remain unwritten.
- Required ledger-mark script ran again from this worktree with `/tmp/fanout-b-ported-final.txt`, marking **10 ported, zero partial** (artifact14611). Earlier invocation marked17 ported/six partial; two existing partials were promoted. Six earlier ported claims were explicitly restored to unported when their minimal-pack fixtures were removed and replaced with the actual shipped pack, now ignored pending f6ac3c1e. Current lane live claims are23 ported/eight partial, not the earlier27-ID snapshot.
- Target contains205 test functions: final observed execution34 passed/171 ignored; ignored tests compiled but were not behaviorally executed.

### Tests added per baseline file

| Baseline | Assigned | Live | Frontier | Pack surface | N/A proposed | Routed | Unwritten |
|---|---:|---:|---:|---:|---:|---:|---:|
| `test_held_source_enrollment.py` | 13 | 0 | 7 | 0 | 0 | 0 | 6 |
| `test_omp_boundary.py` | 50 | 4 | 21 | 7 | 0 | 7 | 11 |
| `test_omp_enrollment_progress.py` | 27 | 0 | 27 | 0 | 0 | 0 | 0 |
| `test_omp_finite_outcome_fence.py` | 2 | 0 | 2 | 0 | 0 | 0 | 0 |
| `test_omp_finite_stream.py` | 43 | 13 | 18 | 0 | 1 | 0 | 11 |
| `test_omp_segment_inventory_batch.py` | 16 | 0 | 12 | 0 | 0 | 0 | 4 |
| `test_omp_source_phase.py` | 53 | 7 | 29 | 0 | 8 | 0 | 9 |
| `test_omp_stored_authority_v2.py` | 33 | 2 | 28 | 0 | 1 | 0 | 2 |
| `test_omp_stored_source_lifecycle.py` | 5 | 1 | 1 | 0 | 0 | 0 | 3 |
| `test_omp_wake_decisions.py` | 19 | 0 | 11 | 0 | 2 | 0 | 6 |
| `test_omp_writer_fence.py` | 3 | 0 | 3 | 0 | 0 | 0 | 0 |
| `test_private_observer_interrupts.py` | 11 | 4 | 7 | 0 | 0 | 0 | 0 |

### Complete final ID-to-test mapping, including ignored-on-frontier IDs

Every test below is in `tests/fanout_b_omp.rs`. Each `frontier` row is explicitly `#[ignore = "recovery_frontier"]` and unannotated in the ledger. Each `pack surface` row is explicitly `#[ignore = "needs f6ac3c1e pack surface (refresh/boundary)"]` and unannotated. The baseline prefix for every ID is `tests/contracts/`.

| Baseline ID | Rust test | State |
|---|---|---|
| `test_omp_source_phase.py::HeldLockObservations.test_actual_descriptor_identity_and_closed_observation` | `actual_descriptor_identity` | ported live |
| `test_omp_source_phase.py::HeldLockObservations.test_parent_checks_only_existing_private_lock_mode` | `parent_checks_only_existing_private_lock_mode` | ported live |
| `test_omp_source_phase.py::HeldLockObservations.test_nested_barrier_exposes_actual_executor_held_observation` | `nested_barrier_exposes_same_held_inode` | ported live |
| `test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_foreign_thread_source_observation_without_fallback` | `foreign_thread_cannot_borrow_source_observation` | partial live |
| `test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_entry_refuses_actual_source_inode_swap` | `borrowed_entry_rejects_actual_inode_swap` | partial live |
| `test_omp_source_phase.py::ArmedSourcePhase.test_executor_borrowed_exit_refuses_actual_source_inode_swap` | `borrowed_exit_rejects_actual_inode_swap` | partial live |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checked_stored_user_is_sole_genuine_root_and_mapping_is_not_required` | `checked_stored_user_is_sole_genuine_root` | frontier |
| `test_omp_boundary.py::NativeSources.test_long_segment_uses_two_shared_observations_and_ingest_is_idempotent` | `long_segment_registers_two_exact_sources_and_replays_idempotently` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_public_observer_seal_and_no_learning_long_segment` | `long_segment_registers_two_exact_sources_and_replays_idempotently` | frontier |
| `test_omp_boundary.py::NativeSources.test_admission_requires_sealed_registered_segment` | `admission_cannot_complete_an_open_segment` | frontier |
| `test_omp_boundary.py::NativeSources.test_gap_refuses_both_assessment_outcomes_and_preserves_marker` | `gapped_source_refuses_both_assessment_outcomes` | frontier |
| `test_omp_boundary.py::NativeSources.test_manual_marker_delete_cannot_hide_unregistered_source` | `manual_marker_delete_cannot_hide_retained_source` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_notification_without_stored_human_has_no_origin_or_source_gap` | `notifications_never_create_native_human_authority` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_on_started_runs_once_after_preflight_before_source_write` | `on_started_runs_once_before_native_source_write` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_legacy_notification_only_cursor_is_neither_reused_mutated_nor_sealed` | `legacy_notification_cursor_stays_immutable_and_unsealed` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_neutral_hint_keeps_sealed_parent_immutable` | `neutral_hint_never_reopens_sealed_parent` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_missing_or_stale_receipt_is_not_a_production_fallback` | `missing_or_stale_receipt_never_falls_back_to_marker` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_removed_public_observer_flag_refuses_before_any_source_write` | `removed_public_observer_flag_refuses_before_source_write` | ported live |
| `test_omp_source_phase.py::LateWindowControls.test_cancel_before_clock_cannot_revive_source_nonce` | `cancel_before_clock_cannot_revive_nonce` | ported live |
| `test_omp_source_phase.py::ArmedSourcePhase.test_missing_clock_commits_pending_refs_then_registers_after_response` | `missing_clock_commits_refs_before_postresponse_registration` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_nonarmed_and_direct_observation_keep_synchronous_registration` | `nonarmed_and_direct_observations_register_synchronously` | frontier |
| `test_omp_finite_stream.py::MetadataBarrierBoundaries.test_exclusively_held_barrier_refuses_close_without_metadata_write` | `exclusive_barrier_refuses_finite_close_without_metadata_write` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_exact_carrier_and_unforgeable_witness` | `exact_owned_carrier_and_unforgeable_witness` | pack surface |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_malformed_carriers_and_invocations_decline` | `malformed_owned_carriers_and_invocations_decline` | pack surface |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_witness_declines_changed_receipt_file_and_permissions` | `manual_witness_declines_changed_receipt_bytes_and_modes` | pack surface |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_link_witness_declines_retargeted_owned_link` | `link_witness_declines_retargeted_owned_link` | pack surface |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_historical_annotation_is_closed_and_installation_independent` | `historical_annotation_is_closed_and_installation_independent` | pack surface |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_empty_segment_does_not_observe_shared_namespace` | `empty_segment_does_not_observe_corrupt_shared_namespace` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_all_enrollments_are_validated_before_shared_namespace_observation` | `invalid_enrollment_refuses_before_shared_namespace_observation` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_distinct_sources_and_orphan_registration_retain_exact_identities` | `distinct_sources_and_orphan_registration_retain_exact_identities` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_duplicate_or_changed_digest_refuses_only_affected_lookup` | `duplicate_or_changed_digest_refuses_only_affected_lookup` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_ordinary_missing_source_retains_only_its_own_registration_fault` | `ordinary_missing_source_retains_only_its_registration_fault` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_and_corrupt_attempts_are_not_confused_with_new_attempts` | `corrupt_and_unknown_attempts_are_not_new_attempts` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_sparse_pending_index_reads_last_slot_and_allocates_lowest_hole` | `sparse_pending_index_reads_last_slot_and_allocates_lowest_hole` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_refuses_malformed_or_symlink_last_slot_before_new_source` | `pending_index_refuses_malformed_and_symlink_last_slot` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_full_pending_index_preserves_actual_4096_slot_capacity` | `full_pending_index_preserves_actual_4096_slot_capacity` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_directory_overflow_retains_existing_source_without_allocating` | `pending_directory_overflow_preserves_sources_without_allocating` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_next_mint_refuses_a_new_raw_only_bank_locator` | `next_context_mint_refuses_new_raw_only_locator` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_context_cannot_escape_its_read_lifetime` | `marker_context_cannot_escape_read_lifetime` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_rechecks_normal_and_ordinary_failures` | `native_absence_rechecks_normal_and_ordinary_errors` | ported live |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_rechecks_normal_and_ordinary_failures` | `completion_absence_rechecks_normal_and_ordinary_errors` | ported live |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_preserves_interrupts_and_fatal_failures` | `native_absence_preserves_real_sigint_and_system_exit` | partial live |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_preserves_interrupts_and_fatal_failures` | `completion_absence_preserves_real_sigint_and_system_exit` | partial live |
| `test_omp_boundary.py::ExplicitNativeOwnership.test_all_never_enables_omp_and_root_is_required` | `all_never_enables_native_and_native_requires_root` | ported live |
| `test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_root_and_peer_attestation_never_come_from_environment` | `root_and_peer_attestation_are_explicit_not_ambient` | ported live |
| `test_omp_boundary.py::ExplicitNativeOwnership.test_saved_root_changes_and_implicit_all_removal_refuse` | `saved_root_changes_and_implicit_all_removal_refuse` | ported live |
| `test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_native_install_applies_preview_for_copy_and_link` | `explicit_native_copy_install_applies_reviewed_preview` | pack surface |
| `test_omp_boundary.py::ExplicitNativeOwnership.test_explicit_native_install_applies_preview_for_copy_and_link` | `explicit_native_link_install_applies_reviewed_preview` | pack surface |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_typed_context_exact_retry_and_no_ack_authority` | `held_context_exact_retry_never_grants_acknowledgement_authority` | frontier |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_unknown_missing_corrupt_hold_and_frontier_only_change_refuse` | `missing_or_corrupt_hold_and_frontier_only_changes_refuse` | frontier |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_error_before_source_boundaries_reconcile_one_original_observation` | `error_before_source_boundaries_reconciles_original_observation` | frontier |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_error_source_publication_boundaries_reconcile_one_original_observation` | `error_source_publication_boundaries_reconcile_original_observation` | frontier |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_before_source_boundaries_reconcile_one_original_observation` | `hard_exit_before_source_boundaries_reconciles_original_observation` | frontier |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_hard_exit_source_publication_boundaries_reconcile_one_original_observation` | `hard_exit_source_publication_boundaries_reconcile_original_observation` | frontier |
| `test_held_source_enrollment.py::HeldSourceEnrollment.test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access` | `every_shared_reader_refuses_unknown_compatibility_before_index_access` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_not_armed_is_distinct_from_armed_without_nonce` | `not_armed_differs_from_armed_without_nonce` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_empty_verified_seal_reports_wake_not_evaluated` | `empty_verified_seal_reports_wake_not_evaluated` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_conflicting_existing_record_is_never_rewritten_or_replayed` | `conflicting_optional_record_never_rewrites_or_replays_source` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_false_observed_send_leaves_decision_unrecorded` | `real_closed_writer_leaves_decision_unrecorded_and_still_drains` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_failed_observed_send_leaves_decision_unrecorded_and_still_drains` | `real_closed_writer_leaves_decision_unrecorded_and_still_drains` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_failure_after_response_preserves_source_root_and_drain` | `non_directory_optional_writer_failure_preserves_verified_source_and_drain` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_nonbusy_outcome_failure_never_writes_second_outcome_or_verified_response` | `nonbusy_outcome_exit_failure_never_publishes_verified_response` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_captured_source_checkpoint_failure_does_not_verify_or_schedule_drain` | `captured_source_checkpoint_failure_never_verifies_or_drains` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_executor_barrier_fd_swap` | `checkpoint_rejects_actual_executor_barrier_inode_swap` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_source_checkpoint_rejects_actual_transport_fd_swap` | `checkpoint_rejects_actual_transport_inode_swap` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_ack_after_source_started_resolves_only_at_reservation` | `late_real_clock_ack_is_resolved_at_reservation` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_offered_binds_actual_root_after_response_without_changing_old_evidence` | `offered_decision_binds_actual_root_and_preserves_old_evidence` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_spoofed_record_keys_types_digests_and_status_are_unbound_without_contents` | `spoofed_diagnostic_keys_types_digests_and_status_are_unbound` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_canonical_parsed_evidence_digests_survive_noncanonical_json_encoding` | `parsed_evidence_digests_survive_noncanonical_json_encoding` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_root_write_failure_after_actual_reservation_is_unknown_not_replayed` | `real_root_write_failure_after_reservation_stays_unknown` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_real_transport_lock_exit_swap_maps_committed_reservation_to_unknown` | `real_transport_exit_swap_preserves_committed_source_and_spent_reservation` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_wake_and_observer_unwind_faults_preserve_source_and_actual_reservation_fact` | `real_transport_exit_swap_preserves_committed_source_and_spent_reservation` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source` | `changed_sampled_attempt_refuses_even_matching_registered_source` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source` | `deleted_sampled_attempt_refuses_even_matching_registered_source` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_observation_failure_is_local_memoized_and_fault_lists_are_independent` | `observation_failure_is_local_memoized_and_fault_lists_are_independent` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_registration_between_row_lookups_cannot_invent_retained_attempt_fault` | `registration_between_lookups_cannot_invent_retained_attempt_fault` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_namespace_change_between_both_locked_passes_refuses_every_row` | `namespace_change_between_locked_passes_refuses_every_row` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_preexisting_empty_slot_is_never_creator_cleanup_authority` | `preexisting_empty_slot_is_never_creator_cleanup_authority` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_local_owner_is_interruption_not_live_pid_authority` | `unheld_local_owner_is_interruption_not_live_pid_authority` | partial live |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy` | `held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy` | partial live |
| `test_omp_wake_decisions.py::AnswerFirstQualification.test_no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status` | `no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status` | frontier |
| `test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_both_contended_lock_families_raise_typed_busy_before_body` | `private_and_completion_lock_contention_is_typed_before_body` | ported live |
| `test_omp_boundary.py::StoredBatchControls` | `stored_batch_controls_keep_foreign_companions_and_humans_as_source` | ported live |
| `test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_observer_verification_cannot_enroll_or_promote_legacy_marker` | `observer_verification_never_enrolls_or_promotes_legacy_marker` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_local_read_reuse_requires_exact_live_context_and_preserves_inventory` | `transaction_local_read_reuse_requires_exact_live_context` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_changed_configured_bank_before_reused_registration_writes_neither_namespace` | `changed_configured_bank_before_reused_registration_writes_neither_namespace` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_transaction_end_fresh_recheck_refuses_late_config_after_read_only_reuse` | `transaction_exit_freshly_refuses_late_authority_config_without_private_writes` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_live_marker_observation_refuses_late_authority_change_without_private_writes` | `transaction_exit_freshly_refuses_late_authority_config_without_private_writes` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_pre_reservation_fault_has_no_second_outcome` | `postcommit_prereservation_refusal_has_no_second_outcome` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_plain_prereserve_open_read_refuses_drift_after_verified_source` | `prereserve_open_read_refuses_drift_after_verified_source` | frontier |
| `test_omp_wake_decisions.py::WakeDecisions.test_retained_root_after_cursor_failure_stays_unknown_and_rootless` | `retained_root_after_cursor_failure_stays_unknown_and_rootless` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_wake_failure_after_reservation_preserves_verified_seal_and_single_budget` | `retained_root_after_cursor_failure_stays_unknown_and_rootless` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_rechecks_normal_and_ordinary_failures` | `public_readonly_drain_rechecks_raced_transport_absence` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_preserves_interrupts_and_fatal_failures` | `public_native_drain_preserves_real_sigint_and_releases_barrier` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_native_drain_preserves_interrupts_and_releases_source_barrier` | `public_native_drain_preserves_real_sigint_and_releases_barrier` | frontier |
| `test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_public_capture_cannot_interpose_before_verified_outcome` | `public_capture_cannot_interpose_before_verified_finite_outcome` | frontier |
| `test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_exhausts_real_shared_barrier_without_canonical_write` | `public_capture_exhausts_real_shared_barrier_without_canonical_write` | frontier |
| `test_omp_writer_fence.py::InstalledCanonicalFence.test_public_capture_refuses_unknown_marker_and_shared_upgrade` | `public_capture_refuses_unknown_marker_and_shared_barrier_upgrade` | frontier |
| `test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders` | `actual_capture_before_real_observer_retains_fence_order` | frontier |
| `test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders` | `actual_observer_before_real_capture_retains_fence_order` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_persistence_truncation_is_not_literal_complete_learning` | `persistence_truncation_is_never_literal_complete_learning` | partial live |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_active_unavailable_tail_is_real_gap_not_a_notification_gap` | `active_unavailable_tail_is_real_gap_not_notification_gap` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity` | `retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_retired_new_truncated_entry_keeps_gap_and_prevents_complete_source_claim` | `retired_new_truncated_entry_keeps_gap_and_prevents_complete_claim` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resumed_fork_postfork_human_is_not_blanket_inherited` | `resumed_fork_postfork_human_is_not_blanket_inherited` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_fork_without_time_evidence_retains_nonhuman_source_with_explicit_gap` | `fork_without_time_evidence_retains_nonhuman_source_with_gap` | frontier |
| `test_omp_boundary.py::NativeSources.test_late_native_tool_result_recovers_original_sealed_gap_without_reopening` | `exact_late_tool_result_recovers_sealed_gap_without_reopening_or_stale_pending` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_resolved_sealed_tool_gap_does_not_leave_stale_tool_pending_or_block_drain` | `exact_late_tool_result_recovers_sealed_gap_without_reopening_or_stale_pending` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_foreign_tool_call_id_cannot_resolve_actual_sealed_tool_gap` | `foreign_tool_call_id_never_resolves_actual_sealed_gap` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_unsafe_exact_recovery_does_not_append_gap_to_sealed_source` | `unsafe_exact_recovery_never_mutates_sealed_source` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_manager_subdirectory_same_bank_accepts_and_native_root_identity_stays_canonical` | `manager_subdirectory_accepts_only_canonical_bank_root_identity` | frontier |
| `test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_contended_entry_refuses_current_frame_and_stops` | `contended_entry_refuses_current_frame_and_stops_with_open_input` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_delayed_old_checked_view_refuses_new_session_and_real_new_human_has_one_root` | `delayed_old_view_never_creates_new_session_authority` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_invalid_views_refuse_before_real_transport_contention_and_bad_receipt` | `invalid_view_precedes_contended_transport_and_bad_receipt` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_completed_historical_origin_exact_replay_is_read_only_and_changed_request_refuses` | `completed_historical_origin_allows_only_exact_readonly_replay` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_same_role_position_decoy_never_resolves_real_event_gap` | `adjacent_role_decoy_cannot_resolve_exact_event_gap` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_weak_historical_event_resolution_does_not_authorize_new_acknowledgement` | `weak_historical_event_resolution_never_grants_new_completion` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_malformed_wake_state_suppresses_reservation_but_retains_and_enrolls_real_source` | `malformed_wake_namespace_preserves_sources_without_spending_window` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_no_active_resume_paths_never_append_to_previously_sealed_instance_segment` | `resumed_carrier_message_and_unavailable_tail_never_append_to_sealed_parent` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_expired_internal_delivery_window_never_reserves_root_wake` | `expired_real_window_does_not_spend_native_human_budget` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_held_and_gapped_segments_do_not_gain_completion_authority` | `held_and_gapped_segment_rows_never_authorize_completion` | frontier |
| `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_missing_corrupt_source_and_completion_faults_remain_visible` | `missing_corrupt_source_and_completion_records_remain_visible` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` | `clean_writer_release_at_every_real_probe_boundary_is_not_recovery` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_replacement_at_checked_boundaries_remains_hard_refusal` | `replacement_at_every_checked_probe_boundary_is_a_hard_refusal` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_rebound_inode_after_flock_observation_is_not_classified_as_busy` | `replacement_at_every_checked_probe_boundary_is_a_hard_refusal` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_requires_exact_safe_slot_properties` | `empty_writer_slot_requires_private_unlinked_ordinary_file` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_cannot_rebind_its_pinned_bank` | `empty_writer_probe_cannot_follow_rebound_pinned_bank` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_bank_replaced_by_symlink_before_open_is_typed_without_closing_foreign_fds` | `symlink_rebound_bank_refuses_without_closing_unrelated_descriptor` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_initial_lstat_io_error_is_typed_without_opening_descriptors` | `real_marker_inspection_permission_failure_is_a_typed_refusal` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_noncontention_first_ex_error_removes_only_creators_empty_slot` | `forced_first_ex_syscall_errors_cleanup_only_created_empty_slot` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_nonempty_short_owner_write_remains_interruption_not_initialization` | `actual_short_owner_publication_retains_only_written_prefix` | ported live |
| `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_owner_write_oserror_preserves_publication_evidence_and_closes_creator` | `owner_write_error_keeps_empty_publication_and_releases_actual_descriptor` | ported live |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_fresh_capture_preserves_surrounding_source_and_retries` | `fresh_owned_capture_preserves_surrounding_source_in_both_install_modes` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_nonread_result_never_mints_manual_witness` | `nonread_tool_output_remains_ordinary_learning_source` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_screened_invocation_does_not_hide_following_answer` | `screened_read_invocation_cannot_hide_following_answer` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_snapshot_publication_recovery_survives_uninstall_and_retry` | `orphan_owned_snapshot_recovers_after_manual_removal_without_reclassification` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_mismatched_orphan_snapshot_is_integrity_failure` | `mismatched_orphan_owned_snapshot_refuses_without_overwrite` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_preexisting_native_gap_is_not_reclassified` | `preexisting_unsafe_native_gap_cannot_become_owned_instruction` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_sealed_tool_gap_does_not_authorize_late_classification` | `sealed_tool_gap_never_lends_late_owned_instruction_authority` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_same_sweep_human_boundary_preserves_sealed_tool_gap` | `same_sweep_human_boundary_preserves_original_tool_gap` | frontier |
| `test_omp_boundary.py::NativeSources.test_protected_private_root_cannot_silently_move_or_lose_binding` | `protected_private_root_cannot_lose_native_binding_silently` | frontier |
| `test_omp_boundary.py::NativeSources.test_root_wake_is_once_and_later_human_carries_unassessed_old_source` | `native_root_wake_is_once_and_next_human_includes_old_unassessed_sources` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_bounded_later_sweeps_enroll_oldest_and_keep_every_new_arrival` | `bounded_sweeps_enroll_oldest_without_losing_new_arrivals` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_lost_success_reuses_observation_and_unknown_attempt_is_not_retried` | `lost_registration_response_recovers_only_actual_original_shared_source` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_shared_fault_retained_across_later_observer_progress` | `actual_shared_source_capacity_fault_remains_retained_across_observer_progress` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_resolves_regular_and_newline_worktree_identity` | `fresh_context_resolves_regular_and_newline_linked_worktree_identity` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_foreign_bank_identity_without_private_writes` | `fresh_context_refuses_physical_foreign_bank_identity_without_private_writes` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_bank_drift_after_actual_scope_read` | `fresh_context_refuses_competing_bank_added_after_actual_scope_read` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_raw_nonrouter_refusal_with_and_without_verified_hold` | `raw_nonrouter_context_refusal_preserves_held_and_ordinary_banks` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_committed_raw_locator_refusal_with_and_without_verified_hold` | `committed_missing_raw_locator_refusal_preserves_held_and_ordinary_banks` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_symlink_locator_refusal_with_and_without_verified_hold` | `symlink_raw_locator_refusal_preserves_held_and_ordinary_banks` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_preserves_late_raw_locator_refusal_with_and_without_verified_hold` | `late_raw_locator_after_real_scope_read_refuses_held_and_ordinary_contexts` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_successful_but_disagreeing_bank_locators` | `successful_but_disagreeing_raw_and_ordinary_bank_locators_never_mint_context` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_reused_locator_keeps_post_source_mutation_scope_check_and_original_fault` | `reused_locator_rechecks_scope_after_actual_source_publication_and_retains_fault` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_concurrent_slot_creation_keeps_both_sources_and_next_hole` | `concurrent_real_pending_slot_creation_keeps_both_sources_and_next_hole` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_retries_transient_scan_change_and_refuses_persistent_churn` | `pending_index_retries_real_transient_change_and_refuses_real_persistent_churn` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_held_read_reuse_keeps_exact_intent_pins_and_refuses_changed_hold_without_new_source` | `held_read_reuse_keeps_exact_intent_pins_and_refuses_new_source_after_hold_change` | frontier |
| `test_omp_enrollment_progress.py::EnrollmentProgress.test_observe_bank_drift_keeps_original_captures_refuses_final_and_next_frame_freshly_locates` | `observer_bank_drift_retains_both_original_captures_and_next_frame_freshly_locates` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state` | `configured_bank_ingest_drift_refuses_without_retirement_or_new_namespace` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_seal_refuses_without_new_private_state` | `configured_bank_seal_drift_refuses_without_new_namespace` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_ingest_canonical_writer_is_busy_before_binding_refusal` | `configured_bank_ingest_writer_busy_precedes_binding_refusal` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_seal_canonical_writer_is_busy_before_binding_refusal` | `configured_bank_seal_writer_busy_precedes_binding_refusal` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_metadata_refuses_without_attempt_or_new_base` | `configured_bank_metadata_drift_refuses_before_attempt_publication` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_ingest_never_acquires_foreign_exclusive_barrier` | `retargeted_ingest_never_acquires_foreign_exclusive_barrier` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_seal_never_acquires_foreign_exclusive_barrier` | `retargeted_seal_never_acquires_foreign_exclusive_barrier` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_ingest_refuses_before_unreadable_native_binding` | `retargeted_ingest_refuses_before_unreadable_foreign_binding` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_activated_root_retarget_at_seal_refuses_before_unreadable_native_binding` | `retargeted_seal_refuses_before_unreadable_foreign_binding` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_checkout_retarget_after_view_validation_refuses_before_started_or_source` | `checkout_alias_retarget_after_validation_refuses_before_started_or_retirement` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_fresh_owned_guides_capture_exact_metadata_in_copy_and_link_modes` | `fresh_owned_guides_retain_exact_closed_context_in_copy_and_link_modes` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_owned_guide_alias_partial_tamper_and_foreign_reads_keep_real_gaps` | `owned_guide_alias_partial_tamper_and_foreign_carriers_keep_original_gaps` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_noninstruction_artifact_read_retains_ordinary_body_not_owned_metadata` | `noninstruction_packaged_artifact_retains_actual_ordinary_body_and_details` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_verification_races_refuse_selected_binding_payload_and_policy_changes` | `owned_manual_verification_rejects_real_binding_payload_and_policy_races` | pack surface |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_unrelated_ancestor_churn_preserves_witness_and_public_annotation` | `unrelated_ancestor_churn_preserves_owned_witness_and_public_source_annotation` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_historical_owned_guide_never_authorizes_read_after_installation_changes` | `historical_owned_guide_metadata_cannot_authorize_fresh_read_after_installation_change` | frontier |
| `test_omp_boundary.py::OwnedInstructionBoundary.test_owned_guide_rechecks_receipt_selection_and_nested_symlinks` | `owned_guide_rechecks_selected_receipt_entry_nested_links_and_actual_rebind` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_cancel_before_reservation_refuses_and_after_start_preserves_result` | `cancellation_before_reservation_refuses_but_after_reservation_keeps_one_root` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_replacement_and_end_after_start_preserve_result` | `replacing_clock_or_ending_after_reservation_preserves_exact_single_root` | frontier |
| `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_pre_reservation_wake_exception_does_not_unverify_committed_source` | `real_prereservation_fault_does_not_unverify_durable_direct_seal` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_io_failure_never_verifies_partial_seal` | `cancelled_uncommitted_outcome_real_io_failure_never_verifies_partial_seal` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_refusal_never_verifies_partial_seal` | `cancelled_uncommitted_outcome_typed_refusal_never_verifies_partial_seal` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_cancel_after_actual_reserve_then_exit_fault_retains_one_unknown_root` | `cancel_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_replacement_after_actual_reserve_then_exit_fault_retains_one_unknown_root` | `replacement_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_eof_after_actual_reserve_then_exit_fault_retains_one_unknown_root` | `eof_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_output_loss_after_actual_reserve_then_exit_fault_retains_one_unknown_root` | `output_loss_after_real_reservation_and_transport_exit_drift_retains_one_unknown_root` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_race_remains_pending_for_normal_and_ordinary_failure` | `public_compatibility_absence_race_remains_pending_for_normal_and_ordinary_failure` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_preserves_interrupts_and_fatal_failures` | `public_compatibility_preserves_real_sigint_and_exit_with_raced_evidence` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_source_inspection_preserves_interrupts_with_raced_completion_lock` | `public_source_inspection_preserves_real_fatal_exits_and_refuses_raced_completion_lock` | frontier |
| `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_unchanged_absence_allows_normal_exit_and_preserves_ordinary_error` | `unchanged_private_absence_allows_success_and_preserves_original_ordinary_error` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source` | `writer_at_outcome_unwinds_and_closes_without_replaying_source` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_actual_transaction_at_outcome_waits_without_adopting_its_marker` | `transaction_at_outcome_waits_without_adopting_real_marker` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_real_writer_pre_ex_empty_publication_defers_without_stealing_its_lock` | `pre_ex_empty_publication_defers_without_stealing_real_writer_slot` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_real_writer_full_owner_fsync_window_has_ex_ownership_before_probe` | `full_owner_fsync_window_has_actual_ex_ownership_before_probe` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_metadata_exhaustion_keeps_source_and_open_unknown` | `actual_writer_metadata_exhaustion_preserves_source_and_open_unknown` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_real_initializing_writer_exhaustion_cannot_verify_or_close` | `initializing_writer_real_deadline_cannot_verify_or_close` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_empty_publication_exhaustion_retains_source_without_outcome` | `unheld_empty_publication_real_deadline_retains_source_without_outcome` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_released_writer_with_unknown_transaction_is_hard_refusal_not_retry` | `released_writer_unknown_transaction_is_not_retryable` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_transaction_left_after_writer_release_remains_interruption` | `released_transaction_retained_marker_remains_interruption` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_writer_release_rediscovers_identity_instead_of_reusing_frame_scope` | `writer_release_freshly_rediscovers_changed_project_identity` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_busy_outcome_beforewrite_or_exit_retries_only_identical_source_after_unlock` | `busy_source_outcome_retries_exact_immutable_source_after_transport_unlock` | frontier |
| `test_omp_source_phase.py::ArmedSourcePhase.test_busy_outcome_exit_exhaustion_keeps_retained_record_without_response_or_drain` | `busy_source_outcome_exit_real_deadline_keeps_record_without_response_or_drain` | frontier |
| `test_omp_finite_stream.py::CoverageFixture.test_real_io_failure_does_not_manufacture_invalid_request_refusal` | `accepted_attempt_publication_failure_is_not_an_invalid_request_refusal` | frontier |
| `test_omp_finite_stream.py::MetadataBarrierBoundaries.test_run_final_close_rejects_barrier_inode_replacement_after_metadata_write` | `final_close_rejects_actual_source_barrier_swap_after_retaining_closed_record` | frontier |
| `test_omp_finite_stream.py::CoverageFixture.test_startup_typed_busy_retries_before_open_and_ready` | `startup_retries_actual_canonical_busy_before_publishing_open_or_ready` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_request_durable_attempt_exit_busy_resumes_exactly_once` | `invalid_request_busy_exit_resumes_exactly_once` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_refused_frame_durable_attempt_exit_busy_counts_once` | `invalid_refused_frame_busy_exit_counts_exactly_once` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_attempt_exit_busy_never_adopts_partial_retry_record` | `invalid_attempt_busy_exit_never_adopts_partial_record` | frontier |
| `test_omp_finite_stream.py::CanonicalWriterContention.test_invalid_attempt_exit_busy_never_adopts_different_retry_record` | `invalid_attempt_busy_exit_never_adopts_different_record` | frontier |

## Final product changes and reasons

1. **PrivateLock parity:** retain the `existing_only` flag. Only true pins the old parent chain; false re-resolves the named path while retaining actual held-inode checks. Default remains false, and read-only still forces true. No caller flag changed. Real hard-link parent replacement, default-created, inode swap and foreign-thread tests run. Every affected false-flag caller and every unaffected true-flag caller is enumerated under “PrivateLock baseline parity” above; that list remains authoritative. LSP constructor references were attempted but no server was configured, so text fallback was used.
2. **Concrete Harness root:** required `pack_root`, explicit `Harness::at`, and existing root resolver delegated through `Harness::installed`; lazy `InstalledHarness` preserves first-install CLI behavior. No new trait was introduced. The public installed API is exercised with the real pack.
3. **Fault selector primitive:** dev-tools comma-separated stage lists reject empty/duplicate entries using existing validation. Single-stage matching remains unchanged. Mode is one mode for all stages or one per stage, otherwise existing validation failure; each selected mode is validated. World token binding and per-stage ready/release files remain unchanged. Existing t1_bank now proves two ordered pauses in one real child and, in the same test, pause then exit86; malformed list coverage is included.
4. **Approved syscall faults:** first EX ENOLCK/ENOTSUP substitute an io::Error in the existing acquisition branch while preserving the actual descriptor. Production cfg uses unchanged real acquisition. Owner publication uses a real short prefix write or approved EIO without fabricated bytes, so existing partial-write/evidence preservation handles real files. Both have passing real tests.
5. **Call-only boundary points:** listed below, all cfg(dev-tools), token-bound, inert unarmed. No recovery_frontier implementation was edited. Five unused planned points were removed before the final checks.

### Final retained added fault points

Each row gives at least one exact baseline obligation served; additional mappings are above. The baseline prefix is `tests/contracts/`.

| File:line | Point | Baseline test served |
|---|---|---|

| `src/fs/lock.rs:94` | `registry.exclusive.owner-write-short` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_nonempty_short_owner_write_remains_interruption_not_initialization` |
| `src/fs/lock.rs:97` | `registry.exclusive.owner-write-eio` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_owner_write_oserror_preserves_publication_evidence_and_closes_creator` |
| `src/omp_transport.rs:1325` | `omp.capture.before-commit` | `test_omp_writer_fence.py::InstalledCanonicalFence.test_actual_capture_and_installed_observer_in_both_orders` |
| `src/omp_transport.rs:1368` | `omp.capture.after-commit` | `test_omp_enrollment_progress.py::EnrollmentProgress.test_observe_bank_drift_keeps_original_captures_refuses_final_and_next_frame_freshly_locates` |
| `src/omp_transport/inventory.rs:396` | `omp.join.before-shared-inventory` | `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_observation_failure_is_local_memoized_and_fault_lists_are_independent` |
| `src/omp_transport/inventory.rs:399` | `omp.join.after-shared-inventory` | `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source` |
| `src/omp_transport/inventory.rs:419` | `omp.join.after-row-lookup` | `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_registration_between_row_lookups_cannot_invent_retained_attempt_fault` |
| `src/omp_transport/observe.rs:245` | `omp.reservation.before-reserve` | `test_omp_source_phase.py::ArmedSourcePhase.test_ack_after_source_started_resolves_only_at_reservation` |
| `src/omp_transport/observe.rs:251` | `omp.reservation.after-reserve` | `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_cancel_before_reservation_refuses_and_after_start_preserves_result` |
| `src/omp_transport/observe.rs:254` | `omp.reservation.after-root` | `test_omp_source_phase.py::ArmedSourcePhase.test_root_write_failure_after_actual_reservation_is_unknown_not_replayed` |
| `src/omp_transport/observe.rs:260` | `omp.reservation.after-cursor` | `test_omp_wake_decisions.py::WakeDecisions.test_retained_root_after_cursor_failure_stays_unknown_and_rootless` |
| `src/omp_transport/observe.rs:580` | `omp.checked-view.after-validation` | `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state` |
| `src/omp_transport/observe.rs:1855` | `omp.checkpoint.before-recheck` | `test_omp_source_phase.py::ArmedSourcePhase.test_captured_source_checkpoint_failure_does_not_verify_or_schedule_drain` |
| `src/omp_stream/coverage.rs:578` | `omp.metadata.after-busy` | `test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source` |
| `src/omp_stream/coverage.rs:738` | `omp.attempt.after-retain` | `test_omp_finite_stream.py::CoverageFixture.test_real_io_failure_does_not_manufacture_invalid_request_refusal` |
| `src/omp_stream/coverage.rs:883` | `omp.outcome.before-guard` | `test_omp_finite_outcome_fence.py::FiniteOutcomeFence.test_public_capture_cannot_interpose_before_verified_outcome` |
| `src/omp_stream/coverage.rs:886` | `omp.outcome.before-commit` | `test_omp_source_phase.py::ArmedSourcePhase.test_cancelled_uncommitted_outcome_io_failure_never_verifies_partial_seal` |
| `src/omp_stream/coverage.rs:896` | `omp.outcome.after-commit` | `test_omp_source_phase.py::ArmedSourcePhase.test_nonbusy_outcome_failure_never_writes_second_outcome_or_verified_response` |
| `src/omp_stream/coverage.rs:1013` | `omp.close.after-retain` | `test_omp_finite_stream.py::MetadataBarrierBoundaries.test_run_final_close_rejects_barrier_inode_replacement_after_metadata_write` |
| `src/omp_stream/executor.rs:150` | `omp.before-reserve.before-open` | `test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_pre_reservation_fault_has_no_second_outcome` |
| `src/omp_stream/executor.rs:402` | `omp.request-identity.after-validation` | `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_metadata_refuses_without_attempt_or_new_base` |
| `src/omp_stream/executor.rs:621` | `omp.observer.after-busy-error` | `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state` |
| `src/omp_stream/executor.rs:623` | `omp.observer.after-error` | `test_omp_source_phase.py::ArmedSourcePhase.test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state` |
| `src/turn_sources.rs:254` | `enrollment.index.during-scan` | `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_retries_transient_scan_change_and_refuses_persistent_churn` |
| `src/turn_sources.rs:763` | `enrollment.index.before-exclusive` | `test_omp_enrollment_progress.py::EnrollmentProgress.test_pending_index_concurrent_slot_creation_keeps_both_sources_and_next_hole` |
| `src/turn_sources.rs:1132` | `enrollment.source.after-exclusive` | `test_omp_enrollment_progress.py::EnrollmentProgress.test_reused_locator_keeps_post_source_mutation_scope_check_and_original_fault` |
| `src/turn_sources.rs:1654` | `enrollment.inventory.before-read` | `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_source_inspection_preserves_interrupts_with_raced_completion_lock` |
| `src/turn_sources.rs:1657` | `enrollment.inventory.after-read` | `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_source_inspection_preserves_interrupts_with_raced_completion_lock` |
| `src/turn_sources.rs:1722` | `omp.join.between-locked-passes` | `test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_namespace_change_between_both_locked_passes_refuses_every_row` |
| `src/omp_boundary.rs:1098` | `omp.drain.before-inventory` | `test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_rechecks_normal_and_ordinary_failures` |
| `src/omp_installation.rs:970` | `omp.manual.before-final-receipt` | `test_omp_boundary.py::OwnedInstructionBoundary.test_verification_races_refuse_selected_binding_payload_and_policy_changes` |
| `src/omp_installation.rs:980` | `omp.manual.after-final-receipt` | `test_omp_boundary.py::OwnedInstructionBoundary.test_verification_races_refuse_selected_binding_payload_and_policy_changes` |
| `src/local_registry.rs:345` | `registry.probe.after-lstat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_bank_replaced_by_symlink_before_open_is_typed_without_closing_foreign_fds` |
| `src/local_registry.rs:360` | `registry.probe.before-open` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:377` | `registry.probe.after-open` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_zero_initialization_cannot_rebind_its_pinned_bank` |
| `src/local_registry.rs:380` | `registry.probe.after-initial-fstat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:385` | `registry.probe.after-first-name-stat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:413` | `registry.probe.after-empty-fstat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:443` | `registry.probe.after-read-fstat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:448` | `registry.probe.after-read-name-stat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:486` | `registry.probe.before-flock` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:491` | `registry.probe.after-busy-flock` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:503` | `registry.probe.after-final-fstat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:508` | `registry.probe.after-final-name-stat` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_clean_release_at_each_observation_boundary_is_not_recovery` |
| `src/local_registry.rs:749` | `registry.exclusive.before-first-ex` | `test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source` |
| `src/local_registry.rs:753` | `registry.exclusive.first-ex-enolck` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_noncontention_first_ex_error_removes_only_creators_empty_slot` |
| `src/local_registry.rs:754` | `registry.exclusive.first-ex-enotsup` | `test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_noncontention_first_ex_error_removes_only_creators_empty_slot` |
| `src/local_registry.rs:797` | `registry.exclusive.before-owner-write` | `test_omp_finite_stream.py::CanonicalWriterContention.test_real_initializing_writer_exhaustion_cannot_verify_or_close` |
| `src/local_registry.rs:810` | `registry.exclusive.after-owner-fsync` | `test_omp_finite_stream.py::CanonicalWriterContention.test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source` |
| `src/scope_binding.rs:677` | `enrollment.scope.after-require` | `test_omp_enrollment_progress.py::EnrollmentProgress.test_fresh_mint_refuses_bank_drift_after_actual_scope_read` |

### Additional future frontier points required by written ignored tests

- `frontier.observe-bank.before-read` and `frontier.observe-bank.after-read`: actual public compatibility absence races, ordinary/fatal unwind, real bank identity drift and held/raw locator tests. These names appear in the fully written ignored tests but the calls are not implemented in this lane, because omp-beast owns recovery_frontier. They must be installed at the actual bounded observation/read boundaries, preserving the real result.

## Final not-applicable proposals (unmarked)

- `tests/contracts/test_omp_finite_stream.py::FrozenImports.test_unpreloaded_late_packed_module_is_refused_without_blocking_stdlib`: Python late-module loader/interpreter execution has no remaining compiled Rust runtime path.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_armed_started_precommit_memory_error_retains_original_unknown_response`: Only catchable Python MemoryError/RecursionError or Python resource-exhaustion injection is tested. Rust exposes no catchable equivalent; allocation/stack exhaustion are process-fatal. Real I/O, SIGINT, exit codes, inode swaps and thread refusal remain separate real tests.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_live_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_released_source_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_stream_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_borrowed_consumer_refuses_transport_observation_without_fallback`: enforced at compile time by Rust ownership/types; no runtime path exists
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_ordinary_started_memory_error_retains_original_unknown_response`: Only catchable Python MemoryError/RecursionError or Python resource-exhaustion injection is tested. Rust exposes no catchable equivalent; allocation/stack exhaustion are process-fatal. Real I/O, SIGINT, exit codes, inode swaps and thread refusal remain separate real tests.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_ordinary_started_recursion_error_retains_original_unknown_response`: Only catchable Python MemoryError/RecursionError or Python resource-exhaustion injection is tested. Rust exposes no catchable equivalent; allocation/stack exhaustion are process-fatal. Real I/O, SIGINT, exit codes, inode swaps and thread refusal remain separate real tests.
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postcommit_resource_exhaustion_is_not_ordinary_wake_success`: Only catchable Python MemoryError/RecursionError or Python resource-exhaustion injection is tested. Rust exposes no catchable equivalent; allocation/stack exhaustion are process-fatal. Real I/O, SIGINT, exit codes, inode swaps and thread refusal remain separate real tests.
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_memory_error_keeps_fatal_policy`: Only catchable Python MemoryError/RecursionError or Python resource-exhaustion injection is tested. Rust exposes no catchable equivalent; allocation/stack exhaustion are process-fatal. Real I/O, SIGINT, exit codes, inode swaps and thread refusal remain separate real tests.
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_optional_writer_recursion_error_keeps_fatal_policy`: Only catchable Python MemoryError/RecursionError or Python resource-exhaustion injection is tested. Rust exposes no catchable equivalent; allocation/stack exhaustion are process-fatal. Real I/O, SIGINT, exit codes, inode swaps and thread refusal remain separate real tests.
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_reparented_linux_window_cannot_reserve`: fabricates private Linux image readiness with impossible PID/fd state; the Rust type cannot be constructed without real qualified image/proc checks. Reparenting refusal is proved by the image tests and in #249 Linux native qualification. Main approved this note and added the Linux refusal to Gate D.

The seven NativeSmokeQualification IDs listed under “Routed elsewhere” above remain routed to OmpSmoke lane, not ported or annotated here.

## Final verification

After unused-hook removal and final ledger annotation, all required commands ran in this worktree:

```sh
~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh /tmp/fanout-b-ported-final.txt tests/fanout_b_omp.rs
cargo fmt
cargo check --offline --all-targets --features dev-tools
cargo test --offline --features dev-tools --test fanout_b_omp
cargo test --offline --features dev-tools --test t1_bank
```

Final observed results (artifact14611): check completed with **zero warnings**; fanout_b_omp **34 passed,171 ignored,zero failed** (1.54s); t1_bank **41 passed,four ignored,zero failed** (0.11s). No full suite or Linux qualification was run. Platform remains Darwin arm64 as recorded above.

## Final gaps and integration requirements

- **Acceptance is not met.** The52 exact unwritten IDs below and eight partial live paths remain incomplete, not waived. Partial live details in the earlier dedicated section remain accurate: borrowed executor integration3, private-observer fatal subcases2, full canonical-writer scope paths2, and retired-source lifecycle1.
- All166 frontier IDs are compilation-only coverage until the real frontier and its named observation hooks land and the tests actually pass.
- The seven pack-surface IDs use the committed real shipped pack, not a substitute. This base lacks the `am refresh`/`am boundary` declared surface from f6ac3c1e. Main explicitly directed this ignore rather than refreshing overlapping product files; remove at merge and execute. Six prior ported statuses were retracted to avoid claiming old minimal-fixture coverage.
- Ignored tests may expose parity defects once the actual frontier/pack surface is available. Their unexecuted behavior is not asserted as observed.
- No signed native update, live harness session, Linux proc/image readiness, musl, actual external endpoint or latency qualification is established here.

### Final unwritten IDs

- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_late_hold_registry_and_ledger_changes_retain_written_source_then_retry`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_public_layout_hold_enrolls_real_codex_and_claude_prompt_hooks`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_real_codex_claude_hooks_keep_task_active_but_boundaries_pending`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_rolled_back_raw_scope_defers_late_real_held_checkpoint_without_source_loss`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_shared_source_barrier_reentrancy_and_lifecycle_contention_preserve_sources`
- `tests/contracts/test_held_source_enrollment.py::HeldSourceEnrollment.test_verified_hold_rejects_location_and_hold_changes_during_observation`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_held_native_preview_consumes_reviewed_envelope_without_publishing`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_managed_native_updates_read_back_only_owned_public_commands`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_native_update_ignores_missing_historical_worktree_but_preserves_required_enrollment`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_public_uninstall_keeps_pending_sources_and_disabled_owned_binding`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_public_uninstall_requires_quiescence_and_actually_drains_completed_sources`
- `tests/contracts/test_omp_boundary.py::NativeLifecyclePublic.test_signed_native_updates_preserve_extension_integrity`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_capacity_retains_arrival_then_reuses_reference_after_capacity_frees`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_completed_snapshots_require_bound_drain_proof_after_marker_removal`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_crashed_pure_snapshot_never_reopens_sealed_parent`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_exact_own_hidden_batch_does_not_open_a_source_successor`
- `tests/contracts/test_omp_boundary.py::NativeSources.test_full_backlog_and_long_unicode_repo_still_offer_one_bounded_wake`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_arbitrary_marker_read_failure_without_writer_is_not_retryable`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_empty_held_and_unheld_slots_never_mint_scope_or_claim_known_writer`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_failed_marker_read_with_unproven_empty_slot_keeps_original_refusal`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_late_transaction_after_marker_read_is_exact_writer_deferral`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_late_transaction_before_frontier_entry_is_exact_writer_deferral`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_late_transaction_during_failed_marker_read_is_exact_writer_deferral`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_writer_arriving_after_preflight_is_classified_at_actual_identity_guard`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_failed_first_ex_preserves_replacement_or_changed_empty_slot`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_interrupt_during_marker_read_is_not_reclassified_as_writer_retry`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_real_owner_publication_between_empty_samples_is_only_initialization`
- `tests/contracts/test_omp_finite_stream.py::CanonicalWriterObservationBoundaries.test_real_recovery_wins_first_ex_without_stranding_creators_empty_slot`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_historical_completed_gate_observation_is_not_reused_for_lookup`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_no_context_preserves_fresh_per_reference_observations`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_real_completion_between_result_and_wake_requires_new_inventory`
- `tests/contracts/test_omp_segment_inventory_batch.py::SegmentInventoryBatch.test_type_and_key_errors_still_reach_whole_segment_fault_handling`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_clock_ack_behind_accepted_16mib_frame_reaches_late_reservation`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_closed_pipe_writer_false_drains_before_next_fifo_and_final_in_full_run`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_completion_guard_discovers_fresh_locations_and_restores_live_locator`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_oldest8_after_response_and_cli_collision_leave_canonical_state_unchanged`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postresponse_busy_is_one_batch_and_leaves_discoverable_enrollments`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_postresponse_control_flow_is_not_masked_or_closed`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_response_loss_still_drains_once_before_next_fifo_and_final`
- `tests/contracts/test_omp_source_phase.py::ArmedSourcePhase.test_reused_header_nonce_decline_and_exit_fault_do_not_claim_own_reservation`
- `tests/contracts/test_omp_source_phase.py::LateWindowControls.test_nonce_cancel_replacement_eof_output_loss_and_one_shot_lookup`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_legacy_random_root_wake_follows_exact_native_user_without_spending_new_human_budget`
- `tests/contracts/test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.test_random_legacy_root_with_real_reconciled_human_still_cannot_acknowledge`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_prior_committed_activation_recovers_after_a_later_install_rolls_back`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_real_binding_proof_rejects_forgery_and_mid_instance_drift_before_source`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_receipt_commit_separates_bank_activation_from_rollback`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_bounded_summary_orphans_and_unreadable_namespace_do_not_change_doctor_health`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_existing_root_retains_legacy_bare_pending_without_inferred_cause`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_malformed_real_result_shapes_are_unknown_without_inventing_a_cause`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_nonseal_and_failed_source_attempts_do_not_acquire_diagnostic_rows`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_offered_writer_checks_actual_request_without_cursor_or_root_scan`
- `tests/contracts/test_omp_wake_decisions.py::WakeDecisions.test_substituted_oversize_response_never_records_an_offered_continuation`

