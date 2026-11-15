# fanout-a obligation port report

Worktree: /opt/agent-memory-worktrees/rust-v015-lane-fanout-a
Integration snapshot: 5764442c (refreshed byte-exact from 5bd11582, including deletions).
Platform: aarch64-apple-darwin; Darwin 27.3.0 arm64; rustc 1.99.1 (58cf8f9c3 2026-10-13).

## Current result

Operator-directed wrap-up: this lane is NOT the complete originally assigned scope.
- 111 newly annotated obligations in 28 new baseline-specific test targets.
- Ledger helper initially marked 28 ported +83 partial; final correction marks **26 ported**, **85 partial** (full bodies gated on actual dependencies).
- **11 not applicable** identified below; these remain unported in the ledger for Main to mark.
- **34 remaining behavioural obligations** are explicitly unimplemented below.
- test_layout_recovery_released_identity.py was excluded as assigned to the layout lane.
- No baseline Python was executed; no Git writes; no new crates; no new mocked domain providers.

## Files

New tests:
- tests/c_test_cache_update_recovery.rs
- tests/c_test_doctor_fix.rs
- tests/c_test_download_diagnostics.rs
- tests/c_test_harness.rs
- tests/c_test_hook_failure.rs
- tests/c_test_incident_evidence.rs
- tests/c_test_installed_layout.rs
- tests/c_test_installer.rs
- tests/c_test_lifecycle_cleanup_after.rs
- tests/c_test_lifecycle_cleanup_before.rs
- tests/c_test_lifecycle_lost_responses.rs
- tests/c_test_lifecycle_partitions.rs
- tests/c_test_lifecycle_security_contract.rs
- tests/c_test_lifecycle_trust.rs
- tests/c_test_offline_skill.rs
- tests/c_test_portable_artifact.rs
- tests/c_test_public_cli.rs
- tests/c_test_purge_unlock_after.rs
- tests/c_test_purge_unlock_before.rs
- tests/c_test_purge_write_after.rs
- tests/c_test_purge_write_before.rs
- tests/c_test_release_canaries.rs
- tests/c_test_runtime_purge.rs
- tests/c_test_runtime_update.rs
- tests/c_test_skill_routes.rs
- tests/c_test_update_discovery_gaps.rs
- tests/c_test_update_mutable_public.rs
- tests/c_test_update_parallel_inventory.rs

New shared test support:
- tests/support/lifecycle.rs: real copied pack + CARGO_BIN_EXE_am development source, CLI, bank render and exact filesystem snapshots.
- tests/support/distribution.rs: real throwaway ed25519 keys, file:// signed manifests/archives/binary, real registered providers; isolated child test entry drives typed lifecycle APIs. SourcePack is a test PackRoot registration, not a domain-provider mock.
- tests/support/canaries.rs: real identity/bank/security workflow fixture.
- tests/support/purge_replay.rs: actual purge trace enumeration and interruption/recovery/confirmation retries.
- tests/support/cleanup_replay.rs: actual error-path stage-cleanup interruption and recovery/retry.
- tests/support/fault_partitions.rs: baseline GROUPS/select/extract/coverage validation over actual observed and executed replay inventories.
- tests/support/mod.rs: appended support modules (formatter sorted only those appended declarations).

Approved instrumentation, without changing existing replay behaviour or assertions:
- tests/t2_installation.rs:2241-2246 emit real install phase inventory, executed points and receipt artifact digest; module declaration at 3708-3709.
- tests/t2_lifecycle.rs:1365-1370 emit update proof; 1934-1935 capture pre-uninstall runtime digest; 2004-2014 emit uninstall proof; module declaration at 2595-2596.

Approved product-adjacent native dev tooling:
- src/bin/am_dev/guide_evidence.rs: am-dev guide-evidence snapshot/check, bounded real guide inventory, fence-aware normative rule collection, routes/reachability, source-bound before evidence, explicit wording maps, force/polarity checks, fixed refusal envelope.
- src/bin/am_dev/capture_examples.rs: am-dev capture-guide-examples [--am PATH], actual native CLI exits 0/1/2 and complete portable inventory binding in isolated credential-free roots.
- src/bin/am_dev/incidents.rs: am-dev run-incident INCIDENT, catalog-to-exact-annotated-Rust-test lookup and actual prebuilt native test execution; missing/ambiguous executable and skipped tests cannot pass. Requires incident test targets compiled in the same profile with exactly one executable per target.
- src/bin/am_dev/main.rs: register those three dev-only subcommands. Nothing added to shipped am.
- tests/obligations/python-baseline-v1.json: helper's 111 initial markings, then two offline obligations corrected from ported to partial after their real missing-provider refusal was observed.

## Dependency gates

Actual-pack installer/distribution tests remain fully written and ignored pending the complete pack command surface. At snapshot5764442c only refresh/boundary are still undeclared; the gate text intentionally retains Main's requested full surface list (context, transaction, refresh, boundary, semantic). A complete-pack validation refusal was observed and correctly identified by Main as an in-flight integration dependency, not a product defect. Remove those ignores only once Admission's routes land.

Bank-writing/purge/provider-dependent workflows are fully written and ignored with recovery_frontier registration reasons. The retained incident runner additionally requires its existing governed incident tests to be enabled; current governance incident is ignored. Actual lifecycle partition controls are active and rerun existing real install/update/uninstall interruption tests in child processes, then mutate the resulting proofs to test omissions, duplicates, foreign artifacts and wrong counts. No fabricated positive proof is used.

## Declared deltas relied on

- Managed Git remote/ref channels are retired: former managed-install selection obligation asserts the named Rust refusal and exact no-write preservation. Signed runtime lifecycle uses genuine signed release-v2 data and separately signed real native binaries.
- Native command links replace interpreter wrappers; client hooks call am hook <name>; no candidate Python dispatch/import occurs.
- Sequential sorted update inventory replaces the four-thread IO pool. The inventory test verifies all eight actual repositories remain observed and a real invalid bank-root config propagates failure. No concurrency-performance assertion is retained.
- Python bytecode cache recovery retired; opaque preservation verified (five CacheUpdateRecovery tests are fully written but surface-gated, so currently partial, not falsely ported). Arbitrary bytes are placed in .pyc-named files with no accompanying Python source; fresh Rust runtime refuses qualification/activation and preserves bytes, including retry/change cases.
- Outside uninstalled target/debug binary diagnostics refuse an unavailable executing package (diagnostics.rs:294-312), while retaining negative publisher/verifier/ACL qualifications; no bootstrap/package trust is invented.
- Fixed native typed failures replace Python exception text: hook tests use actual missing binding, corrupt compatibility record and mode000 Git config. These remain recovery_frontier-gated, so their expected fixed outputs are not yet verified.
- Guide example unknown-operation stderr is native parser wording, not argparse's interpreter-specific layout; actual exit2 and named operation are checked.

## Defects and limitations

- No confirmed product-code defect discovered. Missing full pack routes are declared in-flight dependencies.
- New source panic scan finds only two expect calls for constant valid Regex values, each documented with an invariant; no input-derived unwrap/expect/slices in the three new tool modules.
- The two download retention obligations use real RLIMIT_FSIZE0 failure, rather than Python monkeypatching OSError/DistributionError; primary receive failure and no diagnostic path are verified.
- Native incident runner has a deliberately strict prebuilt-executable prerequisite; zero/duplicate target executables refuse rather than selecting stale artifacts. Partition controls have the same prerequisite; final verification uses a separate clean Cargo target directory.
- Partition controls invoke the existing t2_installation/t2_lifecycle replay targets unchanged. Those pre-existing targets retain their own earlier seam fixtures; this lane adds only real trace recording and coverage validation, as expressly approved by Main.
- Formatting used rustfmt stdout + exact changed-line edit patches. An oversized stdout result was truncated by eval sh while preparing the existing t2_lifecycle patch; immediately restored that file from approved snapshot and re-applied only the approved instrumentation. Final fmt/check below establish the resulting tree compiles.

## Verification

- cargo fmt --check: PASS after final formatting.
- cargo check --offline --all-targets --features dev-tools: PASS, zero warnings.
- Prior active controls: guide evidence/capture 7 passed and1 ignored; World harness7 passed; diagnostic controls5 passed1 ignored; full real lifecycle partition coverage passed all3 obligation controls (10 actual replay groups), plus control child entry.
- One early outside-helper test wrongly expected an uninstalled debug binary to identify a package; corrected to the documented native package-location refusal.
- Final own-target verification: PASS. The 27 non-partition targets passed three consecutive post-correction runs with 85 dependency-gated obligation tests ignored. The partition target passed its third consecutive run (all four tests green, 463.53s); its three runs each replayed all ten real install/update/uninstall groups. Command environment: `CARGO_TARGET_DIR=$PWD/target/fanout-a-verification`; compiled `t2_installation` and `t2_lifecycle` with `cargo test --offline --features dev-tools --test t2_installation --test t2_lifecycle --no-run`, ran each `c_test_*` target with `cargo test --offline --features dev-tools --no-fail-fast --test <target>` (27 targets batched three times), then `cargo test --offline --features dev-tools --test c_test_lifecycle_partitions`.
- Whole suite: NOT RUN for handoff, explicitly cancelled/skipped at Main's direction; Main gates the complete crate on both platforms at merge.
- Linux/musl not run.

## Not applicable (Main marks ledger)

- `tests/contracts/test_gate_diagnostics.py::GateDiagnosticsContract.test_actual_gate_retains_suite_stage_exception_chain_and_partial_timeout_output` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_gate_diagnostics.py::GateDiagnosticsContract.test_actual_timeout_and_failed_completion_record_preserve_both_worker_streams` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_gate_diagnostics.py::GateDiagnosticsContract.test_failed_suite_validation_does_not_report_successful_world_cleanup_as_the_error` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_gate_diagnostics.py::GateDiagnosticsContract.test_merge_rejects_invalid_rows_and_accepts_complete_reordered_shards` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_gate_diagnostics.py::GateDiagnosticsContract.test_optional_row_requires_exact_reason_and_integer_zero` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_harness.py::HarnessContract.test_actual_gate_rejects_empty_skipped_and_failing_suites` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_harness.py::HarnessContract.test_registered_batches_cannot_omit_duplicate_or_invent_executed_cases` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_harness.py::HarnessContract.test_worker_rejects_a_live_home_before_importing_suite` — Python test-gate/contract-suite harness retired; cargo test plus the obligations ledger is the v0.0.15 gate.
- `tests/contracts/test_installer.py::InstallerContract.test_wrapper_chooses_supported_python_after_older_path_entry` — Python interpreter/import/version-probe contract has no native equivalent.
- `tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_core_python_imports_require_only_the_standard_library_and_bundled_package` — Python interpreter/import/version-probe contract has no native equivalent.
- `tests/contracts/test_public_cli.py::PublicCLIContract.test_unsupported_runtime_python_refuses_before_loading_operations` — Python interpreter/import/version-probe contract has no native equivalent.

## Remaining behavioural obligations (not implemented)

- `tests/contracts/test_installed_bank_initialization.py::InstalledBankRoundtrip.test_cold_identity_initialization_and_checkpoint_have_no_bank_dependency`
- `tests/contracts/test_installed_bank_qualified_recall.py::InstalledBankRoundtrip.test_qualified_recall_aliases_preserve_ids_conditions_validity_and_provenance`
- `tests/contracts/test_installer.py::MacOSCommandContract.test_actual_shipped_old_updater_then_install_migrates_without_stranding_ownership`
- `tests/contracts/test_installer.py::MacOSCommandContract.test_current_and_shipped_recovery_read_each_others_wrapper_journals`
- `tests/contracts/test_installer.py::MacOSCommandContract.test_shipped_uninstaller_can_remove_new_wrappers_with_aliased_xdg_and_target_paths`
- `tests/contracts/test_installer.py::OneLineBootstrapContract.test_pipe_preview_repeat_update_and_uninstall_preserve_memory_and_user_instructions`
- `tests/contracts/test_layout_recovery_runtime_rollback.py::HeldRuntimeRollbackContract.test_interrupted_capable_update_rolls_back_exact_runtime_and_keeps_hold`
- `tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_actual_released_v002_updater_accepts_changed_runtime_under_umask002`
- `tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_parent_substitution_before_activation_preserves_targets_and_recovers`
- `tests/contracts/test_lifecycle_trust.py::LifecycleTrustContract.test_private_temp_barrier_native_alias_and_group_write_payloads_work`
- `tests/contracts/test_migration_runtime_rollback.py::MigrationRuntimeRollbackContract.test_partial_runtime_update_requires_explicit_retained_backup_restoration_before_rollback`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_observer_verification_cannot_enroll_or_promote_legacy_marker`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_prior_committed_activation_recovers_after_a_later_install_rolls_back`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_real_binding_proof_rejects_forgery_and_mid_instance_drift_before_source`
- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_receipt_commit_separates_bank_activation_from_rollback`
- `tests/contracts/test_portable_handoff.py::PortableHandoff.test_preparation_recovery_retains_original_source_pins_and_can_rollback`
- `tests/contracts/test_portable_handoff.py::PortableHandoff.test_unknown_conditions_malformed_framing_and_stale_evidence_stay_qualified`
- `tests/contracts/test_portable_lifecycle.py::PortableLifecycleContract.test_copy_all_clients_keep_qualified_memory_through_update_optional_state_loss_uninstall_and_reinstall`
- `tests/contracts/test_portable_lifecycle.py::PortableLifecycleContract.test_link_custom_only_updates_without_optional_clients_identity_or_a_bank`
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_existing_barrier_deleted_before_open_is_not_recreated`
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_native_barrier_appearing_during_layout_observation_refuses_without_erasing_it`
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_released_v014_sources_preview_then_upgrade_without_inventing_native_authority`
- `tests/contracts/test_public_cli.py::PublicCLIContract.test_actual_legacy_wrapper_gains_flags_and_human_output_without_wrapper_replacement`
- `tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_fast_forward_outside_the_pack_needs_no_client_restart`
- `tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_trusted_source_helper_updates_when_the_installed_command_does_not_yet_dispatch_update`
- `tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_actual_v002_index_refresh_and_deleted_worktree_upgrade_through_signed_one_liner`
- `tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_actual_v002_process_then_same_version_signed_adoption_under_022_and_002`
- `tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_missing_signature_verifier_refuses_before_lifecycle_state_is_created`
- `tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_one_line_install_update_failures_recovery_and_preservation`
- `tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_one_command_first_publish_exact_resume_and_harmless_descendant`
- `tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation`
- `tests/contracts/test_unified_cli_operations.py::UnifiedCLIOperations.test_installed_compaction_preserves_distinct_provenance_history_and_converges`
- `tests/contracts/test_unified_cli_operations.py::UnifiedCLIOperations.test_installed_context_requires_an_actual_current_text_bound_token_count`
- `tests/contracts/test_update_mutable_observations.py::FreshObservationControls.test_mutations_between_router_observations_refuse`

These are retained unported, not narrowed, mocked or marked implementation-only. They require a continuation owner; the remaining installed/signed/portable journeys and observation races were not started when operator requested wrap-up.

## Accountability

awa status --json reported managed=true (164 managed issues). Assignment lacks an unambiguous story work key; Main was notified and tracker phases were left unchanged under coordinator ownership. No tracker reconciliation receipt is claimed. Vox reports omp-work Ready, room kh57aq2zwdss.
