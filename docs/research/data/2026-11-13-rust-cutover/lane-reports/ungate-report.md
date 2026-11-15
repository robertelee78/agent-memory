# ungate report

## Root causes

1. The disposable signed channel used raw filesystem paths in file:// URLs; its deliberate space-containing directory was not URL-encoded. curl correctly refused those URLs.
2. Publication, sharing-recovery and the identity fixture still require recovery_frontier through the existing-bank binding snapshot. Landing refresh/sharing/admission/initialization/projections did not satisfy that prerequisite. Lifecycle tests that already have bank data likewise reach the private-state frontier guard.
3. Diagnostics in the signed-channel worker identified the test harness executable rather than the real installed binary. Separately, invoking the release binary against an ephemeral fixture signing key correctly failed the binary’s production trust roots. The fixture now calls the public diagnostics API with its actual signing roots and installed executable; production trust is unchanged.
4. Native fixture mismatches: OpenCode’s installer path is XDG_CONFIG_HOME/opencode/plugins, not OPENCODE_CONFIG_DIR; Git restores executable bits through the private World umask, producing 0700 rather than the native binary’s required 0755; portable_files expects a directory, not an unresolved client symlink; the outside audit must alter an owned copy, not the editable development source (whose drift is only a warning).
5. pack_validation dropped delegated command ancestors while reporting a child operation. Its documented_commands rows could not reconstruct the real native invocation.
6. Two publication tests were deferred_body placeholders, and the projection test did not actually select or stale a projection. All three now contain the full baseline scenarios, remaining ignored solely for the frontier dependency.
7. The CI guide-example fixture passed Cargo’s uplifted executable directly to a release input reader. Such inputs may be hard-linked. The fixture now makes a real private ordinary copy first; the single-link refusal remains intact.

## Product fixes, with baseline lines

- src/pack_validation.rs, check_dispatch: retain each delegated operation in the reported command path before returning the final operation. Baseline skills/agent-memory/scripts/agent_memory/pack_validation.py:480–502 retains the original helper and operation across delegation (especially 490–502); Rust command paths must likewise retain the actual dispatch. Existing native public-help enumerations exercise every reported command. No new trait, provider seam, crate, exported API or foundation primitive.
- No other product changes. In particular, release_distribution’s ordinary-single-link refusal is unchanged and matches baseline release_distribution.py:69–78, especially 74–75.
- Existing guide skills/agent-memory/maintain/pack/GUIDE.md documents command-path readback. No new product delta.

## Tests changed

Outcome for the original 67 tests: 22 enabled; 45 accurately ignored as needs recovery_frontier registered. The extra CI regression is enabled as well.

Fixture changes:
- tests/support/distribution.rs: percent-encode file URLs, set the diagnostic executable to the real installed runtime, and exercise the diagnostics public API with the disposable channel’s real trust roots.
- tests/c_test_installer.rs: match the baseline OpenCode path (baseline tests/contracts/test_installer.py:325).
- tests/c_test_lifecycle_trust.rs: explicitly restore native mode after Git checkout; retain real Git metadata/alias cases.
- tests/c_test_installed_layout.rs: resolve client links before inventory and inspect ephemeral signed releases through the public API rather than pretend the production binary trusts fixture keys.
- tests/c_test_read_only_audits.rs: install a real owned copy, place inert drift there as baseline tests/contracts/test_read_only_audits.py:95–115 does, require runtime.target.copy failure, and preserve the entire World inventory.
- tests/t2_publication.rs: fully implement the unmerged admission/publication journey from baseline test_publication_admission.py:13–45 and the versioned known-hold recovery journey from test_publication_hold_recovery.py:12–45 / test_quarantined_sharing_v2.py:23–53. Neither uses deferred_body.
- tests/t2_diagnostics.rs: full real-install projection current/stale/read-only journey, baseline test_runtime_diagnostics.py:580–596; no stand-in service in this test.
- tests/c_test_incident_evidence.rs: copy the actual native binary into the World before capture-guide-examples.

All changed integration-test targets:
- tests/c_test_cache_update_recovery.rs
- tests/c_test_doctor_fix.rs
- tests/c_test_download_diagnostics.rs
- tests/c_test_installed_layout.rs
- tests/c_test_installer.rs
- tests/c_test_lifecycle_cleanup_after.rs
- tests/c_test_lifecycle_cleanup_before.rs
- tests/c_test_lifecycle_lost_responses.rs
- tests/c_test_lifecycle_trust.rs
- tests/c_test_portable_artifact.rs
- tests/c_test_public_cli.rs
- tests/c_test_read_only_audits.rs
- tests/c_test_runtime_update.rs
- tests/c_test_skill_routes.rs
- tests/c_test_update_discovery_gaps.rs
- tests/t2_diagnostics.rs
- tests/t2_publication.rs
- tests/t2_sharing_recovery.rs
- tests/t_support_world.rs
- tests/c_test_incident_evidence.rs

## Fault points added

None. Existing token-bound native fault boundaries are retained. No mocks, fake providers, Python execution, new crates, subagents, Git writes or tracker writes were introduced.

## Annotations

- Existing annotations retained for the 22 enabled ex-ungate tests and the extra incident regression. Their exact IDs:
- tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_older_signed_runtime_upgrades_preserving_all_opaque_bytes → c_test_cache_update_recovery::rust_signed_runtime_refuses_opaque_bytecode_without_qualification_or_execution
- tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_copy_client_drift_does_not_receive_source_qualification → c_test_cache_update_recovery::copied_client_opaque_drift_never_receives_source_qualification
- tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_reachable_writable_cache_directory_remains_refused → c_test_cache_update_recovery::reachable_writable_opaque_cache_directory_remains_refused
- tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_interrupted_activation_preserves_dirty_tree_as_pending_then_trusted_retry → c_test_cache_update_recovery::opaque_bytecode_is_refused_before_activation_and_retained_on_trusted_retry
- tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_concurrent_cache_writer_is_preserved_then_source_only_retry_converges → c_test_cache_update_recovery::changed_opaque_cache_bytes_are_preserved_and_never_qualified_by_retry
- tests/contracts/test_doctor_fix.py::DoctorFixContract.test_recover_update_interrupted_before_receipt → c_test_doctor_fix::doctor_rolls_back_update_interrupted_before_receipt
- tests/contracts/test_doctor_fix.py::DoctorFixContract.test_recover_committed_interrupted_update_from_recorded_journal → c_test_doctor_fix::doctor_finishes_committed_interrupted_update_from_recorded_journal
- tests/contracts/test_download_diagnostics.py::BootstrapPreview.test_signed_preview_uses_packaged_guidance_and_preserves_profiles → c_test_download_diagnostics::signed_preview_uses_packaged_guidance_without_profile_changes
- tests/contracts/test_installed_layout.py::InstalledLayoutContract.test_no_client_mode_still_exposes_installed_lifecycle_without_a_bank_or_identity → c_test_installed_layout::no_client_mode_exposes_lifecycle_without_bank_or_identity
- tests/contracts/test_installed_layout.py::InstalledLayoutContract.test_custom_only_discovery_does_not_create_default_client_links_or_guidance → c_test_installed_layout::custom_only_targets_never_create_default_client_links_or_guidance
- tests/contracts/test_installer.py::InstallerContract.test_opencode_versioned_hook_is_owned_and_removed → c_test_installer::opencode_hook_is_owned_and_removed_without_touching_other_plugins
- tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_actual_linked_worktree_relative_common_pointer_and_native_alias_work → c_test_lifecycle_trust::linked_worktree_relative_common_pointer_and_native_alias_work
- tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_common_metadata_directory_with_real_trailing_space_is_not_rewritten → c_test_lifecycle_trust::real_trailing_space_in_common_metadata_is_not_trimmed
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_every_implemented_documented_helper_imports_and_dispatches_from_the_copy → c_test_portable_artifact::every_documented_native_command_dispatches_from_a_complete_copy
- tests/contracts/test_read_only_audits.py::ReadOnlyAudits.test_outside_receipt_aware_audit_refuses_and_preserves_inert_cache_drift → c_test_read_only_audits::outside_receipt_aware_audit_refuses_and_preserves_opaque_cache_drift
- tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_changed_owned_copy_update_retains_prior_payload_and_receipt_backups → c_test_runtime_update::changed_owned_copy_retains_payload_and_receipt_backups
- tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_competing_update_observes_the_live_lifecycle_lock → c_test_runtime_update::competing_update_observes_live_lifecycle_lock
- tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_unchanged_revision_refreshes_allowed_outside_text_without_a_code_switch → c_test_runtime_update::unchanged_revision_preserves_outside_text_without_switching_code
- tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_unchanged_channel_does_not_derive_a_new_artifact_from_a_different_umask → c_test_runtime_update::unchanged_signed_channel_does_not_rederive_artifact_under_private_umask
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_every_documented_helper_dispatch_also_accepts_real_public_help → c_test_skill_routes::every_documented_native_dispatch_accepts_real_public_help
- tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_missing_explicit_repository_remains_an_error_and_preserves_targets → c_test_update_discovery_gaps::missing_explicit_repository_is_an_error_and_preserves_targets
- tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_available_repository_without_a_bank_stays_in_compatibility_rechecks → c_test_update_discovery_gaps::available_bankless_repository_remains_in_compatibility_rechecks
- tests/contracts/test_incident_evidence.py::IncidentEvidenceContract.test_examples_are_actual_public_exit_results_bound_to_the_complete_pack → c_test_incident_evidence::guide_examples_are_actual_native_exit_results_bound_to_complete_pack
- Removed 44 baseline-obligation annotations from frontier-blocked tests, per the lane instruction; the additional blocked support fixture has no baseline obligation. All 45 blocked tests are unannotated and have complete bodies.
- Ran ledger-mark.sh from this worktree with ungate-ported-ids.txt and the enabled test files. Result: marked ported=0 partial=0; existing ledger totals not-applicable=90 partial=792 ported=401. It made no ledger change. No not-applicable proposals.

## Remaining items, exact IDs and reasons

These are the only remaining ex-ungate items; all carry the exact recovery_frontier ignore and no obligation annotation.

- tests/contracts/test_lifecycle_cleanup_after.py::CleanupAfterContract.test_after_error_cleanup_write: Install observes the preserved existing bank through private_state::observe_bank; recovery_frontier is required before the real cleanup fault journey can run.
- tests/contracts/test_lifecycle_cleanup_before.py::CleanupBeforeContract.test_before_error_cleanup_write: Install observes the preserved existing bank through private_state::observe_bank; recovery_frontier is required before the real cleanup fault journey can run.
- tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_recovery_response_is_lost_after_real_rollback_is_durable: Uninstall observes the existing preserved bank through private_state::observe_bank; missing recovery_frontier leaves compatibility pending before the requested lifecycle boundary.
- tests/contracts/test_public_cli.py::PublicCLIContract.test_human_bootstrap_preview_repeat_update_and_uninstall_journey_under_temp_alias: Uninstall observes the existing preserved bank through private_state::observe_bank; missing recovery_frontier leaves compatibility pending before the requested lifecycle boundary.
- tests/contracts/test_runtime_diagnostics.py::RuntimeDiagnosticsContract.test_selected_projection_current_and_stale_do_not_create_audit_or_cache: Doctor bank inspection calls bank_writes::snapshot / bank_observation::observe, which requires recovery_frontier. The full current-and-stale projection journey is written against a real installed binary.
- tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_first_publication_excludes_feature_ancestry_and_preserves_original_files_index: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_required_human_review_and_checks_remain_pending_even_for_an_admin: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_first_bank_compares_real_absence_without_a_second_starter: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_unstaged_canonical_deletion_needs_real_prior_ownership_not_an_index_edit: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_policy.py::PublicationPolicyContract.test_rule_inventory_reads_the_next_page_before_deciding_automation_is_allowed: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_policy.py::PublicationPolicyContract.test_clean_rollup_cannot_substitute_for_missing_or_wrong_app_required_checks: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_allowed_user_still_waits_for_reviews_and_checks_then_lands: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_all_negative_identity_and_inventory_cases_refuse_prepare: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_complete_later_page_succeeds_but_incomplete_chains_refuse: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_actor_revoked_after_prepare_and_transport_rejection_stay_unpublished: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_immediate_final_recheck_refuses_actor_allowlist_or_restriction_change: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_pending.py::PublicationPendingContract.test_peer_bank_at_another_root_and_its_later_removal_do_not_authorize_a_second_bank: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_pending.py::PublicationPendingContract.test_discarded_side_bank_is_not_invented_as_historical_authority_ownership: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_pending.py::PublicationPendingContract.test_offline_permission_source_and_unsupported_policy_failures_keep_local_memory_pending: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_pending.py::PublicationPendingContract.test_removed_authority_bank_is_not_treated_as_first_initialization: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_pending.py::PublicationPendingContract.test_tampered_candidate_with_identical_memory_but_feature_code_is_refused: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_recovery.py::PublicationRecoveryContract.test_lost_response_inventory_does_not_duplicate_or_reverse_a_later_correction: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_recovery.py::PublicationRecoveryContract.test_competing_main_during_push_is_rejected_then_fresh_reconciliation_preserves_authority_code: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_worktree.py::PublicationWorktreeContract.test_linked_worktree_uses_a_private_authority_index_and_does_not_push_follow_tags: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_admission.py::PublicationAdmissionContract.test_admitted_unmerged_discovery_keeps_exact_conditions_evidence_origin_and_history: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_publication_hold_recovery.py::PublicationHoldRecoveryContract.test_missing_private_hold_state_reports_specific_recovery_and_keeps_pending_work: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_offline_capture_stays_readable_and_online_observation_preserves_feature_state: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_fetched_capacity_preserves_typed_cause_and_local_feature_state: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_lost_success_response_and_squash_readback_do_not_require_a_duplicate_push: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_reachable_side_parent_discarded_by_main_is_not_a_publication_witness: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_actual_merge_tree_retaining_delta_is_a_witness_even_after_later_correction: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_later_correction_keeps_historical_publication_receipt_without_claiming_current_truth: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_changed_default_remote_history_and_unrelated_candidate_code_require_review: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_cursor_loss_combines_independent_edits_and_repeated_normal_merge_without_reversing_memory: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_older_branch_edit_cannot_restore_tombstoned_learning_after_later_code_merge_or_cursor_loss: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_last_observed_memory_base_does_not_require_importing_authority_code_ancestry: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_both_changed_same_prose_requires_meaning_review_without_choosing_landing_order: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_normal_rebase_keeps_previously_imported_memory_once_and_local_feature_code: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_current_untracked_admitted_record_does_not_block_fresh_remote_inventory: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_inactive_stable_version_cannot_be_reactivated_by_an_older_working_copy: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_remote_advance_during_observation_requires_new_inventory: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_nested_bank_observation_reconciliation_and_replay_use_same_configured_prefix: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_parallel_worktree_observers_leave_remote_tracking_refs_unchanged: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/contracts/test_sharing_recovery.py::SharingRecoveryContract.test_fetch_failure_after_real_advertisement_has_specific_safe_repair: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.
- tests/t_support_world.rs::identity_fixture_creates_developer_and_verifies_or_clearly_refuses_unregistered_binding: Existing-bank identity bind calls bank_writes::snapshot, whose bank_observation::observe requires recovery_frontier; no fake provider or weaker identity fixture was introduced.

## Verification

Platform: aarch64-apple-darwin, Darwin 27.3.0 (ARM64_T6050), rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1. No Linux run claimed.

- cargo fmt: completed.
- cargo check --offline --all-targets --features dev-tools: passed with zero warnings (final log /tmp/ungate-final-check.log).
- Three consecutive final runs of cargo test --offline --features dev-tools --test c_test_cache_update_recovery --test c_test_doctor_fix --test c_test_download_diagnostics --test c_test_installed_layout --test c_test_installer --test c_test_lifecycle_cleanup_after --test c_test_lifecycle_cleanup_before --test c_test_lifecycle_lost_responses --test c_test_lifecycle_trust --test c_test_portable_artifact --test c_test_public_cli --test c_test_read_only_audits --test c_test_runtime_update --test c_test_skill_routes --test c_test_update_discovery_gaps --test t2_diagnostics --test t2_publication --test t2_sharing_recovery --test t_support_world --test c_test_incident_evidence --no-fail-fast: all 20 touched targets passed in every run. The remaining ignored tests include pre-existing non-lane deferrals; the lane’s 45 frontier deferrals are separately enumerated above. Logs /tmp/ungate-final-green-{1,2,3}.log. No whole-suite run.
- cargo test --locked --offline --features dev-tools --target aarch64-apple-darwin --test c_test_incident_evidence guide_examples_are_actual_native_exit_results_bound_to_complete_pack -- --exact: passed, 1 test, 7 filtered out. This reproduces the target-specific Cargo invocation shape requested for the CI hard-link failure. Log /tmp/ungate-explicit-target.log.
- Required input-safety grep on src/pack_validation.rs produced 91 pre-existing scanner/index/poisoning matches; result retained at /tmp/ungate-input-scan.txt. The scanner’s existing character-boundary invariants are documented at that module’s lines 81–88. The product edit adds no unwrap, expect, indexing, slicing, unsafe, unbounded read or child process.
- Earlier exploratory verification exposed the additional fixture causes listed above. One strengthened audit assertion initially lacked its json import; that compile failure was fixed before the final warning-free check and all three final green runs.

## File boundaries

Besides the 20 listed integration-test files, repository changes are limited to tests/support/distribution.rs, src/pack_validation.rs and skills/agent-memory/maintain/pack/GUIDE.md. No ledger modification resulted from the mandated marker. External retained lane artifact: ~/.local/state/agent-memory/research/rust-cutover/ungate-ported-ids.txt.
