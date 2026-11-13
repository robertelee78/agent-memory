# T1 A2 port report

## Delivery and verification status

All eleven assigned source modules are present and registered in alphabetical order in src/lib.rs. Their baseline public behavior is implemented within the explicit later-tier boundaries below. tests/t1_a2.rs now contains 68 test functions, including 62 distinct baseline obligation annotations; all test bodies are in the worktree, with no external include paths. The former local:// fragments were disposable session inputs under ~/.omp/agent/sessions/.../local/, not runtime dependencies.

No Cargo command, Python execution, build, test, linter, formatter or Git write was run by this worker. Main reported one compilation error (sha2 output does not implement LowerHex) and one unused-mut warning; pack_validation now calls the existing validation_memo::hex helper, developer_identity already uses that helper, and the reported unused mut was removed. These fixes and the integrated tests have not been compiled by this worker. Main explicitly owns the final cargo fmt, build and test pass. No passing-test or exact-parity verification claim is made here.

## Modules and public API

- **pack_validation**: Invalid/issue, require, semantic_version, frontmatter, Inventory/inventory, markdown, reference_definitions, LinkOccurrence/link_occurrences, links, anchors, destination, mandatory_targets, git_operation, command_argv, validate, validate_with_surface, with_validated, with_validated_surface, command, and CommandSurface registration. Inventory preserves 4,096-entry, 2 MiB/file and 16 MiB/pack limits, permission checks and complete re-inventory around the caller's inspection. Static memo results are caller-owned clones and bound to file bytes/modes plus command-provider generation.
- **artifact**: COMPATIBILITY, require, declaration, strings, command_names, inspect, inspect_with, command, and MigrationContract registration. Declaration reads remain 65,536-byte bounded and digest-bound; artifact digests use sorted compact ASCII JSON of the file manifest.
- **developer_identity**: SPEC, uuid_valid, digest, encoded, load/load_with_limit, text, alias/aliases, labels, declared, empty, validate, proposal_value, paths, observe, result, status, preview, apply, recover, command_result, and injectable *_with_sources forms. Explicit attribution remains separate from aliases, harnesses, sessions and bank state; replay does not reselect an older identity.
- **repository_config**: constants and root/branch/router validation; safe_label and SecurityScan registration; local_value/local_settings, router_bytes/object_router_bytes, git_roots, physical_others, settings/settings_with, recovery_settings, bank_path, relative, observe_settings. Settings is a typed bank_root/authority_branch value. Git inventories retain native filename bytes; ordinary/raw enrollment observations share an entry inventory and independently recheck on successful exit. Physical context names are read from the committed router schema.
- **local_registry**: paths, private_mkdir, now, exclusive, replace/replace_with_sources, recover, check_canonical_writer. CanonicalWriterError distinguishes Busy, Initializing and Contract refusals; only absence returns success. Lock publication follows first EX flock; cleanup only removes matching owned bytes/inode, and explicit finish paths surface cleanup errors with finally precedence.
- **scopes**: MODEL, condition_value, parse_scope, conditions, decide, model, command_model, command_decide. Scope is applicability, never an ACL or identity namespace.
- **sharing_limits**: all five baseline constants, bank_size, private_bytes, read_bytes, read_json, preflight_private_output, write_private. Typed output operations take the actual executing pack path explicitly so CLI wiring can reproduce Python __file__ location checks for copied/installed runtimes. Input symlinks are resolved before pinned reads; output trust checks still refuse symlinks/unsafe locations.
- **authority**: MODEL, text, comparable, assess, model, command_model, command_assess. Typed sorted JSON comparison remains distinct from semantic truth, source verification and authority approval.
- **bank_capacity**: REPAIR, CapacityCode, Measurement, BankCapacityError/new/details, free details, check_files. Error carries code, path, limit, observed, measurement and unit. The exact nine details fields and baseline repair text are retained; Kind::BankCapacity preserves the typed observation across ordinary ContractError boundaries.
- **handoff**: constants, reference_path, read_inside, observe, validate_observation, check_pins, managed_cues, sections/sections_with, json_section, reusable/reusable_with, assess/assess_with, and Checkpoint registration. Native filename/pathspec/blob comparison bytes remain native; checkout observations are rechecked on exit.
- **records**: MODEL/EVENT/TOMBSTONE, text, fields, identity, reference, captured_source_path, source_capture_id, references, evidence, record, event, validate/validate_with_parser, parse, applicability, definition, command, and TombstoneParser registration. The short source file is dense, not missing baseline functions; Main will format it with the rest of the crate. Record/history/tombstone dispatch, policy fields, unmerged applicability, exact-time/link invariants and fixed CLI error redaction are present.

## Later-tier traits and missing-registration behavior

1. pack_validation::CommandSurface::dispatch(script, files) returns Dispatch { module: String, commands: BTreeSet<String>, flags: BTreeSet<String>, routes: BTreeMap<String, CommandRoute> }; each CommandRoute contains helper: String and prefix: Vec<String>. This is the data shape previously obtained from dispatch(..., details=True): module, command names, accepted flags, and MEMORY_ROUTES helper/prefix pairs. Shell syntax, launcher prefixes, direct/delegated operation selection, help-only forms and used-option checks remain in T1. The later signed command_surface tier authenticates and supplies only this data; it must supply --help/-h flags and the literal route/helper/prefix bindings. No provider is installed by T1. A command-bearing validation without one refuses with issue code command-surface-unregistered, not valid=true.
2. artifact::MigrationContract::inspect(root, compatibility, files) -> Result<Value>; missing registration refuses artifact inspection. The migration slice supplies the real catalog inspection.
3. repository_config::SecurityScan::has_reasons(label) -> Result<bool>; missing registration refuses safety-sensitive root/branch validation. The security slice supplies the actual scan, not a permissive fallback.
4. handoff::Checkpoint supplies headings (UTF-8 byte offsets), managed field labels, extra labels and list_text. Missing registration is a distinct refusal before assess/sections/reusable can silently treat managed content as manual orientation.
5. records::TombstoneParser::parse(content) -> Result<Value>; missing registration refuses tombstone parsing. Full successful tombstone tests are deferred, not satisfied with a fake parser.

## Exact pack exclusion

Not ported: the entire Python dispatch(script, files, details=False, seen=()) function; Python AST parsing/walking/literal_eval; literal packaged-main import discovery; static argparse parser/subparser/argument/choice extraction; literal variable and loop expansion; MEMORY_ROUTES extraction; and AST-recursive helper-delegation discovery/bounds. Its nested strings and visit helpers are consequently absent. There is no Python parser and no candidate import/execution. git_operation and command_argv are retained, not omitted.

Direct dependent baseline functions are pack_validation._validated (private context manager), public pack_validation.validate and main, artifact.inspect and main transitively, and updating.compatible_surface (direct import/calls outside this batch). validate/inspect now use the registered signed-data boundary; updating must migrate its direct dispatch dependency to that same provider in its later slice. The private context-manager shape is expressed as with_validated, bracketing the complete caller closure. No compatibility shim for AST dispatch was added.

## CLI argparse declarations for the CLI tier

- pack_validation: --pack, default current pack root.
- artifact: positional operation choice inspect; --pack default PACK.
- developer_identity: required subcommands status, preview, apply, recover. status: repeatable --observe (default []), optional --harness/--session, --repo default '.'. preview: one required mutually exclusive --create (store_true), --adopt ID, --select ID; repeatable --label/--alias default []. apply: required --proposal. recover: optional --proposal.
- scopes: required model or decide; decide requires --input.
- authority: required model or assess; assess requires --input.
- records: required model, render, validate or applicability; render requires --input; validate requires --file; applicability requires --file and --input; render/validate/applicability accept repeatable --require default [] constrained to schema x-policy-fields.
- repository_config, local_registry, sharing_limits, bank_capacity, handoff: no argparse main in the baseline.

Typed command functions return the exact result value, exit code and stdout/stderr representation rather than parsing argv. File reads for typed records commands are supplied by CLI wiring as Result inputs so errors still pass through fixed redaction.

## T0 prerequisites added with Main approval

- host.rs: OS UUIDv4, pure UTC formatter and OS UTC clock, bounded gethostname, PID state (Alive/Gone/Denied), effective UID/PID, passwd-backed expanduser with explicit HOME injection, and one shared typed native-text refusal. Sources holds UUID/UTC fn pointers with OS defaults. getpwuid_r/getpwnam_r retry ERANGE within a fixed 1 MiB buffer cap. PID Gone follows ProcessLookupError recovery; Denied follows PermissionError refusal; Alive refuses live owners; other errno propagates as a refusal.
- unicode.rs: full baseline Unicode 15.1 casefold, alphanumeric ranges for Python regex word characters, HTML5 entities/numeric rules, plus NFC+casefold via the Main-added unicode-normalization dependency. Const tables are generated exactly from Main's committed vectors, and tests assert table/vector equality and HTML samples.
- fs::canonical_path: shared strict/non-strict name resolution, reused by remote_identity (its previous private helper was removed). Resolution grants no trust; callers still pin/check paths.
- RenameMode::Replace: pinned-parent renameat overwrite; baseline callers synchronize directories.
- FlockGuard::descriptor_mut and try_acquire_owned: preserve lock ownership and the live creator descriptor on failed first flock; existing try_acquire is a thin wrapper.
- Descriptor::read_prefix and write_once: real bounded prefix reads without oversized-file rejection, and one successful write retried only on EINTR.
- error::Kind: BankCapacity, LocalRegistryBound and NonUtf8 typed variants.

Tests are in tests/t0_host.rs, tests/t0_unicode.rs and appended tests/t0_fs.rs. They cover fixed UTC instants, OS UUID/PID/host, injected HOME/user lookup, canonical symlink/.. behavior, strict missing/non-strict tail/loop refusal, atomic overwrite reader visibility, returned failed-flock descriptor identity and locked I/O, and prefix regular/FIFO/directory behavior. A deterministic real short-write case was not produced; write_once's zero-length/ordinary-write behavior is covered without faking a partial OS write.

## Behavior not reproduced exactly / declared deltas

- Python's accidental unbounded developer registry reads are bounded at REGISTRY_MAX_BYTES=64 MiB; developer/local recovery lock-owner reads are bounded at LOCK_MAX_BYTES=1 MiB. local_registry replacement/owned-marker observations also use bounded readers. Oversized developer registry data has the typed fixed refusal Local registry exceeds its reader bound; preserve it and inspect manually. Oversized lock records take their existing unsafe/invalid-lock refusal path. Tests retain and compare original oversized bytes.
- Python json.loads(bytes) can infer UTF-16/32 and accept a UTF-8 BOM; the approved Rust codec accepts UTF-8 only, refuses BOMs/lone surrogates and bounds nesting at 256. Affected loaders are developer_identity.load/load_with_limit (registry/proposal/lock consumers), local_registry recovery, sharing_limits.read_json, handoff.json_section, and artifact/schema declarations. Explicit named limits are passed by callers (including sharing MAX_PRIVATE=512 MiB); no usize::MAX or input-length bypass is used.
- Baseline Python os.fsdecode can produce surrogate-escaped JSON/frontmatter strings from non-UTF-8 OS bytes. Native filename/pathspec/blob/comparison values remain OsString/bytes, never lossy. Values which must be recorded as text instead refuse through host::text/Kind::NonUtf8 with Non-UTF-8 <what> cannot be recorded; rename it or configure a UTF-8 value. Call sites: developer identity registry/proposal path bindings, Git attribution config output, USER; repository local config values and discovered bank-root strings; handoff checkout HEAD, checkpoint branch, blob ID and blob size; sharing private output directory/receipt paths; hostname. Python would have attempted surrogate-escaped text at these points.
- NFC comes from unicode-normalization's Unicode 18 tables rather than the baseline interpreter's Unicode 15.1. Normalization of characters already assigned in 15.1 is stable; newly assigned scalars may differ. Casefold and regex-alphanumeric/HTML entity tables are pinned exactly to the baseline vectors.
- Python/parser-version-dependent malformed URL/encoding behavior is not promised beyond the explicit supported URI forms; malformed authorities are refused/classified unavailable instead of borrowing interpreter quirks.
- handoff parser/cue offsets are UTF-8 byte offsets for Rust slicing, whereas Python indexes Unicode code points. This is explicit in the Checkpoint trait and managed_cues documentation; serialized evidence values do not carry these offsets.
- T0's pinned reads/rechecks refuse source or namespace mutation which some path-based baseline reads could race through. Files are preserved; a changed source is not downgraded to a capacity failure or apparent absence.
- sharing_limits preflight currently uses the shared ContractError with the exact baseline PrivateOutputError message rather than exporting a separate PrivateOutputError subclass analogue. CanonicalWriterError and BankCapacityError do retain their distinct typed refusal classes.
- Argparse and copied Python helper execution are intentionally not ported. Typed API calls replace __file__ pack discovery with an explicit pack location where required; the CLI tier owns actual runtime-location wiring.

## Obligations fully ported (source coverage; unexecuted)

- tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_private_output_preflight_preserves_identical_retry

This port covers the complete helper sequence with real private output bytes, exact durable JSON, preflight, identical retry and file mode. It does not claim the surrounding later refresh workflow is implemented.

## Obligations partially ported

Partial scope: pack tests cover real inventories/permissions/symlinks, Markdown/shell/loading/refusal mechanics and caller bracketing, not signed-provider or copied-CLI execution. Developer tests cover core state, actual Git config, isolated processes, concurrency, replay and recovery but not every original copied-helper/environment assertion. Scope/authority tests cover core decisions/matrix/conflict handling, not their full CLI workflows. Capacity tests cover complete-map measurements, not later bank readers. Local-registry/repository tests cover storage/discovery mechanics, not later project/enrollment workflows. Handoff tests cover classification, pins and cues, not successful later checkpoint parsing/publication/retrieval. Records tests cover core validators/rendering/redaction, not copied-pack execution or successful tombstone dependency. Unsafe private-output tests cover refusal/preservation, not later public sharing command failure receipts.

- tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_every_adr_authority_domain_preserves_the_full_matrix_and_no_memory_proof
- tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_peer_conflict_routes_to_detecting_developer_and_known_other_author_without_last_landing_wins
- tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_complete_map_count_is_exact_and_partial_inventory_is_at_least
- tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_complete_utf8_source_boundary_and_file_only_validator
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_changed_git_hosting_client_and_user_aliases_never_change_attribution
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_collaborator_selection_is_explicit_and_preserves_metadata
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_copied_pack_and_config_paths_with_spaces_work_without_installation
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_failure_contributes_neither_alias_and_recovers_freshly
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_no_match_preserves_caller_observations
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_preserves_duplicates_includes_and_mixed_case_precedence
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_rejects_genuine_embedded_and_trailing_lf_values
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_valueless_and_empty_values_override_earlier_aliases
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_first_setup_has_no_registry_bank_install_or_credentials_prerequisite
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_live_and_foreign_locks_refuse_and_dead_local_owner_can_recover
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_recovery_cannot_remove_a_lock_held_by_another_recovery
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_registry_and_proposal_refuse_preferences_harness_aliases_and_wrong_roots
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_replay_is_idempotent_without_reselecting_an_old_identity
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_same_id_is_explicitly_adopted_across_five_machines
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_stale_preview_and_external_edits_stop_without_overwrite
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_true_concurrent_initializers_preserve_the_winner
- tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_unrelated_or_symlinked_registry_is_preserved
- tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_public_sharing_commands_reject_unsafe_output_before_work
- tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_empty_held_and_unheld_slots_never_mint_scope_or_claim_known_writer
- tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_local_owner_is_interruption_not_live_pid_authority
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_change_after_declaration_read_refuses_cold_and_cached_inspections
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_missing_or_false_runtime_declarations_are_refused_without_modifying_the_copy
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_repeated_inspection_rechecks_changed_source_modes_and_topology
- tests/contracts/test_portable_handoff.py::PortableHandoff.test_mixed_valid_and_malformed_reserved_sections_cannot_hide_duplicate_conditions
- tests/contracts/test_portable_handoff.py::PortableHandoff.test_published_handoff_filters_other_task_and_branch_but_remains_explicitly_inspectable
- tests/contracts/test_portable_handoff.py::PortableHandoff.test_source_pins_refuse_preapply_and_locked_changes_and_outside_reference_never_opens
- tests/contracts/test_portable_handoff.py::PortableHandoff.test_wrong_heading_levels_cannot_turn_managed_checkpoint_into_manual_orientation
- tests/contracts/test_project_identity.py::ProjectIdentityContract.test_changed_registry_bank_remote_or_authority_refuses_stale_proposals
- tests/contracts/test_project_identity.py::ProjectIdentityContract.test_project_registry_stale_lock_recovery_is_bounded_and_read_back
- tests/contracts/test_records.py::RecordContract.test_active_policy_and_stale_versions_remain_explicit_without_history_replay
- tests/contracts/test_records.py::RecordContract.test_all_seven_kinds_roundtrip_with_origin_conditions_evidence_and_exact_times
- tests/contracts/test_records.py::RecordContract.test_arbitrarily_large_integer_confidence_returns_structured_rejection
- tests/contracts/test_records.py::RecordContract.test_copied_artifact_and_five_field_tombstone_exception_require_no_harness_or_state
- tests/contracts/test_records.py::RecordContract.test_malformed_identity_source_time_links_and_metadata_are_rejected_without_echo
- tests/contracts/test_records.py::RecordContract.test_memory_landing_does_not_widen_unmerged_or_unknown_conditions
- tests/contracts/test_records.py::RecordContract.test_optional_event_loss_preserves_readable_current_record_and_stale_state
- tests/contracts/test_records.py::RecordContract.test_unicode_metadata_separators_roundtrip_without_losing_values_or_body_text
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_independent_git_roots_resamples_head_and_validates_its_own_observation
- tests/contracts/test_scope_policy.py::ScopePolicyContract.test_all_conditions_must_match_and_missing_context_never_becomes_universal
- tests/contracts/test_scope_policy.py::ScopePolicyContract.test_default_is_one_repository_bank_and_developer_does_not_select_a_namespace
- tests/contracts/test_scope_policy.py::ScopePolicyContract.test_unknown_personal_federation_and_noncanonical_conditions_are_invalid
- tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_damaged_untracked_context_bank_is_unknown_not_documentation_for_a_replacement
- tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_ignored_existing_router_and_unknown_schema_cannot_become_source_evidence_for_second_bank
- tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_symlink_spelling_alias_and_two_bank_roots_are_refused_without_adoption
- tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_unsafe_duplicate_or_ambiguous_configuration_preserves_all_paths
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_candidate_code_is_never_imported_or_executed_by_static_validation
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_coordinated_objects_and_later_conditions_preserve_the_initial_required_load
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_empty_literal_argument_is_data_not_a_hidden_shell_operator
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_fenced_project_examples_are_data_and_real_reference_and_duplicate_heading_anchors_work
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_internal_symlink_is_refused_without_reading_its_target
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_literal_marker_words_are_prose_and_cannot_index_internal_link_state
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_malformed_routes_links_frontmatter_and_leaf_contracts_are_refused
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_mandatory_loading_cycles_fail_but_conditional_navigation_remains_valid
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_modifiers_and_purpose_words_cannot_hide_required_load_cycles
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_purpose_clauses_cannot_hide_an_unconditional_loading_cycle
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_supported_reference_and_html_links_cannot_hide_a_required_load_cycle
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_unreadable_directory_cannot_hide_a_second_registered_skill

## Deferred primary-module obligations

These IDs have no complete T1 port yet; no provider stub or fake later-tier workflow was used to claim them.

- tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_conflicting_code_intent_and_direct_project_decision_keep_their_own_authorities — CLI and integration tiers
- tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_invalid_identity_fact_references_and_scope_refuse_and_copied_artifact_is_read_only — CLI and integration tiers
- tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_unqualified_inapplicable_or_stale_sources_cannot_promote_memory_to_proof — CLI and integration tiers
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_complete_copy_outside_git_has_the_same_identity_and_explicit_optional_prerequisites — migration, signed command_surface, CLI and bank-operation tiers
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_copied_helpers_initialize_checkpoint_and_recall_while_preserving_unfinished_code — migration, signed command_surface, CLI and bank-operation tiers
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_core_python_imports_require_only_the_standard_library_and_bundled_package — migration, signed command_surface, CLI and bank-operation tiers
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_every_implemented_documented_helper_imports_and_dispatches_from_the_copy — migration, signed command_surface, CLI and bank-operation tiers
- tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_inventory_digest_changes_for_actual_bytes_or_modes_without_claiming_a_signature — migration, signed command_surface, CLI and bank-operation tiers
- tests/contracts/test_portable_handoff.py::PortableHandoff.test_preparation_recovery_retains_original_source_pins_and_can_rollback — checkpoint, transaction and retrieval tiers
- tests/contracts/test_portable_handoff.py::PortableHandoff.test_unknown_conditions_malformed_framing_and_stale_evidence_stay_qualified — checkpoint, transaction and retrieval tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_branch_validation_remains_after_competing_router_refusal — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_changed_head_with_deleted_competing_router_refuses_enrollment_exit — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_coalesced_observation_ignores_ambient_authority_and_fsmonitor — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_config_and_remote_mutation_between_enrollment_entry_exit_are_fresh — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_corrupt_missing_and_dangling_head_never_establish_empty_repository — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_earlier_real_config_refusal_beats_every_later_child_failure — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_grouped_discovery_keeps_effective_includes_rewrites_and_remote_conflicts — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_index_untracked_and_raw_locators_added_after_scope_read_refuse_exit — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_interrupt_in_real_config_validation_preserves_traceback_and_cancels_siblings — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_invalid_config_refuses_before_speculative_real_git_inventory_acceptance — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_config_child_output_never_accepts_speculative_names — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_name_children_refuse_after_valid_real_config — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_successful_observation_never_resolves_existing_identity — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_non_git_directory_remains_readable_without_inventing_identity — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_parent_traversal_name_never_adopts_outside_router — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_config_validation_time_is_charged_to_unresolved_inventory_child — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_root_spellings_and_linked_worktrees_preserve_public_identity — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_verified_hold_remains_readonly_through_grouped_settings_and_enrollment — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_remote_query_cleanup_failure_cannot_become_successful_discovery — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_sampled_head_is_immutable_and_next_settings_read_observes_later_commit — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_sha256_head_retains_committed_locator_after_staged_deletion — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_signaled_or_incomplete_failed_observation_never_adopts_partial_root — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_subdirectory_still_refuses_repository_root_authority — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_timeout_and_launch_failure_never_mint_identity_from_partial_output — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_tracked_body_and_enrollment_boundaries_have_no_live_query_children — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unavailable_commit_or_locator_blob_never_becomes_absent_memory — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unborn_root_preserves_local_settings_and_indexed_bank_identity — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unreadable_or_malformed_metadata_preserves_refusal_and_original_bank — project identity, scope-binding/private enrollment and CLI tiers
- tests/contracts/test_scope_policy.py::ScopePolicyContract.test_router_and_copied_portable_helper_share_the_same_scope_contract — CLI and integration tiers
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_complete_copied_pack_validates_without_changing_pack_or_caller_state — signed command_surface and CLI tier
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_every_documented_helper_dispatch_also_accepts_real_public_help — signed command_surface and CLI tier
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_installed_delegation_checks_real_child_operations_and_flags_without_execution — signed command_surface and CLI tier
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_missing_delegated_helper_cannot_be_hidden_behind_the_installed_command — signed command_surface and CLI tier
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_reference_forms_work_for_owner_bindings_and_root_routes_with_global_definitions — signed command_surface and CLI tier
- tests/contracts/test_skill_routes.py::SkillRouteContract.test_working_delegated_help_commands_are_valid_documented_commands — signed command_surface and CLI tier

Additional annotations for bank capacity, project storage, one-bank boundaries and private outputs are listed above as partial rather than claiming their surrounding integration suites. The obligation manifest was read for IDs only and not edited.

## Handoff / remaining integration

Main should run the single final formatting/build/test pass over the integrated worktree (macOS arm64 and Linux aarch64 musl as planned), then reconcile any surfaced failures. Registered later-tier providers are intentionally absent and must be wired by their owners. Main requested no further worker source expansion/formatter invocation; readability expansion is included in Main's final cargo fmt. No unresolved system-primitive request remains.
