# fanout-d obligation-port report

## Outcome

Operator requested wrap-up before all assigned obligations were implemented. Of the 187 initially unported assigned methods: **26 ported, 115 partial (107 gated plus eight runnable but incomplete), 18 approved not-applicable, 28 still unported gaps**. This lane is not complete. Ignored tests compile but their behavior is not validated. No Python was executed and no Git writes were made.

Worktree: /opt/agent-memory-worktrees/rust-v015-lane-fanout-d. Started at 5bd11582; refreshed Main-approved integration files byte-for-byte from 5764442c. Product changes in that refresh are upstream, not this lane's edits.

## Files

- `tests/c_test_admission.rs`
- `tests/c_test_authority_boundary.rs`
- `tests/c_test_authority_policy.rs`
- `tests/c_test_bank_eligibility_writes.rs`
- `tests/c_test_bank_layout.rs`
- `tests/c_test_bank_transactions.rs`
- `tests/c_test_bank_usability.rs`
- `tests/c_test_bank_writes.rs`
- `tests/c_test_bounded_bank_observation.rs`
- `tests/c_test_bounded_resume.rs`
- `tests/c_test_claude_boundary.rs`
- `tests/c_test_codex_stop.rs`
- `tests/c_test_committed_replanning.rs`
- `tests/c_test_concurrent_bank_writers.rs`
- `tests/c_test_current_bank.rs`
- `tests/c_test_identity_canaries.rs`
- `tests/c_test_identity_git_group.rs`
- `tests/c_test_independent_local_bank.rs`
- `tests/c_test_manual_checkpoint.rs`
- `tests/c_test_read_only_audits.rs`
- `tests/c_test_recall_catalog.rs`
- `tests/c_test_recall_observation.rs`
- `tests/c_test_repository_observation_batch.rs`
- `tests/c_test_router.rs`
- `tests/c_test_scope_policy.rs`
- `tests/c_test_tombstones.rs`
- `tests/c_test_validation_memo.rs`
- `tests/t2_publication.rs`
- `tests/t2_initialization.rs`
- `tests/support/cutover.rs` (shared real fixture/CLI dispatch support)
- `tests/support/mod.rs` (append-only module declaration)
- `tests/obligations/python-baseline-v1.json` (ledger updates)

## Defects and declared deltas

- Reported process cleanup's unbounded post-kill reap while interrupts were masked. Main routed the fix; refreshed process.rs now uses the bounded reap implementation. The identity interrupt tests are no longer gated and pass, but their incomplete stage-specific coverage remains partial in the ledger. Their SIGINT fixtures exercise owned children and preserve foreign children, not Python exceptions. The tests do not force an unreapable kernel task or synthetic selector/registration allocation faults; the latter two obligations remain explicit gaps below.
- Missing project/inspect/transaction/tombstones routes were reported and refreshed from upstream. Some ignored attributes still mention those routes; recovery_frontier remains the actual blocker.
- Runtime recovery_frontier registration remains unavailable in this snapshot. All 105 frontier-dependent tests have bodies and remain ignored, not represented as passing.
- Two native installation audit tests remain ignored pending installation command-surface/providers (refresh/boundary); their current attributes say development installation providers.
- Managed source channel switching is retired (ADR-0029): committed-replanning cases assert native --remote/--ref refusal, guidance, and exact no-write preservation rather than staged switching. Main approved this declared delta.
- Python launcher dispatch is retired. The bytecode-prefix obligation's observable refusal half is ported: native pack validation rejects all baseline Python launch strings and a native shell-metacharacter injection, with no execution or filesystem mutation. No whole-test not-applicable classification was used for that method.
- Evaluation controllers under scripts/evaluations and docs/research Python tooling are retired for v0.0.15 (#206 → v0.0.16); classifications below are Main-approved. Source inspection identified four import-cache-only tests and seven controller tests in recall_qualification (rather than the earlier five/six shorthand).
- Recall catalog installed-CLI coverage currently uses approved-source PackRoot library dispatch; it remains partial. The large-frontier eligibility case is likewise partial, not claimed as a validated full frontier observation.

## Fully ported runnable obligations (26)

- `tests/contracts/test_authority_boundary.py::AuthorityBoundaryContract.test_two_developer_ids_share_one_bank_without_changing_permissions_or_visibility` → `tests/c_test_authority_boundary.rs::developer_attribution_does_not_change_bank_permissions_or_visibility`
- `tests/contracts/test_authority_boundary.py::AuthorityBoundaryContract.test_remembered_cross_project_claim_does_not_bind_another_repository` → `tests/c_test_authority_boundary.rs::remembered_cross_project_claim_never_binds_another_repository`
- `tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_conflicting_code_intent_and_direct_project_decision_keep_their_own_authorities` → `tests/c_test_authority_policy.rs::conflicting_code_intent_and_project_decisions_keep_distinct_authorities`
- `tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_unqualified_inapplicable_or_stale_sources_cannot_promote_memory_to_proof` → `tests/c_test_authority_policy.rs::unqualified_inapplicable_or_stale_sources_do_not_promote_memory_to_proof`
- `tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_invalid_identity_fact_references_and_scope_refuse_and_copied_artifact_is_read_only` → `tests/c_test_authority_policy.rs::invalid_identity_fact_references_and_scope_refuse_and_portable_binary_is_readonly`
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_aggregate_replacement_counts_the_complete_result_not_both_versions` → `tests/c_test_bank_eligibility_writes.rs::aggregate_replacement_counts_only_the_complete_result`
- `tests/contracts/test_claude_boundary.py::ClaudeBoundaryContract.test_uninitialized_and_nonrepository_are_quiet` → `tests/c_test_claude_boundary.rs::uninitialized_and_nonrepository_claude_stop_are_quiet`
- `tests/contracts/test_codex_stop.py::CodexStopContract.test_nonrepository_uninitialized_and_missing_source` → `tests/c_test_codex_stop.rs::nonrepository_uninitialized_and_missing_source_are_quiet`
- `tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_outside_edit_after_lost_commit_response_requires_explicit_replan` → `tests/c_test_committed_replanning.rs::retired_managed_channel_never_replans_over_external_activation_edits`
- `tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_uncommitted_or_conflicting_journals_never_authorize_readonly_plans` → `tests/c_test_committed_replanning.rs::uncommitted_or_conflicting_journals_never_authorize_retired_channel_updates`
- `tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_redirected_target_parent_refuses_at_preflight_without_writes` → `tests/c_test_committed_replanning.rs::redirected_target_parent_never_authorizes_retired_channel_activation`
- `tests/contracts/test_committed_replanning.py::CommittedReplanningContract.test_committed_managed_index_refresh_verifies_without_changing_payload_or_receipt` → `tests/c_test_committed_replanning.rs::retired_managed_index_refresh_preserves_payload_and_receipt_under_refusal`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b1_closed_pipe_child_is_observed_before_quiet_sibling_deadline` → `tests/c_test_identity_git_group.rs::b1_closed_pipe_child_is_observed_before_quiet_sibling_deadline`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b1_no_readable_fds_uses_bounded_owner_wait_then_observes_exit` → `tests/c_test_identity_git_group.rs::b1_no_readable_fds_uses_bounded_owner_wait_then_observes_exit`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_descendant_held_pipes_timeout_without_waiting_for_descendant_eof` → `tests/c_test_identity_git_group.rs::descendant_held_pipes_timeout_without_waiting_for_descendant_eof`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_each_deadline_starts_at_own_popen_return_not_first_launch` → `tests/c_test_identity_git_group.rs::each_deadline_starts_at_own_launch_return`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_later_timeout_does_not_cancel_earlier_pending_success` → `tests/c_test_identity_git_group.rs::later_timeout_does_not_cancel_earlier_success`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_selector_and_fd_infrastructure_never_become_optional_absence` → `tests/c_test_identity_git_group.rs::real_capture_fd_failure_never_becomes_optional_absence`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_terminal_earlier_command_precedence_and_optional_progress_to_fatal_barrier` → `tests/c_test_identity_git_group.rs::terminal_earlier_command_precedes_later_real_infrastructure_fault`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_cleanup_faults_attempt_remaining_real_kills_closes_and_reaps` → `tests/c_test_identity_git_group.rs::cleanup_real_fd_fault_still_finishes_other_children`
- `tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_file_only_checkpoint_resumes_without_generated_state_and_preserves_other_work` → `tests/c_test_manual_checkpoint.rs::file_only_checkpoint_resumes_without_generated_state_preserving_other_work`
- `tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_direct_hook_and_boundary_helpers_do_not_create_source_caches` → `tests/c_test_read_only_audits.rs::direct_native_hooks_and_boundary_help_preserve_runtime_bytes_and_modes`
- `tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_bytecode_prefix_keeps_isolation_dispatch_and_shell_restrictions` → `tests/c_test_read_only_audits.rs::retired_python_launchers_and_shell_injection_are_refused_without_execution`
- `tests/contracts/test_recall_observation.py::RecallObservationContract.test_portable_alias_guard_preserves_component_unicode_and_root_anchor_collisions` → `tests/c_test_recall_observation.rs::portable_aliases_preserve_unicode_components_and_root_anchor_distinctions`
- `tests/contracts/test_scope_policy.py::ScopePolicyContract.test_router_and_copied_portable_helper_share_the_same_scope_contract` → `tests/c_test_scope_policy.rs::router_and_relocated_binary_use_the_same_scope_contract`
- `tests/contracts/test_validation_memo.py::ValidationMemoContract.test_parsed_record_output_is_produced_even_after_a_format_hit` → `tests/c_test_validation_memo.rs::parsed_record_output_survives_format_hits_without_caller_aliasing`

## Runnable but partial identity obligations (8)

These currently pass, but do not yet force every baseline fault/stage. In particular, the signal worker sends SIGINT 20 ms after launch (before the 100 ms timeout); ordinary/parse stages wait for the interrupt before calling group.value, and no delayed/unreapable-child fixture forces the second interrupt into the reap window. The shared-budget/unreaped-count assertion is not fully covered. Parser precedence lacks the simultaneous cleanup fault; ordinary exit coverage lacks delayed unbounded-reap evidence. They must not be promoted from partial solely because they pass. Main confirmed ReapFix now exposes dev-tools-only `process::set_cancellation_signal`, which raises cancellation at a chosen stage, including during reap; use that approved real fault-point hook to complete these eight branches in the follow-up. It was not wired into this lane before the operator-requested wrap-up.

- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_earlier_parser_refusal_survives_later_command_and_cleanup_faults` → `tests/c_test_identity_git_group.rs::earlier_parser_refusal_survives_later_command_failure`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count` → `tests/c_test_identity_git_group.rs::b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup` → `tests/c_test_identity_git_group.rs::b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_second_interrupt_during_reap_ends_all_remaining_waits` → `tests/c_test_identity_git_group.rs::b2_second_interrupt_during_reap_ends_all_remaining_waits`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace` → `tests/c_test_identity_git_group.rs::interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff` → `tests/c_test_identity_git_group.rs::b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_interrupts_during_launch_select_and_parsing_preserve_primary_and_cleanup` → `tests/c_test_identity_git_group.rs::interrupts_during_launch_select_and_parsing_preserve_primary_and_cleanup`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_ordinary_and_system_exit_keep_unbounded_os_wait_parity` → `tests/c_test_identity_git_group.rs::ordinary_refusal_and_nonzero_exit_finish_all_owned_children`

## Gated/partial obligations (107)

- `tests/contracts/test_admission.py::AdmissionContract.test_repository_and_reviewed_tool_evidence_are_bound_to_actual_bytes_and_preserved` → `tests/c_test_admission.rs::reviewed_evidence_is_bound_to_actual_bytes_and_preserved` — needs recovery_frontier registered
- `tests/contracts/test_admission.py::AdmissionContract.test_arrived_learning_tampered_delta_and_changed_evidence_refuse_before_writing` → `tests/c_test_admission.rs::arrived_learning_tampered_delta_and_changed_evidence_refuse_before_writing` — needs recovery_frontier registered
- `tests/contracts/test_admission.py::AdmissionContract.test_duplicate_learning_defers_to_existing_identity_and_peer_conflict_to_detecting_developer` → `tests/c_test_admission.rs::duplicate_defers_to_existing_identity_and_peer_conflict_to_detector` — needs recovery_frontier registered
- `tests/contracts/test_admission.py::AdmissionContract.test_terminal_deleted_identity_and_derivations_are_quarantined_not_recaptured` → `tests/c_test_admission.rs::terminal_identity_and_derivations_are_quarantined_not_recaptured` — needs recovery_frontier registered
- `tests/contracts/test_admission.py::AdmissionContract.test_interrupted_admission_rolls_back_before_fresh_comparison_even_if_source_changed` → `tests/c_test_admission.rs::interrupted_admission_rolls_back_before_new_comparison_even_if_source_changed` — needs recovery_frontier and am transaction route registered
- `tests/contracts/test_admission.py::AdmissionContract.test_recorded_admission_recovers_lost_response_without_replaying_or_requiring_new_evidence` → `tests/c_test_admission.rs::recorded_admission_recovers_lost_response_without_requiring_new_evidence` — needs recovery_frontier and am transaction route registered
- `tests/contracts/test_admission.py::AdmissionContract.test_completed_receipt_cleanup_does_not_trust_corrupt_receipts_or_overwrite_external_memory` → `tests/c_test_admission.rs::completed_receipt_cleanup_rejects_corruption_and_preserves_external_memory` — needs recovery_frontier and am transaction route registered
- `tests/contracts/test_admission.py::AdmissionContract.test_large_existing_inventory_admits_or_finishes_recovery_without_losing_prior_learning` → `tests/c_test_admission.rs::large_existing_inventory_admits_or_finishes_without_losing_prior_learning` — needs recovery_frontier and am transaction route registered
- `tests/contracts/test_admission.py::AdmissionContract.test_evidence_change_at_final_readback_cannot_get_a_verified_admission_receipt` → `tests/c_test_admission.rs::final_readback_evidence_change_never_gets_verified_admission_receipt` — needs recovery_frontier and am transaction route registered
- `tests/contracts/test_authority_boundary.py::AuthorityBoundaryContract.test_raw_read_and_edit_need_no_identity_and_create_no_cli_audit` → `tests/c_test_authority_boundary.rs::raw_read_and_edit_need_no_identity_or_cli_audit` — needs recovery_frontier registered
- `tests/contracts/test_authority_boundary.py::AuthorityBoundaryContract.test_hostile_memory_cannot_execute_shell_or_replace_identity_and_approval` → `tests/c_test_authority_boundary.rs::hostile_memory_never_executes_or_replaces_explicit_identity` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_render_and_single_preview_reject_complete_utf8_source` → `tests/c_test_bank_eligibility_writes.rs::render_and_single_preview_reject_complete_utf8_source` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_single_apply_revalidates_a_schema_valid_changed_request` → `tests/c_test_bank_eligibility_writes.rs::single_apply_revalidates_a_schema_valid_changed_request` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_multifile_preview_and_apply_reject_complete_oversized_map` → `tests/c_test_bank_eligibility_writes.rs::multifile_preview_and_apply_reject_complete_oversized_map` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_multifile_capacity_includes_record_metadata` → `tests/c_test_bank_eligibility_writes.rs::multifile_capacity_includes_record_metadata` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_transaction_preflight_refuses_symlink_inside_runtime_directory` → `tests/c_test_bank_eligibility_writes.rs::transaction_preflight_refuses_symlink_inside_runtime_directory` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_transaction_preflight_refuses_incomplete_inventory_and_failed_target_stat` → `tests/c_test_bank_eligibility_writes.rs::preflight_refuses_inaccessible_inventory_and_unclassifiable_target` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_inventory_ordinary_apply_and_retry` → `tests/c_test_bank_eligibility_writes.rs::large_inventory_ordinary_apply_and_exact_retry` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_inventory_transaction_and_exact_retry` → `tests/c_test_bank_eligibility_writes.rs::large_inventory_transaction_finish_and_exact_retry` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_marker_rollback_and_unknown_marker_refusal` → `tests/c_test_bank_eligibility_writes.rs::large_marker_rollback_and_unknown_marker_refusal` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_large_owned_marker_keeps_frontier_but_unknown_or_changed_bytes_refuse` → `tests/c_test_bank_eligibility_writes.rs::large_owned_marker_keeps_frontier_but_changed_witness_refuses` — needs recovery_frontier registered
- `tests/contracts/test_bank_eligibility_writes.py::BankEligibilityWritesContract.test_unknown_marker_is_refused_without_opening_its_body` → `tests/c_test_bank_eligibility_writes.rs::unknown_fifo_marker_is_refused_without_opening_its_body` — needs recovery_frontier registered
- `tests/contracts/test_bank_layout.py::LayoutContract.test_copied_artifact_preserves_index_and_code_and_refuses_busy_or_interrupted_banks` → `tests/c_test_bank_layout.rs::relocated_binary_preserves_nested_bank_and_unknown_interruption_evidence` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_preview_readonly_and_verified_write_router_last_preserves_code_index_and_external_state_boundary` → `tests/c_test_bank_transactions.rs::preview_readonly_and_verified_write_preserves_external_state` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_recovery_preview_is_observed_and_a_subsequent_even_owned_version_change_refuses` → `tests/c_test_bank_transactions.rs::recovery_preview_refuses_a_subsequent_owned_version_change` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_lost_response_after_receipt_finishes_cleanup_without_replacing_and_cannot_switch_to_rollback` → `tests/c_test_bank_transactions.rs::lost_response_finishes_cleanup_without_replacing_or_switching_outcome` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_recovery_itself_can_be_interrupted_and_resumed` → `tests/c_test_bank_transactions.rs::recovery_itself_can_be_interrupted_and_resumed` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_stale_proposals_unknown_markers_invalid_projection_and_symlinks_preserve_originals` → `tests/c_test_bank_transactions.rs::stale_unknown_invalid_and_symlinked_proposals_preserve_originals` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_raw_crlf_router_rejected_before_preview_or_journal_even_if_changed_after_preview` → `tests/c_test_bank_transactions.rs::raw_crlf_router_refuses_before_preview_or_journal` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_external_omitted_context_directory_before_receipt_cannot_be_reported_as_a_usable_bank` → `tests/c_test_bank_transactions.rs::omitted_context_directory_before_receipt_cannot_be_reported_usable` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_alias_arriving_after_preview_is_not_inventoried_as_an_owned_target` → `tests/c_test_bank_transactions.rs::late_alias_is_not_an_owned_target` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_alias_arriving_after_recovery_preview_cannot_be_overwritten_in_either_direction` → `tests/c_test_bank_transactions.rs::late_recovery_alias_is_preserved_in_both_directions` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_single_file_reader_and_writer_share_exact_component_spelling_validation` → `tests/c_test_bank_transactions.rs::single_reader_and_writer_share_component_spelling_validation` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_bank_root_spelling_alias_refuses_before_lock_and_unique_unicode_target_still_works` → `tests/c_test_bank_transactions.rs::bank_root_alias_refuses_and_unique_unicode_target_works` — needs recovery_frontier registered
- `tests/contracts/test_bank_transactions.py::BankTransactionsContract.test_portable_copy_uses_same_protocol_and_memory_body_remains_unexecuted_data` → `tests/c_test_bank_transactions.rs::relocated_binary_uses_same_protocol_and_never_executes_memory` — needs recovery_frontier registered
- `tests/contracts/test_bank_usability.py::BankUsabilityContract.test_fresh_clone_resumes_from_bounded_current_markdown_without_any_generated_state` → `tests/c_test_bank_usability.rs::fresh_clone_resumes_from_bounded_canonical_markdown_without_state` — needs recovery_frontier registered
- `tests/contracts/test_bank_usability.py::BankUsabilityContract.test_deleting_generated_state_does_not_make_an_interrupted_bank_usable_or_repair_it` → `tests/c_test_bank_usability.rs::state_loss_never_repairs_or_hides_an_interrupted_bank` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_stale_manual_edit_is_preserved_and_fresh_validation_accepts_valid_external_change` → `tests/c_test_bank_writes.rs::stale_manual_edit_is_preserved_and_fresh_validation_accepts_valid_change` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_readback_and_receipt_creation_happen_while_the_lock_is_held` → `tests/c_test_bank_writes.rs::readback_and_receipt_creation_happen_under_lock` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_external_read_set_edit_prevents_a_stale_write_to_another_file` → `tests/c_test_bank_writes.rs::external_read_set_edit_blocks_a_different_target_write` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_live_foreign_and_unknown_stale_locks_are_preserved_for_review` → `tests/c_test_bank_writes.rs::live_foreign_and_unknown_stale_locks_are_preserved` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_runtime_state_is_excluded_and_interrupted_bank_routes_to_recovery` → `tests/c_test_bank_writes.rs::runtime_state_is_excluded_and_interrupted_bank_routes_to_recovery` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_state_root_failure_is_detected_before_replacing_memory` → `tests/c_test_bank_writes.rs::state_root_failure_is_detected_before_replacing_memory` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_two_worktrees_have_separate_locks_and_receipts_without_cross_machine_claims` → `tests/c_test_bank_writes.rs::worktrees_have_separate_locks_and_receipts` — needs recovery_frontier registered
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_portable_copy_and_untrusted_markdown_remain_data` → `tests/c_test_bank_writes.rs::portable_binary_and_untrusted_markdown_remain_data` — needs recovery_frontier registered
- `tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_changed_source_refused_at_recheck` → `tests/c_test_bounded_bank_observation.rs::changed_canonical_source_refuses_at_observation_recheck` — needs recovery_frontier registered
- `tests/contracts/test_bounded_resume.py::BoundedResumeContract.test_router_then_exact_task_context_omits_unrelated_records_without_optional_stores` → `tests/c_test_bounded_resume.rs::exact_task_resume_omits_unrelated_records_without_optional_stores` — needs recovery_frontier registered
- `tests/contracts/test_bounded_resume.py::BoundedResumeContract.test_exact_resume_preserves_branch_unknown_validity_trust_and_kind_gates` → `tests/c_test_bounded_resume.rs::exact_resume_preserves_all_policy_gates` — needs recovery_frontier registered
- `tests/contracts/test_bounded_resume.py::BoundedResumeContract.test_ambiguous_task_keeps_only_router_and_prose_never_gains_typed_trust` → `tests/c_test_bounded_resume.rs::ambiguous_task_does_not_load_details_or_certify_prose_trust` — needs recovery_frontier registered
- `tests/contracts/test_codex_stop.py::CodexStopContract.test_proactive_complete_nullable_final_late_steering_and_durable_wake_cap` → `tests/c_test_codex_stop.rs::proactive_completion_nullable_final_late_steering_and_durable_wake_cap` — needs recovery_frontier registered
- `tests/contracts/test_codex_stop.py::CodexStopContract.test_quiet_completion_surfaces_earlier_pending_once` → `tests/c_test_codex_stop.rs::quiet_completion_surfaces_earlier_pending_only_once` — needs recovery_frontier registered
- `tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_four_contenders_refuse_live_owner_and_stale_retries_then_preserve_every_rebased_fact` → `tests/c_test_concurrent_bank_writers.rs::four_real_contenders_refuse_owner_and_stale_retries_preserving_all_rebased_facts` — needs recovery_frontier registered
- `tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_concurrent_observer_sees_complete_old_or_new_bytes_through_atomic_replacement` → `tests/c_test_concurrent_bank_writers.rs::real_concurrent_observer_sees_only_complete_old_or_new_bytes` — needs recovery_frontier registered
- `tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_actual_external_editor_invalidates_the_observed_target_without_losing_its_edit` → `tests/c_test_concurrent_bank_writers.rs::real_external_editor_invalidates_target_without_losing_its_edit` — needs recovery_frontier registered
- `tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_killed_transaction_owner_cannot_be_stolen_and_public_recovery_preserves_the_complete_bank` → `tests/c_test_concurrent_bank_writers.rs::killed_transaction_owner_cannot_be_stolen_and_explicit_recovery_preserves_bank` — needs recovery_frontier and am transaction route registered
- `tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_distinct_worktrees_hold_independent_locks_and_sharing_manifests_contain_only_canonical_paths` → `tests/c_test_concurrent_bank_writers.rs::distinct_worktrees_hold_independent_locks_and_share_only_canonical_paths` — needs recovery_frontier registered
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_minimal_untracked_bank_survives_poisoned_then_removed_application_state` → `tests/c_test_current_bank.rs::untracked_bank_survives_poisoned_then_removed_application_state` — needs am inspect route and recovery_frontier registered
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_all_six_tracked_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_are_shared` → `tests/c_test_current_bank.rs::all_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_shared` — needs am inspect route and recovery_frontier registered
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_git_root_spelling_preserves_trailing_spaces` → `tests/c_test_current_bank.rs::git_root_spelling_preserves_trailing_spaces` — needs am inspect route and recovery_frontier registered
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_busy_or_interrupted_bank_is_not_repaired_by_state_loss` → `tests/c_test_current_bank.rs::busy_or_interrupted_bank_is_not_repaired_by_state_loss` — needs am inspect route and recovery_frontier registered
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_git_inspection_ignores_ambient_repo_redirection_and_does_not_execute_fsmonitor_or_write_trace` → `tests/c_test_current_bank.rs::git_inspection_ignores_ambient_redirection_fsmonitor_and_trace` — needs am inspect route and recovery_frontier registered
- `tests/contracts/test_identity_canaries.py::IdentityCanaries.test_changed_developer_selection_invalidates_old_checkpoint_without_reattribution` → `tests/c_test_identity_canaries.rs::changed_developer_selection_invalidates_old_checkpoint_without_reattribution` — needs recovery_frontier registered
- `tests/contracts/test_identity_canaries.py::IdentityCanaries.test_replacing_bank_with_another_project_never_silently_rebinds_or_recalls_it` → `tests/c_test_identity_canaries.rs::replacing_bank_with_another_project_never_silently_rebinds_or_recalls_it` — needs recovery_frontier registered
- `tests/contracts/test_identity_canaries.py::IdentityCanaries.test_checkpoint_provenance_survives_fresh_machine_and_second_developer_without_personal_bank` → `tests/c_test_identity_canaries.rs::checkpoint_provenance_survives_fresh_machine_and_second_developer` — needs recovery_frontier registered
- `tests/contracts/test_independent_local_bank.py::IndependentLocalBankContract.test_initialize_checkpoint_resume_and_search_without_any_global_service` → `tests/c_test_independent_local_bank.rs::initialize_checkpoint_resume_and_search_without_global_services` — needs recovery_frontier registered
- `tests/contracts/test_independent_local_bank.py::IndependentLocalBankContract.test_version_export_and_plain_file_repair_after_disposable_state_loss` → `tests/c_test_independent_local_bank.rs::version_export_and_plain_file_repair_after_state_loss` — needs recovery_frontier registered
- `tests/contracts/test_independent_local_bank.py::IndependentLocalBankContract.test_local_interrupted_checkpoint_recovers_with_retained_private_receipt_evidence` → `tests/c_test_independent_local_bank.rs::interrupted_local_checkpoint_recovers_from_retained_receipt_evidence` — needs recovery_frontier registered
- `tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_stale_mediated_proposal_refuses_then_fresh_write_preserves_manual_evidence` → `tests/c_test_manual_checkpoint.rs::stale_mediated_proposal_refuses_and_fresh_write_preserves_manual_evidence` — needs recovery_frontier registered
- `tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_visible_lock_and_interrupted_bank_stop_mediated_resume_without_repair` → `tests/c_test_manual_checkpoint.rs::visible_lock_and_interruption_stop_mediated_resume_without_repair` — needs recovery_frontier registered
- `tests/contracts/test_manual_checkpoint.py::ManualCheckpointContract.test_invalid_external_edit_is_reported_instead_of_template_repair` → `tests/c_test_manual_checkpoint.rs::invalid_external_edit_is_reported_instead_of_template_repair` — needs recovery_frontier registered
- `tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_repeated_installed_and_documented_source_audits_preserve_every_byte_and_mode` → `tests/c_test_read_only_audits.rs::repeated_installed_and_source_audits_preserve_all_runtime_bytes_and_modes` — needs development installation providers registered
- `tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_outside_receipt_aware_audit_refuses_and_preserves_inert_cache_drift` → `tests/c_test_read_only_audits.rs::outside_receipt_aware_audit_refuses_and_preserves_opaque_cache_drift` — needs development installation providers registered
- `tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_ordinary_recall_preserves_optional_navigation_catalog_without_republishing_it` → `tests/c_test_recall_catalog.rs::ordinary_recall_preserves_navigation_catalog_without_republication` — needs recovery_frontier registered
- `tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_common_installed_cli_reuses_the_same_validated_catalog` → `tests/c_test_recall_catalog.rs::common_cli_reuses_the_same_validated_catalog` — needs recovery_frontier registered
- `tests/contracts/test_recall_catalog.py::RecallCatalogContract.test_warm_policy_matches_full_reader_for_hidden_unsafe_and_conditional_corrections` → `tests/c_test_recall_catalog.rs::warm_catalog_policy_matches_full_reader_for_all_replacement_gates` — needs recovery_frontier registered
- `tests/contracts/test_recall_observation.py::RecallObservationContract.test_omitted_context_occupied_by_directory_or_fifo_is_not_absence` → `tests/c_test_recall_observation.rs::omitted_context_directory_or_fifo_is_not_absence` — needs recovery_frontier registered
- `tests/contracts/test_recall_observation.py::RecallObservationContract.test_changed_source_after_read_or_at_final_recheck_is_unavailable` → `tests/c_test_recall_observation.rs::changed_source_after_read_or_final_recheck_is_unavailable` — needs recovery_frontier registered
- `tests/contracts/test_recall_observation.py::RecallObservationContract.test_fifo_marker_arriving_at_final_recheck_refuses_without_blocking_or_removal` → `tests/c_test_recall_observation.rs::fifo_marker_arriving_at_final_recheck_is_preserved_without_blocking` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_changed_head_with_deleted_competing_router_refuses_enrollment_exit` → `tests/c_test_repository_observation_batch.rs::changed_head_with_deleted_competing_router_refuses_enrollment` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_index_untracked_and_raw_locators_added_after_scope_read_refuse_exit` → `tests/c_test_repository_observation_batch.rs::late_indexed_untracked_and_raw_locators_refuse_enrollment` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_earlier_real_config_refusal_beats_every_later_child_failure` → `tests/c_test_repository_observation_batch.rs::invalid_real_config_precedes_every_later_child_failure` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_config_child_output_never_accepts_speculative_names` → `tests/c_test_repository_observation_batch.rs::malformed_config_child_output_never_accepts_speculative_names` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_name_children_refuse_after_valid_real_config` → `tests/c_test_repository_observation_batch.rs::malformed_inventory_children_refuse_after_valid_real_config` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_parent_traversal_name_never_adopts_outside_router` → `tests/c_test_repository_observation_batch.rs::parent_traversal_inventory_never_adopts_outside_router` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_config_validation_time_is_charged_to_unresolved_inventory_child` → `tests/c_test_repository_observation_batch.rs::config_observation_time_is_charged_to_pending_inventory_deadline` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_sampled_head_is_immutable_and_next_settings_read_observes_later_commit` → `tests/c_test_repository_observation_batch.rs::sampled_head_is_immutable_and_next_settings_read_observes_later_commit` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_timeout_and_launch_failure_never_mint_identity_from_partial_output` → `tests/c_test_repository_observation_batch.rs::timeout_and_git_launch_failures_never_mint_identity_from_partial_output` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_tracked_body_and_enrollment_boundaries_have_no_live_query_children` → `tests/c_test_repository_observation_batch.rs::tracked_body_and_enrollment_boundaries_leave_no_live_query_children` — needs recovery_frontier registered
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_interrupt_in_real_config_validation_preserves_traceback_and_cancels_siblings` → `tests/c_test_repository_observation_batch.rs::interrupt_during_real_config_observation_cancels_siblings_and_preserves_bank` — needs recovery_frontier registered
- `tests/contracts/test_router.py::RouterContract.test_minimal_bank_resumes_from_only_canonical_files` → `tests/c_test_router.rs::minimal_bank_resumes_from_canonical_files_only` — needs recovery_frontier registered
- `tests/contracts/test_router.py::RouterContract.test_paths_cannot_route_runtime_state_outside_bank_or_personal_overlays` → `tests/c_test_router.rs::router_never_routes_runtime_outside_or_personal_paths` — needs recovery_frontier registered
- `tests/contracts/test_router.py::RouterContract.test_optional_topic_is_readable_and_runtime_state_is_not_routed` → `tests/c_test_router.rs::optional_topic_is_readable_but_runtime_is_not_routed` — needs recovery_frontier registered
- `tests/contracts/test_router.py::RouterContract.test_portable_helper_works_after_copy_outside_checkout` → `tests/c_test_router.rs::relocated_native_binary_reads_the_same_router` — needs recovery_frontier registered
- `tests/contracts/test_tombstones.py::TombstonesContract.test_delete_rebuild_and_repeat_cannot_resurrect_id_under_changed_conditions` → `tests/c_test_tombstones.rs::repeated_rebuild_cannot_resurrect_terminal_id_under_changed_conditions` — needs recovery_frontier registered
- `tests/contracts/test_tombstones.py::TombstonesContract.test_unknown_ids_wrong_project_invalid_history_and_interrupted_bank_stop_rebuild` → `tests/c_test_tombstones.rs::wrong_project_unknown_ids_invalid_history_and_interruption_stop_rebuild` — needs recovery_frontier registered
- `tests/contracts/test_tombstones.py::TombstonesContract.test_copied_portable_guard_uses_the_same_canonical_project_history` → `tests/c_test_tombstones.rs::portable_binary_uses_the_same_canonical_tombstone_history` — needs recovery_frontier registered
- `tests/contracts/test_tombstones.py::TombstonesContract.test_history_namespace_route_state_and_record_identity_must_match_before_rebuild` → `tests/c_test_tombstones.rs::history_namespace_route_state_and_identity_must_match` — needs recovery_frontier registered
- `tests/contracts/test_validation_memo.py::ValidationMemoContract.test_public_land_rechecks_source_authority_index_and_policy_after_first_check` → `tests/t2_publication.rs::fanout_d_public_land_rechecks_source_authority_index_and_policy` — needs recovery_frontier registered
- `tests/contracts/test_claude_boundary.py::ClaudeBoundaryContract.test_published_ack_requires_real_authority_readback` → `tests/t2_publication.rs::fanout_d_claude_published_ack_requires_real_authority_readback` — needs recovery_frontier registered
- `tests/contracts/test_opencode_boundary_ledger.py::PublishedAcknowledgmentContract.test_requires_real_witnessed_publication` → `tests/t2_publication.rs::fanout_d_opencode_ack_requires_real_witnessed_publication` — needs recovery_frontier registered
- `tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_publication_refuses_a_live_bank_lock_then_lands_without_runtime_state` → `tests/t2_publication.rs::fanout_d_publication_refuses_live_bank_lock_then_lands_only_canonical_paths` — needs recovery_frontier registered
- `tests/contracts/test_new_topic_merge.py::NewTopicMergeContract.test_new_index_with_identical_introduction_and_distinct_routes_preserves_both_sources` → `tests/t2_publication.rs::fanout_d_new_shared_index_preserves_both_independent_source_routes` — needs recovery_frontier registered
- `tests/contracts/test_new_topic_merge.py::NewTopicMergeContract.test_absent_prose_baseline_does_not_license_different_new_introductions` → `tests/t2_publication.rs::fanout_d_absent_prose_baseline_does_not_license_conflicting_introductions` — needs recovery_frontier registered
- `tests/contracts/test_new_topic_merge.py::NewTopicMergeContract.test_absent_route_baseline_does_not_choose_between_different_captions_for_one_target` → `tests/t2_publication.rs::fanout_d_absent_route_baseline_never_chooses_conflicting_captions` — needs recovery_frontier registered
- `tests/contracts/test_observation_applicability.py::ObservationApplicability.test_unchanged_evidence_survives_memory_only_head_and_capture_branch_changes_without_duplication` → `tests/t2_initialization.rs::fanout_d_unchanged_evidence_survives_memory_head_and_branch_changes` — needs recovery_frontier registered
- `tests/contracts/test_observation_applicability.py::ObservationApplicability.test_changed_source_refuses_stale_write_and_existing_changed_hash_requires_review_without_duplicate` → `tests/t2_initialization.rs::fanout_d_changed_source_refuses_stale_write_without_duplicate_observation` — needs recovery_frontier registered
- `tests/contracts/test_observation_applicability.py::ObservationApplicability.test_legacy_inputs_and_real_branch_conditions_stay_conservative_and_unknown_old_hashes_remain_qualified` → `tests/t2_initialization.rs::fanout_d_legacy_observations_and_explicit_branch_conditions_stay_conservative` — needs recovery_frontier registered

## Not applicable (18; Main owns ledger classification)

- `tests/contracts/test_gate_shard_aggregation.py::ShardAggregationContract.test_complete_receipt_requires_the_actual_lifecycle_case` — Python test-gate harness (scripts/test_gate.py) retired; cargo test is the v0.0.15 gate
- `tests/contracts/test_gate_shard_aggregation.py::ShardAggregationContract.test_incomplete_or_incompatible_receipts_are_refused` — Python test-gate harness (scripts/test_gate.py) retired; cargo test is the v0.0.15 gate
- `tests/contracts/test_gate_shard_aggregation.py::ShardAggregationContract.test_unsplit_worker_reports_only_successfully_executed_cases` — Python test-gate harness (scripts/test_gate.py) retired; cargo test is the v0.0.15 gate
- `tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_actual_v002_runtime_failure_is_repaired_only_by_outside_helper` — executes a frozen Python v0.0.2 runtime; the repository holds no Python (ADR-0029 captured-state rule); real legacy adoption is proved in release qualification outside the repo
- `tests/contracts/test_multi_machine_matrix.py::MultiMachineMatrix.test_every_approved_shared_learning_obligation_has_registered_executable_cases` — Python AST registry check; Rust ledger enforced by tests/obligations_ledger.rs
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_active_frozen_inputs_refuse_archived_links_and_source_substitution` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_ignored_linked_target_caches_and_cycles_are_rejected_before_fixture_import` — Python bytecode/import-cache only
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_independent_pagination_batches_compare_every_equivalent_exact_native_request` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_linked_stale_helper_cache_rejected_by_all_bootstraps_without_following_it` — Python bytecode/import-cache only
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_matching_stale_binding_helper_is_rejected_before_every_local_import` — Python bytecode/import-cache only
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_real_public_results_cannot_hide_order_finish_or_recovery_action_changes` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_refresh_restore_retains_public_checkout_binding` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_semantic_adapter_deadline_keeps_unknown_children_unsignalled` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_source_binding_hashes_data_link_bytes_without_following_or_executing_links` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_target_world_cache_rejected_before_import_and_clean_source_still_binds` — Python bytecode/import-cache only
- `tests/contracts/test_recall_qualification.py::RecallQualificationContract.test_wrong_or_mixed_executing_models_leave_quality_unqualified` — evaluation controller retired in v0.0.15 (operator decision, #206 → v0.0.16)
- `tests/contracts/test_worker_collection.py::WorkerCollectionContract.test_completed_and_timed_out_worker_groups_do_not_leave_descendants` — Python World group/killpg collector retired; Rust World owns direct children only (lane brief: never process groups)
- `tests/contracts/test_worker_ownership.py::WorkerOwnershipContract.test_normal_collection_never_signals_a_group_after_reaping_its_owner` — Python World group/killpg collector retired; Rust World owns direct children only (lane brief: never process groups)

## Unported gaps (28)

No ported claim is made for these methods. These require additional real lifecycle/multi-machine/fault-point fixtures, not automatic not-applicable classification. The two identity machinery/registration tests need exposed real fault points or a product-owner decision.

- `tests/contracts/test_checkout_repair.py::CheckoutRepairContract.test_doctor_recognizes_and_persists_old_device_binding`
- `tests/contracts/test_checkout_repair.py::CheckoutRepairContract.test_doctor_reports_prompt_and_stop_failures_independently`
- `tests/contracts/test_discovery.py::DiscoveryContract.test_discovered_equivalent_cannot_be_omitted_from_comparison_to_admit_duplicate`
- `tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.test_fresh_and_existing_install_check_and_apply_refuse_unsupported_bank_without_activation`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_machinery_memory_and_runtime_errors_are_fatal_not_optional_absence`
- `tests/contracts/test_identity_git_group.py::IdentityGitGroup.test_partial_registration_failure_closes_previously_owned_children_and_selector`
- `tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_indexed_review_refuses_changed_source_or_authority`
- `tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_indexed_review_requires_every_pair_and_preserves_conflict`
- `tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_wrong_project_recovery_preserves_private_state_and_stale_bank_marker`
- `tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_doctor_uses_the_same_legacy_ownership_check`
- `tests/contracts/test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_interrupted_switch_restores_exact_drifted_source_and_retries`
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_completed_omp_drains_before_layout_then_old_reader_after_both`
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_managed_update_accepts_repository_under_symlinked_ancestor`
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_omp_first_mixed_pending_rejects_actual_published_old_managed_candidate`
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_private_lifecycle_observation_refuses_config_bank_hold_and_attestation_races`
- `tests/contracts/test_multi_machine_compatible.py::MultiMachineCompatible.test_two_developers_three_clones_and_a_worktree_reconcile_without_importing_feature_code`
- `tests/contracts/test_multi_machine_conflicts.py::MultiMachineConflicts.test_detecting_developer_sees_both_origins_without_pausing_independent_memory_or_code`
- `tests/contracts/test_multi_machine_merge_replay.py::MultiMachineMergeReplay.test_erasure_survives_an_ordinary_code_merge_and_pre_erasure_proposal_replay`
- `tests/contracts/test_multi_machine_race.py::MultiMachineRace.test_simultaneous_public_publishers_consolidate_duplicate_learning_before_retry_lands`
- `tests/contracts/test_recall_observation.py::RecallObservationContract.test_owned_lock_disappearance_or_replacement_refuses_public_maintenance_before_write`
- `tests/contracts/test_recover_commit_write_copy_after.py::LifecycleWriteContract.test_after_every_recover_commit_copy_write`
- `tests/contracts/test_recover_commit_write_copy_before.py::LifecycleWriteContract.test_before_every_recover_commit_copy_write`
- `tests/contracts/test_recover_rollback_write_copy_after.py::LifecycleWriteContract.test_after_every_recover_rollback_copy_write`
- `tests/contracts/test_recover_rollback_write_copy_before.py::LifecycleWriteContract.test_before_every_recover_rollback_copy_write`
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_verified_hold_remains_readonly_through_grouped_settings_and_enrollment`
- `tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_remote_query_cleanup_failure_cannot_become_successful_discovery`
- `tests/contracts/test_shared_learning_loop.py::SharedLearningLoopContract.test_automatic_durable_capture_lands_and_another_actor_recalls_it_without_feature_code`
- `tests/contracts/test_turn_sources.py::Sources.test_real_prompt_registration_steering_stdin_ack_and_cap_blocks_input`

## Ledger hand-off

Ran the required ledger-mark.sh helper against all new c_test files with the 34 initially runnable names; it marked 28 unported names ported and 92 unported names partial (existing partial rows are not promoted by that helper). Then explicitly marked the ten appended t2 obligations partial and the initial runnable set ported. Final correction marks the eight incomplete runnable identity obligations back to partial. Final intended owned accounting is 26 ported / 115 partial; the 18 approved not-applicable IDs are left for Main to classify with their required notes, and 28 missing methods remain unported.

Name artifacts (outside repository):
- fanout-d-ported.txt: final 26 fully ported names (eight incomplete names removed).
- fanout-d-appended-partial.txt: ten appended t2 obligations.
- fanout-d-runnable-partial.txt: eight incomplete identity obligations; this overrides their initial runnable classification.

## Verification environment

Darwin 27.3.0 arm64; rustc 1.99.1 (58cf8f9c3, 2026-10-13), host aarch64-apple-darwin, LLVM 23.1.1. No Linux verification. No baseline Python tests executed.

Each new target was compiled and run when completed, including targets whose obligation tests are ignored. After refreshing 5764442c, identity and authority targets passed (20 passed, two ignored); managed refusal cases passed (four); native source-audit/refusal tests passed (three, two ignored). Ignored frontier tests were never described as successful behavior checks.

## Final verification

- `cargo fmt --check`: passed.
- `cargo check --offline --all-targets --features dev-tools`: passed with zero warnings.
- All 27 new test targets plus the lane's appended t2 tests ran three consecutive rounds: each round **44 passed, 0 failed, 107 ignored**. The 44 include fixture-worker entry tests; 34 are annotated obligation tests, of which eight remain partial.
- `cargo test --offline --no-fail-fast --features dev-tools`: green on macOS, **1,276 passed, 0 failed, 376 ignored** across 108 result groups (including doc tests).
- Final combined verification duration: 649.21 seconds. Observed output artifact: `artifact://13922`.
- The eight runnable identity partial statuses were explicitly corrected with `am-dev obligations mark` (exit 0); they are not ported despite passing.

Hand-off remains incomplete by operator-directed wrap-up: recovery-frontier/provider gates, eight stage-specific identity branches, and 28 unimplemented obligations are explicitly listed above. No product-code fixes beyond Main's authorized integration refresh were made.
